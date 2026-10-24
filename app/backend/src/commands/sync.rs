use outlooker_core::app::Services;
use outlooker_core::dto::error::AppError;
use outlooker_core::dto::sync::{SyncSchedule, SyncStatus};
use tauri::State;

#[tauri::command]
pub async fn sync_now(s: State<'_, Services>, pst_id: Option<i64>) -> Result<(), AppError> {
    s.scheduler.sync_now(pst_id).await
}

#[tauri::command]
pub async fn sync_status(s: State<'_, Services>) -> Result<SyncStatus, AppError> {
    s.scheduler.status().await
}

#[tauri::command]
pub async fn sync_schedule_set(s: State<'_, Services>, schedule: SyncSchedule) -> Result<SyncStatus, AppError> {
    s.scheduler.set_schedule(schedule).await
}
