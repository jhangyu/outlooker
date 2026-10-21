use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::error::AppError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AppStatus {
    pub phase: AppPhase,
    pub data_dir: String,
    pub language: Language,
    pub counts: LibraryCounts,
    pub error: Option<AppError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum AppPhase {
    NeedsSetup,
    InitializingCluster,
    StartingDb,
    Ready,
    StoppingDb,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct LibraryCounts {
    pub messages: i64,
    pub psts: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum Language {
    #[default]
    #[serde(rename = "zh-TW")]
    ZhTw,
    #[serde(rename = "en")]
    En,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct InitializeRequest {
    pub data_dir: String,
    pub language: Language,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_and_phase_wire_values() {
        assert_eq!(serde_json::to_string(&Language::ZhTw).unwrap(), r#""zh-TW""#);
        assert_eq!(serde_json::to_string(&AppPhase::InitializingCluster).unwrap(), r#""initializingCluster""#);
    }
}
