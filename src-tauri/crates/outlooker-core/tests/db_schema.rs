#![cfg(feature = "test-support")]
use outlooker_core::db::{indexes, Credentials, Db};
use outlooker_core::dto::error::AppError;
use outlooker_core::dto::settings::SettingsPatch;
use outlooker_core::settings::SettingsStore;
use outlooker_core::test_support::TestCluster;

fn code(e: &tokio_postgres::Error) -> String {
    e.as_db_error().map(|d| d.code().code().to_owned()).unwrap_or_default()
}

#[tokio::test]
async fn migrations_apply_and_trgm_cjk() {
    let tc = TestCluster::start().await;
    let a = tc.admin().await;
    let n: i64 = a.query_one("SELECT count(*) FROM pg_tables WHERE schemaname = 'public' AND tablename <> 'refinery_schema_history'", &[]).await.unwrap().get(0);
    assert_eq!(n, 14);
    // show_trgm prints multibyte trigrams as hex hashes, so assert on the trigram set (5 for 4 CJK chars) and overlap.
    let n_trgm: i32 = a.query_one("SELECT cardinality(show_trgm('郵件系統'))", &[]).await.unwrap().get(0);
    assert_eq!(n_trgm, 5);
    let shared: bool = a.query_one("SELECT show_trgm('郵件系統') && show_trgm('郵件系')", &[]).await.unwrap().get(0);
    assert!(shared);
    outlooker_core::db::cjk_self_check(&a).await.unwrap();
}

#[tokio::test]
async fn constraints_reject_bad_rows() {
    let tc = TestCluster::start().await;
    let a = tc.admin().await;
    a.batch_execute(
        "INSERT INTO pst_sources (path, display_name, file_size_bytes, file_mtime) VALUES ('/a.pst','a',1,now()), ('/b.pst','b',1,now());
         INSERT INTO folders (pst_id, pst_folder_id, name, path) VALUES (1, 1, 'Inbox', 'Inbox'), (2, 1, 'Inbox', 'Inbox');
         INSERT INTO import_jobs (pst_id, kind, state, started_at) VALUES (1, 'import', 'running', now());
         INSERT INTO messages (dedupe_key, dedupe_kind, body_source, body_text) VALUES (decode(repeat('ab', 32), 'hex'), 'content_hash', 'text', 'x');",
    )
    .await
    .unwrap();
    let cases: [(&str, &str); 7] = [
        ("INSERT INTO import_jobs (pst_id, kind, state, started_at) VALUES (2, 'import', 'running', now())", "23505"),
        ("INSERT INTO message_occurrences (pst_id, pst_item_id, message_id, folder_id) VALUES (1, 9, 1, 2)", "23503"),
        ("INSERT INTO messages (dedupe_key, dedupe_kind, body_source, snippet) VALUES (decode(repeat('cd', 32), 'hex'), 'content_hash', 'none', repeat('x', 201))", "23514"),
        ("INSERT INTO addresses (email, address_type) VALUES ('Upper@Example.com', 'smtp')", "23514"),
        ("INSERT INTO messages (dedupe_key, dedupe_kind, body_source) VALUES (decode(repeat('ef', 32), 'hex'), 'content_hash', 'html')", "23514"),
        ("INSERT INTO messages (dedupe_key, dedupe_kind, body_source) VALUES (decode(repeat('ab', 31), 'hex'), 'content_hash', 'none')", "23514"),
        ("INSERT INTO settings DEFAULT VALUES", "23505"),
    ];
    for (sql, want) in cases {
        let err = a.batch_execute(sql).await.expect_err(sql);
        assert_eq!(code(&err), want, "{sql}");
    }
}

#[tokio::test]
async fn ro_role_sees_only_views() {
    let tc = TestCluster::start().await;
    let ro = tc.ro().await;
    for v in ["v_mail", "v_recipients", "v_occurrences", "v_attachments", "v_folders", "v_pst_sources"] {
        ro.query_one(&format!("SELECT count(*) FROM {v}"), &[]).await.unwrap_or_else(|e| panic!("{v}: {e}"));
    }
    ro.query_one("SELECT count(*) FROM v_mail WHERE body_tsv @@ websearch_to_tsquery('english', 'invoice')", &[]).await.unwrap();
    let err = ro.query_one("SELECT * FROM messages", &[]).await.unwrap_err();
    assert_eq!(code(&err), "42501");
    // read-only default would answer 25006 first; lift it to prove the REVOKE TEMP itself.
    ro.batch_execute("SET default_transaction_read_only = off").await.unwrap();
    let err = ro.batch_execute("CREATE TEMP TABLE t (x int)").await.unwrap_err();
    assert_eq!(code(&err), "42501");
}

#[tokio::test]
async fn app_role_privileges() {
    let tc = TestCluster::start().await;
    let app = tc.app().await;
    app.batch_execute("CREATE TEMP TABLE staging (x int)").await.unwrap();
    app.batch_execute("VACUUM (ANALYZE) messages").await.unwrap();
    let err = app.batch_execute("CREATE INDEX nope ON messages (subject)").await.unwrap_err();
    assert_eq!(code(&err), "42501");
}

#[tokio::test]
async fn admin_owns_tables() {
    let tc = TestCluster::start().await;
    let a = tc.admin().await;
    let others: i64 = a
        .query_one("SELECT count(*) FROM pg_tables WHERE schemaname = 'public' AND tableowner <> 'outlooker_admin'", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(others, 0);
}

#[tokio::test]
async fn search_indexes_drop_and_ensure() {
    let tc = TestCluster::start().await;
    let a = tc.admin().await;
    assert!(indexes::missing_or_invalid(&a).await.unwrap().is_empty());
    indexes::drop_search_indexes(&a).await.unwrap();
    assert_eq!(indexes::missing_or_invalid(&a).await.unwrap().len(), 6);
    let calls = std::sync::Mutex::new(Vec::new());
    indexes::ensure_search_indexes(&a, &|name: &str, done: usize, total: usize| calls.lock().unwrap().push((name.to_owned(), done, total))).await.unwrap();
    assert!(indexes::missing_or_invalid(&a).await.unwrap().is_empty());
    assert_eq!(calls.lock().unwrap().last().unwrap().1, 6);
    indexes::ensure_search_indexes(&a, &|_: &str, _: usize, _: usize| {}).await.unwrap(); // idempotent
}

#[tokio::test]
async fn db_handle_not_ready_when_unset() {
    let db = Db::new();
    assert_eq!(db.app_pool().err(), Some(AppError::DbNotReady));
    assert!(matches!(db.connect_admin().await, Err(AppError::DbNotReady)));
    let tc = TestCluster::start().await;
    let client = tc.db.app_pool().unwrap().get().await.unwrap();
    let one: i32 = client.query_one("SELECT 1", &[]).await.unwrap().get(0);
    assert_eq!(one, 1);
}

#[tokio::test]
async fn settings_get_uses_cached_quality_after_pool_closed() {
    let tc = TestCluster::start().await;
    let store = SettingsStore::load(tc.dir.path().join("settings.json")).unwrap();
    store.update(&tc.db, SettingsPatch { language: None, webp_quality: Some(55) }).await.unwrap();
    assert_eq!(store.get(&tc.db).await.unwrap().webp_quality, 55);
    tc.db.app_pool().unwrap().close();
    assert_eq!(tc.db.app_client().await.err(), Some(AppError::DbNotReady));
    assert_eq!(store.get(&tc.db).await.unwrap().webp_quality, 55, "closed pool → cached quality, not Internal");
}

#[test]
fn credentials_round_trip_and_mode() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("auth").join("credentials");
    let c = Credentials::generate();
    assert_ne!(c.app, c.ro);
    c.save(&path).unwrap();
    assert_eq!(Credentials::load(&path).unwrap(), c);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    }
    std::fs::write(&path, b"{\"admin\":\"x\",\"app\":\"y\",\"ro\":\"z\"}").unwrap();
    assert!(Credentials::load(&path).is_err());
}
