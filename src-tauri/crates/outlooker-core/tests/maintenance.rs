//! Maintenance Service integration tests (spec Part 03 §8.3, Part 04 §9.1).
#![cfg(feature = "test-support")]
use std::sync::Arc;

use outlooker_core::db::indexes::{drop_search_indexes, missing_or_invalid};
use outlooker_core::dto::error::{AppError, BusyReason};
use outlooker_core::dto::events::MaintenanceRunState;
use outlooker_core::dto::settings::MaintenanceTask;
use outlooker_core::gate::{Activity, Gate};
use outlooker_core::maintenance::{Maintenance, REINDEX_LIST_SQL};
use outlooker_core::sink::{CoreEvent, RecordingSink};
use outlooker_core::test_support::TestCluster;

fn service(tc: &TestCluster, gate: &Gate) -> (Maintenance, Arc<RecordingSink>) {
    let sink = Arc::new(RecordingSink::default());
    (Maintenance::new(tc.db.clone(), gate.clone(), sink.clone()), sink)
}

fn states(sink: &RecordingSink) -> Vec<(MaintenanceTask, MaintenanceRunState)> {
    sink.events()
        .into_iter()
        .map(|e| match e {
            CoreEvent::MaintenanceStatusChanged(s) => (s.task, s.state),
            other => panic!("unexpected event {other:?}"),
        })
        .collect()
}

#[tokio::test]
async fn maintenance_refused_while_import_holds_gate() {
    let tc = TestCluster::start().await;
    let gate = Gate::new();
    let (m, sink) = service(&tc, &gate);
    let held = gate.try_enter(Activity::Import).unwrap();
    assert_eq!(m.run(MaintenanceTask::Analyze).await, Err(AppError::Busy { reason: BusyReason::ImportRunning }));
    assert!(sink.events().is_empty());
    assert_eq!(m.current(), None);
    drop(held);
    m.run(MaintenanceTask::Analyze).await.unwrap();
    // a second task while the first runs is refused by the gate
    assert_eq!(m.run(MaintenanceTask::Analyze).await, Err(AppError::Busy { reason: BusyReason::MaintenanceRunning }));
    m.wait_idle().await;
    assert_eq!(gate.current(), None);
    assert_eq!(states(&sink), [(MaintenanceTask::Analyze, MaintenanceRunState::Running), (MaintenanceTask::Analyze, MaintenanceRunState::Done)]);
}

#[tokio::test]
async fn vacuum_removes_orphans() {
    let tc = TestCluster::start().await;
    let a = tc.admin().await;
    a.batch_execute(
        "INSERT INTO addresses (email, address_type) VALUES ('kept@x.example', 'smtp'), ('orphan@x.example', 'smtp');
         INSERT INTO messages (dedupe_key, dedupe_kind, body_source, sender_address_id)
           SELECT decode(repeat('0a', 32), 'hex'), 'content_hash', 'none', id FROM addresses WHERE email = 'kept@x.example';
         INSERT INTO image_blobs (source_hash, webp, width, height, source_mime)
           VALUES (decode(repeat('0b', 32), 'hex'), '\\x00', 1, 1, 'image/png'), (decode(repeat('0c', 32), 'hex'), '\\x00', 1, 1, 'image/png');
         INSERT INTO inline_images (message_id, ordinal, image_blob_id)
           SELECT m.id, 0, b.id FROM messages m, image_blobs b WHERE b.source_hash = decode(repeat('0b', 32), 'hex');",
    )
    .await
    .unwrap();
    let gate = Gate::new();
    let (m, sink) = service(&tc, &gate);
    m.run(MaintenanceTask::Vacuum).await.unwrap();
    let running = m.current().expect("current is Some while the task runs");
    assert_eq!((running.task, running.state), (MaintenanceTask::Vacuum, MaintenanceRunState::Running));
    m.wait_idle().await;
    let emails: Vec<String> = a.query("SELECT email FROM addresses", &[]).await.unwrap().iter().map(|r| r.get(0)).collect();
    assert_eq!(emails, ["kept@x.example"]);
    let blobs: i64 = a.query_one("SELECT count(*) FROM image_blobs", &[]).await.unwrap().get(0);
    assert_eq!(blobs, 1);
    assert_eq!(states(&sink), [(MaintenanceTask::Vacuum, MaintenanceRunState::Running), (MaintenanceTask::Vacuum, MaintenanceRunState::Done)]);
    let CoreEvent::MaintenanceStatusChanged(done) = sink.events().pop().unwrap() else { unreachable!() };
    assert!(done.finished_at.is_some() && done.error.is_none());
    assert_eq!(done.started_at, running.started_at);
    assert_eq!(m.current(), None);
}

#[tokio::test]
async fn reindex_leaves_all_valid() {
    let tc = TestCluster::start().await;
    let a = tc.admin().await;
    let listed: Vec<String> = a.query(REINDEX_LIST_SQL, &[]).await.unwrap().iter().map(|r| r.get(0)).collect();
    for want in ["messages_body_fts", "import_jobs_queue"] {
        assert!(listed.iter().any(|n| n == want), "{want} in {listed:?}");
    }
    assert!(!listed.iter().any(|n| n.ends_with("_pkey")), "constraint indexes excluded: {listed:?}");
    // damage: two search indexes missing, one invalid leftover of a failed concurrent rebuild
    a.batch_execute(
        "DROP INDEX messages_body_fts; DROP INDEX messages_sort;
         CREATE INDEX messages_subject_trgm_ccnew ON messages (subject);
         UPDATE pg_index SET indisvalid = false WHERE indexrelid = 'messages_subject_trgm_ccnew'::regclass;",
    )
    .await
    .unwrap();
    let gate = Gate::new();
    let (m, sink) = service(&tc, &gate);
    m.run(MaintenanceTask::Reindex).await.unwrap();
    m.wait_idle().await;
    assert_eq!(states(&sink).last(), Some(&(MaintenanceTask::Reindex, MaintenanceRunState::Done)), "{:?}", sink.events());
    let invalid: i64 = a
        .query_one(
            "SELECT count(*) FROM pg_index x JOIN pg_class c ON c.oid = x.indexrelid JOIN pg_namespace n ON n.oid = c.relnamespace
             WHERE n.nspname = 'public' AND NOT x.indisvalid",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(invalid, 0);
    let leftover: i64 = a.query_one("SELECT count(*) FROM pg_class WHERE relname LIKE '%ccnew%'", &[]).await.unwrap().get(0);
    assert_eq!(leftover, 0);
    assert!(missing_or_invalid(&a).await.unwrap().is_empty(), "all six search indexes exist and are valid");
    // dropped search indexes alone are also repaired
    drop_search_indexes(&a).await.unwrap();
    m.run(MaintenanceTask::Reindex).await.unwrap();
    m.wait_idle().await;
    assert!(missing_or_invalid(&a).await.unwrap().is_empty());
}
