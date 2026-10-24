//! The shared PostgreSQL cluster for one test run. `app/scripts/cargo-test-runner.mjs` spawns it once before a test
//! binary that needs it, reads `<port>\t<run dir>` from the first stdout line into `OUTLOOKER_TEST_PG`, and closes
//! stdin when the binary exits; then the cluster stops (immediate) and the run dir is removed. Tests only connect
//! (`test_support::TestCluster`); this example is the only place that boots it.

#[cfg(not(feature = "test-support"))]
fn main() {
    eprintln!("test_pg_server needs the test-support feature (cargo test --workspace enables it)");
    std::process::exit(2);
}

#[cfg(feature = "test-support")]
fn main() {
    use std::io::{Read, Write};
    use std::path::Path;

    use outlooker_core::db::{connect, Credentials, ADMIN_ROLE};
    use outlooker_core::lifecycle::pg::PgController;
    use outlooker_core::paths::{dev_pg_dir, Paths};
    use outlooker_core::test_support::seeded_root;
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

    const TEST_CONF: &str = "fsync = off\nfull_page_writes = off\nmax_connections = 400\nshared_buffers = '128MB'\n";
    const RUN_PREFIX: &str = "outlooker-test-pg-run-";

    fn pg_ctl_stop(dir: &Path) {
        let pg_ctl = dev_pg_dir().join("bin").join(format!("pg_ctl{}", std::env::consts::EXE_SUFFIX));
        let _ = std::process::Command::new(pg_ctl).args(["stop", "-m", "immediate", "-w", "-D"]).arg(dir.join("pgdata")).output();
    }

    // Sweep: clusters left behind by servers that died without cleaning up (SIGKILL).
    let mut sys = System::new();
    for e in std::fs::read_dir(std::env::temp_dir()).expect("read temp dir").flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Some(rest) = name.strip_prefix(RUN_PREFIX) else { continue };
        let Some(pid) = rest.split('-').next().and_then(|p| p.parse::<u32>().ok()) else { continue };
        sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[Pid::from_u32(pid)]), true, ProcessRefreshKind::nothing());
        if pid != std::process::id() && sys.process(Pid::from_u32(pid)).is_none() {
            pg_ctl_stop(&e.path());
            let _ = std::fs::remove_dir_all(e.path());
        }
    }

    let gen_ = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("{RUN_PREFIX}{}-{gen_}", std::process::id()));
    seeded_root(&dir);
    let paths = Paths::new(dir.join("app"), dir.join("logs"), dev_pg_dir()).with_data_root(dir.clone());
    std::fs::OpenOptions::new()
        .append(true)
        .open(paths.pgdata().join("postgresql.auto.conf"))
        .and_then(|mut f| f.write_all(TEST_CONF.as_bytes()))
        .expect("append auto conf");
    let creds = Credentials::load(&paths.credentials()).expect("template credentials");
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("runtime");
    let port = rt.block_on(async {
        let port = PgController::new(paths.clone()).start(&paths.pgdata()).await.expect("start shared cluster");
        let admin = connect(port, ADMIN_ROLE, &creds.admin, "postgres").await.expect("admin connect");
        admin.batch_execute("ALTER ROLE outlooker_ro CONNECTION LIMIT -1").await.expect("lift ro connection limit");
        port
    });
    eprintln!("SHARED_BOOT port {port}");
    println!("{port}\t{}", dir.display());
    std::io::stdout().flush().expect("stdout");
    let _ = std::io::stdin().read_to_end(&mut Vec::new());
    pg_ctl_stop(&dir);
    let _ = std::fs::remove_dir_all(&dir);
}
