use outlooker_core::app::Services;
use outlooker_core::dto::error::AppError;
use outlooker_core::dto::settings::{MaintenanceTask, Settings, SettingsPatch};
use tauri::State;

#[tauri::command]
pub async fn settings_get(s: State<'_, Services>) -> Result<Settings, AppError> {
    s.settings.get(&s.db).await
}

#[tauri::command]
pub async fn settings_update(s: State<'_, Services>, patch: SettingsPatch) -> Result<Settings, AppError> {
    s.settings.update(&s.db, patch).await
}

#[tauri::command]
pub async fn maintenance_run(s: State<'_, Services>, task: MaintenanceTask) -> Result<(), AppError> {
    s.maintenance.run(task).await
}
