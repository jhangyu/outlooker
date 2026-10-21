use outlooker_core::app::Services;
use outlooker_core::dto::error::AppError;
use outlooker_core::dto::sql::{SavedQuery, SavedQueryInput, SqlExecuteRequest, SqlResult};
use tauri::State;

#[tauri::command]
pub async fn saved_query_list(s: State<'_, Services>) -> Result<Vec<SavedQuery>, AppError> {
    s.sql.saved_list().await
}

#[tauri::command]
pub async fn saved_query_create(s: State<'_, Services>, input: SavedQueryInput) -> Result<SavedQuery, AppError> {
    s.sql.saved_create(input).await
}

#[tauri::command]
pub async fn saved_query_update(s: State<'_, Services>, id: i64, input: SavedQueryInput) -> Result<SavedQuery, AppError> {
    s.sql.saved_update(id, input).await
}

#[tauri::command]
pub async fn saved_query_delete(s: State<'_, Services>, id: i64) -> Result<(), AppError> {
    s.sql.saved_delete(id).await
}

#[tauri::command]
pub async fn sql_execute(s: State<'_, Services>, req: SqlExecuteRequest) -> Result<SqlResult, AppError> {
    s.sql.execute(req).await
}

#[tauri::command]
pub async fn sql_cancel(s: State<'_, Services>, execution_id: String) -> Result<(), AppError> {
    s.sql.cancel(execution_id).await
}
