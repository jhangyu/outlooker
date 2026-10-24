//! Throughput / ETA over the last 10 s and the 256 MB reader → writer byte budget (spec Part 03 §7.1, Part 04 §5.1).
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

pub const BYTE_BUDGET: u64 = 256 * 1024 * 1024;
const WINDOW: Duration = Duration::from_secs(10);

pub struct Throughput {
    samples: VecDeque<(Instant, i64, u64)>,
}

impl Default for Throughput {
    fn default() -> Self {
        Throughput::new()
    }
}

impl Throughput {
    pub fn new() -> Throughput {
        Throughput { samples: VecDeque::new() }
    }

    /// `items_done` and `bytes` are running totals.
    pub fn record(&mut self, now: Instant, items_done: i64, bytes: u64) {
        self.samples.push_back((now, items_done, bytes));
        // keep one sample at or before the window start, so the rate always spans the full window
        while self.samples.len() > 2 && now.duration_since(self.samples[1].0) >= WINDOW {
            self.samples.pop_front();
        }
    }

    fn span(&self) -> Option<(f64, i64, u64)> {
        let (t0, i0, b0) = *self.samples.front()?;
        let (t1, i1, b1) = *self.samples.back()?;
        let secs = t1.duration_since(t0).as_secs_f64();
        (secs > 0.0).then_some((secs, i1 - i0, b1.saturating_sub(b0)))
    }

    pub fn bytes_per_sec(&self) -> Option<i64> {
        self.span().map(|(secs, _, bytes)| (bytes as f64 / secs) as i64)
    }

    pub fn eta_seconds(&self, items_total: Option<i64>, items_done: i64) -> Option<i64> {
        let (secs, items, _) = self.span()?;
        if items <= 0 {
            return None;
        }
        let remaining = (items_total? - items_done).max(0);
        Some((remaining as f64 * secs / items as f64).ceil() as i64)
    }
}

/// Pending raw bytes between the reader and the writer (Mutex + Condvar; the reader thread blocks, never spins).
pub struct ByteBudget {
    limit: u64,
    used: Mutex<u64>,
    changed: Condvar,
}

impl ByteBudget {
    pub fn new(limit: u64) -> ByteBudget {
        ByteBudget { limit, used: Mutex::new(0), changed: Condvar::new() }
    }

    /// Blocks while `used + n > limit` (an item larger than the whole budget passes when nothing is pending),
    /// unless `cancel` is set.
    pub fn acquire(&self, n: u64, cancel: &AtomicBool) {
        let mut used = self.used.lock().unwrap_or_else(|p| p.into_inner());
        while *used > 0 && *used + n > self.limit && !cancel.load(Ordering::SeqCst) {
            used = self.changed.wait(used).unwrap_or_else(|p| p.into_inner());
        }
        *used += n;
    }

    pub fn release(&self, n: u64) {
        let mut used = self.used.lock().unwrap_or_else(|p| p.into_inner());
        *used = used.saturating_sub(n);
        self.changed.notify_all();
    }

    /// Called after setting a cancel flag: wakes every blocked `acquire` / `wait_cancelled`.
    pub fn wake_all(&self) {
        let _used = self.used.lock().unwrap_or_else(|p| p.into_inner());
        self.changed.notify_all();
    }

    /// Blocks until `cancel` is set (test seam `ImportTuning::hold_reader_after`); woken by `wake_all`.
    pub(crate) fn wait_cancelled(&self, cancel: &AtomicBool) {
        let mut used = self.used.lock().unwrap_or_else(|p| p.into_inner());
        while !cancel.load(Ordering::SeqCst) {
            used = self.changed.wait(used).unwrap_or_else(|p| p.into_inner());
        }
    }

    pub fn used(&self) -> u64 {
        *self.used.lock().unwrap_or_else(|p| p.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn budget_blocks_until_release_or_cancel() {
        let b = Arc::new(ByteBudget::new(100));
        let cancel = Arc::new(AtomicBool::new(false));
        b.acquire(500, &cancel); // larger than the budget, nothing pending → passes
        assert_eq!(b.used(), 500);
        b.release(500);
        b.acquire(60, &cancel);
        let (tx, rx) = std::sync::mpsc::channel();
        let (b2, c2) = (b.clone(), cancel.clone());
        let h = std::thread::spawn(move || {
            b2.acquire(60, &c2); // 60 + 60 > 100 → blocks
            tx.send(()).unwrap();
        });
        assert!(rx.recv_timeout(Duration::from_millis(100)).is_err(), "acquire must block");
        b.release(60);
        rx.recv().unwrap();
        h.join().unwrap();
        // cancel releases a blocked acquire
        let (tx, rx) = std::sync::mpsc::channel();
        let (b3, c3) = (b.clone(), cancel.clone());
        let h = std::thread::spawn(move || {
            b3.acquire(90, &c3);
            tx.send(()).unwrap();
        });
        assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
        cancel.store(true, Ordering::SeqCst);
        b.wake_all();
        rx.recv().unwrap();
        h.join().unwrap();
        b.wait_cancelled(&cancel); // returns at once when already cancelled
    }
}
