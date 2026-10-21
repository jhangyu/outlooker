use outlooker_core::app::Services;
use outlooker_core::dto::app::{AppStatus, InitializeRequest};
use outlooker_core::dto::error::AppError;
use outlooker_core::dto::settings::DbStatus;
use tauri::State;

#[tauri::command]
pub async fn app_status(s: State<'_, Services>) -> Result<AppStatus, AppError> {
    Ok(s.supervisor.status().await)
}

#[tauri::command]
pub async fn app_initialize(s: State<'_, Services>, req: InitializeRequest) -> Result<(), AppError> {
    s.supervisor.initialize(req).await
}

#[tauri::command]
pub async fn app_retry(s: State<'_, Services>) -> Result<(), AppError> {
    s.supervisor.retry().await
}

#[tauri::command]
pub async fn data_dir_move(s: State<'_, Services>, target: String) -> Result<(), AppError> {
    s.supervisor.move_data_dir(target).await
}

#[tauri::command]
pub async fn db_status(s: State<'_, Services>) -> Result<DbStatus, AppError> {
    s.supervisor.db_status(s.maintenance.current()).await
}
