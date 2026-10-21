use crate::db::{pg_err, RFC3339_UTC_FMT};
use crate::dto::app::{AppPhase, LibraryCounts};
use crate::dto::error::AppError;
use crate::dto::settings::{DbStatus, IndexStatus, MaintenanceState};
use crate::lifecycle::supervisor::Supervisor;

impl Supervisor {
    pub async fn db_status(&self, maintenance: Option<MaintenanceState>) -> Result<DbStatus, AppError> {
        self.require_ready()?;
        let c = self.inner.db.app_pool()?.get().await.map_err(|e| AppError::internal(e.to_string()))?;
        let r1 = c.query_one("SELECT current_setting('server_version'), pg_database_size(current_database())", &[]).await.map_err(pg_err)?;
        let r2 = c
            .query_one(
                "SELECT (SELECT count(*) FROM image_blobs), (SELECT count(*) FROM attachments), \
                 (SELECT count(*) FROM messages), (SELECT count(*) FROM pst_sources)",
                &[],
            )
            .await
            .map_err(pg_err)?;
        let idx = c
            .query(
                "SELECT i.relname::text, t.relname::text, pg_relation_size(i.oid), x.indisvalid \
                 FROM pg_index x JOIN pg_class i ON i.oid = x.indexrelid JOIN pg_class t ON t.oid = x.indrelid \
                 JOIN pg_namespace n ON n.oid = t.relnamespace WHERE n.nspname = 'public' \
                 ORDER BY pg_relation_size(i.oid) DESC",
                &[],
            )
            .await
            .map_err(pg_err)?;
        let r3 = c
            .query_one(
                &format!(
                    "SELECT to_char(max(greatest(last_vacuum, last_autovacuum)) AT TIME ZONE 'UTC', '{f}'), \
                     to_char(max(greatest(last_analyze, last_autoanalyze)) AT TIME ZONE 'UTC', '{f}') \
                     FROM pg_stat_user_tables WHERE relname = 'messages'",
                    f = RFC3339_UTC_FMT
                ),
                &[],
            )
            .await
            .map_err(pg_err)?;
        Ok(DbStatus {
            phase: AppPhase::Ready,
            postgres_version: r1.get(0),
            data_dir: self.paths().data_root().display().to_string(),
            database_size_bytes: r1.get(1),
            counts: LibraryCounts { messages: r2.get(2), psts: r2.get(3) },
            image_count: r2.get(0),
            attachment_count: r2.get(1),
            indexes: idx.iter().map(|r| IndexStatus { name: r.get(0), table: r.get(1), size_bytes: r.get(2), valid: r.get(3) }).collect(),
            last_vacuum_at: r3.get(0),
            last_analyze_at: r3.get(1),
            maintenance,
        })
    }
}
