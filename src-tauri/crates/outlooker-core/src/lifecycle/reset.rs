use std::path::{Path, PathBuf};

use crate::dto::app::AppPhase;
use crate::dto::error::AppError;
use crate::lifecycle::pg::{PgController, StopMode};
use crate::lifecycle::supervisor::Supervisor;

/// The real `pgdata` directory, refused unless it is a plain directory (not a symlink) directly inside the
/// canonical data root, so the delete can never leave the database directory.
fn resettable_pgdata(root: &Path, pgdata: &Path) -> Result<PathBuf, AppError> {
    let refuse = || AppError::file_access(pgdata, "notADatabaseDirectory");
    let meta = std::fs::symlink_metadata(pgdata).map_err(|e| AppError::file_access(pgdata, &e.to_string()))?;
    if !meta.is_dir() {
        return Err(refuse()); // a symlink or a file
    }
    let root = root.canonicalize().map_err(|e| AppError::file_access(root, &e.to_string()))?;
    let real = pgdata.canonicalize().map_err(|e| AppError::file_access(pgdata, &e.to_string()))?;
    if real.parent() != Some(root.as_path()) || real.file_name() != pgdata.file_name() {
        return Err(refuse());
    }
    Ok(real)
}

impl Supervisor {
    /// `app_reset_database`: only in `Failed` with `IncompatibleDatabase`. Stops PostgreSQL, deletes only the
    /// `pgdata` directory and the `import-inflight` file, then runs the first-run cluster creation on the same data root (marker rewritten,
    /// credentials reused). Ends in `Ready` (or `Failed`), like onboarding.
    pub async fn reset_database(&self) -> Result<(), AppError> {
        // Compare-and-set on the phase with the error read inside it, so a second reset or a retry racing this one is
        // refused and the error judged is the one of this Failed phase.
        let mut prev = None;
        let claimed = self.inner.phase_tx.send_if_modified(|p| {
            prev = self.inner.error.lock().unwrap_or_else(|e| e.into_inner()).clone();
            let ok = *p == AppPhase::Failed && matches!(prev, Some(AppError::IncompatibleDatabase { .. }));
            if ok {
                *p = AppPhase::InitializingCluster;
            }
            ok
        });
        if !claimed {
            return Err(AppError::invalid("phase", "notResettable"));
        }
        let paths = self.paths();
        let pgdata = paths.pgdata();
        let removed = async {
            let real = resettable_pgdata(paths.data_root(), &pgdata)?;
            // No postmaster.pid = no server on this dir. Skipping the stop lets a reset retry a partly deleted pgdata
            // (no PG_VERSION, where `pg_ctl status` fails).
            if real.join("postmaster.pid").exists() {
                PgController::new(paths.clone()).stop(&real, StopMode::Fast).await?;
            }
            // The import crash record names a job of the deleted database. Removed first (user-approved), so a fresh
            // job 1 never inherits it. remove_file on a symlink removes the link only.
            let inflight = paths.inflight_marker();
            match tokio::fs::remove_file(&inflight).await {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                    return Err(AppError::file_access(&inflight, &e.to_string()));
                }
                _ => {}
            }
            tokio::fs::remove_dir_all(&real).await.map_err(|e| AppError::file_access(&real, &e.to_string()))
        }
        .await;
        if let Err(e) = removed {
            log::error!("database reset refused or failed: {e}");
            self.set_phase(AppPhase::Failed, prev).await;
            return Err(e);
        }
        self.set_phase(AppPhase::InitializingCluster, None).await;
        let me = self.clone();
        tokio::spawn(async move { me.first_run(paths).await });
        Ok(())
    }
}

