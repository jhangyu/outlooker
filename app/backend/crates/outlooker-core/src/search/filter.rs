//! `filter_options` (spec Part 04 §7.6).
use tokio_postgres::Client;

use super::sql;
use crate::db::db_err;
use crate::dto::error::AppError;
use crate::dto::search::{FilterOptions, FolderRef, PstRef};

pub async fn filter_options(client: &Client) -> Result<FilterOptions, AppError> {
    let psts = client
        .query(sql::FILTER_PSTS, &[])
        .await
        .map_err(db_err)?
        .iter()
        .map(|r| PstRef { id: r.get(0), display_name: r.get(1), path: r.get(2) })
        .collect();
    let folders = client
        .query(sql::FILTER_FOLDERS, &[])
        .await
        .map_err(db_err)?
        .iter()
        .map(|r| FolderRef { folder_path: r.get(0), pst_id: r.get(1), message_count: r.get(2) })
        .collect();
    Ok(FilterOptions { psts, folders })
}
