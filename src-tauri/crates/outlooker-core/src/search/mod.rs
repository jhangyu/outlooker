//! Search Service (spec Part 02 §3.1, Part 03 §5, Part 04 §7). Read-only, on the outlooker_app pool.
pub mod sql;

use std::collections::HashMap;

use tokio_postgres::Client;

use crate::db::RFC3339_UTC_FMT;
use crate::dto::error::AppError;
use crate::dto::search::{Address, MailRow};

/// Database error of a read path: connection closed / I/O → `DbNotReady`, else `Internal{"<sqlstate>: <message>"}`.
pub(crate) fn db_err(e: tokio_postgres::Error) -> AppError {
    if e.is_closed() || (e.as_db_error().is_none() && std::error::Error::source(&e).is_some_and(|s| s.is::<std::io::Error>())) {
        return AppError::DbNotReady;
    }
    crate::db::pg_err(e)
}

/// C13. MailRow for `ids` (hydrate SQL of spec Part 04 §7.4); keeps the order of `ids` (duplicates repeated),
/// ids with no message are skipped.
pub async fn hydrate_rows(client: &Client, ids: &[i64]) -> Result<Vec<MailRow>, AppError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let ids_vec = ids.to_vec();
    let rows = client.query(sql::HYDRATE, &[&ids_vec, &RFC3339_UTC_FMT]).await.map_err(db_err)?;
    let by_id: HashMap<i64, MailRow> = rows
        .iter()
        .map(|r| {
            let row = MailRow {
                id: r.get(0),
                sent_at: r.get(1),
                from: Address { name: r.get(2), address: r.get(3) },
                to_preview: r.get(4),
                recipient_count: r.get::<_, i32>(5).max(0) as u32,
                subject: r.get(6),
                snippet: r.get(7),
                has_attachments: r.get(8),
                occurrence_count: r.get::<_, i64>(9).clamp(0, u32::MAX as i64) as u32,
            };
            (row.id, row)
        })
        .collect();
    Ok(ids.iter().filter_map(|id| by_id.get(id).cloned()).collect())
}
