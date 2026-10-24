//! Shared real-PostgreSQL test fixture: ONE cluster per test run, booted by `examples/test_pg_server.rs` (spawned by
//! `app/scripts/cargo-test-runner.mjs`, connection in `OUTLOOKER_TEST_PG`). Tests only connect and get one fresh
//! `WAL_LOG` database each. Production initdb flags, migrated, both pools.
pub mod import_fixtures;
mod template;

use std::collections::HashSet;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

use tempfile::TempDir;

use crate::db::{connect, Credentials, Db, DbConn, Pools, ADMIN_ROLE, DB_NAME};
use crate::paths::{dev_pg_dir, Paths};

pub use template::seeded_root;

/// Builds the template data root (files only, no cluster). The cargo test runner calls it uncapped
/// so a cold template build never lands inside a capped test.
pub fn warm_template() {
    template::template_root();
}

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

struct Run {
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

/// The shared cluster's connection from `OUTLOOKER_TEST_PG` = `<port>\t<run dir>`, set by the cargo test runner.
fn shared() -> Run {
    let v = std::env::var("OUTLOOKER_TEST_PG")
        .expect("OUTLOOKER_TEST_PG is not set: run through cargo test (the runner boots the shared cluster)");
    let (port, dir) = v.split_once('\t').expect("OUTLOOKER_TEST_PG is <port><TAB><run dir>");
    let dir = PathBuf::from(dir);
    let paths = Paths::new(dir.join("app"), dir.join("logs"), dev_pg_dir()).with_data_root(dir);
    let creds = Credentials::load(&paths.credentials()).expect("shared cluster credentials");
    Run { port: port.parse().expect("OUTLOOKER_TEST_PG port"), creds }
}

impl TestCluster {
    /// A fresh migrated database on the shared cluster.
    pub async fn start() -> TestCluster {
        Self::create("outlooker", None).await
    }

    /// Like `start`, but the database is a copy of a shared seeded database: `seed` runs once per process per `name`.
    pub async fn from_seed(name: &'static str, seed: fn(&TestCluster) -> BoxFuture<'_, ()>) -> TestCluster {
        // per process: the runner's solo rerun of a slow test is a second process on the same cluster
        let template = format!("seed_{name}_{}", std::process::id());
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
        let run = RUN.get_or_init(shared);
        let dbname = fixed.map(str::to_owned).unwrap_or_else(|| format!("t{}_{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        let postgres = connect(run.port, ADMIN_ROLE, &run.creds.admin, "postgres").await.expect("admin connect");
        let mut tries = 0;
        loop {
            match postgres.batch_execute(&format!("CREATE DATABASE {dbname} TEMPLATE {template} STRATEGY WAL_LOG")).await {
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
