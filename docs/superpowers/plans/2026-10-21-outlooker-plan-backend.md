# Outlooker Backend Implementation Plan (R1-05, R2-09, R2-10, R2-11, R2-12)

> **For agentic workers:** REQUIRED SUB-SKILL: Use podium:team-spawn (recommended) or podium:team-fable to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the Rust core backend of Outlooker: mail normalization (R1-05), the import pipeline and queue (R2-09), the bulk writer, PST removal and the 1M load generator (R2-10), the Search Service (R2-11), and the SQL Console and Maintenance services (R2-12), as defined in `docs/superpowers/specs/2026-10-21-outlooker-design.md` (the spec; "Part 04 §6.3" = spec Part 04 section 6.3).

**Architecture:** All code lives in crate `outlooker-core` (`src-tauri/crates/outlooker-core`, written `core/` below) plus the dev crate `src-tauri/crates/outlooker-loadgen`. Import = one reader `std::thread` (libpff) → a dedicated rayon pool (`normalize` + `webp`) → one tokio writer task with its own `outlooker_app` connection (reorder by `seq`, binary COPY into session temp tables, set-based upsert, checkpoint in the same transaction). Search compiles a `SearchQuery` into one SQL predicate built only from constant fragments plus `$n` parameters, then runs the two-phase keyset execution of Part 04 §7.4. Advanced SQL runs on the `outlooker_ro` pool inside `BEGIN READ ONLY … ROLLBACK`.

**Tech Stack:** Rust 1.99.1; tokio 1.54.0; tokio-postgres 0.7.18; deadpool-postgres 0.14.2; rayon 1.12.0; encoding_rs 0.8.42; chardetng 1.0.0; ammonia 4.2.4; html2text 0.17.4; image 0.25.10; webp 0.3.1; blake3 1.8.7; serde 1.0.229; serde_json 1.0.154; log 0.4.34. Exact pins: outline Tech Stack + spec Part 01.

**Decisions (hard-to-reverse only):**
- **Writer connection is unpooled** (alternatives: take a pooled `outlooker_app` connection; open a raw connection inside the writer from credentials). Drivers: the writer sets `synchronous_commit = off` and creates session temp tables, neither may leak into a pooled connection (Part 04 §6.2). Drawback: one extra connection per running job (budget in Part 04 §6.6 `max_connections` row: "writer (1)").
- **Image blob dedupe uses a known-hash set updated only after commit** (alternative: insert hashes when encoded). Driver: an item with a lower `seq` can be committed in an earlier batch than the item that encoded the shared image; a hash marked known before its blob commits would lose that image. Drawback: a repeated image inside one uncommitted window is encoded more than once (the `DISTINCT ON` + `ON CONFLICT (source_hash)` insert stores it once).
- **Timestamps cross the Rust ↔ SQL boundary as `std::time::SystemTime` on the write side and as SQL-formatted RFC 3339 text on the read side** (alternative: add `chrono`/`time`). Driver: Part 01 lists no date crate; tokio-postgres implements `ToSql`/`FromSql` for `SystemTime` (postgres-types 0.2 `lib.rs:1241`); `to_char(... AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')` gives the Part 03 §1 format. Drawback: the RFC 3339 format string lives in SQL: `db::RFC3339_UTC_FMT`, bound as the `to_char` format parameter.

Plan gate: `/Users/jhangyu/.claude/plugins/cache/jhangyu/podium/1.1.0/protocols/review.md` §L0 Plan gate.

## Global Constraints

The outline `docs/superpowers/plans/2026-10-21-outlooker-plan.md` §"Global Constraints" applies to every task in this file, verbatim. The rows that bind these tasks most:

- **Versions:** only dependencies of spec Part 01 at their pinned versions. No new dependency without leader approval. `Cargo.lock` committed, builds with `--locked`.
- **Forbidden:** sqlx, bindgen, testcontainers, `postgresql_embedded`, mail-parser.
- **SQL rule:** no user value is ever concatenated into SQL (spec Part 04 §11). SQL text is built only from `&'static str` fragments and `$n` placeholders. The only exception is Part 04 §8.1: the user's own Advanced SQL statement text runs as itself on `outlooker_ro`.
- **Type rule:** request / response / event types are the Rust DTOs in `core/src/dto/` (C1). No module in this file defines a second copy of a DTO.
- **i18n rule:** the core never returns user-facing text. `AppError` carries codes; `ImportErrorEntry.message` and `AppError::Internal.detail` are English diagnostic text.
- **Test seams:** production constants stay as in the spec: search window W = 5,000; writer batch 1,000 items / 64 MB / 1 s; free-space threshold 2 GiB. Tests change them only through `WriterLimits`, `ImportTuning` (R2-09) and the `window` argument of `search::execute::page` (R2-11).
- **Tests that need an external artifact** (fetched PostgreSQL, `tests/fixtures/external/outlook.pst`) fail with an explicit message when it is missing. They never skip silently.
- **No sleep / polling** in code or tests. Waiting uses channels, `tokio::sync::Notify`, `tokio::sync::watch`, or the writer's batch timer (`tokio::time::timeout_at`, a deadline, not a loop).
- **Git:** no `git stash` (incl. `git stash list`), `git reset`, `git checkout --`, `git clean`, `git add -A` / `git add .`. Commit with `git add <own paths>` then `git commit -F <msgfile> -- <own paths>`. Run `git rev-parse --abbrev-ref HEAD` before and `git status --porcelain` after each commit. No `;` in commit message text.
- **Commands:** every `cargo` command runs in `src-tauri/`. Integration tests that need PostgreSQL require `node scripts/fetch-postgres.mjs` first.
- **Ownership:** each task edits only its owned paths (outline §5 / §6). A needed change elsewhere goes to the leader.

## Shared names used by every task in this file

Consumed from the foundation plan (`docs/superpowers/plans/2026-10-21-outlooker-plan-foundation.md`, producer planner-1). The exact signatures below are the ones this file codes against; section "Consumes cross-check" at the end of this file records the confirmation per item.

| Name | Meaning here |
|---|---|
| `crate::dto::error::{AppError, BusyReason}` | Part 03 §1.2 enum, `thiserror` + serde |
| `crate::dto::app::{AppPhase, LibraryCounts}` | Part 03 §4 |
| `crate::dto::search::*` | `SearchQuery`, `ConditionGroup`, `ConditionNode`, `GroupOp`, `RecipientKind`, `SearchRequest`, `SearchPage`, `MailRow`, `Address`, `SearchCount`, `FilterOptions`, `PstRef`, `FolderRef`, `MailDetail`, `AttachmentMeta`, `Occurrence` (Part 03 §5) |
| `crate::dto::sql::*` | `SavedQuery`, `SavedQueryInput`, `SqlExecuteRequest`, `SqlResult`, `ColumnMeta`, `CellValue` (Part 03 §6) |
| `crate::dto::import::*` | `PstSource`, `ImportStatus`, `ImportProgress`, `ImportPhase`, `EnqueueResult`, `ImportErrorsRequest`, `ImportErrorPage`, `ImportErrorEntry`, `ImportErrorStage`, `ImportErrorSeverity` (Part 03 §7) |
| `crate::dto::settings::{MaintenanceTask}` and `crate::dto::events::{ImportStatusChanged, LibraryChanged, MaintenanceState, MaintenanceRunState}` | Part 03 §8.3, §9 |
| `crate::sink::{EventSink, CoreEvent, RecordingSink, NullSink}` | C2: `CoreEvent::{ImportProgress(ImportProgress), ImportStatusChanged(ImportStatusChanged), LibraryChanged(LibraryChanged), MaintenanceStatusChanged(MaintenanceState), ..}` |
| `crate::gate::{Gate, Activity, GateGuard, busy_reason}` | C3: `Gate` is `Clone` (passed by value); one activity at a time; `try_enter(Activity) -> Result<GateGuard, AppError>`; `pub async fn released(&self)` (resolves when no activity holds the gate; Notify, no polling) |
| `crate::db::{Db, DbConn, Pools, RFC3339_UTC_FMT, pg_err, connect_admin, migrate}` | C8: `Db::{app_pool, ro_pool, connect_admin, connect_app, data_root}` → `Err(DbNotReady)` when no cluster; `RFC3339_UTC_FMT` used as `to_char(ts AT TIME ZONE 'UTC', '<fmt>')` |
| `crate::db::indexes::{SEARCH_INDEXES, TSV_EXPR, drop_search_indexes, ensure_search_indexes, missing_or_invalid}` | C8 (admin connection only) |
| `crate::db::credentials::Credentials`, `crate::paths::{Paths, dev_pg_dir}`, `crate::lifecycle::pg::{PgController, StopMode}` | C6, C7, C8 (loadgen `seed`) |
| `crate::test_support::TestCluster` | C8 (feature `test-support`): fields `paths, port, creds, db, locale`; `start()`, `admin()`, `app()`, `ro()`, `kill()`, `restart(&mut self)` |
| `pff::{File, Item, AttachmentType, tags, filetime_to_unix_micros}` | C5 |
| `base64 = "=0.23.1"` (`engine::general_purpose::{URL_SAFE_NO_PAD, STANDARD}`), `sysinfo = "=0.39.6"` (`Disks::new_with_refreshed_list`, `Disk::{mount_point, available_space}`), `tempfile = "=3.27.0"` (dev) | approved by the leader, spec Part 01 §8 |

---

## Task R1-05: Mail normalization — charset, RTF de-encapsulation, sanitizer, search text, WebP, dedupe key

**Files:** (all paths under `src-tauri/crates/outlooker-core/`)
- Modify: `src/importer/mod.rs` (kickoff file; keep the kickoff `mod` lines for `queue`, `pst_reader`, `inflight`, `progress`, `writer`, `remove`; this task owns the `types`, `normalize`, `webp`, `dedupe` lines)
- Create/own: `src/importer/types.rs`
- Create/own: `src/importer/normalize/mod.rs`, `src/importer/normalize/charset.rs`, `src/importer/normalize/rtf.rs`, `src/importer/normalize/sanitize.rs`, `src/importer/normalize/text.rs`, `src/importer/normalize/participants.rs`, `src/importer/normalize/attachments.rs`
- Create/own: `src/importer/webp.rs`, `src/importer/dedupe.rs`
- Create/own (golden files): `tests/golden/plain.html`, `tests/golden/plain.expected.html`, `tests/golden/inline.html`, `tests/golden/inline.expected.html`, `tests/golden/fromhtml.rtf`, `tests/golden/fromhtml.expected.html`, `tests/golden/hostile.html`
- Tests: `#[cfg(test)] mod tests` inside each file above (module paths start with `importer::`, so `cargo test -p outlooker-core importer::` selects them).

**Interfaces:**
- Consumes:
  - C1: `crate::dto::import::{ImportErrorStage, ImportErrorSeverity}` (Part 03 §7.2 variants `Open, Scan, Parse, Html, Image, Store` and `ItemSkipped, Degraded, JobFailed`), `crate::dto::search::RecipientKind` (`To, Cc, Bcc`). No other module.
  - Crates from the kickoff `core/Cargo.toml`: `encoding_rs`, `chardetng`, `ammonia`, `html2text`, `image` (formats `png`, `jpeg`, `gif`, `bmp` enabled), `webp`, `rayon`, `blake3`, `base64` 0.23.1 (`STANDARD` engine, only in `sanitize.rs` for `data:` URIs).
- Produces (C9; consumers R2-09 reader / pipeline, R2-10 writer / loadgen). File `src/importer/types.rs`:

```rust
use std::time::SystemTime;
use crate::dto::import::{ImportErrorSeverity, ImportErrorStage};
use crate::dto::search::RecipientKind;

/// One PST folder found by the scan (Part 04 §5.2). Sent once per job before any item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderIn {
    pub pst_folder_id: i64,                 // libpff_item_get_identifier() of the folder
    pub parent_pst_folder_id: Option<i64>,  // None for the walk root
    pub name: String,                       // original name
    pub path: String,                       // "/"-joined; '/' inside a name replaced by U+2215
}

/// One PST item as the reader hands it to normalize. `seq` starts at 1 (Part 04 §5.1).
#[derive(Debug, Clone)]
pub struct RawItem {
    pub seq: i64,
    pub pst_item_id: i64,
    pub pst_folder_id: i64,
    pub folder_path: String,
    pub content: RawContent,
}

#[derive(Debug, Clone)]
pub enum RawContent {
    Mail(Box<RawMail>),
    /// Message class outside Part 04 §5.3 (contact, appointment, task, note).
    NonMail,
    /// libpff failed on this item, or it is the poison item of Part 02 §6.3.
    Unreadable { stage: ImportErrorStage, message: String },
}

/// Raw MAPI values; property ids in comments (Part 04 §5.4–§5.8). Strings come from libpff UTF-8 getters.
#[derive(Debug, Clone, Default)]
pub struct RawMail {
    pub subject: Option<String>,                         // 0x0037
    pub client_submit_time: Option<SystemTime>,          // 0x0039
    pub delivery_time: Option<SystemTime>,               // 0x0E06
    pub internet_message_id: Option<String>,             // 0x1035
    pub transport_headers: Option<String>,               // 0x007D
    pub sender_smtp_address: Option<String>,             // 0x5D01
    pub sent_representing_smtp_address: Option<String>,  // 0x5D02
    pub sender_email_address: Option<String>,            // 0x0C1F
    pub sender_address_type: Option<String>,             // 0x0C1E
    pub sender_name: Option<String>,                     // 0x0C1A
    pub sent_representing_name: Option<String>,          // 0x0042
    pub internet_codepage: Option<u32>,                  // 0x3FDE
    pub html_body: Option<Vec<u8>>,                      // raw bytes, no charset conversion
    pub rtf_body: Option<Vec<u8>>,                       // already LZFu-decompressed by libpff
    pub plain_body: Option<String>,                      // UTF-8
    pub recipients: Vec<RawRecipient>,                   // PST recipient-table order
    pub attachments: Vec<RawAttachment>,
}

#[derive(Debug, Clone, Default)]
pub struct RawRecipient {
    pub smtp_address: Option<String>,    // 0x39FE
    pub email_address: Option<String>,   // 0x3003
    pub address_type: Option<String>,    // 0x3002
    pub display_name: Option<String>,    // 0x3001
    pub recipient_type: Option<u32>,     // 0x0C15
}

#[derive(Debug, Clone, Default)]
pub struct RawAttachment {
    pub index: i32,                       // index for libpff_message_get_attachment()
    pub long_filename: Option<String>,    // 0x3707
    pub filename: Option<String>,         // 0x3704
    pub display_name: Option<String>,     // 0x3001
    pub mime_tag: Option<String>,         // 0x370E
    pub content_id: Option<String>,       // 0x3712
    pub attach_method: Option<i32>,       // 0x3705
    pub data_size: Option<i64>,           // libpff_attachment_get_data_size (by-value data only)
    pub attach_size: Option<i64>,         // 0x0E20
    pub embedded_subject: Option<String>, // subject of the embedded message (method 5)
    pub data: Option<Vec<u8>>,            // read by the reader only when content_id is Some
}

impl RawItem {
    /// Bytes held in memory (HTML + RTF + plain body + attachment data). Feeds the 256 MB byte budget (Part 04 §5.1).
    pub fn approx_bytes(&self) -> u64;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DedupeKind { MessageId, ContentHash }
impl DedupeKind { pub fn as_db(self) -> &'static str; }          // "message_id" | "content_hash"

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodySource { Html, RtfHtml, Text, RtfText, None }
impl BodySource { pub fn as_db(self) -> &'static str; }          // "html" | "rtf_html" | "text" | "rtf_text" | "none"

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressType { Smtp, Ex, Other }
impl AddressType { pub fn as_db(self) -> &'static str; }         // "smtp" | "ex" | "other"

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressIn { pub email: String, pub address_type: AddressType }   // trimmed + lowercase, 1..=2048 chars

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecipientOut {
    pub ordinal: i32,                 // index in RawMail.recipients
    pub kind: RecipientKind,
    pub address: Option<AddressIn>,
    pub display_name: Option<String>,
    pub search_text: String,          // "Name <addr>" | "Name" | "addr"
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentOut {
    pub attachment_index: i32,
    pub file_name: String,            // 1..=1024 chars
    pub size_bytes: i64,              // >= 0
    pub mime_type: String,            // 1..=255 chars
    pub attach_method: i32,           // 0 when the property is missing
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedImage { pub webp: Vec<u8>, pub width: i32, pub height: i32, pub source_mime: String }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineImageOut {
    pub ordinal: i32,                 // matches src="outlooker-inline:<ordinal>"
    pub content_id: Option<String>,   // normalized cid; None for data: URI images
    pub source_hash: [u8; 32],        // BLAKE3-256 of the source bytes
    pub blob: Option<EncodedImage>,   // None = hash already known (blob committed in image_blobs)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemError {
    pub stage: ImportErrorStage,
    pub severity: ImportErrorSeverity,
    pub folder_path: Option<String>,
    pub pst_item_id: Option<i64>,     // Some whenever severity == ItemSkipped (DB CHECK)
    pub message: String,              // English, 1..=4000 chars
}
impl ItemError {
    /// folder_path / pst_item_id set; message cut to 4,000 chars, "unknown error" when empty.
    pub fn new(stage: ImportErrorStage, severity: ImportErrorSeverity, folder_path: &str, pst_item_id: i64, message: &str) -> ItemError;
}

#[derive(Debug, Clone)]
pub struct NormalizedMail {
    pub dedupe_key: [u8; 32],
    pub dedupe_kind: DedupeKind,
    pub message_id: Option<String>,   // Some iff dedupe_kind == MessageId (DB CHECK)
    pub subject: String,
    pub sent_at: Option<SystemTime>,
    pub received_at: Option<SystemTime>,
    pub sender: Option<AddressIn>,
    pub sender_name: Option<String>,
    pub sender_search: String,
    pub recipients: Vec<RecipientOut>,
    pub recipients_search: String,    // recipient search_text values, '\n'-joined, ordinal order
    pub to_preview: String,
    pub body_source: BodySource,
    pub body_html: Option<String>,    // None iff body_source in {Text, RtfText, None} (DB CHECK)
    pub body_text: String,
    pub snippet: String,              // <= 200 chars
    pub attachments: Vec<AttachmentOut>,
    pub inline_images: Vec<InlineImageOut>,
}

#[derive(Debug, Clone)]
pub enum Outcome { Mail(Box<NormalizedMail>), NonMail, Skipped }

/// What the writer receives for every seq. `errors` holds the degraded rows of a stored mail, or the single
/// item_skipped row of a Skipped item.
#[derive(Debug, Clone)]
pub struct NormalizedItem {
    pub seq: i64,
    pub pst_item_id: i64,
    pub pst_folder_id: i64,
    pub raw_bytes: u64,
    pub outcome: Outcome,
    pub errors: Vec<ItemError>,
}
```

  File `src/importer/webp.rs`:

```rust
pub struct ImageIn { pub bytes: Vec<u8> }

pub enum ImageOut {
    Encoded { source_hash: [u8; 32], image: EncodedImage },
    Known { source_hash: [u8; 32] },          // hash in KnownHashes: no decode, no encode
    Tiny,                                     // width <= 2 or height <= 2: src removed, no blob, no error row
    Rejected { message: String },             // unsupported, over limits, undecodable: src removed + degraded row
}

/// image_blobs.source_hash values committed in the DB. Loaded by the writer (R2-10) at job start, extended after
/// each commit; read by `convert` on rayon threads.
pub struct KnownHashes { /* std::sync::RwLock<std::collections::HashSet<[u8; 32]>> */ }
impl KnownHashes {
    pub fn new() -> Self;
    pub fn contains(&self, h: &[u8; 32]) -> bool;
    pub fn insert_all<I: IntoIterator<Item = [u8; 32]>>(&self, hashes: I);
    pub fn len(&self) -> usize;
}
impl Default for KnownHashes;

pub const MAX_SOURCE_BYTES: usize = 25 * 1024 * 1024;
pub const MAX_PIXELS: u64 = 40_000_000;
pub const MAX_SIDE: u32 = 2048;

pub fn source_hash(bytes: &[u8]) -> [u8; 32];
pub fn convert(input: &ImageIn, quality: u8, known: &KnownHashes) -> ImageOut;
```

  File `src/importer/dedupe.rs`:

```rust
pub struct DedupeKey { pub kind: DedupeKind, pub key: [u8; 32], pub message_id: Option<String> }

/// Part 04 §4.1. `raw_message_id` = 0x1035; `headers` = 0x007D (used when 0x1035 is absent or invalid).
/// `sender` = stored sender email, else lowercase trimmed sender name, else "".
pub fn key(raw_message_id: Option<&str>, headers: Option<&str>, sender: &str,
           sent: Option<std::time::SystemTime>, subject: &str, body_text: &str) -> DedupeKey;
pub fn normalize_message_id(raw: &str) -> Option<String>;
pub fn message_id_from_headers(headers: &str) -> Option<String>;
/// Whole seconds, UTC, "YYYY-MM-DDTHH:MM:SSZ" (proleptic Gregorian; times before 1970 supported).
pub fn format_utc_seconds(t: std::time::SystemTime) -> String;
```

  File `src/importer/normalize/mod.rs`:

```rust
pub struct NormalizeCtx { pub webp_quality: u8, pub known: std::sync::Arc<KnownHashes> }

/// RawItem → NormalizedItem. Never panics: a panic inside becomes Outcome::Skipped with one
/// ItemError { stage: Html, severity: ItemSkipped, message: "panic: <payload>" }.
pub fn normalize(raw: RawItem, ctx: &NormalizeCtx) -> NormalizedItem;

pub const MAX_HTML_BYTES: usize = 16 * 1024 * 1024;
pub const SNIPPET_CHARS: usize = 200;
```

**Behavior:**
- Dispatch: `RawContent::NonMail` → `Outcome::NonMail`, no errors. `Unreadable{stage, message}` → `Outcome::Skipped` + one `ItemError{stage, ItemSkipped, folder_path: Some(..), pst_item_id: Some(..), message}`. `Mail` → the steps below.
- Body selection (Part 04 §5.5), first hit wins: (1) `html_body` non-empty → `charset::decode_html(bytes, internet_codepage)`; decoded length > `MAX_HTML_BYTES` → `ItemError{Html, Degraded, "html body larger than 16 MB, not sanitized"}` and go on with rule 3; else sanitize → `Html`. (2) `rtf_body` whose first 1,024 bytes contain `\fromhtml1` → `rtf::deencapsulate_html` → sanitize → `RtfHtml`. (3) `plain_body` non-empty after trim → `Text`. (4) `rtf_body` present → `rtf::to_text` → `RtfText`. (5) `None`.
- HTML charset (Part 04 §5.5): BOM → `internet_codepage` through the fixed table → `<meta charset>` / `http-equiv` charset in the first 1,024 bytes → chardetng over the whole body (`Iso2022JpDetection::Allow`, `Utf8Detection::Allow`, tld `None`) → windows-1252. Bytes after the BOM decode with `Encoding::decode_without_bom_handling`; malformed bytes become U+FFFD; never fails.
- RTF tokenizer (MS-OXRTFEX, in-house): groups, control words `\word[-N]` (one space delimiter consumed), control symbols `\'hh`, `\\`, `\{`, `\}`, `\~` (U+00A0), `\uN` (signed 16-bit; skip `\ucN` following fallback characters, default 1), `\ansicpg N` (codepage table of the charset rule; default windows-1252). Byte runs from `\'hh` are buffered and decoded together, so Big5 / GBK pairs decode correctly. `deencapsulate_html`: emit the text of `{\*\htmltag N …}` groups and text outside `\htmlrtf … \htmlrtf0`; outside htmltag groups `\par` / `\line` → `\n`, `\tab` → `\t`; other `{\* …}` groups skipped. `to_text`: text plus `\par` / `\line` → `\n`, `\tab` → `\t`; groups `fonttbl colortbl stylesheet info pict object header footer` and every `{\* …}` group skipped.
- Sanitize (Part 04 §5.6; exact lists in Stage 2): one `ammonia::Builder::empty()` per message with `tags`, `clean_content_tags`, `generic_attributes`, `tag_attributes`, `url_schemes = {http, https, mailto, cid, data, outlooker-inline}`, `url_relative(Deny)`, `link_rel(Some("noopener noreferrer"))`, `strip_comments(true)`, `filter_style_properties`, `attribute_filter`. `cid` and `data` must be in `url_schemes`: ammonia drops URL attributes with other schemes before `attribute_filter` runs (ammonia 4.2.4 `src/lib.rs:2081` in `clean_child`, filter at `:2428` in `adjust_node_attributes`, and no scheme re-check after the filter). The filter is the final authority: `a href` survives only for `http:` / `https:` / `mailto:`; `img src` survives only as `outlooker-inline:<n>`. Ordinals are assigned in order of first reference: a `cid:` found in the cid map (normalized: strip `<` `>`, trim, ASCII-lowercase), or a `data:image/(png|jpeg|gif|bmp|webp);base64,` URI. The same cid twice keeps one ordinal. After sanitize every referenced image goes through `webp::convert` (`rayon` `par_iter`); for `Tiny` / `Rejected` the exact text ` src="outlooker-inline:<n>"` is removed from the HTML; `Rejected` adds `ItemError{Image, Degraded, message}`.
- Attachments (`attachments.rs`, Part 04 §5.8): an attachment whose normalized content id is referenced by a `cid:` in the HTML is inline and is not listed. Others: `file_name` = long → short → display → `attachment-<index>`; method 5 → `<embedded_subject or "message">.msg`; cut to 1,024 chars. `size_bytes` = `data_size` else `attach_size` else 0 (negative → 0). `mime_type` = trimmed `mime_tag` (≤ 255) → extension table → `application/octet-stream`; method 5 → `application/vnd.ms-outlook`. `attach_method` = value or 0. `has_attachments` (writer) = `!attachments.is_empty()`.
- Text (`text.rs`): `body_text` = plain body if non-empty; else `html2text::config::plain_no_decorate().raw_mode(true).string_from_read(sanitized.as_bytes(), 1_000_000)` (`Err` → `""` + `ItemError{Html, Degraded}`); else the `rtf_text` output; else `""`. Then CRLF / CR → LF, runs of space/tab → one space, 3+ LF → 2 LF, trim. `snippet` = first 200 chars after every whitespace run → one space.
- Participants (`participants.rs`, Part 04 §5.4): sender address = first non-empty of 0x5D01 (smtp), 0x5D02 (smtp), 0x0C1F (type 0x0C1E); trim + `to_lowercase`; empty or > 2,048 chars → `None`. `address_type`: `Smtp` from 0x5D01 / 0x5D02 / 0x39FE or type `SMTP` (ASCII case-insensitive); `Ex` for type `EX`; else `Other`. `sender_name` = 0x0C1A → 0x0042 (trimmed, empty → None). Recipient address: 0x39FE (smtp) → 0x3003 (type 0x3002). Kind: 1 To, 2 Cc, 3 Bcc, other / missing To. A recipient with neither address nor name is dropped (DB CHECK); ordinals keep their PST index. `search_text` = `Name <addr>` / `Name` / `addr`. `to_preview` = first 3 `To` names (address if no name), `, `-joined.
- Dedupe (Part 04 §4.1): id from 0x1035 else the `Message-ID:` header of 0x007D (name case-insensitive, continuation lines unfolded); normalized with the five rules; `BLAKE3("M\0" + id)`. Fallback `BLAKE3("H\0" + sender + U+001F + sent + U+001F + subject + U+001F + body)`: subject whitespace runs → one space + trim; body = `body_text` with every whitespace char and U+200B, U+200C, U+200D, U+2060, U+FEFF deleted.
- WebP (Part 04 §5.7): `source_hash` first; known → `Known`. Source > 25 MB → `Rejected`. `image::guess_format`; not PNG / JPEG / GIF / BMP → `Rejected`. `ImageReader::into_dimensions` before decoding; `w * h > 40,000,000` → `Rejected`; `w <= 2 || h <= 2` → `Tiny`. Decode with `Limits.max_alloc = 40_000_000 * 4 + 1 MiB`; error → `Rejected`. Longest side > 2,048 → `DynamicImage::resize(2048, 2048, FilterType::Lanczos3)` (keeps aspect ratio). `to_rgba8()` → `webp::Encoder::from_rgba(&rgba, w, h).encode(quality as f32)` (quality clamped to 1..=100) → `Encoded`.

**Constraints:**
- Pure Rust, no DB, no file I/O except tests reading `tests/golden/`. These modules import only `crate::dto` and the crates above.
- Start from `Builder::empty()`; never `Builder::new()` / `default()` (they enable extra tags and schemes).
- `normalize` runs on rayon threads: `NormalizeCtx: Sync`, no thread-local state.
- Every value respects the R1-04 CHECK constraints (snippet ≤ 200 chars; lowercase emails; the `body_html` NULL rule; 32-byte keys; `file_name` 1..=1024; `mime_type` 1..=255; `ItemError.message` 1..=4000 chars, cut at a char boundary).
- Layer: core domain; depends on `dto` only.

**Acceptance criteria:**
- [ ] `cargo test -p outlooker-core importer::` exits 0 and the output lists as `ok`: `importer::normalize::rtf::tests::fromhtml_golden`, `importer::normalize::charset::tests::big5_and_gbk`, `importer::normalize::sanitize::tests::hostile_corpus_is_clean`, `importer::normalize::sanitize::tests::golden_plain`, `importer::normalize::sanitize::tests::golden_inline`, `importer::normalize::tests::one_pixel_image_loses_src`, `importer::webp::tests::known_hash_skips_encode`, `importer::webp::tests::resize_to_2048`, `importer::webp::tests::limits_rejected`, `importer::dedupe::tests::key_table`, `importer::normalize::tests::invariants_over_goldens`.
- [ ] RTF de-encapsulation of `tests/golden/fromhtml.rtf` equals `tests/golden/fromhtml.expected.html` byte for byte.
- [ ] Big5 bytes `B3 F8 BB F9 B3 E6` decode to `報價單`; GBK bytes `B1 A8 BC DB B5 A5` decode to `报价单`.
- [ ] Hostile corpus `tests/golden/hostile.html` (`<script>`, `onerror=`, `javascript:`, CSS `url(`, `<iframe`, `<form`, `<base`, `<meta http-equiv`, `position:fixed`, `<img src="https://…">`): the sanitized output matches none of the patterns `<script`, `on[a-z]+=`, `javascript:`, `url(`, `<iframe`, `<form`, `position`, `src="http` (checked by the test).
- [ ] Golden comparison for the plain and inline samples (sanitized HTML, trimmed).
- [ ] 1×1 pixel image → `<img>` without `src`; the same image bytes give one BLAKE3 hash and the second occurrence (hash in `KnownHashes`) returns `Known` without encoding; a 3000×1000 image becomes 2048×683; a 7000×6000 image (42 MP) and a 26 MB source are `Rejected`.
- [ ] Dedupe key table: `<abc@x>` and ` abc@x ` equal; a Message-ID with an inner space is treated as absent; CRLF vs LF body equal; whitespace-run differences in subject equal; one changed character differs; "M" and "H" keys never equal.
- [ ] Invariants over every golden sample: `snippet` ≤ 200 chars; every stored email equals its lowercase form; `body_html` is `None` exactly for `Text` / `RtfText` / `None`; dedupe keys are 32 bytes; `message_id.is_some() == (dedupe_kind == MessageId)`.
- [ ] `grep -rnE "Builder::(new|default)" src-tauri/crates/outlooker-core/src/importer` finds nothing.

### R1-05 — Stage 2 steps

All code below was compiled and its tests were run (25 tests, all `ok`) against the exact Part 01 crate versions in a scratch crate before this plan was written. Module paths are the production ones.

- [ ] **Step 1: Declare the modules in `src-tauri/crates/outlooker-core/src/importer/mod.rs`**

Keep the six kickoff `mod` lines; the file must contain exactly these lines:

```rust
pub mod types;
pub mod normalize;
pub mod webp;
pub mod dedupe;
pub mod queue;
pub mod pst_reader;
pub mod inflight;
pub mod progress;
pub mod writer;
pub mod remove;
```


- [ ] **Step 2: Create `src-tauri/crates/outlooker-core/src/importer/types.rs` (C9, no tests: data only)**

```rust
//! C9 data types shared by the reader (R2-09), normalize (R1-05) and the writer (R2-10).
use std::time::SystemTime;

use crate::dto::import::{ImportErrorSeverity, ImportErrorStage};
use crate::dto::search::RecipientKind;

/// One PST folder found by the scan (Part 04 §5.2). Sent once per job before any item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderIn {
    pub pst_folder_id: i64,
    pub parent_pst_folder_id: Option<i64>,
    pub name: String,
    pub path: String,
}

/// One PST item as the reader hands it to normalize. `seq` starts at 1 (Part 04 §5.1).
#[derive(Debug, Clone)]
pub struct RawItem {
    pub seq: i64,
    pub pst_item_id: i64,
    pub pst_folder_id: i64,
    pub folder_path: String,
    pub content: RawContent,
}

#[derive(Debug, Clone)]
pub enum RawContent {
    Mail(Box<RawMail>),
    NonMail,
    Unreadable { stage: ImportErrorStage, message: String },
}

#[derive(Debug, Clone, Default)]
pub struct RawMail {
    pub subject: Option<String>,
    pub client_submit_time: Option<SystemTime>,
    pub delivery_time: Option<SystemTime>,
    pub internet_message_id: Option<String>,
    pub transport_headers: Option<String>,
    pub sender_smtp_address: Option<String>,
    pub sent_representing_smtp_address: Option<String>,
    pub sender_email_address: Option<String>,
    pub sender_address_type: Option<String>,
    pub sender_name: Option<String>,
    pub sent_representing_name: Option<String>,
    pub internet_codepage: Option<u32>,
    pub html_body: Option<Vec<u8>>,
    pub rtf_body: Option<Vec<u8>>,
    pub plain_body: Option<String>,
    pub recipients: Vec<RawRecipient>,
    pub attachments: Vec<RawAttachment>,
}

#[derive(Debug, Clone, Default)]
pub struct RawRecipient {
    pub smtp_address: Option<String>,
    pub email_address: Option<String>,
    pub address_type: Option<String>,
    pub display_name: Option<String>,
    pub recipient_type: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct RawAttachment {
    pub index: i32,
    pub long_filename: Option<String>,
    pub filename: Option<String>,
    pub display_name: Option<String>,
    pub mime_tag: Option<String>,
    pub content_id: Option<String>,
    pub attach_method: Option<i32>,
    pub data_size: Option<i64>,
    pub attach_size: Option<i64>,
    pub embedded_subject: Option<String>,
    pub data: Option<Vec<u8>>,
}

impl RawItem {
    pub fn approx_bytes(&self) -> u64 {
        match &self.content {
            RawContent::Mail(m) => {
                let bodies = m.html_body.as_ref().map_or(0, Vec::len)
                    + m.rtf_body.as_ref().map_or(0, Vec::len)
                    + m.plain_body.as_ref().map_or(0, String::len);
                let data: usize = m.attachments.iter().map(|a| a.data.as_ref().map_or(0, Vec::len)).sum();
                (bodies + data) as u64
            }
            _ => 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DedupeKind { MessageId, ContentHash }
impl DedupeKind {
    pub fn as_db(self) -> &'static str {
        match self { DedupeKind::MessageId => "message_id", DedupeKind::ContentHash => "content_hash" }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodySource { Html, RtfHtml, Text, RtfText, None }
impl BodySource {
    pub fn as_db(self) -> &'static str {
        match self {
            BodySource::Html => "html",
            BodySource::RtfHtml => "rtf_html",
            BodySource::Text => "text",
            BodySource::RtfText => "rtf_text",
            BodySource::None => "none",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressType { Smtp, Ex, Other }
impl AddressType {
    pub fn as_db(self) -> &'static str {
        match self { AddressType::Smtp => "smtp", AddressType::Ex => "ex", AddressType::Other => "other" }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressIn { pub email: String, pub address_type: AddressType }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecipientOut {
    pub ordinal: i32,
    pub kind: RecipientKind,
    pub address: Option<AddressIn>,
    pub display_name: Option<String>,
    pub search_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentOut {
    pub attachment_index: i32,
    pub file_name: String,
    pub size_bytes: i64,
    pub mime_type: String,
    pub attach_method: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedImage { pub webp: Vec<u8>, pub width: i32, pub height: i32, pub source_mime: String }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineImageOut {
    pub ordinal: i32,
    pub content_id: Option<String>,
    pub source_hash: [u8; 32],
    pub blob: Option<EncodedImage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemError {
    pub stage: ImportErrorStage,
    pub severity: ImportErrorSeverity,
    pub folder_path: Option<String>,
    pub pst_item_id: Option<i64>,
    pub message: String,
}

impl ItemError {
    /// Builds an error row; `message` is cut to 4,000 chars and never empty (DB CHECK).
    pub fn new(stage: ImportErrorStage, severity: ImportErrorSeverity, folder_path: &str, pst_item_id: i64, message: &str) -> ItemError {
        let mut m: String = message.chars().take(4000).collect();
        if m.is_empty() {
            m.push_str("unknown error");
        }
        ItemError { stage, severity, folder_path: Some(folder_path.to_string()), pst_item_id: Some(pst_item_id), message: m }
    }
}

#[derive(Debug, Clone)]
pub struct NormalizedMail {
    pub dedupe_key: [u8; 32],
    pub dedupe_kind: DedupeKind,
    pub message_id: Option<String>,
    pub subject: String,
    pub sent_at: Option<SystemTime>,
    pub received_at: Option<SystemTime>,
    pub sender: Option<AddressIn>,
    pub sender_name: Option<String>,
    pub sender_search: String,
    pub recipients: Vec<RecipientOut>,
    pub recipients_search: String,
    pub to_preview: String,
    pub body_source: BodySource,
    pub body_html: Option<String>,
    pub body_text: String,
    pub snippet: String,
    pub attachments: Vec<AttachmentOut>,
    pub inline_images: Vec<InlineImageOut>,
}

#[derive(Debug, Clone)]
pub enum Outcome { Mail(Box<NormalizedMail>), NonMail, Skipped }

#[derive(Debug, Clone)]
pub struct NormalizedItem {
    pub seq: i64,
    pub pst_item_id: i64,
    pub pst_folder_id: i64,
    pub raw_bytes: u64,
    pub outcome: Outcome,
    pub errors: Vec<ItemError>,
}
```


- [ ] **Step 3: Check that it compiles**

Run: `cd src-tauri && cargo build -p outlooker-core`
Expected: `Finished` (the other kickoff modules are still empty files).

- [ ] **Step 4: Write the failing tests in `src-tauri/crates/outlooker-core/src/importer/dedupe.rs`**

Create the file with only this test module (the code under test does not exist yet):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn key_table() {
        let a = key(Some("<abc@x>"), None, "", None, "", "");
        let b = key(Some(" abc@x "), None, "", None, "", "");
        assert_eq!(a.key, b.key);
        assert_eq!(a.kind, DedupeKind::MessageId);
        assert_eq!(a.message_id.as_deref(), Some("abc@x"));

        let inner_space = key(Some("<a b@x>"), None, "s", None, "subj", "body");
        assert_eq!(inner_space.kind, DedupeKind::ContentHash);
        assert_eq!(inner_space.message_id, None);

        let t = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let crlf = key(None, None, "A@X", Some(t), "Hello  world", "line 1\r\nline 2\r\n");
        let lf = key(None, None, " a@x ", Some(t), "Hello world ", "line 1\nline 2\n");
        assert_eq!(crlf.key, lf.key);
        let changed = key(None, None, "a@x", Some(t), "Hello world", "line 1\nline 3\n");
        assert_ne!(lf.key, changed.key);

        let m = key(Some("x"), None, "", None, "", "");
        let h = key(None, None, "", None, "", "x");
        assert_ne!(m.key, h.key);
        assert_eq!(m.key.len(), 32);
    }

    #[test]
    fn header_message_id() {
        let h = "Subject: hi\r\nMESSAGE-ID:\r\n <id-1@example.com>\r\nTo: a@b\r\n";
        assert_eq!(message_id_from_headers(h).as_deref(), Some("id-1@example.com"));
        let k = key(None, Some(h), "", None, "", "");
        assert_eq!(k.kind, DedupeKind::MessageId);
    }

    #[test]
    fn utc_format() {
        assert_eq!(format_utc_seconds(UNIX_EPOCH), "1970-01-01T00:00:00Z");
        assert_eq!(format_utc_seconds(UNIX_EPOCH + Duration::from_millis(1_700_000_000_900)), "2023-11-14T22:13:20Z");
        assert_eq!(format_utc_seconds(UNIX_EPOCH - Duration::from_secs(1)), "1969-12-31T23:59:59Z");
        assert_eq!(format_utc_seconds(UNIX_EPOCH + Duration::from_secs(951_782_400)), "2000-02-29T00:00:00Z");
    }
}
```


- [ ] **Step 5: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test -p outlooker-core --lib importer::dedupe`
Expected: compile error (cannot find function `key` / `format_utc_seconds`).

- [ ] **Step 6: Implement `src-tauri/crates/outlooker-core/src/importer/dedupe.rs`**

Insert this code above the test module (the whole file is this code followed by the tests of the previous step):

```rust
//! Part 04 §4.1 dedupe key.
use std::time::{SystemTime, UNIX_EPOCH};

use crate::importer::types::DedupeKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DedupeKey { pub kind: DedupeKind, pub key: [u8; 32], pub message_id: Option<String> }

pub fn key(
    raw_message_id: Option<&str>,
    headers: Option<&str>,
    sender: &str,
    sent: Option<SystemTime>,
    subject: &str,
    body_text: &str,
) -> DedupeKey {
    let id = raw_message_id
        .and_then(normalize_message_id)
        .or_else(|| headers.and_then(message_id_from_headers));
    if let Some(id) = id {
        let mut h = blake3::Hasher::new();
        h.update(b"M\0");
        h.update(id.as_bytes());
        return DedupeKey { kind: DedupeKind::MessageId, key: *h.finalize().as_bytes(), message_id: Some(id) };
    }
    let sender_n = sender.trim().to_lowercase();
    let sent_n = sent.map(format_utc_seconds).unwrap_or_default();
    let subject_n = subject.split_whitespace().collect::<Vec<_>>().join(" ");
    let body_n: String = body_text
        .chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' | '\u{FEFF}'))
        .collect();
    let mut h = blake3::Hasher::new();
    h.update(b"H\0");
    h.update(sender_n.as_bytes());
    h.update(b"\x1F");
    h.update(sent_n.as_bytes());
    h.update(b"\x1F");
    h.update(subject_n.as_bytes());
    h.update(b"\x1F");
    h.update(body_n.as_bytes());
    DedupeKey { kind: DedupeKind::ContentHash, key: *h.finalize().as_bytes(), message_id: None }
}

/// Part 04 §4.1 rules 1–5.
pub fn normalize_message_id(raw: &str) -> Option<String> {
    let mut s = raw.trim();
    if s.len() >= 2 && s.starts_with('<') && s.ends_with('>') {
        s = &s[1..s.len() - 1];
    }
    let s = s.trim();
    let n = s.chars().count();
    if n == 0 || n > 998 || s.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    Some(s.to_string())
}

/// `Message-ID:` header of PidTagTransportMessageHeaders; names case-insensitive, folded lines unfolded.
pub fn message_id_from_headers(headers: &str) -> Option<String> {
    let mut unfolded: Vec<String> = Vec::new();
    for line in headers.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if (line.starts_with(' ') || line.starts_with('\t'))
            && let Some(last) = unfolded.last_mut()
        {
            last.push(' ');
            last.push_str(line.trim_start());
            continue;
        }
        unfolded.push(line.to_string());
    }
    unfolded.iter().find_map(|l| {
        let (name, value) = l.split_once(':')?;
        if name.trim().eq_ignore_ascii_case("message-id") { normalize_message_id(value) } else { None }
    })
}

/// Whole seconds, UTC, `YYYY-MM-DDTHH:MM:SSZ` (proleptic Gregorian, works before 1970).
pub fn format_utc_seconds(t: SystemTime) -> String {
    let secs: i64 = match t.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => {
            let d = e.duration();
            -(d.as_secs() as i64) - if d.subsec_nanos() > 0 { 1 } else { 0 }
        }
    };
    let days = secs.div_euclid(86_400);
    let sod = secs.rem_euclid(86_400);
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, m, d, sod / 3600, sod % 3600 / 60, sod % 60)
}
```


- [ ] **Step 7: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test -p outlooker-core --lib importer::dedupe`
Expected: every test of the module prints `ok`; `test result: ok`.

- [ ] **Step 8: Write the failing tests in `src-tauri/crates/outlooker-core/src/importer/webp.rs`**

Create the file with only this test module (the code under test does not exist yet):

```rust
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};

    pub(crate) fn png(w: u32, h: u32) -> Vec<u8> {
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_fn(w, h, |x, y| Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255]));
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, ImageFormat::Png).unwrap();
        out.into_inner()
    }

    /// 54-byte BMP header (24-bit, uncompressed) with no pixel data.
    fn bmp_header(w: u32, h: u32) -> Vec<u8> {
        let mut b = Vec::with_capacity(54);
        b.extend_from_slice(b"BM");
        b.extend_from_slice(&(54 + w * h * 3).to_le_bytes());
        b.extend_from_slice(&[0, 0, 0, 0]);
        b.extend_from_slice(&54u32.to_le_bytes());
        b.extend_from_slice(&40u32.to_le_bytes());
        b.extend_from_slice(&(w as i32).to_le_bytes());
        b.extend_from_slice(&(h as i32).to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&24u16.to_le_bytes());
        b.extend_from_slice(&[0u8; 24]);
        b
    }

    #[test]
    fn known_hash_skips_encode() {
        let bytes = png(16, 16);
        let known = KnownHashes::new();
        let first = convert(&ImageIn { bytes: bytes.clone() }, 80, &known);
        let ImageOut::Encoded { source_hash, image } = first else { panic!("expected Encoded") };
        assert_eq!(source_hash, super::source_hash(&bytes));
        assert_eq!((image.width, image.height), (16, 16));
        assert_eq!(&image.webp[0..4], b"RIFF");
        known.insert_all([source_hash]);
        assert_eq!(convert(&ImageIn { bytes }, 80, &known), ImageOut::Known { source_hash });
    }

    #[test]
    fn tiny_image() {
        assert_eq!(convert(&ImageIn { bytes: png(1, 1) }, 80, &KnownHashes::new()), ImageOut::Tiny);
    }

    #[test]
    fn resize_to_2048() {
        let out = convert(&ImageIn { bytes: png(3000, 1000) }, 80, &KnownHashes::new());
        let ImageOut::Encoded { image, .. } = out else { panic!("expected Encoded") };
        assert_eq!((image.width, image.height), (2048, 683));
    }

    #[test]
    fn limits_rejected() {
        // 7000x6000 = 42 MP: the check runs on the header only, so a BMP header (no checksum) is enough.
        let out = convert(&ImageIn { bytes: bmp_header(7000, 6000) }, 80, &KnownHashes::new());
        let ImageOut::Rejected { message } = out else { panic!("expected Rejected") };
        assert!(message.contains("40 megapixels"), "{message}");
        let mut big = png(4, 4);
        big.resize(26 * 1024 * 1024, 0);
        assert!(matches!(convert(&ImageIn { bytes: big }, 80, &KnownHashes::new()), ImageOut::Rejected { .. }));
        let tiff = b"II*\0garbage".to_vec();
        assert!(matches!(convert(&ImageIn { bytes: tiff }, 80, &KnownHashes::new()), ImageOut::Rejected { .. }));
    }
}
```


- [ ] **Step 9: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test -p outlooker-core --lib importer::webp`
Expected: compile error (cannot find `KnownHashes` / `convert`).

- [ ] **Step 10: Implement `src-tauri/crates/outlooker-core/src/importer/webp.rs`**

Insert this code above the test module (the whole file is this code followed by the tests of the previous step):

```rust
//! Part 04 §5.7 inline image → WebP.
use std::collections::HashSet;
use std::io::Cursor;
use std::sync::RwLock;

use image::{imageops::FilterType, ImageFormat, ImageReader, Limits};

use crate::importer::types::EncodedImage;

pub struct ImageIn { pub bytes: Vec<u8> }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageOut {
    Encoded { source_hash: [u8; 32], image: EncodedImage },
    Known { source_hash: [u8; 32] },
    Tiny,
    Rejected { message: String },
}

#[derive(Default)]
pub struct KnownHashes { inner: RwLock<HashSet<[u8; 32]>> }

impl KnownHashes {
    pub fn new() -> KnownHashes { KnownHashes::default() }
    pub fn contains(&self, h: &[u8; 32]) -> bool { self.inner.read().unwrap().contains(h) }
    pub fn insert_all<I: IntoIterator<Item = [u8; 32]>>(&self, hashes: I) { self.inner.write().unwrap().extend(hashes) }
    pub fn len(&self) -> usize { self.inner.read().unwrap().len() }
    pub fn is_empty(&self) -> bool { self.len() == 0 }
}

pub const MAX_SOURCE_BYTES: usize = 25 * 1024 * 1024;
pub const MAX_PIXELS: u64 = 40_000_000;
pub const MAX_SIDE: u32 = 2048;

pub fn source_hash(bytes: &[u8]) -> [u8; 32] { *blake3::hash(bytes).as_bytes() }

pub fn convert(input: &ImageIn, quality: u8, known: &KnownHashes) -> ImageOut {
    let source_hash = source_hash(&input.bytes);
    if known.contains(&source_hash) {
        return ImageOut::Known { source_hash };
    }
    if input.bytes.len() > MAX_SOURCE_BYTES {
        return ImageOut::Rejected { message: format!("image source larger than 25 MB ({} bytes)", input.bytes.len()) };
    }
    let format = match image::guess_format(&input.bytes) {
        Ok(f) => f,
        Err(e) => return ImageOut::Rejected { message: format!("unknown image format: {e}") },
    };
    let source_mime = match format {
        ImageFormat::Png => "image/png",
        ImageFormat::Jpeg => "image/jpeg",
        ImageFormat::Gif => "image/gif",
        ImageFormat::Bmp => "image/bmp",
        other => return ImageOut::Rejected { message: format!("unsupported image format {other:?}") },
    };
    let reader = || {
        let mut r = ImageReader::new(Cursor::new(&input.bytes[..]));
        r.set_format(format);
        let mut limits = Limits::default();
        limits.max_alloc = Some(MAX_PIXELS * 4 + 1024 * 1024);
        r.limits(limits);
        r
    };
    let (w, h) = match reader().into_dimensions() {
        Ok(d) => d,
        Err(e) => return ImageOut::Rejected { message: format!("image header unreadable: {e}") },
    };
    if u64::from(w) * u64::from(h) > MAX_PIXELS {
        return ImageOut::Rejected { message: format!("image larger than 40 megapixels ({w}x{h})") };
    }
    if w <= 2 || h <= 2 {
        return ImageOut::Tiny;
    }
    let mut img = match reader().decode() {
        Ok(i) => i,
        Err(e) => return ImageOut::Rejected { message: format!("image decode failed: {e}") },
    };
    if w.max(h) > MAX_SIDE {
        img = img.resize(MAX_SIDE, MAX_SIDE, FilterType::Lanczos3);
    }
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let q = f32::from(quality.clamp(1, 100));
    let webp = webp::Encoder::from_rgba(rgba.as_raw(), w, h).encode(q).to_vec();
    ImageOut::Encoded {
        source_hash,
        image: EncodedImage { webp, width: w as i32, height: h as i32, source_mime: source_mime.to_string() },
    }
}
```


- [ ] **Step 11: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test -p outlooker-core --lib importer::webp`
Expected: every test of the module prints `ok`; `test result: ok`.

- [ ] **Step 12: Create `src-tauri/crates/outlooker-core/src/importer/normalize/mod.rs` with the submodule list only**

Temporary content until the last task step replaces it:

```rust
pub mod attachments;
pub mod charset;
pub mod participants;
pub mod rtf;
pub mod sanitize;
pub mod text;

pub const MAX_HTML_BYTES: usize = 16 * 1024 * 1024;
pub const SNIPPET_CHARS: usize = 200;
```

Create the six submodule files empty.

- [ ] **Step 13: Write the failing tests in `src-tauri/crates/outlooker-core/src/importer/normalize/charset.rs`**

Create the file with only this test module (the code under test does not exist yet):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn big5_and_gbk() {
        assert_eq!(decode_html(&[0xB3, 0xF8, 0xBB, 0xF9, 0xB3, 0xE6], Some(950)), "報價單");
        assert_eq!(decode_html(&[0xB1, 0xA8, 0xBC, 0xDB, 0xB5, 0xA5], Some(936)), "报价单");
    }

    #[test]
    fn meta_and_bom_rules() {
        let mut html = b"<html><head><meta http-equiv=\"Content-Type\" content=\"text/html; charset=big5\"></head><body>".to_vec();
        html.extend_from_slice(&[0xB3, 0xF8, 0xBB, 0xF9]);
        assert!(decode_html(&html, None).ends_with("報價"));
        assert!(decode_html(&html, Some(12345)).ends_with("報價"), "unknown codepage falls through to meta");
        let bom = [0xEF, 0xBB, 0xBF, b'h', b'i'];
        assert_eq!(decode_html(&bom, Some(950)), "hi");
        assert_eq!(decode_html(&[b'a', 0xFF, b'b'], Some(65001)), "a\u{FFFD}b");
    }
}
```


- [ ] **Step 14: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test -p outlooker-core --lib importer::normalize::charset`
Expected: compile error (cannot find function `decode_html`).

- [ ] **Step 15: Implement `src-tauri/crates/outlooker-core/src/importer/normalize/charset.rs`**

Insert this code above the test module (the whole file is this code followed by the tests of the previous step):

```rust
//! Part 04 §5.5 HTML charset decode.
use encoding_rs::Encoding;

/// Windows code page number → encoding (Part 04 §5.5 rule 2). None = not in the table.
pub fn for_codepage(cp: u32) -> Option<&'static Encoding> {
    use encoding_rs::*;
    Some(match cp {
        65001 => UTF_8,
        1200 => UTF_16LE,
        1201 => UTF_16BE,
        936 => GBK,
        950 => BIG5,
        932 => SHIFT_JIS,
        949 => EUC_KR,
        1250 => WINDOWS_1250,
        1251 => WINDOWS_1251,
        1252 | 20127 | 28591 => WINDOWS_1252,
        1253 => WINDOWS_1253,
        1254 => WINDOWS_1254,
        1255 => WINDOWS_1255,
        1256 => WINDOWS_1256,
        1257 => WINDOWS_1257,
        1258 => WINDOWS_1258,
        28592 => ISO_8859_2,
        28593 => ISO_8859_3,
        28594 => ISO_8859_4,
        28595 => ISO_8859_5,
        28596 => ISO_8859_6,
        28597 => ISO_8859_7,
        28598 => ISO_8859_8,
        28599 => WINDOWS_1254, // ISO-8859-9 is windows-1254 in the WHATWG table
        28600 => ISO_8859_10,
        28603 => ISO_8859_13,
        28604 => ISO_8859_14,
        28605 => ISO_8859_15,
        51932 => EUC_JP,
        50220 => ISO_2022_JP,
        20866 => KOI8_R,
        874 => WINDOWS_874,
        _ => return None,
    })
}

/// `<meta charset=…>` / `http-equiv … charset=…` in the first 1,024 bytes.
pub fn sniff_meta(bytes: &[u8]) -> Option<&'static Encoding> {
    let head = &bytes[..bytes.len().min(1024)];
    let lower: Vec<u8> = head.iter().map(u8::to_ascii_lowercase).collect();
    let mut from = 0;
    while let Some(pos) = find(&lower[from..], b"<meta") {
        let start = from + pos;
        let end = find(&lower[start..], b">").map_or(lower.len(), |e| start + e);
        let tag = &lower[start..end];
        if let Some(c) = find(tag, b"charset") {
            let mut i = c + b"charset".len();
            while i < tag.len() && (tag[i] == b' ' || tag[i] == b'=') { i += 1; }
            while i < tag.len() && (tag[i] == b'"' || tag[i] == b'\'') { i += 1; }
            let label_start = i;
            while i < tag.len() && (tag[i].is_ascii_alphanumeric() || b"-_:.".contains(&tag[i])) { i += 1; }
            if let Some(enc) = Encoding::for_label(&tag[label_start..i]) {
                return Some(enc);
            }
        }
        from = end;
    }
    None
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Bytes → UTF-8 text. Never fails; invalid sequences become U+FFFD.
pub fn decode_html(bytes: &[u8], internet_codepage: Option<u32>) -> String {
    if let Some((enc, bom_len)) = Encoding::for_bom(bytes) {
        return enc.decode_without_bom_handling(&bytes[bom_len..]).0.into_owned();
    }
    let enc = internet_codepage
        .and_then(for_codepage)
        .or_else(|| sniff_meta(bytes))
        .unwrap_or_else(|| {
            let mut det = chardetng::EncodingDetector::new(chardetng::Iso2022JpDetection::Allow);
            det.feed(bytes, true);
            det.guess(None, chardetng::Utf8Detection::Allow)
        });
    enc.decode_without_bom_handling(bytes).0.into_owned()
}
```


- [ ] **Step 16: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test -p outlooker-core --lib importer::normalize::charset`
Expected: every test of the module prints `ok`; `test result: ok`.

- [ ] **Step 17: Create `src-tauri/crates/outlooker-core/tests/golden/fromhtml.rtf`**

```rtf
{\rtf1\ansi\ansicpg1252\fromhtml1 \deff0{\fonttbl
{\f0\fswiss Arial;}
{\f1\fmodern Courier New;}}
{\colortbl\red0\green0\blue0;}
{\*\htmltag19 <html>}
{\*\htmltag34 <head>}
{\*\htmltag161 <meta charset="windows-1252">}
{\*\htmltag41 </head>}
{\*\htmltag50 <body>}\htmlrtf\viewkind5\htmlrtf0
{\*\htmltag64 <p>}\htmlrtf {\htmlrtf0 Price: 100\'80 caf\'e9\htmlrtf\par}\htmlrtf0
{\*\htmltag72 </p>}
{\*\htmltag64 <p style="color:red">}\htmlrtf {\htmlrtf0 Line one\htmlrtf\line}\htmlrtf0
{\*\htmltag116 <br>}
\htmlrtf {\htmlrtf0 Line \{two\}\htmlrtf\par}\htmlrtf0
{\*\htmltag72 </p>}
{\*\htmltag58 </body>}
{\*\htmltag27 </html>}}
```


- [ ] **Step 18: Create `src-tauri/crates/outlooker-core/tests/golden/fromhtml.expected.html`**

```html
<html><head><meta charset="windows-1252"></head><body><p>Price: 100€ café</p><p style="color:red">Line one<br>Line {two}</p></body></html>
```


- [ ] **Step 19: Write the failing tests in `src-tauri/crates/outlooker-core/src/importer/normalize/rtf.rs`**

Create the file with only this test module (the code under test does not exist yet):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn golden(name: &str) -> Vec<u8> {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(name);
        std::fs::read(&p).unwrap_or_else(|e| panic!("missing golden file {}: {e}", p.display()))
    }

    #[test]
    fn fromhtml_golden() {
        let rtf = golden("fromhtml.rtf");
        assert!(is_html_encapsulated(&rtf));
        let expected = String::from_utf8(golden("fromhtml.expected.html")).unwrap();
        assert_eq!(deencapsulate_html(&rtf), expected.trim_end_matches('\n'));
    }

    #[test]
    fn plain_rtf_text() {
        let rtf = br"{\rtf1\ansi\ansicpg950\deff0{\fonttbl{\f0 PMingLiU;}}\f0 Hello\par \'b3\'f8\'bb\'f9\tab x\line y}";
        assert!(!is_html_encapsulated(rtf));
        assert_eq!(to_text(rtf), "Hello\n報價\tx\ny");
    }

    #[test]
    fn unicode_fallback_is_skipped() {
        // The control word is written as "\" + "u20729" so no editor turns it into a character.
        let rtf = concat!(r"{\rtf1\ansi\uc2 a\", "u20729", r"\'3f\'3fb\uc1\", "u21934", "?c}");
        assert_eq!(to_text(rtf.as_bytes()), "a\u{50F9}b\u{55AE}c");
    }
}
```


- [ ] **Step 20: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test -p outlooker-core --lib importer::normalize::rtf`
Expected: compile error (cannot find function `deencapsulate_html`).

- [ ] **Step 21: Implement `src-tauri/crates/outlooker-core/src/importer/normalize/rtf.rs`**

Insert this code above the test module (the whole file is this code followed by the tests of the previous step):

```rust
//! In-house RTF tokenizer: MS-OXRTFEX HTML de-encapsulation (`\fromhtml1`) and plain-text extraction.
use encoding_rs::{Encoding, WINDOWS_1252};

use super::charset::for_codepage;

/// True when the RTF header (first 1,024 bytes) declares encapsulated HTML.
pub fn is_html_encapsulated(rtf: &[u8]) -> bool {
    rtf[..rtf.len().min(1024)].windows(10).any(|w| w == b"\\fromhtml1")
}

/// `\fromhtml1` RTF → the original HTML text.
pub fn deencapsulate_html(rtf: &[u8]) -> String { run(rtf, true) }

/// RTF without encapsulated HTML → plain text.
pub fn to_text(rtf: &[u8]) -> String { run(rtf, false) }

/// Groups whose content is never text.
const SKIP_DESTINATIONS: &[&str] = &[
    "fonttbl", "colortbl", "stylesheet", "info", "pict", "object", "header", "headerl", "headerr", "headerf",
    "footer", "footerl", "footerr", "footerf", "footnote", "listtable", "listoverridetable", "revtbl", "rsidtbl",
    "generator", "xmlnstbl", "themedata", "colorschememapping", "datastore", "latentstyles",
];

#[derive(Clone, Copy)]
struct GroupState { skip: bool, htmltag: bool, htmlrtf: bool, uc: usize }

struct Out {
    text: String,
    pending: Vec<u8>,
    enc: &'static Encoding,
}

impl Out {
    fn byte(&mut self, b: u8) { self.pending.push(b); }
    fn flush(&mut self) {
        if !self.pending.is_empty() {
            let (s, _) = self.enc.decode_without_bom_handling(&self.pending);
            self.text.push_str(&s);
            self.pending.clear();
        }
    }
    fn ch(&mut self, c: char) {
        self.flush();
        self.text.push(c);
    }
}

fn run(rtf: &[u8], html_mode: bool) -> String {
    let mut out = Out { text: String::new(), pending: Vec::new(), enc: WINDOWS_1252 };
    let mut stack: Vec<GroupState> = Vec::new();
    let mut st = GroupState { skip: false, htmltag: false, htmlrtf: false, uc: 1 };
    let mut skip_chars = 0usize; // \uN fallback characters still to skip
    let mut new_group = false; // the previous token was '{'
    let mut i = 0;
    let n = rtf.len();
    while i < n {
        let b = rtf[i];
        match b {
            b'{' => {
                stack.push(st);
                new_group = true;
                skip_chars = 0;
                i += 1;
                continue;
            }
            b'}' => {
                if let Some(prev) = stack.pop() {
                    st = prev;
                }
                skip_chars = 0;
                i += 1;
            }
            b'\r' | b'\n' => i += 1,
            b'\\' if i + 1 < n => {
                let c = rtf[i + 1];
                if c.is_ascii_alphabetic() {
                    let ws = i + 1;
                    let mut j = ws;
                    while j < n && rtf[j].is_ascii_alphabetic() { j += 1; }
                    let word = std::str::from_utf8(&rtf[ws..j]).unwrap_or("");
                    let ps = j;
                    if j < n && rtf[j] == b'-' { j += 1; }
                    while j < n && rtf[j].is_ascii_digit() { j += 1; }
                    let param: Option<i32> = if j > ps { std::str::from_utf8(&rtf[ps..j]).ok().and_then(|p| p.parse().ok()) } else { None };
                    if j < n && rtf[j] == b' ' { j += 1; }
                    i = j;
                    if word == "bin" {
                        i += param.unwrap_or(0).max(0) as usize;
                        new_group = false;
                        continue;
                    }
                    if new_group && SKIP_DESTINATIONS.contains(&word) {
                        st.skip = true;
                    }
                    control_word(word, param, &mut st, &mut out, &mut skip_chars, html_mode);
                } else if c == b'*' {
                    // {\*\dest …}: keep only \*\htmltag (HTML mode); skip every other optional destination.
                    let rest = &rtf[i + 2..];
                    let is_htmltag = rest.starts_with(b"\\htmltag");
                    if html_mode && is_htmltag {
                        st.htmltag = true;
                        st.skip = false;
                    } else {
                        st.skip = true;
                    }
                    i += 2;
                    continue; // keep new_group so "\*\htmltag" is still treated as the group's first word
                } else if c == b'\'' && i + 3 < n + 1 && i + 3 <= n {
                    let hex = std::str::from_utf8(&rtf[i + 2..(i + 4).min(n)]).ok().and_then(|h| u8::from_str_radix(h, 16).ok());
                    i += 4;
                    if let Some(v) = hex {
                        if skip_chars > 0 {
                            skip_chars -= 1;
                        } else if emits(&st, html_mode) {
                            out.byte(v);
                        }
                    }
                } else {
                    i += 2;
                    let sym = match c {
                        b'\\' => Some('\\'),
                        b'{' => Some('{'),
                        b'}' => Some('}'),
                        b'~' => Some('\u{00A0}'),
                        b'_' => Some('-'),
                        b'\r' | b'\n' => Some('\n'),
                        _ => None,
                    };
                    if let Some(s) = sym {
                        if skip_chars > 0 {
                            skip_chars -= 1;
                        } else if emits(&st, html_mode) {
                            out.ch(s);
                        }
                    }
                }
            }
            _ => {
                i += 1;
                if skip_chars > 0 {
                    skip_chars -= 1;
                } else if emits(&st, html_mode) {
                    out.byte(b);
                }
            }
        }
        new_group = false;
    }
    out.flush();
    out.text
}

fn emits(st: &GroupState, html_mode: bool) -> bool {
    if st.skip {
        return false;
    }
    if html_mode { st.htmltag || !st.htmlrtf } else { true }
}

fn control_word(word: &str, param: Option<i32>, st: &mut GroupState, out: &mut Out, skip_chars: &mut usize, html_mode: bool) {
    match word {
        "ansicpg" => {
            out.flush();
            if let Some(enc) = param.and_then(|p| u32::try_from(p).ok()).and_then(for_codepage) {
                out.enc = enc;
            }
        }
        "htmltag" => {
            if html_mode {
                st.htmltag = true;
            }
        }
        "htmlrtf" => st.htmlrtf = param != Some(0),
        "uc" => st.uc = param.unwrap_or(1).max(0) as usize,
        "u" => {
            if let Some(p) = param {
                let cp = if p < 0 { (p + 65_536) as u32 } else { p as u32 };
                if emits(st, html_mode) {
                    out.ch(char::from_u32(cp).unwrap_or('\u{FFFD}'));
                }
                *skip_chars = st.uc;
            }
        }
        "par" | "line" => {
            if emits(st, html_mode) {
                out.ch('\n');
            }
        }
        "tab" if emits(st, html_mode) => out.ch('\t'),
        _ => {}
    }
}
```


- [ ] **Step 22: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test -p outlooker-core --lib importer::normalize::rtf`
Expected: every test of the module prints `ok`; `test result: ok`.

- [ ] **Step 23: Create `src-tauri/crates/outlooker-core/tests/golden/plain.html`**

```html
<html xmlns:o="urn:schemas-microsoft-com:office:office">
<head>
<meta http-equiv="Content-Type" content="text/html; charset=utf-8">
<title>Weekly report</title>
<style><!-- p.MsoNormal { margin:0cm; font-family:Calibri } --></style>
<!--[if gte mso 9]><xml><o:shapedefaults v:ext="edit" spidmax="1026" /></xml><![endif]-->
</head>
<body lang="EN-US" link="#0563C1">
<div class="WordSection1">
<p class="MsoNormal" style="font-family:Calibri;mso-fareast-language:ZH-TW;color:#1F497D">Hi team,<o:p></o:p></p>
<p class="MsoNormal">The <b>Q3</b> numbers are in the <a href="https://intranet.example/q3" target="_blank">report</a>.</p>
<table border="1" cellpadding="2" class="MsoTableGrid" style="border-collapse:collapse">
<tr><td width="120" style="padding:4px">Region</td><td>Total</td></tr>
<tr><td>台北</td><td align="right">1,200</td></tr>
</table>
<p class="MsoNormal">Regards,<br>Alice</p>
</div>
</body>
</html>
```


- [ ] **Step 24: Create `src-tauri/crates/outlooker-core/tests/golden/plain.expected.html`**

```html
<div>
<p style="font-family:Calibri;color:#1F497D">Hi team,</p>
<p>The <b>Q3</b> numbers are in the <a href="https://intranet.example/q3" rel="noopener noreferrer">report</a>.</p>
<table border="1" cellpadding="2" style="border-collapse:collapse">
<tbody><tr><td width="120" style="padding:4px">Region</td><td>Total</td></tr>
<tr><td>台北</td><td align="right">1,200</td></tr>
</tbody></table>
<p>Regards,<br>Alice</p>
</div>
```


- [ ] **Step 25: Create `src-tauri/crates/outlooker-core/tests/golden/hostile.html`**

```html
<html><head>
<base href="https://evil.example/">
<meta http-equiv="refresh" content="0;url=https://evil.example/">
<script>alert(1)</script>
<link rel="stylesheet" href="https://evil.example/x.css">
</head>
<body onload="steal()">
<p onclick="steal()" style="background:url(https://evil.example/t.gif)">Quarterly numbers</p>
<div style="position:fixed;top:0;left:0;width:100%;height:100%">overlay</div>
<a href="javascript:alert(2)">click</a>
<a href="vbscript:msgbox(3)">vb</a>
<img src="x.png" onerror="alert(4)">
<img src="https://tracker.example/pixel.gif" alt="t">
<img src="http://tracker.example/p2.gif">
<iframe src="https://evil.example/frame"></iframe>
<form action="https://evil.example/post"><input name="pw"><button>Send</button></form>
<object data="x.swf"></object><embed src="x.swf">
<svg><script>alert(5)</script></svg>
<p style="width:expression(alert(6))">expr</p>
<p style="color:red;behavior:url(x.htc)">behave</p>
<span style="color:blue">safe blue</span>
</body></html>
```


- [ ] **Step 26: Write the failing tests in `src-tauri/crates/outlooker-core/src/importer/normalize/sanitize.rs`**

Create the file with only this test module (the code under test does not exist yet):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn golden(name: &str) -> String {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(name);
        std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("missing golden file {}: {e}", p.display()))
    }

    /// The forbidden patterns of the R1-05 acceptance, matched without a regex crate.
    pub(crate) fn hostile_violations(out: &str) -> Vec<String> {
        let lower = out.to_ascii_lowercase();
        let mut found = Vec::new();
        for p in ["<script", "javascript:", "url(", "<iframe", "<form", "position", "src=\"http"] {
            if lower.contains(p) {
                found.push(p.to_string());
            }
        }
        // on[a-z]+=
        let bytes = lower.as_bytes();
        for i in 0..bytes.len().saturating_sub(2) {
            if bytes[i] == b'o' && bytes[i + 1] == b'n' && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric()) {
                let mut j = i + 2;
                while j < bytes.len() && bytes[j].is_ascii_lowercase() { j += 1; }
                if j > i + 2 && j < bytes.len() && bytes[j] == b'=' {
                    found.push(lower[i..=j].to_string());
                }
            }
        }
        found
    }

    #[test]
    fn hostile_corpus_is_clean() {
        let out = sanitize(&golden("hostile.html"), &HashMap::new());
        assert_eq!(hostile_violations(&out.html), Vec::<String>::new(), "output: {}", out.html);
        assert!(out.html.contains("Quarterly numbers"), "text content kept: {}", out.html);
        assert!(!out.html.contains("<base") && !out.html.contains("<meta"));
    }

    #[test]
    fn golden_plain() {
        let out = sanitize(&golden("plain.html"), &HashMap::new());
        assert_eq!(out.html, golden("plain.expected.html").trim_end_matches('\n'));
        assert!(out.refs.is_empty());
    }

    #[test]
    fn cid_and_data_ordinals() {
        let mut cids = HashMap::new();
        cids.insert("logo@x".to_string(), 0usize);
        let html = r#"<p><img src="cid:LOGO@X"><img src="data:image/png;base64,iVBORw0K"><img src="cid:logo@x"><img src="cid:missing"><img src="https://t.example/p.gif"><a href="javascript:alert(1)">x</a><a href="cid:logo@x">y</a><a href="https://ok.example/">z</a></p>"#;
        let out = sanitize(html, &cids);
        assert_eq!(
            out.html,
            r#"<p><img src="outlooker-inline:0"><img src="outlooker-inline:1"><img src="outlooker-inline:0"><img><img><a rel="noopener noreferrer">x</a><a rel="noopener noreferrer">y</a><a href="https://ok.example/" rel="noopener noreferrer">z</a></p>"#
        );
        assert_eq!(out.refs.len(), 2);
        assert_eq!(drop_src(&out.html, 0).matches("outlooker-inline:0").count(), 0);
    }
}
```


- [ ] **Step 27: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test -p outlooker-core --lib importer::normalize::sanitize`
Expected: compile error (cannot find function `sanitize`).

- [ ] **Step 28: Implement `src-tauri/crates/outlooker-core/src/importer/normalize/sanitize.rs`**

Insert this code above the test module (the whole file is this code followed by the tests of the previous step):

```rust
//! Part 04 §5.6 HTML sanitize + normalize (ammonia allow-list) and the cid / data: image rewrite.
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use ammonia::{Builder, UrlRelative};
use base64::Engine as _;

const TAGS: &[&str] = &[
    "a", "abbr", "b", "blockquote", "br", "caption", "center", "cite", "code", "col", "colgroup", "dd", "del", "div",
    "dl", "dt", "em", "font", "h1", "h2", "h3", "h4", "h5", "h6", "hr", "i", "img", "ins", "li", "ol", "p", "pre", "q",
    "s", "small", "span", "strike", "strong", "sub", "sup", "table", "tbody", "td", "tfoot", "th", "thead", "tr", "u",
    "ul",
];
const CLEAN_CONTENT_TAGS: &[&str] = &[
    "script", "style", "head", "title", "iframe", "frame", "frameset", "object", "embed", "applet", "form", "input",
    "button", "select", "option", "textarea", "svg", "math", "noscript", "template", "link", "meta", "base", "audio",
    "video", "canvas",
];
const GENERIC_ATTRIBUTES: &[&str] = &["style", "title", "dir", "lang", "align", "valign", "width", "height", "bgcolor"];
const TAG_ATTRIBUTES: &[(&str, &[&str])] = &[
    ("a", &["href"]),
    ("img", &["src", "alt", "border", "hspace", "vspace"]),
    ("font", &["color", "face", "size"]),
    ("table", &["border", "cellpadding", "cellspacing"]),
    ("td", &["colspan", "rowspan", "nowrap"]),
    ("th", &["colspan", "rowspan", "nowrap"]),
    ("col", &["span"]),
    ("colgroup", &["span"]),
    ("ol", &["start", "type"]),
    ("li", &["value", "type"]),
];
/// `cid` and `data` must pass ammonia's scheme check (it runs before `attribute_filter`); the filter then
/// rewrites them, so neither survives in the output.
const URL_SCHEMES: &[&str] = &["http", "https", "mailto", "cid", "data", "outlooker-inline"];
const STYLE_PROPERTIES: &[&str] = &[
    "color", "background-color", "font", "font-family", "font-size", "font-weight", "font-style", "font-variant",
    "text-decoration", "text-align", "text-indent", "text-transform", "vertical-align", "line-height",
    "letter-spacing", "word-spacing", "white-space", "margin", "margin-top", "margin-right", "margin-bottom",
    "margin-left", "padding", "padding-top", "padding-right", "padding-bottom", "padding-left", "border",
    "border-top", "border-right", "border-bottom", "border-left", "border-width", "border-style", "border-color",
    "border-collapse", "border-spacing", "width", "height", "max-width", "min-width", "max-height", "min-height",
    "display", "list-style-type", "direction",
];
const STYLE_FORBIDDEN: &[&str] = &["url(", "expression(", "javascript:", "@import", "behavior", "\\", "/*", "<"];
const DATA_IMAGE_PREFIXES: &[&str] = &[
    "data:image/png;base64,", "data:image/jpeg;base64,", "data:image/gif;base64,", "data:image/bmp;base64,",
    "data:image/webp;base64,",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlineSource {
    /// Index into `RawMail.attachments`.
    Cid { cid: String, attachment: usize },
    Data(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineRef { pub ordinal: i32, pub source: InlineSource }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sanitized { pub html: String, pub refs: Vec<InlineRef> }

/// cid normalization for matching: strip surrounding `<` `>`, trim, ASCII-lowercase.
pub fn normalize_cid(s: &str) -> String {
    let t = s.trim();
    let t = t.strip_prefix('<').unwrap_or(t);
    let t = t.strip_suffix('>').unwrap_or(t);
    t.trim().to_ascii_lowercase()
}

#[derive(Default)]
struct RefState { refs: Vec<InlineRef>, by_cid: HashMap<String, i32> }

/// Sanitizes one message body. `cid_map`: normalized content id → attachment index.
pub fn sanitize(html: &str, cid_map: &HashMap<String, usize>) -> Sanitized {
    let state = Arc::new(Mutex::new(RefState::default()));
    let cids = Arc::new(cid_map.clone());
    let filter_state = Arc::clone(&state);
    let mut b = Builder::empty();
    b.tags(TAGS.iter().copied().collect())
        .clean_content_tags(CLEAN_CONTENT_TAGS.iter().copied().collect())
        .generic_attributes(GENERIC_ATTRIBUTES.iter().copied().collect())
        .tag_attributes(TAG_ATTRIBUTES.iter().map(|(t, a)| (*t, a.iter().copied().collect::<HashSet<_>>())).collect())
        .url_schemes(URL_SCHEMES.iter().copied().collect())
        .url_relative(UrlRelative::Deny)
        .link_rel(Some("noopener noreferrer"))
        .strip_comments(true)
        .filter_style_properties(STYLE_PROPERTIES.iter().copied().collect())
        .attribute_filter(move |element, attribute, value| filter(element, attribute, value, &cids, &filter_state));
    let html = b.clean(html).to_string().trim().to_string();
    let refs = std::mem::take(&mut state.lock().unwrap().refs);
    Sanitized { html, refs }
}

fn filter<'u>(
    element: &str,
    attribute: &str,
    value: &'u str,
    cids: &HashMap<String, usize>,
    state: &Mutex<RefState>,
) -> Option<Cow<'u, str>> {
    match (element, attribute) {
        ("a", "href") => {
            let l = value.trim().to_ascii_lowercase();
            if l.starts_with("http:") || l.starts_with("https:") || l.starts_with("mailto:") {
                Some(Cow::Borrowed(value))
            } else {
                None
            }
        }
        ("img", "src") => {
            let v = value.trim();
            let lower = v.to_ascii_lowercase();
            let mut st = state.lock().unwrap();
            if let Some(rest) = lower.strip_prefix("cid:") {
                let cid = normalize_cid(rest);
                if let Some(&ordinal) = st.by_cid.get(&cid) {
                    return Some(Cow::Owned(format!("outlooker-inline:{ordinal}")));
                }
                let &attachment = cids.get(&cid)?;
                let ordinal = st.refs.len() as i32;
                st.by_cid.insert(cid.clone(), ordinal);
                st.refs.push(InlineRef { ordinal, source: InlineSource::Cid { cid, attachment } });
                return Some(Cow::Owned(format!("outlooker-inline:{ordinal}")));
            }
            if let Some(prefix) = DATA_IMAGE_PREFIXES.iter().find(|p| lower.starts_with(**p)) {
                let payload: String = v[prefix.len()..].chars().filter(|c| !c.is_whitespace()).collect();
                let bytes = base64::engine::general_purpose::STANDARD.decode(payload.as_bytes()).ok()?;
                let ordinal = st.refs.len() as i32;
                st.refs.push(InlineRef { ordinal, source: InlineSource::Data(bytes) });
                return Some(Cow::Owned(format!("outlooker-inline:{ordinal}")));
            }
            None // remote http(s) and everything else: never fetched
        }
        (_, "style") => {
            let l = value.to_ascii_lowercase();
            if STYLE_FORBIDDEN.iter().any(|f| l.contains(f)) { None } else { Some(Cow::Borrowed(value)) }
        }
        _ => Some(Cow::Borrowed(value)),
    }
}

/// Removes ` src="outlooker-inline:<ordinal>"` (an image that was skipped or failed); the `<img>` stays.
pub fn drop_src(html: &str, ordinal: i32) -> String {
    html.replace(&format!(" src=\"outlooker-inline:{ordinal}\""), "")
}
```


- [ ] **Step 29: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test -p outlooker-core --lib importer::normalize::sanitize`
Expected: every test of the module prints `ok`; `test result: ok`.

- [ ] **Step 30: Write the failing tests in `src-tauri/crates/outlooker-core/src/importer/normalize/text.rs`**

Create the file with only this test module (the code under test does not exist yet):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_and_snippet() {
        assert_eq!(clean_text("  a \t b\r\n\r\n\r\n\r\nc\rd  "), "a b\n\nc\nd");
        let long = "x ".repeat(300);
        assert_eq!(snippet(&long).chars().count(), 200);
        assert_eq!(snippet("a\n\n b\tc"), "a b c");
    }

    #[test]
    fn html_text_has_no_wrapping() {
        let words = "word ".repeat(100);
        let t = html_to_text(&format!("<p>{words}</p><table><tr><td>a</td><td>b</td></tr></table>")).unwrap();
        assert!(t.lines().next().unwrap().len() >= 400, "one long line: {t:?}");
        assert!(t.contains("a\nb") || t.contains("a\n\nb"), "cells on separate lines: {t:?}");
    }
}
```


- [ ] **Step 31: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test -p outlooker-core --lib importer::normalize::text`
Expected: compile error (cannot find function `clean_text`).

- [ ] **Step 32: Implement `src-tauri/crates/outlooker-core/src/importer/normalize/text.rs`**

Insert this code above the test module (the whole file is this code followed by the tests of the previous step):

```rust
//! Part 04 §5.5 `body_text` and `snippet`.

pub const HTML2TEXT_WIDTH: usize = 1_000_000;

/// Search text of a sanitized HTML fragment (no decorations, table cells on separate lines, no wrapping).
pub fn html_to_text(sanitized_html: &str) -> Result<String, String> {
    html2text::config::plain_no_decorate()
        .raw_mode(true)
        .string_from_read(sanitized_html.as_bytes(), HTML2TEXT_WIDTH)
        .map_err(|e| e.to_string())
}

/// CRLF / CR → LF, runs of spaces / tabs → one space, 3+ LF → 2 LF, trim.
pub fn clean_text(s: &str) -> String {
    let s = s.replace("\r\n", "\n").replace('\r', "\n");
    let mut out = String::with_capacity(s.len());
    let mut lf_run = 0usize;
    let mut in_space = false;
    for c in s.chars() {
        match c {
            '\n' => {
                lf_run += 1;
                in_space = false;
                if lf_run <= 2 {
                    out.push('\n');
                }
            }
            ' ' | '\t' => {
                lf_run = 0;
                if !in_space {
                    out.push(' ');
                    in_space = true;
                }
            }
            _ => {
                lf_run = 0;
                in_space = false;
                out.push(c);
            }
        }
    }
    out.trim().to_string()
}

/// First 200 chars after every whitespace run → one space.
pub fn snippet(body_text: &str) -> String {
    body_text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(super::SNIPPET_CHARS).collect()
}
```


- [ ] **Step 33: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test -p outlooker-core --lib importer::normalize::text`
Expected: every test of the module prints `ok`; `test result: ok`.

- [ ] **Step 34: Write the failing tests in `src-tauri/crates/outlooker-core/src/importer/normalize/participants.rs`**

Create the file with only this test module (the code under test does not exist yet):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::importer::types::RawRecipient;

    #[test]
    fn sender_fallbacks_and_recipients() {
        let m = RawMail {
            sender_email_address: Some(" /O=ORG/CN=Alice ".into()),
            sender_address_type: Some("EX".into()),
            sent_representing_name: Some("Alice Chen".into()),
            recipients: vec![
                RawRecipient { smtp_address: Some("Bob@X.COM".into()), display_name: Some("Bob".into()), recipient_type: Some(1), ..Default::default() },
                RawRecipient { ..Default::default() },
                RawRecipient { email_address: Some("carol@x.com".into()), address_type: Some("SMTP".into()), recipient_type: Some(2), ..Default::default() },
                RawRecipient { display_name: Some("Dave".into()), recipient_type: Some(9), ..Default::default() },
            ],
            ..Default::default()
        };
        let p = participants(&m);
        assert_eq!(p.sender, Some(AddressIn { email: "/o=org/cn=alice".into(), address_type: AddressType::Ex }));
        assert_eq!(p.sender_search, "Alice Chen </o=org/cn=alice>");
        assert_eq!(p.recipients.iter().map(|r| r.ordinal).collect::<Vec<_>>(), vec![0, 2, 3]);
        assert_eq!(p.recipients[1].address.as_ref().unwrap().address_type, AddressType::Smtp);
        assert_eq!(p.recipients[2].kind, RecipientKind::To);
        assert_eq!(p.recipients_search, "Bob <bob@x.com>\ncarol@x.com\nDave");
        assert_eq!(p.to_preview, "Bob, Dave");
    }
}
```


- [ ] **Step 35: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test -p outlooker-core --lib importer::normalize::participants`
Expected: compile error (cannot find function `participants`).

- [ ] **Step 36: Implement `src-tauri/crates/outlooker-core/src/importer/normalize/participants.rs`**

Insert this code above the test module (the whole file is this code followed by the tests of the previous step):

```rust
//! Part 04 §5.4 sender, recipients and search strings.
use crate::dto::search::RecipientKind;
use crate::importer::types::{AddressIn, AddressType, RawMail, RecipientOut};

pub struct Participants {
    pub sender: Option<AddressIn>,
    pub sender_name: Option<String>,
    pub sender_search: String,
    pub recipients: Vec<RecipientOut>,
    pub recipients_search: String,
    pub to_preview: String,
}

fn non_empty(v: &Option<String>) -> Option<&str> {
    v.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

/// trim + lowercase; empty or longer than 2,048 chars → None.
fn address(raw: &str, address_type: AddressType) -> Option<AddressIn> {
    let email = raw.trim().to_lowercase();
    if email.is_empty() || email.chars().count() > 2048 {
        return None;
    }
    Some(AddressIn { email, address_type })
}

fn type_of(t: Option<&str>) -> AddressType {
    match t {
        Some(t) if t.trim().eq_ignore_ascii_case("smtp") => AddressType::Smtp,
        Some(t) if t.trim().eq_ignore_ascii_case("ex") => AddressType::Ex,
        _ => AddressType::Other,
    }
}

/// "Name <addr>" | "Name" | "addr" | "".
pub fn search_text(name: Option<&str>, addr: Option<&str>) -> String {
    match (name, addr) {
        (Some(n), Some(a)) => format!("{n} <{a}>"),
        (Some(n), None) => n.to_string(),
        (None, Some(a)) => a.to_string(),
        (None, None) => String::new(),
    }
}

pub fn participants(m: &RawMail) -> Participants {
    let sender = non_empty(&m.sender_smtp_address)
        .map(|a| (a, AddressType::Smtp))
        .or_else(|| non_empty(&m.sent_representing_smtp_address).map(|a| (a, AddressType::Smtp)))
        .or_else(|| non_empty(&m.sender_email_address).map(|a| (a, type_of(m.sender_address_type.as_deref()))))
        .and_then(|(a, t)| address(a, t));
    let sender_name = non_empty(&m.sender_name).or_else(|| non_empty(&m.sent_representing_name)).map(str::to_string);
    let sender_search = search_text(sender_name.as_deref(), sender.as_ref().map(|a| a.email.as_str()));

    let mut recipients = Vec::new();
    for (i, r) in m.recipients.iter().enumerate() {
        let addr = non_empty(&r.smtp_address)
            .map(|a| (a, AddressType::Smtp))
            .or_else(|| non_empty(&r.email_address).map(|a| (a, type_of(r.address_type.as_deref()))))
            .and_then(|(a, t)| address(a, t));
        let display_name = non_empty(&r.display_name).map(str::to_string);
        if addr.is_none() && display_name.is_none() {
            continue; // DB CHECK: address or name required
        }
        let kind = match r.recipient_type {
            Some(2) => RecipientKind::Cc,
            Some(3) => RecipientKind::Bcc,
            _ => RecipientKind::To,
        };
        let search_text = search_text(display_name.as_deref(), addr.as_ref().map(|a| a.email.as_str()));
        recipients.push(RecipientOut { ordinal: i as i32, kind, address: addr, display_name, search_text });
    }
    let recipients_search = recipients.iter().map(|r| r.search_text.as_str()).collect::<Vec<_>>().join("\n");
    let to_preview = recipients
        .iter()
        .filter(|r| r.kind == RecipientKind::To)
        .take(3)
        .map(|r| r.display_name.clone().or_else(|| r.address.as_ref().map(|a| a.email.clone())).unwrap_or_default())
        .collect::<Vec<_>>()
        .join(", ");
    Participants { sender, sender_name, sender_search, recipients, recipients_search, to_preview }
}
```


- [ ] **Step 37: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test -p outlooker-core --lib importer::normalize::participants`
Expected: every test of the module prints `ok`; `test result: ok`.

- [ ] **Step 38: Write the failing tests in `src-tauri/crates/outlooker-core/src/importer/normalize/attachments.rs`**

Create the file with only this test module (the code under test does not exist yet):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_rules() {
        let embedded = attachment_out(&RawAttachment { index: 2, attach_method: Some(5), embedded_subject: Some("Re: plan".into()), attach_size: Some(-4), ..Default::default() });
        assert_eq!((embedded.file_name.as_str(), embedded.mime_type.as_str(), embedded.size_bytes), ("Re: plan.msg", "application/vnd.ms-outlook", 0));
        let guessed = attachment_out(&RawAttachment { index: 0, attach_method: Some(1), filename: Some("Q3.PDF".into()), data_size: Some(10), ..Default::default() });
        assert_eq!(guessed.mime_type, "application/pdf");
        let unnamed = attachment_out(&RawAttachment { index: 7, ..Default::default() });
        assert_eq!((unnamed.file_name.as_str(), unnamed.mime_type.as_str(), unnamed.attach_method), ("attachment-7", "application/octet-stream", 0));
        let long = attachment_out(&RawAttachment { long_filename: Some(format!("{}.txt", "n".repeat(2000))), ..Default::default() });
        assert_eq!(long.file_name.chars().count(), 1024);
    }
}
```


- [ ] **Step 39: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test -p outlooker-core --lib importer::normalize::attachments`
Expected: compile error (cannot find function `attachment_out`).

- [ ] **Step 40: Implement `src-tauri/crates/outlooker-core/src/importer/normalize/attachments.rs`**

Insert this code above the test module (the whole file is this code followed by the tests of the previous step):

```rust
//! Part 04 §5.8 attachment metadata.
use crate::importer::types::{AttachmentOut, RawAttachment};

const METHOD_EMBEDDED: i32 = 5;

fn cut(s: &str, max: usize) -> String { s.chars().take(max).collect() }

fn non_empty(v: &Option<String>) -> Option<&str> {
    v.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

pub fn mime_from_extension(file_name: &str) -> Option<&'static str> {
    let ext = file_name.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(match ext.as_str() {
        "pdf" => "application/pdf",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "txt" => "text/plain",
        "csv" => "text/csv",
        "htm" | "html" => "text/html",
        "xml" => "application/xml",
        "json" => "application/json",
        "rtf" => "application/rtf",
        "zip" => "application/zip",
        "7z" => "application/x-7z-compressed",
        "rar" => "application/vnd.rar",
        "gz" => "application/gzip",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "msg" => "application/vnd.ms-outlook",
        "eml" => "message/rfc822",
        "ics" => "text/calendar",
        "vcf" => "text/vcard",
        "mp3" => "audio/mpeg",
        "mp4" => "video/mp4",
        "wav" => "audio/wav",
        _ => return None,
    })
}

pub fn attachment_out(a: &RawAttachment) -> AttachmentOut {
    let method = a.attach_method.unwrap_or(0);
    let file_name = if method == METHOD_EMBEDDED {
        format!("{}.msg", non_empty(&a.embedded_subject).unwrap_or("message"))
    } else {
        non_empty(&a.long_filename)
            .or_else(|| non_empty(&a.filename))
            .or_else(|| non_empty(&a.display_name))
            .map(str::to_string)
            .unwrap_or_else(|| format!("attachment-{}", a.index))
    };
    let file_name = cut(&file_name, 1024);
    let size_bytes = a.data_size.or(a.attach_size).unwrap_or(0).max(0);
    let mime_type = if method == METHOD_EMBEDDED {
        "application/vnd.ms-outlook".to_string()
    } else {
        non_empty(&a.mime_tag)
            .map(|m| cut(m, 255))
            .or_else(|| mime_from_extension(&file_name).map(str::to_string))
            .unwrap_or_else(|| "application/octet-stream".to_string())
    };
    AttachmentOut { attachment_index: a.index, file_name, size_bytes, mime_type, attach_method: method }
}
```


- [ ] **Step 41: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test -p outlooker-core --lib importer::normalize::attachments`
Expected: every test of the module prints `ok`; `test result: ok`.

- [ ] **Step 42: Create `src-tauri/crates/outlooker-core/tests/golden/inline.html`**

```html
<html><body>
<p>Logo: <img src="cid:LOGO@X" alt="logo" width="40" height="20"></p>
<p>Again: <img src="cid:logo@x" alt="logo2"></p>
<p>Pixel: <img src="cid:pixel@x" alt="pixel"></p>
<p>Missing: <img src="cid:nothere@x" alt="missing"></p>
<p>Remote: <img src="https://cdn.example/banner.png" alt="remote"></p>
</body></html>
```


- [ ] **Step 43: Create `src-tauri/crates/outlooker-core/tests/golden/inline.expected.html`**

```html
<p>Logo: <img src="outlooker-inline:0" alt="logo" width="40" height="20"></p>
<p>Again: <img src="outlooker-inline:0" alt="logo2"></p>
<p>Pixel: <img alt="pixel"></p>
<p>Missing: <img alt="missing"></p>
<p>Remote: <img alt="remote"></p>
```


- [ ] **Step 44: Write the failing tests in `src-tauri/crates/outlooker-core/src/importer/normalize/mod.rs`**

Create the file with only this test module (the code under test does not exist yet):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::importer::types::{DedupeKind, RawAttachment};
    use crate::importer::webp::tests::png;

    fn golden(name: &str) -> String {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(name);
        std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("missing golden file {}: {e}", p.display()))
    }

    fn ctx() -> NormalizeCtx { NormalizeCtx { webp_quality: 80, known: Arc::new(KnownHashes::new()) } }

    fn item(m: RawMail) -> RawItem {
        RawItem { seq: 1, pst_item_id: 100, pst_folder_id: 1, folder_path: "Inbox".into(), content: RawContent::Mail(Box::new(m)) }
    }

    fn mail_of(item: NormalizedItem) -> NormalizedMail {
        match item.outcome { Outcome::Mail(m) => *m, other => panic!("expected mail, got {other:?}") }
    }

    fn inline_mail() -> RawMail {
        RawMail {
            subject: Some("Inline".into()),
            html_body: Some(golden("inline.html").into_bytes()),
            attachments: vec![
                RawAttachment { index: 0, content_id: Some("<logo@x>".into()), long_filename: Some("logo.png".into()), data: Some(png(40, 20)), attach_method: Some(1), ..Default::default() },
                RawAttachment { index: 1, content_id: Some("pixel@x".into()), long_filename: Some("pixel.png".into()), data: Some(png(1, 1)), attach_method: Some(1), ..Default::default() },
                RawAttachment { index: 2, content_id: Some("unused@x".into()), long_filename: Some("unused.png".into()), data: Some(png(8, 8)), attach_method: Some(1), ..Default::default() },
                RawAttachment { index: 3, long_filename: Some("report.pdf".into()), data_size: Some(1234), attach_method: Some(1), ..Default::default() },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn golden_inline() {
        let m = mail_of(normalize(item(inline_mail()), &ctx()));
        assert_eq!(m.body_html.as_deref(), Some(golden("inline.expected.html").trim_end_matches('\n')));
        assert_eq!(m.inline_images.iter().map(|i| i.ordinal).collect::<Vec<_>>(), vec![0]);
        assert_eq!(m.inline_images[0].content_id.as_deref(), Some("logo@x"));
        // referenced attachments (logo, pixel) are inline; unused cid + pdf stay attachments
        assert_eq!(m.attachments.iter().map(|a| a.file_name.as_str()).collect::<Vec<_>>(), vec!["unused.png", "report.pdf"]);
    }

    #[test]
    fn one_pixel_image_loses_src() {
        let m = mail_of(normalize(item(inline_mail()), &ctx()));
        let html = m.body_html.unwrap();
        assert!(html.contains(r#"<img alt="pixel">"#), "{html}");
        assert!(!html.contains("outlooker-inline:1"));
    }

    #[test]
    fn plain_body_wins_for_text_and_html_kept() {
        let m = RawMail {
            html_body: Some(golden("plain.html").into_bytes()),
            plain_body: Some("Plain   body\r\n\r\n\r\ntext".into()),
            ..Default::default()
        };
        let n = mail_of(normalize(item(m), &ctx()));
        assert_eq!(n.body_source, BodySource::Html);
        assert_eq!(n.body_text, "Plain body\n\ntext");
    }

    #[test]
    fn rtf_paths_and_none() {
        let fromhtml = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/fromhtml.rtf")).unwrap();
        let n = mail_of(normalize(item(RawMail { rtf_body: Some(fromhtml), ..Default::default() }), &ctx()));
        assert_eq!(n.body_source, BodySource::RtfHtml);
        assert!(n.body_text.contains("Price: 100€ café"), "{}", n.body_text);
        let plain_rtf = br"{\rtf1\ansi Hello\par world}".to_vec();
        let n = mail_of(normalize(item(RawMail { rtf_body: Some(plain_rtf), ..Default::default() }), &ctx()));
        assert_eq!((n.body_source, n.body_html.is_none(), n.body_text.as_str()), (BodySource::RtfText, true, "Hello\nworld"));
        let n = mail_of(normalize(item(RawMail::default()), &ctx()));
        assert_eq!((n.body_source, n.body_text.as_str()), (BodySource::None, ""));
    }

    #[test]
    fn non_mail_and_unreadable() {
        let mut raw = item(RawMail::default());
        raw.content = RawContent::NonMail;
        assert!(matches!(normalize(raw, &ctx()).outcome, Outcome::NonMail));
        let mut raw = item(RawMail::default());
        raw.content = RawContent::Unreadable { stage: ImportErrorStage::Parse, message: "bad block".into() };
        let n = normalize(raw, &ctx());
        assert!(matches!(n.outcome, Outcome::Skipped));
        assert_eq!(n.errors[0].severity, ImportErrorSeverity::ItemSkipped);
        assert_eq!(n.errors[0].pst_item_id, Some(100));
    }

    #[test]
    fn invariants_over_goldens() {
        let samples = vec![
            RawMail { html_body: Some(golden("plain.html").into_bytes()), sender_smtp_address: Some("A@B.C".into()), ..Default::default() },
            inline_mail(),
            RawMail { html_body: Some(golden("hostile.html").into_bytes()), internet_message_id: Some("<x@y>".into()), ..Default::default() },
            RawMail { rtf_body: Some(std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/fromhtml.rtf")).unwrap()), ..Default::default() },
            RawMail { plain_body: Some("p".repeat(500)), ..Default::default() },
            RawMail { rtf_body: Some(br"{\rtf1 x}".to_vec()), ..Default::default() },
            RawMail::default(),
        ];
        for s in samples {
            let m = mail_of(normalize(item(s), &ctx()));
            assert!(m.snippet.chars().count() <= 200);
            for e in m.sender.iter().map(|a| &a.email).chain(m.recipients.iter().filter_map(|r| r.address.as_ref()).map(|a| &a.email)) {
                assert_eq!(e, &e.to_lowercase());
            }
            assert_eq!(m.body_html.is_none(), matches!(m.body_source, BodySource::Text | BodySource::RtfText | BodySource::None));
            assert_eq!(m.dedupe_key.len(), 32);
            assert_eq!(m.message_id.is_some(), m.dedupe_kind == DedupeKind::MessageId);
        }
    }
}
```


- [ ] **Step 45: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test -p outlooker-core --lib importer::normalize::tests`
Expected: compile error (cannot find function `normalize`).

- [ ] **Step 46: Implement `src-tauri/crates/outlooker-core/src/importer/normalize/mod.rs`**

Insert this code above the test module (the whole file is this code followed by the tests of the previous step):

```rust
//! RawItem → NormalizedItem (Part 01 §5, Part 04 §4.1, §5.4–§5.8). Pure, panic-safe per item.
pub mod attachments;
pub mod charset;
pub mod participants;
pub mod rtf;
pub mod sanitize;
pub mod text;

use std::collections::{HashMap, HashSet};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

use rayon::prelude::*;

use crate::dto::import::{ImportErrorSeverity, ImportErrorStage};
use crate::importer::dedupe;
use crate::importer::types::{
    BodySource, InlineImageOut, ItemError, NormalizedItem, NormalizedMail, Outcome, RawContent, RawItem, RawMail,
};
use crate::importer::webp::{self, ImageIn, ImageOut, KnownHashes};
use sanitize::{InlineSource, Sanitized};

pub const MAX_HTML_BYTES: usize = 16 * 1024 * 1024;
pub const SNIPPET_CHARS: usize = 200;

pub struct NormalizeCtx { pub webp_quality: u8, pub known: Arc<KnownHashes> }

pub fn normalize(raw: RawItem, ctx: &NormalizeCtx) -> NormalizedItem {
    let (seq, pst_item_id, pst_folder_id) = (raw.seq, raw.pst_item_id, raw.pst_folder_id);
    let raw_bytes = raw.approx_bytes();
    let folder_path = raw.folder_path.clone();
    let result = catch_unwind(AssertUnwindSafe(|| normalize_inner(raw, ctx)));
    match result {
        Ok((outcome, errors)) => NormalizedItem { seq, pst_item_id, pst_folder_id, raw_bytes, outcome, errors },
        Err(payload) => {
            let msg = payload
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".to_string());
            let err = ItemError::new(ImportErrorStage::Html, ImportErrorSeverity::ItemSkipped, &folder_path, pst_item_id, &format!("panic: {msg}"));
            NormalizedItem { seq, pst_item_id, pst_folder_id, raw_bytes, outcome: Outcome::Skipped, errors: vec![err] }
        }
    }
}

fn normalize_inner(raw: RawItem, ctx: &NormalizeCtx) -> (Outcome, Vec<ItemError>) {
    match raw.content {
        RawContent::NonMail => (Outcome::NonMail, Vec::new()),
        RawContent::Unreadable { stage, message } => (
            Outcome::Skipped,
            vec![ItemError::new(stage, ImportErrorSeverity::ItemSkipped, &raw.folder_path, raw.pst_item_id, &message)],
        ),
        RawContent::Mail(m) => {
            let mut errors = Vec::new();
            let mail = normalize_mail(&m, ctx, &raw.folder_path, raw.pst_item_id, &mut errors);
            (Outcome::Mail(Box::new(mail)), errors)
        }
    }
}

fn degraded(errors: &mut Vec<ItemError>, stage: ImportErrorStage, folder: &str, item: i64, msg: &str) {
    errors.push(ItemError::new(stage, ImportErrorSeverity::Degraded, folder, item, msg));
}

fn normalize_mail(m: &RawMail, ctx: &NormalizeCtx, folder: &str, item: i64, errors: &mut Vec<ItemError>) -> NormalizedMail {
    // cid map: normalized content id → attachment index (only attachments with bytes).
    let cid_map: HashMap<String, usize> = m
        .attachments
        .iter()
        .enumerate()
        .filter(|(_, a)| a.data.is_some())
        .filter_map(|(i, a)| a.content_id.as_deref().map(|c| (sanitize::normalize_cid(c), i)))
        .filter(|(c, _)| !c.is_empty())
        .collect();

    // Body selection, Part 04 §5.5.
    let mut sanitized: Option<(BodySource, Sanitized)> = None;
    if let Some(html) = m.html_body.as_deref().filter(|b| !b.is_empty()) {
        let decoded = charset::decode_html(html, m.internet_codepage);
        if decoded.len() > MAX_HTML_BYTES {
            degraded(errors, ImportErrorStage::Html, folder, item, "html body larger than 16 MB, not sanitized");
        } else {
            sanitized = Some((BodySource::Html, sanitize::sanitize(&decoded, &cid_map)));
        }
    }
    if sanitized.is_none()
        && let Some(rtf) = m.rtf_body.as_deref().filter(|r| rtf::is_html_encapsulated(r))
    {
        let html = rtf::deencapsulate_html(rtf);
        sanitized = Some((BodySource::RtfHtml, sanitize::sanitize(&html, &cid_map)));
    }
    let plain = m.plain_body.as_deref().filter(|p| !p.trim().is_empty());

    let (body_source, mut body_html, refs) = match sanitized {
        Some((src, s)) => (src, Some(s.html), s.refs),
        None if plain.is_some() => (BodySource::Text, None, Vec::new()),
        None if m.rtf_body.is_some() => (BodySource::RtfText, None, Vec::new()),
        None => (BodySource::None, None, Vec::new()),
    };

    // Inline images (Part 04 §5.7): convert every referenced image in parallel.
    let mut referenced: HashSet<usize> = HashSet::new();
    let inputs: Vec<(i32, Option<String>, ImageIn)> = refs
        .into_iter()
        .filter_map(|r| match r.source {
            InlineSource::Cid { cid, attachment } => {
                referenced.insert(attachment);
                let bytes = m.attachments[attachment].data.clone()?;
                Some((r.ordinal, Some(cid), ImageIn { bytes }))
            }
            InlineSource::Data(bytes) => Some((r.ordinal, None, ImageIn { bytes })),
        })
        .collect();
    let converted: Vec<(i32, Option<String>, ImageOut)> = inputs
        .into_par_iter()
        .map(|(ordinal, cid, input)| (ordinal, cid, webp::convert(&input, ctx.webp_quality, &ctx.known)))
        .collect();
    let mut inline_images = Vec::new();
    for (ordinal, content_id, out) in converted {
        match out {
            ImageOut::Encoded { source_hash, image } => {
                inline_images.push(InlineImageOut { ordinal, content_id, source_hash, blob: Some(image) })
            }
            ImageOut::Known { source_hash } => inline_images.push(InlineImageOut { ordinal, content_id, source_hash, blob: None }),
            ImageOut::Tiny => body_html = body_html.map(|h| sanitize::drop_src(&h, ordinal)),
            ImageOut::Rejected { message } => {
                degraded(errors, ImportErrorStage::Image, folder, item, &message);
                body_html = body_html.map(|h| sanitize::drop_src(&h, ordinal));
            }
        }
    }
    inline_images.sort_by_key(|i| i.ordinal);

    // body_text (search text).
    let raw_text = if let Some(p) = plain {
        p.to_string()
    } else if let Some(h) = body_html.as_deref() {
        match text::html_to_text(h) {
            Ok(t) => t,
            Err(e) => {
                degraded(errors, ImportErrorStage::Html, folder, item, &format!("html to text failed: {e}"));
                String::new()
            }
        }
    } else if let Some(r) = m.rtf_body.as_deref() {
        rtf::to_text(r)
    } else {
        String::new()
    };
    let body_text = text::clean_text(&raw_text);
    let snippet = text::snippet(&body_text);

    let p = participants::participants(m);
    let attachments = m
        .attachments
        .iter()
        .enumerate()
        .filter(|(i, _)| !referenced.contains(i))
        .map(|(_, a)| attachments::attachment_out(a))
        .collect();
    let subject = m.subject.clone().unwrap_or_default();
    let dedupe_sender = p
        .sender
        .as_ref()
        .map(|a| a.email.clone())
        .or_else(|| p.sender_name.as_ref().map(|n| n.trim().to_lowercase()))
        .unwrap_or_default();
    let key = dedupe::key(
        m.internet_message_id.as_deref(),
        m.transport_headers.as_deref(),
        &dedupe_sender,
        m.client_submit_time,
        &subject,
        &body_text,
    );
    NormalizedMail {
        dedupe_key: key.key,
        dedupe_kind: key.kind,
        message_id: key.message_id,
        subject,
        sent_at: m.client_submit_time,
        received_at: m.delivery_time,
        sender: p.sender,
        sender_name: p.sender_name,
        sender_search: p.sender_search,
        recipients: p.recipients,
        recipients_search: p.recipients_search,
        to_preview: p.to_preview,
        body_source,
        body_html,
        body_text,
        snippet,
        attachments,
        inline_images,
    }
}
```


- [ ] **Step 47: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test -p outlooker-core --lib importer::normalize::tests`
Expected: every test of the module prints `ok`; `test result: ok`.

- [ ] **Step 48: Run the whole task test set**

Run: `cd src-tauri && cargo test -p outlooker-core importer::`
Expected: `test result: ok. 25 passed` and the acceptance test names of this task listed as `ok`.

Run: `grep -rnE "Builder::(new|default)" src-tauri/crates/outlooker-core/src/importer`
Expected: no output (exit 1).

- [ ] **Step 49: Commit**

Write the message file `scripts/tmp/r105-msg.txt` (Write tool; `scripts/tmp/` is gitignored, and the session hook blocks writes to `/tmp`) with: `feat(core): mail normalization - charset, RTF de-encapsulation, sanitizer, WebP, dedupe key (R1-05)`, a blank line, and the attribution line of the session.

```bash
git rev-parse --abbrev-ref HEAD
git add src-tauri/crates/outlooker-core/src/importer/mod.rs src-tauri/crates/outlooker-core/src/importer/types.rs src-tauri/crates/outlooker-core/src/importer/normalize src-tauri/crates/outlooker-core/src/importer/webp.rs src-tauri/crates/outlooker-core/src/importer/dedupe.rs src-tauri/crates/outlooker-core/tests/golden
git commit -F scripts/tmp/r105-msg.txt -- src-tauri/crates/outlooker-core/src/importer/mod.rs src-tauri/crates/outlooker-core/src/importer/types.rs src-tauri/crates/outlooker-core/src/importer/normalize src-tauri/crates/outlooker-core/src/importer/webp.rs src-tauri/crates/outlooker-core/src/importer/dedupe.rs src-tauri/crates/outlooker-core/tests/golden
git status --porcelain
```


## Task R2-10: Bulk writer, PST removal, 1M load generator, perf harness

**Files:** (`core/` = `src-tauri/crates/outlooker-core/`, `loadgen/` = `src-tauri/crates/outlooker-loadgen/`)
- Create/own: `core/src/importer/writer.rs`, `core/src/importer/remove.rs`
- Own (Round 2 owner, schema fixes requested by other tasks land here): `core/src/db/**`, `core/src/test_support/**`, `core/tests/db_schema.rs`
- Create: `core/tests/writer.rs`, `core/tests/removal.rs` (shared helpers live in `core/src/test_support/import_fixtures.rs` behind feature `test-support`)
- Modify: `core/src/test_support/mod.rs` — add `pub mod import_fixtures;` and create `core/src/test_support/import_fixtures.rs` (helpers for writer/removal/pipeline/search tests: `new_source`, `complete_job`, `mail`, `add_recipient`, `item`, `seed_mails`)
- Create/own: `loadgen/Cargo.toml` (replaces the kickoff file), `loadgen/src/main.rs`, `loadgen/src/lib.rs`, `loadgen/src/rng.rs`, `loadgen/src/corpus.rs`, `loadgen/src/generate.rs` (not `gen.rs`: `gen` is a reserved keyword in edition 2024), `loadgen/src/images.rs`, `loadgen/src/root.rs`, `loadgen/src/load.rs`, `loadgen/src/perf.rs`, `loadgen/data/words-en.txt`, `loadgen/data/words-zh.txt`, `loadgen/tests/smoke.rs`, `loadgen/tests/perf.rs`
- Requested from the manifest owner R2-13 (through the leader), not edited by this task: `src-tauri/Cargo.toml` gets `[profile.dev.package."*"] opt-level = 3` (dependencies optimized in dev builds; measured in the scratch crate: the loadgen unit tests take 30.7 s with unoptimized `image` / `webp` and 3.7 s with this profile; the smoke test of 10,000 messages finishes in 15 s with it)
- Writes at run time (not committed by the implementer): `docs/logs/loadtest-<date>.md`, `docs/logs/perf-<date>.md`

**Interfaces:**
- Consumes:
  - C1 `dto::error::AppError`, `dto::import::{ImportPhase, ImportProgress, ImportErrorStage, ImportErrorSeverity}`, `dto::search::RecipientKind`.
  - C2 is not used by the writer (progress goes out through the `on_commit` callback; R2-09 emits events).
  - C6/C7/C8 (planner-1): `db::Db::{app_pool, connect_app, connect_admin, data_root}`, `db::{connect_admin, migrate, pg_err, Pools, DbConn, RFC3339_UTC_FMT}`, `db::credentials::Credentials::{generate, save, load}`, `db::indexes::{drop_search_indexes, ensure_search_indexes}`, `paths::{Paths, dev_pg_dir}`, `lifecycle::pg::{PgController::{new, init, start, stop}, StopMode}`, `test_support::TestCluster::{start, admin, app, db}`.
  - R2-08 `lifecycle::move_dir::free_space(&Path) -> Result<u64, AppError>` for the disk guard (sysinfo 0.39.6 inside; the disk whose mount point is the longest prefix of the canonical path). An `Err` counts as 0 free bytes, so the guard fails closed. R2-08 must land `move_dir.rs` before this task's writer step (see "Consumes cross-check").
  - C9 (R1-05): `importer::types::*`, `importer::webp::{KnownHashes, ImageIn, ImageOut, convert, source_hash}`, `importer::dedupe::key`, `importer::normalize::{normalize, NormalizeCtx}`.
- Produces (C11, C12; consumers R2-09, loadgen, R2-12):

```rust
// core/src/importer/writer.rs
#[derive(Debug, Clone, PartialEq, Eq)]   // Clone + Debug needed by R2-09 ImportTuning
pub struct WriterLimits {
    pub max_items: usize,      // 1_000
    pub max_bytes: usize,      // 64 * 1024 * 1024 (sum of NormalizedItem staged bytes, see staged_bytes())
    pub max_wait: std::time::Duration,   // 1 s since the first item of the batch
    pub min_free_bytes: u64,   // 2 * 1024^3 on the data-root volume
}
impl Default for WriterLimits;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkpoint {
    pub pst_id: i64,
    pub job_id: i64,
    pub phase: ImportPhase,
    pub items_total: Option<i64>,
    pub cursor_seq: i64,
    pub file_size: i64,
    pub file_mtime: std::time::SystemTime,
    pub stored: i64,
    pub duplicates: i64,
    pub errors: i64,
    pub skipped_non_mail: i64,
    pub images_converted: i64,
    pub batches_committed: i64,
}
impl Checkpoint {
    pub fn items_done(&self) -> i64;   // stored + duplicates + errors + skipped_non_mail
    /// Row of import_checkpoints, or None.
    pub async fn load(client: &tokio_postgres::Client, pst_id: i64) -> Result<Option<Checkpoint>, AppError>;
    /// Job start (Part 04 §5.9 step 1): keeps cursor + counters when (file_size, file_mtime) equal the stored
    /// identity, else resets them to 0. Sets job_id and phase Scanning. Returns (checkpoint, was_reset).
    pub async fn begin(client: &tokio_postgres::Client, pst_id: i64, job_id: i64, file_size: i64,
                       file_mtime: std::time::SystemTime) -> Result<(Checkpoint, bool), AppError>;
    pub async fn set_phase(client: &tokio_postgres::Client, pst_id: i64, phase: ImportPhase,
                           items_total: Option<i64>) -> Result<(), AppError>;
}

/// Upsert the scanned folder tree of one PST in parents-first order (one transaction).
pub async fn upsert_folders(client: &mut tokio_postgres::Client, pst_id: i64, folders: &[FolderIn]) -> Result<(), AppError>;
/// Every image_blobs.source_hash.
pub async fn load_known_hashes(client: &tokio_postgres::Client) -> Result<KnownHashes, AppError>;
/// Part 04 §6.5, run after every job on an outlooker_app connection outside a transaction.
pub const AFTER_JOB_VACUUM: &str = "VACUUM (ANALYZE) messages, message_recipients, message_occurrences, attachments, inline_images, image_blobs, addresses";

pub struct WriterConfig {
    pub pst_id: i64,
    pub data_root: std::path::PathBuf,   // disk guard volume
    pub start: Checkpoint,               // from Checkpoint::begin / load
    pub limits: WriterLimits,
    pub known: std::sync::Arc<KnownHashes>,
}

pub type OnCommit = Box<dyn Fn(&Checkpoint) + Send + Sync>;

pub struct Writer;
impl Writer {
    /// Takes ownership of a dedicated outlooker_app connection (Db::connect_app), runs
    /// `SET synchronous_commit = off`, creates the session temp staging tables, and spawns the writer task.
    pub async fn spawn(client: tokio_postgres::Client, cfg: WriterConfig, on_commit: OnCommit) -> Result<WriterHandle, AppError>;
}

pub struct WriterHandle { /* tokio::sync::mpsc::Sender<NormalizedItem> (capacity 256) + JoinHandle */ }
impl WriterHandle {
    /// A clone of the input channel; rayon threads use `blocking_send`. A send error means the writer stopped:
    /// call `finish` to get its error.
    pub fn sender(&self) -> tokio::sync::mpsc::Sender<NormalizedItem>;
    pub async fn submit(&self, item: NormalizedItem) -> Result<(), AppError>;
    /// Closes the input, commits the gap-free prefix still buffered, returns the final checkpoint.
    /// Items after a gap are discarded (they are re-read on resume).
    pub async fn finish(self) -> Result<Checkpoint, AppError>;
    /// Stops at once: no further commit. The open batch transaction (if any) rolls back with the connection.
    pub fn abort(self);
}

/// Error mapping used by writer, remove and R2-09: connection closed / I/O → AppError::DbNotReady;
/// SQLSTATE 53100 → FileAccess{path: data_root, reason: "insufficientSpace"}; other → Internal{detail}.
pub fn map_db_error(e: tokio_postgres::Error, data_root: &std::path::Path) -> AppError;

/// Session setup of the writer connection (synchronous_commit off + the seven staging tables); called only by
/// Writer::spawn, public for the synchronous_commit test.
pub async fn prepare_session(client: &tokio_postgres::Client) -> Result<(), tokio_postgres::Error>;
/// Bytes an item adds to the open batch (the 64 MB cut).
pub fn staged_bytes(item: &NormalizedItem) -> usize;
/// The value PostgreSQL stores for a SystemTime in timestamptz (whole microseconds); R2-09 compares file mtimes
/// with it (enqueue, Checkpoint::begin).
pub fn truncate_to_micros(t: std::time::SystemTime) -> std::time::SystemTime;
/// DB text of the C1 enums ("scanning" …, "open" …, "item_skipped" …); R2-09 writes job-level error rows with them.
pub fn phase_db(p: ImportPhase) -> &'static str;
pub fn stage_db(s: ImportErrorStage) -> &'static str;
pub fn severity_db(s: ImportErrorSeverity) -> &'static str;
```

```rust
// core/src/importer/remove.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RemoveStats { pub occurrences_deleted: u64, pub messages_deleted: u64 }

/// Part 04 §4.3 removal body for one PST: 5,000-occurrence chunks (each its own transaction), then one
/// transaction with the orphan cleanup and `DELETE FROM pst_sources`, then AFTER_JOB_VACUUM.
/// Idempotent: a second run (or a run after a crash mid-removal) finishes the job; on an already deleted PST it
/// returns RemoveStats::default().
pub async fn run(client: &tokio_postgres::Client, pst_id: i64) -> Result<RemoveStats, AppError>;
/// Orphan blobs + orphan addresses (Part 04 §4.3 second block, first two statements). Returns (blobs, addresses).
/// Also used by the maintenance `vacuum` task (R2-12).
pub async fn delete_orphans(client: &tokio_postgres::Client) -> Result<(u64, u64), AppError>;
```

```rust
// core/src/test_support/import_fixtures.rs  (feature test-support; consumers: writer, removal, import_pipeline, search, sql_console tests, loadgen)
/// INSERT pst_sources + one import job (state 'running', started_at now()) + one folder (pst_folder_id 1, path "Inbox").
/// Returns (pst_id, job_id). Only one job may run per database (import_jobs_one_running): complete_job first.
pub async fn new_source(client: &tokio_postgres::Client, path: &str) -> (i64, i64);
pub async fn complete_job(client: &tokio_postgres::Client, job_id: i64);            // state 'completed', finished_at now()
/// A NormalizedMail with deterministic defaults; `message_id: None` → content-hash key from the other fields.
pub fn mail(subject: &str, body_text: &str, sender: &str, message_id: Option<&str>) -> NormalizedMail;
/// Appends a recipient and keeps recipients_search / to_preview consistent.
pub fn add_recipient(m: &mut NormalizedMail, kind: RecipientKind, name: Option<&str>, email: &str);
pub fn item(seq: i64, pst_item_id: i64, mail: NormalizedMail) -> NormalizedItem;   // pst_folder_id 1
/// Runs one writer over `items` for a new source (new_source + Checkpoint::begin + Writer::spawn on
/// db.connect_app() + finish + job 'completed'). Returns pst_id.
pub async fn seed_mails(db: &Db, path: &str, items: Vec<NormalizedItem>) -> i64;
```

  Loadgen (`outlooker-loadgen` crate, lib + bin):

```rust
// loadgen/src/lib.rs
pub mod corpus; pub mod generate; pub mod images; pub mod load; pub mod perf; pub mod rng; pub mod root;
// loadgen/src/generate.rs
pub struct GenConfig { pub messages: u64, pub sources: u32, pub seed: u64, pub msgid_dup_rate: f64, pub hash_dup_rate: f64,
                       pub image_rate: f64, pub image_pool: u32, pub logo_pool: u32, pub max_image_side: u32, pub e2e_message: bool }
impl GenConfig {
    pub fn full() -> GenConfig;              // 1_000_000, 10, 42, 0.03, 0.005, 0.30, 50_000, 200, 1600, false
    pub fn smoke(messages: u64) -> GenConfig; // messages, 10, 7, 0.03, 0.005, 0.30, 200, 20, 400, false
    pub fn seed(messages: u64) -> GenConfig;  // messages, 2, 11, 0.03, 0.005, 0.10, 100, 10, 400, true
    pub fn per_source(&self, s: u32) -> u64;  // messages / sources, the last source takes the remainder
}
pub enum Slot { Fresh, MessageIdDup { s: u32, i: u64 }, HashDup { s: u32, i: u64 } }   // a duplicate regenerates fresh slot (s, i)
pub struct Generator { pub cfg: GenConfig, pub corpus: Corpus, pub images: ImagePool }
impl Generator {
    pub fn new(cfg: GenConfig) -> Generator;
    pub fn slot(&self, s: u32, i: u64) -> Slot;
    pub fn item(&self, s: u32, i: u64, known: &KnownHashes) -> NormalizedItem;   // pure function of (seed, s, i)
}
// loadgen/src/root.rs — a persistent data root (no tempdir), built only from public foundation APIs
pub struct LibraryRoot { pub paths: Paths, pub port: u16, pub creds: Credentials, pub db: Db }
impl LibraryRoot {
    pub async fn create(data_root: &std::path::Path) -> Result<LibraryRoot, AppError>; // marker + credentials + init + start + migrate + pools
    pub async fn open(data_root: &std::path::Path) -> Result<LibraryRoot, AppError>;   // start + credentials + migrate + pools
    pub async fn stop(self) -> Result<(), AppError>;                                     // Db unset, pg_ctl stop -m fast
}
// loadgen/src/load.rs
pub struct LoadReport { pub messages_submitted: u64, pub duplicates_planned: u64, pub messages_stored: i64, pub occurrences: i64,
                       pub write_elapsed: std::time::Duration, pub index_elapsed: std::time::Duration }
/// Writes `sources` of `cfg` through the real writer, one writer per source; `defer_indexes` = drop the six search
/// indexes before and ensure them after (index_elapsed).
pub async fn load_sources(db: &Db, cfg: &GenConfig, sources: std::ops::Range<u32>, defer_indexes: bool) -> Result<LoadReport, AppError>;
/// All sources with deferred indexes.
pub async fn load(db: &Db, cfg: &GenConfig) -> Result<LoadReport, AppError>;
/// Library entry used by main.rs and tests: create the root, load GenConfig::seed(n), stop.
pub async fn seed(data_root: &std::path::Path, messages: u64) -> Result<LoadReport, AppError>;
// main.rs CLI: `outlooker-loadgen seed --data-root <dir> --messages <n>`
```

  Perf helpers (`loadgen/src/perf.rs`): `repo_root`, `perf_root`, `open_library`, `record(Log, gate, threshold, measured, pass, details)`, `percentile`, `canned_queries` (the 30 queries), `explain` (EXPLAIN (ANALYZE, BUFFERS) of phase A and phase B).

**Behavior:**
- Writer task (Part 04 §6.1–§6.3): reorder buffer `BTreeMap<i64, NormalizedItem>` keyed by `seq`; `next = start.cursor_seq + 1`; only the gap-free prefix moves into the open batch. A batch is cut when it holds `max_items` items, or `max_bytes` staged bytes, or `max_wait` has passed since its first item (deadline via `tokio::time::timeout_at`), or the input closes. Per batch: disk guard first (`free_space(data_root)`, an error counting as 0, `< min_free_bytes` → return `FileAccess{path, "insufficientSpace"}` before BEGIN, nothing committed); then in one transaction: binary COPY into the seven staging tables, the statements of §6.3 in order (addresses, messages upsert into `stage_new_ids`, recipients, attachments, image blobs, inline images, occurrences, errors), checkpoint upsert (`cursor_seq` = highest seq of the batch, counters += batch deltas, `batches_committed + 1`), COMMIT; then the hashes of newly inserted blobs go into `KnownHashes`, then `on_commit(&checkpoint)`.
- Batch counters: `stored` = rows of `stage_new_ids`; `duplicates` = mail items − stored; `errors` = `Skipped` items; `skipped_non_mail` = `NonMail` items; `images_converted` = rows inserted into `image_blobs`.
- Children of a dedupe key that appears twice in one batch come only from the lowest `seq` (join to `(SELECT dedupe_key, min(seq) FROM stage_messages GROUP BY dedupe_key)`, a hash join; a correlated `min(seq)` subquery per child row made the scratch 10,000-message load 3× slower), so no primary-key conflict inside a batch. Addresses are inserted only for winner rows whose key is not yet in `messages`, so a duplicate never leaves an orphan address.
- Occurrences: every mail item of the batch gets `(pst_id, pst_item_id, message_id via dedupe_key, folder_id via (pst_id, pst_folder_id))`, `ON CONFLICT (pst_id, pst_item_id) DO NOTHING`.
- `synchronous_commit = off` only on the writer connection (`SET` at session level on the dedicated connection).
- Error mapping (`map_db_error`): `e.is_closed()` or no SQLSTATE (I/O) → `DbNotReady`; `53100` → `FileAccess{insufficientSpace}`; other → `Internal{detail: e.to_string()}`.
- Removal (Part 04 §4.3): loop the chunk statement (wrapped so it returns `(occurrences deleted, messages deleted)`) until it deletes no occurrence; then one transaction: `delete_orphans` + `DELETE FROM pst_sources WHERE id = $1`; then `AFTER_JOB_VACUUM`. No job-state change (R2-09 owns `import_jobs`); the job row is removed by the cascade.
- Loadgen (Part 04 §12.3): deterministic xorshift PRNG (no `rand` crate in Part 01). 10 sources × 100k; 3% of items reuse an earlier item's Message-ID in another source, 0.5% reuse an earlier content-hash item (same sender, time, subject, body; body line endings changed to CRLF) in another source; 60% English (Zipf s = 1.0 over `words-en.txt`), 30% Traditional Chinese (words from `words-zh.txt`, joined without spaces), 10% mixed; body length log-normal median 2 KB, p99 60 KB (capped at 64 KB); 20,000 senders; 1–10 recipients, 1% of mails with 300; dates uniform over the 10 years before 2026-01-01; 30% of mails with 1–3 images drawn from 50,000 generated PNG/JPEG gradient/noise images (50–1,600 px) plus 200 logos, encoded through `webp::convert` with quality 80 and a shared `KnownHashes`. Each source: `new_source`-style rows (one folder per year, `Inbox/<year>`), writer per source, deferred indexes (drop before the first source, `ensure_search_indexes` after the last).
- `seed --data-root <dir> --messages <n>` (n up to at least 120,000): `LibraryRoot::create(dir)` (marker of foundation §0.3, credentials, `PgController::init`, start, migrate, pools), `load(.., GenConfig::seed(n))` with 2 sources, `LibraryRoot::stop`. The seeded library always contains one fixed message: subject `E2E 報價單 inline`, body text containing `報價單`, HTML body with one inline image (`outlooker-inline:0` → a stored WebP blob); every generated sender address ends in `@loadgen.example`, so the search `loadgen` matches every message (R2-14 E2E needs). The result is a complete data root that the app adopts and that `LibraryRoot::open(dir)` starts (C7).
- Perf harness (`loadgen/tests/perf.rs`, all `#[ignore]`, names start with `perf_`; library at `$OUTLOOKER_PERF_ROOT` default `src-tauri/target/perf-root`): `perf_generate` builds the 1M library and records the writer throughput in `<root>/perf-generate.json`; then one test per gate. Each test appends one result row (`gate | threshold | measured | PASS/FAIL | commit`) to `docs/logs/loadtest-<YYYY-MM-DD>.md` (Part 04 §12.3 rows) or `docs/logs/perf-<YYYY-MM-DD>.md` (Part 02 §9 rows), with `EXPLAIN (ANALYZE, BUFFERS)` of each canned query in the search tests, and then asserts its threshold. Thresholds are constants copied from the spec and never changed after a run.

**Constraints:**
- SQL only from constants; values bound as parameters or COPY fields.
- Writer: exactly one connection; never uses a pool; never runs DDL on permanent tables (index DDL is R2-09's job on an admin connection).
- Removal runs on an `outlooker_app` connection (`DELETE` + `MAINTAIN` privileges, Part 04 §1).
- Loadgen depends on `outlooker-core` with feature `test-support`, `tokio`, `tokio-postgres`, `image`, `rayon`, `serde_json` (dev: `tempfile`) — all Part 01 crates at Part 01 versions; no new crate.
- Perf tests never run in the normal `cargo test` (all `#[ignore]`), and the smoke test (10,000 messages) needs no images over 400 px so it stays fast.

**Acceptance criteria:**
- [ ] `cargo test -p outlooker-core --features test-support --test writer --test removal` exits 0 with these tests `ok`: `writer::out_of_order_checkpoint_is_contiguous` (items submitted out of `seq` order are stored with the checkpoint at the highest contiguous `seq`), `writer::message_id_duplicate_adds_occurrence` (a duplicate Message-ID from a second source adds one occurrence and no message), `writer::content_hash_crlf_vs_lf` (two sources, no Message-ID, equal sender/time/subject/body with CRLF vs LF → 1 message, 2 occurrences), `writer::batch_cut_by_items`, `writer::batch_cut_by_bytes`, `writer::batch_cut_by_time` (overridden limits: 3 items, 10 KB, 200 ms; `batches_committed` proves the cut; each test also asserts the production default 1,000 items / 64 MB / 1 s), `writer::synchronous_commit_off_only_on_writer` (`SHOW synchronous_commit` = `off` on the writer connection, `on` on an app-pool connection), `writer::error_rolls_back_data_and_checkpoint` (a forced error mid-batch: no row and no checkpoint advance), `writer::disk_guard_blocks_commit` (`min_free_bytes = u64::MAX` → `FileAccess{reason:"insufficientSpace"}`, nothing committed), `writer::same_key_twice_in_one_batch` (two items with one Message-ID in one batch: 1 message, its children once, 2 occurrences), `removal::keeps_shared_deletes_rest` (shared messages kept with `first_pst_id` NULL, non-shared deleted, orphan blobs and addresses deleted), `removal::second_run_is_noop`.
- [ ] `cargo test -p outlooker-loadgen` (non-ignored smoke: 10,000 messages) exits 0 and prints `messages_stored=<n> occurrences=<m>` with `n` and `m` inside the bounds asserted by the test (derived from the configured duplicate rates ± 1 percentage point).
- [ ] `cargo run -p outlooker-loadgen -- seed --data-root <tmp> --messages 2000` exits 0, and `loadgen/tests/smoke.rs::seeded_root_starts` (runs the same `seed` function into a temp dir, stops it, then `LibraryRoot::open`) counts more than 1,900 messages, finds the message with subject `E2E 報價單 inline` with exactly one `inline_images` row, and finds every `addresses.email` of a sender ending in `@loadgen.example`.
- [ ] `cargo test -p outlooker-loadgen --release -- --ignored --list` lists `perf_generate` plus exactly these 13 gate tests: `perf_writer_throughput`, `perf_e2e_import`, `perf_index_rebuild`, `perf_db_size`, `perf_search_first_page_warm`, `perf_search_first_page_cold`, `perf_short_query`, `perf_search_count`, `perf_remove_source`, `perf_incremental_import`, `perf_search_next_page`, `perf_mail_get_first_image`, `perf_search_first_page_p50`.
- [ ] `cargo test -p outlooker-core --features test-support --test db_schema` still exits 0 (Round 1 regression of the owned `db/**`).

### R2-10 — Stage 2 steps

All code below was compiled with rustc 1.99.1 (edition 2024) and run against PostgreSQL 18.6 test clusters in a scratch workspace with the exact Part 01 crate versions before this plan was written: writer 10 tests, removal 2, loadgen unit 7, loadgen smoke 2, all `ok`; the 14 perf tests compiled and 12 of them were executed against a 3,000-message scratch library (the two that need the 1M library or a real PST were checked for their failure message). Mutation checks turned the matching tests red. Module paths are the production ones. All `cargo` commands run in `src-tauri/`.

- [ ] **Step 1: Check the preconditions**

Run:

```bash
grep -n "pub fn free_space" src-tauri/crates/outlooker-core/src/lifecycle/move_dir.rs
grep -nE "^pub mod (writer|remove);" src-tauri/crates/outlooker-core/src/importer/mod.rs
grep -n "opt-level = 3" src-tauri/Cargo.toml
ls src-tauri/resources/postgresql/bin/postgres
```

Expected: one line each. `free_space` comes from R2-08 (lands before this step); the two `mod` lines from the kickoff / R1-05; the `[profile.dev.package."*"] opt-level = 3` line from the manifest owner R2-13 (requested through the leader; without it the loadgen tests still pass but take minutes in a debug build); PostgreSQL from `node scripts/fetch-postgres.mjs`. If any line is missing, stop and report to the leader.

- [ ] **Step 2: Declare the fixtures module in `src-tauri/crates/outlooker-core/src/test_support/mod.rs`**

Add this line directly after the `//!` doc comment line at the top of the file:

```rust
pub mod import_fixtures;
```

Create `src-tauri/crates/outlooker-core/src/test_support/import_fixtures.rs` empty for now.

- [ ] **Step 3: Write the failing writer tests `src-tauri/crates/outlooker-core/tests/writer.rs`**

```rust
//! Bulk writer tests (spec Part 04 §4.2, §6.1–§6.3) on a real PostgreSQL test cluster.
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use outlooker_core::dto::error::AppError;
use outlooker_core::importer::types::{AttachmentOut, NormalizedItem};
use outlooker_core::importer::webp::KnownHashes;
use outlooker_core::importer::writer::{prepare_session, Checkpoint, Writer, WriterConfig, WriterHandle, WriterLimits};
use outlooker_core::test_support::import_fixtures::{complete_job, item, mail, new_source, seed_mails};
use outlooker_core::test_support::TestCluster;
use tokio::sync::mpsc;

async fn count(c: &tokio_postgres::Client, sql: &str) -> i64 {
    c.query_one(sql, &[]).await.unwrap().get(0)
}

/// New source + checkpoint + writer with `limits`; commits are forwarded on the returned channel.
async fn start_writer(tc: &TestCluster, path: &str, limits: WriterLimits) -> (WriterHandle, mpsc::UnboundedReceiver<Checkpoint>, i64, i64) {
    let app = tc.app().await;
    let (pst_id, job_id) = new_source(&app, path).await;
    let (start, _) = Checkpoint::begin(&app, pst_id, job_id, 1, SystemTime::now()).await.unwrap();
    let cfg = WriterConfig { pst_id, data_root: tc.db.data_root().unwrap(), start, limits, known: Arc::new(KnownHashes::new()) };
    let (tx, rx) = mpsc::unbounded_channel();
    let w = Writer::spawn(tc.app().await, cfg, Box::new(move |cp: &Checkpoint| { let _ = tx.send(cp.clone()); })).await.unwrap();
    (w, rx, pst_id, job_id)
}

fn mails(n: i64, body: &str) -> Vec<NormalizedItem> {
    (1..=n).map(|i| item(i, 100 + i, mail(&format!("subject {i}"), body, "a@x.example", Some(&format!("<m{i}@x>"))))).collect()
}

#[tokio::test]
async fn out_of_order_checkpoint_is_contiguous() {
    let tc = TestCluster::start().await;
    let (w, _rx, pst_id, _) = start_writer(&tc, "/p/a.pst", WriterLimits::default()).await;
    let mut items: Vec<Option<NormalizedItem>> = mails(5, "body").into_iter().map(Some).collect();
    for i in [2, 0, 4, 1] {
        w.submit(items[i].take().unwrap()).await.unwrap(); // seq 3, 1, 5, 2; seq 4 never arrives (a gap)
    }
    let cp = w.finish().await.unwrap();
    assert_eq!(cp.cursor_seq, 3);
    assert_eq!(cp.stored, 3);
    let a = tc.admin().await;
    assert_eq!(count(&a, "SELECT count(*) FROM messages").await, 3);
    assert_eq!(Checkpoint::load(&a, pst_id).await.unwrap().unwrap().cursor_seq, 3);
}

#[tokio::test]
async fn message_id_duplicate_adds_occurrence() {
    let tc = TestCluster::start().await;
    seed_mails(&tc.db, "/p/a.pst", vec![item(1, 11, mail("hello", "one", "a@x.example", Some("<id-1@x>")))]).await;
    let (w, _rx, _, _) = start_writer(&tc, "/p/b.pst", WriterLimits::default()).await;
    w.submit(item(1, 21, mail("hello again", "other body", "b@x.example", Some(" id-1@x ")))).await.unwrap();
    let cp = w.finish().await.unwrap();
    assert_eq!((cp.stored, cp.duplicates), (0, 1));
    let a = tc.admin().await;
    assert_eq!(count(&a, "SELECT count(*) FROM messages").await, 1);
    assert_eq!(count(&a, "SELECT count(*) FROM message_occurrences").await, 2);
}

#[tokio::test]
async fn content_hash_crlf_vs_lf() {
    let tc = TestCluster::start().await;
    seed_mails(&tc.db, "/p/a.pst", vec![item(1, 11, mail("Quote", "line one\nline two\n", "a@x.example", None))]).await;
    seed_mails(&tc.db, "/p/b.pst", vec![item(1, 11, mail("Quote", "line one\r\nline two\r\n", "a@x.example", None))]).await;
    let a = tc.admin().await;
    assert_eq!(count(&a, "SELECT count(*) FROM messages").await, 1);
    assert_eq!(count(&a, "SELECT count(*) FROM message_occurrences").await, 2);
    assert_eq!(count(&a, "SELECT count(*) FROM messages WHERE dedupe_kind = 'content_hash'").await, 1);
}

#[tokio::test]
async fn batch_cut_by_items() {
    assert_eq!(WriterLimits::default().max_items, 1_000);
    let tc = TestCluster::start().await;
    let limits = WriterLimits { max_items: 3, ..WriterLimits::default() };
    let (w, _rx, _, _) = start_writer(&tc, "/p/a.pst", limits).await;
    for it in mails(7, "body") {
        w.submit(it).await.unwrap();
    }
    let cp = w.finish().await.unwrap();
    assert_eq!((cp.batches_committed, cp.cursor_seq, cp.stored), (3, 7, 7)); // 3 + 3 + 1
}

#[tokio::test]
async fn batch_cut_by_bytes() {
    assert_eq!(WriterLimits::default().max_bytes, 64 * 1024 * 1024);
    let tc = TestCluster::start().await;
    let limits = WriterLimits { max_bytes: 10 * 1024, ..WriterLimits::default() };
    let (w, _rx, _, _) = start_writer(&tc, "/p/a.pst", limits).await;
    for it in mails(4, &"x".repeat(6 * 1024)) {
        w.submit(it).await.unwrap();
    }
    let cp = w.finish().await.unwrap();
    assert_eq!((cp.batches_committed, cp.stored), (2, 4)); // two 6 KB items reach 10 KB
}

#[tokio::test]
async fn batch_cut_by_time() {
    assert_eq!(WriterLimits::default().max_wait, Duration::from_secs(1));
    let tc = TestCluster::start().await;
    let limits = WriterLimits { max_wait: Duration::from_millis(200), ..WriterLimits::default() };
    let (w, mut rx, _, _) = start_writer(&tc, "/p/a.pst", limits).await;
    let mut items = mails(2, "body").into_iter();
    w.submit(items.next().unwrap()).await.unwrap();
    // The commit arrives through on_commit while the writer still waits for input: only the deadline can cut it.
    let cp = tokio::time::timeout(Duration::from_secs(10), rx.recv()).await.expect("time cut").unwrap();
    assert_eq!((cp.batches_committed, cp.cursor_seq), (1, 1));
    w.submit(items.next().unwrap()).await.unwrap();
    let cp = w.finish().await.unwrap();
    assert_eq!((cp.batches_committed, cp.cursor_seq), (2, 2));
}

#[tokio::test]
async fn synchronous_commit_off_only_on_writer() {
    let tc = TestCluster::start().await;
    let writer_conn = tc.db.connect_app().await.unwrap();
    prepare_session(&writer_conn).await.unwrap(); // the session setup Writer::spawn runs
    assert_eq!(show_sync(&writer_conn).await, "off");
    seed_mails(&tc.db, "/p/a.pst", mails(2, "body")).await; // a real writer ran
    let pooled = tc.db.app_pool().unwrap().get().await.unwrap();
    assert_eq!(show_sync(&pooled).await, "on");
}

async fn show_sync(c: &tokio_postgres::Client) -> String {
    c.query_one("SHOW synchronous_commit", &[]).await.unwrap().get(0)
}

#[tokio::test]
async fn error_rolls_back_data_and_checkpoint() {
    let tc = TestCluster::start().await;
    let (w, _rx, pst_id, _) = start_writer(&tc, "/p/a.pst", WriterLimits::default()).await;
    let mut items = mails(3, "body");
    let bad = attachment("");
    if let outlooker_core::importer::types::Outcome::Mail(m) = &mut items[2].outcome {
        m.attachments.push(bad); // file_name '' violates the attachments CHECK (length 1..=1024)
    }
    for it in items {
        w.submit(it).await.unwrap();
    }
    let err = w.finish().await.unwrap_err();
    assert!(matches!(&err, AppError::Internal { detail } if detail.starts_with("23514")), "{err:?}");
    let a = tc.admin().await;
    assert_eq!(count(&a, "SELECT count(*) FROM messages").await, 0);
    assert_eq!(count(&a, "SELECT count(*) FROM message_occurrences").await, 0);
    let cp = Checkpoint::load(&a, pst_id).await.unwrap().unwrap();
    assert_eq!((cp.cursor_seq, cp.batches_committed), (0, 0));
}

#[tokio::test]
async fn disk_guard_blocks_commit() {
    assert_eq!(WriterLimits::default().min_free_bytes, 2 * 1024 * 1024 * 1024);
    let tc = TestCluster::start().await;
    let limits = WriterLimits { min_free_bytes: u64::MAX, ..WriterLimits::default() };
    let (w, _rx, pst_id, job_id) = start_writer(&tc, "/p/a.pst", limits).await;
    for it in mails(2, "body") {
        w.submit(it).await.unwrap();
    }
    let err = w.finish().await.unwrap_err();
    assert_eq!(err, AppError::FileAccess { path: tc.db.data_root().unwrap().display().to_string(), reason: "insufficientSpace".into() });
    let a = tc.admin().await;
    assert_eq!(count(&a, "SELECT count(*) FROM messages").await, 0);
    assert_eq!(Checkpoint::load(&a, pst_id).await.unwrap().unwrap().cursor_seq, 0);
    complete_job(&a, job_id).await;
}

#[tokio::test]
async fn same_key_twice_in_one_batch() {
    let tc = TestCluster::start().await;
    let (w, _rx, _, _) = start_writer(&tc, "/p/a.pst", WriterLimits::default()).await;
    for (seq, subject) in [(1, "first copy"), (2, "second copy")] {
        let mut m = mail(subject, "body", "a@x.example", Some("<same@x>"));
        outlooker_core::test_support::import_fixtures::add_recipient(&mut m, outlooker_core::dto::search::RecipientKind::To, None, "r@x.example");
        m.attachments.push(attachment("a.txt"));
        w.submit(item(seq, 100 + seq, m)).await.unwrap();
    }
    let cp = w.finish().await.unwrap();
    assert_eq!((cp.batches_committed, cp.stored, cp.duplicates), (1, 1, 1));
    let a = tc.admin().await;
    assert_eq!(a.query_one("SELECT subject FROM messages", &[]).await.unwrap().get::<_, String>(0), "first copy");
    assert_eq!(count(&a, "SELECT count(*) FROM message_recipients").await, 1);
    assert_eq!(count(&a, "SELECT count(*) FROM attachments").await, 1);
    assert_eq!(count(&a, "SELECT count(*) FROM message_occurrences").await, 2);
}

fn attachment(file_name: &str) -> AttachmentOut {
    AttachmentOut { attachment_index: 0, file_name: file_name.into(), size_bytes: 1, mime_type: "text/plain".into(), attach_method: 1 }
}
```


- [ ] **Step 4: Run the writer tests to verify they fail**

Run: `cd src-tauri && cargo test -p outlooker-core --features test-support --test writer`
Expected: compile error (unresolved imports `outlooker_core::importer::writer::{prepare_session, Checkpoint, Writer, …}` and `outlooker_core::test_support::import_fixtures::{…}`).

- [ ] **Step 5: Create `src-tauri/crates/outlooker-core/src/test_support/import_fixtures.rs`**

```rust
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
```


- [ ] **Step 6: Implement `src-tauri/crates/outlooker-core/src/importer/writer.rs`**

```rust
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

use crate::dto::error::AppError;
use crate::dto::import::{ImportErrorSeverity, ImportErrorStage, ImportPhase};
use crate::dto::search::RecipientKind;
use crate::importer::types::{FolderIn, NormalizedItem, Outcome};
use crate::importer::webp::KnownHashes;
use crate::lifecycle::move_dir::free_space;

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

pub fn phase_db(p: ImportPhase) -> &'static str {
    match p {
        ImportPhase::Scanning => "scanning",
        ImportPhase::Importing => "importing",
        ImportPhase::Finalizing => "finalizing",
    }
}

pub fn stage_db(s: ImportErrorStage) -> &'static str {
    match s {
        ImportErrorStage::Open => "open",
        ImportErrorStage::Scan => "scan",
        ImportErrorStage::Parse => "parse",
        ImportErrorStage::Html => "html",
        ImportErrorStage::Image => "image",
        ImportErrorStage::Store => "store",
    }
}

pub fn severity_db(s: ImportErrorSeverity) -> &'static str {
    match s {
        ImportErrorSeverity::ItemSkipped => "item_skipped",
        ImportErrorSeverity::Degraded => "degraded",
        ImportErrorSeverity::JobFailed => "job_failed",
    }
}

fn kind_db(k: RecipientKind) -> &'static str {
    match k {
        RecipientKind::To => "to",
        RecipientKind::Cc => "cc",
        RecipientKind::Bcc => "bcc",
    }
}

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
    let phase = match r.get::<_, &str>(2) {
        "scanning" => ImportPhase::Scanning,
        "importing" => ImportPhase::Importing,
        "finalizing" => ImportPhase::Finalizing,
        other => return Err(AppError::internal(format!("unknown checkpoint phase {other}"))),
    };
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

fn pg(e: tokio_postgres::Error) -> AppError {
    map_db_error(e, Path::new(""))
}

impl Checkpoint {
    pub fn items_done(&self) -> i64 {
        self.stored + self.duplicates + self.errors + self.skipped_non_mail
    }

    pub async fn load(client: &Client, pst_id: i64) -> Result<Option<Checkpoint>, AppError> {
        match client.query_opt(CP_SELECT, &[&pst_id]).await.map_err(pg)? {
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
            let r = client.query_one(CP_KEEP, &[&pst_id, &job_id]).await.map_err(pg)?;
            return Ok((cp_from_row(&r)?, false));
        }
        let r = client.query_one(CP_RESET, &[&pst_id, &job_id, &file_size, &mtime]).await.map_err(pg)?;
        Ok((cp_from_row(&r)?, old.is_some()))
    }

    /// `items_total: None` keeps the stored value.
    pub async fn set_phase(client: &Client, pst_id: i64, phase: ImportPhase, items_total: Option<i64>) -> Result<(), AppError> {
        client.execute(CP_SET_PHASE, &[&pst_id, &phase_db(phase), &items_total]).await.map_err(pg)?;
        Ok(())
    }
}

/// Upsert the scanned folder tree of one PST in one transaction. `folders` must list parents before children
/// (the scan order of the reader).
pub async fn upsert_folders(client: &mut Client, pst_id: i64, folders: &[FolderIn]) -> Result<(), AppError> {
    let tx = client.transaction().await.map_err(pg)?;
    let stmt = tx
        .prepare(
            "INSERT INTO folders (pst_id, pst_folder_id, parent_id, name, path) \
             VALUES ($1, $2, (SELECT id FROM folders WHERE pst_id = $1 AND pst_folder_id = $3), $4, $5) \
             ON CONFLICT (pst_id, pst_folder_id) DO UPDATE SET parent_id = EXCLUDED.parent_id, name = EXCLUDED.name, path = EXCLUDED.path",
        )
        .await
        .map_err(pg)?;
    for f in folders {
        tx.execute(&stmt, &[&pst_id, &f.pst_folder_id, &f.parent_pst_folder_id, &f.name, &f.path]).await.map_err(pg)?;
    }
    tx.commit().await.map_err(pg)
}

pub async fn load_known_hashes(client: &Client) -> Result<KnownHashes, AppError> {
    let rows = client.query("SELECT source_hash FROM image_blobs", &[]).await.map_err(pg)?;
    let known = KnownHashes::new();
    known.insert_all(rows.iter().filter_map(|r| <[u8; 32]>::try_from(r.get::<_, &[u8]>(0)).ok()));
    Ok(known)
}

/// Spec Part 04 §6.5, run after every job on an outlooker_app connection outside a transaction.
pub const AFTER_JOB_VACUUM: &str =
    "VACUUM (ANALYZE) messages, message_recipients, message_occurrences, attachments, inline_images, image_blobs, addresses";

/// Connection closed / I/O → `DbNotReady`; SQLSTATE 53100 (disk full) → `FileAccess{insufficientSpace}`;
/// any other database error → `Internal`.
pub fn map_db_error(e: tokio_postgres::Error, data_root: &Path) -> AppError {
    match e.as_db_error() {
        Some(db) if db.code().code() == "53100" => AppError::file_access(data_root, "insufficientSpace"),
        Some(db) => AppError::internal(format!("{}: {}", db.code().code(), db.message())),
        None if e.is_closed() => AppError::DbNotReady,
        None if std::error::Error::source(&e).is_some_and(|s| s.is::<std::io::Error>()) => AppError::DbNotReady,
        None => AppError::internal(e.to_string()),
    }
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
/// tables (§6.1). Called only by `Writer::spawn`; public so the test can check the session setting.
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
        Outcome::NonMail | Outcome::Skipped => 64,
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

pub struct Writer;

pub struct WriterHandle {
    tx: mpsc::Sender<NormalizedItem>,
    join: tokio::task::JoinHandle<Result<Checkpoint, AppError>>,
}

impl Writer {
    pub async fn spawn(client: Client, cfg: WriterConfig, on_commit: OnCommit) -> Result<WriterHandle, AppError> {
        prepare_session(&client).await.map_err(|e| map_db_error(e, &cfg.data_root))?;
        let (tx, rx) = mpsc::channel(256);
        let join = tokio::spawn(run(client, cfg, rx, on_commit));
        Ok(WriterHandle { tx, join })
    }
}

impl WriterHandle {
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
                &(mails - stored),
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
                .write(&[&it.seq, &&m.dedupe_key[..], &r.ordinal, &kind_db(r.kind), &email, &atype, &r.display_name, &r.search_text])
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
            w.as_mut().write(&[&stage_db(e.stage), &severity_db(e.severity), &e.folder_path, &e.pst_item_id, &e.message]).await?;
        }
    }
    w.finish().await?;
    Ok(())
}
```


- [ ] **Step 7: Run the writer tests to verify they pass**

Run: `cd src-tauri && cargo test -p outlooker-core --features test-support --test writer`
Expected: `test result: ok. 10 passed` with `out_of_order_checkpoint_is_contiguous`, `message_id_duplicate_adds_occurrence`, `content_hash_crlf_vs_lf`, `batch_cut_by_items`, `batch_cut_by_bytes`, `batch_cut_by_time`, `synchronous_commit_off_only_on_writer`, `error_rolls_back_data_and_checkpoint`, `disk_guard_blocks_commit`, `same_key_twice_in_one_batch` all `ok`.

- [ ] **Step 8: Prove that the in-batch duplicate test bites**

Back up the file, break the winner rule, run, restore from the backup (never from git):

```bash
cp src-tauri/crates/outlooker-core/src/importer/writer.rs scripts/tmp/writer.rs.bak
python3 -c "import pathlib;p=pathlib.Path('src-tauri/crates/outlooker-core/src/importer/writer.rs');s=p.read_text();old='LEFT JOIN addresses a ON a.email = r.email\nJOIN (SELECT dedupe_key, min(seq) AS seq FROM stage_messages GROUP BY dedupe_key) w ON w.dedupe_key = r.dedupe_key AND w.seq = r.seq';assert s.count(old)==1;p.write_text(s.replace(old,'LEFT JOIN addresses a ON a.email = r.email'))"
cd src-tauri && cargo test -p outlooker-core --features test-support --test writer same_key_twice_in_one_batch; cd ..
cp scripts/tmp/writer.rs.bak src-tauri/crates/outlooker-core/src/importer/writer.rs
```

Expected: the test FAILS with `23505: duplicate key value violates unique constraint "message_recipients_pkey"`; after the restore the Step above passes again.

- [ ] **Step 9: Write the failing removal tests `src-tauri/crates/outlooker-core/tests/removal.rs`**

```rust
//! PST removal tests (spec Part 04 §4.3, §12.2 row Removal).
use outlooker_core::importer::remove::{self, RemoveStats};
use outlooker_core::importer::types::{EncodedImage, InlineImageOut, NormalizedMail};
use outlooker_core::importer::webp::source_hash;
use outlooker_core::test_support::import_fixtures::{item, mail, seed_mails};
use outlooker_core::test_support::TestCluster;

async fn count(c: &tokio_postgres::Client, sql: &str) -> i64 {
    c.query_one(sql, &[]).await.unwrap().get(0)
}

fn with_image(mut m: NormalizedMail, bytes: &[u8]) -> NormalizedMail {
    m.body_source = outlooker_core::importer::types::BodySource::Html;
    m.body_html = Some(r#"<p>x<img src="outlooker-inline:0"></p>"#.into());
    m.inline_images.push(InlineImageOut {
        ordinal: 0,
        content_id: Some("logo".into()),
        source_hash: source_hash(bytes),
        blob: Some(EncodedImage { webp: bytes.to_vec(), width: 1, height: 1, source_mime: "image/png".into() }),
    });
    m
}

/// Source A: a mail shared with B (same Message-ID) + a mail only in A (own sender address and own image).
async fn two_sources(tc: &TestCluster) -> (i64, i64) {
    let a = seed_mails(
        &tc.db,
        "/p/a.pst",
        vec![
            item(1, 11, mail("shared", "s", "shared@x.example", Some("<shared@x>"))),
            item(2, 12, with_image(mail("only a", "o", "only-a@x.example", Some("<only-a@x>")), b"image-a")),
        ],
    )
    .await;
    let b = seed_mails(&tc.db, "/p/b.pst", vec![item(1, 21, mail("shared", "s", "shared@x.example", Some("<shared@x>")))]).await;
    (a, b)
}

#[tokio::test]
async fn keeps_shared_deletes_rest() {
    let tc = TestCluster::start().await;
    let (a, b) = two_sources(&tc).await;
    let admin = tc.admin().await;
    assert_eq!(count(&admin, "SELECT count(*) FROM messages").await, 2);
    assert_eq!(count(&admin, "SELECT count(*) FROM image_blobs").await, 1);
    let stats = remove::run(&tc.app().await, a).await.unwrap();
    assert_eq!(stats, RemoveStats { occurrences_deleted: 2, messages_deleted: 1 });
    let left: Vec<(String, Option<i64>)> = admin
        .query("SELECT subject, first_pst_id FROM messages", &[])
        .await
        .unwrap()
        .iter()
        .map(|r| (r.get(0), r.get(1)))
        .collect();
    assert_eq!(left, vec![("shared".to_owned(), None)]); // first_pst_id was A → SET NULL
    assert_eq!(count(&admin, "SELECT count(*) FROM image_blobs").await, 0); // orphan blob deleted
    let emails: Vec<String> = admin.query("SELECT email FROM addresses ORDER BY email", &[]).await.unwrap().iter().map(|r| r.get(0)).collect();
    assert_eq!(emails, vec!["shared@x.example".to_owned()]); // orphan address deleted
    assert_eq!(count(&admin, &format!("SELECT count(*) FROM pst_sources WHERE id = {a}")).await, 0);
    assert_eq!(count(&admin, &format!("SELECT count(*) FROM message_occurrences WHERE pst_id = {b}")).await, 1);
}

#[tokio::test]
async fn second_run_is_noop() {
    let tc = TestCluster::start().await;
    let (a, b) = two_sources(&tc).await;
    let app = tc.app().await;
    remove::run(&app, a).await.unwrap();
    assert_eq!(remove::run(&app, a).await.unwrap(), RemoveStats::default());
    let admin = tc.admin().await;
    assert_eq!(count(&admin, "SELECT count(*) FROM messages").await, 1);
    assert_eq!(count(&admin, "SELECT count(*) FROM addresses").await, 1);
    remove::run(&app, b).await.unwrap();
    assert_eq!(count(&admin, "SELECT count(*) FROM messages").await, 0);
    assert_eq!(count(&admin, "SELECT count(*) FROM addresses").await, 0);
}
```


- [ ] **Step 10: Run the removal tests to verify they fail**

Run: `cd src-tauri && cargo test -p outlooker-core --features test-support --test removal`
Expected: compile error (unresolved import `outlooker_core::importer::remove::{self, RemoveStats}` items).

- [ ] **Step 11: Implement `src-tauri/crates/outlooker-core/src/importer/remove.rs`**

```rust
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
```


- [ ] **Step 12: Run the writer, removal and schema tests**

Run: `cd src-tauri && cargo test -p outlooker-core --features test-support --test writer --test removal --test db_schema`
Expected: writer `10 passed`, removal `2 passed` (`keeps_shared_deletes_rest`, `second_run_is_noop`), db_schema `8 passed` (Round 1 regression of the owned `db/**`).

- [ ] **Step 13: Commit**

Write the message file `scripts/tmp/r210a-msg.txt` (Write tool; `scripts/tmp/` is gitignored, the session hook blocks `/tmp`) with: `feat(core): bulk writer, PST removal, import test fixtures (R2-10)`, a blank line, and the attribution line of the session.

```bash
git rev-parse --abbrev-ref HEAD
git add src-tauri/crates/outlooker-core/src/importer/writer.rs src-tauri/crates/outlooker-core/src/importer/remove.rs src-tauri/crates/outlooker-core/src/test_support/mod.rs src-tauri/crates/outlooker-core/src/test_support/import_fixtures.rs src-tauri/crates/outlooker-core/tests/writer.rs src-tauri/crates/outlooker-core/tests/removal.rs
git commit -F scripts/tmp/r210a-msg.txt -- src-tauri/crates/outlooker-core/src/importer/writer.rs src-tauri/crates/outlooker-core/src/importer/remove.rs src-tauri/crates/outlooker-core/src/test_support/mod.rs src-tauri/crates/outlooker-core/src/test_support/import_fixtures.rs src-tauri/crates/outlooker-core/tests/writer.rs src-tauri/crates/outlooker-core/tests/removal.rs
git status --porcelain
```

Expected: the commit succeeds; `git status --porcelain` lists none of the paths above.

- [ ] **Step 14: Replace `src-tauri/crates/outlooker-loadgen/Cargo.toml`**

```toml
[package]
name = "outlooker-loadgen"
version = "0.1.0"
edition = "2024"
publish = false

[dependencies]
outlooker-core = { path = "../outlooker-core", features = ["test-support"] }
tokio = { version = "=1.54.0", features = ["rt-multi-thread", "macros"] }
tokio-postgres = "=0.7.18"
image = "=0.25.10"
rayon = "=1.12.0"
serde_json = "=1.0.154"

[dev-dependencies]
tempfile = "=3.27.0"
```

Every dependency is a Part 01 crate at its Part 01 version. Then run `cd src-tauri && cargo build -p outlooker-loadgen` (the kickoff `main.rs` is still `fn main() {}`). Expected: `Finished`; `git diff --stat src-tauri/Cargo.lock` shows no new package (only the loadgen dependency edges).

- [ ] **Step 15: Create `src-tauri/crates/outlooker-loadgen/data/words-en.txt`**

Every 100th 3–9-letter lowercase word of the macOS word list (the file is generated once and committed):

```bash
python3 -c "import pathlib,re;w=[x for x in pathlib.Path('/usr/share/dict/words').read_text().split(chr(10)) if re.fullmatch('[a-z]{3,9}',x)];p=pathlib.Path('src-tauri/crates/outlooker-loadgen/data/words-en.txt');p.parent.mkdir(parents=True,exist_ok=True);p.write_text(chr(10).join(w[::100])+chr(10))"
wc -l src-tauri/crates/outlooker-loadgen/data/words-en.txt
head -1 src-tauri/crates/outlooker-loadgen/data/words-en.txt
tail -1 src-tauri/crates/outlooker-loadgen/data/words-en.txt
```

Expected: `1050`, `aal`, `zygotic`. The unit test of `corpus.rs` asserts 1,050 words.

- [ ] **Step 16: Create `src-tauri/crates/outlooker-loadgen/data/words-zh.txt` (296 Traditional Chinese words, one per line)**

```text
報價
報價單
會議
合約
客戶
專案
進度
預算
發票
付款
採購
訂單
出貨
庫存
產品
服務
品質
測試
系統
資料
文件
附件
確認
回覆
通知
安排
時間
地點
日期
下週
本週
明天
今天
上午
下午
晚上
主管
同事
部門
公司
團隊
經理
工程
設計
開發
維護
問題
解決
討論
建議
意見
修改
更新
版本
發布
上線
需求
規格
架構
流程
計畫
目標
策略
市場
銷售
業務
財務
會計
人事
行政
法務
簽核
審查
核准
申請
報告
簡報
說明
摘要
內容
重點
細節
結果
分析
統計
數據
圖表
表格
清單
項目
任務
工作
責任
負責
協助
支援
配合
聯絡
電話
郵件
信箱
地址
網站
帳號
密碼
登入
權限
設定
安裝
下載
上傳
備份
還原
資料庫
伺服器
網路
連線
速度
效能
容量
空間
費用
價格
成本
利潤
收入
支出
預估
實際
差異
調整
變更
延期
提前
完成
開始
結束
暫停
取消
繼續
等待
處理
執行
檢查
驗收
交付
保固
維修
退貨
換貨
退款
折扣
優惠
活動
促銷
廣告
宣傳
品牌
形象
客服
滿意
抱怨
回饋
評估
改善
提升
降低
增加
減少
維持
穩定
風險
安全
保密
合規
規定
政策
程序
標準
原則
方法
工具
資源
人力
物料
設備
場地
交通
住宿
出差
旅費
報銷
薪資
獎金
福利
假期
請假
加班
值班
訓練
課程
講師
學員
考試
證書
面試
招募
新人
離職
異動
升遷
績效
考核
年度
季度
月份
週報
月報
年報
會議室
視訊
電話會議
議程
紀錄
決議
追蹤
提醒
截止
期限
優先
緊急
重要
一般
參考
附上
如下
如上
感謝
麻煩
請問
謝謝
敬請
煩請
盡快
儘速
稍後
另外
此外
因此
所以
但是
然而
如果
雖然
已經
尚未
目前
之前
之後
同時
相關
其他
全部
部分
每個
各位
大家
我們
你們
他們
台北
台中
高雄
新竹
上海
香港
東京
倉庫
工廠
門市
分公司
總部
董事會
股東
投資
貸款
銀行
匯款
帳戶
對帳
結算
稅務
報稅
審計
保險
理賠
```


- [ ] **Step 17: Create `src-tauri/crates/outlooker-loadgen/src/lib.rs` with the first module**

```rust
//! 1M-message load generator, E2E seed and perf harness helpers (spec Part 04 §12.3, Part 02 §9).
pub mod rng;
```


- [ ] **Step 18: Write the failing tests in `src-tauri/crates/outlooker-loadgen/src/rng.rs`**

Create the file with only this test module (the code under test does not exist yet):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_bounded() {
        let (mut a, mut b) = (Rng::new(42), Rng::new(42));
        let xs: Vec<u64> = (0..100).map(|_| a.next_u64()).collect();
        let ys: Vec<u64> = (0..100).map(|_| b.next_u64()).collect();
        assert_eq!(xs, ys);
        assert_ne!(Rng::new(1).next_u64(), Rng::new(2).next_u64());
        let mut r = Rng::new(7);
        for _ in 0..10_000 {
            assert!(r.below(10) < 10);
            let u = r.unit();
            assert!((0.0..1.0).contains(&u));
            assert!((5..=9).contains(&r.range(5, 9)));
        }
    }

    #[test]
    fn zipf_rank_zero_is_most_frequent() {
        let z = Zipf::new(100);
        let mut r = Rng::new(3);
        let mut counts = [0usize; 100];
        for _ in 0..100_000 {
            counts[z.sample(&mut r)] += 1;
        }
        assert!(counts[0] > counts[1] && counts[1] > counts[9] && counts[9] > counts[99]);
        let share0 = counts[0] as f64 / 100_000.0; // 1 / H(100) ≈ 0.193
        assert!((0.18..0.21).contains(&share0), "{share0}");
    }
}
```


- [ ] **Step 19: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test -p outlooker-loadgen --lib rng::`
Expected: compile error (cannot find type `Rng` / `Zipf`).

- [ ] **Step 20: Implement `src-tauri/crates/outlooker-loadgen/src/rng.rs`**

Insert this code above the test module (the file is this code followed by the tests of the previous step):

```rust
//! Deterministic PRNG (xorshift64*; Part 01 has no `rand` crate). Same seed → same data on every OS.

#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        // splitmix64 of the seed, so nearby seeds give unrelated streams; never 0
        let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        Rng((z ^ (z >> 31)) | 1)
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform in [0, 1).
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Uniform in [0, n); n > 0.
    pub fn below(&mut self, n: u64) -> u64 {
        ((self.next_u64() as u128 * n as u128) >> 64) as u64
    }

    /// Uniform in [lo, hi].
    pub fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.below(hi - lo + 1)
    }

    pub fn chance(&mut self, p: f64) -> bool {
        self.unit() < p
    }

    /// Standard normal (Box–Muller).
    pub fn normal(&mut self) -> f64 {
        let u1 = self.unit().max(f64::MIN_POSITIVE);
        let u2 = self.unit();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }
}

/// Zipf(s = 1) sampler over ranks 0..n (rank 0 most frequent): binary search in the cumulative weights.
#[derive(Debug, Clone)]
pub struct Zipf {
    cdf: Vec<f64>,
}

impl Zipf {
    pub fn new(n: usize) -> Zipf {
        let mut acc = 0.0;
        let mut cdf: Vec<f64> = (1..=n)
            .map(|k| {
                acc += 1.0 / k as f64;
                acc
            })
            .collect();
        for c in &mut cdf {
            *c /= acc;
        }
        Zipf { cdf }
    }

    pub fn sample(&self, rng: &mut Rng) -> usize {
        let u = rng.unit();
        self.cdf.partition_point(|&c| c <= u).min(self.cdf.len() - 1)
    }
}
```


- [ ] **Step 21: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test -p outlooker-loadgen --lib rng::`
Expected: 2 tests `ok`, `test result: ok. 2 passed`.

- [ ] **Step 22: Add the corpus module**

Append `pub mod corpus;` to `src-tauri/crates/outlooker-loadgen/src/lib.rs`.

- [ ] **Step 23: Write the failing tests in `src-tauri/crates/outlooker-loadgen/src/corpus.rs`**

Create the file with only this test module (the code under test does not exist yet):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_lists_and_text() {
        let c = Corpus::new();
        assert_eq!(c.en.len(), 1050);
        assert!(c.zh.len() == 296 && c.zh.contains(&"報價單"));
        let mut r = Rng::new(1);
        let en = c.text(&mut r, Lang::En, 2048);
        assert!(en.len() >= 2048 && en.is_ascii());
        let zh = c.text(&mut r, Lang::Zh, 2048);
        assert!(!zh.contains(' ') && zh.len() >= 2048);
        assert!(!c.subject(&mut r, Lang::Mixed).is_empty());
    }
}
```


- [ ] **Step 24: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test -p outlooker-loadgen --lib corpus::`
Expected: compile error (cannot find type `Corpus`).

- [ ] **Step 25: Implement `src-tauri/crates/outlooker-loadgen/src/corpus.rs`**

Insert this code above the test module (the file is this code followed by the tests of the previous step):

```rust
//! Bundled vocabularies (spec Part 04 §12.3): English words with Zipf ranks, Traditional Chinese words joined
//! without spaces, and a 10% mix of both.
use crate::rng::{Rng, Zipf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    Zh,
    Mixed,
}

pub struct Corpus {
    pub en: Vec<&'static str>,
    pub zh: Vec<&'static str>,
    en_zipf: Zipf,
    zh_zipf: Zipf,
}

impl Default for Corpus {
    fn default() -> Self {
        Corpus::new()
    }
}

impl Corpus {
    pub fn new() -> Corpus {
        let en: Vec<&'static str> = include_str!("../data/words-en.txt").lines().filter(|l| !l.is_empty()).collect();
        let zh: Vec<&'static str> = include_str!("../data/words-zh.txt").lines().filter(|l| !l.is_empty()).collect();
        let (en_zipf, zh_zipf) = (Zipf::new(en.len()), Zipf::new(zh.len()));
        Corpus { en, zh, en_zipf, zh_zipf }
    }

    /// 60% English, 30% Chinese, 10% mixed.
    pub fn lang(rng: &mut Rng) -> Lang {
        match rng.below(10) {
            0..=5 => Lang::En,
            6..=8 => Lang::Zh,
            _ => Lang::Mixed,
        }
    }

    fn push_word(&self, rng: &mut Rng, lang: Lang, out: &mut String) {
        let zh = match lang {
            Lang::En => false,
            Lang::Zh => true,
            Lang::Mixed => rng.chance(0.5),
        };
        if zh {
            out.push_str(self.zh[self.zh_zipf.sample(rng)]);
        } else {
            if !out.is_empty() && !out.ends_with(['\n', ' ']) {
                out.push(' ');
            }
            out.push_str(self.en[self.en_zipf.sample(rng)]);
            out.push(' ');
        }
    }

    /// Text of about `target_bytes` UTF-8 bytes: sentences of 6–16 words, a paragraph break after 20% of sentences.
    pub fn text(&self, rng: &mut Rng, lang: Lang, target_bytes: usize) -> String {
        let mut out = String::with_capacity(target_bytes + 64);
        while out.len() < target_bytes {
            for _ in 0..rng.range(6, 16) {
                self.push_word(rng, lang, &mut out);
            }
            let trimmed = out.trim_end_matches(' ').len();
            out.truncate(trimmed);
            out.push_str(if lang == Lang::En { ". " } else { "。" });
            if rng.chance(0.2) {
                let trimmed = out.trim_end_matches(' ').len();
                out.truncate(trimmed);
                out.push('\n');
            }
        }
        out.trim_end().to_owned()
    }

    pub fn subject(&self, rng: &mut Rng, lang: Lang) -> String {
        let mut out = String::new();
        for _ in 0..rng.range(2, 6) {
            self.push_word(rng, lang, &mut out);
        }
        out.trim().to_owned()
    }
}
```


- [ ] **Step 26: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test -p outlooker-loadgen --lib corpus::`
Expected: 1 tests `ok`, `test result: ok. 1 passed`.

- [ ] **Step 27: Add the images module**

Append `pub mod images;` to `src-tauri/crates/outlooker-loadgen/src/lib.rs`.

- [ ] **Step 28: Write the failing tests in `src-tauri/crates/outlooker-loadgen/src/images.rs`**

Create the file with only this test module (the code under test does not exist yet):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_known_skips_encoding() {
        let pool = ImagePool::new(5, 10, 400);
        assert_eq!(pool.source(3), pool.source(3));
        assert_ne!(pool.source(3).0, pool.source(4).0);
        assert_eq!(pool.source(4).1, "image/png");
        let known = KnownHashes::new();
        let first = pool.inline(12, 0, &known).unwrap();
        let blob = first.blob.as_ref().unwrap();
        assert!(blob.width >= 50 && blob.width <= 400 && blob.webp.starts_with(b"RIFF"));
        known.insert_all([first.source_hash]);
        let again = pool.inline(12, 1, &known).unwrap();
        assert_eq!((again.source_hash, again.blob, again.ordinal), (first.source_hash, None, 1));
    }
}
```


- [ ] **Step 29: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test -p outlooker-loadgen --lib images::`
Expected: compile error (cannot find type `ImagePool`).

- [ ] **Step 30: Implement `src-tauri/crates/outlooker-loadgen/src/images.rs`**

Insert this code above the test module (the file is this code followed by the tests of the previous step):

```rust
//! Generated inline images (spec Part 04 §12.3): indexes `0..logos` are small shared logos, the rest a pool of
//! gradient + light-noise PNG / JPEG images. Each image is a pure function of (seed, index).
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Mutex;

use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
use outlooker_core::importer::types::InlineImageOut;
use outlooker_core::importer::webp::{convert, ImageIn, ImageOut, KnownHashes};

use crate::rng::Rng;

pub const WEBP_QUALITY: u8 = 80;

pub struct ImagePool {
    seed: u64,
    logos: u32,
    max_side: u32,
    /// index → source hash of images generated before; lets a known image skip generation entirely.
    hashes: Mutex<HashMap<u32, [u8; 32]>>,
}

impl ImagePool {
    pub fn new(seed: u64, logos: u32, max_side: u32) -> ImagePool {
        ImagePool { seed, logos, max_side, hashes: Mutex::new(HashMap::new()) }
    }

    /// Source bytes and MIME type of image `index`.
    pub fn source(&self, index: u32) -> (Vec<u8>, &'static str) {
        let mut rng = Rng::new(self.seed ^ 0x1A6E ^ ((index as u64) << 20));
        let (lo, hi) = if index < self.logos { (64, 200) } else { (50, self.max_side as u64) };
        let (w, h) = (rng.range(lo, hi) as u32, rng.range(lo, hi) as u32);
        let base = [rng.below(256) as u8, rng.below(256) as u8, rng.below(256) as u8];
        let noise_seed = rng.next_u64();
        let img = RgbImage::from_fn(w, h, |x, y| {
            let n = (((x as u64).wrapping_mul(73_856_093) ^ (y as u64).wrapping_mul(19_349_663) ^ noise_seed) % 17) as u8;
            Rgb([
                base[0].wrapping_add((x * 255 / w) as u8 / 2).wrapping_add(n),
                base[1].wrapping_add((y * 255 / h) as u8 / 2).wrapping_add(n),
                base[2].wrapping_add(((x + y) * 255 / (w + h)) as u8 / 2),
            ])
        });
        let (format, mime) = if index.is_multiple_of(2) { (ImageFormat::Png, "image/png") } else { (ImageFormat::Jpeg, "image/jpeg") };
        let mut out = Vec::new();
        DynamicImage::ImageRgb8(img).write_to(&mut Cursor::new(&mut out), format).expect("encode generated image");
        (out, mime)
    }

    /// The inline-image row for image `index` at `ordinal`; `None` when `webp::convert` drops it (Tiny / Rejected).
    pub fn inline(&self, index: u32, ordinal: i32, known: &KnownHashes) -> Option<InlineImageOut> {
        let content_id = Some(format!("img{index}@loadgen.example"));
        let cached = self.hashes.lock().unwrap().get(&index).copied();
        if let Some(h) = cached.filter(|h| known.contains(h)) {
            return Some(InlineImageOut { ordinal, content_id, source_hash: h, blob: None });
        }
        let (bytes, _) = self.source(index);
        match convert(&ImageIn { bytes }, WEBP_QUALITY, known) {
            ImageOut::Encoded { source_hash, image } => {
                self.hashes.lock().unwrap().insert(index, source_hash);
                Some(InlineImageOut { ordinal, content_id, source_hash, blob: Some(image) })
            }
            ImageOut::Known { source_hash } => {
                self.hashes.lock().unwrap().insert(index, source_hash);
                Some(InlineImageOut { ordinal, content_id, source_hash, blob: None })
            }
            ImageOut::Tiny | ImageOut::Rejected { .. } => None,
        }
    }
}
```


- [ ] **Step 31: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test -p outlooker-loadgen --lib images::`
Expected: 1 tests `ok`, `test result: ok. 1 passed`.

- [ ] **Step 32: Add the generate module**

Append `pub mod generate;` to `src-tauri/crates/outlooker-loadgen/src/lib.rs`.

- [ ] **Step 33: Write the failing tests in `src-tauri/crates/outlooker-loadgen/src/generate.rs`**

Create the file with only this test module (the code under test does not exist yet):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_items_and_duplicate_plan() {
        let g = Generator::new(GenConfig::smoke(10_000));
        let known = KnownHashes::new();
        let a = g.item(3, 17, &known);
        let b = g.item(3, 17, &known);
        let (Outcome::Mail(ma), Outcome::Mail(mb)) = (&a.outcome, &b.outcome) else { panic!() };
        assert_eq!((ma.dedupe_key, &ma.subject, ma.sent_at), (mb.dedupe_key, &mb.subject, mb.sent_at));
        assert!(ma.sender.as_ref().unwrap().email.ends_with("@loadgen.example"));
        let mut dup_msgid = 0;
        let mut dup_hash = 0;
        for s in 0..10 {
            for i in 0..1_000 {
                match g.slot(s, i) {
                    Slot::MessageIdDup { s: bs, i: bi } => {
                        dup_msgid += 1;
                        assert!(bs < s);
                        let (Outcome::Mail(x), Outcome::Mail(y)) = (g.item(s, i, &known).outcome, g.item(bs, bi, &known).outcome) else { panic!() };
                        assert_eq!(x.dedupe_key, y.dedupe_key);
                    }
                    Slot::HashDup { s: bs, i: bi } => {
                        dup_hash += 1;
                        let (Outcome::Mail(x), Outcome::Mail(y)) = (g.item(s, i, &known).outcome, g.item(bs, bi, &known).outcome) else { panic!() };
                        assert_eq!((x.dedupe_key, x.dedupe_kind), (y.dedupe_key, DedupeKind::ContentHash));
                        assert_eq!(x.body_text, y.body_text.replace('\n', "\r\n"), "CRLF copy");
                    }
                    Slot::Fresh => {}
                }
            }
        }
        // 9 of 10 sources can hold duplicates: expected 9,000 × 3% = 270 and 9,000 × 0.5% = 45
        assert!((200..340).contains(&dup_msgid), "{dup_msgid}");
        assert!((20..80).contains(&dup_hash), "{dup_hash}");
    }

    #[test]
    fn e2e_message_in_seed_config() {
        let g = Generator::new(GenConfig::seed(100));
        let known = KnownHashes::new();
        let Outcome::Mail(m) = g.item(0, 0, &known).outcome else { panic!() };
        assert_eq!(m.subject, E2E_SUBJECT);
        assert!(m.body_text.contains("報價單"));
        assert_eq!(m.inline_images.len(), 1);
        assert!(m.inline_images[0].blob.is_some());
        assert!(m.body_html.unwrap().contains("src=\"outlooker-inline:0\""));
        assert_eq!(year_of(UNIX_EPOCH + Duration::from_secs(DATE_END - 1)), 2025);
    }
}
```


- [ ] **Step 34: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test -p outlooker-loadgen --lib generate::`
Expected: compile error (cannot find type `Generator` / `GenConfig`).

- [ ] **Step 35: Implement `src-tauri/crates/outlooker-loadgen/src/generate.rs`**

Insert this code above the test module (the file is this code followed by the tests of the previous step):

```rust
//! Synthetic `NormalizedItem`s (spec Part 04 §12.3). Item `i` of source `s` is a pure function of
//! (seed, s, i), so a duplicate is produced by regenerating an earlier slot.
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use outlooker_core::dto::search::RecipientKind;
use outlooker_core::importer::dedupe;
use outlooker_core::importer::types::{
    AddressIn, AddressType, AttachmentOut, BodySource, DedupeKind, InlineImageOut, NormalizedItem, NormalizedMail, Outcome,
    RecipientOut,
};
use outlooker_core::importer::webp::KnownHashes;

use crate::corpus::Corpus;
use crate::images::ImagePool;
use crate::rng::Rng;

/// 2026-01-01T00:00:00Z: generated dates fall in the 10 years before it.
pub const DATE_END: u64 = 1_767_225_600;
pub const DATE_SPAN: u64 = 10 * 365 * 86_400;
pub const SENDERS: u64 = 20_000;
pub const RECIPIENT_POOL: u64 = 50_000;
pub const NO_MESSAGE_ID_RATE: f64 = 0.10;
pub const E2E_SUBJECT: &str = "E2E 報價單 inline";

#[derive(Debug, Clone, PartialEq)]
pub struct GenConfig {
    pub messages: u64,
    pub sources: u32,
    pub seed: u64,
    pub msgid_dup_rate: f64,
    pub hash_dup_rate: f64,
    pub image_rate: f64,
    pub image_pool: u32,
    pub logo_pool: u32,
    pub max_image_side: u32,
    pub e2e_message: bool,
}

impl GenConfig {
    pub fn full() -> GenConfig {
        GenConfig { messages: 1_000_000, sources: 10, seed: 42, msgid_dup_rate: 0.03, hash_dup_rate: 0.005, image_rate: 0.30, image_pool: 50_000, logo_pool: 200, max_image_side: 1600, e2e_message: false }
    }
    pub fn smoke(messages: u64) -> GenConfig {
        GenConfig { messages, sources: 10, seed: 7, msgid_dup_rate: 0.03, hash_dup_rate: 0.005, image_rate: 0.30, image_pool: 200, logo_pool: 20, max_image_side: 400, e2e_message: false }
    }
    pub fn seed(messages: u64) -> GenConfig {
        GenConfig { messages, sources: 2, seed: 11, msgid_dup_rate: 0.03, hash_dup_rate: 0.005, image_rate: 0.10, image_pool: 100, logo_pool: 10, max_image_side: 400, e2e_message: true }
    }
    /// Items in source `s` (the last source takes the remainder).
    pub fn per_source(&self, s: u32) -> u64 {
        let base = self.messages / self.sources as u64;
        if s + 1 == self.sources { self.messages - base * (self.sources as u64 - 1) } else { base }
    }
}

/// What slot (s, i) holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Fresh,
    /// Same Message-ID (and content) as fresh slot (s, i).
    MessageIdDup { s: u32, i: u64 },
    /// Same sender, time, subject and body as fresh slot (s, i) without Message-ID; body line endings CRLF.
    HashDup { s: u32, i: u64 },
}

pub struct Generator {
    pub cfg: GenConfig,
    pub corpus: Corpus,
    pub images: ImagePool,
}

fn slot_rng(seed: u64, s: u32, i: u64, salt: u64) -> Rng {
    Rng::new(seed.wrapping_mul(1_000_003) ^ ((s as u64) << 40) ^ i ^ salt.wrapping_mul(0x9E37_79B9))
}

impl Generator {
    pub fn new(cfg: GenConfig) -> Generator {
        let images = ImagePool::new(cfg.seed, cfg.logo_pool, cfg.max_image_side);
        Generator { cfg, corpus: Corpus::new(), images }
    }

    fn has_message_id(&self, s: u32, i: u64) -> bool {
        !slot_rng(self.cfg.seed, s, i, 1).chance(NO_MESSAGE_ID_RATE)
    }

    fn is_e2e(&self, s: u32, i: u64) -> bool {
        self.cfg.e2e_message && s == 0 && i == 0
    }

    pub fn slot(&self, s: u32, i: u64) -> Slot {
        if s == 0 || self.is_e2e(s, i) {
            return Slot::Fresh;
        }
        let mut r = slot_rng(self.cfg.seed, s, i, 2);
        let u = r.unit();
        let want_msgid = if u < self.cfg.msgid_dup_rate {
            true
        } else if u < self.cfg.msgid_dup_rate + self.cfg.hash_dup_rate {
            false
        } else {
            return Slot::Fresh;
        };
        // up to 16 candidates in earlier sources; a candidate must be a fresh slot of the wanted key kind
        for _ in 0..16 {
            let cs = r.below(s as u64) as u32;
            let ci = r.below(self.cfg.per_source(cs));
            if self.slot(cs, ci) == Slot::Fresh && !self.is_e2e(cs, ci) && self.has_message_id(cs, ci) == want_msgid {
                return if want_msgid { Slot::MessageIdDup { s: cs, i: ci } } else { Slot::HashDup { s: cs, i: ci } };
            }
        }
        Slot::Fresh
    }

    /// Item `i` (0-based) of source `s`: seq = pst_item_id = i + 1, folder = year of the sent time.
    pub fn item(&self, s: u32, i: u64, known: &KnownHashes) -> NormalizedItem {
        let mail = match self.slot(s, i) {
            Slot::Fresh if self.is_e2e(s, i) => self.e2e_mail(known),
            Slot::Fresh => self.fresh(s, i, known),
            Slot::MessageIdDup { s: bs, i: bi } => self.fresh(bs, bi, known),
            Slot::HashDup { s: bs, i: bi } => {
                let mut m = self.fresh(bs, bi, known);
                m.body_text = m.body_text.replace('\n', "\r\n");
                m
            }
        };
        let year = mail.sent_at.map_or(2025, year_of);
        NormalizedItem { seq: i as i64 + 1, pst_item_id: i as i64 + 1, pst_folder_id: year, raw_bytes: 0, outcome: Outcome::Mail(Box::new(mail)), errors: Vec::new() }
    }

    fn fresh(&self, s: u32, i: u64, known: &KnownHashes) -> NormalizedMail {
        let mut r = slot_rng(self.cfg.seed, s, i, 3);
        let lang = Corpus::lang(&mut r);
        let subject = self.corpus.subject(&mut r, lang);
        // log-normal body: median 2 KB, p99 60 KB (sigma = ln(30) / 2.326), capped at 64 KB
        let len = (2048.0 * (1.462 * r.normal()).exp()).clamp(64.0, 65_536.0) as usize;
        let body_text = self.corpus.text(&mut r, lang, len);
        let sent = UNIX_EPOCH + Duration::from_secs(DATE_END - 1 - r.below(DATE_SPAN));
        let k = r.below(SENDERS);
        let sender_email = format!("sender{k}@loadgen.example");
        let sender_name = format!("Sender {k}");
        let n_rcpt = if r.chance(0.01) { 300 } else { r.range(1, 10) };
        let mut recipients = Vec::with_capacity(n_rcpt as usize);
        for ordinal in 0..n_rcpt as i32 {
            let u = r.below(RECIPIENT_POOL);
            let kind = if ordinal == 0 { RecipientKind::To } else { [RecipientKind::To, RecipientKind::To, RecipientKind::Cc, RecipientKind::Bcc][r.below(4) as usize] };
            let email = format!("user{u}@corp{}.example", u % 50);
            let name = format!("User {u}");
            recipients.push(RecipientOut {
                ordinal,
                kind,
                search_text: format!("{name} <{email}>"),
                address: Some(AddressIn { email, address_type: AddressType::Smtp }),
                display_name: Some(name),
            });
        }
        let attachments = if r.chance(0.25) {
            (0..r.range(1, 3) as i32)
                .map(|a| AttachmentOut { attachment_index: a, file_name: format!("document-{a}.pdf"), size_bytes: r.range(1_000, 5_000_000) as i64, mime_type: "application/pdf".into(), attach_method: 1 })
                .collect()
        } else {
            Vec::new()
        };
        let mut inline_images = Vec::new();
        if r.chance(self.cfg.image_rate) {
            for ordinal in 0..r.range(1, 3) as i32 {
                let index = if r.chance(0.2) { r.below(self.cfg.logo_pool as u64) as u32 } else { self.cfg.logo_pool + r.below(self.cfg.image_pool as u64) as u32 };
                if let Some(img) = self.images.inline(index, ordinal, known) {
                    inline_images.push(img);
                }
            }
        }
        let message_id = self.has_message_id(s, i).then(|| format!("<{s}.{i}.{}@loadgen.example>", self.cfg.seed));
        assemble(subject, body_text, sent, sender_email, sender_name, recipients, attachments, inline_images, message_id.as_deref())
    }

    fn e2e_mail(&self, known: &KnownHashes) -> NormalizedMail {
        let img = self.images.inline(0, 0, known).expect("logo 0 is at least 64 px, so it always encodes");
        let sent = UNIX_EPOCH + Duration::from_secs(DATE_END - 86_400);
        let recipients = vec![RecipientOut {
            ordinal: 0,
            kind: RecipientKind::To,
            address: Some(AddressIn { email: "e2e@loadgen.example".into(), address_type: AddressType::Smtp }),
            display_name: Some("E2E Recipient".into()),
            search_text: "E2E Recipient <e2e@loadgen.example>".into(),
        }];
        assemble(
            E2E_SUBJECT.into(),
            "這是 E2E 測試的報價單，內含一張圖片。\nThis quotation has one inline image.".into(),
            sent,
            "sender0@loadgen.example".into(),
            "Sender 0".into(),
            recipients,
            Vec::new(),
            vec![img],
            Some("<e2e-inline@loadgen.example>"),
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn assemble(
    subject: String,
    body_text: String,
    sent: SystemTime,
    sender_email: String,
    sender_name: String,
    recipients: Vec<RecipientOut>,
    attachments: Vec<AttachmentOut>,
    inline_images: Vec<InlineImageOut>,
    message_id: Option<&str>,
) -> NormalizedMail {
    let key = dedupe::key(message_id, None, &sender_email, Some(sent), &subject, &body_text);
    let mut html = String::with_capacity(body_text.len() + 64);
    for p in body_text.split('\n') {
        html.push_str("<p>");
        html.push_str(p);
        html.push_str("</p>");
    }
    for img in &inline_images {
        html.push_str(&format!("<p><img src=\"outlooker-inline:{}\"></p>", img.ordinal));
    }
    let to_preview = recipients
        .iter()
        .filter(|r| r.kind == RecipientKind::To)
        .take(3)
        .filter_map(|r| r.display_name.clone())
        .collect::<Vec<_>>()
        .join(", ");
    let recipients_search = recipients.iter().map(|r| r.search_text.as_str()).collect::<Vec<_>>().join("\n");
    let snippet: String = body_text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(200).collect();
    debug_assert!(key.kind == DedupeKind::MessageId || message_id.is_none());
    NormalizedMail {
        dedupe_key: key.key,
        dedupe_kind: key.kind,
        message_id: key.message_id,
        subject,
        sent_at: Some(sent),
        received_at: Some(sent + Duration::from_secs(60)),
        sender_search: format!("{sender_name} <{sender_email}>"),
        sender: Some(AddressIn { email: sender_email, address_type: AddressType::Smtp }),
        sender_name: Some(sender_name),
        recipients,
        recipients_search,
        to_preview,
        body_source: BodySource::Html,
        body_html: Some(html),
        body_text,
        snippet,
        attachments,
        inline_images,
    }
}

/// UTC calendar year of `t` (uses the R1-05 RFC 3339 formatter: "YYYY-…").
pub fn year_of(t: SystemTime) -> i64 {
    dedupe::format_utc_seconds(t)[..4].parse().expect("four-digit year")
}
```


- [ ] **Step 36: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test -p outlooker-loadgen --lib generate::`
Expected: 2 tests `ok`, `test result: ok. 2 passed`.

- [ ] **Step 37: Write the failing smoke tests `src-tauri/crates/outlooker-loadgen/tests/smoke.rs`**

```rust
//! Non-ignored load generator checks: 10,000-message smoke load and the E2E seed root.
use outlooker_core::test_support::TestCluster;
use outlooker_loadgen::generate::{GenConfig, E2E_SUBJECT};
use outlooker_loadgen::load::{load, seed};
use outlooker_loadgen::root::LibraryRoot;

#[tokio::test(flavor = "multi_thread")]
async fn smoke_10k() {
    let tc = TestCluster::start().await;
    let cfg = GenConfig::smoke(10_000);
    let r = load(&tc.db, &cfg).await.unwrap();
    println!("messages_stored={} occurrences={}", r.messages_stored, r.occurrences);
    assert_eq!(r.messages_submitted, 10_000);
    assert_eq!(r.occurrences, 10_000); // one occurrence per item: pst_item_id is unique per source
    // every planned duplicate is deduplicated, nothing else is
    assert_eq!(r.messages_stored as u64, r.messages_submitted - r.duplicates_planned);
    // duplicate share within the configured rates ± 1 percentage point (source 0 holds no duplicates)
    let expected = (cfg.msgid_dup_rate + cfg.hash_dup_rate) * (cfg.sources - 1) as f64 / cfg.sources as f64;
    let measured = 1.0 - r.messages_stored as f64 / r.messages_submitted as f64;
    assert!((measured - expected).abs() <= 0.01, "duplicate share {measured:.4}, expected {expected:.4}");
    let admin = tc.admin().await;
    let missing = outlooker_core::db::indexes::missing_or_invalid(&admin).await.unwrap();
    assert!(missing.is_empty(), "{missing:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn seeded_root_starts() {
    let dir = tempfile::tempdir().unwrap();
    let root_dir = dir.path().join("library");
    seed(&root_dir, 2_000).await.unwrap();
    let root = LibraryRoot::open(&root_dir).await.unwrap();
    let a = root.db.connect_admin().await.unwrap();
    let n: i64 = a.query_one("SELECT count(*) FROM messages", &[]).await.unwrap().get(0);
    assert!(n > 1_900, "{n}");
    let inline: i64 = a
        .query_one("SELECT count(*) FROM inline_images i JOIN messages m ON m.id = i.message_id WHERE m.subject = $1", &[&E2E_SUBJECT])
        .await
        .unwrap()
        .get(0);
    assert_eq!(inline, 1);
    let bad: i64 = a
        .query_one("SELECT count(*) FROM addresses a JOIN messages m ON m.sender_address_id = a.id WHERE a.email NOT LIKE '%@loadgen.example'", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(bad, 0);
    assert!(root.paths.marker().exists() && root.paths.credentials().exists());
    root.stop().await.unwrap();
}
```


- [ ] **Step 38: Run the smoke tests to verify they fail**

Run: `cd src-tauri && cargo test -p outlooker-loadgen --test smoke`
Expected: compile error (unresolved modules `load`, `root`).

- [ ] **Step 39: Create `src-tauri/crates/outlooker-loadgen/src/root.rs`**

```rust
//! A persistent data root (spec Part 02 §8.2 layout) built only from public foundation APIs, so the app can adopt
//! it and `LibraryRoot::open` can start it again.
use std::path::Path;
use std::time::SystemTime;

use outlooker_core::db::{connect_admin, migrate, Credentials, Db, DbConn, Pools};
use outlooker_core::dto::error::AppError;
use outlooker_core::importer::dedupe::format_utc_seconds;
use outlooker_core::lifecycle::pg::{PgController, StopMode};
use outlooker_core::paths::{dev_pg_dir, Paths};

pub struct LibraryRoot {
    pub paths: Paths,
    pub port: u16,
    pub creds: Credentials,
    pub db: Db,
}

fn paths_for(data_root: &Path) -> Paths {
    // the app-local and log dirs are never written by the load generator
    Paths::new(data_root.to_path_buf(), data_root.to_path_buf(), dev_pg_dir()).with_data_root(data_root.to_path_buf())
}

async fn connect(paths: Paths, creds: Credentials) -> Result<LibraryRoot, AppError> {
    let port = PgController::new(paths.clone()).start(&paths.pgdata()).await?;
    let mut admin = connect_admin(port, &creds).await?;
    migrate(&mut admin, &creds).await?;
    let db = Db::new();
    db.set(Some(DbConn { port, creds: creds.clone(), pools: Pools::build(port, &creds)?, data_root: paths.data_root().to_path_buf() }));
    Ok(LibraryRoot { paths, port, creds, db })
}

impl LibraryRoot {
    /// Marker + credentials + initdb + start + migrate + pools. `data_root` must not contain a marker.
    pub async fn create(data_root: &Path) -> Result<LibraryRoot, AppError> {
        let paths = paths_for(data_root);
        if paths.marker().exists() {
            return Err(AppError::invalid("dataRoot", "notEmpty"));
        }
        std::fs::create_dir_all(data_root).map_err(|e| AppError::file_access(data_root, &e.to_string()))?;
        let creds = Credentials::generate();
        creds.save(&paths.credentials())?;
        let locale = PgController::new(paths.clone()).init(&paths.pgdata(), &creds.admin).await?;
        let marker = serde_json::json!({
            "format_version": 1,
            "created_at": format_utc_seconds(SystemTime::now()),
            "pg_major": 18,
            "locale": locale,
        });
        std::fs::write(paths.marker(), serde_json::to_vec_pretty(&marker).expect("marker json"))
            .map_err(|e| AppError::file_access(&paths.marker(), &e.to_string()))?;
        connect(paths, creds).await
    }

    /// Start + credentials + migrate + pools of an existing root.
    pub async fn open(data_root: &Path) -> Result<LibraryRoot, AppError> {
        let paths = paths_for(data_root);
        if !paths.marker().exists() {
            return Err(AppError::file_access(&paths.marker(), "notFound"));
        }
        let creds = Credentials::load(&paths.credentials())?;
        connect(paths, creds).await
    }

    /// Db unset, then `pg_ctl stop -m fast`.
    pub async fn stop(self) -> Result<(), AppError> {
        self.db.set(None);
        PgController::new(self.paths.clone()).stop(&self.paths.pgdata(), StopMode::Fast).await
    }
}
```


- [ ] **Step 40: Create `src-tauri/crates/outlooker-loadgen/src/load.rs`**

```rust
//! Writes generated sources through the real writer (spec Part 04 §12.3).
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use outlooker_core::db::Db;
use outlooker_core::db::indexes::{drop_search_indexes, ensure_search_indexes};
use outlooker_core::dto::error::AppError;
use outlooker_core::importer::types::FolderIn;
use outlooker_core::importer::writer::{
    load_known_hashes, map_db_error, upsert_folders, Checkpoint, Writer, WriterConfig, WriterLimits, AFTER_JOB_VACUUM,
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
    let pg = |e| map_db_error(e, Path::new(""));
    let path = source_path(s);
    let name = format!("source-{s:02}.pst");
    let mtime = UNIX_EPOCH + Duration::from_secs(1_767_225_600 + s as u64);
    let pst_id: i64 = app
        .query_one(
            "INSERT INTO pst_sources (path, display_name, file_size_bytes, file_mtime) VALUES ($1, $2, 1, $3) RETURNING id",
            &[&path, &name, &mtime],
        )
        .await
        .map_err(pg)?
        .get(0);
    let job_id: i64 = app
        .query_one("INSERT INTO import_jobs (pst_id, kind, state, started_at) VALUES ($1, 'import', 'running', now()) RETURNING id", &[&pst_id])
        .await
        .map_err(pg)?
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
        let writer = Writer::spawn(db.connect_app().await?, wcfg, Box::new(|_| {})).await?;
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
    app.batch_execute(AFTER_JOB_VACUUM).await.map_err(|e| map_db_error(e, Path::new("")))?;
    let t = Instant::now();
    if defer_indexes {
        ensure_search_indexes(&admin, &|name: &str, done: usize, total: usize| log_progress(name, done, total)).await?;
    }
    let index_elapsed = t.elapsed();
    let count = |sql: &'static str| {
        let admin = &admin;
        async move { admin.query_one(sql, &[]).await.map(|r| r.get::<_, i64>(0)).map_err(|e| map_db_error(e, Path::new(""))) }
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
```

Append `pub mod load;` and `pub mod root;` to `src-tauri/crates/outlooker-loadgen/src/lib.rs`.

- [ ] **Step 41: Run the smoke tests to verify they pass**

Run: `cd src-tauri && cargo test -p outlooker-loadgen --test smoke -- --nocapture`
Expected: `smoke_10k ... ok` and `seeded_root_starts ... ok`; the output contains `messages_stored=9731 occurrences=10000` (the generator is deterministic: 269 planned duplicates = 2.69%, inside 3.15% ± 1 point). About 15 s with the dev profile of Step 1.

- [ ] **Step 42: Add the perf module**

Append `pub mod perf;` to `src-tauri/crates/outlooker-loadgen/src/lib.rs`.

- [ ] **Step 43: Write the failing tests in `src-tauri/crates/outlooker-loadgen/src/perf.rs`**

Create the file with only this test module (the code under test does not exist yet):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_nearest_rank_and_canned_set() {
        let s: Vec<Duration> = (1..=100).map(Duration::from_millis).collect();
        assert_eq!(percentile(&s, 0.95), Duration::from_millis(95));
        assert_eq!(percentile(&s, 0.50), Duration::from_millis(50));
        assert_eq!(percentile(&[Duration::from_millis(7)], 0.95), Duration::from_millis(7));
        let qs = canned_queries(&Corpus::new(), &[1, 2]);
        for (label, q) in &qs {
            let c = outlooker_core::search::compile::compile(q).unwrap_or_else(|e| panic!("{label}: {e:?}"));
            assert!(c.indexable && !c.short_query, "{label} must be an indexable term of ≥ 3 characters");
        }
    }
}
```


- [ ] **Step 44: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test -p outlooker-loadgen --lib perf::`
Expected: compile error (cannot find function `percentile` / `canned_queries`).

- [ ] **Step 45: Implement `src-tauri/crates/outlooker-loadgen/src/perf.rs`**

Insert this code above the test module (the file is this code followed by the tests of the previous step):

```rust
//! Helpers of the `perf_` gate tests (spec Part 04 §12.3, Part 02 §9): library location, result log, canned
//! queries, percentiles, EXPLAIN capture.
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use outlooker_core::dto::search::{ConditionGroup, ConditionNode as N, GroupOp, RecipientKind, SearchQuery};
use outlooker_core::importer::dedupe::format_utc_seconds;
use outlooker_core::search::compile::Compiled;
use outlooker_core::search::execute::{bind, ordered_sql, phase_a_sql, WINDOW};

use crate::corpus::Corpus;
use crate::root::LibraryRoot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Log {
    /// `docs/logs/loadtest-<date>.md` (Part 04 §12.3 rows)
    Loadtest,
    /// `docs/logs/perf-<date>.md` (Part 02 §9 rows)
    Perf,
}

/// Repository root (this crate is `src-tauri/crates/outlooker-loadgen`).
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// `$OUTLOOKER_PERF_ROOT`, default `src-tauri/target/perf-root`.
pub fn perf_root() -> PathBuf {
    std::env::var_os("OUTLOOKER_PERF_ROOT").map(PathBuf::from).unwrap_or_else(|| repo_root().join("src-tauri/target/perf-root"))
}

pub fn generate_json() -> PathBuf {
    perf_root().join("perf-generate.json")
}

pub async fn open_library() -> LibraryRoot {
    let root = perf_root();
    assert!(
        root.join("outlooker-data.json").exists(),
        "perf library missing at {}: run `cargo test -p outlooker-loadgen --release -- --ignored perf_generate` first",
        root.display()
    );
    LibraryRoot::open(&root).await.expect("open perf library")
}

pub fn commit() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(repo_root())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_else(|| "unknown".into())
}

/// Appends one result (a one-row table plus optional details) to the log of today (UTC) and returns its path.
pub fn record(log: Log, gate: &str, threshold: &str, measured: &str, pass: bool, details: &str) -> PathBuf {
    let date = &format_utc_seconds(SystemTime::now())[..10];
    let name = match log {
        Log::Loadtest => format!("loadtest-{date}.md"),
        Log::Perf => format!("perf-{date}.md"),
    };
    let path = repo_root().join("docs/logs").join(name);
    std::fs::create_dir_all(path.parent().unwrap()).expect("docs/logs");
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&path).expect("open log");
    let result = if pass { "PASS" } else { "FAIL" };
    let mut text = format!(
        "\n## {gate}\n\n| gate | threshold | measured | result | commit |\n|---|---|---|---|---|\n| {gate} | {threshold} | {measured} | {result} | {} |\n",
        commit()
    );
    if !details.is_empty() {
        text.push_str(&format!("\n```text\n{}\n```\n", details.trim_end()));
    }
    f.write_all(text.as_bytes()).expect("write log");
    path
}

/// Nearest-rank percentile (`p` in 0..=1) of `samples`.
pub fn percentile(samples: &[Duration], p: f64) -> Duration {
    assert!(!samples.is_empty(), "no samples");
    let mut s = samples.to_vec();
    s.sort();
    let rank = ((p * s.len() as f64).ceil() as usize).clamp(1, s.len());
    s[rank - 1]
}

pub fn ms(d: Duration) -> String {
    format!("{:.1} ms", d.as_secs_f64() * 1000.0)
}

fn dsl(op: GroupOp, children: Vec<N>) -> SearchQuery {
    SearchQuery::Dsl { root: ConditionGroup { op, children } }
}

/// The 30 canned queries of spec Part 04 §12.3 (every text value ≥ 3 characters): 6 sender, 6 recipient,
/// 6 English body (common and rare by Zipf rank), 6 Chinese body, 6 DSL combinations. `psts` = the library's PST ids.
pub fn canned_queries(c: &Corpus, psts: &[i64]) -> Vec<(String, SearchQuery)> {
    let body = |v: &str| dsl(GroupOp::And, vec![N::Body { value: v.into() }]);
    let mut q: Vec<(String, SearchQuery)> = Vec::new();
    for v in ["loadgen.example", "sender12", "sender19999@", "Sender 7", "sender42@loadgen", "sender305"] {
        q.push((format!("sender {v}"), dsl(GroupOp::And, vec![N::Sender { value: v.into() }])));
    }
    for (v, kinds) in [
        ("corp7.example", vec![]),
        ("user123", vec![]),
        ("user49999@", vec![]),
        ("corp1", vec![RecipientKind::Cc]),
        ("User 77", vec![]),
        ("user2024", vec![RecipientKind::To, RecipientKind::Bcc]),
    ] {
        q.push((format!("recipient {v} {kinds:?}"), dsl(GroupOp::And, vec![N::Recipient { value: v.into(), kinds }])));
    }
    for rank in [0, 1, 5, 50, 500, 1049] {
        q.push((format!("body en rank {rank} {}", c.en[rank]), body(c.en[rank])));
    }
    for v in ["報價單", "資料庫", "電話會議", "分公司", "董事會", "會議室"] {
        q.push((format!("body zh {v}"), body(v)));
    }
    let src = psts.first().copied().unwrap_or(1);
    q.push((
        "dsl sender + 2020".into(),
        dsl(GroupOp::And, vec![N::Sender { value: "sender1".into() }, N::DateRange { from: Some("2020-01-01T00:00:00Z".into()), to: Some("2021-01-01T00:00:00Z".into()) }]),
    ));
    q.push(("dsl body en + attachment".into(), dsl(GroupOp::And, vec![N::Body { value: c.en[2].into() }, N::HasAttachment { value: true }])));
    q.push(("dsl recipient + source".into(), dsl(GroupOp::And, vec![N::Recipient { value: "corp3".into(), kinds: vec![] }, N::SourcePst { pst_ids: vec![src] }])));
    q.push((
        "dsl body zh + folder".into(),
        dsl(GroupOp::And, vec![N::Body { value: "報價單".into() }, N::Folder { folder_path: "Inbox/2020".into(), include_subfolders: false }]),
    ));
    q.push(("dsl or(body en, subject zh)".into(), dsl(GroupOp::Or, vec![N::Body { value: c.en[10].into() }, N::Subject { value: "會議室".into() }])));
    q.push((format!("simple {} 報價單", c.en[3]), SearchQuery::Simple { text: format!("{} 報價單", c.en[3]) }));
    assert_eq!(q.len(), 30);
    q
}

/// `EXPLAIN (ANALYZE, BUFFERS)` of phase A and of phase B (planner settings of §7.4) for a first page of 200.
pub async fn explain(client: &mut tokio_postgres::Client, c: &Compiled) -> String {
    let (cs, ci, n) = ("infinity".to_owned(), 0_i64, 201_i64);
    let tx = client.transaction().await.expect("tx");
    let lines = |rows: Vec<tokio_postgres::Row>| rows.iter().map(|r| r.get::<_, String>(0)).collect::<Vec<_>>().join("\n");
    let a = tx.query(&format!("EXPLAIN (ANALYZE, BUFFERS) {}", phase_a_sql(c)), &bind(c, &[&cs, &ci, &WINDOW, &n])).await.expect("explain A");
    tx.batch_execute("SET LOCAL enable_indexscan = off; SET LOCAL enable_seqscan = off;").await.expect("set");
    let b = tx.query(&format!("EXPLAIN (ANALYZE, BUFFERS) {}", ordered_sql(c)), &bind(c, &[&cs, &ci, &n])).await.expect("explain B");
    tx.rollback().await.expect("rollback");
    format!("-- phase A\n{}\n-- phase B\n{}", lines(a), lines(b))
}
```


- [ ] **Step 46: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test -p outlooker-loadgen --lib perf::`
Expected: 1 tests `ok`, `test result: ok. 1 passed`.

- [ ] **Step 47: Check the final `src-tauri/crates/outlooker-loadgen/src/lib.rs`**

The file must now be exactly:

```rust
//! 1M-message load generator, E2E seed and perf harness helpers (spec Part 04 §12.3, Part 02 §9).
pub mod corpus;
pub mod generate;
pub mod images;
pub mod load;
pub mod perf;
pub mod rng;
pub mod root;
```


- [ ] **Step 48: Replace `src-tauri/crates/outlooker-loadgen/src/main.rs`**

```rust
//! `outlooker-loadgen seed --data-root <dir> --messages <n>`
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: outlooker-loadgen seed --data-root <dir> --messages <n>";

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (mut root, mut messages) = (None, None);
    let mut it = args.iter().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--data-root" => root = it.next().map(PathBuf::from),
            "--messages" => messages = it.next().and_then(|v| v.parse::<u64>().ok()),
            _ => messages = None,
        }
    }
    let (Some("seed"), Some(root), Some(messages)) = (args.first().map(String::as_str), root, messages) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    match outlooker_loadgen::load::seed(&root, messages).await {
        Ok(r) => {
            println!("messages_stored={} occurrences={}", r.messages_stored, r.occurrences);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("seed failed: {e}");
            ExitCode::FAILURE
        }
    }
}
```


- [ ] **Step 49: Run the seed command**

```bash
cd src-tauri && cargo run -p outlooker-loadgen -- seed --data-root ../scripts/tmp/seed-check --messages 2000; echo "RC=$?"; cd ..
ls scripts/tmp/seed-check
cd src-tauri && cargo run -p outlooker-loadgen -- seed --data-root ../scripts/tmp/seed-check --messages 2000; echo "RC=$?"; cd ..
rm -rf scripts/tmp/seed-check
```

Expected: first run prints `messages_stored=1971 occurrences=2000` and `RC=0`; `ls` shows `auth`, `outlooker-data.json`, `pgdata`; the second run on the same directory fails with `seed failed: invalid input dataRoot: notEmpty` and `RC=1`. The last line removes only the directory this step created.

- [ ] **Step 50: Create the perf gates `src-tauri/crates/outlooker-loadgen/tests/perf.rs`**

```rust
//! Perf gates (spec Part 04 §12.3, Part 02 §9). All `#[ignore]`; run one at a time on the reference machine:
//! `cargo test -p outlooker-loadgen --release -- --ignored --test-threads 1 <name>`; `perf_generate` first.
//! Each gate appends its result to `docs/logs/loadtest-<date>.md` or `docs/logs/perf-<date>.md`, then asserts the
//! threshold. Thresholds are the spec values and are never changed after a run.
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use outlooker_core::dto::error::AppError;
use outlooker_core::dto::search::{SearchQuery, SearchRequest};
use outlooker_core::importer::remove;
use outlooker_core::search::mail::IMAGE_URL_PREFIX;
use outlooker_core::search::{compile, SearchService};
use outlooker_loadgen::corpus::Corpus;
use outlooker_loadgen::generate::GenConfig;
use outlooker_loadgen::load::{load, load_sources, source_path};
use outlooker_loadgen::perf::{canned_queries, explain, generate_json, ms, open_library, percentile, perf_root, record, repo_root, Log};
use outlooker_loadgen::rng::Rng;
use outlooker_loadgen::root::LibraryRoot;

const MB: f64 = 1024.0 * 1024.0;

fn req(q: &SearchQuery, cursor: Option<String>) -> SearchRequest {
    SearchRequest { query: q.clone(), cursor, page_size: Some(200) }
}

async fn psts(lib: &LibraryRoot) -> Vec<i64> {
    let a = lib.db.connect_admin().await.unwrap();
    a.query("SELECT id FROM pst_sources ORDER BY id", &[]).await.unwrap().iter().map(|r| r.get(0)).collect()
}

/// First page of every canned query: `warmups` untimed runs, then `runs` timed runs each.
async fn first_pages(svc: &SearchService, qs: &[(String, SearchQuery)], warmups: usize, runs: usize) -> (Vec<Duration>, String) {
    let mut samples = Vec::new();
    let mut per_query = String::new();
    for (label, q) in qs {
        for _ in 0..warmups {
            svc.search(req(q, None)).await.unwrap();
        }
        let mut mine = Vec::new();
        for _ in 0..runs {
            let t = Instant::now();
            svc.search(req(q, None)).await.unwrap_or_else(|e| panic!("{label}: {e:?}"));
            mine.push(t.elapsed());
        }
        per_query.push_str(&format!("{label}: max {}\n", ms(*mine.iter().max().unwrap())));
        samples.extend(mine);
    }
    (samples, per_query)
}

fn read_generate() -> serde_json::Value {
    let text = std::fs::read_to_string(generate_json()).expect("perf-generate.json missing: run perf_generate first");
    serde_json::from_str(&text).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn perf_generate() {
    let root = perf_root();
    assert!(!root.join("outlooker-data.json").exists(), "perf library already exists at {}: delete it to regenerate", root.display());
    let lib = LibraryRoot::create(&root).await.unwrap();
    let r = load(&lib.db, &GenConfig::full()).await.unwrap();
    let json = serde_json::json!({
        "messages_submitted": r.messages_submitted,
        "duplicates_planned": r.duplicates_planned,
        "messages_stored": r.messages_stored,
        "occurrences": r.occurrences,
        "write_secs": r.write_elapsed.as_secs_f64(),
        "index_secs": r.index_elapsed.as_secs_f64(),
    });
    std::fs::write(generate_json(), serde_json::to_vec_pretty(&json).unwrap()).unwrap();
    println!("{json}");
    lib.stop().await.unwrap();
    assert_eq!(r.messages_submitted, 1_000_000);
}

#[test]
#[ignore]
fn perf_writer_throughput() {
    let g = read_generate();
    let rate = g["messages_submitted"].as_f64().unwrap() / g["write_secs"].as_f64().unwrap();
    let pass = rate >= 1_500.0;
    record(Log::Loadtest, "Writer throughput, deferred indexes", ">= 1,500 messages/s", &format!("{rate:.0} messages/s"), pass, &g.to_string());
    assert!(pass, "{rate:.0} messages/s");
}

#[test]
#[ignore]
fn perf_e2e_import() {
    let pst = std::env::var("OUTLOOKER_PERF_PST").expect("set OUTLOOKER_PERF_PST to a large real PST (not in git)");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let out = std::process::Command::new(cargo)
        .args(["run", "--release", "-p", "outlooker-core", "--features", "test-support", "--example", "import_bench", "--", &pst])
        .current_dir(repo_root().join("src-tauri"))
        .output()
        .expect("run import_bench");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "import_bench failed: {}", String::from_utf8_lossy(&out.stderr));
    let last = stdout.lines().last().unwrap_or_default();
    let field = |name: &str| -> f64 {
        last.split_whitespace()
            .find_map(|kv| kv.strip_prefix(name).and_then(|v| v.strip_prefix('=')))
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(|| panic!("last line lacks {name}=: {last}"))
    };
    let (rate, rss) = (field("messages_per_sec"), field("peak_rss_bytes"));
    let pass = rate >= 150.0 && rss <= 1.5 * 1024.0 * MB;
    let measured = format!("{rate:.0} messages/s, peak RSS {:.0} MB", rss / MB);
    record(Log::Loadtest, "End-to-end import incl. PST read + WebP", ">= 150 messages/s; peak RSS <= 1.5 GB", &measured, pass, &stdout);
    assert!(pass, "{measured}");
}

#[test]
#[ignore]
fn perf_index_rebuild() {
    let g = read_generate();
    let secs = g["index_secs"].as_f64().unwrap();
    let pass = secs <= 30.0 * 60.0;
    record(Log::Loadtest, "Index rebuild (all six) at 1M", "<= 30 min", &format!("{:.1} min", secs / 60.0), pass, "");
    assert!(pass, "{secs:.0} s");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn perf_db_size() {
    let lib = open_library().await;
    let a = lib.db.connect_admin().await.unwrap();
    a.batch_execute("VACUUM (ANALYZE)").await.unwrap();
    let size = |sql: &'static str| {
        let a = &a;
        async move { a.query_one(sql, &[]).await.unwrap().get::<_, i64>(0) as f64 }
    };
    let messages = size("SELECT count(*) FROM messages").await;
    let per_100k = |bytes: f64| bytes / MB * 100_000.0 / messages;
    // (line of Part 04 §6.7, expected MB per 100k, measured bytes)
    let lines = [
        ("messages heap", 80.0, size("SELECT pg_relation_size('messages')").await),
        // One TOAST relation holds all three columns: body_tsv is measured by its stored value sizes and subtracted.
        (
            "messages TOAST (body_html + body_text)",
            700.0,
            size(
                "SELECT pg_total_relation_size(c.reltoastrelid) - (SELECT coalesce(sum(pg_column_size(body_tsv)), 0)::int8 FROM messages) \
                 FROM pg_class c WHERE c.relname = 'messages'",
            )
            .await,
        ),
        ("TOAST body_tsv", 250.0, size("SELECT coalesce(sum(pg_column_size(body_tsv)), 0)::int8 FROM messages").await),
        ("messages_body_trgm", 300.0, size("SELECT pg_relation_size('messages_body_trgm')").await),
        ("messages_body_fts", 150.0, size("SELECT pg_relation_size('messages_body_fts')").await),
        (
            "subject/sender/recipients trigram indexes",
            50.0,
            size("SELECT pg_relation_size('messages_subject_trgm') + pg_relation_size('messages_sender_trgm') + pg_relation_size('messages_recipients_trgm')").await,
        ),
        ("message_recipients + indexes", 80.0, size("SELECT pg_total_relation_size('message_recipients')").await),
        (
            "occurrences, attachments, inline_images + indexes",
            40.0,
            size("SELECT pg_total_relation_size('message_occurrences') + pg_total_relation_size('attachments') + pg_total_relation_size('inline_images')").await,
        ),
        ("image_blobs", 1_500.0, size("SELECT pg_total_relation_size('image_blobs')").await),
    ];
    let mut details = String::new();
    let mut pass = true;
    for (name, expected, bytes) in lines {
        let got = per_100k(bytes);
        let ok = got >= 0.5 * expected && got <= 2.0 * expected;
        pass &= ok;
        details.push_str(&format!("{name}: {got:.0} MB per 100k (expected {expected:.0}) {}\n", if ok { "ok" } else { "OUT OF RANGE" }));
    }
    let total = per_100k(size("SELECT pg_database_size(current_database())").await);
    details.push_str(&format!("total database: {total:.0} MB per 100k (expected 2,900)\n"));
    record(Log::Loadtest, "DB size per 100k", "each line 0.5x-2x of Part 04 §6.7; total reported", &format!("total {total:.0} MB per 100k"), pass, &details);
    lib.stop().await.unwrap();
    assert!(pass, "{details}");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn perf_search_first_page_warm() {
    let lib = open_library().await;
    let qs = canned_queries(&Corpus::new(), &psts(&lib).await);
    let svc = SearchService::new(lib.db.clone());
    let (samples, mut details) = first_pages(&svc, &qs, 1, 3).await;
    let mut client = lib.db.connect_app().await.unwrap();
    for (label, q) in &qs {
        details.push_str(&format!("\n== {label}\n{}\n", explain(&mut client, &compile::compile(q).unwrap()).await));
    }
    let p95 = percentile(&samples, 0.95);
    let pass = p95 <= Duration::from_millis(500);
    record(Log::Loadtest, "search_mail first page, warm cache (30 canned queries)", "p95 <= 500 ms", &format!("p95 {}", ms(p95)), pass, &details);
    lib.stop().await.unwrap();
    assert!(pass, "p95 {}", ms(p95));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn perf_search_first_page_cold() {
    let lib = open_library().await;
    lib.stop().await.unwrap(); // server restart: PostgreSQL shared buffers are empty
    let lib = open_library().await;
    let qs = canned_queries(&Corpus::new(), &psts(&lib).await);
    let svc = SearchService::new(lib.db.clone());
    let (samples, details) = first_pages(&svc, &qs, 0, 1).await;
    let p95 = percentile(&samples, 0.95);
    let pass = p95 <= Duration::from_secs(2);
    record(Log::Loadtest, "search_mail first page, cold start (after server restart)", "p95 <= 2 s", &format!("p95 {}", ms(p95)), pass, &details);
    lib.stop().await.unwrap();
    assert!(pass, "p95 {}", ms(p95));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn perf_short_query() {
    let lib = open_library().await;
    let svc = SearchService::new(lib.db.clone());
    let mut samples = Vec::new();
    for term in ["報價", "會議", "合約", "客戶", "專案"] {
        let q = SearchQuery::Simple { text: term.into() };
        for _ in 0..3 {
            let t = Instant::now();
            let page = svc.search(req(&q, None)).await.unwrap();
            samples.push(t.elapsed());
            assert!(page.short_query);
        }
    }
    let p95 = percentile(&samples, 0.95);
    let rare = SearchQuery::Simple { text: "qz".into() };
    let t = Instant::now();
    let outcome = svc.search(req(&rare, None)).await;
    let rare_elapsed = t.elapsed();
    let rare_ok = matches!(outcome, Ok(_) | Err(AppError::QueryTimeout { timeout_ms: 20_000 })) && rare_elapsed <= Duration::from_millis(20_500);
    let pool_ok = svc.filter_options().await.is_ok(); // the pool stays healthy after a timeout
    let pass = p95 <= Duration::from_secs(1) && rare_ok && pool_ok;
    let measured = format!("common p95 {}; rare {} ({})", ms(p95), ms(rare_elapsed), if outcome.is_ok() { "results" } else { "queryTimeout" });
    record(Log::Loadtest, "short query (2 chars)", "common p95 <= 1 s; rare: queryTimeout or results within 20 s", &measured, pass, "");
    lib.stop().await.unwrap();
    assert!(pass, "{measured} pool_ok={pool_ok}");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn perf_search_count() {
    let lib = open_library().await;
    let qs = canned_queries(&Corpus::new(), &psts(&lib).await);
    let svc = SearchService::new(lib.db.clone());
    let mut samples = Vec::new();
    for (label, q) in &qs {
        svc.count(q.clone()).await.unwrap_or_else(|e| panic!("{label}: {e:?}"));
        for _ in 0..3 {
            let t = Instant::now();
            svc.count(q.clone()).await.unwrap();
            samples.push(t.elapsed());
        }
    }
    let p95 = percentile(&samples, 0.95);
    let pass = p95 <= Duration::from_secs(2);
    record(Log::Loadtest, "search_count (30 canned queries)", "p95 <= 2 s", &format!("p95 {}", ms(p95)), pass, "");
    lib.stop().await.unwrap();
    assert!(pass, "p95 {}", ms(p95));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn perf_remove_source() {
    let lib = open_library().await;
    let app = lib.db.connect_app().await.unwrap();
    let path = source_path(9);
    let pst_id: i64 = app.query_one("SELECT id FROM pst_sources WHERE path = $1", &[&path]).await.expect("source-09 present").get(0);
    let t = Instant::now();
    let stats = remove::run(&app, pst_id).await.unwrap();
    let elapsed = t.elapsed();
    let pass = elapsed <= Duration::from_secs(300);
    record(Log::Loadtest, "Remove a 100k-message source", "<= 5 min", &format!("{:.1} s", elapsed.as_secs_f64()), pass, &format!("{stats:?}"));
    // restore the library for the next gates (indexes kept: incremental write)
    load_sources(&lib.db, &GenConfig::full(), 9..10, false).await.unwrap();
    lib.stop().await.unwrap();
    assert!(pass, "{elapsed:?}");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn perf_incremental_import() {
    let lib = open_library().await;
    let qs = canned_queries(&Corpus::new(), &psts(&lib).await);
    let svc = SearchService::new(lib.db.clone());
    let done = Arc::new(AtomicBool::new(false));
    let searcher = {
        let (svc, done, qs) = (svc.clone(), done.clone(), qs.clone());
        tokio::spawn(async move {
            let mut samples = Vec::new();
            for (_, q) in qs.iter().cycle() {
                if done.load(Ordering::Relaxed) {
                    break;
                }
                let t = Instant::now();
                svc.search(req(q, None)).await.unwrap();
                samples.push(t.elapsed());
            }
            samples
        })
    };
    // a 50k-message 11th source; sources 0..=9 keep their 100k items
    let cfg = GenConfig { messages: 550_000, sources: 11, ..GenConfig::full() };
    let report = load_sources(&lib.db, &cfg, 10..11, false).await.unwrap();
    done.store(true, Ordering::Relaxed);
    let samples = searcher.await.unwrap();
    let rate = report.messages_submitted as f64 / report.write_elapsed.as_secs_f64();
    let p95 = percentile(&samples, 0.95);
    let pass = rate >= 300.0 && p95 <= Duration::from_secs(1);
    let measured = format!("{rate:.0} messages/s; searches during import p95 {} ({} samples)", ms(p95), samples.len());
    record(Log::Loadtest, "Incremental import of 50k into a 1M library (indexes kept)", ">= 300 messages/s; searches p95 <= 1 s", &measured, pass, "");
    // back to the 1M library
    let app = lib.db.connect_app().await.unwrap();
    let path = source_path(10);
    let id: i64 = app.query_one("SELECT id FROM pst_sources WHERE path = $1", &[&path]).await.unwrap().get(0);
    remove::run(&app, id).await.unwrap();
    lib.stop().await.unwrap();
    assert!(pass, "{measured}");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn perf_search_next_page() {
    let lib = open_library().await;
    let qs = canned_queries(&Corpus::new(), &psts(&lib).await);
    let svc = SearchService::new(lib.db.clone());
    let mut samples = Vec::new();
    for (_, q) in &qs {
        let mut cursor = svc.search(req(q, None)).await.unwrap().next_cursor;
        for _ in 2..=5 {
            let Some(c) = cursor.take() else { break };
            let t = Instant::now();
            cursor = svc.search(req(q, Some(c))).await.unwrap().next_cursor;
            samples.push(t.elapsed());
        }
    }
    let p95 = percentile(&samples, 0.95);
    let pass = p95 <= Duration::from_millis(100);
    record(Log::Perf, "search_mail next page (pages 2-5)", "p95 <= 100 ms", &format!("p95 {} ({} samples)", ms(p95), samples.len()), pass, "");
    lib.stop().await.unwrap();
    assert!(pass, "p95 {}", ms(p95));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn perf_mail_get_first_image() {
    let lib = open_library().await;
    let a = lib.db.connect_admin().await.unwrap();
    let ids: Vec<i64> = a.query("SELECT id FROM messages", &[]).await.unwrap().iter().map(|r| r.get(0)).collect();
    let svc = SearchService::new(lib.db.clone());
    let mut rng = Rng::new(99);
    let mut samples = Vec::new();
    let mut with_image = 0;
    for _ in 0..1_000 {
        let id = ids[rng.below(ids.len() as u64) as usize];
        let t = Instant::now();
        let mail = svc.mail_get(id).await.unwrap();
        let first = mail.body_html.as_deref().and_then(|h| h.split(IMAGE_URL_PREFIX).nth(1)).and_then(|s| s.split('"').next()).and_then(|s| s.parse::<i64>().ok());
        if let Some(blob) = first {
            assert!(svc.image(blob).await.unwrap().is_some());
            with_image += 1;
        }
        samples.push(t.elapsed());
    }
    let p95 = percentile(&samples, 0.95);
    let pass = p95 <= Duration::from_millis(100);
    record(Log::Perf, "mail_get + first mailimg image (1,000 random ids)", "p95 <= 100 ms", &format!("p95 {} ({with_image} with an image)", ms(p95)), pass, "");
    lib.stop().await.unwrap();
    assert!(pass, "p95 {}", ms(p95));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn perf_search_first_page_p50() {
    let lib = open_library().await;
    let qs = canned_queries(&Corpus::new(), &psts(&lib).await);
    let svc = SearchService::new(lib.db.clone());
    let (samples, details) = first_pages(&svc, &qs, 1, 3).await;
    let p50 = percentile(&samples, 0.50);
    let pass = p50 <= Duration::from_millis(150);
    record(Log::Perf, "search_mail first page p50 (30 canned queries, warm)", "p50 <= 150 ms", &format!("p50 {}", ms(p50)), pass, &details);
    lib.stop().await.unwrap();
    assert!(pass, "p50 {}", ms(p50));
}
```


- [ ] **Step 51: Check the perf test list and the non-ignored suite**

Run: `cd src-tauri && cargo test -p outlooker-loadgen --release -- --ignored --list`
Expected (sorted): `perf_db_size`, `perf_e2e_import`, `perf_generate`, `perf_incremental_import`, `perf_index_rebuild`, `perf_mail_get_first_image`, `perf_remove_source`, `perf_search_count`, `perf_search_first_page_cold`, `perf_search_first_page_p50`, `perf_search_first_page_warm`, `perf_search_next_page`, `perf_short_query`, `perf_writer_throughput` (14 = `perf_generate` + 13 gates), each followed by `: test`.

Run: `cd src-tauri && cargo test -p outlooker-loadgen`
Expected: lib `7 passed`, smoke `2 passed`, perf `0 passed; 14 ignored`.

The gates themselves are run later by the gate-runner on the reference machine, one test per command (`cargo test -p outlooker-loadgen --release -- --ignored --exact <name>`), `perf_generate` first; `perf_e2e_import` needs `OUTLOOKER_PERF_PST` and the R2-09 `import_bench` example, whose last stdout line is `import_bench messages_per_sec=<f64> peak_rss_bytes=<u64>`.

- [ ] **Step 52: Commit**

Write the message file `scripts/tmp/r210b-msg.txt` (Write tool; `scripts/tmp/` is gitignored, the session hook blocks `/tmp`) with: `feat(loadgen): 1M load generator, E2E seed, perf gates (R2-10)`, a blank line, and the attribution line of the session.

```bash
git rev-parse --abbrev-ref HEAD
git add src-tauri/crates/outlooker-loadgen
git commit -F scripts/tmp/r210b-msg.txt -- src-tauri/crates/outlooker-loadgen
git status --porcelain
```

Expected: the commit succeeds; `git status --porcelain` lists none of the paths above.

## Task R2-11: Search Service

**Files:** (`core/` = `src-tauri/crates/outlooker-core/`)
- Create/own (replace kickoff stubs): `core/src/search/mod.rs`, `core/src/search/sql.rs`, `core/src/search/compile.rs`, `core/src/search/cursor.rs`, `core/src/search/execute.rs`, `core/src/search/filter.rs`, `core/src/search/mail.rs`, `core/src/search/image.rs`
- Own in Round 2: `src-tauri/schemas/condition-group.json` (no change planned; any schema fix requested by others lands here)
- Create: `core/tests/search.rs`
- Unit tests: `#[cfg(test)] mod tests` in `compile.rs`, `cursor.rs`, `mail.rs` (selected by `cargo test -p outlooker-core search::`)

**Interfaces:**
- Consumes: C1 `dto::search::*`, `dto::error::AppError`; C8 `db::Db::app_pool()`, `db::{pg_err, RFC3339_UTC_FMT}`, the stored generated column `messages.body_tsv` (R1-04 migration, spec Part 04 §2; `messages_body_fts` indexes it; the compiler emits `m.body_tsv` and never `TSV_EXPR`, so no phase recomputes a tsvector), `test_support::TestCluster::{start, admin, app, db}`; crate `base64` 0.23.1 (`URL_SAFE_NO_PAD`). Tests only: R2-10 `test_support::import_fixtures::{seed_mails, mail, item, add_recipient}` and C9 `importer::types` / `importer::webp::source_hash` to seed rows through the real writer (R2-10 lands first).
- Produces (C13, C14 SearchService; consumers R2-12 (C13), R2-13 (facade, `mailimg` handler)):

```rust
// core/src/search/mod.rs
#[derive(Clone)]
pub struct SearchService { /* db: Db */ }
impl SearchService {
    pub fn new(db: crate::db::Db) -> SearchService;
    pub async fn search(&self, req: SearchRequest) -> Result<SearchPage, AppError>;      // search_mail
    pub async fn count(&self, query: SearchQuery) -> Result<SearchCount, AppError>;      // search_count
    pub async fn filter_options(&self) -> Result<FilterOptions, AppError>;
    pub async fn mail_get(&self, id: i64) -> Result<MailDetail, AppError>;
    /// `mailimg` scheme body: WebP bytes of image_blobs.id, None when unknown.
    pub async fn image(&self, id: i64) -> Result<Option<Vec<u8>>, AppError>;
}
/// C13. Loads MailRow for `ids` with the hydrate SQL of Part 04 §7.4; result keeps the order of `ids`
/// (duplicates repeated), ids with no message are skipped. Runs on any outlooker_app or admin connection.
pub async fn hydrate_rows(client: &tokio_postgres::Client, ids: &[i64]) -> Result<Vec<MailRow>, AppError>;

// core/src/search/compile.rs
#[derive(Debug, Clone, PartialEq)]
pub enum Param { Text(String), Bool(bool), TextArray(Vec<String>), BigintArray(Vec<i64>) }
impl Param { pub fn as_sql(&self) -> &(dyn tokio_postgres::types::ToSql + Sync); }
#[derive(Debug, Clone, PartialEq)]
pub struct Compiled { pub predicate: String, pub params: Vec<Param>, pub indexable: bool, pub short_query: bool }
pub const MAX_TERMS: usize = 10; pub const MAX_DEPTH: usize = 5; pub const MAX_LEAVES: usize = 50;
pub const MAX_VALUE_CHARS: usize = 500; pub const MIN_INDEXABLE_CHARS: usize = 3;
pub fn compile(query: &SearchQuery) -> Result<Compiled, AppError>;   // params are $1..$k
pub fn escape_like(v: &str) -> String;                                // \ → \\, % → \%, _ → \_
pub fn like(v: &str) -> String;                                       // "%" + escape_like(v) + "%"
pub fn is_rfc3339(s: &str) -> bool;                                   // dateRange from / to check
pub const MAX_FOLDER_PATH_CHARS: usize = 4096; pub const MAX_PST_IDS: usize = 100;

// core/src/search/cursor.rs
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorKey { pub sort_at: Option<String>, pub id: i64 }    // sort_at = PostgreSQL text of messages.sort_at; None = -infinity
pub fn encode(k: &CursorKey) -> String;                               // base64url (no padding) of {"s":<text|null>,"i":<id>}
pub fn decode(s: &str) -> Result<CursorKey, AppError>;                // invalidInput{field:"cursor", reason:"invalid"}
impl CursorKey { pub fn from_db(sort_at: String, id: i64) -> CursorKey; pub fn sort_text(&self) -> &str; }   // "-infinity" ↔ None

// core/src/search/execute.rs
pub const WINDOW: i64 = 5_000;
pub const COUNT_CAP: i64 = 10_000;
pub const TIMEOUT_MS: u32 = 10_000;
pub const SHORT_TIMEOUT_MS: u32 = 20_000;
#[derive(Debug, Clone, PartialEq)]
pub struct PageIds { pub rows: Vec<(i64, CursorKey)>, pub phase_b_ran: bool }   // up to n rows, newest first
/// Part 04 §7.4 inside one transaction on `client`. n = page_size + 1. `window` = W (production WINDOW).
pub async fn page(client: &mut tokio_postgres::Client, c: &Compiled, after: Option<&CursorKey>, n: i64, window: i64)
    -> Result<PageIds, AppError>;
pub async fn count(client: &mut tokio_postgres::Client, c: &Compiled) -> Result<SearchCount, AppError>;
/// Statement texts (also used by the loadgen perf harness for EXPLAIN): ordered / phase B, phase A, capped count;
/// `bind` = the predicate values followed by the extra placeholders.
pub fn ordered_sql(c: &Compiled) -> String;
pub fn phase_a_sql(c: &Compiled) -> String;
pub fn count_sql(c: &Compiled) -> String;
pub fn bind<'a>(c: &'a Compiled, extra: &[&'a (dyn ToSql + Sync)]) -> Vec<&'a (dyn ToSql + Sync)>;

// core/src/search/mod.rs (test seam used by core/tests/search.rs and SearchService::search with WINDOW)
pub async fn search_page(client: &mut tokio_postgres::Client, req: &SearchRequest, window: i64)
    -> Result<(SearchPage, bool /*phase_b_ran*/), AppError>;

// core/src/search/mail.rs
pub const IMAGE_URL_PREFIX: &str;   // "mailimg://localhost/" (macOS/Linux) or "http://mailimg.localhost/" (Windows), cfg(windows)
pub fn rewrite_inline(html: &str, ordinal_to_blob: &std::collections::HashMap<i32, i64>) -> String;
```

**Behavior:**
- `compile` (Part 04 §7.3): simple query → trim, split on Unicode whitespace; 0 terms → `invalidInput{field:"query.text", reason:"empty"}`; > 10 → `reason:"tooManyTerms"`; a term > 500 chars → `reason:"tooLong"`; compiled as `and(or(sender t, recipient t [], subject t, body t) per term)` through the same tree function. DSL → walk with JSON paths `query.root`, `query.root.children[i]`, `….value` etc.; limits: depth (root = 1) > 5 → `{field:<group path>, reason:"tooDeep"}`; total leaves > 50 → `{field:"query.root", reason:"tooManyLeaves"}`; empty `children` → `{field:<path>.children, reason:"empty"}`; text value trimmed 0 chars → `reason:"empty"`, > 500 → `reason:"tooLong"`; `dateRange` with both null → `{field:<path>, reason:"empty"}`; `from` / `to` not RFC 3339 → `{field:<path>.from|to, reason:"invalidInstant"}`; `folderPath` 1..=4096 chars else `reason:"empty"|"tooLong"`; `pstIds` 1..=100, each ≥ 1, unique, else `{field:<path>.pstIds, reason:"invalid"}`.
- Fragments exactly as the table of Part 04 §7.3 with `m.` columns; text values bound as `like(value.trim())`; body = `(m.body_tsv @@ websearch_to_tsquery('english', $q) OR m.body_text ILIKE $p)` (stored column, tsvector branch written first; `$p` is bound before `$q`); tsvector branch only when the trimmed value contains `[A-Za-z]`, bound as the raw trimmed value; `kinds` empty or all three → no EXISTS; strict subset → EXISTS with `$k::text[]` of `"to"|"cc"|"bcc"` sorted; date bounds `$p::timestamptz` (text parameter); folder `f.path = $p` (exact, not escaped) plus `OR f.path LIKE $pp` with `$pp = escape_like(path) + "/%"` when `includeSubfolders`; `sourcePst` `$p::bigint[]`. Indexable: text condition with ≥ 3 Unicode scalar values; `and` if any child; `or` if all children. `short_query` = any text condition with < 3 scalar values.
- `page`: `BEGIN`; `SET LOCAL statement_timeout = '10s'` or `'20s'` when `short_query`; mode "ordered" when not indexable (one query, cursor default `('infinity', 0)`); mode "two-phase" when indexable: phase A over the newest `window` rows; if it returns fewer than `n` rows, find the window end (`OFFSET window - 1 LIMIT 1` on `messages_sort` after the cursor); none → done; else phase B with `SET LOCAL enable_indexscan = off; SET LOCAL enable_seqscan = off;` and keyset `< (window end)`, `LIMIT n - |A|`; then `RESET enable_indexscan; RESET enable_seqscan;`; `COMMIT`. Cursor values are bound as `$x::timestamptz` text (`'infinity'` on the first page, `'-infinity'` for `sort_at: None`) and `$y::int8`. SQLSTATE `57014` → `queryTimeout{timeoutMs: 10000|20000}`; `22007`/`22008` on cursor text → `invalidInput{field:"cursor", reason:"invalid"}`.
- `search` (`search_mail`): page size default 200, valid 1..=500 else `invalidInput{field:"pageSize", reason:"outOfRange"}`; fetch `page_size + 1`; `next_cursor` = encode(key of row `page_size`) when `page_size + 1` rows came back; hydrate (`hydrate_rows`); `short_query` from the compiler; `elapsed_ms` around the whole call.
- `count`: `SELECT count(*) FROM (SELECT 1 FROM messages m WHERE <predicate> LIMIT 10001) s` in a transaction with `statement_timeout = '10s'` and, when indexable, `SET LOCAL enable_seqscan = off; SET LOCAL enable_indexscan = off`; 10,001 → `{count: 10000, capped: true}`; else `{count, capped: false}`.
- `filter_options`, `mail_get`, `image`: SQL of Part 04 §7.6. `mail_get`: unknown id → `notFound{entity:"mail", id}`; recipients split by kind in ordinal order; `body_html` rewritten by `rewrite_inline` (each ` src="outlooker-inline:<n>"` → ` src="<IMAGE_URL_PREFIX><blob id>"`, unknown ordinal → the attribute is removed); timestamps as RFC 3339 via `sql::RFC3339`.
- Every method takes a connection with `db.app_pool()?.get().await`; a pool error maps to `DbNotReady` (pool closed or backend unreachable: `PoolError::Closed` / `PoolError::Backend` with a closed connection) else `Internal`.

**Constraints:**
- SQL text only from `&'static str` fragments and `$n` (Global Constraints SQL rule). `predicate` never contains a user value; test-enforced.
- Only the `outlooker_app` pool. No writes.
- `IMAGE_URL_PREFIX` is the only platform branch (`#[cfg(windows)]`), as defined in Part 03 §5.6.
- Layer: service; depends on `dto`, `db`.

**Acceptance criteria:**
- [ ] `cargo test -p outlooker-core search::` exits 0 with `ok`: `search::compile::tests::no_user_value_in_sql` (for each of the 8 leaf types the `predicate` contains none of the input values and `params` equal the escaped values), `search::compile::tests::like_escapes_literally`, `search::compile::tests::limits_paths`, `search::compile::tests::worked_example_predicate` (Part 03 §5.1 example compiles to the spec's predicate shape, `short_query == true`, `indexable == true`), `search::cursor::tests::roundtrip_and_infinity`, `search::mail::tests::rewrite_known_and_unknown`, and also `search::compile::tests::simple_query_shape_and_tsquery_rule`, `search::compile::tests::rfc3339_validation`.
- [ ] `cargo test -p outlooker-core --features test-support --test search` exits 0 with `ok`: `semantics_short_and_cjk` (`報價` → `shortQuery: true`; `報價單` matches), `semantics_stem` (`invoice` matches a body with only `invoices`; strengthened: `invoices` matches a body with only `invoice`, which no substring match can do), `semantics_partial_sender_recipient`, `semantics_worked_example` (returns exactly the expected ids), `literal_percent_underscore_backslash` (`%`, `_`, `\` match only literally on seeded rows), `count_capped` (10,001 matches → `{count:10000, capped:true}`; 9,999 → `{9999, false}`), `mail_get_unknown_is_not_found`, `mail_get_image_urls_platform_form`, `two_phase_dense_phase_a_only` (W = 20; result equals the reference `ORDER BY sort_at DESC, id DESC LIMIT n`; `phase_b_ran == false`), `two_phase_sparse_runs_phase_b` (`phase_b_ran == true`, equals the reference), `two_phase_pages_no_gap_no_overlap` (pages 1–3 across the boundary concatenate to the reference), `null_sent_at_last_and_paged`, `reset_after_phase_b` (`SHOW enable_indexscan` and `SHOW enable_seqscan` = `on` on the same connection; this observes a leak into the session, the `RESET` inside the transaction is not observable from outside), `explain_uses_indexes` (after `ANALYZE` and `SET enable_seqscan = off`: `ILIKE '%報價單%'` plan contains `messages_body_trgm`; English body term contains `messages_body_fts`; sender contains `messages_sender_trgm`; recipient contains `messages_recipients_trgm`), `image_returns_bytes_or_none`, `filter_options_lists_psts_and_folders`.

### R2-11 — Stage 2 steps

All code below was compiled with rustc 1.99.1 (edition 2024) and its tests were run in the scratch workspace against PostgreSQL 18.6 (unit 8, integration 16, all `ok`). Mutation checks (no LIKE escaping; phase B disabled) turned `literal_percent_underscore_backslash`, `two_phase_sparse_runs_phase_b`, `two_phase_pages_no_gap_no_overlap` and `reset_after_phase_b` red. R2-10 (writer + `test_support::import_fixtures`) lands first: the integration tests seed rows through the real writer. All `cargo` commands run in `src-tauri/`.

- [ ] **Step 1: Create `src-tauri/crates/outlooker-core/src/search/sql.rs` (constants, no tests)**

```rust
//! Constant SQL of the read paths (spec Part 04 §7.4, §7.6). Timestamps leave as RFC 3339 text through
//! `to_char(... AT TIME ZONE 'UTC', $fmt)` with `$fmt` = `db::RFC3339_UTC_FMT` bound as a parameter.

/// $1 = ids (bigint[]), $2 = RFC3339_UTC_FMT.
pub const HYDRATE: &str = "SELECT m.id, to_char(m.sent_at AT TIME ZONE 'UTC', $2), m.sender_name, a.email, m.to_preview,
       m.recipient_count, m.subject, m.snippet, m.has_attachments,
       (SELECT count(*) FROM message_occurrences o WHERE o.message_id = m.id)
FROM messages m LEFT JOIN addresses a ON a.id = m.sender_address_id
WHERE m.id = ANY($1)";

pub const FILTER_PSTS: &str = "SELECT id, display_name, path FROM pst_sources ORDER BY display_name, id";
pub const FILTER_FOLDERS: &str = "SELECT f.path, f.pst_id, count(o.message_id) FROM folders f
JOIN message_occurrences o ON o.folder_id = f.id GROUP BY f.pst_id, f.path ORDER BY f.pst_id, f.path";

/// $1 = id, $2 = RFC3339_UTC_FMT.
pub const MAIL: &str = "SELECT m.id, m.message_id, m.subject, to_char(m.sent_at AT TIME ZONE 'UTC', $2),
       to_char(m.received_at AT TIME ZONE 'UTC', $2), m.sender_name, a.email, m.body_html, m.body_text
FROM messages m LEFT JOIN addresses a ON a.id = m.sender_address_id WHERE m.id = $1";
pub const MAIL_RECIPIENTS: &str = "SELECT r.kind, r.display_name, a.email FROM message_recipients r
LEFT JOIN addresses a ON a.id = r.address_id WHERE r.message_id = $1 ORDER BY r.ordinal";
pub const MAIL_ATTACHMENTS: &str =
    "SELECT id, file_name, size_bytes, mime_type FROM attachments WHERE message_id = $1 ORDER BY attachment_index";
pub const MAIL_OCCURRENCES: &str = "SELECT o.pst_id, s.display_name, s.path, f.path, o.pst_item_id FROM message_occurrences o
JOIN pst_sources s ON s.id = o.pst_id JOIN folders f ON f.id = o.folder_id
WHERE o.message_id = $1 ORDER BY o.pst_id, o.pst_item_id";
pub const MAIL_INLINE: &str = "SELECT ordinal, image_blob_id FROM inline_images WHERE message_id = $1";

pub const IMAGE: &str = "SELECT webp FROM image_blobs WHERE id = $1";

/// $1 = cursor sort_at text, $2 = cursor id, $3 = OFFSET (window - 1).
pub const WINDOW_END: &str = "SELECT sort_at::text, id FROM messages
WHERE (sort_at, id) < ($1::text::timestamptz, $2::int8) ORDER BY sort_at DESC, id DESC OFFSET $3 LIMIT 1";
```


- [ ] **Step 2: Replace `src-tauri/crates/outlooker-core/src/search/mod.rs` with the module list and the error helper**

Temporary content until the last steps of this task replace it:

```rust
pub mod compile;
pub mod cursor;
pub mod execute;
pub mod filter;
pub mod image;
pub mod mail;
pub mod sql;

use crate::dto::error::AppError;

pub(crate) fn db_err(e: tokio_postgres::Error) -> AppError {
    if e.is_closed() || (e.as_db_error().is_none() && std::error::Error::source(&e).is_some_and(|s| s.is::<std::io::Error>())) {
        return AppError::DbNotReady;
    }
    crate::db::pg_err(e)
}
```

Create `execute.rs`, `filter.rs` and `image.rs` of `search/` empty for now (the kickoff stubs may already exist; empty them).

- [ ] **Step 3: Write the failing tests in `src-tauri/crates/outlooker-core/src/search/compile.rs`**

Create the file with only this test module (the code under test does not exist yet):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::search::ConditionNode as N;

    fn dsl(op: GroupOp, children: Vec<ConditionNode>) -> SearchQuery {
        SearchQuery::Dsl { root: ConditionGroup { op, children } }
    }

    fn text_params(c: &Compiled) -> Vec<String> {
        c.params
            .iter()
            .filter_map(|p| match p {
                Param::Text(t) => Some(t.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn no_user_value_in_sql() {
        let marker = "zq'x%_\\";
        let cases: Vec<(ConditionNode, Vec<Param>)> = vec![
            (N::Sender { value: marker.into() }, vec![Param::Text(like(marker))]),
            (N::Recipient { value: marker.into(), kinds: vec![RecipientKind::Cc] }, vec![Param::Text(like(marker)), Param::TextArray(vec!["cc".into()])]),
            (N::Subject { value: marker.into() }, vec![Param::Text(like(marker))]),
            (N::Body { value: marker.into() }, vec![Param::Text(like(marker)), Param::Text(marker.into())]),
            (
                N::DateRange { from: Some("2020-01-01T00:00:00Z".into()), to: Some("2021-01-01T00:00:00+08:00".into()) },
                vec![Param::Text("2020-01-01T00:00:00Z".into()), Param::Text("2021-01-01T00:00:00+08:00".into())],
            ),
            (
                N::Folder { folder_path: format!("Inbox/{marker}"), include_subfolders: true },
                vec![Param::Text(format!("Inbox/{marker}")), Param::Text(format!("Inbox/{}/%", escape_like(marker)))],
            ),
            (N::HasAttachment { value: true }, vec![Param::Bool(true)]),
            (N::SourcePst { pst_ids: vec![424242, 7] }, vec![Param::BigintArray(vec![424242, 7])]),
        ];
        for (node, expected) in cases {
            let c = compile(&dsl(GroupOp::And, vec![node.clone()])).unwrap();
            for needle in ["zq", "2020-01-01", "2021", "Inbox", "424242"] {
                assert!(!c.predicate.contains(needle), "{needle} leaked into {}", c.predicate);
            }
            // the only quoted literals allowed are the constants of the fragment table
            let stripped = c.predicate.replace("'english'", "").replace("' '", "").replace("'-infinity'", "");
            assert!(!stripped.contains('\''), "{}", c.predicate);
            assert_eq!(c.params, expected, "{node:?}");
        }
    }

    #[test]
    fn like_escapes_literally() {
        assert_eq!(escape_like(r"50%_off\now"), r"50\%\_off\\now");
        assert_eq!(like("a%b"), r"%a\%b%");
        let c = compile(&SearchQuery::Simple { text: "100%".into() }).unwrap();
        assert_eq!(text_params(&c)[0], r"%100\%%");
    }

    #[test]
    fn limits_paths() {
        let err = |q: SearchQuery| compile(&q).unwrap_err();
        let inv = |f: &str, r: &str| AppError::invalid(f, r);
        assert_eq!(err(SearchQuery::Simple { text: "  ".into() }), inv("query.text", "empty"));
        assert_eq!(err(SearchQuery::Simple { text: "a ".repeat(11) }), inv("query.text", "tooManyTerms"));
        assert_eq!(err(SearchQuery::Simple { text: "a".repeat(501) }), inv("query.text", "tooLong"));
        assert_eq!(err(dsl(GroupOp::And, vec![])), inv("query.root.children", "empty"));
        assert_eq!(err(dsl(GroupOp::And, vec![N::Subject { value: " ".into() }])), inv("query.root.children[0].value", "empty"));
        assert_eq!(err(dsl(GroupOp::And, vec![N::Body { value: "x".repeat(501) }])), inv("query.root.children[0].value", "tooLong"));
        let mut deep = N::Subject { value: "abc".into() };
        for _ in 0..5 {
            deep = N::Group { op: GroupOp::And, children: vec![deep] };
        }
        assert_eq!(err(dsl(GroupOp::And, vec![deep])), inv("query.root.children[0].children[0].children[0].children[0].children[0]", "tooDeep"));
        let many: Vec<_> = (0..51).map(|_| N::HasAttachment { value: true }).collect();
        assert_eq!(err(dsl(GroupOp::And, many)), inv("query.root", "tooManyLeaves"));
        assert_eq!(err(dsl(GroupOp::And, vec![N::DateRange { from: None, to: None }])), inv("query.root.children[0]", "empty"));
        assert_eq!(
            err(dsl(GroupOp::And, vec![N::HasAttachment { value: true }, N::DateRange { from: Some("2020-13-01T00:00:00Z".into()), to: None }])),
            inv("query.root.children[1].from", "invalidInstant")
        );
        assert_eq!(err(dsl(GroupOp::And, vec![N::DateRange { from: None, to: Some("yesterday".into()) }])), inv("query.root.children[0].to", "invalidInstant"));
        assert_eq!(err(dsl(GroupOp::And, vec![N::Folder { folder_path: String::new(), include_subfolders: false }])), inv("query.root.children[0].folderPath", "empty"));
        assert_eq!(err(dsl(GroupOp::And, vec![N::Folder { folder_path: "f".repeat(4097), include_subfolders: false }])), inv("query.root.children[0].folderPath", "tooLong"));
        for ids in [vec![], vec![0], vec![3, 3], (1..=101).collect()] {
            assert_eq!(err(dsl(GroupOp::And, vec![N::SourcePst { pst_ids: ids }])), inv("query.root.children[0].pstIds", "invalid"));
        }
        // depth 5 is allowed
        let mut ok = N::Subject { value: "abc".into() };
        for _ in 0..3 {
            ok = N::Group { op: GroupOp::And, children: vec![ok] };
        }
        compile(&dsl(GroupOp::And, vec![ok])).unwrap();
    }

    #[test]
    fn worked_example_predicate() {
        let q = dsl(
            GroupOp::And,
            vec![
                N::Group { op: GroupOp::Or, children: vec![N::Sender { value: "contoso.com".into() }, N::Recipient { value: "alice".into(), kinds: vec![] }] },
                N::DateRange { from: Some("2022-12-31T16:00:00Z".into()), to: Some("2023-12-31T16:00:00Z".into()) },
                N::Body { value: "報價".into() },
                N::HasAttachment { value: true },
                N::SourcePst { pst_ids: vec![3, 7] },
            ],
        );
        let c = compile(&q).unwrap();
        assert_eq!(
            c.predicate,
            "((m.sender_search ILIKE $1 OR m.recipients_search ILIKE $2) AND (m.sort_at >= $3::text::timestamptz AND m.sort_at < $4::text::timestamptz) \
             AND m.body_text ILIKE $5 AND m.has_attachments = $6 AND EXISTS (SELECT 1 FROM message_occurrences o WHERE o.message_id = m.id \
             AND o.pst_id = ANY($7::bigint[])))"
        );
        assert_eq!(
            c.params,
            vec![
                Param::Text("%contoso.com%".into()),
                Param::Text("%alice%".into()),
                Param::Text("2022-12-31T16:00:00Z".into()),
                Param::Text("2023-12-31T16:00:00Z".into()),
                Param::Text("%報價%".into()),
                Param::Bool(true),
                Param::BigintArray(vec![3, 7]),
            ]
        );
        assert!(c.short_query, "報價 has 2 characters");
        assert!(c.indexable, "an AND with one indexable child is indexable");
    }

    #[test]
    fn simple_query_shape_and_tsquery_rule() {
        let c = compile(&SearchQuery::Simple { text: "invoice 報價單".into() }).unwrap();
        assert_eq!(
            c.predicate,
            "((m.sender_search ILIKE $1 OR m.recipients_search ILIKE $2 OR m.subject ILIKE $3 OR (m.body_tsv @@ \
             websearch_to_tsquery('english', $5) OR m.body_text ILIKE $4)) AND (m.sender_search ILIKE $6 OR m.recipients_search ILIKE $7 OR \
             m.subject ILIKE $8 OR m.body_text ILIKE $9))"
        );
        assert!(c.indexable && !c.short_query);
        let short = compile(&SearchQuery::Simple { text: "ab".into() }).unwrap();
        assert!(!short.indexable && short.short_query);
    }

    #[test]
    fn rfc3339_validation() {
        for ok in ["2023-01-01T00:00:00Z", "2023-01-01t10:20:30.123456z", "2023-06-30T23:59:60+08:00", "1999-12-31T00:00:00-05:30"] {
            assert!(is_rfc3339(ok), "{ok}");
        }
        for bad in ["2023-01-01", "2023-01-01 00:00:00Z", "2023-00-01T00:00:00Z", "2023-01-01T24:00:00Z", "2023-01-01T00:00:00", "2023-01-01T00:00:00.Z", "2023-01-01T00:00:00+0800"] {
            assert!(!is_rfc3339(bad), "{bad}");
        }
    }
}
```


- [ ] **Step 4: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test -p outlooker-core --lib search::compile`
Expected: compile error (cannot find function `compile` / `escape_like`).

- [ ] **Step 5: Implement `src-tauri/crates/outlooker-core/src/search/compile.rs`**

Insert this code above the test module (the file is this code followed by the tests of the previous step):

```rust
//! SearchQuery → one SQL predicate over `messages m` (spec Part 04 §7.3). The predicate text is built only from the
//! constant fragments below and `$n` placeholders; every user value travels in `params`.
use tokio_postgres::types::ToSql;

use crate::dto::error::AppError;
use crate::dto::search::{ConditionGroup, ConditionNode, GroupOp, RecipientKind, SearchQuery};

#[derive(Debug, Clone, PartialEq)]
pub enum Param {
    Text(String),
    Bool(bool),
    TextArray(Vec<String>),
    BigintArray(Vec<i64>),
}

impl Param {
    pub fn as_sql(&self) -> &(dyn ToSql + Sync) {
        match self {
            Param::Text(v) => v,
            Param::Bool(v) => v,
            Param::TextArray(v) => v,
            Param::BigintArray(v) => v,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Compiled {
    pub predicate: String,
    pub params: Vec<Param>,
    pub indexable: bool,
    pub short_query: bool,
}

pub const MAX_TERMS: usize = 10;
pub const MAX_DEPTH: usize = 5;
pub const MAX_LEAVES: usize = 50;
pub const MAX_VALUE_CHARS: usize = 500;
pub const MIN_INDEXABLE_CHARS: usize = 3;
pub const MAX_FOLDER_PATH_CHARS: usize = 4096;
pub const MAX_PST_IDS: usize = 100;

/// `\` → `\\`, `%` → `\%`, `_` → `\_` (backslash is the default LIKE escape character).
pub fn escape_like(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    for c in v.chars() {
        if matches!(c, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

pub fn like(v: &str) -> String {
    format!("%{}%", escape_like(v))
}

struct Out {
    sql: String,
    indexable: bool,
}

struct Builder {
    params: Vec<Param>,
    short: bool,
}

impl Builder {
    fn param(&mut self, p: Param) -> String {
        self.params.push(p);
        format!("${}", self.params.len())
    }

    fn text_value(&mut self, value: &str, path: &str) -> Result<(String, bool), AppError> {
        let v = value.trim();
        let n = v.chars().count();
        if n == 0 {
            return Err(AppError::invalid(&format!("{path}.value"), "empty"));
        }
        if n > MAX_VALUE_CHARS {
            return Err(AppError::invalid(&format!("{path}.value"), "tooLong"));
        }
        let indexable = n >= MIN_INDEXABLE_CHARS;
        self.short |= !indexable;
        Ok((v.to_owned(), indexable))
    }

    fn node(&mut self, node: &ConditionNode, path: &str, depth: usize) -> Result<Out, AppError> {
        match node {
            ConditionNode::Group { op, children } => self.group(*op, children, path, depth),
            ConditionNode::Sender { value } => {
                let (v, ix) = self.text_value(value, path)?;
                let p = self.param(Param::Text(like(&v)));
                Ok(Out { sql: format!("m.sender_search ILIKE {p}"), indexable: ix })
            }
            ConditionNode::Subject { value } => {
                let (v, ix) = self.text_value(value, path)?;
                let p = self.param(Param::Text(like(&v)));
                Ok(Out { sql: format!("m.subject ILIKE {p}"), indexable: ix })
            }
            ConditionNode::Body { value } => {
                let (v, ix) = self.text_value(value, path)?;
                let p = self.param(Param::Text(like(&v)));
                if v.chars().any(|c| c.is_ascii_alphabetic()) {
                    let q = self.param(Param::Text(v));
                    // Stored column first: rows matching by stem skip the ILIKE (spec Part 04 §7.3, §7.4).
                    Ok(Out { sql: format!("(m.body_tsv @@ websearch_to_tsquery('english', {q}) OR m.body_text ILIKE {p})"), indexable: ix })
                } else {
                    Ok(Out { sql: format!("m.body_text ILIKE {p}"), indexable: ix })
                }
            }
            ConditionNode::Recipient { value, kinds } => {
                let (v, ix) = self.text_value(value, path)?;
                let p = self.param(Param::Text(like(&v)));
                let mut names: Vec<String> = kinds
                    .iter()
                    .map(|k| match k {
                        RecipientKind::To => "to",
                        RecipientKind::Cc => "cc",
                        RecipientKind::Bcc => "bcc",
                    })
                    .map(str::to_owned)
                    .collect();
                names.sort();
                names.dedup();
                if names.is_empty() || names.len() == 3 {
                    Ok(Out { sql: format!("m.recipients_search ILIKE {p}"), indexable: ix })
                } else {
                    let k = self.param(Param::TextArray(names));
                    Ok(Out {
                        sql: format!(
                            "(m.recipients_search ILIKE {p} AND EXISTS (SELECT 1 FROM message_recipients r WHERE r.message_id = m.id \
                             AND r.kind = ANY({k}::text[]) AND r.search_text ILIKE {p}))"
                        ),
                        indexable: ix,
                    })
                }
            }
            ConditionNode::DateRange { from, to } => {
                if from.is_none() && to.is_none() {
                    return Err(AppError::invalid(path, "empty"));
                }
                let mut parts = Vec::new();
                match from {
                    Some(f) => {
                        if !is_rfc3339(f) {
                            return Err(AppError::invalid(&format!("{path}.from"), "invalidInstant"));
                        }
                        let p = self.param(Param::Text(f.clone()));
                        parts.push(format!("m.sort_at >= {p}::text::timestamptz"));
                    }
                    None => parts.push("m.sort_at > '-infinity'".to_owned()),
                }
                if let Some(t) = to {
                    if !is_rfc3339(t) {
                        return Err(AppError::invalid(&format!("{path}.to"), "invalidInstant"));
                    }
                    let p = self.param(Param::Text(t.clone()));
                    parts.push(format!("m.sort_at < {p}::text::timestamptz"));
                }
                Ok(Out { sql: format!("({})", parts.join(" AND ")), indexable: false })
            }
            ConditionNode::Folder { folder_path, include_subfolders } => {
                let n = folder_path.chars().count();
                if n == 0 {
                    return Err(AppError::invalid(&format!("{path}.folderPath"), "empty"));
                }
                if n > MAX_FOLDER_PATH_CHARS {
                    return Err(AppError::invalid(&format!("{path}.folderPath"), "tooLong"));
                }
                let p = self.param(Param::Text(folder_path.clone()));
                let sub = if *include_subfolders {
                    let pp = self.param(Param::Text(format!("{}/%", escape_like(folder_path))));
                    format!(" OR f.path LIKE {pp}")
                } else {
                    String::new()
                };
                Ok(Out {
                    sql: format!(
                        "EXISTS (SELECT 1 FROM message_occurrences o WHERE o.message_id = m.id AND o.folder_id = ANY \
                         (SELECT f.id FROM folders f WHERE f.path = {p}{sub}))"
                    ),
                    indexable: false,
                })
            }
            ConditionNode::HasAttachment { value } => {
                let p = self.param(Param::Bool(*value));
                Ok(Out { sql: format!("m.has_attachments = {p}"), indexable: false })
            }
            ConditionNode::SourcePst { pst_ids } => {
                let mut unique = pst_ids.clone();
                unique.sort_unstable();
                unique.dedup();
                if pst_ids.is_empty() || pst_ids.len() > MAX_PST_IDS || unique.len() != pst_ids.len() || pst_ids.iter().any(|&i| i < 1) {
                    return Err(AppError::invalid(&format!("{path}.pstIds"), "invalid"));
                }
                let p = self.param(Param::BigintArray(pst_ids.clone()));
                Ok(Out {
                    sql: format!("EXISTS (SELECT 1 FROM message_occurrences o WHERE o.message_id = m.id AND o.pst_id = ANY({p}::bigint[]))"),
                    indexable: false,
                })
            }
        }
    }

    fn group(&mut self, op: GroupOp, children: &[ConditionNode], path: &str, depth: usize) -> Result<Out, AppError> {
        if depth > MAX_DEPTH {
            return Err(AppError::invalid(path, "tooDeep"));
        }
        if children.is_empty() {
            return Err(AppError::invalid(&format!("{path}.children"), "empty"));
        }
        let mut parts = Vec::with_capacity(children.len());
        let mut any = false;
        let mut all = true;
        for (i, c) in children.iter().enumerate() {
            let out = self.node(c, &format!("{path}.children[{i}]"), depth + 1)?;
            any |= out.indexable;
            all &= out.indexable;
            parts.push(out.sql);
        }
        let (joiner, indexable) = match op {
            GroupOp::And => (" AND ", any),
            GroupOp::Or => (" OR ", all),
        };
        Ok(Out { sql: format!("({})", parts.join(joiner)), indexable })
    }
}

fn leaves(nodes: &[ConditionNode]) -> usize {
    nodes
        .iter()
        .map(|n| match n {
            ConditionNode::Group { children, .. } => leaves(children),
            _ => 1,
        })
        .sum()
}

pub fn compile(query: &SearchQuery) -> Result<Compiled, AppError> {
    let mut b = Builder { params: Vec::new(), short: false };
    let out = match query {
        SearchQuery::Simple { text } => {
            let terms: Vec<&str> = text.split_whitespace().collect();
            if terms.is_empty() {
                return Err(AppError::invalid("query.text", "empty"));
            }
            if terms.len() > MAX_TERMS {
                return Err(AppError::invalid("query.text", "tooManyTerms"));
            }
            if terms.iter().any(|t| t.chars().count() > MAX_VALUE_CHARS) {
                return Err(AppError::invalid("query.text", "tooLong"));
            }
            let children = terms
                .iter()
                .map(|t| ConditionNode::Group {
                    op: GroupOp::Or,
                    children: vec![
                        ConditionNode::Sender { value: (*t).to_owned() },
                        ConditionNode::Recipient { value: (*t).to_owned(), kinds: Vec::new() },
                        ConditionNode::Subject { value: (*t).to_owned() },
                        ConditionNode::Body { value: (*t).to_owned() },
                    ],
                })
                .collect::<Vec<_>>();
            b.group(GroupOp::And, &children, "query.text", 1)?
        }
        SearchQuery::Dsl { root } => {
            let ConditionGroup { op, children } = root;
            if leaves(children) > MAX_LEAVES {
                return Err(AppError::invalid("query.root", "tooManyLeaves"));
            }
            b.group(*op, children, "query.root", 1)?
        }
    };
    Ok(Compiled { predicate: out.sql, params: b.params, indexable: out.indexable, short_query: b.short })
}

/// RFC 3339 date-time (`YYYY-MM-DDTHH:MM:SS[.fraction](Z|±HH:MM)`); `T`/`Z` also lowercase (RFC 3339 §5.6 note).
pub fn is_rfc3339(s: &str) -> bool {
    let b = s.as_bytes();
    let digits = |r: std::ops::Range<usize>| b.get(r.clone()).is_some_and(|x| x.iter().all(u8::is_ascii_digit));
    let num = |r: std::ops::Range<usize>| s[r].parse::<u32>().unwrap_or(99);
    if b.len() < 20 || !digits(0..4) || b[4] != b'-' || !digits(5..7) || b[7] != b'-' || !digits(8..10) {
        return false;
    }
    if !matches!(b[10], b'T' | b't') || !digits(11..13) || b[13] != b':' || !digits(14..16) || b[16] != b':' || !digits(17..19) {
        return false;
    }
    if !(1..=12).contains(&num(5..7)) || !(1..=31).contains(&num(8..10)) || num(11..13) > 23 || num(14..16) > 59 || num(17..19) > 60 {
        return false;
    }
    let mut i = 19;
    if b[i] == b'.' {
        i += 1;
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return false;
        }
    }
    match &b[i..] {
        [b'Z' | b'z'] => true,
        [sign, ..] if (*sign == b'+' || *sign == b'-') && b.len() == i + 6 => {
            digits(i + 1..i + 3) && b[i + 3] == b':' && digits(i + 4..i + 6) && num(i + 1..i + 3) <= 23 && num(i + 4..i + 6) <= 59
        }
        _ => false,
    }
}
```


- [ ] **Step 6: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test -p outlooker-core --lib search::compile`
Expected: 6 tests `ok`, `test result: ok. 6 passed`.

- [ ] **Step 7: Write the failing tests in `src-tauri/crates/outlooker-core/src/search/cursor.rs`**

Create the file with only this test module (the code under test does not exist yet):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_infinity() {
        let k = CursorKey { sort_at: Some("2024-03-01 08:00:00.123456+00".into()), id: 42 };
        let enc = encode(&k);
        assert!(!enc.contains('=') && !enc.contains('+') && !enc.contains('/'));
        assert_eq!(decode(&enc).unwrap(), k);
        let neg = CursorKey::from_db("-infinity".into(), 7);
        assert_eq!(neg, CursorKey { sort_at: None, id: 7 });
        assert_eq!(neg.sort_text(), "-infinity");
        assert_eq!(decode(&encode(&neg)).unwrap(), neg);
        assert_eq!(URL_SAFE_NO_PAD.decode(encode(&neg)).unwrap(), br#"{"s":null,"i":7}"#);
        for bad in ["", "!!!", "e30", &URL_SAFE_NO_PAD.encode(br#"{"s":"","i":1}"#)] {
            assert_eq!(decode(bad), Err(AppError::invalid("cursor", "invalid")), "{bad}");
        }
    }
}
```


- [ ] **Step 8: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test -p outlooker-core --lib search::cursor`
Expected: compile error (cannot find type `CursorKey`).

- [ ] **Step 9: Implement `src-tauri/crates/outlooker-core/src/search/cursor.rs`**

Insert this code above the test module (the file is this code followed by the tests of the previous step):

```rust
//! Keyset cursor (spec Part 03 §5.2): base64url (no padding) of `{"s": <sort_at text | null>, "i": <id>}`.
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};

use crate::dto::error::AppError;

/// `sort_at` = PostgreSQL text output of `messages.sort_at`; `None` = `-infinity` (no sent time).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorKey {
    pub sort_at: Option<String>,
    pub id: i64,
}

#[derive(Serialize, Deserialize)]
struct Wire {
    s: Option<String>,
    i: i64,
}

impl CursorKey {
    /// From the `sort_at::text` column value.
    pub fn from_db(sort_at: String, id: i64) -> CursorKey {
        CursorKey { sort_at: (sort_at != "-infinity").then_some(sort_at), id }
    }

    /// The text bound as `$n::text::timestamptz`.
    pub fn sort_text(&self) -> &str {
        self.sort_at.as_deref().unwrap_or("-infinity")
    }
}

pub fn encode(k: &CursorKey) -> String {
    let json = serde_json::to_vec(&Wire { s: k.sort_at.clone(), i: k.id }).expect("cursor serializes");
    URL_SAFE_NO_PAD.encode(json)
}

pub fn decode(s: &str) -> Result<CursorKey, AppError> {
    let invalid = || AppError::invalid("cursor", "invalid");
    let bytes = URL_SAFE_NO_PAD.decode(s).map_err(|_| invalid())?;
    let w: Wire = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    if w.s.as_deref().is_some_and(|t| t.is_empty() || t.len() > 64) {
        return Err(invalid());
    }
    Ok(CursorKey { sort_at: w.s, id: w.i })
}
```


- [ ] **Step 10: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test -p outlooker-core --lib search::cursor`
Expected: 1 tests `ok`, `test result: ok. 1 passed`.

- [ ] **Step 11: Write the failing tests in `src-tauri/crates/outlooker-core/src/search/mail.rs`**

Create the file with only this test module (the code under test does not exist yet):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_known_and_unknown() {
        let map = HashMap::from([(0, 17_i64), (2, 99)]);
        let html = r#"<p><img src="outlooker-inline:0" alt="a"><img src="outlooker-inline:1"><img src="outlooker-inline:2"></p>"#;
        assert_eq!(
            rewrite_inline(html, &map),
            format!(r#"<p><img src="{p}17" alt="a"><img><img src="{p}99"></p>"#, p = IMAGE_URL_PREFIX)
        );
        let odd = r#"<img src="outlooker-inline:x"> src="outlooker-inline:5"#;
        assert_eq!(rewrite_inline(odd, &map), odd);
        assert_eq!(rewrite_inline("no images", &map), "no images");
        #[cfg(windows)]
        assert_eq!(IMAGE_URL_PREFIX, "http://mailimg.localhost/");
        #[cfg(not(windows))]
        assert_eq!(IMAGE_URL_PREFIX, "mailimg://localhost/");
    }
}
```


- [ ] **Step 12: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test -p outlooker-core --lib search::mail`
Expected: compile error (cannot find function `rewrite_inline`).

- [ ] **Step 13: Implement `src-tauri/crates/outlooker-core/src/search/mail.rs`**

Insert this code above the test module (the file is this code followed by the tests of the previous step):

```rust
//! `mail_get` (spec Part 04 §7.6, Part 03 §5.5–§5.6).
use std::collections::HashMap;

use tokio_postgres::Client;

use super::sql;
use super::db_err;
use crate::db::RFC3339_UTC_FMT;
use crate::dto::error::AppError;
use crate::dto::search::{Address, AttachmentMeta, MailDetail, Occurrence};

/// The only platform branch of the Search Service (Part 03 §5.6): WebView2 maps custom schemes to http.
#[cfg(windows)]
pub const IMAGE_URL_PREFIX: &str = "http://mailimg.localhost/";
#[cfg(not(windows))]
pub const IMAGE_URL_PREFIX: &str = "mailimg://localhost/";

const NEEDLE: &str = " src=\"outlooker-inline:";

/// Each ` src="outlooker-inline:<n>"` → ` src="<IMAGE_URL_PREFIX><blob id>"`; an unknown ordinal loses the attribute.
pub fn rewrite_inline(html: &str, ordinal_to_blob: &HashMap<i32, i64>) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(pos) = rest.find(NEEDLE) {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + NEEDLE.len()..];
        let digits = after.bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0 && after.as_bytes().get(digits) == Some(&b'"') {
            if let Some(blob) = after[..digits].parse::<i32>().ok().and_then(|n| ordinal_to_blob.get(&n)) {
                out.push_str(&format!(" src=\"{IMAGE_URL_PREFIX}{blob}\""));
            }
            rest = &after[digits + 1..];
        } else {
            out.push_str(NEEDLE);
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

pub async fn mail_get(client: &Client, id: i64) -> Result<MailDetail, AppError> {
    let row = client.query_opt(sql::MAIL, &[&id, &RFC3339_UTC_FMT]).await.map_err(db_err)?.ok_or_else(|| AppError::not_found("mail", id))?;
    let (mut to, mut cc, mut bcc) = (Vec::new(), Vec::new(), Vec::new());
    for r in client.query(sql::MAIL_RECIPIENTS, &[&id]).await.map_err(db_err)? {
        let a = Address { name: r.get(1), address: r.get(2) };
        match r.get::<_, &str>(0) {
            "cc" => cc.push(a),
            "bcc" => bcc.push(a),
            _ => to.push(a),
        }
    }
    let attachments = client
        .query(sql::MAIL_ATTACHMENTS, &[&id])
        .await
        .map_err(db_err)?
        .iter()
        .map(|r| AttachmentMeta { id: r.get(0), file_name: r.get(1), size_bytes: r.get(2), mime_type: r.get(3) })
        .collect();
    let occurrences = client
        .query(sql::MAIL_OCCURRENCES, &[&id])
        .await
        .map_err(db_err)?
        .iter()
        .map(|r| Occurrence { pst_id: r.get(0), pst_display_name: r.get(1), pst_path: r.get(2), folder_path: r.get(3), pst_item_id: r.get(4) })
        .collect();
    let inline: HashMap<i32, i64> =
        client.query(sql::MAIL_INLINE, &[&id]).await.map_err(db_err)?.iter().map(|r| (r.get(0), r.get(1))).collect();
    let body_html: Option<String> = row.get(7);
    Ok(MailDetail {
        id: row.get(0),
        message_id: row.get(1),
        subject: row.get(2),
        sent_at: row.get(3),
        received_at: row.get(4),
        from: Address { name: row.get(5), address: row.get(6) },
        to,
        cc,
        bcc,
        body_html: body_html.map(|h| rewrite_inline(&h, &inline)),
        body_text: row.get(8),
        attachments,
        occurrences,
    })
}
```


- [ ] **Step 14: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test -p outlooker-core --lib search::mail`
Expected: 1 tests `ok`, `test result: ok. 1 passed`.

- [ ] **Step 15: Write the failing integration tests `src-tauri/crates/outlooker-core/tests/search.rs`**

```rust
//! Search Service tests on seeded rows (spec Part 04 §7, §12.2 rows Search compiler / Search semantics).
use std::time::{Duration, UNIX_EPOCH};

use outlooker_core::dto::error::AppError;
use outlooker_core::dto::search::{ConditionGroup, ConditionNode as N, GroupOp, RecipientKind, SearchQuery, SearchRequest};
use outlooker_core::importer::types::{AttachmentOut, BodySource, EncodedImage, InlineImageOut, NormalizedItem, NormalizedMail};
use outlooker_core::importer::webp::source_hash;
use outlooker_core::search::mail::IMAGE_URL_PREFIX;
use outlooker_core::search::{compile, search_page, SearchService};
use outlooker_core::test_support::import_fixtures::{add_recipient, item, mail, seed_mails};
use outlooker_core::test_support::TestCluster;

const T0: u64 = 1_600_000_000; // 2020-09-13T12:26:40Z

/// Mail number `n`: unique Message-ID, sent T0 + n hours (or no sent time).
fn m(n: i64, subject: &str, body: &str, sender: &str, sent: Option<u64>) -> NormalizedMail {
    let mut x = mail(subject, body, sender, Some(&format!("<n{n}@seed>")));
    x.sent_at = sent.map(|s| UNIX_EPOCH + Duration::from_secs(s));
    x
}

fn items(mails: Vec<NormalizedMail>) -> Vec<NormalizedItem> {
    mails.into_iter().enumerate().map(|(i, x)| item(i as i64 + 1, i as i64 + 1, x)).collect()
}

fn simple(text: &str) -> SearchRequest {
    SearchRequest { query: SearchQuery::Simple { text: text.into() }, cursor: None, page_size: None }
}

fn dsl(children: Vec<N>, page_size: u32) -> SearchRequest {
    SearchRequest { query: SearchQuery::Dsl { root: ConditionGroup { op: GroupOp::And, children } }, cursor: None, page_size: Some(page_size) }
}

async fn subjects(svc: &SearchService, req: SearchRequest) -> (Vec<String>, bool) {
    let page = svc.search(req).await.unwrap();
    (page.rows.into_iter().map(|r| r.subject).collect(), page.short_query)
}

async fn reference(tc: &TestCluster, subject_like: &str, limit: i64) -> Vec<i64> {
    let a = tc.admin().await;
    a.query("SELECT id FROM messages WHERE subject LIKE $1 ORDER BY sort_at DESC, id DESC LIMIT $2", &[&subject_like, &limit])
        .await
        .unwrap()
        .iter()
        .map(|r| r.get(0))
        .collect()
}

/// 100 mails, one hour apart; subject "dense" on all, "sparse" on every 10th (ids 10, 20, …), 5 without sent time.
async fn seed_window_set(tc: &TestCluster) {
    let mails = (1..=100)
        .map(|i| {
            let subject = if i % 10 == 0 { format!("dense sparse {i}") } else { format!("dense {i}") };
            let sent = if i > 95 { None } else { Some(T0 + i as u64 * 3600) };
            m(i, &subject, "body", "s@x.example", sent)
        })
        .collect();
    seed_mails(&tc.db, "/p/window.pst", items(mails)).await;
}

async fn page_ids(client: &mut tokio_postgres::Client, req: &SearchRequest, w: i64) -> (Vec<i64>, Option<String>, bool) {
    let (page, phase_b) = search_page(client, req, w).await.unwrap();
    (page.rows.iter().map(|r| r.id).collect(), page.next_cursor, phase_b)
}

#[tokio::test]
async fn semantics_short_and_cjk() {
    let tc = TestCluster::start().await;
    seed_mails(
        &tc.db,
        "/p/a.pst",
        items(vec![
            m(1, "a", "請見報價單附件", "s@x.example", Some(T0)),
            m(2, "b", "報價很好", "s@x.example", Some(T0 + 1)),
            m(3, "c", "unrelated", "s@x.example", Some(T0 + 2)),
        ]),
    )
    .await;
    let svc = SearchService::new(tc.db.clone());
    assert_eq!(subjects(&svc, simple("報價")).await, (vec!["b".to_owned(), "a".to_owned()], true));
    assert_eq!(subjects(&svc, simple("報價單")).await, (vec!["a".to_owned()], false));
}

#[tokio::test]
async fn semantics_stem() {
    let tc = TestCluster::start().await;
    seed_mails(
        &tc.db,
        "/p/a.pst",
        items(vec![m(1, "plural", "please pay the invoices", "s@x.example", Some(T0)), m(2, "singular", "one invoice", "s@x.example", Some(T0 + 1))]),
    )
    .await;
    let svc = SearchService::new(tc.db.clone());
    let body = |v: &str| dsl(vec![N::Body { value: v.into() }], 50);
    assert_eq!(subjects(&svc, body("invoice")).await.0, vec!["singular", "plural"]);
    // "invoices" is no substring of "one invoice": only the English stem branch can match it
    assert_eq!(subjects(&svc, body("invoices")).await.0, vec!["singular", "plural"]);
}

#[tokio::test]
async fn semantics_partial_sender_recipient() {
    let tc = TestCluster::start().await;
    let mut a = m(1, "from contoso", "x", "alice.wong@contoso.com", Some(T0));
    a.sender_search = "Alice Wong <alice.wong@contoso.com>".into();
    add_recipient(&mut a, RecipientKind::To, Some("Carol"), "carol@example.org");
    add_recipient(&mut a, RecipientKind::Cc, Some("Bob Lin"), "bob@fabrikam.com");
    let b = m(2, "other", "x", "dave@example.org", Some(T0 + 1));
    seed_mails(&tc.db, "/p/a.pst", items(vec![a, b])).await;
    let svc = SearchService::new(tc.db.clone());
    let hit = vec!["from contoso".to_owned()];
    assert_eq!(subjects(&svc, dsl(vec![N::Sender { value: "Wong".into() }], 50)).await.0, hit);
    assert_eq!(subjects(&svc, dsl(vec![N::Sender { value: "contoso".into() }], 50)).await.0, hit);
    assert_eq!(subjects(&svc, dsl(vec![N::Recipient { value: "fabrik".into(), kinds: vec![] }], 50)).await.0, hit);
    assert_eq!(subjects(&svc, dsl(vec![N::Recipient { value: "fabrik".into(), kinds: vec![RecipientKind::Cc] }], 50)).await.0, hit);
    assert!(subjects(&svc, dsl(vec![N::Recipient { value: "fabrik".into(), kinds: vec![RecipientKind::To] }], 50)).await.0.is_empty());
    assert_eq!(subjects(&svc, simple("bob")).await.0, hit);
}

fn with_attachment(mut x: NormalizedMail) -> NormalizedMail {
    x.attachments.push(AttachmentOut { attachment_index: 0, file_name: "q.pdf".into(), size_bytes: 10, mime_type: "application/pdf".into(), attach_method: 1 });
    x
}

#[tokio::test]
async fn semantics_worked_example() {
    let tc = TestCluster::start().await;
    let in_2023 = 1_688_169_600; // 2023-07-01T00:00:00Z
    let p1 = seed_mails(
        &tc.db,
        "/p/one.pst",
        items(vec![
            with_attachment(m(1, "match sender", "請看報價", "sales@contoso.com", Some(in_2023))),
            m(2, "no attachment", "請看報價", "sales@contoso.com", Some(in_2023)),
            with_attachment(m(3, "wrong year", "請看報價", "sales@contoso.com", Some(1_625_097_600))),
            with_attachment(m(4, "wrong body", "nothing", "sales@contoso.com", Some(in_2023))),
        ]),
    )
    .await;
    let mut by_recipient = with_attachment(m(5, "match recipient", "報價如下", "x@example.org", Some(in_2023 + 60)));
    add_recipient(&mut by_recipient, RecipientKind::To, None, "alice@example.org");
    let p2 = seed_mails(&tc.db, "/p/two.pst", items(vec![by_recipient])).await;
    let p3 = seed_mails(&tc.db, "/p/three.pst", items(vec![with_attachment(m(6, "other source", "請看報價", "sales@contoso.com", Some(in_2023)))])).await;
    assert!(p3 > p2 && p2 > p1);
    let req = dsl(
        vec![
            N::Group { op: GroupOp::Or, children: vec![N::Sender { value: "contoso.com".into() }, N::Recipient { value: "alice".into(), kinds: vec![] }] },
            N::DateRange { from: Some("2022-12-31T16:00:00Z".into()), to: Some("2023-12-31T16:00:00Z".into()) },
            N::Body { value: "報價".into() },
            N::HasAttachment { value: true },
            N::SourcePst { pst_ids: vec![p1, p2] },
        ],
        200,
    );
    let page = SearchService::new(tc.db.clone()).search(req).await.unwrap();
    let got: Vec<&str> = page.rows.iter().map(|r| r.subject.as_str()).collect();
    assert_eq!(got, vec!["match recipient", "match sender"]);
    assert!(page.short_query);
}

#[tokio::test]
async fn literal_percent_underscore_backslash() {
    let tc = TestCluster::start().await;
    let subjects_in = ["100% done", "100 done", "a_b", "axb", r"c\d", "cxd"];
    let mails = subjects_in.iter().enumerate().map(|(i, s)| m(i as i64, s, "x", "s@x.example", Some(T0 + i as u64))).collect();
    seed_mails(&tc.db, "/p/a.pst", items(mails)).await;
    let svc = SearchService::new(tc.db.clone());
    for (needle, only) in [("100%", "100% done"), ("a_b", "a_b"), (r"c\d", r"c\d")] {
        assert_eq!(subjects(&svc, dsl(vec![N::Subject { value: needle.into() }], 50)).await.0, vec![only.to_owned()], "{needle}");
    }
}

#[tokio::test]
async fn count_capped() {
    let tc = TestCluster::start().await;
    let mails = (1..=10_001).map(|i| m(i, &format!("capword {i}"), "x", "s@x.example", Some(T0 + i as u64))).collect();
    seed_mails(&tc.db, "/p/a.pst", items(mails)).await;
    let svc = SearchService::new(tc.db.clone());
    let q = || SearchQuery::Dsl { root: ConditionGroup { op: GroupOp::And, children: vec![N::Subject { value: "capword".into() }] } };
    let c = svc.count(q()).await.unwrap();
    assert_eq!((c.count, c.capped), (10_000, true));
    tc.admin().await.batch_execute("DELETE FROM messages WHERE id IN (SELECT id FROM messages ORDER BY id LIMIT 2)").await.unwrap();
    let c = svc.count(q()).await.unwrap();
    assert_eq!((c.count, c.capped), (9_999, false));
}

#[tokio::test]
async fn mail_get_unknown_is_not_found() {
    let tc = TestCluster::start().await;
    let err = SearchService::new(tc.db.clone()).mail_get(987_654).await.unwrap_err();
    assert_eq!(err, AppError::NotFound { entity: "mail".into(), id: "987654".into() });
}

fn with_inline_image(mut x: NormalizedMail, bytes: &[u8]) -> NormalizedMail {
    x.body_source = BodySource::Html;
    x.body_html = Some(r#"<p>logo <img src="outlooker-inline:0"><img src="outlooker-inline:1"></p>"#.into());
    x.inline_images.push(InlineImageOut {
        ordinal: 0,
        content_id: Some("logo".into()),
        source_hash: source_hash(bytes),
        blob: Some(EncodedImage { webp: bytes.to_vec(), width: 2, height: 2, source_mime: "image/png".into() }),
    });
    x
}

#[tokio::test]
async fn mail_get_image_urls_platform_form() {
    let tc = TestCluster::start().await;
    let mut x = with_attachment(with_inline_image(m(1, "pic", "logo", "s@x.example", Some(T0)), b"webp-bytes"));
    add_recipient(&mut x, RecipientKind::To, Some("To One"), "to1@x.example");
    add_recipient(&mut x, RecipientKind::Bcc, None, "bcc@x.example");
    add_recipient(&mut x, RecipientKind::Cc, Some("Cc One"), "cc1@x.example");
    seed_mails(&tc.db, "/p/a.pst", items(vec![x])).await;
    let a = tc.admin().await;
    let id: i64 = a.query_one("SELECT id FROM messages", &[]).await.unwrap().get(0);
    let blob: i64 = a.query_one("SELECT id FROM image_blobs", &[]).await.unwrap().get(0);
    let d = SearchService::new(tc.db.clone()).mail_get(id).await.unwrap();
    assert_eq!(d.body_html.unwrap(), format!(r#"<p>logo <img src="{IMAGE_URL_PREFIX}{blob}"><img></p>"#));
    #[cfg(windows)]
    assert!(IMAGE_URL_PREFIX.starts_with("http://mailimg.localhost/"));
    #[cfg(not(windows))]
    assert!(IMAGE_URL_PREFIX.starts_with("mailimg://localhost/"));
    assert_eq!(d.to.len(), 1);
    assert_eq!(d.cc[0].name.as_deref(), Some("Cc One"));
    assert_eq!(d.bcc[0].address.as_deref(), Some("bcc@x.example"));
    assert_eq!(d.sent_at.as_deref(), Some("2020-09-13T12:26:40Z"));
    assert_eq!(d.attachments[0].file_name, "q.pdf");
    assert_eq!((d.occurrences[0].pst_path.as_str(), d.occurrences[0].folder_path.as_str()), ("/p/a.pst", "Inbox"));
}

#[tokio::test]
async fn two_phase_dense_phase_a_only() {
    let tc = TestCluster::start().await;
    seed_window_set(&tc).await;
    let mut client = tc.app().await;
    let (ids, _, phase_b) = page_ids(&mut client, &dsl(vec![N::Subject { value: "dense".into() }], 10), 20).await;
    assert!(!phase_b);
    assert_eq!(ids, reference(&tc, "%dense%", 10).await);
}

#[tokio::test]
async fn two_phase_sparse_runs_phase_b() {
    let tc = TestCluster::start().await;
    seed_window_set(&tc).await;
    let mut client = tc.app().await;
    let (ids, _, phase_b) = page_ids(&mut client, &dsl(vec![N::Subject { value: "sparse".into() }], 5), 20).await;
    assert!(phase_b);
    assert_eq!(ids, reference(&tc, "%sparse%", 5).await);
}

#[tokio::test]
async fn two_phase_pages_no_gap_no_overlap() {
    let tc = TestCluster::start().await;
    seed_window_set(&tc).await;
    let mut client = tc.app().await;
    let mut req = dsl(vec![N::Subject { value: "sparse".into() }], 3);
    let mut all = Vec::new();
    for _ in 0..3 {
        let (ids, next, _) = page_ids(&mut client, &req, 20).await;
        all.extend(ids);
        req.cursor = next;
    }
    assert_eq!(all, reference(&tc, "%sparse%", 9).await);
    let (last, next, _) = page_ids(&mut client, &req, 20).await;
    assert_eq!((last.len(), next), (1, None)); // 10 matches in total
}

#[tokio::test]
async fn null_sent_at_last_and_paged() {
    let tc = TestCluster::start().await;
    seed_window_set(&tc).await;
    let mut client = tc.app().await;
    let expected = reference(&tc, "%dense%", 100).await;
    let mut req = dsl(vec![N::Subject { value: "dense".into() }], 7);
    let mut rows = Vec::new();
    loop {
        let (page, _) = search_page(&mut client, &req, 20).await.unwrap();
        rows.extend(page.rows);
        match page.next_cursor {
            Some(c) => req.cursor = Some(c),
            None => break,
        }
    }
    assert_eq!(rows.iter().map(|r| r.id).collect::<Vec<_>>(), expected);
    // the 5 mails without a sent time (sort_at = -infinity) come last, after all 95 dated ones
    assert!(rows[..95].iter().all(|r| r.sent_at.is_some()));
    assert!(rows[95..].iter().all(|r| r.sent_at.is_none()));
}

#[tokio::test]
async fn reset_after_phase_b() {
    let tc = TestCluster::start().await;
    seed_window_set(&tc).await;
    let mut client = tc.app().await;
    let (_, _, phase_b) = page_ids(&mut client, &dsl(vec![N::Subject { value: "sparse".into() }], 5), 20).await;
    assert!(phase_b);
    for setting in ["SHOW enable_indexscan", "SHOW enable_seqscan"] {
        let v: String = client.query_one(setting, &[]).await.unwrap().get(0);
        assert_eq!(v, "on", "{setting}");
    }
}

#[tokio::test]
async fn explain_uses_indexes() {
    let tc = TestCluster::start().await;
    let mails = (1..=300)
        .map(|i| {
            let mut x = m(i, &format!("subject {i}"), &format!("body {i} 報價單 invoice text"), &format!("user{i}@contoso.com"), Some(T0 + i as u64));
            add_recipient(&mut x, RecipientKind::To, None, &format!("r{i}@fabrikam.com"));
            x
        })
        .collect();
    seed_mails(&tc.db, "/p/a.pst", items(mails)).await;
    let a = tc.admin().await;
    a.batch_execute("ANALYZE messages; SET enable_seqscan = off").await.unwrap();
    for (node, index) in [
        (N::Body { value: "報價單".into() }, "messages_body_trgm"),
        (N::Body { value: "invoice".into() }, "messages_body_fts"),
        (N::Sender { value: "contoso".into() }, "messages_sender_trgm"),
        (N::Recipient { value: "fabrikam".into(), kinds: vec![] }, "messages_recipients_trgm"),
    ] {
        let c = compile::compile(&SearchQuery::Dsl { root: ConditionGroup { op: GroupOp::And, children: vec![node] } }).unwrap();
        let params: Vec<_> = c.params.iter().map(|p| p.as_sql()).collect();
        let plan: String = a
            .query(&format!("EXPLAIN SELECT m.id FROM messages m WHERE {}", c.predicate), &params)
            .await
            .unwrap()
            .iter()
            .map(|r| r.get::<_, String>(0))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(plan.contains(index), "{index} missing in\n{plan}");
    }
}

#[tokio::test]
async fn image_returns_bytes_or_none() {
    let tc = TestCluster::start().await;
    seed_mails(&tc.db, "/p/a.pst", items(vec![with_inline_image(m(1, "pic", "x", "s@x.example", Some(T0)), b"RIFF-webp")])).await;
    let blob: i64 = tc.admin().await.query_one("SELECT id FROM image_blobs", &[]).await.unwrap().get(0);
    let svc = SearchService::new(tc.db.clone());
    assert_eq!(svc.image(blob).await.unwrap(), Some(b"RIFF-webp".to_vec()));
    assert_eq!(svc.image(blob + 1000).await.unwrap(), None);
}

#[tokio::test]
async fn filter_options_lists_psts_and_folders() {
    let tc = TestCluster::start().await;
    let a = seed_mails(&tc.db, "/p/b-name.pst", items(vec![m(1, "x", "x", "s@x.example", Some(T0)), m(2, "y", "y", "s@x.example", Some(T0))])).await;
    let b = seed_mails(&tc.db, "/p/a-name.pst", items(vec![m(3, "z", "z", "s@x.example", Some(T0))])).await;
    let f = SearchService::new(tc.db.clone()).filter_options().await.unwrap();
    let psts: Vec<(i64, &str)> = f.psts.iter().map(|p| (p.id, p.display_name.as_str())).collect();
    assert_eq!(psts, vec![(b, "a-name.pst"), (a, "b-name.pst")]);
    let folders: Vec<(i64, &str, i64)> = f.folders.iter().map(|x| (x.pst_id, x.folder_path.as_str(), x.message_count)).collect();
    assert_eq!(folders, vec![(a, "Inbox", 2), (b, "Inbox", 1)]);
}
```


- [ ] **Step 16: Run the integration tests to verify they fail**

Run: `cd src-tauri && cargo test -p outlooker-core --features test-support --test search`
Expected: compile error (unresolved imports `outlooker_core::search::{search_page, SearchService}`).

- [ ] **Step 17: Implement `src-tauri/crates/outlooker-core/src/search/execute.rs`**

```rust
//! Two-phase keyset execution (spec Part 04 §7.4) and the capped count.
use tokio_postgres::types::ToSql;
use tokio_postgres::{Client, Transaction};

use super::compile::Compiled;
use super::cursor::CursorKey;
use super::sql::WINDOW_END;
use crate::dto::error::AppError;
use crate::dto::search::SearchCount;

pub const WINDOW: i64 = 5_000;
pub const COUNT_CAP: i64 = 10_000;
pub const TIMEOUT_MS: u32 = 10_000;
pub const SHORT_TIMEOUT_MS: u32 = 20_000;

/// Up to n rows, newest first; `phase_b_ran` tells the tests which path ran.
#[derive(Debug, Clone, PartialEq)]
pub struct PageIds {
    pub rows: Vec<(i64, CursorKey)>,
    pub phase_b_ran: bool,
}

/// Mode "ordered" and phase B: `$k+1` cursor sort_at text, `$k+2` cursor id, `$k+3` limit.
pub fn ordered_sql(c: &Compiled) -> String {
    let k = c.params.len();
    format!(
        "SELECT m.id, m.sort_at::text FROM messages m WHERE {} AND (m.sort_at, m.id) < (${}::text::timestamptz, ${}::int8) \
         ORDER BY m.sort_at DESC, m.id DESC LIMIT ${}",
        c.predicate,
        k + 1,
        k + 2,
        k + 3
    )
}

/// Phase A: `$k+1` cursor sort_at text, `$k+2` cursor id, `$k+3` window, `$k+4` limit.
pub fn phase_a_sql(c: &Compiled) -> String {
    let k = c.params.len();
    format!(
        "SELECT m.id, m.sort_at::text FROM (SELECT m0.* FROM messages m0 WHERE (m0.sort_at, m0.id) < (${}::text::timestamptz, ${}::int8) \
         ORDER BY m0.sort_at DESC, m0.id DESC LIMIT ${}) AS m WHERE {} ORDER BY m.sort_at DESC, m.id DESC LIMIT ${}",
        k + 1,
        k + 2,
        k + 3,
        c.predicate,
        k + 4
    )
}

/// `$k+1` = COUNT_CAP + 1.
pub fn count_sql(c: &Compiled) -> String {
    format!("SELECT count(*) FROM (SELECT 1 FROM messages m WHERE {} LIMIT ${}) s", c.predicate, c.params.len() + 1)
}

/// The predicate's `$1..$k` values followed by `extra` (`$k+1..`).
pub fn bind<'a>(c: &'a Compiled, extra: &[&'a (dyn ToSql + Sync)]) -> Vec<&'a (dyn ToSql + Sync)> {
    let mut v: Vec<&(dyn ToSql + Sync)> = c.params.iter().map(|p| p.as_sql()).collect();
    v.extend_from_slice(extra);
    v
}

fn timeout_ms(c: &Compiled) -> u32 {
    if c.short_query { SHORT_TIMEOUT_MS } else { TIMEOUT_MS }
}

fn map_err(e: tokio_postgres::Error, timeout: u32) -> AppError {
    match e.as_db_error().map(|d| d.code().code()) {
        Some("57014") => AppError::QueryTimeout { timeout_ms: timeout },
        Some("22007") | Some("22008") => AppError::invalid("cursor", "invalid"),
        _ => super::db_err(e),
    }
}

async fn begin<'a>(client: &'a mut Client, c: &Compiled) -> Result<Transaction<'a>, AppError> {
    let t = timeout_ms(c);
    let tx = client.transaction().await.map_err(|e| map_err(e, t))?;
    let set = if c.short_query { "SET LOCAL statement_timeout = '20s'" } else { "SET LOCAL statement_timeout = '10s'" };
    tx.batch_execute(set).await.map_err(|e| map_err(e, t))?;
    Ok(tx)
}

fn rows_to_keys(rows: Vec<tokio_postgres::Row>) -> Vec<(i64, CursorKey)> {
    rows.into_iter()
        .map(|r| {
            let id: i64 = r.get(0);
            (id, CursorKey::from_db(r.get(1), id))
        })
        .collect()
}

/// Spec Part 04 §7.4 inside one transaction on `client`. n = page_size + 1. `window` = W (production `WINDOW`).
pub async fn page(client: &mut Client, c: &Compiled, after: Option<&CursorKey>, n: i64, window: i64) -> Result<PageIds, AppError> {
    let t = timeout_ms(c);
    let (cs, ci) = match after {
        Some(key) => (key.sort_text().to_owned(), key.id),
        None => ("infinity".to_owned(), 0_i64),
    };
    let tx = begin(client, c).await?;
    if !c.indexable {
        let rows = tx.query(&ordered_sql(c), &bind(c, &[&cs, &ci, &n])).await.map_err(|e| map_err(e, t))?;
        tx.commit().await.map_err(|e| map_err(e, t))?;
        return Ok(PageIds { rows: rows_to_keys(rows), phase_b_ran: false });
    }
    let mut rows = rows_to_keys(tx.query(&phase_a_sql(c), &bind(c, &[&cs, &ci, &window, &n])).await.map_err(|e| map_err(e, t))?);
    let mut phase_b_ran = false;
    if (rows.len() as i64) < n {
        let offset = window - 1;
        if let Some(end) = tx.query_opt(WINDOW_END, &[&cs, &ci, &offset]).await.map_err(|e| map_err(e, t))? {
            let (ws, wi): (String, i64) = (end.get(0), end.get(1));
            let remaining = n - rows.len() as i64;
            tx.batch_execute("SET LOCAL enable_indexscan = off; SET LOCAL enable_seqscan = off;").await.map_err(|e| map_err(e, t))?;
            let b = tx.query(&ordered_sql(c), &bind(c, &[&ws, &wi, &remaining])).await.map_err(|e| map_err(e, t))?;
            tx.batch_execute("RESET enable_indexscan; RESET enable_seqscan;").await.map_err(|e| map_err(e, t))?;
            rows.extend(rows_to_keys(b));
            phase_b_ran = true;
        }
    }
    tx.commit().await.map_err(|e| map_err(e, t))?;
    Ok(PageIds { rows, phase_b_ran })
}

pub async fn count(client: &mut Client, c: &Compiled) -> Result<SearchCount, AppError> {
    let t = TIMEOUT_MS;
    let tx = client.transaction().await.map_err(|e| map_err(e, t))?;
    tx.batch_execute("SET LOCAL statement_timeout = '10s'").await.map_err(|e| map_err(e, t))?;
    if c.indexable {
        tx.batch_execute("SET LOCAL enable_seqscan = off; SET LOCAL enable_indexscan = off;").await.map_err(|e| map_err(e, t))?;
    }
    let cap = COUNT_CAP + 1;
    let n: i64 = tx.query_one(&count_sql(c), &bind(c, &[&cap])).await.map_err(|e| map_err(e, t))?.get(0);
    tx.commit().await.map_err(|e| map_err(e, t))?;
    Ok(if n > COUNT_CAP { SearchCount { count: COUNT_CAP, capped: true } } else { SearchCount { count: n, capped: false } })
}
```


- [ ] **Step 18: Implement `src-tauri/crates/outlooker-core/src/search/filter.rs`**

```rust
//! `filter_options` (spec Part 04 §7.6).
use tokio_postgres::Client;

use super::{db_err, sql};
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
```


- [ ] **Step 19: Implement `src-tauri/crates/outlooker-core/src/search/image.rs`**

```rust
//! `mailimg` scheme body (spec Part 04 §7.6).
use tokio_postgres::Client;

use super::{db_err, sql};
use crate::dto::error::AppError;

/// WebP bytes of `image_blobs.id`, `None` when unknown.
pub async fn image(client: &Client, id: i64) -> Result<Option<Vec<u8>>, AppError> {
    Ok(client.query_opt(sql::IMAGE, &[&id]).await.map_err(db_err)?.map(|r| r.get(0)))
}
```


- [ ] **Step 20: Replace `src-tauri/crates/outlooker-core/src/search/mod.rs` with the service**

```rust
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

use crate::db::{Db, RFC3339_UTC_FMT};
use crate::dto::error::AppError;
use crate::dto::search::{Address, FilterOptions, MailDetail, MailRow, SearchCount, SearchPage, SearchQuery, SearchRequest};

pub const DEFAULT_PAGE_SIZE: u32 = 200;
pub const MAX_PAGE_SIZE: u32 = 500;

/// Database error of a read path: connection closed / I/O → `DbNotReady`, else `Internal{"<sqlstate>: <message>"}`.
pub(crate) fn db_err(e: tokio_postgres::Error) -> AppError {
    if e.is_closed() || (e.as_db_error().is_none() && std::error::Error::source(&e).is_some_and(|s| s.is::<std::io::Error>())) {
        return AppError::DbNotReady;
    }
    crate::db::pg_err(e)
}

fn pool_err(e: deadpool_postgres::PoolError) -> AppError {
    match e {
        deadpool_postgres::PoolError::Closed => AppError::DbNotReady,
        deadpool_postgres::PoolError::Backend(e) if e.as_db_error().is_none() => AppError::DbNotReady,
        other => AppError::internal(format!("pool: {other}")),
    }
}

#[derive(Clone)]
pub struct SearchService {
    db: Db,
}

impl SearchService {
    pub fn new(db: Db) -> SearchService {
        SearchService { db }
    }

    async fn client(&self) -> Result<deadpool_postgres::Object, AppError> {
        self.db.app_pool()?.get().await.map_err(pool_err)
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
```


- [ ] **Step 21: Run all search tests**

Run: `cd src-tauri && cargo test -p outlooker-core search::`
Expected: `test result: ok. 8 passed` — `search::compile::tests::{no_user_value_in_sql, like_escapes_literally, limits_paths, worked_example_predicate, simple_query_shape_and_tsquery_rule, rfc3339_validation}`, `search::cursor::tests::roundtrip_and_infinity`, `search::mail::tests::rewrite_known_and_unknown`.

Run: `cd src-tauri && cargo test -p outlooker-core --features test-support --test search`
Expected: `test result: ok. 16 passed` — `semantics_short_and_cjk`, `semantics_stem`, `semantics_partial_sender_recipient`, `semantics_worked_example`, `literal_percent_underscore_backslash`, `count_capped`, `mail_get_unknown_is_not_found`, `mail_get_image_urls_platform_form`, `two_phase_dense_phase_a_only`, `two_phase_sparse_runs_phase_b`, `two_phase_pages_no_gap_no_overlap`, `null_sent_at_last_and_paged`, `reset_after_phase_b`, `explain_uses_indexes`, `image_returns_bytes_or_none`, `filter_options_lists_psts_and_folders`.

- [ ] **Step 22: Prove that the escaping and phase-B tests bite**

Back up, mutate, run, restore from the backups (never from git):

```bash
cp src-tauri/crates/outlooker-core/src/search/compile.rs scripts/tmp/compile.rs.bak
cp src-tauri/crates/outlooker-core/src/search/execute.rs scripts/tmp/execute.rs.bak
python3 -c "import pathlib;p=pathlib.Path('src-tauri/crates/outlooker-core/src/search/compile.rs');s=p.read_text();old='format!(\"%{}%\", escape_like(v))';assert s.count(old)==1;p.write_text(s.replace(old,'format!(\"%{}%\", v)'))"
python3 -c "import pathlib;p=pathlib.Path('src-tauri/crates/outlooker-core/src/search/execute.rs');s=p.read_text();old='if (rows.len() as i64) < n {';assert s.count(old)==1;p.write_text(s.replace(old,'if false {'))"
cd src-tauri && cargo test -p outlooker-core --features test-support --test search; cd ..
cp scripts/tmp/compile.rs.bak src-tauri/crates/outlooker-core/src/search/compile.rs
cp scripts/tmp/execute.rs.bak src-tauri/crates/outlooker-core/src/search/execute.rs
```

Expected: exactly `two_phase_sparse_runs_phase_b`, `two_phase_pages_no_gap_no_overlap`, `literal_percent_underscore_backslash` and `reset_after_phase_b` FAIL (12 passed; 4 failed). After the restore the previous step passes again.

- [ ] **Step 23: Check the SQL rule**

Run: `grep -nE "format!\(" src-tauri/crates/outlooker-core/src/search/*.rs`
Expected: each hit is one of: SQL text from `c.predicate` (itself built from constant fragments), `$n` numbers; a bound parameter value (`like`, the folder prefix); a JSON path of an error field; the `<img src>` HTML of `rewrite_inline`. No hit formats a request value into SQL text. `no_user_value_in_sql` enforces the same for the compiler.

- [ ] **Step 24: Commit**

Write the message file `scripts/tmp/r211-msg.txt` (Write tool; `scripts/tmp/` is gitignored, the session hook blocks `/tmp`) with: `feat(core): search service - compiler, two-phase keyset execution, mail_get, images (R2-11)`, a blank line, and the attribution line of the session.

```bash
git rev-parse --abbrev-ref HEAD
git add src-tauri/crates/outlooker-core/src/search src-tauri/crates/outlooker-core/tests/search.rs
git commit -F scripts/tmp/r211-msg.txt -- src-tauri/crates/outlooker-core/src/search src-tauri/crates/outlooker-core/tests/search.rs
git status --porcelain
```

Expected: the commit succeeds; `git status --porcelain` lists none of the paths above.

## Task R2-12: SQL Console Service and Maintenance Service

**Files:** (`core/` = `src-tauri/crates/outlooker-core/`)
- Create/own (replace kickoff stubs): `core/src/sql_console/mod.rs`, `core/src/sql_console/saved.rs`, `core/src/sql_console/execute.rs`, `core/src/maintenance/mod.rs`
- Create: `core/tests/sql_console.rs`, `core/tests/maintenance.rs`
- Unit tests: `#[cfg(test)] mod tests` in `sql_console/execute.rs` (error mapping, cell cut, command tag)

**Interfaces:**
- Consumes: C1 `dto::sql::*`, `dto::settings::MaintenanceTask`, `dto::events::{MaintenanceState, MaintenanceRunState}`, `dto::error::AppError`; C2 `EventSink`, `CoreEvent::MaintenanceStatusChanged`; C3 `Gate::try_enter(Activity::Maintenance)`; C8 `Db::{app_pool, ro_pool, connect_admin}`, `db::{pg_err, RFC3339_UTC_FMT}`, `db::indexes::ensure_search_indexes`, `TestCluster`; C12 `importer::remove::delete_orphans` (R2-10); C13 `search::hydrate_rows` (R2-11, lands first).
- Produces (C14 SqlConsole, Maintenance; consumer R2-13 `core::app` / Command Layer; `Maintenance::current` also feeds R2-08 `Supervisor::db_status(maintenance)`):

```rust
// core/src/sql_console/mod.rs
#[derive(Clone)]
pub struct SqlConsole { /* db: Db, running: Arc<Mutex<Option<Running>>> */ }
impl SqlConsole {
    pub fn new(db: crate::db::Db) -> SqlConsole;
    pub async fn saved_list(&self) -> Result<Vec<SavedQuery>, AppError>;                          // saved_query_list
    pub async fn saved_create(&self, input: SavedQueryInput) -> Result<SavedQuery, AppError>;     // saved_query_create
    pub async fn saved_update(&self, id: i64, input: SavedQueryInput) -> Result<SavedQuery, AppError>; // saved_query_update
    pub async fn saved_delete(&self, id: i64) -> Result<(), AppError>;                            // saved_query_delete
    pub async fn execute(&self, req: SqlExecuteRequest) -> Result<SqlResult, AppError>;           // sql_execute
    pub async fn cancel(&self, execution_id: String) -> Result<(), AppError>;                     // sql_cancel, idempotent
    pub async fn cancel_running(&self);                                                           // shutdown step 2
}
pub const DEFAULT_MAX_ROWS: u32 = 1_000;
pub const MAX_ROWS: u32 = 10_000;
pub const CELL_MAX_CHARS: usize = 2_000;
pub const TIMEOUT_MS: u32 = 30_000;

// core/src/sql_console/execute.rs (pure helpers, unit-tested)
pub fn map_sql_error(e: &tokio_postgres::Error, has_columns: bool, cancelled: bool) -> AppError;
pub fn cut_cell(v: &str) -> String;           // > 2,000 chars → first 2,000 chars + "…"
pub fn command_tag(sql: &str) -> String;      // first keyword, ASCII-uppercased ("SET", "DO", …)

// core/src/maintenance/mod.rs
#[derive(Clone)]
pub struct Maintenance { /* db: Db, gate: Gate, sink: Arc<dyn EventSink>, current: Arc<Mutex<Option<MaintenanceState>>> */ }
impl Maintenance {
    pub fn new(db: crate::db::Db, gate: crate::gate::Gate, sink: std::sync::Arc<dyn EventSink>) -> Maintenance;
    /// maintenance_run: enters the gate (Busy on conflict), emits `running`, spawns the task, returns at once.
    pub async fn run(&self, task: MaintenanceTask) -> Result<(), AppError>;
    /// The running task's state (None when idle).
    pub fn current(&self) -> Option<MaintenanceState>;
    /// Test seam (also used by R2-13 shutdown ordering tests if needed): resolves when no task runs.
    pub async fn wait_idle(&self);
}
/// Every index created by Part 04 §3, in this order (REINDEX order of the `reindex` task).
pub const REINDEX_INDEXES: [&str; 17];
```

**Behavior:**
- Saved queries (Part 03 §6.1, Part 04 §7.6) on the app pool: `name` trimmed, 1..=200 chars else `invalidInput{field:"name", reason:"empty"|"tooLong"}`; `sql` 1..=100,000 chars (not trimmed; empty after trim → `reason:"empty"`) else `{field:"sql", ..}`; SQLSTATE `23505` → `invalidInput{field:"name", reason:"duplicate"}`; update / delete of an unknown id → `notFound{entity:"savedQuery", id}`; list ordered by `name`; `updated_at` as RFC 3339 (`db::RFC3339_UTC_FMT`); update sets `updated_at = now()`.
- `execute` (Part 04 §8.1): validate first (`max_rows` default 1,000, valid 1..=10,000 else `invalidInput{field:"maxRows", reason:"outOfRange"}`; `sql.trim()` empty → `{field:"sql", reason:"empty"}`; `execution_id` empty → `{field:"executionId", reason:"empty"}`); one running execution (`{field:"executionId", reason:"alreadyRunning"}`); register `(execution_id, client.cancel_token(), cancelled flag)`; on the ro pool connection: `BEGIN READ ONLY` then `SET LOCAL statement_timeout = '30s'` (two `batch_execute` calls); `prepare(sql)` (42601 with message containing `cannot insert multiple commands into a prepared statement` → `{field:"sql", reason:"multipleStatements"}`); zero columns → Command mode (`simple_query(sql)`, `tag = command_tag(sql)`); else `simple_query("DECLARE outlooker_c NO SCROLL CURSOR FOR " + sql)` then `simple_query("FETCH FORWARD <max_rows + 1> FROM outlooker_c")`; a DECLARE failure with SQLSTATE `42601` (SHOW / EXPLAIN) → `ROLLBACK`, `BEGIN READ ONLY; SET LOCAL statement_timeout = '30s'`, `simple_query(sql)`, keep the first `max_rows + 1` rows. `truncated` = more than `max_rows` rows came back; keep `max_rows`. Always `ROLLBACK` then `DISCARD ALL` (errors of these two are ignored; a broken connection is dropped by the pool's recycle check). The registration is removed in every path.
- Mode: `MailList` when the prepared columns contain one named exactly `id` with type name `int8`, `int4` or `int2`; the ids are parsed from that column's text (null / unparsable skipped), hydrated with `search::hydrate_rows` on an app-pool connection; `missing_ids = ids.len() - rows.len()`. Else `Table { columns: [ColumnMeta { name, pg_type: type().name() }], rows: text cells cut by `cut_cell` }`. `elapsed_ms` around the whole execution.
- Error mapping (`map_sql_error`, exactly spec Part 04 §8.1; deviation S1 is resolved in the spec, see "Consumes cross-check"): 42601 with message containing `cannot insert multiple commands into a prepared statement` → `invalidInput{field:"sql", reason:"multipleStatements"}`; `25006`, `55000` (write into a non-updatable view, measured: `DELETE FROM v_mail` → 55000 at prepare) → `readOnlyViolation`; `42501` → `readOnlyViolation` only when the statement prepared and has no result columns (a write), else `sqlError` (Part 04 §12.2: `SELECT * FROM messages` → `sqlError 42501`); `57014` → `cancelled` when `cancel` was called for this execution, else `queryTimeout{timeoutMs: 30000}`; any other database error → `sqlError{sqlstate, message, position}` (`ErrorPosition::Original(n)` → `Some(n)`); connection errors → `DbNotReady`.
- `cancel(id)`: when `id` matches the running execution, set its flag and `cancel_token.cancel_query(tokio_postgres::NoTls)` (error ignored); otherwise `Ok(())`. `cancel_running()`: the same for whatever runs.
- Maintenance (Part 04 §9.1): `run(task)` → `gate.try_enter(Activity::Maintenance)?` (Busy reason from the gate holder); `started_at` from `SELECT to_char(now() AT TIME ZONE 'UTC', <fmt>)` on the admin connection; state `Running` stored + emitted; the spawned task holds the guard and the admin connection: `analyze` → `ANALYZE`; `vacuum` → `VACUUM (ANALYZE)` then `remove::delete_orphans`; `reindex` → `ensure_search_indexes(admin, log progress)`, then `DROP INDEX CONCURRENTLY` for each invalid index whose name matches `_ccnew[0-9]*$` (names read from `pg_index`/`pg_class` with `quote_ident`), then `REINDEX INDEX CONCURRENTLY <name>` for each of `REINDEX_INDEXES` in order (constant names; `log::info!` per index). End: state `Done` (or `Failed` with the error), `finished_at`, emitted; `current` back to `None`; guard dropped.

**Constraints:**
- User SQL runs only on the `outlooker_ro` pool; saved queries and hydration on the app pool; maintenance on an admin connection (table owner; `REINDEX CONCURRENTLY` needs it).
- No SQL text inspection decides safety (Part 02 §7.2); `command_tag` only labels the result.
- Command-mode tag: tokio-postgres 0.7.18 reports only the row count of `CommandComplete` (`SimpleQueryMessage::CommandComplete(u64)`, `src/lib.rs:252-258`), so the PostgreSQL command tag is not available; `tag` = first keyword of the statement, ASCII-uppercased. Recorded in the spec as a known limit (deviation S2, see "Consumes cross-check").
- Layer: service; depends on `dto`, `db`, `gate`, `sink`, `search::hydrate_rows`, `importer::remove::delete_orphans`.

**Acceptance criteria:**
- [ ] `cargo test -p outlooker-core --features test-support --test sql_console --test maintenance` exits 0 with these tests `ok`:
  - `bypass_attempts_all_fail` — each of `INSERT INTO v_pst_sources (path) VALUES ('x')`, `UPDATE v_pst_sources SET path = 'x'`, `DELETE FROM v_pst_sources`, `SET TRANSACTION READ WRITE; DELETE FROM v_pst_sources` (one `sql`), `SET ROLE outlooker_app`, `CREATE TEMP TABLE t (x int)`, `COPY (SELECT 1) TO 'copy-probe.txt'` (measured as `outlooker_ro`: "permission denied to COPY to a file"), `SELECT pg_read_file('/etc/hosts')`, `DO $$BEGIN DELETE FROM v_pst_sources; END$$`, `SELECT nextval('messages_id_seq')`, `CREATE FUNCTION f() RETURNS int LANGUAGE sql AS 'SELECT 1'`, `ALTER ROLE outlooker_ro SET work_mem = '1GB'` returns `Err`; `SET TRANSACTION READ WRITE` alone returns `Ok(Command)` (measured: PostgreSQL accepts it inside `BEGIN READ ONLY`) and the following `execute` of `DELETE FROM v_pst_sources` returns `Err`; afterwards the `pst_sources` row count (admin connection) is unchanged.
  - `read_only_role_rows` — `DELETE FROM v_mail` → `readOnlyViolation`; `SELECT * FROM messages` → `sqlError` with `sqlstate == "42501"`; `SET statement_timeout=0; SELECT 1` → `invalidInput{field:"sql", reason:"multipleStatements"}`; `SELECT pg_sleep(40)` → `queryTimeout{timeoutMs: 30000}`.
  - `cancel_during_sleep` — `sql_cancel` during `SELECT pg_sleep(10)` → `cancelled`; `cancel` of an unknown id → `Ok(())`.
  - `mode_detection` — `SELECT id FROM v_mail` → `MailList`; `SELECT 1 AS x` → `Table`; `SET work_mem = '8MB'` → `Command { tag: "SET" }`; `SHOW work_mem` → `Table` with one row.
  - `max_rows_limits` — `maxRows` 10,001 → `invalidInput{field:"maxRows"}`; omitted `maxRows` over `generate_series(1, 1500)` → 1,000 rows, `truncated: true`; `maxRows: 5` over 5 rows → `truncated: false`, over 6 rows → `truncated: true`.
  - `one_execution_at_a_time` — a second `execute` while `pg_sleep(2)` runs → `invalidInput{field:"executionId", reason:"alreadyRunning"}`.
  - `saved_queries_crud` — create, duplicate name → `invalidInput{field:"name", reason:"duplicate"}`, update, list order by name, delete, unknown id → `notFound{entity:"savedQuery"}`.
  - `maintenance_refused_while_import_holds_gate` — with a held `Activity::Import` guard, `run(Analyze)` → `busy{reason:"importRunning"}`.
  - `vacuum_removes_orphans` — one seeded orphan blob and one seeded orphan address are gone after `vacuum`, and `maintenance:status-changed` events are `running` then `done`.
  - `reindex_leaves_all_valid` — after `reindex`, every index of the `public` schema has `indisvalid = true` and all six search indexes exist.
- [ ] `cargo test -p outlooker-core sql_console::` exits 0 (unit tests `cut_cell_adds_ellipsis`, `command_tag_first_keyword`, `error_mapping_table`).

### R2-12 — Stage 2 steps

Verified before writing: every file below compiled and ran in planner-7's scratch crate (`scripts/tmp/p7core`, crate versions of Part 01, rustc 1.94.1, edition 2024) against PostgreSQL 18.6 with the spec schema, roles and views: `sql_console` 3 unit tests + 7 integration tests ok, `maintenance` 3 integration tests ok. A mutation of the `42501` rule turned `error_mapping_table` red, then green again after the revert.

Facts the code relies on (measured, see `scripts/tmp/baton-planner-2.md`): as `outlooker_ro` inside `BEGIN READ ONLY`, `DELETE FROM v_mail` fails at prepare with 55000; `SELECT * FROM messages` prepares and fails at `DECLARE` with 42501; `DELETE FROM v_pst_sources` fails at execute with 42501; `SET TRANSACTION READ WRITE` succeeds; `CREATE TEMP TABLE` 25006; `DECLARE … FOR SHOW` 42601. tokio-postgres 0.7.18 `SimpleQueryMessage` has `Row`, `CommandComplete(u64)`, `RowDescription` only (`src/lib.rs:250-260`), so the command tag is the first keyword (spec Part 04 §8.1 known limit).

Interface addition (test seam, feature `test-support` only): `SqlConsole::wait_statement_started(&self)` resolves once the running execution has sent its user statement (`tokio::sync::Notify`). `cancel_during_sleep` and `one_execution_at_a_time` need it to act while the statement runs without sleeping or polling. No production caller.

- [ ] **Step 1: Write the failing integration tests.** Create `src-tauri/crates/outlooker-core/tests/sql_console.rs`:

```rust
//! R2-12 SQL Console integration tests (spec Part 02 §10 bypass list, Part 04 §8.1, §12.2 row Read-only role).
#![cfg(feature = "test-support")]
use std::time::{Duration, Instant};

use outlooker_core::dto::error::AppError;
use outlooker_core::dto::sql::{SavedQueryInput, SqlExecuteRequest, SqlResult};
use outlooker_core::sql_console::SqlConsole;
use outlooker_core::test_support::TestCluster;

fn req(id: &str, sql: &str, max_rows: Option<u32>) -> SqlExecuteRequest {
    SqlExecuteRequest { execution_id: id.to_owned(), sql: sql.to_owned(), max_rows }
}

/// One source row and two messages, written by the table owner.
async fn seed(tc: &TestCluster) {
    tc.admin()
        .await
        .batch_execute(
            "INSERT INTO pst_sources (path, display_name, file_size_bytes, file_mtime) VALUES ('/seed.pst', 'seed', 1, now());
             INSERT INTO messages (dedupe_key, dedupe_kind, body_source, subject, body_text) VALUES
               (decode(repeat('01', 32), 'hex'), 'content_hash', 'text', 'first', 'body one'),
               (decode(repeat('02', 32), 'hex'), 'content_hash', 'text', 'second', 'body two');",
        )
        .await
        .unwrap();
}

async fn pst_count(tc: &TestCluster) -> i64 {
    tc.admin().await.query_one("SELECT count(*) FROM pst_sources", &[]).await.unwrap().get(0)
}

#[tokio::test]
async fn bypass_attempts_all_fail() {
    let tc = TestCluster::start().await;
    seed(&tc).await;
    let sc = SqlConsole::new(tc.db.clone());
    let before = pst_count(&tc).await;
    let attempts = [
        "INSERT INTO v_pst_sources (path) VALUES ('x')",
        "UPDATE v_pst_sources SET path = 'x'",
        "DELETE FROM v_pst_sources",
        "SET TRANSACTION READ WRITE; DELETE FROM v_pst_sources",
        "SET ROLE outlooker_app",
        "CREATE TEMP TABLE t (x int)",
        "COPY (SELECT 1) TO 'copy-probe.txt'",
        "SELECT pg_read_file('/etc/hosts')",
        "DO $$BEGIN DELETE FROM v_pst_sources; END$$",
        "SELECT nextval('messages_id_seq')",
        "CREATE FUNCTION f() RETURNS int LANGUAGE sql AS 'SELECT 1'",
        "ALTER ROLE outlooker_ro SET work_mem = '1GB'",
    ];
    for sql in attempts {
        let r = sc.execute(req("e1", sql, None)).await;
        assert!(r.is_err(), "{sql} must fail, got {r:?}");
    }
    // Accepted by PostgreSQL inside BEGIN READ ONLY (measured), but the transaction is rolled back per execution.
    let r = sc.execute(req("e2", "SET TRANSACTION READ WRITE", None)).await.unwrap();
    assert!(matches!(r, SqlResult::Command { ref tag, .. } if tag == "SET"), "{r:?}");
    assert!(sc.execute(req("e3", "DELETE FROM v_pst_sources", None)).await.is_err());
    assert_eq!(pst_count(&tc).await, before);
    assert_eq!(before, 1);
}

#[tokio::test]
async fn read_only_role_rows() {
    let tc = TestCluster::start().await;
    let sc = SqlConsole::new(tc.db.clone());
    assert_eq!(sc.execute(req("r1", "DELETE FROM v_mail", None)).await, Err(AppError::ReadOnlyViolation));
    match sc.execute(req("r2", "SELECT * FROM messages", None)).await {
        Err(AppError::SqlError { sqlstate, .. }) => assert_eq!(sqlstate, "42501"),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        sc.execute(req("r3", "SET statement_timeout=0; SELECT 1", None)).await,
        Err(AppError::invalid("sql", "multipleStatements"))
    );
    let t = Instant::now();
    assert_eq!(sc.execute(req("r4", "SELECT pg_sleep(40)", None)).await, Err(AppError::QueryTimeout { timeout_ms: 30_000 }));
    assert!(t.elapsed() < Duration::from_secs(39), "statement_timeout 30 s must fire, took {:?}", t.elapsed());
    // syntax errors keep their 1-based position for the editor marker
    match sc.execute(req("r5", "SELECT nosuchcolumn FROM v_mail", None)).await {
        Err(AppError::SqlError { sqlstate, position, .. }) => assert_eq!((sqlstate.as_str(), position), ("42703", Some(8))),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn cancel_during_sleep() {
    let tc = TestCluster::start().await;
    let sc = SqlConsole::new(tc.db.clone());
    let runner = sc.clone();
    let t = Instant::now();
    let mut task = tokio::spawn(async move { runner.execute(req("c1", "SELECT pg_sleep(10)", None)).await });
    tokio::select! {
        _ = sc.wait_statement_started() => {}
        r = &mut task => panic!("execute ended before its statement started: {r:?}"),
    }
    sc.cancel("c1".to_owned()).await.unwrap();
    assert_eq!(task.await.unwrap(), Err(AppError::Cancelled));
    assert!(t.elapsed() < Duration::from_secs(9), "cancel must interrupt pg_sleep, took {:?}", t.elapsed());
    assert_eq!(sc.cancel("unknown".to_owned()).await, Ok(()));
    assert_eq!(sc.cancel("c1".to_owned()).await, Ok(())); // finished id: idempotent
    // the connection went back to the pool clean: the next execution works
    assert!(matches!(sc.execute(req("c2", "SELECT 1 AS x", None)).await, Ok(SqlResult::Table { .. })));
}

#[tokio::test]
async fn mode_detection() {
    let tc = TestCluster::start().await;
    seed(&tc).await;
    let sc = SqlConsole::new(tc.db.clone());
    match sc.execute(req("m1", "SELECT id FROM v_mail ORDER BY id DESC", None)).await.unwrap() {
        SqlResult::MailList { rows, missing_ids, truncated, .. } => {
            assert_eq!(rows.iter().map(|r| r.subject.as_str()).collect::<Vec<_>>(), ["second", "first"]);
            assert_eq!((missing_ids, truncated), (0, false));
        }
        other => panic!("{other:?}"),
    }
    match sc.execute(req("m2", "SELECT 999999::int8 AS id UNION ALL SELECT min(id) FROM v_mail", None)).await.unwrap() {
        SqlResult::MailList { rows, missing_ids, .. } => assert_eq!((rows.len(), missing_ids), (1, 1)),
        other => panic!("{other:?}"),
    }
    match sc.execute(req("m3", "SELECT 1 AS x", None)).await.unwrap() {
        SqlResult::Table { columns, rows, truncated, .. } => {
            assert_eq!((columns[0].name.as_str(), columns[0].pg_type.as_str()), ("x", "int4"));
            assert_eq!(rows, vec![vec![Some("1".to_owned())]]);
            assert!(!truncated);
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        sc.execute(req("m4", "SET work_mem = '8MB'", None)).await.unwrap(),
        SqlResult::Command { ref tag, .. } if tag == "SET"
    ));
    match sc.execute(req("m5", "SHOW work_mem", None)).await.unwrap() {
        SqlResult::Table { rows, .. } => assert_eq!(rows, vec![vec![Some("64MB".to_owned())]]), // role default, the SET above was rolled back
        other => panic!("{other:?}"),
    }
    match sc.execute(req("m6", "SELECT repeat('x', 2500) AS long, NULL::text AS n", None)).await.unwrap() {
        SqlResult::Table { rows, .. } => {
            assert_eq!(rows[0][0].as_ref().unwrap().chars().count(), 2_001);
            assert_eq!(rows[0][1], None);
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn max_rows_limits() {
    let tc = TestCluster::start().await;
    let sc = SqlConsole::new(tc.db.clone());
    assert!(matches!(
        sc.execute(req("x1", "SELECT 1", Some(10_001))).await,
        Err(AppError::InvalidInput { ref field, ref reason }) if field == "maxRows" && reason == "outOfRange"
    ));
    assert_eq!(sc.execute(req("x2", "SELECT 1", Some(0))).await, Err(AppError::invalid("maxRows", "outOfRange")));
    assert_eq!(sc.execute(req("x3", "   ", None)).await, Err(AppError::invalid("sql", "empty")));
    assert_eq!(sc.execute(req("", "SELECT 1", None)).await, Err(AppError::invalid("executionId", "empty")));
    let table = |r: SqlResult| match r {
        SqlResult::Table { rows, truncated, .. } => (rows.len(), truncated),
        other => panic!("{other:?}"),
    };
    assert_eq!(table(sc.execute(req("x4", "SELECT g FROM generate_series(1, 1500) g", None)).await.unwrap()), (1_000, true));
    assert_eq!(table(sc.execute(req("x5", "SELECT g FROM generate_series(1, 5) g", Some(5))).await.unwrap()), (5, false));
    assert_eq!(table(sc.execute(req("x6", "SELECT g FROM generate_series(1, 6) g", Some(5))).await.unwrap()), (5, true));
    assert_eq!(table(sc.execute(req("x7", "SELECT g FROM generate_series(1, 10000) g", Some(10_000))).await.unwrap()), (10_000, false));
}

#[tokio::test]
async fn one_execution_at_a_time() {
    let tc = TestCluster::start().await;
    let sc = SqlConsole::new(tc.db.clone());
    let runner = sc.clone();
    let mut task = tokio::spawn(async move { runner.execute(req("a1", "SELECT pg_sleep(2)", None)).await });
    tokio::select! {
        _ = sc.wait_statement_started() => {}
        r = &mut task => panic!("execute ended before its statement started: {r:?}"),
    }
    assert_eq!(sc.execute(req("a2", "SELECT 1", None)).await, Err(AppError::invalid("executionId", "alreadyRunning")));
    assert!(matches!(task.await.unwrap(), Ok(SqlResult::Table { .. })));
    // the registration is removed after the first finished
    assert!(sc.execute(req("a3", "SELECT 1", None)).await.is_ok());
}

#[tokio::test]
async fn saved_queries_crud() {
    let tc = TestCluster::start().await;
    let sc = SqlConsole::new(tc.db.clone());
    let input = |name: &str, sql: &str| SavedQueryInput { name: name.to_owned(), sql: sql.to_owned() };
    let b = sc.saved_create(input("  beta  ", "SELECT 2")).await.unwrap();
    assert_eq!(b.name, "beta");
    assert_eq!(b.updated_at.len(), 20, "RFC 3339 UTC: {}", b.updated_at);
    assert!(b.updated_at.ends_with('Z'));
    let a = sc.saved_create(input("alpha", "SELECT 1")).await.unwrap();
    assert_eq!(sc.saved_create(input("alpha", "SELECT 3")).await, Err(AppError::invalid("name", "duplicate")));
    assert_eq!(sc.saved_create(input("   ", "SELECT 3")).await, Err(AppError::invalid("name", "empty")));
    assert_eq!(sc.saved_create(input(&"n".repeat(201), "SELECT 3")).await, Err(AppError::invalid("name", "tooLong")));
    assert_eq!(sc.saved_create(input("ok", " \n ")).await, Err(AppError::invalid("sql", "empty")));
    assert_eq!(sc.saved_create(input("ok", &"x".repeat(100_001))).await, Err(AppError::invalid("sql", "tooLong")));
    assert!(sc.saved_create(input(&"報".repeat(200), &"x".repeat(100_000))).await.is_ok());
    let updated = sc.saved_update(b.id, input("gamma", "SELECT 22")).await.unwrap();
    assert_eq!((updated.id, updated.name.as_str(), updated.sql.as_str()), (b.id, "gamma", "SELECT 22"));
    assert_eq!(sc.saved_update(b.id, input("alpha", "SELECT 9")).await, Err(AppError::invalid("name", "duplicate")));
    let names: Vec<String> = sc.saved_list().await.unwrap().into_iter().map(|q| q.name).collect();
    assert_eq!(names[..2], ["alpha".to_owned(), "gamma".to_owned()]);
    sc.saved_delete(a.id).await.unwrap();
    assert_eq!(sc.saved_delete(a.id).await, Err(AppError::not_found("savedQuery", a.id)));
    assert_eq!(sc.saved_update(a.id, input("zeta", "SELECT 1")).await, Err(AppError::not_found("savedQuery", a.id)));
    assert_eq!(sc.saved_list().await.unwrap().len(), 2);
}
```

Create `src-tauri/crates/outlooker-core/tests/maintenance.rs`:

```rust
//! R2-12 Maintenance Service integration tests (spec Part 03 §8.3, Part 04 §9.1).
#![cfg(feature = "test-support")]
use std::sync::Arc;

use outlooker_core::db::indexes::{drop_search_indexes, missing_or_invalid};
use outlooker_core::dto::error::{AppError, BusyReason};
use outlooker_core::dto::events::MaintenanceRunState;
use outlooker_core::dto::settings::MaintenanceTask;
use outlooker_core::gate::{Activity, Gate};
use outlooker_core::maintenance::{Maintenance, REINDEX_INDEXES};
use outlooker_core::sink::{CoreEvent, RecordingSink};
use outlooker_core::test_support::TestCluster;

fn service(tc: &TestCluster, gate: &Gate) -> (Maintenance, Arc<RecordingSink>) {
    let sink = Arc::new(RecordingSink::default());
    (Maintenance::new(tc.db.clone(), gate.clone(), sink.clone()), sink)
}

fn states(sink: &RecordingSink) -> Vec<(MaintenanceTask, MaintenanceRunState)> {
    sink.events()
        .into_iter()
        .map(|e| match e {
            CoreEvent::MaintenanceStatusChanged(s) => (s.task, s.state),
            other => panic!("unexpected event {other:?}"),
        })
        .collect()
}

#[tokio::test]
async fn maintenance_refused_while_import_holds_gate() {
    let tc = TestCluster::start().await;
    let gate = Gate::new();
    let (m, sink) = service(&tc, &gate);
    let held = gate.try_enter(Activity::Import).unwrap();
    assert_eq!(m.run(MaintenanceTask::Analyze).await, Err(AppError::Busy { reason: BusyReason::ImportRunning }));
    assert!(sink.events().is_empty());
    assert_eq!(m.current(), None);
    drop(held);
    m.run(MaintenanceTask::Analyze).await.unwrap();
    // a second task while the first runs is refused by the gate
    assert_eq!(m.run(MaintenanceTask::Analyze).await, Err(AppError::Busy { reason: BusyReason::MaintenanceRunning }));
    m.wait_idle().await;
    assert_eq!(gate.current(), None);
    assert_eq!(states(&sink), [(MaintenanceTask::Analyze, MaintenanceRunState::Running), (MaintenanceTask::Analyze, MaintenanceRunState::Done)]);
}

#[tokio::test]
async fn vacuum_removes_orphans() {
    let tc = TestCluster::start().await;
    let a = tc.admin().await;
    a.batch_execute(
        "INSERT INTO addresses (email, address_type) VALUES ('kept@x.example', 'smtp'), ('orphan@x.example', 'smtp');
         INSERT INTO messages (dedupe_key, dedupe_kind, body_source, sender_address_id)
           SELECT decode(repeat('0a', 32), 'hex'), 'content_hash', 'none', id FROM addresses WHERE email = 'kept@x.example';
         INSERT INTO image_blobs (source_hash, webp, width, height, source_mime)
           VALUES (decode(repeat('0b', 32), 'hex'), '\\x00', 1, 1, 'image/png'), (decode(repeat('0c', 32), 'hex'), '\\x00', 1, 1, 'image/png');
         INSERT INTO inline_images (message_id, ordinal, image_blob_id)
           SELECT m.id, 0, b.id FROM messages m, image_blobs b WHERE b.source_hash = decode(repeat('0b', 32), 'hex');",
    )
    .await
    .unwrap();
    let gate = Gate::new();
    let (m, sink) = service(&tc, &gate);
    m.run(MaintenanceTask::Vacuum).await.unwrap();
    let running = m.current().expect("current is Some while the task runs");
    assert_eq!((running.task, running.state), (MaintenanceTask::Vacuum, MaintenanceRunState::Running));
    m.wait_idle().await;
    let emails: Vec<String> = a.query("SELECT email FROM addresses", &[]).await.unwrap().iter().map(|r| r.get(0)).collect();
    assert_eq!(emails, ["kept@x.example"]);
    let blobs: i64 = a.query_one("SELECT count(*) FROM image_blobs", &[]).await.unwrap().get(0);
    assert_eq!(blobs, 1);
    assert_eq!(states(&sink), [(MaintenanceTask::Vacuum, MaintenanceRunState::Running), (MaintenanceTask::Vacuum, MaintenanceRunState::Done)]);
    let CoreEvent::MaintenanceStatusChanged(done) = sink.events().pop().unwrap() else { unreachable!() };
    assert!(done.finished_at.is_some() && done.error.is_none());
    assert_eq!(done.started_at, running.started_at);
    assert_eq!(m.current(), None);
}

#[tokio::test]
async fn reindex_leaves_all_valid() {
    let tc = TestCluster::start().await;
    let a = tc.admin().await;
    // REINDEX_INDEXES is exactly the set of non-constraint indexes of the migrated schema (Part 04 §3)
    let mut plain: Vec<String> = a
        .query(
            "SELECT c.relname FROM pg_index x JOIN pg_class c ON c.oid = x.indexrelid JOIN pg_namespace n ON n.oid = c.relnamespace
             WHERE n.nspname = 'public' AND NOT EXISTS (SELECT 1 FROM pg_constraint k WHERE k.conindid = x.indexrelid)",
            &[],
        )
        .await
        .unwrap()
        .iter()
        .map(|r| r.get(0))
        .collect();
    plain.sort();
    let mut listed: Vec<String> = REINDEX_INDEXES.iter().map(|s| s.to_string()).collect();
    listed.sort();
    assert_eq!(plain, listed);
    // damage: two search indexes missing, one invalid leftover of a failed concurrent rebuild
    a.batch_execute(
        "DROP INDEX messages_body_fts; DROP INDEX messages_sort;
         CREATE INDEX messages_subject_trgm_ccnew ON messages (subject);
         UPDATE pg_index SET indisvalid = false WHERE indexrelid = 'messages_subject_trgm_ccnew'::regclass;",
    )
    .await
    .unwrap();
    let gate = Gate::new();
    let (m, sink) = service(&tc, &gate);
    m.run(MaintenanceTask::Reindex).await.unwrap();
    m.wait_idle().await;
    assert_eq!(states(&sink).last(), Some(&(MaintenanceTask::Reindex, MaintenanceRunState::Done)), "{:?}", sink.events());
    let invalid: i64 = a
        .query_one(
            "SELECT count(*) FROM pg_index x JOIN pg_class c ON c.oid = x.indexrelid JOIN pg_namespace n ON n.oid = c.relnamespace
             WHERE n.nspname = 'public' AND NOT x.indisvalid",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(invalid, 0);
    let leftover: i64 = a.query_one("SELECT count(*) FROM pg_class WHERE relname LIKE '%ccnew%'", &[]).await.unwrap().get(0);
    assert_eq!(leftover, 0);
    assert!(missing_or_invalid(&a).await.unwrap().is_empty(), "all six search indexes exist and are valid");
    // dropped search indexes alone are also repaired
    drop_search_indexes(&a).await.unwrap();
    m.run(MaintenanceTask::Reindex).await.unwrap();
    m.wait_idle().await;
    assert!(missing_or_invalid(&a).await.unwrap().is_empty());
}
```

Run (in `src-tauri/`): `cargo test -p outlooker-core --features test-support --test sql_console --test maintenance`.
Expected: compile errors `unresolved import outlooker_core::sql_console::SqlConsole` and `unresolved import outlooker_core::maintenance::Maintenance` (the kickoff module files are empty).

- [ ] **Step 2: Write the pure helpers with their unit tests.** Create `src-tauri/crates/outlooker-core/src/sql_console/execute.rs`:

```rust
//! Pure helpers of the SQL Console Service (spec Part 04 §8.1): error mapping, cell cut, command tag.
use tokio_postgres::error::ErrorPosition;

use crate::dto::error::AppError;

use super::{CELL_MAX_CHARS, TIMEOUT_MS};

const MULTIPLE_COMMANDS: &str = "cannot insert multiple commands into a prepared statement";

/// Part 04 §8.1 SQLSTATE table, applied to an error of `prepare`, `DECLARE`, `FETCH` or `simple_query`.
/// `has_columns` is `false` only when `prepare` succeeded and the statement has no result columns (a write);
/// pass `true` for errors raised by `prepare` itself. `cancelled` = `cancel` was called for this execution.
pub fn map_sql_error(e: &tokio_postgres::Error, has_columns: bool, cancelled: bool) -> AppError {
    match e.as_db_error() {
        // Closed connection or I/O error: no SQLSTATE.
        None => AppError::DbNotReady,
        Some(db) => {
            let position = match db.position() {
                Some(ErrorPosition::Original(n)) => Some(*n),
                _ => None,
            };
            map_sqlstate(db.code().code(), db.message(), position, has_columns, cancelled)
        }
    }
}

/// The table itself, on plain values (unit-tested without a server).
pub fn map_sqlstate(sqlstate: &str, message: &str, position: Option<u32>, has_columns: bool, cancelled: bool) -> AppError {
    match sqlstate {
        "42601" if message.contains(MULTIPLE_COMMANDS) => AppError::invalid("sql", "multipleStatements"),
        "25006" | "55000" => AppError::ReadOnlyViolation,
        "42501" if !has_columns => AppError::ReadOnlyViolation,
        "57014" if cancelled => AppError::Cancelled,
        "57014" => AppError::QueryTimeout { timeout_ms: TIMEOUT_MS },
        _ => AppError::SqlError { sqlstate: sqlstate.to_owned(), message: message.to_owned(), position },
    }
}

/// Values longer than 2,000 chars → first 2,000 chars + "…".
pub fn cut_cell(v: &str) -> String {
    match v.char_indices().nth(CELL_MAX_CHARS) {
        Some((byte, _)) => format!("{}…", &v[..byte]),
        None => v.to_owned(),
    }
}

/// First keyword of the statement, ASCII-uppercased (known limit, Part 04 §8.1 step 3).
/// Leading whitespace, `--` line comments and `/* */` block comments are skipped.
pub fn command_tag(sql: &str) -> String {
    let mut s = sql;
    loop {
        s = s.trim_start();
        if let Some(rest) = s.strip_prefix("--") {
            s = rest.split_once('\n').map_or("", |(_, r)| r);
        } else if let Some(rest) = s.strip_prefix("/*") {
            s = rest.split_once("*/").map_or("", |(_, r)| r);
        } else {
            break;
        }
    }
    s.chars().take_while(|c| c.is_ascii_alphabetic()).collect::<String>().to_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cut_cell_adds_ellipsis() {
        assert_eq!(cut_cell("abc"), "abc");
        let exact = "報".repeat(2_000);
        assert_eq!(cut_cell(&exact), exact);
        let cut = cut_cell(&"報".repeat(2_001));
        assert_eq!(cut.chars().count(), 2_001);
        assert!(cut.ends_with('…'));
        assert_eq!(cut.chars().filter(|c| *c == '報').count(), 2_000);
    }

    #[test]
    fn command_tag_first_keyword() {
        assert_eq!(command_tag("set work_mem = '8MB'"), "SET");
        assert_eq!(command_tag("  \n\tDO $$BEGIN END$$"), "DO");
        assert_eq!(command_tag("-- note\n/* block */ Analyze v_mail"), "ANALYZE");
        assert_eq!(command_tag("(SELECT 1)"), "");
        assert_eq!(command_tag(""), "");
    }

    #[test]
    fn error_mapping_table() {
        let multi = "cannot insert multiple commands into a prepared statement";
        assert_eq!(map_sqlstate("42601", multi, None, true, false), AppError::invalid("sql", "multipleStatements"));
        assert_eq!(map_sqlstate("25006", "read-only", None, false, false), AppError::ReadOnlyViolation);
        assert_eq!(map_sqlstate("25006", "read-only", None, true, false), AppError::ReadOnlyViolation);
        assert_eq!(map_sqlstate("55000", "cannot delete from view", None, true, false), AppError::ReadOnlyViolation);
        assert_eq!(map_sqlstate("42501", "permission denied", None, false, false), AppError::ReadOnlyViolation);
        assert_eq!(
            map_sqlstate("42501", "permission denied for table messages", None, true, false),
            AppError::SqlError { sqlstate: "42501".into(), message: "permission denied for table messages".into(), position: None }
        );
        assert_eq!(map_sqlstate("57014", "canceling statement", None, true, true), AppError::Cancelled);
        assert_eq!(map_sqlstate("57014", "canceling statement", None, true, false), AppError::QueryTimeout { timeout_ms: 30_000 });
        assert_eq!(
            map_sqlstate("42601", "syntax error at or near \"SELEC\"", Some(1), true, false),
            AppError::SqlError { sqlstate: "42601".into(), message: "syntax error at or near \"SELEC\"".into(), position: Some(1) }
        );
        assert!(matches!(map_sqlstate("428C9", "generated", None, false, false), AppError::SqlError { .. }));
    }
}
```

`map_sqlstate` holds the table so the unit test runs without a server; `map_sql_error` (Stage 1 signature) only extracts SQLSTATE, message and `ErrorPosition::Original` from the driver error. `sql_console/mod.rs` is written in Step 4; until then add the four constants of Step 4 to `mod.rs` together with `pub mod execute;` so this step compiles.
Run: `cargo test -p outlooker-core sql_console::`.
Expected: `sql_console::execute::tests::cut_cell_adds_ellipsis ... ok`, `sql_console::execute::tests::command_tag_first_keyword ... ok`, `sql_console::execute::tests::error_mapping_table ... ok`; `test result: ok. 3 passed`.

Red check (before Step 3): in `map_sqlstate` change the arm `"42501" if !has_columns` to `"42501"`, run the command again → `error_mapping_table ... FAILED`; undo the change (re-apply this step's file content) → 3 passed.

- [ ] **Step 3: Write the saved-query CRUD.** Create `src-tauri/crates/outlooker-core/src/sql_console/saved.rs`:

```rust
//! Saved queries (spec Part 03 §6.1, Part 04 §7.6) on the outlooker_app pool.
use tokio_postgres::error::SqlState;

use crate::db::{RFC3339_UTC_FMT, pg_err};
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
        pg_err(e)
    }
}

pub async fn list(c: &tokio_postgres::Client) -> Result<Vec<SavedQuery>, AppError> {
    let sql = format!("SELECT {COLUMNS} FROM saved_queries ORDER BY name");
    let rows = c.query(&sql, &[&RFC3339_UTC_FMT]).await.map_err(pg_err)?;
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
    let n = c.execute("DELETE FROM saved_queries WHERE id = $1", &[&id]).await.map_err(pg_err)?;
    if n == 0 { Err(AppError::not_found("savedQuery", id)) } else { Ok(()) }
}
```

SQL text is built only from the constant `COLUMNS` and `$n` placeholders; the RFC 3339 format string `db::RFC3339_UTC_FMT` is bound as parameter `$1` of `to_char`, so no format text is pasted into SQL.

- [ ] **Step 4: Write the service.** Replace `src-tauri/crates/outlooker-core/src/sql_console/mod.rs` with:

```rust
//! SQL Console Service (spec Part 02 §3.1, Part 03 §6, Part 04 §8.1).
pub mod execute;
pub mod saved;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use deadpool_postgres::PoolError;
use tokio::sync::Notify;
use tokio_postgres::error::SqlState;
use tokio_postgres::types::Type;
use tokio_postgres::{CancelToken, Client, NoTls, SimpleQueryMessage};

use crate::db::Db;
use crate::dto::error::AppError;
use crate::dto::sql::{ColumnMeta, SavedQuery, SavedQueryInput, SqlExecuteRequest, SqlResult};

use execute::{command_tag, cut_cell, map_sql_error};

pub const DEFAULT_MAX_ROWS: u32 = 1_000;
pub const MAX_ROWS: u32 = 10_000;
pub const CELL_MAX_CHARS: usize = 2_000;
pub const TIMEOUT_MS: u32 = 30_000;

const BEGIN: &str = "BEGIN READ ONLY";
const SET_TIMEOUT: &str = "SET LOCAL statement_timeout = '30s'";
const DECLARE: &str = "DECLARE outlooker_c NO SCROLL CURSOR FOR ";

/// The one running execution (Part 03 §6.3 `execution_id → CancelToken`).
struct Running {
    execution_id: String,
    token: Option<CancelToken>,
    cancelled: Arc<AtomicBool>,
    /// The user statement has been sent (test seam `wait_statement_started`).
    started: bool,
}

impl Running {
    /// Sets the flag; returns the token to cancel with (used outside the lock).
    fn mark_cancelled(&self) -> Option<CancelToken> {
        self.cancelled.store(true, Ordering::SeqCst);
        self.token.clone()
    }
}

async fn send_cancel(token: Option<CancelToken>) {
    if let Some(token) = token {
        let _ = token.cancel_query(NoTls).await; // error ignored (Part 03 §6.3)
    }
}

#[derive(Clone)]
pub struct SqlConsole {
    db: Db,
    running: Arc<Mutex<Option<Running>>>,
    started: Arc<Notify>,
}

/// Removes the registration in every path (drop).
struct Registration {
    running: Arc<Mutex<Option<Running>>>,
}

impl Drop for Registration {
    fn drop(&mut self) {
        *self.running.lock().unwrap_or_else(|p| p.into_inner()) = None;
    }
}

fn pool_err(e: PoolError) -> AppError {
    match e {
        PoolError::Closed | PoolError::Backend(_) => AppError::DbNotReady,
        other => AppError::internal(format!("pool: {other}")),
    }
}

fn elapsed_ms(t: Instant) -> u32 {
    u32::try_from(t.elapsed().as_millis()).unwrap_or(u32::MAX)
}

impl SqlConsole {
    pub fn new(db: Db) -> SqlConsole {
        SqlConsole { db, running: Arc::new(Mutex::new(None)), started: Arc::new(Notify::new()) }
    }

    pub async fn saved_list(&self) -> Result<Vec<SavedQuery>, AppError> {
        let c = self.db.app_pool()?.get().await.map_err(pool_err)?;
        saved::list(&c).await
    }
    pub async fn saved_create(&self, input: SavedQueryInput) -> Result<SavedQuery, AppError> {
        let c = self.db.app_pool()?.get().await.map_err(pool_err)?;
        saved::create(&c, &input).await
    }
    pub async fn saved_update(&self, id: i64, input: SavedQueryInput) -> Result<SavedQuery, AppError> {
        let c = self.db.app_pool()?.get().await.map_err(pool_err)?;
        saved::update(&c, id, &input).await
    }
    pub async fn saved_delete(&self, id: i64) -> Result<(), AppError> {
        let c = self.db.app_pool()?.get().await.map_err(pool_err)?;
        saved::delete(&c, id).await
    }

    pub async fn execute(&self, req: SqlExecuteRequest) -> Result<SqlResult, AppError> {
        let started = Instant::now();
        let max_rows = req.max_rows.unwrap_or(DEFAULT_MAX_ROWS);
        if !(1..=MAX_ROWS).contains(&max_rows) {
            return Err(AppError::invalid("maxRows", "outOfRange"));
        }
        if req.sql.trim().is_empty() {
            return Err(AppError::invalid("sql", "empty"));
        }
        if req.execution_id.is_empty() {
            return Err(AppError::invalid("executionId", "empty"));
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        let _registration = {
            let mut slot = self.running.lock().unwrap_or_else(|p| p.into_inner());
            if slot.is_some() {
                return Err(AppError::invalid("executionId", "alreadyRunning"));
            }
            *slot = Some(Running { execution_id: req.execution_id.clone(), token: None, cancelled: cancelled.clone(), started: false });
            Registration { running: self.running.clone() }
        };
        let client = self.db.ro_pool()?.get().await.map_err(pool_err)?;
        if let Some(r) = self.running.lock().unwrap_or_else(|p| p.into_inner()).as_mut() {
            r.token = Some(client.cancel_token());
        }
        let outcome = self.run(&client, &req.sql, max_rows, &cancelled).await;
        let _ = client.batch_execute("ROLLBACK").await;
        let _ = client.batch_execute("DISCARD ALL").await;
        let mut result = outcome?;
        if cancelled.load(Ordering::SeqCst) {
            // cancel arrived while no statement was executing on the server
            return Err(AppError::Cancelled);
        }
        match &mut result {
            SqlResult::MailList { elapsed_ms: e, .. } | SqlResult::Table { elapsed_ms: e, .. } | SqlResult::Command { elapsed_ms: e, .. } => {
                *e = elapsed_ms(started);
            }
        }
        Ok(result)
    }

    /// Marks the user statement as sent; returns Err(Cancelled) when cancel came first.
    fn statement_starts(&self, cancelled: &AtomicBool) -> Result<(), AppError> {
        if cancelled.load(Ordering::SeqCst) {
            return Err(AppError::Cancelled);
        }
        if let Some(r) = self.running.lock().unwrap_or_else(|p| p.into_inner()).as_mut() {
            r.started = true;
        }
        self.started.notify_waiters();
        Ok(())
    }

    async fn run(&self, client: &Client, sql: &str, max_rows: u32, cancelled: &AtomicBool) -> Result<SqlResult, AppError> {
        let flag = || cancelled.load(Ordering::SeqCst);
        client.batch_execute(BEGIN).await.map_err(|e| map_sql_error(&e, true, flag()))?;
        client.batch_execute(SET_TIMEOUT).await.map_err(|e| map_sql_error(&e, true, flag()))?;
        let stmt = client.prepare(sql).await.map_err(|e| map_sql_error(&e, true, flag()))?;
        let columns: Vec<ColumnMeta> =
            stmt.columns().iter().map(|c| ColumnMeta { name: c.name().to_owned(), pg_type: c.type_().name().to_owned() }).collect();
        self.statement_starts(cancelled)?;
        if columns.is_empty() {
            client.simple_query(sql).await.map_err(|e| map_sql_error(&e, false, flag()))?;
            return Ok(SqlResult::Command { tag: command_tag(sql), elapsed_ms: 0 });
        }
        let limit = max_rows as usize + 1;
        let fetched = match client.simple_query(&format!("{DECLARE}{sql}")).await {
            Ok(_) => {
                let fetch = format!("FETCH FORWARD {limit} FROM outlooker_c");
                client.simple_query(&fetch).await.map_err(|e| map_sql_error(&e, true, flag()))?
            }
            Err(e) if e.code() == Some(&SqlState::SYNTAX_ERROR) => {
                // SHOW / EXPLAIN: DECLARE accepts only SELECT / VALUES (Part 04 §8.1 step 4).
                let _ = client.batch_execute("ROLLBACK").await;
                client.batch_execute(BEGIN).await.map_err(|e| map_sql_error(&e, true, flag()))?;
                client.batch_execute(SET_TIMEOUT).await.map_err(|e| map_sql_error(&e, true, flag()))?;
                client.simple_query(sql).await.map_err(|e| map_sql_error(&e, true, flag()))?
            }
            Err(e) => return Err(map_sql_error(&e, true, flag())),
        };
        let mut rows: Vec<Vec<Option<String>>> = fetched
            .iter()
            .filter_map(|m| match m {
                SimpleQueryMessage::Row(r) => Some((0..r.len()).map(|i| r.get(i).map(str::to_owned)).collect()),
                _ => None,
            })
            .take(limit)
            .collect();
        let truncated = rows.len() > max_rows as usize;
        rows.truncate(max_rows as usize);
        let id_col = stmt
            .columns()
            .iter()
            .position(|c| c.name() == "id" && [Type::INT8, Type::INT4, Type::INT2].contains(c.type_()));
        if let Some(i) = id_col {
            let ids: Vec<i64> = rows.iter().filter_map(|r| r[i].as_deref()?.parse().ok()).collect();
            let app = self.db.app_pool()?.get().await.map_err(pool_err)?;
            let mail = crate::search::hydrate_rows(&app, &ids).await?;
            let missing_ids = u32::try_from(ids.len() - mail.len()).unwrap_or(u32::MAX);
            return Ok(SqlResult::MailList { rows: mail, missing_ids, truncated, elapsed_ms: 0 });
        }
        let rows = rows.into_iter().map(|r| r.into_iter().map(|v| v.map(|s| cut_cell(&s))).collect()).collect();
        Ok(SqlResult::Table { columns, rows, truncated, elapsed_ms: 0 })
    }

    /// sql_cancel: idempotent; unknown or finished id → Ok(()).
    pub async fn cancel(&self, execution_id: String) -> Result<(), AppError> {
        let token = {
            let slot = self.running.lock().unwrap_or_else(|p| p.into_inner());
            slot.as_ref().filter(|r| r.execution_id == execution_id).and_then(Running::mark_cancelled)
        };
        send_cancel(token).await;
        Ok(())
    }

    /// Part 02 §2.5 step 2: cancel whatever runs.
    pub async fn cancel_running(&self) {
        let token = self.running.lock().unwrap_or_else(|p| p.into_inner()).as_ref().and_then(Running::mark_cancelled);
        send_cancel(token).await;
    }

    /// Test seam: resolves once the running execution has sent its user statement (Notify, no polling).
    #[cfg(feature = "test-support")]
    pub async fn wait_statement_started(&self) {
        loop {
            let notified = self.started.notified();
            if self.running.lock().unwrap_or_else(|p| p.into_inner()).as_ref().is_some_and(|r| r.started) {
                return;
            }
            notified.await;
        }
    }
}
```

Notes for the implementer:
  - The registration slot is taken before a pool connection is requested, so a second `execute` fails with `alreadyRunning` even while the first waits for a connection; `Registration::drop` frees the slot in every path (including `?` returns and panics).
  - `statement_starts` checks the cancel flag before the user statement, and `execute` returns `Cancelled` when the flag is set after a successful run: a cancel that reached the server while it was idle (between `prepare` and the statement) is not lost.
  - `ROLLBACK` then `DISCARD ALL` run after every outcome; their errors are ignored (a broken connection is dropped by the pool's recycle check, `RecyclingMethod::Fast` in `db::pools`).
  - MailList mode uses `search::hydrate_rows` (C13, R2-11) on an app-pool connection; `missing_ids = ids.len() - rows.len()`.
Run: `cargo test -p outlooker-core --features test-support --test sql_console`.
Expected: `bypass_attempts_all_fail`, `read_only_role_rows`, `cancel_during_sleep`, `mode_detection`, `max_rows_limits`, `one_execution_at_a_time`, `saved_queries_crud` all `ok`; `test result: ok. 7 passed`. `read_only_role_rows` takes about 30 s (it waits for the real `statement_timeout`); the whole file finishes in about 33 s.

- [ ] **Step 5: Write the Maintenance Service.** Replace `src-tauri/crates/outlooker-core/src/maintenance/mod.rs` with:

```rust
//! Maintenance Service (spec Part 03 §8.3, Part 04 §9.1): analyze / vacuum / reindex on an admin connection.
use std::sync::{Arc, Mutex};

use tokio::sync::Notify;

use crate::db::indexes::ensure_search_indexes;
use crate::db::{Db, RFC3339_UTC_FMT, pg_err};
use crate::dto::error::AppError;
use crate::dto::events::{MaintenanceRunState, MaintenanceState};
use crate::dto::settings::MaintenanceTask;
use crate::gate::{Activity, Gate};
use crate::sink::{CoreEvent, EventSink};

/// Every index created by Part 04 §3, in this order (REINDEX order of the `reindex` task).
pub const REINDEX_INDEXES: [&str; 17] = [
    "import_jobs_one_active_per_pst",
    "import_jobs_one_running",
    "import_jobs_pst_newest",
    "import_jobs_queue",
    "import_errors_page",
    "occurrences_message",
    "occurrences_folder",
    "recipients_address",
    "inline_images_blob",
    "messages_sender_address",
    "messages_first_pst",
    "messages_sort",
    "messages_body_trgm",
    "messages_subject_trgm",
    "messages_sender_trgm",
    "messages_recipients_trgm",
    "messages_body_fts",
];

/// Invalid leftovers of failed concurrent rebuilds, already quoted by the server.
const CCNEW_LEFTOVERS: &str = "SELECT quote_ident(n.nspname) || '.' || quote_ident(c.relname) \
     FROM pg_index x JOIN pg_class c ON c.oid = x.indexrelid JOIN pg_namespace n ON n.oid = c.relnamespace \
     WHERE n.nspname = 'public' AND NOT x.indisvalid AND c.relname ~ '_ccnew[0-9]*$' ORDER BY c.relname";
const NOW_TEXT: &str = "SELECT to_char(now() AT TIME ZONE 'UTC', $1)";

#[derive(Clone)]
pub struct Maintenance {
    db: Db,
    gate: Gate,
    sink: Arc<dyn EventSink>,
    current: Arc<Mutex<Option<MaintenanceState>>>,
    idle: Arc<Notify>,
}

async fn now_text(admin: &tokio_postgres::Client) -> Result<String, AppError> {
    Ok(admin.query_one(NOW_TEXT, &[&RFC3339_UTC_FMT]).await.map_err(pg_err)?.get(0))
}

async fn run_task(admin: &tokio_postgres::Client, task: MaintenanceTask) -> Result<(), AppError> {
    match task {
        MaintenanceTask::Analyze => admin.batch_execute("ANALYZE").await.map_err(pg_err),
        MaintenanceTask::Vacuum => {
            admin.batch_execute("VACUUM (ANALYZE)").await.map_err(pg_err)?;
            let (blobs, addresses) = crate::importer::remove::delete_orphans(admin).await?;
            log::info!("maintenance vacuum: {blobs} orphan blobs, {addresses} orphan addresses deleted");
            Ok(())
        }
        MaintenanceTask::Reindex => {
            ensure_search_indexes(admin, &|name: &str, done: usize, total: usize| {
                log::info!("maintenance ensure index {name} ({done}/{total})")
            })
            .await?;
            let leftovers = admin.query(CCNEW_LEFTOVERS, &[]).await.map_err(pg_err)?;
            for row in leftovers {
                let name: String = row.get(0); // catalog name quoted by quote_ident, not a user value
                admin.batch_execute(&format!("DROP INDEX CONCURRENTLY {name}")).await.map_err(pg_err)?;
                log::info!("maintenance dropped leftover index {name}");
            }
            for (i, name) in REINDEX_INDEXES.iter().enumerate() {
                admin.batch_execute(&format!("REINDEX INDEX CONCURRENTLY {name}")).await.map_err(pg_err)?;
                log::info!("maintenance reindex {name} ({}/{})", i + 1, REINDEX_INDEXES.len());
            }
            Ok(())
        }
    }
}

impl Maintenance {
    pub fn new(db: Db, gate: Gate, sink: Arc<dyn EventSink>) -> Maintenance {
        Maintenance { db, gate, sink, current: Arc::new(Mutex::new(None)), idle: Arc::new(Notify::new()) }
    }

    fn set_current(&self, state: Option<MaintenanceState>) {
        *self.current.lock().unwrap_or_else(|p| p.into_inner()) = state;
    }

    /// maintenance_run: enters the gate (Busy on conflict), emits `running`, spawns the task, returns at once.
    pub async fn run(&self, task: MaintenanceTask) -> Result<(), AppError> {
        let guard = self.gate.try_enter(Activity::Maintenance)?;
        let admin = self.db.connect_admin().await?;
        let started_at = now_text(&admin).await?;
        let running = MaintenanceState { task, state: MaintenanceRunState::Running, started_at, finished_at: None, error: None };
        self.set_current(Some(running.clone()));
        self.sink.emit(CoreEvent::MaintenanceStatusChanged(running.clone()));
        let this = self.clone();
        tokio::spawn(async move {
            let result = run_task(&admin, task).await;
            let finished_at = now_text(&admin).await.ok();
            let (state, error) = match result {
                Ok(()) => (MaintenanceRunState::Done, None),
                Err(e) => {
                    log::warn!("maintenance {task:?} failed: {e}");
                    (MaintenanceRunState::Failed, Some(e))
                }
            };
            this.sink.emit(CoreEvent::MaintenanceStatusChanged(MaintenanceState { state, finished_at, error, ..running }));
            drop(guard);
            this.set_current(None);
            this.idle.notify_waiters(); // wait_idle returns after the end event, the gate is free
        });
        Ok(())
    }

    /// The running task's state (None when idle).
    pub fn current(&self) -> Option<MaintenanceState> {
        self.current.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// Resolves when no task runs.
    pub async fn wait_idle(&self) {
        loop {
            let notified = self.idle.notified();
            if self.current().is_none() {
                return;
            }
            notified.await;
        }
    }
}
```

Notes: the spawned task owns the `GateGuard` and the admin connection; the end event is emitted before the guard is dropped and before `current` becomes `None`, so `wait_idle` returns only after the `done`/`failed` event and with a free gate. `DROP INDEX CONCURRENTLY` / `REINDEX INDEX CONCURRENTLY` run as single statements outside a transaction block (required by PostgreSQL). The leftover names come from `pg_class` already quoted by `quote_ident`; the `REINDEX` names are the constants of `REINDEX_INDEXES` (the 17 `CREATE INDEX` statements of Part 04 §3; `reindex_leaves_all_valid` asserts the set equals every non-constraint index of the migrated schema).
Run: `cargo test -p outlooker-core --features test-support --test maintenance`.
Expected: `maintenance_refused_while_import_holds_gate ... ok`, `vacuum_removes_orphans ... ok`, `reindex_leaves_all_valid ... ok`; `test result: ok. 3 passed`.

- [ ] **Step 6: Run the task acceptance commands.**
  - `cargo test -p outlooker-core --features test-support --test sql_console --test maintenance` → exit 0, 10 tests `ok` (names as in Steps 4 and 5).
  - `cargo test -p outlooker-core sql_console::` → exit 0 with `cut_cell_adds_ellipsis`, `command_tag_first_keyword`, `error_mapping_table` `ok` (the two integration files carry `#![cfg(feature = "test-support")]`, so they compile to 0 tests without the feature).
  - `grep -n "ro_pool" src-tauri/crates/outlooker-core/src/sql_console/mod.rs` → exactly one line (`execute`); `grep -rn "app_pool\|ro_pool\|connect_app" src-tauri/crates/outlooker-core/src/maintenance` → nothing (maintenance runs only on `connect_admin`).
  - `grep -rnE "428C9" src-tauri/crates/outlooker-core/src/sql_console` → only the unit-test line in `execute.rs` (no mapping arm).

- [ ] **Step 7: Commit.** Run `git rev-parse --abbrev-ref HEAD`; then `git add src-tauri/crates/outlooker-core/src/sql_console/mod.rs src-tauri/crates/outlooker-core/src/sql_console/saved.rs src-tauri/crates/outlooker-core/src/sql_console/execute.rs src-tauri/crates/outlooker-core/src/maintenance/mod.rs src-tauri/crates/outlooker-core/tests/sql_console.rs src-tauri/crates/outlooker-core/tests/maintenance.rs`, write the message `feat(core): SQL Console and Maintenance services (R2-12)` (plus the attribution line of the session) to `scripts/tmp/r212-msg.txt` (Write tool; the session hook blocks `/tmp`), `git commit -F scripts/tmp/r212-msg.txt -- <the six paths>`, then `git status --porcelain` shows none of the six paths.

## Task R2-09: Import Manager — queue, PST reader, pipeline orchestration

**Files:** (`core/` = `src-tauri/crates/outlooker-core/`)
- Modify/own: `core/src/importer/mod.rs` (adds `ImportManager`, `ImportTuning`, `RecoveryReport`, re-exports; keeps the module lines)
- Create/own (replace kickoff stubs): `core/src/importer/queue.rs` (job SQL, status derivation, enqueue/cancel/resume/remove, error paging), `core/src/importer/pipeline.rs` is **not** created — the job runner lives in `queue.rs` section "runner"; `core/src/importer/pst_reader.rs`, `core/src/importer/inflight.rs`, `core/src/importer/progress.rs`
- Own in Round 2 (defect fixes only): `core/src/importer/{types,webp,dedupe}.rs`, `core/src/importer/normalize/**`, `core/tests/golden/**`, `crates/pff/**`, `src-tauri/native/libpff/**`, `scripts/vendor-libpff.sh`, `scripts/fixtures/**`, `tests/fixtures/**`
- Create: `core/tests/import_pipeline.rs`, `core/examples/import_bench.rs`
- Modify: none outside the list (the `outlook-fixtures` cargo feature is requested from the manifest owner R2-13: `outlook-fixtures = ["test-support"]` in `core/Cargo.toml`)

**Interfaces:**
- Consumes: C1 `dto::import::*`, `dto::app::{AppPhase, LibraryCounts}`, `dto::events::{ImportStatusChanged, LibraryChanged}`; C2 `EventSink`, `CoreEvent::{ImportProgress, ImportStatusChanged, LibraryChanged}`; C3 `Gate::{try_enter, released}`, `Activity::{Import, Removal}`; C5 `pff::{File, Item, AttachmentType, tags, filetime_to_unix_micros}` + `File::message_store`, `Item::attached_item` (requested); C6 `paths::Paths`; C8 `Db::{app_pool, connect_app, connect_admin, data_root}`, `db::indexes::{drop_search_indexes, ensure_search_indexes}`, `TestCluster::{start, kill, restart, db, admin}`; C9 (R1-05); C11 `writer::{Writer, WriterHandle, WriterConfig, WriterLimits, Checkpoint, upsert_folders, load_known_hashes, map_db_error, stage_db, AFTER_JOB_VACUUM}`, C12 `remove::run` (R2-10, lands first); C15 `watch::Receiver<AppPhase>` from `Supervisor::phase_watch()` (R2-08).
- Produces (C14 ImportManager; consumer R2-13 `core::app` and the Command Layer — signatures agreed with planner-3, requests P2-1 … P2-4):

```rust
// core/src/importer/mod.rs
#[derive(Clone)]
pub struct ImportTuning {
    pub writer: WriterLimits,                 // production: WriterLimits::default()
    /// Test seam: after this many committed batches the job is dropped like a process kill
    /// (no state change, marker left on disk, writer aborted). None in production.
    pub crash_after_batches: Option<i64>,
    /// Test seam, one-shot: in the first job attempt of this ImportManager the reader blocks after sending the item
    /// whose seq equals it, until the job is cancelled (user cancel, shutdown or DB pause). Later attempts (re-run
    /// after a DB pause, resumed job) never block. None in production.
    pub hold_reader_after: Option<i64>,
    /// Test seam: called after every commit with batches_committed (writer task; must not block).
    pub on_batch: Option<std::sync::Arc<dyn Fn(i64) + Send + Sync>>,
}
impl Default for ImportTuning;   // WriterLimits::default(), None, None, None

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RecoveryReport { pub requeued: Vec<i64>, pub cancelled: Vec<i64> }   // pst ids

#[derive(Clone)]
pub struct ImportManager { /* Arc<Inner> */ }
impl ImportManager {
    pub fn new(db: Db, gate: Gate, paths: Paths, sink: std::sync::Arc<dyn EventSink>,
               phase: tokio::sync::watch::Receiver<AppPhase>, tuning: ImportTuning) -> ImportManager;
    pub async fn list(&self) -> Result<Vec<PstSource>, AppError>;                          // pst_list
    pub async fn enqueue(&self, paths: Vec<String>) -> Result<Vec<EnqueueResult>, AppError>; // import_enqueue
    pub async fn cancel(&self, pst_id: i64) -> Result<(), AppError>;                       // import_cancel
    pub async fn resume(&self, pst_id: i64) -> Result<(), AppError>;                       // import_resume
    pub async fn remove(&self, pst_id: i64) -> Result<(), AppError>;                       // pst_remove
    pub async fn errors(&self, req: ImportErrorsRequest) -> Result<ImportErrorPage, AppError>; // import_errors
    /// Part 02 §2.2 step 8: running → queued, cancelling → cancelled.
    pub async fn recover_after_crash(&self) -> Result<RecoveryReport, AppError>;
    /// Part 02 §2.2 step 9: holds gate Import, ensure_search_indexes on an admin connection.
    pub async fn self_heal_indexes(&self) -> Result<(), AppError>;
    /// Spawns the queue worker once (later calls do nothing).
    pub fn start_worker(&self);
    /// Part 02 §2.5 step 1: running job → cancel flag "shutdown", drain, last batch + checkpoint committed,
    /// state 'queued'. Waits at most `timeout`; Err(Internal{"import shutdown timed out"}) when it did not stop.
    pub async fn shutdown(&self, timeout: std::time::Duration) -> Result<(), AppError>;
    /// Every terminal job outcome as (pst_id, status): Completed, Cancelled, Failed, Queued (after shutdown /
    /// pause), and Removing → removed (sent as (pst_id, ImportStatus::Removing) once the row is deleted).
    /// The crash_after_batches seam sends (pst_id, Running) when it drops the job; the worker task ends with it.
    pub fn job_events(&self) -> tokio::sync::broadcast::Receiver<(i64, ImportStatus)>;
}
#[cfg(feature = "test-support")]
impl ImportManager {
    /// Runs one import job whose items come from `items` instead of a PST (the stages after the reader are the
    /// production ones). Creates the source row (path, size 1, mtime now), the job, the folders. Returns pst_id
    /// after the job ended. Identity = the file's size + mtime when `path` exists.
    pub async fn import_synthetic(&self, path: &str, folders: Vec<FolderIn>, items: Vec<RawItem>) -> Result<i64, AppError>;
    /// Same, but returns pst_id right after queuing. Synthetic items are registered per source path in a
    /// process-global map, so a resumed or recovered job (also on a fresh ImportManager) reads them again.
    /// Synthetic jobs write no inflight marker (no libpff calls).
    pub async fn enqueue_synthetic(&self, path: &str, folders: Vec<FolderIn>, items: Vec<RawItem>) -> Result<i64, AppError>;
}

// core/src/importer/pst_reader.rs
pub struct ScanResult { pub folders: Vec<FolderIn>, pub items_total: i64, pub scan_errors: Vec<ItemError> }
pub enum ReaderMsg { Scanned(ScanResult), Item(RawItem), Failed { stage: ImportErrorStage, message: String } }
pub struct ReaderParams { pub path: std::path::PathBuf, pub cursor_seq: i64, pub pending: Option<crate::importer::inflight::InflightRecord>,
                          pub inflight: Option<crate::importer::inflight::Inflight>, pub cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
                          pub hold_after: Option<i64>, pub budget: std::sync::Arc<crate::importer::progress::ByteBudget>,
                          pub job_id: i64 }   // written into every inflight record
/// Spawns the reader std::thread; it owns the pff::File. Messages arrive on a sync_channel(256).
pub fn spawn(params: ReaderParams) -> (std::sync::mpsc::Receiver<ReaderMsg>, std::thread::JoinHandle<()>);
/// Deterministic list of (seq, pst_item_id, folder path) without reading message properties (tests).
pub fn walk_ids(path: &std::path::Path) -> Result<Vec<(i64, i64, String)>, pff::Error>;

// core/src/importer/inflight.rs
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InflightRecord { pub job_id: i64, pub seq: i64, pub folder_path: String,
                            pub pst_item_id: i64,   // 0 until the reader has the item's identifier
                            pub attempts: u32 }     // times the reader has started this item (Part 02 §6.3)
#[derive(Debug, Clone)]
pub struct Inflight { /* path */ }
impl Inflight {
    pub fn new(path: std::path::PathBuf) -> Inflight;    // <data_root>/import-inflight
    pub fn write(&self, r: &InflightRecord);              // overwrite, no fsync, errors logged
    pub fn read(&self) -> Option<InflightRecord>;
    pub fn clear(&self);
}

// core/src/importer/progress.rs
pub struct Throughput { /* VecDeque<(Instant, items_done, bytes)> over 10 s */ }
impl Throughput {
    pub fn new() -> Throughput;
    pub fn record(&mut self, now: std::time::Instant, items_done: i64, bytes: u64);
    pub fn bytes_per_sec(&self) -> Option<i64>;
    pub fn eta_seconds(&self, items_total: Option<i64>, items_done: i64) -> Option<i64>;
}
/// 256 MB pending-bytes budget between reader and writer (Mutex + Condvar).
pub struct ByteBudget { /* */ }
impl ByteBudget {
    pub fn new(limit: u64) -> ByteBudget;
    pub fn acquire(&self, n: u64, cancel: &std::sync::atomic::AtomicBool);   // blocks while used + n > limit (unless used == 0) and not cancelled
    pub fn release(&self, n: u64);
    pub fn wake_all(&self);                                                   // called on cancel
    pub fn used(&self) -> u64;                                                // pending bytes (tests)
    pub(crate) fn wait_cancelled(&self, cancel: &std::sync::atomic::AtomicBool); // hold seam: blocks until cancel
}
pub const BYTE_BUDGET: u64 = 256 * 1024 * 1024;
```

**Behavior:**
- `enqueue` (Part 03 §7.2): `paths` empty → `invalidInput{field:"paths", reason:"empty"}`; > 100 → `reason:"tooMany"`. Per path, in input order: not absolute → `Rejected{FileAccess{path, reason:"notAbsolute"}}`; `metadata` error → `"notFound"`; not a file → `"notAFile"`; open / read of the first 4 bytes fails → `"unreadable"`; first 4 bytes ≠ `!BDN` → `Rejected{NotAPst{path}}`; size + mtime (mtime truncated to whole microseconds, the precision of `timestamptz`) → `INSERT INTO pst_sources … ON CONFLICT ON CONSTRAINT pst_sources_same_file_once DO NOTHING RETURNING id`; no row → `Rejected{AlreadyImported{pst_id}}` (existing id); else `INSERT INTO import_jobs (pst_id, kind, state) VALUES ($1, 'import', 'queued')`, emit `ImportStatusChanged{status: Queued, source}`, `Queued{pst}`. Then kick the worker.
- `list` (Part 04 §2.1, §7.6): one query with `LATERAL` newest job + checkpoint + error count; status mapping table of §2.1 (`remove` queued/running → `Removing`, `remove` failed → `Failed`); `progress` = checkpoint row mapped to `ImportProgress` (`bytes_per_sec` / `eta_seconds` only for the running PST from `Throughput`), `None` without a checkpoint; `message_count = stored + duplicates`, `new_message_count = stored`; order: `Running`/`Cancelling` first, then `Queued` and `Removing` by job id ascending, then the rest by `queued_at` descending.
- `cancel`: unknown → `notFound{entity:"pst", id}`; newest job import `queued` → `cancelled` (finished_at now), emit; `running` → `cancelling` + cancel flag (reason User), emit; else `invalidInput{field:"pstId", reason:"notCancellable"}`.
- `resume`: newest job import `cancelled` / `failed` only (else `invalidInput{field:"pstId", reason:"notResumable"}`); re-validate the file exactly as enqueue (`FileAccess` / `NotAPst` errors returned directly); `INSERT` a new `queued` import job; emit; kick.
- `remove` (Part 04 §4.3, Part 03 §7.2): status `Running`/`Cancelling` → `busy{reason:"importRunning"}`; `Removing` → `busy{reason:"pstRemoving"}`; else in one transaction: newest job import `queued` → `cancelled`, then `INSERT … kind 'remove', state 'queued'`; emit `Removing`; kick.
- `errors`: unknown pst → `notFound`; `limit` default 100, 1..=500 else `invalidInput{field:"limit", reason:"outOfRange"}`; keyset on `(occurred_at, id)` ascending; cursor = base64url (no padding) JSON `{"t":"<occurred_at text>","i":<id>}`, bad cursor → `invalidInput{field:"cursor", reason:"invalid"}`.
- Worker loop (spawned by `start_worker`): wait until phase `Ready` (`watch`), pick `SELECT id, pst_id, kind FROM import_jobs WHERE state = 'queued' ORDER BY id LIMIT 1`; none → wait on the kick `Notify` (or phase change); `gate.try_enter(Import|Removal)`; on `Busy` wait on `gate.released()` and retry; `UPDATE … SET state='running', started_at=now() WHERE id=$1 AND state='queued'` (0 rows → next); run the job; release the gate; emit; loop.
- Import job runner: (1) file identity (`metadata`; error → job `failed` + error row `open`/`job_failed`); (2) `Checkpoint::begin` (resets the cursor when size or mtime changed); (3) inflight marker (spec Part 02 §6.3 rule 1): `Inflight::read()` with `job_id` = this job → `pending = Some(record)`; a marker of another job is ignored (and overwritten by the first item); when step 2 reset the checkpoint (file changed, items may be renumbered) → `pending = None`; (4) `pst_reader::spawn`; `Scanned` → `upsert_folders`, scan error rows, `Checkpoint::set_phase(Importing, items_total)`; emit progress; `Failed{stage}` → job `failed` + error row (`job_failed`); (5) index policy: `remaining = items_total - cursor_seq`, `messages = count(*)`; `2 * remaining >= messages` → `drop_search_indexes(admin)` before the first batch (Part 04 §6.4); (6) `webp_quality` = `SELECT webp_quality FROM settings`; `known = load_known_hashes`; `Writer::spawn(db.connect_app())` with `on_commit` = update counters, emit `ImportProgress`, emit `LibraryChanged` when `stored` grew, call `tuning.on_batch`; (7) pipeline thread: a dedicated `rayon::ThreadPool` (`cores - 1`, at least 1) runs `rx.into_iter().par_bridge()` over reader items: `normalize` then `writer.sender().blocking_send`, releasing the item's bytes from `ByteBudget` after the send; the reader acquires bytes before each send; (8) end of input → `writer.finish()`; (9) `Finalizing`: `set_phase(Finalizing)`, emit, `ensure_search_indexes(admin)` always (idempotent; covers jobs that dropped indexes, also when cancelled or failed), `AFTER_JOB_VACUUM` on an app connection, `Inflight::clear` (marker deleted at every job end: Completed, Cancelled, Failed); (10) final state: completed / cancelled (user) / queued (shutdown) / failed; emit `ImportStatusChanged`, `LibraryChanged` with fresh counts, `job_events`.
- Writer error `FileAccess{reason:"insufficientSpace"}` → cancel the reader, drain, job `failed`, error row stage `store`, severity `job_failed`, message `disk space low`; then step 9.
- PG crash (Part 02 §6.1 row, C15): a writer / DB error mapping to `DbNotReady`, or the phase leaving `Ready`, → `pause_for_db`: set the cancel flag (reason Pause), abort the writer (uncommitted batch dropped), join the reader; wait for a non-`Ready` phase (30 s deadline, one timer, not a loop) and then for `Ready`; re-run the job from step 1 with the job still `running` (the checkpoint resumes it; the inflight marker is deleted before the re-run, because the reader stopped cleanly between items). If the phase becomes `Failed` (restart budget used up), `Failed` is not a wake-up: the job stays paused (row `running`) and resumes from its checkpoint on the next `Ready` (`app_retry` in the same process; `recover_after_crash` runs only once per process). Only shutdown ends the wait; the row then stays `running` and `recover_after_crash` re-queues it at the next start.
- Shutdown: cancel flag reason Shutdown, wake the budget and a held reader, writer `finish`, state `queued` (`started_at` kept, `finished_at` NULL), `Inflight::clear` (the item in progress completed cleanly, Part 02 §6.3 rule 4), skip the rest of step 9 (startup self-heal covers indexes), wait ≤ `timeout`.
- Removal job: gate `Removal`; `remove::run(&app_conn, pst_id)`; Ok → emit `ImportStatusChanged{removed: true, source: None}`, `LibraryChanged`, `job_events (pst_id, Removing)`; Err → job `failed`, error row stage `store` / `job_failed`, emit `Failed`.
- Reader (`pst_reader.rs`, Part 04 §5.1–§5.3, §5.8): `File::open` error → `Failed{Open}`; IPM subtree root = folder whose identifier equals the little-endian u32 in the last 4 bytes of `message_store().binary(0, 0x35e0)`; walk from the PST root depth-first by sub-folder index; folders under the IPM root get paths relative to it (`Inbox/Projects`), others `~/<path from PST root>`; `/` in a name → U+2215 in `path`; a folder that fails to list → `scan_errors` row (`scan`, `degraded`) and its subtree skipped; `items_total` = Σ `sub_message_count`. Items: per folder, per sub-message index: `seq += 1`; `seq <= cursor_seq` → skip without reading; inflight marker per spec Part 02 §6.3 rule 2 (written before the first libpff call on the item with `pst_item_id` 0, and again with the PST item id after `identifier()` in the same attempt, so a crash inside `sub_message` / `identifier` also counts): no pending record for this `seq` → write the marker with `attempts = 1`; pending record for this `seq` with `attempts == 1` → write `attempts = 2` and read the item again (retry once); pending record with `attempts >= 2` → `Unreadable{Parse, "parser crashed on this item"}` without touching libpff (one `item_skipped` / `parse` row; the cursor advances); once the reader passes the pending `seq` the pending record is dropped; then read: message class (`0x001a`) not starting (ASCII case-insensitive) with `IPM.Note`, `IPM.Schedule.Meeting`, `REPORT.IPM.Note` → `NonMail`; properties of `RawMail` (FILETIME → `SystemTime` via `filetime_to_unix_micros`), bodies, recipients (one record set per recipient), attachments (method, names, size, MIME, content id; data bytes only when a content id exists and size ≤ 25 MB, via `attachment_data(25 MB)`; embedded message subject via `attached_item()`); any `pff::Error` → `Unreadable{Parse, message}`. The cancel flag is checked before each item.

**Constraints:**
- libpff only on the reader thread (`pff::File` is `!Send`). Bounded channels of 256 at reader → normalize and normalize → writer; byte budget 256 MB.
- Index DDL only on admin connections from `Db::connect_admin`. `importer` imports no `settings` module.
- No polling: waits use `watch`, `Notify`, `broadcast`, `Condvar`, `gate.released()` and the writer's batch deadline.
- Layer: service; depends on `dto`, `db`, `gate`, `sink`, `paths`, `pff`, `importer::{types, normalize, webp, dedupe, writer, remove}`.

**Acceptance criteria:**
- [ ] `cargo test -p outlooker-core --features test-support --test import_pipeline` exits 0 with these tests `ok` (each test pre-registers its expected values as constants at the top of the test before running):
  - `outlook_pst_import` — on `outlook.pst`: folder paths include `Inbox` and `Sent Items`; the one message stored with `body_source = 'html'` and its attachment metadata, with 2 occurrences (Inbox and Recovered_Group2); `scan_is_deterministic` — two `walk_ids` give identical `(seq, pst_item_id)` lists.
  - `byte_copy_dedupes_and_removal` — `outlook.pst` holds the Welcome mail twice (Inbox and Recovered_Group2), so one import gives 1 message + 2 occurrences; `outlook.pst` + its byte copy (no Message-ID) → 1 message, 4 occurrences (fallback-hash dedupe within and across PSTs); removing one keeps the message; removing both deletes it and its orphans (`addresses`, `attachments`).
  - `corrupt_copy_outcome` — the test copies `outlook.pst`, zeroes the 1,454 bytes at offsets 173,568 and 138,240 (the two data blocks of the `IPM.Note` "Welcome" property contexts, NIDs `0x200024` / `0x200044`, measured in `scripts/tmp/p2-pstprobe.py`; the test first asserts the BLAKE3 of the original file so the offsets stay valid) and pre-registers: job `Completed`, ≥ 1 `import_errors` row with `severity = 'item_skipped'`, `stage = 'parse'`, 0 messages. The observed outcome must equal it; any other outcome is reported, never re-tuned.
  - `synthetic_cjk` — Big5 HTML (`internet_codepage` 950) and UTF-8 HTML items: stored `body_text` contains `報價單`.
  - `synthetic_inline_images` — cid images → `outlooker-inline:n` with `inline_images` rows; a logo repeated in 10 items → 1 `image_blobs` row.
  - `synthetic_attachments` — embedded message (method 5 → `.msg`, `application/vnd.ms-outlook`), missing MIME tag (extension guess), long file name (kept up to 1,024 chars).
  - `synthetic_hostile` — the stored `body_html` of the hostile corpus passes the R1-05 pattern check (a copy `hostile_violations` inside `tests/import_pipeline.rs`, because the R1-05 helper is `pub(crate)` in a `#[cfg(test)]` module).
  - `synthetic_message_id_dedupe` — two synthetic sources sharing 3 Message-IDs: each shared id stored once with one occurrence per source; a mail copied into a second folder of one source adds one occurrence.
  - `index_policy_first_import_drops_and_rebuilds` — first import into an empty library: during the job (reader held by `hold_reader_after`, deterministic; an `on_batch` hook cannot run an async query without blocking the writer) `missing_or_invalid` lists all six; then cancel + resume reach `Completed`; afterwards it is empty, all six are valid and all six index OIDs changed.
  - `index_policy_rebuilt_after_cancel_and_disk_failure` — same check after a cancelled job and after a job failed through `WriterLimits.min_free_bytes = u64::MAX` (error row `store` / `job_failed`); the drop in the failed job is proven by changed OIDs.
  - `index_policy_incremental_keeps_indexes` — an import whose remaining items are < 50% of `messages`: the six index OIDs in `pg_class` are unchanged before / after (a drop + create always gives a new OID; this is the statement-log check).
  - `after_job_vacuum_ran` — `pg_stat_user_tables.last_vacuum` (or `last_autovacuum`) of `messages` is set after a job.
  - `resume_after_cancel_equals_uninterrupted` — cancel after batch 1, resume: same `dedupe_key` set and counts as an uninterrupted run (reference library in a second `TestCluster`).
  - `resume_after_kill_equals_uninterrupted` — `crash_after_batches = 1`, then `recover_after_crash` + `start_worker` on a fresh `ImportManager`: identical rows.
  - `mtime_change_resets_cursor` — touch the PST mtime between cancel and resume: the checkpoint resets (`cursor_seq` restarts) and the final counts are correct.
  - Poison item (spec Part 02 §6.3 and §10 Fault injection; each case asserts that the marker file is gone after the job and that nothing is written under the `Paths` data root, only under `Db::data_root()`):
    - `poison_marker_attempts_1_retries_item` — one earlier crash: a marker of this job with seq 1 and `attempts = 1` → item 1 is retried and stored, no error row (1 message, 2 occurrences).
    - `poison_marker_attempts_2_skips_item` — two earlier crashes: a marker of this job with seq 2 and `attempts = 2` → item 2 is skipped with exactly one `import_errors` row (`parse`, `item_skipped`, its PST item id and folder path, "parser crashed on this item"); the cursor passes it (1 message, 1 occurrence).
    - `poison_marker_of_other_job_is_ignored` — a marker with another job's id → ignored, no error row (1 message, 2 occurrences).
    - `poison_marker_dropped_on_checkpoint_reset` — a marker of this job with seq 2 and `attempts = 2`, plus a checkpoint with another file identity (the job resets its cursor) → the marker is not honored, no error row (1 message, 2 occurrences).
    - `retry_rewrites_marker_before_reading` — with a pending record `attempts = 1`, the marker on disk says `attempts = 2` when item 1 arrives (a death during the retry leads to the skip on the next start).
  - `pg_crash_mid_import_resumes` — during an import (`on_batch` after batch 2): `TestCluster::kill`, phase `StartingDb` on the test's watch sender, `TestCluster::restart`, phase `Ready` → job `Completed`; `dedupe_key` set and counts equal one uninterrupted run; no duplicate occurrence rows.
  - `pg_crash_failed_then_retry_resumes` — same start, but the phase goes `StartingDb` → `Failed` before `TestCluster::restart` and `Ready` (restart budget used up, then `app_retry`) → the job ends `Completed` within 20 s with the same `dedupe_key` set and counts as one uninterrupted run.
  - `recover_after_crash_states` — rows left `running` / `cancelling` become `queued` / `cancelled` (set in two rounds: `import_jobs_one_running` allows one running / cancelling job per library); the `queued` one resumes from `cursor_seq`.
  - `pst_list_status_derivation` — every job kind/state row of Part 04 §2.1 including `Removing`, checked one row at a time (same unique index), then the list order on a second set; `pst_remove_with_queued_job` — remove a PST whose import job is queued (worker not started): no `23505`, the PST is gone after the worker runs.
  - `enqueue_validation` — not absolute, missing, not a PST, already imported → the four `Rejected` errors; 0 and 101 paths → `invalidInput`.
- [ ] `cargo build -p outlooker-core --example import_bench` exits 0.
- [ ] With feature `outlook-fixtures` (Windows phase W2): `cargo test -p outlooker-core --features test-support,outlook-fixtures --test import_pipeline` runs `fixtures_basic`, `fixtures_cjk`, `fixtures_inline`, `fixtures_attach`, `fixtures_dup` (`dup-a` + `dup-b` → `messages` = 50, `message_occurrences` = 81), `fixtures_hostile`, `fixtures_corrupt`; each fails with "missing fixture tests/fixtures/pst/<name>.pst (Windows phase W2)" when the file is absent.

### R2-09 — Stage 2 steps

Verified before writing: every file below compiled (0 warnings in the R2-09 files) and ran in planner-8's scratch crate (`scripts/tmp/p7core`: crate versions of Part 01, rustc 1.99.1 (project toolchain; sysinfo 0.39.6 needs ≥ 1.95), edition 2024, foundation R1-02 `pff` linked to the prepared libpff sdist, R1-05 normalize, stand-ins with the Stage 1 signatures for C8 `Db` / `TestCluster` and C11 / C12) against PostgreSQL 18.6 with the spec schema: unit tests `importer::{inflight,progress,pst_reader,queue}` 9 ok; `--test import_pipeline` 26 ok (planner-9 added `pg_crash_failed_then_retry_resumes` and `poison_marker_dropped_on_checkpoint_reset` after the arch-system review, each seen red before its fix; the two `pg_crash_*` tests run with `--test-threads=1` in scratch only, because the scratch `TestCluster::kill` stops a shared cluster); `--features outlook-fixtures` → the 7 `fixtures_*` tests fail with the missing-fixture message; `cargo build --example import_bench` ok with and without `test-support`. Five mutations turned tests red and were reverted (Step 8).

Measured facts the tests pre-register (outlook.pst through this reader): `walk_ids` = `[(1, 2097188, "Inbox"), (2, 2097220, "Recovered_Group2")]`, 22 folders, `items_total` 2. Both items are the same IPM.Note "Welcome to Microsoft Office Outlook 2003" (HTML body, 1 recipient, 11 attachments `1.jpg` … `12.jpg` without `11.jpg`, `image/jpeg`, method 1, no content id, no Message-ID). So one import = 1 message + 2 occurrences, outlook.pst + its byte copy = 1 message + 4 occurrences (the Stage 1 bullet `byte_copy_dedupes_and_removal` said 2; the outline and this step use 4).

Producer facts honored: `pff::Item::string` uses flag 0 (foundation fix; with flag 0x01 every string read fails "unsupported flags: 0x01"). The inflight marker and every data-root file use `Db::data_root()` at use time (`Inner::inflight()` = `paths.with_data_root(db.data_root()?).inflight_marker()`; the writer gets `db.data_root()?`), not the `Paths` given to `ImportManager::new`; the three `poison_marker_*` tests run with a different `Paths` data root to prove it.

Poison-item rule (spec Part 02 §6.3 as updated by arch-system, lead ruling): the marker is `{ job_id, seq, folder_path, pst_item_id, attempts }`. At job start the queue loads the marker as the pending record only if its `job_id` is this job. Before the reader touches item `seq`: no pending record for `seq` → marker with `attempts = 1`; pending `attempts == 1` → marker with `attempts = 2`, then retry once; pending `attempts >= 2` → no libpff call, the item becomes `Unreadable{Parse, "parser crashed on this item"}` with the pending folder path and PST item id (one `item_skipped` row, the cursor advances). The marker is written before the first libpff call on the item (`pst_item_id` 0) and again with the PST item id after `identifier()` (same attempt), so a crash inside `sub_message` / `identifier` also counts. The marker is deleted at job end (step 9), on the shutdown that leaves the job `queued`, and before the re-run after a DB pause (the reader stopped cleanly between items).

Changes against the first Stage 1 draft (all now reflected in the Stage 1 block above):
1. `ReaderParams.poison_seq: Option<i64>` is replaced by `pub pending: Option<InflightRecord>` and `pub job_id: i64` is added; `InflightRecord` drops `pst_id` and gains `pub attempts: u32` (spec §6.3 field list).
2. `ImportTuning::hold_reader_after` is seq-based and one-shot: in the first job attempt of an `ImportManager` the reader blocks after sending the item whose `seq` equals it, until the job is cancelled; later attempts (re-run after a DB pause, resumed job) never block, so a resumed run cannot hang.
3. `job_events` also sends `(pst_id, Running)` when the `crash_after_batches` seam drops a job; the worker task ends with it.
4. Test-support `ImportManager::enqueue_synthetic(path, folders, items) -> Result<i64, AppError>` (returns after queuing); `import_synthetic` uses it. Synthetic items are registered per source path in a process-global map, so a resumed or recovered job (also on a fresh `ImportManager`) reads them again; identity = the file's size + mtime when `path` exists.
5. The inflight marker is also deleted on shutdown (`queued`, spec §6.3 rule 4) and before the re-run after a DB pause; Stage 1 "skip step 9" would leave it. `poison_item_is_skipped` is replaced by the four §6.3 fault-injection tests `poison_marker_attempts_1_retries_item`, `poison_marker_attempts_2_skips_item`, `poison_marker_of_other_job_is_ignored`, `retry_rewrites_marker_before_reading`.
6. `ByteBudget::wait_cancelled(&self, cancel)` (pub(crate), hold seam) and `ByteBudget::used() -> u64` (pub) are added.
7. `index_policy_first_import_drops_and_rebuilds` observes "during the job" while the reader is held (deterministic), then cancel + resume reach `Completed`; it also asserts that all six index OIDs changed. `index_policy_rebuilt_after_cancel_and_disk_failure` proves the drop in the failed job by changed OIDs. An `on_batch` hook cannot run an async query without blocking the writer.
8. `pst_list_status_derivation` checks the §2.1 rows one at a time (`import_jobs_one_running` allows one running / cancelling job per library), then the list order on a second set. `recover_after_crash_states` sets `cancelling` and `running` in two rounds for the same reason.
9. The hostile pattern check is a copy of the R1-05 test helper (`hostile_violations`) inside `tests/import_pipeline.rs`; the R1-05 one is `pub(crate)` in a `#[cfg(test)]` module.
10. `import_bench`'s last stdout line is `import_bench messages_per_sec=<f64> peak_rss_bytes=<u64>` (lead ruling, consumed by the R2-10 perf test); the example uses `sysinfo` (already a core dependency, Part 01).
From R2-10: `WriterLimits` derives `Debug, Clone` (`ImportTuning` derives `Clone`); the job runner uses `writer::stage_db` for error rows. Only real-PST jobs write markers (the synthetic reader has no libpff calls).
Known limits (accepted by the leader): (a) the `hold_reader_after` test seam acts only in the first job attempt of an `ImportManager`; (b) after an mtime change resets the cursor, items stored before the reset are read again and count as `duplicates`, so the PST's `new_message_count` (= `stored`) is lower than the messages it added; `message_count`, `messages` and `message_occurrences` stay correct (asserted in `mtime_change_resets_cursor`).

- [ ] **Step 1: Write the failing integration tests.** Create `src-tauri/crates/outlooker-core/tests/import_pipeline.rs`:

```rust
//! R2-09 Import Manager integration tests (spec Part 02 §4, §6.1–§6.3; Part 03 §7; Part 04 §2.1, §5, §6.4, §6.5, §12.2).
//! Every test pre-registers its expected values as constants before it runs the import.
#![cfg(feature = "test-support")]
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use outlooker_core::db::indexes::{SEARCH_INDEXES, missing_or_invalid};
use outlooker_core::dto::app::AppPhase;
use outlooker_core::dto::error::AppError;
use outlooker_core::dto::import::{EnqueueResult, ImportStatus};
use outlooker_core::gate::Gate;
use outlooker_core::importer::inflight::{Inflight, InflightRecord};
use outlooker_core::importer::pst_reader::walk_ids;
use outlooker_core::importer::types::{FolderIn, RawAttachment, RawContent, RawItem, RawMail, RawRecipient};
use outlooker_core::importer::writer::{Checkpoint, WriterLimits};
use outlooker_core::importer::{ImportManager, ImportTuning, RecoveryReport};
use outlooker_core::sink::NullSink;
use outlooker_core::test_support::TestCluster;
use tokio::sync::{broadcast, mpsc, watch};

/// libyal testdata `pst/outlook.pst`, relative to this crate.
const OUTLOOK_PST: &str = "../../../tests/fixtures/external/outlook.pst";
/// Repository root, relative to this crate (Windows-phase fixtures live in `tests/fixtures/pst/`).
#[cfg(feature = "outlook-fixtures")]
const REPO_ROOT: &str = "../../..";
const JOB_TIMEOUT: Duration = Duration::from_secs(120);

// ------------------------------------------------------------------------------------------------ helpers

struct Fx {
    mgr: ImportManager,
    events: broadcast::Receiver<(i64, ImportStatus)>,
    phase: watch::Sender<AppPhase>,
}

fn manager(tc: &TestCluster, tuning: ImportTuning) -> Fx {
    let (phase, rx) = watch::channel(AppPhase::Ready);
    let mgr = ImportManager::new(tc.db.clone(), Gate::new(), tc.paths.clone(), Arc::new(NullSink), rx, tuning);
    let events = mgr.job_events();
    Fx { mgr, events, phase }
}

/// Batches of 5 items, so 20–40 synthetic items give several commits.
fn small_batches() -> ImportTuning {
    ImportTuning { writer: WriterLimits { max_items: 5, ..WriterLimits::default() }, ..ImportTuning::default() }
}

/// `on_batch` hook that reports `batches_committed` on a channel (never blocks the writer).
fn batch_signal(t: &mut ImportTuning) -> mpsc::UnboundedReceiver<i64> {
    let (tx, rx) = mpsc::unbounded_channel();
    t.on_batch = Some(Arc::new(move |n| {
        let _ = tx.send(n);
    }));
    rx
}

async fn wait_batch(rx: &mut mpsc::UnboundedReceiver<i64>, n: i64) {
    loop {
        let got = tokio::time::timeout(JOB_TIMEOUT, rx.recv()).await.expect("batch in time").expect("hook alive");
        if got >= n {
            return;
        }
    }
}

/// Terminal job outcome of each PST in `ids` (events of other PSTs are ignored).
async fn wait_jobs(events: &mut broadcast::Receiver<(i64, ImportStatus)>, ids: &[i64]) -> Vec<ImportStatus> {
    let mut got: BTreeMap<i64, ImportStatus> = BTreeMap::new();
    while ids.iter().any(|id| !got.contains_key(id)) {
        let (id, status) = tokio::time::timeout(JOB_TIMEOUT, events.recv()).await.expect("job ended in time").expect("job events open");
        if ids.contains(&id) {
            got.insert(id, status);
        }
    }
    ids.iter().map(|id| got[id]).collect()
}

async fn wait_job(events: &mut broadcast::Receiver<(i64, ImportStatus)>, pst_id: i64) -> ImportStatus {
    wait_jobs(events, &[pst_id]).await[0]
}

fn outlook_pst() -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join(OUTLOOK_PST);
    std::fs::canonicalize(&p).unwrap_or_else(|e| panic!("missing {}: {e}", p.display()))
}

fn s(p: &Path) -> String {
    p.to_str().expect("utf-8 path").to_owned()
}

/// Source file for synthetic items: 20 bytes starting with the PST magic, so enqueue identity (size + mtime) and
/// the re-validation of `resume` work as for a real PST.
fn synthetic_path(tc: &TestCluster, name: &str) -> String {
    let p = tc.dir.path().join(name);
    std::fs::write(&p, b"!BDN0000000000000000").unwrap();
    s(&p)
}

async fn count(tc: &TestCluster, sql: &str) -> i64 {
    tc.admin().await.query_one(sql, &[]).await.unwrap().get(0)
}

#[derive(Debug, PartialEq, Eq)]
struct Snapshot {
    keys: BTreeSet<Vec<u8>>,
    messages: i64,
    occurrences: i64,
    recipients: i64,
    attachments: i64,
}

async fn snapshot(tc: &TestCluster) -> Snapshot {
    let a = tc.admin().await;
    let keys = a.query("SELECT dedupe_key FROM messages", &[]).await.unwrap().iter().map(|r| r.get::<_, Vec<u8>>(0)).collect();
    let r = a
        .query_one(
            "SELECT (SELECT count(*) FROM messages), (SELECT count(*) FROM message_occurrences), \
                    (SELECT count(*) FROM message_recipients), (SELECT count(*) FROM attachments)",
            &[],
        )
        .await
        .unwrap();
    Snapshot { keys, messages: r.get(0), occurrences: r.get(1), recipients: r.get(2), attachments: r.get(3) }
}

/// The six search index OIDs by name (absent → not in the map).
async fn index_oids(tc: &TestCluster) -> BTreeMap<String, u32> {
    let names: Vec<&str> = SEARCH_INDEXES.iter().map(|i| i.name).collect();
    let rows = tc.admin().await.query("SELECT relname::text, oid FROM pg_class WHERE relname = ANY($1)", &[&names]).await.unwrap();
    rows.iter().map(|r| (r.get(0), r.get(1))).collect()
}

async fn missing(tc: &TestCluster) -> Vec<&'static str> {
    missing_or_invalid(&tc.admin().await).await.unwrap()
}

fn all_six() -> Vec<&'static str> {
    SEARCH_INDEXES.iter().map(|i| i.name).collect()
}

fn folders() -> Vec<FolderIn> {
    vec![
        FolderIn { pst_folder_id: 1, parent_pst_folder_id: None, name: "Inbox".into(), path: "Inbox".into() },
        FolderIn { pst_folder_id: 2, parent_pst_folder_id: Some(1), name: "Projects".into(), path: "Inbox/Projects".into() },
    ]
}

fn item(seq: i64, folder: i64, content: RawContent) -> RawItem {
    let folder_path = if folder == 1 { "Inbox" } else { "Inbox/Projects" };
    RawItem { seq, pst_item_id: 10_000 + seq, pst_folder_id: folder, folder_path: folder_path.into(), content }
}

fn mail(m: RawMail) -> RawContent {
    RawContent::Mail(Box::new(m))
}

/// `n` items in two folders: every 10th is a contact (non-mail), the rest distinct mails with a Message-ID,
/// one recipient and one attachment.
fn corpus(n: i64) -> Vec<RawItem> {
    corpus_tagged(n, "m")
}

fn corpus_tagged(n: i64, tag: &str) -> Vec<RawItem> {
    (1..=n)
        .map(|i| {
            let content = if i % 10 == 0 {
                RawContent::NonMail
            } else {
                mail(RawMail {
                    subject: Some(format!("Mail {tag}{i}")),
                    internet_message_id: Some(format!("<{tag}{i}@synthetic.example>")),
                    sender_smtp_address: Some(format!("sender{}@synthetic.example", i % 3)),
                    html_body: Some(format!("<p>Body {tag}{i} 報價單</p>").into_bytes()),
                    recipients: vec![RawRecipient { smtp_address: Some("to@synthetic.example".into()), recipient_type: Some(1), ..Default::default() }],
                    attachments: vec![RawAttachment {
                        index: 0,
                        long_filename: Some(format!("a{i}.pdf")),
                        data_size: Some(100),
                        attach_method: Some(1),
                        ..Default::default()
                    }],
                    ..Default::default()
                })
            };
            item(i, 1 + i % 2, content)
        })
        .collect()
}

/// The library one uninterrupted import of `items` produces (own TestCluster).
async fn reference(items: Vec<RawItem>) -> Snapshot {
    let tc = TestCluster::start().await;
    let fx = manager(&tc, small_batches());
    let pst_id = fx.mgr.import_synthetic(&synthetic_path(&tc, "ref.pst"), folders(), items).await.unwrap();
    let list = fx.mgr.list().await.unwrap();
    assert_eq!(list.iter().find(|p| p.id == pst_id).unwrap().status, ImportStatus::Completed);
    snapshot(&tc).await
}

fn png(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_fn(w, h, |x, y| image::Rgba([(x % 256) as u8, (y % 256) as u8, 90, 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

/// The R1-05 sanitizer acceptance patterns (spec Part 04 §12.2 row Sanitizer), matched without a regex crate.
fn hostile_violations(out: &str) -> Vec<String> {
    let lower = out.to_ascii_lowercase();
    let mut found: Vec<String> =
        ["<script", "javascript:", "url(", "<iframe", "<form", "position", "src=\"http"].iter().filter(|p| lower.contains(*p)).map(|p| p.to_string()).collect();
    let b = lower.as_bytes();
    for i in 0..b.len().saturating_sub(2) {
        if b[i] == b'o' && b[i + 1] == b'n' && (i == 0 || !b[i - 1].is_ascii_alphanumeric()) {
            let mut j = i + 2;
            while j < b.len() && b[j].is_ascii_lowercase() {
                j += 1;
            }
            if j > i + 2 && j < b.len() && b[j] == b'=' {
                found.push(lower[i..=j].to_string());
            }
        }
    }
    found
}

// ------------------------------------------------------------------------------------------------ outlook.pst

#[tokio::test(flavor = "multi_thread")]
async fn outlook_pst_import() {
    // pre-registered: measured with this reader during planning (walk order Part 04 §5.2)
    const WALK: [(i64, i64, &str); 2] = [(1, 2_097_188, "Inbox"), (2, 2_097_220, "Recovered_Group2")];
    const MESSAGES: i64 = 1;
    const OCCURRENCES: i64 = 2;
    const ATTACHMENT_NAMES: [&str; 11] = ["1.jpg", "2.jpg", "3.jpg", "4.jpg", "5.jpg", "6.jpg", "7.jpg", "8.jpg", "9.jpg", "10.jpg", "12.jpg"];
    let pst = outlook_pst();
    let ids = walk_ids(&pst).unwrap();
    assert_eq!(ids, walk_ids(&pst).unwrap(), "scan_is_deterministic: two walks give identical (seq, pst_item_id) lists");
    assert_eq!(ids.iter().map(|(seq, id, p)| (*seq, *id, p.as_str())).collect::<Vec<_>>(), WALK);

    let tc = TestCluster::start().await;
    let mut fx = manager(&tc, ImportTuning::default());
    let res = fx.mgr.enqueue(vec![s(&pst)]).await.unwrap();
    let EnqueueResult::Queued { pst: src } = &res[0] else { panic!("{res:?}") };
    assert_eq!(src.status, ImportStatus::Queued);
    fx.mgr.start_worker();
    assert_eq!(wait_job(&mut fx.events, src.id).await, ImportStatus::Completed);

    let a = tc.admin().await;
    let paths: Vec<String> = a.query("SELECT path FROM folders", &[]).await.unwrap().iter().map(|r| r.get(0)).collect();
    assert!(paths.iter().any(|p| p == "Inbox") && paths.iter().any(|p| p == "Sent Items"), "{paths:?}");
    assert_eq!(count(&tc, "SELECT count(*) FROM messages").await, MESSAGES);
    assert_eq!(count(&tc, "SELECT count(*) FROM message_occurrences").await, OCCURRENCES);
    let m = a.query_one("SELECT body_source, has_attachments, body_html IS NOT NULL FROM messages", &[]).await.unwrap();
    assert_eq!((m.get::<_, &str>(0), m.get::<_, bool>(1), m.get::<_, bool>(2)), ("html", true, true));
    let att = a.query("SELECT file_name, mime_type, attach_method FROM attachments ORDER BY attachment_index", &[]).await.unwrap();
    assert_eq!(att.iter().map(|r| r.get::<_, String>(0)).collect::<Vec<_>>(), ATTACHMENT_NAMES);
    assert!(att.iter().all(|r| r.get::<_, &str>(1) == "image/jpeg" && r.get::<_, i32>(2) == 1));
    let listed = fx.mgr.list().await.unwrap();
    assert_eq!((listed[0].status, listed[0].message_count, listed[0].new_message_count), (ImportStatus::Completed, 2, 1));
}

#[tokio::test(flavor = "multi_thread")]
async fn byte_copy_dedupes_and_removal() {
    // pre-registered: the Welcome mail is stored twice in outlook.pst (Inbox + Recovered_Group2) → 2 occurrences per copy
    const MESSAGES: i64 = 1;
    const OCCURRENCES_BOTH: i64 = 4;
    const OCCURRENCES_ONE: i64 = 2;
    let dir = tempfile::tempdir().unwrap();
    let copy = dir.path().join("copy.pst");
    std::fs::copy(outlook_pst(), &copy).unwrap();
    let tc = TestCluster::start().await;
    let mut fx = manager(&tc, ImportTuning::default());
    let res = fx.mgr.enqueue(vec![s(&outlook_pst()), s(&copy)]).await.unwrap();
    let ids: Vec<i64> = res.iter().map(|r| if let EnqueueResult::Queued { pst } = r { pst.id } else { panic!("{r:?}") }).collect();
    fx.mgr.start_worker();
    assert_eq!(wait_jobs(&mut fx.events, &ids).await, [ImportStatus::Completed, ImportStatus::Completed]);
    assert_eq!(count(&tc, "SELECT count(*) FROM messages").await, MESSAGES);
    assert_eq!(count(&tc, "SELECT count(*) FROM message_occurrences").await, OCCURRENCES_BOTH);
    assert_eq!(count(&tc, "SELECT count(*) FROM messages WHERE dedupe_kind = 'content_hash'").await, 1, "no Message-ID → fallback hash");

    fx.mgr.remove(ids[0]).await.unwrap();
    assert_eq!(wait_job(&mut fx.events, ids[0]).await, ImportStatus::Removing);
    assert_eq!(count(&tc, "SELECT count(*) FROM messages").await, MESSAGES, "removing one source keeps the shared message");
    assert_eq!(count(&tc, "SELECT count(*) FROM message_occurrences").await, OCCURRENCES_ONE);
    assert!(count(&tc, "SELECT count(*) FROM addresses").await > 0);

    fx.mgr.remove(ids[1]).await.unwrap();
    assert_eq!(wait_job(&mut fx.events, ids[1]).await, ImportStatus::Removing);
    for table in ["pst_sources", "messages", "message_occurrences", "attachments", "addresses", "message_recipients"] {
        assert_eq!(count(&tc, &format!("SELECT count(*) FROM {table}")).await, 0, "{table} after removing both");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn corrupt_copy_outcome() {
    // fixture identity: the offsets below are valid only for this file (measured with a PST block probe during planning)
    const ORIGINAL_BLAKE3: &str = "1a16e61df9833c3cbe1bf74d53a1ef3abacfcbb33c8f58db8ac86d6c0f6b1eb5";
    // the two data blocks of the IPM.Note "Welcome" property contexts (NIDs 0x200024 / 0x200044)
    const ZEROED: [(u64, usize); 2] = [(173_568, 1_454), (138_240, 1_454)];
    // pre-registered outcome; any other observed outcome is reported, never re-tuned
    const EXPECT_STATUS: ImportStatus = ImportStatus::Completed;
    const EXPECT_MIN_SKIPPED_PARSE: i64 = 1;
    const EXPECT_MESSAGES: i64 = 0;
    let original = std::fs::read(outlook_pst()).unwrap();
    assert_eq!(blake3::hash(&original).to_hex().as_str(), ORIGINAL_BLAKE3, "outlook.pst changed: re-measure the offsets");
    let dir = tempfile::tempdir().unwrap();
    let corrupt = dir.path().join("corrupt.pst");
    std::fs::write(&corrupt, &original).unwrap();
    let mut f = std::fs::OpenOptions::new().write(true).open(&corrupt).unwrap();
    for (offset, len) in ZEROED {
        f.seek(SeekFrom::Start(offset)).unwrap();
        f.write_all(&vec![0u8; len]).unwrap();
    }
    drop(f);

    let tc = TestCluster::start().await;
    let mut fx = manager(&tc, ImportTuning::default());
    let res = fx.mgr.enqueue(vec![s(&corrupt)]).await.unwrap();
    let EnqueueResult::Queued { pst } = &res[0] else { panic!("{res:?}") };
    fx.mgr.start_worker();
    assert_eq!(wait_job(&mut fx.events, pst.id).await, EXPECT_STATUS);
    let skipped = count(&tc, "SELECT count(*) FROM import_errors WHERE severity = 'item_skipped' AND stage = 'parse'").await;
    assert!(skipped >= EXPECT_MIN_SKIPPED_PARSE, "item_skipped/parse rows: {skipped}");
    assert_eq!(count(&tc, "SELECT count(*) FROM messages").await, EXPECT_MESSAGES);
}

// ------------------------------------------------------------------------------------------------ synthetic items

#[tokio::test(flavor = "multi_thread")]
async fn synthetic_cjk() {
    const EXPECTED: &str = "報價單";
    let (big5, _, unmappable) = encoding_rs::BIG5.encode("<html><body><p>報價單 Big5 版本</p></body></html>");
    assert!(!unmappable);
    let items = vec![
        item(1, 1, mail(RawMail { subject: Some("Big5".into()), internet_codepage: Some(950), html_body: Some(big5.into_owned()), ..Default::default() })),
        item(
            2,
            1,
            mail(RawMail {
                subject: Some("UTF-8".into()),
                internet_codepage: Some(65001),
                html_body: Some("<html><body><p>報價單 UTF-8 版本</p></body></html>".as_bytes().to_vec()),
                ..Default::default()
            }),
        ),
    ];
    let tc = TestCluster::start().await;
    let fx = manager(&tc, ImportTuning::default());
    fx.mgr.import_synthetic(&synthetic_path(&tc, "cjk.pst"), folders(), items).await.unwrap();
    let rows = tc.admin().await.query("SELECT subject, body_text FROM messages ORDER BY subject", &[]).await.unwrap();
    assert_eq!(rows.len(), 2);
    for r in &rows {
        let (subject, text): (String, String) = (r.get(0), r.get(1));
        assert!(text.contains(EXPECTED), "{subject}: {text:?}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn synthetic_inline_images() {
    const MAILS: i64 = 10;
    const BLOBS: i64 = 1;
    let logo = png(40, 20);
    let items: Vec<RawItem> = (1..=MAILS)
        .map(|i| {
            item(
                i,
                1,
                mail(RawMail {
                    subject: Some(format!("Logo {i}")),
                    internet_message_id: Some(format!("<logo{i}@synthetic.example>")),
                    html_body: Some(format!(r#"<p>Mail {i} <img src="cid:logo@x" alt="logo"></p>"#).into_bytes()),
                    attachments: vec![RawAttachment {
                        index: 0,
                        content_id: Some("<logo@x>".into()),
                        long_filename: Some("logo.png".into()),
                        mime_tag: Some("image/png".into()),
                        data: Some(logo.clone()),
                        data_size: Some(logo.len() as i64),
                        attach_method: Some(1),
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
            )
        })
        .collect();
    let tc = TestCluster::start().await;
    let fx = manager(&tc, ImportTuning::default());
    fx.mgr.import_synthetic(&synthetic_path(&tc, "inline.pst"), folders(), items).await.unwrap();
    assert_eq!(count(&tc, "SELECT count(*) FROM messages").await, MAILS);
    assert_eq!(count(&tc, "SELECT count(*) FROM image_blobs").await, BLOBS, "the repeated logo is stored once");
    assert_eq!(count(&tc, "SELECT count(*) FROM inline_images").await, MAILS);
    assert_eq!(count(&tc, "SELECT count(*) FROM messages WHERE body_html LIKE '%outlooker-inline:0%'").await, MAILS);
    // every outlooker-inline:n reference has its inline_images row
    let dangling = count(
        &tc,
        "SELECT count(*) FROM messages m, regexp_matches(m.body_html, 'outlooker-inline:([0-9]+)', 'g') x \
          WHERE NOT EXISTS (SELECT 1 FROM inline_images i WHERE i.message_id = m.id AND i.ordinal = x[1]::int)",
    )
    .await;
    assert_eq!(dangling, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn synthetic_attachments() {
    let long_name = format!("{}.txt", "n".repeat(1_100));
    let atts = vec![
        RawAttachment { index: 0, attach_method: Some(5), embedded_subject: Some("Re: plan".into()), ..Default::default() },
        RawAttachment { index: 1, long_filename: Some("report.pdf".into()), data_size: Some(1_234), attach_method: Some(1), ..Default::default() },
        RawAttachment { index: 2, long_filename: Some(long_name), data_size: Some(10), attach_method: Some(1), ..Default::default() },
    ];
    // pre-registered (Part 04 §5.8): (file_name, chars, mime_type)
    let expected: Vec<(String, i32, String)> = vec![
        ("Re: plan.msg".into(), 12, "application/vnd.ms-outlook".into()),
        ("report.pdf".into(), 10, "application/pdf".into()),
        // cut to 1,024 chars before the MIME guess: the extension is gone
        ("n".repeat(1_024), 1_024, "application/octet-stream".into()),
    ];
    let tc = TestCluster::start().await;
    let fx = manager(&tc, ImportTuning::default());
    let items = vec![item(1, 1, mail(RawMail { subject: Some("Attachments".into()), attachments: atts, ..Default::default() }))];
    fx.mgr.import_synthetic(&synthetic_path(&tc, "attach.pst"), folders(), items).await.unwrap();
    let rows = tc
        .admin()
        .await
        .query("SELECT file_name, length(file_name), mime_type FROM attachments ORDER BY attachment_index", &[])
        .await
        .unwrap();
    let got: Vec<(String, i32, String)> = rows.iter().map(|r| (r.get(0), r.get(1), r.get(2))).collect();
    assert_eq!(got, expected);
}

#[tokio::test(flavor = "multi_thread")]
async fn synthetic_hostile() {
    let hostile = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/hostile.html")).unwrap();
    let tc = TestCluster::start().await;
    let fx = manager(&tc, ImportTuning::default());
    let items = vec![item(1, 1, mail(RawMail { subject: Some("Hostile".into()), html_body: Some(hostile), ..Default::default() }))];
    fx.mgr.import_synthetic(&synthetic_path(&tc, "hostile.pst"), folders(), items).await.unwrap();
    let html: String = tc.admin().await.query_one("SELECT body_html FROM messages", &[]).await.unwrap().get(0);
    assert_eq!(hostile_violations(&html), Vec::<String>::new(), "stored body_html: {html}");
}

#[tokio::test(flavor = "multi_thread")]
async fn synthetic_message_id_dedupe() {
    // A: shared 1–3 + own a4 + shared 1 copied into the second folder; B: shared 1–3 + own b4
    const MESSAGES: i64 = 5;
    // stored message_id is the normalized form (angle brackets removed, Part 04 §4.1)
    const OCCURRENCES_PER_SHARED: [(&str, i64); 3] = [("shared1@x", 3), ("shared2@x", 2), ("shared3@x", 2)];
    let m = |id: &str, body: &str| mail(RawMail { internet_message_id: Some(id.into()), plain_body: Some(body.into()), ..Default::default() });
    let a = vec![
        item(1, 1, m("<shared1@x>", "a one")),
        item(2, 1, m("<shared2@x>", "a two")),
        item(3, 1, m("<shared3@x>", "a three")),
        item(4, 1, m("<a4@x>", "a four")),
        item(5, 2, m("<shared1@x>", "a one, copy in Projects")),
    ];
    let b = vec![
        item(1, 1, m("<shared1@x>", "b one")),
        item(2, 1, m("<shared2@x>", "b two")),
        item(3, 2, m("<shared3@x>", "b three")),
        item(4, 2, m("<b4@x>", "b four")),
    ];
    let tc = TestCluster::start().await;
    let fx = manager(&tc, ImportTuning::default());
    let pa = fx.mgr.import_synthetic(&synthetic_path(&tc, "a.pst"), folders(), a).await.unwrap();
    let pb = fx.mgr.import_synthetic(&synthetic_path(&tc, "b.pst"), folders(), b).await.unwrap();
    assert_eq!(count(&tc, "SELECT count(*) FROM messages").await, MESSAGES);
    let adm = tc.admin().await;
    for (id, occ) in OCCURRENCES_PER_SHARED {
        let r = adm
            .query_one(
                "SELECT count(DISTINCT m.id), count(*), count(*) FILTER (WHERE o.pst_id = $2), count(*) FILTER (WHERE o.pst_id = $3) \
                   FROM messages m JOIN message_occurrences o ON o.message_id = m.id WHERE m.message_id = $1",
                &[&id, &pa, &pb],
            )
            .await
            .unwrap();
        let (msgs, total, in_a, in_b): (i64, i64, i64, i64) = (r.get(0), r.get(1), r.get(2), r.get(3));
        assert_eq!((msgs, total), (1, occ), "{id}");
        assert_eq!((in_a, in_b), (occ - 1, 1), "{id}: one occurrence per source, the folder copy adds one in A");
    }
}

// ------------------------------------------------------------------------------------------------ index policy, vacuum

#[tokio::test(flavor = "multi_thread")]
async fn index_policy_first_import_drops_and_rebuilds() {
    const ITEMS: i64 = 20;
    const HOLD_AT: i64 = 10;
    let tc = TestCluster::start().await;
    let before = index_oids(&tc).await;
    assert_eq!(before.len(), 6);
    let mut tuning = small_batches();
    tuning.hold_reader_after = Some(HOLD_AT);
    let mut batches = batch_signal(&mut tuning);
    let mut fx = manager(&tc, tuning);
    let pst_id = fx.mgr.enqueue_synthetic(&synthetic_path(&tc, "first.pst"), folders(), corpus(ITEMS)).await.unwrap();
    fx.mgr.start_worker();
    wait_batch(&mut batches, 1).await;
    // during the job (reader held after item 10, job running): all six dropped
    assert_eq!(missing(&tc).await, all_six());
    fx.mgr.cancel(pst_id).await.unwrap();
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Cancelled);
    fx.mgr.resume(pst_id).await.unwrap();
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Completed);
    assert_eq!(missing(&tc).await, Vec::<&str>::new(), "after Completed all six exist and are valid");
    let after = index_oids(&tc).await;
    assert!(SEARCH_INDEXES.iter().all(|i| before[i.name] != after[i.name]), "drop + create gives new OIDs: {before:?} {after:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn index_policy_rebuilt_after_cancel_and_disk_failure() {
    let tc = TestCluster::start().await;
    // (a) cancelled job
    let mut tuning = small_batches();
    tuning.hold_reader_after = Some(10);
    let mut batches = batch_signal(&mut tuning);
    let mut fx = manager(&tc, tuning);
    let p1 = fx.mgr.enqueue_synthetic(&synthetic_path(&tc, "cancelled.pst"), folders(), corpus(20)).await.unwrap();
    fx.mgr.start_worker();
    wait_batch(&mut batches, 1).await;
    assert_eq!(missing(&tc).await, all_six());
    fx.mgr.cancel(p1).await.unwrap();
    assert_eq!(wait_job(&mut fx.events, p1).await, ImportStatus::Cancelled);
    assert_eq!(missing(&tc).await, Vec::<&str>::new(), "rebuilt after the cancelled job");
    fx.mgr.shutdown(JOB_TIMEOUT).await.unwrap();

    // (b) job failed by the free-space guard
    let before = index_oids(&tc).await;
    let tuning = ImportTuning { writer: WriterLimits { min_free_bytes: u64::MAX, ..WriterLimits::default() }, ..ImportTuning::default() };
    let mut fx = manager(&tc, tuning);
    let p2 = fx.mgr.enqueue_synthetic(&synthetic_path(&tc, "disk.pst"), folders(), corpus_tagged(20, "d")).await.unwrap();
    fx.mgr.start_worker();
    assert_eq!(wait_job(&mut fx.events, p2).await, ImportStatus::Failed);
    let r = tc.admin().await.query_one("SELECT stage, severity, message FROM import_errors WHERE pst_id = $1", &[&p2]).await.unwrap();
    assert_eq!((r.get::<_, &str>(0), r.get::<_, &str>(1), r.get::<_, &str>(2)), ("store", "job_failed", "disk space low"));
    assert_eq!(missing(&tc).await, Vec::<&str>::new(), "rebuilt after the failed job");
    let after = index_oids(&tc).await;
    assert!(SEARCH_INDEXES.iter().all(|i| before[i.name] != after[i.name]), "the failed job dropped and rebuilt them");
}

#[tokio::test(flavor = "multi_thread")]
async fn index_policy_incremental_keeps_indexes() {
    const FIRST: i64 = 20; // 18 messages
    const SECOND: i64 = 4; // 2 * 4 < 18 → incremental
    let tc = TestCluster::start().await;
    let fx = manager(&tc, small_batches());
    fx.mgr.import_synthetic(&synthetic_path(&tc, "big.pst"), folders(), corpus(FIRST)).await.unwrap();
    let before = index_oids(&tc).await;
    assert_eq!(before.len(), 6);
    fx.mgr.import_synthetic(&synthetic_path(&tc, "small.pst"), folders(), corpus_tagged(SECOND, "s")).await.unwrap();
    assert_eq!(count(&tc, "SELECT count(*) FROM messages").await, 18 + 4);
    assert_eq!(index_oids(&tc).await, before, "no DROP INDEX: OIDs unchanged");
}

#[tokio::test(flavor = "multi_thread")]
async fn after_job_vacuum_ran() {
    const SQL: &str = "SELECT count(*) FROM pg_stat_user_tables WHERE relname = 'messages' AND coalesce(last_vacuum, last_autovacuum) IS NOT NULL";
    let tc = TestCluster::start().await;
    assert_eq!(count(&tc, SQL).await, 0, "fresh database: never vacuumed");
    let fx = manager(&tc, ImportTuning::default());
    fx.mgr.import_synthetic(&synthetic_path(&tc, "v.pst"), folders(), corpus(3)).await.unwrap();
    assert_eq!(count(&tc, SQL).await, 1);
}

// ------------------------------------------------------------------------------------------------ resume, crash, poison

#[tokio::test(flavor = "multi_thread")]
async fn resume_after_cancel_equals_uninterrupted() {
    const ITEMS: i64 = 40;
    let want = reference(corpus(ITEMS)).await;
    let tc = TestCluster::start().await;
    let mut tuning = small_batches();
    tuning.hold_reader_after = Some(20);
    let mut batches = batch_signal(&mut tuning);
    let mut fx = manager(&tc, tuning);
    let pst_id = fx.mgr.enqueue_synthetic(&synthetic_path(&tc, "c.pst"), folders(), corpus(ITEMS)).await.unwrap();
    fx.mgr.start_worker();
    wait_batch(&mut batches, 1).await;
    fx.mgr.cancel(pst_id).await.unwrap();
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Cancelled);
    let cursor = count(&tc, "SELECT cursor_seq FROM import_checkpoints").await;
    assert!((5..ITEMS).contains(&cursor), "cancelled mid-job: cursor {cursor}");
    fx.mgr.resume(pst_id).await.unwrap();
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Completed);
    assert_eq!(snapshot(&tc).await, want);
}

#[tokio::test(flavor = "multi_thread")]
async fn resume_after_kill_equals_uninterrupted() {
    const ITEMS: i64 = 40;
    let want = reference(corpus(ITEMS)).await;
    let tc = TestCluster::start().await;
    let path = synthetic_path(&tc, "k.pst");
    let mut tuning = small_batches();
    tuning.crash_after_batches = Some(1);
    tuning.hold_reader_after = Some(20); // the input cannot end before the crash seam fires
    let mut fx = manager(&tc, tuning);
    let pst_id = fx.mgr.enqueue_synthetic(&path, folders(), corpus(ITEMS)).await.unwrap();
    fx.mgr.start_worker();
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Running, "crash seam: job dropped like a process kill");
    assert_eq!(count(&tc, "SELECT count(*) FROM import_jobs WHERE state = 'running'").await, 1);
    drop(fx);
    // next process start
    let mut fx = manager(&tc, small_batches());
    assert_eq!(fx.mgr.recover_after_crash().await.unwrap(), RecoveryReport { requeued: vec![pst_id], cancelled: vec![] });
    fx.mgr.start_worker();
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Completed);
    assert_eq!(snapshot(&tc).await, want);
}

#[tokio::test(flavor = "multi_thread")]
async fn mtime_change_resets_cursor() {
    const ITEMS: i64 = 20;
    const MESSAGES: i64 = 18;
    let tc = TestCluster::start().await;
    let path = synthetic_path(&tc, "m.pst");
    let mut tuning = small_batches();
    tuning.hold_reader_after = Some(10);
    let mut batches = batch_signal(&mut tuning);
    let mut fx = manager(&tc, tuning);
    let pst_id = fx.mgr.enqueue_synthetic(&path, folders(), corpus(ITEMS)).await.unwrap();
    fx.mgr.start_worker();
    wait_batch(&mut batches, 1).await;
    fx.mgr.cancel(pst_id).await.unwrap();
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Cancelled);
    let before_resume = count(&tc, "SELECT count(*) FROM messages").await;
    assert!(before_resume > 0);
    // touch: mtime + 60 s
    let f = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
    f.set_modified(SystemTime::now() + Duration::from_secs(60)).unwrap();
    drop(f);
    fx.mgr.resume(pst_id).await.unwrap();
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Completed);
    let r = tc
        .admin()
        .await
        .query_one("SELECT cursor_seq, stored, duplicates, skipped_non_mail FROM import_checkpoints WHERE pst_id = $1", &[&pst_id])
        .await
        .unwrap();
    let (cursor, stored, dups, non_mail): (i64, i64, i64, i64) = (r.get(0), r.get(1), r.get(2), r.get(3));
    assert_eq!(dups, before_resume, "cursor restarted at 0: every message stored before the touch was read again");
    assert_eq!((cursor, stored + dups, non_mail), (ITEMS, MESSAGES, 2));
    assert_eq!(count(&tc, "SELECT count(*) FROM messages").await, MESSAGES);
    assert_eq!(count(&tc, "SELECT count(*) FROM message_occurrences").await, MESSAGES);
}

type ErrorRow = (String, String, Option<i64>, Option<String>, String);

struct MarkerRun {
    errors: Vec<ErrorRow>,
    messages: i64,
    occurrences: i64,
}

/// Imports outlook.pst with an inflight marker written before the job starts, as a dead process leaves it
/// (Part 02 §6.3). `own_job`: the marker names this job, else another job id. The manager runs with a `Paths`
/// data root that differs from `Db::data_root()`: the marker must be read from and deleted in `Db::data_root()`.
/// `stale_checkpoint`: a checkpoint with another file identity exists, so the job resets its cursor.
async fn run_with_marker(own_job: bool, seq: i64, pst_item_id: i64, folder: &str, attempts: u32, stale_checkpoint: bool) -> MarkerRun {
    let tc = TestCluster::start().await;
    let elsewhere = tempfile::tempdir().unwrap();
    let (_phase, rx) = watch::channel(AppPhase::Ready);
    let mgr = ImportManager::new(
        tc.db.clone(),
        Gate::new(),
        tc.paths.with_data_root(elsewhere.path().to_path_buf()),
        Arc::new(NullSink),
        rx,
        ImportTuning::default(),
    );
    let mut events = mgr.job_events();
    let res = mgr.enqueue(vec![s(&outlook_pst())]).await.unwrap();
    let EnqueueResult::Queued { pst } = &res[0] else { panic!("{res:?}") };
    let job_id: i64 = tc.admin().await.query_one("SELECT id FROM import_jobs WHERE pst_id = $1", &[&pst.id]).await.unwrap().get(0);
    let marker = Inflight::new(tc.db.data_root().unwrap().join("import-inflight"));
    let marker_job = if own_job { job_id } else { job_id + 1_000 };
    marker.write(&InflightRecord { job_id: marker_job, seq, folder_path: folder.into(), pst_item_id, attempts });
    if stale_checkpoint {
        Checkpoint::begin(&tc.app().await, pst.id, job_id, 1, SystemTime::UNIX_EPOCH).await.unwrap();
    }
    mgr.start_worker();
    assert_eq!(wait_job(&mut events, pst.id).await, ImportStatus::Completed);
    assert_eq!(marker.read(), None, "marker deleted at job end");
    assert!(!elsewhere.path().join("import-inflight").exists(), "nothing written under the Paths data root");
    let rows = tc.admin().await.query("SELECT stage, severity, pst_item_id, folder_path, message FROM import_errors", &[]).await.unwrap();
    MarkerRun {
        errors: rows.iter().map(|r| (r.get(0), r.get(1), r.get(2), r.get(3), r.get(4))).collect(),
        messages: count(&tc, "SELECT count(*) FROM messages").await,
        occurrences: count(&tc, "SELECT count(*) FROM message_occurrences").await,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn poison_marker_attempts_1_retries_item() {
    // pre-registered: one death on item 1 → the item is retried once and imported, no error row
    let r = run_with_marker(true, 1, 2_097_188, "Inbox", 1, false).await;
    assert_eq!(r.errors, Vec::<ErrorRow>::new());
    assert_eq!((r.messages, r.occurrences), (1, 2));
}

#[tokio::test(flavor = "multi_thread")]
async fn poison_marker_attempts_2_skips_item() {
    // pre-registered: two deaths on item 2 → item 1 is read again normally, item 2 is skipped with one parse row
    const POISON: (i64, i64, &str) = (2, 2_097_220, "Recovered_Group2"); // seq, pst_item_id, folder
    let r = run_with_marker(true, POISON.0, POISON.1, POISON.2, 2, false).await;
    assert_eq!(
        r.errors,
        [("parse".into(), "item_skipped".into(), Some(POISON.1), Some(POISON.2.into()), "parser crashed on this item".into())]
    );
    assert_eq!((r.messages, r.occurrences), (1, 1));
}

#[tokio::test(flavor = "multi_thread")]
async fn poison_marker_of_other_job_is_ignored() {
    let r = run_with_marker(false, 1, 2_097_188, "Inbox", 2, false).await;
    assert_eq!(r.errors, Vec::<ErrorRow>::new());
    assert_eq!((r.messages, r.occurrences), (1, 2));
}

#[tokio::test(flavor = "multi_thread")]
async fn poison_marker_dropped_on_checkpoint_reset() {
    // pre-registered: the file identity changed, so `seq` may name another item → the marker is not honored
    let r = run_with_marker(true, 2, 2_097_220, "Recovered_Group2", 2, true).await;
    assert_eq!(r.errors, Vec::<ErrorRow>::new());
    assert_eq!((r.messages, r.occurrences), (1, 2));
}

/// Rule 2: before the retry the marker on disk already says attempts = 2, so a death during the retry leads to the
/// skip of `poison_marker_attempts_2_skips_item` on the next start.
#[tokio::test(flavor = "multi_thread")]
async fn retry_rewrites_marker_before_reading() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use outlooker_core::importer::progress::{BYTE_BUDGET, ByteBudget};
    use outlooker_core::importer::pst_reader::{ReaderMsg, ReaderParams, spawn};
    let dir = tempfile::tempdir().unwrap();
    let marker = Inflight::new(dir.path().join("import-inflight"));
    let pending = InflightRecord { job_id: 9, seq: 1, folder_path: "Inbox".into(), pst_item_id: 2_097_188, attempts: 1 };
    let cancel = Arc::new(AtomicBool::new(false));
    let budget = Arc::new(ByteBudget::new(BYTE_BUDGET));
    let (rx, reader) = spawn(ReaderParams {
        path: outlook_pst(),
        cursor_seq: 0,
        pending: Some(pending.clone()),
        inflight: Some(marker.clone()),
        cancel: cancel.clone(),
        hold_after: Some(1), // the reader stops after item 1 until cancelled
        budget: budget.clone(),
        job_id: 9,
    });
    assert!(matches!(rx.recv().unwrap(), ReaderMsg::Scanned(_)));
    let ReaderMsg::Item(item) = rx.recv().unwrap() else { panic!("item 1 expected") };
    assert_eq!(item.seq, 1);
    assert_eq!(marker.read(), Some(InflightRecord { attempts: 2, ..pending }));
    cancel.store(true, Ordering::SeqCst);
    budget.wake_all();
    reader.join().unwrap();
}

/// `TestCluster::kill` = `pg_ctl stop -m immediate` of this test's own cluster; `restart` starts it again (new port).
#[tokio::test(flavor = "multi_thread")]
async fn pg_crash_mid_import_resumes() {
    const ITEMS: i64 = 40;
    let want = reference(corpus(ITEMS)).await;
    let mut tc = TestCluster::start().await;
    let mut tuning = small_batches();
    tuning.hold_reader_after = Some(30);
    let mut batches = batch_signal(&mut tuning);
    let mut fx = manager(&tc, tuning);
    let pst_id = fx.mgr.enqueue_synthetic(&synthetic_path(&tc, "pg.pst"), folders(), corpus(ITEMS)).await.unwrap();
    fx.mgr.start_worker();
    wait_batch(&mut batches, 2).await;
    tc.kill().await;
    fx.phase.send_replace(AppPhase::StartingDb);
    tc.restart().await;
    fx.phase.send_replace(AppPhase::Ready);
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Completed);
    assert_eq!(snapshot(&tc).await, want);
    let dup = count(&tc, "SELECT count(*) FROM (SELECT 1 FROM message_occurrences GROUP BY pst_id, pst_item_id HAVING count(*) > 1) d").await;
    assert_eq!(dup, 0);
}

/// Part 02 §6.1: the restart budget runs out (phase `Failed`), the user retries (`app_retry` → `Ready` in the same
/// process). The paused job must resume from its checkpoint; `recover_after_crash` runs only once per process.
#[tokio::test(flavor = "multi_thread")]
async fn pg_crash_failed_then_retry_resumes() {
    const ITEMS: i64 = 40;
    let want = reference(corpus(ITEMS)).await;
    let mut tc = TestCluster::start().await;
    let mut tuning = small_batches();
    tuning.hold_reader_after = Some(30);
    let mut batches = batch_signal(&mut tuning);
    let mut fx = manager(&tc, tuning);
    let pst_id = fx.mgr.enqueue_synthetic(&synthetic_path(&tc, "failed.pst"), folders(), corpus(ITEMS)).await.unwrap();
    fx.mgr.start_worker();
    wait_batch(&mut batches, 2).await;
    tc.kill().await;
    fx.phase.send_replace(AppPhase::StartingDb);
    fx.phase.send_replace(AppPhase::Failed);
    tc.restart().await; // the job observes `Failed` while the cluster starts again
    fx.phase.send_replace(AppPhase::Ready);
    let end = tokio::time::timeout(Duration::from_secs(20), wait_job(&mut fx.events, pst_id)).await;
    assert_eq!(end.ok(), Some(ImportStatus::Completed), "the paused job resumes on the next Ready");
    assert_eq!(snapshot(&tc).await, want);
}

#[tokio::test(flavor = "multi_thread")]
async fn recover_after_crash_states() {
    const CURSOR: i64 = 10;
    const RESUMED_SUBJECTS: [&str; 9] =
        ["Mail m11", "Mail m12", "Mail m13", "Mail m14", "Mail m15", "Mail m16", "Mail m17", "Mail m18", "Mail m19"];
    let tc = TestCluster::start().await;
    let mut fx = manager(&tc, small_batches());
    let a = fx.mgr.enqueue_synthetic(&synthetic_path(&tc, "ra.pst"), folders(), corpus(20)).await.unwrap();
    let b = fx.mgr.enqueue_synthetic(&synthetic_path(&tc, "rb.pst"), folders(), corpus_tagged(20, "b")).await.unwrap();
    let adm = tc.admin().await;
    // states a crash leaves (one at a time: import_jobs_one_running allows one running / cancelling job)
    adm.execute("UPDATE import_jobs SET state = 'cancelling', started_at = now() WHERE pst_id = $1", &[&b]).await.unwrap();
    assert_eq!(fx.mgr.recover_after_crash().await.unwrap(), RecoveryReport { requeued: vec![], cancelled: vec![b] });
    // a: running with a checkpoint at seq 10
    adm.execute("UPDATE import_jobs SET state = 'running', started_at = now() WHERE pst_id = $1", &[&a]).await.unwrap();
    adm.execute(
        "INSERT INTO import_checkpoints (pst_id, job_id, phase, items_total, cursor_seq, pst_file_size, pst_file_mtime) \
         SELECT s.id, j.id, 'importing', 20, $2, s.file_size_bytes, s.file_mtime FROM pst_sources s JOIN import_jobs j ON j.pst_id = s.id WHERE s.id = $1",
        &[&a, &CURSOR],
    )
    .await
    .unwrap();
    assert_eq!(fx.mgr.recover_after_crash().await.unwrap(), RecoveryReport { requeued: vec![a], cancelled: vec![] });
    let states: Vec<(i64, String, bool)> = adm
        .query("SELECT pst_id, state, finished_at IS NOT NULL FROM import_jobs ORDER BY pst_id", &[])
        .await
        .unwrap()
        .iter()
        .map(|r| (r.get(0), r.get(1), r.get(2)))
        .collect();
    assert_eq!(states, [(a, "queued".to_string(), false), (b, "cancelled".to_string(), true)]);
    fx.mgr.start_worker();
    assert_eq!(wait_job(&mut fx.events, a).await, ImportStatus::Completed);
    let subjects: Vec<String> = adm.query("SELECT subject FROM messages ORDER BY subject", &[]).await.unwrap().iter().map(|r| r.get(0)).collect();
    assert_eq!(subjects, RESUMED_SUBJECTS, "only seq > cursor_seq was read");
}

// ------------------------------------------------------------------------------------------------ queue commands

#[tokio::test(flavor = "multi_thread")]
async fn pst_list_status_derivation() {
    // Part 04 §2.1: newest job (kind, state) → status; the last row has an older completed import
    const TABLE: [(&[(&str, &str)], ImportStatus); 10] = [
        (&[("import", "queued")], ImportStatus::Queued),
        (&[("import", "running")], ImportStatus::Running),
        (&[("import", "cancelling")], ImportStatus::Cancelling),
        (&[("import", "cancelled")], ImportStatus::Cancelled),
        (&[("import", "failed")], ImportStatus::Failed),
        (&[("import", "completed")], ImportStatus::Completed),
        (&[("remove", "queued")], ImportStatus::Removing),
        (&[("remove", "running")], ImportStatus::Removing),
        (&[("remove", "failed")], ImportStatus::Failed),
        (&[("import", "completed"), ("remove", "queued")], ImportStatus::Removing),
    ];
    let tc = TestCluster::start().await;
    let adm = tc.admin().await;
    let fx = manager(&tc, ImportTuning::default());
    // one case at a time: import_jobs_one_running allows one running / cancelling job in the library
    for (n, (jobs, status)) in TABLE.iter().enumerate() {
        let id = insert_source(&adm, n, jobs).await;
        let list = fx.mgr.list().await.unwrap();
        assert_eq!((list.len(), list[0].status), (1, *status), "case {n}: {jobs:?}");
        adm.execute("DELETE FROM pst_sources WHERE id = $1", &[&id]).await.unwrap();
    }
    // order: Running / Cancelling first, then Queued / Removing by job id, then the rest newest first
    let order = [("import", "completed"), ("import", "queued"), ("remove", "queued"), ("import", "running"), ("import", "failed")];
    let mut ids = Vec::new();
    for (n, job) in order.iter().enumerate() {
        ids.push(insert_source(&adm, 100 + n, std::slice::from_ref(job)).await);
    }
    let listed: Vec<i64> = fx.mgr.list().await.unwrap().iter().map(|p| p.id).collect();
    assert_eq!(listed, [ids[3], ids[1], ids[2], ids[4], ids[0]]);
}

/// A source row with the given jobs (oldest first); returns its id.
async fn insert_source(adm: &tokio_postgres::Client, n: usize, jobs: &[(&str, &str)]) -> i64 {
    let id: i64 = adm
        .query_one(
            "INSERT INTO pst_sources (path, display_name, file_size_bytes, file_mtime) VALUES ($1, 'p', 1, now()) RETURNING id",
            &[&format!("/status/{n}.pst")],
        )
        .await
        .unwrap()
        .get(0);
    for (kind, state) in jobs {
        adm.execute(
            "INSERT INTO import_jobs (pst_id, kind, state, started_at, finished_at) VALUES ($1, $2, $3, \
               CASE WHEN $3 = 'queued' THEN NULL ELSE now() END, \
               CASE WHEN $3 IN ('cancelled', 'failed', 'completed') THEN now() END)",
            &[&id, kind, state],
        )
        .await
        .unwrap();
    }
    id
}

#[tokio::test(flavor = "multi_thread")]
async fn pst_remove_with_queued_job() {
    let tc = TestCluster::start().await;
    let mut fx = manager(&tc, ImportTuning::default());
    let pst_id = fx.mgr.enqueue_synthetic(&synthetic_path(&tc, "q.pst"), folders(), corpus(3)).await.unwrap();
    // worker not started: the import job is still queued
    fx.mgr.remove(pst_id).await.unwrap(); // no 23505 from import_jobs_one_active_per_pst
    assert_eq!(fx.mgr.list().await.unwrap()[0].status, ImportStatus::Removing);
    assert!(matches!(fx.mgr.remove(pst_id).await, Err(AppError::Busy { .. })));
    fx.mgr.start_worker();
    assert_eq!(wait_job(&mut fx.events, pst_id).await, ImportStatus::Removing);
    assert_eq!(count(&tc, "SELECT count(*) FROM pst_sources").await, 0);
    assert_eq!(count(&tc, "SELECT count(*) FROM messages").await, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn enqueue_validation() {
    let tc = TestCluster::start().await;
    let fx = manager(&tc, ImportTuning::default());
    let dir = tempfile::tempdir().unwrap();
    let not_pst = dir.path().join("notes.pst");
    std::fs::write(&not_pst, b"hello, not a PST").unwrap();
    let missing = s(&dir.path().join("missing.pst"));
    let fa = |path: &str, reason: &str| AppError::FileAccess { path: path.into(), reason: reason.into() };
    let res = fx.mgr.enqueue(vec!["relative/x.pst".into(), missing.clone(), s(&not_pst), s(dir.path()), s(&outlook_pst())]).await.unwrap();
    let errors: Vec<Option<AppError>> =
        res.iter().map(|r| if let EnqueueResult::Rejected { error, .. } = r { Some(error.clone()) } else { None }).collect();
    assert_eq!(
        errors,
        [
            Some(fa("relative/x.pst", "notAbsolute")),
            Some(fa(&missing, "notFound")),
            Some(AppError::NotAPst { path: s(&not_pst) }),
            Some(fa(&s(dir.path()), "notAFile")),
            None,
        ]
    );
    let EnqueueResult::Queued { pst } = &res[4] else { panic!() };
    let again = fx.mgr.enqueue(vec![s(&outlook_pst())]).await.unwrap();
    assert!(matches!(&again[0], EnqueueResult::Rejected { error: AppError::AlreadyImported { pst_id }, .. } if *pst_id == pst.id), "{again:?}");
    assert_eq!(fx.mgr.enqueue(vec![]).await.unwrap_err(), AppError::invalid("paths", "empty"));
    assert_eq!(fx.mgr.enqueue(vec![missing; 101]).await.unwrap_err(), AppError::invalid("paths", "tooMany"));
}

// ------------------------------------------------------------------------------------------------ Outlook fixtures (W2)

#[cfg(feature = "outlook-fixtures")]
mod fixtures {
    use super::*;

    fn fixture(name: &str) -> String {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join(REPO_ROOT).join(format!("tests/fixtures/pst/{name}.pst"));
        assert!(p.exists(), "missing fixture tests/fixtures/pst/{name}.pst (Windows phase W2)");
        s(&std::fs::canonicalize(p).unwrap())
    }

    /// Imports the fixtures in order; every job must end Completed.
    async fn import(names: &[&str]) -> (TestCluster, Fx, Vec<i64>) {
        let paths: Vec<String> = names.iter().map(|n| fixture(n)).collect();
        let tc = TestCluster::start().await;
        let mut fx = manager(&tc, ImportTuning::default());
        let res = fx.mgr.enqueue(paths).await.unwrap();
        let ids: Vec<i64> = res.iter().map(|r| if let EnqueueResult::Queued { pst } = r { pst.id } else { panic!("{r:?}") }).collect();
        fx.mgr.start_worker();
        assert!(wait_jobs(&mut fx.events, &ids).await.iter().all(|s| *s == ImportStatus::Completed));
        (tc, fx, ids)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fixtures_basic() {
        let ids = walk_ids(Path::new(&fixture("basic"))).unwrap();
        assert_eq!(ids, walk_ids(Path::new(&fixture("basic"))).unwrap(), "deterministic order");
        let (tc, _fx, _) = import(&["basic"]).await;
        assert_eq!(count(&tc, "SELECT count(*) FROM messages").await, 50);
        assert!(count(&tc, "SELECT count(*) FROM folders WHERE path = 'Inbox/Projects'").await == 1);
        assert!(count(&tc, "SELECT count(DISTINCT body_source) FROM messages WHERE body_source IN ('html', 'rtf_html', 'rtf_text')").await >= 2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fixtures_cjk() {
        let (tc, _fx, _) = import(&["cjk"]).await;
        assert_eq!(count(&tc, "SELECT count(*) FROM messages").await, 30);
        assert_eq!(count(&tc, "SELECT count(*) FROM messages WHERE body_text ~ '[一-鿿]'").await, 30);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fixtures_inline() {
        let (tc, _fx, _) = import(&["inline"]).await;
        assert_eq!(count(&tc, "SELECT count(*) FROM messages").await, 20);
        assert_eq!(
            count(&tc, "SELECT count(*) FROM (SELECT image_blob_id FROM inline_images GROUP BY 1 HAVING count(DISTINCT message_id) = 10) l").await,
            1,
            "the logo of 10 mails is one blob"
        );
        let dangling = count(
            &tc,
            "SELECT count(*) FROM messages m, regexp_matches(m.body_html, 'outlooker-inline:([0-9]+)', 'g') x \
              WHERE NOT EXISTS (SELECT 1 FROM inline_images i WHERE i.message_id = m.id AND i.ordinal = x[1]::int)",
        )
        .await;
        assert_eq!(dangling, 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fixtures_attach() {
        let (tc, _fx, _) = import(&["attach"]).await;
        assert!(count(&tc, "SELECT count(*) FROM attachments WHERE file_name LIKE '%.msg' AND mime_type = 'application/vnd.ms-outlook'").await >= 1);
        assert!(count(&tc, "SELECT count(*) FROM attachments WHERE length(file_name) > 255").await >= 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fixtures_dup() {
        let (tc, mut fx, ids) = import(&["dup-a", "dup-b"]).await;
        assert_eq!(count(&tc, "SELECT count(*) FROM messages").await, 50);
        assert_eq!(count(&tc, "SELECT count(*) FROM message_occurrences").await, 81);
        fx.mgr.remove(ids[0]).await.unwrap();
        assert_eq!(wait_job(&mut fx.events, ids[0]).await, ImportStatus::Removing);
        assert_eq!(count(&tc, "SELECT count(*) FROM messages").await, 40, "shared messages remain");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fixtures_hostile() {
        let (tc, _fx, _) = import(&["hostile"]).await;
        let rows = tc.admin().await.query("SELECT body_html FROM messages WHERE body_html IS NOT NULL", &[]).await.unwrap();
        assert!(!rows.is_empty());
        for r in rows {
            let html: String = r.get(0);
            assert_eq!(hostile_violations(&html), Vec::<String>::new(), "{html}");
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fixtures_corrupt() {
        let (tc, _fx, _) = import(&["corrupt"]).await;
        assert!(count(&tc, "SELECT count(*) FROM import_errors WHERE severity = 'item_skipped'").await >= 1);
        assert_eq!(count(&tc, "SELECT count(*) FROM messages").await, 49);
    }
}
```

Run (in `src-tauri/`): `cargo test -p outlooker-core --features test-support --test import_pipeline`.
Expected: compile errors, among them `unresolved imports outlooker_core::importer::ImportManager, outlooker_core::importer::ImportTuning, outlooker_core::importer::RecoveryReport` and `unresolved import outlooker_core::importer::inflight::Inflight` (the kickoff module files are empty).

- [ ] **Step 2: Write the poison-item marker.** Replace `src-tauri/crates/outlooker-core/src/importer/inflight.rs` with:

```rust
//! Poison-item marker `<data_root>/import-inflight` (spec Part 02 §6.3).
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InflightRecord {
    pub job_id: i64,
    pub seq: i64,
    pub folder_path: String,
    /// 0 until the reader has the item's identifier (the first write happens before any libpff call on the item).
    pub pst_item_id: i64,
    /// How many times the reader has started this item (Part 02 §6.3).
    pub attempts: u32,
}

#[derive(Debug, Clone)]
pub struct Inflight {
    path: PathBuf,
}

impl Inflight {
    pub fn new(path: PathBuf) -> Inflight {
        Inflight { path }
    }

    /// Overwrites the marker before the reader touches an item. No fsync: page-cache writes survive a process crash.
    pub fn write(&self, r: &InflightRecord) {
        let body = serde_json::to_vec(r).expect("record serializes");
        if let Err(e) = std::fs::write(&self.path, body) {
            log::warn!("cannot write {}: {e}", self.path.display());
        }
    }

    /// None when the file is missing or unreadable (a torn write after a crash is treated as no marker).
    pub fn read(&self) -> Option<InflightRecord> {
        let body = std::fs::read(&self.path).ok()?;
        serde_json::from_slice(&body).ok()
    }

    pub fn clear(&self) {
        match std::fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => log::warn!("cannot remove {}: {e}", self.path.display()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inflight_round_trip_and_clear() {
        let dir = tempfile::tempdir().unwrap();
        let m = Inflight::new(dir.path().join("import-inflight"));
        assert_eq!(m.read(), None);
        let r = InflightRecord { job_id: 7, seq: 42, folder_path: "Inbox/報價".into(), pst_item_id: 2_097_188, attempts: 2 };
        m.write(&r);
        assert_eq!(m.read(), Some(r.clone()));
        let text = std::fs::read_to_string(dir.path().join("import-inflight")).unwrap();
        assert!(text.contains("\"jobId\":7") && text.contains("\"pstItemId\":2097188") && text.contains("\"attempts\":2"), "{text}");
        m.write(&InflightRecord { seq: 43, ..r });
        assert_eq!(m.read().unwrap().seq, 43);
        m.clear();
        assert_eq!(m.read(), None);
        m.clear(); // missing file is fine
        std::fs::write(dir.path().join("import-inflight"), b"{\"jobId\":").unwrap();
        assert_eq!(m.read(), None, "torn write reads as no marker");
    }
}
```

Run: `cargo test -p outlooker-core --lib importer::inflight`.
Expected: `importer::inflight::tests::inflight_round_trip_and_clear ... ok`; `test result: ok. 1 passed`.

- [ ] **Step 3: Write throughput / ETA and the byte budget.** Replace `src-tauri/crates/outlooker-core/src/importer/progress.rs` with:

```rust
//! Throughput / ETA over the last 10 s and the 256 MB reader → writer byte budget (spec Part 03 §7.1, Part 04 §5.1).
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

pub const BYTE_BUDGET: u64 = 256 * 1024 * 1024;
const WINDOW: Duration = Duration::from_secs(10);

pub struct Throughput {
    samples: VecDeque<(Instant, i64, u64)>,
}

impl Default for Throughput {
    fn default() -> Self {
        Throughput::new()
    }
}

impl Throughput {
    pub fn new() -> Throughput {
        Throughput { samples: VecDeque::new() }
    }

    /// `items_done` and `bytes` are running totals.
    pub fn record(&mut self, now: Instant, items_done: i64, bytes: u64) {
        self.samples.push_back((now, items_done, bytes));
        // keep one sample at or before the window start, so the rate always spans the full window
        while self.samples.len() > 2 && now.duration_since(self.samples[1].0) >= WINDOW {
            self.samples.pop_front();
        }
    }

    fn span(&self) -> Option<(f64, i64, u64)> {
        let (t0, i0, b0) = *self.samples.front()?;
        let (t1, i1, b1) = *self.samples.back()?;
        let secs = t1.duration_since(t0).as_secs_f64();
        (secs > 0.0).then_some((secs, i1 - i0, b1.saturating_sub(b0)))
    }

    pub fn bytes_per_sec(&self) -> Option<i64> {
        self.span().map(|(secs, _, bytes)| (bytes as f64 / secs) as i64)
    }

    pub fn eta_seconds(&self, items_total: Option<i64>, items_done: i64) -> Option<i64> {
        let (secs, items, _) = self.span()?;
        if items <= 0 {
            return None;
        }
        let remaining = (items_total? - items_done).max(0);
        Some((remaining as f64 * secs / items as f64).ceil() as i64)
    }
}

/// Pending raw bytes between the reader and the writer (Mutex + Condvar; the reader thread blocks, never spins).
pub struct ByteBudget {
    limit: u64,
    used: Mutex<u64>,
    changed: Condvar,
}

impl ByteBudget {
    pub fn new(limit: u64) -> ByteBudget {
        ByteBudget { limit, used: Mutex::new(0), changed: Condvar::new() }
    }

    /// Blocks while `used + n > limit` (an item larger than the whole budget passes when nothing is pending),
    /// unless `cancel` is set.
    pub fn acquire(&self, n: u64, cancel: &AtomicBool) {
        let mut used = self.used.lock().unwrap_or_else(|p| p.into_inner());
        while *used > 0 && *used + n > self.limit && !cancel.load(Ordering::SeqCst) {
            used = self.changed.wait(used).unwrap_or_else(|p| p.into_inner());
        }
        *used += n;
    }

    pub fn release(&self, n: u64) {
        let mut used = self.used.lock().unwrap_or_else(|p| p.into_inner());
        *used = used.saturating_sub(n);
        self.changed.notify_all();
    }

    /// Called after setting a cancel flag: wakes every blocked `acquire` / `wait_cancelled`.
    pub fn wake_all(&self) {
        let _used = self.used.lock().unwrap_or_else(|p| p.into_inner());
        self.changed.notify_all();
    }

    /// Blocks until `cancel` is set (test seam `ImportTuning::hold_reader_after`); woken by `wake_all`.
    pub(crate) fn wait_cancelled(&self, cancel: &AtomicBool) {
        let mut used = self.used.lock().unwrap_or_else(|p| p.into_inner());
        while !cancel.load(Ordering::SeqCst) {
            used = self.changed.wait(used).unwrap_or_else(|p| p.into_inner());
        }
    }

    pub fn used(&self) -> u64 {
        *self.used.lock().unwrap_or_else(|p| p.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn throughput_and_eta() {
        let t0 = Instant::now();
        let mut t = Throughput::new();
        assert_eq!(t.bytes_per_sec(), None);
        t.record(t0, 0, 0);
        assert_eq!(t.bytes_per_sec(), None, "one sample has no span");
        t.record(t0 + Duration::from_secs(2), 100, 2_000_000);
        assert_eq!(t.bytes_per_sec(), Some(1_000_000));
        assert_eq!(t.eta_seconds(Some(1_100), 100), Some(20));
        assert_eq!(t.eta_seconds(None, 100), None);
        // old samples leave the 10 s window
        t.record(t0 + Duration::from_secs(13), 200, 2_000_000);
        t.record(t0 + Duration::from_secs(14), 300, 3_000_000);
        assert_eq!(t.samples.front().unwrap().1, 100, "keeps the newest sample at or before now - 10 s");
        assert_eq!(t.bytes_per_sec(), Some(83_333));
    }

    #[test]
    fn budget_blocks_until_release_or_cancel() {
        let b = Arc::new(ByteBudget::new(100));
        let cancel = Arc::new(AtomicBool::new(false));
        b.acquire(500, &cancel); // larger than the budget, nothing pending → passes
        assert_eq!(b.used(), 500);
        b.release(500);
        b.acquire(60, &cancel);
        let (tx, rx) = std::sync::mpsc::channel();
        let (b2, c2) = (b.clone(), cancel.clone());
        let h = std::thread::spawn(move || {
            b2.acquire(60, &c2); // 60 + 60 > 100 → blocks
            tx.send(()).unwrap();
        });
        assert!(rx.recv_timeout(Duration::from_millis(100)).is_err(), "acquire must block");
        b.release(60);
        rx.recv().unwrap();
        h.join().unwrap();
        // cancel releases a blocked acquire
        let (tx, rx) = std::sync::mpsc::channel();
        let (b3, c3) = (b.clone(), cancel.clone());
        let h = std::thread::spawn(move || {
            b3.acquire(90, &c3);
            tx.send(()).unwrap();
        });
        assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
        cancel.store(true, Ordering::SeqCst);
        b.wake_all();
        rx.recv().unwrap();
        h.join().unwrap();
        b.wait_cancelled(&cancel); // returns at once when already cancelled
    }
}
```

Run: `cargo test -p outlooker-core --lib importer::progress`.
Expected: `throughput_and_eta ... ok`, `budget_blocks_until_release_or_cancel ... ok`; `test result: ok. 2 passed`.

- [ ] **Step 4: Write the reader thread.** Replace `src-tauri/crates/outlooker-core/src/importer/pst_reader.rs` with:

```rust
//! Reader thread (spec Part 04 §5.1–§5.3, §5.8; Part 02 §6.3). The only code that calls libpff; it owns the
//! `pff::File` (`!Send`) for the whole job.
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use pff::{AttachmentType, File, Item, tags};

use crate::dto::import::{ImportErrorSeverity, ImportErrorStage};
use crate::importer::inflight::{Inflight, InflightRecord};
use crate::importer::progress::ByteBudget;
use crate::importer::types::{FolderIn, ItemError, RawAttachment, RawContent, RawItem, RawMail, RawRecipient};

/// Bounded channel size reader → normalize (Part 02 §4).
pub const CHANNEL: usize = 256;
/// Attachment bytes are read only for content-id attachments up to this size (Part 04 §5.7 source limit).
pub const MAX_ATTACHMENT_DATA: usize = 25 * 1024 * 1024;

// MAPI property ids not in `pff::tags` (MS-OXPROPS).
const IPM_SUBTREE_ENTRY_ID: u32 = 0x35e0;
const TRANSPORT_MESSAGE_HEADERS: u32 = 0x007d;
const SENT_REPRESENTING_SMTP_ADDRESS: u32 = 0x5d02;
const SENT_REPRESENTING_NAME: u32 = 0x0042;
const MAIL_CLASSES: [&str; 3] = ["IPM.Note", "IPM.Schedule.Meeting", "REPORT.IPM.Note"];
const POISON_MESSAGE: &str = "parser crashed on this item";

pub struct ScanResult {
    pub folders: Vec<FolderIn>,
    pub items_total: i64,
    pub scan_errors: Vec<ItemError>,
}

pub enum ReaderMsg {
    Scanned(ScanResult),
    Item(RawItem),
    Failed { stage: ImportErrorStage, message: String },
}

pub struct ReaderParams {
    pub path: PathBuf,
    pub cursor_seq: i64,
    /// Marker of this job left by a dead process (Part 02 §6.3 rule 1); None for a fresh start.
    pub pending: Option<InflightRecord>,
    pub inflight: Option<Inflight>,
    pub cancel: Arc<AtomicBool>,
    pub hold_after: Option<i64>,
    pub budget: Arc<ByteBudget>,
    /// Written into every inflight record.
    pub job_id: i64,
}

/// One folder of the walk, in walk order.
struct WalkFolder<'f> {
    item: Item<'f>,
    id: i64,
    path: String,
    messages: usize,
}

fn display_segment(name: &str) -> String {
    name.replace('/', "\u{2215}")
}

/// NID of the IPM subtree root: little-endian u32 in the last 4 bytes of PidTagIpmSubtreeEntryId (Part 04 §5.2).
fn ipm_root_id(file: &File) -> Option<u32> {
    let store = file.message_store().ok()?;
    let entry = store.binary(0, IPM_SUBTREE_ENTRY_ID).ok()??;
    let tail: [u8; 4] = entry.get(entry.len().checked_sub(4)?..)?.try_into().ok()?;
    Some(u32::from_le_bytes(tail))
}

/// Folder tree, depth-first by sub-folder index (pre-order). A folder that fails to list is a `scan` / `degraded`
/// error row and its subtree is skipped. Paths: below the IPM root relative to it (`Inbox/Projects`); the IPM
/// root itself is its own name; other folders `~/<path from the PST root>` (the PST root itself is `~`).
fn scan<'f>(file: &'f File) -> pff::Result<(Vec<WalkFolder<'f>>, ScanResult)> {
    let ipm = ipm_root_id(file);
    let root = file.root_folder()?;
    let mut walk = Vec::new();
    let mut out = ScanResult { folders: Vec::new(), items_total: 0, scan_errors: Vec::new() };
    // (item, parent id, path from the PST root, path relative to the IPM root if below it)
    let mut stack: Vec<(Item<'f>, Option<i64>, Vec<String>, Option<Vec<String>>)> = vec![(root, None, Vec::new(), None)];
    while let Some((item, parent, from_root, below_ipm)) = stack.pop() {
        let id = i64::from(item.identifier()?);
        let name = item.folder_name().ok().flatten().unwrap_or_default();
        let is_ipm = ipm.is_some_and(|n| i64::from(n) == id);
        let rel: Option<Vec<String>> = if is_ipm { Some(Vec::new()) } else { below_ipm };
        let path = match &rel {
            Some(r) if r.is_empty() => if name.is_empty() { "~".to_owned() } else { display_segment(&name) },
            Some(r) => r.join("/"),
            None if from_root.is_empty() => "~".to_owned(),
            None => format!("~/{}", from_root.join("/")),
        };
        let listed = (|| -> pff::Result<(usize, Vec<Item<'f>>)> {
            let messages = item.sub_message_count()?;
            let n = item.sub_folder_count()?;
            let subs = (0..n).map(|i| item.sub_folder(i)).collect::<pff::Result<Vec<_>>>()?;
            Ok((messages, subs))
        })();
        out.folders.push(FolderIn { pst_folder_id: id, parent_pst_folder_id: parent, name: name.clone(), path: path.clone() });
        let (messages, subs) = match listed {
            Ok(v) => v,
            Err(e) => {
                let mut err = ItemError::new(ImportErrorStage::Scan, ImportErrorSeverity::Degraded, &path, 0, &e.message);
                err.pst_item_id = None;
                out.scan_errors.push(err);
                continue;
            }
        };
        out.items_total += messages as i64;
        // children are pushed in reverse so they pop in index order
        for sub in subs.into_iter().rev() {
            let seg = display_segment(&sub.folder_name().ok().flatten().unwrap_or_default());
            let child_root: Vec<String> = from_root.iter().cloned().chain(std::iter::once(seg.clone())).collect();
            let child_rel = rel.as_ref().map(|r| r.iter().cloned().chain(std::iter::once(seg)).collect());
            stack.push((sub, Some(id), child_root, child_rel));
        }
        walk.push(WalkFolder { item, id, path, messages });
    }
    Ok((walk, out))
}

fn filetime(ft: Option<u64>) -> Option<SystemTime> {
    let us = pff::filetime_to_unix_micros(ft?)?;
    Some(if us >= 0 { UNIX_EPOCH + Duration::from_micros(us as u64) } else { UNIX_EPOCH - Duration::from_micros(us.unsigned_abs()) })
}

fn is_mail_class(class: &str) -> bool {
    MAIL_CLASSES.iter().any(|c| class.len() >= c.len() && class[..c.len()].eq_ignore_ascii_case(c))
}

fn read_mail(item: &Item<'_>) -> pff::Result<RawContent> {
    let class = item.string(0, tags::MESSAGE_CLASS)?.unwrap_or_default();
    if !is_mail_class(&class) {
        return Ok(RawContent::NonMail);
    }
    let s = |tag| item.string(0, tag);
    let mut m = RawMail {
        subject: s(tags::SUBJECT)?,
        client_submit_time: filetime(item.client_submit_time()?),
        delivery_time: filetime(item.delivery_time()?),
        internet_message_id: s(tags::INTERNET_MESSAGE_ID)?,
        transport_headers: s(TRANSPORT_MESSAGE_HEADERS)?,
        sender_smtp_address: s(tags::SENDER_SMTP_ADDRESS)?,
        sent_representing_smtp_address: s(SENT_REPRESENTING_SMTP_ADDRESS)?,
        sender_email_address: s(tags::SENDER_EMAIL_ADDRESS)?,
        sender_address_type: s(tags::SENDER_ADDRESS_TYPE)?,
        sender_name: s(tags::SENDER_NAME)?,
        sent_representing_name: s(SENT_REPRESENTING_NAME)?,
        internet_codepage: item.u32_value(0, tags::INTERNET_CODEPAGE)?,
        html_body: item.html_body()?,
        rtf_body: item.rtf_body()?,
        plain_body: item.plain_text_body()?.map(|b| String::from_utf8_lossy(&b).into_owned()),
        recipients: Vec::new(),
        attachments: Vec::new(),
    };
    if let Some(r) = item.recipients()? {
        for set in 0..r.record_set_count()? {
            m.recipients.push(RawRecipient {
                smtp_address: r.string(set, tags::SMTP_ADDRESS)?,
                email_address: r.string(set, tags::EMAIL_ADDRESS)?,
                address_type: r.string(set, tags::ADDRESS_TYPE)?,
                display_name: r.string(set, tags::DISPLAY_NAME)?,
                recipient_type: r.u32_value(set, tags::RECIPIENT_TYPE)?,
            });
        }
    }
    for index in 0..item.attachment_count()? {
        let a = item.attachment(index)?;
        let kind = a.attachment_type()?;
        let content_id = a.string(0, tags::ATTACH_CONTENT_ID)?;
        let data = match (kind, &content_id) {
            (AttachmentType::Data, Some(_)) => a.attachment_data(MAX_ATTACHMENT_DATA)?,
            _ => None,
        };
        let embedded_subject = match kind {
            AttachmentType::Item => match a.attached_item()? {
                Some(inner) => inner.string(0, tags::SUBJECT)?,
                None => None,
            },
            _ => None,
        };
        m.attachments.push(RawAttachment {
            index: index as i32,
            long_filename: a.string(0, tags::ATTACH_LONG_FILENAME)?,
            filename: a.string(0, tags::ATTACH_FILENAME)?,
            display_name: a.string(0, tags::DISPLAY_NAME)?,
            mime_tag: a.string(0, tags::ATTACH_MIME_TAG)?,
            content_id,
            attach_method: a.u32_value(0, tags::ATTACH_METHOD)?.map(|v| v as i32),
            data_size: if kind == AttachmentType::Data { Some(a.attachment_data_size()? as i64) } else { None },
            attach_size: a.u32_value(0, tags::ATTACH_SIZE)?.map(i64::from),
            embedded_subject,
            data,
        });
    }
    Ok(RawContent::Mail(Box::new(m)))
}

/// Shared send path of both readers: cancel check is done by the caller; budget acquire before the send;
/// the hold seam blocks after the item whose seq equals `hold_after` until the job is cancelled.
fn send_item(p: &ReaderParams, tx: &SyncSender<ReaderMsg>, raw: RawItem) -> bool {
    let seq = raw.seq;
    p.budget.acquire(raw.approx_bytes(), &p.cancel);
    if tx.send(ReaderMsg::Item(raw)).is_err() {
        return false;
    }
    if p.hold_after == Some(seq) {
        p.budget.wait_cancelled(&p.cancel);
    }
    true
}

fn run(p: ReaderParams, tx: SyncSender<ReaderMsg>) {
    let file = match File::open(&p.path) {
        Ok(f) => f,
        Err(e) => {
            let _ = tx.send(ReaderMsg::Failed { stage: ImportErrorStage::Open, message: e.message });
            return;
        }
    };
    let (walk, scanned) = match scan(&file) {
        Ok(v) => v,
        Err(e) => {
            let _ = tx.send(ReaderMsg::Failed { stage: ImportErrorStage::Scan, message: e.message });
            return;
        }
    };
    if tx.send(ReaderMsg::Scanned(scanned)).is_err() {
        return;
    }
    let mut seq = 0i64;
    for folder in &walk {
        for index in 0..folder.messages {
            seq += 1;
            if seq <= p.cursor_seq {
                continue; // committed before: not read at all (Part 04 §5.9 step 3)
            }
            if p.cancel.load(Ordering::SeqCst) {
                return;
            }
            let base = |pst_item_id: i64, content: RawContent| RawItem {
                seq,
                pst_item_id,
                pst_folder_id: folder.id,
                folder_path: folder.path.clone(),
                content,
            };
            // Part 02 §6.3 rule 2: attempts of this item, counted before any libpff call on it
            let pending = p.pending.as_ref().filter(|r| r.seq == seq);
            let raw = match pending {
                Some(r) if r.attempts >= 2 => {
                    // died twice on this item: do not touch it
                    base(r.pst_item_id, RawContent::Unreadable { stage: ImportErrorStage::Parse, message: POISON_MESSAGE.to_owned() })
                }
                _ => {
                    let attempts = pending.map_or(1, |r| r.attempts + 1);
                    let mark = |pst_item_id: i64| {
                        if let Some(marker) = &p.inflight {
                            marker.write(&InflightRecord { job_id: p.job_id, seq, folder_path: folder.path.clone(), pst_item_id, attempts });
                        }
                    };
                    mark(pending.map_or(0, |r| r.pst_item_id));
                    match folder.item.sub_message(index).and_then(|m| Ok((i64::from(m.identifier()?), m))) {
                        Err(e) => base(0, RawContent::Unreadable { stage: ImportErrorStage::Parse, message: e.message }),
                        Ok((id, message)) => {
                            mark(id); // same attempt, now with the PST item id for a later error row
                            let content = read_mail(&message)
                                .unwrap_or_else(|e| RawContent::Unreadable { stage: ImportErrorStage::Parse, message: e.message });
                            base(id, content)
                        }
                    }
                }
            };
            if !send_item(&p, &tx, raw) {
                return;
            }
        }
    }
}

/// Spawns the reader std::thread; it owns the pff::File. Messages arrive on a sync_channel(256).
pub fn spawn(params: ReaderParams) -> (Receiver<ReaderMsg>, JoinHandle<()>) {
    let (tx, rx) = sync_channel(CHANNEL);
    let handle = std::thread::Builder::new().name("pst-reader".into()).spawn(move || run(params, tx)).expect("spawn reader thread");
    (rx, handle)
}

/// Test entry point after the reader (feature test-support): same protocol, items from memory.
#[cfg(feature = "test-support")]
pub fn spawn_synthetic(params: ReaderParams, folders: Vec<FolderIn>, items: Vec<RawItem>) -> (Receiver<ReaderMsg>, JoinHandle<()>) {
    let (tx, rx) = sync_channel(CHANNEL);
    let handle = std::thread::Builder::new()
        .name("synthetic-reader".into())
        .spawn(move || {
            let items_total = items.len() as i64;
            if tx.send(ReaderMsg::Scanned(ScanResult { folders, items_total, scan_errors: Vec::new() })).is_err() {
                return;
            }
            for raw in items.into_iter().filter(|r| r.seq > params.cursor_seq) {
                if params.cancel.load(Ordering::SeqCst) || !send_item(&params, &tx, raw) {
                    return;
                }
            }
        })
        .expect("spawn synthetic reader");
    (rx, handle)
}

/// Deterministic list of (seq, pst_item_id, folder path) without reading message properties (tests).
pub fn walk_ids(path: &Path) -> Result<Vec<(i64, i64, String)>, pff::Error> {
    let file = File::open(path)?;
    let (walk, _) = scan(&file)?;
    let mut out = Vec::new();
    for folder in &walk {
        for index in 0..folder.messages {
            let id = i64::from(folder.item.sub_message(index)?.identifier()?);
            out.push((out.len() as i64 + 1, id, folder.path.clone()));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mail_class_filter() {
        for c in ["IPM.Note", "ipm.note.SMIME", "IPM.Schedule.Meeting.Request", "REPORT.IPM.Note.NDR"] {
            assert!(is_mail_class(c), "{c}");
        }
        for c in ["IPM.Contact", "IPM.Appointment", "IPM.Task", "IPM.StickyNote", "IPM", ""] {
            assert!(!is_mail_class(c), "{c}");
        }
    }

    #[test]
    fn filetime_to_system_time() {
        // 2023-01-01T00:00:00Z = 133170048000000000 in 100 ns units since 1601
        let t = filetime(Some(133_170_048_000_000_000)).unwrap();
        assert_eq!(crate::importer::dedupe::format_utc_seconds(t), "2023-01-01T00:00:00Z");
        assert_eq!(filetime(Some(0)), None);
        assert_eq!(filetime(None), None);
        let old = filetime(Some(100_000_000_000_000_000)).unwrap(); // 1917
        assert!(old < UNIX_EPOCH);
    }

    #[test]
    fn slash_in_folder_name() {
        assert_eq!(display_segment("A/B"), "A\u{2215}B");
    }
}
```

Run: `cargo test -p outlooker-core --features test-support --lib importer::pst_reader`.
Expected: `mail_class_filter ... ok`, `filetime_to_system_time ... ok`, `slash_in_folder_name ... ok`; `test result: ok. 3 passed`.

- [ ] **Step 5: Write the queue and the job runner.** Replace `src-tauri/crates/outlooker-core/src/importer/queue.rs` with (job SQL, status derivation, commands, error paging, worker, section "runner"):

```rust
//! Durable job queue (spec Part 03 §7.1 state machine) and, in section "runner", the import / removal job runner.
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use tokio::sync::{Notify, broadcast, oneshot, watch};
use tokio_postgres::error::SqlState;

use crate::db::indexes::{drop_search_indexes, ensure_search_indexes};
use crate::db::{Db, RFC3339_UTC_FMT, pg_err};
use crate::dto::app::{AppPhase, LibraryCounts};
use crate::dto::error::AppError;
use crate::dto::events::{ImportStatusChanged, LibraryChanged};
use crate::dto::import::{
    EnqueueResult, ImportErrorEntry, ImportErrorPage, ImportErrorSeverity, ImportErrorStage, ImportErrorsRequest, ImportPhase,
    ImportProgress, ImportStatus, PstSource,
};
use crate::gate::{Activity, Gate};
use crate::importer::inflight::Inflight;
use crate::importer::normalize::{NormalizeCtx, normalize};
use crate::importer::progress::{BYTE_BUDGET, ByteBudget, Throughput};
use crate::importer::pst_reader::{self, ReaderMsg, ReaderParams};
use crate::importer::types::{FolderIn, ItemError, RawItem};
use crate::importer::writer::{AFTER_JOB_VACUUM, Checkpoint, Writer, WriterConfig, load_known_hashes, stage_db, upsert_folders};
use crate::importer::{ImportTuning, RecoveryReport, remove};
use crate::paths::Paths;
use crate::sink::{CoreEvent, EventSink};

pub const MAX_ENQUEUE_PATHS: usize = 100;
pub const ERRORS_DEFAULT_LIMIT: u32 = 100;
pub const ERRORS_MAX_LIMIT: u32 = 500;
/// How long `pause_for_db` waits for the Supervisor to leave `Ready` after a DB error (Part 02 §6.1 PG crash row).
pub const PAUSE_DETECT: Duration = Duration::from_secs(30);
const PST_MAGIC: &[u8; 4] = b"!BDN";
const DISPLAY_NAME_MAX: usize = 512;

// ------------------------------------------------------------------------------------------------ state

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CancelReason {
    User,
    Shutdown,
    Pause,
}

/// Cancel state of the running job. `flag` is what the reader, the byte budget and the pipeline see.
pub(crate) struct JobCancel {
    flag: Arc<AtomicBool>,
    reason: Mutex<Option<CancelReason>>,
    budget: Arc<ByteBudget>,
}

impl JobCancel {
    fn new() -> JobCancel {
        JobCancel { flag: Arc::new(AtomicBool::new(false)), reason: Mutex::new(None), budget: Arc::new(ByteBudget::new(BYTE_BUDGET)) }
    }
    /// User and Shutdown are final; Pause only applies when nothing stronger is set.
    fn cancel(&self, why: CancelReason) {
        let mut r = self.reason.lock().unwrap_or_else(|p| p.into_inner());
        if matches!(*r, None | Some(CancelReason::Pause)) {
            *r = Some(why);
        }
        drop(r);
        self.stop();
    }
    /// Stops reader and pipeline without a reason (writer stopped, crash seam).
    fn stop(&self) {
        self.flag.store(true, Ordering::SeqCst);
        self.budget.wake_all();
    }
    fn reason(&self) -> Option<CancelReason> {
        *self.reason.lock().unwrap_or_else(|p| p.into_inner())
    }
    /// Before a re-run after a DB pause.
    fn clear_pause(&self) {
        let mut r = self.reason.lock().unwrap_or_else(|p| p.into_inner());
        if *r == Some(CancelReason::Pause) {
            *r = None;
        }
        self.flag.store(false, Ordering::SeqCst);
    }
}

#[derive(Clone)]
struct RunningJob {
    job_id: i64,
    pst_id: i64,
    cancel: Arc<JobCancel>,
}

pub(crate) struct Inner {
    db: Db,
    gate: Gate,
    paths: Paths,
    sink: Arc<dyn EventSink>,
    phase: watch::Receiver<AppPhase>,
    tuning: ImportTuning,
    kick: Notify,
    worker: Mutex<Option<tokio::task::JoinHandle<()>>>,
    shutdown: watch::Sender<bool>,
    running: Mutex<Option<RunningJob>>,
    throughput: Mutex<Throughput>,
    hold_used: AtomicBool,
    pub(crate) events: broadcast::Sender<(i64, ImportStatus)>,
}

impl Inner {
    pub(crate) fn new(db: Db, gate: Gate, paths: Paths, sink: Arc<dyn EventSink>, phase: watch::Receiver<AppPhase>, tuning: ImportTuning) -> Inner {
        Inner {
            db,
            gate,
            paths,
            sink,
            phase,
            tuning,
            kick: Notify::new(),
            worker: Mutex::new(None),
            shutdown: watch::Sender::new(false),
            running: Mutex::new(None),
            throughput: Mutex::new(Throughput::new()),
            hold_used: AtomicBool::new(false),
            events: broadcast::Sender::new(64),
        }
    }
    fn running(&self) -> Option<RunningJob> {
        self.running.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }
    fn set_running(&self, r: Option<RunningJob>) {
        *self.running.lock().unwrap_or_else(|p| p.into_inner()) = r;
    }
    fn shutting_down(&self) -> bool {
        *self.shutdown.borrow()
    }
    async fn shutdown_signal(&self) {
        let mut rx = self.shutdown.subscribe();
        let _ = rx.wait_for(|s| *s).await;
    }
    /// The inflight marker of the current data root (the root can move between jobs, Part 02 §8.3).
    fn inflight(&self) -> Result<Inflight, AppError> {
        Ok(Inflight::new(self.paths.with_data_root(self.db.data_root()?).inflight_marker()))
    }
}

fn pool_err(e: deadpool_postgres::PoolError) -> AppError {
    match e {
        deadpool_postgres::PoolError::Closed | deadpool_postgres::PoolError::Backend(_) => AppError::DbNotReady,
        other => AppError::internal(format!("pool: {other}")),
    }
}

async fn app(inner: &Inner) -> Result<deadpool_postgres::Object, AppError> {
    inner.db.app_pool()?.get().await.map_err(pool_err)
}

/// DB error → AppError: no SQLSTATE (closed / I/O) → DbNotReady, else `pg_err`.
fn db_err(e: tokio_postgres::Error) -> AppError {
    if e.as_db_error().is_none() { AppError::DbNotReady } else { pg_err(e) }
}

// ------------------------------------------------------------------------------------------------ status

/// Part 04 §2.1 table.
pub fn status_of(kind: &str, state: &str) -> ImportStatus {
    match (kind, state) {
        ("remove", "failed") => ImportStatus::Failed,
        ("remove", _) => ImportStatus::Removing,
        (_, "queued") => ImportStatus::Queued,
        (_, "running") => ImportStatus::Running,
        (_, "cancelling") => ImportStatus::Cancelling,
        (_, "cancelled") => ImportStatus::Cancelled,
        (_, "failed") => ImportStatus::Failed,
        _ => ImportStatus::Completed,
    }
}

fn phase_of(s: &str) -> ImportPhase {
    match s {
        "scanning" => ImportPhase::Scanning,
        "importing" => ImportPhase::Importing,
        _ => ImportPhase::Finalizing,
    }
}

const LIST_SQL: &str = "SELECT s.id, s.path, s.display_name, s.file_size_bytes, j.kind, j.state, j.id, \
       to_char(j.created_at AT TIME ZONE 'UTC', $1), to_char(j.finished_at AT TIME ZONE 'UTC', $1), \
       c.phase, c.items_total, c.stored, c.duplicates, c.errors, c.skipped_non_mail, c.images_converted, \
       (SELECT count(*) FROM import_errors e WHERE e.pst_id = s.id) \
  FROM pst_sources s \
  CROSS JOIN LATERAL (SELECT kind, state, id, created_at, finished_at FROM import_jobs j \
                      WHERE j.pst_id = s.id ORDER BY j.id DESC LIMIT 1) j \
  LEFT JOIN import_checkpoints c ON c.pst_id = s.id \
 WHERE $2::bigint IS NULL OR s.id = $2";

pub(crate) async fn list(inner: &Inner, only: Option<i64>) -> Result<Vec<PstSource>, AppError> {
    let c = app(inner).await?;
    let rows = c.query(LIST_SQL, &[&RFC3339_UTC_FMT, &only]).await.map_err(db_err)?;
    let running = inner.running();
    let mut out: Vec<(u8, i64, PstSource)> = rows
        .iter()
        .map(|r| {
            let id: i64 = r.get(0);
            let status = status_of(r.get(4), r.get(5));
            let job_id: i64 = r.get(6);
            let phase: Option<&str> = r.get(9);
            let progress = phase.map(|p| {
                let (stored, duplicates, errors, skipped_non_mail): (i64, i64, i64, i64) = (r.get(11), r.get(12), r.get(13), r.get(14));
                let items_total: Option<i64> = r.get(10);
                let items_done = stored + duplicates + errors + skipped_non_mail;
                let live = running.as_ref().is_some_and(|j| j.pst_id == id) && status == ImportStatus::Running;
                let tp = inner.throughput.lock().unwrap_or_else(|p| p.into_inner());
                ImportProgress {
                    pst_id: id,
                    phase: phase_of(p),
                    items_total,
                    items_done,
                    stored,
                    duplicates,
                    errors,
                    skipped_non_mail,
                    images_converted: r.get(15),
                    bytes_per_sec: if live { tp.bytes_per_sec() } else { None },
                    eta_seconds: if live { tp.eta_seconds(items_total, items_done) } else { None },
                }
            });
            let rank = match status {
                ImportStatus::Running | ImportStatus::Cancelling => 0,
                ImportStatus::Queued | ImportStatus::Removing => 1,
                _ => 2,
            };
            let src = PstSource {
                id,
                path: r.get(1),
                display_name: r.get(2),
                file_size_bytes: r.get(3),
                status,
                message_count: progress.as_ref().map_or(0, |p| p.stored + p.duplicates),
                new_message_count: progress.as_ref().map_or(0, |p| p.stored),
                progress,
                error_count: r.get(16),
                queued_at: r.get(7),
                finished_at: r.get(8),
            };
            (rank, job_id, src)
        })
        .collect();
    // running first; queued / removing in queue order; the rest newest first
    out.sort_by(|a, b| {
        a.0.cmp(&b.0).then_with(|| if a.0 == 2 { (&b.2.queued_at, b.1).cmp(&(&a.2.queued_at, a.1)) } else { a.1.cmp(&b.1) })
    });
    Ok(out.into_iter().map(|(_, _, s)| s).collect())
}

async fn source(inner: &Inner, pst_id: i64) -> Option<PstSource> {
    list(inner, Some(pst_id)).await.ok()?.pop()
}

async fn emit_status(inner: &Inner, pst_id: i64) {
    if let Some(src) = source(inner, pst_id).await {
        inner.sink.emit(CoreEvent::ImportStatusChanged(ImportStatusChanged { pst_id, status: src.status, removed: false, source: Some(src) }));
    }
}

async fn library_counts(inner: &Inner) -> Result<LibraryCounts, AppError> {
    let c = app(inner).await?;
    let r = c.query_one("SELECT (SELECT count(*) FROM messages), (SELECT count(*) FROM pst_sources)", &[]).await.map_err(db_err)?;
    Ok(LibraryCounts { messages: r.get(0), psts: r.get(1) })
}

async fn emit_library(inner: &Inner) {
    if let Ok(counts) = library_counts(inner).await {
        inner.sink.emit(CoreEvent::LibraryChanged(LibraryChanged { counts }));
    }
}

fn progress_event(cp: &Checkpoint, tp: Option<&Throughput>) -> ImportProgress {
    ImportProgress {
        pst_id: cp.pst_id,
        phase: cp.phase,
        items_total: cp.items_total,
        items_done: cp.items_done(),
        stored: cp.stored,
        duplicates: cp.duplicates,
        errors: cp.errors,
        skipped_non_mail: cp.skipped_non_mail,
        images_converted: cp.images_converted,
        bytes_per_sec: tp.and_then(Throughput::bytes_per_sec),
        eta_seconds: tp.and_then(|t| t.eta_seconds(cp.items_total, cp.items_done())),
    }
}

// ------------------------------------------------------------------------------------------------ commands

fn trunc_micros(t: SystemTime) -> SystemTime {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => UNIX_EPOCH + Duration::from_micros(d.as_micros() as u64),
        Err(e) => UNIX_EPOCH - Duration::from_micros(e.duration().as_micros() as u64),
    }
}

/// Part 03 §7.2 path checks. Returns (size, mtime truncated to whole microseconds).
pub fn check_file(path: &str) -> Result<(i64, SystemTime), AppError> {
    let p = Path::new(path);
    let fa = |reason: &str| AppError::FileAccess { path: path.to_owned(), reason: reason.to_owned() };
    if !p.is_absolute() {
        return Err(fa("notAbsolute"));
    }
    let meta = std::fs::metadata(p).map_err(|_| fa("notFound"))?;
    if !meta.is_file() {
        return Err(fa("notAFile"));
    }
    let mut magic = [0u8; 4];
    std::fs::File::open(p).and_then(|mut f| f.read_exact(&mut magic)).map_err(|_| fa("unreadable"))?;
    if &magic != PST_MAGIC {
        return Err(AppError::NotAPst { path: path.to_owned() });
    }
    let mtime = meta.modified().map_err(|_| fa("unreadable"))?;
    Ok((meta.len() as i64, trunc_micros(mtime)))
}

fn display_name(path: &str) -> String {
    let p = Path::new(path);
    let stem = p.file_stem().or(p.file_name()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let stem = if stem.is_empty() { "pst".to_owned() } else { stem };
    stem.chars().take(DISPLAY_NAME_MAX).collect()
}

/// Source row + queued import job in one transaction. Err(AlreadyImported) when the identity exists.
async fn insert_source(inner: &Inner, path: &str, size: i64, mtime: SystemTime) -> Result<i64, AppError> {
    let mut c = app(inner).await?;
    let tx = c.transaction().await.map_err(db_err)?;
    let row = tx
        .query_opt(
            "INSERT INTO pst_sources (path, display_name, file_size_bytes, file_mtime) VALUES ($1, $2, $3, $4) \
             ON CONFLICT ON CONSTRAINT pst_sources_same_file_once DO NOTHING RETURNING id",
            &[&path, &display_name(path), &size, &mtime],
        )
        .await
        .map_err(db_err)?;
    let Some(row) = row else {
        let existing: i64 = tx
            .query_one("SELECT id FROM pst_sources WHERE path = $1 AND file_size_bytes = $2 AND file_mtime = $3", &[&path, &size, &mtime])
            .await
            .map_err(db_err)?
            .get(0);
        return Err(AppError::AlreadyImported { pst_id: existing });
    };
    let pst_id: i64 = row.get(0);
    tx.execute("INSERT INTO import_jobs (pst_id, kind, state) VALUES ($1, 'import', 'queued')", &[&pst_id]).await.map_err(db_err)?;
    tx.commit().await.map_err(db_err)?;
    Ok(pst_id)
}

pub(crate) async fn enqueue(inner: &Inner, paths: Vec<String>) -> Result<Vec<EnqueueResult>, AppError> {
    if paths.is_empty() {
        return Err(AppError::invalid("paths", "empty"));
    }
    if paths.len() > MAX_ENQUEUE_PATHS {
        return Err(AppError::invalid("paths", "tooMany"));
    }
    let mut out = Vec::with_capacity(paths.len());
    for path in paths {
        let result = match check_file(&path) {
            Err(error) => EnqueueResult::Rejected { path, error },
            Ok((size, mtime)) => match insert_source(inner, &path, size, mtime).await {
                Err(e @ AppError::AlreadyImported { .. }) => EnqueueResult::Rejected { path, error: e },
                Err(e) => return Err(e),
                Ok(pst_id) => {
                    emit_status(inner, pst_id).await;
                    let pst = source(inner, pst_id).await.ok_or_else(|| AppError::internal("enqueued source vanished"))?;
                    EnqueueResult::Queued { pst }
                }
            },
        };
        out.push(result);
    }
    inner.kick.notify_one();
    Ok(out)
}

/// Newest job of a PST, locked for the rest of the transaction. None when the PST does not exist.
async fn newest_job_for_update(tx: &tokio_postgres::Transaction<'_>, pst_id: i64) -> Result<Option<(i64, String, String)>, AppError> {
    let exists = tx.query_opt("SELECT 1 FROM pst_sources WHERE id = $1 FOR UPDATE", &[&pst_id]).await.map_err(db_err)?;
    if exists.is_none() {
        return Ok(None);
    }
    let row = tx
        .query_opt("SELECT id, kind, state FROM import_jobs WHERE pst_id = $1 ORDER BY id DESC LIMIT 1 FOR UPDATE", &[&pst_id])
        .await
        .map_err(db_err)?;
    Ok(row.map(|r| (r.get(0), r.get(1), r.get(2))))
}

pub(crate) async fn cancel(inner: &Inner, pst_id: i64) -> Result<(), AppError> {
    let mut c = app(inner).await?;
    let tx = c.transaction().await.map_err(db_err)?;
    let Some((job_id, kind, state)) = newest_job_for_update(&tx, pst_id).await? else {
        return Err(AppError::not_found("pst", pst_id));
    };
    match (kind.as_str(), state.as_str()) {
        ("import", "queued") => {
            tx.execute("UPDATE import_jobs SET state = 'cancelled', finished_at = now() WHERE id = $1", &[&job_id]).await.map_err(db_err)?;
        }
        ("import", "running") => {
            tx.execute("UPDATE import_jobs SET state = 'cancelling' WHERE id = $1", &[&job_id]).await.map_err(db_err)?;
        }
        _ => return Err(AppError::invalid("pstId", "notCancellable")),
    }
    tx.commit().await.map_err(db_err)?;
    if let Some(r) = inner.running().filter(|r| r.job_id == job_id) {
        r.cancel.cancel(CancelReason::User);
    }
    emit_status(inner, pst_id).await;
    Ok(())
}

pub(crate) async fn resume(inner: &Inner, pst_id: i64) -> Result<(), AppError> {
    let mut c = app(inner).await?;
    let tx = c.transaction().await.map_err(db_err)?;
    let Some((_, kind, state)) = newest_job_for_update(&tx, pst_id).await? else {
        return Err(AppError::not_found("pst", pst_id));
    };
    if kind != "import" || !matches!(state.as_str(), "cancelled" | "failed") {
        return Err(AppError::invalid("pstId", "notResumable"));
    }
    let path: String = tx.query_one("SELECT path FROM pst_sources WHERE id = $1", &[&pst_id]).await.map_err(db_err)?.get(0);
    check_file(&path)?;
    tx.execute("INSERT INTO import_jobs (pst_id, kind, state) VALUES ($1, 'import', 'queued')", &[&pst_id]).await.map_err(db_err)?;
    tx.commit().await.map_err(db_err)?;
    emit_status(inner, pst_id).await;
    inner.kick.notify_one();
    Ok(())
}

pub(crate) async fn remove(inner: &Inner, pst_id: i64) -> Result<(), AppError> {
    let mut c = app(inner).await?;
    let tx = c.transaction().await.map_err(db_err)?;
    let Some((job_id, kind, state)) = newest_job_for_update(&tx, pst_id).await? else {
        return Err(AppError::not_found("pst", pst_id));
    };
    match status_of(&kind, &state) {
        ImportStatus::Running | ImportStatus::Cancelling => return Err(AppError::Busy { reason: crate::dto::error::BusyReason::ImportRunning }),
        ImportStatus::Removing => return Err(AppError::Busy { reason: crate::dto::error::BusyReason::PstRemoving }),
        _ => {}
    }
    if kind == "import" && state == "queued" {
        // required by import_jobs_one_active_per_pst (Part 04 §4.3)
        tx.execute("UPDATE import_jobs SET state = 'cancelled', finished_at = now() WHERE id = $1", &[&job_id]).await.map_err(db_err)?;
    }
    tx.execute("INSERT INTO import_jobs (pst_id, kind, state) VALUES ($1, 'remove', 'queued')", &[&pst_id]).await.map_err(db_err)?;
    tx.commit().await.map_err(db_err)?;
    emit_status(inner, pst_id).await;
    inner.kick.notify_one();
    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ErrorCursor {
    t: String,
    i: i64,
}

pub(crate) async fn errors(inner: &Inner, req: ImportErrorsRequest) -> Result<ImportErrorPage, AppError> {
    let limit = req.limit.unwrap_or(ERRORS_DEFAULT_LIMIT);
    if !(1..=ERRORS_MAX_LIMIT).contains(&limit) {
        return Err(AppError::invalid("limit", "outOfRange"));
    }
    let bad_cursor = || AppError::invalid("cursor", "invalid");
    let (after_t, after_i) = match &req.cursor {
        None => ("-infinity".to_owned(), 0i64),
        Some(s) => {
            let bytes = URL_SAFE_NO_PAD.decode(s).map_err(|_| bad_cursor())?;
            let c: ErrorCursor = serde_json::from_slice(&bytes).map_err(|_| bad_cursor())?;
            (c.t, c.i)
        }
    };
    let c = app(inner).await?;
    if c.query_opt("SELECT 1 FROM pst_sources WHERE id = $1", &[&req.pst_id]).await.map_err(db_err)?.is_none() {
        return Err(AppError::not_found("pst", req.pst_id));
    }
    let rows = c
        .query(
            "SELECT id, to_char(occurred_at AT TIME ZONE 'UTC', $5), occurred_at::text, folder_path, pst_item_id, stage, severity, message \
               FROM import_errors WHERE pst_id = $1 AND (occurred_at, id) > ($2::text::timestamptz, $3) ORDER BY occurred_at, id LIMIT $4",
            &[&req.pst_id, &after_t, &after_i, &(i64::from(limit) + 1), &RFC3339_UTC_FMT],
        )
        .await
        .map_err(|e| match e.code() {
            Some(c) if *c == SqlState::INVALID_DATETIME_FORMAT || *c == SqlState::DATETIME_FIELD_OVERFLOW => bad_cursor(),
            _ => db_err(e),
        })?;
    let stage = |s: &str| match s {
        "open" => ImportErrorStage::Open,
        "scan" => ImportErrorStage::Scan,
        "parse" => ImportErrorStage::Parse,
        "html" => ImportErrorStage::Html,
        "image" => ImportErrorStage::Image,
        _ => ImportErrorStage::Store,
    };
    let severity = |s: &str| match s {
        "item_skipped" => ImportErrorSeverity::ItemSkipped,
        "degraded" => ImportErrorSeverity::Degraded,
        _ => ImportErrorSeverity::JobFailed,
    };
    let more = rows.len() > limit as usize;
    let page: Vec<_> = rows.iter().take(limit as usize).collect();
    let next_cursor = more.then(|| {
        let last = page.last().expect("limit >= 1");
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&ErrorCursor { t: last.get(2), i: last.get(0) }).expect("cursor serializes"))
    });
    Ok(ImportErrorPage {
        rows: page
            .iter()
            .map(|r| ImportErrorEntry {
                id: r.get(0),
                occurred_at: r.get(1),
                folder_path: r.get(3),
                pst_item_id: r.get(4),
                stage: stage(r.get(5)),
                severity: severity(r.get(6)),
                message: r.get(7),
            })
            .collect(),
        next_cursor,
    })
}

pub(crate) async fn recover_after_crash(inner: &Inner) -> Result<RecoveryReport, AppError> {
    let c = app(inner).await?;
    let rows = c
        .query(
            "UPDATE import_jobs SET state = CASE state WHEN 'running' THEN 'queued' ELSE 'cancelled' END, \
                    finished_at = CASE state WHEN 'cancelling' THEN now() END \
              WHERE state IN ('running', 'cancelling') RETURNING pst_id, state",
            &[],
        )
        .await
        .map_err(db_err)?;
    let mut report = RecoveryReport::default();
    for r in &rows {
        let pst_id: i64 = r.get(0);
        if r.get::<_, &str>(1) == "queued" { report.requeued.push(pst_id) } else { report.cancelled.push(pst_id) }
    }
    report.requeued.sort_unstable();
    report.cancelled.sort_unstable();
    for pst_id in report.requeued.iter().chain(&report.cancelled) {
        emit_status(inner, *pst_id).await;
    }
    Ok(report)
}

pub(crate) async fn self_heal_indexes(inner: &Inner) -> Result<(), AppError> {
    let _guard = inner.gate.try_enter(Activity::Import)?;
    let admin = inner.db.connect_admin().await?;
    ensure_search_indexes(&admin, &|name: &str, done: usize, total: usize| log::info!("self-heal index {name} ({done}/{total})")).await
}

pub(crate) fn start_worker(inner: &Arc<Inner>) {
    let mut slot = inner.worker.lock().unwrap_or_else(|p| p.into_inner());
    if slot.is_none() {
        *slot = Some(tokio::spawn(worker(inner.clone())));
    }
}

pub(crate) async fn shutdown(inner: &Inner, timeout: Duration) -> Result<(), AppError> {
    inner.shutdown.send_replace(true);
    if let Some(r) = inner.running() {
        r.cancel.cancel(CancelReason::Shutdown);
    }
    let handle = inner.worker.lock().unwrap_or_else(|p| p.into_inner()).take();
    let Some(handle) = handle else { return Ok(()) };
    match tokio::time::timeout(timeout, handle).await {
        Ok(_) => Ok(()),
        Err(_) => Err(AppError::internal("import shutdown timed out")),
    }
}

#[cfg(feature = "test-support")]
static SYNTHETIC: std::sync::LazyLock<Mutex<std::collections::HashMap<String, (Vec<FolderIn>, Vec<RawItem>)>>> =
    std::sync::LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

#[cfg(feature = "test-support")]
fn synthetic_items(path: &str) -> Option<(Vec<FolderIn>, Vec<RawItem>)> {
    SYNTHETIC.lock().unwrap_or_else(|p| p.into_inner()).get(path).cloned()
}
#[cfg(not(feature = "test-support"))]
fn synthetic_items(_path: &str) -> Option<(Vec<FolderIn>, Vec<RawItem>)> {
    None
}

#[cfg(feature = "test-support")]
pub(crate) async fn enqueue_synthetic(inner: &Inner, path: &str, folders: Vec<FolderIn>, items: Vec<RawItem>) -> Result<i64, AppError> {
    let (size, mtime) = check_file(path).unwrap_or((1, trunc_micros(SystemTime::now())));
    SYNTHETIC.lock().unwrap_or_else(|p| p.into_inner()).insert(path.to_owned(), (folders, items));
    let pst_id = insert_source(inner, path, size, mtime).await?;
    emit_status(inner, pst_id).await;
    inner.kick.notify_one();
    Ok(pst_id)
}

// ------------------------------------------------------------------------------------------------ runner

/// Why a job attempt failed the job (one `job_failed` error row).
struct JobFailure {
    stage: ImportErrorStage,
    message: String,
}

enum AttemptEnd {
    /// Input exhausted or cancelled (user / shutdown): writer finished, checkpoint committed.
    Finished,
    /// DB went away (writer DbNotReady or phase change): uncommitted batch dropped.
    Paused,
    /// Test seam crash_after_batches.
    Crashed,
    Failed(JobFailure),
}

enum Next {
    Job { id: i64, pst_id: i64, kind: String },
    Idle,
}

async fn worker(inner: Arc<Inner>) {
    let mut phase = inner.phase.clone();
    loop {
        if inner.shutting_down() {
            return;
        }
        if *phase.borrow_and_update() != AppPhase::Ready {
            tokio::select! {
                r = phase.changed() => if r.is_err() { return },
                _ = inner.shutdown_signal() => return,
            }
            continue;
        }
        let kicked = inner.kick.notified();
        tokio::pin!(kicked);
        kicked.as_mut().enable(); // a kick during the query below is not lost
        let next = next_job(&inner).await;
        match next {
            Ok(Next::Job { id, pst_id, kind }) => {
                let activity = if kind == "remove" { Activity::Removal } else { Activity::Import };
                let guard = match inner.gate.try_enter(activity) {
                    Ok(g) => g,
                    Err(_) => {
                        tokio::select! {
                            _ = inner.gate.released() => {},
                            _ = inner.shutdown_signal() => return,
                        }
                        continue;
                    }
                };
                let crashed = run_job(&inner, id, pst_id, &kind).await;
                drop(guard);
                if crashed {
                    return; // simulated process kill: the worker dies with the job
                }
                continue;
            }
            Ok(Next::Idle) => {}
            Err(e) => log::warn!("import queue: {e}"),
        }
        tokio::select! {
            _ = kicked => {},
            r = phase.changed() => if r.is_err() { return },
            _ = inner.shutdown_signal() => return,
        }
    }
}

async fn next_job(inner: &Inner) -> Result<Next, AppError> {
    let c = app(inner).await?;
    let row = c
        .query_opt("SELECT id, pst_id, kind FROM import_jobs WHERE state = 'queued' ORDER BY id LIMIT 1", &[])
        .await
        .map_err(db_err)?;
    Ok(row.map_or(Next::Idle, |r| Next::Job { id: r.get(0), pst_id: r.get(1), kind: r.get(2) }))
}

/// Takes the job (`queued` → `running`) and runs it. Returns true when the crash seam dropped it.
async fn run_job(inner: &Arc<Inner>, job_id: i64, pst_id: i64, kind: &str) -> bool {
    let cancel = Arc::new(JobCancel::new());
    // registered before the row turns `running`, so a cancel that sees `running` finds the flag
    inner.set_running(Some(RunningJob { job_id, pst_id, cancel: cancel.clone() }));
    *inner.throughput.lock().unwrap_or_else(|p| p.into_inner()) = Throughput::new();
    let taken = match app(inner).await {
        Ok(c) => c
            .execute("UPDATE import_jobs SET state = 'running', started_at = now() WHERE id = $1 AND state = 'queued'", &[&job_id])
            .await
            .map_err(db_err),
        Err(e) => Err(e),
    };
    let crashed = match taken {
        Ok(1) => {
            emit_status(inner, pst_id).await;
            if kind == "remove" {
                run_removal(inner, job_id, pst_id).await;
                false
            } else {
                run_import(inner, job_id, pst_id, &cancel).await
            }
        }
        Ok(_) => false, // cancelled or removed meanwhile
        Err(e) => {
            log::warn!("cannot start job {job_id}: {e}");
            false
        }
    };
    inner.set_running(None);
    crashed
}

async fn run_removal(inner: &Arc<Inner>, job_id: i64, pst_id: i64) {
    loop {
        let result = match app(inner).await {
            Ok(c) => remove::run(&c, pst_id).await,
            Err(e) => Err(e),
        };
        match result {
            Ok(stats) => {
                log::info!("removed pst {pst_id}: {stats:?}");
                inner.sink.emit(CoreEvent::ImportStatusChanged(ImportStatusChanged { pst_id, status: ImportStatus::Removing, removed: true, source: None }));
                emit_library(inner).await;
                let _ = inner.events.send((pst_id, ImportStatus::Removing));
                return;
            }
            Err(AppError::DbNotReady) => {
                if !pause_wait(inner).await {
                    return; // row stays `running`; recover_after_crash re-queues it (idempotent removal)
                }
            }
            Err(e) => {
                let failure = JobFailure { stage: ImportErrorStage::Store, message: e.to_string() };
                if set_final(inner, job_id, pst_id, "failed", Some(&failure)).await.is_ok() {
                    emit_status(inner, pst_id).await;
                    let _ = inner.events.send((pst_id, ImportStatus::Failed));
                }
                return;
            }
        }
    }
}

/// Final job row state (+ the job_failed error row in the same transaction).
async fn set_final(inner: &Inner, job_id: i64, pst_id: i64, state: &str, failure: Option<&JobFailure>) -> Result<(), AppError> {
    let mut c = app(inner).await?;
    let tx = c.transaction().await.map_err(db_err)?;
    if let Some(f) = failure {
        let e = ItemError::new(f.stage, ImportErrorSeverity::JobFailed, "", 0, &f.message);
        tx.execute(
            "INSERT INTO import_errors (pst_id, job_id, stage, severity, message) VALUES ($1, $2, $3, 'job_failed', $4)",
            &[&pst_id, &job_id, &stage_db(f.stage), &e.message],
        )
        .await
        .map_err(db_err)?;
    }
    tx.execute(
        "UPDATE import_jobs SET state = $2, finished_at = CASE WHEN $2 = 'queued' THEN NULL ELSE now() END WHERE id = $1",
        &[&job_id, &state],
    )
    .await
    .map_err(db_err)?;
    tx.commit().await.map_err(db_err)
}

/// pause_for_db wait (Part 02 §6.1 PG crash row): DB reachable → resume at once; else wait up to 30 s for the
/// phase to leave `Ready`, then for `Ready` again (also after `Failed`: app_retry resumes the job in this process).
/// false = shutdown: the job row stays `running` and `recover_after_crash` re-queues it at the next start.
async fn pause_wait(inner: &Inner) -> bool {
    if let Ok(c) = app(inner).await {
        if c.simple_query("SELECT 1").await.is_ok() {
            return true;
        }
    }
    let mut ph = inner.phase.clone();
    if *ph.borrow() == AppPhase::Ready {
        let _ = tokio::time::timeout(PAUSE_DETECT, ph.wait_for(|p| *p != AppPhase::Ready)).await;
    }
    tokio::select! {
        // `Failed` is not a wake-up: the job stays paused and resumes on the next `Ready` (app_retry), Part 02 §6.1
        r = ph.wait_for(|p| *p == AppPhase::Ready) => r.is_ok(),
        _ = inner.shutdown_signal() => false,
    }
}

/// Returns true when the crash seam dropped the job.
async fn run_import(inner: &Arc<Inner>, job_id: i64, pst_id: i64, cancel: &Arc<JobCancel>) -> bool {
    let end = loop {
        let end = attempt(inner, job_id, pst_id, cancel).await.unwrap_or_else(|e| match e {
            AppError::DbNotReady => AttemptEnd::Paused,
            other => AttemptEnd::Failed(JobFailure { stage: ImportErrorStage::Store, message: other.to_string() }),
        });
        match end {
            AttemptEnd::Paused => {
                if let Ok(m) = inner.inflight() {
                    m.clear(); // the reader stopped cleanly: the marker names no poison item
                }
                if !pause_wait(inner).await || cancel.reason() == Some(CancelReason::Shutdown) {
                    return false;
                }
                if cancel.reason() == Some(CancelReason::User) {
                    break AttemptEnd::Finished;
                }
                cancel.clear_pause();
                log::info!("import job {job_id}: database back, resuming from the checkpoint");
            }
            other => break other,
        }
    };
    let failure = match end {
        AttemptEnd::Crashed => {
            let _ = inner.events.send((pst_id, ImportStatus::Running));
            return true;
        }
        AttemptEnd::Failed(f) => Some(f),
        _ => None,
    };
    let reason = cancel.reason();
    if reason == Some(CancelReason::Shutdown) && failure.is_none() {
        // Part 02 §2.5 step 1: back to queued; index self-heal at the next start covers dropped indexes
        if let Ok(m) = inner.inflight() {
            m.clear();
        }
        if set_final(inner, job_id, pst_id, "queued", None).await.is_ok() {
            emit_status(inner, pst_id).await;
            let _ = inner.events.send((pst_id, ImportStatus::Queued));
        }
        return false;
    }
    if let Err(e) = finalize(inner, pst_id).await {
        log::warn!("import job {job_id}: finalize: {e}"); // DB gone: row stays running, recovered at next start
        return false;
    }
    let (state, status) = match (&failure, reason) {
        (Some(_), _) => ("failed", ImportStatus::Failed),
        (None, Some(CancelReason::User)) => ("cancelled", ImportStatus::Cancelled),
        _ => ("completed", ImportStatus::Completed),
    };
    if let Err(e) = set_final(inner, job_id, pst_id, state, failure.as_ref()).await {
        log::warn!("import job {job_id}: final state: {e}");
        return false;
    }
    emit_status(inner, pst_id).await;
    emit_library(inner).await;
    let _ = inner.events.send((pst_id, status));
    false
}

/// Step 9: Finalizing phase, indexes (always; idempotent), after-job VACUUM, marker removed.
async fn finalize(inner: &Inner, pst_id: i64) -> Result<(), AppError> {
    let c = app(inner).await?;
    if Checkpoint::load(&c, pst_id).await?.is_some() {
        Checkpoint::set_phase(&c, pst_id, ImportPhase::Finalizing, None).await?;
        if let Some(cp) = Checkpoint::load(&c, pst_id).await? {
            inner.sink.emit(CoreEvent::ImportProgress(progress_event(&cp, None)));
        }
    }
    let admin = inner.db.connect_admin().await?;
    ensure_search_indexes(&admin, &|name: &str, done: usize, total: usize| log::info!("import finalize index {name} ({done}/{total})")).await?;
    c.batch_execute(AFTER_JOB_VACUUM).await.map_err(db_err)?;
    inner.inflight()?.clear();
    Ok(())
}

/// One run of the pipeline from the checkpoint (steps 1–8).
async fn attempt(inner: &Arc<Inner>, job_id: i64, pst_id: i64, cancel: &Arc<JobCancel>) -> Result<AttemptEnd, AppError> {
    let mut c = app(inner).await?;
    let row = c.query_one("SELECT path, file_size_bytes, file_mtime FROM pst_sources WHERE id = $1", &[&pst_id]).await.map_err(db_err)?;
    let path: String = row.get(0);
    let synthetic = synthetic_items(&path);
    // (1) file identity
    let identity = match std::fs::metadata(&path) {
        Ok(m) => Ok((m.len() as i64, trunc_micros(m.modified().unwrap_or(UNIX_EPOCH)))),
        Err(_) if synthetic.is_some() => Ok((row.get(1), row.get(2))),
        Err(e) => Err(e),
    };
    let (size, mtime) = match identity {
        Ok(v) => v,
        Err(e) => return Ok(AttemptEnd::Failed(JobFailure { stage: ImportErrorStage::Open, message: format!("cannot open PST file: {e}") })),
    };
    // (2) checkpoint
    let (start, reset) = Checkpoint::begin(&c, pst_id, job_id, size, mtime).await?;
    if reset {
        log::info!("pst {pst_id}: file size or mtime changed, import restarts from the beginning");
    }
    // (3) poison item: the marker of this job is the pending record; another job's marker is ignored (Part 02 §6.3 rule 1)
    let inflight = inner.inflight()?;
    // A reset checkpoint (file changed) renumbers the items: the marker's seq may name another item, so drop it.
    let pending = if reset { None } else { inflight.read().filter(|r| r.job_id == job_id) };
    // (4) reader
    let params = ReaderParams {
        path: PathBuf::from(&path),
        cursor_seq: start.cursor_seq,
        pending,
        inflight: Some(inflight),
        cancel: cancel.flag.clone(),
        // test seam: only the first attempt of this manager holds (a re-run after a pause or resume never blocks)
        hold_after: inner.tuning.hold_reader_after.filter(|_| !inner.hold_used.swap(true, Ordering::SeqCst)),
        budget: cancel.budget.clone(),
        job_id,
    };
    let (rx, reader) = match synthetic {
        #[cfg(feature = "test-support")]
        Some((folders, items)) => pst_reader::spawn_synthetic(params, folders, items),
        _ => pst_reader::spawn(params),
    };
    let first = tokio::task::spawn_blocking(move || (rx.recv(), rx)).await.map_err(|e| AppError::internal(e.to_string()))?;
    let (first, rx) = first;
    let scanned = match first {
        Ok(ReaderMsg::Scanned(s)) => s,
        Ok(ReaderMsg::Failed { stage, message }) => {
            join_thread(reader).await;
            return Ok(AttemptEnd::Failed(JobFailure { stage, message }));
        }
        Ok(ReaderMsg::Item(_)) | Err(_) => {
            cancel.stop();
            join_thread(reader).await;
            return Ok(AttemptEnd::Failed(JobFailure { stage: ImportErrorStage::Scan, message: "reader stopped before the scan finished".into() }));
        }
    };
    upsert_folders(&mut c, pst_id, &scanned.folders).await?;
    for e in &scanned.scan_errors {
        c.execute(
            "INSERT INTO import_errors (pst_id, job_id, stage, severity, folder_path, message) VALUES ($1, $2, 'scan', 'degraded', $3, $4)",
            &[&pst_id, &job_id, &e.folder_path, &e.message],
        )
        .await
        .map_err(db_err)?;
    }
    Checkpoint::set_phase(&c, pst_id, ImportPhase::Importing, Some(scanned.items_total)).await?;
    let start = Checkpoint { phase: ImportPhase::Importing, items_total: Some(scanned.items_total), ..start };
    inner.sink.emit(CoreEvent::ImportProgress(progress_event(&start, None)));
    // (5) index policy (Part 04 §6.4)
    let counts = c.query_one("SELECT (SELECT count(*) FROM messages), (SELECT count(*) FROM pst_sources)", &[]).await.map_err(db_err)?;
    let (messages, psts): (i64, i64) = (counts.get(0), counts.get(1));
    let remaining = scanned.items_total - start.cursor_seq;
    if 2 * remaining >= messages {
        let admin = inner.db.connect_admin().await?;
        drop_search_indexes(&admin).await?;
        log::info!("pst {pst_id}: {remaining} items for {messages} messages: search indexes dropped until finalize");
    }
    // (6) writer
    let quality: i16 = c.query_one("SELECT webp_quality FROM settings", &[]).await.map_err(db_err)?.get(0);
    let known = Arc::new(load_known_hashes(&c).await?);
    drop(c);
    let crash = Arc::new(Notify::new());
    let sent_bytes = Arc::new(AtomicU64::new(0));
    let on_commit = {
        let inner = inner.clone();
        let crash = crash.clone();
        let sent_bytes = sent_bytes.clone();
        let start_batches = start.batches_committed;
        let start_stored = start.stored;
        Box::new(move |cp: &Checkpoint| {
            let progress = {
                let mut tp = inner.throughput.lock().unwrap_or_else(|p| p.into_inner());
                tp.record(Instant::now(), cp.items_done(), sent_bytes.load(Ordering::Relaxed));
                progress_event(cp, Some(&tp))
            };
            inner.sink.emit(CoreEvent::ImportProgress(progress));
            if cp.stored > start_stored {
                let counts = LibraryCounts { messages: messages + (cp.stored - start_stored), psts };
                inner.sink.emit(CoreEvent::LibraryChanged(LibraryChanged { counts }));
            }
            if let Some(f) = &inner.tuning.on_batch {
                f(cp.batches_committed);
            }
            if inner.tuning.crash_after_batches.is_some_and(|n| cp.batches_committed - start_batches >= n) {
                crash.notify_one();
            }
        })
    };
    let cfg = WriterConfig { pst_id, data_root: inner.db.data_root()?, start: start.clone(), limits: inner.tuning.writer.clone(), known: known.clone() };
    let writer = Writer::spawn(inner.db.connect_app().await?, cfg, on_commit).await?;
    // (7) pipeline thread: rayon pool runs normalize, results go to the writer
    let ctx = NormalizeCtx { webp_quality: quality.clamp(1, 100) as u8, known };
    let sender = writer.sender();
    let flag = cancel.clone();
    let late_failure: Arc<Mutex<Option<JobFailure>>> = Arc::new(Mutex::new(None));
    let late = late_failure.clone();
    let (done_tx, done_rx) = oneshot::channel::<()>();
    let pipeline = std::thread::Builder::new()
        .name("import-pipeline".into())
        .spawn(move || {
            use rayon::iter::{ParallelBridge, ParallelIterator};
            let threads = std::thread::available_parallelism().map_or(1, |n| n.get().saturating_sub(1).max(1));
            let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).thread_name(|i| format!("normalize-{i}")).build().expect("rayon pool");
            let writer_gone = AtomicBool::new(false);
            pool.install(|| {
                rx.into_iter().par_bridge().for_each(|msg| match msg {
                    ReaderMsg::Item(raw) => {
                        let bytes = raw.approx_bytes();
                        if !writer_gone.load(Ordering::SeqCst) {
                            let item = normalize(raw, &ctx);
                            if sender.blocking_send(item).is_err() {
                                writer_gone.store(true, Ordering::SeqCst);
                                flag.stop(); // writer stopped: stop the reader too
                            } else {
                                sent_bytes.fetch_add(bytes, Ordering::Relaxed);
                            }
                        }
                        flag.budget.release(bytes);
                    }
                    ReaderMsg::Failed { stage, message } => {
                        *late.lock().unwrap_or_else(|p| p.into_inner()) = Some(JobFailure { stage, message });
                        flag.stop();
                    }
                    ReaderMsg::Scanned(_) => {}
                })
            });
            drop(sender);
            let _ = done_tx.send(());
        })
        .expect("spawn pipeline thread");
    // wait for: end of input, the crash seam, or a phase change (Supervisor restarting PostgreSQL)
    let mut phase = inner.phase.clone();
    phase.borrow_and_update();
    enum Wake {
        Done,
        Crash,
        Phase,
    }
    let wake = tokio::select! {
        _ = done_rx => Wake::Done,
        _ = crash.notified() => Wake::Crash,
        _ = phase.changed() => Wake::Phase,
    };
    match wake {
        Wake::Done => {}
        Wake::Crash => {
            cancel.stop();
            writer.abort();
            join_thread(pipeline).await;
            join_thread(reader).await;
            return Ok(AttemptEnd::Crashed);
        }
        Wake::Phase => {
            cancel.cancel(CancelReason::Pause);
            writer.abort();
            join_thread(pipeline).await;
            join_thread(reader).await;
            return Ok(AttemptEnd::Paused);
        }
    }
    join_thread(pipeline).await;
    join_thread(reader).await;
    // (8) end of input: commit the gap-free prefix
    let finished = writer.finish().await;
    if let Some(f) = late_failure.lock().unwrap_or_else(|p| p.into_inner()).take() {
        return Ok(AttemptEnd::Failed(f));
    }
    match finished {
        Ok(_) => {
            if cancel.reason() == Some(CancelReason::Pause) {
                return Ok(AttemptEnd::Paused);
            }
            Ok(AttemptEnd::Finished)
        }
        Err(AppError::DbNotReady) => Ok(AttemptEnd::Paused),
        Err(AppError::FileAccess { reason, .. }) if reason == "insufficientSpace" => {
            Ok(AttemptEnd::Failed(JobFailure { stage: ImportErrorStage::Store, message: "disk space low".into() }))
        }
        Err(e) => Ok(AttemptEnd::Failed(JobFailure { stage: ImportErrorStage::Store, message: e.to_string() })),
    }
}

async fn join_thread(h: std::thread::JoinHandle<()>) {
    let _ = tokio::task::spawn_blocking(move || h.join()).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_table() {
        let cases = [
            ("import", "queued", ImportStatus::Queued),
            ("import", "running", ImportStatus::Running),
            ("import", "cancelling", ImportStatus::Cancelling),
            ("import", "cancelled", ImportStatus::Cancelled),
            ("import", "failed", ImportStatus::Failed),
            ("import", "completed", ImportStatus::Completed),
            ("remove", "queued", ImportStatus::Removing),
            ("remove", "running", ImportStatus::Removing),
            ("remove", "failed", ImportStatus::Failed),
        ];
        for (kind, state, want) in cases {
            assert_eq!(status_of(kind, state), want, "{kind}/{state}");
        }
    }

    #[test]
    fn mtime_truncated_to_micros() {
        let t = UNIX_EPOCH + Duration::new(1_700_000_000, 123_456_789);
        assert_eq!(trunc_micros(t), UNIX_EPOCH + Duration::new(1_700_000_000, 123_456_000));
    }

    #[test]
    fn display_names() {
        assert_eq!(display_name("/x/Archive 2023.pst"), "Archive 2023");
        assert_eq!(display_name("/x/.pst"), ".pst");
        assert_eq!(display_name(&format!("/x/{}.pst", "a".repeat(600))).len(), 512);
    }
}
```

- [ ] **Step 6: Write the module root.** Replace `src-tauri/crates/outlooker-core/src/importer/mod.rs` with (the module lines of the kickoff stay; `ImportManager`, `ImportTuning`, `RecoveryReport` are the C14 interface):

```rust
//! Import Manager (spec Part 02 §3.1, §4; Part 03 §7; Part 04 §2.1, §5, §6.4, §6.5).
pub mod dedupe;
pub mod inflight;
pub mod normalize;
pub mod progress;
pub mod pst_reader;
pub mod queue;
pub mod remove;
pub mod types;
pub mod webp;
pub mod writer;

use std::sync::Arc;

use tokio::sync::{broadcast, watch};

use crate::db::Db;
use crate::dto::app::AppPhase;
use crate::dto::error::AppError;
use crate::dto::import::{EnqueueResult, ImportErrorPage, ImportErrorsRequest, ImportStatus, PstSource};
use crate::gate::Gate;
use crate::paths::Paths;
use crate::sink::EventSink;
use writer::WriterLimits;

#[derive(Clone)]
pub struct ImportTuning {
    /// Production: WriterLimits::default().
    pub writer: WriterLimits,
    /// Test seam: after this many committed batches of one run the job is dropped like a process kill
    /// (no state change, marker left on disk, writer aborted, the queue worker stops). None in production.
    pub crash_after_batches: Option<i64>,
    /// Test seam: in the first job attempt of this ImportManager the reader blocks after sending the item whose
    /// seq equals this value until the job is cancelled (user cancel, shutdown or DB pause); later attempts
    /// (re-run after a pause, resumed job, next job) never block. None in production.
    pub hold_reader_after: Option<i64>,
    /// Test seam: called after every commit with batches_committed (writer task; must not block).
    pub on_batch: Option<Arc<dyn Fn(i64) + Send + Sync>>,
}

impl Default for ImportTuning {
    fn default() -> Self {
        ImportTuning { writer: WriterLimits::default(), crash_after_batches: None, hold_reader_after: None, on_batch: None }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RecoveryReport {
    pub requeued: Vec<i64>,
    pub cancelled: Vec<i64>,
}

#[derive(Clone)]
pub struct ImportManager {
    inner: Arc<queue::Inner>,
}

impl ImportManager {
    pub fn new(db: Db, gate: Gate, paths: Paths, sink: Arc<dyn EventSink>, phase: watch::Receiver<AppPhase>, tuning: ImportTuning) -> ImportManager {
        ImportManager { inner: Arc::new(queue::Inner::new(db, gate, paths, sink, phase, tuning)) }
    }
    pub async fn list(&self) -> Result<Vec<PstSource>, AppError> {
        queue::list(&self.inner, None).await
    }
    pub async fn enqueue(&self, paths: Vec<String>) -> Result<Vec<EnqueueResult>, AppError> {
        queue::enqueue(&self.inner, paths).await
    }
    pub async fn cancel(&self, pst_id: i64) -> Result<(), AppError> {
        queue::cancel(&self.inner, pst_id).await
    }
    pub async fn resume(&self, pst_id: i64) -> Result<(), AppError> {
        queue::resume(&self.inner, pst_id).await
    }
    pub async fn remove(&self, pst_id: i64) -> Result<(), AppError> {
        queue::remove(&self.inner, pst_id).await
    }
    pub async fn errors(&self, req: ImportErrorsRequest) -> Result<ImportErrorPage, AppError> {
        queue::errors(&self.inner, req).await
    }
    /// Part 02 §2.2 step 8: running → queued, cancelling → cancelled.
    pub async fn recover_after_crash(&self) -> Result<RecoveryReport, AppError> {
        queue::recover_after_crash(&self.inner).await
    }
    /// Part 02 §2.2 step 9: holds gate Import, ensure_search_indexes on an admin connection.
    pub async fn self_heal_indexes(&self) -> Result<(), AppError> {
        queue::self_heal_indexes(&self.inner).await
    }
    /// Spawns the queue worker once (later calls do nothing).
    pub fn start_worker(&self) {
        queue::start_worker(&self.inner);
    }
    /// Part 02 §2.5 step 1: running job → cancel flag "shutdown", drain, last batch + checkpoint committed,
    /// state 'queued'. Waits at most `timeout`; Err(Internal{"import shutdown timed out"}) when it did not stop.
    pub async fn shutdown(&self, timeout: std::time::Duration) -> Result<(), AppError> {
        queue::shutdown(&self.inner, timeout).await
    }
    /// Every terminal job outcome as (pst_id, status): Completed, Cancelled, Failed, Queued (after shutdown),
    /// Removing (the removal deleted the source row), Running (test seam `crash_after_batches` dropped the job).
    pub fn job_events(&self) -> broadcast::Receiver<(i64, ImportStatus)> {
        self.inner.events.subscribe()
    }
}

#[cfg(feature = "test-support")]
impl ImportManager {
    /// Runs one import job whose items come from `items` instead of a PST (the stages after the reader are the
    /// production ones). Creates the source row, the job, the folders. Returns pst_id after the job ended.
    pub async fn import_synthetic(&self, path: &str, folders: Vec<types::FolderIn>, items: Vec<types::RawItem>) -> Result<i64, AppError> {
        let mut events = self.job_events();
        let pst_id = self.enqueue_synthetic(path, folders, items).await?;
        self.start_worker();
        loop {
            match events.recv().await {
                Ok((id, _)) if id == pst_id => return Ok(pst_id),
                Ok(_) => {}
                Err(e) => return Err(AppError::internal(format!("job events: {e}"))),
            }
        }
    }
    /// Like `import_synthetic` but returns right after the job row is queued. The items are registered under
    /// `path` for every ImportManager of this process, so a resumed or recovered job reads them again.
    /// Source identity: `path`'s size and mtime when it is an existing file (enables resume and mtime tests),
    /// else size 1 and mtime now.
    pub async fn enqueue_synthetic(&self, path: &str, folders: Vec<types::FolderIn>, items: Vec<types::RawItem>) -> Result<i64, AppError> {
        queue::enqueue_synthetic(&self.inner, path, folders, items).await
    }
}
```

Run: `cargo test -p outlooker-core --features test-support --lib importer::queue`.
Expected: `status_table ... ok`, `mtime_truncated_to_micros ... ok`, `display_names ... ok`; `test result: ok. 3 passed`; `cargo build -p outlooker-core` prints no warning for `importer/{mod,queue,pst_reader,inflight,progress}.rs`.

- [ ] **Step 7: Run the integration tests.**

Run: `cargo test -p outlooker-core --features test-support --test import_pipeline`.
Expected: 26 tests `ok`: `outlook_pst_import`, `byte_copy_dedupes_and_removal`, `corrupt_copy_outcome`, `synthetic_cjk`, `synthetic_inline_images`, `synthetic_attachments`, `synthetic_hostile`, `synthetic_message_id_dedupe`, `index_policy_first_import_drops_and_rebuilds`, `index_policy_rebuilt_after_cancel_and_disk_failure`, `index_policy_incremental_keeps_indexes`, `after_job_vacuum_ran`, `resume_after_cancel_equals_uninterrupted`, `resume_after_kill_equals_uninterrupted`, `mtime_change_resets_cursor`, `poison_marker_attempts_1_retries_item`, `poison_marker_attempts_2_skips_item`, `poison_marker_of_other_job_is_ignored`, `poison_marker_dropped_on_checkpoint_reset`, `retry_rewrites_marker_before_reading`, `pg_crash_mid_import_resumes`, `pg_crash_failed_then_retry_resumes`, `recover_after_crash_states`, `pst_list_status_derivation`, `pst_remove_with_queued_job`, `enqueue_validation`; `test result: ok. 26 passed; 0 failed`.
`corrupt_copy_outcome` pre-registers `Completed` + at least one `item_skipped` / `parse` row + 0 messages (observed in planning: exactly this outcome). If a run observes another outcome, report it to the leader; do not change the constants.

- [ ] **Step 8: Red checks (mutations; copy the file first, restore only from the copy).**

1. `cp src-tauri/crates/outlooker-core/src/importer/queue.rs scripts/tmp/queue.rs.bak`; in `Inner::inflight` replace `self.paths.with_data_root(self.db.data_root()?).inflight_marker()` with `self.paths.inflight_marker()`; run `cargo test -p outlooker-core --features test-support --test import_pipeline poison_marker` → the three `poison_marker_*` tests FAILED with "marker deleted at job end" (the marker under `Db::data_root()` was never read). Restore: `cp scripts/tmp/queue.rs.bak src-tauri/crates/outlooker-core/src/importer/queue.rs`.
2. In `run_import` delete the line `cancel.clear_pause();`; run `cargo test -p outlooker-core --features test-support --test import_pipeline pg_crash_mid_import_resumes` → the job never completes (the re-run stops at once and pauses again; observed: no result within 30 s; the test fails after `JOB_TIMEOUT` 120 s with "job ended in time"). Restore from the copy.
3. `cp src-tauri/crates/outlooker-core/src/importer/pst_reader.rs scripts/tmp/pst_reader.rs.bak`; in `run` change `Some(r) if r.attempts >= 2` to `Some(r) if r.attempts >= 1`; run `cargo test -p outlooker-core --features test-support --test import_pipeline poison retry_rewrites` → `poison_marker_attempts_1_retries_item` and `retry_rewrites_marker_before_reading` FAILED (2 passed, 2 failed). Restore: `cp scripts/tmp/pst_reader.rs.bak src-tauri/crates/outlooker-core/src/importer/pst_reader.rs`.
4. `cp src-tauri/crates/outlooker-core/src/importer/queue.rs scripts/tmp/queue.rs.bak`; in `pause_wait` change `ph.wait_for(|p| *p == AppPhase::Ready) => r.is_ok()` back to `ph.wait_for(|p| matches!(p, AppPhase::Ready | AppPhase::Failed)) => matches!(r.as_deref(), Ok(AppPhase::Ready))`; run `cargo test -p outlooker-core --features test-support --test import_pipeline pg_crash_failed_then_retry_resumes` → FAILED with `left: None, right: Some(Completed)` (observed in scratch: the job gave up on `Failed` and never resumed). Restore: `cp scripts/tmp/queue.rs.bak src-tauri/crates/outlooker-core/src/importer/queue.rs`.
5. Same backup; in `attempt` change `let pending = if reset { None } else { inflight.read().filter(|r| r.job_id == job_id) };` to `let pending = inflight.read().filter(|r| r.job_id == job_id);`; run `cargo test -p outlooker-core --features test-support --test import_pipeline poison_marker_dropped_on_checkpoint_reset` → FAILED with `left: [("parse", "item_skipped", Some(2097220), Some("Recovered_Group2"), "parser crashed on this item")]`, `right: []` (observed in scratch). Restore from the copy.
6. Run Step 7 again → 26 passed.

- [ ] **Step 9: Write the bench.** Create `src-tauri/crates/outlooker-core/examples/import_bench.rs`:

```rust
//! Import throughput bench (spec Part 02 §9): imports a given PST into a fresh cluster and prints
//! messages/s and MB/s per phase. The last stdout line is exactly
//! `import_bench messages_per_sec=<f64> peak_rss_bytes=<u64>` (messages/s = items done / import-phase seconds;
//! peak RSS of this process, sampled at every progress event). Run: `cargo run --release -p outlooker-core --features test-support
//! --example import_bench -- <absolute path to .pst>`.

#[cfg(not(feature = "test-support"))]
fn main() {
    eprintln!("import_bench runs on a TestCluster: build with --features test-support");
    std::process::exit(2);
}

#[cfg(feature = "test-support")]
#[tokio::main]
async fn main() {
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    use outlooker_core::dto::app::AppPhase;
    use outlooker_core::dto::import::{EnqueueResult, ImportPhase, ImportStatus};
    use outlooker_core::gate::Gate;
    use outlooker_core::importer::{ImportManager, ImportTuning};
    use outlooker_core::sink::{CoreEvent, EventSink};
    use outlooker_core::test_support::TestCluster;

    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

    /// First time each phase is seen, the last progress counters, and the bench process RSS sampled at every
    /// progress event (one per committed batch and phase change; no timer).
    struct PhaseClock {
        seen: Mutex<Vec<(ImportPhase, Instant)>>,
        last: Mutex<Option<(i64, i64)>>, // (items_done, stored)
        rss: Mutex<(System, Pid, u64)>,  // (sampler, own pid, peak bytes)
    }
    impl PhaseClock {
        fn sample_rss(&self) {
            let mut guard = self.rss.lock().unwrap();
            let (sys, pid, peak) = &mut *guard;
            sys.refresh_processes_specifics(ProcessesToUpdate::Some(std::slice::from_ref(pid)), false, ProcessRefreshKind::nothing().with_memory());
            if let Some(p) = sys.process(*pid) {
                *peak = (*peak).max(p.memory());
            }
        }
    }
    impl EventSink for PhaseClock {
        fn emit(&self, event: CoreEvent) {
            if let CoreEvent::ImportProgress(p) = event {
                let mut seen = self.seen.lock().unwrap();
                if !seen.iter().any(|(ph, _)| *ph == p.phase) {
                    seen.push((p.phase, Instant::now()));
                }
                *self.last.lock().unwrap() = Some((p.items_done, p.stored));
                drop(seen);
                self.sample_rss();
            }
        }
    }

    let path = std::env::args().nth(1).expect("usage: import_bench <absolute path to .pst>");
    let size_mb = std::fs::metadata(&path).expect("PST file").len() as f64 / (1024.0 * 1024.0);
    let tc = TestCluster::start().await;
    let pid = sysinfo::get_current_pid().expect("own pid");
    let clock = Arc::new(PhaseClock { seen: Mutex::new(Vec::new()), last: Mutex::new(None), rss: Mutex::new((System::new(), pid, 0)) });
    clock.sample_rss();
    let (_phase, rx) = tokio::sync::watch::channel(AppPhase::Ready);
    let mgr = ImportManager::new(tc.db.clone(), Gate::new(), tc.paths.clone(), clock.clone(), rx, ImportTuning::default());
    let mut events = mgr.job_events();
    let start = Instant::now();
    let res = mgr.enqueue(vec![path.clone()]).await.expect("enqueue");
    let EnqueueResult::Queued { pst } = &res[0] else { panic!("rejected: {res:?}") };
    mgr.start_worker();
    let status = loop {
        let (id, status) = events.recv().await.expect("job events");
        if id == pst.id {
            break status;
        }
    };
    let end = Instant::now();
    assert_eq!(status, ImportStatus::Completed, "job ended {status:?}");
    let seen = clock.seen.lock().unwrap().clone();
    let at = |ph: ImportPhase| seen.iter().find(|(p, _)| *p == ph).map(|(_, t)| *t);
    let importing = at(ImportPhase::Importing).unwrap_or(start);
    let finalizing = at(ImportPhase::Finalizing).unwrap_or(end);
    let (items, stored) = clock.last.lock().unwrap().unwrap_or((0, 0));
    let secs = |a: Instant, b: Instant| b.duration_since(a).as_secs_f64().max(1e-9);
    println!("file            {path} ({size_mb:.1} MB)");
    println!("scan            {:.2} s", secs(start, importing));
    println!(
        "import          {:.2} s  {:.1} messages/s  {:.2} MB/s  ({items} items, {stored} stored)",
        secs(importing, finalizing),
        items as f64 / secs(importing, finalizing),
        size_mb / secs(importing, finalizing)
    );
    println!("finalize        {:.2} s  (indexes, VACUUM)", secs(finalizing, end));
    println!("total           {:.2} s  {:.2} MB/s", secs(start, end), size_mb / secs(start, end));
    clock.sample_rss();
    let peak_rss_bytes = clock.rss.lock().unwrap().2;
    // last line: machine-read by the R2-10 perf test; keep the exact format
    println!("import_bench messages_per_sec={} peak_rss_bytes={peak_rss_bytes}", items as f64 / secs(importing, finalizing));
}
```

Run: `cargo build -p outlooker-core --example import_bench` → exit 0 (without `test-support` the binary only prints how to build it). Smoke run: `cargo run -p outlooker-core --features test-support --example import_bench -- "$PWD/../tests/fixtures/external/outlook.pst"` → prints the lines `file`, `scan`, `import … messages/s … MB/s (2 items, 1 stored)`, `finalize`, `total`, and as the last stdout line exactly `import_bench messages_per_sec=<f64> peak_rss_bytes=<u64>` (read by the R2-10 perf test; observed in planning: `import_bench messages_per_sec=118.96706842837067 peak_rss_bytes=14761984`). Check the contract: `cargo run -q -p outlooker-core --features test-support --example import_bench -- "$PWD/../tests/fixtures/external/outlook.pst" | tail -n 1 | grep -E '^import_bench messages_per_sec=[0-9.]+ peak_rss_bytes=[0-9]+$'` → exit 0. `messages_per_sec` = items done / import-phase seconds; `peak_rss_bytes` = largest RSS of the bench process (sysinfo `Process::memory()`), sampled at every progress event and at the end, so a peak between two events is not seen and the PostgreSQL server's memory is not included.

- [ ] **Step 10: Check the Windows-phase fixture gate.**

Run: `cargo test -p outlooker-core --features test-support,outlook-fixtures --test import_pipeline fixtures_`.
Expected before phase W2 (no `tests/fixtures/pst/*.pst`): `test result: FAILED. 0 passed; 7 failed`, each panic message `missing fixture tests/fixtures/pst/<name>.pst (Windows phase W2)` for `basic`, `cjk`, `inline`, `attach`, `dup-a`, `hostile`, `corrupt`. After W2 commits the fixtures: `test result: ok. 7 passed` (`fixtures_dup`: `messages` = 50, `message_occurrences` = 81).

- [ ] **Step 11: Run the task acceptance commands.**

`cargo test -p outlooker-core --features test-support --test import_pipeline` → 26 passed; `cargo build -p outlooker-core --example import_bench` → exit 0; `cargo test -p outlooker-core --features test-support --lib importer::` → all `importer::` unit tests pass (R1-05 tests included).

- [ ] **Step 12: Commit.** Run `git rev-parse --abbrev-ref HEAD`; then `git add src-tauri/crates/outlooker-core/src/importer/mod.rs src-tauri/crates/outlooker-core/src/importer/queue.rs src-tauri/crates/outlooker-core/src/importer/pst_reader.rs src-tauri/crates/outlooker-core/src/importer/inflight.rs src-tauri/crates/outlooker-core/src/importer/progress.rs src-tauri/crates/outlooker-core/tests/import_pipeline.rs src-tauri/crates/outlooker-core/examples/import_bench.rs`, write the message `feat(core): import manager, PST reader and pipeline` (plus the Co-Authored-By trailer) to a message file, `git commit -F <msgfile> -- <the seven paths>`, then `git status --porcelain` shows none of the seven paths.

## Consumes cross-check

One row per name this file consumes from another producer. "Confirmed" = the exact signature was read in the producer's plan file and the code of this file was compiled against a copy of it in the scratch workspace (rustc 1.99.1, edition 2024).

| Consumed name | Consumer task(s) | Producer | Status | Note |
|---|---|---|---|---|
| `dto::error::{AppError, BusyReason}`, `AppError::{invalid, internal, file_access, not_found}` | all | R1-01 (foundation Step 7) | Confirmed | `AppError` derives `Eq`; tests compare errors with `assert_eq!`. |
| `dto::app::{AppPhase, LibraryCounts}` | R2-09 | R1-01 (Step 8) | Confirmed | |
| `dto::search::*` (`SearchQuery`, `ConditionGroup`, `ConditionNode`, `GroupOp`, `RecipientKind`, `SearchRequest`, `SearchPage`, `MailRow`, `Address`, `SearchCount`, `FilterOptions`, `PstRef`, `FolderRef`, `MailDetail`, `AttachmentMeta`, `Occurrence`) | R1-05, R2-10, R2-11, R2-12 | R1-01 (Step 9) | Confirmed | `RecipientKind` has no `Ord`; the compiler sorts the kind names as strings. |
| `dto::sql::{SavedQuery, SavedQueryInput, SqlExecuteRequest, SqlResult, ColumnMeta}` | R2-12 | R1-01 (Step 11) | Confirmed | `SqlResult::Table.rows` is `Vec<Vec<Option<String>>>` (no `CellValue` alias). |
| `dto::import::*` (`PstSource`, `ImportStatus`, `ImportProgress`, `ImportPhase`, `EnqueueResult`, `ImportErrorsRequest`, `ImportErrorPage`, `ImportErrorEntry`, `ImportErrorStage`, `ImportErrorSeverity`) | R1-05, R2-09, R2-10 | R1-01 (Step 12) | Confirmed | |
| `dto::settings::MaintenanceTask`; `MaintenanceState`, `MaintenanceRunState` (defined in `dto::settings`, re-exported by `dto::events`); `dto::events::{ImportStatusChanged, LibraryChanged}` | R2-09, R2-12 | R1-01 | Confirmed | planner-1 answer. |
| `sink::{EventSink, CoreEvent, RecordingSink, NullSink}` | R2-09, R2-12 | R1-01 (C2) | Confirmed | |
| `gate::{Gate, Activity, GateGuard, busy_reason}`, `Gate::released()` (`pub async fn`) | R2-09, R2-12 | R1-01 (C3) | Confirmed | Added on planner-2's request; loop `try_enter` / `released().await`. |
| `db::{Db, DbConn, Pools, Credentials, connect_admin, migrate, pg_err, RFC3339_UTC_FMT, DB_NAME}`, `Db::{app_pool, ro_pool, connect_admin, connect_app, data_root}` | all DB tasks | R1-04 (Step 3) | Confirmed | `Db::data_root()` and `DbConn.data_root` added on request. |
| `db::indexes::{TSV_EXPR, SEARCH_INDEXES, drop_search_indexes, ensure_search_indexes(admin, &dyn Fn(&str, usize, usize)), missing_or_invalid}` | R2-09, R2-10, R2-11, R2-12 | R1-04 (Step 4) | Confirmed | Spec commit 926f372: `messages.body_tsv` is a stored generated column (generation expression = `TSV_EXPR`) and `messages_body_fts` DDL is `USING gin (body_tsv)`; planner-1 adopted it in the foundation (migration generated from the spec, `SEARCH_INDEXES[5].ddl`, `v_mail.body_tsv`, `ddl_matches_migration`). The compiler emits `m.body_tsv`, never `TSV_EXPR`; `explain_uses_indexes` shows `messages_body_fts` is used (scratch, new schema). |
| `test_support::TestCluster::{start, admin, app, ro, kill, restart, db, paths}` | R2-09 … R2-12 tests, loadgen smoke | R1-04 (Step 5) | Confirmed | |
| `paths::{Paths, dev_pg_dir}`, `lifecycle::pg::{PgController::{new, init, start, stop}, StopMode}` | R2-10 loadgen `root.rs` | R1-03 (C6, C7) | Confirmed | `LibraryRoot` passes the data root as app-local and log dir (never written). |
| `lifecycle::move_dir::free_space(&Path) -> Result<u64, AppError>` | R2-10 writer disk guard | R2-08 | Confirmed; ordering open | Replaces the planned `writer::free_bytes`. R2-08 must land `move_dir.rs` before R2-10 Step 6 (both Round 2): leader to sequence. |
| `Supervisor::phase_watch()` (C15) | R2-09 | R2-08 | Confirmed | |
| `pff::{File::{open, root_folder, message_store}, Item::{…, attached_item}, AttachmentType, tags, filetime_to_unix_micros}`; property getters take the 16-bit property id | R2-09 | R1-02 (C5) | Confirmed | `message_store()` and `attached_item()` added on request. |
| Cargo features `test-support`, `outlook-fixtures = ["test-support"]`; crates `base64 =0.23.1`, `sysinfo =0.39.6`, `tempfile =3.27.0` | all | R1-01 kickoff manifest | Confirmed | |
| `[profile.dev.package."*"] opt-level = 3` in `src-tauri/Cargo.toml` | R2-10 loadgen tests (speed only) | R2-13 (manifest owner) | Requested | Not a correctness dependency: without it the loadgen tests pass but take minutes (scratch: 30.7 s vs 3.7 s for the unit tests). |
| `import_bench` last stdout line `import_bench messages_per_sec=<f64> peak_rss_bytes=<u64>`, first argument = PST path | R2-10 `perf_e2e_import` | R2-09 (Step 9) | Confirmed | The bench prints it last (items over the Importing phase; peak RSS sampled with sysinfo). Observed in planner-8's scratch run on `outlook.pst`: `import_bench messages_per_sec=118.967… peak_rss_bytes=14761984`. `perf_e2e_import` runs it with `--features test-support` (without it the bench exits 2). |
| `ReaderParams{pending, job_id}`, `InflightRecord{job_id, seq, folder_path, pst_item_id, attempts}`, `ImportManager::enqueue_synthetic`, `ByteBudget::{used, wait_cancelled}` | R2-09 tests | R2-09 (this file) | Confirmed | Stage 1 aligned with the Stage 2 code (spec Part 02 §6.3 attempts rule). |
| C9 `importer::{types, normalize, webp, dedupe}` | R2-09, R2-10 | R1-05 (this file) | Confirmed | Compiled together in one scratch crate. |
| C11 `importer::writer::{Writer, WriterHandle, WriterConfig, WriterLimits, Checkpoint, upsert_folders, load_known_hashes, map_db_error, AFTER_JOB_VACUUM, truncate_to_micros, stage_db, severity_db, phase_db}`, C12 `importer::remove::{run, delete_orphans}` | R2-09, R2-12 | R2-10 (this file) | Confirmed | |
| `test_support::import_fixtures::{new_source, complete_job, mail, add_recipient, item, seed_mails}` | R2-09, R2-11, R2-12 tests, loadgen | R2-10 (this file) | Confirmed | |
| C13 `search::hydrate_rows` | R2-12 | R2-11 (this file) | Confirmed | |
| C14 facades `ImportManager`, `SearchService`, `SqlConsole`, `Maintenance` (method names of planner-3 requests P2-1 … P2-9) | R2-13 | this file | Confirmed with planner-3 | |

Spec deviations found while planning:
- **S1 (resolved):** the SQLSTATE mapping of Advanced SQL is now defined once in spec Part 04 §8.1 (`25006`, `55000` → `readOnlyViolation`; `42501` → `readOnlyViolation` only when the statement prepared and has no result columns, else `sqlError`; `57014` → `cancelled` / `queryTimeout`; `428C9` dropped). R2-12 follows it.
- **S2 (known limit in the spec):** Command-mode `tag` = first keyword, ASCII-uppercased (tokio-postgres 0.7.18 exposes no command tag).
- **S3 (resolved, spec commit 926f372):** phase A recomputed `to_tsvector` per window row (0.32 ms/row). The spec now stores `messages.body_tsv` (generated column, `messages_body_fts` on it), the compiler emits `(m.body_tsv @@ … OR m.body_text ILIKE …)`, and W = 5,000 (window end `OFFSET 4999`). R2-11 follows it; R2-10 `perf_db_size` has the new line `TOAST body_tsv` (250 MB per 100k). Scratch re-run on the new schema: lib 33, search 16, writer 10, removal 2, loadgen smoke 2 ok.

