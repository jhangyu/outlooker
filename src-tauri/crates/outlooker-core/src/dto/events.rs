use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::app::LibraryCounts;
use super::error::AppError;
use super::import::{ImportStatus, PstSource};
pub use super::settings::{MaintenanceRunState, MaintenanceState};

pub mod names {
    pub const APP_STATUS_CHANGED: &str = "app:status-changed";
    pub const IMPORT_PROGRESS: &str = "import:progress";
    pub const IMPORT_STATUS_CHANGED: &str = "import:status-changed";
    pub const LIBRARY_CHANGED: &str = "library:changed";
    pub const DATA_DIR_MOVE_PROGRESS: &str = "data-dir:move-progress";
    pub const MAINTENANCE_STATUS_CHANGED: &str = "maintenance:status-changed";
    pub const ALL: [&str; 6] = [
        APP_STATUS_CHANGED, IMPORT_PROGRESS, IMPORT_STATUS_CHANGED,
        LIBRARY_CHANGED, DATA_DIR_MOVE_PROGRESS, MAINTENANCE_STATUS_CHANGED,
    ];
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ImportStatusChanged {
    pub pst_id: i64,
    pub status: ImportStatus,
    pub removed: bool,
    pub source: Option<PstSource>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct LibraryChanged {
    pub counts: LibraryCounts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum DataDirMovePhase {
    StoppingDb,
    Copying,
    Verifying,
    StartingDb,
    Done,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DataDirMoveProgress {
    pub phase: DataDirMovePhase,
    pub bytes_copied: i64,
    pub bytes_total: i64,
    pub error: Option<AppError>,
}

/// Writes `<TS_RS_EXPORT_DIR>/<file>` as a pretty JSON array with a trailing newline.
#[cfg(test)]
pub(crate) fn write_name_list(file: &str, names: &[&str]) {
    let dir = std::env::var("TS_RS_EXPORT_DIR")
        .expect("TS_RS_EXPORT_DIR is set by src-tauri/.cargo/config.toml");
    std::fs::create_dir_all(&dir).unwrap();
    let body = serde_json::to_string_pretty(names).unwrap() + "\n";
    std::fs::write(std::path::Path::new(&dir).join(file), body).unwrap();
}

#[cfg(test)]
mod tests {
    #[test]
    fn export_event_names() {
        super::write_name_list("events.json", &super::names::ALL);
    }
}
