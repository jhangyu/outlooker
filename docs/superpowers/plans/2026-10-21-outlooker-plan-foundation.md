# Outlooker Implementation Plan — Foundation Tasks (R1-01, R1-02, R1-03, R1-04, R2-08, reviewers, Windows phase)

> **For agentic workers:** REQUIRED SUB-SKILL: Use podium:team-spawn (recommended) or podium:team-fable to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

Parent plan (binding): `docs/superpowers/plans/2026-10-21-outlooker-plan.md` — Global Constraints, file ownership (§5, §6), cross-task interfaces C1–C15 (§2.2), gates (§7). This file adds the full Stage 1 blocks and the Stage 2 steps for the tasks listed in the parent plan §8 for planner-1. The spec is `docs/superpowers/specs/2026-10-21-outlooker-design.md` ("the spec"); "Part 0N §x" refers to its Parts.

**Goal:** Land the workspace, the API contract, the libpff FFI, the bundled PostgreSQL runtime, the database schema, and (Round 2) the Lifecycle Supervisor and Settings Store.

**Architecture:** See the parent plan header. This file covers the layers below the services: `dto` + `sink` + `gate` (contract), `pff` (parser FFI), `paths` + `lifecycle::pg` (process control of PostgreSQL), `db` (schema, roles, pools, the `Db` handle), and the `lifecycle` Supervisor that owns `AppPhase`.

**Tech Stack:** as the parent plan, plus the four crates of §0.2.

**Decisions (hard-to-reverse only):**
- `db::Db` is a cloneable handle (`Arc<RwLock<Option<DbConn>>>`) that every service reads at call time (alternatives: pass `Pools` by value at construction; services ask the Supervisor). Drivers: PostgreSQL gets a new random port on every (re)start (Part 02 §2.2 step 5), so a pool captured at construction goes stale after a crash restart; a service must not import the Supervisor (parent plan §2.3). Drawback: every DB call starts with a cheap read-lock.
- Timestamps are formatted to RFC 3339 in SQL (`to_char(ts AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')`) and FILETIME is converted in Rust to Unix microseconds, so no date-time crate is added (alternative: tokio-postgres `with-chrono`). Driver: Part 01 lists no date-time crate. Drawback: one SQL helper constant must be used everywhere (`db::RFC3339_UTC_FMT`).

Plan gate: `/Users/jhangyu/.claude/plugins/cache/jhangyu/podium/1.1.0/protocols/review.md` §L0 Plan gate.

## 0. Global constraints for this file

### 0.1 Inherited
All Global Constraints of the parent plan apply. Commands typed by workers never name a `/tmp` path (a hook blocks it): scratch and commit-message files go to `scripts/tmp/` (gitignored). Runtime temp dirs (tempfile, `os.tmpdir()`, `mktemp -d`) are fine. Paths: `core` = `src-tauri/crates/outlooker-core`; `crates/` = `src-tauri/crates/`. All `cargo` commands run in `src-tauri/`; all `npm` commands at the repo root.

### 0.2 Dependencies added after the first spec draft (approved by the leader, now in spec Part 01 §8)
| Crate | Version | Scope | Needed for |
|---|---|---|---|
| `getrandom` | `=0.4.3` | `outlooker-core` | 32-byte random role passwords (Part 02 §2.3 step 3) |
| `sysinfo` | `=0.39.6` | `outlooker-core` | physical RAM for `shared_buffers` (Part 04 §6.6); free disk space (Part 02 §6.1, §8.3); "PID's executable is the bundled postgres" (Part 02 §2.2 step 4) |
| `tempfile` | `=3.27.0` | `outlooker-core` dev-dependency + feature `test-support` | temp data roots in tests |
| `base64` | `=0.23.1` | `outlooker-core` | search cursor (Part 03 §5.2 base64url JSON) and `data:` image URIs (backend plan R1-05, R2-11) |

No hand-written per-OS replacement is allowed (Platform parity).

### 0.3 Spec facts used by several tasks (copied here so workers need not search)
- App identifier `us.jhangy.outlooker`. Database name `outlooker`. Roles `outlooker_admin` (initdb superuser), `outlooker_app`, `outlooker_ro`.
- Locale candidates (Part 01 §6): Windows `["en-US", "English_United States.65001"]`; macOS / Linux `["en_US.UTF-8", "C.UTF-8"]`. Self-check: `SELECT similarity('測試測試','測試') > 0`.
- Data root layout (Part 02 §8.2): `outlooker-data.json` (marker `{ "format_version": 1, "created_at": "<rfc3339>", "pg_major": 18, "locale": "<locale>" }`), `auth/credentials`, `import-inflight`, `pgdata/`.
- Bootstrap file (Part 02 §2.2 step 2): `<app_local_data_dir>/settings.json` = `{ "data_dir": "<abs path>" | null, "language": "zh-TW" | "en" }`.
- 24 commands (Part 03 §3 + §4 `app_retry`), 6 events (Part 03 §9).

---

# Part A — Stage 1 skeleton (all tasks)

### Task R1-01: Kickoff skeleton, App Shell base, API contract, Work Gate

**Files:**
- Create (kickoff commit): `mise.toml`, `package.json`, `package-lock.json`, `vite.config.ts`, `tsconfig.json`, `index.html`, `.gitignore`, `src/main.tsx`, `src/test/setup.ts`, the typed TS stubs and `{}` locale files of parent §2.1 item 5; `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, `src-tauri/rust-toolchain.toml`, `src-tauri/build.rs`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`, `src-tauri/icons/` (Tauri default icon set from `npm run tauri icon` of a 1024 px placeholder PNG), `src-tauri/.cargo/config.toml`, `src-tauri/src/main.rs`, `src-tauri/src/lib.rs`, every kickoff Rust file of parent §1.3 (empty), `crates/pff/Cargo.toml`, `crates/outlooker-loadgen/{Cargo.toml,src/main.rs}`, `core/Cargo.toml`, `core/src/lib.rs`.
- Create (contract commit): `core/src/dto/{mod,error,app,search,sql,import,settings,events,commands}.rs`, `core/src/sink.rs`, `core/src/gate/mod.rs`, `src-tauri/schemas/condition-group.json`, `src/api/{call,commands,events,dialog}.ts`, `src/api/api.test.ts`, `src/api/bindings/**` (generated).
- Test: unit tests inside `core/src/dto/*.rs`, `core/src/gate/mod.rs`, `core/src/sink.rs`; `src/api/api.test.ts`.

**Interfaces:**
- Consumes: nothing.
- Produces:
  - C1 `outlooker_core::dto::*` — every type of Part 03 §1.2 and §4–§9, names and fields exactly as the spec, with these four precise rules:
    1. Every type derives `Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS` and has `#[ts(export)]`; structs use `#[serde(rename_all = "camelCase")]`.
    2. `AppPhase`, `Language`, `GroupOp`, `RecipientKind`, `ImportStatus`, `ImportPhase`, `ImportErrorStage`, `ImportErrorSeverity`, `MaintenanceTask`, `BusyReason`, `DataDirMovePhase`, `MaintenanceRunState` also derive `Copy, Eq, Hash`.
    3. `AppError` additionally derives `thiserror::Error` and `Eq`.
    4. `CellValue` is not a type alias (ts-rs cannot export an alias with serde attributes): `SqlResult::Table.rows` is `Vec<Vec<Option<String>>>` (TS `Array<Array<string | null>>`).
  - Event payload types (Part 03 §9): `ImportStatusChanged { pst_id: i64, status: ImportStatus, removed: bool, source: Option<PstSource> }`, `LibraryChanged { counts: LibraryCounts }`, `DataDirMoveProgress { phase: DataDirMovePhase, bytes_copied: i64, bytes_total: i64, error: Option<AppError> }`, `DataDirMovePhase { StoppingDb, Copying, Verifying, StartingDb, Done, Failed }`, `MaintenanceState { task: MaintenanceTask, state: MaintenanceRunState, started_at: String, finished_at: Option<String>, error: Option<AppError> }`, `MaintenanceRunState { Running, Done, Failed }`.
  - Request types for multi-field commands: `ImportErrorsRequest { pst_id: i64, cursor: Option<String>, limit: Option<u32> }`, `ImportErrorPage { rows: Vec<ImportErrorEntry>, next_cursor: Option<String> }`.
  - `dto::events::names::{APP_STATUS_CHANGED = "app:status-changed", IMPORT_PROGRESS = "import:progress", IMPORT_STATUS_CHANGED = "import:status-changed", LIBRARY_CHANGED = "library:changed", DATA_DIR_MOVE_PROGRESS = "data-dir:move-progress", MAINTENANCE_STATUS_CHANGED = "maintenance:status-changed"}` and `pub const ALL: [&str; 6]`.
  - `dto::commands::COMMANDS: [&str; 24]` in Part 03 §3 order with `app_retry` after `app_initialize`.
  - C2 `outlooker_core::sink`:
    ```rust
    pub trait EventSink: Send + Sync { fn emit(&self, event: CoreEvent); }
    #[derive(Debug, Clone, PartialEq)]
    pub enum CoreEvent {
        AppStatusChanged(AppStatus), ImportProgress(ImportProgress), ImportStatusChanged(ImportStatusChanged),
        LibraryChanged(LibraryChanged), DataDirMoveProgress(DataDirMoveProgress), MaintenanceStatusChanged(MaintenanceState),
    }
    impl CoreEvent { pub fn name(&self) -> &'static str; pub fn payload(&self) -> serde_json::Value; }
    #[derive(Default)] pub struct RecordingSink { /* Mutex<Vec<CoreEvent>> */ }
    impl RecordingSink { pub fn events(&self) -> Vec<CoreEvent>; pub fn names(&self) -> Vec<&'static str>; }
    impl EventSink for RecordingSink { .. }
    pub struct NullSink; impl EventSink for NullSink { .. }
    ```
  - C3 `outlooker_core::gate`:
    ```rust
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)] pub enum Activity { Import, Removal, Maintenance, DataDirMove }
    #[derive(Clone, Default)] pub struct Gate { /* Arc<Mutex<Option<Activity>>> */ }
    impl Gate {
        pub fn new() -> Gate;
        pub fn try_enter(&self, activity: Activity) -> Result<GateGuard, AppError>; // Err(AppError::Busy { reason })
        pub fn current(&self) -> Option<Activity>;
        pub async fn released(&self);   // resolves when no activity holds the gate (Notify, no polling)
    }
    pub struct GateGuard { /* gate + activity */ }   // Drop clears the slot
    pub fn busy_reason(active: Activity) -> BusyReason; // Import→ImportRunning, Removal→PstRemoving, Maintenance→MaintenanceRunning, DataDirMove→DataDirMoving
    ```
  - C4 TS: `src/api/call.ts` `export async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T>` (rejection value is re-thrown as `AppError`); `src/api/commands.ts` `export const api = { appStatus, appInitialize, appRetry, searchMail, … }` (24 members, camelCase of `COMMANDS`); `export const COMMAND_NAMES: readonly string[]`; `src/api/events.ts` `export const EVENTS = { appStatusChanged: "app:status-changed", … } as const`, `export type EventPayloads`, `export function onEvent<K extends keyof EventPayloads>(name: K, handler: (payload: EventPayloads[K]) => void): Promise<UnlistenFn>`. `src/api/dialog.ts`: `export function pickPstFiles(): Promise<string[]>` (empty array when cancelled) and `export function pickFolder(): Promise<string | null>`; when built with `VITE_E2E=1` each call returns `window.__outlookerE2ePicks.shift()` instead of opening the native dialog (E2E bypass for R2-14).

**Behavior:**
- Kickoff commit first: manifests with every Part 01 dependency (versions in Stage 2 Step 1–3), the parent §0.2 extras of this file, empty module files, TS stubs; `tauri.conf.json` has no `bundle.resources`. After it, `cargo build --workspace --locked` and `npm run typecheck` pass on a fresh checkout without `src-tauri/resources/`.
- The Work Gate holds at most one activity at a time: every activity conflicts with every activity (including itself), which is exactly the union of the conflict rules in Part 03 §7.2 and §8 (one job at a time; maintenance refused during a job; move refused during anything). `Busy.reason` names the activity that holds the gate.
- `CoreEvent::name()` returns the `names::*` constant of its variant; `payload()` is `serde_json::to_value` of the inner payload.
- ts-rs export runs in `cargo test`; `dto::commands` and `dto::events` each have a test that writes `src/api/bindings/commands.json` / `events.json` (pretty JSON array, trailing newline) using `TS_RS_EXPORT_DIR`.
- The JSON-schema test serializes the worked example of Part 03 §5.1 from Rust values (not from a JSON literal) and validates it with `jsonschema::validator_for`.
- App Shell base: `tauri::Builder::default().plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| focus main window)).plugin(tauri_plugin_dialog::init()).plugin(tauri_plugin_log::Builder::new().build()).invoke_handler(tauri::generate_handler![])`.

**Constraints:**
- `dto` is a leaf module: it imports only `serde`, `ts_rs`, `thiserror`, `serde_json` (tests), `jsonschema` (tests).
- No `bundle.resources` in Round 1.
- npm: `npm install --save-exact`; no `^`/`~` in `package.json`.

**Acceptance criteria:**
- [ ] `cargo build --workspace --locked` exits 0 at the kickoff commit and the final commit on a checkout without `src-tauri/resources/`.
- [ ] In `src-tauri/`, `rustc --version` prints `rustc 1.99.1`.
- [ ] At the repo root, after `mise trust`, `node --version` prints `v24.22.1`.
- [ ] `grep -nE '"[\^~]' package.json` finds nothing.
- [ ] `cargo tree -p outlooker-core --depth 1 -e normal` lists `tokio-postgres v0.7.18`, `deadpool-postgres v0.14.2`, `refinery v0.10.0`, `ammonia v4.2.4`, `html2text v0.17.4`, `encoding_rs v0.8.42`, `chardetng v1.0.0`, `image v0.25.10`, `webp v0.3.1`, `rayon v1.12.0`, `blake3 v1.8.7`, `serde v1.0.229`, `serde_json v1.0.154`, `thiserror v2.0.21`, `log v0.4.34`, `ts-rs v12.0.1`, `tokio v1.54.0` (or newer patches recorded in the commit message).
- [ ] `cargo test -p outlooker-core --lib dto:: gate:: sink::` exits 0 (tests named in Stage 2).
- [ ] After `cargo test --workspace --locked`, `git diff --exit-code src/api/bindings` exits 0.
- [ ] `node -e "const c=require('./src/api/bindings/commands.json');process.exit(c.length===24&&c.includes('app_retry')?0:1)"` exits 0; same for `events.json` with length 6.
- [ ] `grep -rE "crate::(lifecycle|importer|search|sql_console|maintenance|settings|db|gate|paths|app)" src-tauri/crates/outlooker-core/src/dto` finds nothing.
- [ ] `npm run typecheck` and `npx vitest run src/api` exit 0.
- [ ] `grep -rE "webDarkTheme|teamsDarkTheme|theme-one-dark|prefers-color-scheme: *dark" package.json src src-tauri/tauri.conf.json` finds nothing.

### Task R1-02: libpff vendoring, FFI crate, PST fixture tooling

**Files:**
- Create: `scripts/vendor-libpff.sh`, `src-tauri/native/libpff/**` (+ `ORIGIN`), `crates/pff/build.rs`, `crates/pff/src/{lib,ffi,tags}.rs`, `crates/pff/tests/outlook_pst.rs`, `tests/fixtures/external/{outlook.pst,NOTICE}`, `scripts/fixtures/make-pst.ps1`.
- Modify: `crates/pff/Cargo.toml` (created in the kickoff).

**Interfaces:**
- Consumes: nothing.
- Produces (C5, crate `pff`):
  ```rust
  pub fn version() -> &'static str;                         // "20260625"
  #[derive(Debug, Clone, PartialEq, Eq)] pub struct Error { pub message: String }   // impl Display + std::error::Error
  pub type Result<T> = std::result::Result<T, Error>;
  pub fn check_signature(path: &std::path::Path) -> Result<bool>;
  pub struct File { /* *mut libpff_file_t, !Send, !Sync */ }
  impl File {
      pub fn open(path: &std::path::Path) -> Result<File>;   // read-only
      pub fn root_folder(&self) -> Result<Item<'_>>;
      pub fn message_store(&self) -> Result<Item<'_>>;          // libpff_file_get_message_store (IPM subtree entry id 0x35e0)
  }
  pub struct Item<'f> { /* *mut libpff_item_t, borrows File, !Send */ }
  #[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum AttachmentType { Data, Item, Reference, Undefined }
  impl<'f> Item<'f> {
      pub fn identifier(&self) -> Result<u32>;
      // folders
      pub fn folder_name(&self) -> Result<Option<String>>;
      pub fn sub_folder_count(&self) -> Result<usize>;
      pub fn sub_folder(&self, index: usize) -> Result<Item<'f>>;
      pub fn sub_message_count(&self) -> Result<usize>;
      pub fn sub_message(&self, index: usize) -> Result<Item<'f>>;
      // generic MAPI property access (record set 0 for messages / attachments; one set per recipient)
      pub fn record_set_count(&self) -> Result<usize>;
      pub fn string(&self, set: usize, tag: u32) -> Result<Option<String>>;
      pub fn u32_value(&self, set: usize, tag: u32) -> Result<Option<u32>>;
      pub fn filetime(&self, set: usize, tag: u32) -> Result<Option<u64>>;
      pub fn binary(&self, set: usize, tag: u32) -> Result<Option<Vec<u8>>>;
      // messages
      pub fn client_submit_time(&self) -> Result<Option<u64>>;    // FILETIME
      pub fn delivery_time(&self) -> Result<Option<u64>>;
      pub fn plain_text_body(&self) -> Result<Option<Vec<u8>>>;    // UTF-8 bytes, trailing NUL removed
      pub fn rtf_body(&self) -> Result<Option<Vec<u8>>>;           // LZFu-decompressed RTF bytes
      pub fn html_body(&self) -> Result<Option<Vec<u8>>>;          // raw bytes, no charset conversion
      pub fn recipients(&self) -> Result<Option<Item<'f>>>;
      pub fn attachment_count(&self) -> Result<usize>;
      pub fn attachment(&self, index: usize) -> Result<Item<'f>>;
      // attachments
      pub fn attachment_type(&self) -> Result<AttachmentType>;
      pub fn attachment_data_size(&self) -> Result<u64>;
      pub fn attachment_data(&self, max_bytes: usize) -> Result<Option<Vec<u8>>>;   // None when size > max_bytes
      pub fn attached_item(&self) -> Result<Option<Item<'f>>>;   // embedded message of an Item-type attachment
  }
  pub fn filetime_to_unix_micros(filetime: u64) -> Option<i64>;   // None for 0
  pub mod tags { pub const MESSAGE_CLASS: u32 = 0x001a; SUBJECT 0x0037; SENDER_NAME 0x0c1a; SENDER_ADDRESS_TYPE 0x0c1e; SENDER_EMAIL_ADDRESS 0x0c1f; SENDER_SMTP_ADDRESS 0x5d01; INTERNET_MESSAGE_ID 0x1035; INTERNET_CODEPAGE 0x3fde; MESSAGE_CODEPAGE 0x3ffd; RECIPIENT_TYPE 0x0c15; DISPLAY_NAME 0x3001; ADDRESS_TYPE 0x3002; EMAIL_ADDRESS 0x3003; SMTP_ADDRESS 0x39fe; ATTACH_FILENAME 0x3704; ATTACH_METHOD 0x3705; ATTACH_LONG_FILENAME 0x3707; ATTACH_MIME_TAG 0x370e; ATTACH_CONTENT_ID 0x3712; ATTACH_SIZE 0x0e20; }
  ```

**Behavior:**
- Every libpff call: return `1` → `Ok(Some/value)`, `0` → `Ok(None)` (value not present), `-1` → `Err(Error { message })` where `message` is `libpff_error_sprint` output; the `libpff_error_t` is freed in every path.
- `File::open`: `libpff_file_initialize` + `libpff_file_open(path UTF-8, LIBPFF_OPEN_READ)` on Unix; on Windows the path is passed as UTF-16 through `libpff_file_open_wide` (the only platform branch: the narrow API uses the ANSI codepage on Windows). `Drop for File`: `libpff_file_close` then `libpff_file_free`. `Drop for Item`: `libpff_item_free`.
- `!Send`/`!Sync` via `PhantomData<*mut ()>` in both types (Part 01 §4.1 thread rule).
- Strings: call `*_size` first, allocate, read, strip the trailing NUL, `String::from_utf8_lossy` (libpff already converts MAPI strings to UTF-8).
- `attachment_data`: if `attachment_data_size > max_bytes` return `Ok(None)`; else seek 0 and read until the size is filled.
- No call to `libpff_set_codepage` anywhere.
- `build.rs`: copy `native/libpff` → `$OUT_DIR/libpff-src` (skip if `$OUT_DIR/libpff-src/.copied` exists); Unix: `sh configure --disable-nls --disable-shared-libs --with-zlib=no` inside the copy, then compile with `-DHAVE_CONFIG_H`; MSVC: no configure, defines `_CRT_SECURE_NO_WARNINGS`, `UNICODE`, `WINVER=0x0501`; both: `HAVE_LOCAL_<LIB>` for the 17 local libs, include dirs `.`, `common`, `include`, each `lib*`; compile every `.c` in `libpff/` and the 17 lib dirs; `cargo:rerun-if-changed=../../native/libpff/ORIGIN` and `=build.rs`.

**Constraints:**
- Deadline: `src-tauri/native/libpff/` committed before 2026-11-18.
- No bindgen, no zlib, no `libpff_set_codepage`.
- Only crate deps: `cc` (build). Dev-deps: none.

**Acceptance criteria:**
- [ ] `grep -q bb5bcc0 src-tauri/native/libpff/ORIGIN` and `test -f` of `src-tauri/native/libpff/{include/libpff.h,common/types.h,libpff/libpff_definitions.h}` exit 0.
- [ ] `cargo test -p pff --locked` exits 0 with tests `version_is_20260625`, `non_pst_is_error`, `outlook_pst_folders`, `outlook_pst_walk_is_deterministic`, `outlook_pst_welcome_message`, `filetime_conversion`.
- [ ] `cargo test -p pff --doc` exits 0 (`compile_fail` doctest: `File` is not `Send`).
- [ ] `otool -L $(ls -t target/debug/deps/outlook_pst-* | grep -v '\.d$' | head -1)` output does not contain `libz`.
- [ ] `grep -rn libpff_set_codepage src-tauri/crates` finds nothing.
- [ ] `grep -q CC-BY-4.0 tests/fixtures/external/NOTICE && grep -q c3631ff tests/fixtures/external/NOTICE` exits 0.
- [ ] `pwsh -NoProfile -Command "[ScriptBlock]::Create((Get-Content -Raw scripts/fixtures/make-pst.ps1)) | Out-Null"` exits 0.

### Task R1-03: Bundled PostgreSQL runtime: fetch script, Paths, PG Controller

**Files:**
- Create: `scripts/fetch-postgres.mjs`, `src-tauri/native/postgresql.sha256`, `core/src/paths/mod.rs`, `core/src/lifecycle/pg.rs`, `core/tests/pg_lifecycle.rs`.
- Modify: `core/src/lifecycle/mod.rs` (kickoff file; adds re-exports only).

**Interfaces:**
- Consumes: C1 `AppError`.
- Produces:
  - C6 `outlooker_core::paths::Paths`:
    ```rust
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Paths { app_local_data_dir: PathBuf, app_log_dir: PathBuf, pg_dir: PathBuf, data_root: PathBuf }
    impl Paths {
        pub fn new(app_local_data_dir: PathBuf, app_log_dir: PathBuf, pg_dir: PathBuf) -> Paths; // data_root = default_data_root()
        pub fn with_data_root(&self, data_root: PathBuf) -> Paths;
        pub fn bootstrap_file(&self) -> PathBuf;     // <app_local_data_dir>/settings.json
        pub fn default_data_root(&self) -> PathBuf;  // <app_local_data_dir>/data
        pub fn log_dir(&self) -> &Path;
        pub fn data_root(&self) -> &Path;
        pub fn marker(&self) -> PathBuf;             // <data_root>/outlooker-data.json
        pub fn credentials(&self) -> PathBuf;        // <data_root>/auth/credentials
        pub fn inflight_marker(&self) -> PathBuf;    // <data_root>/import-inflight
        pub fn pgdata(&self) -> PathBuf;             // <data_root>/pgdata
        pub fn pg_bin(&self, name: &str) -> PathBuf; // <pg_dir>/bin/<name><EXE_SUFFIX>
        pub fn pg_dir(&self) -> &Path;
    }
    pub fn dev_pg_dir() -> PathBuf; // <CARGO_MANIFEST_DIR of core>/../../resources/postgresql (tests, loadgen)
    ```
  - C7 `outlooker_core::lifecycle::pg`:
    ```rust
    #[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum StopMode { Fast, Immediate }
    #[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum PgStatus { Running { pid: u32 }, Stopped }
    #[derive(Debug, Clone)] pub struct PgController { paths: Paths }
    impl PgController {
        pub fn new(paths: Paths) -> PgController;
        pub fn locale_candidates() -> &'static [&'static str];
        /// initdb (locale candidates, self-check each, keep the first that passes), write
        /// postgresql.auto.conf + pg_hba.conf, CREATE DATABASE outlooker, CREATE EXTENSION pg_trgm.
        /// Leaves the cluster stopped. Returns the chosen locale.
        pub async fn init(&self, pgdata: &Path, admin_password: &str) -> Result<String, AppError>;
        /// Stale/orphan/PID-reuse recovery, port selection (≤ 3 tries), pg_ctl start -w -t 60. Returns the port.
        pub async fn start(&self, pgdata: &Path) -> Result<u16, AppError>;
        pub async fn stop(&self, pgdata: &Path, mode: StopMode) -> Result<(), AppError>;  // fast: -w -t 30, then immediate on timeout
        pub async fn status(&self, pgdata: &Path) -> Result<PgStatus, AppError>;
        pub fn last_log_line(&self, pgdata: &Path) -> Option<String>;   // newest file in pgdata/log
    }
    pub fn auto_conf(ram_bytes: u64) -> String;   // full postgresql.auto.conf text (Part 02 §2.3 step 5 + Part 04 §6.6)
    pub const HBA_CONF: &str = "host all all 127.0.0.1/32 scram-sha-256\n";
    pub const CJK_SELF_CHECK: &str = "SELECT similarity('測試測試','測試') > 0";
    ```

**Behavior:**
- Fetch script: `node scripts/fetch-postgres.mjs [--target <triple>] [--self-test]`. Default triple from `process.platform` + `process.arch` (`darwin/arm64` → `aarch64-apple-darwin`, `darwin/x64` → `x86_64-apple-darwin`, `win32/x64` → `x86_64-pc-windows-msvc`, `win32/arm64` → `aarch64-pc-windows-msvc`, `linux/x64` → `x86_64-unknown-linux-gnu`, `linux/arm64` → `aarch64-unknown-linux-gnu`). URL `https://github.com/theseus-rs/postgresql-binaries/releases/download/18.6.1/postgresql-18.6.1-<triple>.tar.gz`. Verify sha256; mismatch → exit 2 with "sha256 mismatch". Extract with `tar -xzf` into a temp dir, move the single top-level dir to `src-tauri/resources/postgresql/`, prune. Idempotent: if `src-tauri/resources/postgresql/.sha256` equals the pinned hash, exit 0 without download. `--self-test`: verifies that `verify(buffer, wrongHash)` throws, exits 0 if it did.
- Every child process (`initdb`, `pg_ctl`) runs with `LC_ALL=<candidate or C.UTF-8>`, `LANG` removed, `PGTZ` unset, and the PG `lib` dir on the dynamic loader path is not needed (macOS binaries use `@loader_path`).
- `init`: for each candidate: delete `pgdata` if this attempt created it, write a temp pwfile (deleted right after initdb), `initdb -D pgdata -U outlooker_admin --pwfile=<tmp> -A scram-sha-256 -E UTF8 --locale-provider=libc --locale=<c>`; on initdb failure try the next candidate; write `postgresql.auto.conf` (`auto_conf(ram)`), `pg_hba.conf` (`HBA_CONF`); `start`; connect to db `postgres` as admin, `CREATE DATABASE outlooker`; connect to `outlooker`, `CREATE EXTENSION IF NOT EXISTS pg_trgm`, run `CJK_SELF_CHECK`; `stop(Fast)`. First passing candidate wins (logged with `log::info!`). None passes → `AppError::Internal { detail: "database misconfigured: no locale candidate passed the CJK trigram self-check" }`, pgdata removed.
- `start`: (1) no `postmaster.pid` → step 3; (2) file exists: `pg_ctl status` running → `stop(Fast)` (orphan), continue; not running → continue (PostgreSQL removes a stale file); (3) bind `127.0.0.1:0`, read port, drop listener, `pg_ctl start -w -t 60 -D pgdata -l <pgdata>/log/startup.log -o "-p <port>"`; on output containing `could not bind` retry with a new port (3 tries total); on `lock file "postmaster.pid" already exists`: read PID (line 1), use `sysinfo` to find the process; if none or its executable is not `paths.pg_bin("postgres")` (canonicalized compare) → delete `postmaster.pid`, retry once; if it is a postgres binary of another user → `FileAccess { path: pgdata, reason: "inUseByOtherUser" }`. Other failure → `Internal { detail: last_log_line }`.
- `auto_conf(ram)`: `shared_buffers = clamp(ram/16, 256MB, 1GB)` written in MB, `effective_cache_size = ram/4` in MB, plus every fixed value of Part 04 §6.6 and `listen_addresses = '127.0.0.1'`, `unix_socket_directories = ''`, `logging_collector = on`, `log_directory = 'log'`, `log_rotation_size = 10MB`, `log_truncate_on_rotation = on`, `timezone = 'UTC'`. `synchronous_commit` stays server default `on`.
- RAM from `sysinfo::System::new_with_specifics(RefreshKind::nothing().with_memory(MemoryRefreshKind::nothing().with_ram())).total_memory()`.

**Constraints:**
- `tokio::process::Command` for every child; never a shell.
- Only `tokio-postgres` for the self-check connection (no pool in R1-03).
- Test helper code in `core/tests/pg_lifecycle.rs` only (TestCluster belongs to R1-04).

**Acceptance criteria:**
- [ ] `node scripts/fetch-postgres.mjs` exits 0; `test -x src-tauri/resources/postgresql/bin/postgres`; `ls src-tauri/resources/postgresql/lib/pg_trgm.*` lists a file; `ls src-tauri/resources/postgresql/lib/libLLVM* 2>/dev/null` lists nothing.
- [ ] `node scripts/fetch-postgres.mjs --self-test` exits 0.
- [ ] `cargo test -p outlooker-core --locked --test pg_lifecycle` exits 0 with tests `init_passes_cjk_self_check`, `c_locale_is_rejected`, `stop_start_round_trip`, `orphan_postmaster_is_restarted`, `stale_pid_with_dead_process`, `pid_reused_by_live_non_postgres_process`, `port_conflict_retries`, `auto_conf_values_are_live`.
- [ ] `cargo test -p outlooker-core --lib lifecycle::pg paths::` exits 0 (unit tests `auto_conf_clamps_shared_buffers`, `paths_layout`).

### Task R1-04: Database schema, roles, views, pools, `Db` handle, test cluster helper

**Files:**
- Create: `core/src/db/{mod,credentials,indexes,pools}.rs`, `core/src/db/migrations/V1__schema.sql`, `core/src/db/migrations/V2__roles_views.sql`, `core/src/test_support/mod.rs`, `core/tests/db_schema.rs`.

**Interfaces:**
- Consumes: C1 `AppError`; C6 `Paths`, `dev_pg_dir`; C7 `PgController`, `StopMode`.
- Produces (C8):
  ```rust
  // db::credentials
  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  pub struct Credentials { pub admin: String, pub app: String, pub ro: String }   // 64 lowercase hex chars each
  impl Credentials {
      pub fn generate() -> Credentials;                      // getrandom 32 bytes each, hex-encoded
      pub fn save(&self, path: &Path) -> Result<(), AppError>; // creates parent dir; mode 0600 on Unix; atomic temp + rename
      pub fn load(path: &Path) -> Result<Credentials, AppError>;
  }
  // db (mod.rs)
  pub const DB_NAME: &str = "outlooker";
  pub const RFC3339_UTC_FMT: &str = r#"YYYY-MM-DD"T"HH24:MI:SS"Z""#; // use as to_char(ts AT TIME ZONE 'UTC', <fmt>)
  pub async fn connect(port: u16, user: &str, password: &str, dbname: &str) -> Result<tokio_postgres::Client, AppError>; // NoTls, 127.0.0.1, spawns the connection task
  pub async fn connect_admin(port: u16, creds: &Credentials) -> Result<tokio_postgres::Client, AppError>;
  pub async fn migrate(admin: &mut tokio_postgres::Client, creds: &Credentials) -> Result<(), AppError>; // refinery + role passwords
  pub async fn cjk_self_check(client: &tokio_postgres::Client) -> Result<(), AppError>;
  pub fn pg_err(e: tokio_postgres::Error) -> AppError;   // Internal { detail: "<sqlstate>: <message>" }
  // db::pools
  #[derive(Clone)] pub struct Pools { pub app: deadpool_postgres::Pool, pub ro: deadpool_postgres::Pool }
  impl Pools { pub fn build(port: u16, creds: &Credentials) -> Result<Pools, AppError>; pub fn close(&self); }  // app max 8, ro max 2
  #[derive(Clone)] pub struct DbConn { pub port: u16, pub creds: Credentials, pub pools: Pools, pub data_root: PathBuf }
  #[derive(Clone, Default)] pub struct Db { /* Arc<RwLock<Option<DbConn>>> */ }
  impl Db {
      pub fn new() -> Db;
      pub fn set(&self, conn: Option<DbConn>);       // Supervisor only; None closes the old pools
      pub fn is_connected(&self) -> bool;
      pub fn app_pool(&self) -> Result<deadpool_postgres::Pool, AppError>;   // Err(DbNotReady) when None
      pub fn ro_pool(&self) -> Result<deadpool_postgres::Pool, AppError>;
      pub async fn connect_admin(&self) -> Result<tokio_postgres::Client, AppError>;
      pub async fn connect_app(&self) -> Result<tokio_postgres::Client, AppError>;  // dedicated (writer)
      pub fn data_root(&self) -> Result<PathBuf, AppError>;   // volume for free-space checks
  }
  // db::indexes
  pub const TSV_EXPR: &str = "to_tsvector('english'::regconfig, left(subject || ' ' || body_text, 100000))";  // generation expression of messages.body_tsv only
  pub struct SearchIndex { pub name: &'static str, pub ddl: &'static str }   // ddl = text after "CREATE INDEX ", no trailing ';'
  pub const SEARCH_INDEXES: [SearchIndex; 6];   // messages_sort, messages_body_trgm, messages_subject_trgm, messages_sender_trgm, messages_recipients_trgm, messages_body_fts
  pub async fn drop_search_indexes(admin: &tokio_postgres::Client) -> Result<(), AppError>;
  pub async fn ensure_search_indexes(admin: &tokio_postgres::Client, progress: &(dyn Fn(&str, usize, usize) + Send + Sync)) -> Result<(), AppError>; // (index name, indexes done, 6)
  pub async fn missing_or_invalid(admin: &tokio_postgres::Client) -> Result<Vec<&'static str>, AppError>;
  // test_support (feature "test-support")
  pub struct TestCluster { pub dir: tempfile::TempDir, pub paths: Paths, pub port: u16, pub creds: Credentials, pub db: Db, pub locale: String }
  impl TestCluster {
      pub async fn start() -> TestCluster;            // init + start + migrate + Db set
      pub async fn admin(&self) -> tokio_postgres::Client;
      pub async fn app(&self) -> tokio_postgres::Client;   // dedicated app-role connection
      pub async fn ro(&self) -> tokio_postgres::Client;    // read-only role connection
      pub async fn kill(&self);                       // Db unset, then pg_ctl stop -m immediate
      pub async fn restart(&mut self);                // start again (new port), rebuild Db
  }
  impl Drop for TestCluster { /* blocking pg_ctl stop -m immediate */ }
  ```

**Behavior:**
- `V1__schema.sql` = `CREATE ROLE outlooker_app LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOREPLICATION NOBYPASSRLS;` and the `CREATE ROLE outlooker_ro …` line of Part 04 §8.1 (roles first, so V1 can grant), then the SQL block of Part 04 §2 verbatim, then the SQL block of Part 04 §3 verbatim, then the four grant lines of §8.1 (`GRANT TEMP …`, `GRANT USAGE ON SCHEMA …`, `GRANT SELECT, INSERT, UPDATE, DELETE, MAINTAIN …`, `GRANT USAGE ON ALL SEQUENCES …`). `V2__roles_views.sql` = the `ALTER ROLE outlooker_ro SET …` and `REVOKE …` lines of §8.1, the six views of §8.2, `ALTER VIEW … OWNER TO outlooker_app` for each, `GRANT SELECT ON v_mail, … TO outlooker_ro;`, then the four grant lines again ("every migration re-runs the grants").
- `migrate`: `refinery::embed_migrations!("src/db/migrations")` runner on the admin client; then `ALTER ROLE outlooker_app PASSWORD '<hex>'` and same for `outlooker_ro` (passwords are 64 hex chars, validated by `Credentials::load`/`generate`, so no quoting is possible to break).
- `cjk_self_check`: false or error → `Internal { detail: "database misconfigured: CJK trigram self-check failed" }`.
- `Pools::build`: deadpool `Manager` with `RecyclingMethod::Fast`; app pool max 8 as `outlooker_app`; ro pool max 2 as `outlooker_ro`; host `127.0.0.1`, db `outlooker`.
- `ensure_search_indexes`: `SET maintenance_work_mem = '<min(1GB, RAM/8)>'`, `SET max_parallel_maintenance_workers = 2`; for each of `SEARCH_INDEXES` whose index is missing or invalid (`pg_index.indisvalid = false` → `DROP INDEX` first): `CREATE INDEX IF NOT EXISTS <ddl>`, then `progress(name, done, 6)`. Idempotent. Per-index granularity replaces `pg_stat_progress_create_index` block counts (the building connection is busy; block polling needs a second connection) — open item in Part C.
- `TestCluster::start` panics with `"PostgreSQL binaries missing: run node scripts/fetch-postgres.mjs"` when `dev_pg_dir()/bin/postgres` does not exist.

**Constraints:**
- No SQL in `db` that is built from user values.
- Each `"CREATE INDEX " + ddl` in `indexes.rs` occurs in `V1__schema.sql` after collapsing whitespace runs to one space in both (the spec aligns columns with spaces); `TSV_EXPR` occurs in it as the generation expression of `body_tsv`. Checked by a unit test.

**Acceptance criteria:**
- [ ] `cargo test -p outlooker-core --locked --features test-support --test db_schema` exits 0 with tests `migrations_apply_and_trgm_cjk`, `constraints_reject_bad_rows`, `ro_role_sees_only_views`, `app_role_privileges`, `admin_owns_tables`, `search_indexes_drop_and_ensure`, `db_handle_not_ready_when_unset`, `credentials_round_trip_and_mode`.
- [ ] `cargo test -p outlooker-core --lib db::indexes` exits 0 (test `ddl_matches_migration`).

### Task R2-08: Lifecycle Supervisor and Settings Store

**Files:**
- Create: `core/src/lifecycle/{supervisor,init,start,health,move_dir,status}.rs` (kickoff files), `core/src/settings/mod.rs`, `core/tests/lifecycle.rs`.
- Modify: `core/src/lifecycle/mod.rs` (re-exports), `core/src/paths/mod.rs` and `core/src/gate/mod.rs` only for defects found.

**Interfaces:**
- Consumes: C1, C2, C3, C6, C7, C8.
- Produces (C14 Supervisor / SettingsStore, C15):
  ```rust
  // settings
  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  pub struct Bootstrap { pub data_dir: Option<String>, pub language: Language }   // settings.json; default { None, ZhTw }
  #[derive(Clone)] pub struct SettingsStore { /* bootstrap path + Arc<Mutex<(Bootstrap, u8 /*last known webp_quality*/)>> */ }
  impl SettingsStore {
      pub fn load(bootstrap_file: PathBuf) -> Result<SettingsStore, AppError>;     // missing file → default
      pub fn bootstrap(&self) -> Bootstrap;
      pub fn set_data_dir(&self, data_dir: &str) -> Result<(), AppError>;          // atomic temp + rename
      pub async fn get(&self, db: &Db) -> Result<Settings, AppError>;              // webp from DB when connected, else last known (default 80)
      pub async fn update(&self, db: &Db, patch: SettingsPatch) -> Result<Settings, AppError>;
      pub async fn webp_quality(&self, db: &Db) -> Result<u8, AppError>;           // DB row; Err(DbNotReady) when not connected
  }
  // lifecycle::supervisor
  #[derive(Clone)] pub struct Supervisor { /* Arc<Inner> */ }
  #[derive(Debug, Clone)] pub struct SupervisorConfig { pub restart_budget: u32, pub restart_window: Duration, pub probe_interval: Duration } // Default = 3, 10 min, 5 s; tests use a 100 ms probe (the 10 min window is longer than any test, so no clock seam is needed)
  impl Supervisor {
      pub fn new(paths: Paths, settings: SettingsStore, gate: Gate, db: Db, sink: Arc<dyn EventSink>, config: SupervisorConfig) -> Supervisor;
      pub fn phase(&self) -> AppPhase;
      pub fn phase_watch(&self) -> tokio::sync::watch::Receiver<AppPhase>;           // C15
      pub async fn status(&self) -> AppStatus;                                       // app_status
      pub async fn boot(&self) -> Result<(), AppError>;                              // NeedsSetup, or start (02 §2.2 steps 2–7); spawns the health monitor on Ready
      pub async fn initialize(&self, req: InitializeRequest) -> Result<(), AppError>; // app_initialize: validates, then spawns init; returns at once
      pub async fn retry(&self) -> Result<(), AppError>;                             // app_retry
      pub async fn move_data_dir(&self, target: String) -> Result<(), AppError>;     // validates, spawns the move; returns at once
      pub async fn db_status(&self, maintenance: Option<MaintenanceState>) -> Result<DbStatus, AppError>;
      pub async fn shutdown_db(&self) -> Result<(), AppError>;                       // 02 §2.5 steps 3–4
      pub fn require_ready(&self) -> Result<(), AppError>;                           // Err(DbNotReady) unless Ready
  }
  // test hooks (cfg(any(test, feature = "test-support")))
  impl Supervisor { pub fn fail_next_move_verify(&self); }
  ```

**Behavior:**
- Phase transitions only through one private `set_phase(phase, error)` that updates the watch channel, the stored error, and emits `CoreEvent::AppStatusChanged(status())`.
- `boot`: load bootstrap; no `data_dir`, or no marker → `NeedsSetup` (`data_dir` in `AppStatus` = proposed default). Else `start_cluster()`.
- `start_cluster()` (shared by boot, retry, health restart, move): marker check; `pgdata/PG_VERSION` must read `18` (else `Failed` with `Internal { detail: "unsupported PostgreSQL data version <v>" }`); `StartingDb`; `PgController::start`; `Credentials::load`; `connect_admin`; `SELECT 1`; `migrate`; `cjk_self_check`; `Pools::build`; `db.set(Some(..))`; `Ready`; spawn the health monitor once.
- `initialize(req)`: refuses unless `NeedsSetup` with `InvalidInput { field: "phase", reason: "alreadyInitialized" }` (Part 03 §4 says `busy`, but no `BusyReason` value fits; this deviation is listed in §C open items); validates `req.data_dir`: absolute; if it exists and contains the marker → adopt (write bootstrap, `start_cluster()`); if it exists, is non-empty and has no marker → `InvalidInput { field: "dataDir", reason: "notEmpty" }`; not writable → `FileAccess`. Otherwise spawn: write bootstrap (data_dir, language), create root + marker (`pg_major: 18`, chosen locale is written after init), `Credentials::generate().save`, `InitializingCluster`, `PgController::init`, update marker locale, `start_cluster()`. Any failure → delete `pgdata` only if this attempt created it → `Failed` with the error.
- `retry`: only from `Failed` (`InvalidInput { field: "phase", reason: "notFailed" }`); resets the restart history; if `paths.pgdata()` exists → spawn `start_cluster()`; else → `NeedsSetup`.
- Health monitor: `tokio::time::interval(probe_interval)`; probe `SELECT 1` on an app-pool connection with a 2 s timeout; two consecutive failures → `pg_ctl status`; if down: `db.set(None)`, `StartingDb`, record restart time; if more than `restart_budget` restarts inside `restart_window` → `Failed` with `Internal { detail: last_log_line }`; else `start_cluster()`.
- `move_data_dir(target)`: `require_ready`; `gate.try_enter(DataDirMove)`; target absolute, empty or non-existent (`InvalidInput { field: "target", reason: "notEmpty" | "notAbsolute" }`); free space at target ≥ 1.1 × size of data root (`FileAccess { reason: "insufficientSpace" }`); spawn: emit `DataDirMoveProgress` phases `StoppingDb` (db.set(None), `StoppingDb`, stop Fast) → `Copying` (recursive copy, progress ≤ every 250 ms by bytes) → `Verifying` (file count + total bytes equal; `fail_next_move_verify` forces a mismatch) → write bootstrap `data_dir = target` → `StartingDb` with `paths.with_data_root(target)` → `Done`, delete old root. Failure after copy began: delete the partial target, restore bootstrap and paths to the old root, `start_cluster()` there, emit `Failed` with the error.
- `db_status(maintenance)`: the four queries of Part 04 §9.2 on the app pool; `IndexStatus.table` = table relname; timestamps via `RFC3339_UTC_FMT`; `counts` = `SELECT (SELECT count(*) FROM messages), (SELECT count(*) FROM pst_sources)`.
- `status()`: `counts` zeros unless `Ready`; `language` from bootstrap; `data_dir` = bootstrap data_dir or proposed default.
- `shutdown_db`: `StoppingDb`, `db.set(None)`, `PgController::stop(Fast)` (falls back to Immediate inside C7).
- Settings `update`: `language` only → bootstrap write, works in any phase; `webp_quality` → validate 1..=100 (`InvalidInput { field: "webpQuality", reason: "outOfRange" }`), `UPDATE settings SET webp_quality = $1, updated_at = now()` (Err `DbNotReady` when not connected).

**Constraints:**
- `lifecycle` and `settings` import no `importer`, `search`, `sql_console`, `maintenance` (grep check).
- Health monitor and move use `tokio::time::interval` / events, never sleep loops.

**Acceptance criteria:**
- [ ] `cargo test -p outlooker-core --locked --features test-support --test lifecycle` exits 0 with tests `first_run_reaches_ready`, `adopt_existing_data_root`, `wrong_pg_version_fails`, `init_failure_removes_only_own_pgdata`, `retry_from_failed_reaches_ready`, `retry_when_not_failed_is_invalid`, `retry_without_cluster_returns_to_setup`, `health_restarts_killed_postmaster`, `restart_budget_exhaustion_fails`, `move_data_dir_succeeds`, `move_verify_failure_rolls_back`, `move_into_non_empty_is_invalid`, `move_while_import_is_busy`, `settings_language_before_ready`, `settings_webp_before_ready_is_not_ready`, `bootstrap_write_is_atomic`.
- [ ] `grep -rE "crate::(importer|search|sql_console|maintenance)" src-tauri/crates/outlooker-core/src/lifecycle src-tauri/crates/outlooker-core/src/settings` finds nothing.

### Task R1-REV / R2-REV: Round reviewers
Scope, ownership and acceptance as parent plan §3 R1-REV and §4 R2-REV. Stage 2 below gives the exact review procedure.

### Windows phase (parent plan §7.3)
Steps W1–W3 as parent plan §7.3. Stage 2 below gives the exact remote command sequence.

---

# Part B — Stage 2 steps

## R1-01 steps

Commit-message files go to `scripts/tmp/msg-<task>.txt` (scratch, gitignored). Every commit: `git rev-parse --abbrev-ref HEAD` first, `git add <listed paths>`, `git commit -F scripts/tmp/msg-r1-01.txt -- <listed paths>`, then `git status --porcelain` must not list your paths.

- [ ] **Step 1: Re-check registry versions.** Run `npm view <pkg> version` for each npm package below and `curl -s https://crates.io/api/v1/crates/<crate> | jq -r .crate.max_stable_version` for each crate. Use the plan version unless the registry shows a newer patch of the same minor; record any change in the commit message.

- [ ] **Step 2: Write the Rust manifests.**

`src-tauri/rust-toolchain.toml`:
```toml
[toolchain]
channel = "1.99.1"
components = ["rustfmt", "clippy"]
```

`src-tauri/Cargo.toml`:
```toml
[workspace]
members = [".", "crates/pff", "crates/outlooker-core", "crates/outlooker-loadgen"]
resolver = "3"

[package]
name = "outlooker"
version = "0.1.0"
edition = "2024"
rust-version = "1.99"

[lib]
name = "outlooker_lib"
crate-type = ["rlib"]

[build-dependencies]
tauri-build = { version = "=2.7.4", features = [] }

[dependencies]
tauri = { version = "=2.12.5", features = [] }
tauri-plugin-dialog = "=2.8.3"
tauri-plugin-log = "=2.10.1"
tauri-plugin-single-instance = "=2.5.4"
tauri-plugin-wdio-webdriver = { version = "=1.5.0", optional = true }
serde = { version = "=1.0.229", features = ["derive"] }
serde_json = "=1.0.154"
log = "=0.4.34"
tokio = { version = "=1.54.0", features = ["rt-multi-thread", "macros", "sync", "time"] }
outlooker-core = { path = "crates/outlooker-core" }

[features]
e2e = ["dep:tauri-plugin-wdio-webdriver"]

# Dependencies optimized in dev/test builds: loadgen and unit tests 30.7 s -> 3.7 s, keeps gates under the foreground timeout.
[profile.dev.package."*"]
opt-level = 3
```

`src-tauri/crates/outlooker-core/Cargo.toml`:
```toml
[package]
name = "outlooker-core"
version = "0.1.0"
edition = "2024"

[dependencies]
pff = { path = "../pff" }
serde = { version = "=1.0.229", features = ["derive"] }
serde_json = "=1.0.154"
thiserror = "=2.0.21"
log = "=0.4.34"
ts-rs = "=12.0.1"
tokio = { version = "=1.54.0", features = ["rt-multi-thread", "macros", "process", "sync", "time", "fs", "net", "io-util"] }
tokio-postgres = "=0.7.18"
deadpool-postgres = "=0.14.2"
refinery = { version = "=0.10.0", features = ["tokio-postgres"] }
ammonia = "=4.2.4"
html2text = "=0.17.4"
encoding_rs = "=0.8.42"
chardetng = "=1.0.0"
image = "=0.25.10"
webp = "=0.3.1"
rayon = "=1.12.0"
blake3 = "=1.8.7"
getrandom = "=0.4.3"
sysinfo = "=0.39.6"
base64 = "=0.23.1"
tempfile = { version = "=3.27.0", optional = true }

[dev-dependencies]
jsonschema = { version = "=0.59.0", default-features = false }
tempfile = "=3.27.0"

[features]
test-support = ["dep:tempfile"]
outlook-fixtures = ["test-support"]
```

`src-tauri/crates/pff/Cargo.toml`:
```toml
[package]
name = "pff"
version = "0.1.0"
edition = "2024"
links = "pff"
build = "build.rs"

[build-dependencies]
cc = "=1.9.0"
```
`src-tauri/crates/pff/build.rs` (kickoff placeholder, replaced by R1-02): `fn main() {}`; `src-tauri/crates/pff/src/lib.rs`: empty.

`src-tauri/crates/outlooker-loadgen/Cargo.toml`:
```toml
[package]
name = "outlooker-loadgen"
version = "0.1.0"
edition = "2024"
publish = false

[dependencies]
outlooker-core = { path = "../outlooker-core", features = ["test-support"] }
tokio = { version = "=1.54.0", features = ["rt-multi-thread", "macros"] }
```
`src-tauri/crates/outlooker-loadgen/src/main.rs`: `fn main() {}`.

`src-tauri/.cargo/config.toml`:
```toml
[env]
TS_RS_EXPORT_DIR = { value = "../src/api/bindings", relative = true }
TS_RS_LARGE_INT = "number"
```

- [ ] **Step 3: Write the npm manifest and frontend config.**

`mise.toml` (repo root; mise is the Node manager on the dev machine):
```toml
[tools]
node = "24.22.1"
```
Run: `mise trust && node --version`. Expected: `v24.22.1` (run every later npm command in this repo so mise selects it).


`package.json`:
```json
{
  "name": "outlooker",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "engines": { "node": ">=24.22 <25" },
  "scripts": {
    "dev": "vite",
    "build": "tsc --noEmit -p . && vite build",
    "typecheck": "tsc --noEmit -p .",
    "test": "vitest run",
    "e2e": "wdio run wdio.conf.ts",
    "tauri": "tauri"
  },
  "dependencies": {
    "@codemirror/lang-sql": "6.10.0",
    "@codemirror/state": "6.7.6",
    "@codemirror/view": "6.43.15",
    "@fluentui/react-components": "9.74.10",
    "@fluentui/react-icons": "2.0.344",
    "@tanstack/react-query": "5.104.5",
    "@tanstack/react-virtual": "3.14.21",
    "@tauri-apps/api": "2.12.4",
    "@tauri-apps/plugin-dialog": "2.8.3",
    "@tauri-apps/plugin-log": "2.10.1",
    "codemirror": "6.0.2",
    "i18next": "26.5.4",
    "react": "19.3.1",
    "react-dom": "19.3.1",
    "react-i18next": "17.1.0"
  },
  "devDependencies": {
    "@tauri-apps/cli": "2.12.4",
    "@testing-library/dom": "10.4.2",
    "@testing-library/react": "16.3.3",
    "@types/react": "19.3.0",
    "@types/react-dom": "19.3.0",
    "@vitejs/plugin-react": "6.1.2",
    "@wdio/cli": "9.32.0",
    "@wdio/mocha-framework": "9.32.0",
    "@wdio/tauri-service": "1.5.0",
    "jsdom": "30.1.3",
    "typescript": "7.0.2",
    "vite": "8.3.5",
    "vitest": "5.0.5",
    "webdriverio": "9.32.0"
  }
}
```
Then `npm install --save-exact` (creates `package-lock.json`). If `npx tsc --version` does not print `Version 7.0.2` (TypeScript 7 may ship its compiler under another binary name), use the binary that the installed `typescript@7.0.2` package exposes in `node_modules/.bin` in the two scripts, and note it in the commit message.

`tsconfig.json`:
```json
{
  "compilerOptions": {
    "target": "ES2022",
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "moduleResolution": "bundler",
    "jsx": "react-jsx",
    "strict": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "skipLibCheck": true,
    "types": ["vite/client", "vitest/globals"]
  },
  "include": ["src"]
}
```

`vite.config.ts`:
```ts
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["src/test/setup.ts"],
    include: ["src/**/*.test.{ts,tsx}"],
  },
});
```

`src/test/setup.ts`:
```ts
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

afterEach(() => cleanup());

// jsdom has no layout. CodeMirror measures text through Range rects; empty rects make it fall back to defaults.
const emptyRect = { x: 0, y: 0, width: 0, height: 0, top: 0, left: 0, right: 0, bottom: 0, toJSON: () => ({}) } as DOMRect;
if (!Range.prototype.getClientRects) Range.prototype.getClientRects = () => [] as unknown as DOMRectList;
if (!Range.prototype.getBoundingClientRect) Range.prototype.getBoundingClientRect = () => emptyRect;
```

`index.html`:
```html
<!doctype html>
<html lang="zh-TW">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Outlooker</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

`src/main.tsx` (R1-06 replaces the inner component later through `src/app/`, but this file stays as is):
```tsx
import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./app/App";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
```

`.gitignore` (append to the existing file):
```
src-tauri/target/
src-tauri/resources/postgresql/
src-tauri/gen/
```

- [ ] **Step 4: Write the kickoff stubs.** Each file below with exactly this content (owners in parent §5 replace them later).

`src/app/App.tsx` (R1-06):
```tsx
export function App() {
  return null;
}
```
`src/app/AppStatusContext.tsx` (R1-06):
```tsx
import type { AppStatus } from "../api/bindings/AppStatus";

const INITIAL: AppStatus = {
  phase: "needsSetup",
  dataDir: "",
  language: "zh-TW",
  counts: { messages: 0, psts: 0 },
  error: null,
};

export function useAppStatus(): AppStatus {
  return INITIAL;
}

export type ScreenId = "search" | "advancedSql" | "import" | "settings";

export function useNavigation(): { screen: ScreenId; go(screen: ScreenId): void } {
  return { screen: "search", go: () => {} };
}
```
`src/components/common/index.ts` (R1-06 replaces it; prop shape = planner-3's C10 block in the frontend plan):
```ts
import type { JSX, ReactNode } from "react";
import type { TFunction } from "i18next";
import type { AppError } from "../../api/bindings/AppError";

export interface EmptyStateProps {
  icon: JSX.Element;
  title: string;
  body?: string;
  action?: ReactNode;
  testId?: string;
}

export function EmptyState(_props: EmptyStateProps): JSX.Element | null {
  return null;
}

export function formatAppError(err: AppError, t: TFunction): string {
  return t(`common:error.${err.kind}`);
}

export function toAppError(e: unknown): AppError {
  return typeof e === "object" && e !== null && typeof (e as { kind?: unknown }).kind === "string"
    ? (e as AppError)
    : { kind: "internal", detail: String(e) };
}

export function formatNumber(n: number, lng: string): string {
  return n.toLocaleString(lng);
}

export function formatBytes(n: number, lng: string): string {
  return n.toLocaleString(lng);
}

export function formatDateTime(iso: string | null, _lng: string): string {
  return iso ?? "";
}

export function formatDay(iso: string | null, _lng: string): string {
  return iso ?? "";
}
```
`src/screens/{search,advanced-sql,import,settings,onboarding}/index.tsx` — one each:
```tsx
export function SearchScreen() {
  return null;
}
```
(names: `SearchScreen`, `AdvancedSqlScreen`, `ImportScreen`, `SettingsScreen`, `OnboardingScreen`).
`src/locales/{zh-TW,en}/{common,onboarding,import,settings,search,sql}.json`: `{}` plus a newline (12 files).

Rust kickoff files (empty unless shown): `core/src/lib.rs`:
```rust
//! Outlooker core services. No Tauri dependency (spec Part 02 §3).
pub mod app;
pub mod db;
pub mod dto;
pub mod gate;
pub mod importer;
pub mod lifecycle;
pub mod maintenance;
pub mod paths;
pub mod search;
pub mod settings;
pub mod sink;
pub mod sql_console;
#[cfg(feature = "test-support")]
pub mod test_support;
```
`core/src/lifecycle/mod.rs`: `pub mod health; pub mod init; pub mod move_dir; pub mod pg; pub mod start; pub mod status; pub mod supervisor;` (one per line). `core/src/importer/mod.rs`: `pub mod dedupe; pub mod inflight; pub mod normalize; pub mod progress; pub mod pst_reader; pub mod queue; pub mod remove; pub mod types; pub mod webp; pub mod writer;`. `core/src/dto/mod.rs`: `pub mod app; pub mod commands; pub mod error; pub mod events; pub mod import; pub mod search; pub mod settings; pub mod sql;`. Empty files: `core/src/{sink.rs}`, `core/src/{app,db,gate,paths,search,settings,sql_console,maintenance,test_support}/mod.rs`, `core/src/importer/normalize/mod.rs`, every file named in the two `mod` lists above, `core/src/dto/*.rs`.

`src-tauri/build.rs`:
```rust
fn main() {
    tauri_build::build()
}
```
`src-tauri/src/main.rs`:
```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    outlooker_lib::run()
}
```
`src-tauri/src/lib.rs`:
```rust
mod commands;
mod events;
mod mailimg;

use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_log::Builder::new().build())
        .run(tauri::generate_context!())
        .expect("error while running Outlooker");
}
```
`src-tauri/src/commands/mod.rs`: `pub mod app; pub mod import; pub mod search; pub mod settings; pub mod sql;` and the five files empty; `src-tauri/src/events.rs`, `src-tauri/src/mailimg.rs` empty.

`src-tauri/tauri.conf.json`:
```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Outlooker",
  "version": "0.1.0",
  "identifier": "us.jhangy.outlooker",
  "build": {
    "beforeDevCommand": "npm run dev",
    "devUrl": "http://localhost:1420",
    "beforeBuildCommand": "npm run build",
    "frontendDist": "../dist"
  },
  "app": {
    "windows": [
      { "label": "main", "title": "Outlooker", "width": 1920, "height": 1080, "minWidth": 1280, "minHeight": 720 }
    ],
    "security": {
      "csp": "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: mailimg: http://mailimg.localhost; frame-src 'self'; connect-src ipc: http://ipc.localhost; font-src 'self' data:"
    }
  },
  "bundle": {
    "active": true,
    "targets": ["dmg", "nsis"],
    "icon": ["icons/32x32.png", "icons/128x128.png", "icons/128x128@2x.png", "icons/icon.icns", "icons/icon.ico"]
  }
}
```
`style-src 'unsafe-inline'` is required (accepted deviation, Part C.2): Fluent UI v9 (Griffel) inserts style elements at runtime, and sanitized mail HTML keeps inline `style` attributes (Part 04 §5.6); `srcdoc` iframes inherit this CSP. Scripts stay `'self'` only.

`src-tauri/capabilities/default.json`:
```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Main window permissions",
  "windows": ["main"],
  "permissions": ["core:default", "dialog:allow-open", "log:default"]
}
```
Icons: create a 1024×1024 PNG with any solid brand colour `#0f6cbd` at `scripts/tmp/icon.png` (scratch), then `npx tauri icon scripts/tmp/icon.png -o src-tauri/icons`.

- [ ] **Step 5: Verify and land the kickoff commit.**

Run: `cd src-tauri && cargo build --workspace --locked` (first run without `--locked` to create `Cargo.lock`, then again with it). Expected: exit 0.
Run: `ls src-tauri/resources 2>/dev/null; echo "rc=$?"` Expected: `rc=1` (directory absent, so the build proves no resource dependency).
Run: `npm run typecheck`. Expected: exit 0.
Run: `cd src-tauri && rustc --version`. Expected: `rustc 1.99.1 (...)`.
Run: `node --version` (repo root). Expected: `v24.22.1`.
Run: `grep -nE '"[\^~]' package.json; echo "rc=$?"`. Expected: `rc=1`.
Commit all Step 2–4 paths with message `chore: kickoff skeleton (workspace, manifests, module stubs)`.

- [ ] **Step 6: Write the failing DTO serialization tests.** Append to `core/src/dto/error.rs` (the type does not exist yet, so the test fails to compile):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_error_wire_shape() {
        let e = AppError::QueryTimeout { timeout_ms: 30000 };
        assert_eq!(serde_json::to_string(&e).unwrap(), r#"{"kind":"queryTimeout","timeoutMs":30000}"#);
        let b = AppError::Busy { reason: BusyReason::ImportRunning };
        assert_eq!(serde_json::to_string(&b).unwrap(), r#"{"kind":"busy","reason":"importRunning"}"#);
        assert_eq!(serde_json::to_string(&AppError::DbNotReady).unwrap(), r#"{"kind":"dbNotReady"}"#);
        let s = AppError::SqlError { sqlstate: "42601".into(), message: "x".into(), position: None };
        assert_eq!(serde_json::to_string(&s).unwrap(), r#"{"kind":"sqlError","sqlstate":"42601","message":"x","position":null}"#);
    }
}
```
Run: `cargo test -p outlooker-core --lib dto::error`. Expected: compile error "cannot find type `AppError`".

- [ ] **Step 7: Write `core/src/dto/error.rs`.**
```rust
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, thiserror::Error)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum AppError {
    #[error("database not ready")]
    DbNotReady,
    #[error("{entity} {id} not found")]
    NotFound { entity: String, id: String },
    #[error("invalid input {field}: {reason}")]
    InvalidInput { field: String, reason: String },
    #[error("query timed out after {timeout_ms} ms")]
    QueryTimeout { timeout_ms: u32 },
    #[error("cancelled")]
    Cancelled,
    #[error("SQL error {sqlstate}: {message}")]
    SqlError { sqlstate: String, message: String, position: Option<u32> },
    #[error("read-only violation")]
    ReadOnlyViolation,
    #[error("busy: {reason:?}")]
    Busy { reason: BusyReason },
    #[error("file access {path}: {reason}")]
    FileAccess { path: String, reason: String },
    #[error("not a PST file: {path}")]
    NotAPst { path: String },
    #[error("already imported as PST {pst_id}")]
    AlreadyImported { pst_id: i64 },
    #[error("internal error: {detail}")]
    Internal { detail: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum BusyReason {
    ImportRunning,
    MaintenanceRunning,
    DataDirMoving,
    PstRemoving,
}

impl AppError {
    pub fn invalid(field: &str, reason: &str) -> AppError {
        AppError::InvalidInput { field: field.to_owned(), reason: reason.to_owned() }
    }
    pub fn internal(detail: impl Into<String>) -> AppError {
        AppError::Internal { detail: detail.into() }
    }
    pub fn file_access(path: &std::path::Path, reason: &str) -> AppError {
        AppError::FileAccess { path: path.display().to_string(), reason: reason.to_owned() }
    }
    pub fn not_found(entity: &str, id: impl ToString) -> AppError {
        AppError::NotFound { entity: entity.to_owned(), id: id.to_string() }
    }
}
```
(The Step 6 test module stays at the end of the file.)

- [ ] **Step 8: Write `core/src/dto/app.rs`.**
```rust
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::error::AppError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AppStatus {
    pub phase: AppPhase,
    pub data_dir: String,
    pub language: Language,
    pub counts: LibraryCounts,
    pub error: Option<AppError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum AppPhase {
    NeedsSetup,
    InitializingCluster,
    StartingDb,
    Ready,
    StoppingDb,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct LibraryCounts {
    pub messages: i64,
    pub psts: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum Language {
    #[default]
    #[serde(rename = "zh-TW")]
    ZhTw,
    #[serde(rename = "en")]
    En,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct InitializeRequest {
    pub data_dir: String,
    pub language: Language,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_and_phase_wire_values() {
        assert_eq!(serde_json::to_string(&Language::ZhTw).unwrap(), r#""zh-TW""#);
        assert_eq!(serde_json::to_string(&AppPhase::InitializingCluster).unwrap(), r#""initializingCluster""#);
    }
}
```

- [ ] **Step 9: Write `core/src/dto/search.rs`.** Types and fields exactly as Part 03 §5.1–§5.5 (comments there are the field docs):
```rust
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SearchQuery {
    Simple { text: String },
    Dsl { root: ConditionGroup },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ConditionGroup {
    pub op: GroupOp,
    pub children: Vec<ConditionNode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum GroupOp {
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ConditionNode {
    Group { op: GroupOp, children: Vec<ConditionNode> },
    Sender { value: String },
    Recipient { value: String, kinds: Vec<RecipientKind> },
    Subject { value: String },
    Body { value: String },
    DateRange { from: Option<String>, to: Option<String> },
    Folder { folder_path: String, include_subfolders: bool },
    HasAttachment { value: bool },
    SourcePst { pst_ids: Vec<i64> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum RecipientKind {
    To,
    Cc,
    Bcc,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    pub query: SearchQuery,
    pub cursor: Option<String>,
    pub page_size: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SearchPage {
    pub rows: Vec<MailRow>,
    pub next_cursor: Option<String>,
    pub short_query: bool,
    pub elapsed_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct MailRow {
    pub id: i64,
    pub sent_at: Option<String>,
    pub from: Address,
    pub to_preview: String,
    pub recipient_count: u32,
    pub subject: String,
    pub snippet: String,
    pub has_attachments: bool,
    pub occurrence_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct Address {
    pub name: Option<String>,
    pub address: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SearchCount {
    pub count: i64,
    pub capped: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct FilterOptions {
    pub psts: Vec<PstRef>,
    pub folders: Vec<FolderRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct PstRef {
    pub id: i64,
    pub display_name: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct FolderRef {
    pub folder_path: String,
    pub pst_id: i64,
    pub message_count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct MailDetail {
    pub id: i64,
    pub message_id: Option<String>,
    pub subject: String,
    pub sent_at: Option<String>,
    pub received_at: Option<String>,
    pub from: Address,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub bcc: Vec<Address>,
    pub body_html: Option<String>,
    pub body_text: String,
    pub attachments: Vec<AttachmentMeta>,
    pub occurrences: Vec<Occurrence>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentMeta {
    pub id: i64,
    pub file_name: String,
    pub size_bytes: i64,
    pub mime_type: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct Occurrence {
    pub pst_id: i64,
    pub pst_display_name: String,
    pub pst_path: String,
    pub folder_path: String,
    pub pst_item_id: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Part 03 §5.1 worked example, built from Rust values.
    pub fn worked_example() -> SearchRequest {
        SearchRequest {
            query: SearchQuery::Dsl {
                root: ConditionGroup {
                    op: GroupOp::And,
                    children: vec![
                        ConditionNode::Group {
                            op: GroupOp::Or,
                            children: vec![
                                ConditionNode::Sender { value: "contoso.com".into() },
                                ConditionNode::Recipient { value: "alice".into(), kinds: vec![] },
                            ],
                        },
                        ConditionNode::DateRange {
                            from: Some("2022-12-31T16:00:00Z".into()),
                            to: Some("2023-12-31T16:00:00Z".into()),
                        },
                        ConditionNode::Body { value: "報價".into() },
                        ConditionNode::HasAttachment { value: true },
                        ConditionNode::SourcePst { pst_ids: vec![3, 7] },
                    ],
                },
            },
            cursor: None,
            page_size: Some(200),
        }
    }

    #[test]
    fn worked_example_matches_spec_json() {
        let expected: serde_json::Value = serde_json::json!({
            "query": { "kind": "dsl", "root": { "op": "and", "children": [
                { "type": "group", "op": "or", "children": [
                    { "type": "sender", "value": "contoso.com" },
                    { "type": "recipient", "value": "alice", "kinds": [] } ] },
                { "type": "dateRange", "from": "2022-12-31T16:00:00Z", "to": "2023-12-31T16:00:00Z" },
                { "type": "body", "value": "報價" },
                { "type": "hasAttachment", "value": true },
                { "type": "sourcePst", "pstIds": [3, 7] } ] } },
            "cursor": null,
            "pageSize": 200
        });
        assert_eq!(serde_json::to_value(worked_example()).unwrap(), expected);
    }

    #[test]
    fn worked_example_root_validates_against_schema() {
        let schema_text = include_str!("../../../../schemas/condition-group.json");
        let schema: serde_json::Value = serde_json::from_str(schema_text).unwrap();
        let validator = jsonschema::validator_for(&schema).expect("schema compiles");
        let SearchQuery::Dsl { root } = worked_example().query else { unreachable!() };
        let instance = serde_json::to_value(root).unwrap();
        assert!(validator.is_valid(&instance));
        let bad = serde_json::json!({ "op": "and", "children": [] });
        assert!(!validator.is_valid(&bad), "minItems 1 must reject an empty group");
    }
}
```
The `include_str!` path is relative to `core/src/dto/search.rs`: `../../../../schemas/` = `src-tauri/schemas/`.

- [ ] **Step 10: Write `src-tauri/schemas/condition-group.json`** by extracting the spec block (exact bytes):
```bash
awk '/^#### JSON schema \(DSL root\)/{f=1} f&&/^```json/{p=1;next} p&&/^```/{exit} p' docs/superpowers/specs/2026-10-21-outlooker-design.md > src-tauri/schemas/condition-group.json
head -c 60 src-tauri/schemas/condition-group.json
```
Expected first line: `{`, and `node -e "JSON.parse(require('fs').readFileSync('src-tauri/schemas/condition-group.json','utf8'))"` exits 0.

- [ ] **Step 11: Write `core/src/dto/sql.rs`.**
```rust
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::search::MailRow;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SavedQuery {
    pub id: i64,
    pub name: String,
    pub sql: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SavedQueryInput {
    pub name: String,
    pub sql: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SqlExecuteRequest {
    pub execution_id: String,
    pub sql: String,
    pub max_rows: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "mode", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SqlResult {
    MailList { rows: Vec<MailRow>, missing_ids: u32, truncated: bool, elapsed_ms: u32 },
    Table { columns: Vec<ColumnMeta>, rows: Vec<Vec<Option<String>>>, truncated: bool, elapsed_ms: u32 },
    Command { tag: String, elapsed_ms: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ColumnMeta {
    pub name: String,
    pub pg_type: String,
}
```

- [ ] **Step 12: Write `core/src/dto/import.rs`.**
```rust
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::error::AppError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct PstSource {
    pub id: i64,
    pub path: String,
    pub display_name: String,
    pub file_size_bytes: i64,
    pub status: ImportStatus,
    pub progress: Option<ImportProgress>,
    pub message_count: i64,
    pub new_message_count: i64,
    pub error_count: i64,
    pub queued_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum ImportStatus {
    Queued,
    Running,
    Cancelling,
    Cancelled,
    Failed,
    Completed,
    Removing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ImportProgress {
    pub pst_id: i64,
    pub phase: ImportPhase,
    pub items_total: Option<i64>,
    pub items_done: i64,
    pub stored: i64,
    pub duplicates: i64,
    pub errors: i64,
    pub skipped_non_mail: i64,
    pub images_converted: i64,
    pub bytes_per_sec: Option<i64>,
    pub eta_seconds: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum ImportPhase {
    Scanning,
    Importing,
    Finalizing,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "result", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum EnqueueResult {
    Queued { pst: PstSource },
    Rejected { path: String, error: AppError },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ImportErrorsRequest {
    pub pst_id: i64,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ImportErrorPage {
    pub rows: Vec<ImportErrorEntry>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ImportErrorEntry {
    pub id: i64,
    pub occurred_at: String,
    pub folder_path: Option<String>,
    pub pst_item_id: Option<i64>,
    pub stage: ImportErrorStage,
    pub severity: ImportErrorSeverity,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum ImportErrorStage {
    Open,
    Scan,
    Parse,
    Html,
    Image,
    Store,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum ImportErrorSeverity {
    ItemSkipped,
    Degraded,
    JobFailed,
}
```

- [ ] **Step 13: Write `core/src/dto/settings.rs`.**
```rust
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::app::{AppPhase, Language, LibraryCounts};
use super::error::AppError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub language: Language,
    pub webp_quality: u8,
    pub data_dir: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    pub language: Option<Language>,
    pub webp_quality: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum MaintenanceTask {
    Analyze,
    Vacuum,
    Reindex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum MaintenanceRunState {
    Running,
    Done,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceState {
    pub task: MaintenanceTask,
    pub state: MaintenanceRunState,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub error: Option<AppError>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DbStatus {
    pub phase: AppPhase,
    pub postgres_version: String,
    pub data_dir: String,
    pub database_size_bytes: i64,
    pub counts: LibraryCounts,
    pub image_count: i64,
    pub attachment_count: i64,
    pub indexes: Vec<IndexStatus>,
    pub last_vacuum_at: Option<String>,
    pub last_analyze_at: Option<String>,
    pub maintenance: Option<MaintenanceState>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct IndexStatus {
    pub name: String,
    pub table: String,
    pub size_bytes: i64,
    pub valid: bool,
}
```

- [ ] **Step 14: Write `core/src/dto/events.rs` and `core/src/dto/commands.rs`.**
```rust
// core/src/dto/events.rs
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::app::LibraryCounts;
use super::error::AppError;
use super::import::{ImportStatus, PstSource};
pub use super::settings::{MaintenanceRunState, MaintenanceState};

pub mod names {
    pub const APP_STATUS_CHANGED: &str = "app:status-changed";
    pub const IMPORT_PROGRESS: &str = "import:progress";
    pub const IMPORT_STATUS_CHANGED: &str = "import:status-changed";
    pub const LIBRARY_CHANGED: &str = "library:changed";
    pub const DATA_DIR_MOVE_PROGRESS: &str = "data-dir:move-progress";
    pub const MAINTENANCE_STATUS_CHANGED: &str = "maintenance:status-changed";
    pub const ALL: [&str; 6] = [
        APP_STATUS_CHANGED, IMPORT_PROGRESS, IMPORT_STATUS_CHANGED,
        LIBRARY_CHANGED, DATA_DIR_MOVE_PROGRESS, MAINTENANCE_STATUS_CHANGED,
    ];
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ImportStatusChanged {
    pub pst_id: i64,
    pub status: ImportStatus,
    pub removed: bool,
    pub source: Option<PstSource>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct LibraryChanged {
    pub counts: LibraryCounts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum DataDirMovePhase {
    StoppingDb,
    Copying,
    Verifying,
    StartingDb,
    Done,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DataDirMoveProgress {
    pub phase: DataDirMovePhase,
    pub bytes_copied: i64,
    pub bytes_total: i64,
    pub error: Option<AppError>,
}

/// Writes `<TS_RS_EXPORT_DIR>/<file>` as a pretty JSON array with a trailing newline.
#[cfg(test)]
pub(crate) fn write_name_list(file: &str, names: &[&str]) {
    let dir = std::env::var("TS_RS_EXPORT_DIR")
        .expect("TS_RS_EXPORT_DIR is set by src-tauri/.cargo/config.toml");
    std::fs::create_dir_all(&dir).unwrap();
    let body = serde_json::to_string_pretty(names).unwrap() + "\n";
    std::fs::write(std::path::Path::new(&dir).join(file), body).unwrap();
}

#[cfg(test)]
mod tests {
    #[test]
    fn export_event_names() {
        super::write_name_list("events.json", &super::names::ALL);
    }
}
```
```rust
// core/src/dto/commands.rs
/// Every Tauri command (spec Part 03 §3 plus §4 `app_retry`). R2-13 registers exactly this list.
pub const COMMANDS: [&str; 24] = [
    "app_status", "app_initialize", "app_retry",
    "search_mail", "search_count", "filter_options", "mail_get",
    "saved_query_list", "saved_query_create", "saved_query_update", "saved_query_delete",
    "sql_execute", "sql_cancel",
    "pst_list", "import_enqueue", "import_cancel", "import_resume", "pst_remove", "import_errors",
    "settings_get", "settings_update",
    "data_dir_move", "maintenance_run", "db_status",
];

#[cfg(test)]
mod tests {
    #[test]
    fn export_command_names() {
        let mut sorted = super::COMMANDS.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 24, "command names are unique");
        crate::dto::events::write_name_list("commands.json", &super::COMMANDS);
    }
}
```

- [ ] **Step 15: Run the DTO tests and export the bindings.**
Run: `cargo test -p outlooker-core --lib dto::`. Expected: `app_error_wire_shape`, `language_and_phase_wire_values`, `worked_example_matches_spec_json`, `worked_example_root_validates_against_schema`, `export_event_names`, `export_command_names` and the ts-rs `export_bindings_*` tests all `ok`.
Run: `ls src/api/bindings | wc -l`. Expected: ≥ 45 `.ts` files plus `commands.json`, `events.json`.

- [ ] **Step 16: Write the failing sink and gate tests**, then the code.
```rust
// core/src/sink.rs
use std::sync::Mutex;

use crate::dto::app::AppStatus;
use crate::dto::events::{names, DataDirMoveProgress, ImportStatusChanged, LibraryChanged};
use crate::dto::import::ImportProgress;
use crate::dto::settings::MaintenanceState;

pub trait EventSink: Send + Sync {
    fn emit(&self, event: CoreEvent);
}

#[derive(Debug, Clone, PartialEq)]
pub enum CoreEvent {
    AppStatusChanged(AppStatus),
    ImportProgress(ImportProgress),
    ImportStatusChanged(ImportStatusChanged),
    LibraryChanged(LibraryChanged),
    DataDirMoveProgress(DataDirMoveProgress),
    MaintenanceStatusChanged(MaintenanceState),
}

impl CoreEvent {
    pub fn name(&self) -> &'static str {
        match self {
            CoreEvent::AppStatusChanged(_) => names::APP_STATUS_CHANGED,
            CoreEvent::ImportProgress(_) => names::IMPORT_PROGRESS,
            CoreEvent::ImportStatusChanged(_) => names::IMPORT_STATUS_CHANGED,
            CoreEvent::LibraryChanged(_) => names::LIBRARY_CHANGED,
            CoreEvent::DataDirMoveProgress(_) => names::DATA_DIR_MOVE_PROGRESS,
            CoreEvent::MaintenanceStatusChanged(_) => names::MAINTENANCE_STATUS_CHANGED,
        }
    }

    pub fn payload(&self) -> serde_json::Value {
        let v = match self {
            CoreEvent::AppStatusChanged(p) => serde_json::to_value(p),
            CoreEvent::ImportProgress(p) => serde_json::to_value(p),
            CoreEvent::ImportStatusChanged(p) => serde_json::to_value(p),
            CoreEvent::LibraryChanged(p) => serde_json::to_value(p),
            CoreEvent::DataDirMoveProgress(p) => serde_json::to_value(p),
            CoreEvent::MaintenanceStatusChanged(p) => serde_json::to_value(p),
        };
        v.expect("event payloads always serialize")
    }
}

#[derive(Default)]
pub struct RecordingSink {
    events: Mutex<Vec<CoreEvent>>,
}

impl RecordingSink {
    pub fn events(&self) -> Vec<CoreEvent> {
        self.events.lock().unwrap().clone()
    }
    pub fn names(&self) -> Vec<&'static str> {
        self.events.lock().unwrap().iter().map(CoreEvent::name).collect()
    }
}

impl EventSink for RecordingSink {
    fn emit(&self, event: CoreEvent) {
        self.events.lock().unwrap().push(event);
    }
}

pub struct NullSink;

impl EventSink for NullSink {
    fn emit(&self, _event: CoreEvent) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::app::LibraryCounts;

    #[test]
    fn recording_sink_keeps_order_and_names() {
        let sink = RecordingSink::default();
        sink.emit(CoreEvent::LibraryChanged(LibraryChanged { counts: LibraryCounts { messages: 3, psts: 1 } }));
        assert_eq!(sink.names(), vec!["library:changed"]);
        assert_eq!(sink.events()[0].payload(), serde_json::json!({ "counts": { "messages": 3, "psts": 1 } }));
    }
}
```
```rust
// core/src/gate/mod.rs
use std::sync::{Arc, Mutex};

use tokio::sync::Notify;

use crate::dto::error::{AppError, BusyReason};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Activity {
    Import,
    Removal,
    Maintenance,
    DataDirMove,
}

pub fn busy_reason(active: Activity) -> BusyReason {
    match active {
        Activity::Import => BusyReason::ImportRunning,
        Activity::Removal => BusyReason::PstRemoving,
        Activity::Maintenance => BusyReason::MaintenanceRunning,
        Activity::DataDirMove => BusyReason::DataDirMoving,
    }
}

/// At most one exclusive activity at a time (union of the conflict rules in spec Part 03 §7.2 and §8).
#[derive(Debug, Clone, Default)]
pub struct Gate {
    slot: Arc<Mutex<Option<Activity>>>,
    freed: Arc<Notify>,
}

impl Gate {
    pub fn new() -> Gate {
        Gate::default()
    }

    pub fn try_enter(&self, activity: Activity) -> Result<GateGuard, AppError> {
        let mut slot = self.slot.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(active) = *slot {
            return Err(AppError::Busy { reason: busy_reason(active) });
        }
        *slot = Some(activity);
        Ok(GateGuard { gate: self.clone(), activity })
    }

    pub fn current(&self) -> Option<Activity> {
        *self.slot.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Resolves when no activity holds the gate.
    pub async fn released(&self) {
        loop {
            let notified = self.freed.notified();
            if self.current().is_none() {
                return;
            }
            notified.await;
        }
    }
}

#[derive(Debug)]
pub struct GateGuard {
    gate: Gate,
    activity: Activity,
}

impl GateGuard {
    pub fn activity(&self) -> Activity {
        self.activity
    }
}

impl Drop for GateGuard {
    fn drop(&mut self) {
        let mut slot = self.gate.slot.lock().unwrap_or_else(|p| p.into_inner());
        if *slot == Some(self.activity) {
            *slot = None;
        }
        drop(slot);
        self.gate.freed.notify_waiters();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Activity; 4] = [Activity::Import, Activity::Removal, Activity::Maintenance, Activity::DataDirMove];

    #[test]
    fn every_activity_blocks_every_activity() {
        for held in ALL {
            let gate = Gate::new();
            let _guard = gate.try_enter(held).unwrap();
            for wanted in ALL {
                assert_eq!(
                    gate.try_enter(wanted).unwrap_err(),
                    AppError::Busy { reason: busy_reason(held) },
                    "{held:?} must block {wanted:?}"
                );
            }
        }
    }

    #[tokio::test]
    async fn released_resolves_after_drop() {
        let gate = Gate::new();
        let guard = gate.try_enter(Activity::Removal).unwrap();
        let g2 = gate.clone();
        let waiter = tokio::spawn(async move { g2.released().await });
        drop(guard);
        tokio::time::timeout(std::time::Duration::from_secs(5), waiter).await.unwrap().unwrap();
    }

    #[test]
    fn guard_drop_releases() {
        let gate = Gate::new();
        drop(gate.try_enter(Activity::Maintenance).unwrap());
        assert_eq!(gate.current(), None);
        assert!(gate.try_enter(Activity::Import).is_ok());
    }
}
```
Run: `cargo test -p outlooker-core --lib sink:: gate::`. Expected: 4 tests `ok`. (Write the test modules first, run once to see the compile failure, then add the code above the test modules.)

- [ ] **Step 17: Write the failing TS drift tests** `src/api/api.test.ts`:
```ts
import { describe, expect, it, vi } from "vitest";
import commandNames from "./bindings/commands.json";
import eventNames from "./bindings/events.json";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { api } from "./commands";
import { EVENTS } from "./events";

const toSnake = (s: string) => s.replace(/[A-Z]/g, (c) => `_${c.toLowerCase()}`);

describe("typed API client", () => {
  it("wraps exactly the registered commands", () => {
    expect(Object.keys(api).map(toSnake).sort()).toEqual([...commandNames].sort());
  });

  it("names exactly the emitted events", () => {
    expect(Object.values(EVENTS).sort()).toEqual([...eventNames].sort());
  });

  it("passes camelCase arguments and returns the payload", async () => {
    invoke.mockResolvedValueOnce(null);
    await api.importCancel(7);
    expect(invoke).toHaveBeenCalledWith("import_cancel", { pstId: 7 });
  });

  it("re-throws AppError rejections unchanged and wraps others", async () => {
    invoke.mockRejectedValueOnce({ kind: "dbNotReady" });
    await expect(api.appStatus()).rejects.toEqual({ kind: "dbNotReady" });
    invoke.mockRejectedValueOnce("boom");
    await expect(api.appStatus()).rejects.toEqual({ kind: "internal", detail: "boom" });
  });
});
```
Run: `npx vitest run src/api`. Expected: FAIL (cannot resolve `./commands`).

- [ ] **Step 18: Write `src/api/call.ts`, `src/api/commands.ts`, `src/api/events.ts`.**
```ts
// src/api/call.ts
import { invoke } from "@tauri-apps/api/core";
import type { AppError } from "./bindings/AppError";

export function isAppError(value: unknown): value is AppError {
  return typeof value === "object" && value !== null && typeof (value as { kind?: unknown }).kind === "string";
}

export async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    if (isAppError(e)) throw e;
    const wrapped: AppError = { kind: "internal", detail: String(e) };
    throw wrapped;
  }
}
```
```ts
// src/api/commands.ts
import { call } from "./call";
import type { AppStatus } from "./bindings/AppStatus";
import type { InitializeRequest } from "./bindings/InitializeRequest";
import type { SearchRequest } from "./bindings/SearchRequest";
import type { SearchPage } from "./bindings/SearchPage";
import type { SearchQuery } from "./bindings/SearchQuery";
import type { SearchCount } from "./bindings/SearchCount";
import type { FilterOptions } from "./bindings/FilterOptions";
import type { MailDetail } from "./bindings/MailDetail";
import type { SavedQuery } from "./bindings/SavedQuery";
import type { SavedQueryInput } from "./bindings/SavedQueryInput";
import type { SqlExecuteRequest } from "./bindings/SqlExecuteRequest";
import type { SqlResult } from "./bindings/SqlResult";
import type { PstSource } from "./bindings/PstSource";
import type { EnqueueResult } from "./bindings/EnqueueResult";
import type { ImportErrorsRequest } from "./bindings/ImportErrorsRequest";
import type { ImportErrorPage } from "./bindings/ImportErrorPage";
import type { Settings } from "./bindings/Settings";
import type { SettingsPatch } from "./bindings/SettingsPatch";
import type { MaintenanceTask } from "./bindings/MaintenanceTask";
import type { DbStatus } from "./bindings/DbStatus";

/** One wrapper per Tauri command (spec Part 03 §1.1). Argument keys are the Rust parameter names in camelCase. */
export const api = {
  appStatus: () => call<AppStatus>("app_status"),
  appInitialize: (req: InitializeRequest) => call<null>("app_initialize", { req }),
  appRetry: () => call<null>("app_retry"),
  searchMail: (req: SearchRequest) => call<SearchPage>("search_mail", { req }),
  searchCount: (query: SearchQuery) => call<SearchCount>("search_count", { query }),
  filterOptions: () => call<FilterOptions>("filter_options"),
  mailGet: (id: number) => call<MailDetail>("mail_get", { id }),
  savedQueryList: () => call<SavedQuery[]>("saved_query_list"),
  savedQueryCreate: (input: SavedQueryInput) => call<SavedQuery>("saved_query_create", { input }),
  savedQueryUpdate: (id: number, input: SavedQueryInput) => call<SavedQuery>("saved_query_update", { id, input }),
  savedQueryDelete: (id: number) => call<null>("saved_query_delete", { id }),
  sqlExecute: (req: SqlExecuteRequest) => call<SqlResult>("sql_execute", { req }),
  sqlCancel: (executionId: string) => call<null>("sql_cancel", { executionId }),
  pstList: () => call<PstSource[]>("pst_list"),
  importEnqueue: (paths: string[]) => call<EnqueueResult[]>("import_enqueue", { paths }),
  importCancel: (pstId: number) => call<null>("import_cancel", { pstId }),
  importResume: (pstId: number) => call<null>("import_resume", { pstId }),
  pstRemove: (pstId: number) => call<null>("pst_remove", { pstId }),
  importErrors: (req: ImportErrorsRequest) => call<ImportErrorPage>("import_errors", { req }),
  settingsGet: () => call<Settings>("settings_get"),
  settingsUpdate: (patch: SettingsPatch) => call<Settings>("settings_update", { patch }),
  dataDirMove: (target: string) => call<null>("data_dir_move", { target }),
  maintenanceRun: (task: MaintenanceTask) => call<null>("maintenance_run", { task }),
  dbStatus: () => call<DbStatus>("db_status"),
} as const;

export type Api = typeof api;
```
```ts
// src/api/events.ts
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AppStatus } from "./bindings/AppStatus";
import type { ImportProgress } from "./bindings/ImportProgress";
import type { ImportStatusChanged } from "./bindings/ImportStatusChanged";
import type { LibraryChanged } from "./bindings/LibraryChanged";
import type { DataDirMoveProgress } from "./bindings/DataDirMoveProgress";
import type { MaintenanceState } from "./bindings/MaintenanceState";

export const EVENTS = {
  appStatusChanged: "app:status-changed",
  importProgress: "import:progress",
  importStatusChanged: "import:status-changed",
  libraryChanged: "library:changed",
  dataDirMoveProgress: "data-dir:move-progress",
  maintenanceStatusChanged: "maintenance:status-changed",
} as const;

export type EventName = (typeof EVENTS)[keyof typeof EVENTS];

export interface EventPayloads {
  "app:status-changed": AppStatus;
  "import:progress": ImportProgress;
  "import:status-changed": ImportStatusChanged;
  "library:changed": LibraryChanged;
  "data-dir:move-progress": DataDirMoveProgress;
  "maintenance:status-changed": MaintenanceState;
}

export function onEvent<K extends EventName>(name: K, handler: (payload: EventPayloads[K]) => void): Promise<UnlistenFn> {
  return listen<EventPayloads[K]>(name, (event) => handler(event.payload));
}
```
```ts
// src/api/dialog.ts
import { open } from "@tauri-apps/plugin-dialog";

declare global {
  interface Window {
    __outlookerE2ePicks?: Array<string[] | string | null>;
  }
}

const E2E = import.meta.env.VITE_E2E === "1";

function nextE2ePick(): string[] | string | null {
  return window.__outlookerE2ePicks?.shift() ?? null;
}

/** Native "Open PST" picker (spec Part 03 §2). The core re-validates every path. */
export async function pickPstFiles(): Promise<string[]> {
  if (E2E) {
    const pick = nextE2ePick();
    return Array.isArray(pick) ? pick : pick ? [pick] : [];
  }
  const chosen = await open({ multiple: true, directory: false, filters: [{ name: "Outlook PST", extensions: ["pst"] }] });
  return chosen === null ? [] : Array.isArray(chosen) ? chosen : [chosen];
}

/** Native folder picker for the data location. */
export async function pickFolder(): Promise<string | null> {
  if (E2E) {
    const pick = nextE2ePick();
    return Array.isArray(pick) ? (pick[0] ?? null) : pick;
  }
  const chosen = await open({ multiple: false, directory: true });
  return typeof chosen === "string" ? chosen : null;
}
```
`src/api/dialog.test.ts`:
```ts
import { describe, expect, it, vi } from "vitest";

const open = vi.fn();
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: (...a: unknown[]) => open(...a) }));

import { pickFolder, pickPstFiles } from "./dialog";

describe("dialog pickers", () => {
  it("normalizes the PST picker result", async () => {
    open.mockResolvedValueOnce(null);
    expect(await pickPstFiles()).toEqual([]);
    open.mockResolvedValueOnce("a");
    expect(await pickPstFiles()).toEqual(["a"]);
    open.mockResolvedValueOnce(["a", "b"]);
    expect(await pickPstFiles()).toEqual(["a", "b"]);
  });

  it("returns the chosen folder or null", async () => {
    open.mockResolvedValueOnce("/x");
    expect(await pickFolder()).toBe("/x");
    open.mockResolvedValueOnce(null);
    expect(await pickFolder()).toBeNull();
  });
});
```
Check the `open()` return type of `@tauri-apps/plugin-dialog` 2.8.3 with context7 before writing (`string | string[] | null` per option set); keep the two return types above.
`import.meta.env` is typed by `"types": ["vite/client", "vitest/globals"]` in `tsconfig.json` (Step 3).
Run: `npx vitest run src/api` and `npm run typecheck`. Expected: 6 tests pass; typecheck exit 0.

- [ ] **Step 19: Full verification and contract commit.**
Run: `cd src-tauri && cargo test --workspace --locked && cd .. && git diff --exit-code src/api/bindings; echo "rc=$?"`. Expected: tests pass; first run creates bindings (commit them), second run `rc=0`.
Run: `cd src-tauri && cargo tree -p outlooker-core --depth 1 -e normal`. Expected: the versions listed in the Stage 1 acceptance.
Run: `grep -rE "crate::(lifecycle|importer|search|sql_console|maintenance|settings|db|gate|paths|app)" src-tauri/crates/outlooker-core/src/dto; echo "rc=$?"`. Expected: `rc=1`.
Run: `grep -rE "webDarkTheme|teamsDarkTheme|theme-one-dark|prefers-color-scheme: *dark" package.json src src-tauri/tauri.conf.json; echo "rc=$?"`. Expected: `rc=1`.
Commit `core/src/dto/`, `core/src/sink.rs`, `core/src/gate/mod.rs`, `src-tauri/schemas/condition-group.json`, `src/api/` with message `feat(core): API contract DTOs, event sink, work gate, typed TS client`.

---

## R1-02 steps

- [ ] **Step 1: Write `scripts/vendor-libpff.sh`** (deadline: committed tree before 2026-11-18).
```sh
#!/bin/sh
# Vendors the prepared libpff sdist (fork jhangyu/libpff, commit bb5bcc0) into src-tauri/native/libpff.
# Usage: scripts/vendor-libpff.sh [<run-id> | <path-to-libpff_python-*.tar.gz>]
# The CI artifact of run 32393676269 expires 2026-11-18T16:44:21Z. After that, re-run the fork's
# build_wheels workflow (workflow_dispatch) at commit bb5bcc0 and pass the new run id.
set -eu
ARG="${1:-32393676269}"
REPO=jhangyu/libpff
COMMIT=bb5bcc0
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="$ROOT/src-tauri/native/libpff"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

case "$ARG" in
  *.tar.gz) TARBALL="$ARG"; RUN_ID="local-tarball" ;;
  *) gh run download "$ARG" -R "$REPO" -n cibw-sdist -D "$TMP/dl"
     TARBALL="$(ls "$TMP"/dl/libpff_python-*.tar.gz)"; RUN_ID="$ARG" ;;
esac

mkdir -p "$TMP/x"
tar -xzf "$TARBALL" -C "$TMP/x"
SRC="$(ls -d "$TMP"/x/libpff_python-*)"
test -f "$SRC/include/libpff.h"
test -f "$SRC/common/types.h"
test -f "$SRC/libpff/libpff_definitions.h"

rm -rf "$DEST"
mkdir -p "$DEST"
cp -R "$SRC"/. "$DEST"/
{
  echo "repo=$REPO"
  echo "commit=$COMMIT"
  echo "run_id=$RUN_ID"
  echo "artifact=cibw-sdist"
  echo "sdist=$(basename "$TARBALL")"
  echo "sha256=$(shasum -a 256 "$TARBALL" | cut -d' ' -f1)"
} > "$DEST/ORIGIN"
echo "vendored $(basename "$TARBALL") into $DEST"
```
The whole tree is kept (configure needs every `Makefile.in` listed in `AC_CONFIG_FILES`).
Run: `chmod +x scripts/vendor-libpff.sh && scripts/vendor-libpff.sh`. If `gh` is not logged in, use the local copy of the same artifact: `scripts/vendor-libpff.sh scripts/tmp/sdist/libpff_python-20260625.tar.gz` and then edit `run_id=32393676269` into `ORIGIN` by re-running with the run id once `gh` works (the ORIGIN must name the run).
Expected: `grep -q bb5bcc0 src-tauri/native/libpff/ORIGIN; echo $?` prints `0`.
Commit `scripts/vendor-libpff.sh` and `src-tauri/native/libpff` with message `chore(pff): vendor libpff sdist 20260625 (fork bb5bcc0)`.

- [ ] **Step 2: Write `crates/pff/build.rs`.**
```rust
use std::{env, fs, path::Path, path::PathBuf, process::Command};

/// The 17 local libyal libraries compiled with libpff (spec Part 01 §4.3).
const LOCAL_LIBS: [&str; 17] = [
    "libbfio", "libcdata", "libcerror", "libcfile", "libclocale", "libcnotify", "libcpath", "libcsplit",
    "libcthreads", "libfcache", "libfdata", "libfdatetime", "libfguid", "libfmapi", "libfvalue", "libfwnt", "libuna",
];

fn copy_dir(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let target = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
}

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let vendored = manifest.join("../../native/libpff");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", vendored.join("ORIGIN").display());
    assert!(vendored.join("ORIGIN").exists(), "src-tauri/native/libpff is missing: run scripts/vendor-libpff.sh");

    let msvc = env::var("CARGO_CFG_TARGET_ENV").unwrap() == "msvc";
    let src = PathBuf::from(env::var("OUT_DIR").unwrap()).join("libpff-src");
    if !src.join(".prepared").exists() {
        let _ = fs::remove_dir_all(&src);
        copy_dir(&vendored, &src);
        if !msvc {
            // Configure in the copy: the sdist ships a Linux common/config.h that common/common.h
            // includes from its own directory first (spec Part 01 §4.3).
            let status = Command::new("sh")
                .args(["configure", "--disable-nls", "--disable-shared-libs", "--with-zlib=no"])
                .current_dir(&src)
                .status()
                .expect("failed to run libpff configure");
            assert!(status.success(), "libpff configure failed, see {}", src.join("config.log").display());
        }
        fs::write(src.join(".prepared"), b"ok").unwrap();
    }

    let mut dirs = vec!["libpff"];
    dirs.extend(LOCAL_LIBS);
    let mut build = cc::Build::new();
    build.warnings(false).include(&src).include(src.join("common")).include(src.join("include"));
    for dir in &dirs {
        build.include(src.join(dir));
    }
    for lib in LOCAL_LIBS {
        build.define(&format!("HAVE_LOCAL_{}", lib.to_uppercase()), None);
    }
    if msvc {
        build.define("_CRT_SECURE_NO_WARNINGS", None).define("UNICODE", None).define("WINVER", "0x0501");
    } else {
        build.define("HAVE_CONFIG_H", None);
    }
    for dir in &dirs {
        let mut files: Vec<PathBuf> = fs::read_dir(src.join(dir))
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|x| x == "c"))
            .collect();
        files.sort();
        build.files(files);
    }
    build.compile("pff");
}
```
Run: `cd src-tauri && cargo build -p pff`. Expected: exit 0 (first build runs configure, ~30 s). If configure fails, read `target/debug/build/pff-*/out/libpff-src/config.log` and report it; do not patch the vendored tree.

- [ ] **Step 3: Write `crates/pff/src/ffi.rs`** (signatures copied from `include/libpff.h`; `size64_t` = `u64`, `off64_t` = `i64`).
```rust
#![allow(non_camel_case_types)]
use std::ffi::{c_char, c_int};

#[repr(C)]
pub struct libpff_error_t {
    _private: [u8; 0],
}
#[repr(C)]
pub struct libpff_file_t {
    _private: [u8; 0],
}
#[repr(C)]
pub struct libpff_item_t {
    _private: [u8; 0],
}

pub type Err = *mut *mut libpff_error_t;
pub const LIBPFF_OPEN_READ: c_int = 0x01;
pub const SEEK_SET: c_int = 0;

unsafe extern "C" {
    pub fn libpff_get_version() -> *const c_char;
    pub fn libpff_error_free(error: *mut *mut libpff_error_t);
    pub fn libpff_error_sprint(error: *mut libpff_error_t, string: *mut c_char, size: usize) -> c_int;
    pub fn libpff_check_file_signature(filename: *const c_char, error: Err) -> c_int;
    #[cfg(windows)]
    pub fn libpff_check_file_signature_wide(filename: *const u16, error: Err) -> c_int;
    pub fn libpff_file_initialize(file: *mut *mut libpff_file_t, error: Err) -> c_int;
    pub fn libpff_file_free(file: *mut *mut libpff_file_t, error: Err) -> c_int;
    pub fn libpff_file_open(file: *mut libpff_file_t, filename: *const c_char, access_flags: c_int, error: Err) -> c_int;
    #[cfg(windows)]
    pub fn libpff_file_open_wide(file: *mut libpff_file_t, filename: *const u16, access_flags: c_int, error: Err) -> c_int;
    pub fn libpff_file_close(file: *mut libpff_file_t, error: Err) -> c_int;
    pub fn libpff_file_get_root_folder(file: *mut libpff_file_t, root: *mut *mut libpff_item_t, error: Err) -> c_int;
    pub fn libpff_file_get_message_store(file: *mut libpff_file_t, store: *mut *mut libpff_item_t, error: Err) -> c_int;
    pub fn libpff_attachment_get_item(attachment: *mut libpff_item_t, item: *mut *mut libpff_item_t, error: Err) -> c_int;
    pub fn libpff_item_free(item: *mut *mut libpff_item_t, error: Err) -> c_int;
    pub fn libpff_item_get_identifier(item: *mut libpff_item_t, identifier: *mut u32, error: Err) -> c_int;
    pub fn libpff_item_get_number_of_record_sets(item: *mut libpff_item_t, count: *mut c_int, error: Err) -> c_int;
    pub fn libpff_item_get_entry_value_utf8_string_size(item: *mut libpff_item_t, set: c_int, entry_type: u32, size: *mut usize, flags: u8, error: Err) -> c_int;
    pub fn libpff_item_get_entry_value_utf8_string(item: *mut libpff_item_t, set: c_int, entry_type: u32, value: *mut u8, size: usize, flags: u8, error: Err) -> c_int;
    pub fn libpff_item_get_entry_value_32bit(item: *mut libpff_item_t, set: c_int, entry_type: u32, value: *mut u32, flags: u8, error: Err) -> c_int;
    pub fn libpff_item_get_entry_value_filetime(item: *mut libpff_item_t, set: c_int, entry_type: u32, value: *mut u64, flags: u8, error: Err) -> c_int;
    pub fn libpff_item_get_entry_value_binary_data_size(item: *mut libpff_item_t, set: c_int, entry_type: u32, size: *mut usize, flags: u8, error: Err) -> c_int;
    pub fn libpff_item_get_entry_value_binary_data(item: *mut libpff_item_t, set: c_int, entry_type: u32, value: *mut u8, size: usize, flags: u8, error: Err) -> c_int;
    pub fn libpff_folder_get_utf8_name_size(folder: *mut libpff_item_t, size: *mut usize, error: Err) -> c_int;
    pub fn libpff_folder_get_utf8_name(folder: *mut libpff_item_t, value: *mut u8, size: usize, error: Err) -> c_int;
    pub fn libpff_folder_get_number_of_sub_folders(folder: *mut libpff_item_t, count: *mut c_int, error: Err) -> c_int;
    pub fn libpff_folder_get_sub_folder(folder: *mut libpff_item_t, index: c_int, sub: *mut *mut libpff_item_t, error: Err) -> c_int;
    pub fn libpff_folder_get_number_of_sub_messages(folder: *mut libpff_item_t, count: *mut c_int, error: Err) -> c_int;
    pub fn libpff_folder_get_sub_message(folder: *mut libpff_item_t, index: c_int, sub: *mut *mut libpff_item_t, error: Err) -> c_int;
    pub fn libpff_message_get_client_submit_time(message: *mut libpff_item_t, filetime: *mut u64, error: Err) -> c_int;
    pub fn libpff_message_get_delivery_time(message: *mut libpff_item_t, filetime: *mut u64, error: Err) -> c_int;
    pub fn libpff_message_get_plain_text_body_size(message: *mut libpff_item_t, size: *mut usize, error: Err) -> c_int;
    pub fn libpff_message_get_plain_text_body(message: *mut libpff_item_t, body: *mut u8, size: usize, error: Err) -> c_int;
    pub fn libpff_message_get_rtf_body_size(message: *mut libpff_item_t, size: *mut usize, error: Err) -> c_int;
    pub fn libpff_message_get_rtf_body(message: *mut libpff_item_t, body: *mut u8, size: usize, error: Err) -> c_int;
    pub fn libpff_message_get_html_body_size(message: *mut libpff_item_t, size: *mut usize, error: Err) -> c_int;
    pub fn libpff_message_get_html_body(message: *mut libpff_item_t, body: *mut u8, size: usize, error: Err) -> c_int;
    pub fn libpff_message_get_recipients(message: *mut libpff_item_t, recipients: *mut *mut libpff_item_t, error: Err) -> c_int;
    pub fn libpff_message_get_number_of_attachments(message: *mut libpff_item_t, count: *mut c_int, error: Err) -> c_int;
    pub fn libpff_message_get_attachment(message: *mut libpff_item_t, index: c_int, attachment: *mut *mut libpff_item_t, error: Err) -> c_int;
    pub fn libpff_attachment_get_type(attachment: *mut libpff_item_t, kind: *mut c_int, error: Err) -> c_int;
    pub fn libpff_attachment_get_data_size(attachment: *mut libpff_item_t, size: *mut u64, error: Err) -> c_int;
    pub fn libpff_attachment_data_read_buffer(attachment: *mut libpff_item_t, buffer: *mut u8, size: usize, error: Err) -> isize;
    pub fn libpff_attachment_data_seek_offset(attachment: *mut libpff_item_t, offset: i64, whence: c_int, error: Err) -> i64;
}
```

- [ ] **Step 4: Write `crates/pff/src/tags.rs`.**
```rust
//! MAPI property tags used by the importer (values from include/libpff/mapi.h and [MS-OXPROPS]).
pub const MESSAGE_CLASS: u32 = 0x001a;
pub const SUBJECT: u32 = 0x0037;
pub const SENDER_NAME: u32 = 0x0c1a;
pub const SENDER_ADDRESS_TYPE: u32 = 0x0c1e;
pub const SENDER_EMAIL_ADDRESS: u32 = 0x0c1f;
pub const SENDER_SMTP_ADDRESS: u32 = 0x5d01;
pub const INTERNET_MESSAGE_ID: u32 = 0x1035;
pub const INTERNET_CODEPAGE: u32 = 0x3fde;
pub const MESSAGE_CODEPAGE: u32 = 0x3ffd;
pub const RECIPIENT_TYPE: u32 = 0x0c15;
pub const DISPLAY_NAME: u32 = 0x3001;
pub const ADDRESS_TYPE: u32 = 0x3002;
pub const EMAIL_ADDRESS: u32 = 0x3003;
pub const SMTP_ADDRESS: u32 = 0x39fe;
pub const ATTACH_SIZE: u32 = 0x0e20;
pub const ATTACH_FILENAME: u32 = 0x3704;
pub const ATTACH_METHOD: u32 = 0x3705;
pub const ATTACH_LONG_FILENAME: u32 = 0x3707;
pub const ATTACH_MIME_TAG: u32 = 0x370e;
pub const ATTACH_CONTENT_ID: u32 = 0x3712;
```

- [ ] **Step 5: Write the failing tests** `crates/pff/tests/outlook_pst.rs`:
```rust
use std::path::PathBuf;

use pff::{tags, File, Item};

fn fixture() -> PathBuf {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../tests/fixtures/external/outlook.pst");
    assert!(p.exists(), "fixture missing: {} (committed by R1-02)", p.display());
    p
}

#[derive(Debug, PartialEq)]
struct Msg {
    folder: String,
    id: u32,
    class: Option<String>,
    subject: Option<String>,
    sender: Option<String>,
    has_html: bool,
    attachments: usize,
}

fn walk(item: &Item<'_>, path: &str, folders: &mut Vec<(String, u32)>, msgs: &mut Vec<Msg>) {
    folders.push((path.to_owned(), item.identifier().unwrap()));
    for i in 0..item.sub_message_count().unwrap() {
        let m = item.sub_message(i).unwrap();
        msgs.push(Msg {
            folder: path.to_owned(),
            id: m.identifier().unwrap(),
            class: m.string(0, tags::MESSAGE_CLASS).unwrap(),
            subject: m.string(0, tags::SUBJECT).unwrap(),
            sender: m.string(0, tags::SENDER_NAME).unwrap(),
            has_html: m.html_body().unwrap().is_some(),
            attachments: m.attachment_count().unwrap(),
        });
    }
    for i in 0..item.sub_folder_count().unwrap() {
        let f = item.sub_folder(i).unwrap();
        let name = f.folder_name().unwrap().unwrap_or_default();
        walk(&f, &format!("{path}/{name}"), folders, msgs);
    }
}

fn scan() -> (Vec<(String, u32)>, Vec<Msg>) {
    let file = File::open(&fixture()).unwrap();
    let root = file.root_folder().unwrap();
    let (mut folders, mut msgs) = (Vec::new(), Vec::new());
    walk(&root, "", &mut folders, &mut msgs);
    (folders, msgs)
}

#[test]
fn version_is_20260625() {
    assert_eq!(pff::version(), "20260625");
}

#[test]
fn non_pst_is_error() {
    let cargo_toml = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    assert_eq!(pff::check_signature(&cargo_toml).unwrap(), false);
    assert!(File::open(&cargo_toml).is_err());
    assert!(pff::check_signature(&fixture()).unwrap());
}

#[test]
fn outlook_pst_folders() {
    let (folders, _) = scan();
    let names: Vec<&str> = folders.iter().map(|(p, _)| p.rsplit('/').next().unwrap()).collect();
    assert!(names.contains(&"Inbox"), "folders: {folders:?}");
    assert!(names.contains(&"Sent Items"), "folders: {folders:?}");
    let file = File::open(&fixture()).unwrap();
    let store = file.message_store().unwrap();
    assert!(store.binary(0, 0x35e0).unwrap().is_some_and(|b| b.len() >= 4), "IPM subtree entry id present");
}

#[test]
fn outlook_pst_walk_is_deterministic() {
    let (f1, m1) = scan();
    let (f2, m2) = scan();
    assert_eq!(f1, f2);
    assert_eq!(m1, m2);
}

#[test]
fn outlook_pst_welcome_message() {
    let (_, msgs) = scan();
    let notes: Vec<&Msg> = msgs.iter().filter(|m| m.class.as_deref() == Some("IPM.Note")).collect();
    // The Welcome mail is stored twice: in Inbox and in the recovered folder Recovered_Group2.
    assert_eq!(notes.len(), 2, "messages: {msgs:?}");
    assert!(notes.iter().any(|m| m.folder.ends_with("/Inbox")), "{notes:?}");
    assert!(notes.iter().any(|m| m.folder.contains("Recovered_Group2")), "{notes:?}");
    for m in notes {
        // Real string properties: guards the libpff flags argument (0, not 0x01) of Item::string.
        assert_eq!(m.subject.as_deref(), Some("Welcome to Microsoft Office Outlook 2003"));
        assert_eq!(m.sender.as_deref(), Some("Outlook 2003 Team"));
        assert!(m.has_html);
        assert!(m.attachments >= 1);
    }
}

#[test]
fn filetime_conversion() {
    // 2024-05-01T08:30:00Z = 1714552200 s since the Unix epoch.
    let filetime = (1_714_552_200u64 + 11_644_473_600) * 10_000_000;
    assert_eq!(pff::filetime_to_unix_micros(filetime), Some(1_714_552_200_000_000));
    assert_eq!(pff::filetime_to_unix_micros(0), None);
}
```
Copy the fixture: `mkdir -p tests/fixtures/external && cp scripts/tmp/outlook.pst tests/fixtures/external/outlook.pst`; check `shasum -a 256` of both are equal and the size is 271,360 bytes.
Run: `cd src-tauri && cargo test -p pff`. Expected: compile errors (`File`, `Item` not found).

- [ ] **Step 6: Write `crates/pff/src/lib.rs`.**
```rust
//! Safe wrapper over the vendored libpff C library (spec Part 01 §4).
//! A `File` and its `Item`s belong to one thread: raw pointers make both `!Send` and `!Sync`.
//!
//! ```compile_fail
//! fn assert_send<T: Send>() {}
//! assert_send::<pff::File>();
//! ```

mod ffi;
pub mod tags;

use std::ffi::{c_char, c_int, CStr};
use std::fmt;
use std::marker::PhantomData;
use std::path::Path;
use std::ptr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub message: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// Owns a `libpff_error_t` and frees it on drop.
struct ErrSlot(*mut ffi::libpff_error_t);

impl ErrSlot {
    fn new() -> ErrSlot {
        ErrSlot(ptr::null_mut())
    }
    fn ptr(&mut self) -> ffi::Err {
        &mut self.0
    }
    fn to_error(&self, context: &str) -> Error {
        if self.0.is_null() {
            return Error { message: format!("{context} failed") };
        }
        let mut buf = [0 as c_char; 2048];
        let rc = unsafe { ffi::libpff_error_sprint(self.0, buf.as_mut_ptr(), buf.len()) };
        let detail = if rc > 0 { unsafe { CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().into_owned() } else { String::new() };
        Error { message: format!("{context}: {detail}") }
    }
}

impl Drop for ErrSlot {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { ffi::libpff_error_free(&mut self.0) };
        }
    }
}

/// Runs one libpff call. 1 → Ok(true), 0 → Ok(false) (value not present), -1 → Err.
fn call(context: &str, f: impl FnOnce(ffi::Err) -> c_int) -> Result<bool> {
    let mut slot = ErrSlot::new();
    match f(slot.ptr()) {
        1 => Ok(true),
        0 => Ok(false),
        _ => Err(slot.to_error(context)),
    }
}

/// Size-then-read pattern; trailing NUL bytes are removed.
fn read_sized(
    context: &str,
    size_fn: impl FnOnce(*mut usize, ffi::Err) -> c_int,
    read_fn: impl FnOnce(*mut u8, usize, ffi::Err) -> c_int,
) -> Result<Option<Vec<u8>>> {
    let mut size = 0usize;
    if !call(context, |e| size_fn(&mut size, e))? || size == 0 {
        return Ok(None);
    }
    let mut buf = vec![0u8; size];
    if !call(context, |e| read_fn(buf.as_mut_ptr(), size, e))? {
        return Ok(None);
    }
    while buf.last() == Some(&0) {
        buf.pop();
    }
    Ok(Some(buf))
}

fn read_string(
    context: &str,
    size_fn: impl FnOnce(*mut usize, ffi::Err) -> c_int,
    read_fn: impl FnOnce(*mut u8, usize, ffi::Err) -> c_int,
) -> Result<Option<String>> {
    Ok(read_sized(context, size_fn, read_fn)?.map(|b| String::from_utf8_lossy(&b).into_owned()))
}

pub fn version() -> &'static str {
    unsafe { CStr::from_ptr(ffi::libpff_get_version()) }.to_str().unwrap_or("")
}

/// FILETIME (100 ns ticks since 1601-01-01) → Unix microseconds. 0 means "not set".
pub fn filetime_to_unix_micros(filetime: u64) -> Option<i64> {
    if filetime == 0 {
        return None;
    }
    Some((filetime / 10) as i64 - 11_644_473_600_000_000)
}

#[cfg(unix)]
fn narrow_path(path: &Path) -> Result<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::CString::new(path.as_os_str().as_bytes()).map_err(|_| Error { message: "path contains NUL".into() })
}

#[cfg(windows)]
fn wide_path(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

pub fn check_signature(path: &Path) -> Result<bool> {
    #[cfg(unix)]
    {
        let c = narrow_path(path)?;
        call("libpff_check_file_signature", |e| unsafe { ffi::libpff_check_file_signature(c.as_ptr(), e) })
    }
    #[cfg(windows)]
    {
        let w = wide_path(path);
        call("libpff_check_file_signature_wide", |e| unsafe { ffi::libpff_check_file_signature_wide(w.as_ptr(), e) })
    }
}

pub struct File {
    raw: *mut ffi::libpff_file_t,
    opened: bool,
}

impl File {
    pub fn open(path: &Path) -> Result<File> {
        let mut raw = ptr::null_mut();
        call("libpff_file_initialize", |e| unsafe { ffi::libpff_file_initialize(&mut raw, e) })?;
        let mut file = File { raw, opened: false };
        #[cfg(unix)]
        {
            let c = narrow_path(path)?;
            call("libpff_file_open", |e| unsafe { ffi::libpff_file_open(file.raw, c.as_ptr(), ffi::LIBPFF_OPEN_READ, e) })?;
        }
        #[cfg(windows)]
        {
            let w = wide_path(path);
            call("libpff_file_open_wide", |e| unsafe { ffi::libpff_file_open_wide(file.raw, w.as_ptr(), ffi::LIBPFF_OPEN_READ, e) })?;
        }
        file.opened = true;
        Ok(file)
    }

    pub fn root_folder(&self) -> Result<Item<'_>> {
        let mut raw = ptr::null_mut();
        if !call("libpff_file_get_root_folder", |e| unsafe { ffi::libpff_file_get_root_folder(self.raw, &mut raw, e) })? {
            return Err(Error { message: "PST has no root folder".into() });
        }
        Ok(Item::wrap(raw))
    }

    pub fn message_store(&self) -> Result<Item<'_>> {
        let mut raw = ptr::null_mut();
        if !call("libpff_file_get_message_store", |e| unsafe { ffi::libpff_file_get_message_store(self.raw, &mut raw, e) })? {
            return Err(Error { message: "PST has no message store".into() });
        }
        Ok(Item::wrap(raw))
    }
}

impl Drop for File {
    fn drop(&mut self) {
        if self.opened {
            let _ = call("libpff_file_close", |e| unsafe { ffi::libpff_file_close(self.raw, e) });
        }
        let _ = call("libpff_file_free", |e| unsafe { ffi::libpff_file_free(&mut self.raw, e) });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentType {
    Data,
    Item,
    Reference,
    Undefined,
}

pub struct Item<'f> {
    raw: *mut ffi::libpff_item_t,
    _file: PhantomData<&'f File>,
}

impl Drop for Item<'_> {
    fn drop(&mut self) {
        let _ = call("libpff_item_free", |e| unsafe { ffi::libpff_item_free(&mut self.raw, e) });
    }
}

impl<'f> Item<'f> {
    fn wrap(raw: *mut ffi::libpff_item_t) -> Item<'f> {
        Item { raw, _file: PhantomData }
    }

    fn child(&self, context: &str, f: impl FnOnce(*mut *mut ffi::libpff_item_t, ffi::Err) -> c_int) -> Result<Option<Item<'f>>> {
        let mut raw = ptr::null_mut();
        if call(context, |e| f(&mut raw, e))? && !raw.is_null() {
            Ok(Some(Item::wrap(raw)))
        } else {
            Ok(None)
        }
    }

    fn count(&self, context: &str, f: impl FnOnce(*mut c_int, ffi::Err) -> c_int) -> Result<usize> {
        let mut n: c_int = 0;
        call(context, |e| f(&mut n, e))?;
        Ok(n.max(0) as usize)
    }

    pub fn identifier(&self) -> Result<u32> {
        let mut id = 0u32;
        call("libpff_item_get_identifier", |e| unsafe { ffi::libpff_item_get_identifier(self.raw, &mut id, e) })?;
        Ok(id)
    }

    pub fn folder_name(&self) -> Result<Option<String>> {
        read_string(
            "libpff_folder_get_utf8_name",
            |s, e| unsafe { ffi::libpff_folder_get_utf8_name_size(self.raw, s, e) },
            |b, n, e| unsafe { ffi::libpff_folder_get_utf8_name(self.raw, b, n, e) },
        )
    }

    pub fn sub_folder_count(&self) -> Result<usize> {
        self.count("libpff_folder_get_number_of_sub_folders", |n, e| unsafe { ffi::libpff_folder_get_number_of_sub_folders(self.raw, n, e) })
    }

    pub fn sub_folder(&self, index: usize) -> Result<Item<'f>> {
        self.child("libpff_folder_get_sub_folder", |r, e| unsafe { ffi::libpff_folder_get_sub_folder(self.raw, index as c_int, r, e) })?
            .ok_or_else(|| Error { message: format!("sub folder {index} missing") })
    }

    pub fn sub_message_count(&self) -> Result<usize> {
        self.count("libpff_folder_get_number_of_sub_messages", |n, e| unsafe { ffi::libpff_folder_get_number_of_sub_messages(self.raw, n, e) })
    }

    pub fn sub_message(&self, index: usize) -> Result<Item<'f>> {
        self.child("libpff_folder_get_sub_message", |r, e| unsafe { ffi::libpff_folder_get_sub_message(self.raw, index as c_int, r, e) })?
            .ok_or_else(|| Error { message: format!("sub message {index} missing") })
    }

    pub fn record_set_count(&self) -> Result<usize> {
        self.count("libpff_item_get_number_of_record_sets", |n, e| unsafe { ffi::libpff_item_get_number_of_record_sets(self.raw, n, e) })
    }

    pub fn string(&self, set: usize, tag: u32) -> Result<Option<String>> {
        let set = set as c_int;
        read_string(
            "libpff_item_get_entry_value_utf8_string",
            |s, e| unsafe { ffi::libpff_item_get_entry_value_utf8_string_size(self.raw, set, tag, s, 0, e) },
            |b, n, e| unsafe { ffi::libpff_item_get_entry_value_utf8_string(self.raw, set, tag, b, n, 0, e) },
        )
    }

    pub fn u32_value(&self, set: usize, tag: u32) -> Result<Option<u32>> {
        let mut v = 0u32;
        let found = call("libpff_item_get_entry_value_32bit", |e| unsafe {
            ffi::libpff_item_get_entry_value_32bit(self.raw, set as c_int, tag, &mut v, 0, e)
        })?;
        Ok(found.then_some(v))
    }

    pub fn filetime(&self, set: usize, tag: u32) -> Result<Option<u64>> {
        let mut v = 0u64;
        let found = call("libpff_item_get_entry_value_filetime", |e| unsafe {
            ffi::libpff_item_get_entry_value_filetime(self.raw, set as c_int, tag, &mut v, 0, e)
        })?;
        Ok(found.then_some(v))
    }

    pub fn binary(&self, set: usize, tag: u32) -> Result<Option<Vec<u8>>> {
        let set = set as c_int;
        let mut size = 0usize;
        if !call("libpff_item_get_entry_value_binary_data_size", |e| unsafe {
            ffi::libpff_item_get_entry_value_binary_data_size(self.raw, set, tag, &mut size, 0, e)
        })? {
            return Ok(None);
        }
        let mut buf = vec![0u8; size];
        if size > 0 {
            call("libpff_item_get_entry_value_binary_data", |e| unsafe {
                ffi::libpff_item_get_entry_value_binary_data(self.raw, set, tag, buf.as_mut_ptr(), size, 0, e)
            })?;
        }
        Ok(Some(buf))
    }

    pub fn client_submit_time(&self) -> Result<Option<u64>> {
        let mut v = 0u64;
        let found = call("libpff_message_get_client_submit_time", |e| unsafe { ffi::libpff_message_get_client_submit_time(self.raw, &mut v, e) })?;
        Ok(found.then_some(v).filter(|&t| t != 0))
    }

    pub fn delivery_time(&self) -> Result<Option<u64>> {
        let mut v = 0u64;
        let found = call("libpff_message_get_delivery_time", |e| unsafe { ffi::libpff_message_get_delivery_time(self.raw, &mut v, e) })?;
        Ok(found.then_some(v).filter(|&t| t != 0))
    }

    pub fn plain_text_body(&self) -> Result<Option<Vec<u8>>> {
        read_sized(
            "libpff_message_get_plain_text_body",
            |s, e| unsafe { ffi::libpff_message_get_plain_text_body_size(self.raw, s, e) },
            |b, n, e| unsafe { ffi::libpff_message_get_plain_text_body(self.raw, b, n, e) },
        )
    }

    pub fn rtf_body(&self) -> Result<Option<Vec<u8>>> {
        read_sized(
            "libpff_message_get_rtf_body",
            |s, e| unsafe { ffi::libpff_message_get_rtf_body_size(self.raw, s, e) },
            |b, n, e| unsafe { ffi::libpff_message_get_rtf_body(self.raw, b, n, e) },
        )
    }

    pub fn html_body(&self) -> Result<Option<Vec<u8>>> {
        read_sized(
            "libpff_message_get_html_body",
            |s, e| unsafe { ffi::libpff_message_get_html_body_size(self.raw, s, e) },
            |b, n, e| unsafe { ffi::libpff_message_get_html_body(self.raw, b, n, e) },
        )
    }

    pub fn recipients(&self) -> Result<Option<Item<'f>>> {
        self.child("libpff_message_get_recipients", |r, e| unsafe { ffi::libpff_message_get_recipients(self.raw, r, e) })
    }

    pub fn attachment_count(&self) -> Result<usize> {
        self.count("libpff_message_get_number_of_attachments", |n, e| unsafe { ffi::libpff_message_get_number_of_attachments(self.raw, n, e) })
    }

    pub fn attachment(&self, index: usize) -> Result<Item<'f>> {
        self.child("libpff_message_get_attachment", |r, e| unsafe { ffi::libpff_message_get_attachment(self.raw, index as c_int, r, e) })?
            .ok_or_else(|| Error { message: format!("attachment {index} missing") })
    }

    pub fn attachment_type(&self) -> Result<AttachmentType> {
        let mut t: c_int = 0;
        call("libpff_attachment_get_type", |e| unsafe { ffi::libpff_attachment_get_type(self.raw, &mut t, e) })?;
        Ok(match t as u8 {
            b'd' => AttachmentType::Data,
            b'i' => AttachmentType::Item,
            b'r' => AttachmentType::Reference,
            _ => AttachmentType::Undefined,
        })
    }

    pub fn attachment_data_size(&self) -> Result<u64> {
        let mut size = 0u64;
        call("libpff_attachment_get_data_size", |e| unsafe { ffi::libpff_attachment_get_data_size(self.raw, &mut size, e) })?;
        Ok(size)
    }

    pub fn attachment_data(&self, max_bytes: usize) -> Result<Option<Vec<u8>>> {
        let size = self.attachment_data_size()?;
        if size > max_bytes as u64 {
            return Ok(None);
        }
        let size = size as usize;
        let mut slot = ErrSlot::new();
        if unsafe { ffi::libpff_attachment_data_seek_offset(self.raw, 0, ffi::SEEK_SET, slot.ptr()) } < 0 {
            return Err(slot.to_error("libpff_attachment_data_seek_offset"));
        }
        let mut buf = vec![0u8; size];
        let mut filled = 0usize;
        while filled < size {
            let mut slot = ErrSlot::new();
            let n = unsafe { ffi::libpff_attachment_data_read_buffer(self.raw, buf[filled..].as_mut_ptr(), size - filled, slot.ptr()) };
            if n < 0 {
                return Err(slot.to_error("libpff_attachment_data_read_buffer"));
            }
            if n == 0 {
                break;
            }
            filled += n as usize;
        }
        buf.truncate(filled);
        Ok(Some(buf))
    }

    pub fn attached_item(&self) -> Result<Option<Item<'f>>> {
        self.child("libpff_attachment_get_item", |r, e| unsafe { ffi::libpff_attachment_get_item(self.raw, r, e) })
    }
}
```
Run: `cd src-tauri && cargo test -p pff --locked`. Expected: 6 tests in `outlook_pst` pass and the doctest `compile_fail` passes. If `outlook_pst_welcome_message` fails on the sender or attachment count, print the `msgs` vector and report the observed values to the leader instead of changing the assertion (the assertion values come from the architect's inspection of the file).

- [ ] **Step 7: Check for a hidden zlib dependency and forbidden calls.**
Run: `cd src-tauri && otool -L $(ls -t target/debug/deps/outlook_pst-* | grep -v '\.d$' | head -1) | grep -c libz`. Expected: `0`.
Run: `grep -rn libpff_set_codepage src-tauri/crates; echo "rc=$?"`. Expected: `rc=1`.

- [ ] **Step 8: Write `tests/fixtures/external/NOTICE`.**
```
tests/fixtures/external/outlook.pst

Source: https://raw.githubusercontent.com/libyal/testdata/refs/heads/main/pst/outlook.pst
Repository: https://github.com/libyal/testdata (commit c3631ff)
License: CC-BY-4.0 (Creative Commons Attribution 4.0 International)
Copyright libyal contributors.
The file is used unmodified as a parser test fixture.
```

- [ ] **Step 9: Write `scripts/fixtures/make-pst.ps1`** (runs only in the Windows phase, W2; on macOS only the syntax check runs).
```powershell
<#
.SYNOPSIS
  Generates the Outlooker fixture PSTs of spec Part 04 §12.1 with Outlook desktop (COM).
.PARAMETER Out
  Output directory (created if missing). Existing fixture files there are replaced.
#>
param([Parameter(Mandatory = $true)][string]$Out)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$PR_MESSAGE_ID   = 'http://schemas.microsoft.com/mapi/proptag/0x1035001F'
$PR_SUBMIT_TIME  = 'http://schemas.microsoft.com/mapi/proptag/0x00390040'
$PR_SENDER_NAME  = 'http://schemas.microsoft.com/mapi/proptag/0x0C1A001F'
$PR_SENDER_EMAIL = 'http://schemas.microsoft.com/mapi/proptag/0x0C1F001F'
$PR_HTML         = 'http://schemas.microsoft.com/mapi/proptag/0x10130102'
$PR_INET_CPID    = 'http://schemas.microsoft.com/mapi/proptag/0x3FDE0003'
$PR_CONTENT_ID   = 'http://schemas.microsoft.com/mapi/proptag/0x3712001F'

New-Item -ItemType Directory -Force -Path $Out | Out-Null
$Out = (Resolve-Path $Out).Path
$work = Join-Path $env:TEMP ("outlooker-fixtures-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $work | Out-Null
$ol = New-Object -ComObject Outlook.Application
$ns = $ol.GetNamespace('MAPI')

function Set-Prop($item, $tag, $value) {
  $item.PropertyAccessor.SetProperty($tag, $value)
  $back = $item.PropertyAccessor.GetProperty($tag)
  if (($value -is [byte[]] -and -not [System.Linq.Enumerable]::SequenceEqual([byte[]]$back, [byte[]]$value)) -or
      ($value -isnot [byte[]] -and "$back" -ne "$value")) {
    throw "Outlook did not keep property $tag (wrote '$value', read '$back')"
  }
}

function New-Store($name) {
  $path = Join-Path $Out "$name.pst"
  if (Test-Path $path) { Remove-Item $path }
  $ns.AddStoreEx($path, 3)   # olStoreUnicode
  $store = $ns.Stores | Where-Object { $_.FilePath -eq $path }
  if (-not $store) { throw "store $path not mounted" }
  return @{ Path = $path; Store = $store; Root = $store.GetRootFolder() }
}

function Close-Store($s) { $ns.RemoveStore($s.Root) }

function Get-Folder($parent, $name) {
  foreach ($f in $parent.Folders) { if ($f.Name -eq $name) { return $f } }
  return $parent.Folders.Add($name)
}

function New-Mail($folder, [hashtable]$m) {
  $item = $ol.CreateItem(0)   # olMailItem
  $item.Subject = $m.Subject
  $item.To = $m.To
  if ($m.Cc) { $item.CC = $m.Cc }
  switch ($m.Body) {
    'html' { $item.HTMLBody = $m.Html }
    'text' { $item.BodyFormat = 1; $item.Body = $m.Text }
    'rtf'  { $item.BodyFormat = 3; $item.RTFBody = [Text.Encoding]::ASCII.GetBytes($m.Rtf) }
  }
  if ($m.Attach) { foreach ($a in $m.Attach) { $att = $item.Attachments.Add($a.Path, $a.Type, 1, $a.Name); if ($a.Cid) { Set-Prop $att $PR_CONTENT_ID $a.Cid } } }
  if ($m.Embed) { $item.Attachments.Add($m.Embed, 5) | Out-Null }   # olEmbeddeditem
  $item.Save()
  if ($m.MessageId) { Set-Prop $item $PR_MESSAGE_ID $m.MessageId }
  Set-Prop $item $PR_SUBMIT_TIME $m.Sent
  Set-Prop $item $PR_SENDER_NAME $m.FromName
  Set-Prop $item $PR_SENDER_EMAIL $m.FromEmail
  if ($m.RawHtml) { Set-Prop $item $PR_HTML $m.RawHtml; Set-Prop $item $PR_INET_CPID $m.Codepage }
  $item.Save()
  $moved = $item.Move($folder)
  return $moved
}

function New-Png($path, $w, $h, $color) {
  $bmp = New-Object System.Drawing.Bitmap $w, $h
  $g = [System.Drawing.Graphics]::FromImage($bmp); $g.Clear($color); $g.Dispose()
  $fmt = switch ([IO.Path]::GetExtension($path)) { '.jpg' { [System.Drawing.Imaging.ImageFormat]::Jpeg } '.gif' { [System.Drawing.Imaging.ImageFormat]::Gif } default { [System.Drawing.Imaging.ImageFormat]::Png } }
  $bmp.Save($path, $fmt); $bmp.Dispose()
}

$base = [datetime]::new(2023, 3, 1, 9, 0, 0, [DateTimeKind]::Utc)
function Sent($i) { return $base.AddHours($i) }
$rtfOnly = '{\rtf1\ansi\ansicpg1252\deff0{\fonttbl{\f0 Arial;}}\f0\fs20 RTF only body number NUM.\par}'

# ---- basic.pst: 50 mails, Inbox / Sent Items / Inbox/Projects, HTML + plain + RTF-only bodies
$s = New-Store 'basic'
$inbox = Get-Folder $s.Root 'Inbox'; $sent = Get-Folder $s.Root 'Sent Items'; $proj = Get-Folder $inbox 'Projects'
for ($i = 0; $i -lt 50; $i++) {
  $kind = @('html', 'text', 'rtf')[$i % 3]
  $folder = @($inbox, $sent, $proj)[$i % 3]
  New-Mail $folder @{ Subject = "Basic mail $i"; To = "alice$i@example.com"; Body = $kind
    Html = "<html><body><p>Basic HTML body $i invoice</p></body></html>"; Text = "Basic plain body $i invoice"
    Rtf = $rtfOnly.Replace('NUM', "$i"); MessageId = "<basic-$i@fixtures.outlooker>"; Sent = (Sent $i)
    FromName = 'Basic Sender'; FromEmail = 'sender@example.com' } | Out-Null
}
Close-Store $s

# ---- cjk.pst: 30 mails in Traditional Chinese, Big5 and UTF-8 HTML, mixed EN/ZH subjects
$s = New-Store 'cjk'
$inbox = Get-Folder $s.Root 'Inbox'
$big5 = [Text.Encoding]::GetEncoding(950)
for ($i = 0; $i -lt 30; $i++) {
  $html = "<html><head><meta charset=""" + $(if ($i % 2) { 'big5' } else { 'utf-8' }) + """></head><body><p>報價單 第 $i 號，請查收郵件系統通知。</p></body></html>"
  $bytes = if ($i % 2) { $big5.GetBytes($html) } else { [Text.Encoding]::UTF8.GetBytes($html) }
  New-Mail $inbox @{ Subject = $(if ($i % 3) { "報價單 $i" } else { "Quotation 報價 $i" }); To = "客戶$i <client$i@example.com.tw>"
    Body = 'html'; Html = $html; RawHtml = $bytes; Codepage = $(if ($i % 2) { 950 } else { 65001 })
    MessageId = "<cjk-$i@fixtures.outlooker>"; Sent = (Sent $i); FromName = '王小明'; FromEmail = 'wang@example.com.tw' } | Out-Null
}
Close-Store $s

# ---- inline.pst: 20 mails with cid images; the same logo in 10 mails
$img = Join-Path $work 'img'; New-Item -ItemType Directory -Path $img | Out-Null
New-Png (Join-Path $img 'logo.png') 120 40 ([System.Drawing.Color]::FromArgb(15, 108, 189))
New-Png (Join-Path $img 'photo.jpg') 800 600 ([System.Drawing.Color]::Orange)
New-Png (Join-Path $img 'anim.gif') 64 64 ([System.Drawing.Color]::Green)
New-Png (Join-Path $img 'pixel.png') 1 1 ([System.Drawing.Color]::White)
New-Png (Join-Path $img 'unused.png') 32 32 ([System.Drawing.Color]::Red)
$s = New-Store 'inline'
$inbox = Get-Folder $s.Root 'Inbox'
$dataUri = 'data:image/png;base64,' + [Convert]::ToBase64String([IO.File]::ReadAllBytes((Join-Path $img 'pixel.png')))
for ($i = 0; $i -lt 20; $i++) {
  $att = @()
  $html = "<html><body><p>Inline image mail $i</p>"
  if ($i -lt 10) { $att += @{ Path = (Join-Path $img 'logo.png'); Type = 1; Name = 'logo.png'; Cid = "logo$i@x" }; $html += "<img src=""cid:logo$i@x"">" }
  switch ($i % 5) {
    0 { $att += @{ Path = (Join-Path $img 'photo.jpg'); Type = 1; Name = 'photo.jpg'; Cid = "photo$i@x" }; $html += "<img src=""cid:photo$i@x"">" }
    1 { $att += @{ Path = (Join-Path $img 'anim.gif'); Type = 1; Name = 'anim.gif'; Cid = "gif$i@x" }; $html += "<img src=""cid:gif$i@x"">" }
    2 { $att += @{ Path = (Join-Path $img 'pixel.png'); Type = 1; Name = 'pixel.png'; Cid = "px$i@x" }; $html += "<img src=""cid:px$i@x"" width=1 height=1>" }
    3 { $att += @{ Path = (Join-Path $img 'unused.png'); Type = 1; Name = 'unused.png'; Cid = "unused$i@x" }; $html += "<img src=""$dataUri"">" }
    4 { $html += "<img src=""https://tracker.example.com/p.gif"">" }
  }
  New-Mail $inbox @{ Subject = "Inline $i"; To = 'bob@example.com'; Body = 'html'; Html = ($html + '</body></html>'); Attach = $att
    MessageId = "<inline-$i@fixtures.outlooker>"; Sent = (Sent $i); FromName = 'Image Sender'; FromEmail = 'img@example.com' } | Out-Null
}
Close-Store $s

# ---- attach.pst: file, embedded .msg, missing MIME tag, long file name
$s = New-Store 'attach'
$inbox = Get-Folder $s.Root 'Inbox'
$file = Join-Path $work 'report.pdf'; [IO.File]::WriteAllBytes($file, [byte[]](1..200))
$bin = Join-Path $work 'blob.unknownext'; [IO.File]::WriteAllBytes($bin, [byte[]](1..64))
$long = Join-Path $work (('L' * 180) + '.txt'); [IO.File]::WriteAllText($long, 'long name')
$embed = $ol.CreateItem(0); $embed.Subject = 'Embedded message'; $embed.Body = 'inner'; $embed.Save()
New-Mail $inbox @{ Subject = 'Attach file'; To = 'c@example.com'; Body = 'text'; Text = 'see file'; Attach = @(@{ Path = $file; Type = 1; Name = 'report.pdf' })
  MessageId = '<attach-1@fixtures.outlooker>'; Sent = (Sent 1); FromName = 'A'; FromEmail = 'a@example.com' } | Out-Null
New-Mail $inbox @{ Subject = 'Attach embedded'; To = 'c@example.com'; Body = 'text'; Text = 'see msg'; Embed = $embed
  MessageId = '<attach-2@fixtures.outlooker>'; Sent = (Sent 2); FromName = 'A'; FromEmail = 'a@example.com' } | Out-Null
New-Mail $inbox @{ Subject = 'Attach no mime'; To = 'c@example.com'; Body = 'text'; Text = 'see blob'; Attach = @(@{ Path = $bin; Type = 1; Name = 'blob.unknownext' })
  MessageId = '<attach-3@fixtures.outlooker>'; Sent = (Sent 3); FromName = 'A'; FromEmail = 'a@example.com' } | Out-Null
New-Mail $inbox @{ Subject = 'Attach long name'; To = 'c@example.com'; Body = 'text'; Text = 'see long'; Attach = @(@{ Path = $long; Type = 1; Name = [IO.Path]::GetFileName($long) })
  MessageId = '<attach-4@fixtures.outlooker>'; Sent = (Sent 4); FromName = 'A'; FromEmail = 'a@example.com' } | Out-Null
$embed.Delete()
Close-Store $s

# ---- dup-a.pst / dup-b.pst: 40 each; 25 shared by Message-ID, 5 shared without Message-ID; one dup-a mail also in a 2nd folder
function Dup-Mail($folder, $key, [bool]$withId) {
  $m = @{ Subject = "Dup $key"; To = 'd@example.com'; Body = 'text'; Text = "Dup body $key"; Sent = (Sent ([int]($key -replace '\D', '')))
          FromName = 'Dup Sender'; FromEmail = 'dup@example.com' }
  if ($withId) { $m.MessageId = "<dup-$key@fixtures.outlooker>" }
  return New-Mail $folder $m
}
foreach ($name in 'dup-a', 'dup-b') {
  $s = New-Store $name
  $inbox = Get-Folder $s.Root 'Inbox'
  for ($i = 0; $i -lt 25; $i++) { Dup-Mail $inbox "shared$i" $true | Out-Null }
  for ($i = 0; $i -lt 5; $i++) { Dup-Mail $inbox "nomid$i" $false | Out-Null }
  for ($i = 0; $i -lt 10; $i++) { Dup-Mail $inbox "$name-own$i" $true | Out-Null }
  if ($name -eq 'dup-a') {
    $archive = Get-Folder $s.Root 'Archive'
    $first = $inbox.Items | Where-Object { $_.Subject -eq 'Dup shared0' } | Select-Object -First 1
    $first.Copy().Move($archive) | Out-Null
  }
  Close-Store $s
}

# ---- hostile.pst: sanitizer corpus
$s = New-Store 'hostile'
$inbox = Get-Folder $s.Root 'Inbox'
$hostile = @(
  '<script>alert(1)</script><p>script</p>',
  '<img src="x" onerror="alert(1)"><p>onerror</p>',
  '<a href="javascript:alert(1)">js link</a>',
  '<div style="background:url(https://evil.example/x.png)">css url</div>',
  '<div style="position:fixed;top:0;left:0;width:100%;height:100%">overlay</div>',
  '<iframe src="https://evil.example"></iframe><p>iframe</p>',
  '<form action="https://evil.example"><input name="p"></form>'
)
for ($i = 0; $i -lt $hostile.Count; $i++) {
  New-Mail $inbox @{ Subject = "Hostile $i"; To = 'e@example.com'; Body = 'html'; Html = "<html><body>$($hostile[$i])</body></html>"
    MessageId = "<hostile-$i@fixtures.outlooker>"; Sent = (Sent $i); FromName = 'H'; FromEmail = 'h@example.com' } | Out-Null
}
Close-Store $s

# ---- corrupt.pst: basic.pst with 512 bytes zeroed at 60% of the file (block-aligned)
$src = Join-Path $Out 'basic.pst'; $dst = Join-Path $Out 'corrupt.pst'
Copy-Item $src $dst -Force
$bytes = [IO.File]::ReadAllBytes($dst)
$offset = [int]([math]::Floor($bytes.Length * 0.6 / 512) * 512)
for ($k = 0; $k -lt 512; $k++) { $bytes[$offset + $k] = 0 }
[IO.File]::WriteAllBytes($dst, $bytes)

# ---- checks
$expected = 'basic', 'cjk', 'inline', 'attach', 'dup-a', 'dup-b', 'hostile', 'corrupt'
foreach ($n in $expected) {
  $f = Get-Item (Join-Path $Out "$n.pst")
  if ($f.Length -ge 5MB) { throw "$n.pst is $($f.Length) bytes (limit 5 MB)" }
}
Remove-Item -Recurse -Force $work
Write-Output "fixtures ok: $($expected -join ', ')"
```
Run (macOS, syntax only): `pwsh -NoProfile -Command "[ScriptBlock]::Create((Get-Content -Raw scripts/fixtures/make-pst.ps1)) | Out-Null"; echo "rc=$?"`. Expected: `rc=0`. If `pwsh` is not installed, install it with `brew install --cask powershell` (dev tool, not a project dependency).

- [ ] **Step 10: Commit.** Commit `crates/pff/`, `tests/fixtures/external/`, `scripts/fixtures/make-pst.ps1` with message `feat(pff): libpff FFI wrapper, outlook.pst fixture, Outlook fixture generator`.

---

## R1-03 steps

Before writing `sysinfo` calls, check the 0.39.6 API with context7 (`query-docs` for `sysinfo` 0.39: `System::new_with_specifics`, `RefreshKind::nothing().with_memory(..)`, `refresh_processes_specifics`, `ProcessRefreshKind::nothing().with_exe(UpdateKind::Always)`, `Process::exe`, `Process::name`). If a name differs, use the 0.39.6 name; behaviour stays as below.

- [ ] **Step 1: Write `src-tauri/native/postgresql.sha256`** (values from the release's `.sha256` assets, fetched 2026-10-22):
```
89e91cc80650a43d22c018b95ca356a0c9e15838a1182fed2880bfb5d7d9c9a6  aarch64-apple-darwin
4229812543e19c91864ebdf5d8f78883c5816d51d4cc5d065298260c1f64d37e  x86_64-apple-darwin
0c0c845626ed77c605d716051c9e8df3368d07fbbd9f61ea39a4c5c0536fde7e  x86_64-pc-windows-msvc
29669a4e020f252d8ff7a59e5e5c4632901fcff0e5ee7793f8b011a0e2157f68  aarch64-pc-windows-msvc
e84b02dd8d0ce5879fb697330a789231c8b8f8d958dae3ad4d90fecc710b1315  x86_64-unknown-linux-gnu
1ab47fa2f06ff70454309ad02e5d89c46488300d43977231481fd6b160b9d556  aarch64-unknown-linux-gnu
```
Re-check one value: `curl -sL https://github.com/theseus-rs/postgresql-binaries/releases/download/18.6.1/postgresql-18.6.1-aarch64-apple-darwin.tar.gz.sha256` must print the first hash.

- [ ] **Step 2: Write `scripts/fetch-postgres.mjs`.**
```js
#!/usr/bin/env node
// Downloads, verifies and prunes the bundled PostgreSQL 18.6 (spec Part 01 §3, §6).
// Usage: node scripts/fetch-postgres.mjs [--target <triple>] [--self-test]
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, readdirSync, renameSync, rmSync, writeFileSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const VERSION = "18.6.1";
const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const SHA_FILE = join(ROOT, "src-tauri", "native", "postgresql.sha256");
const DEST = join(ROOT, "src-tauri", "resources", "postgresql");
const PRUNE_DIRS = ["include", join("lib", "bitcode"), join("lib", "pgxs"), join("lib", "Python")];
const TRIPLES = {
  "darwin-arm64": "aarch64-apple-darwin",
  "darwin-x64": "x86_64-apple-darwin",
  "win32-x64": "x86_64-pc-windows-msvc",
  "win32-arm64": "aarch64-pc-windows-msvc",
  "linux-x64": "x86_64-unknown-linux-gnu",
  "linux-arm64": "aarch64-unknown-linux-gnu",
};

export function verify(buffer, expectedHex) {
  const actual = createHash("sha256").update(buffer).digest("hex");
  if (actual !== expectedHex) throw new Error(`sha256 mismatch: expected ${expectedHex}, got ${actual}`);
}

function pinnedHash(triple) {
  for (const line of readFileSync(SHA_FILE, "utf8").split("\n")) {
    const [hash, name] = line.trim().split(/\s+/);
    if (name === triple) return hash;
  }
  throw new Error(`no pinned hash for ${triple} in ${SHA_FILE}`);
}

function prune(dir) {
  for (const sub of PRUNE_DIRS) rmSync(join(dir, sub), { recursive: true, force: true });
  const lib = join(dir, "lib");
  for (const name of readdirSync(lib)) {
    if (name.startsWith("libLLVM") || name.startsWith("llvmjit") || name.endsWith(".a")) {
      rmSync(join(lib, name), { recursive: true, force: true });
    }
  }
}

async function main() {
  const args = process.argv.slice(2);
  if (args.includes("--self-test")) {
    try {
      verify(Buffer.from("x"), "0".repeat(64));
    } catch {
      console.log("self-test ok: wrong hash rejected");
      return;
    }
    throw new Error("self-test failed: wrong hash accepted");
  }
  const ti = args.indexOf("--target");
  const triple = ti >= 0 ? args[ti + 1] : TRIPLES[`${process.platform}-${process.arch}`];
  if (!triple) throw new Error(`unsupported host ${process.platform}-${process.arch}`);
  const expected = pinnedHash(triple);
  const stamp = join(DEST, ".sha256");
  if (existsSync(stamp) && readFileSync(stamp, "utf8").trim() === expected) {
    console.log(`PostgreSQL ${VERSION} (${triple}) already present`);
    return;
  }
  const asset = `postgresql-${VERSION}-${triple}.tar.gz`;
  const url = `https://github.com/theseus-rs/postgresql-binaries/releases/download/${VERSION}/${asset}`;
  console.log(`downloading ${url}`);
  const res = await fetch(url);
  if (!res.ok) throw new Error(`download failed: HTTP ${res.status}`);
  const buffer = Buffer.from(await res.arrayBuffer());
  verify(buffer, expected);
  const tmp = mkdtempSync(join(tmpdir(), "outlooker-pg-"));
  try {
    const archive = join(tmp, asset);
    writeFileSync(archive, buffer);
    execFileSync("tar", ["-xzf", archive, "-C", tmp], { stdio: "inherit" });
    const top = join(tmp, `postgresql-${VERSION}-${triple}`);
    if (!existsSync(join(top, "bin"))) throw new Error(`unexpected archive layout in ${asset}`);
    prune(top);
    rmSync(DEST, { recursive: true, force: true });
    mkdirSync(dirname(DEST), { recursive: true });
    renameSync(top, DEST);
    writeFileSync(stamp, `${expected}\n`);
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
  console.log(`PostgreSQL ${VERSION} (${triple}) ready in ${DEST}`);
}

main().catch((e) => {
  console.error(e.message);
  process.exit(e.message.startsWith("sha256 mismatch") ? 2 : 1);
});
```
Run: `node scripts/fetch-postgres.mjs --self-test`. Expected: `self-test ok: wrong hash rejected`, exit 0.
Run: `node scripts/fetch-postgres.mjs && test -x src-tauri/resources/postgresql/bin/postgres && ls src-tauri/resources/postgresql/lib/pg_trgm.* && ls src-tauri/resources/postgresql/lib/libLLVM* 2>/dev/null | wc -l`. Expected: exit 0, one `pg_trgm.dylib` line, count `0`. A second run prints `already present`.
Commit `scripts/fetch-postgres.mjs`, `src-tauri/native/postgresql.sha256` with message `feat(pg): fetch, verify and prune bundled PostgreSQL 18.6.1`.

- [ ] **Step 3: Write the failing unit tests**, then `core/src/paths/mod.rs`.
```rust
use std::path::{Path, PathBuf};

/// Every on-disk location the core uses (spec Part 02 §8). Pure after construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    app_local_data_dir: PathBuf,
    app_log_dir: PathBuf,
    pg_dir: PathBuf,
    data_root: PathBuf,
}

impl Paths {
    pub fn new(app_local_data_dir: PathBuf, app_log_dir: PathBuf, pg_dir: PathBuf) -> Paths {
        let data_root = app_local_data_dir.join("data");
        Paths { app_local_data_dir, app_log_dir, pg_dir, data_root }
    }

    pub fn with_data_root(&self, data_root: PathBuf) -> Paths {
        Paths { data_root, ..self.clone() }
    }

    pub fn bootstrap_file(&self) -> PathBuf {
        self.app_local_data_dir.join("settings.json")
    }
    pub fn default_data_root(&self) -> PathBuf {
        self.app_local_data_dir.join("data")
    }
    pub fn log_dir(&self) -> &Path {
        &self.app_log_dir
    }
    pub fn data_root(&self) -> &Path {
        &self.data_root
    }
    pub fn marker(&self) -> PathBuf {
        self.data_root.join("outlooker-data.json")
    }
    pub fn credentials(&self) -> PathBuf {
        self.data_root.join("auth").join("credentials")
    }
    pub fn inflight_marker(&self) -> PathBuf {
        self.data_root.join("import-inflight")
    }
    pub fn pgdata(&self) -> PathBuf {
        self.data_root.join("pgdata")
    }
    pub fn pg_dir(&self) -> &Path {
        &self.pg_dir
    }
    pub fn pg_bin(&self, name: &str) -> PathBuf {
        self.pg_dir.join("bin").join(format!("{name}{}", std::env::consts::EXE_SUFFIX))
    }
}

/// The fetched PostgreSQL tree used by tests and the load generator (not by the shipped app).
pub fn dev_pg_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../resources/postgresql")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_layout() {
        let p = Paths::new("/a".into(), "/l".into(), "/pg".into());
        assert_eq!(p.bootstrap_file(), PathBuf::from("/a/settings.json"));
        assert_eq!(p.data_root(), Path::new("/a/data"));
        let q = p.with_data_root("/d".into());
        assert_eq!(q.pgdata(), PathBuf::from("/d/pgdata"));
        assert_eq!(q.credentials(), PathBuf::from("/d/auth/credentials"));
        assert_eq!(q.marker(), PathBuf::from("/d/outlooker-data.json"));
        assert_eq!(q.inflight_marker(), PathBuf::from("/d/import-inflight"));
        assert_eq!(q.pg_bin("initdb"), PathBuf::from(format!("/pg/bin/initdb{}", std::env::consts::EXE_SUFFIX)));
    }
}
```
Run: `cargo test -p outlooker-core --lib paths::`. Expected: `paths_layout ... ok`.

- [ ] **Step 4: Write `core/src/lifecycle/pg.rs`.**
```rust
//! PG Controller (spec Part 02 §2.2 steps 4–5, §2.3 steps 4–6; Part 01 §6; Part 04 §6.6).
use std::ffi::OsString;
use std::net::TcpListener;
use std::path::Path;
use std::process::Output;
use std::sync::{Arc, Mutex};

use sysinfo::{MemoryRefreshKind, Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System, UpdateKind};
use tokio::process::Command;

use crate::dto::error::AppError;
use crate::paths::Paths;

pub const HBA_CONF: &str = "host all all 127.0.0.1/32 scram-sha-256\n";
pub const CJK_SELF_CHECK: &str = "SELECT similarity('測試測試','測試') > 0";
pub const ADMIN_ROLE: &str = "outlooker_admin";
const MB: u64 = 1024 * 1024;

#[cfg(windows)]
const LOCALE_CANDIDATES: &[&str] = &["en-US", "English_United States.65001"];
#[cfg(not(windows))]
const LOCALE_CANDIDATES: &[&str] = &["en_US.UTF-8", "C.UTF-8"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopMode {
    Fast,
    Immediate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PgStatus {
    Running { pid: u32 },
    Stopped,
}

/// `postgresql.auto.conf` (spec Part 02 §2.3 step 5 + Part 04 §6.6).
pub fn auto_conf(ram_bytes: u64) -> String {
    let shared_mb = (ram_bytes / 16).clamp(256 * MB, 1024 * MB) / MB;
    let cache_mb = (ram_bytes / 4).max(256 * MB) / MB;
    format!(
        "# Written by Outlooker. Do not edit.\n\
         listen_addresses = '127.0.0.1'\n\
         unix_socket_directories = ''\n\
         jit = off\n\
         logging_collector = on\n\
         log_directory = 'log'\n\
         log_rotation_size = '10MB'\n\
         log_truncate_on_rotation = on\n\
         shared_buffers = '{shared_mb}MB'\n\
         effective_cache_size = '{cache_mb}MB'\n\
         work_mem = '32MB'\n\
         maintenance_work_mem = '256MB'\n\
         max_parallel_maintenance_workers = 2\n\
         max_parallel_workers_per_gather = 2\n\
         wal_level = minimal\n\
         max_wal_senders = 0\n\
         archive_mode = off\n\
         max_wal_size = '4GB'\n\
         min_wal_size = '256MB'\n\
         checkpoint_timeout = '15min'\n\
         synchronous_commit = on\n\
         fsync = on\n\
         full_page_writes = on\n\
         random_page_cost = 1.1\n\
         max_connections = 20\n\
         plan_cache_mode = force_custom_plan\n\
         timezone = 'UTC'\n"
    )
}

pub fn total_ram_bytes() -> u64 {
    let sys = System::new_with_specifics(RefreshKind::nothing().with_memory(MemoryRefreshKind::nothing().with_ram()));
    sys.total_memory()
}

fn io_err(context: &str, e: std::io::Error) -> AppError {
    AppError::internal(format!("{context}: {e}"))
}

fn last_line(text: &str) -> String {
    text.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_owned()
}

/// One connection for the self-check and role setup. `db::connect` re-exports this.
pub async fn connect(port: u16, user: &str, password: &str, dbname: &str) -> Result<tokio_postgres::Client, AppError> {
    let mut cfg = tokio_postgres::Config::new();
    cfg.host("127.0.0.1").port(port).user(user).password(password).dbname(dbname).application_name("outlooker");
    let (client, conn) = cfg
        .connect(tokio_postgres::NoTls)
        .await
        .map_err(|e| AppError::internal(format!("connect as {user}: {e}")))?;
    tokio::spawn(async move {
        if let Err(e) = conn.await {
            log::debug!("postgres connection closed: {e}");
        }
    });
    Ok(client)
}

/// What owns the PID in `postmaster.pid`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PidOwner {
    Dead,
    /// Our bundled postgres binary (exe path readable and equal): an orphan from an earlier run.
    OurPostgres,
    /// A postgres we cannot prove is ours (another user's: `exe()` is None for foreign processes),
    /// or a process whose name is unreadable. Never signalled; reported as fileAccess.
    Foreign,
    /// A live process that is not postgres (PID reused).
    NotPostgres,
}

fn pid_owner(pid: u32, our_postgres: &Path) -> PidOwner {
    let mut sys = System::new();
    let spid = Pid::from_u32(pid);
    sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[spid]), true, ProcessRefreshKind::nothing().with_exe(UpdateKind::Always));
    let Some(p) = sys.process(spid) else { return PidOwner::Dead };
    let name = p.name().to_string_lossy().trim_end_matches(".exe").to_owned();
    let ours = our_postgres.canonicalize().ok();
    match p.exe().and_then(|e| e.canonicalize().ok()) {
        Some(exe) if Some(&exe) == ours.as_ref() => PidOwner::OurPostgres,
        Some(exe) if exe.file_stem().is_some_and(|s| s == "postgres") => PidOwner::Foreign,
        Some(_) => PidOwner::NotPostgres,
        None if name.is_empty() || name == "postgres" => PidOwner::Foreign,
        None => PidOwner::NotPostgres,
    }
}

fn read_pid(pgdata: &Path) -> Option<u32> {
    std::fs::read_to_string(pgdata.join("postmaster.pid")).ok()?.lines().next()?.trim().parse().ok()
}

#[derive(Debug, Clone)]
pub struct PgController {
    paths: Paths,
    /// Test seam: ports tried before asking the OS for a free one.
    forced_ports: Arc<Mutex<Vec<u16>>>,
}

impl PgController {
    pub fn new(paths: Paths) -> PgController {
        PgController { paths, forced_ports: Arc::new(Mutex::new(Vec::new())) }
    }

    /// Test seam (named in the plan): the next `start` tries these ports first, in order.
    pub fn with_ports(self, ports: Vec<u16>) -> PgController {
        *self.forced_ports.lock().unwrap() = ports;
        self
    }

    pub fn locale_candidates() -> &'static [&'static str] {
        LOCALE_CANDIDATES
    }

    async fn run(&self, bin: &str, args: Vec<OsString>, lc_all: &str) -> Result<Output, AppError> {
        Command::new(self.paths.pg_bin(bin))
            .args(args)
            .env("LC_ALL", lc_all)
            .env_remove("LANG")
            .env_remove("PGDATA")
            .env_remove("PGPORT")
            .env_remove("PGTZ")
            .stdin(std::process::Stdio::null())
            .output()
            .await
            .map_err(|e| io_err(bin, e))
    }

    async fn pg_ctl(&self, args: &[&str], pgdata: &Path) -> Result<Output, AppError> {
        let mut v: Vec<OsString> = args.iter().map(OsString::from).collect();
        v.push("-D".into());
        v.push(pgdata.into());
        self.run("pg_ctl", v, "C.UTF-8").await
    }

    fn next_port(&self) -> Result<u16, AppError> {
        if let Some(p) = self.forced_ports.lock().unwrap().pop() {
            return Ok(p);
        }
        let l = TcpListener::bind("127.0.0.1:0").map_err(|e| io_err("bind 127.0.0.1:0", e))?;
        Ok(l.local_addr().map_err(|e| io_err("local_addr", e))?.port())
    }

    pub async fn init(&self, pgdata: &Path, admin_password: &str) -> Result<String, AppError> {
        self.init_with_candidates(pgdata, admin_password, Self::locale_candidates()).await
    }

    /// `init` with an explicit candidate list (also used by tests to prove that a C locale is rejected).
    pub async fn init_with_candidates(&self, pgdata: &Path, admin_password: &str, candidates: &[&str]) -> Result<String, AppError> {
        if pgdata.exists() {
            return Err(AppError::internal(format!("{} already exists; init never touches an existing cluster", pgdata.display())));
        }
        let ram = total_ram_bytes();
        let mut failures = Vec::new();
        for &locale in candidates {
            match self.try_candidate(pgdata, admin_password, locale, ram).await {
                Ok(()) => {
                    log::info!("initdb locale {locale} passed the CJK trigram self-check");
                    return Ok(locale.to_owned());
                }
                Err(e) => {
                    log::warn!("initdb locale {locale} rejected: {e}");
                    failures.push(format!("{locale}: {e}"));
                    let _ = tokio::fs::remove_dir_all(pgdata).await;
                }
            }
        }
        Err(AppError::internal(format!(
            "database misconfigured: no locale candidate passed the CJK trigram self-check ({})",
            failures.join("; ")
        )))
    }

    async fn try_candidate(&self, pgdata: &Path, pw: &str, locale: &str, ram: u64) -> Result<(), AppError> {
        let parent = pgdata.parent().ok_or_else(|| AppError::internal("pgdata has no parent"))?;
        tokio::fs::create_dir_all(parent).await.map_err(|e| io_err("create data root", e))?;
        let pwfile = parent.join(".initdb-pw");
        tokio::fs::write(&pwfile, pw).await.map_err(|e| io_err("write pwfile", e))?;
        let args: Vec<OsString> = vec![
            "-D".into(), pgdata.into(), "-U".into(), ADMIN_ROLE.into(),
            OsString::from(format!("--pwfile={}", pwfile.display())),
            "-A".into(), "scram-sha-256".into(), "-E".into(), "UTF8".into(),
            "--locale-provider=libc".into(), OsString::from(format!("--locale={locale}")),
        ];
        let out = self.run("initdb", args, locale).await;
        let _ = tokio::fs::remove_file(&pwfile).await;
        let out = out?;
        if !out.status.success() {
            return Err(AppError::internal(format!("initdb: {}", last_line(&String::from_utf8_lossy(&out.stderr)))));
        }
        tokio::fs::write(pgdata.join("postgresql.auto.conf"), auto_conf(ram)).await.map_err(|e| io_err("write postgresql.auto.conf", e))?;
        tokio::fs::write(pgdata.join("pg_hba.conf"), HBA_CONF).await.map_err(|e| io_err("write pg_hba.conf", e))?;
        let port = self.start(pgdata).await?;
        let check = async {
            let postgres = connect(port, ADMIN_ROLE, pw, "postgres").await?;
            postgres.batch_execute("CREATE DATABASE outlooker").await.map_err(|e| AppError::internal(e.to_string()))?;
            drop(postgres);
            let db = connect(port, ADMIN_ROLE, pw, "outlooker").await?;
            db.batch_execute("CREATE EXTENSION IF NOT EXISTS pg_trgm").await.map_err(|e| AppError::internal(e.to_string()))?;
            let ok: bool = db.query_one(CJK_SELF_CHECK, &[]).await.map_err(|e| AppError::internal(e.to_string()))?.get(0);
            if ok { Ok(()) } else { Err(AppError::internal(format!("CJK trigram self-check failed for locale {locale}"))) }
        }
        .await;
        self.stop(pgdata, StopMode::Fast).await?;
        check
    }

    /// Spec Part 02 §2.2 step 4 before step 5. Never signals a process that is not postgres.
    async fn recover_pid_file(&self, pgdata: &Path) -> Result<(), AppError> {
        let Some(pid) = read_pid(pgdata) else { return Ok(()) };
        match pid_owner(pid, &self.paths.pg_bin("postgres")) {
            PidOwner::Dead => Ok(()), // PostgreSQL removes the stale lock file itself
            PidOwner::NotPostgres => {
                log::warn!("postmaster.pid names live non-postgres PID {pid}; removing the stale file");
                tokio::fs::remove_file(pgdata.join("postmaster.pid")).await.map_err(|e| io_err("remove postmaster.pid", e))
            }
            PidOwner::OurPostgres => {
                log::warn!("orphan postmaster PID {pid} from an earlier run; stopping it");
                self.stop(pgdata, StopMode::Fast).await
            }
            PidOwner::Foreign => Err(AppError::file_access(pgdata, "inUseByOtherUser")),
        }
    }

    pub async fn start(&self, pgdata: &Path) -> Result<u16, AppError> {
        self.recover_pid_file(pgdata).await?;
        let log_dir = pgdata.join("log");
        tokio::fs::create_dir_all(&log_dir).await.map_err(|e| io_err("create log dir", e))?;
        let startup_log = log_dir.join("startup.log");
        let mut bind_attempts = 0;
        let mut pid_retry_done = false;
        loop {
            let port = self.next_port()?;
            let _ = tokio::fs::remove_file(&startup_log).await;
            let opts = format!("-p {port}");
            let log_s = startup_log.display().to_string();
            let out = self.pg_ctl(&["start", "-w", "-t", "60", "-l", &log_s, "-o", &opts], pgdata).await?;
            if out.status.success() {
                return Ok(port);
            }
            let text = format!(
                "{}\n{}\n{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr),
                tokio::fs::read_to_string(&startup_log).await.unwrap_or_default()
            );
            if text.contains("could not bind") || text.contains("Address already in use") {
                bind_attempts += 1;
                if bind_attempts < 3 {
                    continue;
                }
            }
            if text.contains("lock file") && text.contains("already exists") && !pid_retry_done {
                pid_retry_done = true;
                if let Some(pid) = read_pid(pgdata) {
                    if matches!(pid_owner(pid, &self.paths.pg_bin("postgres")), PidOwner::OurPostgres | PidOwner::Foreign) {
                        return Err(AppError::file_access(pgdata, "inUseByOtherUser"));
                    }
                }
                let _ = tokio::fs::remove_file(pgdata.join("postmaster.pid")).await;
                continue;
            }
            return Err(AppError::internal(format!("PostgreSQL failed to start: {}", last_line(&text))));
        }
    }

    pub async fn stop(&self, pgdata: &Path, mode: StopMode) -> Result<(), AppError> {
        if self.status(pgdata).await? == PgStatus::Stopped {
            return Ok(());
        }
        let m = if mode == StopMode::Fast { "fast" } else { "immediate" };
        let out = self.pg_ctl(&["stop", "-m", m, "-w", "-t", "30"], pgdata).await?;
        if out.status.success() {
            return Ok(());
        }
        if mode == StopMode::Fast {
            let out = self.pg_ctl(&["stop", "-m", "immediate", "-w", "-t", "30"], pgdata).await?;
            if out.status.success() {
                return Ok(());
            }
        }
        Err(AppError::internal(format!("pg_ctl stop: {}", last_line(&String::from_utf8_lossy(&out.stderr)))))
    }

    pub async fn status(&self, pgdata: &Path) -> Result<PgStatus, AppError> {
        let out = self.pg_ctl(&["status"], pgdata).await?;
        match out.status.code() {
            Some(0) => {
                let text = String::from_utf8_lossy(&out.stdout);
                let pid = text.split("PID: ").nth(1).and_then(|s| s.split(')').next()).and_then(|s| s.trim().parse().ok()).unwrap_or(0);
                Ok(PgStatus::Running { pid })
            }
            Some(3) => Ok(PgStatus::Stopped),
            _ => Err(AppError::file_access(pgdata, "noDataDirectory")),
        }
    }

    /// Last non-empty line of the newest server log in `pgdata/log` (for `AppError.detail`).
    pub fn last_log_line(&self, pgdata: &Path) -> Option<String> {
        let newest = std::fs::read_dir(pgdata.join("log"))
            .ok()?
            .filter_map(|e| e.ok())
            .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())?;
        Some(last_line(&std::fs::read_to_string(newest.path()).ok()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_conf_clamps_shared_buffers() {
        assert!(auto_conf(2 * 1024 * MB).contains("shared_buffers = '256MB'"));
        assert!(auto_conf(8 * 1024 * MB).contains("shared_buffers = '512MB'"));
        assert!(auto_conf(64 * 1024 * MB).contains("shared_buffers = '1024MB'"));
        assert!(auto_conf(8 * 1024 * MB).contains("effective_cache_size = '2048MB'"));
        assert!(auto_conf(8 * 1024 * MB).contains("wal_level = minimal\nmax_wal_senders = 0\n"));
    }
}
```
`core/src/lifecycle/mod.rs` keeps the kickoff `pub mod` lines; add nothing else in R1-03.
Run: `cargo test -p outlooker-core --lib lifecycle::pg`. Expected: `auto_conf_clamps_shared_buffers ... ok`.

- [ ] **Step 5: Write the failing integration tests** `core/tests/pg_lifecycle.rs`:
```rust
use std::net::TcpListener;

use outlooker_core::lifecycle::pg::{auto_conf, connect, total_ram_bytes, PgController, PgStatus, StopMode, ADMIN_ROLE, CJK_SELF_CHECK};
use outlooker_core::paths::{dev_pg_dir, Paths};

const PW: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn setup() -> (tempfile::TempDir, Paths, PgController) {
    assert!(dev_pg_dir().join("bin").exists(), "PostgreSQL binaries missing: run node scripts/fetch-postgres.mjs");
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(dir.path().join("app"), dir.path().join("logs"), dev_pg_dir()).with_data_root(dir.path().join("data"));
    let ctl = PgController::new(paths.clone());
    (dir, paths, ctl)
}

async fn self_check(port: u16) -> bool {
    let c = connect(port, ADMIN_ROLE, PW, "outlooker").await.unwrap();
    c.query_one(CJK_SELF_CHECK, &[]).await.unwrap().get(0)
}

#[tokio::test]
async fn init_passes_cjk_self_check() {
    let (_d, paths, ctl) = setup();
    let locale = ctl.init(&paths.pgdata(), PW).await.unwrap();
    assert!(PgController::locale_candidates().contains(&locale.as_str()));
    assert_eq!(ctl.status(&paths.pgdata()).await.unwrap(), PgStatus::Stopped, "init leaves the cluster stopped");
    let port = ctl.start(&paths.pgdata()).await.unwrap();
    assert!(self_check(port).await);
    ctl.stop(&paths.pgdata(), StopMode::Fast).await.unwrap();
}

#[tokio::test]
async fn c_locale_is_rejected() {
    let (_d, paths, ctl) = setup();
    let err = ctl.init_with_candidates(&paths.pgdata(), PW, &["C"]).await.unwrap_err();
    assert!(format!("{err:?}").contains("self-check"), "{err:?}");
    assert!(!paths.pgdata().exists(), "a rejected candidate leaves no cluster behind");
}

#[tokio::test]
async fn stop_start_round_trip() {
    let (_d, paths, ctl) = setup();
    ctl.init(&paths.pgdata(), PW).await.unwrap();
    let p1 = ctl.start(&paths.pgdata()).await.unwrap();
    assert!(matches!(ctl.status(&paths.pgdata()).await.unwrap(), PgStatus::Running { pid } if pid > 0));
    ctl.stop(&paths.pgdata(), StopMode::Fast).await.unwrap();
    assert_eq!(ctl.status(&paths.pgdata()).await.unwrap(), PgStatus::Stopped);
    let p2 = ctl.start(&paths.pgdata()).await.unwrap();
    assert!(self_check(p2).await);
    let _ = p1;
    ctl.stop(&paths.pgdata(), StopMode::Fast).await.unwrap();
}

#[tokio::test]
async fn orphan_postmaster_is_restarted() {
    let (_d, paths, ctl) = setup();
    ctl.init(&paths.pgdata(), PW).await.unwrap();
    let old_port = ctl.start(&paths.pgdata()).await.unwrap();
    // Simulate an app that was killed: a new controller finds the running postmaster.
    let ctl2 = PgController::new(paths.clone());
    let new_port = ctl2.start(&paths.pgdata()).await.unwrap();
    assert!(self_check(new_port).await);
    if new_port != old_port {
        assert!(connect(old_port, ADMIN_ROLE, PW, "outlooker").await.is_err(), "orphan must be gone");
    }
    ctl2.stop(&paths.pgdata(), StopMode::Fast).await.unwrap();
}

fn fake_pid_file(paths: &Paths, pid: u32) {
    let body = format!("{pid}\n{}\n1700000000\n5432\n\n127.0.0.1\n  5432001  0\nready   \n", paths.pgdata().display());
    std::fs::write(paths.pgdata().join("postmaster.pid"), body).unwrap();
}

#[tokio::test]
async fn stale_pid_with_dead_process() {
    let (_d, paths, ctl) = setup();
    ctl.init(&paths.pgdata(), PW).await.unwrap();
    let mut child = std::process::Command::new(if cfg!(windows) { "cmd" } else { "true" })
        .args(if cfg!(windows) { vec!["/C", "exit"] } else { vec![] })
        .spawn()
        .unwrap();
    let dead = child.id();
    child.wait().unwrap();
    fake_pid_file(&paths, dead);
    let port = ctl.start(&paths.pgdata()).await.unwrap();
    assert!(self_check(port).await);
    ctl.stop(&paths.pgdata(), StopMode::Fast).await.unwrap();
}

#[tokio::test]
async fn pid_reused_by_live_non_postgres_process() {
    let (_d, paths, ctl) = setup();
    ctl.init(&paths.pgdata(), PW).await.unwrap();
    fake_pid_file(&paths, std::process::id()); // this test process: alive, not postgres
    let port = ctl.start(&paths.pgdata()).await.unwrap();
    assert!(self_check(port).await, "the test process survived and the cluster started");
    ctl.stop(&paths.pgdata(), StopMode::Fast).await.unwrap();
}

#[tokio::test]
async fn port_conflict_retries() {
    let (_d, paths, ctl) = setup();
    ctl.init(&paths.pgdata(), PW).await.unwrap();
    let busy = TcpListener::bind("127.0.0.1:0").unwrap();
    let busy_port = busy.local_addr().unwrap().port();
    let ctl = ctl.with_ports(vec![busy_port]);
    let port = ctl.start(&paths.pgdata()).await.unwrap();
    assert_ne!(port, busy_port);
    assert!(self_check(port).await);
    ctl.stop(&paths.pgdata(), StopMode::Fast).await.unwrap();
    drop(busy);
}

#[tokio::test]
async fn auto_conf_values_are_live() {
    let (_d, paths, ctl) = setup();
    ctl.init(&paths.pgdata(), PW).await.unwrap();
    let port = ctl.start(&paths.pgdata()).await.unwrap();
    let c = connect(port, ADMIN_ROLE, PW, "outlooker").await.unwrap();
    for (name, want) in [("wal_level", "minimal"), ("max_wal_senders", "0"), ("plan_cache_mode", "force_custom_plan"), ("jit", "off"), ("TimeZone", "UTC")] {
        let got: String = c.query_one(&format!("SHOW {name}"), &[]).await.unwrap().get(0);
        assert_eq!(got, want, "{name}");
    }
    let shared: i64 = c.query_one("SELECT setting::bigint * 8192 FROM pg_settings WHERE name = 'shared_buffers'", &[]).await.unwrap().get(0);
    let expected_mb = auto_conf(total_ram_bytes()).lines().find(|l| l.starts_with("shared_buffers")).unwrap()
        .split('\'').nth(1).unwrap().trim_end_matches("MB").parse::<i64>().unwrap();
    assert_eq!(shared, expected_mb * 1024 * 1024);
    ctl.stop(&paths.pgdata(), StopMode::Fast).await.unwrap();
}
```
Run: `cargo test -p outlooker-core --locked --test pg_lifecycle`. Expected before Step 4 is complete: compile errors; after: 8 tests `ok`.

- [ ] **Step 6: Commit.** Commit `core/src/paths/mod.rs`, `core/src/lifecycle/pg.rs`, `core/tests/pg_lifecycle.rs` with message `feat(core): Paths and PG Controller (initdb locale self-check, start/stop, pid recovery)`.

---

## R1-04 steps

- [ ] **Step 1: Generate the migrations from the spec** (exact bytes; no hand-typing of DDL).
```bash
SPEC=docs/superpowers/specs/2026-10-21-outlooker-design.md
M=src-tauri/crates/outlooker-core/src/db/migrations
mkdir -p $M
block() { awk -v h="$1" 'index($0,h)==1{f=1} f&&/^```sql/{p=1;next} p&&/^```/{exit} p' $SPEC; }
GRANTS='GRANT TEMP ON DATABASE outlooker TO outlooker_app;
GRANT USAGE ON SCHEMA public TO outlooker_app, outlooker_ro;
GRANT SELECT, INSERT, UPDATE, DELETE, MAINTAIN ON ALL TABLES IN SCHEMA public TO outlooker_app;
GRANT USAGE ON ALL SEQUENCES IN SCHEMA public TO outlooker_app;'
{
  echo '-- Generated from spec Part 04 §2, §3, §8.1 by the R1-04 plan step. Do not hand-edit.'
  echo 'CREATE ROLE outlooker_app LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOREPLICATION NOBYPASSRLS;'
  block '### 8.1 Role and session limits' | grep '^CREATE ROLE outlooker_ro'
  block '## 2. Schema (DDL)'
  block '## 3. Indexes'
  echo "$GRANTS"
} > $M/V1__schema.sql
{
  echo '-- Generated from spec Part 04 §8.1, §8.2 by the R1-04 plan step. Do not hand-edit.'
  block '### 8.1 Role and session limits' | grep -E '^(ALTER ROLE|REVOKE)'
  block '### 8.2 Views'
  for v in v_mail v_recipients v_occurrences v_attachments v_folders v_pst_sources; do echo "ALTER VIEW $v OWNER TO outlooker_app;"; done
  echo 'GRANT SELECT ON v_mail, v_recipients, v_occurrences, v_attachments, v_folders, v_pst_sources TO outlooker_ro;'
  echo "$GRANTS"
} > $M/V2__roles_views.sql
grep -c 'CREATE TABLE' $M/V1__schema.sql; grep -c 'CREATE VIEW' $M/V2__roles_views.sql
```
Expected: `14` and `6`. (`index($0,h)==1` matches the heading at line start; headings are unique in the spec.)

- [ ] **Step 2: Write `core/src/db/credentials.rs`.**
```rust
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::dto::error::AppError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credentials {
    pub admin: String,
    pub app: String,
    pub ro: String,
}

fn random_hex() -> String {
    let mut buf = [0u8; 32];
    getrandom::fill(&mut buf).expect("OS random source unavailable");
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

fn is_hex64(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

impl Credentials {
    pub fn generate() -> Credentials {
        Credentials { admin: random_hex(), app: random_hex(), ro: random_hex() }
    }

    pub fn save(&self, path: &Path) -> Result<(), AppError> {
        let dir = path.parent().ok_or_else(|| AppError::internal("credentials path has no parent"))?;
        std::fs::create_dir_all(dir).map_err(|e| AppError::file_access(dir, &e.to_string()))?;
        let tmp = path.with_extension("tmp");
        let body = serde_json::to_vec(self).expect("credentials serialize");
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600); // spec Part 02 §2.3 step 3; on Windows the file inherits the profile ACL
        }
        use std::io::Write;
        let mut f = opts.open(&tmp).map_err(|e| AppError::file_access(&tmp, &e.to_string()))?;
        f.write_all(&body).and_then(|_| f.sync_all()).map_err(|e| AppError::file_access(&tmp, &e.to_string()))?;
        std::fs::rename(&tmp, path).map_err(|e| AppError::file_access(path, &e.to_string()))
    }

    pub fn load(path: &Path) -> Result<Credentials, AppError> {
        let body = std::fs::read(path).map_err(|e| AppError::file_access(path, &e.to_string()))?;
        let c: Credentials = serde_json::from_slice(&body).map_err(|_| AppError::file_access(path, "corrupt"))?;
        if ![&c.admin, &c.app, &c.ro].iter().all(|s| is_hex64(s)) {
            return Err(AppError::file_access(path, "corrupt"));
        }
        Ok(c)
    }
}
```

- [ ] **Step 3: Write `core/src/db/pools.rs` and `core/src/db/mod.rs`.**
```rust
// core/src/db/pools.rs
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod};
use tokio_postgres::NoTls;

use super::credentials::Credentials;
use super::{connect, connect_admin, DB_NAME};
use crate::dto::error::AppError;

fn pool(port: u16, user: &str, password: &str, max: usize) -> Result<Pool, AppError> {
    let mut cfg = tokio_postgres::Config::new();
    cfg.host("127.0.0.1").port(port).user(user).password(password).dbname(DB_NAME).application_name("outlooker");
    let mgr = Manager::from_config(cfg, NoTls, ManagerConfig { recycling_method: RecyclingMethod::Fast });
    Pool::builder(mgr).max_size(max).build().map_err(|e| AppError::internal(format!("pool {user}: {e}")))
}

#[derive(Clone)]
pub struct Pools {
    pub app: Pool,
    pub ro: Pool,
}

impl Pools {
    /// App pool ≤ 8, read-only pool ≤ 2 (spec Part 04 §6.6 `max_connections` budget).
    pub fn build(port: u16, creds: &Credentials) -> Result<Pools, AppError> {
        Ok(Pools { app: pool(port, "outlooker_app", &creds.app, 8)?, ro: pool(port, "outlooker_ro", &creds.ro, 2)? })
    }
    pub fn close(&self) {
        self.app.close();
        self.ro.close();
    }
}

#[derive(Clone)]
pub struct DbConn {
    pub port: u16,
    pub creds: Credentials,
    pub pools: Pools,
    pub data_root: PathBuf,
}

/// Shared handle to the running cluster. The Supervisor sets it on Ready and clears it before any stop,
/// so services never hold a stale port after a PostgreSQL restart.
#[derive(Clone, Default)]
pub struct Db {
    inner: Arc<RwLock<Option<DbConn>>>,
}

impl Db {
    pub fn new() -> Db {
        Db::default()
    }
    pub fn set(&self, conn: Option<DbConn>) {
        let old = std::mem::replace(&mut *self.inner.write().unwrap_or_else(|p| p.into_inner()), conn);
        if let Some(old) = old {
            old.pools.close();
        }
    }
    pub fn conn(&self) -> Result<DbConn, AppError> {
        self.inner.read().unwrap_or_else(|p| p.into_inner()).clone().ok_or(AppError::DbNotReady)
    }
    pub fn is_connected(&self) -> bool {
        self.conn().is_ok()
    }
    pub fn app_pool(&self) -> Result<Pool, AppError> {
        Ok(self.conn()?.pools.app)
    }
    pub fn ro_pool(&self) -> Result<Pool, AppError> {
        Ok(self.conn()?.pools.ro)
    }
    pub async fn connect_admin(&self) -> Result<tokio_postgres::Client, AppError> {
        let c = self.conn()?;
        connect_admin(c.port, &c.creds).await
    }
    pub async fn connect_app(&self) -> Result<tokio_postgres::Client, AppError> {
        let c = self.conn()?;
        connect(c.port, "outlooker_app", &c.creds.app, DB_NAME).await
    }
    pub fn data_root(&self) -> Result<PathBuf, AppError> {
        Ok(self.conn()?.data_root)
    }
}
```
```rust
// core/src/db/mod.rs
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
```

- [ ] **Step 4: Write `core/src/db/indexes.rs`** with its unit test.
```rust
use crate::dto::error::AppError;
use crate::lifecycle::pg::total_ram_bytes;

use super::pg_err;

/// Generation expression of the stored column `messages.body_tsv` (spec Part 04 §2, §3). The search compiler and
/// `v_mail` use the column `body_tsv`; nothing recomputes this expression at query time.
pub const TSV_EXPR: &str = "to_tsvector('english'::regconfig, left(subject || ' ' || body_text, 100000))";

pub struct SearchIndex {
    pub name: &'static str,
    pub ddl: &'static str,
}

/// The six search indexes dropped and rebuilt around large imports (spec Part 04 §6.4).
pub const SEARCH_INDEXES: [SearchIndex; 6] = [
    SearchIndex { name: "messages_sort", ddl: "messages_sort ON messages (sort_at, id)" },
    SearchIndex { name: "messages_body_trgm", ddl: "messages_body_trgm ON messages USING gin (body_text gin_trgm_ops) WITH (gin_pending_list_limit = 65536)" },
    SearchIndex { name: "messages_subject_trgm", ddl: "messages_subject_trgm ON messages USING gin (subject gin_trgm_ops)" },
    SearchIndex { name: "messages_sender_trgm", ddl: "messages_sender_trgm ON messages USING gin (sender_search gin_trgm_ops)" },
    SearchIndex { name: "messages_recipients_trgm", ddl: "messages_recipients_trgm ON messages USING gin (recipients_search gin_trgm_ops) WITH (gin_pending_list_limit = 16384)" },
    SearchIndex { name: "messages_body_fts", ddl: "messages_body_fts ON messages USING gin (body_tsv)" },
];

/// Names of search indexes that are missing or invalid.
pub async fn missing_or_invalid(admin: &tokio_postgres::Client) -> Result<Vec<&'static str>, AppError> {
    let names: Vec<&str> = SEARCH_INDEXES.iter().map(|i| i.name).collect();
    let rows = admin
        .query(
            "SELECT c.relname, x.indisvalid FROM pg_class c JOIN pg_index x ON x.indexrelid = c.oid WHERE c.relname = ANY($1)",
            &[&names],
        )
        .await
        .map_err(pg_err)?;
    Ok(SEARCH_INDEXES
        .iter()
        .map(|i| i.name)
        .filter(|n| !rows.iter().any(|r| r.get::<_, String>(0) == *n && r.get::<_, bool>(1)))
        .collect())
}

pub async fn drop_search_indexes(admin: &tokio_postgres::Client) -> Result<(), AppError> {
    for idx in &SEARCH_INDEXES {
        admin.batch_execute(&format!("DROP INDEX IF EXISTS {}", idx.name)).await.map_err(pg_err)?;
    }
    Ok(())
}

pub async fn ensure_search_indexes(
    admin: &tokio_postgres::Client,
    progress: &(dyn Fn(&str, usize, usize) + Send + Sync),
) -> Result<(), AppError> {
    let mwm_mb = (total_ram_bytes() / 8 / (1024 * 1024)).clamp(64, 1024);
    admin
        .batch_execute(&format!("SET maintenance_work_mem = '{mwm_mb}MB'; SET max_parallel_maintenance_workers = 2;"))
        .await
        .map_err(pg_err)?;
    let todo = missing_or_invalid(admin).await?;
    for (done, idx) in SEARCH_INDEXES.iter().enumerate() {
        if todo.contains(&idx.name) {
            admin.batch_execute(&format!("DROP INDEX IF EXISTS {}", idx.name)).await.map_err(pg_err)?;
            admin.batch_execute(&format!("CREATE INDEX IF NOT EXISTS {}", idx.ddl)).await.map_err(pg_err)?;
        }
        progress(idx.name, done + 1, SEARCH_INDEXES.len());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn squash(s: &str) -> String {
        s.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn ddl_matches_migration() {
        let v1 = squash(include_str!("migrations/V1__schema.sql"));
        for idx in &SEARCH_INDEXES {
            assert!(v1.contains(&squash(&format!("CREATE INDEX {};", idx.ddl))), "{} differs from V1__schema.sql", idx.name);
            assert!(idx.ddl.starts_with(idx.name));
        }
        // TSV_EXPR is the generation expression of the stored column body_tsv.
        assert!(v1.contains(&format!("body_tsv tsvector NOT NULL GENERATED ALWAYS AS ({TSV_EXPR}) STORED")));
        assert_eq!(SEARCH_INDEXES[5].ddl, "messages_body_fts ON messages USING gin (body_tsv)");
    }
}
```
Run: `cargo test -p outlooker-core --lib db::indexes`. Expected: `ddl_matches_migration ... ok`. If it fails, fix the constant in `indexes.rs` (the migration is generated from the spec and is the reference).

- [ ] **Step 5: Write `core/src/test_support/mod.rs`.**
```rust
//! Real-PostgreSQL test cluster (spec Part 01 §9): production initdb flags, migrated, both pools.
use tempfile::TempDir;

use crate::db::{connect, connect_admin, migrate, Credentials, Db, DbConn, Pools, DB_NAME};
use crate::lifecycle::pg::{PgController, StopMode};
use crate::paths::{dev_pg_dir, Paths};

pub struct TestCluster {
    pub dir: TempDir,
    pub paths: Paths,
    pub port: u16,
    pub creds: Credentials,
    pub db: Db,
    pub locale: String,
    ctl: PgController,
}

impl TestCluster {
    pub async fn start() -> TestCluster {
        assert!(dev_pg_dir().join("bin").exists(), "PostgreSQL binaries missing: run node scripts/fetch-postgres.mjs");
        let dir = tempfile::tempdir().expect("temp dir");
        let paths = Paths::new(dir.path().join("app"), dir.path().join("logs"), dev_pg_dir()).with_data_root(dir.path().join("data"));
        let ctl = PgController::new(paths.clone());
        let creds = Credentials::generate();
        let locale = ctl.init(&paths.pgdata(), &creds.admin).await.expect("initdb");
        creds.save(&paths.credentials()).expect("save credentials");
        let port = ctl.start(&paths.pgdata()).await.expect("start");
        let mut admin = connect_admin(port, &creds).await.expect("admin connect");
        migrate(&mut admin, &creds).await.expect("migrate");
        let db = Db::new();
        db.set(Some(DbConn { port, creds: creds.clone(), pools: Pools::build(port, &creds).expect("pools"), data_root: paths.data_root().to_path_buf() }));
        TestCluster { dir, paths, port, creds, db, locale, ctl }
    }

    pub fn controller(&self) -> &PgController {
        &self.ctl
    }
    pub async fn admin(&self) -> tokio_postgres::Client {
        connect_admin(self.port, &self.creds).await.expect("admin connect")
    }
    pub async fn app(&self) -> tokio_postgres::Client {
        connect(self.port, "outlooker_app", &self.creds.app, DB_NAME).await.expect("app connect")
    }
    pub async fn ro(&self) -> tokio_postgres::Client {
        connect(self.port, "outlooker_ro", &self.creds.ro, DB_NAME).await.expect("ro connect")
    }
    pub async fn kill(&self) {
        self.db.set(None);
        self.ctl.stop(&self.paths.pgdata(), StopMode::Immediate).await.expect("stop immediate");
    }
    pub async fn restart(&mut self) {
        self.port = self.ctl.start(&self.paths.pgdata()).await.expect("restart");
        self.db.set(Some(DbConn {
            port: self.port,
            creds: self.creds.clone(),
            pools: Pools::build(self.port, &self.creds).expect("pools"),
            data_root: self.paths.data_root().to_path_buf(),
        }));
    }
}

impl Drop for TestCluster {
    fn drop(&mut self) {
        self.db.set(None);
        let _ = std::process::Command::new(self.paths.pg_bin("pg_ctl"))
            .args(["stop", "-m", "immediate", "-w", "-D"])
            .arg(self.paths.pgdata())
            .output();
    }
}
```

- [ ] **Step 6: Write the integration tests** `core/tests/db_schema.rs` (run them before Steps 2–5 compile to see them fail, then again after):
```rust
use outlooker_core::db::{indexes, Credentials, Db};
use outlooker_core::dto::error::AppError;
use outlooker_core::test_support::TestCluster;

fn code(e: &tokio_postgres::Error) -> String {
    e.as_db_error().map(|d| d.code().code().to_owned()).unwrap_or_default()
}

#[tokio::test]
async fn migrations_apply_and_trgm_cjk() {
    let tc = TestCluster::start().await;
    let a = tc.admin().await;
    let n: i64 = a.query_one("SELECT count(*) FROM pg_tables WHERE schemaname = 'public' AND tablename <> 'refinery_schema_history'", &[]).await.unwrap().get(0);
    assert_eq!(n, 14);
    let trgm: String = a.query_one("SELECT array_to_string(show_trgm('郵件系統'), ',')", &[]).await.unwrap().get(0);
    assert!(trgm.contains("郵件系"), "{trgm}");
    outlooker_core::db::cjk_self_check(&a).await.unwrap();
}

#[tokio::test]
async fn constraints_reject_bad_rows() {
    let tc = TestCluster::start().await;
    let a = tc.admin().await;
    a.batch_execute(
        "INSERT INTO pst_sources (path, display_name, file_size_bytes, file_mtime) VALUES ('/a.pst','a',1,now()), ('/b.pst','b',1,now());
         INSERT INTO folders (pst_id, pst_folder_id, name, path) VALUES (1, 1, 'Inbox', 'Inbox'), (2, 1, 'Inbox', 'Inbox');
         INSERT INTO import_jobs (pst_id, kind, state, started_at) VALUES (1, 'import', 'running', now());
         INSERT INTO messages (dedupe_key, dedupe_kind, body_source, body_text) VALUES (decode(repeat('ab', 32), 'hex'), 'content_hash', 'text', 'x');",
    )
    .await
    .unwrap();
    let cases: [(&str, &str); 7] = [
        ("INSERT INTO import_jobs (pst_id, kind, state, started_at) VALUES (2, 'import', 'running', now())", "23505"),
        ("INSERT INTO message_occurrences (pst_id, pst_item_id, message_id, folder_id) VALUES (1, 9, 1, 2)", "23503"),
        ("INSERT INTO messages (dedupe_key, dedupe_kind, body_source, snippet) VALUES (decode(repeat('cd', 32), 'hex'), 'content_hash', 'none', repeat('x', 201))", "23514"),
        ("INSERT INTO addresses (email, address_type) VALUES ('Upper@Example.com', 'smtp')", "23514"),
        ("INSERT INTO messages (dedupe_key, dedupe_kind, body_source) VALUES (decode(repeat('ef', 32), 'hex'), 'content_hash', 'html')", "23514"),
        ("INSERT INTO messages (dedupe_key, dedupe_kind, body_source) VALUES (decode(repeat('ab', 31), 'hex'), 'content_hash', 'none')", "23514"),
        ("INSERT INTO settings DEFAULT VALUES", "23505"),
    ];
    for (sql, want) in cases {
        let err = a.batch_execute(sql).await.expect_err(sql);
        assert_eq!(code(&err), want, "{sql}");
    }
}

#[tokio::test]
async fn ro_role_sees_only_views() {
    let tc = TestCluster::start().await;
    let ro = tc.ro().await;
    for v in ["v_mail", "v_recipients", "v_occurrences", "v_attachments", "v_folders", "v_pst_sources"] {
        ro.query_one(&format!("SELECT count(*) FROM {v}"), &[]).await.unwrap_or_else(|e| panic!("{v}: {e}"));
    }
    ro.query_one("SELECT count(*) FROM v_mail WHERE body_tsv @@ websearch_to_tsquery('english', 'invoice')", &[]).await.unwrap();
    let err = ro.query_one("SELECT * FROM messages", &[]).await.unwrap_err();
    assert_eq!(code(&err), "42501");
    let err = ro.batch_execute("CREATE TEMP TABLE t (x int)").await.unwrap_err();
    assert_eq!(code(&err), "42501");
}

#[tokio::test]
async fn app_role_privileges() {
    let tc = TestCluster::start().await;
    let app = tc.app().await;
    app.batch_execute("CREATE TEMP TABLE staging (x int)").await.unwrap();
    app.batch_execute("VACUUM (ANALYZE) messages").await.unwrap();
    let err = app.batch_execute("CREATE INDEX nope ON messages (subject)").await.unwrap_err();
    assert_eq!(code(&err), "42501");
}

#[tokio::test]
async fn admin_owns_tables() {
    let tc = TestCluster::start().await;
    let a = tc.admin().await;
    let others: i64 = a
        .query_one("SELECT count(*) FROM pg_tables WHERE schemaname = 'public' AND tableowner <> 'outlooker_admin'", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(others, 0);
}

#[tokio::test]
async fn search_indexes_drop_and_ensure() {
    let tc = TestCluster::start().await;
    let a = tc.admin().await;
    assert!(indexes::missing_or_invalid(&a).await.unwrap().is_empty());
    indexes::drop_search_indexes(&a).await.unwrap();
    assert_eq!(indexes::missing_or_invalid(&a).await.unwrap().len(), 6);
    let calls = std::sync::Mutex::new(Vec::new());
    indexes::ensure_search_indexes(&a, &|name: &str, done: usize, total: usize| calls.lock().unwrap().push((name.to_owned(), done, total))).await.unwrap();
    assert!(indexes::missing_or_invalid(&a).await.unwrap().is_empty());
    assert_eq!(calls.lock().unwrap().last().unwrap().1, 6);
    indexes::ensure_search_indexes(&a, &|_: &str, _: usize, _: usize| {}).await.unwrap(); // idempotent
}

#[tokio::test]
async fn db_handle_not_ready_when_unset() {
    let db = Db::new();
    assert_eq!(db.app_pool().err(), Some(AppError::DbNotReady));
    assert!(matches!(db.connect_admin().await, Err(AppError::DbNotReady)));
    let tc = TestCluster::start().await;
    let client = tc.db.app_pool().unwrap().get().await.unwrap();
    let one: i32 = client.query_one("SELECT 1", &[]).await.unwrap().get(0);
    assert_eq!(one, 1);
}

#[test]
fn credentials_round_trip_and_mode() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("auth").join("credentials");
    let c = Credentials::generate();
    assert_ne!(c.app, c.ro);
    c.save(&path).unwrap();
    assert_eq!(Credentials::load(&path).unwrap(), c);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    }
    std::fs::write(&path, b"{\"admin\":\"x\",\"app\":\"y\",\"ro\":\"z\"}").unwrap();
    assert!(Credentials::load(&path).is_err());
}
```
`db_handle_not_ready_when_unset` compares `Option<AppError>`; `deadpool_postgres::Pool` is not `PartialEq`, hence `.err()`.
Run: `cargo test -p outlooker-core --locked --features test-support --test db_schema`. Expected: 8 tests `ok`.

- [ ] **Step 7: Commit.** Commit `core/src/db/`, `core/src/test_support/mod.rs`, `core/tests/db_schema.rs` with message `feat(core): schema migrations, roles, views, pools, Db handle, test cluster`.

---

## R2-08 steps

Check the sysinfo 0.39.6 `Disks` API with context7 (`Disks::new_with_refreshed_list`, `Disk::mount_point`, `Disk::available_space`) before Step 6.

- [ ] **Step 1: Write the failing settings tests, then `core/src/settings/mod.rs`.**
```rust
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::db::{pg_err, Db};
use crate::dto::app::Language;
use crate::dto::error::AppError;
use crate::dto::settings::{Settings, SettingsPatch};

/// `<app_local_data_dir>/settings.json` (spec Part 02 §2.2 step 2).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Bootstrap {
    pub data_dir: Option<String>,
    pub language: Language,
}

/// Write to `<path>.tmp`, fsync, rename: a reader sees the old or the new file, never a partial one.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    use std::io::Write;
    let dir = path.parent().ok_or_else(|| AppError::internal("path has no parent"))?;
    std::fs::create_dir_all(dir).map_err(|e| AppError::file_access(dir, &e.to_string()))?;
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let mut f = std::fs::File::create(&tmp).map_err(|e| AppError::file_access(&tmp, &e.to_string()))?;
    f.write_all(bytes).and_then(|_| f.sync_all()).map_err(|e| AppError::file_access(&tmp, &e.to_string()))?;
    std::fs::rename(&tmp, path).map_err(|e| AppError::file_access(path, &e.to_string()))
}

async fn pool_client(db: &Db) -> Result<deadpool_postgres::Object, AppError> {
    db.app_pool()?.get().await.map_err(|e| AppError::internal(format!("app pool: {e}")))
}

#[derive(Clone)]
pub struct SettingsStore {
    file: PathBuf,
    state: Arc<Mutex<(Bootstrap, u8)>>, // (bootstrap, last known webp_quality)
}

impl SettingsStore {
    pub fn load(bootstrap_file: PathBuf) -> Result<SettingsStore, AppError> {
        let boot = match std::fs::read(&bootstrap_file) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| AppError::file_access(&bootstrap_file, "corrupt"))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Bootstrap::default(),
            Err(e) => return Err(AppError::file_access(&bootstrap_file, &e.to_string())),
        };
        Ok(SettingsStore { file: bootstrap_file, state: Arc::new(Mutex::new((boot, 80))) })
    }

    pub fn bootstrap(&self) -> Bootstrap {
        self.state.lock().unwrap().0.clone()
    }

    fn save(&self, boot: Bootstrap) -> Result<(), AppError> {
        write_atomic(&self.file, &serde_json::to_vec_pretty(&boot).expect("bootstrap serializes"))?;
        self.state.lock().unwrap().0 = boot;
        Ok(())
    }

    pub fn set_bootstrap(&self, data_dir: Option<String>, language: Language) -> Result<(), AppError> {
        self.save(Bootstrap { data_dir, language })
    }

    pub fn set_data_dir(&self, data_dir: &str) -> Result<(), AppError> {
        let mut b = self.bootstrap();
        b.data_dir = Some(data_dir.to_owned());
        self.save(b)
    }

    pub async fn webp_quality(&self, db: &Db) -> Result<u8, AppError> {
        let c = pool_client(db).await?;
        let q: i16 = c.query_one("SELECT webp_quality FROM settings", &[]).await.map_err(pg_err)?.get(0);
        self.state.lock().unwrap().1 = q as u8;
        Ok(q as u8)
    }

    pub async fn get(&self, db: &Db) -> Result<Settings, AppError> {
        let quality = match self.webp_quality(db).await {
            Ok(q) => q,
            Err(AppError::DbNotReady) => self.state.lock().unwrap().1,
            Err(e) => return Err(e),
        };
        let b = self.bootstrap();
        Ok(Settings { language: b.language, webp_quality: quality, data_dir: b.data_dir.unwrap_or_default() })
    }

    pub async fn update(&self, db: &Db, patch: SettingsPatch) -> Result<Settings, AppError> {
        if let Some(q) = patch.webp_quality {
            if !(1..=100).contains(&q) {
                return Err(AppError::invalid("webpQuality", "outOfRange"));
            }
            let c = pool_client(db).await?;
            c.execute("UPDATE settings SET webp_quality = $1, updated_at = now()", &[&(q as i16)]).await.map_err(pg_err)?;
        }
        if let Some(language) = patch.language {
            let mut b = self.bootstrap();
            b.language = language;
            self.save(b)?;
        }
        self.get(db).await
    }
}
```

- [ ] **Step 2: Write `core/src/lifecycle/supervisor.rs`** (state, phase, status, boot, retry, shutdown).
```rust
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::watch;

use crate::db::{pg_err, Db};
use crate::dto::app::{AppPhase, AppStatus, LibraryCounts};
use crate::dto::error::AppError;
use crate::gate::Gate;
use crate::lifecycle::pg::{PgController, StopMode};
use crate::paths::Paths;
use crate::settings::SettingsStore;
use crate::sink::{CoreEvent, EventSink};

#[derive(Debug, Clone)]
pub struct SupervisorConfig {
    pub restart_budget: u32,
    pub restart_window: Duration,
    pub probe_interval: Duration,
}

impl Default for SupervisorConfig {
    fn default() -> Self {
        SupervisorConfig { restart_budget: 3, restart_window: Duration::from_secs(600), probe_interval: Duration::from_secs(5) }
    }
}

pub(crate) struct Inner {
    pub(crate) paths: Mutex<Paths>,
    pub(crate) settings: SettingsStore,
    pub(crate) gate: Gate,
    pub(crate) db: Db,
    pub(crate) sink: Arc<dyn EventSink>,
    pub(crate) config: SupervisorConfig,
    pub(crate) phase_tx: watch::Sender<AppPhase>,
    pub(crate) error: Mutex<Option<AppError>>,
    pub(crate) restarts: Mutex<Vec<Instant>>,
    pub(crate) health_started: AtomicBool,
    pub(crate) fail_next_verify: AtomicBool,
}

/// Owns `AppPhase` (spec Part 02 §2.1). Imports no importer / search / SQL console code.
#[derive(Clone)]
pub struct Supervisor {
    pub(crate) inner: Arc<Inner>,
}

impl Supervisor {
    pub fn new(paths: Paths, settings: SettingsStore, gate: Gate, db: Db, sink: Arc<dyn EventSink>, config: SupervisorConfig) -> Supervisor {
        let (phase_tx, _) = watch::channel(AppPhase::NeedsSetup);
        Supervisor {
            inner: Arc::new(Inner {
                paths: Mutex::new(paths),
                settings,
                gate,
                db,
                sink,
                config,
                phase_tx,
                error: Mutex::new(None),
                restarts: Mutex::new(Vec::new()),
                health_started: AtomicBool::new(false),
                fail_next_verify: AtomicBool::new(false),
            }),
        }
    }

    pub fn phase(&self) -> AppPhase {
        *self.inner.phase_tx.borrow()
    }

    pub fn phase_watch(&self) -> watch::Receiver<AppPhase> {
        self.inner.phase_tx.subscribe()
    }

    pub fn require_ready(&self) -> Result<(), AppError> {
        if self.phase() == AppPhase::Ready { Ok(()) } else { Err(AppError::DbNotReady) }
    }

    pub(crate) fn paths(&self) -> Paths {
        self.inner.paths.lock().unwrap().clone()
    }

    pub(crate) fn set_paths(&self, paths: Paths) {
        *self.inner.paths.lock().unwrap() = paths;
    }

    pub(crate) async fn set_phase(&self, phase: AppPhase, error: Option<AppError>) {
        *self.inner.error.lock().unwrap() = error;
        self.inner.phase_tx.send_replace(phase);
        let status = self.status().await;
        self.inner.sink.emit(CoreEvent::AppStatusChanged(status));
    }

    async fn counts(&self) -> Result<LibraryCounts, AppError> {
        let c = self.inner.db.app_pool()?.get().await.map_err(|e| AppError::internal(e.to_string()))?;
        let r = c
            .query_one("SELECT (SELECT count(*) FROM messages), (SELECT count(*) FROM pst_sources)", &[])
            .await
            .map_err(pg_err)?;
        Ok(LibraryCounts { messages: r.get(0), psts: r.get(1) })
    }

    pub async fn status(&self) -> AppStatus {
        let phase = self.phase();
        let boot = self.inner.settings.bootstrap();
        let data_dir = boot.data_dir.clone().unwrap_or_else(|| self.paths().default_data_root().display().to_string());
        let counts = if phase == AppPhase::Ready { self.counts().await.unwrap_or_default() } else { LibraryCounts::default() };
        AppStatus { phase, data_dir, language: boot.language, counts, error: self.inner.error.lock().unwrap().clone() }
    }

    /// App start (spec Part 02 §2.2 steps 2–7).
    pub async fn boot(&self) -> Result<(), AppError> {
        let Some(dir) = self.inner.settings.bootstrap().data_dir else {
            self.set_phase(AppPhase::NeedsSetup, None).await;
            return Ok(());
        };
        self.set_paths(self.paths().with_data_root(PathBuf::from(dir)));
        if !self.paths().marker().exists() {
            self.set_phase(AppPhase::NeedsSetup, None).await;
            return Ok(());
        }
        self.start_cluster().await
    }

    /// `app_retry` (spec Part 03 §4).
    pub async fn retry(&self) -> Result<(), AppError> {
        if self.phase() != AppPhase::Failed {
            return Err(AppError::invalid("phase", "notFailed"));
        }
        self.inner.restarts.lock().unwrap().clear();
        if self.paths().pgdata().exists() {
            self.set_phase(AppPhase::StartingDb, None).await;
            let me = self.clone();
            tokio::spawn(async move {
                let _ = me.start_cluster().await;
            });
        } else {
            self.set_phase(AppPhase::NeedsSetup, None).await;
        }
        Ok(())
    }

    /// Spec Part 02 §2.5 steps 3–4 (the importer and SQL console are stopped before this by `core::app`).
    pub async fn shutdown_db(&self) -> Result<(), AppError> {
        self.set_phase(AppPhase::StoppingDb, None).await;
        self.inner.db.set(None);
        let p = self.paths();
        PgController::new(p.clone()).stop(&p.pgdata(), StopMode::Fast).await
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn fail_next_move_verify(&self) {
        self.inner.fail_next_verify.store(true, Ordering::SeqCst);
    }
}
```

- [ ] **Step 3: Write `core/src/lifecycle/start.rs`** (start sequence shared by boot, init, retry, health, move).
```rust
use crate::db::{cjk_self_check, connect_admin, migrate, pg_err, Credentials, DbConn, Pools};
use crate::dto::app::AppPhase;
use crate::dto::error::AppError;
use crate::lifecycle::pg::{PgController, StopMode};
use crate::lifecycle::supervisor::Supervisor;
use crate::paths::Paths;

impl Supervisor {
    /// Starts the cluster of the current data root and ends in `Ready` or `Failed`.
    pub(crate) async fn start_cluster(&self) -> Result<(), AppError> {
        let paths = self.paths();
        match self.try_start(&paths).await {
            Ok(conn) => {
                self.inner.db.set(Some(conn));
                self.set_phase(AppPhase::Ready, None).await;
                self.spawn_health_monitor();
                Ok(())
            }
            Err(e) => {
                self.inner.db.set(None);
                self.set_phase(AppPhase::Failed, Some(e.clone())).await;
                Err(e)
            }
        }
    }

    async fn try_start(&self, paths: &Paths) -> Result<DbConn, AppError> {
        if !paths.marker().exists() {
            return Err(AppError::file_access(&paths.marker(), "notADataRoot"));
        }
        let version = std::fs::read_to_string(paths.pgdata().join("PG_VERSION"))
            .map_err(|e| AppError::file_access(&paths.pgdata(), &e.to_string()))?;
        if version.trim() != "18" {
            return Err(AppError::internal(format!("unsupported PostgreSQL data version {}", version.trim())));
        }
        self.set_phase(AppPhase::StartingDb, None).await;
        let ctl = PgController::new(paths.clone());
        let port = ctl.start(&paths.pgdata()).await?;
        let connected = async {
            let creds = Credentials::load(&paths.credentials())?;
            let mut admin = connect_admin(port, &creds).await?;
            admin.query_one("SELECT 1", &[]).await.map_err(pg_err)?;
            migrate(&mut admin, &creds).await?;
            cjk_self_check(&admin).await?;
            let pools = Pools::build(port, &creds)?;
            Ok::<DbConn, AppError>(DbConn { port, creds, pools, data_root: paths.data_root().to_path_buf() })
        }
        .await;
        if connected.is_err() {
            let _ = ctl.stop(&paths.pgdata(), StopMode::Fast).await;
        }
        connected
    }
}
```

- [ ] **Step 4: Write `core/src/lifecycle/init.rs`** (`app_initialize`, spec Part 02 §2.3).
```rust
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::db::Credentials;
use crate::dto::app::{AppPhase, InitializeRequest};
use crate::dto::error::AppError;
use crate::lifecycle::pg::{PgController, StopMode};
use crate::lifecycle::supervisor::Supervisor;
use crate::paths::Paths;
use crate::settings::write_atomic;

/// RFC 3339 UTC for a SystemTime (days-from-civil inverse, Howard Hinnant), so no date crate is needed.
pub fn rfc3339_utc(t: SystemTime) -> String {
    let secs = t.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

fn write_marker(paths: &Paths, locale: &str) -> Result<(), AppError> {
    let body = serde_json::json!({
        "format_version": 1,
        "created_at": rfc3339_utc(SystemTime::now()),
        "pg_major": 18,
        "locale": locale,
    });
    write_atomic(&paths.marker(), &serde_json::to_vec_pretty(&body).expect("marker serializes"))
}

fn is_empty_dir(dir: &Path) -> bool {
    std::fs::read_dir(dir).map(|mut d| d.next().is_none()).unwrap_or(false)
}

impl Supervisor {
    pub async fn initialize(&self, req: InitializeRequest) -> Result<(), AppError> {
        if self.phase() != AppPhase::NeedsSetup {
            return Err(AppError::invalid("phase", "alreadyInitialized"));
        }
        let target = PathBuf::from(&req.data_dir);
        if !target.is_absolute() {
            return Err(AppError::invalid("dataDir", "notAbsolute"));
        }
        let paths = self.paths().with_data_root(target.clone());
        let adopt = paths.marker().exists();
        if !adopt && target.exists() && !is_empty_dir(&target) {
            return Err(AppError::invalid("dataDir", "notEmpty"));
        }
        std::fs::create_dir_all(&target).map_err(|e| AppError::file_access(&target, &e.to_string()))?;
        let probe = target.join(".outlooker-write-test");
        std::fs::write(&probe, b"ok").map_err(|_| AppError::file_access(&target, "notWritable"))?;
        let _ = std::fs::remove_file(&probe);
        self.inner.settings.set_bootstrap(Some(req.data_dir.clone()), req.language)?;
        self.set_paths(paths.clone());
        let me = self.clone();
        if adopt {
            tokio::spawn(async move {
                let _ = me.start_cluster().await;
            });
            return Ok(());
        }
        self.set_phase(AppPhase::InitializingCluster, None).await;
        tokio::spawn(async move { me.first_run(paths).await });
        Ok(())
    }

    async fn first_run(&self, paths: Paths) {
        let pgdata = paths.pgdata();
        let created = !pgdata.exists();
        let cleanup = |paths: &Paths| {
            let pgdata = paths.pgdata();
            async move {
                if created {
                    let _ = tokio::fs::remove_dir_all(&pgdata).await;
                }
            }
        };
        let prepared = async {
            let creds = Credentials::generate();
            creds.save(&paths.credentials())?;
            let locale = PgController::new(paths.clone()).init(&pgdata, &creds.admin).await?;
            write_marker(&paths, &locale)
        }
        .await;
        if let Err(e) = prepared {
            cleanup(&paths).await;
            self.set_phase(AppPhase::Failed, Some(e)).await;
            return;
        }
        if self.start_cluster().await.is_err() {
            let _ = PgController::new(paths.clone()).stop(&pgdata, StopMode::Immediate).await;
            cleanup(&paths).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc3339_known_instants() {
        assert_eq!(rfc3339_utc(UNIX_EPOCH), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339_utc(UNIX_EPOCH + std::time::Duration::from_secs(1_714_552_200)), "2024-05-01T08:30:00Z");
        assert_eq!(rfc3339_utc(UNIX_EPOCH + std::time::Duration::from_secs(951_782_400)), "2000-02-29T00:00:00Z");
    }
}
```

- [ ] **Step 5: Write `core/src/lifecycle/health.rs`** (spec Part 02 §2.4).
```rust
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use tokio::time::MissedTickBehavior;

use crate::dto::app::AppPhase;
use crate::dto::error::AppError;
use crate::lifecycle::pg::{PgController, PgStatus};
use crate::lifecycle::supervisor::Supervisor;

impl Supervisor {
    pub(crate) fn spawn_health_monitor(&self) {
        if self.inner.health_started.swap(true, Ordering::SeqCst) {
            return;
        }
        let me = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(me.inner.config.probe_interval);
            tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
            let mut failures = 0u32;
            loop {
                tick.tick().await;
                if me.phase() != AppPhase::Ready || me.probe().await {
                    failures = 0;
                    continue;
                }
                failures += 1;
                if failures >= 2 {
                    failures = 0;
                    me.handle_db_loss().await;
                }
            }
        });
    }

    async fn probe(&self) -> bool {
        let Ok(pool) = self.inner.db.app_pool() else { return false };
        let probe = async {
            let c = pool.get().await.ok()?;
            c.query_one("SELECT 1", &[]).await.ok()
        };
        matches!(tokio::time::timeout(Duration::from_secs(2), probe).await, Ok(Some(_)))
    }

    async fn handle_db_loss(&self) {
        let paths = self.paths();
        let ctl = PgController::new(paths.clone());
        if matches!(ctl.status(&paths.pgdata()).await, Ok(PgStatus::Running { .. })) {
            return; // server is up; the probe failure was transient
        }
        self.inner.db.set(None);
        let now = Instant::now();
        let over_budget = {
            let mut r = self.inner.restarts.lock().unwrap();
            r.retain(|t| now.duration_since(*t) < self.inner.config.restart_window);
            r.push(now);
            r.len() as u32 > self.inner.config.restart_budget
        };
        if over_budget {
            let detail = ctl.last_log_line(&paths.pgdata()).unwrap_or_else(|| "PostgreSQL stopped repeatedly".into());
            self.set_phase(AppPhase::Failed, Some(AppError::internal(detail))).await;
            return;
        }
        let _ = self.start_cluster().await;
    }
}
```

- [ ] **Step 6: Write `core/src/lifecycle/move_dir.rs`** (spec Part 02 §8.3, Part 03 §8.2).
```rust
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sysinfo::Disks;

use crate::dto::error::AppError;
use crate::dto::events::{DataDirMovePhase, DataDirMoveProgress};
use crate::gate::Activity;
use crate::lifecycle::pg::{PgController, StopMode};
use crate::lifecycle::supervisor::Supervisor;
use crate::paths::Paths;
use crate::sink::{CoreEvent, EventSink};

fn tree_stats(dir: &Path) -> std::io::Result<(u64, u64)> {
    let (mut files, mut bytes) = (0, 0);
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        if e.file_type()?.is_dir() {
            let (f, b) = tree_stats(&e.path())?;
            files += f;
            bytes += b;
        } else {
            files += 1;
            bytes += e.metadata()?.len();
        }
    }
    Ok((files, bytes))
}

fn copy_tree(src: &Path, dst: &Path, copied: &mut u64, on_progress: &mut dyn FnMut(u64)) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src)? {
        let e = e?;
        let to = dst.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_tree(&e.path(), &to, copied, on_progress)?;
        } else {
            *copied += std::fs::copy(e.path(), &to)?;
            on_progress(*copied);
        }
    }
    Ok(())
}

/// Free bytes on the volume that will hold `path` (nearest existing ancestor, longest mount-point match).
pub fn free_space(path: &Path) -> Result<u64, AppError> {
    let mut probe = path.to_path_buf();
    while !probe.exists() {
        if !probe.pop() {
            return Err(AppError::file_access(path, "noExistingAncestor"));
        }
    }
    let probe = probe.canonicalize().map_err(|e| AppError::file_access(path, &e.to_string()))?;
    let disks = Disks::new_with_refreshed_list();
    disks
        .iter()
        .filter(|d| probe.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space())
        .ok_or_else(|| AppError::file_access(path, "unknownVolume"))
}

fn emit(sink: &Arc<dyn EventSink>, phase: DataDirMovePhase, copied: u64, total: u64, error: Option<AppError>) {
    sink.emit(CoreEvent::DataDirMoveProgress(DataDirMoveProgress { phase, bytes_copied: copied as i64, bytes_total: total as i64, error }));
}

impl Supervisor {
    pub async fn move_data_dir(&self, target: String) -> Result<(), AppError> {
        self.require_ready()?;
        let guard = self.inner.gate.try_enter(Activity::DataDirMove)?;
        let target = PathBuf::from(target);
        if !target.is_absolute() {
            return Err(AppError::invalid("target", "notAbsolute"));
        }
        if target.exists() && std::fs::read_dir(&target).map(|mut d| d.next().is_some()).unwrap_or(true) {
            return Err(AppError::invalid("target", "notEmpty"));
        }
        let old = self.paths();
        let (files, bytes) = tree_stats(old.data_root()).map_err(|e| AppError::file_access(old.data_root(), &e.to_string()))?;
        if (free_space(&target)? as f64) < bytes as f64 * 1.1 {
            return Err(AppError::file_access(&target, "insufficientSpace"));
        }
        let me = self.clone();
        tokio::spawn(async move {
            let _guard = guard;
            me.run_move(old, target, files, bytes).await;
        });
        Ok(())
    }

    async fn run_move(&self, old: Paths, target: PathBuf, files: u64, bytes: u64) {
        let sink = self.inner.sink.clone();
        emit(&sink, DataDirMovePhase::StoppingDb, 0, bytes, None);
        if let Err(e) = self.shutdown_db().await {
            let _ = self.start_cluster().await;
            emit(&sink, DataDirMovePhase::Failed, 0, bytes, Some(e));
            return;
        }
        emit(&sink, DataDirMovePhase::Copying, 0, bytes, None);
        let moved = async {
            let (src, dst, sink2) = (old.data_root().to_path_buf(), target.clone(), sink.clone());
            tokio::task::spawn_blocking(move || {
                let mut last = Instant::now();
                let mut copied = 0u64;
                copy_tree(&src, &dst, &mut copied, &mut |n| {
                    if last.elapsed() >= Duration::from_millis(250) {
                        last = Instant::now();
                        emit(&sink2, DataDirMovePhase::Copying, n, bytes, None);
                    }
                })
            })
            .await
            .map_err(|e| AppError::internal(e.to_string()))?
            .map_err(|e| AppError::file_access(&target, &e.to_string()))?;
            emit(&sink, DataDirMovePhase::Verifying, bytes, bytes, None);
            let (f2, b2) = tree_stats(&target).map_err(|e| AppError::file_access(&target, &e.to_string()))?;
            if self.inner.fail_next_verify.swap(false, Ordering::SeqCst) || f2 != files || b2 != bytes {
                return Err(AppError::internal("data dir move verify failed: file count or size differs"));
            }
            self.inner.settings.set_data_dir(&target.display().to_string())?;
            self.set_paths(old.with_data_root(target.clone()));
            emit(&sink, DataDirMovePhase::StartingDb, bytes, bytes, None);
            self.start_cluster().await
        }
        .await;
        match moved {
            Ok(()) => {
                let _ = tokio::fs::remove_dir_all(old.data_root()).await;
                emit(&sink, DataDirMovePhase::Done, bytes, bytes, None);
            }
            Err(e) => {
                let at_target = old.with_data_root(target.clone());
                let _ = PgController::new(at_target.clone()).stop(&at_target.pgdata(), StopMode::Immediate).await;
                let _ = tokio::fs::remove_dir_all(&target).await;
                let _ = self.inner.settings.set_data_dir(&old.data_root().display().to_string());
                self.set_paths(old.clone());
                let _ = self.start_cluster().await;
                emit(&sink, DataDirMovePhase::Failed, 0, bytes, Some(e));
            }
        }
    }
}
```

- [ ] **Step 7: Write `core/src/lifecycle/status.rs`** (spec Part 04 §9.2).
```rust
use crate::db::{pg_err, RFC3339_UTC_FMT};
use crate::dto::app::{AppPhase, LibraryCounts};
use crate::dto::error::AppError;
use crate::dto::settings::{DbStatus, IndexStatus, MaintenanceState};
use crate::lifecycle::supervisor::Supervisor;

impl Supervisor {
    pub async fn db_status(&self, maintenance: Option<MaintenanceState>) -> Result<DbStatus, AppError> {
        self.require_ready()?;
        let c = self.inner.db.app_pool()?.get().await.map_err(|e| AppError::internal(e.to_string()))?;
        let r1 = c.query_one("SELECT current_setting('server_version'), pg_database_size(current_database())", &[]).await.map_err(pg_err)?;
        let r2 = c
            .query_one(
                "SELECT (SELECT count(*) FROM image_blobs), (SELECT count(*) FROM attachments), \
                 (SELECT count(*) FROM messages), (SELECT count(*) FROM pst_sources)",
                &[],
            )
            .await
            .map_err(pg_err)?;
        let idx = c
            .query(
                "SELECT i.relname::text, t.relname::text, pg_relation_size(i.oid), x.indisvalid \
                 FROM pg_index x JOIN pg_class i ON i.oid = x.indexrelid JOIN pg_class t ON t.oid = x.indrelid \
                 JOIN pg_namespace n ON n.oid = t.relnamespace WHERE n.nspname = 'public' \
                 ORDER BY pg_relation_size(i.oid) DESC",
                &[],
            )
            .await
            .map_err(pg_err)?;
        let r3 = c
            .query_one(
                &format!(
                    "SELECT to_char(max(greatest(last_vacuum, last_autovacuum)) AT TIME ZONE 'UTC', '{f}'), \
                     to_char(max(greatest(last_analyze, last_autoanalyze)) AT TIME ZONE 'UTC', '{f}') \
                     FROM pg_stat_user_tables WHERE relname = 'messages'",
                    f = RFC3339_UTC_FMT
                ),
                &[],
            )
            .await
            .map_err(pg_err)?;
        Ok(DbStatus {
            phase: AppPhase::Ready,
            postgres_version: r1.get(0),
            data_dir: self.paths().data_root().display().to_string(),
            database_size_bytes: r1.get(1),
            counts: LibraryCounts { messages: r2.get(2), psts: r2.get(3) },
            image_count: r2.get(0),
            attachment_count: r2.get(1),
            indexes: idx.iter().map(|r| IndexStatus { name: r.get(0), table: r.get(1), size_bytes: r.get(2), valid: r.get(3) }).collect(),
            last_vacuum_at: r3.get(0),
            last_analyze_at: r3.get(1),
            maintenance,
        })
    }
}
```
`core/src/lifecycle/mod.rs`: keep the `pub mod` lines and add `pub use supervisor::{Supervisor, SupervisorConfig};`.

- [ ] **Step 8: Write the integration tests** `core/tests/lifecycle.rs` (write first, run to see the compile failure, then again after Steps 1–7):
```rust
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use outlooker_core::db::Db;
use outlooker_core::dto::app::{AppPhase, InitializeRequest, Language};
use outlooker_core::dto::error::{AppError, BusyReason};
use outlooker_core::dto::events::DataDirMovePhase;
use outlooker_core::dto::settings::SettingsPatch;
use outlooker_core::gate::{Activity, Gate};
use outlooker_core::lifecycle::{Supervisor, SupervisorConfig};
use outlooker_core::paths::{dev_pg_dir, Paths};
use outlooker_core::settings::SettingsStore;
use outlooker_core::sink::{CoreEvent, EventSink};
use tokio::sync::mpsc;

/// Forwards events to a channel so tests wait on events instead of polling.
struct ChannelSink(mpsc::UnboundedSender<CoreEvent>);
impl EventSink for ChannelSink {
    fn emit(&self, e: CoreEvent) {
        let _ = self.0.send(e);
    }
}

struct Rig {
    _dir: tempfile::TempDir,
    root: std::path::PathBuf,
    sup: Supervisor,
    db: Db,
    gate: Gate,
    settings: SettingsStore,
    events: mpsc::UnboundedReceiver<CoreEvent>,
}

fn rig_at(dir: tempfile::TempDir, app: &str, pg_dir: std::path::PathBuf) -> Rig {
    assert!(dev_pg_dir().join("bin").exists(), "PostgreSQL binaries missing: run node scripts/fetch-postgres.mjs");
    let paths = Paths::new(dir.path().join(app), dir.path().join("logs"), pg_dir);
    let settings = SettingsStore::load(paths.bootstrap_file()).unwrap();
    let (tx, events) = mpsc::unbounded_channel();
    let (db, gate) = (Db::new(), Gate::new());
    let config = SupervisorConfig { probe_interval: Duration::from_millis(100), ..SupervisorConfig::default() };
    let sup = Supervisor::new(paths, settings.clone(), gate.clone(), db.clone(), Arc::new(ChannelSink(tx)), config);
    let root = dir.path().join("data");
    Rig { _dir: dir, root, sup, db, gate, settings, events }
}

fn rig() -> Rig {
    rig_at(tempfile::tempdir().unwrap(), "app", dev_pg_dir())
}

async fn wait_phase(sup: &Supervisor, want: AppPhase) {
    let mut w = sup.phase_watch();
    tokio::time::timeout(Duration::from_secs(120), w.wait_for(|p| *p == want)).await.expect("phase timeout").unwrap();
}

async fn ready(r: &Rig) {
    r.sup.boot().await.unwrap();
    r.sup.initialize(InitializeRequest { data_dir: r.root.display().to_string(), language: Language::ZhTw }).await.unwrap();
    wait_phase(&r.sup, AppPhase::Ready).await;
}

async fn next_move_end(events: &mut mpsc::UnboundedReceiver<CoreEvent>) -> DataDirMovePhase {
    loop {
        let e = tokio::time::timeout(Duration::from_secs(120), events.recv()).await.expect("event timeout").unwrap();
        if let CoreEvent::DataDirMoveProgress(p) = e {
            if matches!(p.phase, DataDirMovePhase::Done | DataDirMovePhase::Failed) {
                return p.phase;
            }
        }
    }
}

fn phases(events: &mut mpsc::UnboundedReceiver<CoreEvent>) -> Vec<AppPhase> {
    let mut v = Vec::new();
    while let Ok(e) = events.try_recv() {
        if let CoreEvent::AppStatusChanged(s) = e {
            v.push(s.phase);
        }
    }
    v
}

fn kill(root: &Path) {
    let paths = Paths::new("/unused".into(), "/unused".into(), dev_pg_dir()).with_data_root(root.to_path_buf());
    let _ = std::process::Command::new(paths.pg_bin("pg_ctl")).args(["stop", "-m", "immediate", "-w", "-D"]).arg(paths.pgdata()).output();
}

#[tokio::test]
async fn first_run_reaches_ready() {
    let mut r = rig();
    ready(&r).await;
    let p = phases(&mut r.events);
    let order = [AppPhase::NeedsSetup, AppPhase::InitializingCluster, AppPhase::StartingDb, AppPhase::Ready];
    let mut it = p.iter();
    assert!(order.iter().all(|want| it.any(|got| got == want)), "phases {p:?}");
    assert!(r.root.join("outlooker-data.json").exists());
    assert_eq!(r.sup.status().await.counts.messages, 0);
    r.sup.shutdown_db().await.unwrap();
}

#[tokio::test]
async fn adopt_existing_data_root() {
    let r = rig();
    ready(&r).await;
    r.sup.shutdown_db().await.unwrap();
    let root = r.root.clone();
    let r2 = rig_at(tempfile::tempdir().unwrap(), "other-app", dev_pg_dir());
    r2.sup.boot().await.unwrap();
    r2.sup.initialize(InitializeRequest { data_dir: root.display().to_string(), language: Language::En }).await.unwrap();
    wait_phase(&r2.sup, AppPhase::Ready).await;
    r2.sup.shutdown_db().await.unwrap();
}

#[tokio::test]
async fn wrong_pg_version_fails() {
    let r = rig();
    ready(&r).await;
    r.sup.shutdown_db().await.unwrap();
    std::fs::write(r.root.join("pgdata/PG_VERSION"), "17\n").unwrap();
    let r2 = rig_at(tempfile::tempdir().unwrap(), "app2", dev_pg_dir());
    r2.settings.set_bootstrap(Some(r.root.display().to_string()), Language::En).unwrap();
    let err = r2.sup.boot().await.unwrap_err();
    assert!(format!("{err:?}").contains("unsupported PostgreSQL data version 17"));
    assert_eq!(r2.sup.phase(), AppPhase::Failed);
}

#[tokio::test]
async fn init_failure_removes_only_own_pgdata() {
    let r = rig_at(tempfile::tempdir().unwrap(), "app", std::path::PathBuf::from("/nonexistent/pg"));
    r.sup.boot().await.unwrap();
    r.sup.initialize(InitializeRequest { data_dir: r.root.display().to_string(), language: Language::ZhTw }).await.unwrap();
    wait_phase(&r.sup, AppPhase::Failed).await;
    assert!(!r.root.join("pgdata").exists());
    assert!(r.root.exists(), "the chosen data root itself is kept");
}

#[tokio::test]
async fn retry_from_failed_reaches_ready() {
    let r = rig();
    ready(&r).await;
    r.sup.shutdown_db().await.unwrap(); // not kill(): r's health monitor would restart the cluster
    std::fs::write(r.root.join("pgdata/PG_VERSION"), "17\n").unwrap();
    let r2 = rig_at(tempfile::tempdir().unwrap(), "app2", dev_pg_dir());
    r2.settings.set_bootstrap(Some(r.root.display().to_string()), Language::En).unwrap();
    assert!(r2.sup.boot().await.is_err());
    std::fs::write(r.root.join("pgdata/PG_VERSION"), "18\n").unwrap();
    r2.sup.retry().await.unwrap();
    wait_phase(&r2.sup, AppPhase::Ready).await;
    r2.sup.shutdown_db().await.unwrap();
}

#[tokio::test]
async fn retry_when_not_failed_is_invalid() {
    let r = rig();
    r.sup.boot().await.unwrap();
    assert_eq!(r.sup.retry().await, Err(AppError::invalid("phase", "notFailed")));
}

#[tokio::test]
async fn retry_without_cluster_returns_to_setup() {
    let r = rig_at(tempfile::tempdir().unwrap(), "app", std::path::PathBuf::from("/nonexistent/pg"));
    r.sup.boot().await.unwrap();
    r.sup.initialize(InitializeRequest { data_dir: r.root.display().to_string(), language: Language::ZhTw }).await.unwrap();
    wait_phase(&r.sup, AppPhase::Failed).await;
    r.sup.retry().await.unwrap();
    assert_eq!(r.sup.phase(), AppPhase::NeedsSetup);
}

#[tokio::test]
async fn health_restarts_killed_postmaster() {
    let r = rig();
    ready(&r).await;
    kill(&r.root);
    wait_phase(&r.sup, AppPhase::StartingDb).await;
    wait_phase(&r.sup, AppPhase::Ready).await;
    r.sup.shutdown_db().await.unwrap();
}

#[tokio::test]
async fn restart_budget_exhaustion_fails() {
    let r = rig();
    ready(&r).await;
    for _ in 0..3 {
        kill(&r.root);
        wait_phase(&r.sup, AppPhase::StartingDb).await;
        wait_phase(&r.sup, AppPhase::Ready).await;
    }
    kill(&r.root);
    wait_phase(&r.sup, AppPhase::Failed).await;
    assert!(r.sup.status().await.error.is_some());
}

#[tokio::test]
async fn move_data_dir_succeeds() {
    let mut r = rig();
    ready(&r).await;
    let target = r.root.with_file_name("moved");
    r.sup.move_data_dir(target.display().to_string()).await.unwrap();
    assert_eq!(next_move_end(&mut r.events).await, DataDirMovePhase::Done);
    wait_phase(&r.sup, AppPhase::Ready).await;
    assert!(target.join("outlooker-data.json").exists());
    assert!(!r.root.exists());
    assert_eq!(r.settings.bootstrap().data_dir.as_deref(), Some(target.display().to_string().as_str()));
    r.sup.shutdown_db().await.unwrap();
}

fn tree_bytes(dir: &Path) -> (u64, u64) {
    let (mut f, mut b) = (0, 0);
    for e in std::fs::read_dir(dir).unwrap() {
        let e = e.unwrap();
        if e.file_type().unwrap().is_dir() {
            let (x, y) = tree_bytes(&e.path());
            f += x;
            b += y;
        } else if !e.path().to_string_lossy().contains("pgdata/log") {
            f += 1;
            b += e.metadata().unwrap().len();
        }
    }
    (f, b)
}

#[tokio::test]
async fn move_verify_failure_rolls_back() {
    let mut r = rig();
    ready(&r).await;
    r.sup.shutdown_db().await.unwrap();
    let before = tree_bytes(&r.root.join("auth"));
    r.sup.boot().await.unwrap(); // restart after shutdown_db goes through boot
    wait_phase(&r.sup, AppPhase::Ready).await;
    r.sup.fail_next_move_verify();
    let target = r.root.with_file_name("moved");
    r.sup.move_data_dir(target.display().to_string()).await.unwrap();
    assert_eq!(next_move_end(&mut r.events).await, DataDirMovePhase::Failed);
    wait_phase(&r.sup, AppPhase::Ready).await;
    assert!(!target.exists(), "partial copy removed");
    assert_eq!(tree_bytes(&r.root.join("auth")), before, "old root untouched");
    assert_eq!(r.settings.bootstrap().data_dir.as_deref(), Some(r.root.display().to_string().as_str()));
    r.sup.shutdown_db().await.unwrap();
}

#[tokio::test]
async fn move_into_non_empty_is_invalid() {
    let r = rig();
    ready(&r).await;
    let target = r.root.with_file_name("busy");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("x"), b"x").unwrap();
    assert_eq!(r.sup.move_data_dir(target.display().to_string()).await, Err(AppError::invalid("target", "notEmpty")));
    r.sup.shutdown_db().await.unwrap();
}

#[tokio::test]
async fn move_while_import_is_busy() {
    let r = rig();
    ready(&r).await;
    let _import = r.gate.try_enter(Activity::Import).unwrap();
    let target = r.root.with_file_name("moved");
    assert_eq!(r.sup.move_data_dir(target.display().to_string()).await, Err(AppError::Busy { reason: BusyReason::ImportRunning }));
    r.sup.shutdown_db().await.unwrap();
}

#[tokio::test]
async fn settings_language_before_ready() {
    let r = rig();
    let s = r.settings.update(&r.db, SettingsPatch { language: Some(Language::En), webp_quality: None }).await.unwrap();
    assert_eq!(s.language, Language::En);
    assert_eq!(s.webp_quality, 80);
    assert_eq!(SettingsStore::load(r.root.with_file_name("app").join("settings.json")).unwrap().bootstrap().language, Language::En);
}

#[tokio::test]
async fn settings_webp_before_ready_is_not_ready() {
    let r = rig();
    let err = r.settings.update(&r.db, SettingsPatch { language: Some(Language::En), webp_quality: Some(50) }).await.unwrap_err();
    assert_eq!(err, AppError::DbNotReady);
    assert_eq!(r.settings.bootstrap().language, Language::ZhTw, "nothing written on error");
}

#[test]
fn bootstrap_write_is_atomic() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("settings.json");
    let s = SettingsStore::load(file.clone()).unwrap();
    s.set_bootstrap(Some("/old".into()), Language::En).unwrap();
    // Simulated crash after the temp write and before the rename: a stale, partial temp file.
    std::fs::write(dir.path().join("settings.json.tmp"), b"{\"data_dir\": \"/ne").unwrap();
    assert_eq!(SettingsStore::load(file.clone()).unwrap().bootstrap().data_dir.as_deref(), Some("/old"));
    s.set_data_dir("/new").unwrap();
    assert_eq!(SettingsStore::load(file).unwrap().bootstrap().data_dir.as_deref(), Some("/new"));
    assert!(!dir.path().join("settings.json.tmp").exists());
}
```
Run: `cargo test -p outlooker-core --locked --features test-support --test lifecycle` and `cargo test -p outlooker-core --lib lifecycle::init`. Expected: 16 + 1 tests `ok`.
Run: `grep -rE "crate::(importer|search|sql_console|maintenance)" src-tauri/crates/outlooker-core/src/lifecycle src-tauri/crates/outlooker-core/src/settings; echo "rc=$?"`. Expected: `rc=1`.

- [ ] **Step 9: Commit.** Commit `core/src/lifecycle/`, `core/src/settings/mod.rs`, `core/tests/lifecycle.rs` with message `feat(core): lifecycle supervisor (start, first run, retry, health, data dir move, db status) and settings store`.

---

## R1-REV / R2-REV steps (reviewer; edits no code)

- [ ] **Step 1: Per landed commit** (the leader forwards each task's `READY_FOR_SIGNOFF` with its commit hashes):
  - `git show --stat <hash>`: every path is owned by that task in parent §5 (Round 1) or §6 (Round 2); any other path → blocker "ownership".
  - Read the task's Stage 1 block (this file or the backend / frontend plan file) and check every Produces signature in the code (`grep -n` for each name).
  - Re-run the task's acceptance commands exactly as written; record each command and its RC.
  - Versions: `cd src-tauri && cargo tree -p outlooker-core --depth 1 -e normal` and `grep -nE '"[\^~]' package.json` against Part 01.
  - Security focus: R1-04 (grants), R1-05 (sanitizer), R1-07 (iframe sandbox), R2-11 / R2-12 (no user value in SQL text: `grep -nE 'format!\(.*(SELECT|WHERE|INSERT)' src-tauri/crates/outlooker-core/src` and read every hit), R2-13 (CSP, `mailimg` method/id checks).
- [ ] **Step 2: Write `docs/logs/review-round<N>.md`** with one section per task: `## R1-0x — PASS|BLOCKED`, commit hashes, then findings as `- [blocker|should-fix|nit] file:line — finding — evidence (command + RC or quoted line)`. R2-REV adds `## Security` and `## Round 1 regression` (all §7.1 gate commands re-run on the Round 2 head).
- [ ] **Step 3:** Send blockers to the leader (≤ 1500 chars, path to the log). At most two review passes per task; should-fix and nit go to the parking lot. Close with `grep -c BLOCKED docs/logs/review-round<N>.md` = `0`.

## Windows phase steps (W1–W3, via SSH; after both macOS gates)

Rules: every remote multi-step command is one `powershell -Command "<a>; <b>"` (semicolons, never cmd `&`); each step's log is captured on the Windows machine and copied back to `docs/logs/windows-phase-<date>.md` by the leader; exit codes are read from `$LASTEXITCODE` printed right after each command.

- [ ] **W0: Prepare.** `ssh <host> powershell -Command "git --version; rustup show active-toolchain; node --version; gh --version; Get-ItemProperty 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}' | Select-Object pv"`. Expected: Rust 1.99.1 (MSVC host), Node 24.22.x, WebView2 version printed. Transfer the repo at the Round 2 head commit (`git bundle create scripts/tmp/outlooker.bundle main` on macOS, `scp`, `git clone outlooker.bundle outlooker` on Windows).
- [ ] **W1: First Windows build + locale self-check.** `ssh <host> powershell -Command "cd outlooker; node scripts/fetch-postgres.mjs; echo RC=$LASTEXITCODE; cd src-tauri; cargo build --workspace --locked; echo RC=$LASTEXITCODE; cargo test -p outlooker-core --locked --features test-support --test pg_lifecycle -- --nocapture; echo RC=$LASTEXITCODE"`. Pass: three `RC=0`; the log line `initdb locale ... passed the CJK trigram self-check` names `en-US` or `English_United States.65001`. If no candidate passes, stop and report (release blocker; spec §0.3).
- [ ] **W2: Outlook fixtures.** On the Windows desktop session (Outlook needs the interactive user): `powershell -Command "cd outlooker; pwsh -NoProfile -File scripts/fixtures/make-pst.ps1 -Out tests/fixtures/pst; echo RC=$LASTEXITCODE"`. Pass: `fixtures ok: basic, cjk, inline, attach, dup-a, dup-b, hostile, corrupt` and `RC=0`. A thrown "Outlook did not keep property …" is the spec §0.3 risk; report it verbatim. Copy the 8 files back (`scp`), the leader commits them, then run on both machines: `cargo test -p outlooker-core --locked --features test-support,outlook-fixtures --test import_pipeline` → `RC=0`.
- [ ] **W3: Windows release acceptance.** `ssh <host> powershell -Command "cd outlooker/src-tauri; cargo test --workspace --locked --features outlooker-core/test-support; echo RC=$LASTEXITCODE; cd ..; npm ci; npm run e2e; echo RC=$LASTEXITCODE; npm run tauri build; echo RC=$LASTEXITCODE; Get-ChildItem src-tauri/target/release/bundle/nsis | Select-Object Name, Length"`. Pass: three `RC=0`, the NSIS installer listed with its size. Manual checklist in the desktop session, ticked in the log: (a) install, launch, onboarding to `Ready`; (b) launch a second time → `(Get-Process Outlooker).Count` is 1 and the first window is focused; (c) Settings → move the data folder to another volume (e.g. `D:\OutlookerData`) → `Done`, app `Ready`; (d) preview a mail with an inline image (loaded from `http://mailimg.localhost/<id>`).

---

# Part C — Open items and self-review

## C.1 Open items (for the leader)
1. The four crates of §0.2 are approved and in spec Part 01 §8; R1-01 kickoff writes them into `core/Cargo.toml`.
2. `app_initialize` when not in `NeedsSetup` returns `invalidInput{field:"phase", reason:"alreadyInitialized"}`; spec Part 03 §4 says `busy`, but no `BusyReason` fits. Spec owner may add a reason or accept this.
3. `ensure_search_indexes` reports progress per index (6 steps), not `pg_stat_progress_create_index` block counts (Part 04 §6.4), because the building connection is busy.
4. Command argument names that R2-13's `#[tauri::command]` functions must use (they are the TS wrapper keys): `req`, `query`, `id`, `input`, `execution_id` (TS `executionId`), `paths`, `pst_id` (TS `pstId`), `patch`, `target`, `task`.
5. CSP keeps `style-src 'unsafe-inline'` (Fluent UI runtime styles and sanitized inline `style` attributes in mail HTML). Scripts stay `'self'`.

## C.2 Accepted deviations from the spec (leader rulings)
1. `app_initialize` when the phase is not `NeedsSetup` returns `invalidInput{field:"phase", reason:"alreadyInitialized"}` (spec Part 03 §4 says `busy`; no `BusyReason` fits).
2. `ensure_search_indexes` reports progress per index (6 steps), not `pg_stat_progress_create_index` block counts (Part 04 §6.4).
3. CSP keeps `style-src 'unsafe-inline'`: Fluent UI v9 (Griffel) injects runtime styles and sanitized mail keeps inline `style` attributes.

## C.3 Self-review results
- Spec coverage: every spec section the parent plan §10.1 assigns to R1-01, R1-02, R1-03, R1-04 and R2-08 maps to a Stage 1 Behavior line and a Stage 2 step in this file; Windows-only items are in the Windows phase steps.
- Placeholder scan: `grep -nE 'T[B]D|T[O]DO|implement [l]ater|fill [i]n' docs/superpowers/plans/2026-10-21-outlooker-plan-foundation.md` finds nothing.
- Type consistency: Stage 2 code uses only names from the Part A Produces blocks (`Paths`, `PgController`, `StopMode`, `PgStatus`, `Credentials`, `Db`, `DbConn`, `Pools`, `SEARCH_INDEXES`, `TSV_EXPR`, `TestCluster`, `Supervisor`, `SupervisorConfig`, `SettingsStore`, `Bootstrap`, `Gate`, `Activity`, `CoreEvent`, `EventSink`, `RecordingSink`).
- Cross-check with planner-2 (backend plan) and planner-3 (frontend plan): see the leader report; consumers of C1–C8, C14, C15 must match Part A of this file.
