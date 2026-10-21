//! Reader thread (spec Part 04 §5.1–§5.3, §5.8; Part 02 §6.3). The only code that calls libpff; it owns the
//! `pff::File` (`!Send`) for the whole job.
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use pff::{AttachmentType, File, Item, tags};

use crate::dto::import::{ImportErrorSeverity, ImportErrorStage};
use crate::importer::inflight::{Inflight, InflightRecord};
use crate::importer::progress::ByteBudget;
use crate::importer::types::{FolderIn, ItemError, RawAttachment, RawContent, RawItem, RawMail, RawRecipient};

/// Bounded channel size reader → normalize (Part 02 §4).
pub const CHANNEL: usize = 256;
/// Attachment bytes are read only for content-id attachments up to this size (Part 04 §5.7 source limit).
pub const MAX_ATTACHMENT_DATA: usize = 25 * 1024 * 1024;

// MAPI property ids not in `pff::tags` (MS-OXPROPS).
const IPM_SUBTREE_ENTRY_ID: u32 = 0x35e0;
const TRANSPORT_MESSAGE_HEADERS: u32 = 0x007d;
const SENT_REPRESENTING_SMTP_ADDRESS: u32 = 0x5d02;
const SENT_REPRESENTING_NAME: u32 = 0x0042;
const MAIL_CLASSES: [&str; 3] = ["IPM.Note", "IPM.Schedule.Meeting", "REPORT.IPM.Note"];
const POISON_MESSAGE: &str = "parser crashed on this item";

pub struct ScanResult {
    pub folders: Vec<FolderIn>,
    pub items_total: i64,
    pub scan_errors: Vec<ItemError>,
}

pub enum ReaderMsg {
    Scanned(ScanResult),
    Item(RawItem),
    Failed { stage: ImportErrorStage, message: String },
}

pub struct ReaderParams {
    pub path: PathBuf,
    pub cursor_seq: i64,
    /// Marker of this job left by a dead process (Part 02 §6.3 rule 1); None for a fresh start.
    pub pending: Option<InflightRecord>,
    pub inflight: Option<Inflight>,
    pub cancel: Arc<AtomicBool>,
    pub hold_after: Option<i64>,
    pub budget: Arc<ByteBudget>,
    /// Written into every inflight record.
    pub job_id: i64,
}

/// One folder of the walk, in walk order.
struct WalkFolder<'f> {
    item: Item<'f>,
    id: i64,
    path: String,
    messages: usize,
}

fn display_segment(name: &str) -> String {
    name.replace('/', "\u{2215}")
}

/// NID of the IPM subtree root: little-endian u32 in the last 4 bytes of PidTagIpmSubtreeEntryId (Part 04 §5.2).
fn ipm_root_id(file: &File) -> Option<u32> {
    let store = file.message_store().ok()?;
    let entry = store.binary(0, IPM_SUBTREE_ENTRY_ID).ok()??;
    let tail: [u8; 4] = entry.get(entry.len().checked_sub(4)?..)?.try_into().ok()?;
    Some(u32::from_le_bytes(tail))
}

/// Folder tree, depth-first by sub-folder index (pre-order). A folder that fails to list is a `scan` / `degraded`
/// error row and its subtree is skipped. Paths: below the IPM root relative to it (`Inbox/Projects`); the IPM
/// root itself is its own name; other folders `~/<path from the PST root>` (the PST root itself is `~`).
fn scan<'f>(file: &'f File) -> pff::Result<(Vec<WalkFolder<'f>>, ScanResult)> {
    let ipm = ipm_root_id(file);
    let root = file.root_folder()?;
    let mut walk = Vec::new();
    let mut out = ScanResult { folders: Vec::new(), items_total: 0, scan_errors: Vec::new() };
    // (item, parent id, path from the PST root, path relative to the IPM root if below it)
    type Pending<'f> = (Item<'f>, Option<i64>, Vec<String>, Option<Vec<String>>);
    let mut stack: Vec<Pending<'f>> = vec![(root, None, Vec::new(), None)];
    while let Some((item, parent, from_root, below_ipm)) = stack.pop() {
        let id = i64::from(item.identifier()?);
        let name = item.folder_name().ok().flatten().unwrap_or_default();
        let is_ipm = ipm.is_some_and(|n| i64::from(n) == id);
        let rel: Option<Vec<String>> = if is_ipm { Some(Vec::new()) } else { below_ipm };
        let path = match &rel {
            Some(r) if r.is_empty() => if name.is_empty() { "~".to_owned() } else { display_segment(&name) },
            Some(r) => r.join("/"),
            None if from_root.is_empty() => "~".to_owned(),
            None => format!("~/{}", from_root.join("/")),
        };
        let listed = (|| -> pff::Result<(usize, Vec<Item<'f>>)> {
            let messages = item.sub_message_count()?;
            let n = item.sub_folder_count()?;
            let subs = (0..n).map(|i| item.sub_folder(i)).collect::<pff::Result<Vec<_>>>()?;
            Ok((messages, subs))
        })();
        out.folders.push(FolderIn { pst_folder_id: id, parent_pst_folder_id: parent, name: name.clone(), path: path.clone() });
        let (messages, subs) = match listed {
            Ok(v) => v,
            Err(e) => {
                let mut err = ItemError::new(ImportErrorStage::Scan, ImportErrorSeverity::Degraded, &path, 0, &e.message);
                err.pst_item_id = None;
                out.scan_errors.push(err);
                continue;
            }
        };
        out.items_total += messages as i64;
        // children are pushed in reverse so they pop in index order
        for sub in subs.into_iter().rev() {
            let seg = display_segment(&sub.folder_name().ok().flatten().unwrap_or_default());
            let child_root: Vec<String> = from_root.iter().cloned().chain(std::iter::once(seg.clone())).collect();
            let child_rel = rel.as_ref().map(|r| r.iter().cloned().chain(std::iter::once(seg)).collect());
            stack.push((sub, Some(id), child_root, child_rel));
        }
        walk.push(WalkFolder { item, id, path, messages });
    }
    Ok((walk, out))
}

fn filetime(ft: Option<u64>) -> Option<SystemTime> {
    let us = pff::filetime_to_unix_micros(ft?)?;
    Some(if us >= 0 { UNIX_EPOCH + Duration::from_micros(us as u64) } else { UNIX_EPOCH - Duration::from_micros(us.unsigned_abs()) })
}

fn is_mail_class(class: &str) -> bool {
    MAIL_CLASSES.iter().any(|c| class.len() >= c.len() && class[..c.len()].eq_ignore_ascii_case(c))
}

fn read_mail(item: &Item<'_>) -> pff::Result<RawContent> {
    let class = item.string(0, tags::MESSAGE_CLASS)?.unwrap_or_default();
    if !is_mail_class(&class) {
        return Ok(RawContent::NonMail);
    }
    let s = |tag| item.string(0, tag);
    let mut m = RawMail {
        subject: s(tags::SUBJECT)?,
        client_submit_time: filetime(item.client_submit_time()?),
        delivery_time: filetime(item.delivery_time()?),
        internet_message_id: s(tags::INTERNET_MESSAGE_ID)?,
        transport_headers: s(TRANSPORT_MESSAGE_HEADERS)?,
        sender_smtp_address: s(tags::SENDER_SMTP_ADDRESS)?,
        sent_representing_smtp_address: s(SENT_REPRESENTING_SMTP_ADDRESS)?,
        sender_email_address: s(tags::SENDER_EMAIL_ADDRESS)?,
        sender_address_type: s(tags::SENDER_ADDRESS_TYPE)?,
        sender_name: s(tags::SENDER_NAME)?,
        sent_representing_name: s(SENT_REPRESENTING_NAME)?,
        internet_codepage: item.u32_value(0, tags::INTERNET_CODEPAGE)?,
        html_body: item.html_body()?,
        rtf_body: item.rtf_body()?,
        plain_body: item.plain_text_body()?.map(|b| String::from_utf8_lossy(&b).into_owned()),
        recipients: Vec::new(),
        attachments: Vec::new(),
    };
    if let Some(r) = item.recipients()? {
        for set in 0..r.record_set_count()? {
            m.recipients.push(RawRecipient {
                smtp_address: r.string(set, tags::SMTP_ADDRESS)?,
                email_address: r.string(set, tags::EMAIL_ADDRESS)?,
                address_type: r.string(set, tags::ADDRESS_TYPE)?,
                display_name: r.string(set, tags::DISPLAY_NAME)?,
                recipient_type: r.u32_value(set, tags::RECIPIENT_TYPE)?,
            });
        }
    }
    for index in 0..item.attachment_count()? {
        let a = item.attachment(index)?;
        let kind = a.attachment_type()?;
        let content_id = a.string(0, tags::ATTACH_CONTENT_ID)?;
        let data = match (kind, &content_id) {
            (AttachmentType::Data, Some(_)) => a.attachment_data(MAX_ATTACHMENT_DATA)?,
            _ => None,
        };
        let embedded_subject = match kind {
            AttachmentType::Item => match a.attached_item()? {
                Some(inner) => inner.string(0, tags::SUBJECT)?,
                None => None,
            },
            _ => None,
        };
        m.attachments.push(RawAttachment {
            index: index as i32,
            long_filename: a.string(0, tags::ATTACH_LONG_FILENAME)?,
            filename: a.string(0, tags::ATTACH_FILENAME)?,
            display_name: a.string(0, tags::DISPLAY_NAME)?,
            mime_tag: a.string(0, tags::ATTACH_MIME_TAG)?,
            content_id,
            attach_method: a.u32_value(0, tags::ATTACH_METHOD)?.map(|v| v as i32),
            data_size: if kind == AttachmentType::Data { Some(a.attachment_data_size()? as i64) } else { None },
            attach_size: a.u32_value(0, tags::ATTACH_SIZE)?.map(i64::from),
            embedded_subject,
            data,
        });
    }
    Ok(RawContent::Mail(Box::new(m)))
}

/// Shared send path of both readers: cancel check is done by the caller; budget acquire before the send;
/// the hold seam blocks after the item whose seq equals `hold_after` until the job is cancelled.
fn send_item(p: &ReaderParams, tx: &SyncSender<ReaderMsg>, raw: RawItem) -> bool {
    let seq = raw.seq;
    p.budget.acquire(raw.approx_bytes(), &p.cancel);
    if tx.send(ReaderMsg::Item(raw)).is_err() {
        return false;
    }
    if p.hold_after == Some(seq) {
        p.budget.wait_cancelled(&p.cancel);
    }
    true
}

fn run(p: ReaderParams, tx: SyncSender<ReaderMsg>) {
    let file = match File::open(&p.path) {
        Ok(f) => f,
        Err(e) => {
            let _ = tx.send(ReaderMsg::Failed { stage: ImportErrorStage::Open, message: e.message });
            return;
        }
    };
    let (walk, scanned) = match scan(&file) {
        Ok(v) => v,
        Err(e) => {
            let _ = tx.send(ReaderMsg::Failed { stage: ImportErrorStage::Scan, message: e.message });
            return;
        }
    };
    if tx.send(ReaderMsg::Scanned(scanned)).is_err() {
        return;
    }
    let mut seq = 0i64;
    for folder in &walk {
        for index in 0..folder.messages {
            seq += 1;
            if seq <= p.cursor_seq {
                continue; // committed before: not read at all (Part 04 §5.9 step 3)
            }
            if p.cancel.load(Ordering::SeqCst) {
                return;
            }
            let base = |pst_item_id: i64, content: RawContent| RawItem {
                seq,
                pst_item_id,
                pst_folder_id: folder.id,
                folder_path: folder.path.clone(),
                content,
            };
            // Part 02 §6.3 rule 2: attempts of this item, counted before any libpff call on it
            let pending = p.pending.as_ref().filter(|r| r.seq == seq);
            let raw = match pending {
                Some(r) if r.attempts >= 2 => {
                    // died twice on this item: do not touch it
                    base(r.pst_item_id, RawContent::Unreadable { stage: ImportErrorStage::Parse, message: POISON_MESSAGE.to_owned() })
                }
                _ => {
                    let attempts = pending.map_or(1, |r| r.attempts + 1);
                    let mark = |pst_item_id: i64| {
                        if let Some(marker) = &p.inflight {
                            marker.write(&InflightRecord { job_id: p.job_id, seq, folder_path: folder.path.clone(), pst_item_id, attempts });
                        }
                    };
                    mark(pending.map_or(0, |r| r.pst_item_id));
                    match folder.item.sub_message(index).and_then(|m| Ok((i64::from(m.identifier()?), m))) {
                        Err(e) => base(0, RawContent::Unreadable { stage: ImportErrorStage::Parse, message: e.message }),
                        Ok((id, message)) => {
                            mark(id); // same attempt, now with the PST item id for a later error row
                            let content = read_mail(&message)
                                .unwrap_or_else(|e| RawContent::Unreadable { stage: ImportErrorStage::Parse, message: e.message });
                            base(id, content)
                        }
                    }
                }
            };
            if !send_item(&p, &tx, raw) {
                return;
            }
        }
    }
}

/// Spawns the reader std::thread; it owns the pff::File. Messages arrive on a sync_channel(256).
pub fn spawn(params: ReaderParams) -> (Receiver<ReaderMsg>, JoinHandle<()>) {
    let (tx, rx) = sync_channel(CHANNEL);
    let handle = std::thread::Builder::new().name("pst-reader".into()).spawn(move || run(params, tx)).expect("spawn reader thread");
    (rx, handle)
}

/// Test entry point after the reader (feature test-support): same protocol, items from memory.
#[cfg(feature = "test-support")]
pub fn spawn_synthetic(params: ReaderParams, folders: Vec<FolderIn>, items: Vec<RawItem>) -> (Receiver<ReaderMsg>, JoinHandle<()>) {
    let (tx, rx) = sync_channel(CHANNEL);
    let handle = std::thread::Builder::new()
        .name("synthetic-reader".into())
        .spawn(move || {
            let items_total = items.len() as i64;
            if tx.send(ReaderMsg::Scanned(ScanResult { folders, items_total, scan_errors: Vec::new() })).is_err() {
                return;
            }
            for raw in items.into_iter().filter(|r| r.seq > params.cursor_seq) {
                if params.cancel.load(Ordering::SeqCst) || !send_item(&params, &tx, raw) {
                    return;
                }
            }
        })
        .expect("spawn synthetic reader");
    (rx, handle)
}

/// Deterministic list of (seq, pst_item_id, folder path) without reading message properties (tests).
pub fn walk_ids(path: &Path) -> Result<Vec<(i64, i64, String)>, pff::Error> {
    let file = File::open(path)?;
    let (walk, _) = scan(&file)?;
    let mut out = Vec::new();
    for folder in &walk {
        for index in 0..folder.messages {
            let id = i64::from(folder.item.sub_message(index)?.identifier()?);
            out.push((out.len() as i64 + 1, id, folder.path.clone()));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mail_class_filter() {
        for c in ["IPM.Note", "ipm.note.SMIME", "IPM.Schedule.Meeting.Request", "REPORT.IPM.Note.NDR"] {
            assert!(is_mail_class(c), "{c}");
        }
        for c in ["IPM.Contact", "IPM.Appointment", "IPM.Task", "IPM.StickyNote", "IPM", ""] {
            assert!(!is_mail_class(c), "{c}");
        }
    }

    #[test]
    fn filetime_to_system_time() {
        // 2023-01-01T00:00:00Z = 133170048000000000 in 100 ns units since 1601
        let t = filetime(Some(133_170_048_000_000_000)).unwrap();
        assert_eq!(crate::importer::dedupe::format_utc_seconds(t), "2023-01-01T00:00:00Z");
        assert_eq!(filetime(Some(0)), None);
        assert_eq!(filetime(None), None);
        let old = filetime(Some(100_000_000_000_000_000)).unwrap(); // 1917
        assert!(old < UNIX_EPOCH);
    }

    #[test]
    fn slash_in_folder_name() {
        assert_eq!(display_segment("A/B"), "A\u{2215}B");
    }
}
