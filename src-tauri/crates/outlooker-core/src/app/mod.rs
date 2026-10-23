//! App composition (spec Part 02 §2.2 steps 2–9, §2.5): builds every service, starts them in order, stops them.
//! The only module that imports every service; no service imports this module.
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::db::Db;
use crate::dto::app::AppPhase;
use crate::dto::error::AppError;
use crate::gate::Gate;
use crate::importer::{ImportManager, ImportTuning, RecoveryReport};
use crate::lifecycle::{Supervisor, SupervisorConfig};
use crate::maintenance::Maintenance;
use crate::paths::Paths;
use crate::search::SearchService;
use crate::settings::SettingsStore;
use crate::sink::EventSink;
use crate::sql_console::SqlConsole;
use crate::sync::Scheduler;

pub struct AppDirs {
    pub app_local_data_dir: PathBuf,
    pub app_log_dir: PathBuf,
    pub pg_dir: PathBuf,
}

#[derive(Default)]
pub struct BuildOptions {
    pub supervisor: SupervisorConfig,
    pub import_tuning: ImportTuning,
}

#[derive(Clone)]
pub struct Services {
    pub paths: Paths,
    pub db: Db,
    pub gate: Gate,
    pub sink: Arc<dyn EventSink>,
    pub settings: SettingsStore,
    pub supervisor: Supervisor,
    pub importer: ImportManager,
    pub search: SearchService,
    pub sql: SqlConsole,
    pub maintenance: Maintenance,
    pub scheduler: Scheduler,
}

/// Wires every service. No I/O except reading the bootstrap file; spawns nothing.
pub fn build(dirs: AppDirs, sink: Arc<dyn EventSink>, opts: BuildOptions) -> Result<Services, AppError> {
    let paths = Paths::new(dirs.app_local_data_dir, dirs.app_log_dir, dirs.pg_dir);
    let settings = SettingsStore::load(paths.bootstrap_file())?;
    let gate = Gate::new();
    let db = Db::new();
    let supervisor = Supervisor::new(paths.clone(), settings.clone(), gate.clone(), db.clone(), sink.clone(), opts.supervisor);
    let importer = ImportManager::new(
        db.clone(),
        gate.clone(),
        paths.clone(),
        sink.clone(),
        supervisor.phase_watch(),
        opts.import_tuning,
    );
    let search = SearchService::new(db.clone());
    let sql = SqlConsole::new(db.clone());
    let maintenance = Maintenance::new(db.clone(), gate.clone(), sink.clone());
    let scheduler = Scheduler::new(db.clone(), importer.clone(), sink.clone(), supervisor.phase_watch());
    Ok(Services { paths, db, gate, sink, settings, supervisor, importer, search, sql, maintenance, scheduler })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartStep {
    Recovered,
    IndexesEnsured,
    WorkerStarted,
    SchedulerStarted,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StartReport {
    pub steps: Vec<StartStep>,
    pub recovery: RecoveryReport,
}

/// Runs once per process. Later `Ready` transitions (PG restart, data dir move) are handled inside
/// `ImportManager` through the phase watch (C15).
pub async fn start_services(s: &Services) -> Result<StartReport, AppError> {
    if let Err(e) = s.supervisor.boot().await {
        // Phase is now Failed; `app_retry` recovers, and the wait below ends when it does.
        log::error!("boot failed: {e}");
    }
    // First run: this wait ends after onboarding's `app_initialize` reached Ready.
    s.supervisor
        .phase_watch()
        .wait_for(|p| *p == AppPhase::Ready)
        .await
        .map_err(|_| AppError::internal("supervisor dropped"))?;
    let mut steps = Vec::new();
    let recovery = s.importer.recover_after_crash().await?;
    steps.push(StartStep::Recovered);
    match s.importer.self_heal_indexes().await {
        Ok(()) => steps.push(StartStep::IndexesEnsured),
        // Search works without the indexes (slower), so the queue still starts.
        Err(e) => log::error!("search index self-heal failed: {e}"),
    }
    s.importer.start_worker();
    steps.push(StartStep::WorkerStarted);
    s.scheduler.start();
    steps.push(StartStep::SchedulerStarted);
    Ok(StartReport { steps, recovery })
}

#[derive(Debug, Clone, Copy)]
pub struct ShutdownLimits {
    pub import_drain: Duration,
}

impl Default for ShutdownLimits {
    fn default() -> Self {
        ShutdownLimits { import_drain: Duration::from_secs(15) }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShutdownReport {
    pub import_drained: bool,
    pub db_stopped: bool,
}

/// Part 02 §2.5: sync scheduler first (nothing is enqueued during the drain), importer (job → Queued, ≤ 15 s),
/// running SQL cancelled, then the database (pools closed, `pg_ctl stop -m fast -w -t 30`, immediate on timeout — inside the Supervisor). Never panics.
pub async fn shutdown(s: &Services, limits: ShutdownLimits) -> ShutdownReport {
    s.scheduler.stop().await;
    let import_drained = match s.importer.shutdown(limits.import_drain).await {
        Ok(()) => true,
        Err(e) => {
            log::warn!("import shutdown: {e}");
            false
        }
    };
    s.sql.cancel_running().await;
    let db_stopped = match s.supervisor.shutdown_db().await {
        Ok(()) => true,
        Err(e) => {
            log::warn!("database shutdown: {e}");
            false
        }
    };
    ShutdownReport { import_drained, db_stopped }
}
