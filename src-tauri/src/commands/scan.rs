use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;

use crate::core::{
    central_repo, error::AppError, installer, scanner, skill_store::SkillStore, tool_adapters,
};

#[derive(Debug, Serialize)]
pub struct ScanResultDto {
    pub tools_scanned: usize,
    pub skills_found: usize,
    pub groups: Vec<scanner::DiscoveredGroup>,
}

/// Run a local-skill discovery scan, persist results into `discovered_skills`,
/// and return the grouped result. Shared by the tauri command and the
/// startup auto-scan in `lib.rs`.
pub fn run_discovery_scan(store: &SkillStore) -> Result<ScanResultDto, AppError> {
    let all_targets = store.get_all_targets().map_err(AppError::db)?;
    let managed_paths: Vec<String> = all_targets.iter().map(|t| t.target_path.clone()).collect();
    let managed_skills = store.get_all_skills().map_err(AppError::db)?;

    let adapters = tool_adapters::all_tool_adapters(store);
    let mut plan = scanner::scan_local_skills_with_adapters(&managed_paths, &adapters)
        .map_err(AppError::io)?;

    for rec in &mut plan.discovered {
        if let Some(name) = rec.name_guess.as_deref() {
            if let Some(existing) = managed_skills.iter().find(|skill| skill.name == name) {
                rec.imported_skill_id = Some(existing.id.clone());
            }
        }
    }

    store.clear_discovered().map_err(AppError::db)?;
    for rec in &plan.discovered {
        store.insert_discovered(rec).map_err(AppError::db)?;
    }

    let all_discovered = store.get_all_discovered().map_err(AppError::db)?;
    let groups = scanner::group_discovered(&all_discovered);

    Ok(ScanResultDto {
        tools_scanned: plan.tools_scanned,
        skills_found: plan.skills_found,
        groups,
    })
}

#[tauri::command]
pub async fn scan_local_skills(
    store: State<'_, Arc<SkillStore>>,
) -> Result<ScanResultDto, AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || run_discovery_scan(&store)).await?
}

/// Read-only counterpart: return whatever is already in `discovered_skills`
/// without touching the filesystem. Used on app start and when the UI opens
/// the Local tab, so the user sees cached results instantly.
#[tauri::command]
pub async fn get_discovered_groups(
    store: State<'_, Arc<SkillStore>>,
) -> Result<ScanResultDto, AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let discovered = store.get_all_discovered().map_err(AppError::db)?;
        let groups = scanner::group_discovered(&discovered);
        let tools_scanned = groups
            .iter()
            .flat_map(|g| g.locations.iter().map(|l| l.tool.clone()))
            .collect::<std::collections::HashSet<_>>()
            .len();
        Ok(ScanResultDto {
            tools_scanned,
            skills_found: discovered.len(),
            groups,
        })
    })
    .await?
}

#[tauri::command]
pub async fn import_existing_skill(
    source_path: String,
    name: Option<String>,
    store: State<'_, Arc<SkillStore>>,
) -> Result<(), AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let path = PathBuf::from(&source_path);
        let resolved_name =
            installer::resolve_local_skill_name(&path, name.as_deref()).map_err(AppError::io)?;
        let central_path = central_repo::skills_dir().join(&resolved_name);

        if let Some(existing) = store
            .get_skill_by_central_path(&central_path.to_string_lossy())
            .map_err(AppError::db)?
        {
            if let Ok(Some(scenario_id)) = store.get_active_scenario_id() {
                store
                    .add_skill_to_scenario(&scenario_id, &existing.id)
                    .map_err(AppError::db)?;
                crate::commands::scenarios::sync_skill_to_scenario_active_tools(
                    &store,
                    &scenario_id,
                    &existing.id,
                )?;
            }
            return Ok(());
        }

        let result = installer::install_from_local_to_destination(
            &path,
            Some(&resolved_name),
            &central_path,
        )
        .map_err(AppError::io)?;

        let now = chrono::Utc::now().timestamp_millis();
        let id = uuid::Uuid::new_v4().to_string();

        let record = crate::core::skill_store::SkillRecord {
            id: id.clone(),
            name: result.name,
            description: result.description,
            source_type: "import".to_string(),
            source_ref: Some(source_path),
            source_ref_resolved: None,
            source_subpath: None,
            source_branch: None,
            source_revision: None,
            remote_revision: None,
            central_path: result.central_path.to_string_lossy().to_string(),
            content_hash: Some(result.content_hash),
            enabled: true,
            created_at: now,
            updated_at: now,
            status: "ok".to_string(),
            update_status: "local_only".to_string(),
            last_checked_at: Some(now),
            last_check_error: None,
        };

        store.insert_skill(&record).map_err(AppError::db)?;

        // Auto-add to active scenario + sync to enabled agent tools
        if let Ok(Some(scenario_id)) = store.get_active_scenario_id() {
            store
                .add_skill_to_scenario(&scenario_id, &id)
                .map_err(AppError::db)?;
            crate::commands::scenarios::sync_skill_to_scenario_active_tools(
                &store,
                &scenario_id,
                &id,
            )?;
        }

        Ok(())
    })
    .await?
}

#[tauri::command]
pub async fn import_all_discovered(store: State<'_, Arc<SkillStore>>) -> Result<(), AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let discovered = store.get_all_discovered().map_err(AppError::db)?;
        let groups = scanner::group_discovered(&discovered);

        let active_scenario = store.get_active_scenario_id().ok().flatten();

        for group in groups {
            if group.imported {
                continue;
            }
            if let Some(first) = group.locations.first() {
                let path = PathBuf::from(&first.found_path);
                let central_path = central_repo::skills_dir().join(&group.name);

                if let Ok(Some(existing)) =
                    store.get_skill_by_central_path(&central_path.to_string_lossy())
                {
                    if let Some(ref scenario_id) = active_scenario {
                        if store.add_skill_to_scenario(scenario_id, &existing.id).is_ok() {
                            crate::commands::scenarios::sync_skill_to_scenario_active_tools(
                                &store,
                                scenario_id,
                                &existing.id,
                            )
                            .ok();
                        }
                    }
                    continue;
                }

                if let Ok(result) = installer::install_from_local_to_destination(
                    &path,
                    Some(&group.name),
                    &central_path,
                ) {
                    let now = chrono::Utc::now().timestamp_millis();
                    let id = uuid::Uuid::new_v4().to_string();
                    let record = crate::core::skill_store::SkillRecord {
                        id: id.clone(),
                        name: result.name,
                        description: result.description,
                        source_type: "import".to_string(),
                        source_ref: Some(first.found_path.clone()),
                        source_ref_resolved: None,
                        source_subpath: None,
                        source_branch: None,
                        source_revision: None,
                        remote_revision: None,
                        central_path: result.central_path.to_string_lossy().to_string(),
                        content_hash: Some(result.content_hash),
                        enabled: true,
                        created_at: now,
                        updated_at: now,
                        status: "ok".to_string(),
                        update_status: "local_only".to_string(),
                        last_checked_at: Some(now),
                        last_check_error: None,
                    };
                    store.insert_skill(&record).ok();

                    if let Some(ref scenario_id) = active_scenario {
                        if store.add_skill_to_scenario(scenario_id, &id).is_ok() {
                            crate::commands::scenarios::sync_skill_to_scenario_active_tools(
                                &store,
                                scenario_id,
                                &id,
                            )
                            .ok();
                        }
                    }
                }
            }
        }

        Ok(())
    })
    .await?
}
