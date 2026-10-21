//! R2-09 Import Manager integration tests (spec Part 02 §4, §6.1–§6.3; Part 03 §7; Part 04 §2.1, §5, §6.4, §6.5, §12.2).
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
use outlooker_core::dto::import::{EnqueueResult, ImportStatus};
use outlooker_core::gate::Gate;
use outlooker_core::importer::inflight::{Inflight, InflightRecord};
use outlooker_core::importer::pst_reader::walk_ids;
use outlooker_core::importer::types::{FolderIn, RawAttachment, RawContent, RawItem, RawMail, RawRecipient};
use outlooker_core::importer::writer::{Checkpoint, WriterLimits};
use outlooker_core::importer::{ImportManager, ImportTuning, RecoveryReport};
use outlooker_core::sink::NullSink;
use outlooker_core::test_support::TestCluster;
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

/// Source file for synthetic items: 20 bytes starting with the PST magic, so enqueue identity (size + mtime) and
/// the re-validation of `resume` work as for a real PST.
fn synthetic_path(tc: &TestCluster, name: &str) -> String {
    let p = tc.dir.path().join(name);
    std::fs::write(&p, b"!BDN0000000000000000").unwrap();
    s(&p)
}

async fn count(tc: &TestCluster, sql: &str) -> i64 {
    tc.admin().await.query_one(sql, &[]).await.unwrap().get(0)
}

#[derive(Debug, PartialEq, Eq)]
struct Snapshot {
    keys: BTreeSet<Vec<u8>>,
    messages: i64,
    occurrences: i64,
    recipients: i64,
    attachments: i64,
}

async fn snapshot(tc: &TestCluster) -> Snapshot {
    let a = tc.admin().await;
    let keys = a.query("SELECT dedupe_key FROM messages", &[]).await.unwrap().iter().map(|r| r.get::<_, Vec<u8>>(0)).collect();
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

/// The library one uninterrupted import of `items` produces (own TestCluster).
async fn reference(items: Vec<RawItem>) -> Snapshot {
    let tc = TestCluster::start().await;
    let fx = manager(&tc, small_batches());
    let pst_id = fx.mgr.import_synthetic(&synthetic_path(&tc, "ref.pst"), folders(), items).await.unwrap();
    let list = fx.mgr.list().await.unwrap();
    assert_eq!(list.iter().find(|p| p.id == pst_id).unwrap().status, ImportStatus::Completed);
    snapshot(&tc).await
}

fn png(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_fn(w, h, |x, y| image::Rgba([(x % 256) as u8, (y % 256) as u8, 90, 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

/// The R1-05 sanitizer acceptance patterns (spec Part 04 §12.2 row Sanitizer), matched without a regex crate.
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
    let res = fx.mgr.enqueue(vec![s(&pst)]).await.unwrap();
    let EnqueueResult::Queued { pst: src } = &res[0] else { panic!("{res:?}") };
    assert_eq!(src.status, ImportStatus::Queued);
    fx.mgr.start_worker();
    assert_eq!(wait_job(&mut fx.events, src.id).await, ImportStatus::Completed);

    let a = tc.admin().await;
    let paths: Vec<String> = a.query("SELECT path FROM folders", &[]).await.unwrap().iter().map(|r| r.get(0)).collect();
    assert!(paths.iter().any(|p| p == "Inbox") && paths.iter().any(|p| p == "Sent Items"), "{paths:?}");
    assert_eq!(count(&tc, "SELECT count(*) FROM messages").await, MESSAGES);
    assert_eq!(count(&tc, "SELECT count(*) FROM message_occurrences").await, OCCURRENCES);
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
    let res = fx.mgr.enqueue(vec![s(&outlook_pst()), s(&copy)]).await.unwrap();
    let ids: Vec<i64> = res.iter().map(|r| if let EnqueueResult::Queued { pst } = r { pst.id } else { panic!("{r:?}") }).collect();
    fx.mgr.start_worker();
    assert_eq!(wait_jobs(&mut fx.events, &ids).await, [ImportStatus::Completed, ImportStatus::Completed]);
    assert_eq!(count(&tc, "SELECT count(*) FROM messages").await, MESSAGES);
    assert_eq!(count(&tc, "SELECT count(*) FROM message_occurrences").await, OCCURRENCES_BOTH);
    assert_eq!(count(&tc, "SELECT count(*) FROM messages WHERE dedupe_kind = 'content_hash'").await, 1, "no Message-ID → fallback hash");

    fx.mgr.remove(ids[0]).await.unwrap();
    assert_eq!(wait_job(&mut fx.events, ids[0]).await, ImportStatus::Removing);
    assert_eq!(count(&tc, "SELECT count(*) FROM messages").await, MESSAGES, "removing one source keeps the shared message");
    assert_eq!(count(&tc, "SELECT count(*) FROM message_occurrences").await, OCCURRENCES_ONE);
    assert!(count(&tc, "SELECT count(*) FROM addresses").await > 0);

    fx.mgr.remove(ids[1]).await.unwrap();
    assert_eq!(wait_job(&mut fx.events, ids[1]).await, ImportStatus::Removing);
    for table in ["pst_sources", "messages", "message_occurrences", "attachments", "addresses", "message_recipients"] {
        assert_eq!(count(&tc, &format!("SELECT count(*) FROM {table}")).await, 0, "{table} after removing both");
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
    let res = fx.mgr.enqueue(vec![s(&corrupt)]).await.unwrap();
    let EnqueueResult::Queued { pst } = &res[0] else { panic!("{res:?}") };
    fx.mgr.start_worker();
    assert_eq!(wait_job(&mut fx.events, pst.id).await, EXPECT_STATUS);
    let skipped = count(&tc, "SELECT count(*) FROM import_errors WHERE severity = 'item_skipped' AND stage = 'parse'").await;
    assert!(skipped >= EXPECT_MIN_SKIPPED_PARSE, "item_skipped/parse rows: {skipped}");
    assert_eq!(count(&tc, "SELECT count(*) FROM messages").await, EXPECT_MESSAGES);
}

// ------------------------------------------------------------------------------------------------ synthetic items

#[tokio::test(flavor = "multi_thread")]
async fn synthetic_cjk() {
    const EXPECTED: &str = "報價單";
    let (big5, _, unmappable) = encoding_rs::BIG5.encode("<html><body><p>報價單 Big5 版本</p></body></html>");
    assert!(!unmappable);
    let items = vec![
        item(1, 1, mail(RawMail { subject: Some("Big5".into()), internet_codepage: Some(950), html_body: Some(big5.into_owned()), ..Default::default() })),
        item(
            2,
            1,
            mail(RawMail {
                subject: Some("UTF-8".into()),
                internet_codepage: Some(65001),
                html_body: Some("<html><body><p>報價單 UTF-8 版本</p></body></html>".as_bytes().to_vec()),
                ..Default::default()
            }),
        ),
    ];
    let tc = TestCluster::start().await;
    let fx = manager(&tc, ImportTuning::default());
    fx.mgr.import_synthetic(&synthetic_path(&tc, "cjk.pst"), folders(), items).await.unwrap();
    let rows = tc.admin().await.query("SELECT subject, body_text FROM messages ORDER BY subject", &[]).await.unwrap();
    assert_eq!(rows.len(), 2);
    for r in &rows {
        let (subject, text): (String, String) = (r.get(0), r.get(1));
        assert!(text.contains(EXPECTED), "{subject}: {text:?}");
    }
}

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
    assert_eq!(count(&tc, "SELECT count(*) FROM messages").await, MAILS);
    assert_eq!(count(&tc, "SELECT count(*) FROM image_blobs").await, BLOBS, "the repeated logo is stored once");
    assert_eq!(count(&tc, "SELECT count(*) FROM inline_images").await, MAILS);
    assert_eq!(count(&tc, "SELECT count(*) FROM messages WHERE body_html LIKE '%outlooker-inline:0%'").await, MAILS);
    // every outlooker-inline:n reference has its inline_images row
    let dangling = count(
        &tc,
        "SELECT count(*) FROM messages m, regexp_matches(m.body_html, 'outlooker-inline:([0-9]+)', 'g') x \
          WHERE NOT EXISTS (SELECT 1 FROM inline_images i WHERE i.message_id = m.id AND i.ordinal = x[1]::int)",
    )
    .await;
    assert_eq!(dangling, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn synthetic_attachments() {
    let long_name = format!("{}.txt", "n".repeat(1_100));
    let atts = vec![
        RawAttachment { index: 0, attach_method: Some(5), embedded_subject: Some("Re: plan".into()), ..Default::default() },
        RawAttachment { index: 1, long_filename: Some("report.pdf".into()), data_size: Some(1_234), attach_method: Some(1), ..Default::default() },
        RawAttachment { index: 2, long_filename: Some(long_name), data_size: Some(10), attach_method: Some(1), ..Default::default() },
    ];
    // pre-registered (Part 04 §5.8): (file_name, chars, mime_type)
    let expected: Vec<(String, i32, String)> = vec![
        ("Re: plan.msg".into(), 12, "application/vnd.ms-outlook".into()),
        ("report.pdf".into(), 10, "application/pdf".into()),
        // cut to 1,024 chars before the MIME guess: the extension is gone
        ("n".repeat(1_024), 1_024, "application/octet-stream".into()),
    ];
    let tc = TestCluster::start().await;
    let fx = manager(&tc, ImportTuning::default());
    let items = vec![item(1, 1, mail(RawMail { subject: Some("Attachments".into()), attachments: atts, ..Default::default() }))];
    fx.mgr.import_synthetic(&synthetic_path(&tc, "attach.pst"), folders(), items).await.unwrap();
    let rows = tc
        .admin()
        .await
        .query("SELECT file_name, length(file_name), mime_type FROM attachments ORDER BY attachment_index", &[])
        .await
        .unwrap();
    let got: Vec<(String, i32, String)> = rows.iter().map(|r| (r.get(0), r.get(1), r.get(2))).collect();
    assert_eq!(got, expected);
}

#[tokio::test(flavor = "multi_thread")]
async fn synthetic_hostile() {
    let hostile = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/hostile.html")).unwrap();
    let tc = TestCluster::start().await;
    let fx = manager(&tc, ImportTuning::default());
    let items = vec![item(1, 1, mail(RawMail { subject: Some("Hostile".into()), html_body: Some(hostile), ..Default::default() }))];
    fx.mgr.import_synthetic(&synthetic_path(&tc, "hostile.pst"), folders(), items).await.unwrap();
    let html: String = tc.admin().await.query_one("SELECT body_html FROM messages", &[]).await.unwrap().get(0);
    assert_eq!(hostile_violations(&html), Vec::<String>::new(), "stored body_html: {html}");
}

#[tokio::test(flavor = "multi_thread")]
async fn synthetic_message_id_dedupe() {
    // A: shared 1–3 + own a4 + shared 1 copied into the second folder; B: shared 1–3 + own b4
    const MESSAGES: i64 = 5;
    // stored message_id is the normalized form (angle brackets removed, Part 04 §4.1)
    const OCCURRENCES_PER_SHARED: [(&str, i64); 3] = [("shared1@x", 3), ("shared2@x", 2), ("shared3@x", 2)];
    let m = |id: &str, body: &str| mail(RawMail { internet_message_id: Some(id.into()), plain_body: Some(body.into()), ..Default::default() });
    let a = vec![
        item(1, 1, m("<shared1@x>", "a one")),
        item(2, 1, m("<shared2@x>", "a two")),
        item(3, 1, m("<shared3@x>", "a three")),
        item(4, 1, m("<a4@x>", "a four")),
        item(5, 2, m("<shared1@x>", "a one, copy in Projects")),
    ];
    let b = vec![
        item(1, 1, m("<shared1@x>", "b one")),
        item(2, 1, m("<shared2@x>", "b two")),
        item(3, 2, m("<shared3@x>", "b three")),
        item(4, 2, m("<b4@x>", "b four")),
    ];
    let tc = TestCluster::start().await;
    let fx = manager(&tc, ImportTuning::default());
    let pa = fx.mgr.import_synthetic(&synthetic_path(&tc, "a.pst"), folders(), a).await.unwrap();
    let pb = fx.mgr.import_synthetic(&synthetic_path(&tc, "b.pst"), folders(), b).await.unwrap();
    assert_eq!(count(&tc, "SELECT count(*) FROM messages").await, MESSAGES);
    let adm = tc.admin().await;
    for (id, occ) in OCCURRENCES_PER_SHARED {
        let r = adm
            .query_one(
                "SELECT count(DISTINCT m.id), count(*), count(*) FILTER (WHERE o.pst_id = $2), count(*) FILTER (WHERE o.pst_id = $3) \
                   FROM messages m JOIN message_occurrences o ON o.message_id = m.id WHERE m.message_id = $1",
                &[&id, &pa, &pb],
            )
            .await
            .unwrap();
        let (msgs, total, in_a, in_b): (i64, i64, i64, i64) = (r.get(0), r.get(1), r.get(2), r.get(3));
        assert_eq!((msgs, total), (1, occ), "{id}");
        assert_eq!((in_a, in_b), (occ - 1, 1), "{id}: one occurrence per source, the folder copy adds one in A");
    }
}

// ------------------------------------------------------------------------------------------------ index policy, vacuum

#[tokio::test(flavor = "multi_thread")]
async fn index_policy_first_import_drops_and_rebuilds() {
    const ITEMS: i64 = 20;
    const HOLD_AT: i64 = 10;
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
    // during the job (reader held after item 10, job running): all six dropped
    assert_eq!(missing(&tc).await, all_six());
    fx.mgr.cancel(pst_id).await.unwrap();
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Cancelled);
    fx.mgr.resume(pst_id).await.unwrap();
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Completed);
    assert_eq!(missing(&tc).await, Vec::<&str>::new(), "after Completed all six exist and are valid");
    let after = index_oids(&tc).await;
    assert!(SEARCH_INDEXES.iter().all(|i| before[i.name] != after[i.name]), "drop + create gives new OIDs: {before:?} {after:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn index_policy_rebuilt_after_cancel_and_disk_failure() {
    let tc = TestCluster::start().await;
    // (a) cancelled job
    let mut tuning = small_batches();
    tuning.hold_reader_after = Some(10);
    let mut batches = batch_signal(&mut tuning);
    let mut fx = manager(&tc, tuning);
    let p1 = fx.mgr.enqueue_synthetic(&synthetic_path(&tc, "cancelled.pst"), folders(), corpus(20)).await.unwrap();
    fx.mgr.start_worker();
    wait_batch(&mut batches, 1).await;
    assert_eq!(missing(&tc).await, all_six());
    fx.mgr.cancel(p1).await.unwrap();
    assert_eq!(wait_job(&mut fx.events, p1).await, ImportStatus::Cancelled);
    assert_eq!(missing(&tc).await, Vec::<&str>::new(), "rebuilt after the cancelled job");
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
    let tc = TestCluster::start().await;
    let fx = manager(&tc, small_batches());
    fx.mgr.import_synthetic(&synthetic_path(&tc, "big.pst"), folders(), corpus(FIRST)).await.unwrap();
    let before = index_oids(&tc).await;
    assert_eq!(before.len(), 6);
    fx.mgr.import_synthetic(&synthetic_path(&tc, "small.pst"), folders(), corpus_tagged(SECOND, "s")).await.unwrap();
    assert_eq!(count(&tc, "SELECT count(*) FROM messages").await, 18 + 4);
    assert_eq!(index_oids(&tc).await, before, "no DROP INDEX: OIDs unchanged");
}

#[tokio::test(flavor = "multi_thread")]
async fn after_job_vacuum_ran() {
    const SQL: &str = "SELECT count(*) FROM pg_stat_user_tables WHERE relname = 'messages' AND coalesce(last_vacuum, last_autovacuum) IS NOT NULL";
    let tc = TestCluster::start().await;
    assert_eq!(count(&tc, SQL).await, 0, "fresh database: never vacuumed");
    let fx = manager(&tc, ImportTuning::default());
    fx.mgr.import_synthetic(&synthetic_path(&tc, "v.pst"), folders(), corpus(3)).await.unwrap();
    assert_eq!(count(&tc, SQL).await, 1);
}

// ------------------------------------------------------------------------------------------------ resume, crash, poison

#[tokio::test(flavor = "multi_thread")]
async fn resume_after_cancel_equals_uninterrupted() {
    const ITEMS: i64 = 40;
    let want = reference(corpus(ITEMS)).await;
    let tc = TestCluster::start().await;
    let mut tuning = small_batches();
    tuning.hold_reader_after = Some(20);
    let mut batches = batch_signal(&mut tuning);
    let mut fx = manager(&tc, tuning);
    let pst_id = fx.mgr.enqueue_synthetic(&synthetic_path(&tc, "c.pst"), folders(), corpus(ITEMS)).await.unwrap();
    fx.mgr.start_worker();
    wait_batch(&mut batches, 1).await;
    fx.mgr.cancel(pst_id).await.unwrap();
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Cancelled);
    let cursor = count(&tc, "SELECT cursor_seq FROM import_checkpoints").await;
    assert!((5..ITEMS).contains(&cursor), "cancelled mid-job: cursor {cursor}");
    fx.mgr.resume(pst_id).await.unwrap();
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Completed);
    assert_eq!(snapshot(&tc).await, want);
}

#[tokio::test(flavor = "multi_thread")]
async fn resume_after_kill_equals_uninterrupted() {
    const ITEMS: i64 = 40;
    let want = reference(corpus(ITEMS)).await;
    let tc = TestCluster::start().await;
    let path = synthetic_path(&tc, "k.pst");
    let mut tuning = small_batches();
    tuning.crash_after_batches = Some(1);
    tuning.hold_reader_after = Some(20); // the input cannot end before the crash seam fires
    let mut fx = manager(&tc, tuning);
    let pst_id = fx.mgr.enqueue_synthetic(&path, folders(), corpus(ITEMS)).await.unwrap();
    fx.mgr.start_worker();
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Running, "crash seam: job dropped like a process kill");
    assert_eq!(count(&tc, "SELECT count(*) FROM import_jobs WHERE state = 'running'").await, 1);
    drop(fx);
    // next process start
    let mut fx = manager(&tc, small_batches());
    assert_eq!(fx.mgr.recover_after_crash().await.unwrap(), RecoveryReport { requeued: vec![pst_id], cancelled: vec![] });
    fx.mgr.start_worker();
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Completed);
    assert_eq!(snapshot(&tc).await, want);
}

#[tokio::test(flavor = "multi_thread")]
async fn mtime_change_resets_cursor() {
    const ITEMS: i64 = 20;
    const MESSAGES: i64 = 18;
    let tc = TestCluster::start().await;
    let path = synthetic_path(&tc, "m.pst");
    let mut tuning = small_batches();
    tuning.hold_reader_after = Some(10);
    let mut batches = batch_signal(&mut tuning);
    let mut fx = manager(&tc, tuning);
    let pst_id = fx.mgr.enqueue_synthetic(&path, folders(), corpus(ITEMS)).await.unwrap();
    fx.mgr.start_worker();
    wait_batch(&mut batches, 1).await;
    fx.mgr.cancel(pst_id).await.unwrap();
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Cancelled);
    let before_resume = count(&tc, "SELECT count(*) FROM messages").await;
    assert!(before_resume > 0);
    // touch: mtime + 60 s
    let f = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
    f.set_modified(SystemTime::now() + Duration::from_secs(60)).unwrap();
    drop(f);
    fx.mgr.resume(pst_id).await.unwrap();
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Completed);
    let r = tc
        .admin()
        .await
        .query_one("SELECT cursor_seq, stored, duplicates, skipped_non_mail FROM import_checkpoints WHERE pst_id = $1", &[&pst_id])
        .await
        .unwrap();
    let (cursor, stored, dups, non_mail): (i64, i64, i64, i64) = (r.get(0), r.get(1), r.get(2), r.get(3));
    assert_eq!(dups, before_resume, "cursor restarted at 0: every message stored before the touch was read again");
    assert_eq!((cursor, stored + dups, non_mail), (ITEMS, MESSAGES, 2));
    assert_eq!(count(&tc, "SELECT count(*) FROM messages").await, MESSAGES);
    assert_eq!(count(&tc, "SELECT count(*) FROM message_occurrences").await, MESSAGES);
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
    let res = mgr.enqueue(vec![s(&outlook_pst())]).await.unwrap();
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
        messages: count(&tc, "SELECT count(*) FROM messages").await,
        occurrences: count(&tc, "SELECT count(*) FROM message_occurrences").await,
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
    });
    assert!(matches!(rx.recv().unwrap(), ReaderMsg::Scanned(_)));
    let ReaderMsg::Item(item) = rx.recv().unwrap() else { panic!("item 1 expected") };
    assert_eq!(item.seq, 1);
    assert_eq!(marker.read(), Some(InflightRecord { attempts: 2, ..pending }));
    cancel.store(true, Ordering::SeqCst);
    budget.wake_all();
    reader.join().unwrap();
}

/// `TestCluster::kill` = `pg_ctl stop -m immediate` of this test's own cluster; `restart` starts it again (new port).
#[tokio::test(flavor = "multi_thread")]
async fn pg_crash_mid_import_resumes() {
    const ITEMS: i64 = 40;
    let want = reference(corpus(ITEMS)).await;
    let mut tc = TestCluster::start().await;
    let mut tuning = small_batches();
    tuning.hold_reader_after = Some(30);
    let mut batches = batch_signal(&mut tuning);
    let mut fx = manager(&tc, tuning);
    let pst_id = fx.mgr.enqueue_synthetic(&synthetic_path(&tc, "pg.pst"), folders(), corpus(ITEMS)).await.unwrap();
    fx.mgr.start_worker();
    wait_batch(&mut batches, 2).await;
    tc.kill().await;
    fx.phase.send_replace(AppPhase::StartingDb);
    tc.restart().await;
    fx.phase.send_replace(AppPhase::Ready);
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Completed);
    assert_eq!(snapshot(&tc).await, want);
    let dup = count(&tc, "SELECT count(*) FROM (SELECT 1 FROM message_occurrences GROUP BY pst_id, pst_item_id HAVING count(*) > 1) d").await;
    assert_eq!(dup, 0);
}

/// Part 02 §6.1: the restart budget runs out (phase `Failed`), the user retries (`app_retry` → `Ready` in the same
/// process). The paused job must resume from its checkpoint; `recover_after_crash` runs only once per process.
#[tokio::test(flavor = "multi_thread")]
async fn pg_crash_failed_then_retry_resumes() {
    const ITEMS: i64 = 40;
    let want = reference(corpus(ITEMS)).await;
    let mut tc = TestCluster::start().await;
    let mut tuning = small_batches();
    tuning.hold_reader_after = Some(30);
    let mut batches = batch_signal(&mut tuning);
    let mut fx = manager(&tc, tuning);
    let pst_id = fx.mgr.enqueue_synthetic(&synthetic_path(&tc, "failed.pst"), folders(), corpus(ITEMS)).await.unwrap();
    fx.mgr.start_worker();
    wait_batch(&mut batches, 2).await;
    tc.kill().await;
    fx.phase.send_replace(AppPhase::StartingDb);
    fx.phase.send_replace(AppPhase::Failed);
    tc.restart().await; // the job observes `Failed` while the cluster starts again
    fx.phase.send_replace(AppPhase::Ready);
    let end = tokio::time::timeout(Duration::from_secs(20), wait_job(&mut fx.events, pst_id)).await;
    assert_eq!(end.ok(), Some(ImportStatus::Completed), "the paused job resumes on the next Ready");
    assert_eq!(snapshot(&tc).await, want);
}

#[tokio::test(flavor = "multi_thread")]
async fn recover_after_crash_states() {
    const CURSOR: i64 = 10;
    const RESUMED_SUBJECTS: [&str; 9] =
        ["Mail m11", "Mail m12", "Mail m13", "Mail m14", "Mail m15", "Mail m16", "Mail m17", "Mail m18", "Mail m19"];
    let tc = TestCluster::start().await;
    let mut fx = manager(&tc, small_batches());
    let a = fx.mgr.enqueue_synthetic(&synthetic_path(&tc, "ra.pst"), folders(), corpus(20)).await.unwrap();
    let b = fx.mgr.enqueue_synthetic(&synthetic_path(&tc, "rb.pst"), folders(), corpus_tagged(20, "b")).await.unwrap();
    let adm = tc.admin().await;
    // states a crash leaves (one at a time: import_jobs_one_running allows one running / cancelling job)
    adm.execute("UPDATE import_jobs SET state = 'cancelling', started_at = now() WHERE pst_id = $1", &[&b]).await.unwrap();
    assert_eq!(fx.mgr.recover_after_crash().await.unwrap(), RecoveryReport { requeued: vec![], cancelled: vec![b] });
    // a: running with a checkpoint at seq 10
    adm.execute("UPDATE import_jobs SET state = 'running', started_at = now() WHERE pst_id = $1", &[&a]).await.unwrap();
    adm.execute(
        "INSERT INTO import_checkpoints (pst_id, job_id, phase, items_total, cursor_seq, pst_file_size, pst_file_mtime) \
         SELECT s.id, j.id, 'importing', 20, $2, s.file_size_bytes, s.file_mtime FROM pst_sources s JOIN import_jobs j ON j.pst_id = s.id WHERE s.id = $1",
        &[&a, &CURSOR],
    )
    .await
    .unwrap();
    assert_eq!(fx.mgr.recover_after_crash().await.unwrap(), RecoveryReport { requeued: vec![a], cancelled: vec![] });
    let states: Vec<(i64, String, bool)> = adm
        .query("SELECT pst_id, state, finished_at IS NOT NULL FROM import_jobs ORDER BY pst_id", &[])
        .await
        .unwrap()
        .iter()
        .map(|r| (r.get(0), r.get(1), r.get(2)))
        .collect();
    assert_eq!(states, [(a, "queued".to_string(), false), (b, "cancelled".to_string(), true)]);
    fx.mgr.start_worker();
    assert_eq!(wait_job(&mut fx.events, a).await, ImportStatus::Completed);
    let subjects: Vec<String> = adm.query("SELECT subject FROM messages ORDER BY subject", &[]).await.unwrap().iter().map(|r| r.get(0)).collect();
    assert_eq!(subjects, RESUMED_SUBJECTS, "only seq > cursor_seq was read");
}

// ------------------------------------------------------------------------------------------------ queue commands

#[tokio::test(flavor = "multi_thread")]
async fn pst_list_status_derivation() {
    // Part 04 §2.1: newest job (kind, state) → status; the last row has an older completed import
    const TABLE: [(&[(&str, &str)], ImportStatus); 10] = [
        (&[("import", "queued")], ImportStatus::Queued),
        (&[("import", "running")], ImportStatus::Running),
        (&[("import", "cancelling")], ImportStatus::Cancelling),
        (&[("import", "cancelled")], ImportStatus::Cancelled),
        (&[("import", "failed")], ImportStatus::Failed),
        (&[("import", "completed")], ImportStatus::Completed),
        (&[("remove", "queued")], ImportStatus::Removing),
        (&[("remove", "running")], ImportStatus::Removing),
        (&[("remove", "failed")], ImportStatus::Failed),
        (&[("import", "completed"), ("remove", "queued")], ImportStatus::Removing),
    ];
    let tc = TestCluster::start().await;
    let adm = tc.admin().await;
    let fx = manager(&tc, ImportTuning::default());
    // one case at a time: import_jobs_one_running allows one running / cancelling job in the library
    for (n, (jobs, status)) in TABLE.iter().enumerate() {
        let id = insert_source(&adm, n, jobs).await;
        let list = fx.mgr.list().await.unwrap();
        assert_eq!((list.len(), list[0].status), (1, *status), "case {n}: {jobs:?}");
        adm.execute("DELETE FROM pst_sources WHERE id = $1", &[&id]).await.unwrap();
    }
    // order: Running / Cancelling first, then Queued / Removing by job id, then the rest newest first
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
            "INSERT INTO pst_sources (path, display_name, file_size_bytes, file_mtime) VALUES ($1, 'p', 1, now()) RETURNING id",
            &[&format!("/status/{n}.pst")],
        )
        .await
        .unwrap()
        .get(0);
    for (kind, state) in jobs {
        adm.execute(
            "INSERT INTO import_jobs (pst_id, kind, state, started_at, finished_at) VALUES ($1, $2, $3, \
               CASE WHEN $3 = 'queued' THEN NULL ELSE now() END, \
               CASE WHEN $3 IN ('cancelled', 'failed', 'completed') THEN now() END)",
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
    // worker not started: the import job is still queued
    fx.mgr.remove(pst_id).await.unwrap(); // no 23505 from import_jobs_one_active_per_pst
    assert_eq!(fx.mgr.list().await.unwrap()[0].status, ImportStatus::Removing);
    assert!(matches!(fx.mgr.remove(pst_id).await, Err(AppError::Busy { .. })));
    fx.mgr.start_worker();
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Removing);
    assert_eq!(count(&tc, "SELECT count(*) FROM pst_sources").await, 0);
    assert_eq!(count(&tc, "SELECT count(*) FROM messages").await, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn enqueue_validation() {
    let tc = TestCluster::start().await;
    let fx = manager(&tc, ImportTuning::default());
    let dir = tempfile::tempdir().unwrap();
    let not_pst = dir.path().join("notes.pst");
    std::fs::write(&not_pst, b"hello, not a PST").unwrap();
    let missing = s(&dir.path().join("missing.pst"));
    let fa = |path: &str, reason: &str| AppError::FileAccess { path: path.into(), reason: reason.into() };
    let res = fx.mgr.enqueue(vec!["relative/x.pst".into(), missing.clone(), s(&not_pst), s(dir.path()), s(&outlook_pst())]).await.unwrap();
    let errors: Vec<Option<AppError>> =
        res.iter().map(|r| if let EnqueueResult::Rejected { error, .. } = r { Some(error.clone()) } else { None }).collect();
    assert_eq!(
        errors,
        [
            Some(fa("relative/x.pst", "notAbsolute")),
            Some(fa(&missing, "notFound")),
            Some(AppError::NotAPst { path: s(&not_pst) }),
            Some(fa(&s(dir.path()), "notAFile")),
            None,
        ]
    );
    let EnqueueResult::Queued { pst } = &res[4] else { panic!() };
    let again = fx.mgr.enqueue(vec![s(&outlook_pst())]).await.unwrap();
    assert!(matches!(&again[0], EnqueueResult::Rejected { error: AppError::AlreadyImported { pst_id }, .. } if *pst_id == pst.id), "{again:?}");
    assert_eq!(fx.mgr.enqueue(vec![]).await.unwrap_err(), AppError::invalid("paths", "empty"));
    assert_eq!(fx.mgr.enqueue(vec![missing; 101]).await.unwrap_err(), AppError::invalid("paths", "tooMany"));
}

// ------------------------------------------------------------------------------------------------ Outlook fixtures (W2)

#[cfg(feature = "outlook-fixtures")]
mod fixtures {
    use super::*;

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
        let res = fx.mgr.enqueue(paths).await.unwrap();
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
        assert_eq!(count(&tc, "SELECT count(*) FROM messages").await, 50);
        assert!(count(&tc, "SELECT count(*) FROM folders WHERE path = 'Inbox/Projects'").await == 1);
        assert!(count(&tc, "SELECT count(DISTINCT body_source) FROM messages WHERE body_source IN ('html', 'rtf_html', 'rtf_text')").await >= 2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fixtures_cjk() {
        let (tc, _fx, _) = import(&["cjk"]).await;
        assert_eq!(count(&tc, "SELECT count(*) FROM messages").await, 30);
        assert_eq!(count(&tc, "SELECT count(*) FROM messages WHERE body_text ~ '[一-鿿]'").await, 30);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fixtures_inline() {
        let (tc, _fx, _) = import(&["inline"]).await;
        assert_eq!(count(&tc, "SELECT count(*) FROM messages").await, 20);
        assert_eq!(
            count(&tc, "SELECT count(*) FROM (SELECT image_blob_id FROM inline_images GROUP BY 1 HAVING count(DISTINCT message_id) = 10) l").await,
            1,
            "the logo of 10 mails is one blob"
        );
        let dangling = count(
            &tc,
            "SELECT count(*) FROM messages m, regexp_matches(m.body_html, 'outlooker-inline:([0-9]+)', 'g') x \
              WHERE NOT EXISTS (SELECT 1 FROM inline_images i WHERE i.message_id = m.id AND i.ordinal = x[1]::int)",
        )
        .await;
        assert_eq!(dangling, 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fixtures_attach() {
        let (tc, _fx, _) = import(&["attach"]).await;
        assert!(count(&tc, "SELECT count(*) FROM attachments WHERE file_name LIKE '%.msg' AND mime_type = 'application/vnd.ms-outlook'").await >= 1);
        assert!(count(&tc, "SELECT count(*) FROM attachments WHERE length(file_name) > 255").await >= 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fixtures_dup() {
        let (tc, mut fx, ids) = import(&["dup-a", "dup-b"]).await;
        assert_eq!(count(&tc, "SELECT count(*) FROM messages").await, 50);
        assert_eq!(count(&tc, "SELECT count(*) FROM message_occurrences").await, 81);
        fx.mgr.remove(ids[0]).await.unwrap();
        assert_eq!(wait_job(&mut fx.events, ids[0]).await, ImportStatus::Removing);
        assert_eq!(count(&tc, "SELECT count(*) FROM messages").await, 40, "shared messages remain");
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
        assert!(count(&tc, "SELECT count(*) FROM import_errors WHERE severity = 'item_skipped'").await >= 1);
        assert_eq!(count(&tc, "SELECT count(*) FROM messages").await, 49);
    }
}
