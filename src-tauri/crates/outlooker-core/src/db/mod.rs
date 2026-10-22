pub mod credentials;
pub mod indexes;
pub mod pools;

pub use credentials::Credentials;
pub use crate::lifecycle::pg::connect;
pub use pools::{Db, DbConn, Pools};

use crate::dto::app::LibraryCounts;
use crate::dto::error::AppError;
use crate::lifecycle::pg::{ADMIN_ROLE, CJK_SELF_CHECK};

pub const DB_NAME: &str = "outlooker";
/// Use as `to_char(ts AT TIME ZONE 'UTC', '<RFC3339_UTC_FMT>')` (spec Part 03 §1 timestamps).
pub const RFC3339_UTC_FMT: &str = r#"YYYY-MM-DD"T"HH24:MI:SS"Z""#;

mod embedded {
    refinery::embed_migrations!("src/db/migrations");
}

/// The one database error policy. No SQLSTATE: closed connection or I/O → `DbNotReady`, else `Internal`.
/// With SQLSTATE: see `sqlstate_err`. Domain tables (search, SQL console, saved queries) fall through to this.
pub fn db_err(e: tokio_postgres::Error) -> AppError {
    match e.as_db_error() {
        Some(db) => sqlstate_err(db.code().code(), db.message()),
        None => no_sqlstate_err(&e),
    }
}

/// An error without SQLSTATE: closed connection or I/O → `DbNotReady`, else `Internal`.
pub fn no_sqlstate_err(e: &tokio_postgres::Error) -> AppError {
    if e.is_closed() || std::error::Error::source(e).is_some_and(|s| s.is::<std::io::Error>()) {
        AppError::DbNotReady
    } else {
        AppError::internal(e.to_string())
    }
}

/// 57P01/57P02/57P03 (server shutting down / starting) → `DbNotReady`; any other SQLSTATE →
/// `Internal{"<code>: <message>"}` (53100 disk full included: only the writer knows the data root path).
pub fn sqlstate_err(code: &str, message: &str) -> AppError {
    match code {
        "57P01" | "57P02" | "57P03" => AppError::DbNotReady,
        _ => AppError::internal(format!("{code}: {message}")),
    }
}

/// Pool checkout error: closed pool → `DbNotReady`; connect error → `db_err`; other → `Internal`.
pub fn pool_err(e: deadpool_postgres::PoolError) -> AppError {
    match e {
        deadpool_postgres::PoolError::Closed => AppError::DbNotReady,
        deadpool_postgres::PoolError::Backend(e) => db_err(e),
        other => AppError::internal(format!("pool: {other}")),
    }
}

/// `SELECT` of the library counts (messages, PST sources).
pub async fn library_counts(c: &tokio_postgres::Client) -> Result<LibraryCounts, AppError> {
    let r = c.query_one("SELECT (SELECT count(*) FROM messages), (SELECT count(*) FROM pst_sources)", &[]).await.map_err(db_err)?;
    Ok(LibraryCounts { messages: r.get(0), psts: r.get(1) })
}

pub async fn connect_admin(port: u16, creds: &Credentials) -> Result<tokio_postgres::Client, AppError> {
    connect(port, ADMIN_ROLE, &creds.admin, DB_NAME).await
}

/// Applies the embedded migrations, then sets the two login role passwords
/// (64 lowercase hex chars, validated by `Credentials`, so the literal cannot break quoting).
pub async fn migrate(admin: &mut tokio_postgres::Client, creds: &Credentials) -> Result<(), AppError> {
    embedded::migrations::runner()
        .run_async(admin)
        .await
        .map_err(|e| AppError::internal(format!("migration failed: {e}")))?;
    admin
        .batch_execute(&format!(
            "ALTER ROLE outlooker_app PASSWORD '{}'; ALTER ROLE outlooker_ro PASSWORD '{}';",
            creds.app, creds.ro
        ))
        .await
        .map_err(db_err)
}

pub async fn cjk_self_check(client: &tokio_postgres::Client) -> Result<(), AppError> {
    let ok: bool = client.query_one(CJK_SELF_CHECK, &[]).await.map_err(db_err)?.get(0);
    if ok { Ok(()) } else { Err(AppError::internal("database misconfigured: CJK trigram self-check failed")) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_closed_is_not_ready() {
        assert_eq!(pool_err(deadpool_postgres::PoolError::Closed), AppError::DbNotReady);
    }

    #[test]
    fn sqlstate_policy() {
        for code in ["57P01", "57P02", "57P03"] {
            assert_eq!(sqlstate_err(code, "x"), AppError::DbNotReady, "{code}");
        }
        assert_eq!(sqlstate_err("53100", "disk full"), AppError::internal("53100: disk full"));
        assert_eq!(sqlstate_err("23505", "dup"), AppError::internal("23505: dup"));
    }
}
