//! Tauri command surface for the agent distribution feature. All logic
//! lives in `core::agent_service` so the CLI can reuse it verbatim.

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::core::{
    agent_service, agent_store::AgentRecord, agent_store::AgentTargetRecord, agent_variant,
    central_repo, error::AppError, skill_store::SkillStore, tool_adapters,
};

#[derive(Debug, Serialize)]
pub struct AgentWithTargets {
    #[serde(flatten)]
    pub agent: AgentRecord,
    pub targets: Vec<AgentTargetRecord>,
    /// Variant file names present in the central agent directory
    /// (e.g. `["codex.toml", "opencode.md"]`).
    pub variants: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct AgentDocumentDto {
    pub content: String,
    pub variants: Vec<String>,
    /// Files deployable per tool: `tool -> variant file or "AGENT.md"`.
    pub resolution: Vec<AgentResolutionDto>,
}

#[derive(Debug, Serialize)]
pub struct AgentResolutionDto {
    pub tool: String,
    pub display_name: String,
    /// The file that would be deployed, or `null` when the variant is
    /// missing and the tool cannot take the canonical file directly.
    pub resolved_source: Option<String>,
    pub deployable: bool,
}

fn agent_capable_adapters(store: &SkillStore) -> Vec<tool_adapters::ToolAdapter> {
    tool_adapters::all_tool_adapters(store)
        .into_iter()
        .filter(|a| a.supports_agents())
        .collect()
}

fn list_variants(central_path: &str) -> Vec<String> {
    let mut variants = Vec::new();
    if let Ok(entries) = std::fs::read_dir(central_path) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name == agent_variant::CANONICAL_FILE_NAME {
                continue;
            }
            if entry.path().is_file() && name.contains('.') {
                variants.push(name);
            }
        }
    }
    variants.sort();
    variants
}

fn resolution_for(
    store: &SkillStore,
    agent_name: &str,
    central_path: &str,
) -> Vec<AgentResolutionDto> {
    agent_capable_adapters(store)
        .iter()
        .map(|adapter| {
            let Some(kind) = &adapter.agent_deploy_kind else {
                return AgentResolutionDto {
                    tool: adapter.key.clone(),
                    display_name: adapter.display_name.clone(),
                    resolved_source: None,
                    deployable: false,
                };
            };
            let variant_file = agent_variant::variant_filename(&adapter.key, kind);
            let variant_exists =
                std::path::Path::new(central_path).join(&variant_file).is_file();
            let canonical_fallback =
                matches!(kind, tool_adapters::AgentDeployKind::File { ext } if ext == "md");
            let deployable = variant_exists || canonical_fallback;
            let builtin_collision = adapter
                .builtin_agent_names
                .iter()
                .any(|n| *n == agent_name);
            AgentResolutionDto {
                tool: adapter.key.clone(),
                display_name: adapter.display_name.clone(),
                resolved_source: if variant_exists {
                    Some(variant_file)
                } else if canonical_fallback {
                    Some(agent_variant::CANONICAL_FILE_NAME.to_string())
                } else {
                    None
                },
                deployable: deployable && !builtin_collision,
            }
        })
        .collect()
}

#[tauri::command]
pub async fn get_agents(
    store: State<'_, Arc<SkillStore>>,
) -> Result<Vec<AgentWithTargets>, AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let agents = store.get_all_agents().map_err(AppError::db)?;
        let mut result = Vec::with_capacity(agents.len());
        for agent in agents {
            let targets = store.get_targets_for_agent(&agent.id).map_err(AppError::db)?;
            let variants = list_variants(&agent.central_path);
            result.push(AgentWithTargets {
                agent,
                targets,
                variants,
            });
        }
        Ok(result)
    })
    .await?
}

#[tauri::command]
pub async fn get_agent_document(
    id: String,
    store: State<'_, Arc<SkillStore>>,
) -> Result<AgentDocumentDto, AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let agent = store
            .get_agent_by_id(&id)
            .map_err(AppError::db)?
            .ok_or_else(|| AppError::not_found("agent not found"))?;
        let canonical = PathBuf::from(&agent.central_path).join(agent_variant::CANONICAL_FILE_NAME);
        let content = std::fs::read_to_string(&canonical).map_err(AppError::db)?;
        let variants = list_variants(&agent.central_path);
        let resolution = resolution_for(&store, &agent.name, &agent.central_path);
        Ok(AgentDocumentDto {
            content,
            variants,
            resolution,
        })
    })
    .await?
}

#[derive(Debug, Serialize)]
pub struct ScanAgentFilesDto {
    pub tool: String,
    pub display_name: String,
    pub files: Vec<agent_service::AgentFileEntry>,
}

#[tauri::command]
pub async fn scan_agent_files(
    tool: Option<String>,
    store: State<'_, Arc<SkillStore>>,
) -> Result<Vec<ScanAgentFilesDto>, AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let adapters = agent_capable_adapters(&store)
            .into_iter()
            .filter(|a| tool.as_deref().map(|t| a.key == t).unwrap_or(true))
            .filter(|a| a.is_installed());
        let mut result = Vec::new();
        for adapter in adapters {
            let files = agent_service::scan_tool_agent_files(&store, &adapter)
                .map_err(AppError::db)?;
            result.push(ScanAgentFilesDto {
                tool: adapter.key.clone(),
                display_name: adapter.display_name.clone(),
                files,
            });
        }
        Ok(result)
    })
    .await?
}

#[tauri::command]
pub async fn import_agent(
    tool: String,
    path: String,
    store: State<'_, Arc<SkillStore>>,
) -> Result<AgentRecord, AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let record = agent_service::import_agent_from_file(&store, &tool, &PathBuf::from(&path))
            .map_err(AppError::db)?;
        Ok(record)
    })
    .await?
}

#[tauri::command]
pub async fn import_agents_from_dir(
    tool: String,
    dir: String,
    store: State<'_, Arc<SkillStore>>,
) -> Result<Vec<AgentRecord>, AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        agent_service::import_agents_from_dir(&store, &tool, &PathBuf::from(&dir))
            .map_err(AppError::db)
    })
    .await?
}

/// Import agent definition files picked from the local disk (upload flow).
/// `*.toml` sources are reversed into the canonical AGENT.md; other files
/// are taken as canonical markdown.
#[tauri::command]
pub async fn import_agent_files(
    paths: Vec<String>,
    store: State<'_, Arc<SkillStore>>,
) -> Result<Vec<AgentRecord>, AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut records = Vec::with_capacity(paths.len());
        for path in paths {
            records.push(
                agent_service::import_agent_upload(&store, &PathBuf::from(&path))
                    .map_err(AppError::db)?,
            );
        }
        Ok(records)
    })
    .await?
}

#[derive(Debug, Serialize)]
pub struct AgentSyncOutcomeDto {
    pub target_path: String,
    pub mode: String,
}

#[tauri::command]
pub async fn sync_agent_to_tool(
    id: String,
    tool: String,
    store: State<'_, Arc<SkillStore>>,
) -> Result<AgentSyncOutcomeDto, AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let outcome =
            crate::core::sync_metadata::with_repo_lock("sync agent to tool", || {
                agent_service::sync_agent_to_tool(&store, &id, &tool)
            })
            .map_err(AppError::db)?;
        Ok(AgentSyncOutcomeDto {
            target_path: outcome.target_path.to_string_lossy().to_string(),
            mode: outcome.mode.as_str().to_string(),
        })
    })
    .await?
}

#[tauri::command]
pub async fn unsync_agent_from_tool(
    id: String,
    tool: String,
    store: State<'_, Arc<SkillStore>>,
) -> Result<(), AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::core::sync_metadata::with_repo_lock("unsync agent from tool", || {
            agent_service::unsync_agent_from_tool(&store, &id, &tool)
        })
        .map_err(AppError::db)
    })
    .await?
}

#[tauri::command]
pub async fn generate_agent_variant(
    id: String,
    tool: String,
    store: State<'_, Arc<SkillStore>>,
) -> Result<String, AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let path = agent_service::generate_agent_variant_file(&store, &id, &tool)
            .map_err(AppError::db)?;
        Ok(path.to_string_lossy().to_string())
    })
    .await?
}

#[tauri::command]
pub async fn export_agent(
    id: String,
    dest_dir: String,
    store: State<'_, Arc<SkillStore>>,
) -> Result<String, AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let path =
            agent_service::export_agent(&store, &id, &PathBuf::from(&dest_dir)).map_err(AppError::db)?;
        Ok(path.to_string_lossy().to_string())
    })
    .await?
}

#[tauri::command]
pub async fn delete_agent(
    id: String,
    store: State<'_, Arc<SkillStore>>,
) -> Result<(), AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::core::sync_metadata::with_repo_lock("delete agent", || {
            agent_service::delete_agent_artifact(&store, &id)
        })
        .map_err(AppError::db)
    })
    .await?
}

#[tauri::command]
pub async fn set_agent_enabled(
    id: String,
    enabled: bool,
    store: State<'_, Arc<SkillStore>>,
) -> Result<(), AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        store.update_agent_enabled(&id, enabled).map_err(AppError::db)
    })
    .await?
}

/// Path of the central agents root, for "open folder" affordances.
#[tauri::command]
pub async fn get_agents_root(store: State<'_, Arc<SkillStore>>) -> Result<String, AppError> {
    let _ = store.inner();
    Ok(central_repo::agents_dir().to_string_lossy().to_string())
}
