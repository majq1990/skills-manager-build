//! Deploy shared memory (as `memory-*` skills) to every installed agent.
//!
//! Phase 2 strategy: dispatch to per-agent `MemoryAdapter` implementations
//! (symlink for claude-lineage, JS plugin for opencode, default copy for all
//! others). Any existing skill whose directory name begins with `memory-` is
//! considered managed-by-us and gets swept if not in the current shared root.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::Result;
use serde::Serialize;

use crate::core::memory::materializer::{self, MaterializedMemory, MEMORY_SKILL_PREFIX};
use crate::core::memory::shared_root;
use crate::core::memory::{adapters, bridge, sources};
use crate::core::skill_store::SkillStore;
use crate::core::tool_adapters::{self, ToolAdapter};

static SYNC_LOCK: Mutex<()> = Mutex::new(());

/// Setting key holding a JSON array of tool keys the user opted OUT of memory
/// deployment (e.g. `["dsh"]`). Tools on this list get their `memory-*` skills
/// and the bridge swept instead of redeployed, so an excluded tool stays clean
/// across the periodic sync.
pub const EXCLUDED_TOOLS_SETTING: &str = "memory_excluded_tools";

/// Parse the `memory_excluded_tools` JSON array. Unparseable values are
/// ignored loudly rather than silently dropping the user's exclusion.
pub fn parse_excluded_tools(raw: &str) -> HashSet<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return HashSet::new();
    }
    match serde_json::from_str::<Vec<String>>(trimmed) {
        Ok(keys) => keys
            .into_iter()
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty())
            .collect(),
        Err(err) => {
            log::warn!("memory: ignoring unparseable {EXCLUDED_TOOLS_SETTING} ({err}): {trimmed}");
            HashSet::new()
        }
    }
}

/// Tools excluded from memory deployment, read from the store setting.
pub fn excluded_tools(store: &SkillStore) -> HashSet<String> {
    match store.get_setting(EXCLUDED_TOOLS_SETTING) {
        Ok(Some(raw)) => parse_excluded_tools(&raw),
        Ok(None) => HashSet::new(),
        Err(err) => {
            log::warn!("memory: failed to read {EXCLUDED_TOOLS_SETTING}: {err}");
            HashSet::new()
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SyncReport {
    pub shared_root: String,
    pub shared_root_exists: bool,
    pub memory_count: usize,
    pub memories: Vec<MaterializedMemory>,
    pub tools_installed: usize,
    pub tools: Vec<ToolSyncReport>,
    pub dry_run: bool,
    pub reconcile: sources::ReconcileReport,
}

#[derive(Debug, Serialize)]
pub struct ToolSyncReport {
    pub tool_key: String,
    pub display_name: String,
    pub skills_dir: String,
    pub adapter: &'static str,
    pub deployed: usize,
    pub removed: usize,
    pub skipped: bool,
    /// True when the tool is on the user's exclusion list: its memory-* skills
    /// and bridge are swept and nothing is deployed.
    pub excluded: bool,
    pub reason: Option<String>,
    pub error: Option<String>,
}

/// Sync without an exclusion list (CLI/tests that have no store at hand).
pub fn sync_all(dry_run: bool) -> Result<SyncReport> {
    sync_all_with(None, dry_run)
}

/// Sync, honoring the user's `memory_excluded_tools` setting when a store is
/// supplied. Excluded tools are swept, never deployed to.
pub fn sync_all_with(store: Option<&SkillStore>, dry_run: bool) -> Result<SyncReport> {
    let _guard = SYNC_LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("unified-memory sync lock poisoned"))?;
    let reconcile = sources::reconcile(dry_run)?;
    let root = shared_root::shared_root();
    let shared_root_exists = root.exists();
    let memories = if shared_root_exists {
        // Healing writes canonical sources back when they still carry
        // generated artifacts; a dry run stays read-only.
        materializer::scan_and_materialize(&root, !dry_run)?
    } else {
        Vec::new()
    };

    let installed: Vec<ToolAdapter> = tool_adapters::default_tool_adapters()
        .into_iter()
        .filter(|t| t.is_installed())
        .collect();

    let excluded = store.map(excluded_tools).unwrap_or_default();

    let mut tool_reports = Vec::with_capacity(installed.len());
    for tool in &installed {
        let skills_dir = pick_skills_dir(tool);
        let is_excluded = excluded.contains(&tool.key);
        // Excluded tools get an empty "want" set: every managed entry
        // (memory-* and the bridge) is swept and nothing is deployed. This is
        // what keeps a deliberately cleaned tool clean across the 60s sync.
        let wanted: &[MaterializedMemory] = if is_excluded { &[] } else { &memories };
        tool_reports.push(sync_tool(tool, &skills_dir, wanted, dry_run, is_excluded));
    }

    Ok(SyncReport {
        shared_root: root.to_string_lossy().to_string(),
        shared_root_exists,
        memory_count: memories.len(),
        memories,
        tools_installed: installed.len(),
        tools: tool_reports,
        dry_run,
        reconcile,
    })
}

fn pick_skills_dir(tool: &ToolAdapter) -> PathBuf {
    dirs::home_dir()
        .expect("cannot determine home directory")
        .join(&tool.relative_skills_dir)
}

fn sync_tool(
    tool: &ToolAdapter,
    skills_dir: &PathBuf,
    memories: &[MaterializedMemory],
    dry_run: bool,
    excluded: bool,
) -> ToolSyncReport {
    let adapter = adapters::resolve(tool);
    let mut report = ToolSyncReport {
        tool_key: tool.key.clone(),
        display_name: tool.display_name.clone(),
        skills_dir: skills_dir.to_string_lossy().to_string(),
        adapter: adapter.name(),
        deployed: 0,
        removed: 0,
        skipped: false,
        excluded,
        reason: None,
        error: None,
    };

    if excluded {
        report.reason = Some("excluded".to_string());
    }

    // An excluded tool is sweep-only: don't create its skills dir, don't
    // re-deploy the bridge, just clear anything this module previously put
    // there. Nothing to do when the directory is already gone.
    if excluded && !skills_dir.exists() {
        report.skipped = true;
        return report;
    }

    if !dry_run && !excluded {
        if let Err(e) = fs::create_dir_all(skills_dir) {
            report.skipped = true;
            report.reason = Some("skills dir not writable".to_string());
            report.error = Some(format!("mkdir {}: {}", skills_dir.display(), e));
            return report;
        }
    }

    // Every agent receives the same bridge contract. Agents that can access
    // the filesystem write directly to the shared root; sandboxed agents write
    // to their native memory folder and are collected on the next sync.
    if !dry_run && !excluded {
        if let Err(e) = bridge::ensure_bridge(skills_dir) {
            report.error = Some(format!("memory bridge deploy failed: {}", e));
        }
    }

    // What currently exists that we manage (memory-* dirs).
    let mut existing_managed: HashSet<String> = HashSet::new();
    if skills_dir.exists() {
        if let Ok(read) = fs::read_dir(skills_dir) {
            for entry in read.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.starts_with(MEMORY_SKILL_PREFIX)
                        || name == "memory-opencode-bridge"
                        || name == bridge::BRIDGE_SKILL_NAME
                    {
                        existing_managed.insert(name.to_string());
                    }
                }
            }
        }
    }

    // For an excluded tool nothing is wanted, so the sweep below clears every
    // managed entry (memories and the bridge alike).
    let mut want: HashSet<String> = memories.iter().map(|m| m.skill_name.clone()).collect();
    if !excluded {
        // The universal bridge has no corresponding source memory file.
        want.insert(bridge::BRIDGE_SKILL_NAME.to_string());
    }

    // Deploy each memory via the resolved adapter.
    for m in memories {
        if !dry_run {
            if let Err(e) = adapter.deploy(m, skills_dir) {
                report.error = Some(format!(
                    "deploy {} via {}: {}",
                    m.skill_name,
                    adapter.name(),
                    e
                ));
                continue;
            }
        }
        report.deployed += 1;
    }

    // Sweep stale memory-* / bridge skills that no longer have a source.
    for old in existing_managed.difference(&want) {
        if !dry_run {
            if let Err(e) = adapter.remove(old, skills_dir) {
                log::warn!("memory sync: failed to remove stale {}: {}", old, e);
                continue;
            }
        }
        report.removed += 1;
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_excluded_tools_reads_json_array_and_ignores_junk() {
        let parsed = parse_excluded_tools(r#"["dsh", " kimi ", ""]"#);
        assert!(parsed.contains("dsh"));
        assert!(parsed.contains("kimi"));
        assert!(!parsed.contains(""));
        assert_eq!(parsed.len(), 2);

        // empty / unparseable values must not panic and must not exclude anything
        assert!(parse_excluded_tools("").is_empty());
        assert!(parse_excluded_tools("   ").is_empty());
        assert!(parse_excluded_tools("dsh").is_empty());
        assert!(parse_excluded_tools("[1,2,3]").is_empty());
    }
}
