//! Writes generated sources through the real writer (spec Part 04 §12.3).
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use outlooker_core::db::{db_err, Db};
use outlooker_core::db::indexes::{drop_search_indexes, ensure_search_indexes};
use outlooker_core::dto::error::AppError;
use outlooker_core::importer::types::FolderIn;
use outlooker_core::importer::writer::{
    load_known_hashes, upsert_folders, Checkpoint, WriterConfig, WriterHandle, WriterLimits, AFTER_JOB_VACUUM,
};
use outlooker_core::test_support::import_fixtures::complete_job;
use rayon::prelude::*;

use crate::generate::{GenConfig, Generator, Slot};
use crate::root::LibraryRoot;

const CHUNK: u64 = 1_000;

#[derive(Debug, Clone, PartialEq)]
pub struct LoadReport {
    pub messages_submitted: u64,
    /// Slots that regenerate an earlier slot (Message-ID or content-hash duplicates).
    pub duplicates_planned: u64,
    pub messages_stored: i64,
    pub occurrences: i64,
    pub write_elapsed: Duration,
    pub index_elapsed: Duration,
}

pub fn source_path(s: u32) -> String {
    format!("/loadgen/source-{s:02}.pst")
}

async fn create_source(db: &Db, s: u32) -> Result<(i64, i64), AppError> {
    let mut app = db.connect_app().await?;
    let path = source_path(s);
    let name = format!("source-{s:02}.pst");
    let pst_id: i64 = app
        .query_one(
            "INSERT INTO pst_sources (path, display_name, file_size_bytes, paused) VALUES ($1, $2, 1, true) RETURNING id",
            &[&path, &name],
        )
        .await
        .map_err(db_err)?
        .get(0);
    let job_id: i64 = app
        .query_one("INSERT INTO import_jobs (pst_id, kind, state, started_at) VALUES ($1, 'import', 'running', now()) RETURNING id", &[&pst_id])
        .await
        .map_err(db_err)?
        .get(0);
    let mut folders = vec![FolderIn { pst_folder_id: 1, parent_pst_folder_id: None, name: "Inbox".into(), path: "Inbox".into() }];
    for year in 2015..=2025 {
        folders.push(FolderIn { pst_folder_id: year, parent_pst_folder_id: Some(1), name: year.to_string(), path: format!("Inbox/{year}") });
    }
    upsert_folders(&mut app, pst_id, &folders).await?;
    Ok((pst_id, job_id))
}

/// Generates and writes `sources` of `cfg`, one writer per source. `defer_indexes`: drop the six search indexes
/// before the first source and rebuild them after the last (`index_elapsed`), as the importer's deferred policy.
pub async fn load_sources(db: &Db, cfg: &GenConfig, sources: Range<u32>, defer_indexes: bool) -> Result<LoadReport, AppError> {
    let admin = db.connect_admin().await?;
    if defer_indexes {
        drop_search_indexes(&admin).await?;
    }
    let known = Arc::new(load_known_hashes(&db.connect_app().await?).await?);
    let generator = Arc::new(Generator::new(cfg.clone()));
    let (mut submitted, mut planned) = (0u64, 0u64);
    let started = Instant::now();
    for s in sources {
        let (pst_id, job_id) = create_source(db, s).await?;
        let app = db.connect_app().await?;
        let (start, _) = Checkpoint::begin(&app, pst_id, job_id, 1, SystemTime::now()).await?;
        let wcfg = WriterConfig { pst_id, data_root: db.data_root()?, start, limits: WriterLimits::default(), known: known.clone() };
        let writer = WriterHandle::spawn(db.connect_app().await?, wcfg, Box::new(|_| {})).await?;
        let n = cfg.per_source(s);
        let mut i = 0;
        'generate: while i < n {
            let range = i..(i + CHUNK).min(n);
            let (g, k) = (generator.clone(), known.clone());
            let items = tokio::task::spawn_blocking(move || range.into_par_iter().map(|i| g.item(s, i, &k)).collect::<Vec<_>>())
                .await
                .map_err(|e| AppError::internal(format!("generator: {e}")))?;
            for it in items {
                if writer.submit(it).await.is_err() {
                    break 'generate; // the writer stopped; finish() returns its error
                }
            }
            i = (i + CHUNK).min(n);
        }
        writer.finish().await?;
        complete_job(&app, job_id).await;
        submitted += n;
        planned += (0..n).filter(|&i| generator.slot(s, i) != Slot::Fresh).count() as u64;
    }
    let write_elapsed = started.elapsed();
    let app = db.connect_app().await?;
    app.batch_execute(AFTER_JOB_VACUUM).await.map_err(db_err)?;
    let t = Instant::now();
    if defer_indexes {
        ensure_search_indexes(&admin, &|name: &str, done: usize, total: usize| log_progress(name, done, total)).await?;
    }
    let index_elapsed = t.elapsed();
    let count = |sql: &'static str| {
        let admin = &admin;
        async move { admin.query_one(sql, &[]).await.map(|r| r.get::<_, i64>(0)).map_err(db_err) }
    };
    Ok(LoadReport {
        messages_submitted: submitted,
        duplicates_planned: planned,
        messages_stored: count("SELECT count(*) FROM messages").await?,
        occurrences: count("SELECT count(*) FROM message_occurrences").await?,
        write_elapsed,
        index_elapsed,
    })
}

fn log_progress(name: &str, done: usize, total: usize) {
    eprintln!("index {name} ({done}/{total})");
}

/// All sources of `cfg` with deferred indexes.
pub async fn load(db: &Db, cfg: &GenConfig) -> Result<LoadReport, AppError> {
    load_sources(db, cfg, 0..cfg.sources, true).await
}

/// `outlooker-loadgen seed`: create the root, load `GenConfig::seed(messages)`, stop.
pub async fn seed(data_root: &Path, messages: u64) -> Result<LoadReport, AppError> {
    let root = LibraryRoot::create(data_root).await?;
    let report = load(&root.db, &GenConfig::seed(messages)).await;
    root.stop().await?;
    report
}
