//! The Tauri `EventSink` (spec Part 02 §3.1, Part 03 §9): throttles through the core's `Throttle`.
use std::sync::{Arc, Mutex};
use std::time::Instant;

use outlooker_core::sink::{CoreEvent, EventSink, Throttle};
use tauri::{AppHandle, Emitter};
use tokio::sync::Notify;

/// The core's `EventSink` inside the app: throttles, then `app.emit(name, payload)`.
pub struct TauriSink {
    app: AppHandle,
    throttle: Arc<Mutex<Throttle>>,
    notify: Arc<Notify>,
}

fn emit_now(app: &AppHandle, e: &CoreEvent) {
    if let Err(err) = app.emit(e.name(), e.payload()) {
        log::warn!("emit {} failed: {err}", e.name());
    }
}

impl TauriSink {
    /// Spawns the trailing-flush task: it waits on a deadline timer (or on `notify` when nothing is pending).
    pub fn new(app: AppHandle) -> Arc<TauriSink> {
        let sink = Arc::new(TauriSink {
            app: app.clone(),
            throttle: Arc::new(Mutex::new(Throttle::default())),
            notify: Arc::new(Notify::new()),
        });
        let (throttle, notify) = (sink.throttle.clone(), sink.notify.clone());
        tauri::async_runtime::spawn(async move {
            loop {
                let deadline = throttle.lock().unwrap_or_else(|p| p.into_inner()).next_deadline();
                match deadline {
                    Some(d) => {
                        let _ = tokio::time::timeout_at(tokio::time::Instant::from_std(d), notify.notified()).await;
                    }
                    None => notify.notified().await,
                }
                let due = throttle.lock().unwrap_or_else(|p| p.into_inner()).due(Instant::now());
                for e in &due {
                    emit_now(&app, e);
                }
            }
        });
        sink
    }
}

impl EventSink for TauriSink {
    fn emit(&self, e: CoreEvent) {
        let out = self.throttle.lock().unwrap_or_else(|p| p.into_inner()).offer(e, Instant::now());
        for e in &out {
            emit_now(&self.app, e);
        }
        self.notify.notify_one();
    }
}
