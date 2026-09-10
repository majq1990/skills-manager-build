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
use crate::core::tool_adapters::{self, ToolAdapter};

static SYNC_LOCK: Mutex<()> = Mutex::new(());

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
    pub reason: Option<String>,
    pub error: Option<String>,
}

pub fn sync_all(dry_run: bool) -> Result<SyncReport> {
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

    let mut tool_reports = Vec::with_capacity(installed.len());
    for tool in &installed {
        let skills_dir = pick_skills_dir(tool);
        tool_reports.push(sync_tool(tool, &skills_dir, &memories, dry_run));
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
        reason: None,
        error: None,
    };

    if !dry_run {
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
    if !dry_run {
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

    let mut want: HashSet<String> = memories.iter().map(|m| m.skill_name.clone()).collect();
    // The universal bridge has no corresponding source memory file.
    want.insert(bridge::BRIDGE_SKILL_NAME.to_string());

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
