//! Bulk writer (spec Part 04 §6.1–§6.3): one tokio task that owns one dedicated `outlooker_app` connection,
//! reorders items by `seq`, and commits each batch (binary COPY into session temp tables, set-based upsert,
//! checkpoint) in one transaction.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::pin::pin;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::mpsc;
use tokio::time::{timeout_at, Instant};
use tokio_postgres::binary_copy::BinaryCopyInWriter;
use tokio_postgres::types::Type;
use tokio_postgres::{Client, Row, Transaction};

use crate::db::db_err;
use crate::dto::error::AppError;
use crate::dto::import::ImportPhase;
use crate::importer::types::{FolderIn, NormalizedItem, Outcome};
use crate::importer::webp::KnownHashes;
use crate::sys::free_space;

/// Batch limits and the disk guard threshold (spec Part 04 §6.2, §5.1). Tests override them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriterLimits {
    pub max_items: usize,
    pub max_bytes: usize,
    pub max_wait: Duration,
    pub min_free_bytes: u64,
}

impl Default for WriterLimits {
    fn default() -> Self {
        WriterLimits { max_items: 1_000, max_bytes: 64 * 1024 * 1024, max_wait: Duration::from_secs(1), min_free_bytes: 2 * 1024 * 1024 * 1024 }
    }
}

/// One row of `import_checkpoints` (spec Part 04 §5.9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkpoint {
    pub pst_id: i64,
    pub job_id: i64,
    pub phase: ImportPhase,
    pub items_total: Option<i64>,
    pub cursor_seq: i64,
    pub file_size: i64,
    pub file_mtime: SystemTime,
    pub stored: i64,
    pub duplicates: i64,
    pub errors: i64,
    pub skipped_non_mail: i64,
    pub images_converted: i64,
    pub batches_committed: i64,
}

macro_rules! cp_cols {
    () => {
        "pst_id, job_id, phase, items_total, cursor_seq, pst_file_size, pst_file_mtime, stored, duplicates, errors, \
         skipped_non_mail, images_converted, batches_committed"
    };
}

const CP_SELECT: &str = concat!("SELECT ", cp_cols!(), " FROM import_checkpoints WHERE pst_id = $1");
const CP_KEEP: &str = concat!(
    "UPDATE import_checkpoints SET job_id = $2, phase = 'scanning', updated_at = now() WHERE pst_id = $1 RETURNING ",
    cp_cols!()
);
const CP_RESET: &str = concat!(
    "INSERT INTO import_checkpoints (", cp_cols!(), ") VALUES ($1, $2, 'scanning', NULL, 0, $3, $4, 0, 0, 0, 0, 0, 0) \
     ON CONFLICT (pst_id) DO UPDATE SET job_id = EXCLUDED.job_id, phase = 'scanning', items_total = NULL, cursor_seq = 0, \
     pst_file_size = EXCLUDED.pst_file_size, pst_file_mtime = EXCLUDED.pst_file_mtime, stored = 0, duplicates = 0, errors = 0, \
     skipped_non_mail = 0, images_converted = 0, batches_committed = 0, updated_at = now() RETURNING ",
    cp_cols!()
);
const CP_SET_PHASE: &str =
    "UPDATE import_checkpoints SET phase = $2, items_total = coalesce($3, items_total), updated_at = now() WHERE pst_id = $1";
const CP_BATCH: &str = concat!(
    "INSERT INTO import_checkpoints (", cp_cols!(), ") VALUES ($1, $2, 'importing', $3, $4, $5, $6, $7, $8, $9, $10, $11, 1) \
     ON CONFLICT (pst_id) DO UPDATE SET job_id = EXCLUDED.job_id, cursor_seq = EXCLUDED.cursor_seq, \
     stored = import_checkpoints.stored + EXCLUDED.stored, duplicates = import_checkpoints.duplicates + EXCLUDED.duplicates, \
     errors = import_checkpoints.errors + EXCLUDED.errors, skipped_non_mail = import_checkpoints.skipped_non_mail + EXCLUDED.skipped_non_mail, \
     images_converted = import_checkpoints.images_converted + EXCLUDED.images_converted, \
     batches_committed = import_checkpoints.batches_committed + 1, updated_at = now() RETURNING ",
    cp_cols!()
);

/// The value PostgreSQL stores for `t` in a `timestamptz` column (postgres-types 0.2.14 truncates toward
/// 2000-01-01 to whole microseconds). Compare file mtimes only after this truncation.
pub fn truncate_to_micros(t: SystemTime) -> SystemTime {
    let pg_epoch = UNIX_EPOCH + Duration::from_secs(946_684_800);
    match t.duration_since(pg_epoch) {
        Ok(d) => pg_epoch + Duration::from_micros(d.as_micros() as u64),
        Err(e) => pg_epoch - Duration::from_micros(e.duration().as_micros() as u64),
    }
}

fn cp_from_row(r: &Row) -> Result<Checkpoint, AppError> {
    let phase = ImportPhase::from_db(r.get(2))?;
    Ok(Checkpoint {
        pst_id: r.get(0),
        job_id: r.get(1),
        phase,
        items_total: r.get(3),
        cursor_seq: r.get(4),
        file_size: r.get(5),
        file_mtime: r.get(6),
        stored: r.get(7),
        duplicates: r.get(8),
        errors: r.get(9),
        skipped_non_mail: r.get(10),
        images_converted: r.get(11),
        batches_committed: r.get(12),
    })
}

impl Checkpoint {
    pub fn items_done(&self) -> i64 {
        self.stored + self.duplicates + self.errors + self.skipped_non_mail
    }

    pub async fn load(client: &Client, pst_id: i64) -> Result<Option<Checkpoint>, AppError> {
        match client.query_opt(CP_SELECT, &[&pst_id]).await.map_err(db_err)? {
            Some(r) => Ok(Some(cp_from_row(&r)?)),
            None => Ok(None),
        }
    }

    /// Job start (spec Part 04 §5.9 step 1). Keeps cursor and counters when (`file_size`, `file_mtime`) equal the
    /// stored identity, else resets them to 0. Sets `job_id` and phase `Scanning`. `was_reset` is true only when an
    /// existing checkpoint was reset.
    pub async fn begin(client: &Client, pst_id: i64, job_id: i64, file_size: i64, file_mtime: SystemTime) -> Result<(Checkpoint, bool), AppError> {
        let mtime = truncate_to_micros(file_mtime);
        let old = Checkpoint::load(client, pst_id).await?;
        if old.as_ref().is_some_and(|cp| cp.file_size == file_size && cp.file_mtime == mtime) {
            let r = client.query_one(CP_KEEP, &[&pst_id, &job_id]).await.map_err(db_err)?;
            return Ok((cp_from_row(&r)?, false));
        }
        let r = client.query_one(CP_RESET, &[&pst_id, &job_id, &file_size, &mtime]).await.map_err(db_err)?;
        Ok((cp_from_row(&r)?, old.is_some()))
    }

    /// `items_total: None` keeps the stored value.
    pub async fn set_phase(client: &Client, pst_id: i64, phase: ImportPhase, items_total: Option<i64>) -> Result<(), AppError> {
        client.execute(CP_SET_PHASE, &[&pst_id, &phase.as_db(), &items_total]).await.map_err(db_err)?;
        Ok(())
    }
}

/// Upsert the scanned folder tree of one PST in one transaction. `folders` must list parents before children
/// (the scan order of the reader).
pub async fn upsert_folders(client: &mut Client, pst_id: i64, folders: &[FolderIn]) -> Result<(), AppError> {
    let tx = client.transaction().await.map_err(db_err)?;
    let stmt = tx
        .prepare(
            "INSERT INTO folders (pst_id, pst_folder_id, parent_id, name, path) \
             VALUES ($1, $2, (SELECT id FROM folders WHERE pst_id = $1 AND pst_folder_id = $3), $4, $5) \
             ON CONFLICT (pst_id, pst_folder_id) DO UPDATE SET parent_id = EXCLUDED.parent_id, name = EXCLUDED.name, path = EXCLUDED.path",
        )
        .await
        .map_err(db_err)?;
    for f in folders {
        tx.execute(&stmt, &[&pst_id, &f.pst_folder_id, &f.parent_pst_folder_id, &f.name, &f.path]).await.map_err(db_err)?;
    }
    tx.commit().await.map_err(db_err)
}

pub async fn load_known_hashes(client: &Client) -> Result<KnownHashes, AppError> {
    let rows = client.query("SELECT source_hash FROM image_blobs", &[]).await.map_err(db_err)?;
    let known = KnownHashes::new();
    known.insert_all(rows.iter().filter_map(|r| <[u8; 32]>::try_from(r.get::<_, &[u8]>(0)).ok()));
    Ok(known)
}

/// Spec Part 04 §6.5, run after every job on an outlooker_app connection outside a transaction.
pub const AFTER_JOB_VACUUM: &str =
    "VACUUM (ANALYZE) messages, message_recipients, message_occurrences, attachments, inline_images, image_blobs, addresses";

/// SQLSTATE 53100 (disk full) → `FileAccess{data_root, insufficientSpace}`, else `db_err`.
fn map_db_error(e: tokio_postgres::Error, data_root: &Path) -> AppError {
    disk_full(e.code().map(|c| c.code()), data_root).unwrap_or_else(|| db_err(e))
}

fn disk_full(sqlstate: Option<&str>, data_root: &Path) -> Option<AppError> {
    (sqlstate == Some("53100")).then(|| AppError::file_access(data_root, "insufficientSpace"))
}

const SESSION_SETUP: &str = "SET synchronous_commit = off;
CREATE TEMP TABLE stage_messages (seq bigint, dedupe_key bytea, dedupe_kind text, message_id text, subject text,
    sent_at timestamptz, received_at timestamptz, sender_email text, sender_address_type text, sender_name text,
    sender_search text, recipients_search text, recipient_count integer, to_preview text, body_source text,
    body_html text, body_text text, snippet text, has_attachments boolean) ON COMMIT DELETE ROWS;
CREATE TEMP TABLE stage_recipients (seq bigint, dedupe_key bytea, ordinal integer, kind text, email text,
    address_type text, display_name text, search_text text) ON COMMIT DELETE ROWS;
CREATE TEMP TABLE stage_occurrences (pst_item_id bigint, pst_folder_id bigint, dedupe_key bytea) ON COMMIT DELETE ROWS;
CREATE TEMP TABLE stage_attachments (seq bigint, dedupe_key bytea, attachment_index integer, file_name text,
    size_bytes bigint, mime_type text, attach_method integer) ON COMMIT DELETE ROWS;
CREATE TEMP TABLE stage_inline_images (seq bigint, dedupe_key bytea, ordinal integer, content_id text, source_hash bytea,
    webp bytea, width integer, height integer, source_mime text) ON COMMIT DELETE ROWS;
CREATE TEMP TABLE stage_errors (stage text, severity text, folder_path text, pst_item_id bigint, message text) ON COMMIT DELETE ROWS;
CREATE TEMP TABLE stage_new_ids (message_id bigint, dedupe_key bytea) ON COMMIT DELETE ROWS;";

/// Session setup of the writer connection: `synchronous_commit = off` (spec Part 04 §6.2) and the seven staging
/// tables (§6.1). Called only by `WriterHandle::spawn`; public so the test can check the session setting.
pub async fn prepare_session(client: &Client) -> Result<(), tokio_postgres::Error> {
    client.batch_execute(SESSION_SETUP).await
}

// Child rows come only from the lowest seq per dedupe key in the batch (the copy that the messages upsert stored).
const INS_ADDRESSES: &str = "INSERT INTO addresses (email, address_type)
SELECT DISTINCT ON (x.email) x.email, x.address_type
FROM (SELECT s.seq, 0 AS ord, s.sender_email AS email, s.sender_address_type AS address_type, s.dedupe_key
        FROM stage_messages s WHERE s.sender_email IS NOT NULL
      UNION ALL
      SELECT r.seq, r.ordinal + 1, r.email, r.address_type, r.dedupe_key FROM stage_recipients r WHERE r.email IS NOT NULL) x
JOIN (SELECT dedupe_key, min(seq) AS seq FROM stage_messages GROUP BY dedupe_key) w ON w.dedupe_key = x.dedupe_key AND w.seq = x.seq
WHERE NOT EXISTS (SELECT 1 FROM messages m WHERE m.dedupe_key = x.dedupe_key)
ORDER BY x.email, x.seq, x.ord
ON CONFLICT (email) DO NOTHING";

const INS_MESSAGES: &str = "WITH ins AS (
    INSERT INTO messages (dedupe_key, dedupe_kind, message_id, subject, sent_at, received_at,
                          sender_address_id, sender_name, sender_search, recipients_search, recipient_count,
                          to_preview, body_source, body_html, body_text, snippet, has_attachments, first_pst_id)
    SELECT DISTINCT ON (s.dedupe_key)
           s.dedupe_key, s.dedupe_kind, s.message_id, s.subject, s.sent_at, s.received_at,
           a.id, s.sender_name, s.sender_search, s.recipients_search, s.recipient_count,
           s.to_preview, s.body_source, s.body_html, s.body_text, s.snippet, s.has_attachments, $1
    FROM stage_messages s
    LEFT JOIN addresses a ON a.email = s.sender_email
    ORDER BY s.dedupe_key, s.seq
    ON CONFLICT (dedupe_key) DO NOTHING
    RETURNING id, dedupe_key
)
INSERT INTO stage_new_ids (message_id, dedupe_key) SELECT id, dedupe_key FROM ins";

const INS_RECIPIENTS: &str = "INSERT INTO message_recipients (message_id, ordinal, kind, address_id, display_name, search_text)
SELECT n.message_id, r.ordinal, r.kind, a.id, r.display_name, r.search_text
FROM stage_recipients r
JOIN stage_new_ids n ON n.dedupe_key = r.dedupe_key
LEFT JOIN addresses a ON a.email = r.email
JOIN (SELECT dedupe_key, min(seq) AS seq FROM stage_messages GROUP BY dedupe_key) w ON w.dedupe_key = r.dedupe_key AND w.seq = r.seq";

const INS_ATTACHMENTS: &str = "INSERT INTO attachments (message_id, attachment_index, file_name, size_bytes, mime_type, attach_method)
SELECT n.message_id, t.attachment_index, t.file_name, t.size_bytes, t.mime_type, t.attach_method
FROM stage_attachments t
JOIN stage_new_ids n ON n.dedupe_key = t.dedupe_key
JOIN (SELECT dedupe_key, min(seq) AS seq FROM stage_messages GROUP BY dedupe_key) w ON w.dedupe_key = t.dedupe_key AND w.seq = t.seq";

const INS_BLOBS: &str = "INSERT INTO image_blobs (source_hash, webp, width, height, source_mime)
SELECT DISTINCT ON (i.source_hash) i.source_hash, i.webp, i.width, i.height, i.source_mime
FROM stage_inline_images i
JOIN stage_new_ids n ON n.dedupe_key = i.dedupe_key
JOIN (SELECT dedupe_key, min(seq) AS seq FROM stage_messages GROUP BY dedupe_key) w ON w.dedupe_key = i.dedupe_key AND w.seq = i.seq
WHERE i.webp IS NOT NULL
ORDER BY i.source_hash, i.seq
ON CONFLICT (source_hash) DO NOTHING
RETURNING source_hash";

const INS_INLINE: &str = "INSERT INTO inline_images (message_id, ordinal, content_id, image_blob_id)
SELECT n.message_id, i.ordinal, i.content_id, b.id
FROM stage_inline_images i
JOIN stage_new_ids n ON n.dedupe_key = i.dedupe_key
JOIN image_blobs b ON b.source_hash = i.source_hash
JOIN (SELECT dedupe_key, min(seq) AS seq FROM stage_messages GROUP BY dedupe_key) w ON w.dedupe_key = i.dedupe_key AND w.seq = i.seq";

const INS_OCCURRENCES: &str = "INSERT INTO message_occurrences (pst_id, pst_item_id, message_id, folder_id)
SELECT $1, o.pst_item_id, m.id, f.id
FROM stage_occurrences o
JOIN messages m ON m.dedupe_key = o.dedupe_key
JOIN folders f ON f.pst_id = $1 AND f.pst_folder_id = o.pst_folder_id
ON CONFLICT (pst_id, pst_item_id) DO NOTHING";

const INS_ERRORS: &str = "INSERT INTO import_errors (pst_id, job_id, stage, severity, folder_path, pst_item_id, message)
SELECT $1, $2, stage, severity, folder_path, pst_item_id, message FROM stage_errors";

/// Bytes an item adds to the open batch (the 64 MB cut of spec Part 04 §6.2).
pub fn staged_bytes(item: &NormalizedItem) -> usize {
    let errors: usize = item.errors.iter().map(|e| 64 + e.message.len() + e.folder_path.as_ref().map_or(0, |p| p.len())).sum();
    let mail = match &item.outcome {
        Outcome::Mail(m) => {
            512 + m.subject.len()
                + m.body_text.len()
                + m.body_html.as_ref().map_or(0, |h| h.len())
                + m.sender_search.len()
                + m.recipients_search.len()
                + m.to_preview.len()
                + m.snippet.len()
                + m.recipients.iter().map(|r| 64 + r.search_text.len()).sum::<usize>()
                + m.attachments.iter().map(|a| 64 + a.file_name.len() + a.mime_type.len()).sum::<usize>()
                + m.inline_images.iter().map(|i| 64 + i.blob.as_ref().map_or(0, |b| b.webp.len())).sum::<usize>()
        }
        Outcome::NonMail | Outcome::Skipped | Outcome::Known => 64,
    };
    mail + errors
}

pub struct WriterConfig {
    pub pst_id: i64,
    pub data_root: PathBuf,
    pub start: Checkpoint,
    pub limits: WriterLimits,
    pub known: Arc<KnownHashes>,
}

pub type OnCommit = Box<dyn Fn(&Checkpoint) + Send + Sync>;

pub struct WriterHandle {
    tx: mpsc::Sender<NormalizedItem>,
    join: tokio::task::JoinHandle<Result<Checkpoint, AppError>>,
}

impl WriterHandle {
    pub async fn spawn(client: Client, cfg: WriterConfig, on_commit: OnCommit) -> Result<WriterHandle, AppError> {
        prepare_session(&client).await.map_err(|e| map_db_error(e, &cfg.data_root))?;
        let (tx, rx) = mpsc::channel(256);
        let join = tokio::spawn(run(client, cfg, rx, on_commit));
        Ok(WriterHandle { tx, join })
    }
    pub fn sender(&self) -> mpsc::Sender<NormalizedItem> {
        self.tx.clone()
    }

    pub async fn submit(&self, item: NormalizedItem) -> Result<(), AppError> {
        self.tx.send(item).await.map_err(|_| AppError::internal("writer stopped"))
    }

    pub async fn finish(self) -> Result<Checkpoint, AppError> {
        drop(self.tx);
        self.join.await.map_err(|e| AppError::internal(format!("writer task: {e}")))?
    }

    pub fn abort(self) {
        self.join.abort();
    }
}

struct Batch {
    items: Vec<NormalizedItem>,
    bytes: usize,
    deadline: Option<Instant>,
}

async fn run(mut client: Client, cfg: WriterConfig, mut rx: mpsc::Receiver<NormalizedItem>, on_commit: OnCommit) -> Result<Checkpoint, AppError> {
    let mut cp = cfg.start.clone();
    let mut next = cp.cursor_seq + 1;
    let mut pending: BTreeMap<i64, NormalizedItem> = BTreeMap::new();
    let mut batch = Batch { items: Vec::new(), bytes: 0, deadline: None };
    loop {
        let received = match batch.deadline {
            Some(d) => timeout_at(d, rx.recv()).await.ok(),
            None => Some(rx.recv().await),
        };
        match received {
            None => {} // batch deadline passed
            Some(None) => {
                if !batch.items.is_empty() {
                    cp = flush(&mut client, &cfg, &cp, &mut batch, &on_commit).await?;
                }
                return Ok(cp);
            }
            Some(Some(item)) => {
                if item.seq >= next {
                    pending.insert(item.seq, item);
                }
                while let Some(item) = pending.remove(&next) {
                    next += 1;
                    if batch.items.is_empty() {
                        batch.deadline = Some(Instant::now() + cfg.limits.max_wait);
                    }
                    batch.bytes += staged_bytes(&item);
                    batch.items.push(item);
                    if batch.items.len() >= cfg.limits.max_items || batch.bytes >= cfg.limits.max_bytes {
                        cp = flush(&mut client, &cfg, &cp, &mut batch, &on_commit).await?;
                    }
                }
            }
        }
        if !batch.items.is_empty() && batch.deadline.is_some_and(|d| Instant::now() >= d) {
            cp = flush(&mut client, &cfg, &cp, &mut batch, &on_commit).await?;
        }
    }
}

async fn flush(client: &mut Client, cfg: &WriterConfig, cp: &Checkpoint, batch: &mut Batch, on_commit: &OnCommit) -> Result<Checkpoint, AppError> {
    let free = free_space(&cfg.data_root).unwrap_or(0); // unknown volume → 0: the guard fails closed
    if free < cfg.limits.min_free_bytes {
        return Err(AppError::file_access(&cfg.data_root, "insufficientSpace"));
    }
    let (next, new_hashes) = commit_batch(client, cfg, cp, &batch.items).await.map_err(|e| map_db_error(e, &cfg.data_root))?;
    cfg.known.insert_all(new_hashes);
    on_commit(&next);
    batch.items.clear();
    batch.bytes = 0;
    batch.deadline = None;
    Ok(next)
}

async fn commit_batch(
    client: &mut Client,
    cfg: &WriterConfig,
    cp: &Checkpoint,
    items: &[NormalizedItem],
) -> Result<(Checkpoint, Vec<[u8; 32]>), tokio_postgres::Error> {
    let tx = client.transaction().await?;
    copy_batch(&tx, items).await?;
    tx.execute(INS_ADDRESSES, &[]).await?;
    let stored = tx.execute(INS_MESSAGES, &[&cfg.pst_id]).await? as i64;
    tx.execute(INS_RECIPIENTS, &[]).await?;
    tx.execute(INS_ATTACHMENTS, &[]).await?;
    let new_hashes: Vec<[u8; 32]> = tx
        .query(INS_BLOBS, &[])
        .await?
        .iter()
        .filter_map(|r| <[u8; 32]>::try_from(r.get::<_, &[u8]>(0)).ok())
        .collect();
    tx.execute(INS_INLINE, &[]).await?;
    tx.execute(INS_OCCURRENCES, &[&cfg.pst_id]).await?;
    tx.execute(INS_ERRORS, &[&cfg.pst_id, &cp.job_id]).await?;
    let mails = items.iter().filter(|i| matches!(i.outcome, Outcome::Mail(_))).count() as i64;
    let skipped = items.iter().filter(|i| matches!(i.outcome, Outcome::Skipped)).count() as i64;
    let non_mail = items.iter().filter(|i| matches!(i.outcome, Outcome::NonMail)).count() as i64;
    let known = items.iter().filter(|i| matches!(i.outcome, Outcome::Known)).count() as i64;
    let cursor = items.last().map_or(cp.cursor_seq, |i| i.seq);
    let row = tx
        .query_one(
            CP_BATCH,
            &[
                &cfg.pst_id,
                &cp.job_id,
                &cp.items_total,
                &cursor,
                &cp.file_size,
                &cp.file_mtime,
                &stored,
                &(mails - stored + known),
                &skipped,
                &non_mail,
                &(new_hashes.len() as i64),
            ],
        )
        .await?;
    tx.commit().await?;
    let next = cp_from_row(&row).expect("checkpoint row written by this function");
    Ok((next, new_hashes))
}

async fn copy_batch(tx: &Transaction<'_>, items: &[NormalizedItem]) -> Result<(), tokio_postgres::Error> {
    use Type as T;
    let sink = tx.copy_in("COPY stage_messages FROM STDIN (FORMAT binary)").await?;
    let types = [T::INT8, T::BYTEA, T::TEXT, T::TEXT, T::TEXT, T::TIMESTAMPTZ, T::TIMESTAMPTZ, T::TEXT, T::TEXT, T::TEXT, T::TEXT, T::TEXT, T::INT4, T::TEXT, T::TEXT, T::TEXT, T::TEXT, T::TEXT, T::BOOL];
    let mut w = pin!(BinaryCopyInWriter::new(sink, &types));
    for it in items {
        let Outcome::Mail(m) = &it.outcome else { continue };
        let email = m.sender.as_ref().map(|a| a.email.as_str());
        let atype = m.sender.as_ref().map(|a| a.address_type.as_db());
        w.as_mut()
            .write(&[
                &it.seq,
                &&m.dedupe_key[..],
                &m.dedupe_kind.as_db(),
                &m.message_id,
                &m.subject,
                &m.sent_at,
                &m.received_at,
                &email,
                &atype,
                &m.sender_name,
                &m.sender_search,
                &m.recipients_search,
                &(m.recipients.len() as i32),
                &m.to_preview,
                &m.body_source.as_db(),
                &m.body_html,
                &m.body_text,
                &m.snippet,
                &!m.attachments.is_empty(),
            ])
            .await?;
    }
    w.finish().await?;

    let sink = tx.copy_in("COPY stage_recipients FROM STDIN (FORMAT binary)").await?;
    let mut w = pin!(BinaryCopyInWriter::new(sink, &[T::INT8, T::BYTEA, T::INT4, T::TEXT, T::TEXT, T::TEXT, T::TEXT, T::TEXT]));
    for it in items {
        let Outcome::Mail(m) = &it.outcome else { continue };
        for r in &m.recipients {
            let email = r.address.as_ref().map(|a| a.email.as_str());
            let atype = r.address.as_ref().map(|a| a.address_type.as_db());
            w.as_mut()
                .write(&[&it.seq, &&m.dedupe_key[..], &r.ordinal, &r.kind.as_db(), &email, &atype, &r.display_name, &r.search_text])
                .await?;
        }
    }
    w.finish().await?;

    let sink = tx.copy_in("COPY stage_occurrences FROM STDIN (FORMAT binary)").await?;
    let mut w = pin!(BinaryCopyInWriter::new(sink, &[T::INT8, T::INT8, T::BYTEA]));
    for it in items {
        let Outcome::Mail(m) = &it.outcome else { continue };
        w.as_mut().write(&[&it.pst_item_id, &it.pst_folder_id, &&m.dedupe_key[..]]).await?;
    }
    w.finish().await?;

    let sink = tx.copy_in("COPY stage_attachments FROM STDIN (FORMAT binary)").await?;
    let mut w = pin!(BinaryCopyInWriter::new(sink, &[T::INT8, T::BYTEA, T::INT4, T::TEXT, T::INT8, T::TEXT, T::INT4]));
    for it in items {
        let Outcome::Mail(m) = &it.outcome else { continue };
        for a in &m.attachments {
            w.as_mut()
                .write(&[&it.seq, &&m.dedupe_key[..], &a.attachment_index, &a.file_name, &a.size_bytes, &a.mime_type, &a.attach_method])
                .await?;
        }
    }
    w.finish().await?;

    let sink = tx.copy_in("COPY stage_inline_images FROM STDIN (FORMAT binary)").await?;
    let mut w = pin!(BinaryCopyInWriter::new(sink, &[T::INT8, T::BYTEA, T::INT4, T::TEXT, T::BYTEA, T::BYTEA, T::INT4, T::INT4, T::TEXT]));
    for it in items {
        let Outcome::Mail(m) = &it.outcome else { continue };
        for i in &m.inline_images {
            let webp = i.blob.as_ref().map(|b| b.webp.as_slice());
            let width = i.blob.as_ref().map(|b| b.width);
            let height = i.blob.as_ref().map(|b| b.height);
            let mime = i.blob.as_ref().map(|b| b.source_mime.as_str());
            w.as_mut()
                .write(&[&it.seq, &&m.dedupe_key[..], &i.ordinal, &i.content_id, &&i.source_hash[..], &webp, &width, &height, &mime])
                .await?;
        }
    }
    w.finish().await?;

    let sink = tx.copy_in("COPY stage_errors FROM STDIN (FORMAT binary)").await?;
    let mut w = pin!(BinaryCopyInWriter::new(sink, &[T::TEXT, T::TEXT, T::TEXT, T::INT8, T::TEXT]));
    for it in items {
        for e in &it.errors {
            w.as_mut().write(&[&e.stage.as_db(), &e.severity.as_db(), &e.folder_path, &e.pst_item_id, &e.message]).await?;
        }
    }
    w.finish().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disk_full_names_the_data_root() {
        let root = Path::new("/data/root");
        assert_eq!(disk_full(Some("53100"), root), Some(AppError::file_access(root, "insufficientSpace")));
        assert_eq!(disk_full(Some("23505"), root), None);
        assert_eq!(disk_full(None, root), None);
    }
}
