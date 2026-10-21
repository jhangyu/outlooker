//! Bulk writer tests (spec Part 04 §4.2, §6.1–§6.3) on a real PostgreSQL test cluster.
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use outlooker_core::dto::error::AppError;
use outlooker_core::importer::types::{AttachmentOut, NormalizedItem};
use outlooker_core::importer::webp::KnownHashes;
use outlooker_core::importer::writer::{prepare_session, Checkpoint, Writer, WriterConfig, WriterHandle, WriterLimits};
use outlooker_core::test_support::import_fixtures::{complete_job, item, mail, new_source, seed_mails};
use outlooker_core::test_support::TestCluster;
use tokio::sync::mpsc;

async fn count(c: &tokio_postgres::Client, sql: &str) -> i64 {
    c.query_one(sql, &[]).await.unwrap().get(0)
}

/// New source + checkpoint + writer with `limits`; commits are forwarded on the returned channel.
async fn start_writer(tc: &TestCluster, path: &str, limits: WriterLimits) -> (WriterHandle, mpsc::UnboundedReceiver<Checkpoint>, i64, i64) {
    let app = tc.app().await;
    let (pst_id, job_id) = new_source(&app, path).await;
    let (start, _) = Checkpoint::begin(&app, pst_id, job_id, 1, SystemTime::now()).await.unwrap();
    let cfg = WriterConfig { pst_id, data_root: tc.db.data_root().unwrap(), start, limits, known: Arc::new(KnownHashes::new()) };
    let (tx, rx) = mpsc::unbounded_channel();
    let w = Writer::spawn(tc.app().await, cfg, Box::new(move |cp: &Checkpoint| { let _ = tx.send(cp.clone()); })).await.unwrap();
    (w, rx, pst_id, job_id)
}

fn mails(n: i64, body: &str) -> Vec<NormalizedItem> {
    (1..=n).map(|i| item(i, 100 + i, mail(&format!("subject {i}"), body, "a@x.example", Some(&format!("<m{i}@x>"))))).collect()
}

#[tokio::test]
async fn out_of_order_checkpoint_is_contiguous() {
    let tc = TestCluster::start().await;
    let (w, _rx, pst_id, _) = start_writer(&tc, "/p/a.pst", WriterLimits::default()).await;
    let mut items: Vec<Option<NormalizedItem>> = mails(5, "body").into_iter().map(Some).collect();
    for i in [2, 0, 4, 1] {
        w.submit(items[i].take().unwrap()).await.unwrap(); // seq 3, 1, 5, 2; seq 4 never arrives (a gap)
    }
    let cp = w.finish().await.unwrap();
    assert_eq!(cp.cursor_seq, 3);
    assert_eq!(cp.stored, 3);
    let a = tc.admin().await;
    assert_eq!(count(&a, "SELECT count(*) FROM messages").await, 3);
    assert_eq!(Checkpoint::load(&a, pst_id).await.unwrap().unwrap().cursor_seq, 3);
}

#[tokio::test]
async fn message_id_duplicate_adds_occurrence() {
    let tc = TestCluster::start().await;
    seed_mails(&tc.db, "/p/a.pst", vec![item(1, 11, mail("hello", "one", "a@x.example", Some("<id-1@x>")))]).await;
    let (w, _rx, _, _) = start_writer(&tc, "/p/b.pst", WriterLimits::default()).await;
    w.submit(item(1, 21, mail("hello again", "other body", "b@x.example", Some(" id-1@x ")))).await.unwrap();
    let cp = w.finish().await.unwrap();
    assert_eq!((cp.stored, cp.duplicates), (0, 1));
    let a = tc.admin().await;
    assert_eq!(count(&a, "SELECT count(*) FROM messages").await, 1);
    assert_eq!(count(&a, "SELECT count(*) FROM message_occurrences").await, 2);
}

#[tokio::test]
async fn content_hash_crlf_vs_lf() {
    let tc = TestCluster::start().await;
    seed_mails(&tc.db, "/p/a.pst", vec![item(1, 11, mail("Quote", "line one\nline two\n", "a@x.example", None))]).await;
    seed_mails(&tc.db, "/p/b.pst", vec![item(1, 11, mail("Quote", "line one\r\nline two\r\n", "a@x.example", None))]).await;
    let a = tc.admin().await;
    assert_eq!(count(&a, "SELECT count(*) FROM messages").await, 1);
    assert_eq!(count(&a, "SELECT count(*) FROM message_occurrences").await, 2);
    assert_eq!(count(&a, "SELECT count(*) FROM messages WHERE dedupe_kind = 'content_hash'").await, 1);
}

#[tokio::test]
async fn batch_cut_by_items() {
    assert_eq!(WriterLimits::default().max_items, 1_000);
    let tc = TestCluster::start().await;
    let limits = WriterLimits { max_items: 3, ..WriterLimits::default() };
    let (w, _rx, _, _) = start_writer(&tc, "/p/a.pst", limits).await;
    for it in mails(7, "body") {
        w.submit(it).await.unwrap();
    }
    let cp = w.finish().await.unwrap();
    assert_eq!((cp.batches_committed, cp.cursor_seq, cp.stored), (3, 7, 7)); // 3 + 3 + 1
}

#[tokio::test]
async fn batch_cut_by_bytes() {
    assert_eq!(WriterLimits::default().max_bytes, 64 * 1024 * 1024);
    let tc = TestCluster::start().await;
    let limits = WriterLimits { max_bytes: 10 * 1024, ..WriterLimits::default() };
    let (w, _rx, _, _) = start_writer(&tc, "/p/a.pst", limits).await;
    for it in mails(4, &"x".repeat(6 * 1024)) {
        w.submit(it).await.unwrap();
    }
    let cp = w.finish().await.unwrap();
    assert_eq!((cp.batches_committed, cp.stored), (2, 4)); // two 6 KB items reach 10 KB
}

#[tokio::test]
async fn batch_cut_by_time() {
    assert_eq!(WriterLimits::default().max_wait, Duration::from_secs(1));
    let tc = TestCluster::start().await;
    let limits = WriterLimits { max_wait: Duration::from_millis(200), ..WriterLimits::default() };
    let (w, mut rx, _, _) = start_writer(&tc, "/p/a.pst", limits).await;
    let mut items = mails(2, "body").into_iter();
    w.submit(items.next().unwrap()).await.unwrap();
    // The commit arrives through on_commit while the writer still waits for input: only the deadline can cut it.
    let cp = tokio::time::timeout(Duration::from_secs(10), rx.recv()).await.expect("time cut").unwrap();
    assert_eq!((cp.batches_committed, cp.cursor_seq), (1, 1));
    w.submit(items.next().unwrap()).await.unwrap();
    let cp = w.finish().await.unwrap();
    assert_eq!((cp.batches_committed, cp.cursor_seq), (2, 2));
}

#[tokio::test]
async fn synchronous_commit_off_only_on_writer() {
    let tc = TestCluster::start().await;
    let writer_conn = tc.db.connect_app().await.unwrap();
    prepare_session(&writer_conn).await.unwrap(); // the session setup Writer::spawn runs
    assert_eq!(show_sync(&writer_conn).await, "off");
    seed_mails(&tc.db, "/p/a.pst", mails(2, "body")).await; // a real writer ran
    let pooled = tc.db.app_pool().unwrap().get().await.unwrap();
    assert_eq!(show_sync(&pooled).await, "on");
}

async fn show_sync(c: &tokio_postgres::Client) -> String {
    c.query_one("SHOW synchronous_commit", &[]).await.unwrap().get(0)
}

#[tokio::test]
async fn error_rolls_back_data_and_checkpoint() {
    let tc = TestCluster::start().await;
    let (w, _rx, pst_id, _) = start_writer(&tc, "/p/a.pst", WriterLimits::default()).await;
    let mut items = mails(3, "body");
    let bad = attachment("");
    if let outlooker_core::importer::types::Outcome::Mail(m) = &mut items[2].outcome {
        m.attachments.push(bad); // file_name '' violates the attachments CHECK (length 1..=1024)
    }
    for it in items {
        w.submit(it).await.unwrap();
    }
    let err = w.finish().await.unwrap_err();
    assert!(matches!(&err, AppError::Internal { detail } if detail.starts_with("23514")), "{err:?}");
    let a = tc.admin().await;
    assert_eq!(count(&a, "SELECT count(*) FROM messages").await, 0);
    assert_eq!(count(&a, "SELECT count(*) FROM message_occurrences").await, 0);
    let cp = Checkpoint::load(&a, pst_id).await.unwrap().unwrap();
    assert_eq!((cp.cursor_seq, cp.batches_committed), (0, 0));
}

#[tokio::test]
async fn disk_guard_blocks_commit() {
    assert_eq!(WriterLimits::default().min_free_bytes, 2 * 1024 * 1024 * 1024);
    let tc = TestCluster::start().await;
    let limits = WriterLimits { min_free_bytes: u64::MAX, ..WriterLimits::default() };
    let (w, _rx, pst_id, job_id) = start_writer(&tc, "/p/a.pst", limits).await;
    for it in mails(2, "body") {
        w.submit(it).await.unwrap();
    }
    let err = w.finish().await.unwrap_err();
    assert_eq!(err, AppError::FileAccess { path: tc.db.data_root().unwrap().display().to_string(), reason: "insufficientSpace".into() });
    let a = tc.admin().await;
    assert_eq!(count(&a, "SELECT count(*) FROM messages").await, 0);
    assert_eq!(Checkpoint::load(&a, pst_id).await.unwrap().unwrap().cursor_seq, 0);
    complete_job(&a, job_id).await;
}

#[tokio::test]
async fn same_key_twice_in_one_batch() {
    let tc = TestCluster::start().await;
    let (w, _rx, _, _) = start_writer(&tc, "/p/a.pst", WriterLimits::default()).await;
    for (seq, subject) in [(1, "first copy"), (2, "second copy")] {
        let mut m = mail(subject, "body", "a@x.example", Some("<same@x>"));
        outlooker_core::test_support::import_fixtures::add_recipient(&mut m, outlooker_core::dto::search::RecipientKind::To, None, "r@x.example");
        m.attachments.push(attachment("a.txt"));
        w.submit(item(seq, 100 + seq, m)).await.unwrap();
    }
    let cp = w.finish().await.unwrap();
    assert_eq!((cp.batches_committed, cp.stored, cp.duplicates), (1, 1, 1));
    let a = tc.admin().await;
    assert_eq!(a.query_one("SELECT subject FROM messages", &[]).await.unwrap().get::<_, String>(0), "first copy");
    assert_eq!(count(&a, "SELECT count(*) FROM message_recipients").await, 1);
    assert_eq!(count(&a, "SELECT count(*) FROM attachments").await, 1);
    assert_eq!(count(&a, "SELECT count(*) FROM message_occurrences").await, 2);
}

fn attachment(file_name: &str) -> AttachmentOut {
    AttachmentOut { attachment_index: 0, file_name: file_name.into(), size_bytes: 1, mime_type: "text/plain".into(), attach_method: 1 }
}
