use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SearchQuery {
    Simple { text: String },
    Dsl { root: ConditionGroup },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ConditionGroup {
    pub op: GroupOp,
    pub children: Vec<ConditionNode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum GroupOp {
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ConditionNode {
    Group { op: GroupOp, children: Vec<ConditionNode> },
    Sender { value: String },
    Recipient { value: String, kinds: Vec<RecipientKind> },
    Subject { value: String },
    Body { value: String },
    DateRange { from: Option<String>, to: Option<String> },
    Folder { folder_path: String, include_subfolders: bool },
    HasAttachment { value: bool },
    SourcePst { pst_ids: Vec<i64> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum RecipientKind {
    To,
    Cc,
    Bcc,
}
super::import::db_text!(RecipientKind, "recipient kind", { To => "to", Cc => "cc", Bcc => "bcc" });

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    pub query: SearchQuery,
    pub cursor: Option<String>,
    pub page_size: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SearchPage {
    pub rows: Vec<MailRow>,
    pub next_cursor: Option<String>,
    pub short_query: bool,
    pub elapsed_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct MailRow {
    pub id: i64,
    pub sent_at: Option<String>,
    pub from: Address,
    pub to_preview: String,
    pub recipient_count: u32,
    pub subject: String,
    pub snippet: String,
    pub has_attachments: bool,
    pub occurrence_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct Address {
    pub name: Option<String>,
    pub address: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SearchCount {
    pub count: i64,
    pub capped: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct FilterOptions {
    pub psts: Vec<PstRef>,
    pub folders: Vec<FolderRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct PstRef {
    pub id: i64,
    pub display_name: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct FolderRef {
    pub folder_path: String,
    pub pst_id: i64,
    pub message_count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct MailDetail {
    pub id: i64,
    pub message_id: Option<String>,
    pub subject: String,
    pub sent_at: Option<String>,
    pub received_at: Option<String>,
    pub from: Address,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub bcc: Vec<Address>,
    pub body_html: Option<String>,
    pub body_text: String,
    pub attachments: Vec<AttachmentMeta>,
    pub occurrences: Vec<Occurrence>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentMeta {
    pub id: i64,
    pub file_name: String,
    pub size_bytes: i64,
    pub mime_type: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct Occurrence {
    pub pst_id: i64,
    pub pst_display_name: String,
    pub pst_path: String,
    pub folder_path: String,
    pub pst_item_id: i64,
}
