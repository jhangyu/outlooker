//! Import Manager (spec Part 02 §3.1, §4; Part 03 §7; Part 04 §2.1, §5, §6.4, §6.5).
pub mod dedupe;
pub mod inflight;
pub mod normalize;
mod pipeline;
pub mod probe;
pub mod progress;
pub mod pst_reader;
pub mod queue;
pub mod remove;
mod runner;
pub mod types;
pub mod webp;
pub mod writer;

use std::sync::Arc;

use tokio::sync::{broadcast, watch};

use crate::db::Db;
use crate::dto::app::AppPhase;
use crate::dto::error::AppError;
use crate::dto::import::{EnqueueResult, ImportErrorPage, ImportErrorsRequest, ImportStatus, PstSource};
use crate::gate::Gate;
use crate::paths::Paths;
use crate::sink::EventSink;
use writer::WriterLimits;

#[derive(Clone, Default)]
pub struct ImportTuning {
    /// Production: WriterLimits::default().
    pub writer: WriterLimits,
    /// Test seam: after this many committed batches of one run the job is dropped like a process kill
    /// (no state change, marker left on disk, writer aborted, the queue worker stops). None in production.
    pub crash_after_batches: Option<i64>,
    /// Test seam: in the first job attempt of this ImportManager the reader blocks after sending the item whose
    /// seq equals this value until the job is cancelled (user cancel, shutdown or DB pause); later attempts
    /// (re-run after a pause, resumed job, next job) never block. None in production.
    pub hold_reader_after: Option<i64>,
    /// Test seam: called after every commit with batches_committed (writer task; must not block).
    pub on_batch: Option<Arc<dyn Fn(i64) + Send + Sync>>,
}


/// Which registered paths one `ImportManager::sync` call looks at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncTarget {
    /// Every path with `sync_enabled`.
    All,
    /// This path, even when disabled (explicit user action).
    One(i64),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RecoveryReport {
    pub requeued: Vec<i64>,
    pub cancelled: Vec<i64>,
}

#[derive(Clone)]
pub struct ImportManager {
    inner: Arc<queue::Inner>,
}

impl ImportManager {
    pub fn new(db: Db, gate: Gate, paths: Paths, sink: Arc<dyn EventSink>, phase: watch::Receiver<AppPhase>, tuning: ImportTuning) -> ImportManager {
        ImportManager { inner: Arc::new(queue::Inner::new(db, gate, paths, sink, phase, tuning)) }
    }
    pub async fn list(&self) -> Result<Vec<PstSource>, AppError> {
        queue::list(&self.inner, None).await
    }
    /// Registers PST paths (one source row per path): readable → queued import, held by another program → `locked`
    /// job, already registered → Rejected{AlreadyImported}, other probe errors → Rejected.
    pub async fn register(&self, paths: Vec<String>) -> Result<Vec<EnqueueResult>, AppError> {
        queue::register(&self.inner, paths).await
    }
    /// One sync step per target path: skip while a job is active, unchanged → only `last_checked_at`, else probe and
    /// queue a job, or record the locked / unavailable streak. Paths run concurrently; kicks the worker per queued job.
    pub async fn sync(&self, target: SyncTarget) -> Result<(), AppError> {
        queue::sync(&self.inner, target).await
    }
    /// Disabled paths keep their mail; `SyncTarget::All` skips them. A queued or running job is not cancelled.
    pub async fn set_enabled(&self, pst_id: i64, enabled: bool) -> Result<PstSource, AppError> {
        queue::set_enabled(&self.inner, pst_id, enabled).await
    }
    pub async fn cancel(&self, pst_id: i64) -> Result<(), AppError> {
        queue::cancel(&self.inner, pst_id).await
    }
    pub async fn remove(&self, pst_id: i64) -> Result<(), AppError> {
        queue::remove(&self.inner, pst_id).await
    }
    pub async fn errors(&self, req: ImportErrorsRequest) -> Result<ImportErrorPage, AppError> {
        queue::errors(&self.inner, req).await
    }
    /// Part 02 §2.2 step 8: running → queued, cancelling → cancelled.
    pub async fn recover_after_crash(&self) -> Result<RecoveryReport, AppError> {
        queue::recover_after_crash(&self.inner).await
    }
    /// Part 02 §2.2 step 9: holds gate Import, ensure_search_indexes on an admin connection.
    pub async fn self_heal_indexes(&self) -> Result<(), AppError> {
        queue::self_heal_indexes(&self.inner).await
    }
    /// Spawns the queue worker once (later calls do nothing).
    pub fn start_worker(&self) {
        queue::start_worker(&self.inner);
    }
    /// Part 02 §2.5 step 1: running job → cancel flag "shutdown", drain, last batch + checkpoint committed,
    /// state 'queued'. Waits at most `timeout`; Err(Internal{"import shutdown timed out"}) when it did not stop.
    pub async fn shutdown(&self, timeout: std::time::Duration) -> Result<(), AppError> {
        queue::shutdown(&self.inner, timeout).await
    }
    /// Every terminal job outcome as (pst_id, status): Completed, Cancelled, Failed, Locked, Queued (after shutdown),
    /// Removing (the removal deleted the source row), Running (test seam `crash_after_batches` dropped the job).
    pub fn job_events(&self) -> broadcast::Receiver<(i64, ImportStatus)> {
        self.inner.events.subscribe()
    }
}

#[cfg(feature = "test-support")]
impl ImportManager {
    /// Runs one import job whose items come from `items` instead of a PST (the stages after the reader are the
    /// production ones). Creates the source row, the job, the folders. Returns pst_id after the job ended.
    pub async fn import_synthetic(&self, path: &str, folders: Vec<types::FolderIn>, items: Vec<types::RawItem>) -> Result<i64, AppError> {
        let mut events = self.job_events();
        let pst_id = self.enqueue_synthetic(path, folders, items).await?;
        self.start_worker();
        loop {
            match events.recv().await {
                Ok((id, _)) if id == pst_id => return Ok(pst_id),
                Ok(_) => {}
                Err(e) => return Err(AppError::internal(format!("job events: {e}"))),
            }
        }
    }
    /// Like `import_synthetic` but returns right after the job row is queued. The items are registered under
    /// `path` for every ImportManager of this process, so a re-synced or recovered job reads them again.
    /// `path` must be a file starting with the PST magic: its size and mtime are the source identity.
    pub async fn enqueue_synthetic(&self, path: &str, folders: Vec<types::FolderIn>, items: Vec<types::RawItem>) -> Result<i64, AppError> {
        queue::enqueue_synthetic(&self.inner, path, folders, items).await
    }
    /// Replaces the synthetic items read for `path` by the next job (a changed PST for a sync test).
    pub fn set_synthetic_items(path: &str, folders: Vec<types::FolderIn>, items: Vec<types::RawItem>) {
        queue::set_synthetic_items(path, folders, items);
    }
}
