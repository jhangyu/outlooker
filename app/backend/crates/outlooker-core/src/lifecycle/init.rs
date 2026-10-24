use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::db::Credentials;
use crate::dto::app::{AppPhase, InitializeRequest};
use crate::dto::error::AppError;
use crate::lifecycle::pg::{PgController, StopMode};
use crate::lifecycle::supervisor::Supervisor;
use crate::paths::Paths;
use crate::fsutil::write_atomic;

/// RFC 3339 UTC for a SystemTime (days-from-civil inverse, Howard Hinnant), so no date crate is needed.
pub fn rfc3339_utc(t: SystemTime) -> String {
    let secs = t.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

fn write_marker(paths: &Paths, locale: &str) -> Result<(), AppError> {
    let body = serde_json::json!({
        "format_version": 1,
        "created_at": rfc3339_utc(SystemTime::now()),
        "pg_major": 18,
        "locale": locale,
    });
    write_atomic(&paths.marker(), &serde_json::to_vec_pretty(&body).expect("marker serializes"), false)
}

fn is_empty_dir(dir: &Path) -> bool {
    std::fs::read_dir(dir).map(|mut d| d.next().is_none()).unwrap_or(false)
}

impl Supervisor {
    pub async fn initialize(&self, req: InitializeRequest) -> Result<(), AppError> {
        if self.phase() != AppPhase::NeedsSetup {
            return Err(AppError::invalid("phase", "alreadyInitialized"));
        }
        let target = PathBuf::from(&req.data_dir);
        if !target.is_absolute() {
            return Err(AppError::invalid("dataDir", "notAbsolute"));
        }
        let paths = self.paths().with_data_root(target.clone());
        let adopt = paths.marker().exists();
        if !adopt && target.exists() && !is_empty_dir(&target) {
            return Err(AppError::invalid("dataDir", "notEmpty"));
        }
        std::fs::create_dir_all(&target).map_err(|e| AppError::file_access(&target, &e.to_string()))?;
        let probe = target.join(".outlooker-write-test");
        std::fs::write(&probe, b"ok").map_err(|_| AppError::file_access(&target, "notWritable"))?;
        let _ = std::fs::remove_file(&probe);
        self.inner.settings.set_bootstrap(Some(req.data_dir.clone()), req.language)?;
        self.set_paths(paths.clone());
        let me = self.clone();
        if adopt {
            tokio::spawn(async move {
                let _ = me.start_cluster().await;
            });
            return Ok(());
        }
        self.set_phase(AppPhase::InitializingCluster, None).await;
        tokio::spawn(async move { me.first_run(paths).await });
        Ok(())
    }

    /// Creates the cluster in `paths.pgdata()` (absent) and starts it. Existing credentials are reused (a database
    /// reset keeps `auth/`), else new ones are generated and saved.
    pub(crate) async fn first_run(&self, paths: Paths) {
        let pgdata = paths.pgdata();
        let created = !pgdata.exists();
        let cleanup = |paths: &Paths| {
            let pgdata = paths.pgdata();
            async move {
                if created {
                    let _ = tokio::fs::remove_dir_all(&pgdata).await;
                }
            }
        };
        let prepared = async {
            let creds = match Credentials::load(&paths.credentials()) {
                Ok(c) => c,
                Err(_) => {
                    let c = Credentials::generate();
                    c.save(&paths.credentials())?;
                    c
                }
            };
            let locale = PgController::new(paths.clone()).init(&pgdata, &creds.admin).await?;
            write_marker(&paths, &locale)
        }
        .await;
        if let Err(e) = prepared {
            cleanup(&paths).await;
            self.set_phase(AppPhase::Failed, Some(e)).await;
            return;
        }
        if self.start_cluster().await.is_err() {
            let _ = PgController::new(paths.clone()).stop(&pgdata, StopMode::Immediate).await;
            cleanup(&paths).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc3339_known_instants() {
        assert_eq!(rfc3339_utc(UNIX_EPOCH), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339_utc(UNIX_EPOCH + std::time::Duration::from_secs(1_714_552_200)), "2024-05-01T08:30:00Z");
        assert_eq!(rfc3339_utc(UNIX_EPOCH + std::time::Duration::from_secs(951_782_400)), "2000-02-29T00:00:00Z");
    }
}
