//! Search Service (spec Part 02 §3.1, Part 03 §5, Part 04 §7). Read-only, on the outlooker_app pool.
pub mod compile;
pub mod cursor;
pub mod execute;
pub mod filter;
pub mod image;
pub mod mail;
pub mod sql;

use std::collections::HashMap;
use std::time::Instant;

use tokio_postgres::Client;

use crate::db::{db_err, Db, RFC3339_UTC_FMT};
use crate::dto::error::AppError;
use crate::dto::search::{Address, FilterOptions, MailDetail, MailRow, SearchCount, SearchPage, SearchQuery, SearchRequest};

pub const DEFAULT_PAGE_SIZE: u32 = 200;
pub const MAX_PAGE_SIZE: u32 = 500;

#[derive(Clone)]
pub struct SearchService {
    db: Db,
}

impl SearchService {
    pub fn new(db: Db) -> SearchService {
        SearchService { db }
    }

    async fn client(&self) -> Result<deadpool_postgres::Object, AppError> {
        self.db.app_client().await
    }

    pub async fn search(&self, req: SearchRequest) -> Result<SearchPage, AppError> {
        let started = Instant::now();
        let mut client = self.client().await?;
        let (mut page, _) = search_page(&mut client, &req, execute::WINDOW).await?;
        page.elapsed_ms = started.elapsed().as_millis().min(u32::MAX as u128) as u32;
        Ok(page)
    }

    pub async fn count(&self, query: SearchQuery) -> Result<SearchCount, AppError> {
        let c = compile::compile(&query)?;
        let mut client = self.client().await?;
        execute::count(&mut client, &c).await
    }

    pub async fn filter_options(&self) -> Result<FilterOptions, AppError> {
        let client = self.client().await?;
        filter::filter_options(&client).await
    }

    pub async fn mail_get(&self, id: i64) -> Result<MailDetail, AppError> {
        let client = self.client().await?;
        mail::mail_get(&client, id).await
    }

    pub async fn image(&self, id: i64) -> Result<Option<Vec<u8>>, AppError> {
        let client = self.client().await?;
        image::image(&client, id).await
    }
}

/// One `search_mail` call on `client` with window `window` (production `execute::WINDOW`).
/// Returns the page and whether phase B ran (test seam).
pub async fn search_page(client: &mut Client, req: &SearchRequest, window: i64) -> Result<(SearchPage, bool), AppError> {
    let started = Instant::now();
    let page_size = req.page_size.unwrap_or(DEFAULT_PAGE_SIZE);
    if !(1..=MAX_PAGE_SIZE).contains(&page_size) {
        return Err(AppError::invalid("pageSize", "outOfRange"));
    }
    let c = compile::compile(&req.query)?;
    let after = req.cursor.as_deref().map(cursor::decode).transpose()?;
    let n = page_size as i64 + 1;
    let ids = execute::page(client, &c, after.as_ref(), n, window).await?;
    let next_cursor = (ids.rows.len() as i64 == n).then(|| cursor::encode(&ids.rows[page_size as usize - 1].1));
    let keep: Vec<i64> = ids.rows.iter().take(page_size as usize).map(|(id, _)| *id).collect();
    let rows = hydrate_rows(client, &keep).await?;
    let elapsed_ms = started.elapsed().as_millis().min(u32::MAX as u128) as u32;
    Ok((SearchPage { rows, next_cursor, short_query: c.short_query, elapsed_ms }, ids.phase_b_ran))
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
