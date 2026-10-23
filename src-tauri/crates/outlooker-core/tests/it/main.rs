#![cfg(feature = "test-support")]
mod app_compose; mod db_schema; mod import_pipeline; mod lifecycle; mod maintenance;
mod pg_lifecycle; mod removal; mod search; mod sql_console; mod writer;

#[test]
#[ignore]
fn warm_template() {
    outlooker_core::test_support::warm_template()
}
