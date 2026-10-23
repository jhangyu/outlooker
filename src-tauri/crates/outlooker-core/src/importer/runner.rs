//! Import / removal job runner: the queue worker loop, one job from `running` to its final state.
use std::sync::Arc;
use std::time::Duration;

use crate::db::indexes::ensure_search_indexes;
use crate::db::{db_err, library_counts};
use crate::dto::app::AppPhase;
use crate::dto::error::AppError;
use crate::dto::events::{ImportStatusChanged, LibraryChanged};
use crate::dto::import::{ImportErrorSeverity, ImportErrorStage, ImportPhase, ImportStatus};
use crate::gate::Activity;
use crate::importer::pipeline::{AttemptEnd, JobFailure, attempt, progress_event};
use crate::importer::progress::Throughput;
use crate::importer::queue::{CancelReason, Inner, JobCancel, JobKind, JobState, RunningJob, emit_status};
use crate::importer::remove;
use crate::importer::types::ItemError;
use crate::importer::writer::{AFTER_JOB_VACUUM, Checkpoint};
use crate::sink::CoreEvent;

/// How long `pause_for_db` waits for the Supervisor to leave `Ready` after a DB error (Part 02 §6.1 PG crash row).
const PAUSE_DETECT: Duration = Duration::from_secs(30);

async fn emit_library(inner: &Inner) {
    let counts = async { library_counts(&*inner.db.app_client().await?).await };
    if let Ok(counts) = counts.await {
        inner.sink.emit(CoreEvent::LibraryChanged(LibraryChanged { counts }));
    }
}

enum Next {
    Job { id: i64, pst_id: i64, kind: JobKind },
    Idle,
}

pub(super) async fn worker(inner: Arc<Inner>) {
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
                let activity = if kind == JobKind::Remove { Activity::Removal } else { Activity::Import };
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
                let crashed = run_job(&inner, id, pst_id, kind).await;
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
    match row {
        None => Ok(Next::Idle),
        Some(r) => Ok(Next::Job { id: r.get(0), pst_id: r.get(1), kind: JobKind::from_db(r.get(2))? }),
    }
}

/// Takes the job (`queued` → `running`) and runs it. Returns true when the crash seam dropped it.
async fn run_job(inner: &Arc<Inner>, job_id: i64, pst_id: i64, kind: JobKind) -> bool {
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
            if kind == JobKind::Remove {
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
                if set_final(inner, job_id, pst_id, JobState::Failed, Some(&failure)).await.is_ok() {
                    emit_status(inner, pst_id).await;
                    let _ = inner.events.send((pst_id, ImportStatus::Failed));
                }
                return;
            }
        }
    }
}

/// Final job row state (+ the job_failed error row in the same transaction).
async fn set_final(inner: &Inner, job_id: i64, pst_id: i64, state: JobState, failure: Option<&JobFailure>) -> Result<(), AppError> {
    let mut c = inner.db.app_client().await?;
    let tx = c.transaction().await.map_err(db_err)?;
    if let Some(f) = failure {
        let e = ItemError::new(f.stage, ImportErrorSeverity::JobFailed, "", 0, &f.message);
        tx.execute(
            "INSERT INTO import_errors (pst_id, job_id, stage, severity, message) VALUES ($1, $2, $3, 'job_failed', $4)",
            &[&pst_id, &job_id, &f.stage.as_db(), &e.message],
        )
        .await
        .map_err(db_err)?;
    }
    tx.execute(
        "UPDATE import_jobs SET state = $2, finished_at = CASE WHEN $2 = 'queued' THEN NULL ELSE now() END WHERE id = $1",
        &[&job_id, &state.as_db()],
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

/// What `run_import` does with the job once its attempt loop ended.
#[derive(Debug, PartialEq, Eq)]
enum EndAction {
    /// Crash seam: no state change, the worker stops.
    Crash,
    /// Shutdown without a failure: state `queued`, resumed at the next start.
    Requeue,
    /// Finalize (indexes, VACUUM, marker), then this final state and status.
    Finalize(JobState, ImportStatus),
}

/// End-of-attempt decision (Part 03 §7.1). A failure beats any cancel reason; `Paused` never gets here (the loop
/// re-runs or returns) and decides like `Finished`.
fn end_of_attempt(end: &AttemptEnd, reason: Option<CancelReason>) -> EndAction {
    match (end, reason) {
        (AttemptEnd::Crashed, _) => EndAction::Crash,
        (AttemptEnd::Failed(_), _) => EndAction::Finalize(JobState::Failed, ImportStatus::Failed),
        (_, Some(CancelReason::Shutdown)) => EndAction::Requeue,
        (_, Some(CancelReason::User)) => EndAction::Finalize(JobState::Cancelled, ImportStatus::Cancelled),
        (_, None | Some(CancelReason::Pause)) => EndAction::Finalize(JobState::Completed, ImportStatus::Completed),
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
    let (state, status) = match end_of_attempt(&end, cancel.reason()) {
        EndAction::Crash => {
            let _ = inner.events.send((pst_id, ImportStatus::Running));
            return true;
        }
        EndAction::Requeue => {
            // Part 02 §2.5 step 1: back to queued; index self-heal at the next start covers dropped indexes
            if let Ok(m) = inner.inflight() {
                m.clear();
            }
            if set_final(inner, job_id, pst_id, JobState::Queued, None).await.is_ok() {
                emit_status(inner, pst_id).await;
                let _ = inner.events.send((pst_id, ImportStatus::Queued));
            }
            return false;
        }
        EndAction::Finalize(state, status) => (state, status),
    };
    let failure = match end {
        AttemptEnd::Failed(f) => Some(f),
        _ => None,
    };
    if let Err(e) = finalize(inner, pst_id).await {
        log::warn!("import job {job_id}: finalize: {e}"); // DB gone: row stays running, recovered at next start
        return false;
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn end_of_attempt_table() {
        use CancelReason::{Pause, Shutdown, User};
        use EndAction::{Crash, Finalize, Requeue};
        let failed = || AttemptEnd::Failed(JobFailure { stage: ImportErrorStage::Store, message: "x".into() });
        let completed = Finalize(JobState::Completed, ImportStatus::Completed);
        let cancelled = Finalize(JobState::Cancelled, ImportStatus::Cancelled);
        let failure = Finalize(JobState::Failed, ImportStatus::Failed);
        let rows = [
            (AttemptEnd::Finished, None, &completed),
            (AttemptEnd::Finished, Some(Pause), &completed),
            (AttemptEnd::Finished, Some(User), &cancelled),
            (AttemptEnd::Finished, Some(Shutdown), &Requeue),
            (AttemptEnd::Paused, None, &completed),
            (AttemptEnd::Paused, Some(Pause), &completed),
            (AttemptEnd::Paused, Some(User), &cancelled),
            (AttemptEnd::Paused, Some(Shutdown), &Requeue),
            (AttemptEnd::Crashed, None, &Crash),
            (AttemptEnd::Crashed, Some(Pause), &Crash),
            (AttemptEnd::Crashed, Some(User), &Crash),
            (AttemptEnd::Crashed, Some(Shutdown), &Crash),
            (failed(), None, &failure),
            (failed(), Some(Pause), &failure),
            (failed(), Some(User), &failure),
            (failed(), Some(Shutdown), &failure),
        ];
        for (i, (end, reason, want)) in rows.iter().enumerate() {
            assert_eq!(&end_of_attempt(end, *reason), *want, "row {i}: {reason:?}");
        }
    }
}

