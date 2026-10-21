//! Non-ignored load generator checks: 10,000-message smoke load and the E2E seed root.
use outlooker_core::test_support::TestCluster;
use outlooker_loadgen::generate::{GenConfig, E2E_SUBJECT};
use outlooker_loadgen::load::{load, seed};
use outlooker_loadgen::root::LibraryRoot;

#[tokio::test(flavor = "multi_thread")]
async fn smoke_10k() {
    let tc = TestCluster::start().await;
    let cfg = GenConfig::smoke(10_000);
    let r = load(&tc.db, &cfg).await.unwrap();
    println!("messages_stored={} occurrences={}", r.messages_stored, r.occurrences);
    assert_eq!(r.messages_submitted, 10_000);
    assert_eq!(r.occurrences, 10_000); // one occurrence per item: pst_item_id is unique per source
    // every planned duplicate is deduplicated, nothing else is
    assert_eq!(r.messages_stored as u64, r.messages_submitted - r.duplicates_planned);
    // duplicate share within the configured rates ± 1 percentage point (source 0 holds no duplicates)
    let expected = (cfg.msgid_dup_rate + cfg.hash_dup_rate) * (cfg.sources - 1) as f64 / cfg.sources as f64;
    let measured = 1.0 - r.messages_stored as f64 / r.messages_submitted as f64;
    assert!((measured - expected).abs() <= 0.01, "duplicate share {measured:.4}, expected {expected:.4}");
    let admin = tc.admin().await;
    let missing = outlooker_core::db::indexes::missing_or_invalid(&admin).await.unwrap();
    assert!(missing.is_empty(), "{missing:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn seeded_root_starts() {
    let dir = tempfile::tempdir().unwrap();
    let root_dir = dir.path().join("library");
    seed(&root_dir, 2_000).await.unwrap();
    let root = LibraryRoot::open(&root_dir).await.unwrap();
    let a = root.db.connect_admin().await.unwrap();
    let n: i64 = a.query_one("SELECT count(*) FROM messages", &[]).await.unwrap().get(0);
    assert!(n > 1_900, "{n}");
    let inline: i64 = a
        .query_one("SELECT count(*) FROM inline_images i JOIN messages m ON m.id = i.message_id WHERE m.subject = $1", &[&E2E_SUBJECT])
        .await
        .unwrap()
        .get(0);
    assert_eq!(inline, 1);
    let bad: i64 = a
        .query_one("SELECT count(*) FROM addresses a JOIN messages m ON m.sender_address_id = a.id WHERE a.email NOT LIKE '%@loadgen.example'", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(bad, 0);
    assert!(root.paths.marker().exists() && root.paths.credentials().exists());
    root.stop().await.unwrap();
}
