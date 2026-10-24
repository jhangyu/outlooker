//! Durable job queue (spec Part 03 §7.1 state machine): job state, SQL commands, list, errors, recovery, worker
//! start / shutdown. The job runner is `runner.rs`, one import attempt is `pipeline.rs`.
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
use crate::importer::probe::{Probe, probe};
use crate::importer::progress::{BYTE_BUDGET, ByteBudget, Throughput};
use crate::importer::runner::worker;
use crate::importer::types::{FolderIn, ItemError, RawItem};
use crate::importer::writer::{INS_JOB_FAILED, truncate_to_micros};
use crate::importer::{ImportTuning, RecoveryReport, SyncTarget};
use crate::paths::Paths;
use crate::sink::{CoreEvent, EventSink};

pub const MAX_REGISTER_PATHS: usize = 100;
pub const ERRORS_DEFAULT_LIMIT: u32 = 100;
pub const ERRORS_MAX_LIMIT: u32 = 500;
const DISPLAY_NAME_MAX: usize = 512;

// ------------------------------------------------------------------------------------------------ state

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CancelReason {
    /// The user paused (or unregistered) the path: back to `queued`, the job resumes from the checkpoint.
    Hold,
    Shutdown,
    /// DB pause (pause_for_db): the attempt re-runs when the database is back.
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
    /// Hold and Shutdown are final; Pause only applies when nothing stronger is set.
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
    Failed,
    Completed,
    /// Terminal: the PST was held by another program; finished_at = last seen locked.
    Locked,
}
db_text!(JobState, "job state", {
    Queued => "queued", Running => "running", Failed => "failed", Completed => "completed", Locked => "locked",
});

/// Part 04 §2.1 table.
pub(crate) fn status_of(kind: JobKind, state: JobState) -> ImportStatus {
    match (kind, state) {
        (JobKind::Remove, JobState::Failed) => ImportStatus::Failed,
        (JobKind::Remove, _) => ImportStatus::Removing,
        (JobKind::Import, JobState::Queued) => ImportStatus::Queued,
        (JobKind::Import, JobState::Running) => ImportStatus::Running,
        (JobKind::Import, JobState::Failed) => ImportStatus::Failed,
        (JobKind::Import, JobState::Completed) => ImportStatus::Completed,
        (JobKind::Import, JobState::Locked) => ImportStatus::Locked,
    }
}

const LIST_SQL: &str = "SELECT s.id, s.path, s.display_name, s.file_size_bytes, j.kind, j.state, j.id, \
       to_char(j.created_at AT TIME ZONE 'UTC', $1), to_char(j.finished_at AT TIME ZONE 'UTC', $1), \
       c.phase, c.items_total, c.stored, c.duplicates, c.errors, c.skipped_non_mail, c.images_converted, \
       (SELECT count(*) FROM import_errors e WHERE e.pst_id = s.id), \
       s.paused, to_char(s.last_checked_at AT TIME ZONE 'UTC', $1), \
       (SELECT to_char(d.finished_at AT TIME ZONE 'UTC', $1) FROM import_jobs d \
         WHERE d.pst_id = s.id AND d.kind = 'import' AND d.state = 'completed' ORDER BY d.id DESC LIMIT 1), \
       (SELECT e.message FROM import_errors e WHERE e.pst_id = s.id AND e.job_id = j.id AND e.severity = 'job_failed' \
         ORDER BY e.id DESC LIMIT 1) \
  FROM pst_sources s \
  CROSS JOIN LATERAL (SELECT kind, state, id, created_at, finished_at FROM import_jobs j \
                      WHERE j.pst_id = s.id ORDER BY j.id DESC LIMIT 1) j \
  LEFT JOIN import_checkpoints c ON c.pst_id = s.id \
 WHERE NOT s.unregistered AND ($2::bigint IS NULL OR s.id = $2)";

pub(crate) async fn list(inner: &Inner, only: Option<i64>) -> Result<Vec<PstSource>, AppError> {
    let c = inner.db.app_client().await?;
    let rows = c.query(LIST_SQL, &[&RFC3339_UTC_FMT, &only]).await.map_err(db_err)?;
    let running = inner.running();
    let mut out = rows
        .iter()
        .map(|r| {
            let id: i64 = r.get(0);
            let (kind, state) = (JobKind::from_db(r.get(4))?, JobState::from_db(r.get(5))?);
            let status = status_of(kind, state);
            let job_id: i64 = r.get(6);
            let phase: Option<&str> = r.get(9);
            let phase = phase.map(ImportPhase::from_db).transpose()?;
            let progress = phase.map(|phase| {
                let (stored, duplicates, errors, skipped_non_mail): (i64, i64, i64, i64) = (r.get(11), r.get(12), r.get(13), r.get(14));
                let items_total: Option<i64> = r.get(10);
                let items_done = stored + duplicates + errors + skipped_non_mail;
                let live = running.as_ref().is_some_and(|j| j.pst_id == id) && status == ImportStatus::Running;
                let tp = inner.throughput.lock().unwrap_or_else(|p| p.into_inner());
                Box::new(ImportProgress {
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
                })
            });
            let rank = match status {
                ImportStatus::Running => 0,
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
                paused: r.get(17),
                last_checked_at: r.get(18),
                last_synced_at: r.get(19),
                failure_reason: if status == ImportStatus::Failed { r.get(20) } else { None },
                remove_failed: kind == JobKind::Remove && state == JobState::Failed,
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

fn display_name(path: &str) -> String {
    let p = Path::new(path);
    let stem = p.file_stem().or(p.file_name()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let stem = if stem.is_empty() { "pst".to_owned() } else { stem };
    stem.chars().take(DISPLAY_NAME_MAX).collect()
}

/// Path identity (contract override 8): `std::fs::canonicalize` gives the absolute path with `.` / `..` and symlinks
/// resolved, separators normalized and the letter case as stored on disk (macOS `realpath`, Windows
/// `GetFinalPathNameByHandleW`), so every spelling of one file maps to one string. None when the path does not resolve.
fn canonical(path: &str) -> Option<String> {
    Some(plain_path(std::fs::canonicalize(path).ok()?.into_os_string().into_string().ok()?))
}

/// Drops the `\\?\` prefix Windows canonicalization adds (drive and UNC paths below MAX_PATH), so the stored path
/// reads like the one the user picked; longer paths keep it (they need it to open). Other strings pass unchanged.
fn plain_path(p: String) -> String {
    let plain = if let Some(rest) = p.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = p.strip_prefix(r"\\?\").filter(|r| r.as_bytes().get(1) == Some(&b':')) {
        rest.to_owned()
    } else {
        return p;
    };
    if plain.len() < 260 { plain } else { p }
}

/// Source row + its first import job (`queued`, or `locked` when the PST is held) in one transaction.
/// An unregistered row at the path is registered again on the same pst_id: its occurrences make every known item a
/// duplicate, and a job the unregister stopped resumes from its checkpoint. Err(AlreadyImported) when registered.
async fn insert_source(inner: &Inner, path: &str, size: i64, state: JobState) -> Result<i64, AppError> {
    let mut c = inner.db.app_client().await?;
    let tx = c.transaction().await.map_err(db_err)?;
    let row = tx
        .query_opt(
            "INSERT INTO pst_sources (path, display_name, file_size_bytes) VALUES ($1, $2, $3) \
             ON CONFLICT ON CONSTRAINT pst_sources_path_once DO NOTHING RETURNING id",
            &[&path, &display_name(path), &size],
        )
        .await
        .map_err(db_err)?;
    let pst_id: i64 = match row {
        Some(row) => row.get(0),
        None => {
            let r = tx.query_one("SELECT id, unregistered FROM pst_sources WHERE path = $1 FOR UPDATE", &[&path]).await.map_err(db_err)?;
            let (pst_id, unregistered): (i64, bool) = (r.get(0), r.get(1));
            if !unregistered {
                return Err(AppError::AlreadyImported { pst_id });
            }
            tx.execute("UPDATE pst_sources SET unregistered = false, paused = false, file_size_bytes = $2 WHERE id = $1", &[&pst_id, &size])
                .await
                .map_err(db_err)?;
            let active = tx
                .query_opt("SELECT 1 FROM import_jobs WHERE pst_id = $1 AND state IN ('queued', 'running')", &[&pst_id])
                .await
                .map_err(db_err)?;
            if active.is_some() {
                tx.commit().await.map_err(db_err)?;
                return Ok(pst_id);
            }
            pst_id
        }
    };
    tx.execute(
        "INSERT INTO import_jobs (pst_id, kind, state, finished_at) VALUES ($1, 'import', $2, CASE WHEN $2 = 'locked' THEN now() END)",
        &[&pst_id, &state.as_db()],
    )
    .await
    .map_err(db_err)?;
    tx.commit().await.map_err(db_err)?;
    Ok(pst_id)
}

pub(crate) async fn register(inner: &Inner, paths: Vec<String>) -> Result<Vec<EnqueueResult>, AppError> {
    if paths.is_empty() {
        return Err(AppError::invalid("paths", "empty"));
    }
    if paths.len() > MAX_REGISTER_PATHS {
        return Err(AppError::invalid("paths", "tooMany"));
    }
    let mut out = Vec::with_capacity(paths.len());
    for input in paths {
        let path = input.clone();
        // file I/O off the async workers; a path that does not resolve keeps its input form and the probe rejects it
        let (path, probed) = tokio::task::spawn_blocking(move || {
            let path = canonical(&path).unwrap_or(path);
            let probed = match probe(&path) {
                Probe::Ready { size, .. } => Ok((size, JobState::Queued)),
                // ponytail: a held PST usually still stats (Windows sharing violations hit open, not stat); size 1 when
                // it does not, the first unlocked attempt writes the real size
                Probe::Locked => Ok((std::fs::metadata(&path).map_or(1, |m| (m.len() as i64).max(1)), JobState::Locked)),
                Probe::Unavailable(error) => Err(error),
            };
            (path, probed)
        })
        .await
        .map_err(|e| AppError::internal(format!("register probe: {e}")))?;
        let inserted = match probed {
            Err(error) => {
                out.push(EnqueueResult::Rejected { path: input, error });
                continue;
            }
            Ok((size, state)) => insert_source(inner, &path, size, state).await,
        };
        let result = match inserted {
            Err(e @ AppError::AlreadyImported { .. }) => EnqueueResult::Rejected { path: input, error: e },
            Err(e) => return Err(e),
            Ok(pst_id) => {
                let pst = emit_status(inner, pst_id).await.ok_or_else(|| AppError::internal("registered source vanished"))?;
                EnqueueResult::Queued { pst }
            }
        };
        out.push(result);
    }
    inner.kick.notify_one();
    Ok(out)
}

/// Locks a listed source row and its newest job for Pause / Unregister; returns the newest job's status.
/// NotFound for an unknown or unregistered id, Busy while a removal is queued or running.
async fn listed_for_update(tx: &tokio_postgres::Transaction<'_>, pst_id: i64) -> Result<ImportStatus, AppError> {
    let Some((_, kind, state)) = newest_job_for_update(tx, pst_id).await? else {
        return Err(AppError::not_found("pst", pst_id));
    };
    let unregistered: bool = tx.query_one("SELECT unregistered FROM pst_sources WHERE id = $1", &[&pst_id]).await.map_err(db_err)?.get(0);
    if unregistered {
        return Err(AppError::not_found("pst", pst_id));
    }
    let status = status_of(kind, state);
    if status == ImportStatus::Removing {
        return Err(AppError::Busy { reason: crate::dto::error::BusyReason::PstRemoving });
    }
    Ok(status)
}

/// The running job of this PST stops after the current item; its checkpoint is committed and it goes back to `queued`.
/// The job is registered as running before its row turns `running`, and the row only turns `running` while the source
/// is neither paused nor unregistered (`runner::run_job`), so a job is either never started or stopped here.
fn hold_running(inner: &Inner, pst_id: i64) {
    if let Some(r) = inner.running().filter(|r| r.pst_id == pst_id) {
        r.cancel.cancel(CancelReason::Hold);
    }
}

/// Pause: no job of the path starts and no sync touches it until Resume; a running sync is held (checkpoint kept).
/// Resume: the held job continues from its checkpoint.
pub(crate) async fn set_paused(inner: &Inner, pst_id: i64, paused: bool) -> Result<PstSource, AppError> {
    let mut c = inner.db.app_client().await?;
    let tx = c.transaction().await.map_err(db_err)?;
    listed_for_update(&tx, pst_id).await?;
    tx.execute("UPDATE pst_sources SET paused = $2 WHERE id = $1", &[&pst_id, &paused]).await.map_err(db_err)?;
    tx.commit().await.map_err(db_err)?;
    if paused {
        hold_running(inner, pst_id);
    } else {
        inner.kick.notify_one();
    }
    emit_status(inner, pst_id).await.ok_or_else(|| AppError::not_found("pst", pst_id))
}

/// Hides the path from the list and from every sync; its folders, occurrences and mail stay (search and reading keep
/// working). A running sync is held; registering the path again continues on the same source row.
pub(crate) async fn unregister(inner: &Inner, pst_id: i64) -> Result<(), AppError> {
    let mut c = inner.db.app_client().await?;
    let tx = c.transaction().await.map_err(db_err)?;
    let status = listed_for_update(&tx, pst_id).await?;
    tx.execute("UPDATE pst_sources SET unregistered = true, paused = false WHERE id = $1", &[&pst_id]).await.map_err(db_err)?;
    tx.commit().await.map_err(db_err)?;
    hold_running(inner, pst_id);
    inner.sink.emit(CoreEvent::ImportStatusChanged(ImportStatusChanged { pst_id, status, removed: true, source: None }));
    Ok(())
}

/// What one sync step did to a path.
#[derive(Debug, PartialEq, Eq)]
enum SyncStep {
    /// A job is active (queued, running, removing) or the source vanished: nothing touched.
    Skipped,
    /// Unchanged since the last completed run, or still locked / unavailable: only timestamps moved.
    Checked,
    Queued,
}

/// One sync step for one path (spec 2026-10-24 PST scheduled sync; arch-2 §2–§4). Unchanged = stat size and mtime
/// equal the checkpoint identity and the newest job is a completed import: no job, no open. Locked / unavailable
/// streaks keep one job row (and one error row) per streak.
/// File I/O (stat, probe) runs on the blocking pool before the transaction, holding no lock (contract override 9: a
/// stalled network share delays only its own path). The transaction then holds the source row and applies the result
/// only when the newest job is still the one the decision was based on, so concurrent syncs of one path (scheduler and
/// "sync now") never both act on it.
async fn sync_one(inner: &Inner, pst_id: i64) -> Result<SyncStep, AppError> {
    let row = {
        let c = inner.db.app_client().await?;
        c.query_opt(
            "SELECT s.path, j.id, j.kind, j.state, cp.pst_file_size, cp.pst_file_mtime FROM pst_sources s \
               CROSS JOIN LATERAL (SELECT id, kind, state FROM import_jobs WHERE pst_id = s.id ORDER BY id DESC LIMIT 1) j \
               LEFT JOIN import_checkpoints cp ON cp.pst_id = s.id WHERE s.id = $1",
            &[&pst_id],
        )
        .await
        .map_err(db_err)?
    };
    let Some(row) = row else { return Ok(SyncStep::Skipped) };
    let (path, seen_job, kind, state): (String, i64, JobKind, JobState) = (row.get(0), row.get(1), JobKind::from_db(row.get(2))?, JobState::from_db(row.get(3))?);
    // A removal owns the path until it succeeds, also after it failed half-way: a re-import would bring back mail the
    // user deleted (review round 1 finding 1).
    if kind == JobKind::Remove || matches!(state, JobState::Queued | JobState::Running) {
        return Ok(SyncStep::Skipped);
    }
    let identity: Option<(i64, SystemTime)> =
        if (kind, state) == (JobKind::Import, JobState::Completed) { row.get::<_, Option<i64>>(4).zip(row.get(5)) } else { None };
    // None = unchanged
    let probed = tokio::task::spawn_blocking(move || {
        let now = std::fs::metadata(&path).ok().and_then(|m| Some((m.len() as i64, truncate_to_micros(m.modified().ok()?))));
        (identity.is_none() || now != identity).then(|| probe(&path))
    })
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;

    let mut c = inner.db.app_client().await?;
    let tx = c.transaction().await.map_err(db_err)?;
    match newest_job_for_update(&tx, pst_id).await? {
        Some((job_id, ..)) if job_id == seen_job => {}
        _ => return Ok(SyncStep::Skipped), // a job started, ended or was added meanwhile: the next sync looks again
    }
    let mut step = SyncStep::Checked;
    match probed {
        None => {}
        Some(Probe::Ready { .. }) => {
            tx.execute("INSERT INTO import_jobs (pst_id, kind, state) VALUES ($1, 'import', 'queued')", &[&pst_id]).await.map_err(db_err)?;
            step = SyncStep::Queued;
        }
        Some(Probe::Locked) if state == JobState::Locked => {
            tx.execute("UPDATE import_jobs SET finished_at = now() WHERE id = $1", &[&seen_job]).await.map_err(db_err)?;
        }
        Some(Probe::Locked) => {
            tx.execute("INSERT INTO import_jobs (pst_id, kind, state, finished_at) VALUES ($1, 'import', 'locked', now())", &[&pst_id])
                .await
                .map_err(db_err)?;
        }
        Some(Probe::Unavailable(e)) => {
            let e = ItemError::new(ImportErrorStage::Open, ImportErrorSeverity::JobFailed, "", 0, &e.to_string());
            // A failed streak keeps its row while the reason stays the same; a different reason is a new failed job so the
            // listed failure reason is the current one.
            let same = state == JobState::Failed
                && tx
                    .query_opt("SELECT 1 FROM import_errors WHERE job_id = $1 AND severity = 'job_failed' AND message = $2", &[&seen_job, &e.message])
                    .await
                    .map_err(db_err)?
                    .is_some();
            if !same {
                let failed: i64 = tx
                    .query_one("INSERT INTO import_jobs (pst_id, kind, state, finished_at) VALUES ($1, 'import', 'failed', now()) RETURNING id", &[&pst_id])
                    .await
                    .map_err(db_err)?
                    .get(0);
                tx.execute(INS_JOB_FAILED, &[&pst_id, &failed, &ImportErrorStage::Open.as_db(), &e.message]).await.map_err(db_err)?;
            }
        }
    }
    tx.execute("UPDATE pst_sources SET last_checked_at = now() WHERE id = $1", &[&pst_id]).await.map_err(db_err)?;
    tx.commit().await.map_err(db_err)?;
    Ok(step)
}

/// `SyncTarget::All` = every listed path; `One(id)` = that path. Both skip a paused path; an unregistered one is not
/// listed (`One`: NotFound).
/// Paths run concurrently, each kicks the worker as soon as it queued a job; the first error is returned after all ran.
pub(crate) async fn sync(inner: &Arc<Inner>, target: SyncTarget) -> Result<(), AppError> {
    let ids: Vec<i64> = {
        let c = inner.db.app_client().await?;
        let rows = match target {
            SyncTarget::All => c.query("SELECT id, paused FROM pst_sources WHERE NOT unregistered ORDER BY id", &[]).await,
            SyncTarget::One(id) => c.query("SELECT id, paused FROM pst_sources WHERE id = $1 AND NOT unregistered", &[&id]).await,
        }
        .map_err(db_err)?;
        if let (SyncTarget::One(id), true) = (target, rows.is_empty()) {
            return Err(AppError::not_found("pst", id));
        }
        rows.iter().filter(|r| !r.get::<_, bool>(1)).map(|r| r.get(0)).collect()
    };
    let mut steps = tokio::task::JoinSet::new();
    for pst_id in ids {
        let inner = inner.clone();
        steps.spawn(async move {
            let step = sync_one(&inner, pst_id).await?;
            if step != SyncStep::Skipped {
                emit_status(&inner, pst_id).await;
            }
            if step == SyncStep::Queued {
                inner.kick.notify_one();
            }
            Ok::<(), AppError>(())
        });
    }
    let mut first_err = None;
    while let Some(r) = steps.join_next().await {
        if let Err(e) = r.map_err(|e| AppError::internal(e.to_string())).and_then(|r| r) {
            first_err.get_or_insert(e);
        }
    }
    first_err.map_or(Ok(()), Err)
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

pub(crate) async fn remove(inner: &Inner, pst_id: i64) -> Result<(), AppError> {
    let mut c = inner.db.app_client().await?;
    let tx = c.transaction().await.map_err(db_err)?;
    let Some((job_id, kind, state)) = newest_job_for_update(&tx, pst_id).await? else {
        return Err(AppError::not_found("pst", pst_id));
    };
    match status_of(kind, state) {
        ImportStatus::Running => return Err(AppError::Busy { reason: crate::dto::error::BusyReason::ImportRunning }),
        ImportStatus::Removing => return Err(AppError::Busy { reason: crate::dto::error::BusyReason::PstRemoving }),
        _ => {}
    }
    if (kind, state) == (JobKind::Import, JobState::Queued) {
        // required by import_jobs_one_active_per_pst (Part 04 §4.3); the removal supersedes it and its checkpoint
        tx.execute("DELETE FROM import_jobs WHERE id = $1", &[&job_id]).await.map_err(db_err)?;
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
        .query("UPDATE import_jobs SET state = 'queued' WHERE state = 'running' RETURNING pst_id", &[])
        .await
        .map_err(db_err)?;
    let mut requeued: Vec<i64> = rows.iter().map(|r| r.get(0)).collect();
    requeued.sort_unstable();
    let report = RecoveryReport { requeued };
    for pst_id in &report.requeued {
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
#[cfg(feature = "test-support")]
pub(super) fn set_synthetic_items(path: &str, folders: Vec<FolderIn>, items: Vec<RawItem>) {
    SYNTHETIC.lock().unwrap_or_else(|p| p.into_inner()).insert(path.to_owned(), (folders, items));
}
#[cfg(not(feature = "test-support"))]
pub(super) fn synthetic_items(_path: &str) -> Option<(Vec<FolderIn>, Vec<RawItem>)> {
    None
}

#[cfg(feature = "test-support")]
pub(crate) async fn enqueue_synthetic(inner: &Inner, path: &str, folders: Vec<FolderIn>, items: Vec<RawItem>) -> Result<i64, AppError> {
    let size = match probe(path) {
        Probe::Ready { size, .. } => size,
        other => return Err(AppError::internal(format!("synthetic source {path} needs a file with the PST magic: {other:?}"))),
    };
    set_synthetic_items(path, folders, items);
    let pst_id = insert_source(inner, path, size, JobState::Queued).await?;
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
            (Import, JobState::Failed, ImportStatus::Failed),
            (Import, JobState::Completed, ImportStatus::Completed),
            (Import, JobState::Locked, ImportStatus::Locked),
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

    #[test]
    fn windows_verbatim_prefix_dropped() {
        assert_eq!(plain_path(r"\\?\C:\Mail\a.pst".into()), r"C:\Mail\a.pst");
        assert_eq!(plain_path(r"\\?\UNC\srv\share\a.pst".into()), r"\\srv\share\a.pst");
        assert_eq!(plain_path(r"\\?\Volume{x}\a.pst".into()), r"\\?\Volume{x}\a.pst");
        assert_eq!(plain_path("/Users/x/a.pst".into()), "/Users/x/a.pst");
        let long = format!(r"\\?\C:\{}.pst", "a".repeat(300));
        assert_eq!(plain_path(long.clone()), long, "beyond MAX_PATH the prefix is needed to open the file");
    }
}
