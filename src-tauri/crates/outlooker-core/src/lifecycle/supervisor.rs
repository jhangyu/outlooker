use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::watch;

use crate::db::{pg_err, Db};
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
        self.inner.paths.lock().unwrap().clone()
    }

    pub(crate) fn set_paths(&self, paths: Paths) {
        *self.inner.paths.lock().unwrap() = paths;
    }

    pub(crate) async fn set_phase(&self, phase: AppPhase, error: Option<AppError>) {
        *self.inner.error.lock().unwrap() = error;
        self.inner.phase_tx.send_replace(phase);
        let status = self.status().await;
        self.inner.sink.emit(CoreEvent::AppStatusChanged(status));
    }

    async fn counts(&self) -> Result<LibraryCounts, AppError> {
        let c = self.inner.db.app_pool()?.get().await.map_err(|e| AppError::internal(e.to_string()))?;
        let r = c
            .query_one("SELECT (SELECT count(*) FROM messages), (SELECT count(*) FROM pst_sources)", &[])
            .await
            .map_err(pg_err)?;
        Ok(LibraryCounts { messages: r.get(0), psts: r.get(1) })
    }

    pub async fn status(&self) -> AppStatus {
        let phase = self.phase();
        let boot = self.inner.settings.bootstrap();
        let data_dir = boot.data_dir.clone().unwrap_or_else(|| self.paths().default_data_root().display().to_string());
        let counts = if phase == AppPhase::Ready { self.counts().await.unwrap_or_default() } else { LibraryCounts::default() };
        AppStatus { phase, data_dir, language: boot.language, counts, error: self.inner.error.lock().unwrap().clone() }
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
        if self.phase() != AppPhase::Failed {
            return Err(AppError::invalid("phase", "notFailed"));
        }
        self.inner.restarts.lock().unwrap().clear();
        if self.paths().pgdata().exists() {
            self.set_phase(AppPhase::StartingDb, None).await;
            let me = self.clone();
            tokio::spawn(async move {
                let _ = me.start_cluster().await;
            });
        } else {
            self.set_phase(AppPhase::NeedsSetup, None).await;
        }
        Ok(())
    }

    /// Spec Part 02 §2.5 steps 3–4 (the importer and SQL console are stopped before this by `core::app`).
    pub async fn shutdown_db(&self) -> Result<(), AppError> {
        self.set_phase(AppPhase::StoppingDb, None).await;
        self.inner.db.set(None);
        let p = self.paths();
        PgController::new(p.clone()).stop(&p.pgdata(), StopMode::Fast).await
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn fail_next_move_verify(&self) {
        self.inner.fail_next_verify.store(true, Ordering::SeqCst);
    }
}
