use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use tokio::time::MissedTickBehavior;

use crate::dto::app::AppPhase;
use crate::dto::error::AppError;
use crate::lifecycle::pg::{PgController, PgStatus};
use crate::lifecycle::supervisor::Supervisor;

impl Supervisor {
    pub(crate) fn spawn_health_monitor(&self) {
        if self.inner.health_started.swap(true, Ordering::SeqCst) {
            return;
        }
        let me = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(me.inner.config.probe_interval);
            tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
            let mut failures = 0u32;
            loop {
                tick.tick().await;
                if me.phase() != AppPhase::Ready || me.probe().await {
                    failures = 0;
                    continue;
                }
                failures += 1;
                if failures >= 2 {
                    failures = 0;
                    me.handle_db_loss().await;
                }
            }
        });
    }

    async fn probe(&self) -> bool {
        let Ok(pool) = self.inner.db.app_pool() else { return false };
        let probe = async {
            let c = pool.get().await.ok()?;
            c.query_one("SELECT 1", &[]).await.ok()
        };
        matches!(tokio::time::timeout(Duration::from_secs(2), probe).await, Ok(Some(_)))
    }

    async fn handle_db_loss(&self) {
        let paths = self.paths();
        let ctl = PgController::new(paths.clone());
        if matches!(ctl.status(&paths.pgdata()).await, Ok(PgStatus::Running { .. })) {
            return; // server is up; the probe failure was transient
        }
        self.inner.db.set(None);
        let now = Instant::now();
        let over_budget = {
            let mut r = self.inner.restarts.lock().unwrap_or_else(|p| p.into_inner());
            r.retain(|t| now.duration_since(*t) < self.inner.config.restart_window);
            r.push(now);
            r.len() as u32 > self.inner.config.restart_budget
        };
        if over_budget {
            let detail = ctl.last_log_line(&paths.pgdata()).unwrap_or_else(|| "PostgreSQL stopped repeatedly".into());
            self.set_phase(AppPhase::Failed, Some(AppError::internal(detail))).await;
            return;
        }
        let _ = self.start_cluster().await;
    }
}
