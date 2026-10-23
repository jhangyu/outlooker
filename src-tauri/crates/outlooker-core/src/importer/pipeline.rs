//! One import attempt (spec Part 02 §4 steps 1–8): reader thread, rayon normalize pool, writer task, from the
//! checkpoint to the end of input, a cancel, a DB pause or the crash seam. Every `ImportTuning` test seam is read
//! here (`hold_reader_after`, `crash_after_batches`, `on_batch`).
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use tokio::sync::{Notify, mpsc, oneshot};
use tokio_postgres::{Client, Row};

use crate::db::indexes::drop_search_indexes;
use crate::db::{db_err, library_counts};
use crate::dto::app::LibraryCounts;
use crate::dto::error::AppError;
use crate::dto::events::LibraryChanged;
use crate::dto::import::{ImportErrorStage, ImportPhase, ImportProgress};
use crate::importer::normalize::{NormalizeCtx, normalize};
use crate::importer::progress::Throughput;
use crate::importer::pst_reader::{self, ReaderMsg, ReaderParams, ScanResult};
use crate::importer::queue::{CancelReason, Inner, JobCancel, synthetic_items};
use crate::importer::types::{FolderIn, NormalizedItem, RawItem};
use crate::importer::writer::{Checkpoint, OnCommit, WriterConfig, WriterHandle, load_known_hashes, truncate_to_micros, upsert_folders};
use crate::sink::CoreEvent;

pub(super) fn progress_event(cp: &Checkpoint, tp: Option<&Throughput>) -> ImportProgress {
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

/// Why a job attempt failed the job (one `job_failed` error row).
pub(super) struct JobFailure {
    pub(super) stage: ImportErrorStage,
    pub(super) message: String,
}

pub(super) enum AttemptEnd {
    /// Input exhausted or cancelled (user / shutdown): writer finished, checkpoint committed.
    Finished,
    /// DB went away (writer DbNotReady or phase change): uncommitted batch dropped.
    Paused,
    /// Test seam crash_after_batches.
    Crashed,
    Failed(JobFailure),
}

/// What ended the wait for the pipeline thread.
enum Wake {
    Done,
    Crash,
    Phase,
}

type LateFailure = Arc<Mutex<Option<JobFailure>>>;

/// One run of the pipeline from the checkpoint (steps 1–8).
pub(super) async fn attempt(inner: &Arc<Inner>, job_id: i64, pst_id: i64, cancel: &Arc<JobCancel>) -> Result<AttemptEnd, AppError> {
    let mut c = inner.db.app_client().await?;
    let row = c.query_one("SELECT path, file_size_bytes, file_mtime FROM pst_sources WHERE id = $1", &[&pst_id]).await.map_err(db_err)?;
    let path: String = row.get(0);
    let synthetic = synthetic_items(&path);
    // (1) file identity
    let (size, mtime) = match file_identity(&path, &row, synthetic.is_some()) {
        Ok(v) => v,
        Err(f) => return Ok(AttemptEnd::Failed(f)),
    };
    // (2) checkpoint
    let (start, reset) = Checkpoint::begin(&c, pst_id, job_id, size, mtime).await?;
    if reset {
        log::info!("pst {pst_id}: file size or mtime changed, import restarts from the beginning");
    }
    // (3) poison item, (4) reader
    let (rx, reader) = spawn_reader(inner, job_id, &path, &start, reset, synthetic, cancel)?;
    let (rx, scanned) = first_message(rx, cancel).await?;
    let scanned = match scanned {
        Ok(s) => s,
        Err(f) => {
            join_thread(reader).await;
            return Ok(AttemptEnd::Failed(f));
        }
    };
    let start = record_scan(&mut c, pst_id, job_id, &scanned, start).await?;
    inner.sink.emit(CoreEvent::ImportProgress(progress_event(&start, None)));
    // (5) index policy (Part 04 §6.4)
    let counts = index_policy(inner, &c, pst_id, scanned.items_total - start.cursor_seq).await?;
    // (6) writer
    let quality: i16 = c.query_one("SELECT webp_quality FROM settings", &[]).await.map_err(db_err)?.get(0);
    let known = Arc::new(load_known_hashes(&c).await?);
    drop(c);
    let crash = Arc::new(Notify::new());
    let sent_bytes = Arc::new(AtomicU64::new(0));
    let on_commit = on_commit(inner, &crash, &sent_bytes, &start, counts);
    let cfg = WriterConfig { pst_id, data_root: inner.db.data_root()?, start: start.clone(), limits: inner.tuning.writer.clone(), known: known.clone() };
    let writer = WriterHandle::spawn(inner.db.connect_app().await?, cfg, on_commit).await?;
    // (7) pipeline thread: rayon pool runs normalize, results go to the writer
    let ctx = NormalizeCtx { webp_quality: quality.clamp(1, 100) as u8, known };
    let (pipeline, done_rx, late_failure) = spawn_pipeline(rx, ctx, writer.sender(), cancel.clone(), sent_bytes);
    match wait_wake(inner, done_rx, &crash).await {
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
    let late = late_failure.lock().unwrap_or_else(|p| p.into_inner()).take();
    Ok(end_of_input(finished, late, cancel.reason()))
}

/// Size and mtime of the PST now; a synthetic source without a file keeps the identity stored at enqueue.
fn file_identity(path: &str, row: &Row, synthetic: bool) -> Result<(i64, SystemTime), JobFailure> {
    match std::fs::metadata(path) {
        Ok(m) => Ok((m.len() as i64, truncate_to_micros(m.modified().unwrap_or(UNIX_EPOCH)))),
        Err(_) if synthetic => Ok((row.get(1), row.get(2))),
        Err(e) => Err(JobFailure { stage: ImportErrorStage::Open, message: format!("cannot open PST file: {e}") }),
    }
}

/// Steps 3–4: the pending poison record and the reader thread (synthetic items when the test source exists).
fn spawn_reader(
    inner: &Inner,
    job_id: i64,
    path: &str,
    start: &Checkpoint,
    reset: bool,
    synthetic: Option<(Vec<FolderIn>, Vec<RawItem>)>,
    cancel: &JobCancel,
) -> Result<(Receiver<ReaderMsg>, JoinHandle<()>), AppError> {
    // the marker of this job is the pending record; another job's marker is ignored (Part 02 §6.3 rule 1)
    let inflight = inner.inflight()?;
    // A reset checkpoint (file changed) renumbers the items: the marker's seq may name another item, so drop it.
    let pending = if reset { None } else { inflight.read().filter(|r| r.job_id == job_id) };
    let params = ReaderParams {
        path: PathBuf::from(path),
        cursor_seq: start.cursor_seq,
        pending,
        inflight: Some(inflight),
        cancel: cancel.flag.clone(),
        // test seam: only the first attempt of this manager holds (a re-run after a pause or resume never blocks)
        hold_after: inner.tuning.hold_reader_after.filter(|_| !inner.hold_used.swap(true, Ordering::SeqCst)),
        budget: cancel.budget.clone(),
        job_id,
    };
    Ok(match synthetic {
        #[cfg(feature = "test-support")]
        Some((folders, items)) => pst_reader::spawn_synthetic(params, folders, items),
        _ => pst_reader::spawn(params),
    })
}

/// The reader's first message must be the scan result. The receiver is returned in every case so it outlives
/// the reader join.
async fn first_message(rx: Receiver<ReaderMsg>, cancel: &JobCancel) -> Result<(Receiver<ReaderMsg>, Result<ScanResult, JobFailure>), AppError> {
    let (first, rx) = tokio::task::spawn_blocking(move || (rx.recv(), rx)).await.map_err(|e| AppError::internal(e.to_string()))?;
    let scanned = match first {
        Ok(ReaderMsg::Scanned(s)) => Ok(s),
        Ok(ReaderMsg::Failed { stage, message }) => Err(JobFailure { stage, message }),
        Ok(ReaderMsg::Item(_)) | Err(_) => {
            cancel.stop();
            Err(JobFailure { stage: ImportErrorStage::Scan, message: "reader stopped before the scan finished".into() })
        }
    };
    Ok((rx, scanned))
}

/// Folders, scan error rows, phase `importing` with the item total. Returns the checkpoint the writer starts from.
async fn record_scan(c: &mut Client, pst_id: i64, job_id: i64, scanned: &ScanResult, start: Checkpoint) -> Result<Checkpoint, AppError> {
    upsert_folders(c, pst_id, &scanned.folders).await?;
    for e in &scanned.scan_errors {
        c.execute(
            "INSERT INTO import_errors (pst_id, job_id, stage, severity, folder_path, message) VALUES ($1, $2, 'scan', 'degraded', $3, $4)",
            &[&pst_id, &job_id, &e.folder_path, &e.message],
        )
        .await
        .map_err(db_err)?;
    }
    Checkpoint::set_phase(c, pst_id, ImportPhase::Importing, Some(scanned.items_total)).await?;
    Ok(Checkpoint { phase: ImportPhase::Importing, items_total: Some(scanned.items_total), ..start })
}

/// Part 04 §6.4: search indexes are dropped until finalize when the remaining items are at least half the library.
/// Returns the library counts before the import.
async fn index_policy(inner: &Inner, c: &Client, pst_id: i64, remaining: i64) -> Result<LibraryCounts, AppError> {
    let counts = library_counts(c).await?;
    if 2 * remaining >= counts.messages {
        let admin = inner.db.connect_admin().await?;
        drop_search_indexes(&admin).await?;
        log::info!("pst {pst_id}: {remaining} items for {} messages: search indexes dropped until finalize", counts.messages);
    }
    Ok(counts)
}

/// Writer commit callback: progress and library events, test seams `on_batch` and `crash_after_batches`.
fn on_commit(inner: &Arc<Inner>, crash: &Arc<Notify>, sent_bytes: &Arc<AtomicU64>, start: &Checkpoint, counts: LibraryCounts) -> OnCommit {
    let inner = inner.clone();
    let crash = crash.clone();
    let sent_bytes = sent_bytes.clone();
    let start_batches = start.batches_committed;
    let start_stored = start.stored;
    let LibraryCounts { messages, psts } = counts;
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
}

/// Step 7: the pipeline thread drains the reader through a rayon normalize pool into the writer. Returns the
/// thread, its done signal and the slot for a reader failure that arrives after the scan.
fn spawn_pipeline(
    rx: Receiver<ReaderMsg>,
    ctx: NormalizeCtx,
    sender: mpsc::Sender<NormalizedItem>,
    flag: Arc<JobCancel>,
    sent_bytes: Arc<AtomicU64>,
) -> (JoinHandle<()>, oneshot::Receiver<()>, LateFailure) {
    let late_failure: LateFailure = Arc::new(Mutex::new(None));
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
    (pipeline, done_rx, late_failure)
}

/// Waits for the end of input, the crash seam, or a phase change (Supervisor restarting PostgreSQL).
async fn wait_wake(inner: &Inner, done_rx: oneshot::Receiver<()>, crash: &Notify) -> Wake {
    let mut phase = inner.phase.clone();
    phase.borrow_and_update();
    tokio::select! {
        _ = done_rx => Wake::Done,
        _ = crash.notified() => Wake::Crash,
        _ = phase.changed() => Wake::Phase,
    }
}

/// Step 8 outcome: a late reader failure first, then the writer's result.
fn end_of_input(finished: Result<Checkpoint, AppError>, late: Option<JobFailure>, reason: Option<CancelReason>) -> AttemptEnd {
    if let Some(f) = late {
        return AttemptEnd::Failed(f);
    }
    match finished {
        Ok(_) if reason == Some(CancelReason::Pause) => AttemptEnd::Paused,
        Ok(_) => AttemptEnd::Finished,
        Err(AppError::DbNotReady) => AttemptEnd::Paused,
        Err(AppError::FileAccess { reason, .. }) if reason == "insufficientSpace" => {
            AttemptEnd::Failed(JobFailure { stage: ImportErrorStage::Store, message: "disk space low".into() })
        }
        Err(e) => AttemptEnd::Failed(JobFailure { stage: ImportErrorStage::Store, message: e.to_string() }),
    }
}

async fn join_thread(h: JoinHandle<()>) {
    let _ = tokio::task::spawn_blocking(move || h.join()).await;
}
