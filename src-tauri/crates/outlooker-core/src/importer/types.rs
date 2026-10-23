//! Data types shared by the reader, normalize and the writer.
use std::time::SystemTime;

use crate::dto::import::{ImportErrorSeverity, ImportErrorStage};
use crate::dto::search::RecipientKind;

/// One PST folder found by the scan (Part 04 §5.2). Sent once per job before any item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderIn {
    pub pst_folder_id: i64,
    pub parent_pst_folder_id: Option<i64>,
    pub name: String,
    pub path: String,
}

/// One PST item as the reader hands it to normalize. `seq` starts at 1 (Part 04 §5.1).
#[derive(Debug, Clone)]
pub struct RawItem {
    pub seq: i64,
    pub pst_item_id: i64,
    pub pst_folder_id: i64,
    pub folder_path: String,
    pub content: RawContent,
}

#[derive(Debug, Clone)]
pub enum RawContent {
    Mail(Box<RawMail>),
    NonMail,
    Unreadable { stage: ImportErrorStage, message: String },
    /// The PST item id already has an occurrence under this source (an earlier sync stored it): not read.
    Known,
}

#[derive(Debug, Clone, Default)]
pub struct RawMail {
    pub subject: Option<String>,
    pub client_submit_time: Option<SystemTime>,
    pub delivery_time: Option<SystemTime>,
    pub internet_message_id: Option<String>,
    pub transport_headers: Option<String>,
    pub sender_smtp_address: Option<String>,
    pub sent_representing_smtp_address: Option<String>,
    pub sender_email_address: Option<String>,
    pub sender_address_type: Option<String>,
    pub sender_name: Option<String>,
    pub sent_representing_name: Option<String>,
    pub internet_codepage: Option<u32>,
    pub html_body: Option<Vec<u8>>,
    pub rtf_body: Option<Vec<u8>>,
    pub plain_body: Option<String>,
    pub recipients: Vec<RawRecipient>,
    pub attachments: Vec<RawAttachment>,
}

#[derive(Debug, Clone, Default)]
pub struct RawRecipient {
    pub smtp_address: Option<String>,
    pub email_address: Option<String>,
    pub address_type: Option<String>,
    pub display_name: Option<String>,
    pub recipient_type: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct RawAttachment {
    pub index: i32,
    pub long_filename: Option<String>,
    pub filename: Option<String>,
    pub display_name: Option<String>,
    pub mime_tag: Option<String>,
    pub content_id: Option<String>,
    pub attach_method: Option<i32>,
    pub data_size: Option<i64>,
    pub attach_size: Option<i64>,
    pub embedded_subject: Option<String>,
    pub data: Option<Vec<u8>>,
}

impl RawItem {
    pub fn approx_bytes(&self) -> u64 {
        match &self.content {
            RawContent::Mail(m) => {
                let bodies = m.html_body.as_ref().map_or(0, Vec::len)
                    + m.rtf_body.as_ref().map_or(0, Vec::len)
                    + m.plain_body.as_ref().map_or(0, String::len);
                let data: usize = m.attachments.iter().map(|a| a.data.as_ref().map_or(0, Vec::len)).sum();
                (bodies + data) as u64
            }
            _ => 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DedupeKind { MessageId, ContentHash }
impl DedupeKind {
    pub fn as_db(self) -> &'static str {
        match self { DedupeKind::MessageId => "message_id", DedupeKind::ContentHash => "content_hash" }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodySource { Html, RtfHtml, Text, RtfText, None }
impl BodySource {
    pub fn as_db(self) -> &'static str {
        match self {
            BodySource::Html => "html",
            BodySource::RtfHtml => "rtf_html",
            BodySource::Text => "text",
            BodySource::RtfText => "rtf_text",
            BodySource::None => "none",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressType { Smtp, Ex, Other }
impl AddressType {
    pub fn as_db(self) -> &'static str {
        match self { AddressType::Smtp => "smtp", AddressType::Ex => "ex", AddressType::Other => "other" }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressIn { pub email: String, pub address_type: AddressType }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecipientOut {
    pub ordinal: i32,
    pub kind: RecipientKind,
    pub address: Option<AddressIn>,
    pub display_name: Option<String>,
    pub search_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentOut {
    pub attachment_index: i32,
    pub file_name: String,
    pub size_bytes: i64,
    pub mime_type: String,
    pub attach_method: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedImage { pub webp: Vec<u8>, pub width: i32, pub height: i32, pub source_mime: String }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineImageOut {
    pub ordinal: i32,
    pub content_id: Option<String>,
    pub source_hash: [u8; 32],
    pub blob: Option<EncodedImage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemError {
    pub stage: ImportErrorStage,
    pub severity: ImportErrorSeverity,
    pub folder_path: Option<String>,
    pub pst_item_id: Option<i64>,
    pub message: String,
}

impl ItemError {
    /// Builds an error row; `message` is cut to 4,000 chars and never empty (DB CHECK).
    pub fn new(stage: ImportErrorStage, severity: ImportErrorSeverity, folder_path: &str, pst_item_id: i64, message: &str) -> ItemError {
        let mut m: String = message.chars().take(4000).collect();
        if m.is_empty() {
            m.push_str("unknown error");
        }
        ItemError { stage, severity, folder_path: Some(folder_path.to_string()), pst_item_id: Some(pst_item_id), message: m }
    }
}

#[derive(Debug, Clone)]
pub struct NormalizedMail {
    pub dedupe_key: [u8; 32],
    pub dedupe_kind: DedupeKind,
    pub message_id: Option<String>,
    pub subject: String,
    pub sent_at: Option<SystemTime>,
    pub received_at: Option<SystemTime>,
    pub sender: Option<AddressIn>,
    pub sender_name: Option<String>,
    pub sender_search: String,
    pub recipients: Vec<RecipientOut>,
    pub recipients_search: String,
    pub to_preview: String,
    pub body_source: BodySource,
    pub body_html: Option<String>,
    pub body_text: String,
    pub snippet: String,
    pub attachments: Vec<AttachmentOut>,
    pub inline_images: Vec<InlineImageOut>,
}

#[derive(Debug, Clone)]
/// `Known`: stored by an earlier sync of this source, counted as a duplicate, no rows written.
pub enum Outcome { Mail(Box<NormalizedMail>), NonMail, Skipped, Known }

#[derive(Debug, Clone)]
pub struct NormalizedItem {
    pub seq: i64,
    pub pst_item_id: i64,
    pub pst_folder_id: i64,
    pub raw_bytes: u64,
    pub outcome: Outcome,
    pub errors: Vec<ItemError>,
}
