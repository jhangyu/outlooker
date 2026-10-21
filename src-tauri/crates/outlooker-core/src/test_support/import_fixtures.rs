//! Row helpers for the writer, removal, import-pipeline, search and SQL-console tests and the load generator.
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::db::Db;
use crate::dto::search::RecipientKind;
use crate::importer::dedupe;
use crate::importer::types::{AddressIn, AddressType, BodySource, NormalizedItem, NormalizedMail, Outcome, RecipientOut};
use crate::importer::webp::KnownHashes;
use crate::importer::writer::{Checkpoint, Writer, WriterConfig, WriterLimits};

/// INSERT pst_sources + one import job (state 'running', started_at now()) + one folder (pst_folder_id 1, path "Inbox").
/// Only one job may be running in a database (`import_jobs_one_running`): call `complete_job` before the next source.
pub async fn new_source(client: &tokio_postgres::Client, path: &str) -> (i64, i64) {
    let name = path.rsplit('/').next().unwrap_or(path);
    let pst_id: i64 = client
        .query_one(
            "INSERT INTO pst_sources (path, display_name, file_size_bytes, file_mtime) VALUES ($1, $2, 1, now()) RETURNING id",
            &[&path, &name],
        )
        .await
        .expect("insert pst_sources")
        .get(0);
    let job_id: i64 = client
        .query_one("INSERT INTO import_jobs (pst_id, kind, state, started_at) VALUES ($1, 'import', 'running', now()) RETURNING id", &[&pst_id])
        .await
        .expect("insert import_jobs")
        .get(0);
    client
        .execute("INSERT INTO folders (pst_id, pst_folder_id, name, path) VALUES ($1, 1, 'Inbox', 'Inbox')", &[&pst_id])
        .await
        .expect("insert folder");
    (pst_id, job_id)
}

/// Marks a job 'completed' (finished_at now()).
pub async fn complete_job(client: &tokio_postgres::Client, job_id: i64) {
    client
        .execute("UPDATE import_jobs SET state = 'completed', finished_at = now() WHERE id = $1", &[&job_id])
        .await
        .expect("complete job");
}

/// A NormalizedMail with deterministic defaults: sent 2023-11-14T22:13:20Z, body source `Text`, no recipients.
/// `message_id: None` → content-hash key from sender, sent time, subject and body.
pub fn mail(subject: &str, body_text: &str, sender: &str, message_id: Option<&str>) -> NormalizedMail {
    let sent = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    let key = dedupe::key(message_id, None, sender, Some(sent), subject, body_text);
    let address = (!sender.is_empty()).then(|| AddressIn { email: sender.to_lowercase(), address_type: AddressType::Smtp });
    NormalizedMail {
        dedupe_key: key.key,
        dedupe_kind: key.kind,
        message_id: key.message_id,
        subject: subject.to_owned(),
        sent_at: Some(sent),
        received_at: Some(sent),
        sender: address,
        sender_name: None,
        sender_search: sender.to_owned(),
        recipients: Vec::new(),
        recipients_search: String::new(),
        to_preview: String::new(),
        body_source: BodySource::Text,
        body_html: None,
        body_text: body_text.to_owned(),
        snippet: body_text.chars().take(200).collect(),
        attachments: Vec::new(),
        inline_images: Vec::new(),
    }
}

/// Adds a recipient with an SMTP address and keeps `recipients_search` / `to_preview` consistent.
pub fn add_recipient(m: &mut NormalizedMail, kind: RecipientKind, name: Option<&str>, email: &str) {
    let search_text = match name {
        Some(n) => format!("{n} <{email}>"),
        None => email.to_owned(),
    };
    m.recipients.push(RecipientOut {
        ordinal: m.recipients.len() as i32,
        kind,
        address: Some(AddressIn { email: email.to_lowercase(), address_type: AddressType::Smtp }),
        display_name: name.map(str::to_owned),
        search_text,
    });
    m.recipients_search = m.recipients.iter().map(|r| r.search_text.as_str()).collect::<Vec<_>>().join("\n");
    m.to_preview = m
        .recipients
        .iter()
        .filter(|r| r.kind == RecipientKind::To)
        .take(3)
        .map(|r| r.display_name.clone().unwrap_or_else(|| r.address.as_ref().map(|a| a.email.clone()).unwrap_or_default()))
        .collect::<Vec<_>>()
        .join(", ");
}

/// `pst_folder_id` 1 (the folder created by `new_source`).
pub fn item(seq: i64, pst_item_id: i64, mail: NormalizedMail) -> NormalizedItem {
    NormalizedItem { seq, pst_item_id, pst_folder_id: 1, raw_bytes: 0, outcome: Outcome::Mail(Box::new(mail)), errors: Vec::new() }
}

/// Runs one writer over `items` for a new source (new_source + Checkpoint::begin + Writer::spawn on db.connect_app()
/// + finish + job 'completed'). Returns pst_id.
pub async fn seed_mails(db: &Db, path: &str, items: Vec<NormalizedItem>) -> i64 {
    let app = db.connect_app().await.expect("app connection");
    let (pst_id, job_id) = new_source(&app, path).await;
    let (start, _) = Checkpoint::begin(&app, pst_id, job_id, 1, SystemTime::now()).await.expect("checkpoint");
    let cfg = WriterConfig {
        pst_id,
        data_root: db.data_root().expect("data root"),
        start,
        limits: WriterLimits::default(),
        known: Arc::new(KnownHashes::new()),
    };
    let writer = Writer::spawn(db.connect_app().await.expect("writer connection"), cfg, Box::new(|_| {})).await.expect("writer");
    for it in items {
        writer.submit(it).await.expect("submit");
    }
    writer.finish().await.expect("writer finish");
    complete_job(&app, job_id).await;
    pst_id
}
