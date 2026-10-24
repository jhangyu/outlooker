//! A persistent data root (spec Part 02 §8.2 layout) built only from public foundation APIs, so the app can adopt
//! it and `LibraryRoot::open` can start it again.
use std::path::Path;
use std::time::SystemTime;

use outlooker_core::db::{connect_admin, migrate, Credentials, Db, DbConn, Pools, DB_NAME};
use outlooker_core::dto::error::AppError;
use outlooker_core::importer::dedupe::format_utc_seconds;
use outlooker_core::lifecycle::pg::{PgController, StopMode};
use outlooker_core::paths::{dev_pg_dir, Paths};

pub struct LibraryRoot {
    pub paths: Paths,
    pub port: u16,
    pub creds: Credentials,
    pub db: Db,
}

fn paths_for(data_root: &Path) -> Paths {
    // the app-local and log dirs are never written by the load generator
    Paths::new(data_root.to_path_buf(), data_root.to_path_buf(), dev_pg_dir()).with_data_root(data_root.to_path_buf())
}

async fn connect(paths: Paths, creds: Credentials) -> Result<LibraryRoot, AppError> {
    let port = PgController::new(paths.clone()).start(&paths.pgdata()).await?;
    let mut admin = connect_admin(port, &creds).await?;
    migrate(&mut admin, &creds).await?;
    let db = Db::new();
    db.set(Some(DbConn { port, creds: creds.clone(), pools: Pools::build(port, &creds, DB_NAME)?, data_root: paths.data_root().to_path_buf(), dbname: DB_NAME.to_owned() }));
    Ok(LibraryRoot { paths, port, creds, db })
}

impl LibraryRoot {
    /// Marker + credentials + initdb + start + migrate + pools. `data_root` must not contain a marker.
    pub async fn create(data_root: &Path) -> Result<LibraryRoot, AppError> {
        let paths = paths_for(data_root);
        if paths.marker().exists() {
            return Err(AppError::invalid("dataRoot", "notEmpty"));
        }
        std::fs::create_dir_all(data_root).map_err(|e| AppError::file_access(data_root, &e.to_string()))?;
        let creds = Credentials::generate();
        creds.save(&paths.credentials())?;
        let locale = PgController::new(paths.clone()).init(&paths.pgdata(), &creds.admin).await?;
        let marker = serde_json::json!({
            "format_version": 1,
            "created_at": format_utc_seconds(SystemTime::now()),
            "pg_major": 18,
            "locale": locale,
        });
        std::fs::write(paths.marker(), serde_json::to_vec_pretty(&marker).expect("marker json"))
            .map_err(|e| AppError::file_access(&paths.marker(), &e.to_string()))?;
        connect(paths, creds).await
    }

    /// Start + credentials + migrate + pools of an existing root.
    pub async fn open(data_root: &Path) -> Result<LibraryRoot, AppError> {
        let paths = paths_for(data_root);
        if !paths.marker().exists() {
            return Err(AppError::file_access(&paths.marker(), "notFound"));
        }
        let creds = Credentials::load(&paths.credentials())?;
        connect(paths, creds).await
    }

    /// Db unset, then `pg_ctl stop -m fast`.
    pub async fn stop(self) -> Result<(), AppError> {
        self.db.set(None);
        PgController::new(self.paths.clone()).stop(&self.paths.pgdata(), StopMode::Fast).await
    }
}
