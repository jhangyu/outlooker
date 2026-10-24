use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::watch;

#[cfg(feature = "test-support")]
use crate::db::DbConn;
use crate::db::{library_counts, Db};
use crate::dto::app::{AppPhase, AppStatus, LibraryCounts};
use crate::dto::error::AppError;
use crate::gate::Gate;
use crate::lifecycle::pg::{PgController, StopMode};
use crate::paths::Paths;
use crate::settings::SettingsStore;
use crate::sink::{CoreEvent, EventSink};

#[derive(Debug, Clone)]
pub struct SupervisorConfig {
    pub restart_budget: u32,
    pub restart_window: Duration,
    pub probe_interval: Duration,
}

impl Default for SupervisorConfig {
    fn default() -> Self {
        SupervisorConfig { restart_budget: 3, restart_window: Duration::from_secs(600), probe_interval: Duration::from_secs(5) }
    }
}

pub(crate) struct Inner {
    pub(crate) paths: Mutex<Paths>,
    pub(crate) settings: SettingsStore,
    pub(crate) gate: Gate,
    pub(crate) db: Db,
    pub(crate) sink: Arc<dyn EventSink>,
    pub(crate) config: SupervisorConfig,
    pub(crate) phase_tx: watch::Sender<AppPhase>,
    pub(crate) error: Mutex<Option<AppError>>,
    pub(crate) restarts: Mutex<Vec<Instant>>,
    pub(crate) health_started: AtomicBool,
    pub(crate) fail_next_verify: AtomicBool,
    #[cfg(feature = "test-support")]
    pub(crate) attached: Mutex<Option<DbConn>>,
}

/// Owns `AppPhase` (spec Part 02 §2.1). Imports no importer / search / SQL console code.
#[derive(Clone)]
pub struct Supervisor {
    pub(crate) inner: Arc<Inner>,
}

impl Supervisor {
    pub fn new(paths: Paths, settings: SettingsStore, gate: Gate, db: Db, sink: Arc<dyn EventSink>, config: SupervisorConfig) -> Supervisor {
        let (phase_tx, _) = watch::channel(AppPhase::NeedsSetup);
        Supervisor {
            inner: Arc::new(Inner {
                paths: Mutex::new(paths),
                settings,
                gate,
                db,
                sink,
                config,
                phase_tx,
                error: Mutex::new(None),
                restarts: Mutex::new(Vec::new()),
                health_started: AtomicBool::new(false),
                fail_next_verify: AtomicBool::new(false),
                #[cfg(feature = "test-support")]
                attached: Mutex::new(None),
            }),
        }
    }

    pub fn phase(&self) -> AppPhase {
        *self.inner.phase_tx.borrow()
    }

    pub fn phase_watch(&self) -> watch::Receiver<AppPhase> {
        self.inner.phase_tx.subscribe()
    }

    pub fn require_ready(&self) -> Result<(), AppError> {
        if self.phase() == AppPhase::Ready { Ok(()) } else { Err(AppError::DbNotReady) }
    }

    pub(crate) fn paths(&self) -> Paths {
        self.inner.paths.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    pub(crate) fn set_paths(&self, paths: Paths) {
        *self.inner.paths.lock().unwrap_or_else(|p| p.into_inner()) = paths;
    }

    pub(crate) async fn set_phase(&self, phase: AppPhase, error: Option<AppError>) {
        *self.inner.error.lock().unwrap_or_else(|p| p.into_inner()) = error;
        self.inner.phase_tx.send_replace(phase);
        let status = self.status().await;
        self.inner.sink.emit(CoreEvent::AppStatusChanged(status));
    }

    async fn counts(&self) -> Result<LibraryCounts, AppError> {
        library_counts(&*self.inner.db.app_client().await?).await
    }

    pub async fn status(&self) -> AppStatus {
        let phase = self.phase();
        let boot = self.inner.settings.bootstrap();
        let data_dir = boot.data_dir.clone().unwrap_or_else(|| self.paths().default_data_root().display().to_string());
        let counts = if phase == AppPhase::Ready { self.counts().await.unwrap_or_default() } else { LibraryCounts::default() };
        AppStatus { phase, data_dir, language: boot.language, counts, error: self.inner.error.lock().unwrap_or_else(|p| p.into_inner()).clone() }
    }

    /// App start (spec Part 02 §2.2 steps 2–7).
    pub async fn boot(&self) -> Result<(), AppError> {
        let Some(dir) = self.inner.settings.bootstrap().data_dir else {
            self.set_phase(AppPhase::NeedsSetup, None).await;
            return Ok(());
        };
        self.set_paths(self.paths().with_data_root(PathBuf::from(dir)));
        if !self.paths().marker().exists() {
            self.set_phase(AppPhase::NeedsSetup, None).await;
            return Ok(());
        }
        self.start_cluster().await
    }

    /// `app_retry` (spec Part 03 §4).
    pub async fn retry(&self) -> Result<(), AppError> {
        let paths = self.paths();
        // A marker without pgdata: cluster creation (first run or reset) failed after the marker was written. Create
        // the cluster again on this root; onboarding would adopt the root and fail on the missing cluster.
        let target = match (paths.pgdata().exists(), paths.marker().exists()) {
            (true, _) => AppPhase::StartingDb,
            (false, true) => AppPhase::InitializingCluster,
            (false, false) => AppPhase::NeedsSetup,
        };
        // Compare-and-set, so a retry racing a reset (or another retry) is refused.
        let claimed = self.inner.phase_tx.send_if_modified(|p| {
            let ok = *p == AppPhase::Failed;
            if ok {
                *p = target;
            }
            ok
        });
        if !claimed {
            return Err(AppError::invalid("phase", "notFailed"));
        }
        self.inner.restarts.lock().unwrap_or_else(|p| p.into_inner()).clear();
        self.set_phase(target, None).await;
        let me = self.clone();
        match target {
            AppPhase::StartingDb => {
                tokio::spawn(async move {
                    let _ = me.start_cluster().await;
                });
            }
            AppPhase::InitializingCluster => {
                tokio::spawn(async move { me.first_run(paths).await });
            }
            _ => {}
        }
        Ok(())
    }

    /// Spec Part 02 §2.5 steps 3–4 (the importer and SQL console are stopped before this by `core::app`).
    pub async fn shutdown_db(&self) -> Result<(), AppError> {
        self.set_phase(AppPhase::StoppingDb, None).await;
        self.inner.db.set(None);
        #[cfg(feature = "test-support")]
        if self.inner.attached.lock().unwrap_or_else(|p| p.into_inner()).is_some() {
            return Ok(());
        }
        let p = self.paths();
        PgController::new(p.clone()).stop(&p.pgdata(), StopMode::Fast).await
    }

    /// Test seam: run on a database of a shared cluster; PostgreSQL itself is never started or stopped.
    #[cfg(feature = "test-support")]
    pub fn attach_test_db(&self, c: DbConn) {
        *self.inner.attached.lock().unwrap_or_else(|p| p.into_inner()) = Some(c);
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn fail_next_move_verify(&self) {
        self.inner.fail_next_verify.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}
