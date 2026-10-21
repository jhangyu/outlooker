pub mod credentials;
pub mod indexes;
pub mod pools;

pub use credentials::Credentials;
pub use crate::lifecycle::pg::connect;
pub use pools::{Db, DbConn, Pools};

use crate::dto::error::AppError;
use crate::lifecycle::pg::{ADMIN_ROLE, CJK_SELF_CHECK};

pub const DB_NAME: &str = "outlooker";
/// Use as `to_char(ts AT TIME ZONE 'UTC', '<RFC3339_UTC_FMT>')` (spec Part 03 §1 timestamps).
pub const RFC3339_UTC_FMT: &str = r#"YYYY-MM-DD"T"HH24:MI:SS"Z""#;

mod embedded {
    refinery::embed_migrations!("src/db/migrations");
}

pub fn pg_err(e: tokio_postgres::Error) -> AppError {
    match e.as_db_error() {
        Some(db) => AppError::internal(format!("{}: {}", db.code().code(), db.message())),
        None => AppError::internal(e.to_string()),
    }
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
        .map_err(pg_err)
}

pub async fn cjk_self_check(client: &tokio_postgres::Client) -> Result<(), AppError> {
    let ok: bool = client.query_one(CJK_SELF_CHECK, &[]).await.map_err(pg_err)?.get(0);
    if ok { Ok(()) } else { Err(AppError::internal("database misconfigured: CJK trigram self-check failed")) }
}
