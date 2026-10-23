use outlooker_core::db::{connect, ADMIN_ROLE, CJK_SELF_CHECK};
use outlooker_core::test_support::TestCluster;

/// pg_trgm classifies characters with the database's LC_CTYPE, so a C-locale database fails the CJK self-check
/// (the production init rejects such a candidate locale). Runs on the shared cluster as its own database.
#[tokio::test]
async fn c_locale_is_rejected() {
    let tc = TestCluster::start().await;
    tc.admin().await.batch_execute("CREATE DATABASE c_loc TEMPLATE template0 LOCALE 'C'").await.unwrap();
    let c = connect(tc.port, ADMIN_ROLE, &tc.creds.admin, "c_loc").await.unwrap();
    c.batch_execute("CREATE EXTENSION pg_trgm").await.unwrap();
    let ok: bool = c.query_one(CJK_SELF_CHECK, &[]).await.unwrap().get(0);
    assert!(!ok, "a C-locale database must fail the CJK self-check");
}
