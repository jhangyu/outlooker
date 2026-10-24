use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, thiserror::Error)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum AppError {
    #[error("database not ready")]
    DbNotReady,
    #[error("{entity} {id} not found")]
    NotFound { entity: String, id: String },
    #[error("invalid input {field}: {reason}")]
    InvalidInput { field: String, reason: String },
    #[error("query timed out after {timeout_ms} ms")]
    QueryTimeout { timeout_ms: u32 },
    #[error("cancelled")]
    Cancelled,
    #[error("SQL error {sqlstate}: {message}")]
    SqlError { sqlstate: String, message: String, position: Option<u32> },
    #[error("read-only violation")]
    ReadOnlyViolation,
    #[error("busy: {reason:?}")]
    Busy { reason: BusyReason },
    #[error("file access {path}: {reason}")]
    FileAccess { path: String, reason: String },
    #[error("not a PST file: {path}")]
    NotAPst { path: String },
    #[error("already imported as PST {pst_id}")]
    AlreadyImported { pst_id: i64 },
    /// The data directory holds a database this build cannot open (edited or unknown migration, other PostgreSQL
    /// major). The failed screen offers `app_reset_database` only for this kind.
    #[error("incompatible database: {detail}")]
    IncompatibleDatabase { detail: String },
    #[error("internal error: {detail}")]
    Internal { detail: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum BusyReason {
    ImportRunning,
    MaintenanceRunning,
    DataDirMoving,
    PstRemoving,
}

impl AppError {
    pub fn invalid(field: &str, reason: &str) -> AppError {
        AppError::InvalidInput { field: field.to_owned(), reason: reason.to_owned() }
    }
    pub fn internal(detail: impl Into<String>) -> AppError {
        AppError::Internal { detail: detail.into() }
    }
    pub fn incompatible(detail: impl Into<String>) -> AppError {
        AppError::IncompatibleDatabase { detail: detail.into() }
    }
    pub fn file_access(path: &std::path::Path, reason: &str) -> AppError {
        AppError::FileAccess { path: path.display().to_string(), reason: reason.to_owned() }
    }
    pub fn not_found(entity: &str, id: impl ToString) -> AppError {
        AppError::NotFound { entity: entity.to_owned(), id: id.to_string() }
    }
}
