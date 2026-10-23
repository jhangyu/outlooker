//! Per-key event throttle (spec Part 02 §3.1, Part 03 §9): pure state, time is passed in.
use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::CoreEvent;
use crate::dto::events::DataDirMovePhase;
use crate::dto::import::{ImportPhase, ImportStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThrottleKey {
    ImportProgress(i64),
    LibraryChanged,
    DataDirMoveProgress,
}

pub const IMPORT_PROGRESS_WINDOW: Duration = Duration::from_millis(250);
pub const LIBRARY_CHANGED_WINDOW: Duration = Duration::from_secs(5);
pub const MOVE_PROGRESS_WINDOW: Duration = Duration::from_millis(250);

/// Throttled events and their windows; every other event is emitted at once.
pub fn throttle_key(e: &CoreEvent) -> Option<(ThrottleKey, Duration)> {
    match e {
        CoreEvent::ImportProgress(p) => Some((ThrottleKey::ImportProgress(p.pst_id), IMPORT_PROGRESS_WINDOW)),
        CoreEvent::LibraryChanged(_) => Some((ThrottleKey::LibraryChanged, LIBRARY_CHANGED_WINDOW)),
        CoreEvent::DataDirMoveProgress(_) => Some((ThrottleKey::DataDirMoveProgress, MOVE_PROGRESS_WINDOW)),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Import(ImportPhase),
    Move(DataDirMovePhase),
}

fn phase(e: &CoreEvent) -> Option<Phase> {
    match e {
        CoreEvent::ImportProgress(p) => Some(Phase::Import(p.phase)),
        CoreEvent::DataDirMoveProgress(p) => Some(Phase::Move(p.phase)),
        _ => None,
    }
}

/// Part 03 §9 "plus once when a job ends": these status changes flush a pending `library:changed`.
fn ends_job(e: &CoreEvent) -> bool {
    matches!(e, CoreEvent::ImportStatusChanged(c)
        if c.removed
            || matches!(c.status, ImportStatus::Completed | ImportStatus::Failed | ImportStatus::Cancelled | ImportStatus::Queued))
}

struct Slot {
    window: Duration,
    last_emit: Option<Instant>,
    last_phase: Option<Phase>,
    pending: Option<CoreEvent>,
}

impl Slot {
    fn window_over(&self, now: Instant) -> bool {
        self.last_emit.is_none_or(|t| now.duration_since(t) >= self.window)
    }

    fn mark(&mut self, now: Instant, e: &CoreEvent) {
        self.pending = None;
        self.last_emit = Some(now);
        self.last_phase = phase(e);
    }
}

/// Pure throttle state; time is passed in, so tests drive it with explicit `Instant`s.
#[derive(Default)]
pub struct Throttle {
    slots: HashMap<ThrottleKey, Slot>,
}

impl Throttle {
    /// Events to emit now for `e` offered at `now`.
    pub fn offer(&mut self, e: CoreEvent, now: Instant) -> Vec<CoreEvent> {
        let Some((key, window)) = throttle_key(&e) else {
            let flush = ends_job(&e);
            let mut out = vec![e];
            if flush
                && let Some(slot) = self.slots.get_mut(&ThrottleKey::LibraryChanged)
                && let Some(pending) = slot.pending.take()
            {
                slot.mark(now, &pending);
                out.push(pending);
            }
            return out;
        };
        let slot = self.slots.entry(key).or_insert(Slot { window, last_emit: None, last_phase: None, pending: None });
        if slot.window_over(now) || phase(&e) != slot.last_phase {
            slot.mark(now, &e);
            vec![e]
        } else {
            slot.pending = Some(e);
            Vec::new()
        }
    }

    /// Trailing values whose window ended at `now`.
    pub fn due(&mut self, now: Instant) -> Vec<CoreEvent> {
        let mut out = Vec::new();
        for slot in self.slots.values_mut() {
            if slot.pending.is_some() && slot.window_over(now) {
                let pending = slot.pending.take().expect("checked above");
                slot.mark(now, &pending);
                out.push(pending);
            }
        }
        out
    }

    /// Earliest instant at which `due` returns something.
    pub fn next_deadline(&self) -> Option<Instant> {
        self.slots
            .values()
            .filter(|s| s.pending.is_some())
            .filter_map(|s| s.last_emit.map(|t| t + s.window))
            .min()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::app::LibraryCounts;
    use crate::dto::events::{DataDirMoveProgress, ImportStatusChanged, LibraryChanged};
    use crate::dto::import::ImportProgress;

    fn progress(pst_id: i64, phase: ImportPhase, items_done: i64) -> CoreEvent {
        CoreEvent::ImportProgress(ImportProgress {
            pst_id,
            phase,
            items_total: Some(1000),
            items_done,
            stored: items_done,
            duplicates: 0,
            errors: 0,
            skipped_non_mail: 0,
            images_converted: 0,
            bytes_per_sec: None,
            eta_seconds: None,
        })
    }

    fn library(messages: i64) -> CoreEvent {
        CoreEvent::LibraryChanged(LibraryChanged { counts: LibraryCounts { messages, psts: 1 } })
    }

    fn status(pst_id: i64, status: ImportStatus) -> CoreEvent {
        CoreEvent::ImportStatusChanged(ImportStatusChanged { pst_id, status, removed: false, source: None })
    }

    fn moving(phase: DataDirMovePhase) -> CoreEvent {
        CoreEvent::DataDirMoveProgress(DataDirMoveProgress { phase, bytes_copied: 0, bytes_total: 10, error: None })
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn throttle_at_most_four_per_second_per_key() {
        let t0 = Instant::now();
        let mut th = Throttle::default();
        let mut emitted: Vec<Duration> = Vec::new();
        for i in 0..100u64 {
            let now = t0 + ms(i * 10);
            for _ in th.offer(progress(1, ImportPhase::Importing, i as i64), now) {
                emitted.push(now - t0);
            }
            for _ in th.due(now) {
                emitted.push(now - t0);
            }
        }
        assert!(emitted.len() >= 4, "too few events: {emitted:?}");
        for &start in &emitted {
            let n = emitted.iter().filter(|&&t| t >= start && t < start + Duration::from_secs(1)).count();
            assert!(n <= 4, "{n} events in the 1 s window starting at {start:?}: {emitted:?}");
        }
    }

    #[test]
    fn throttle_always_emits_trailing_value() {
        let t0 = Instant::now();
        let mut th = Throttle::default();
        assert_eq!(th.offer(progress(1, ImportPhase::Importing, 1), t0).len(), 1);
        assert!(th.offer(progress(1, ImportPhase::Importing, 2), t0 + ms(10)).is_empty());
        assert!(th.offer(progress(1, ImportPhase::Importing, 3), t0 + ms(20)).is_empty());
        assert!(th.due(t0 + ms(100)).is_empty());
        assert_eq!(th.next_deadline(), Some(t0 + ms(250)));
        assert_eq!(th.due(t0 + ms(250)), vec![progress(1, ImportPhase::Importing, 3)]);
        assert_eq!(th.next_deadline(), None);
        assert_eq!(th.offer(library(1), t0).len(), 1);
        assert!(th.offer(library(2), t0 + ms(1000)).is_empty());
        assert!(th.due(t0 + ms(4999)).is_empty());
        assert_eq!(th.due(t0 + Duration::from_secs(5)), vec![library(2)]);
    }

    #[test]
    fn throttle_emits_phase_change_immediately() {
        let t0 = Instant::now();
        let mut th = Throttle::default();
        assert_eq!(th.offer(progress(1, ImportPhase::Scanning, 0), t0).len(), 1);
        assert!(th.offer(progress(1, ImportPhase::Scanning, 0), t0 + ms(10)).is_empty());
        assert_eq!(
            th.offer(progress(1, ImportPhase::Importing, 1), t0 + ms(50)),
            vec![progress(1, ImportPhase::Importing, 1)]
        );
        assert!(th.offer(progress(1, ImportPhase::Importing, 2), t0 + ms(60)).is_empty());
        assert_eq!(th.offer(progress(1, ImportPhase::Finalizing, 3), t0 + ms(70)).len(), 1);
        assert_eq!(th.offer(progress(2, ImportPhase::Importing, 1), t0 + ms(70)).len(), 1, "each PST has its own key");
        assert_eq!(th.offer(moving(DataDirMovePhase::Copying), t0).len(), 1);
        assert!(th.offer(moving(DataDirMovePhase::Copying), t0 + ms(5)).is_empty());
        assert_eq!(th.offer(moving(DataDirMovePhase::Verifying), t0 + ms(6)).len(), 1);
    }

    #[test]
    fn job_end_flushes_library_changed() {
        let t0 = Instant::now();
        let mut th = Throttle::default();
        assert_eq!(th.offer(library(1), t0).len(), 1);
        assert!(th.offer(library(2), t0 + ms(1000)).is_empty());
        assert_eq!(
            th.offer(status(1, ImportStatus::Running), t0 + ms(1500)),
            vec![status(1, ImportStatus::Running)],
            "a running job does not flush"
        );
        assert_eq!(
            th.offer(status(1, ImportStatus::Completed), t0 + ms(2000)),
            vec![status(1, ImportStatus::Completed), library(2)]
        );
        assert!(th.due(t0 + Duration::from_secs(60)).is_empty(), "the flushed value is not emitted twice");
        assert_eq!(th.offer(status(1, ImportStatus::Completed), t0 + ms(2100)).len(), 1);
    }
}
