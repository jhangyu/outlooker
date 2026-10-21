use crate::dto::error::AppError;
use crate::lifecycle::pg::total_ram_bytes;

use super::pg_err;

/// Generation expression of the stored column `messages.body_tsv` (spec Part 04 §2, §3). The search compiler and
/// `v_mail` use the column `body_tsv`; nothing recomputes this expression at query time.
pub const TSV_EXPR: &str = "to_tsvector('english'::regconfig, left(subject || ' ' || body_text, 100000))";

pub struct SearchIndex {
    pub name: &'static str,
    pub ddl: &'static str,
}

/// The six search indexes dropped and rebuilt around large imports (spec Part 04 §6.4).
pub const SEARCH_INDEXES: [SearchIndex; 6] = [
    SearchIndex { name: "messages_sort", ddl: "messages_sort ON messages (sort_at, id)" },
    SearchIndex { name: "messages_body_trgm", ddl: "messages_body_trgm ON messages USING gin (body_text gin_trgm_ops) WITH (gin_pending_list_limit = 65536)" },
    SearchIndex { name: "messages_subject_trgm", ddl: "messages_subject_trgm ON messages USING gin (subject gin_trgm_ops)" },
    SearchIndex { name: "messages_sender_trgm", ddl: "messages_sender_trgm ON messages USING gin (sender_search gin_trgm_ops)" },
    SearchIndex { name: "messages_recipients_trgm", ddl: "messages_recipients_trgm ON messages USING gin (recipients_search gin_trgm_ops) WITH (gin_pending_list_limit = 16384)" },
    SearchIndex { name: "messages_body_fts", ddl: "messages_body_fts ON messages USING gin (body_tsv)" },
];

/// Names of search indexes that are missing or invalid.
pub async fn missing_or_invalid(admin: &tokio_postgres::Client) -> Result<Vec<&'static str>, AppError> {
    let names: Vec<&str> = SEARCH_INDEXES.iter().map(|i| i.name).collect();
    let rows = admin
        .query(
            "SELECT c.relname, x.indisvalid FROM pg_class c JOIN pg_index x ON x.indexrelid = c.oid WHERE c.relname = ANY($1)",
            &[&names],
        )
        .await
        .map_err(pg_err)?;
    Ok(SEARCH_INDEXES
        .iter()
        .map(|i| i.name)
        .filter(|n| !rows.iter().any(|r| r.get::<_, String>(0) == *n && r.get::<_, bool>(1)))
        .collect())
}

pub async fn drop_search_indexes(admin: &tokio_postgres::Client) -> Result<(), AppError> {
    for idx in &SEARCH_INDEXES {
        admin.batch_execute(&format!("DROP INDEX IF EXISTS {}", idx.name)).await.map_err(pg_err)?;
    }
    Ok(())
}

pub async fn ensure_search_indexes(
    admin: &tokio_postgres::Client,
    progress: &(dyn Fn(&str, usize, usize) + Send + Sync),
) -> Result<(), AppError> {
    let mwm_mb = (total_ram_bytes() / 8 / (1024 * 1024)).clamp(64, 1024);
    admin
        .batch_execute(&format!("SET maintenance_work_mem = '{mwm_mb}MB'; SET max_parallel_maintenance_workers = 2;"))
        .await
        .map_err(pg_err)?;
    let todo = missing_or_invalid(admin).await?;
    for (done, idx) in SEARCH_INDEXES.iter().enumerate() {
        if todo.contains(&idx.name) {
            admin.batch_execute(&format!("DROP INDEX IF EXISTS {}", idx.name)).await.map_err(pg_err)?;
            admin.batch_execute(&format!("CREATE INDEX IF NOT EXISTS {}", idx.ddl)).await.map_err(pg_err)?;
        }
        progress(idx.name, done + 1, SEARCH_INDEXES.len());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn squash(s: &str) -> String {
        s.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn ddl_matches_migration() {
        let v1 = squash(include_str!("migrations/V1__schema.sql"));
        for idx in &SEARCH_INDEXES {
            assert!(v1.contains(&squash(&format!("CREATE INDEX {};", idx.ddl))), "{} differs from V1__schema.sql", idx.name);
            assert!(idx.ddl.starts_with(idx.name));
        }
        // TSV_EXPR is the generation expression of the stored column body_tsv.
        assert!(v1.contains(&format!("body_tsv tsvector NOT NULL GENERATED ALWAYS AS ({TSV_EXPR}) STORED")));
        assert_eq!(SEARCH_INDEXES[5].ddl, "messages_body_fts ON messages USING gin (body_tsv)");
    }
}
