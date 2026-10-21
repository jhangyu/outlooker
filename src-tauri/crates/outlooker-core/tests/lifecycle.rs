#![cfg(feature = "test-support")]

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use outlooker_core::db::Db;
use outlooker_core::dto::app::{AppPhase, InitializeRequest, Language};
use outlooker_core::dto::error::{AppError, BusyReason};
use outlooker_core::dto::events::DataDirMovePhase;
use outlooker_core::dto::settings::SettingsPatch;
use outlooker_core::gate::{Activity, Gate};
use outlooker_core::lifecycle::{Supervisor, SupervisorConfig};
use outlooker_core::paths::{dev_pg_dir, Paths};
use outlooker_core::settings::SettingsStore;
use outlooker_core::sink::{CoreEvent, EventSink};
use tokio::sync::mpsc;

/// Forwards events to a channel so tests wait on events instead of polling.
struct ChannelSink(mpsc::UnboundedSender<CoreEvent>);
impl EventSink for ChannelSink {
    fn emit(&self, e: CoreEvent) {
        let _ = self.0.send(e);
    }
}

struct Rig {
    _dir: tempfile::TempDir,
    root: std::path::PathBuf,
    sup: Supervisor,
    db: Db,
    gate: Gate,
    settings: SettingsStore,
    events: mpsc::UnboundedReceiver<CoreEvent>,
}

fn rig_at(dir: tempfile::TempDir, app: &str, pg_dir: std::path::PathBuf) -> Rig {
    assert!(dev_pg_dir().join("bin").exists(), "PostgreSQL binaries missing: run node scripts/fetch-postgres.mjs");
    let paths = Paths::new(dir.path().join(app), dir.path().join("logs"), pg_dir);
    let settings = SettingsStore::load(paths.bootstrap_file()).unwrap();
    let (tx, events) = mpsc::unbounded_channel();
    let (db, gate) = (Db::new(), Gate::new());
    let config = SupervisorConfig { probe_interval: Duration::from_millis(100), ..SupervisorConfig::default() };
    let sup = Supervisor::new(paths, settings.clone(), gate.clone(), db.clone(), Arc::new(ChannelSink(tx)), config);
    let root = dir.path().join("data");
    Rig { _dir: dir, root, sup, db, gate, settings, events }
}

fn rig() -> Rig {
    rig_at(tempfile::tempdir().unwrap(), "app", dev_pg_dir())
}

async fn wait_phase(sup: &Supervisor, want: AppPhase) {
    let mut w = sup.phase_watch();
    tokio::time::timeout(Duration::from_secs(120), w.wait_for(|p| *p == want)).await.expect("phase timeout").unwrap();
}

async fn ready(r: &Rig) {
    r.sup.boot().await.unwrap();
    r.sup.initialize(InitializeRequest { data_dir: r.root.display().to_string(), language: Language::ZhTw }).await.unwrap();
    wait_phase(&r.sup, AppPhase::Ready).await;
}

async fn next_move_end(events: &mut mpsc::UnboundedReceiver<CoreEvent>) -> DataDirMovePhase {
    loop {
        let e = tokio::time::timeout(Duration::from_secs(120), events.recv()).await.expect("event timeout").unwrap();
        if let CoreEvent::DataDirMoveProgress(p) = e
            && matches!(p.phase, DataDirMovePhase::Done | DataDirMovePhase::Failed)
        {
            eprintln!("move ended {:?}: {:?}", p.phase, p.error);
            return p.phase;
        }
    }
}

/// Phases from the event stream up to and including the first `Ready` (the event is emitted after the watch value changes).
async fn phases(events: &mut mpsc::UnboundedReceiver<CoreEvent>) -> Vec<AppPhase> {
    let mut v = Vec::new();
    while !v.contains(&AppPhase::Ready) {
        let e = tokio::time::timeout(Duration::from_secs(60), events.recv()).await.expect("event timeout").unwrap();
        if let CoreEvent::AppStatusChanged(s) = e {
            v.push(s.phase);
        }
    }
    v
}

fn kill(root: &Path) {
    let paths = Paths::new("/unused".into(), "/unused".into(), dev_pg_dir()).with_data_root(root.to_path_buf());
    let _ = std::process::Command::new(paths.pg_bin("pg_ctl")).args(["stop", "-m", "immediate", "-w", "-D"]).arg(paths.pgdata()).output();
}

#[tokio::test]
async fn first_run_reaches_ready() {
    let mut r = rig();
    ready(&r).await;
    let p = phases(&mut r.events).await;
    let order = [AppPhase::NeedsSetup, AppPhase::InitializingCluster, AppPhase::StartingDb, AppPhase::Ready];
    let mut it = p.iter();
    assert!(order.iter().all(|want| it.any(|got| got == want)), "phases {p:?}");
    assert!(r.root.join("outlooker-data.json").exists());
    assert_eq!(r.sup.status().await.counts.messages, 0);
    r.sup.shutdown_db().await.unwrap();
}

#[tokio::test]
async fn adopt_existing_data_root() {
    let r = rig();
    ready(&r).await;
    r.sup.shutdown_db().await.unwrap();
    let root = r.root.clone();
    let r2 = rig_at(tempfile::tempdir().unwrap(), "other-app", dev_pg_dir());
    r2.sup.boot().await.unwrap();
    r2.sup.initialize(InitializeRequest { data_dir: root.display().to_string(), language: Language::En }).await.unwrap();
    wait_phase(&r2.sup, AppPhase::Ready).await;
    r2.sup.shutdown_db().await.unwrap();
}

#[tokio::test]
async fn wrong_pg_version_fails() {
    let r = rig();
    ready(&r).await;
    r.sup.shutdown_db().await.unwrap();
    std::fs::write(r.root.join("pgdata/PG_VERSION"), "17\n").unwrap();
    let r2 = rig_at(tempfile::tempdir().unwrap(), "app2", dev_pg_dir());
    r2.settings.set_bootstrap(Some(r.root.display().to_string()), Language::En).unwrap();
    let err = r2.sup.boot().await.unwrap_err();
    assert!(format!("{err:?}").contains("unsupported PostgreSQL data version 17"));
    assert_eq!(r2.sup.phase(), AppPhase::Failed);
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
async fn retry_from_failed_reaches_ready() {
    let r = rig();
    ready(&r).await;
    r.sup.shutdown_db().await.unwrap(); // not kill(): r's health monitor would restart the cluster
    std::fs::write(r.root.join("pgdata/PG_VERSION"), "17\n").unwrap();
    let r2 = rig_at(tempfile::tempdir().unwrap(), "app2", dev_pg_dir());
    r2.settings.set_bootstrap(Some(r.root.display().to_string()), Language::En).unwrap();
    assert!(r2.sup.boot().await.is_err());
    std::fs::write(r.root.join("pgdata/PG_VERSION"), "18\n").unwrap();
    r2.sup.retry().await.unwrap();
    wait_phase(&r2.sup, AppPhase::Ready).await;
    r2.sup.shutdown_db().await.unwrap();
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
}

#[tokio::test]
async fn health_restarts_killed_postmaster() {
    let r = rig();
    ready(&r).await;
    kill(&r.root);
    wait_phase(&r.sup, AppPhase::StartingDb).await;
    wait_phase(&r.sup, AppPhase::Ready).await;
    r.sup.shutdown_db().await.unwrap();
}

#[tokio::test]
async fn restart_budget_exhaustion_fails() {
    let r = rig();
    ready(&r).await;
    for _ in 0..3 {
        kill(&r.root);
        wait_phase(&r.sup, AppPhase::StartingDb).await;
        wait_phase(&r.sup, AppPhase::Ready).await;
    }
    kill(&r.root);
    wait_phase(&r.sup, AppPhase::Failed).await;
    assert!(r.sup.status().await.error.is_some());
}

#[tokio::test]
async fn move_data_dir_succeeds() {
    let mut r = rig();
    ready(&r).await;
    let target = r.root.with_file_name("moved");
    r.sup.move_data_dir(target.display().to_string()).await.unwrap();
    assert_eq!(next_move_end(&mut r.events).await, DataDirMovePhase::Done);
    wait_phase(&r.sup, AppPhase::Ready).await;
    assert!(target.join("outlooker-data.json").exists());
    assert!(!r.root.exists());
    assert_eq!(r.settings.bootstrap().data_dir.as_deref(), Some(target.display().to_string().as_str()));
    r.sup.shutdown_db().await.unwrap();
}

fn tree_bytes(dir: &Path) -> (u64, u64) {
    let (mut f, mut b) = (0, 0);
    for e in std::fs::read_dir(dir).unwrap() {
        let e = e.unwrap();
        if e.file_type().unwrap().is_dir() {
            let (x, y) = tree_bytes(&e.path());
            f += x;
            b += y;
        } else if !e.path().to_string_lossy().contains("pgdata/log") {
            f += 1;
            b += e.metadata().unwrap().len();
        }
    }
    (f, b)
}

#[tokio::test]
async fn move_verify_failure_rolls_back() {
    let mut r = rig();
    ready(&r).await;
    r.sup.shutdown_db().await.unwrap();
    let before = tree_bytes(&r.root.join("auth"));
    r.sup.boot().await.unwrap(); // restart after shutdown_db goes through boot
    wait_phase(&r.sup, AppPhase::Ready).await;
    r.sup.fail_next_move_verify();
    let target = r.root.with_file_name("moved");
    r.sup.move_data_dir(target.display().to_string()).await.unwrap();
    assert_eq!(next_move_end(&mut r.events).await, DataDirMovePhase::Failed);
    wait_phase(&r.sup, AppPhase::Ready).await;
    assert!(!target.exists(), "partial copy removed");
    assert_eq!(tree_bytes(&r.root.join("auth")), before, "old root untouched");
    assert_eq!(r.settings.bootstrap().data_dir.as_deref(), Some(r.root.display().to_string().as_str()));
    r.sup.shutdown_db().await.unwrap();
}

#[tokio::test]
async fn move_into_non_empty_is_invalid() {
    let r = rig();
    ready(&r).await;
    let target = r.root.with_file_name("busy");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("x"), b"x").unwrap();
    assert_eq!(r.sup.move_data_dir(target.display().to_string()).await, Err(AppError::invalid("target", "notEmpty")));
    r.sup.shutdown_db().await.unwrap();
}

#[tokio::test]
async fn move_into_own_subfolder_is_invalid() {
    let r = rig();
    ready(&r).await;
    for target in [r.root.join("backup"), r.root.join("auth/x/y"), r.root.clone()] {
        assert_eq!(r.sup.move_data_dir(target.display().to_string()).await, Err(AppError::invalid("target", "insideDataRoot")));
    }
    assert!(!r.root.join("backup").exists());
    r.sup.shutdown_db().await.unwrap();
}

#[tokio::test]
async fn move_while_import_is_busy() {
    let r = rig();
    ready(&r).await;
    let _import = r.gate.try_enter(Activity::Import).unwrap();
    let target = r.root.with_file_name("moved");
    assert_eq!(r.sup.move_data_dir(target.display().to_string()).await, Err(AppError::Busy { reason: BusyReason::ImportRunning }));
    r.sup.shutdown_db().await.unwrap();
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
