//! Durable job queue (spec Part 03 §7.1 state machine) and, in section "runner", the import / removal job runner.
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use tokio::sync::{Notify, broadcast, oneshot, watch};
use tokio_postgres::error::SqlState;

use crate::db::indexes::{drop_search_indexes, ensure_search_indexes};
use crate::db::{db_err, library_counts, Db, RFC3339_UTC_FMT};
use crate::dto::app::{AppPhase, LibraryCounts};
use crate::dto::error::AppError;
use crate::dto::events::{ImportStatusChanged, LibraryChanged};
use crate::dto::import::{
    EnqueueResult, ImportErrorEntry, ImportErrorPage, ImportErrorSeverity, ImportErrorStage, ImportErrorsRequest, ImportPhase,
    ImportProgress, ImportStatus, PstSource,
};
use crate::gate::{Activity, Gate};
use crate::importer::inflight::Inflight;
use crate::importer::normalize::{NormalizeCtx, normalize};
use crate::importer::progress::{BYTE_BUDGET, ByteBudget, Throughput};
use crate::importer::pst_reader::{self, ReaderMsg, ReaderParams};
use crate::importer::types::{FolderIn, ItemError, RawItem};
use crate::importer::writer::{AFTER_JOB_VACUUM, Checkpoint, Writer, WriterConfig, load_known_hashes, stage_db, truncate_to_micros, upsert_folders};
use crate::importer::{ImportTuning, RecoveryReport, remove};
use crate::paths::Paths;
use crate::sink::{CoreEvent, EventSink};

pub const MAX_ENQUEUE_PATHS: usize = 100;
pub const ERRORS_DEFAULT_LIMIT: u32 = 100;
pub const ERRORS_MAX_LIMIT: u32 = 500;
/// How long `pause_for_db` waits for the Supervisor to leave `Ready` after a DB error (Part 02 §6.1 PG crash row).
pub const PAUSE_DETECT: Duration = Duration::from_secs(30);
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
    flag: Arc<AtomicBool>,
    reason: Mutex<Option<CancelReason>>,
    budget: Arc<ByteBudget>,
}

impl JobCancel {
    fn new() -> JobCancel {
        JobCancel { flag: Arc::new(AtomicBool::new(false)), reason: Mutex::new(None), budget: Arc::new(ByteBudget::new(BYTE_BUDGET)) }
    }
    /// User and Shutdown are final; Pause only applies when nothing stronger is set.
    fn cancel(&self, why: CancelReason) {
        let mut r = self.reason.lock().unwrap_or_else(|p| p.into_inner());
        if matches!(*r, None | Some(CancelReason::Pause)) {
            *r = Some(why);
        }
        drop(r);
        self.stop();
    }
    /// Stops reader and pipeline without a reason (writer stopped, crash seam).
    fn stop(&self) {
        self.flag.store(true, Ordering::SeqCst);
        self.budget.wake_all();
    }
    fn reason(&self) -> Option<CancelReason> {
        *self.reason.lock().unwrap_or_else(|p| p.into_inner())
    }
    /// Before a re-run after a DB pause.
    fn clear_pause(&self) {
        let mut r = self.reason.lock().unwrap_or_else(|p| p.into_inner());
        if *r == Some(CancelReason::Pause) {
            *r = None;
        }
        self.flag.store(false, Ordering::SeqCst);
    }
}

#[derive(Clone)]
struct RunningJob {
    job_id: i64,
    pst_id: i64,
    cancel: Arc<JobCancel>,
}

pub(crate) struct Inner {
    db: Db,
    gate: Gate,
    paths: Paths,
    sink: Arc<dyn EventSink>,
    phase: watch::Receiver<AppPhase>,
    tuning: ImportTuning,
    kick: Notify,
    worker: Mutex<Option<tokio::task::JoinHandle<()>>>,
    shutdown: watch::Sender<bool>,
    running: Mutex<Option<RunningJob>>,
    throughput: Mutex<Throughput>,
    hold_used: AtomicBool,
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
    fn running(&self) -> Option<RunningJob> {
        self.running.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }
    fn set_running(&self, r: Option<RunningJob>) {
        *self.running.lock().unwrap_or_else(|p| p.into_inner()) = r;
    }
    fn shutting_down(&self) -> bool {
        *self.shutdown.borrow()
    }
    async fn shutdown_signal(&self) {
        let mut rx = self.shutdown.subscribe();
        let _ = rx.wait_for(|s| *s).await;
    }
    /// The inflight marker of the current data root (the root can move between jobs, Part 02 §8.3).
    fn inflight(&self) -> Result<Inflight, AppError> {
        Ok(Inflight::new(self.paths.with_data_root(self.db.data_root()?).inflight_marker()))
    }
}

// ------------------------------------------------------------------------------------------------ status

/// Part 04 §2.1 table.
pub fn status_of(kind: &str, state: &str) -> ImportStatus {
    match (kind, state) {
        ("remove", "failed") => ImportStatus::Failed,
        ("remove", _) => ImportStatus::Removing,
        (_, "queued") => ImportStatus::Queued,
        (_, "running") => ImportStatus::Running,
        (_, "cancelling") => ImportStatus::Cancelling,
        (_, "cancelled") => ImportStatus::Cancelled,
        (_, "failed") => ImportStatus::Failed,
        _ => ImportStatus::Completed,
    }
}

fn phase_of(s: &str) -> ImportPhase {
    match s {
        "scanning" => ImportPhase::Scanning,
        "importing" => ImportPhase::Importing,
        _ => ImportPhase::Finalizing,
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
    let mut out: Vec<(u8, i64, PstSource)> = rows
        .iter()
        .map(|r| {
            let id: i64 = r.get(0);
            let status = status_of(r.get(4), r.get(5));
            let job_id: i64 = r.get(6);
            let phase: Option<&str> = r.get(9);
            let progress = phase.map(|p| {
                let (stored, duplicates, errors, skipped_non_mail): (i64, i64, i64, i64) = (r.get(11), r.get(12), r.get(13), r.get(14));
                let items_total: Option<i64> = r.get(10);
                let items_done = stored + duplicates + errors + skipped_non_mail;
                let live = running.as_ref().is_some_and(|j| j.pst_id == id) && status == ImportStatus::Running;
                let tp = inner.throughput.lock().unwrap_or_else(|p| p.into_inner());
                ImportProgress {
                    pst_id: id,
                    phase: phase_of(p),
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
            (rank, job_id, src)
        })
        .collect();
    // running first; queued / removing in queue order; the rest newest first
    out.sort_by(|a, b| {
        a.0.cmp(&b.0).then_with(|| if a.0 == 2 { (&b.2.queued_at, b.1).cmp(&(&a.2.queued_at, a.1)) } else { a.1.cmp(&b.1) })
    });
    Ok(out.into_iter().map(|(_, _, s)| s).collect())
}

async fn source(inner: &Inner, pst_id: i64) -> Option<PstSource> {
    list(inner, Some(pst_id)).await.ok()?.pop()
}

async fn emit_status(inner: &Inner, pst_id: i64) {
    if let Some(src) = source(inner, pst_id).await {
        inner.sink.emit(CoreEvent::ImportStatusChanged(ImportStatusChanged { pst_id, status: src.status, removed: false, source: Some(src) }));
    }
}

async fn emit_library(inner: &Inner) {
    let counts = async { library_counts(&*inner.db.app_client().await?).await };
    if let Ok(counts) = counts.await {
        inner.sink.emit(CoreEvent::LibraryChanged(LibraryChanged { counts }));
    }
}

fn progress_event(cp: &Checkpoint, tp: Option<&Throughput>) -> ImportProgress {
    ImportProgress {
        pst_id: cp.pst_id,
        phase: cp.phase,
        items_total: cp.items_total,
        items_done: cp.items_done(),
        stored: cp.stored,
        duplicates: cp.duplicates,
        errors: cp.errors,
        skipped_non_mail: cp.skipped_non_mail,
        images_converted: cp.images_converted,
        bytes_per_sec: tp.and_then(Throughput::bytes_per_sec),
        eta_seconds: tp.and_then(|t| t.eta_seconds(cp.items_total, cp.items_done())),
    }
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
                    emit_status(inner, pst_id).await;
                    let pst = source(inner, pst_id).await.ok_or_else(|| AppError::internal("enqueued source vanished"))?;
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
async fn newest_job_for_update(tx: &tokio_postgres::Transaction<'_>, pst_id: i64) -> Result<Option<(i64, String, String)>, AppError> {
    let exists = tx.query_opt("SELECT 1 FROM pst_sources WHERE id = $1 FOR UPDATE", &[&pst_id]).await.map_err(db_err)?;
    if exists.is_none() {
        return Ok(None);
    }
    let row = tx
        .query_opt("SELECT id, kind, state FROM import_jobs WHERE pst_id = $1 ORDER BY id DESC LIMIT 1 FOR UPDATE", &[&pst_id])
        .await
        .map_err(db_err)?;
    Ok(row.map(|r| (r.get(0), r.get(1), r.get(2))))
}

pub(crate) async fn cancel(inner: &Inner, pst_id: i64) -> Result<(), AppError> {
    let mut c = inner.db.app_client().await?;
    let tx = c.transaction().await.map_err(db_err)?;
    let Some((job_id, kind, state)) = newest_job_for_update(&tx, pst_id).await? else {
        return Err(AppError::not_found("pst", pst_id));
    };
    match (kind.as_str(), state.as_str()) {
        ("import", "queued") => {
            tx.execute("UPDATE import_jobs SET state = 'cancelled', finished_at = now() WHERE id = $1", &[&job_id]).await.map_err(db_err)?;
        }
        ("import", "running") => {
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
    if kind != "import" || !matches!(state.as_str(), "cancelled" | "failed") {
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
    match status_of(&kind, &state) {
        ImportStatus::Running | ImportStatus::Cancelling => return Err(AppError::Busy { reason: crate::dto::error::BusyReason::ImportRunning }),
        ImportStatus::Removing => return Err(AppError::Busy { reason: crate::dto::error::BusyReason::PstRemoving }),
        _ => {}
    }
    if kind == "import" && state == "queued" {
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
    let stage = |s: &str| match s {
        "open" => ImportErrorStage::Open,
        "scan" => ImportErrorStage::Scan,
        "parse" => ImportErrorStage::Parse,
        "html" => ImportErrorStage::Html,
        "image" => ImportErrorStage::Image,
        _ => ImportErrorStage::Store,
    };
    let severity = |s: &str| match s {
        "item_skipped" => ImportErrorSeverity::ItemSkipped,
        "degraded" => ImportErrorSeverity::Degraded,
        _ => ImportErrorSeverity::JobFailed,
    };
    let more = rows.len() > limit as usize;
    let page: Vec<_> = rows.iter().take(limit as usize).collect();
    let next_cursor = more.then(|| {
        let last = page.last().expect("limit >= 1");
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&ErrorCursor { t: last.get(2), i: last.get(0) }).expect("cursor serializes"))
    });
    Ok(ImportErrorPage {
        rows: page
            .iter()
            .map(|r| ImportErrorEntry {
                id: r.get(0),
                occurred_at: r.get(1),
                folder_path: r.get(3),
                pst_item_id: r.get(4),
                stage: stage(r.get(5)),
                severity: severity(r.get(6)),
                message: r.get(7),
            })
            .collect(),
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
        if r.get::<_, &str>(1) == "queued" { report.requeued.push(pst_id) } else { report.cancelled.push(pst_id) }
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
fn synthetic_items(path: &str) -> Option<(Vec<FolderIn>, Vec<RawItem>)> {
    SYNTHETIC.lock().unwrap_or_else(|p| p.into_inner()).get(path).cloned()
}
#[cfg(not(feature = "test-support"))]
fn synthetic_items(_path: &str) -> Option<(Vec<FolderIn>, Vec<RawItem>)> {
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

// ------------------------------------------------------------------------------------------------ runner

/// Why a job attempt failed the job (one `job_failed` error row).
struct JobFailure {
    stage: ImportErrorStage,
    message: String,
}

enum AttemptEnd {
    /// Input exhausted or cancelled (user / shutdown): writer finished, checkpoint committed.
    Finished,
    /// DB went away (writer DbNotReady or phase change): uncommitted batch dropped.
    Paused,
    /// Test seam crash_after_batches.
    Crashed,
    Failed(JobFailure),
}

enum Next {
    Job { id: i64, pst_id: i64, kind: String },
    Idle,
}

async fn worker(inner: Arc<Inner>) {
    let mut phase = inner.phase.clone();
    loop {
        if inner.shutting_down() {
            return;
        }
        if *phase.borrow_and_update() != AppPhase::Ready {
            tokio::select! {
                r = phase.changed() => if r.is_err() { return },
                _ = inner.shutdown_signal() => return,
            }
            continue;
        }
        let kicked = inner.kick.notified();
        tokio::pin!(kicked);
        kicked.as_mut().enable(); // a kick during the query below is not lost
        let next = next_job(&inner).await;
        match next {
            Ok(Next::Job { id, pst_id, kind }) => {
                let activity = if kind == "remove" { Activity::Removal } else { Activity::Import };
                let guard = match inner.gate.try_enter(activity) {
                    Ok(g) => g,
                    Err(_) => {
                        tokio::select! {
                            _ = inner.gate.released() => {},
                            _ = inner.shutdown_signal() => return,
                        }
                        continue;
                    }
                };
                let crashed = run_job(&inner, id, pst_id, &kind).await;
                drop(guard);
                if crashed {
                    return; // simulated process kill: the worker dies with the job
                }
                continue;
            }
            Ok(Next::Idle) => {}
            Err(e) => log::warn!("import queue: {e}"),
        }
        tokio::select! {
            _ = kicked => {},
            r = phase.changed() => if r.is_err() { return },
            _ = inner.shutdown_signal() => return,
        }
    }
}

async fn next_job(inner: &Inner) -> Result<Next, AppError> {
    let c = inner.db.app_client().await?;
    let row = c
        .query_opt("SELECT id, pst_id, kind FROM import_jobs WHERE state = 'queued' ORDER BY id LIMIT 1", &[])
        .await
        .map_err(db_err)?;
    Ok(row.map_or(Next::Idle, |r| Next::Job { id: r.get(0), pst_id: r.get(1), kind: r.get(2) }))
}

/// Takes the job (`queued` → `running`) and runs it. Returns true when the crash seam dropped it.
async fn run_job(inner: &Arc<Inner>, job_id: i64, pst_id: i64, kind: &str) -> bool {
    let cancel = Arc::new(JobCancel::new());
    // registered before the row turns `running`, so a cancel that sees `running` finds the flag
    inner.set_running(Some(RunningJob { job_id, pst_id, cancel: cancel.clone() }));
    *inner.throughput.lock().unwrap_or_else(|p| p.into_inner()) = Throughput::new();
    let taken = match inner.db.app_client().await {
        Ok(c) => c
            .execute("UPDATE import_jobs SET state = 'running', started_at = now() WHERE id = $1 AND state = 'queued'", &[&job_id])
            .await
            .map_err(db_err),
        Err(e) => Err(e),
    };
    let crashed = match taken {
        Ok(1) => {
            emit_status(inner, pst_id).await;
            if kind == "remove" {
                run_removal(inner, job_id, pst_id).await;
                false
            } else {
                run_import(inner, job_id, pst_id, &cancel).await
            }
        }
        Ok(_) => false, // cancelled or removed meanwhile
        Err(e) => {
            log::warn!("cannot start job {job_id}: {e}");
            false
        }
    };
    inner.set_running(None);
    crashed
}

async fn run_removal(inner: &Arc<Inner>, job_id: i64, pst_id: i64) {
    loop {
        let result = match inner.db.app_client().await {
            Ok(c) => remove::run(&c, pst_id).await,
            Err(e) => Err(e),
        };
        match result {
            Ok(stats) => {
                log::info!("removed pst {pst_id}: {stats:?}");
                inner.sink.emit(CoreEvent::ImportStatusChanged(ImportStatusChanged { pst_id, status: ImportStatus::Removing, removed: true, source: None }));
                emit_library(inner).await;
                let _ = inner.events.send((pst_id, ImportStatus::Removing));
                return;
            }
            Err(AppError::DbNotReady) => {
                if !pause_wait(inner).await {
                    return; // row stays `running`; recover_after_crash re-queues it (idempotent removal)
                }
            }
            Err(e) => {
                let failure = JobFailure { stage: ImportErrorStage::Store, message: e.to_string() };
                if set_final(inner, job_id, pst_id, "failed", Some(&failure)).await.is_ok() {
                    emit_status(inner, pst_id).await;
                    let _ = inner.events.send((pst_id, ImportStatus::Failed));
                }
                return;
            }
        }
    }
}

/// Final job row state (+ the job_failed error row in the same transaction).
async fn set_final(inner: &Inner, job_id: i64, pst_id: i64, state: &str, failure: Option<&JobFailure>) -> Result<(), AppError> {
    let mut c = inner.db.app_client().await?;
    let tx = c.transaction().await.map_err(db_err)?;
    if let Some(f) = failure {
        let e = ItemError::new(f.stage, ImportErrorSeverity::JobFailed, "", 0, &f.message);
        tx.execute(
            "INSERT INTO import_errors (pst_id, job_id, stage, severity, message) VALUES ($1, $2, $3, 'job_failed', $4)",
            &[&pst_id, &job_id, &stage_db(f.stage), &e.message],
        )
        .await
        .map_err(db_err)?;
    }
    tx.execute(
        "UPDATE import_jobs SET state = $2, finished_at = CASE WHEN $2 = 'queued' THEN NULL ELSE now() END WHERE id = $1",
        &[&job_id, &state],
    )
    .await
    .map_err(db_err)?;
    tx.commit().await.map_err(db_err)
}

/// pause_for_db wait (Part 02 §6.1 PG crash row): DB reachable → resume at once; else wait up to 30 s for the
/// phase to leave `Ready`, then for `Ready` again (also after `Failed`: app_retry resumes the job in this process).
/// false = shutdown: the job row stays `running` and `recover_after_crash` re-queues it at the next start.
async fn pause_wait(inner: &Inner) -> bool {
    if let Ok(c) = inner.db.app_client().await
        && c.simple_query("SELECT 1").await.is_ok()
    {
        return true;
    }
    let mut ph = inner.phase.clone();
    if *ph.borrow() == AppPhase::Ready {
        let _ = tokio::time::timeout(PAUSE_DETECT, ph.wait_for(|p| *p != AppPhase::Ready)).await;
    }
    tokio::select! {
        // `Failed` is not a wake-up: the job stays paused and resumes on the next `Ready` (app_retry), Part 02 §6.1
        r = ph.wait_for(|p| *p == AppPhase::Ready) => r.is_ok(),
        _ = inner.shutdown_signal() => false,
    }
}

/// Returns true when the crash seam dropped the job.
async fn run_import(inner: &Arc<Inner>, job_id: i64, pst_id: i64, cancel: &Arc<JobCancel>) -> bool {
    let end = loop {
        let end = attempt(inner, job_id, pst_id, cancel).await.unwrap_or_else(|e| match e {
            AppError::DbNotReady => AttemptEnd::Paused,
            other => AttemptEnd::Failed(JobFailure { stage: ImportErrorStage::Store, message: other.to_string() }),
        });
        match end {
            AttemptEnd::Paused => {
                if let Ok(m) = inner.inflight() {
                    m.clear(); // the reader stopped cleanly: the marker names no poison item
                }
                if !pause_wait(inner).await || cancel.reason() == Some(CancelReason::Shutdown) {
                    return false;
                }
                if cancel.reason() == Some(CancelReason::User) {
                    break AttemptEnd::Finished;
                }
                cancel.clear_pause();
                log::info!("import job {job_id}: database back, resuming from the checkpoint");
            }
            other => break other,
        }
    };
    let failure = match end {
        AttemptEnd::Crashed => {
            let _ = inner.events.send((pst_id, ImportStatus::Running));
            return true;
        }
        AttemptEnd::Failed(f) => Some(f),
        _ => None,
    };
    let reason = cancel.reason();
    if reason == Some(CancelReason::Shutdown) && failure.is_none() {
        // Part 02 §2.5 step 1: back to queued; index self-heal at the next start covers dropped indexes
        if let Ok(m) = inner.inflight() {
            m.clear();
        }
        if set_final(inner, job_id, pst_id, "queued", None).await.is_ok() {
            emit_status(inner, pst_id).await;
            let _ = inner.events.send((pst_id, ImportStatus::Queued));
        }
        return false;
    }
    if let Err(e) = finalize(inner, pst_id).await {
        log::warn!("import job {job_id}: finalize: {e}"); // DB gone: row stays running, recovered at next start
        return false;
    }
    let (state, status) = match (&failure, reason) {
        (Some(_), _) => ("failed", ImportStatus::Failed),
        (None, Some(CancelReason::User)) => ("cancelled", ImportStatus::Cancelled),
        _ => ("completed", ImportStatus::Completed),
    };
    if let Err(e) = set_final(inner, job_id, pst_id, state, failure.as_ref()).await {
        log::warn!("import job {job_id}: final state: {e}");
        return false;
    }
    emit_status(inner, pst_id).await;
    emit_library(inner).await;
    let _ = inner.events.send((pst_id, status));
    false
}

/// Step 9: Finalizing phase, indexes (always; idempotent), after-job VACUUM, marker removed.
async fn finalize(inner: &Inner, pst_id: i64) -> Result<(), AppError> {
    let c = inner.db.app_client().await?;
    if Checkpoint::load(&c, pst_id).await?.is_some() {
        Checkpoint::set_phase(&c, pst_id, ImportPhase::Finalizing, None).await?;
        if let Some(cp) = Checkpoint::load(&c, pst_id).await? {
            inner.sink.emit(CoreEvent::ImportProgress(progress_event(&cp, None)));
        }
    }
    let admin = inner.db.connect_admin().await?;
    ensure_search_indexes(&admin, &|name: &str, done: usize, total: usize| log::info!("import finalize index {name} ({done}/{total})")).await?;
    c.batch_execute(AFTER_JOB_VACUUM).await.map_err(db_err)?;
    inner.inflight()?.clear();
    Ok(())
}

/// One run of the pipeline from the checkpoint (steps 1–8).
async fn attempt(inner: &Arc<Inner>, job_id: i64, pst_id: i64, cancel: &Arc<JobCancel>) -> Result<AttemptEnd, AppError> {
    let mut c = inner.db.app_client().await?;
    let row = c.query_one("SELECT path, file_size_bytes, file_mtime FROM pst_sources WHERE id = $1", &[&pst_id]).await.map_err(db_err)?;
    let path: String = row.get(0);
    let synthetic = synthetic_items(&path);
    // (1) file identity
    let identity = match std::fs::metadata(&path) {
        Ok(m) => Ok((m.len() as i64, truncate_to_micros(m.modified().unwrap_or(UNIX_EPOCH)))),
        Err(_) if synthetic.is_some() => Ok((row.get(1), row.get(2))),
        Err(e) => Err(e),
    };
    let (size, mtime) = match identity {
        Ok(v) => v,
        Err(e) => return Ok(AttemptEnd::Failed(JobFailure { stage: ImportErrorStage::Open, message: format!("cannot open PST file: {e}") })),
    };
    // (2) checkpoint
    let (start, reset) = Checkpoint::begin(&c, pst_id, job_id, size, mtime).await?;
    if reset {
        log::info!("pst {pst_id}: file size or mtime changed, import restarts from the beginning");
    }
    // (3) poison item: the marker of this job is the pending record; another job's marker is ignored (Part 02 §6.3 rule 1)
    let inflight = inner.inflight()?;
    // A reset checkpoint (file changed) renumbers the items: the marker's seq may name another item, so drop it.
    let pending = if reset { None } else { inflight.read().filter(|r| r.job_id == job_id) };
    // (4) reader
    let params = ReaderParams {
        path: PathBuf::from(&path),
        cursor_seq: start.cursor_seq,
        pending,
        inflight: Some(inflight),
        cancel: cancel.flag.clone(),
        // test seam: only the first attempt of this manager holds (a re-run after a pause or resume never blocks)
        hold_after: inner.tuning.hold_reader_after.filter(|_| !inner.hold_used.swap(true, Ordering::SeqCst)),
        budget: cancel.budget.clone(),
        job_id,
    };
    let (rx, reader) = match synthetic {
        #[cfg(feature = "test-support")]
        Some((folders, items)) => pst_reader::spawn_synthetic(params, folders, items),
        _ => pst_reader::spawn(params),
    };
    let first = tokio::task::spawn_blocking(move || (rx.recv(), rx)).await.map_err(|e| AppError::internal(e.to_string()))?;
    let (first, rx) = first;
    let scanned = match first {
        Ok(ReaderMsg::Scanned(s)) => s,
        Ok(ReaderMsg::Failed { stage, message }) => {
            join_thread(reader).await;
            return Ok(AttemptEnd::Failed(JobFailure { stage, message }));
        }
        Ok(ReaderMsg::Item(_)) | Err(_) => {
            cancel.stop();
            join_thread(reader).await;
            return Ok(AttemptEnd::Failed(JobFailure { stage: ImportErrorStage::Scan, message: "reader stopped before the scan finished".into() }));
        }
    };
    upsert_folders(&mut c, pst_id, &scanned.folders).await?;
    for e in &scanned.scan_errors {
        c.execute(
            "INSERT INTO import_errors (pst_id, job_id, stage, severity, folder_path, message) VALUES ($1, $2, 'scan', 'degraded', $3, $4)",
            &[&pst_id, &job_id, &e.folder_path, &e.message],
        )
        .await
        .map_err(db_err)?;
    }
    Checkpoint::set_phase(&c, pst_id, ImportPhase::Importing, Some(scanned.items_total)).await?;
    let start = Checkpoint { phase: ImportPhase::Importing, items_total: Some(scanned.items_total), ..start };
    inner.sink.emit(CoreEvent::ImportProgress(progress_event(&start, None)));
    // (5) index policy (Part 04 §6.4)
    let LibraryCounts { messages, psts } = library_counts(&c).await?;
    let remaining = scanned.items_total - start.cursor_seq;
    if 2 * remaining >= messages {
        let admin = inner.db.connect_admin().await?;
        drop_search_indexes(&admin).await?;
        log::info!("pst {pst_id}: {remaining} items for {messages} messages: search indexes dropped until finalize");
    }
    // (6) writer
    let quality: i16 = c.query_one("SELECT webp_quality FROM settings", &[]).await.map_err(db_err)?.get(0);
    let known = Arc::new(load_known_hashes(&c).await?);
    drop(c);
    let crash = Arc::new(Notify::new());
    let sent_bytes = Arc::new(AtomicU64::new(0));
    let on_commit = {
        let inner = inner.clone();
        let crash = crash.clone();
        let sent_bytes = sent_bytes.clone();
        let start_batches = start.batches_committed;
        let start_stored = start.stored;
        Box::new(move |cp: &Checkpoint| {
            let progress = {
                let mut tp = inner.throughput.lock().unwrap_or_else(|p| p.into_inner());
                tp.record(Instant::now(), cp.items_done(), sent_bytes.load(Ordering::Relaxed));
                progress_event(cp, Some(&tp))
            };
            inner.sink.emit(CoreEvent::ImportProgress(progress));
            if cp.stored > start_stored {
                let counts = LibraryCounts { messages: messages + (cp.stored - start_stored), psts };
                inner.sink.emit(CoreEvent::LibraryChanged(LibraryChanged { counts }));
            }
            if let Some(f) = &inner.tuning.on_batch {
                f(cp.batches_committed);
            }
            if inner.tuning.crash_after_batches.is_some_and(|n| cp.batches_committed - start_batches >= n) {
                crash.notify_one();
            }
        })
    };
    let cfg = WriterConfig { pst_id, data_root: inner.db.data_root()?, start: start.clone(), limits: inner.tuning.writer.clone(), known: known.clone() };
    let writer = Writer::spawn(inner.db.connect_app().await?, cfg, on_commit).await?;
    // (7) pipeline thread: rayon pool runs normalize, results go to the writer
    let ctx = NormalizeCtx { webp_quality: quality.clamp(1, 100) as u8, known };
    let sender = writer.sender();
    let flag = cancel.clone();
    let late_failure: Arc<Mutex<Option<JobFailure>>> = Arc::new(Mutex::new(None));
    let late = late_failure.clone();
    let (done_tx, done_rx) = oneshot::channel::<()>();
    let pipeline = std::thread::Builder::new()
        .name("import-pipeline".into())
        .spawn(move || {
            use rayon::iter::{ParallelBridge, ParallelIterator};
            let threads = std::thread::available_parallelism().map_or(1, |n| n.get().saturating_sub(1).max(1));
            let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).thread_name(|i| format!("normalize-{i}")).build().expect("rayon pool");
            let writer_gone = AtomicBool::new(false);
            pool.install(|| {
                rx.into_iter().par_bridge().for_each(|msg| match msg {
                    ReaderMsg::Item(raw) => {
                        let bytes = raw.approx_bytes();
                        if !writer_gone.load(Ordering::SeqCst) {
                            let item = normalize(raw, &ctx);
                            if sender.blocking_send(item).is_err() {
                                writer_gone.store(true, Ordering::SeqCst);
                                flag.stop(); // writer stopped: stop the reader too
                            } else {
                                sent_bytes.fetch_add(bytes, Ordering::Relaxed);
                            }
                        }
                        flag.budget.release(bytes);
                    }
                    ReaderMsg::Failed { stage, message } => {
                        *late.lock().unwrap_or_else(|p| p.into_inner()) = Some(JobFailure { stage, message });
                        flag.stop();
                    }
                    ReaderMsg::Scanned(_) => {}
                })
            });
            drop(sender);
            let _ = done_tx.send(());
        })
        .expect("spawn pipeline thread");
    // wait for: end of input, the crash seam, or a phase change (Supervisor restarting PostgreSQL)
    let mut phase = inner.phase.clone();
    phase.borrow_and_update();
    enum Wake {
        Done,
        Crash,
        Phase,
    }
    let wake = tokio::select! {
        _ = done_rx => Wake::Done,
        _ = crash.notified() => Wake::Crash,
        _ = phase.changed() => Wake::Phase,
    };
    match wake {
        Wake::Done => {}
        Wake::Crash => {
            cancel.stop();
            writer.abort();
            join_thread(pipeline).await;
            join_thread(reader).await;
            return Ok(AttemptEnd::Crashed);
        }
        Wake::Phase => {
            cancel.cancel(CancelReason::Pause);
            writer.abort();
            join_thread(pipeline).await;
            join_thread(reader).await;
            return Ok(AttemptEnd::Paused);
        }
    }
    join_thread(pipeline).await;
    join_thread(reader).await;
    // (8) end of input: commit the gap-free prefix
    let finished = writer.finish().await;
    if let Some(f) = late_failure.lock().unwrap_or_else(|p| p.into_inner()).take() {
        return Ok(AttemptEnd::Failed(f));
    }
    match finished {
        Ok(_) => {
            if cancel.reason() == Some(CancelReason::Pause) {
                return Ok(AttemptEnd::Paused);
            }
            Ok(AttemptEnd::Finished)
        }
        Err(AppError::DbNotReady) => Ok(AttemptEnd::Paused),
        Err(AppError::FileAccess { reason, .. }) if reason == "insufficientSpace" => {
            Ok(AttemptEnd::Failed(JobFailure { stage: ImportErrorStage::Store, message: "disk space low".into() }))
        }
        Err(e) => Ok(AttemptEnd::Failed(JobFailure { stage: ImportErrorStage::Store, message: e.to_string() })),
    }
}

async fn join_thread(h: std::thread::JoinHandle<()>) {
    let _ = tokio::task::spawn_blocking(move || h.join()).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_table() {
        let cases = [
            ("import", "queued", ImportStatus::Queued),
            ("import", "running", ImportStatus::Running),
            ("import", "cancelling", ImportStatus::Cancelling),
            ("import", "cancelled", ImportStatus::Cancelled),
            ("import", "failed", ImportStatus::Failed),
            ("import", "completed", ImportStatus::Completed),
            ("remove", "queued", ImportStatus::Removing),
            ("remove", "running", ImportStatus::Removing),
            ("remove", "failed", ImportStatus::Failed),
        ];
        for (kind, state, want) in cases {
            assert_eq!(status_of(kind, state), want, "{kind}/{state}");
        }
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
