use crate::core::error::AppError;
use crate::core::memory::{self, sources::ReconcileReport, store::SavedMemory, sync::SyncReport};

#[tauri::command]
pub async fn memory_get_status() -> Result<SyncReport, AppError> {
    tauri::async_runtime::spawn_blocking(|| memory::sync::sync_all(true))
        .await?
        .map_err(AppError::internal)
}

#[tauri::command]
pub async fn memory_sync() -> Result<SyncReport, AppError> {
    tauri::async_runtime::spawn_blocking(|| memory::sync::sync_all(false))
        .await?
        .map_err(AppError::internal)
}

#[tauri::command]
pub async fn memory_migrate(dry_run: bool) -> Result<ReconcileReport, AppError> {
    tauri::async_runtime::spawn_blocking(move || memory::sources::reconcile(dry_run))
        .await?
        .map_err(AppError::internal)
}

#[tauri::command]
pub async fn memory_remember(
    title: String,
    description: String,
    content: String,
    memory_type: String,
    source_agent: Option<String>,
) -> Result<SavedMemory, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let saved = memory::store::remember(
            &title,
            &description,
            &content,
            &memory_type,
            source_agent.as_deref(),
        )?;
        memory::sync::sync_all(false)?;
        Ok::<SavedMemory, anyhow::Error>(saved)
    })
    .await?
    .map_err(AppError::internal)
}
