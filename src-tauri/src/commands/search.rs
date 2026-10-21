use outlooker_core::app::Services;
use outlooker_core::dto::error::AppError;
use outlooker_core::dto::search::{FilterOptions, MailDetail, SearchCount, SearchPage, SearchQuery, SearchRequest};
use tauri::State;

#[tauri::command]
pub async fn search_mail(s: State<'_, Services>, req: SearchRequest) -> Result<SearchPage, AppError> {
    s.search.search(req).await
}

#[tauri::command]
pub async fn search_count(s: State<'_, Services>, query: SearchQuery) -> Result<SearchCount, AppError> {
    s.search.count(query).await
}

#[tauri::command]
pub async fn filter_options(s: State<'_, Services>) -> Result<FilterOptions, AppError> {
    s.search.filter_options().await
}

#[tauri::command]
pub async fn mail_get(s: State<'_, Services>, id: i64) -> Result<MailDetail, AppError> {
    s.search.mail_get(id).await
}
