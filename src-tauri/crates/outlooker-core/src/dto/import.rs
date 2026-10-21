use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::error::AppError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct PstSource {
    pub id: i64,
    pub path: String,
    pub display_name: String,
    pub file_size_bytes: i64,
    pub status: ImportStatus,
    pub progress: Option<ImportProgress>,
    pub message_count: i64,
    pub new_message_count: i64,
    pub error_count: i64,
    pub queued_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum ImportStatus {
    Queued,
    Running,
    Cancelling,
    Cancelled,
    Failed,
    Completed,
    Removing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ImportProgress {
    pub pst_id: i64,
    pub phase: ImportPhase,
    pub items_total: Option<i64>,
    pub items_done: i64,
    pub stored: i64,
    pub duplicates: i64,
    pub errors: i64,
    pub skipped_non_mail: i64,
    pub images_converted: i64,
    pub bytes_per_sec: Option<i64>,
    pub eta_seconds: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum ImportPhase {
    Scanning,
    Importing,
    Finalizing,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "result", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum EnqueueResult {
    Queued { pst: PstSource },
    Rejected { path: String, error: AppError },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ImportErrorsRequest {
    pub pst_id: i64,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ImportErrorPage {
    pub rows: Vec<ImportErrorEntry>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ImportErrorEntry {
    pub id: i64,
    pub occurred_at: String,
    pub folder_path: Option<String>,
    pub pst_item_id: Option<i64>,
    pub stage: ImportErrorStage,
    pub severity: ImportErrorSeverity,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum ImportErrorStage {
    Open,
    Scan,
    Parse,
    Html,
    Image,
    Store,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum ImportErrorSeverity {
    ItemSkipped,
    Degraded,
    JobFailed,
}
