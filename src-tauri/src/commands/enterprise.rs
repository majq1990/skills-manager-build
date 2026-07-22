use std::sync::Mutex;

use crate::core::{
    enterprise_api::{EnterpriseApi, EnterpriseSkill, FrontendLoginResponse, UploadResponse},
    error::AppError,
    skill_packer,
};

/// Global enterprise API instance (protected by Mutex for thread safety)
static ENTERPRISE_API: Mutex<Option<EnterpriseApi>> = Mutex::new(None);

/// Default enterprise server URL
const DEFAULT_ENTERPRISE_URL: &str = "https://demo.egova.com.cn/skill-api";

fn get_or_create_api() -> std::sync::MutexGuard<'static, Option<EnterpriseApi>> {
    let mut api = ENTERPRISE_API.lock().unwrap();
    if api.is_none() {
        *api = Some(EnterpriseApi::new(DEFAULT_ENTERPRISE_URL));
    }
    api
}

/// 拉服务端企业 skill 列表，返回 name -> 最新版本 映射（供自动更新比较）。
pub(crate) fn fetch_enterprise_versions() -> Result<std::collections::HashMap<String, String>, AppError> {
    let api_guard = get_or_create_api();
    let api = api_guard.as_ref().unwrap();
    if api.get_token().is_none() {
        return Err(AppError::internal("Not authenticated"));
    }
    let skills = api
        .list_skills()
        .map_err(|e| AppError::internal(format!("Failed to list enterprise skills: {}", e)))?;
    Ok(skills.into_iter().map(|s| (s.name, s.version)).collect())
}

/// 从 app 资源目录部署 feedback-monitor 插件到已安装 agent；失败只记日志。
pub(crate) fn deploy_feedback_from_app(app: &tauri::AppHandle) {
    use tauri::Manager;
    match app.path().resource_dir() {
        Ok(rd) => {
            let bundle = crate::core::feedback_deploy::resource_bundle(&rd);
            match crate::core::feedback_deploy::deploy(&bundle) {
                Ok(agents) => log::info!("feedback-monitor 部署到: {:?}", agents),
                Err(e) => log::warn!("feedback-monitor 部署失败: {e}"),
            }
        }
        Err(e) => log::warn!("resource_dir 解析失败，跳过 feedback-monitor 部署: {e}"),
    }
}

#[tauri::command]
pub async fn enterprise_login(
    app: tauri::AppHandle,
    store: tauri::State<'_, std::sync::Arc<crate::core::skill_store::SkillStore>>,
    username: String,
    password: String,
) -> Result<FrontendLoginResponse, AppError> {
    let resp = {
        let mut api_guard = get_or_create_api();
        let api = api_guard.as_mut().unwrap();
        api.login(&username, &password)
            .map_err(|e| AppError::internal(format!("Enterprise login failed: {}", e)))?
    };

    // 登录成功后：①把 JWT 投递给各 agent 的 token 文件；②联动部署插件到已装 agent；
    // ③后台自动更新已装的企业 skill（有 token 才能拉/下载）。均失败不影响登录本身。
    if resp.success {
        crate::core::feedback_token::write_token(&resp.token);
        deploy_feedback_from_app(&app);

        let store = store.inner().clone();
        let app_bg = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            crate::commands::skills::auto_update_enterprise_skills(&store, &app_bg);
        });
    }

    Ok(EnterpriseApi::map_login_response(resp))
}

#[tauri::command]
pub async fn enterprise_list_skills() -> Result<Vec<EnterpriseSkill>, AppError> {
    let api_guard = get_or_create_api();
    let api = api_guard.as_ref().unwrap();

    api.list_skills()
        .map_err(|e| AppError::internal(format!("Failed to list enterprise skills: {}", e)))
}

#[tauri::command]
pub async fn enterprise_get_tags() -> Result<Vec<String>, AppError> {
    let api_guard = get_or_create_api();
    let api = api_guard.as_ref().unwrap();

    api.get_tags()
        .map_err(|e| AppError::internal(format!("Failed to get enterprise tags: {}", e)))
}

#[tauri::command]
pub async fn enterprise_search_by_tag(tag: String) -> Result<Vec<EnterpriseSkill>, AppError> {
    let api_guard = get_or_create_api();
    let api = api_guard.as_ref().unwrap();

    api.search_by_tag(&tag)
        .map_err(|e| AppError::internal(format!("Failed to search by tag: {}", e)))
}

#[tauri::command]
pub async fn enterprise_search_by_query(query: String) -> Result<Vec<EnterpriseSkill>, AppError> {
    let api_guard = get_or_create_api();
    let api = api_guard.as_ref().unwrap();

    api.search_by_query(&query)
        .map_err(|e| AppError::internal(format!("Failed to search enterprise skills: {}", e)))
}

#[tauri::command]
pub async fn enterprise_is_authenticated() -> Result<bool, AppError> {
    let api_guard = get_or_create_api();
    let api = api_guard.as_ref().unwrap();

    Ok(api.get_token().is_some())
}

#[tauri::command]
pub async fn enterprise_logout() -> Result<(), AppError> {
    let mut api_guard = get_or_create_api();
    let api = api_guard.as_mut().unwrap();

    api.set_token(String::new());
    crate::core::feedback_token::clear_token();
    Ok(())
}

/// 把 feedback-monitor 插件联动部署到已安装的 agent（opencode/Claude Code/WorkBuddy）。
/// bundle_src = feedback-monitor bundle 目录（后续由 app 资源目录提供；现阶段由调用方传入）。
/// 返回已部署的 agent key 列表。
#[tauri::command]
pub async fn feedback_deploy_plugin(
    app: tauri::AppHandle,
    bundle_src: Option<String>,
) -> Result<Vec<String>, AppError> {
    let bundle = match bundle_src {
        Some(s) => std::path::PathBuf::from(s),
        None => {
            use tauri::Manager;
            let rd = app
                .path()
                .resource_dir()
                .map_err(|e| AppError::internal(format!("resource_dir 解析失败: {}", e)))?;
            crate::core::feedback_deploy::resource_bundle(&rd)
        }
    };
    tauri::async_runtime::spawn_blocking(move || {
        crate::core::feedback_deploy::deploy(&bundle)
            .map_err(|e| AppError::internal(format!("Feedback plugin deploy failed: {}", e)))
    })
    .await?
}

#[tauri::command]
pub async fn enterprise_submit_feedback(
    feedback_type: String,
    skill: String,
    title: String,
    description: String,
) -> Result<(), AppError> {
    let api_guard = get_or_create_api();
    let api = api_guard.as_ref().unwrap();
    if api.get_token().is_none() {
        return Err(AppError::internal("Not authenticated"));
    }
    api.submit_feedback(&feedback_type, &skill, &title, &description)
        .map_err(|e| AppError::internal(format!("Submit feedback failed: {}", e)))
}

/// 用当前登录态（全局 ENTERPRISE_API 内存 token）下载企业技能 zip 字节。
/// 同步函数，必须在 spawn_blocking 内调用（reqwest::blocking + std Mutex）。
/// 安装命令 `enterprise_install_skill` 在 skills.rs，复用本地安装 helper。
pub fn download_enterprise_zip(name: &str, version: &str) -> Result<Vec<u8>, AppError> {
    let api_guard = get_or_create_api();
    let api = api_guard.as_ref().unwrap();
    if api.get_token().is_none() {
        return Err(AppError::internal("Not authenticated"));
    }
    api.download_skill(name, version)
        .map_err(|e| AppError::internal(format!("Download failed: {}", e)))
}

/// 同步打包 central_path 目录内容为 zip 并上传到企业服务器。
/// 必须在 spawn_blocking 内调用（pack 走 std::fs，upload 走 reqwest::blocking + std Mutex）。
fn pack_and_upload(
    name: &str,
    central_path: &str,
    version: Option<&str>,
    visibility: Option<&str>,
) -> Result<UploadResponse, AppError> {
    let zip_bytes = skill_packer::pack_dir(std::path::Path::new(central_path))
        .map_err(|e| AppError::internal(format!("Failed to package skill: {}", e)))?;

    let api_guard = get_or_create_api();
    let api = api_guard.as_ref().unwrap();
    if api.get_token().is_none() {
        return Err(AppError::internal("Not authenticated"));
    }
    api.upload_skill(name, zip_bytes, version, visibility)
        .map_err(|e| AppError::internal(format!("Upload failed: {}", e)))
}

/// 把本地技能（central_path 目录内容）打包上传/发布到企业服务器。
/// 服务端做安全扫描：不过返回 HTTP 400，错误信息会一并带出。
/// version 留空 = 服务端自动递增。
#[tauri::command]
pub async fn enterprise_upload_skill(
    name: String,
    central_path: String,
    version: Option<String>,
    visibility: Option<String>,
) -> Result<UploadResponse, AppError> {
    // 上传前先检查登录态（无 token 直接报错，避免白白打包）。
    {
        let api_guard = get_or_create_api();
        let api = api_guard.as_ref().unwrap();
        if api.get_token().is_none() {
            return Err(AppError::internal("Not authenticated"));
        }
    }

    tauri::async_runtime::spawn_blocking(move || {
        pack_and_upload(&name, &central_path, version.as_deref(), visibility.as_deref())
    })
    .await?
}
