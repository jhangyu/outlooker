//! Outlooker core services. No Tauri dependency (spec Part 02 §3).
pub mod app;
pub mod db;
pub mod dto;
pub mod fsutil;
pub mod gate;
pub mod importer;
pub mod lifecycle;
pub mod maintenance;
pub mod paths;
pub mod search;
pub mod settings;
pub mod sink;
pub mod sql_console;
#[cfg(feature = "test-support")]
pub mod test_support;
