#![cfg(feature = "test-support")]
//! The only tests that start their own PostgreSQL: creating, restarting, moving and taking over the cluster IS the
//! behavior under test. Each runs on a clone of the cached template data root (`seeded_root`) except X1, which runs
//! the real initdb. Run `node scripts/fetch-postgres.mjs` first.

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use outlooker_core::db::{connect, Credentials, Db, ADMIN_ROLE, CJK_SELF_CHECK};
use outlooker_core::dto::app::{AppPhase, InitializeRequest, Language};
use outlooker_core::dto::error::AppError;
use outlooker_core::dto::events::DataDirMovePhase;
use outlooker_core::gate::Gate;
use outlooker_core::lifecycle::pg::{auto_conf, PgController, StopMode};
use outlooker_core::lifecycle::{Supervisor, SupervisorConfig};
use outlooker_core::paths::{dev_pg_dir, Paths};
use outlooker_core::settings::SettingsStore;
use outlooker_core::sink::{CoreEvent, EventSink};
use outlooker_core::sys::total_ram_bytes;
use outlooker_core::test_support::seeded_root;
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
    root: PathBuf,
    sup: Supervisor,
    db: Db,
    settings: SettingsStore,
    events: mpsc::UnboundedReceiver<CoreEvent>,
}

fn rig_at(dir: tempfile::TempDir, app: &str, config: SupervisorConfig) -> Rig {
    assert!(dev_pg_dir().join("bin").exists(), "PostgreSQL binaries missing: run node scripts/fetch-postgres.mjs");
    let paths = Paths::new(dir.path().join(app), dir.path().join("logs"), dev_pg_dir());
    let settings = SettingsStore::load(paths.bootstrap_file()).unwrap();
    let (tx, events) = mpsc::unbounded_channel();
    let db = Db::new();
    let sup = Supervisor::new(paths, settings.clone(), Gate::new(), db.clone(), Arc::new(ChannelSink(tx)), config);
    let root = dir.path().join("data");
    Rig { _dir: dir, root, sup, db, settings, events }
}

fn fast_probe() -> SupervisorConfig {
    SupervisorConfig { probe_interval: Duration::from_millis(100), ..SupervisorConfig::default() }
}

/// A rig whose data root is a clone of the template (no initdb): `boot()` starts it.
fn seeded_rig(config: SupervisorConfig) -> Rig {
    let r = rig_at(tempfile::tempdir().unwrap(), "app", config);
    seeded_root(&r.root);
    r.settings.set_bootstrap(Some(r.root.display().to_string()), Language::En).unwrap();
    r
}

async fn wait_phase(sup: &Supervisor, want: AppPhase) {
    let mut w = sup.phase_watch();
    tokio::time::timeout(Duration::from_secs(120), w.wait_for(|p| *p == want)).await.expect("phase timeout").unwrap();
}

async fn next_move_end(events: &mut mpsc::UnboundedReceiver<CoreEvent>) -> DataDirMovePhase {
    loop {
        let e = tokio::time::timeout(Duration::from_secs(120), events.recv()).await.expect("event timeout").unwrap();
        if let CoreEvent::DataDirMoveProgress(p) = e
            && matches!(p.phase, DataDirMovePhase::Done | DataDirMovePhase::Failed)
        {
            return p.phase;
        }
    }
}

/// Phases from the event stream up to and including the first `Ready`.
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

fn tree_bytes(dir: &Path) -> (u64, u64) {
    let (mut f, mut b) = (0, 0);
    for e in std::fs::read_dir(dir).unwrap() {
        let e = e.unwrap();
        if e.file_type().unwrap().is_dir() {
            let (x, y) = tree_bytes(&e.path());
            f += x;
            b += y;
        } else {
            f += 1;
            b += e.metadata().unwrap().len();
        }
    }
    (f, b)
}

/// X1: first launch creates a working library (initdb), an unsupported data version fails and `retry` recovers.
#[tokio::test]
async fn first_run_reaches_ready() {
    let mut r = rig_at(tempfile::tempdir().unwrap(), "app", fast_probe());
    r.sup.boot().await.unwrap();
    r.sup.initialize(InitializeRequest { data_dir: r.root.display().to_string(), language: Language::ZhTw }).await.unwrap();
    wait_phase(&r.sup, AppPhase::Ready).await;
    let p = phases(&mut r.events).await;
    let order = [AppPhase::NeedsSetup, AppPhase::InitializingCluster, AppPhase::StartingDb, AppPhase::Ready];
    let mut it = p.iter();
    assert!(order.iter().all(|want| it.any(|got| got == want)), "phases {p:?}");
    assert!(r.root.join("outlooker-data.json").exists());
    assert_eq!(r.sup.status().await.counts.messages, 0);
    r.sup.shutdown_db().await.unwrap(); // not kill(): the health monitor would restart the cluster

    // A second app instance adopts the same root; an unsupported data version fails boot, then retry recovers.
    std::fs::write(r.root.join("pgdata/PG_VERSION"), "17\n").unwrap();
    let r2 = rig_at(tempfile::tempdir().unwrap(), "app2", fast_probe());
    r2.settings.set_bootstrap(Some(r.root.display().to_string()), Language::En).unwrap();
    let err = r2.sup.boot().await.unwrap_err();
    assert!(format!("{err:?}").contains("unsupported PostgreSQL data version 17"));
    assert_eq!(r2.sup.phase(), AppPhase::Failed);
    std::fs::write(r.root.join("pgdata/PG_VERSION"), "18\n").unwrap();
    r2.sup.retry().await.unwrap();
    wait_phase(&r2.sup, AppPhase::Ready).await;
    r2.sup.shutdown_db().await.unwrap();
}

/// X2: a dead postmaster is restarted, and after the restart budget is spent the app stops and reports an error.
#[tokio::test]
async fn restart_budget_exhaustion_fails() {
    let r = seeded_rig(SupervisorConfig { restart_budget: 1, ..fast_probe() });
    r.sup.boot().await.unwrap();
    wait_phase(&r.sup, AppPhase::Ready).await;
    kill(&r.root);
    wait_phase(&r.sup, AppPhase::StartingDb).await;
    wait_phase(&r.sup, AppPhase::Ready).await;
    kill(&r.root);
    wait_phase(&r.sup, AppPhase::Failed).await;
    assert!(r.sup.status().await.error.is_some());
}

/// X3b: a failed move restores the old root without data loss.
#[tokio::test]
async fn move_verify_failure_rolls_back() {
    let mut r = seeded_rig(fast_probe());
    r.sup.boot().await.unwrap();
    wait_phase(&r.sup, AppPhase::Ready).await;

    // Failed verify into a fresh target: the partial copy is removed and the old root is untouched.
    let before = tree_bytes(&r.root.join("auth"));
    let fresh = r.root.with_file_name("fresh");
    r.sup.fail_next_move_verify();
    r.sup.move_data_dir(fresh.display().to_string()).await.unwrap();
    assert_eq!(next_move_end(&mut r.events).await, DataDirMovePhase::Failed);
    wait_phase(&r.sup, AppPhase::Ready).await;
    assert!(!fresh.exists(), "partial copy removed");
    assert_eq!(tree_bytes(&r.root.join("auth")), before, "old root untouched");
    assert_eq!(r.settings.bootstrap().data_dir.as_deref(), Some(r.root.display().to_string().as_str()));
    r.sup.shutdown_db().await.unwrap();
}

/// X3a: moving the library works.
#[tokio::test]
async fn move_data_dir_succeeds() {
    let mut r = seeded_rig(fast_probe());
    r.sup.boot().await.unwrap();
    wait_phase(&r.sup, AppPhase::Ready).await;

    let target = r.root.with_file_name("moved");
    r.sup.move_data_dir(target.display().to_string()).await.unwrap();
    assert_eq!(next_move_end(&mut r.events).await, DataDirMovePhase::Done);
    wait_phase(&r.sup, AppPhase::Ready).await;
    assert!(target.join("outlooker-data.json").exists());
    assert!(!r.root.exists());
    assert_eq!(r.settings.bootstrap().data_dir.as_deref(), Some(target.display().to_string().as_str()));
    r.sup.shutdown_db().await.unwrap();
}

/// X5: a library migrated by another build (edited migration) fails boot as incompatible; the reset deletes only
/// `pgdata` and the same data root comes back empty and working, with its other files and credentials untouched.
#[tokio::test]
async fn reset_recreates_incompatible_database() {
    let r = seeded_rig(fast_probe());
    r.sup.boot().await.unwrap();
    let admin = r.db.connect_admin().await.unwrap();
    admin.batch_execute("CREATE TABLE reset_probe (); UPDATE refinery_schema_history SET checksum = '1' WHERE version = 1").await.unwrap();
    drop(admin);
    r.sup.shutdown_db().await.unwrap();
    let creds = std::fs::read(r.root.join("auth/credentials")).unwrap();
    std::fs::write(r.root.join("keep.txt"), b"keep").unwrap();

    std::fs::write(r.root.join("import-inflight"), b"{}").unwrap();

    let err = r.sup.boot().await.unwrap_err();
    assert!(matches!(err, AppError::IncompatibleDatabase { .. }), "{err:?}");
    // A pgdata that is not a plain directory (a file here, a symlink takes the same check) is refused, nothing deleted.
    let moved = r._dir.path().join("pgdata-moved");
    std::fs::rename(r.root.join("pgdata"), &moved).unwrap();
    std::fs::write(r.root.join("pgdata"), b"x").unwrap();
    assert!(r.sup.reset_database().await.is_err());
    assert_eq!(r.sup.phase(), AppPhase::Failed);
    assert!(r.root.join("pgdata").is_file() && r.root.join("import-inflight").exists());
    std::fs::remove_file(r.root.join("pgdata")).unwrap();
    std::fs::rename(&moved, r.root.join("pgdata")).unwrap();
    // A reset that stopped partway (PG_VERSION already deleted, `pg_ctl status` fails there) can be repeated.
    std::fs::remove_file(r.root.join("pgdata/PG_VERSION")).unwrap();

    r.sup.reset_database().await.unwrap();
    assert!(!r.root.join("import-inflight").exists(), "the import crash record of the old database is deleted");
    assert!(r.sup.reset_database().await.is_err(), "a second reset is refused while the first runs");
    wait_phase(&r.sup, AppPhase::Ready).await;

    let admin = r.db.connect_admin().await.unwrap();
    let fresh: bool = admin.query_one("SELECT to_regclass('reset_probe') IS NULL", &[]).await.unwrap().get(0);
    assert!(fresh, "the old database is gone");
    let counts = r.sup.status().await.counts;
    assert_eq!((counts.messages, counts.psts), (0, 0));
    assert_eq!(std::fs::read(r.root.join("keep.txt")).unwrap(), b"keep");
    assert_eq!(std::fs::read(r.root.join("auth/credentials")).unwrap(), creds);
    assert!(r.root.join("outlooker-data.json").exists());
    r.sup.shutdown_db().await.unwrap();
}

fn fake_pid_file(paths: &Paths, pid: u32) {
    let body = format!("{pid}\n{}\n1700000000\n5432\n\n127.0.0.1\n  5432001  0\nready   \n", paths.pgdata().display());
    std::fs::write(paths.pgdata().join("postmaster.pid"), body).unwrap();
}

/// X4: starting after a crash or reboot works with a stale pid file, a still-running orphan and a busy port.
#[tokio::test]
async fn orphan_postmaster_is_restarted() {
    assert!(dev_pg_dir().join("bin").exists(), "PostgreSQL binaries missing: run node scripts/fetch-postgres.mjs");
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(dir.path().join("app"), dir.path().join("logs"), dev_pg_dir()).with_data_root(dir.path().join("data"));
    seeded_root(paths.data_root());
    let pw = Credentials::load(&paths.credentials()).unwrap().admin;
    let ctl = PgController::new(paths.clone());
    let self_check = |port: u16| {
        let pw = pw.clone();
        async move {
            let c = connect(port, ADMIN_ROLE, &pw, "outlooker").await.unwrap();
            c.query_one(CJK_SELF_CHECK, &[]).await.unwrap().get::<_, bool>(0)
        }
    };

    // (a) a pid file naming a live non-postgres process (this test process) is ignored; the live server has the production settings.
    fake_pid_file(&paths, std::process::id());
    let old_port = ctl.start(&paths.pgdata()).await.unwrap();
    assert!(self_check(old_port).await, "the test process survived and the cluster started");
    let c = connect(old_port, ADMIN_ROLE, &pw, "outlooker").await.unwrap();
    for (name, want) in [("wal_level", "minimal"), ("max_wal_senders", "0"), ("plan_cache_mode", "force_custom_plan"), ("jit", "off"), ("TimeZone", "UTC")] {
        let got: String = c.query_one(&format!("SHOW {name}"), &[]).await.unwrap().get(0);
        assert_eq!(got, want, "{name}");
    }
    let shared: i64 = c.query_one("SELECT setting::bigint * 8192 FROM pg_settings WHERE name = 'shared_buffers'", &[]).await.unwrap().get(0);
    let expected_mb = auto_conf(total_ram_bytes()).lines().find(|l| l.starts_with("shared_buffers")).unwrap()
        .split('\'').nth(1).unwrap().trim_end_matches("MB").parse::<i64>().unwrap();
    assert_eq!(shared, expected_mb * 1024 * 1024);
    drop(c);

    // (b) an app that was killed leaves a running postmaster: a new controller replaces it.
    let ctl2 = PgController::new(paths.clone());
    let new_port = ctl2.start(&paths.pgdata()).await.unwrap();
    assert!(self_check(new_port).await);
    if new_port != old_port {
        assert!(connect(old_port, ADMIN_ROLE, &pw, "outlooker").await.is_err(), "orphan must be gone");
    }
    ctl2.stop(&paths.pgdata(), StopMode::Fast).await.unwrap();

    // (c) a busy preferred port makes the controller pick another one.
    let busy = TcpListener::bind("127.0.0.1:0").unwrap();
    let busy_port = busy.local_addr().unwrap().port();
    let ctl3 = PgController::new(paths.clone()).with_ports(vec![busy_port]);
    let port = ctl3.start(&paths.pgdata()).await.unwrap();
    assert_ne!(port, busy_port);
    assert!(self_check(port).await);
    ctl3.stop(&paths.pgdata(), StopMode::Fast).await.unwrap();
    drop(busy);
}

/// Pre-builds the template data root without a time cap (the cargo test runner calls it first).
#[test]
#[ignore]
fn warm_template() {
    outlooker_core::test_support::warm_template()
}
