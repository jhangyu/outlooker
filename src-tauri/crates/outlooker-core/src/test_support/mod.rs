//! Shared real-PostgreSQL test fixture: ONE cluster per test process (cloned from a cached template data root),
//! one fresh `FILE_COPY` database per test. Production initdb flags, migrated, both pools.
pub mod import_fixtures;
mod template;

use std::collections::HashSet;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use tempfile::TempDir;

use crate::db::{connect, Credentials, Db, DbConn, Pools, ADMIN_ROLE, DB_NAME};
use crate::lifecycle::pg::PgController;
use crate::paths::{dev_pg_dir, Paths};

pub use template::seeded_root;

/// Builds the template data root (files only, no shared cluster). The cargo test runner calls it uncapped
/// so a cold template build never lands inside a capped test.
pub fn warm_template() {
    template::template_root();
}

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

const TEST_CONF: &str = "fsync = off\nfull_page_writes = off\nmax_connections = 400\nshared_buffers = '128MB'\n";
const RUN_PREFIX: &str = "outlooker-test-pg-run-";

struct Run {
    dir: PathBuf,
    port: u16,
    creds: Credentials,
}

static RUN: OnceLock<Run> = OnceLock::new();
static SEEDS: Mutex<Option<HashSet<&'static str>>> = Mutex::new(None);
static NEXT: AtomicU32 = AtomicU32::new(0);

pub struct TestCluster {
    pub dir: TempDir,
    pub paths: Paths,
    pub port: u16,
    pub creds: Credentials,
    pub db: Db,
    dbname: String,
}

/// `SELECT count(*)`-style single-integer query.
pub async fn count(c: &tokio_postgres::Client, sql: &str) -> i64 {
    c.query_one(sql, &[]).await.unwrap().get(0)
}

/// Deterministic gradient PNG of `w` x `h` pixels.
pub fn png(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_fn(w, h, |x, y| image::Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

/// Runs `f` on its own current-thread runtime in a scoped thread, so no tokio task of the shared state is tied
/// to one test's runtime.
fn block_on<F: Future + Send>(f: F) -> F::Output
where
    F::Output: Send,
{
    std::thread::scope(|s| {
        s.spawn(|| tokio::runtime::Builder::new_current_thread().enable_all().build().expect("runtime").block_on(f))
            .join()
            .expect("block_on thread")
    })
}

fn pg_ctl_stop(dir: &std::path::Path) {
    let pg_ctl = dev_pg_dir().join("bin").join(format!("pg_ctl{}", std::env::consts::EXE_SUFFIX));
    let _ = std::process::Command::new(pg_ctl).args(["stop", "-m", "immediate", "-w", "-D"]).arg(dir.join("pgdata")).output();
}

extern "C" fn stop_shared() {
    if let Some(run) = RUN.get() {
        pg_ctl_stop(&run.dir);
        let _ = std::fs::remove_dir_all(&run.dir);
    }
}

unsafe extern "C" {
    safe fn atexit(cb: extern "C" fn()) -> std::ffi::c_int;
}

/// Removes clusters left behind by test processes that died without running the exit hook.
fn sweep_dead_runs() {
    let mut sys = System::new();
    for e in std::fs::read_dir(std::env::temp_dir()).expect("read temp dir").flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Some(rest) = name.strip_prefix(RUN_PREFIX) else { continue };
        let Some(pid) = rest.split('-').next().and_then(|p| p.parse::<u32>().ok()) else { continue };
        if pid == std::process::id() {
            continue;
        }
        sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[Pid::from_u32(pid)]), true, ProcessRefreshKind::nothing());
        if sys.process(Pid::from_u32(pid)).is_none() {
            pg_ctl_stop(&e.path());
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

fn boot_shared() -> Run {
    sweep_dead_runs();
    let gen_ = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("{RUN_PREFIX}{}-{gen_}", std::process::id()));
    seeded_root(&dir);
    let paths = Paths::new(dir.join("app"), dir.join("logs"), dev_pg_dir()).with_data_root(dir.clone());
    {
        use std::io::Write;
        let mut conf = std::fs::OpenOptions::new().append(true).open(paths.pgdata().join("postgresql.auto.conf")).expect("open auto conf");
        conf.write_all(TEST_CONF.as_bytes()).expect("append auto conf");
    }
    let creds = Credentials::load(&paths.credentials()).expect("template credentials");
    let port = block_on(async {
        let port = PgController::new(paths.clone()).start(&paths.pgdata()).await.expect("start shared cluster");
        let admin = connect(port, ADMIN_ROLE, &creds.admin, "postgres").await.expect("admin connect");
        admin.batch_execute("ALTER ROLE outlooker_ro CONNECTION LIMIT -1").await.expect("lift ro connection limit");
        port
    });
    atexit(stop_shared);
    Run { dir, port, creds }
}

impl TestCluster {
    /// A fresh migrated database on the shared cluster (the first caller boots the cluster).
    pub async fn start() -> TestCluster {
        Self::create("outlooker", None).await
    }

    /// Like `start`, but the database is a copy of a shared seeded database: `seed` runs once per process per `name`.
    pub async fn from_seed(name: &'static str, seed: fn(&TestCluster) -> BoxFuture<'_, ()>) -> TestCluster {
        let template = format!("seed_{name}");
        {
            let mut seeds = SEEDS.lock().unwrap_or_else(|p| p.into_inner());
            let seeds = seeds.get_or_insert_with(HashSet::new);
            if !seeds.contains(name) {
                block_on(async {
                    let tc = TestCluster::create("outlooker", Some(&template)).await;
                    seed(&tc).await;
                    tc.db.set(None);
                    tc.terminate_connections().await;
                });
                seeds.insert(name);
            }
        }
        Self::create(&template, None).await
    }

    /// Creates database `fixed` (when given) or a numbered one from `template`, and the handle for it.
    async fn create(template: &str, fixed: Option<&str>) -> TestCluster {
        let run = RUN.get_or_init(boot_shared);
        let dbname = fixed.map(str::to_owned).unwrap_or_else(|| format!("t{}", NEXT.fetch_add(1, Ordering::Relaxed)));
        let postgres = connect(run.port, ADMIN_ROLE, &run.creds.admin, "postgres").await.expect("admin connect");
        let mut tries = 0;
        loop {
            match postgres.batch_execute(&format!("CREATE DATABASE {dbname} TEMPLATE {template} STRATEGY FILE_COPY")).await {
                Ok(()) => break,
                // a just-closed seed connection may still be exiting
                Err(e) if e.code().is_some_and(|c| c.code() == "55006") && tries < 100 => {
                    tries += 1;
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
                Err(e) => panic!("create database {dbname}: {e}"),
            }
        }
        postgres
            .execute(
                "UPDATE pg_database SET datacl = (SELECT datacl FROM pg_database WHERE datname = $2) WHERE datname = $1",
                &[&dbname, &DB_NAME],
            )
            .await
            .expect("copy database grants");
        let dir = tempfile::tempdir().expect("temp dir");
        let paths = Paths::new(dir.path().join("app"), dir.path().join("logs"), dev_pg_dir()).with_data_root(dir.path().join("data"));
        std::fs::create_dir_all(paths.data_root()).expect("data root");
        run.creds.save(&paths.credentials()).expect("save credentials");
        let tc = TestCluster { dir, paths, port: run.port, creds: run.creds.clone(), db: Db::new(), dbname };
        tc.db.set(Some(tc.db_conn()));
        tc
    }

    async fn terminate_connections(&self) {
        let postgres = connect(self.port, ADMIN_ROLE, &self.creds.admin, "postgres").await.expect("admin connect");
        postgres
            .execute("SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = $1", &[&self.dbname])
            .await
            .expect("terminate backends");
    }

    pub async fn count(&self, sql: &str) -> i64 {
        count(&self.admin().await, sql).await
    }

    /// New pools for this test's database (also used to attach a Supervisor).
    pub fn db_conn(&self) -> DbConn {
        DbConn {
            port: self.port,
            creds: self.creds.clone(),
            pools: Pools::build(self.port, &self.creds, &self.dbname).expect("pools"),
            data_root: self.paths.data_root().to_path_buf(),
            dbname: self.dbname.clone(),
        }
    }

    pub async fn admin(&self) -> tokio_postgres::Client {
        connect(self.port, ADMIN_ROLE, &self.creds.admin, &self.dbname).await.expect("admin connect")
    }
    pub async fn app(&self) -> tokio_postgres::Client {
        connect(self.port, "outlooker_app", &self.creds.app, &self.dbname).await.expect("app connect")
    }
    pub async fn ro(&self) -> tokio_postgres::Client {
        connect(self.port, "outlooker_ro", &self.creds.ro, &self.dbname).await.expect("ro connect")
    }

    /// Simulates a database crash for this test only: pools dropped, new connections refused, backends terminated.
    pub async fn cut_db(&self) {
        self.db.set(None);
        let postgres = connect(self.port, ADMIN_ROLE, &self.creds.admin, "postgres").await.expect("admin connect");
        postgres.batch_execute(&format!("ALTER DATABASE {} ALLOW_CONNECTIONS false", self.dbname)).await.expect("disallow connections");
        self.terminate_connections().await;
    }

    pub async fn restore_db(&self) {
        let postgres = connect(self.port, ADMIN_ROLE, &self.creds.admin, "postgres").await.expect("admin connect");
        postgres.batch_execute(&format!("ALTER DATABASE {} ALLOW_CONNECTIONS true", self.dbname)).await.expect("allow connections");
        self.db.set(Some(self.db_conn()));
    }
}

impl Drop for TestCluster {
    fn drop(&mut self) {
        self.db.set(None);
    }
}
