use std::sync::{Arc, Mutex};

use tokio::sync::Notify;

use crate::dto::error::{AppError, BusyReason};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Activity {
    Import,
    Removal,
    Maintenance,
    DataDirMove,
}

pub fn busy_reason(active: Activity) -> BusyReason {
    match active {
        Activity::Import => BusyReason::ImportRunning,
        Activity::Removal => BusyReason::PstRemoving,
        Activity::Maintenance => BusyReason::MaintenanceRunning,
        Activity::DataDirMove => BusyReason::DataDirMoving,
    }
}

/// At most one exclusive activity at a time (union of the conflict rules in spec Part 03 §7.2 and §8).
#[derive(Debug, Clone, Default)]
pub struct Gate {
    slot: Arc<Mutex<Option<Activity>>>,
    freed: Arc<Notify>,
}

impl Gate {
    pub fn new() -> Gate {
        Gate::default()
    }

    pub fn try_enter(&self, activity: Activity) -> Result<GateGuard, AppError> {
        let mut slot = self.slot.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(active) = *slot {
            return Err(AppError::Busy { reason: busy_reason(active) });
        }
        *slot = Some(activity);
        Ok(GateGuard { gate: self.clone(), activity })
    }

    pub fn current(&self) -> Option<Activity> {
        *self.slot.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Resolves when no activity holds the gate.
    pub async fn released(&self) {
        loop {
            let notified = self.freed.notified();
            if self.current().is_none() {
                return;
            }
            notified.await;
        }
    }
}

#[derive(Debug)]
pub struct GateGuard {
    gate: Gate,
    activity: Activity,
}

impl GateGuard {
    pub fn activity(&self) -> Activity {
        self.activity
    }
}

impl Drop for GateGuard {
    fn drop(&mut self) {
        let mut slot = self.gate.slot.lock().unwrap_or_else(|p| p.into_inner());
        if *slot == Some(self.activity) {
            *slot = None;
        }
        drop(slot);
        self.gate.freed.notify_waiters();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Activity; 4] = [Activity::Import, Activity::Removal, Activity::Maintenance, Activity::DataDirMove];

    #[test]
    fn every_activity_blocks_every_activity() {
        for held in ALL {
            let gate = Gate::new();
            let _guard = gate.try_enter(held).unwrap();
            for wanted in ALL {
                assert_eq!(
                    gate.try_enter(wanted).unwrap_err(),
                    AppError::Busy { reason: busy_reason(held) },
                    "{held:?} must block {wanted:?}"
                );
            }
        }
    }

    #[tokio::test]
    async fn released_resolves_after_drop() {
        let gate = Gate::new();
        let guard = gate.try_enter(Activity::Removal).unwrap();
        let g2 = gate.clone();
        let waiter = tokio::spawn(async move { g2.released().await });
        drop(guard);
        tokio::time::timeout(std::time::Duration::from_secs(5), waiter).await.unwrap().unwrap();
        assert_eq!(gate.current(), None);
        assert!(gate.try_enter(Activity::Import).is_ok());
    }

}
