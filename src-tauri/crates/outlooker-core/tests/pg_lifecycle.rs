use std::net::TcpListener;

use outlooker_core::lifecycle::pg::{auto_conf, connect, total_ram_bytes, PgController, PgStatus, StopMode, ADMIN_ROLE, CJK_SELF_CHECK};
use outlooker_core::paths::{dev_pg_dir, Paths};

const PW: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn setup() -> (tempfile::TempDir, Paths, PgController) {
    assert!(dev_pg_dir().join("bin").exists(), "PostgreSQL binaries missing: run node scripts/fetch-postgres.mjs");
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(dir.path().join("app"), dir.path().join("logs"), dev_pg_dir()).with_data_root(dir.path().join("data"));
    let ctl = PgController::new(paths.clone());
    (dir, paths, ctl)
}

async fn self_check(port: u16) -> bool {
    let c = connect(port, ADMIN_ROLE, PW, "outlooker").await.unwrap();
    c.query_one(CJK_SELF_CHECK, &[]).await.unwrap().get(0)
}

#[tokio::test]
async fn init_passes_cjk_self_check() {
    let (_d, paths, ctl) = setup();
    let locale = ctl.init(&paths.pgdata(), PW).await.unwrap();
    assert!(PgController::locale_candidates().contains(&locale.as_str()));
    assert_eq!(ctl.status(&paths.pgdata()).await.unwrap(), PgStatus::Stopped, "init leaves the cluster stopped");
    let port = ctl.start(&paths.pgdata()).await.unwrap();
    assert!(self_check(port).await);
    ctl.stop(&paths.pgdata(), StopMode::Fast).await.unwrap();
}

#[tokio::test]
async fn c_locale_is_rejected() {
    let (_d, paths, ctl) = setup();
    let err = ctl.init_with_candidates(&paths.pgdata(), PW, &["C"]).await.unwrap_err();
    assert!(format!("{err:?}").contains("self-check"), "{err:?}");
    assert!(!paths.pgdata().exists(), "a rejected candidate leaves no cluster behind");
}

#[tokio::test]
async fn stop_start_round_trip() {
    let (_d, paths, ctl) = setup();
    ctl.init(&paths.pgdata(), PW).await.unwrap();
    let p1 = ctl.start(&paths.pgdata()).await.unwrap();
    assert!(matches!(ctl.status(&paths.pgdata()).await.unwrap(), PgStatus::Running { pid } if pid > 0));
    ctl.stop(&paths.pgdata(), StopMode::Fast).await.unwrap();
    assert_eq!(ctl.status(&paths.pgdata()).await.unwrap(), PgStatus::Stopped);
    let p2 = ctl.start(&paths.pgdata()).await.unwrap();
    assert!(self_check(p2).await);
    let _ = p1;
    ctl.stop(&paths.pgdata(), StopMode::Fast).await.unwrap();
}

#[tokio::test]
async fn orphan_postmaster_is_restarted() {
    let (_d, paths, ctl) = setup();
    ctl.init(&paths.pgdata(), PW).await.unwrap();
    let old_port = ctl.start(&paths.pgdata()).await.unwrap();
    // Simulate an app that was killed: a new controller finds the running postmaster.
    let ctl2 = PgController::new(paths.clone());
    let new_port = ctl2.start(&paths.pgdata()).await.unwrap();
    assert!(self_check(new_port).await);
    if new_port != old_port {
        assert!(connect(old_port, ADMIN_ROLE, PW, "outlooker").await.is_err(), "orphan must be gone");
    }
    ctl2.stop(&paths.pgdata(), StopMode::Fast).await.unwrap();
}

fn fake_pid_file(paths: &Paths, pid: u32) {
    let body = format!("{pid}\n{}\n1700000000\n5432\n\n127.0.0.1\n  5432001  0\nready   \n", paths.pgdata().display());
    std::fs::write(paths.pgdata().join("postmaster.pid"), body).unwrap();
}

#[tokio::test]
async fn stale_pid_with_dead_process() {
    let (_d, paths, ctl) = setup();
    ctl.init(&paths.pgdata(), PW).await.unwrap();
    let mut child = std::process::Command::new(if cfg!(windows) { "cmd" } else { "true" })
        .args(if cfg!(windows) { vec!["/C", "exit"] } else { vec![] })
        .spawn()
        .unwrap();
    let dead = child.id();
    child.wait().unwrap();
    fake_pid_file(&paths, dead);
    let port = ctl.start(&paths.pgdata()).await.unwrap();
    assert!(self_check(port).await);
    ctl.stop(&paths.pgdata(), StopMode::Fast).await.unwrap();
}

#[tokio::test]
async fn pid_reused_by_live_non_postgres_process() {
    let (_d, paths, ctl) = setup();
    ctl.init(&paths.pgdata(), PW).await.unwrap();
    fake_pid_file(&paths, std::process::id()); // this test process: alive, not postgres
    let port = ctl.start(&paths.pgdata()).await.unwrap();
    assert!(self_check(port).await, "the test process survived and the cluster started");
    ctl.stop(&paths.pgdata(), StopMode::Fast).await.unwrap();
}

#[tokio::test]
async fn port_conflict_retries() {
    let (_d, paths, ctl) = setup();
    ctl.init(&paths.pgdata(), PW).await.unwrap();
    let busy = TcpListener::bind("127.0.0.1:0").unwrap();
    let busy_port = busy.local_addr().unwrap().port();
    let ctl = ctl.with_ports(vec![busy_port]);
    let port = ctl.start(&paths.pgdata()).await.unwrap();
    assert_ne!(port, busy_port);
    assert!(self_check(port).await);
    ctl.stop(&paths.pgdata(), StopMode::Fast).await.unwrap();
    drop(busy);
}

#[tokio::test]
async fn auto_conf_values_are_live() {
    let (_d, paths, ctl) = setup();
    ctl.init(&paths.pgdata(), PW).await.unwrap();
    let port = ctl.start(&paths.pgdata()).await.unwrap();
    let c = connect(port, ADMIN_ROLE, PW, "outlooker").await.unwrap();
    for (name, want) in [("wal_level", "minimal"), ("max_wal_senders", "0"), ("plan_cache_mode", "force_custom_plan"), ("jit", "off"), ("TimeZone", "UTC")] {
        let got: String = c.query_one(&format!("SHOW {name}"), &[]).await.unwrap().get(0);
        assert_eq!(got, want, "{name}");
    }
    let shared: i64 = c.query_one("SELECT setting::bigint * 8192 FROM pg_settings WHERE name = 'shared_buffers'", &[]).await.unwrap().get(0);
    let expected_mb = auto_conf(total_ram_bytes()).lines().find(|l| l.starts_with("shared_buffers")).unwrap()
        .split('\'').nth(1).unwrap().trim_end_matches("MB").parse::<i64>().unwrap();
    assert_eq!(shared, expected_mb * 1024 * 1024);
    ctl.stop(&paths.pgdata(), StopMode::Fast).await.unwrap();
}
