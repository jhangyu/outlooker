//! Real-PostgreSQL test cluster (spec Part 01 §9): production initdb flags, migrated, both pools.
pub mod import_fixtures;

use tempfile::TempDir;

use crate::db::{connect, connect_admin, migrate, Credentials, Db, DbConn, Pools, DB_NAME};
use crate::lifecycle::pg::{PgController, StopMode};
use crate::paths::{dev_pg_dir, Paths};

pub struct TestCluster {
    pub dir: TempDir,
    pub paths: Paths,
    pub port: u16,
    pub creds: Credentials,
    pub db: Db,
    pub locale: String,
    ctl: PgController,
}

impl TestCluster {
    pub async fn start() -> TestCluster {
        assert!(dev_pg_dir().join("bin").exists(), "PostgreSQL binaries missing: run node scripts/fetch-postgres.mjs");
        let dir = tempfile::tempdir().expect("temp dir");
        let paths = Paths::new(dir.path().join("app"), dir.path().join("logs"), dev_pg_dir()).with_data_root(dir.path().join("data"));
        let ctl = PgController::new(paths.clone());
        let creds = Credentials::generate();
        let locale = ctl.init(&paths.pgdata(), &creds.admin).await.expect("initdb");
        creds.save(&paths.credentials()).expect("save credentials");
        let port = ctl.start(&paths.pgdata()).await.expect("start");
        let mut admin = connect_admin(port, &creds).await.expect("admin connect");
        migrate(&mut admin, &creds).await.expect("migrate");
        let db = Db::new();
        db.set(Some(DbConn { port, creds: creds.clone(), pools: Pools::build(port, &creds).expect("pools"), data_root: paths.data_root().to_path_buf() }));
        TestCluster { dir, paths, port, creds, db, locale, ctl }
    }

    pub fn controller(&self) -> &PgController {
        &self.ctl
    }
    pub async fn admin(&self) -> tokio_postgres::Client {
        connect_admin(self.port, &self.creds).await.expect("admin connect")
    }
    pub async fn app(&self) -> tokio_postgres::Client {
        connect(self.port, "outlooker_app", &self.creds.app, DB_NAME).await.expect("app connect")
    }
    pub async fn ro(&self) -> tokio_postgres::Client {
        connect(self.port, "outlooker_ro", &self.creds.ro, DB_NAME).await.expect("ro connect")
    }
    pub async fn kill(&self) {
        self.db.set(None);
        self.ctl.stop(&self.paths.pgdata(), StopMode::Immediate).await.expect("stop immediate");
    }
    pub async fn restart(&mut self) {
        self.port = self.ctl.start(&self.paths.pgdata()).await.expect("restart");
        self.db.set(Some(DbConn {
            port: self.port,
            creds: self.creds.clone(),
            pools: Pools::build(self.port, &self.creds).expect("pools"),
            data_root: self.paths.data_root().to_path_buf(),
        }));
    }
}

impl Drop for TestCluster {
    fn drop(&mut self) {
        self.db.set(None);
        let _ = std::process::Command::new(self.paths.pg_bin("pg_ctl"))
            .args(["stop", "-m", "immediate", "-w", "-D"])
            .arg(self.paths.pgdata())
            .output();
    }
}
