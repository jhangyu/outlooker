use crate::db::{cjk_self_check, connect_admin, migrate, pg_err, Credentials, DbConn, Pools};
use crate::dto::app::AppPhase;
use crate::dto::error::AppError;
use crate::lifecycle::pg::{PgController, StopMode};
use crate::lifecycle::supervisor::Supervisor;
use crate::paths::Paths;

impl Supervisor {
    /// Starts the cluster of the current data root and ends in `Ready` or `Failed`.
    pub(crate) async fn start_cluster(&self) -> Result<(), AppError> {
        let paths = self.paths();
        match self.try_start(&paths).await {
            Ok(conn) => {
                self.inner.db.set(Some(conn));
                self.set_phase(AppPhase::Ready, None).await;
                self.spawn_health_monitor();
                Ok(())
            }
            Err(e) => {
                self.inner.db.set(None);
                self.set_phase(AppPhase::Failed, Some(e.clone())).await;
                Err(e)
            }
        }
    }

    async fn try_start(&self, paths: &Paths) -> Result<DbConn, AppError> {
        if !paths.marker().exists() {
            return Err(AppError::file_access(&paths.marker(), "notADataRoot"));
        }
        let version = std::fs::read_to_string(paths.pgdata().join("PG_VERSION"))
            .map_err(|e| AppError::file_access(&paths.pgdata(), &e.to_string()))?;
        if version.trim() != "18" {
            return Err(AppError::internal(format!("unsupported PostgreSQL data version {}", version.trim())));
        }
        self.set_phase(AppPhase::StartingDb, None).await;
        let ctl = PgController::new(paths.clone());
        let port = ctl.start(&paths.pgdata()).await?;
        let connected = async {
            let creds = Credentials::load(&paths.credentials())?;
            let mut admin = connect_admin(port, &creds).await?;
            admin.query_one("SELECT 1", &[]).await.map_err(pg_err)?;
            migrate(&mut admin, &creds).await?;
            cjk_self_check(&admin).await?;
            let pools = Pools::build(port, &creds)?;
            Ok::<DbConn, AppError>(DbConn { port, creds, pools, data_root: paths.data_root().to_path_buf() })
        }
        .await;
        if connected.is_err() {
            let _ = ctl.stop(&paths.pgdata(), StopMode::Fast).await;
        }
        connected
    }
}
