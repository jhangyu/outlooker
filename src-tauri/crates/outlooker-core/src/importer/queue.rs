//! Durable job queue (spec Part 03 §7.1 state machine): job state, SQL commands, list, errors, recovery, worker
//! start / shutdown. The job runner is `runner.rs`, one import attempt is `pipeline.rs`.
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use tokio::sync::{Notify, broadcast, watch};
use tokio_postgres::error::SqlState;

use crate::db::indexes::ensure_search_indexes;
use crate::db::{db_err, Db, RFC3339_UTC_FMT};
use crate::dto::app::AppPhase;
use crate::dto::error::AppError;
use crate::dto::events::ImportStatusChanged;
use crate::dto::import::{
    db_text, EnqueueResult, ImportErrorEntry, ImportErrorPage, ImportErrorSeverity, ImportErrorStage, ImportErrorsRequest, ImportPhase,
    ImportProgress, ImportStatus, PstSource,
};
use crate::gate::{Activity, Gate};
use crate::importer::inflight::Inflight;
use crate::importer::progress::{BYTE_BUDGET, ByteBudget, Throughput};
use crate::importer::runner::worker;
use crate::importer::types::{FolderIn, RawItem};
use crate::importer::writer::truncate_to_micros;
use crate::importer::{ImportTuning, RecoveryReport};
use crate::paths::Paths;
use crate::sink::{CoreEvent, EventSink};

pub const MAX_ENQUEUE_PATHS: usize = 100;
pub const ERRORS_DEFAULT_LIMIT: u32 = 100;
pub const ERRORS_MAX_LIMIT: u32 = 500;
const PST_MAGIC: &[u8; 4] = b"!BDN";
const DISPLAY_NAME_MAX: usize = 512;

// ------------------------------------------------------------------------------------------------ state

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CancelReason {
    User,
    Shutdown,
    Pause,
}

/// Cancel state of the running job. `flag` is what the reader, the byte budget and the pipeline see.
pub(crate) struct JobCancel {
    pub(super) flag: Arc<AtomicBool>,
    reason: Mutex<Option<CancelReason>>,
    pub(super) budget: Arc<ByteBudget>,
}

impl JobCancel {
    pub(super) fn new() -> JobCancel {
        JobCancel { flag: Arc::new(AtomicBool::new(false)), reason: Mutex::new(None), budget: Arc::new(ByteBudget::new(BYTE_BUDGET)) }
    }
    /// User and Shutdown are final; Pause only applies when nothing stronger is set.
    pub(super) fn cancel(&self, why: CancelReason) {
        let mut r = self.reason.lock().unwrap_or_else(|p| p.into_inner());
        if matches!(*r, None | Some(CancelReason::Pause)) {
            *r = Some(why);
        }
        drop(r);
        self.stop();
    }
    /// Stops reader and pipeline without a reason (writer stopped, crash seam).
    pub(super) fn stop(&self) {
        self.flag.store(true, Ordering::SeqCst);
        self.budget.wake_all();
    }
    pub(super) fn reason(&self) -> Option<CancelReason> {
        *self.reason.lock().unwrap_or_else(|p| p.into_inner())
    }
    /// Before a re-run after a DB pause.
    pub(super) fn clear_pause(&self) {
        let mut r = self.reason.lock().unwrap_or_else(|p| p.into_inner());
        if *r == Some(CancelReason::Pause) {
            *r = None;
        }
        self.flag.store(false, Ordering::SeqCst);
    }
}

#[derive(Clone)]
pub(super) struct RunningJob {
    pub(super) job_id: i64,
    pub(super) pst_id: i64,
    pub(super) cancel: Arc<JobCancel>,
}

pub(crate) struct Inner {
    pub(super) db: Db,
    pub(super) gate: Gate,
    pub(super) paths: Paths,
    pub(super) sink: Arc<dyn EventSink>,
    pub(super) phase: watch::Receiver<AppPhase>,
    pub(super) tuning: ImportTuning,
    pub(super) kick: Notify,
    worker: Mutex<Option<tokio::task::JoinHandle<()>>>,
    shutdown: watch::Sender<bool>,
    running: Mutex<Option<RunningJob>>,
    pub(super) throughput: Mutex<Throughput>,
    pub(super) hold_used: AtomicBool,
    pub(crate) events: broadcast::Sender<(i64, ImportStatus)>,
}

impl Inner {
    pub(crate) fn new(db: Db, gate: Gate, paths: Paths, sink: Arc<dyn EventSink>, phase: watch::Receiver<AppPhase>, tuning: ImportTuning) -> Inner {
        Inner {
            db,
            gate,
            paths,
            sink,
            phase,
            tuning,
            kick: Notify::new(),
            worker: Mutex::new(None),
            shutdown: watch::Sender::new(false),
            running: Mutex::new(None),
            throughput: Mutex::new(Throughput::new()),
            hold_used: AtomicBool::new(false),
            events: broadcast::Sender::new(64),
        }
    }
    pub(super) fn running(&self) -> Option<RunningJob> {
        self.running.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }
    pub(super) fn set_running(&self, r: Option<RunningJob>) {
        *self.running.lock().unwrap_or_else(|p| p.into_inner()) = r;
    }
    pub(super) fn shutting_down(&self) -> bool {
        *self.shutdown.borrow()
    }
    pub(super) async fn shutdown_signal(&self) {
        let mut rx = self.shutdown.subscribe();
        let _ = rx.wait_for(|s| *s).await;
    }
    /// The inflight marker of the current data root (the root can move between jobs, Part 02 §8.3).
    pub(super) fn inflight(&self) -> Result<Inflight, AppError> {
        Ok(Inflight::new(self.paths.with_data_root(self.db.data_root()?).inflight_marker()))
    }
}

// ------------------------------------------------------------------------------------------------ status

/// `import_jobs.kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JobKind {
    Import,
    Remove,
}
db_text!(JobKind, "job kind", { Import => "import", Remove => "remove" });

/// `import_jobs.state` (Part 03 §7.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JobState {
    Queued,
    Running,
    Cancelling,
    Cancelled,
    Failed,
    Completed,
}
db_text!(JobState, "job state", {
    Queued => "queued", Running => "running", Cancelling => "cancelling",
    Cancelled => "cancelled", Failed => "failed", Completed => "completed",
});

/// Part 04 §2.1 table.
pub(crate) fn status_of(kind: JobKind, state: JobState) -> ImportStatus {
    match (kind, state) {
        (JobKind::Remove, JobState::Failed) => ImportStatus::Failed,
        (JobKind::Remove, _) => ImportStatus::Removing,
        (JobKind::Import, JobState::Queued) => ImportStatus::Queued,
        (JobKind::Import, JobState::Running) => ImportStatus::Running,
        (JobKind::Import, JobState::Cancelling) => ImportStatus::Cancelling,
        (JobKind::Import, JobState::Cancelled) => ImportStatus::Cancelled,
        (JobKind::Import, JobState::Failed) => ImportStatus::Failed,
        (JobKind::Import, JobState::Completed) => ImportStatus::Completed,
    }
}

const LIST_SQL: &str = "SELECT s.id, s.path, s.display_name, s.file_size_bytes, j.kind, j.state, j.id, \
       to_char(j.created_at AT TIME ZONE 'UTC', $1), to_char(j.finished_at AT TIME ZONE 'UTC', $1), \
       c.phase, c.items_total, c.stored, c.duplicates, c.errors, c.skipped_non_mail, c.images_converted, \
       (SELECT count(*) FROM import_errors e WHERE e.pst_id = s.id) \
  FROM pst_sources s \
  CROSS JOIN LATERAL (SELECT kind, state, id, created_at, finished_at FROM import_jobs j \
                      WHERE j.pst_id = s.id ORDER BY j.id DESC LIMIT 1) j \
  LEFT JOIN import_checkpoints c ON c.pst_id = s.id \
 WHERE $2::bigint IS NULL OR s.id = $2";

pub(crate) async fn list(inner: &Inner, only: Option<i64>) -> Result<Vec<PstSource>, AppError> {
    let c = inner.db.app_client().await?;
    let rows = c.query(LIST_SQL, &[&RFC3339_UTC_FMT, &only]).await.map_err(db_err)?;
    let running = inner.running();
    let mut out = rows
        .iter()
        .map(|r| {
            let id: i64 = r.get(0);
            let status = status_of(JobKind::from_db(r.get(4))?, JobState::from_db(r.get(5))?);
            let job_id: i64 = r.get(6);
            let phase: Option<&str> = r.get(9);
            let phase = phase.map(ImportPhase::from_db).transpose()?;
            let progress = phase.map(|phase| {
                let (stored, duplicates, errors, skipped_non_mail): (i64, i64, i64, i64) = (r.get(11), r.get(12), r.get(13), r.get(14));
                let items_total: Option<i64> = r.get(10);
                let items_done = stored + duplicates + errors + skipped_non_mail;
                let live = running.as_ref().is_some_and(|j| j.pst_id == id) && status == ImportStatus::Running;
                let tp = inner.throughput.lock().unwrap_or_else(|p| p.into_inner());
                ImportProgress {
                    pst_id: id,
                    phase,
                    items_total,
                    items_done,
                    stored,
                    duplicates,
                    errors,
                    skipped_non_mail,
                    images_converted: r.get(15),
                    bytes_per_sec: if live { tp.bytes_per_sec() } else { None },
                    eta_seconds: if live { tp.eta_seconds(items_total, items_done) } else { None },
                }
            });
            let rank = match status {
                ImportStatus::Running | ImportStatus::Cancelling => 0,
                ImportStatus::Queued | ImportStatus::Removing => 1,
                _ => 2,
            };
            let src = PstSource {
                id,
                path: r.get(1),
                display_name: r.get(2),
                file_size_bytes: r.get(3),
                status,
                message_count: progress.as_ref().map_or(0, |p| p.stored + p.duplicates),
                new_message_count: progress.as_ref().map_or(0, |p| p.stored),
                progress,
                error_count: r.get(16),
                queued_at: r.get(7),
                finished_at: r.get(8),
            };
            Ok((rank, job_id, src))
        })
        .collect::<Result<Vec<(u8, i64, PstSource)>, AppError>>()?;
    // running first; queued / removing in queue order; the rest newest first
    out.sort_by(|a, b| {
        a.0.cmp(&b.0).then_with(|| if a.0 == 2 { (&b.2.queued_at, b.1).cmp(&(&a.2.queued_at, a.1)) } else { a.1.cmp(&b.1) })
    });
    Ok(out.into_iter().map(|(_, _, s)| s).collect())
}

/// Emits the PST's current status and returns the source it fetched (None when the list query failed or the PST is gone).
pub(super) async fn emit_status(inner: &Inner, pst_id: i64) -> Option<PstSource> {
    let src = list(inner, Some(pst_id)).await.ok()?.pop()?;
    inner.sink.emit(CoreEvent::ImportStatusChanged(ImportStatusChanged { pst_id, status: src.status, removed: false, source: Some(src.clone()) }));
    Some(src)
}

// ------------------------------------------------------------------------------------------------ commands

/// Part 03 §7.2 path checks. Returns (size, mtime truncated to whole microseconds).
pub fn check_file(path: &str) -> Result<(i64, SystemTime), AppError> {
    let p = Path::new(path);
    let fa = |reason: &str| AppError::FileAccess { path: path.to_owned(), reason: reason.to_owned() };
    if !p.is_absolute() {
        return Err(fa("notAbsolute"));
    }
    let meta = std::fs::metadata(p).map_err(|_| fa("notFound"))?;
    if !meta.is_file() {
        return Err(fa("notAFile"));
    }
    let mut magic = [0u8; 4];
    std::fs::File::open(p).and_then(|mut f| f.read_exact(&mut magic)).map_err(|_| fa("unreadable"))?;
    if &magic != PST_MAGIC {
        return Err(AppError::NotAPst { path: path.to_owned() });
    }
    let mtime = meta.modified().map_err(|_| fa("unreadable"))?;
    Ok((meta.len() as i64, truncate_to_micros(mtime)))
}

fn display_name(path: &str) -> String {
    let p = Path::new(path);
    let stem = p.file_stem().or(p.file_name()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let stem = if stem.is_empty() { "pst".to_owned() } else { stem };
    stem.chars().take(DISPLAY_NAME_MAX).collect()
}

/// Source row + queued import job in one transaction. Err(AlreadyImported) when the identity exists.
async fn insert_source(inner: &Inner, path: &str, size: i64, mtime: SystemTime) -> Result<i64, AppError> {
    let mut c = inner.db.app_client().await?;
    let tx = c.transaction().await.map_err(db_err)?;
    let row = tx
        .query_opt(
            "INSERT INTO pst_sources (path, display_name, file_size_bytes, file_mtime) VALUES ($1, $2, $3, $4) \
             ON CONFLICT ON CONSTRAINT pst_sources_same_file_once DO NOTHING RETURNING id",
            &[&path, &display_name(path), &size, &mtime],
        )
        .await
        .map_err(db_err)?;
    let Some(row) = row else {
        let existing: i64 = tx
            .query_one("SELECT id FROM pst_sources WHERE path = $1 AND file_size_bytes = $2 AND file_mtime = $3", &[&path, &size, &mtime])
            .await
            .map_err(db_err)?
            .get(0);
        return Err(AppError::AlreadyImported { pst_id: existing });
    };
    let pst_id: i64 = row.get(0);
    tx.execute("INSERT INTO import_jobs (pst_id, kind, state) VALUES ($1, 'import', 'queued')", &[&pst_id]).await.map_err(db_err)?;
    tx.commit().await.map_err(db_err)?;
    Ok(pst_id)
}

pub(crate) async fn enqueue(inner: &Inner, paths: Vec<String>) -> Result<Vec<EnqueueResult>, AppError> {
    if paths.is_empty() {
        return Err(AppError::invalid("paths", "empty"));
    }
    if paths.len() > MAX_ENQUEUE_PATHS {
        return Err(AppError::invalid("paths", "tooMany"));
    }
    let mut out = Vec::with_capacity(paths.len());
    for path in paths {
        let result = match check_file(&path) {
            Err(error) => EnqueueResult::Rejected { path, error },
            Ok((size, mtime)) => match insert_source(inner, &path, size, mtime).await {
                Err(e @ AppError::AlreadyImported { .. }) => EnqueueResult::Rejected { path, error: e },
                Err(e) => return Err(e),
                Ok(pst_id) => {
                    let pst = emit_status(inner, pst_id).await.ok_or_else(|| AppError::internal("enqueued source vanished"))?;
                    EnqueueResult::Queued { pst }
                }
            },
        };
        out.push(result);
    }
    inner.kick.notify_one();
    Ok(out)
}

/// Newest job of a PST, locked for the rest of the transaction. None when the PST does not exist.
async fn newest_job_for_update(tx: &tokio_postgres::Transaction<'_>, pst_id: i64) -> Result<Option<(i64, JobKind, JobState)>, AppError> {
    let exists = tx.query_opt("SELECT 1 FROM pst_sources WHERE id = $1 FOR UPDATE", &[&pst_id]).await.map_err(db_err)?;
    if exists.is_none() {
        return Ok(None);
    }
    let row = tx
        .query_opt("SELECT id, kind, state FROM import_jobs WHERE pst_id = $1 ORDER BY id DESC LIMIT 1 FOR UPDATE", &[&pst_id])
        .await
        .map_err(db_err)?;
    row.map(|r| Ok((r.get(0), JobKind::from_db(r.get(1))?, JobState::from_db(r.get(2))?))).transpose()
}

pub(crate) async fn cancel(inner: &Inner, pst_id: i64) -> Result<(), AppError> {
    let mut c = inner.db.app_client().await?;
    let tx = c.transaction().await.map_err(db_err)?;
    let Some((job_id, kind, state)) = newest_job_for_update(&tx, pst_id).await? else {
        return Err(AppError::not_found("pst", pst_id));
    };
    match (kind, state) {
        (JobKind::Import, JobState::Queued) => {
            tx.execute("UPDATE import_jobs SET state = 'cancelled', finished_at = now() WHERE id = $1", &[&job_id]).await.map_err(db_err)?;
        }
        (JobKind::Import, JobState::Running) => {
            tx.execute("UPDATE import_jobs SET state = 'cancelling' WHERE id = $1", &[&job_id]).await.map_err(db_err)?;
        }
        _ => return Err(AppError::invalid("pstId", "notCancellable")),
    }
    tx.commit().await.map_err(db_err)?;
    if let Some(r) = inner.running().filter(|r| r.job_id == job_id) {
        r.cancel.cancel(CancelReason::User);
    }
    emit_status(inner, pst_id).await;
    Ok(())
}

pub(crate) async fn resume(inner: &Inner, pst_id: i64) -> Result<(), AppError> {
    let mut c = inner.db.app_client().await?;
    let tx = c.transaction().await.map_err(db_err)?;
    let Some((_, kind, state)) = newest_job_for_update(&tx, pst_id).await? else {
        return Err(AppError::not_found("pst", pst_id));
    };
    if kind != JobKind::Import || !matches!(state, JobState::Cancelled | JobState::Failed) {
        return Err(AppError::invalid("pstId", "notResumable"));
    }
    let path: String = tx.query_one("SELECT path FROM pst_sources WHERE id = $1", &[&pst_id]).await.map_err(db_err)?.get(0);
    check_file(&path)?;
    tx.execute("INSERT INTO import_jobs (pst_id, kind, state) VALUES ($1, 'import', 'queued')", &[&pst_id]).await.map_err(db_err)?;
    tx.commit().await.map_err(db_err)?;
    emit_status(inner, pst_id).await;
    inner.kick.notify_one();
    Ok(())
}

pub(crate) async fn remove(inner: &Inner, pst_id: i64) -> Result<(), AppError> {
    let mut c = inner.db.app_client().await?;
    let tx = c.transaction().await.map_err(db_err)?;
    let Some((job_id, kind, state)) = newest_job_for_update(&tx, pst_id).await? else {
        return Err(AppError::not_found("pst", pst_id));
    };
    match status_of(kind, state) {
        ImportStatus::Running | ImportStatus::Cancelling => return Err(AppError::Busy { reason: crate::dto::error::BusyReason::ImportRunning }),
        ImportStatus::Removing => return Err(AppError::Busy { reason: crate::dto::error::BusyReason::PstRemoving }),
        _ => {}
    }
    if (kind, state) == (JobKind::Import, JobState::Queued) {
        // required by import_jobs_one_active_per_pst (Part 04 §4.3)
        tx.execute("UPDATE import_jobs SET state = 'cancelled', finished_at = now() WHERE id = $1", &[&job_id]).await.map_err(db_err)?;
    }
    tx.execute("INSERT INTO import_jobs (pst_id, kind, state) VALUES ($1, 'remove', 'queued')", &[&pst_id]).await.map_err(db_err)?;
    tx.commit().await.map_err(db_err)?;
    emit_status(inner, pst_id).await;
    inner.kick.notify_one();
    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ErrorCursor {
    t: String,
    i: i64,
}

pub(crate) async fn errors(inner: &Inner, req: ImportErrorsRequest) -> Result<ImportErrorPage, AppError> {
    let limit = req.limit.unwrap_or(ERRORS_DEFAULT_LIMIT);
    if !(1..=ERRORS_MAX_LIMIT).contains(&limit) {
        return Err(AppError::invalid("limit", "outOfRange"));
    }
    let bad_cursor = || AppError::invalid("cursor", "invalid");
    let (after_t, after_i) = match &req.cursor {
        None => ("-infinity".to_owned(), 0i64),
        Some(s) => {
            let bytes = URL_SAFE_NO_PAD.decode(s).map_err(|_| bad_cursor())?;
            let c: ErrorCursor = serde_json::from_slice(&bytes).map_err(|_| bad_cursor())?;
            (c.t, c.i)
        }
    };
    let c = inner.db.app_client().await?;
    if c.query_opt("SELECT 1 FROM pst_sources WHERE id = $1", &[&req.pst_id]).await.map_err(db_err)?.is_none() {
        return Err(AppError::not_found("pst", req.pst_id));
    }
    let rows = c
        .query(
            "SELECT id, to_char(occurred_at AT TIME ZONE 'UTC', $5), occurred_at::text, folder_path, pst_item_id, stage, severity, message \
               FROM import_errors WHERE pst_id = $1 AND (occurred_at, id) > ($2::text::timestamptz, $3) ORDER BY occurred_at, id LIMIT $4",
            &[&req.pst_id, &after_t, &after_i, &(i64::from(limit) + 1), &RFC3339_UTC_FMT],
        )
        .await
        .map_err(|e| match e.code() {
            Some(c) if *c == SqlState::INVALID_DATETIME_FORMAT || *c == SqlState::DATETIME_FIELD_OVERFLOW => bad_cursor(),
            _ => db_err(e),
        })?;
    let more = rows.len() > limit as usize;
    let page: Vec<_> = rows.iter().take(limit as usize).collect();
    let next_cursor = more.then(|| {
        let last = page.last().expect("limit >= 1");
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&ErrorCursor { t: last.get(2), i: last.get(0) }).expect("cursor serializes"))
    });
    Ok(ImportErrorPage {
        rows: page
            .iter()
            .map(|r| {
                Ok(ImportErrorEntry {
                    id: r.get(0),
                    occurred_at: r.get(1),
                    folder_path: r.get(3),
                    pst_item_id: r.get(4),
                    stage: ImportErrorStage::from_db(r.get(5))?,
                    severity: ImportErrorSeverity::from_db(r.get(6))?,
                    message: r.get(7),
                })
            })
            .collect::<Result<_, AppError>>()?,
        next_cursor,
    })
}

pub(crate) async fn recover_after_crash(inner: &Inner) -> Result<RecoveryReport, AppError> {
    let c = inner.db.app_client().await?;
    let rows = c
        .query(
            "UPDATE import_jobs SET state = CASE state WHEN 'running' THEN 'queued' ELSE 'cancelled' END, \
                    finished_at = CASE state WHEN 'cancelling' THEN now() END \
              WHERE state IN ('running', 'cancelling') RETURNING pst_id, state",
            &[],
        )
        .await
        .map_err(db_err)?;
    let mut report = RecoveryReport::default();
    for r in &rows {
        let pst_id: i64 = r.get(0);
        if JobState::from_db(r.get(1))? == JobState::Queued { report.requeued.push(pst_id) } else { report.cancelled.push(pst_id) }
    }
    report.requeued.sort_unstable();
    report.cancelled.sort_unstable();
    for pst_id in report.requeued.iter().chain(&report.cancelled) {
        emit_status(inner, *pst_id).await;
    }
    Ok(report)
}

pub(crate) async fn self_heal_indexes(inner: &Inner) -> Result<(), AppError> {
    let _guard = inner.gate.try_enter(Activity::Import)?;
    let admin = inner.db.connect_admin().await?;
    ensure_search_indexes(&admin, &|name: &str, done: usize, total: usize| log::info!("self-heal index {name} ({done}/{total})")).await
}

pub(crate) fn start_worker(inner: &Arc<Inner>) {
    let mut slot = inner.worker.lock().unwrap_or_else(|p| p.into_inner());
    if slot.is_none() {
        *slot = Some(tokio::spawn(worker(inner.clone())));
    }
}

pub(crate) async fn shutdown(inner: &Inner, timeout: Duration) -> Result<(), AppError> {
    inner.shutdown.send_replace(true);
    if let Some(r) = inner.running() {
        r.cancel.cancel(CancelReason::Shutdown);
    }
    let handle = inner.worker.lock().unwrap_or_else(|p| p.into_inner()).take();
    let Some(handle) = handle else { return Ok(()) };
    match tokio::time::timeout(timeout, handle).await {
        Ok(_) => Ok(()),
        Err(_) => Err(AppError::internal("import shutdown timed out")),
    }
}

#[cfg(feature = "test-support")]
type SyntheticSource = (Vec<FolderIn>, Vec<RawItem>);
#[cfg(feature = "test-support")]
static SYNTHETIC: std::sync::LazyLock<Mutex<std::collections::HashMap<String, SyntheticSource>>> =
    std::sync::LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

#[cfg(feature = "test-support")]
pub(super) fn synthetic_items(path: &str) -> Option<(Vec<FolderIn>, Vec<RawItem>)> {
    SYNTHETIC.lock().unwrap_or_else(|p| p.into_inner()).get(path).cloned()
}
#[cfg(not(feature = "test-support"))]
pub(super) fn synthetic_items(_path: &str) -> Option<(Vec<FolderIn>, Vec<RawItem>)> {
    None
}

#[cfg(feature = "test-support")]
pub(crate) async fn enqueue_synthetic(inner: &Inner, path: &str, folders: Vec<FolderIn>, items: Vec<RawItem>) -> Result<i64, AppError> {
    let (size, mtime) = check_file(path).unwrap_or((1, truncate_to_micros(SystemTime::now())));
    SYNTHETIC.lock().unwrap_or_else(|p| p.into_inner()).insert(path.to_owned(), (folders, items));
    let pst_id = insert_source(inner, path, size, mtime).await?;
    emit_status(inner, pst_id).await;
    inner.kick.notify_one();
    Ok(pst_id)
}

#[cfg(test)]
mod tests {
    use std::time::UNIX_EPOCH;

    use super::*;

    #[test]
    fn status_table() {
        use JobKind::{Import, Remove};
        let cases = [
            (Import, JobState::Queued, ImportStatus::Queued),
            (Import, JobState::Running, ImportStatus::Running),
            (Import, JobState::Cancelling, ImportStatus::Cancelling),
            (Import, JobState::Cancelled, ImportStatus::Cancelled),
            (Import, JobState::Failed, ImportStatus::Failed),
            (Import, JobState::Completed, ImportStatus::Completed),
            (Remove, JobState::Queued, ImportStatus::Removing),
            (Remove, JobState::Running, ImportStatus::Removing),
            (Remove, JobState::Failed, ImportStatus::Failed),
        ];
        for (kind, state, want) in cases {
            assert_eq!(status_of(kind, state), want, "{kind:?}/{state:?}");
        }
        for &k in JobKind::ALL {
            assert_eq!(JobKind::from_db(k.as_db()).unwrap(), k);
        }
        assert!(JobKind::from_db("bogus").is_err());
        for &st in JobState::ALL {
            assert_eq!(JobState::from_db(st.as_db()).unwrap(), st);
        }
        assert!(JobState::from_db("bogus").is_err());
    }

    #[test]
    fn mtime_truncated_to_micros() {
        let t = UNIX_EPOCH + Duration::new(1_700_000_000, 123_456_789);
        assert_eq!(truncate_to_micros(t), UNIX_EPOCH + Duration::new(1_700_000_000, 123_456_000));
        // Before 2000-01-01 PostgreSQL truncates toward its epoch, i.e. up in absolute time.
        assert_eq!(truncate_to_micros(UNIX_EPOCH + Duration::new(1, 500)), UNIX_EPOCH + Duration::new(1, 1_000));
    }

    #[test]
    fn display_names() {
        assert_eq!(display_name("/x/Archive 2023.pst"), "Archive 2023");
        assert_eq!(display_name("/x/.pst"), ".pst");
        assert_eq!(display_name(&format!("/x/{}.pst", "a".repeat(600))).len(), 512);
    }
}
