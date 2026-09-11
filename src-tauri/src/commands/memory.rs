use std::sync::Arc;

use tauri::State;

use crate::core::error::AppError;
use crate::core::memory::{self, sources::ReconcileReport, store::SavedMemory, sync::SyncReport};
use crate::core::skill_store::SkillStore;

#[tauri::command]
pub async fn memory_get_status(
    store: State<'_, Arc<SkillStore>>,
) -> Result<SyncReport, AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || memory::sync::sync_all_with(Some(&store), true))
        .await?
        .map_err(AppError::internal)
}

#[tauri::command]
pub async fn memory_sync(store: State<'_, Arc<SkillStore>>) -> Result<SyncReport, AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || memory::sync::sync_all_with(Some(&store), false))
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
    store: State<'_, Arc<SkillStore>>,
) -> Result<SavedMemory, AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let saved = memory::store::remember(
            &title,
            &description,
            &content,
            &memory_type,
            source_agent.as_deref(),
        )?;
        memory::sync::sync_all_with(Some(&store), false)?;
        Ok::<SavedMemory, anyhow::Error>(saved)
    })
    .await?
    .map_err(AppError::internal)
}
