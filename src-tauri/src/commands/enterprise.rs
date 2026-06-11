use std::sync::Mutex;

use crate::core::{
    enterprise_api::{EnterpriseApi, EnterpriseSkill, FrontendLoginResponse},
    error::AppError,
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

#[tauri::command]
pub async fn enterprise_login(
    username: String,
    password: String,
) -> Result<FrontendLoginResponse, AppError> {
    let mut api_guard = get_or_create_api();
    let api = api_guard.as_mut().unwrap();

    let resp = api
        .login(&username, &password)
        .map_err(|e| AppError::internal(format!("Enterprise login failed: {}", e)))?;

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
    Ok(())
}
