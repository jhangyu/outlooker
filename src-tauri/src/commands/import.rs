use outlooker_core::app::Services;
use outlooker_core::dto::error::AppError;
use outlooker_core::dto::import::{EnqueueResult, ImportErrorPage, ImportErrorsRequest, PstSource};
use tauri::State;

#[tauri::command]
pub async fn pst_list(s: State<'_, Services>) -> Result<Vec<PstSource>, AppError> {
    s.importer.list().await
}

#[tauri::command]
pub async fn pst_register(s: State<'_, Services>, paths: Vec<String>) -> Result<Vec<EnqueueResult>, AppError> {
    s.importer.register(paths).await
}

#[tauri::command]
pub async fn pst_set_paused(s: State<'_, Services>, pst_id: i64, paused: bool) -> Result<PstSource, AppError> {
    s.importer.set_paused(pst_id, paused).await
}

#[tauri::command]
pub async fn pst_unregister(s: State<'_, Services>, pst_id: i64) -> Result<(), AppError> {
    s.importer.unregister(pst_id).await
}

#[tauri::command]
pub async fn pst_remove(s: State<'_, Services>, pst_id: i64) -> Result<(), AppError> {
    s.importer.remove(pst_id).await
}

#[tauri::command]
pub async fn import_errors(s: State<'_, Services>, req: ImportErrorsRequest) -> Result<ImportErrorPage, AppError> {
    s.importer.errors(req).await
}
