//! Saved queries (spec Part 03 §6.1, Part 04 §7.6) on the outlooker_app pool.
use tokio_postgres::error::SqlState;

use crate::db::{db_err, RFC3339_UTC_FMT};
use crate::dto::error::AppError;
use crate::dto::sql::{SavedQuery, SavedQueryInput};

pub const NAME_MAX_CHARS: usize = 200;
pub const SQL_MAX_CHARS: usize = 100_000;

const COLUMNS: &str = "id, name, sql, to_char(updated_at AT TIME ZONE 'UTC', $1)";

/// Trimmed name + untrimmed sql, or the invalidInput error.
pub fn validate(input: &SavedQueryInput) -> Result<(String, String), AppError> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(AppError::invalid("name", "empty"));
    }
    if name.chars().count() > NAME_MAX_CHARS {
        return Err(AppError::invalid("name", "tooLong"));
    }
    if input.sql.trim().is_empty() {
        return Err(AppError::invalid("sql", "empty"));
    }
    if input.sql.chars().count() > SQL_MAX_CHARS {
        return Err(AppError::invalid("sql", "tooLong"));
    }
    Ok((name.to_owned(), input.sql.clone()))
}

fn row_to_saved(r: &tokio_postgres::Row) -> SavedQuery {
    SavedQuery { id: r.get(0), name: r.get(1), sql: r.get(2), updated_at: r.get(3) }
}

fn write_err(e: tokio_postgres::Error) -> AppError {
    if e.code() == Some(&SqlState::UNIQUE_VIOLATION) {
        AppError::invalid("name", "duplicate")
    } else {
        db_err(e)
    }
}

pub async fn list(c: &tokio_postgres::Client) -> Result<Vec<SavedQuery>, AppError> {
    let sql = format!("SELECT {COLUMNS} FROM saved_queries ORDER BY name");
    let rows = c.query(&sql, &[&RFC3339_UTC_FMT]).await.map_err(db_err)?;
    Ok(rows.iter().map(row_to_saved).collect())
}

pub async fn create(c: &tokio_postgres::Client, input: &SavedQueryInput) -> Result<SavedQuery, AppError> {
    let (name, sql_text) = validate(input)?;
    let sql = format!("INSERT INTO saved_queries (name, sql) VALUES ($2, $3) RETURNING {COLUMNS}");
    let row = c.query_one(&sql, &[&RFC3339_UTC_FMT, &name, &sql_text]).await.map_err(write_err)?;
    Ok(row_to_saved(&row))
}

pub async fn update(c: &tokio_postgres::Client, id: i64, input: &SavedQueryInput) -> Result<SavedQuery, AppError> {
    let (name, sql_text) = validate(input)?;
    let sql = format!("UPDATE saved_queries SET name = $3, sql = $4, updated_at = now() WHERE id = $2 RETURNING {COLUMNS}");
    let row = c.query_opt(&sql, &[&RFC3339_UTC_FMT, &id, &name, &sql_text]).await.map_err(write_err)?;
    row.as_ref().map(row_to_saved).ok_or_else(|| AppError::not_found("savedQuery", id))
}

pub async fn delete(c: &tokio_postgres::Client, id: i64) -> Result<(), AppError> {
    let n = c.execute("DELETE FROM saved_queries WHERE id = $1", &[&id]).await.map_err(db_err)?;
    if n == 0 { Err(AppError::not_found("savedQuery", id)) } else { Ok(()) }
}
