//! Maintenance Service (spec Part 03 §8.3, Part 04 §9.1): analyze / vacuum / reindex on an admin connection.
use std::sync::{Arc, Mutex};

use tokio::sync::Notify;

use crate::db::indexes::ensure_search_indexes;
use crate::db::{db_err, Db, RFC3339_UTC_FMT};
use crate::dto::error::AppError;
use crate::dto::events::{MaintenanceRunState, MaintenanceState};
use crate::dto::settings::MaintenanceTask;
use crate::gate::{Activity, Gate};
use crate::sink::{CoreEvent, EventSink};

/// Every index created by Part 04 §3, in this order (REINDEX order of the `reindex` task).
pub const REINDEX_INDEXES: [&str; 17] = [
    "import_jobs_one_active_per_pst",
    "import_jobs_one_running",
    "import_jobs_pst_newest",
    "import_jobs_queue",
    "import_errors_page",
    "occurrences_message",
    "occurrences_folder",
    "recipients_address",
    "inline_images_blob",
    "messages_sender_address",
    "messages_first_pst",
    "messages_sort",
    "messages_body_trgm",
    "messages_subject_trgm",
    "messages_sender_trgm",
    "messages_recipients_trgm",
    "messages_body_fts",
];

/// Invalid leftovers of failed concurrent rebuilds, already quoted by the server.
const CCNEW_LEFTOVERS: &str = "SELECT quote_ident(n.nspname) || '.' || quote_ident(c.relname) \
     FROM pg_index x JOIN pg_class c ON c.oid = x.indexrelid JOIN pg_namespace n ON n.oid = c.relnamespace \
     WHERE n.nspname = 'public' AND NOT x.indisvalid AND c.relname ~ '_ccnew[0-9]*$' ORDER BY c.relname";
const NOW_TEXT: &str = "SELECT to_char(now() AT TIME ZONE 'UTC', $1)";

#[derive(Clone)]
pub struct Maintenance {
    db: Db,
    gate: Gate,
    sink: Arc<dyn EventSink>,
    current: Arc<Mutex<Option<MaintenanceState>>>,
    idle: Arc<Notify>,
}

async fn now_text(admin: &tokio_postgres::Client) -> Result<String, AppError> {
    Ok(admin.query_one(NOW_TEXT, &[&RFC3339_UTC_FMT]).await.map_err(db_err)?.get(0))
}

async fn run_task(admin: &tokio_postgres::Client, task: MaintenanceTask) -> Result<(), AppError> {
    match task {
        MaintenanceTask::Analyze => admin.batch_execute("ANALYZE").await.map_err(db_err),
        MaintenanceTask::Vacuum => {
            admin.batch_execute("VACUUM (ANALYZE)").await.map_err(db_err)?;
            let (blobs, addresses) = crate::importer::remove::delete_orphans(admin).await?;
            log::info!("maintenance vacuum: {blobs} orphan blobs, {addresses} orphan addresses deleted");
            Ok(())
        }
        MaintenanceTask::Reindex => {
            ensure_search_indexes(admin, &|name: &str, done: usize, total: usize| {
                log::info!("maintenance ensure index {name} ({done}/{total})")
            })
            .await?;
            let leftovers = admin.query(CCNEW_LEFTOVERS, &[]).await.map_err(db_err)?;
            for row in leftovers {
                let name: String = row.get(0); // catalog name quoted by quote_ident, not a user value
                admin.batch_execute(&format!("DROP INDEX CONCURRENTLY {name}")).await.map_err(db_err)?;
                log::info!("maintenance dropped leftover index {name}");
            }
            for (i, name) in REINDEX_INDEXES.iter().enumerate() {
                admin.batch_execute(&format!("REINDEX INDEX CONCURRENTLY {name}")).await.map_err(db_err)?;
                log::info!("maintenance reindex {name} ({}/{})", i + 1, REINDEX_INDEXES.len());
            }
            Ok(())
        }
    }
}

impl Maintenance {
    pub fn new(db: Db, gate: Gate, sink: Arc<dyn EventSink>) -> Maintenance {
        Maintenance { db, gate, sink, current: Arc::new(Mutex::new(None)), idle: Arc::new(Notify::new()) }
    }

    fn set_current(&self, state: Option<MaintenanceState>) {
        *self.current.lock().unwrap_or_else(|p| p.into_inner()) = state;
    }

    /// maintenance_run: enters the gate (Busy on conflict), emits `running`, spawns the task, returns at once.
    pub async fn run(&self, task: MaintenanceTask) -> Result<(), AppError> {
        let guard = self.gate.try_enter(Activity::Maintenance)?;
        let admin = self.db.connect_admin().await?;
        let started_at = now_text(&admin).await?;
        let running = MaintenanceState { task, state: MaintenanceRunState::Running, started_at, finished_at: None, error: None };
        self.set_current(Some(running.clone()));
        self.sink.emit(CoreEvent::MaintenanceStatusChanged(running.clone()));
        let this = self.clone();
        tokio::spawn(async move {
            let result = run_task(&admin, task).await;
            let finished_at = now_text(&admin).await.ok();
            let (state, error) = match result {
                Ok(()) => (MaintenanceRunState::Done, None),
                Err(e) => {
                    log::warn!("maintenance {task:?} failed: {e}");
                    (MaintenanceRunState::Failed, Some(e))
                }
            };
            this.sink.emit(CoreEvent::MaintenanceStatusChanged(MaintenanceState { state, finished_at, error, ..running }));
            drop(guard);
            this.set_current(None);
            this.idle.notify_waiters(); // wait_idle returns after the end event, the gate is free
        });
        Ok(())
    }

    /// The running task's state (None when idle).
    pub fn current(&self) -> Option<MaintenanceState> {
        self.current.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// Resolves when no task runs.
    pub async fn wait_idle(&self) {
        loop {
            let notified = self.idle.notified();
            if self.current().is_none() {
                return;
            }
            notified.await;
        }
    }
}
