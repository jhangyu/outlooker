use std::sync::Mutex;

use crate::dto::app::AppStatus;
use crate::dto::events::{names, DataDirMoveProgress, ImportStatusChanged, LibraryChanged};
use crate::dto::import::ImportProgress;
use crate::dto::settings::MaintenanceState;

mod throttle;

pub use throttle::Throttle;

pub trait EventSink: Send + Sync {
    fn emit(&self, event: CoreEvent);
}

#[derive(Debug, Clone, PartialEq)]
pub enum CoreEvent {
    AppStatusChanged(AppStatus),
    ImportProgress(ImportProgress),
    ImportStatusChanged(ImportStatusChanged),
    LibraryChanged(LibraryChanged),
    DataDirMoveProgress(DataDirMoveProgress),
    MaintenanceStatusChanged(MaintenanceState),
}

impl CoreEvent {
    pub fn name(&self) -> &'static str {
        match self {
            CoreEvent::AppStatusChanged(_) => names::APP_STATUS_CHANGED,
            CoreEvent::ImportProgress(_) => names::IMPORT_PROGRESS,
            CoreEvent::ImportStatusChanged(_) => names::IMPORT_STATUS_CHANGED,
            CoreEvent::LibraryChanged(_) => names::LIBRARY_CHANGED,
            CoreEvent::DataDirMoveProgress(_) => names::DATA_DIR_MOVE_PROGRESS,
            CoreEvent::MaintenanceStatusChanged(_) => names::MAINTENANCE_STATUS_CHANGED,
        }
    }

    pub fn payload(&self) -> serde_json::Value {
        let v = match self {
            CoreEvent::AppStatusChanged(p) => serde_json::to_value(p),
            CoreEvent::ImportProgress(p) => serde_json::to_value(p),
            CoreEvent::ImportStatusChanged(p) => serde_json::to_value(p),
            CoreEvent::LibraryChanged(p) => serde_json::to_value(p),
            CoreEvent::DataDirMoveProgress(p) => serde_json::to_value(p),
            CoreEvent::MaintenanceStatusChanged(p) => serde_json::to_value(p),
        };
        v.expect("event payloads always serialize")
    }
}

#[derive(Default)]
pub struct RecordingSink {
    events: Mutex<Vec<CoreEvent>>,
}

impl RecordingSink {
    pub fn events(&self) -> Vec<CoreEvent> {
        self.events.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }
    pub fn names(&self) -> Vec<&'static str> {
        self.events.lock().unwrap_or_else(|p| p.into_inner()).iter().map(CoreEvent::name).collect()
    }
}

impl EventSink for RecordingSink {
    fn emit(&self, event: CoreEvent) {
        self.events.lock().unwrap_or_else(|p| p.into_inner()).push(event);
    }
}

pub struct NullSink;

impl EventSink for NullSink {
    fn emit(&self, _event: CoreEvent) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::app::LibraryCounts;

    #[test]
    fn recording_sink_keeps_order_and_names() {
        let sink = RecordingSink::default();
        sink.emit(CoreEvent::LibraryChanged(LibraryChanged { counts: LibraryCounts { messages: 3, psts: 1 } }));
        assert_eq!(sink.names(), vec!["library:changed"]);
        assert_eq!(sink.events()[0].payload(), serde_json::json!({ "counts": { "messages": 3, "psts": 1 } }));
    }
}
