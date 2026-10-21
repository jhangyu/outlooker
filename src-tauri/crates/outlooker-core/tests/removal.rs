#![cfg(feature = "test-support")]
//! PST removal tests (spec Part 04 §4.3, §12.2 row Removal).
use outlooker_core::importer::remove::{self, RemoveStats};
use outlooker_core::importer::types::{EncodedImage, InlineImageOut, NormalizedMail};
use outlooker_core::importer::webp::source_hash;
use outlooker_core::test_support::import_fixtures::{item, mail, seed_mails};
use outlooker_core::test_support::TestCluster;

async fn count(c: &tokio_postgres::Client, sql: &str) -> i64 {
    c.query_one(sql, &[]).await.unwrap().get(0)
}

fn with_image(mut m: NormalizedMail, bytes: &[u8]) -> NormalizedMail {
    m.body_source = outlooker_core::importer::types::BodySource::Html;
    m.body_html = Some(r#"<p>x<img src="outlooker-inline:0"></p>"#.into());
    m.inline_images.push(InlineImageOut {
        ordinal: 0,
        content_id: Some("logo".into()),
        source_hash: source_hash(bytes),
        blob: Some(EncodedImage { webp: bytes.to_vec(), width: 1, height: 1, source_mime: "image/png".into() }),
    });
    m
}

/// Source A: a mail shared with B (same Message-ID) + a mail only in A (own sender address and own image).
async fn two_sources(tc: &TestCluster) -> (i64, i64) {
    let a = seed_mails(
        &tc.db,
        "/p/a.pst",
        vec![
            item(1, 11, mail("shared", "s", "shared@x.example", Some("<shared@x>"))),
            item(2, 12, with_image(mail("only a", "o", "only-a@x.example", Some("<only-a@x>")), b"image-a")),
        ],
    )
    .await;
    let b = seed_mails(&tc.db, "/p/b.pst", vec![item(1, 21, mail("shared", "s", "shared@x.example", Some("<shared@x>")))]).await;
    (a, b)
}

#[tokio::test]
async fn keeps_shared_deletes_rest() {
    let tc = TestCluster::start().await;
    let (a, b) = two_sources(&tc).await;
    let admin = tc.admin().await;
    assert_eq!(count(&admin, "SELECT count(*) FROM messages").await, 2);
    assert_eq!(count(&admin, "SELECT count(*) FROM image_blobs").await, 1);
    let stats = remove::run(&tc.app().await, a).await.unwrap();
    assert_eq!(stats, RemoveStats { occurrences_deleted: 2, messages_deleted: 1 });
    let left: Vec<(String, Option<i64>)> = admin
        .query("SELECT subject, first_pst_id FROM messages", &[])
        .await
        .unwrap()
        .iter()
        .map(|r| (r.get(0), r.get(1)))
        .collect();
    assert_eq!(left, vec![("shared".to_owned(), None)]); // first_pst_id was A → SET NULL
    assert_eq!(count(&admin, "SELECT count(*) FROM image_blobs").await, 0); // orphan blob deleted
    let emails: Vec<String> = admin.query("SELECT email FROM addresses ORDER BY email", &[]).await.unwrap().iter().map(|r| r.get(0)).collect();
    assert_eq!(emails, vec!["shared@x.example".to_owned()]); // orphan address deleted
    assert_eq!(count(&admin, &format!("SELECT count(*) FROM pst_sources WHERE id = {a}")).await, 0);
    assert_eq!(count(&admin, &format!("SELECT count(*) FROM message_occurrences WHERE pst_id = {b}")).await, 1);
}

#[tokio::test]
async fn second_run_is_noop() {
    let tc = TestCluster::start().await;
    let (a, b) = two_sources(&tc).await;
    let app = tc.app().await;
    remove::run(&app, a).await.unwrap();
    assert_eq!(remove::run(&app, a).await.unwrap(), RemoveStats::default());
    let admin = tc.admin().await;
    assert_eq!(count(&admin, "SELECT count(*) FROM messages").await, 1);
    assert_eq!(count(&admin, "SELECT count(*) FROM addresses").await, 1);
    remove::run(&app, b).await.unwrap();
    assert_eq!(count(&admin, "SELECT count(*) FROM messages").await, 0);
    assert_eq!(count(&admin, "SELECT count(*) FROM addresses").await, 0);
}
