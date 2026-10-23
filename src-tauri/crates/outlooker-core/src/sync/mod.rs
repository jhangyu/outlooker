//! Sync scheduler: runs `ImportManager::sync(All)` on the schedule stored in the DB `settings` row, once after
//! downtime, never while the app is not Ready. It never takes the Work Gate (the worker does, per job) and only one
//! task runs, so scheduled runs never overlap; manual and scheduled runs are serialized inside `ImportManager::sync`.
pub mod schedule;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use time::{OffsetDateTime, UtcOffset};
use tokio::sync::{watch, Notify};

use crate::db::{db_err, Db};
use crate::dto::app::AppPhase;
use crate::dto::error::AppError;
use crate::dto::sync::{SyncSchedule, SyncStatus};
use crate::importer::ImportManager;
use crate::sink::{CoreEvent, EventSink};
use schedule::{due_at, local_offset};

/// Tokio sleeps on a monotonic clock that stops while the laptop sleeps; re-reading the wall clock at least this
/// often also covers clock, zone and DST changes.
const RECHECK: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub struct Scheduler {
    inner: Arc<Inner>,
}

struct Inner {
    db: Db,
    importer: ImportManager,
    sink: Arc<dyn EventSink>,
    phase: watch::Receiver<AppPhase>,
    wake: Notify,
    shutdown: watch::Sender<bool>,
    task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl Scheduler {
    pub fn new(db: Db, importer: ImportManager, sink: Arc<dyn EventSink>, phase: watch::Receiver<AppPhase>) -> Scheduler {
        let inner = Inner { db, importer, sink, phase, wake: Notify::new(), shutdown: watch::Sender::new(false), task: Mutex::new(None) };
        Scheduler { inner: Arc::new(inner) }
    }

    /// Spawns the scheduler task once (later calls do nothing).
    pub fn start(&self) {
        let mut slot = self.inner.task.lock().unwrap_or_else(|p| p.into_inner());
        if slot.is_none() {
            *slot = Some(tokio::spawn(run(self.inner.clone())));
        }
    }

    /// Stops the task and waits for it; a run in progress (probe + job inserts, no import work) finishes first.
    pub async fn stop(&self) {
        self.inner.shutdown.send_replace(true);
        let handle = self.inner.task.lock().unwrap_or_else(|p| p.into_inner()).take();
        if let Some(h) = handle {
            let _ = h.await;
        }
    }

    pub async fn status(&self) -> Result<SyncStatus, AppError> {
        let (schedule, last) = load(&self.inner.db).await?;
        Ok(status_of(schedule, last, OffsetDateTime::now_utc()))
    }

    /// Validates and stores the schedule; the task recomputes the next run and emits `sync:status-changed`.
    pub async fn set_schedule(&self, s: SyncSchedule) -> Result<SyncStatus, AppError> {
        s.validate()?;
        store(&self.inner.db, &s).await?;
        self.inner.wake.notify_one();
        self.status().await
    }

    /// "Sync now": all enabled paths, or one path even when disabled. Does not move the schedule.
    pub async fn sync_now(&self, pst_id: Option<i64>) -> Result<(), AppError> {
        self.inner.importer.sync(pst_id.map_or(SyncTarget::All, SyncTarget::One)).await
    }
}

async fn run(inner: Arc<Inner>) {
    let mut phase = inner.phase.clone();
    let mut shutdown = inner.shutdown.subscribe();
    let mut emitted: Option<SyncStatus> = None;
    loop {
        if *shutdown.borrow_and_update() {
            return;
        }
        if *phase.borrow_and_update() != AppPhase::Ready {
            tokio::select! {
                r = phase.changed() => if r.is_err() { return },
                _ = shutdown.changed() => return,
            }
            continue;
        }
        let woken = inner.wake.notified();
        tokio::pin!(woken);
        woken.as_mut().enable(); // a schedule change during the step below is not lost
        let wait = match step(&inner, &mut emitted).await {
            Ok(wait) => wait,
            Err(e) => {
                log::warn!("sync scheduler: {e}");
                RECHECK
            }
        };
        tokio::select! {
            _ = tokio::time::sleep(wait) => {},
            _ = woken => {},
            r = phase.changed() => if r.is_err() { return },
            _ = shutdown.changed() => return,
        }
    }
}

/// Runs the sync when due, emits the status when it changed, returns how long to wait.
async fn step(inner: &Inner, emitted: &mut Option<SyncStatus>) -> Result<Duration, AppError> {
    let (schedule, mut last) = load(&inner.db).await?;
    let now = OffsetDateTime::now_utc();
    if due_at(&schedule, last, now, &local_offset).is_some_and(|due| due <= now) {
        // Recorded first: a failing sync waits for the next slot instead of retrying every minute.
        store_last_run(&inner.db, now).await?;
        last = Some(now);
        if let Err(e) = inner.importer.sync(SyncTarget::All).await {
            log::warn!("scheduled sync: {e}");
        }
    }
    let status = status_of(schedule.clone(), last, now);
    if emitted.as_ref() != Some(&status) {
        inner.sink.emit(CoreEvent::SyncStatusChanged(status.clone()));
        *emitted = Some(status);
    }
    Ok(match due_at(&schedule, last, now, &local_offset) {
        Some(due) => Duration::try_from(due - now).unwrap_or(Duration::ZERO).min(RECHECK),
        None => RECHECK,
    })
}

fn status_of(schedule: SyncSchedule, last: Option<OffsetDateTime>, now: OffsetDateTime) -> SyncStatus {
    let next = due_at(&schedule, last, now, &local_offset);
    SyncStatus { schedule, last_run_at: last.map(rfc3339), next_run_at: next.map(rfc3339) }
}

/// RFC 3339 UTC with whole seconds, like every other timestamp in the API (`db::RFC3339_UTC_FMT`).
fn rfc3339(t: OffsetDateTime) -> String {
    let u = t.to_offset(UtcOffset::UTC);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", u.year(), u8::from(u.month()), u.day(), u.hour(), u.minute(), u.second())
}

async fn load(db: &Db) -> Result<(SyncSchedule, Option<OffsetDateTime>), AppError> {
    let c = db.app_client().await?;
    let row = c
        .query_one(
            "SELECT sync_mode, sync_interval_minutes, sync_daily_minute, sync_weekdays, \
             floor(extract(epoch FROM sync_last_run_at))::int8 FROM settings",
            &[],
        )
        .await
        .map_err(db_err)?;
    let (mode, minutes, daily_minute, mask): (String, i32, i16, i16) = (row.get(0), row.get(1), row.get(2), row.get(3));
    let schedule = match mode.as_str() {
        "off" => SyncSchedule::Off,
        "interval" => SyncSchedule::Interval { minutes: minutes as u32 },
        "daily" => SyncSchedule::Daily {
            time: format!("{:02}:{:02}", daily_minute / 60, daily_minute % 60),
            weekdays: (1..=7u8).filter(|d| mask & (1 << (d - 1)) != 0).collect(),
        },
        other => return Err(AppError::internal(format!("unknown sync_mode {other}"))),
    };
    let last = row
        .get::<_, Option<i64>>(4)
        .map(|s| OffsetDateTime::from_unix_timestamp(s).map_err(|e| AppError::internal(format!("sync_last_run_at: {e}"))))
        .transpose()?;
    Ok((schedule, last))
}

/// Only the active mode's fields change; the other mode keeps its values.
async fn store(db: &Db, s: &SyncSchedule) -> Result<(), AppError> {
    let c = db.app_client().await?;
    let r = match s {
        SyncSchedule::Off => c.execute("UPDATE settings SET sync_mode = 'off', updated_at = now()", &[]).await,
        SyncSchedule::Interval { minutes } => {
            let minutes = *minutes as i32;
            c.execute("UPDATE settings SET sync_mode = 'interval', sync_interval_minutes = $1, updated_at = now()", &[&minutes]).await
        }
        SyncSchedule::Daily { time, weekdays } => {
            // validate() ran: both parse
            let minute = SyncSchedule::parse_time(time).unwrap_or_default() as i16;
            let mask = i16::from(SyncSchedule::weekday_mask(weekdays).unwrap_or_default());
            c.execute(
                "UPDATE settings SET sync_mode = 'daily', sync_daily_minute = $1, sync_weekdays = $2, updated_at = now()",
                &[&minute, &mask],
            )
            .await
        }
    };
    r.map(drop).map_err(db_err)
}

async fn store_last_run(db: &Db, at: OffsetDateTime) -> Result<(), AppError> {
    let c = db.app_client().await?;
    c.execute("UPDATE settings SET sync_last_run_at = to_timestamp($1::float8)", &[&(at.unix_timestamp() as f64)])
        .await
        .map(drop)
        .map_err(db_err)
}

// MERGE-STUB (deliverable A owns the real ones in importer/): delete this block at merge and import
// `crate::importer::SyncTarget`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncTarget {
    All,
    One(i64),
}
impl ImportManager {
    pub async fn sync(&self, _target: SyncTarget) -> Result<(), AppError> {
        Ok(())
    }
}
