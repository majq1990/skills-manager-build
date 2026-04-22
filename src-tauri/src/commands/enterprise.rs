use std::sync::Arc;
use tauri::State;

use crate::core::{
    central_repo,
    error::AppError,
    enterprise_api::{
        EnterpriseApi, EnterpriseSkill, EnterpriseSkillDetail, ScanStatusResponse,
        ScanTriggerResponse, UploadResponse, VersionInfo,
    },
    enterprise_auth::{EnterpriseAuth, LoginResponse, StoredAuth},
    installer,
    skill_packer,
    skill_store::{SkillRecord, SkillStore},
};

#[tauri::command]
pub async fn enterprise_login(
    username: String,
    password: String,
    store: State<'_, Arc<SkillStore>>,
) -> Result<LoginResponse, AppError> {
    let store = store.inner().clone();
    let server_url = store
        .get_setting("enterprise_server_url")
        .map_err(|e| AppError::internal(format!("Failed to read settings: {}", e)))?
        .unwrap_or_else(|| "https://demo.egova.com.cn/skill-api/".to_string());
    let auth = EnterpriseAuth::new();
    auth.login(&store, &server_url, &username, &password)
        .await
        .map_err(|e| AppError::network(e.to_string()))
}

#[tauri::command]
pub async fn enterprise_logout(store: State<'_, Arc<SkillStore>>) -> Result<(), AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        EnterpriseAuth::logout(&store).map_err(AppError::db)
    })
    .await?
}

#[tauri::command]
pub async fn enterprise_get_auth(
    store: State<'_, Arc<SkillStore>>,
) -> Result<Option<StoredAuth>, AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        EnterpriseAuth::get_auth(&store).map_err(AppError::db)
    })
    .await?
}

#[tauri::command]
pub async fn enterprise_check_auth(store: State<'_, Arc<SkillStore>>) -> Result<bool, AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        EnterpriseAuth::is_authenticated(&store).map_err(AppError::db)
    })
    .await?
}

#[tauri::command]
pub async fn enterprise_is_support_dept(
    store: State<'_, Arc<SkillStore>>,
) -> Result<bool, AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        EnterpriseAuth::is_support_dept(&store).map_err(AppError::db)
    })
    .await?
}

#[tauri::command]
pub async fn enterprise_list_skills(
    store: State<'_, Arc<SkillStore>>,
) -> Result<Vec<EnterpriseSkill>, AppError> {
    let store = store.inner().clone();
    // Clone store early for use later
    let store_for_list = store.clone();

    let auth = tauri::async_runtime::spawn_blocking(move || {
        EnterpriseAuth::get_auth(&store).map_err(AppError::db)
    })
    .await??;

    match auth {
        Some(stored) => {
            let api = EnterpriseApi::new();
            let skills = api.list_skills(&stored.server_url, &stored.token)
                .await
                .map_err(|e| AppError::network(e.to_string()))?;

            // Get installed skill names from local database
            let installed_skills: Vec<String> = tauri::async_runtime::spawn_blocking(move || {
                match store_for_list.get_all_skills() {
                    Ok(skills) => skills.into_iter().map(|s| s.name).collect::<Vec<String>>(),
                    Err(_) => vec![],
                }
            })
            .await
            .unwrap_or_default();

            // Mark skills as installed if they exist locally
            let skills_with_install_status: Vec<EnterpriseSkill> = skills
                .into_iter()
                .map(|mut skill| {
                    skill.installed = installed_skills.contains(&skill.name);
                    skill
                })
                .collect();

            Ok(skills_with_install_status)
        }
        None => Err(AppError::unauthorized("Not authenticated")),
    }
}

#[tauri::command]
pub async fn enterprise_get_skill(
    name: String,
    store: State<'_, Arc<SkillStore>>,
) -> Result<EnterpriseSkillDetail, AppError> {
    let store = store.inner().clone();

    let auth = tauri::async_runtime::spawn_blocking(move || {
        EnterpriseAuth::get_auth(&store).map_err(AppError::db)
    })
    .await??;

    match auth {
        Some(stored) => {
            let api = EnterpriseApi::new();
            api.get_skill(&stored.server_url, &stored.token, &name)
                .await
                .map_err(|e| AppError::network(e.to_string()))
        }
        None => Err(AppError::unauthorized("Not authenticated")),
    }
}

#[tauri::command]
pub async fn enterprise_download_skill(
    name: String,
    store: State<'_, Arc<SkillStore>>,
) -> Result<Vec<u8>, AppError> {
    let store = store.inner().clone();

    let auth = tauri::async_runtime::spawn_blocking(move || {
        EnterpriseAuth::get_auth(&store).map_err(AppError::db)
    })
    .await??;

    match auth {
        Some(stored) => {
            let api = EnterpriseApi::new();
            api.download_skill(&stored.server_url, &stored.token, &name)
                .await
                .map_err(|e| AppError::network(e.to_string()))
        }
        None => Err(AppError::unauthorized("Not authenticated")),
    }
}

#[tauri::command]
pub async fn enterprise_install_skill(
    name: String,
    store: State<'_, Arc<SkillStore>>,
) -> Result<String, AppError> {
    log::info!("[enterprise_install_skill] Starting installation for skill: {}", name);

    let store = store.inner().clone();
    // Clone store early for use in the install closure
    let store_for_install = store.clone();

    let auth = tauri::async_runtime::spawn_blocking(move || {
        EnterpriseAuth::get_auth(&store).map_err(AppError::db)
    })
    .await??;

    let auth = match auth {
        Some(stored) => stored,
        None => {
            log::error!("[enterprise_install_skill] Not authenticated");
            return Err(AppError::unauthorized("Not authenticated"));
        }
    };

    // Capture the currently active scenario so the newly installed skill can be
    // immediately enabled within it (matches the behavior of install_local/install_git).
    let store_for_scenario = store_for_install.clone();
    let active_scenario_id = tauri::async_runtime::spawn_blocking(move || {
        store_for_scenario
            .get_active_scenario_id()
            .map_err(AppError::db)
    })
    .await??;

    log::info!(
        "[enterprise_install_skill] Downloading skill from: {} (active_scenario={:?})",
        auth.server_url,
        active_scenario_id
    );

    let api = EnterpriseApi::new();
    let zip_bytes = api
        .download_skill(&auth.server_url, &auth.token, &name)
        .await
        .map_err(|e| {
            log::error!("[enterprise_install_skill] Download failed: {}", e);
            AppError::network(format!("Download failed: {}", e))
        })?;

    log::info!("[enterprise_install_skill] Downloaded {} bytes", zip_bytes.len());

    // Verify zip file magic bytes
    if zip_bytes.len() < 4 {
        log::error!("[enterprise_install_skill] Downloaded file too small: {} bytes", zip_bytes.len());
        return Err(AppError::internal("Downloaded file is too small or empty"));
    }

    // Check for ZIP magic number (PK\x03\x04)
    if zip_bytes[0] != 0x50 || zip_bytes[1] != 0x4B || zip_bytes[2] != 0x03 || zip_bytes[3] != 0x04 {
        // Check for empty response or HTML error page
        let preview = String::from_utf8_lossy(&zip_bytes[..zip_bytes.len().min(100)]);
        log::error!("[enterprise_install_skill] Downloaded file is not a valid ZIP. Preview: {}", preview);
        return Err(AppError::internal(format!("Downloaded file is not a valid ZIP archive. Preview: {}", preview)));
    }

    // Install to central repo
    let skills_dir = central_repo::skills_dir();
    log::info!("[enterprise_install_skill] Installing to: {:?}", skills_dir);

    let install_result = tauri::async_runtime::spawn_blocking(move || {
        log::info!("[enterprise_install_skill] Starting install in spawn_blocking");

        let temp_dir = tempfile::tempdir().map_err(|e| {
            log::error!("[enterprise_install_skill] Failed to create temp dir: {}", e);
            AppError::io(e)
        })?;
        let zip_path = temp_dir.path().join("skill.zip");

        log::info!("[enterprise_install_skill] Writing zip to: {:?}", zip_path);
        std::fs::write(&zip_path, &zip_bytes).map_err(|e| {
            log::error!("[enterprise_install_skill] Failed to write zip: {}", e);
            AppError::io(e)
        })?;

        // Verify file was written
        let written_size = std::fs::metadata(&zip_path)
            .map(|m| m.len())
            .unwrap_or(0);
        log::info!("[enterprise_install_skill] Written {} bytes to disk", written_size);

        log::info!("[enterprise_install_skill] Extracting zip: {:?}", zip_path);
        let result = installer::install_from_local(&zip_path, Some(&name))
            .map_err(|e| {
                log::error!("[enterprise_install_skill] Install failed: {}", e);
                AppError::internal(format!("Install failed: {}", e))
            })?;

        log::info!("[enterprise_install_skill] Successfully installed: {}, central_path: {:?}", result.name, result.central_path);

        // Insert skill record to database
        let now = chrono::Utc::now().timestamp_millis();
        let skill_id = uuid::Uuid::new_v4().to_string();
        let central_path_str = result.central_path.to_string_lossy().to_string();

        log::info!("[enterprise_install_skill] Preparing skill record: id={}, name={}, source_type=enterprise, central_path={}",
            skill_id, result.name, central_path_str);

        let record = SkillRecord {
            id: skill_id.clone(),
            name: result.name.clone(),
            description: result.description.clone(),
            source_type: "enterprise".to_string(),
            source_ref: Some(name.clone()),
            source_ref_resolved: None,
            source_subpath: None,
            source_branch: None,
            source_revision: None,
            remote_revision: None,
            central_path: central_path_str,
            content_hash: Some(result.content_hash.clone()),
            enabled: true,
            created_at: now,
            updated_at: now,
            status: "ok".to_string(),
            update_status: "unknown".to_string(),
            last_checked_at: Some(now),
            last_check_error: None,
        };

        log::info!("[enterprise_install_skill] Inserting skill record to database...");
        match store_for_install.insert_skill(&record) {
            Ok(_) => {
                log::info!("[enterprise_install_skill] Skill record inserted successfully: {}", skill_id);
            }
            Err(e) => {
                log::error!("[enterprise_install_skill] Failed to insert skill record: {}", e);
                return Err(AppError::db(e));
            }
        }

        // Attach the newly installed skill to the currently active scenario and
        // sync it to every enabled agent tool, so it shows up under "当前场景已
        // 启用" in My Skills and inside tool folders (e.g. ~/.claude/skills) right away.
        if let Some(scenario_id) = active_scenario_id.as_deref() {
            if let Err(e) = store_for_install.add_skill_to_scenario(scenario_id, &skill_id) {
                log::warn!(
                    "[enterprise_install_skill] Failed to add skill {} to active scenario {}: {}",
                    skill_id, scenario_id, e
                );
            } else {
                log::info!(
                    "[enterprise_install_skill] Added skill {} to active scenario {}",
                    skill_id, scenario_id
                );
                if let Err(e) = crate::commands::scenarios::sync_skill_to_scenario_active_tools(
                    &store_for_install,
                    scenario_id,
                    &skill_id,
                ) {
                    log::warn!(
                        "[enterprise_install_skill] sync_skill_to_scenario_active_tools failed for {}: {}",
                        skill_id, e
                    );
                }
            }
        }

        Ok(result.name)
    })
    .await;

    match install_result {
        Ok(Ok(name)) => {
            log::info!("[enterprise_install_skill] Install completed successfully: {}", name);
            Ok(name)
        }
        Ok(Err(e)) => {
            log::error!("[enterprise_install_skill] Install failed: {}", e);
            Err(e)
        }
        Err(e) => {
            log::error!("[enterprise_install_skill] Task failed: {}", e);
            Err(AppError::internal(format!("Task failed: {}", e)))
        }
    }
}

// ── Upload / Scan / History commands ──

#[tauri::command]
pub async fn enterprise_upload_skill(
    skill_id: String,
    version: Option<String>,
    category: Option<String>,
    store: State<'_, Arc<SkillStore>>,
) -> Result<UploadResponse, AppError> {
    let store = store.inner().clone();
    let store_for_auth = store.clone();

    let skill = tauri::async_runtime::spawn_blocking({
        let store = store.clone();
        let skill_id = skill_id.clone();
        move || {
            store
                .get_skill_by_id(&skill_id)
                .map_err(AppError::db)?
                .ok_or_else(|| AppError::not_found(format!("Skill {} not found", skill_id)))
        }
    })
    .await??;

    let is_support = tauri::async_runtime::spawn_blocking({
        let store = store_for_auth.clone();
        move || EnterpriseAuth::is_support_dept(&store).map_err(AppError::db)
    })
    .await??;

    if !is_support {
        return Err(AppError::unauthorized("Only support department can upload skills"));
    }

    let auth = tauri::async_runtime::spawn_blocking({
        let store = store_for_auth.clone();
        move || EnterpriseAuth::get_auth(&store).map_err(AppError::db)
    })
    .await??
    .ok_or_else(|| AppError::unauthorized("Not authenticated"))?;

    let central_path = std::path::PathBuf::from(&skill.central_path);
    let zip_bytes = tauri::async_runtime::spawn_blocking(move || {
        skill_packer::pack_skill(&central_path)
            .map_err(|e| AppError::internal(format!("Pack failed: {}", e)))
    })
    .await??;

    let api = EnterpriseApi::new();
    api.upload_skill(
        &auth.server_url,
        &auth.token,
        &skill.name,
        zip_bytes,
        version.as_deref(),
        category.as_deref(),
    )
    .await
    .map_err(|e| AppError::network(e.to_string()))
}

#[tauri::command]
pub async fn enterprise_check_scan(
    name: String,
    version: String,
    store: State<'_, Arc<SkillStore>>,
) -> Result<ScanStatusResponse, AppError> {
    let store = store.inner().clone();
    let auth = tauri::async_runtime::spawn_blocking(move || {
        EnterpriseAuth::get_auth(&store).map_err(AppError::db)
    })
    .await??
    .ok_or_else(|| AppError::unauthorized("Not authenticated"))?;

    let api = EnterpriseApi::new();
    api.get_scan_status(&auth.server_url, &auth.token, &name, &version)
        .await
        .map_err(|e| AppError::network(e.to_string()))
}

#[tauri::command]
pub async fn enterprise_trigger_scan(
    name: String,
    version: String,
    store: State<'_, Arc<SkillStore>>,
) -> Result<ScanTriggerResponse, AppError> {
    let store = store.inner().clone();
    let auth = tauri::async_runtime::spawn_blocking(move || {
        EnterpriseAuth::get_auth(&store).map_err(AppError::db)
    })
    .await??
    .ok_or_else(|| AppError::unauthorized("Not authenticated"))?;

    let api = EnterpriseApi::new();
    api.trigger_scan(&auth.server_url, &auth.token, &name, &version)
        .await
        .map_err(|e| AppError::network(e.to_string()))
}

#[tauri::command]
pub async fn enterprise_upload_history(
    name: String,
    store: State<'_, Arc<SkillStore>>,
) -> Result<Vec<VersionInfo>, AppError> {
    let store = store.inner().clone();
    let auth = tauri::async_runtime::spawn_blocking(move || {
        EnterpriseAuth::get_auth(&store).map_err(AppError::db)
    })
    .await??
    .ok_or_else(|| AppError::unauthorized("Not authenticated"))?;

    let api = EnterpriseApi::new();
    api.get_upload_history(&auth.server_url, &auth.token, &name)
        .await
        .map_err(|e| AppError::network(e.to_string()))
}