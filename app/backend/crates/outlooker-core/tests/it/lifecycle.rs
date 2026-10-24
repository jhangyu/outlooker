#![cfg(feature = "test-support")]

use std::sync::Arc;
use std::time::Duration;

use outlooker_core::db::Db;
use outlooker_core::dto::app::{AppPhase, InitializeRequest, Language};
use outlooker_core::dto::error::{AppError, BusyReason};
use outlooker_core::dto::settings::SettingsPatch;
use outlooker_core::gate::{Activity, Gate};
use outlooker_core::lifecycle::{Supervisor, SupervisorConfig};
use outlooker_core::paths::{dev_pg_dir, Paths};
use outlooker_core::settings::SettingsStore;
use outlooker_core::sink::NullSink;
use outlooker_core::test_support::TestCluster;

struct Rig {
    _dir: tempfile::TempDir,
    root: std::path::PathBuf,
    sup: Supervisor,
    db: Db,
    gate: Gate,
    settings: SettingsStore,
}

fn rig_at(dir: tempfile::TempDir, app: &str, pg_dir: std::path::PathBuf) -> Rig {
    assert!(dev_pg_dir().join("bin").exists(), "PostgreSQL binaries missing: run node app/scripts/fetch-postgres.mjs");
    let paths = Paths::new(dir.path().join(app), dir.path().join("logs"), pg_dir);
    let settings = SettingsStore::load(paths.bootstrap_file()).unwrap();
    let (db, gate) = (Db::new(), Gate::new());
    let config = SupervisorConfig { probe_interval: Duration::from_millis(100), ..SupervisorConfig::default() };
    let sup = Supervisor::new(paths, settings.clone(), gate.clone(), db.clone(), Arc::new(NullSink), config);
    let root = dir.path().join("data");
    Rig { _dir: dir, root, sup, db, gate, settings }
}

fn rig() -> Rig {
    rig_at(tempfile::tempdir().unwrap(), "app", dev_pg_dir())
}

async fn wait_phase(sup: &Supervisor, want: AppPhase) {
    let mut w = sup.phase_watch();
    tokio::time::timeout(Duration::from_secs(120), w.wait_for(|p| *p == want)).await.expect("phase timeout").unwrap();
}

/// Boot rejects an unsupported data version before any PostgreSQL start, so no server is needed.
#[tokio::test]
async fn wrong_pg_version_fails() {
    let r = rig();
    std::fs::create_dir_all(r.root.join("pgdata")).unwrap();
    std::fs::write(r.root.join("outlooker-data.json"), b"{}").unwrap();
    std::fs::write(r.root.join("pgdata/PG_VERSION"), "17\n").unwrap();
    r.settings.set_bootstrap(Some(r.root.display().to_string()), Language::En).unwrap();
    let err = r.sup.boot().await.unwrap_err();
    assert!(format!("{err:?}").contains("unsupported PostgreSQL data version 17"));
    assert_eq!(r.sup.phase(), AppPhase::Failed);
}

#[tokio::test]
async fn init_failure_removes_only_own_pgdata() {
    let r = rig_at(tempfile::tempdir().unwrap(), "app", std::path::PathBuf::from("/nonexistent/pg"));
    r.sup.boot().await.unwrap();
    r.sup.initialize(InitializeRequest { data_dir: r.root.display().to_string(), language: Language::ZhTw }).await.unwrap();
    wait_phase(&r.sup, AppPhase::Failed).await;
    assert!(!r.root.join("pgdata").exists());
    assert!(r.root.exists(), "the chosen data root itself is kept");
}

#[tokio::test]
async fn retry_when_not_failed_is_invalid() {
    let r = rig();
    r.sup.boot().await.unwrap();
    assert_eq!(r.sup.retry().await, Err(AppError::invalid("phase", "notFailed")));
}

#[tokio::test]
async fn retry_without_cluster_returns_to_setup() {
    let r = rig_at(tempfile::tempdir().unwrap(), "app", std::path::PathBuf::from("/nonexistent/pg"));
    r.sup.boot().await.unwrap();
    r.sup.initialize(InitializeRequest { data_dir: r.root.display().to_string(), language: Language::ZhTw }).await.unwrap();
    wait_phase(&r.sup, AppPhase::Failed).await;
    r.sup.retry().await.unwrap();
    assert_eq!(r.sup.phase(), AppPhase::NeedsSetup);

    // A marker without a cluster (creation failed after a reset): onboarding adopts the root and fails on the missing
    // cluster, so retry must create the cluster again instead of going back to onboarding.
    std::fs::write(r.root.join("outlooker-data.json"), b"{}").unwrap();
    r.sup.initialize(InitializeRequest { data_dir: r.root.display().to_string(), language: Language::ZhTw }).await.unwrap();
    wait_phase(&r.sup, AppPhase::Failed).await;
    r.sup.retry().await.unwrap();
    assert_eq!(r.sup.phase(), AppPhase::InitializingCluster);
    wait_phase(&r.sup, AppPhase::Failed).await;
}

/// Move validation runs before any stop, so a Supervisor attached to a database of the shared cluster is enough.
#[tokio::test]
async fn move_rejections() {
    let tc = TestCluster::start().await;
    let r = rig();
    let root = tc.paths.data_root().to_path_buf();
    std::fs::write(root.join("outlooker-data.json"), b"{}").unwrap();
    r.settings.set_bootstrap(Some(root.display().to_string()), Language::En).unwrap();
    r.sup.attach_test_db(tc.db_conn());
    r.sup.boot().await.unwrap();
    wait_phase(&r.sup, AppPhase::Ready).await;

    let busy = root.with_file_name("busy");
    std::fs::create_dir_all(&busy).unwrap();
    std::fs::write(busy.join("x"), b"x").unwrap();
    assert_eq!(r.sup.move_data_dir(busy.display().to_string()).await, Err(AppError::invalid("target", "notEmpty")));

    for target in [root.join("backup"), root.join("auth/x/y"), root.clone()] {
        assert_eq!(r.sup.move_data_dir(target.display().to_string()).await, Err(AppError::invalid("target", "insideDataRoot")));
    }
    assert!(!root.join("backup").exists());

    let _import = r.gate.try_enter(Activity::Import).unwrap();
    let target = root.with_file_name("moved");
    assert_eq!(r.sup.move_data_dir(target.display().to_string()).await, Err(AppError::Busy { reason: BusyReason::ImportRunning }));
}

#[tokio::test]
async fn settings_language_before_ready() {
    let r = rig();
    let s = r.settings.update(&r.db, SettingsPatch { language: Some(Language::En), webp_quality: None }).await.unwrap();
    assert_eq!(s.language, Language::En);
    assert_eq!(s.webp_quality, 80);
    assert_eq!(SettingsStore::load(r.root.with_file_name("app").join("settings.json")).unwrap().bootstrap().language, Language::En);
}

#[tokio::test]
async fn settings_webp_before_ready_is_not_ready() {
    let r = rig();
    let err = r.settings.update(&r.db, SettingsPatch { language: Some(Language::En), webp_quality: Some(50) }).await.unwrap_err();
    assert_eq!(err, AppError::DbNotReady);
    assert_eq!(r.settings.bootstrap().language, Language::ZhTw, "nothing written on error");
}

#[test]
fn bootstrap_write_is_atomic() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("settings.json");
    let s = SettingsStore::load(file.clone()).unwrap();
    s.set_bootstrap(Some("/old".into()), Language::En).unwrap();
    // Simulated crash after the temp write and before the rename: a stale, partial temp file.
    std::fs::write(dir.path().join("settings.json.tmp"), b"{\"data_dir\": \"/ne").unwrap();
    assert_eq!(SettingsStore::load(file.clone()).unwrap().bootstrap().data_dir.as_deref(), Some("/old"));
    s.set_data_dir("/new").unwrap();
    assert_eq!(SettingsStore::load(file).unwrap().bootstrap().data_dir.as_deref(), Some("/new"));
    assert!(!dir.path().join("settings.json.tmp").exists());
}
