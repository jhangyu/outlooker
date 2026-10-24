use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::search::MailRow;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SavedQuery {
    pub id: i64,
    pub name: String,
    pub sql: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SavedQueryInput {
    pub name: String,
    pub sql: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SqlExecuteRequest {
    pub execution_id: String,
    pub sql: String,
    pub max_rows: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "mode", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SqlResult {
    MailList { rows: Vec<MailRow>, missing_ids: u32, truncated: bool, elapsed_ms: u32 },
    Table { columns: Vec<ColumnMeta>, rows: Vec<Vec<Option<String>>>, truncated: bool, elapsed_ms: u32 },
    Command { tag: String, elapsed_ms: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ColumnMeta {
    pub name: String,
    pub pg_type: String,
}
