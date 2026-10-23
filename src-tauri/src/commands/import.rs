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
pub async fn pst_set_enabled(s: State<'_, Services>, pst_id: i64, enabled: bool) -> Result<PstSource, AppError> {
    s.importer.set_enabled(pst_id, enabled).await
}

#[tauri::command]
pub async fn import_cancel(s: State<'_, Services>, pst_id: i64) -> Result<(), AppError> {
    s.importer.cancel(pst_id).await
}

#[tauri::command]
pub async fn pst_remove(s: State<'_, Services>, pst_id: i64) -> Result<(), AppError> {
    s.importer.remove(pst_id).await
}

#[tauri::command]
pub async fn import_errors(s: State<'_, Services>, req: ImportErrorsRequest) -> Result<ImportErrorPage, AppError> {
    s.importer.errors(req).await
}
