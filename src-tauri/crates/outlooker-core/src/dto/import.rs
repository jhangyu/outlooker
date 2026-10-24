use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::error::AppError;

/// `as_db` / `from_db` for an enum stored as text in PostgreSQL. An unknown DB value is an internal error, never a fallback.
macro_rules! db_text {
    ($ty:ident, $what:literal, { $($v:ident => $s:literal),+ $(,)? }) => {
        impl $ty {
            #[cfg(test)]
            pub const ALL: &'static [$ty] = &[$($ty::$v),+];
            #[allow(dead_code)] // ponytail: JobKind is only written by SQL literals; the pair stays for the round-trip test
            pub fn as_db(self) -> &'static str {
                match self { $($ty::$v => $s),+ }
            }
            pub fn from_db(s: &str) -> Result<Self, $crate::dto::error::AppError> {
                match s {
                    $($s => Ok($ty::$v),)+
                    other => Err($crate::dto::error::AppError::internal(format!(concat!("unknown ", $what, " {}"), other))),
                }
            }
        }
    };
}
pub(crate) use db_text;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct PstSource {
    pub id: i64,
    pub path: String,
    pub display_name: String,
    pub file_size_bytes: i64,
    pub status: ImportStatus,
    pub progress: Option<Box<ImportProgress>>, // boxed: keeps EnqueueResult and CoreEvent variants small
    pub message_count: i64,
    pub new_message_count: i64,
    pub error_count: i64,
    /// Newest job: created (for `locked`: locked since) and finished (for `locked`: last seen locked).
    pub queued_at: String,
    pub finished_at: Option<String>,
    /// Paused by the user: no sync and no job start until Resume (a running sync stops after the current item).
    pub paused: bool,
    /// Last time a sync step looked at the path (also "unchanged" and "locked").
    pub last_checked_at: Option<String>,
    /// finished_at of the newest completed import job.
    pub last_synced_at: Option<String>,
    /// Message of the newest job's `job_failed` row; set only when status is `failed`.
    pub failure_reason: Option<String>,
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
    /// Another program (Outlook) holds the PST; retried on the next sync.
    Locked,
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
db_text!(ImportPhase, "import phase", { Scanning => "scanning", Importing => "importing", Finalizing => "finalizing" });

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
db_text!(ImportErrorStage, "import error stage", {
    Open => "open", Scan => "scan", Parse => "parse", Html => "html", Image => "image", Store => "store",
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum ImportErrorSeverity {
    ItemSkipped,
    Degraded,
    JobFailed,
}
db_text!(ImportErrorSeverity, "import error severity", { ItemSkipped => "item_skipped", Degraded => "degraded", JobFailed => "job_failed" });

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip<T: Copy + PartialEq + std::fmt::Debug>(all: &[T], as_db: fn(T) -> &'static str, from_db: fn(&str) -> Result<T, AppError>) {
        for &v in all {
            assert_eq!(from_db(as_db(v)).unwrap(), v);
        }
        assert!(from_db("bogus").is_err());
    }

    #[test]
    fn db_text_round_trips() {
        round_trip(ImportPhase::ALL, ImportPhase::as_db, ImportPhase::from_db);
        round_trip(ImportErrorStage::ALL, ImportErrorStage::as_db, ImportErrorStage::from_db);
        round_trip(ImportErrorSeverity::ALL, ImportErrorSeverity::as_db, ImportErrorSeverity::from_db);
        round_trip(crate::dto::search::RecipientKind::ALL, crate::dto::search::RecipientKind::as_db, crate::dto::search::RecipientKind::from_db);
    }
}
