//! Import Manager integration tests (spec Part 02 §4, §6.1–§6.3; Part 03 §7; Part 04 §2.1, §5, §6.4, §6.5, §12.2).
//! Every test pre-registers its expected values as constants before it runs the import.
#![cfg(feature = "test-support")]
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use outlooker_core::db::indexes::{SEARCH_INDEXES, missing_or_invalid};
use outlooker_core::dto::app::AppPhase;
use outlooker_core::dto::error::AppError;
use outlooker_core::dto::import::{EnqueueResult, ImportErrorStage, ImportStatus};
use outlooker_core::gate::Gate;
use outlooker_core::importer::inflight::{Inflight, InflightRecord};
use outlooker_core::importer::probe::{hold_probe, set_test_lock};
use outlooker_core::importer::pst_reader::walk_ids;
use outlooker_core::importer::types::{FolderIn, RawAttachment, RawContent, RawItem, RawMail, RawRecipient};
use outlooker_core::importer::writer::{Checkpoint, WriterLimits, truncate_to_micros};
use outlooker_core::dto::search::{SearchQuery, SearchRequest};
use outlooker_core::importer::{ImportManager, ImportTuning, RecoveryReport, SyncTarget};
use outlooker_core::search::SearchService;
use outlooker_core::sink::NullSink;
use outlooker_core::test_support::{png, TestCluster};
use tokio::sync::{broadcast, mpsc, watch};

/// libyal testdata `pst/outlook.pst`, relative to this crate.
const OUTLOOK_PST: &str = "../../../tests/fixtures/external/outlook.pst";
/// Repository root, relative to this crate (Windows-phase fixtures live in `tests/fixtures/pst/`).
#[cfg(feature = "outlook-fixtures")]
const REPO_ROOT: &str = "../../..";
const JOB_TIMEOUT: Duration = Duration::from_secs(120);

// ------------------------------------------------------------------------------------------------ helpers

struct Fx {
    mgr: ImportManager,
    events: broadcast::Receiver<(i64, ImportStatus)>,
    phase: watch::Sender<AppPhase>,
}

fn manager(tc: &TestCluster, tuning: ImportTuning) -> Fx {
    let (phase, rx) = watch::channel(AppPhase::Ready);
    let mgr = ImportManager::new(tc.db.clone(), Gate::new(), tc.paths.clone(), Arc::new(NullSink), rx, tuning);
    let events = mgr.job_events();
    Fx { mgr, events, phase }
}

/// Batches of 5 items, so 20–40 synthetic items give several commits.
fn small_batches() -> ImportTuning {
    ImportTuning { writer: WriterLimits { max_items: 5, ..WriterLimits::default() }, ..ImportTuning::default() }
}

/// `on_batch` hook that reports `batches_committed` on a channel (never blocks the writer).
fn batch_signal(t: &mut ImportTuning) -> mpsc::UnboundedReceiver<i64> {
    let (tx, rx) = mpsc::unbounded_channel();
    t.on_batch = Some(Arc::new(move |n| {
        let _ = tx.send(n);
    }));
    rx
}

async fn wait_batch(rx: &mut mpsc::UnboundedReceiver<i64>, n: i64) {
    loop {
        let got = tokio::time::timeout(JOB_TIMEOUT, rx.recv()).await.expect("batch in time").expect("hook alive");
        if got >= n {
            return;
        }
    }
}

/// Terminal job outcome of each PST in `ids` (events of other PSTs are ignored).
async fn wait_jobs(events: &mut broadcast::Receiver<(i64, ImportStatus)>, ids: &[i64]) -> Vec<ImportStatus> {
    let mut got: BTreeMap<i64, ImportStatus> = BTreeMap::new();
    while ids.iter().any(|id| !got.contains_key(id)) {
        let (id, status) = tokio::time::timeout(JOB_TIMEOUT, events.recv()).await.expect("job ended in time").expect("job events open");
        if ids.contains(&id) {
            got.insert(id, status);
        }
    }
    ids.iter().map(|id| got[id]).collect()
}

async fn wait_job(events: &mut broadcast::Receiver<(i64, ImportStatus)>, pst_id: i64) -> ImportStatus {
    wait_jobs(events, &[pst_id]).await[0]
}

fn outlook_pst() -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join(OUTLOOK_PST);
    std::fs::canonicalize(&p).unwrap_or_else(|e| panic!("missing {}: {e}", p.display()))
}

fn s(p: &Path) -> String {
    p.to_str().expect("utf-8 path").to_owned()
}

/// Source file for synthetic items: 20 bytes starting with the PST magic, so the probe and the file identity
/// (size + mtime) work as for a real PST.
fn synthetic_path(tc: &TestCluster, name: &str) -> String {
    let p = tc.dir.path().join(name);
    std::fs::write(&p, b"!BDN0000000000000000").unwrap();
    s(&p)
}

#[derive(Debug, PartialEq, Eq)]
struct Snapshot {
    keys: BTreeSet<String>,
    messages: i64,
    occurrences: i64,
    recipients: i64,
    attachments: i64,
}

async fn snapshot(tc: &TestCluster) -> Snapshot {
    let a = tc.admin().await;
    let keys = a.query("SELECT message_id FROM messages", &[]).await.unwrap().iter().map(|r| r.get::<_, String>(0)).collect();
    let r = a
        .query_one(
            "SELECT (SELECT count(*) FROM messages), (SELECT count(*) FROM message_occurrences), \
                    (SELECT count(*) FROM message_recipients), (SELECT count(*) FROM attachments)",
            &[],
        )
        .await
        .unwrap();
    Snapshot { keys, messages: r.get(0), occurrences: r.get(1), recipients: r.get(2), attachments: r.get(3) }
}

/// The six search index OIDs by name (absent → not in the map).
async fn index_oids(tc: &TestCluster) -> BTreeMap<String, u32> {
    let names: Vec<&str> = SEARCH_INDEXES.iter().map(|i| i.name).collect();
    let rows = tc.admin().await.query("SELECT relname::text, oid FROM pg_class WHERE relname = ANY($1)", &[&names]).await.unwrap();
    rows.iter().map(|r| (r.get(0), r.get(1))).collect()
}

async fn missing(tc: &TestCluster) -> Vec<&'static str> {
    missing_or_invalid(&tc.admin().await).await.unwrap()
}

fn all_six() -> Vec<&'static str> {
    SEARCH_INDEXES.iter().map(|i| i.name).collect()
}

fn folders() -> Vec<FolderIn> {
    vec![
        FolderIn { pst_folder_id: 1, parent_pst_folder_id: None, name: "Inbox".into(), path: "Inbox".into() },
        FolderIn { pst_folder_id: 2, parent_pst_folder_id: Some(1), name: "Projects".into(), path: "Inbox/Projects".into() },
    ]
}

fn item(seq: i64, folder: i64, content: RawContent) -> RawItem {
    let folder_path = if folder == 1 { "Inbox" } else { "Inbox/Projects" };
    RawItem { seq, pst_item_id: 10_000 + seq, pst_folder_id: folder, folder_path: folder_path.into(), content }
}

fn mail(m: RawMail) -> RawContent {
    RawContent::Mail(Box::new(m))
}

/// `n` items in two folders: every 10th is a contact (non-mail), the rest distinct mails with a Message-ID,
/// one recipient and one attachment.
fn corpus(n: i64) -> Vec<RawItem> {
    corpus_tagged(n, "m")
}

fn corpus_tagged(n: i64, tag: &str) -> Vec<RawItem> {
    (1..=n)
        .map(|i| {
            let content = if i % 10 == 0 {
                RawContent::NonMail
            } else {
                mail(RawMail {
                    subject: Some(format!("Mail {tag}{i}")),
                    internet_message_id: Some(format!("<{tag}{i}@synthetic.example>")),
                    sender_smtp_address: Some(format!("sender{}@synthetic.example", i % 3)),
                    html_body: Some(format!("<p>Body {tag}{i} 報價單</p>").into_bytes()),
                    recipients: vec![RawRecipient { smtp_address: Some("to@synthetic.example".into()), recipient_type: Some(1), ..Default::default() }],
                    attachments: vec![RawAttachment {
                        index: 0,
                        long_filename: Some(format!("a{i}.pdf")),
                        data_size: Some(100),
                        attach_method: Some(1),
                        ..Default::default()
                    }],
                    ..Default::default()
                })
            };
            item(i, 1 + i % 2, content)
        })
        .collect()
}

/// The library one uninterrupted import of `items` produces, derived from the items (distinct Message-IDs,
/// one occurrence per mail). `index_policy_incremental_keeps_indexes` checks it against a real import.
fn expected(items: &[RawItem]) -> Snapshot {
    let mut by_key: BTreeMap<String, &RawMail> = BTreeMap::new();
    let mut occurrences = 0;
    for i in items {
        if let RawContent::Mail(m) = &i.content {
            let id = m.internet_message_id.as_deref().expect("corpus mails carry a Message-ID");
            by_key.entry(id.trim_matches(['<', '>']).to_owned()).or_insert(m);
            occurrences += 1;
        }
    }
    Snapshot {
        messages: by_key.len() as i64,
        occurrences,
        recipients: by_key.values().map(|m| m.recipients.len() as i64).sum(),
        attachments: by_key.values().map(|m| m.attachments.len() as i64).sum(),
        keys: by_key.into_keys().collect(),
    }
}

// ------------------------------------------------------------------------------------------------ outlook.pst

#[tokio::test(flavor = "multi_thread")]
async fn outlook_pst_import() {
    // pre-registered: measured with this reader during planning (walk order Part 04 §5.2)
    const WALK: [(i64, i64, &str); 2] = [(1, 2_097_188, "Inbox"), (2, 2_097_220, "Recovered_Group2")];
    const MESSAGES: i64 = 1;
    const OCCURRENCES: i64 = 2;
    const ATTACHMENT_NAMES: [&str; 11] = ["1.jpg", "2.jpg", "3.jpg", "4.jpg", "5.jpg", "6.jpg", "7.jpg", "8.jpg", "9.jpg", "10.jpg", "12.jpg"];
    let pst = outlook_pst();
    let ids = walk_ids(&pst).unwrap();
    assert_eq!(ids, walk_ids(&pst).unwrap(), "scan_is_deterministic: two walks give identical (seq, pst_item_id) lists");
    assert_eq!(ids.iter().map(|(seq, id, p)| (*seq, *id, p.as_str())).collect::<Vec<_>>(), WALK);

    let tc = TestCluster::start().await;
    let mut fx = manager(&tc, ImportTuning::default());
    let res = fx.mgr.register(vec![s(&pst)]).await.unwrap();
    let EnqueueResult::Queued { pst: src } = &res[0] else { panic!("{res:?}") };
    assert_eq!(src.status, ImportStatus::Queued);
    fx.mgr.start_worker();
    assert_eq!(wait_job(&mut fx.events, src.id).await, ImportStatus::Completed);

    let a = tc.admin().await;
    let paths: Vec<String> = a.query("SELECT path FROM folders", &[]).await.unwrap().iter().map(|r| r.get(0)).collect();
    assert!(paths.iter().any(|p| p == "Inbox") && paths.iter().any(|p| p == "Sent Items"), "{paths:?}");
    assert_eq!(tc.count("SELECT count(*) FROM messages").await, MESSAGES);
    assert_eq!(tc.count("SELECT count(*) FROM message_occurrences").await, OCCURRENCES);
    let m = a.query_one("SELECT body_source, has_attachments, body_html IS NOT NULL FROM messages", &[]).await.unwrap();
    assert_eq!((m.get::<_, &str>(0), m.get::<_, bool>(1), m.get::<_, bool>(2)), ("html", true, true));
    let att = a.query("SELECT file_name, mime_type, attach_method FROM attachments ORDER BY attachment_index", &[]).await.unwrap();
    assert_eq!(att.iter().map(|r| r.get::<_, String>(0)).collect::<Vec<_>>(), ATTACHMENT_NAMES);
    assert!(att.iter().all(|r| r.get::<_, &str>(1) == "image/jpeg" && r.get::<_, i32>(2) == 1));
    let listed = fx.mgr.list().await.unwrap();
    assert_eq!((listed[0].status, listed[0].message_count, listed[0].new_message_count), (ImportStatus::Completed, 2, 1));
}

#[tokio::test(flavor = "multi_thread")]
async fn byte_copy_dedupes_and_removal() {
    // pre-registered: the Welcome mail is stored twice in outlook.pst (Inbox + Recovered_Group2) → 2 occurrences per copy
    const MESSAGES: i64 = 1;
    const OCCURRENCES_BOTH: i64 = 4;
    const OCCURRENCES_ONE: i64 = 2;
    let dir = tempfile::tempdir().unwrap();
    let copy = dir.path().join("copy.pst");
    std::fs::copy(outlook_pst(), &copy).unwrap();
    let tc = TestCluster::start().await;
    let mut fx = manager(&tc, ImportTuning::default());
    let res = fx.mgr.register(vec![s(&outlook_pst()), s(&copy)]).await.unwrap();
    let ids: Vec<i64> = res.iter().map(|r| if let EnqueueResult::Queued { pst } = r { pst.id } else { panic!("{r:?}") }).collect();
    fx.mgr.start_worker();
    assert_eq!(wait_jobs(&mut fx.events, &ids).await, [ImportStatus::Completed, ImportStatus::Completed]);
    assert_eq!(tc.count("SELECT count(*) FROM messages").await, MESSAGES);
    assert_eq!(tc.count("SELECT count(*) FROM message_occurrences").await, OCCURRENCES_BOTH);
    assert_eq!(tc.count("SELECT count(*) FROM messages WHERE dedupe_kind = 'content_hash'").await, 1, "no Message-ID → fallback hash");

    fx.mgr.remove(ids[0]).await.unwrap();
    assert_eq!(wait_job(&mut fx.events, ids[0]).await, ImportStatus::Removing);
    assert_eq!(tc.count("SELECT count(*) FROM messages").await, MESSAGES, "removing one source keeps the shared message");
    assert_eq!(tc.count("SELECT count(*) FROM message_occurrences").await, OCCURRENCES_ONE);
    assert!(tc.count("SELECT count(*) FROM addresses").await > 0);

    fx.mgr.remove(ids[1]).await.unwrap();
    assert_eq!(wait_job(&mut fx.events, ids[1]).await, ImportStatus::Removing);
    for table in ["pst_sources", "messages", "message_occurrences", "attachments", "addresses", "message_recipients"] {
        assert_eq!(tc.count(&format!("SELECT count(*) FROM {table}")).await, 0, "{table} after removing both");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn corrupt_copy_outcome() {
    // fixture identity: the offsets below are valid only for this file (measured with a PST block probe during planning)
    const ORIGINAL_BLAKE3: &str = "1a16e61df9833c3cbe1bf74d53a1ef3abacfcbb33c8f58db8ac86d6c0f6b1eb5";
    // the two data blocks of the IPM.Note "Welcome" property contexts (NIDs 0x200024 / 0x200044)
    const ZEROED: [(u64, usize); 2] = [(173_568, 1_454), (138_240, 1_454)];
    // pre-registered outcome; any other observed outcome is reported, never re-tuned
    const EXPECT_STATUS: ImportStatus = ImportStatus::Completed;
    const EXPECT_MIN_SKIPPED_PARSE: i64 = 1;
    const EXPECT_MESSAGES: i64 = 0;
    let original = std::fs::read(outlook_pst()).unwrap();
    assert_eq!(blake3::hash(&original).to_hex().as_str(), ORIGINAL_BLAKE3, "outlook.pst changed: re-measure the offsets");
    let dir = tempfile::tempdir().unwrap();
    let corrupt = dir.path().join("corrupt.pst");
    std::fs::write(&corrupt, &original).unwrap();
    let mut f = std::fs::OpenOptions::new().write(true).open(&corrupt).unwrap();
    for (offset, len) in ZEROED {
        f.seek(SeekFrom::Start(offset)).unwrap();
        f.write_all(&vec![0u8; len]).unwrap();
    }
    drop(f);

    let tc = TestCluster::start().await;
    let mut fx = manager(&tc, ImportTuning::default());
    let res = fx.mgr.register(vec![s(&corrupt)]).await.unwrap();
    let EnqueueResult::Queued { pst } = &res[0] else { panic!("{res:?}") };
    fx.mgr.start_worker();
    assert_eq!(wait_job(&mut fx.events, pst.id).await, EXPECT_STATUS);
    let skipped = tc.count("SELECT count(*) FROM import_errors WHERE severity = 'item_skipped' AND stage = 'parse'").await;
    assert!(skipped >= EXPECT_MIN_SKIPPED_PARSE, "item_skipped/parse rows: {skipped}");
    assert_eq!(tc.count("SELECT count(*) FROM messages").await, EXPECT_MESSAGES);
}

// ------------------------------------------------------------------------------------------------ synthetic items

#[tokio::test(flavor = "multi_thread")]
async fn synthetic_inline_images() {
    const MAILS: i64 = 10;
    const BLOBS: i64 = 1;
    let logo = png(40, 20);
    let items: Vec<RawItem> = (1..=MAILS)
        .map(|i| {
            item(
                i,
                1,
                mail(RawMail {
                    subject: Some(format!("Logo {i}")),
                    internet_message_id: Some(format!("<logo{i}@synthetic.example>")),
                    html_body: Some(format!(r#"<p>Mail {i} <img src="cid:logo@x" alt="logo"></p>"#).into_bytes()),
                    attachments: vec![RawAttachment {
                        index: 0,
                        content_id: Some("<logo@x>".into()),
                        long_filename: Some("logo.png".into()),
                        mime_tag: Some("image/png".into()),
                        data: Some(logo.clone()),
                        data_size: Some(logo.len() as i64),
                        attach_method: Some(1),
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
            )
        })
        .collect();
    let tc = TestCluster::start().await;
    let fx = manager(&tc, ImportTuning::default());
    fx.mgr.import_synthetic(&synthetic_path(&tc, "inline.pst"), folders(), items).await.unwrap();
    assert_eq!(tc.count("SELECT count(*) FROM messages").await, MAILS);
    assert_eq!(tc.count("SELECT count(*) FROM image_blobs").await, BLOBS, "the repeated logo is stored once");
    assert_eq!(tc.count("SELECT count(*) FROM inline_images").await, MAILS);
    assert_eq!(tc.count("SELECT count(*) FROM messages WHERE body_html LIKE '%outlooker-inline:0%'").await, MAILS);
    // every outlooker-inline:n reference has its inline_images row
    let dangling = tc.count(
        "SELECT count(*) FROM messages m, regexp_matches(m.body_html, 'outlooker-inline:([0-9]+)', 'g') x \
          WHERE NOT EXISTS (SELECT 1 FROM inline_images i WHERE i.message_id = m.id AND i.ordinal = x[1]::int)",
    )
    .await;
    assert_eq!(dangling, 0);
}

// ------------------------------------------------------------------------------------------------ index policy, vacuum

#[tokio::test(flavor = "multi_thread")]
async fn index_policy_first_import_drops_and_rebuilds() {
    const ITEMS: i64 = 10;
    const HOLD_AT: i64 = 5;
    let tc = TestCluster::start().await;
    let before = index_oids(&tc).await;
    assert_eq!(before.len(), 6);
    let mut tuning = small_batches();
    tuning.hold_reader_after = Some(HOLD_AT);
    let mut batches = batch_signal(&mut tuning);
    let mut fx = manager(&tc, tuning);
    let pst_id = fx.mgr.enqueue_synthetic(&synthetic_path(&tc, "first.pst"), folders(), corpus(ITEMS)).await.unwrap();
    fx.mgr.start_worker();
    wait_batch(&mut batches, 1).await;
    // during the job (reader held after item 5, job running): all six dropped
    assert_eq!(missing(&tc).await, all_six());
    fx.mgr.set_paused(pst_id, true).await.unwrap();
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Queued);
    fx.mgr.set_paused(pst_id, false).await.unwrap();
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Completed);
    assert_eq!(missing(&tc).await, Vec::<&str>::new(), "after Completed all six exist and are valid");
    let after = index_oids(&tc).await;
    assert!(SEARCH_INDEXES.iter().all(|i| before[i.name] != after[i.name]), "drop + create gives new OIDs: {before:?} {after:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn index_policy_rebuilt_after_pause_and_disk_failure() {
    let tc = TestCluster::start().await;
    // (a) paused job (back to queued, no finalize)
    let mut tuning = small_batches();
    tuning.hold_reader_after = Some(10);
    let mut batches = batch_signal(&mut tuning);
    let mut fx = manager(&tc, tuning);
    let p1 = fx.mgr.enqueue_synthetic(&synthetic_path(&tc, "paused.pst"), folders(), corpus(20)).await.unwrap();
    fx.mgr.start_worker();
    wait_batch(&mut batches, 1).await;
    assert_eq!(missing(&tc).await, all_six());
    fx.mgr.set_paused(p1, true).await.unwrap();
    assert_eq!(wait_job(&mut fx.events, p1).await, ImportStatus::Queued);
    assert_eq!(missing(&tc).await, Vec::<&str>::new(), "rebuilt after the paused job");
    fx.mgr.shutdown(JOB_TIMEOUT).await.unwrap();

    // (b) job failed by the free-space guard
    let before = index_oids(&tc).await;
    let tuning = ImportTuning { writer: WriterLimits { min_free_bytes: u64::MAX, ..WriterLimits::default() }, ..ImportTuning::default() };
    let mut fx = manager(&tc, tuning);
    let p2 = fx.mgr.enqueue_synthetic(&synthetic_path(&tc, "disk.pst"), folders(), corpus_tagged(20, "d")).await.unwrap();
    fx.mgr.start_worker();
    assert_eq!(wait_job(&mut fx.events, p2).await, ImportStatus::Failed);
    let r = tc.admin().await.query_one("SELECT stage, severity, message FROM import_errors WHERE pst_id = $1", &[&p2]).await.unwrap();
    assert_eq!((r.get::<_, &str>(0), r.get::<_, &str>(1), r.get::<_, &str>(2)), ("store", "job_failed", "disk space low"));
    assert_eq!(missing(&tc).await, Vec::<&str>::new(), "rebuilt after the failed job");
    let after = index_oids(&tc).await;
    assert!(SEARCH_INDEXES.iter().all(|i| before[i.name] != after[i.name]), "the failed job dropped and rebuilt them");
}

#[tokio::test(flavor = "multi_thread")]
async fn index_policy_incremental_keeps_indexes() {
    const FIRST: i64 = 20; // 18 messages
    const SECOND: i64 = 4; // 2 * 4 < 18 → incremental
    const VACUUMED: &str = "SELECT count(*) FROM pg_stat_user_tables WHERE relname = 'messages' AND coalesce(last_vacuum, last_autovacuum) IS NOT NULL";
    let tc = TestCluster::start().await;
    assert_eq!(tc.count(VACUUMED).await, 0, "fresh database: never vacuumed");
    let fx = manager(&tc, small_batches());
    fx.mgr.import_synthetic(&synthetic_path(&tc, "big.pst"), folders(), corpus(FIRST)).await.unwrap();
    assert_eq!(snapshot(&tc).await, expected(&corpus(FIRST)), "the resume oracle matches a real uninterrupted import");
    assert_eq!(tc.count(VACUUMED).await, 1, "the after-job VACUUM ran");
    let before = index_oids(&tc).await;
    assert_eq!(before.len(), 6);
    fx.mgr.import_synthetic(&synthetic_path(&tc, "small.pst"), folders(), corpus_tagged(SECOND, "s")).await.unwrap();
    assert_eq!(tc.count("SELECT count(*) FROM messages").await, 18 + 4);
    assert_eq!(index_oids(&tc).await, before, "no DROP INDEX: OIDs unchanged");
}

// ------------------------------------------------------------------------------------------------ resume, crash, poison

/// Pause stops a running sync with its checkpoint and holds the job in the queue: no sync and no job start while
/// paused (a later queued path runs past it). Resume continues the same job from the checkpoint.
#[tokio::test(flavor = "multi_thread")]
async fn pause_resume_continues_from_checkpoint() {
    const ITEMS: i64 = 10;
    const JOBS: &str = "SELECT count(*) FROM import_jobs WHERE kind = 'import'";
    let tc = TestCluster::start().await;
    let mut tuning = small_batches();
    tuning.hold_reader_after = Some(5);
    let mut batches = batch_signal(&mut tuning);
    let mut fx = manager(&tc, tuning);
    let path = synthetic_path(&tc, "c.pst");
    let mut items = corpus(ITEMS);
    items[1].content = RawContent::Unreadable { stage: ImportErrorStage::Parse, message: "bad item".into() };
    let pst_id = fx.mgr.enqueue_synthetic(&path, folders(), items.clone()).await.unwrap();
    fx.mgr.start_worker();
    wait_batch(&mut batches, 1).await;
    assert!(fx.mgr.set_paused(pst_id, true).await.unwrap().paused);
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Queued, "held, not cancelled");
    let cursor = tc.count("SELECT cursor_seq FROM import_checkpoints").await;
    assert!((5..ITEMS).contains(&cursor), "paused mid-job: cursor {cursor}");
    let stored = tc.count("SELECT count(*) FROM messages").await;
    const ERRORS: &str = "SELECT count(*) FROM import_errors";
    assert_eq!(tc.count(ERRORS).await, 1, "the unreadable item is recorded once");
    // no sync while paused, even for a changed file; a later path is not blocked by the held job
    std::fs::write(&path, b"!BDN changed while paused").unwrap();
    fx.mgr.sync(SyncTarget::All).await.unwrap();
    fx.mgr.sync(SyncTarget::One(pst_id)).await.unwrap();
    let other = fx.mgr.enqueue_synthetic(&synthetic_path(&tc, "other.pst"), folders(), corpus_tagged(1, "o")).await.unwrap();
    assert_eq!(wait_job(&mut fx.events, other).await, ImportStatus::Completed);
    assert_eq!(tc.count(JOBS).await, 2, "no job queued for the paused path");
    assert_eq!(fx.mgr.list().await.unwrap().iter().find(|p| p.id == pst_id).unwrap().status, ImportStatus::Queued);
    assert_eq!(tc.count("SELECT count(*) FROM messages").await, stored + 1, "only the other path stored mail");
    // Resume: same job, checkpoint identity changed by the write above → cursor reset, known items skipped
    assert!(!fx.mgr.set_paused(pst_id, false).await.unwrap().paused);
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Completed);
    assert_eq!(tc.count(JOBS).await, 2, "the held job was resumed, not replaced");
    assert_eq!(snapshot(&tc).await, expected(&[items, corpus_tagged(1, "o")].concat()));
    assert_eq!(tc.count(ERRORS).await, 1, "the cursor reset re-read the item: still one error row");
}

#[tokio::test(flavor = "multi_thread")]
async fn resume_after_kill_equals_uninterrupted() {
    const ITEMS: i64 = 10;
    let tc = TestCluster::start().await;
    let path = synthetic_path(&tc, "k.pst");
    let mut tuning = small_batches();
    tuning.crash_after_batches = Some(1);
    tuning.hold_reader_after = Some(5); // the input cannot end before the crash seam fires
    let mut fx = manager(&tc, tuning);
    let pst_id = fx.mgr.enqueue_synthetic(&path, folders(), corpus(ITEMS)).await.unwrap();
    fx.mgr.start_worker();
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Running, "crash seam: job dropped like a process kill");
    assert_eq!(tc.count("SELECT count(*) FROM import_jobs WHERE state = 'running'").await, 1);
    drop(fx);
    // next process start
    let mut fx = manager(&tc, small_batches());
    assert_eq!(fx.mgr.recover_after_crash().await.unwrap(), RecoveryReport { requeued: vec![pst_id] });
    fx.mgr.start_worker();
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Completed);
    assert_eq!(snapshot(&tc).await, expected(&corpus(ITEMS)));
}

#[tokio::test(flavor = "multi_thread")]
async fn mtime_change_resets_cursor() {
    const ITEMS: i64 = 10;
    const MESSAGES: i64 = 9;
    let tc = TestCluster::start().await;
    let path = synthetic_path(&tc, "m.pst");
    let mut tuning = small_batches();
    tuning.hold_reader_after = Some(5);
    let mut batches = batch_signal(&mut tuning);
    let mut fx = manager(&tc, tuning);
    let pst_id = fx.mgr.enqueue_synthetic(&path, folders(), corpus(ITEMS)).await.unwrap();
    fx.mgr.start_worker();
    wait_batch(&mut batches, 1).await;
    fx.mgr.set_paused(pst_id, true).await.unwrap();
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Queued);
    let before_resume = tc.count("SELECT count(*) FROM messages").await;
    assert!(before_resume > 0);
    // touch: mtime + 60 s
    let f = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
    f.set_modified(SystemTime::now() + Duration::from_secs(60)).unwrap();
    drop(f);
    fx.mgr.set_paused(pst_id, false).await.unwrap();
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Completed);
    let r = tc
        .admin()
        .await
        .query_one("SELECT cursor_seq, stored, duplicates, skipped_non_mail FROM import_checkpoints WHERE pst_id = $1", &[&pst_id])
        .await
        .unwrap();
    let (cursor, stored, dups, non_mail): (i64, i64, i64, i64) = (r.get(0), r.get(1), r.get(2), r.get(3));
    assert_eq!(dups, before_resume, "cursor restarted at 0: every message stored before the touch counts again (known id)");
    assert_eq!((cursor, stored + dups, non_mail), (ITEMS, MESSAGES, 1));
    assert_eq!(tc.count("SELECT count(*) FROM messages").await, MESSAGES);
    assert_eq!(tc.count("SELECT count(*) FROM message_occurrences").await, MESSAGES);
}

type ErrorRow = (String, String, Option<i64>, Option<String>, String);

struct MarkerRun {
    errors: Vec<ErrorRow>,
    messages: i64,
    occurrences: i64,
}

/// Imports outlook.pst with an inflight marker written before the job starts, as a dead process leaves it
/// (Part 02 §6.3). `own_job`: the marker names this job, else another job id. The manager runs with a `Paths`
/// data root that differs from `Db::data_root()`: the marker must be read from and deleted in `Db::data_root()`.
/// `stale_checkpoint`: a checkpoint with another file identity exists, so the job resets its cursor.
async fn run_with_marker(own_job: bool, seq: i64, pst_item_id: i64, folder: &str, attempts: u32, stale_checkpoint: bool) -> MarkerRun {
    let tc = TestCluster::start().await;
    let elsewhere = tempfile::tempdir().unwrap();
    let (_phase, rx) = watch::channel(AppPhase::Ready);
    let mgr = ImportManager::new(
        tc.db.clone(),
        Gate::new(),
        tc.paths.with_data_root(elsewhere.path().to_path_buf()),
        Arc::new(NullSink),
        rx,
        ImportTuning::default(),
    );
    let mut events = mgr.job_events();
    let res = mgr.register(vec![s(&outlook_pst())]).await.unwrap();
    let EnqueueResult::Queued { pst } = &res[0] else { panic!("{res:?}") };
    let job_id: i64 = tc.admin().await.query_one("SELECT id FROM import_jobs WHERE pst_id = $1", &[&pst.id]).await.unwrap().get(0);
    let marker = Inflight::new(tc.db.data_root().unwrap().join("import-inflight"));
    let marker_job = if own_job { job_id } else { job_id + 1_000 };
    marker.write(&InflightRecord { job_id: marker_job, seq, folder_path: folder.into(), pst_item_id, attempts });
    if stale_checkpoint {
        Checkpoint::begin(&tc.app().await, pst.id, job_id, 1, SystemTime::UNIX_EPOCH).await.unwrap();
    }
    mgr.start_worker();
    assert_eq!(wait_job(&mut events, pst.id).await, ImportStatus::Completed);
    assert_eq!(marker.read(), None, "marker deleted at job end");
    assert!(!elsewhere.path().join("import-inflight").exists(), "nothing written under the Paths data root");
    let rows = tc.admin().await.query("SELECT stage, severity, pst_item_id, folder_path, message FROM import_errors", &[]).await.unwrap();
    MarkerRun {
        errors: rows.iter().map(|r| (r.get(0), r.get(1), r.get(2), r.get(3), r.get(4))).collect(),
        messages: tc.count("SELECT count(*) FROM messages").await,
        occurrences: tc.count("SELECT count(*) FROM message_occurrences").await,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn poison_marker_attempts_1_retries_item() {
    // pre-registered: one death on item 1 → the item is retried once and imported, no error row
    let r = run_with_marker(true, 1, 2_097_188, "Inbox", 1, false).await;
    assert_eq!(r.errors, Vec::<ErrorRow>::new());
    assert_eq!((r.messages, r.occurrences), (1, 2));
}

#[tokio::test(flavor = "multi_thread")]
async fn poison_marker_attempts_2_skips_item() {
    // pre-registered: two deaths on item 2 → item 1 is read again normally, item 2 is skipped with one parse row
    const POISON: (i64, i64, &str) = (2, 2_097_220, "Recovered_Group2"); // seq, pst_item_id, folder
    let r = run_with_marker(true, POISON.0, POISON.1, POISON.2, 2, false).await;
    assert_eq!(
        r.errors,
        [("parse".into(), "item_skipped".into(), Some(POISON.1), Some(POISON.2.into()), "parser crashed on this item".into())]
    );
    assert_eq!((r.messages, r.occurrences), (1, 1));
}

#[tokio::test(flavor = "multi_thread")]
async fn poison_marker_of_other_job_is_ignored() {
    let r = run_with_marker(false, 1, 2_097_188, "Inbox", 2, false).await;
    assert_eq!(r.errors, Vec::<ErrorRow>::new());
    assert_eq!((r.messages, r.occurrences), (1, 2));
}

#[tokio::test(flavor = "multi_thread")]
async fn poison_marker_dropped_on_checkpoint_reset() {
    // pre-registered: the file identity changed, so `seq` may name another item → the marker is not honored
    let r = run_with_marker(true, 2, 2_097_220, "Recovered_Group2", 2, true).await;
    assert_eq!(r.errors, Vec::<ErrorRow>::new());
    assert_eq!((r.messages, r.occurrences), (1, 2));
}

/// Rule 2: before the retry the marker on disk already says attempts = 2, so a death during the retry leads to the
/// skip of `poison_marker_attempts_2_skips_item` on the next start.
#[tokio::test(flavor = "multi_thread")]
async fn retry_rewrites_marker_before_reading() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use outlooker_core::importer::progress::{BYTE_BUDGET, ByteBudget};
    use outlooker_core::importer::pst_reader::{ReaderMsg, ReaderParams, spawn};
    let dir = tempfile::tempdir().unwrap();
    let marker = Inflight::new(dir.path().join("import-inflight"));
    let pending = InflightRecord { job_id: 9, seq: 1, folder_path: "Inbox".into(), pst_item_id: 2_097_188, attempts: 1 };
    let cancel = Arc::new(AtomicBool::new(false));
    let budget = Arc::new(ByteBudget::new(BYTE_BUDGET));
    let (rx, reader) = spawn(ReaderParams {
        path: outlook_pst(),
        cursor_seq: 0,
        pending: Some(pending.clone()),
        inflight: Some(marker.clone()),
        cancel: cancel.clone(),
        hold_after: Some(1), // the reader stops after item 1 until cancelled
        budget: budget.clone(),
        job_id: 9,
        known: Default::default(),
        poison: Default::default(),
    });
    assert!(matches!(rx.recv().unwrap(), ReaderMsg::Scanned(_)));
    let ReaderMsg::Item(item) = rx.recv().unwrap() else { panic!("item 1 expected") };
    assert_eq!(item.seq, 1);
    assert_eq!(marker.read(), Some(InflightRecord { attempts: 2, ..pending }));
    cancel.store(true, Ordering::SeqCst);
    budget.wake_all();
    reader.join().unwrap();
}

/// Part 02 §6.1: the restart budget runs out (phase `Failed`), the user retries (`app_retry` → `Ready` in the same
/// process). The paused job must resume from its checkpoint; `recover_after_crash` runs only once per process.
#[tokio::test(flavor = "multi_thread")]
async fn pg_crash_failed_then_retry_resumes() {
    const ITEMS: i64 = 10;
    let tc = TestCluster::start().await;
    let mut tuning = small_batches();
    tuning.hold_reader_after = Some(5);
    let mut batches = batch_signal(&mut tuning);
    let mut fx = manager(&tc, tuning);
    let pst_id = fx.mgr.enqueue_synthetic(&synthetic_path(&tc, "failed.pst"), folders(), corpus(ITEMS)).await.unwrap();
    fx.mgr.start_worker();
    wait_batch(&mut batches, 1).await;
    tc.cut_db().await;
    fx.phase.send_replace(AppPhase::StartingDb);
    fx.phase.send_replace(AppPhase::Failed);
    tc.restore_db().await; // the job observes `Failed` while the cluster starts again
    fx.phase.send_replace(AppPhase::Ready);
    let end = tokio::time::timeout(Duration::from_secs(20), wait_job(&mut fx.events, pst_id)).await;
    assert_eq!(end.ok(), Some(ImportStatus::Completed), "the paused job resumes on the next Ready");
    assert_eq!(snapshot(&tc).await, expected(&corpus(ITEMS)));
    let dup = tc.count("SELECT count(*) FROM (SELECT 1 FROM message_occurrences GROUP BY pst_id, pst_item_id HAVING count(*) > 1) d").await;
    assert_eq!(dup, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn recover_after_crash_states() {
    const CURSOR: i64 = 10;
    const RESUMED_SUBJECTS: [&str; 9] =
        ["Mail m11", "Mail m12", "Mail m13", "Mail m14", "Mail m15", "Mail m16", "Mail m17", "Mail m18", "Mail m19"];
    let tc = TestCluster::start().await;
    let mut fx = manager(&tc, small_batches());
    let path_a = synthetic_path(&tc, "ra.pst");
    let a = fx.mgr.enqueue_synthetic(&path_a, folders(), corpus(20)).await.unwrap();
    let adm = tc.admin().await;
    // a: running with a checkpoint at seq 10
    adm.execute("UPDATE import_jobs SET state = 'running', started_at = now() WHERE pst_id = $1", &[&a]).await.unwrap();
    let meta = std::fs::metadata(&path_a).unwrap();
    adm.execute(
        "INSERT INTO import_checkpoints (pst_id, job_id, phase, items_total, cursor_seq, pst_file_size, pst_file_mtime) \
         SELECT $1, j.id, 'importing', 20, $2, $3, $4 FROM import_jobs j WHERE j.pst_id = $1",
        &[&a, &CURSOR, &(meta.len() as i64), &truncate_to_micros(meta.modified().unwrap())],
    )
    .await
    .unwrap();
    assert_eq!(fx.mgr.recover_after_crash().await.unwrap(), RecoveryReport { requeued: vec![a] });
    let states: Vec<(i64, String, bool)> = adm
        .query("SELECT pst_id, state, finished_at IS NOT NULL FROM import_jobs ORDER BY pst_id", &[])
        .await
        .unwrap()
        .iter()
        .map(|r| (r.get(0), r.get(1), r.get(2)))
        .collect();
    assert_eq!(states, [(a, "queued".to_string(), false)]);
    fx.mgr.start_worker();
    assert_eq!(wait_job(&mut fx.events, a).await, ImportStatus::Completed);
    let subjects: Vec<String> = adm.query("SELECT subject FROM messages ORDER BY subject", &[]).await.unwrap().iter().map(|r| r.get(0)).collect();
    assert_eq!(subjects, RESUMED_SUBJECTS, "only seq > cursor_seq was read");
}

// ------------------------------------------------------------------------------------------------ queue commands

#[tokio::test(flavor = "multi_thread")]
async fn pst_list_status_derivation() {
    // newest job wins: an older completed import plus a queued remove is Removing (single-job rows: queue.rs status_table)
    const TABLE: [(&[(&str, &str)], ImportStatus); 1] = [(&[("import", "completed"), ("remove", "queued")], ImportStatus::Removing)];
    let tc = TestCluster::start().await;
    let adm = tc.admin().await;
    let fx = manager(&tc, ImportTuning::default());
    // one case at a time: import_jobs_one_running allows one running job in the library
    for (n, (jobs, status)) in TABLE.iter().enumerate() {
        let id = insert_source(&adm, n, jobs).await;
        let list = fx.mgr.list().await.unwrap();
        assert_eq!((list.len(), list[0].status), (1, *status), "case {n}: {jobs:?}");
        adm.execute("DELETE FROM pst_sources WHERE id = $1", &[&id]).await.unwrap();
    }
    // order: Running first, then Queued / Removing by job id, then the rest newest first
    let order = [("import", "completed"), ("import", "queued"), ("remove", "queued"), ("import", "running"), ("import", "failed")];
    let mut ids = Vec::new();
    for (n, job) in order.iter().enumerate() {
        ids.push(insert_source(&adm, 100 + n, std::slice::from_ref(job)).await);
    }
    let listed: Vec<i64> = fx.mgr.list().await.unwrap().iter().map(|p| p.id).collect();
    assert_eq!(listed, [ids[3], ids[1], ids[2], ids[4], ids[0]]);
}

/// A source row with the given jobs (oldest first); returns its id.
async fn insert_source(adm: &tokio_postgres::Client, n: usize, jobs: &[(&str, &str)]) -> i64 {
    let id: i64 = adm
        .query_one(
            "INSERT INTO pst_sources (path, display_name, file_size_bytes) VALUES ($1, 'p', 1) RETURNING id",
            &[&format!("/status/{n}.pst")],
        )
        .await
        .unwrap()
        .get(0);
    for (kind, state) in jobs {
        adm.execute(
            "INSERT INTO import_jobs (pst_id, kind, state, started_at, finished_at) VALUES ($1, $2, $3, \
               CASE WHEN $3 = 'queued' THEN NULL ELSE now() END, \
               CASE WHEN $3 IN ('failed', 'completed') THEN now() END)",
            &[&id, kind, state],
        )
        .await
        .unwrap();
    }
    id
}

#[tokio::test(flavor = "multi_thread")]
async fn pst_remove_with_queued_job() {
    let tc = TestCluster::start().await;
    let mut fx = manager(&tc, ImportTuning::default());
    let pst_id = fx.mgr.enqueue_synthetic(&synthetic_path(&tc, "q.pst"), folders(), corpus(3)).await.unwrap();
    // worker not started: the import job is still queued; Delete works on a paused path
    fx.mgr.set_paused(pst_id, true).await.unwrap();
    fx.mgr.remove(pst_id).await.unwrap(); // no 23505 from import_jobs_one_active_per_pst
    assert_eq!(tc.count("SELECT count(*) FROM import_jobs WHERE kind = 'import'").await, 0, "the queued import job is deleted");
    assert_eq!(fx.mgr.list().await.unwrap()[0].status, ImportStatus::Removing);
    assert!(matches!(fx.mgr.remove(pst_id).await, Err(AppError::Busy { .. })));
    assert!(matches!(fx.mgr.set_paused(pst_id, false).await, Err(AppError::Busy { .. })));
    assert!(matches!(fx.mgr.unregister(pst_id).await, Err(AppError::Busy { .. })));
    fx.mgr.start_worker();
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Removing);
    assert_eq!(tc.count("SELECT count(*) FROM pst_sources").await, 0);
    assert_eq!(tc.count("SELECT count(*) FROM messages").await, 0);
}

/// Registration identity is the canonical path: a changed file at a registered path, and other spellings of it
/// (`.`, `..`, doubled separators, letter case where the file system ignores case), are rejected, not a second source.
/// Probe rejections pass through (the classification itself: probe.rs unit tests); a held file registers as locked.
#[tokio::test(flavor = "multi_thread")]
async fn register_same_path_rejected() {
    let tc = TestCluster::start().await;
    let fx = manager(&tc, ImportTuning::default());
    let dir = tempfile::tempdir().unwrap();
    let missing = s(&dir.path().join("missing.pst"));
    let path = synthetic_path(&tc, "r.pst");
    let held = s(&std::fs::canonicalize(synthetic_path(&tc, "held.pst")).unwrap()); // the probe sees the canonical path
    set_test_lock(&held, true);
    let res = fx.mgr.register(vec![missing.clone(), path.clone(), held.clone()]).await.unwrap();
    set_test_lock(&held, false);
    assert!(matches!(&res[0], EnqueueResult::Rejected { error: AppError::FileAccess { reason, .. }, .. } if reason == "notFound"), "{res:?}");
    let EnqueueResult::Queued { pst } = &res[1] else { panic!("{res:?}") };
    assert_eq!(pst.status, ImportStatus::Queued);
    let EnqueueResult::Queued { pst: locked } = &res[2] else { panic!("{res:?}") };
    assert_eq!(locked.status, ImportStatus::Locked, "registered while held: one locked job, retried by the next sync");
    std::fs::write(&path, b"!BDN changed and longer").unwrap();
    let canonical = s(&std::fs::canonicalize(&path).unwrap());
    assert_eq!(pst.path, canonical);
    let dir = Path::new(&path).parent().unwrap();
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    let d = s(dir);
    let upper = format!("{d}/R.PST");
    let mut variants = vec![path.clone(), format!("{d}//r.pst"), format!("{d}/./r.pst"), format!("{d}/sub/../r.pst")];
    // ponytail: the case variant is the same file only where the file system ignores case (macOS, Windows)
    let case_insensitive = Path::new(&upper).exists();
    if case_insensitive {
        variants.push(upper.clone());
    }
    let again = fx.mgr.register(variants.clone()).await.unwrap();
    for (v, r) in variants.iter().zip(&again) {
        assert!(matches!(r, EnqueueResult::Rejected { error: AppError::AlreadyImported { pst_id }, path } if *pst_id == pst.id && path == v), "{v}: {r:?}");
    }
    if !case_insensitive {
        let r = fx.mgr.register(vec![upper]).await.unwrap();
        assert!(matches!(&r[0], EnqueueResult::Rejected { error: AppError::FileAccess { .. }, .. }), "{r:?}");
    }
    assert_eq!(tc.count("SELECT count(*) FROM pst_sources").await, 2);
    assert_eq!(fx.mgr.register(vec![]).await.unwrap_err(), AppError::invalid("paths", "empty"));
    assert_eq!(fx.mgr.register(vec![missing; 101]).await.unwrap_err(), AppError::invalid("paths", "tooMany"));
}

// ------------------------------------------------------------------------------------------------ sync

/// A changed PST is re-synced on the same source row; items whose PST item id already has an occurrence are not
/// read (the second run serves them as unreadable: a re-read would leave item_skipped rows) and count as duplicates.
#[tokio::test(flavor = "multi_thread")]
async fn sync_changed_file_keeps_one_row_and_skips_known() {
    const FIRST: i64 = 20; // 18 mails, 2 contacts
    const SECOND: i64 = 25; // + 5 new mails
    const NEW: i64 = 5;
    const KNOWN: i64 = 18;
    let tc = TestCluster::start().await;
    let mut fx = manager(&tc, small_batches());
    let path = synthetic_path(&tc, "sync.pst");
    let pst_id = fx.mgr.enqueue_synthetic(&path, folders(), corpus(FIRST)).await.unwrap();
    fx.mgr.start_worker();
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Completed);
    let oids = index_oids(&tc).await;
    let mut second = corpus(SECOND);
    for it in second.iter_mut().filter(|i| i.seq <= FIRST && matches!(i.content, RawContent::Mail(_))) {
        it.content = RawContent::Unreadable { stage: ImportErrorStage::Parse, message: "known item was read".into() };
    }
    ImportManager::set_synthetic_items(&path, folders(), second);
    std::fs::write(&path, b"!BDN the file grew in Outlook").unwrap();
    fx.mgr.sync(SyncTarget::One(pst_id)).await.unwrap();
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Completed);
    assert_eq!(tc.count("SELECT count(*) FROM pst_sources").await, 1);
    assert_eq!(tc.count("SELECT count(*) FROM import_errors").await, 0, "no known item reached normalize");
    let r = tc.admin().await.query_one("SELECT stored, duplicates, skipped_non_mail FROM import_checkpoints", &[]).await.unwrap();
    assert_eq!((r.get::<_, i64>(0), r.get::<_, i64>(1), r.get::<_, i64>(2)), (NEW, KNOWN, 2));
    let listed = &fx.mgr.list().await.unwrap()[0];
    assert_eq!((listed.new_message_count, listed.message_count), (NEW, KNOWN + NEW));
    assert!(listed.last_synced_at.is_some() && listed.last_checked_at.is_some());
    assert_eq!(tc.count("SELECT count(*) FROM messages").await, KNOWN + NEW);
    assert_eq!(index_oids(&tc).await, oids, "known items do not count as remaining: no index drop");
}

/// Unchanged file after a completed run: no job, only last_checked_at. A paused path is skipped by `All` and `One`.
#[tokio::test(flavor = "multi_thread")]
async fn sync_unchanged_creates_no_job() {
    const JOBS: &str = "SELECT count(*) FROM import_jobs";
    let tc = TestCluster::start().await;
    let mut fx = manager(&tc, ImportTuning::default());
    let path = synthetic_path(&tc, "u.pst");
    let pst_id = fx.mgr.enqueue_synthetic(&path, folders(), corpus(3)).await.unwrap();
    fx.mgr.start_worker();
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Completed);
    assert_eq!(fx.mgr.list().await.unwrap()[0].last_checked_at, None);
    fx.mgr.sync(SyncTarget::All).await.unwrap();
    assert_eq!(tc.count(JOBS).await, 1, "unchanged: no job");
    assert!(fx.mgr.list().await.unwrap()[0].last_checked_at.is_some());

    fx.mgr.set_paused(pst_id, true).await.unwrap();
    std::fs::write(&path, b"!BDN changed").unwrap();
    fx.mgr.sync(SyncTarget::All).await.unwrap();
    fx.mgr.sync(SyncTarget::One(pst_id)).await.unwrap();
    assert_eq!(tc.count(JOBS).await, 1, "paused: All and One skip it");
    fx.mgr.set_paused(pst_id, false).await.unwrap();
    fx.mgr.sync(SyncTarget::One(pst_id)).await.unwrap();
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Completed);
    assert_eq!(tc.count(JOBS).await, 2);
    assert_eq!(fx.mgr.sync(SyncTarget::One(pst_id + 1)).await.unwrap_err(), AppError::not_found("pst", pst_id + 1));
}

/// Unregister hides the path and stops every sync of it; its mail stays searchable and readable. Registering the same
/// file again (another spelling) continues on the same source row: no second row, no duplicate message or occurrence.
#[tokio::test(flavor = "multi_thread")]
async fn unregister_keeps_mail_and_reregister_adds_no_duplicates() {
    const FIRST: i64 = 10; // 9 mails
    const SECOND: i64 = 12; // + 2 mails (seq 11, 12)
    let tc = TestCluster::start().await;
    let mut fx = manager(&tc, ImportTuning::default());
    let path = s(&std::fs::canonicalize(synthetic_path(&tc, "un.pst")).unwrap());
    let pst_id = fx.mgr.enqueue_synthetic(&path, folders(), corpus(FIRST)).await.unwrap();
    fx.mgr.start_worker();
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Completed);
    fx.mgr.unregister(pst_id).await.unwrap();
    assert!(fx.mgr.list().await.unwrap().is_empty(), "gone from the list");
    let search = SearchService::new(tc.db.clone());
    let page = search.search(SearchRequest { query: SearchQuery::Simple { text: "Mail".into() }, cursor: None, page_size: None }).await.unwrap();
    assert_eq!(page.rows.len(), 9, "still searchable");
    let detail = search.mail_get(page.rows[0].id).await.unwrap();
    assert_eq!(detail.occurrences[0].pst_path, path, "still readable, with its source");
    // never synced again; Pause / Unregister / Sync now see no such path
    ImportManager::set_synthetic_items(&path, folders(), corpus(SECOND));
    std::fs::write(&path, b"!BDN the file grew after unregister").unwrap();
    fx.mgr.sync(SyncTarget::All).await.unwrap();
    assert_eq!(fx.mgr.sync(SyncTarget::One(pst_id)).await.unwrap_err(), AppError::not_found("pst", pst_id));
    assert_eq!(fx.mgr.set_paused(pst_id, true).await.unwrap_err(), AppError::not_found("pst", pst_id));
    assert_eq!(fx.mgr.unregister(pst_id).await.unwrap_err(), AppError::not_found("pst", pst_id));
    assert_eq!(tc.count("SELECT count(*) FROM import_jobs").await, 1);
    // registered again via another spelling of the same file
    let dir = Path::new(&path).parent().unwrap();
    let res = fx.mgr.register(vec![format!("{}/./un.pst", s(dir))]).await.unwrap();
    let EnqueueResult::Queued { pst } = &res[0] else { panic!("{res:?}") };
    assert_eq!((pst.id, pst.paused), (pst_id, false), "same source row");
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Completed);
    assert_eq!(tc.count("SELECT count(*) FROM pst_sources").await, 1);
    assert_eq!(tc.count("SELECT count(*) FROM messages").await, 11, "9 kept + 2 new, no duplicates");
    assert_eq!(tc.count("SELECT count(*) FROM message_occurrences").await, 11);
}

/// A held PST: the job-start probe ends the job `locked` before the checkpoint; later syncs while held keep that one
/// job row; no error row and no finalize (no VACUUM) at any point. Unlocked, the next sync completes.
#[tokio::test(flavor = "multi_thread")]
async fn sync_locked_no_error_row_no_finalize() {
    const VACUUMS: &str = "SELECT coalesce(sum(vacuum_count), 0)::bigint FROM pg_stat_user_tables WHERE relname = 'messages'";
    let tc = TestCluster::start().await;
    let mut fx = manager(&tc, ImportTuning::default());
    let path = synthetic_path(&tc, "l.pst");
    let pst_id = fx.mgr.enqueue_synthetic(&path, folders(), corpus(3)).await.unwrap();
    set_test_lock(&path, true); // locked while the job waited in the queue
    fx.mgr.start_worker();
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Locked);
    assert_eq!(tc.count("SELECT count(*) FROM import_checkpoints").await, 0, "checkpoint untouched");
    fx.mgr.sync(SyncTarget::One(pst_id)).await.unwrap();
    fx.mgr.sync(SyncTarget::All).await.unwrap();
    assert_eq!(tc.count("SELECT count(*) FROM import_jobs").await, 1, "one job row per locked streak");
    assert_eq!(tc.count("SELECT count(*) FROM import_jobs WHERE state = 'locked' AND finished_at IS NOT NULL").await, 1);
    assert_eq!(tc.count("SELECT count(*) FROM import_errors").await, 0);
    assert_eq!(tc.count(VACUUMS).await, 0, "no finalize");
    assert_eq!(fx.mgr.list().await.unwrap()[0].status, ImportStatus::Locked);
    set_test_lock(&path, false);
    fx.mgr.sync(SyncTarget::One(pst_id)).await.unwrap();
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Completed);
    assert_eq!(tc.count("SELECT count(*) FROM messages").await, 3);
}

/// Review round 1 finding 1: a removal that failed half-way keeps its path out of every sync, so the mail it already
/// deleted is not imported again.
#[tokio::test(flavor = "multi_thread")]
async fn sync_skips_path_after_failed_removal() {
    let tc = TestCluster::start().await;
    let mut fx = manager(&tc, ImportTuning::default());
    let path = synthetic_path(&tc, "rm.pst");
    let pst_id = fx.mgr.enqueue_synthetic(&path, folders(), corpus(3)).await.unwrap();
    fx.mgr.start_worker();
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Completed);
    tc.admin()
        .await
        .execute("INSERT INTO import_jobs (pst_id, kind, state, started_at, finished_at) VALUES ($1, 'remove', 'failed', now(), now())", &[&pst_id])
        .await
        .unwrap();
    std::fs::write(&path, b"!BDN changed since the import").unwrap();
    fx.mgr.sync(SyncTarget::All).await.unwrap();
    fx.mgr.sync(SyncTarget::One(pst_id)).await.unwrap();
    assert_eq!(tc.count("SELECT count(*) FROM import_jobs WHERE kind = 'import'").await, 1, "no re-import after a failed removal");
    assert_eq!(fx.mgr.list().await.unwrap()[0].status, ImportStatus::Failed);
}

/// Review round 1 finding 2: an item an earlier job skipped as poison (it killed the process twice) is not read by a
/// later sync job; it stays one error row.
#[tokio::test(flavor = "multi_thread")]
async fn sync_skips_earlier_poison_items() {
    let tc = TestCluster::start().await;
    let mut fx = manager(&tc, ImportTuning::default());
    let path = synthetic_path(&tc, "poison.pst");
    let pst_id = fx.mgr.enqueue_synthetic(&path, folders(), corpus(3)).await.unwrap();
    fx.mgr.start_worker();
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Completed);
    let poison = item(4, 1, RawContent::Unreadable { stage: ImportErrorStage::Parse, message: "poison item was read".into() });
    tc.admin()
        .await
        .execute(
            "INSERT INTO import_errors (pst_id, job_id, stage, severity, folder_path, pst_item_id, message) \
             SELECT $1, id, 'parse', 'item_skipped', 'Inbox', $2, 'parser crashed on this item' FROM import_jobs WHERE pst_id = $1",
            &[&pst_id, &poison.pst_item_id],
        )
        .await
        .unwrap();
    let mut second = corpus(3);
    second.push(poison);
    ImportManager::set_synthetic_items(&path, folders(), second);
    std::fs::write(&path, b"!BDN the file grew in Outlook").unwrap();
    fx.mgr.sync(SyncTarget::One(pst_id)).await.unwrap();
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Completed);
    assert_eq!(tc.count("SELECT count(*) FROM import_errors WHERE message = 'poison item was read'").await, 0, "poison item not read");
    assert_eq!(tc.count("SELECT count(*) FROM import_errors").await, 1);
}

/// Review round 1 finding 3: every sync re-reads unreadable items and re-runs a job that fails the same way; the
/// source keeps one error row per distinct error, so rows and the listed count do not grow per sync.
#[tokio::test(flavor = "multi_thread")]
async fn sync_repeated_errors_keep_one_row() {
    let tc = TestCluster::start().await;
    let mut fx = manager(&tc, ImportTuning::default());
    let path = synthetic_path(&tc, "bad-item.pst");
    let bad = vec![item(1, 1, RawContent::Unreadable { stage: ImportErrorStage::Parse, message: "bad item".into() })];
    let item_src = fx.mgr.enqueue_synthetic(&path, folders(), bad).await.unwrap();
    let broken = synthetic_path(&tc, "broken.pst"); // PST magic, but libpff cannot open it: the job fails every time
    let res = fx.mgr.register(vec![broken]).await.unwrap();
    let EnqueueResult::Queued { pst } = &res[0] else { panic!("{res:?}") };
    let failing = pst.id;
    fx.mgr.start_worker();
    assert_eq!(wait_jobs(&mut fx.events, &[item_src, failing]).await, [ImportStatus::Completed, ImportStatus::Failed]);
    std::fs::write(&path, b"!BDN changed").unwrap();
    fx.mgr.sync(SyncTarget::All).await.unwrap();
    assert_eq!(wait_jobs(&mut fx.events, &[item_src, failing]).await, [ImportStatus::Completed, ImportStatus::Failed]);
    assert_eq!(tc.count("SELECT count(*) FROM import_jobs").await, 4, "two runs of each path");
    let rows = |id: i64| format!("SELECT count(*) FROM import_errors WHERE pst_id = {id}");
    assert_eq!((tc.count(&rows(item_src)).await, tc.count(&rows(failing)).await), (1, 1));
    let list = fx.mgr.list().await.unwrap();
    let listed = |id: i64| list.iter().find(|p| p.id == id).unwrap();
    assert_eq!((listed(item_src).error_count, listed(failing).error_count), (1, 1));
    assert!(listed(failing).failure_reason.is_some(), "the newest failed job still shows its reason");
}

/// Contract item 11: a failed path whose next sync fails for another reason (file now missing) gets one new failed
/// job and error row, so the listed reason is the current one. The same reason again writes nothing.
#[tokio::test(flavor = "multi_thread")]
async fn sync_changed_failure_reason_writes_one_new_row() {
    let tc = TestCluster::start().await;
    let mut fx = manager(&tc, ImportTuning::default());
    let broken = synthetic_path(&tc, "reason.pst"); // PST magic, but libpff cannot open it: the job fails
    let res = fx.mgr.register(vec![broken.clone()]).await.unwrap();
    let EnqueueResult::Queued { pst } = &res[0] else { panic!("{res:?}") };
    let id = pst.id;
    fx.mgr.start_worker();
    assert_eq!(wait_job(&mut fx.events, id).await, ImportStatus::Failed);
    let old = fx.mgr.list().await.unwrap()[0].failure_reason.clone();
    let counts = || async { (tc.count("SELECT count(*) FROM import_jobs").await, tc.count("SELECT count(*) FROM import_errors").await) };
    assert_eq!(counts().await, (1, 1));
    std::fs::remove_file(&broken).unwrap();
    fx.mgr.sync(SyncTarget::One(id)).await.unwrap();
    assert_eq!(counts().await, (2, 2), "new reason: one new job and error row");
    let new = fx.mgr.list().await.unwrap()[0].failure_reason.clone();
    assert_ne!(new, old);
    fx.mgr.sync(SyncTarget::One(id)).await.unwrap();
    fx.mgr.sync(SyncTarget::All).await.unwrap();
    assert_eq!(counts().await, (2, 2), "same reason: nothing written");
}

/// Contract override 9: a probe stuck on one path (a stalled network share) does not delay another path's sync or
/// the import worker.
#[tokio::test(flavor = "multi_thread")]
async fn sync_blocked_probe_does_not_delay_other_paths() {
    let tc = TestCluster::start().await;
    let mut fx = manager(&tc, ImportTuning::default());
    let slow = synthetic_path(&tc, "slow.pst");
    let fast = synthetic_path(&tc, "fast.pst");
    let a = fx.mgr.enqueue_synthetic(&slow, folders(), corpus(3)).await.unwrap();
    let b = fx.mgr.enqueue_synthetic(&fast, folders(), corpus_tagged(3, "f")).await.unwrap();
    fx.mgr.start_worker();
    assert_eq!(wait_jobs(&mut fx.events, &[a, b]).await, [ImportStatus::Completed; 2]);
    std::fs::write(&slow, b"!BDN changed slow").unwrap();
    std::fs::write(&fast, b"!BDN changed fast").unwrap();
    let hold = hold_probe(&slow); // the slow path sorts first: a serial sync would wait on it
    let mgr = fx.mgr.clone();
    let sync = tokio::spawn(async move { mgr.sync(SyncTarget::All).await });
    let fast_end = tokio::time::timeout(Duration::from_secs(3), wait_job(&mut fx.events, b)).await;
    assert_eq!(fast_end.ok(), Some(ImportStatus::Completed), "the fast path synced and imported while the slow probe hangs");
    assert!(!sync.is_finished());
    drop(hold);
    sync.await.unwrap().unwrap();
    assert_eq!(wait_job(&mut fx.events, a).await, ImportStatus::Completed);
}

// ------------------------------------------------------------------------------------------------ Outlook fixtures (W2)

#[cfg(feature = "outlook-fixtures")]
mod fixtures {
    use super::*;

    /// The sanitizer acceptance patterns (spec Part 04 §12.2 row Sanitizer), matched without a regex crate.
    fn hostile_violations(out: &str) -> Vec<String> {
        let lower = out.to_ascii_lowercase();
        let mut found: Vec<String> =
            ["<script", "javascript:", "url(", "<iframe", "<form", "position", "src=\"http"].iter().filter(|p| lower.contains(*p)).map(|p| p.to_string()).collect();
        let b = lower.as_bytes();
        for i in 0..b.len().saturating_sub(2) {
            if b[i] == b'o' && b[i + 1] == b'n' && (i == 0 || !b[i - 1].is_ascii_alphanumeric()) {
                let mut j = i + 2;
                while j < b.len() && b[j].is_ascii_lowercase() {
                    j += 1;
                }
                if j > i + 2 && j < b.len() && b[j] == b'=' {
                    found.push(lower[i..=j].to_string());
                }
            }
        }
        found
    }

    fn fixture(name: &str) -> String {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join(REPO_ROOT).join(format!("tests/fixtures/pst/{name}.pst"));
        assert!(p.exists(), "missing fixture tests/fixtures/pst/{name}.pst (Windows phase W2)");
        s(&std::fs::canonicalize(p).unwrap())
    }

    /// Imports the fixtures in order; every job must end Completed.
    async fn import(names: &[&str]) -> (TestCluster, Fx, Vec<i64>) {
        let paths: Vec<String> = names.iter().map(|n| fixture(n)).collect();
        let tc = TestCluster::start().await;
        let mut fx = manager(&tc, ImportTuning::default());
        let res = fx.mgr.register(paths).await.unwrap();
        let ids: Vec<i64> = res.iter().map(|r| if let EnqueueResult::Queued { pst } = r { pst.id } else { panic!("{r:?}") }).collect();
        fx.mgr.start_worker();
        assert!(wait_jobs(&mut fx.events, &ids).await.iter().all(|s| *s == ImportStatus::Completed));
        (tc, fx, ids)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fixtures_basic() {
        let ids = walk_ids(Path::new(&fixture("basic"))).unwrap();
        assert_eq!(ids, walk_ids(Path::new(&fixture("basic"))).unwrap(), "deterministic order");
        let (tc, _fx, _) = import(&["basic"]).await;
        assert_eq!(tc.count("SELECT count(*) FROM messages").await, 50);
        assert!(tc.count("SELECT count(*) FROM folders WHERE path = 'Inbox/Projects'").await == 1);
        assert!(tc.count("SELECT count(DISTINCT body_source) FROM messages WHERE body_source IN ('html', 'rtf_html', 'rtf_text')").await >= 2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fixtures_cjk() {
        let (tc, _fx, _) = import(&["cjk"]).await;
        assert_eq!(tc.count("SELECT count(*) FROM messages").await, 30);
        assert_eq!(tc.count("SELECT count(*) FROM messages WHERE body_text ~ '[一-鿿]'").await, 30);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fixtures_inline() {
        let (tc, _fx, _) = import(&["inline"]).await;
        assert_eq!(tc.count("SELECT count(*) FROM messages").await, 20);
        assert_eq!(
            tc.count("SELECT count(*) FROM (SELECT image_blob_id FROM inline_images GROUP BY 1 HAVING count(DISTINCT message_id) = 10) l").await,
            1,
            "the logo of 10 mails is one blob"
        );
        let dangling = tc.count(
            "SELECT count(*) FROM messages m, regexp_matches(m.body_html, 'outlooker-inline:([0-9]+)', 'g') x \
              WHERE NOT EXISTS (SELECT 1 FROM inline_images i WHERE i.message_id = m.id AND i.ordinal = x[1]::int)",
        )
        .await;
        assert_eq!(dangling, 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fixtures_attach() {
        let (tc, _fx, _) = import(&["attach"]).await;
        assert!(tc.count("SELECT count(*) FROM attachments WHERE file_name LIKE '%.msg' AND mime_type = 'application/vnd.ms-outlook'").await >= 1);
        assert!(tc.count("SELECT count(*) FROM attachments WHERE length(file_name) > 255").await >= 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fixtures_dup() {
        let (tc, mut fx, ids) = import(&["dup-a", "dup-b"]).await;
        assert_eq!(tc.count("SELECT count(*) FROM messages").await, 50);
        assert_eq!(tc.count("SELECT count(*) FROM message_occurrences").await, 81);
        fx.mgr.remove(ids[0]).await.unwrap();
        assert_eq!(wait_job(&mut fx.events, ids[0]).await, ImportStatus::Removing);
        assert_eq!(tc.count("SELECT count(*) FROM messages").await, 40, "shared messages remain");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fixtures_hostile() {
        let (tc, _fx, _) = import(&["hostile"]).await;
        let rows = tc.admin().await.query("SELECT body_html FROM messages WHERE body_html IS NOT NULL", &[]).await.unwrap();
        assert!(!rows.is_empty());
        for r in rows {
            let html: String = r.get(0);
            assert_eq!(hostile_violations(&html), Vec::<String>::new(), "{html}");
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fixtures_corrupt() {
        let (tc, _fx, _) = import(&["corrupt"]).await;
        assert!(tc.count("SELECT count(*) FROM import_errors WHERE severity = 'item_skipped'").await >= 1);
        assert_eq!(tc.count("SELECT count(*) FROM messages").await, 49);
    }
}
