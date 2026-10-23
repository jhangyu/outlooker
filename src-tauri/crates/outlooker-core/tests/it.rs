#![cfg(feature = "test-support")]
// All shared-cluster DB tests in one binary: the cargo test runner boots the cluster once for it (CLAUDE.md "Test policy").
// `mod x;` in a crate root resolves beside the root file, hence the explicit paths into tests/it/.
#[path = "it/app_compose.rs"]
mod app_compose;
#[path = "it/db_schema.rs"]
mod db_schema;
#[path = "it/import_pipeline.rs"]
mod import_pipeline;
#[path = "it/lifecycle.rs"]
mod lifecycle;
#[path = "it/maintenance.rs"]
mod maintenance;
#[path = "it/pg_lifecycle.rs"]
mod pg_lifecycle;
#[path = "it/removal.rs"]
mod removal;
#[path = "it/search.rs"]
mod search;
#[path = "it/sql_console.rs"]
mod sql_console;
#[path = "it/writer.rs"]
mod writer;

#[test]
#[ignore]
fn warm_template() {
    outlooker_core::test_support::warm_template()
}
