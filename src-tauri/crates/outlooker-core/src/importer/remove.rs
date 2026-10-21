//! PST removal body (spec Part 04 §4.3). Runs on an outlooker_app connection (DELETE + MAINTAIN privileges).
use tokio_postgres::Client;

use crate::dto::error::AppError;
use crate::importer::writer::{map_db_error, AFTER_JOB_VACUUM};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RemoveStats {
    pub occurrences_deleted: u64,
    pub messages_deleted: u64,
}

/// The chunk statement of §4.3, wrapped so it returns (occurrences deleted, messages deleted).
const CHUNK: &str = "WITH chunk AS (
    SELECT pst_item_id FROM message_occurrences
    WHERE pst_id = $1 ORDER BY pst_item_id LIMIT 5000
), gone AS (
    DELETE FROM message_occurrences o USING chunk c
    WHERE o.pst_id = $1 AND o.pst_item_id = c.pst_item_id
    RETURNING o.message_id
), dropped AS (
    DELETE FROM messages m
    WHERE m.id IN (SELECT message_id FROM gone)
      AND NOT EXISTS (SELECT 1 FROM message_occurrences o2
                      WHERE o2.message_id = m.id AND o2.pst_id <> $1)
    RETURNING 1
)
SELECT (SELECT count(*) FROM gone), (SELECT count(*) FROM dropped)";

const ORPHAN_BLOBS: &str = "DELETE FROM image_blobs b WHERE NOT EXISTS (SELECT 1 FROM inline_images i WHERE i.image_blob_id = b.id)";
const ORPHAN_ADDRESSES: &str = "DELETE FROM addresses a
WHERE NOT EXISTS (SELECT 1 FROM messages m WHERE m.sender_address_id = a.id)
  AND NOT EXISTS (SELECT 1 FROM message_recipients r WHERE r.address_id = a.id)";
const DELETE_SOURCE: &str = "DELETE FROM pst_sources WHERE id = $1";

fn pg(e: tokio_postgres::Error) -> AppError {
    map_db_error(e, std::path::Path::new(""))
}

/// Idempotent: after a crash mid-removal a second run finishes the job; on a deleted PST it returns the default stats.
pub async fn run(client: &Client, pst_id: i64) -> Result<RemoveStats, AppError> {
    let mut stats = RemoveStats::default();
    loop {
        // One statement = one transaction (autocommit), i.e. one transaction per 5,000-occurrence chunk.
        let row = client.query_one(CHUNK, &[&pst_id]).await.map_err(pg)?;
        let (occ, msg): (i64, i64) = (row.get(0), row.get(1));
        if occ == 0 {
            break;
        }
        stats.occurrences_deleted += occ as u64;
        stats.messages_deleted += msg as u64;
    }
    client.batch_execute("BEGIN").await.map_err(pg)?;
    let last = async {
        delete_orphans(client).await?;
        client.execute(DELETE_SOURCE, &[&pst_id]).await.map_err(pg)?;
        client.batch_execute("COMMIT").await.map_err(pg)
    }
    .await;
    if let Err(e) = last {
        let _ = client.batch_execute("ROLLBACK").await;
        return Err(e);
    }
    client.batch_execute(AFTER_JOB_VACUUM).await.map_err(pg)?;
    Ok(stats)
}

/// Orphan blobs, then orphan addresses (spec Part 04 §4.3 second block). Returns (blobs, addresses) deleted.
pub async fn delete_orphans(client: &Client) -> Result<(u64, u64), AppError> {
    let blobs = client.execute(ORPHAN_BLOBS, &[]).await.map_err(pg)?;
    let addresses = client.execute(ORPHAN_ADDRESSES, &[]).await.map_err(pg)?;
    Ok((blobs, addresses))
}
