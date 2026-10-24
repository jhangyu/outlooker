//! Two-phase keyset execution (spec Part 04 §7.4) and the capped count.
use tokio_postgres::types::ToSql;
use tokio_postgres::{Client, Transaction};

use super::compile::Compiled;
use super::cursor::CursorKey;
use super::sql::WINDOW_END;
use crate::dto::error::AppError;
use crate::dto::search::SearchCount;

pub const WINDOW: i64 = 5_000;
pub const COUNT_CAP: i64 = 10_000;
pub const TIMEOUT_MS: u32 = 10_000;
pub const SHORT_TIMEOUT_MS: u32 = 20_000;

/// Up to n rows, newest first; `phase_b_ran` tells the tests which path ran.
#[derive(Debug, Clone, PartialEq)]
pub struct PageIds {
    pub rows: Vec<(i64, CursorKey)>,
    pub phase_b_ran: bool,
}

/// Mode "ordered" and phase B: `$k+1` cursor sort_at text, `$k+2` cursor id, `$k+3` limit.
pub fn ordered_sql(c: &Compiled) -> String {
    let k = c.params.len();
    format!(
        "SELECT m.id, m.sort_at::text FROM messages m WHERE {} AND (m.sort_at, m.id) < (${}::text::timestamptz, ${}::int8) \
         ORDER BY m.sort_at DESC, m.id DESC LIMIT ${}",
        c.predicate,
        k + 1,
        k + 2,
        k + 3
    )
}

/// Phase A: `$k+1` cursor sort_at text, `$k+2` cursor id, `$k+3` window, `$k+4` limit.
pub fn phase_a_sql(c: &Compiled) -> String {
    let k = c.params.len();
    format!(
        "SELECT m.id, m.sort_at::text FROM (SELECT m0.* FROM messages m0 WHERE (m0.sort_at, m0.id) < (${}::text::timestamptz, ${}::int8) \
         ORDER BY m0.sort_at DESC, m0.id DESC LIMIT ${}) AS m WHERE {} ORDER BY m.sort_at DESC, m.id DESC LIMIT ${}",
        k + 1,
        k + 2,
        k + 3,
        c.predicate,
        k + 4
    )
}

/// `$k+1` = COUNT_CAP + 1.
pub fn count_sql(c: &Compiled) -> String {
    format!("SELECT count(*) FROM (SELECT 1 FROM messages m WHERE {} LIMIT ${}) s", c.predicate, c.params.len() + 1)
}

/// The predicate's `$1..$k` values followed by `extra` (`$k+1..`).
pub fn bind<'a>(c: &'a Compiled, extra: &[&'a (dyn ToSql + Sync)]) -> Vec<&'a (dyn ToSql + Sync)> {
    let mut v: Vec<&(dyn ToSql + Sync)> = c.params.iter().map(|p| p.as_sql()).collect();
    v.extend_from_slice(extra);
    v
}

fn timeout_ms(c: &Compiled) -> u32 {
    if c.short_query { SHORT_TIMEOUT_MS } else { TIMEOUT_MS }
}

fn map_err(e: tokio_postgres::Error, timeout: u32) -> AppError {
    match e.as_db_error().map(|d| d.code().code()) {
        Some("57014") => AppError::QueryTimeout { timeout_ms: timeout },
        Some("22007") | Some("22008") => AppError::invalid("cursor", "invalid"),
        _ => crate::db::db_err(e),
    }
}

async fn begin<'a>(client: &'a mut Client, c: &Compiled) -> Result<Transaction<'a>, AppError> {
    let t = timeout_ms(c);
    let tx = client.transaction().await.map_err(|e| map_err(e, t))?;
    tx.batch_execute(&format!("SET LOCAL statement_timeout = {t}")).await.map_err(|e| map_err(e, t))?;
    Ok(tx)
}

fn rows_to_keys(rows: Vec<tokio_postgres::Row>) -> Vec<(i64, CursorKey)> {
    rows.into_iter()
        .map(|r| {
            let id: i64 = r.get(0);
            (id, CursorKey::from_db(r.get(1), id))
        })
        .collect()
}

/// Spec Part 04 §7.4 inside one transaction on `client`. n = page_size + 1. `window` = W (production `WINDOW`).
pub async fn page(client: &mut Client, c: &Compiled, after: Option<&CursorKey>, n: i64, window: i64) -> Result<PageIds, AppError> {
    let t = timeout_ms(c);
    let (cs, ci) = match after {
        Some(key) => (key.sort_text().to_owned(), key.id),
        None => ("infinity".to_owned(), 0_i64),
    };
    let tx = begin(client, c).await?;
    if !c.indexable {
        let rows = tx.query(&ordered_sql(c), &bind(c, &[&cs, &ci, &n])).await.map_err(|e| map_err(e, t))?;
        tx.commit().await.map_err(|e| map_err(e, t))?;
        return Ok(PageIds { rows: rows_to_keys(rows), phase_b_ran: false });
    }
    let mut rows = rows_to_keys(tx.query(&phase_a_sql(c), &bind(c, &[&cs, &ci, &window, &n])).await.map_err(|e| map_err(e, t))?);
    let mut phase_b_ran = false;
    if (rows.len() as i64) < n {
        let offset = window - 1;
        if let Some(end) = tx.query_opt(WINDOW_END, &[&cs, &ci, &offset]).await.map_err(|e| map_err(e, t))? {
            let (ws, wi): (String, i64) = (end.get(0), end.get(1));
            let remaining = n - rows.len() as i64;
            tx.batch_execute("SET LOCAL enable_indexscan = off; SET LOCAL enable_seqscan = off;").await.map_err(|e| map_err(e, t))?;
            let b = tx.query(&ordered_sql(c), &bind(c, &[&ws, &wi, &remaining])).await.map_err(|e| map_err(e, t))?;
            tx.batch_execute("RESET enable_indexscan; RESET enable_seqscan;").await.map_err(|e| map_err(e, t))?;
            rows.extend(rows_to_keys(b));
            phase_b_ran = true;
        }
    }
    tx.commit().await.map_err(|e| map_err(e, t))?;
    Ok(PageIds { rows, phase_b_ran })
}

pub async fn count(client: &mut Client, c: &Compiled) -> Result<SearchCount, AppError> {
    let t = TIMEOUT_MS;
    let tx = client.transaction().await.map_err(|e| map_err(e, t))?;
    tx.batch_execute(&format!("SET LOCAL statement_timeout = {t}")).await.map_err(|e| map_err(e, t))?;
    if c.indexable {
        tx.batch_execute("SET LOCAL enable_seqscan = off; SET LOCAL enable_indexscan = off;").await.map_err(|e| map_err(e, t))?;
    }
    let cap = COUNT_CAP + 1;
    let n: i64 = tx.query_one(&count_sql(c), &bind(c, &[&cap])).await.map_err(|e| map_err(e, t))?.get(0);
    tx.commit().await.map_err(|e| map_err(e, t))?;
    Ok(if n > COUNT_CAP { SearchCount { count: COUNT_CAP, capped: true } } else { SearchCount { count: n, capped: false } })
}
