//! `mailimg` scheme body (spec Part 04 §7.6).
use tokio_postgres::Client;

use super::sql;
use crate::db::db_err;
use crate::dto::error::AppError;

/// WebP bytes of `image_blobs.id`, `None` when unknown.
pub async fn image(client: &Client, id: i64) -> Result<Option<Vec<u8>>, AppError> {
    Ok(client.query_opt(sql::IMAGE, &[&id]).await.map_err(db_err)?.map(|r| r.get(0)))
}
