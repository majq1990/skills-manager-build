//! Agent distribution service: discovery, import, variant management,
//! and per-tool deployment. Shared by the Tauri commands (commands/agents.rs)
//! and the CLI (`agents` command group).

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};

use super::agent_variant::{
    self, content_sha256, generate_expert_plugin_json, remove_expert_marketplace_entry,
    resolve_deploy_source, upsert_expert_marketplace_entry, validate_agent_markdown,
    variant_filename, DeploySource,
};
use super::agent_store::AgentRecord;
use super::central_repo;
use super::skill_store::SkillStore;
use super::sync_engine::{self, SyncMode};
use super::tool_adapters::{find_adapter_with_store, AgentDeployKind, ToolAdapter};

/// A file on disk that looks like an agent definition for `tool`.
#[derive(Debug, Clone, Serialize)]
pub struct AgentFileEntry {
    pub tool: String,
    pub path: String,
    pub name_guess: String,
    /// Set when the tool already has a centrally managed agent with this name.
    pub imported_agent_id: Option<String>,
}

pub struct AgentSyncOutcome {
    pub target_path: PathBuf,
    pub mode: SyncMode,
}

fn configured_sync_mode(store: &super::skill_store::SkillStore, tool_key: &str) -> SyncMode {
    let configured = store.get_setting("sync_mode").ok().flatten();
    sync_engine::sync_mode_for_tool(tool_key, configured.as_deref())
}

fn builtin_deny_list(store: &super::skill_store::SkillStore, adapter: &ToolAdapter) -> Vec<String> {
    let mut names: Vec<String> = adapter
        .builtin_agent_names
        .iter()
        .map(|s| s.to_string())
        .collect();
    // Settings augmentations (P1 UI surface; honored here from day one).
    if let Ok(Some(raw)) = store.get_setting("extra_builtin_agent_names") {
        if let Ok(extra) = serde_json::from_str::<Vec<String>>(&raw) {
            names.extend(extra);
        }
    }
    names
}

// ── discovery ──

/// List agent definition files actually present in the tool's agents /
/// skills directory. Built-in names are skipped — skills-manager only ever
/// sees user-defined agents (design spec 搂7).
pub fn scan_tool_agent_files(store: &super::skill_store::SkillStore, adapter: &ToolAdapter) -> Result<Vec<AgentFileEntry>> {
    let Some(kind) = adapter.agent_deploy_kind.clone() else {
        return Ok(vec![]);
    };
    let Some(root) = adapter.agents_dir() else {
        return Ok(vec![]);
    };
    if !root.is_dir() {
        return Ok(vec![]);
    }

    let deny = builtin_deny_list(store, adapter);
    let mut entries: Vec<AgentFileEntry> = Vec::new();

    match &kind {
        AgentDeployKind::File { ext } => {
            let suffix = format!(".{ext}");
            for entry in std::fs::read_dir(&root)
                .with_context(|| format!("reading {}", root.display()))?
            {
                let entry = entry?;
                let path = entry.path();
                if !path.is_file() || !path.to_string_lossy().ends_with(&suffix) {
                    continue;
                }
                let stem = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                if deny.iter().any(|n| *n == stem) {
                    continue;
                }
                let imported_agent_id = store.get_agent_by_name(&stem)?.map(|a| a.id);
                entries.push(AgentFileEntry {
                    tool: adapter.key.clone(),
                    path: path.to_string_lossy().to_string(),
                    name_guess: stem,
                    imported_agent_id,
                });
            }
        }
        AgentDeployKind::SkillWrapped => {
            // SkillWrapped 工具（dsh）的 agent 承载目录就是它的 skills 目录：
            // 里面全部是真正的技能（SKILL.md），没有可区分的独立 agent 存储。
            // 把它们列成"可导入 agent"会把技能误收编成 agent（用户明确禁止），
            // 因此这类工具不做导入发现；部署（sync）不受影响。
            return Ok(vec![]);
        }
        AgentDeployKind::ExpertPlugin => {
            // Each plugin directory holds agents/*.md; the plugin directory
            // name is the expert name by convention.
            for entry in std::fs::read_dir(&root)
                .with_context(|| format!("reading {}", root.display()))?
            {
                let plugin_dir = entry?.path();
                if !plugin_dir.is_dir() {
                    continue;
                }
                let stem = plugin_dir
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                if deny.iter().any(|n| *n == stem) {
                    continue;
                }
                let agents_dir = plugin_dir.join("agents");
                if !agents_dir.is_dir() {
                    continue;
                }
                let mut md_files: Vec<PathBuf> = std::fs::read_dir(&agents_dir)
                    .with_context(|| format!("reading {}", agents_dir.display()))?
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.is_file() && p.extension().map(|x| x == "md").unwrap_or(false))
                    .collect();
                md_files.sort();
                let Some(first_md) = md_files.first() else {
                    continue;
                };
                let imported_agent_id = store.get_agent_by_name(&stem)?.map(|a| a.id);
                entries.push(AgentFileEntry {
                    tool: adapter.key.clone(),
                    path: first_md.to_string_lossy().to_string(),
                    name_guess: stem,
                    imported_agent_id,
                });
            }
        }
    }

    entries.sort_by(|a, b| a.name_guess.cmp(&b.name_guess));
    Ok(entries)
}

// ── import ──

/// Import one on-disk agent definition into the central repo.
///
/// - First import for a name: writes the canonical `AGENT.md` (converting
///   codex TOML / SKILL.md as needed) plus the original as the tool variant.
/// - Repeat import for an existing name: refreshes that tool's variant file
///   only — the canonical stays untouched (design spec 搂5.3).
pub fn import_agent_from_file(
    store: &super::skill_store::SkillStore,
    tool_key: &str,
    path: &Path,
) -> Result<AgentRecord> {
    let adapter = find_adapter_with_store(store, tool_key)
        .ok_or_else(|| anyhow!("unknown tool '{tool_key}'"))?;
    let kind = adapter
        .agent_deploy_kind
        .clone()
        .ok_or_else(|| anyhow!("tool '{tool_key}' does not support agents"))?;

    let content = std::fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .ok_or_else(|| anyhow!("path has no file stem"))?;
    let name = agent_variant::agent_id_from_source(&stem, &content)?;

    let central_dir = central_repo::agents_dir().join(&name);
    let canonical_path = central_dir.join(agent_variant::CANONICAL_FILE_NAME);
    let variant_name = variant_filename(tool_key, &kind);
    let variant_path = central_dir.join(&variant_name);

    std::fs::create_dir_all(&central_dir)
        .with_context(|| format!("creating {}", central_dir.display()))?;

    let existing = store.get_agent_by_name(&name)?;
    let canonical_content = match &kind {
        AgentDeployKind::File { ext } if ext == "md" => content.clone(),
        // Expert plugins embed the markdown verbatim too.
        AgentDeployKind::ExpertPlugin => content.clone(),
        AgentDeployKind::File { .. } => agent_variant::codex_toml_to_canonical(&content)?,
        AgentDeployKind::SkillWrapped => agent_variant::skill_md_to_canonical(&content)?,
    };

    if existing.is_none() {
        validate_agent_markdown(&canonical_content)
            .map_err(|e| anyhow!("'{name}' failed validation: {e}"))?;
        std::fs::write(&canonical_path, &canonical_content)
            .with_context(|| format!("writing {}", canonical_path.display()))?;
    }
    // The tool variant always stores the verbatim source, so deployment
    // reproduces exactly what the tool had on disk.
    std::fs::write(&variant_path, &content)
        .with_context(|| format!("writing {}", variant_path.display()))?;

    let parsed = agent_variant::parse_agent_markdown(&canonical_content);
    let record = match existing {
        Some(mut agent) => {
            agent.updated_at = chrono::Utc::now().timestamp();
            store.upsert_agent(&agent)?;
            agent
        }
        None => {
            let record = SkillStore::new_agent_record(
                name.clone(),
                parsed.as_ref().and_then(|p| p.description.clone()),
                "local-imported",
                central_dir.to_string_lossy().to_string(),
                Some(content_sha256(canonical_content.as_bytes())),
            );
            store.insert_agent(&record)?;
            record
        }
    };

    let mut draft = super::audit_log::AuditDraft::new("agent_import")
        .skill(record.id.clone(), name.clone())
        .tool(tool_key.to_string())
        .ok();
    draft.detail = Some(format!("kind=agent from {}", path.display()));
    store.log_audit(draft);

    Ok(record)
}

/// Import every agent-looking file under `dir` for `tool`.
pub fn import_agents_from_dir(
    store: &super::skill_store::SkillStore,
    tool_key: &str,
    dir: &Path,
) -> Result<Vec<AgentRecord>> {
    let adapter = find_adapter_with_store(store, tool_key)
        .ok_or_else(|| anyhow!("unknown tool '{tool_key}'"))?;
    let kind = adapter
        .agent_deploy_kind
        .clone()
        .ok_or_else(|| anyhow!("tool '{tool_key}' does not support agents"))?;

    let mut imported = Vec::new();
    match &kind {
        AgentDeployKind::File { ext } => {
            let suffix = format!(".{ext}");
            for entry in std::fs::read_dir(dir)
                .with_context(|| format!("reading {}", dir.display()))?
            {
                let path = entry?.path();
                if path.is_file() && path.to_string_lossy().ends_with(&suffix) {
                    imported.push(import_agent_from_file(store, tool_key, &path)?);
                }
            }
        }
        AgentDeployKind::SkillWrapped => {
            for entry in std::fs::read_dir(dir)
                .with_context(|| format!("reading {}", dir.display()))?
            {
                let path = entry?.path();
                let skill_md = path.join("SKILL.md");
                if path.is_dir() && skill_md.is_file() {
                    imported.push(import_agent_from_file(store, tool_key, &skill_md)?);
                }
            }
        }
        AgentDeployKind::ExpertPlugin => {
            for entry in std::fs::read_dir(dir)
                .with_context(|| format!("reading {}", dir.display()))?
            {
                let plugin_dir = entry?.path();
                if !plugin_dir.is_dir() {
                    continue;
                }
                let agents_dir = plugin_dir.join("agents");
                if !agents_dir.is_dir() {
                    continue;
                }
                for md in std::fs::read_dir(&agents_dir)
                    .with_context(|| format!("reading {}", agents_dir.display()))?
                {
                    let path = md?.path();
                    if path.is_file() && path.extension().map(|x| x == "md").unwrap_or(false) {
                        imported.push(import_agent_from_file(store, tool_key, &path)?);
                    }
                }
            }
        }
    }
    Ok(imported)
}

/// Import an agent definition picked from the local disk (file-picker upload,
/// mirroring the skill local-import flow). Source format is inferred:
///
/// - `*.toml` is treated as a codex agent: reversed into the canonical
///   `AGENT.md`, with the original kept as the `codex.toml` variant.
/// - anything else is taken as canonical markdown verbatim.
///
/// First import writes the canonical; a repeat upload of the same name only
/// refreshes that tool's variant file (same rule as per-tool imports).
pub fn import_agent_upload(
    store: &super::skill_store::SkillStore,
    path: &Path,
) -> Result<AgentRecord> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .ok_or_else(|| anyhow!("path has no file stem"))?;
    let name = agent_variant::agent_id_from_source(&stem, &content)?;

    let central_dir = central_repo::agents_dir().join(&name);
    let canonical_path = central_dir.join(agent_variant::CANONICAL_FILE_NAME);
    let is_codex_toml = path
        .extension()
        .map(|ext| ext.eq_ignore_ascii_case("toml"))
        .unwrap_or(false);

    let canonical_content = if is_codex_toml {
        agent_variant::codex_toml_to_canonical(&content)?
    } else {
        content.clone()
    };
    validate_agent_markdown(&canonical_content)
        .map_err(|e| anyhow!("'{name}' failed validation: {e}"))?;

    std::fs::create_dir_all(&central_dir)
        .with_context(|| format!("creating {}", central_dir.display()))?;

    let existing = store.get_agent_by_name(&name)?;
    if existing.is_none() {
        std::fs::write(&canonical_path, &canonical_content)
            .with_context(|| format!("writing {}", canonical_path.display()))?;
    }
    if is_codex_toml {
        let kind = AgentDeployKind::File {
            ext: "toml".to_string(),
        };
        std::fs::write(central_dir.join(agent_variant::variant_filename("codex", &kind)), &content)
            .with_context(|| format!("writing codex variant of {name}"))?;
    }

    let parsed = agent_variant::parse_agent_markdown(&canonical_content);
    let record = match existing {
        Some(mut agent) => {
            agent.updated_at = chrono::Utc::now().timestamp();
            store.upsert_agent(&agent)?;
            agent
        }
        None => {
            let record = SkillStore::new_agent_record(
                name.clone(),
                parsed.as_ref().and_then(|p| p.description.clone()),
                "local-imported",
                central_dir.to_string_lossy().to_string(),
                Some(content_sha256(canonical_content.as_bytes())),
            );
            store.insert_agent(&record)?;
            record
        }
    };

    let mut draft = super::audit_log::AuditDraft::new("agent_import")
        .skill(record.id.clone(), name.clone())
        .ok();
    draft.detail = Some(format!("kind=agent upload from {}", path.display()));
    store.log_audit(draft);

    Ok(record)
}

// ── deployment ──

/// Deploy one centrally managed agent to a tool (design spec 搂8).
pub fn sync_agent_to_tool(
    store: &super::skill_store::SkillStore,
    agent_id: &str,
    tool: &str,
) -> Result<AgentSyncOutcome> {
    let adapter = find_adapter_with_store(store, tool)
        .ok_or_else(|| anyhow!("unknown tool '{tool}'"))?;
    deploy_with_adapter(store, agent_id, &adapter)
}

fn deploy_with_adapter(
    store: &super::skill_store::SkillStore,
    agent_id: &str,
    adapter: &ToolAdapter,
) -> Result<AgentSyncOutcome> {
    let agent = store
        .get_agent_by_id(agent_id)?
        .ok_or_else(|| anyhow!("agent '{agent_id}' not found"))?;
    if !adapter.supports_agents() {
        bail!("tool '{}' does not support agent distribution", adapter.key);
    }
    let kind = adapter
        .agent_deploy_kind
        .clone()
        .expect("supports_agents guarantees kind");

    // Built-in deny list: refuse instead of asking (design spec 搂7).
    let deny = builtin_deny_list(store, adapter);
    if deny.iter().any(|n| *n == agent.name) {
        bail!(
            "'{}' collides with a built-in agent of {} — rename the agent id first",
            agent.name,
            adapter.display_name
        );
    }

    let central_dir = PathBuf::from(&agent.central_path);
    let source = resolve_deploy_source(&central_dir, &adapter.key, &kind)?;
    let mode = configured_sync_mode(store, &adapter.key);

    let (target_path, source_hash): (PathBuf, String) = match (&source, &kind) {
        (DeploySource::File(file), AgentDeployKind::File { ext }) => {
            let agents_root = adapter
                .agents_dir()
                .ok_or_else(|| anyhow!("tool '{}' has no agents dir", adapter.key))?;
            let target = agents_root.join(format!("{}.{ext}", agent.name));
            let current_hash = content_sha256(&std::fs::read(file)?);
            let last_hash = store
                .get_agent_target(agent_id, &adapter.key)?
                .and_then(|t| t.source_hash);
            sync_engine::sync_agent_file(
                file,
                &target,
                mode,
                last_hash.as_deref(),
                Some(&current_hash),
            )?;
            (target, current_hash)
        }
        (
            DeploySource::SkillWrapped { content, hash },
            AgentDeployKind::SkillWrapped,
        ) => {
            // Stage SKILL.md centrally, then reuse the directory-level sync
            // engine so symlink/copy semantics match skills exactly.
            let staging_root = central_repo::agent_staging_dir().join(&agent.name);
            std::fs::create_dir_all(&staging_root)?;
            let staging_md = staging_root.join("SKILL.md");
            std::fs::write(&staging_md, content)?;
            let skills_root = adapter
                .agents_dir()
                .ok_or_else(|| anyhow!("tool '{}' has no skills dir", adapter.key))?;
            let target_dir = skills_root.join(&agent.name);
            sync_engine::sync_skill(&staging_root, &target_dir, mode)?;
            (target_dir, hash.clone())
        }
        (DeploySource::File(file), AgentDeployKind::ExpertPlugin) => {
            // Assemble a CodeBuddy expert plugin in staging, then deploy the
            // whole directory with the standard skill sync engine.
            let staging_root = central_repo::agent_staging_dir().join(&agent.name);
            std::fs::create_dir_all(staging_root.join("agents"))?;
            std::fs::create_dir_all(staging_root.join(".codebuddy-plugin"))?;
            std::fs::copy(file, staging_root.join("agents").join(format!("{}.md", agent.name)))?;
            let plugin_json = generate_expert_plugin_json(&agent.name, agent.description.as_deref())?;
            std::fs::write(
                staging_root.join(".codebuddy-plugin").join("plugin.json"),
                plugin_json,
            )?;

            let plugins_root = adapter
                .agents_dir()
                .ok_or_else(|| anyhow!("tool '{}' has no expert plugins dir", adapter.key))?;
            let target_dir = plugins_root.join(&agent.name);
            sync_engine::sync_skill(&staging_root, &target_dir, mode)?;

            // Register the plugin in the local marketplace manifest.
            let marketplace_dir = plugins_root
                .parent()
                .ok_or_else(|| anyhow!("expert plugins dir has no parent"))?
                .to_path_buf();
            let marketplace_file = marketplace_dir
                .join(".codebuddy-plugin")
                .join("marketplace.json");
            let existing = std::fs::read_to_string(&marketplace_file).ok();
            let marketplace_name = existing
                .as_deref()
                .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
                .and_then(|v| {
                    v.get("name")
                        .and_then(|n| n.as_str())
                        .map(ToOwned::to_owned)
                })
                .unwrap_or_else(|| "my-experts".to_string());
            let updated = upsert_expert_marketplace_entry(
                existing.as_deref(),
                &marketplace_name,
                &agent.name,
                agent.description.as_deref(),
            )?;
            std::fs::create_dir_all(marketplace_file.parent().unwrap())?;
            std::fs::write(&marketplace_file, updated)?;

            let hash = content_sha256(&std::fs::read(file)?);
            (target_dir, hash)
        }
        (source, kind) => bail!(
            "deploy source {source:?} does not match deploy kind {kind:?}"
        ),
    };

    let mode_str = mode.as_str().to_string();
    let target_record = SkillStore::new_agent_target_record(
        agent.id.clone(),
        adapter.key.clone(),
        target_path.to_string_lossy().to_string(),
        mode_str,
        Some(source_hash),
    );
    store.insert_agent_target(&target_record)?;

    let mut draft = super::audit_log::AuditDraft::new("agent_enable")
        .skill(agent.id.clone(), agent.name.clone())
        .tool(adapter.key.clone())
        .ok();
    draft.detail = Some(format!(
        "kind=agent -> {}",
        target_path.display()
    ));
    store.log_audit(draft);

    Ok(AgentSyncOutcome {
        target_path,
        mode,
    })
}

/// Remove a deployment previously recorded in `agent_targets`. Only the
/// recorded path is touched.
/// Nothing else in the tool's directories is ever modified.
pub fn unsync_agent_from_tool(store: &SkillStore, agent_id: &str, tool: &str) -> Result<()> {
    let Some(target) = store.get_agent_target(agent_id, tool)? else {
        return Ok(());
    };
    sync_engine::remove_agent_target(&PathBuf::from(&target.target_path)).ok();

    // Expert plugins: also drop the marketplace.json entry.
    if let Some(adapter) = find_adapter_with_store(store, tool) {
        if adapter.agent_deploy_kind == Some(AgentDeployKind::ExpertPlugin) {
            if let (Some(plugins_root), Some(agent)) = (
                adapter.agents_dir(),
                store.get_agent_by_id(agent_id)?,
            ) {
                if let Some(marketplace_dir) = plugins_root.parent() {
                    let marketplace_file = marketplace_dir
                        .join(".codebuddy-plugin")
                        .join("marketplace.json");
                    if let Ok(existing) = std::fs::read_to_string(&marketplace_file) {
                        if let Some(updated) =
                            remove_expert_marketplace_entry(&existing, &agent.name)?
                        {
                            let _ = std::fs::write(&marketplace_file, updated);
                        }
                    }
                }
            }
        }
    }

    store.delete_agent_target(agent_id, tool)?;

    let agent_name = store
        .get_agent_by_id(agent_id)?
        .map(|a| a.name)
        .unwrap_or_default();
    let mut draft = super::audit_log::AuditDraft::new("agent_disable")
        .skill(agent_id.to_string(), agent_name)
        .tool(tool.to_string())
        .ok();
    draft.detail = Some("kind=agent".into());
    store.log_audit(draft);
    Ok(())
}

// ── variants / export / delete ──

/// Explicitly generate (or refresh) the variant file for (agent, tool).
/// Returns the variant path.
pub fn generate_agent_variant_file(
    store: &super::skill_store::SkillStore,
    agent_id: &str,
    tool: &str,
) -> Result<PathBuf> {
    let agent = store
        .get_agent_by_id(agent_id)?
        .ok_or_else(|| anyhow!("agent '{agent_id}' not found"))?;
    let adapter = find_adapter_with_store(store, tool)
        .ok_or_else(|| anyhow!("unknown tool '{tool}'"))?;
    let kind = adapter
        .agent_deploy_kind
        .clone()
        .ok_or_else(|| anyhow!("tool '{tool}' does not support agents"))?;

    let canonical_path = PathBuf::from(&agent.central_path).join(agent_variant::CANONICAL_FILE_NAME);
    let canonical = std::fs::read_to_string(&canonical_path).with_context(|| {
        format!("reading canonical {}", canonical_path.display())
    })?;
    let variant_content = agent_variant::generate_variant_for_tool(&canonical, tool, &kind)?;

    let variant_path = PathBuf::from(&agent.central_path).join(variant_filename(tool, &kind));
    std::fs::write(&variant_path, &variant_content)
        .with_context(|| format!("writing {}", variant_path.display()))?;
    Ok(variant_path)
}

/// Copy the canonical definition (and all variants) to `dest_dir`.
pub fn export_agent(store: &super::skill_store::SkillStore, agent_id: &str, dest_dir: &Path) -> Result<PathBuf> {
    let agent = store
        .get_agent_by_id(agent_id)?
        .ok_or_else(|| anyhow!("agent '{agent_id}' not found"))?;
    let out = dest_dir.join(format!("{}.md", agent.name));
    let canonical = PathBuf::from(&agent.central_path).join(agent_variant::CANONICAL_FILE_NAME);
    std::fs::copy(&canonical, &out)
        .with_context(|| format!("copying {} -> {}", canonical.display(), out.display()))?;
    Ok(out)
}

/// Delete a centrally managed agent: undeploy all recorded targets, then
/// remove the central directory and the DB rows.
pub fn delete_agent_artifact(store: &super::skill_store::SkillStore, agent_id: &str) -> Result<()> {
    let agent = store
        .get_agent_by_id(agent_id)?
        .ok_or_else(|| anyhow!("agent '{agent_id}' not found"))?;
    for target in store.get_targets_for_agent(agent_id)? {
        sync_engine::remove_agent_target(&PathBuf::from(&target.target_path)).ok();
    }
    let central_dir = PathBuf::from(&agent.central_path);
    if central_dir.is_dir() {
        std::fs::remove_dir_all(&central_dir)
            .with_context(|| format!("removing {}", central_dir.display()))?;
    }
    store.delete_agent(agent_id)?;

    let mut draft = super::audit_log::AuditDraft::new("agent_delete")
        .skill(agent_id.to_string(), agent.name.clone())
        .ok();
    draft.detail = Some("kind=agent".into());
    store.log_audit(draft);
    Ok(())
}

// ── scenario (Preset 混装) ────────────────────────────────────────────────

/// Agent-capable, installed and globally-enabled tool adapters eligible for
/// scenario deployment.
pub fn agent_capable_installed_adapters(store: &SkillStore) -> Vec<ToolAdapter> {
    super::tool_adapters::enabled_installed_adapters(store)
        .into_iter()
        .filter(|a| a.supports_agents())
        .collect()
}

/// Tools a scenario agent deploys to: agent-capable × installed × enabled ×
/// scenario toggle. Missing toggle rows are defaulted to enabled first.
pub fn enabled_tools_for_scenario_agent(
    store: &SkillStore,
    scenario_id: &str,
    agent_id: &str,
) -> Result<Vec<ToolAdapter>> {
    let adapters = agent_capable_installed_adapters(store);
    let keys: Vec<String> = adapters.iter().map(|a| a.key.clone()).collect();
    store.ensure_scenario_agent_tool_defaults(scenario_id, agent_id, &keys)?;
    let enabled = store.get_enabled_tools_for_scenario_agent(scenario_id, agent_id)?;
    let enabled_set: std::collections::HashSet<String> = enabled.into_iter().collect();
    Ok(adapters
        .into_iter()
        .filter(|a| enabled_set.contains(&a.key))
        .collect())
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentDeployAttempt {
    pub agent_id: String,
    pub agent_name: String,
    pub tool: String,
    pub ok: bool,
    pub error: Option<String>,
}

/// Deploy every agent in a scenario to its enabled tools. Best-effort per
/// (agent, tool) pair: failures are collected in the report, never aborting
/// the rest of the preset application.
pub fn sync_agent_scenario(store: &SkillStore, scenario_id: &str) -> Vec<AgentDeployAttempt> {
    let mut attempts = Vec::new();
    let Ok(agent_ids) = store.get_agent_ids_for_scenario(scenario_id) else {
        return attempts;
    };
    for agent_id in agent_ids {
        let name = store
            .get_agent_by_id(&agent_id)
            .ok()
            .flatten()
            .map(|a| a.name)
            .unwrap_or_default();
        let adapters = enabled_tools_for_scenario_agent(store, scenario_id, &agent_id)
            .unwrap_or_default();
        for adapter in &adapters {
            let outcome = deploy_with_adapter(store, &agent_id, adapter);
            attempts.push(AgentDeployAttempt {
                agent_id: agent_id.clone(),
                agent_name: name.clone(),
                tool: adapter.key.clone(),
                ok: outcome.is_ok(),
                error: outcome.err().map(|e| e.to_string()),
            });
        }
    }
    attempts
}

/// Undeploy every agent in a scenario (removes recorded targets only).
pub fn unsync_agent_scenario(store: &SkillStore, scenario_id: &str) {
    let Ok(agent_ids) = store.get_agent_ids_for_scenario(scenario_id) else {
        return;
    };
    for agent_id in agent_ids {
        let targets = store.get_targets_for_agent(&agent_id).unwrap_or_default();
        for target in &targets {
            let _ = unsync_agent_from_tool(store, &agent_id, &target.tool);
        }
    }
}

/// Download-and-install an agent from the enterprise server into the central
/// repo (mirror of the enterprise skill install). The package zip root must
/// contain `AGENT.md`; other files (tool variants) are kept verbatim. The
/// record is registered with an `enterprise` source; deployment still goes
/// through the per-tool sync flow.
pub fn install_enterprise_agent(
    store: &SkillStore,
    name: &str,
    version: &str,
) -> Result<AgentRecord> {
    // 1. 下载企业 agent 包（全局 ENTERPRISE_API 内存 token）
    let zip_bytes = crate::commands::enterprise::download_agent_zip(name, version)
        .map_err(|e| anyhow!("{e}"))?;

    // 2. 校验 ZIP magic（PK\x03\x04），防止把错误页/空响应当成包
    if zip_bytes.len() < 4
        || zip_bytes[0] != 0x50
        || zip_bytes[1] != 0x4B
        || zip_bytes[2] != 0x03
        || zip_bytes[3] != 0x04
    {
        bail!("下载内容不是有效 ZIP（可能是错误页）");
    }

    // 3. 解压到临时目录（路径穿越防护：拒绝 ".." 与绝对路径）
    let temp = tempfile::tempdir().context("creating temp dir")?;
    let zip_path = temp.path().join("agent.zip");
    std::fs::write(&zip_path, &zip_bytes)?;
    let file = std::fs::File::open(&zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let extract_dir = temp.path().join("extract");
    std::fs::create_dir_all(&extract_dir)?;

    let mut names: Vec<String> = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        let raw = entry.name().to_string();
        if raw.contains("..") || raw.starts_with('/') || raw.starts_with('\\') {
            bail!("agent 包内含非法路径: {raw}");
        }
        let rel = raw.replace('\\', "/");
        let dest = extract_dir.join(&rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = std::fs::File::create(&dest)?;
        std::io::copy(&mut entry, &mut out)?;
        names.push(rel);
    }

    // 4. 允许 zip 根带一层顶层目录（与服务器 extractZip 同款行为）
    let root = if names.len() > 1 && names.iter().all(|n| n.contains('/')) {
        let first = names[0].split('/').next().unwrap_or("").to_string();
        if !first.is_empty() && names.iter().all(|n| n.starts_with(&format!("{first}/"))) {
            extract_dir.join(&first)
        } else {
            extract_dir.clone()
        }
    } else {
        extract_dir.clone()
    };

    // 5. 校验 AGENT.md 存在且合法、zip 内名与请求名一致
    let agent_md_path = root.join(agent_variant::CANONICAL_FILE_NAME);
    if !agent_md_path.is_file() {
        bail!("agent 包缺少 AGENT.md");
    }
    let canonical = std::fs::read_to_string(&agent_md_path)?;
    validate_agent_markdown(&canonical).map_err(|e| anyhow!("'{name}' failed validation: {e}"))?;
    let parsed = agent_variant::parse_agent_markdown(&canonical);
    let agent_name = parsed
        .as_ref()
        .and_then(|p| p.name.clone())
        .unwrap_or_else(|| name.to_string());
    if agent_name != name {
        bail!("agent 包内名称 '{agent_name}' 与请求名 '{name}' 不一致");
    }

    // 6. 拷贝 AGENT.md + 变体文件到中央库
    let central_dir = central_repo::agents_dir().join(&agent_name);
    std::fs::create_dir_all(&central_dir)?;
    for entry in std::fs::read_dir(&root)? {
        let entry = entry?;
        if entry.path().is_file() {
            let dest = central_dir.join(entry.file_name());
            std::fs::copy(entry.path(), &dest)?;
        }
    }

    // 7. 入库（source_ref 记录 企业源@版本）
    let record = match store.get_agent_by_name(&agent_name)? {
        Some(mut agent) => {
            agent.updated_at = chrono::Utc::now().timestamp();
            agent.source_ref = Some(format!("enterprise@{}", version));
            agent.content_hash = Some(content_sha256(canonical.as_bytes()));
            store.upsert_agent(&agent)?;
            agent
        }
        None => {
            let mut record = SkillStore::new_agent_record(
                agent_name.clone(),
                parsed.as_ref().and_then(|p| p.description.clone()),
                "enterprise",
                central_dir.to_string_lossy().to_string(),
                Some(content_sha256(canonical.as_bytes())),
            );
            record.source_ref = Some(format!("enterprise@{}", version));
            store.insert_agent(&record)?;
            record
        }
    };

    let mut draft = super::audit_log::AuditDraft::new("agent_import")
        .skill(record.id.clone(), agent_name.clone())
        .ok();
    draft.detail = Some(format!("kind=agent enterprise install {}@{}", name, version));
    store.log_audit(draft);

    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::central_repo;
    use std::fs;

    /// Base dir is process-wide static state; serialize tests that redirect
    /// it via the shared guard and always restore.
    fn with_isolated_base<T>(f: impl FnOnce(&tempfile::TempDir) -> T) -> T {
        let _guard = central_repo::test_base_dir_lock();
        let tmp = tempfile::tempdir().unwrap();
        central_repo::set_test_base_dir_override(Some(tmp.path().to_path_buf()));
        let result = f(&tmp);
        central_repo::set_test_base_dir_override(None);
        result
    }

    #[test]
    fn upload_md_imports_as_canonical() {
        with_isolated_base(|tmp| {
            let store = SkillStore::new(&tmp.path().join("test.db")).unwrap();
            let src = tmp.path().join("my-agent.md");
            fs::write(&src, "---\ndescription: test agent\n---\n# Body\nplain body\n").unwrap();

            let record = import_agent_upload(&store, &src).unwrap();
            assert_eq!(record.name, "my-agent");
            let canonical =
                PathBuf::from(&record.central_path).join(agent_variant::CANONICAL_FILE_NAME);
            let text = fs::read_to_string(canonical).unwrap();
            assert!(text.contains("# Body"));
            assert_eq!(store.get_all_agents().unwrap().len(), 1);
        });
    }

    #[test]
    fn upload_codex_toml_reverses_to_canonical_and_keeps_variant() {
        with_isolated_base(|tmp| {
            let store = SkillStore::new(&tmp.path().join("test.db")).unwrap();
            let src = tmp.path().join("helper.toml");
            fs::write(
                &src,
                "name = \"helper\"\ndescription = \"a codex agent\"\n\
                 sandbox_mode = \"read-only\"\ndeveloper_instructions = \"Do the thing.\"\n",
            )
            .unwrap();

            let record = import_agent_upload(&store, &src).unwrap();
            assert_eq!(record.name, "helper");
            let dir = PathBuf::from(&record.central_path);
            let canonical =
                fs::read_to_string(dir.join(agent_variant::CANONICAL_FILE_NAME)).unwrap();
            assert!(canonical.contains("Do the thing."));
            assert!(dir.join("codex.toml").is_file());
        });
    }
}
