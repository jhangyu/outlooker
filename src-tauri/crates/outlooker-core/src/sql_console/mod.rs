//! SQL Console Service (spec Part 02 §3.1, Part 03 §6, Part 04 §8.1).
pub mod execute;
pub mod saved;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use deadpool_postgres::PoolError;
use tokio::sync::Notify;
use tokio_postgres::error::SqlState;
use tokio_postgres::types::Type;
use tokio_postgres::{CancelToken, Client, NoTls, SimpleQueryMessage};

use crate::db::Db;
use crate::dto::error::AppError;
use crate::dto::sql::{ColumnMeta, SavedQuery, SavedQueryInput, SqlExecuteRequest, SqlResult};

use execute::{command_tag, cut_cell, map_sql_error};

pub const DEFAULT_MAX_ROWS: u32 = 1_000;
pub const MAX_ROWS: u32 = 10_000;
pub const CELL_MAX_CHARS: usize = 2_000;
pub const TIMEOUT_MS: u32 = 30_000;

const BEGIN: &str = "BEGIN READ ONLY";
const SET_TIMEOUT: &str = "SET LOCAL statement_timeout = '30s'";
const DECLARE: &str = "DECLARE outlooker_c NO SCROLL CURSOR FOR ";

/// The one running execution (Part 03 §6.3 `execution_id → CancelToken`).
struct Running {
    execution_id: String,
    token: Option<CancelToken>,
    cancelled: Arc<AtomicBool>,
    /// The user statement has been sent (test seam `wait_statement_started`).
    started: bool,
}

impl Running {
    /// Sets the flag; returns the token to cancel with (used outside the lock).
    fn mark_cancelled(&self) -> Option<CancelToken> {
        self.cancelled.store(true, Ordering::SeqCst);
        self.token.clone()
    }
}

async fn send_cancel(token: Option<CancelToken>) {
    if let Some(token) = token {
        let _ = token.cancel_query(NoTls).await; // error ignored (Part 03 §6.3)
    }
}

#[derive(Clone)]
pub struct SqlConsole {
    db: Db,
    running: Arc<Mutex<Option<Running>>>,
    started: Arc<Notify>,
}

/// Removes the registration in every path (drop).
struct Registration {
    running: Arc<Mutex<Option<Running>>>,
}

impl Drop for Registration {
    fn drop(&mut self) {
        *self.running.lock().unwrap_or_else(|p| p.into_inner()) = None;
    }
}

fn pool_err(e: PoolError) -> AppError {
    match e {
        PoolError::Closed | PoolError::Backend(_) => AppError::DbNotReady,
        other => AppError::internal(format!("pool: {other}")),
    }
}

fn elapsed_ms(t: Instant) -> u32 {
    u32::try_from(t.elapsed().as_millis()).unwrap_or(u32::MAX)
}

impl SqlConsole {
    pub fn new(db: Db) -> SqlConsole {
        SqlConsole { db, running: Arc::new(Mutex::new(None)), started: Arc::new(Notify::new()) }
    }

    pub async fn saved_list(&self) -> Result<Vec<SavedQuery>, AppError> {
        let c = self.db.app_pool()?.get().await.map_err(pool_err)?;
        saved::list(&c).await
    }
    pub async fn saved_create(&self, input: SavedQueryInput) -> Result<SavedQuery, AppError> {
        let c = self.db.app_pool()?.get().await.map_err(pool_err)?;
        saved::create(&c, &input).await
    }
    pub async fn saved_update(&self, id: i64, input: SavedQueryInput) -> Result<SavedQuery, AppError> {
        let c = self.db.app_pool()?.get().await.map_err(pool_err)?;
        saved::update(&c, id, &input).await
    }
    pub async fn saved_delete(&self, id: i64) -> Result<(), AppError> {
        let c = self.db.app_pool()?.get().await.map_err(pool_err)?;
        saved::delete(&c, id).await
    }

    pub async fn execute(&self, req: SqlExecuteRequest) -> Result<SqlResult, AppError> {
        let started = Instant::now();
        let max_rows = req.max_rows.unwrap_or(DEFAULT_MAX_ROWS);
        if !(1..=MAX_ROWS).contains(&max_rows) {
            return Err(AppError::invalid("maxRows", "outOfRange"));
        }
        if req.sql.trim().is_empty() {
            return Err(AppError::invalid("sql", "empty"));
        }
        if req.execution_id.is_empty() {
            return Err(AppError::invalid("executionId", "empty"));
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        let _registration = {
            let mut slot = self.running.lock().unwrap_or_else(|p| p.into_inner());
            if slot.is_some() {
                return Err(AppError::invalid("executionId", "alreadyRunning"));
            }
            *slot = Some(Running { execution_id: req.execution_id.clone(), token: None, cancelled: cancelled.clone(), started: false });
            Registration { running: self.running.clone() }
        };
        let client = self.db.ro_pool()?.get().await.map_err(pool_err)?;
        if let Some(r) = self.running.lock().unwrap_or_else(|p| p.into_inner()).as_mut() {
            r.token = Some(client.cancel_token());
        }
        let outcome = self.run(&client, &req.sql, max_rows, &cancelled).await;
        // A connection that fails to reset must not return to the pool with leftover session state:
        // detach it (dropping the wrapper closes the connection) so the pool opens a fresh one.
        if client.batch_execute("ROLLBACK").await.is_err() || client.batch_execute("DISCARD ALL").await.is_err() {
            drop(deadpool_postgres::Object::take(client));
        }
        let mut result = outcome?;
        if cancelled.load(Ordering::SeqCst) {
            // cancel arrived while no statement was executing on the server
            return Err(AppError::Cancelled);
        }
        match &mut result {
            SqlResult::MailList { elapsed_ms: e, .. } | SqlResult::Table { elapsed_ms: e, .. } | SqlResult::Command { elapsed_ms: e, .. } => {
                *e = elapsed_ms(started);
            }
        }
        Ok(result)
    }

    /// Marks the user statement as sent; returns Err(Cancelled) when cancel came first.
    fn statement_starts(&self, cancelled: &AtomicBool) -> Result<(), AppError> {
        if cancelled.load(Ordering::SeqCst) {
            return Err(AppError::Cancelled);
        }
        if let Some(r) = self.running.lock().unwrap_or_else(|p| p.into_inner()).as_mut() {
            r.started = true;
        }
        self.started.notify_waiters();
        Ok(())
    }

    async fn run(&self, client: &Client, sql: &str, max_rows: u32, cancelled: &AtomicBool) -> Result<SqlResult, AppError> {
        let flag = || cancelled.load(Ordering::SeqCst);
        client.batch_execute(BEGIN).await.map_err(|e| map_sql_error(&e, true, flag()))?;
        client.batch_execute(SET_TIMEOUT).await.map_err(|e| map_sql_error(&e, true, flag()))?;
        let stmt = client.prepare(sql).await.map_err(|e| map_sql_error(&e, true, flag()))?;
        let columns: Vec<ColumnMeta> =
            stmt.columns().iter().map(|c| ColumnMeta { name: c.name().to_owned(), pg_type: c.type_().name().to_owned() }).collect();
        self.statement_starts(cancelled)?;
        if columns.is_empty() {
            client.simple_query(sql).await.map_err(|e| map_sql_error(&e, false, flag()))?;
            return Ok(SqlResult::Command { tag: command_tag(sql), elapsed_ms: 0 });
        }
        let limit = max_rows as usize + 1;
        let fetched = match client.simple_query(&format!("{DECLARE}{sql}")).await {
            Ok(_) => {
                let fetch = format!("FETCH FORWARD {limit} FROM outlooker_c");
                client.simple_query(&fetch).await.map_err(|e| map_sql_error(&e, true, flag()))?
            }
            Err(e) if e.code() == Some(&SqlState::SYNTAX_ERROR) => {
                // SHOW / EXPLAIN: DECLARE accepts only SELECT / VALUES (Part 04 §8.1 step 4).
                let _ = client.batch_execute("ROLLBACK").await;
                client.batch_execute(BEGIN).await.map_err(|e| map_sql_error(&e, true, flag()))?;
                client.batch_execute(SET_TIMEOUT).await.map_err(|e| map_sql_error(&e, true, flag()))?;
                client.simple_query(sql).await.map_err(|e| map_sql_error(&e, true, flag()))?
            }
            Err(e) => return Err(map_sql_error(&e, true, flag())),
        };
        let mut rows: Vec<Vec<Option<String>>> = fetched
            .iter()
            .filter_map(|m| match m {
                SimpleQueryMessage::Row(r) => Some((0..r.len()).map(|i| r.get(i).map(str::to_owned)).collect()),
                _ => None,
            })
            .take(limit)
            .collect();
        let truncated = rows.len() > max_rows as usize;
        rows.truncate(max_rows as usize);
        let id_col = stmt
            .columns()
            .iter()
            .position(|c| c.name() == "id" && [Type::INT8, Type::INT4, Type::INT2].contains(c.type_()));
        if let Some(i) = id_col {
            let ids: Vec<i64> = rows.iter().filter_map(|r| r[i].as_deref()?.parse().ok()).collect();
            let app = self.db.app_pool()?.get().await.map_err(pool_err)?;
            let mail = crate::search::hydrate_rows(&app, &ids).await?;
            let missing_ids = u32::try_from(ids.len() - mail.len()).unwrap_or(u32::MAX);
            return Ok(SqlResult::MailList { rows: mail, missing_ids, truncated, elapsed_ms: 0 });
        }
        let rows = rows.into_iter().map(|r| r.into_iter().map(|v| v.map(|s| cut_cell(&s))).collect()).collect();
        Ok(SqlResult::Table { columns, rows, truncated, elapsed_ms: 0 })
    }

    /// sql_cancel: idempotent; unknown or finished id → Ok(()).
    pub async fn cancel(&self, execution_id: String) -> Result<(), AppError> {
        let token = {
            let slot = self.running.lock().unwrap_or_else(|p| p.into_inner());
            slot.as_ref().filter(|r| r.execution_id == execution_id).and_then(Running::mark_cancelled)
        };
        send_cancel(token).await;
        Ok(())
    }

    /// Part 02 §2.5 step 2: cancel whatever runs.
    pub async fn cancel_running(&self) {
        let token = self.running.lock().unwrap_or_else(|p| p.into_inner()).as_ref().and_then(Running::mark_cancelled);
        send_cancel(token).await;
    }

    /// Test seam: resolves once the running execution has sent its user statement (Notify, no polling).
    #[cfg(feature = "test-support")]
    pub async fn wait_statement_started(&self) {
        loop {
            let notified = self.started.notified();
            if self.running.lock().unwrap_or_else(|p| p.into_inner()).as_ref().is_some_and(|r| r.started) {
                return;
            }
            notified.await;
        }
    }
}
