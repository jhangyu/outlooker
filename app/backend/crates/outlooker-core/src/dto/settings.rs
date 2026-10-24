use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::app::{AppPhase, Language, LibraryCounts};
use super::error::AppError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub language: Language,
    pub webp_quality: u8,
    pub data_dir: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    pub language: Option<Language>,
    pub webp_quality: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum MaintenanceTask {
    Analyze,
    Vacuum,
    Reindex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum MaintenanceRunState {
    Running,
    Done,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceState {
    pub task: MaintenanceTask,
    pub state: MaintenanceRunState,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub error: Option<AppError>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DbStatus {
    pub phase: AppPhase,
    pub postgres_version: String,
    pub data_dir: String,
    pub database_size_bytes: i64,
    pub counts: LibraryCounts,
    pub image_count: i64,
    pub attachment_count: i64,
    pub indexes: Vec<IndexStatus>,
    pub last_vacuum_at: Option<String>,
    pub last_analyze_at: Option<String>,
    pub maintenance: Option<MaintenanceState>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct IndexStatus {
    pub name: String,
    pub table: String,
    pub size_bytes: i64,
    pub valid: bool,
}
