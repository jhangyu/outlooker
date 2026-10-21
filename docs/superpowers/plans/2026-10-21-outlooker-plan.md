# Outlooker Implementation Plan — Outline

> **For agentic workers:** REQUIRED SUB-SKILL: Use podium:team-spawn (recommended) or podium:team-fable to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

Status: OUTLINE, revision 2 (architect reviews applied, §11). Stage 1 skeleton scope only: task boundaries, file ownership, interfaces between tasks, acceptance. Full per-task Interfaces/Behavior/Constraints blocks and Stage 2 steps are written by three planners in the files named in §8 "Detail-fill split".

**Goal:** Build Outlooker, a Tauri 2 desktop app (Windows + macOS) that imports Outlook PST files into a bundled PostgreSQL 18.6 and searches and reads the mail fast, as defined in `docs/superpowers/specs/2026-10-21-outlooker-design.md` (the spec) and `docs/specs/requirements.md`.

**Architecture:**
- One OS process: Tauri 2 shell (`src-tauri/`, crate `outlooker`) + Rust core crate `outlooker-core` (no Tauri dependency; also holds the start/shutdown composition) + crate `pff` (vendored libpff C compiled by `cc`, hand-written FFI, `!Send` handles).
- PostgreSQL 18.6 binaries ship as Tauri resources; the core runs `initdb` / `pg_ctl` as child processes on `127.0.0.1` with a random port and SCRAM roles `outlooker_admin` / `outlooker_app` / `outlooker_ro`.
- Import pipeline: 1 reader thread (libpff) → rayon (charset, RTF de-encapsulation, ammonia sanitize, WebP) → 1 tokio writer (reorder by seq, binary COPY to staging, set-based upsert, checkpoint in the same transaction).
- Search: DSL tree → parameterized SQL over pg_trgm GIN + English tsvector, two-phase keyset paging, capped count. Advanced SQL runs on the read-only role inside `BEGIN READ ONLY … ROLLBACK`.
- Frontend: Vite + React 19 + Fluent UI v9 (`webLightTheme` only), TanStack Query + `@tanstack/react-virtual`, i18next zh-TW + en, CodeMirror 6, mail HTML in `<iframe sandbox="" srcdoc>`, images via the `mailimg` URI scheme.
- Types: Rust DTOs are the single source; ts-rs 12.0.1 generates `src/api/bindings/`; hand-written `commands.ts` / `events.ts` wrappers with drift tests. 24 commands (Part 03 §3 plus `app_retry`), 6 events.
- Progress: global Tauri events (≤ 4/s, < 1 KB), no `Channel`.

**Tech Stack:** Rust 1.99.1, Tauri 2.12.5, tokio 1.54.0, tokio-postgres 0.7.18 + deadpool-postgres 0.14.2 + refinery 0.10.0, PostgreSQL 18.6 (theseus-rs 18.6.1), libpff 20260625 via cc 1.9.0, ammonia 4.2.4, html2text 0.17.4, encoding_rs 0.8.42, chardetng 1.0.0, image 0.25.10, webp 0.3.1, rayon 1.12.0, blake3 1.8.7, ts-rs 12.0.1, jsonschema 0.59.0 (dev only); Node 24.22.1, Vite 8.3.5, React 19.3.1, TypeScript 7.0.2, @fluentui/react-components 9.74.10, CodeMirror 6, TanStack Query 5.104.5 / Virtual 3.14.21, i18next 26.5.4, Vitest 5.0.5, WebdriverIO 9.32.0 + @wdio/tauri-service 1.5.0.

**Decisions (hard-to-reverse only; all approved by the leader after architect review):**
- DTO placement: all ts-rs DTOs live in the leaf module `outlooker-core/src/dto/` owned by one Round 1 task (alternatives: DTOs inside each service module; a separate `outlooker-dto` crate). Drivers: Round 1 parallel tasks code against one frozen type set; the Command Layer and the services both need the types; no Tauri dependency needed. Drawback: a module not named in spec Part 02 §3. `dto` imports no service module; `From<ServiceError> for AppError` mappings live in the services (grep check in R1-01).
- Locale files split by namespace: `src/locales/<lang>/<namespace>.json` with `<lang>` exactly `zh-TW` and `en` (alternatives: the spec's literal `src/locales/{zh-TW,en}.json`; one owner who writes every key). Drivers: two frontend tasks run in parallel and must not edit the same file; i18next namespaces are native; Vite imports JSON statically. Drawback: spec Part 01 §2 / Part 02 §3 wording differs until the spec owner updates it.
- Kickoff skeleton: task R1-01 lands the workspace, every manifest with every Part 01 dependency, the toolchain pin, and every module file of the §1 file map as an empty file before other tasks commit (alternatives: each task adds its own `mod` lines and dependencies). Drivers: `lib.rs`, `mod.rs`, `Cargo.toml` and `package.json` are touched by everyone otherwise. Drawback: R1-01 is on every task's critical path for its first commit (target: kickoff commit within its first step).
- Start/shutdown composition lives in core (`core::app`), not in `src-tauri/src/lib.rs` (alternative: compose in the Tauri crate). Driver: Part 02 §3 keeps services testable without a window; the exit and recovery order must have integration tests.

Plan gate: `/Users/jhangyu/.claude/plugins/cache/jhangyu/podium/1.1.0/protocols/review.md` §L0 Plan gate.

## Global Constraints

Every task's requirements implicitly include this section.

- **Versions:** every dependency is pinned to the exact version in spec Part 01 (§1–§9), plus `jsonschema = { version = "=0.59.0", default-features = false }` as a dev-dependency of `outlooker-core` only. Before the first manifest write, re-check each version against its registry (crates.io / npm). A newer compatible patch is allowed only after that check; record the checked version in the commit message. npm installs use `--save-exact` (no `^` / `~`). `Cargo.lock` is committed and every build uses `--locked`. Do not add a dependency that Part 01 does not list without leader approval.
- **Toolchain:** Rust 1.99.1 via `src-tauri/rust-toolchain.toml`; Node 24.22.1 via repo-root `mise.toml` (mise) and `>=24.22 <25` in `package.json` `engines`.
- **Forbidden dependencies:** `@uiw/react-codemirror`, `@codemirror/theme-one-dark`, any Fluent dark theme (`webDarkTheme`, `teamsDarkTheme`), zustand, sqlx, bindgen, testcontainers, `postgresql_embedded`, tauri-specta, mail-parser, webdriverio 10.x, any npm archive-extraction package (the PG fetch script uses the system `tar`).
- **Theme:** light theme only. One `FluentProvider theme={webLightTheme}`. No `prefers-color-scheme: dark`, no dark tokens, no theme switch.
- **i18n:** zh-TW and en from day one. No user-visible string literal in components; every string comes from `src/locales/<lang>/<namespace>.json`. Both languages have identical key sets. The core never returns user-facing text (spec Part 03 §1).
- **Layout and prototype:** target 1920×1080, full-height multi-column grids, only the 48 px title bar as header. `docs/design/variant-1/` (and its `NOTES.md` tokens and component map) is the visual reference **minus the out-of-scope controls below**.
- **Out-of-scope UI controls (user ruling — the UI must not show them):** exact deleted / kept preview in the PST removal dialog; true pause, "pause all"; converter worker-count setting; re-convert / re-encode existing images; disk-usage breakdown by category (show `databaseSizeBytes` only); SQL "Format" button; search-term highlighting in list or reading pane; drag-and-drop import of `.pst` files.
- **App identifier:** `us.jhangy.outlooker`.
- **Fixed limits (spec §0.2):** search count exact up to 10,000 then "10,000+"; result list ≤ 100,000 rows; Advanced SQL 30 s timeout and ≤ 10,000 rows (the UI sends `maxRows: 10000`); progress events ≤ 4/s, payload < 1 KB.
- **Type rule:** Rust structs/enums in `outlooker-core/src/dto/` are the only type definitions. Hand-written TS copies of these types are forbidden (also as temporary code); TS imports from `src/api/bindings/`.
- **SQL rule:** no user value is ever concatenated into SQL (spec Part 04 §11).
- **Platform parity:** one code path per feature on all platforms. Platform branches are allowed only where the spec defines them (MSVC vs Unix libpff build, Windows `http://mailimg.localhost` URL, initdb locale candidates).
- **Git (shared working tree):** no `git stash` (including `git stash list`), no `git reset`, no `git checkout --`, no `git clean`, no `git add -A` / `git add .`. Commit only your own files: `git add <own paths>` then `git commit -F <msgfile> -- <own paths>`. No `;` inside `-m` text. Run `git rev-parse --abbrev-ref HEAD` before and `git status --porcelain` after each commit. `git mv` lists both old and new paths.
- **Ownership:** a task edits only the paths it owns in the round ownership table (§5, §6). A needed change in another task's file goes to the leader, who routes it to the owner.
- **No sleep / polling** in scripts or tests; long-running commands (full builds, 1M load test) run only through the round's gate-runner.
- **Tests that need an external artifact** (fetched PostgreSQL, Outlook-generated fixture PSTs) fail with an explicit message when the artifact is missing. They never skip silently.
- **Test seams:** production constants stay as in the spec (search window W = 5,000; batch limits 1,000 items / 64 MB / 1 s; free-space threshold 2 GiB); tests may override them only through an explicit parameter or `cfg(test)` hook named in the task.
- **macOS first, Windows last:** every Round 1 and Round 2 gate runs on the macOS dev machine. Windows work runs only in the Windows phase (§7.3) after both macOS gates pass; no macOS gate depends on it.

---

## 1. Repo file map and owners

Source: spec Part 02 §3 (repo layout), Part 01 §3 (native paths), Part 03 §1.1 (bindings), Part 04 §12 (fixtures, load generator). Columns R1 / R2 give the owner task in each round. "kickoff" = R1-01 creates the file empty (or as a typed stub, §2.1) in its kickoff commit and never edits it again in Round 1; the file is untouched in Round 1 unless another R1 task is named. Paths relative to repo root. `core` = `src-tauri/crates/outlooker-core`; `crates/` = `src-tauri/crates/`.

### 1.1 Frontend (Vite app at repo root)

| Path | Purpose | R1 owner | R2 owner |
|---|---|---|---|
| `mise.toml` | `[tools] node = "24.22.1"` (Node pin for mise) | R1-01 | R2-14 |
| `package.json`, `package-lock.json` | All npm deps of Part 01 §2, §3, §9 pinned exactly; `engines.node`; scripts `dev`, `build`, `typecheck`, `test`, `e2e`, `tauri` | R1-01 | R2-14 |
| `vite.config.ts` (includes Vitest config), `tsconfig.json`, `index.html` | Build + test config | R1-01 | R2-14 |
| `.gitignore` | Adds `src-tauri/resources/postgresql/`, `src-tauri/target/` | R1-01 | R2-14 |
| `src/main.tsx` | React root mount | R1-01 | R2-14 |
| `src/test/setup.ts` | Vitest + Testing Library setup | R1-01 | R2-14 |
| `src/api/call.ts`, `src/api/commands.ts`, `src/api/events.ts`, `src/api/api.test.ts` | Typed invoke/listen wrappers + drift tests (Part 03 §1.1) | R1-01 | R2-14 |
| `src/api/bindings/**` | ts-rs output + `commands.json` + `events.json` (generated by `cargo test`, committed) | R1-01 | R2-13 |
| `src/app/**` | App Frame: FluentProvider `webLightTheme`, `i18n.ts`, TitleBar, NavRail, screen router, boot sequence, AppStatus context, Failed screen, Toaster, global tests (locale parity, no-literal, no-dark, no-out-of-scope-controls) | R1-06 | R2-14 |
| `src/components/common/**` | `EmptyState`, `formatAppError` | R1-06 | R2-14 |
| `src/components/mail/**` | `MailList` (virtualized), `ReadingPane`, `MailPreview` (sandboxed iframe), `AttachmentCard` | R1-07 | R2-14 |
| `src/components/dsl/**` | DSL blocks condition builder | R1-07 | R2-14 |
| `src/components/sql-editor/**` | In-house CodeMirror 6 wrapper (~30 lines) + static view schema | R1-07 | R2-14 |
| `src/screens/onboarding/**`, `src/screens/import/**`, `src/screens/settings/**` | Screens 3, 4, 5 (onboarding) | R1-06 | R2-14 |
| `src/screens/search/**`, `src/screens/advanced-sql/**` | Screens 1, 2 | R1-07 | R2-14 |
| `src/locales/{zh-TW,en}/{common,onboarding,import,settings}.json` | Locale namespaces | R1-06 | R2-14 |
| `src/locales/{zh-TW,en}/{search,sql}.json` | Locale namespaces | R1-07 | R2-14 |
| `wdio.conf.ts`, `tests/e2e/**` | WebdriverIO E2E (Part 01 §9, Part 02 §10) | — (not created in R1) | R2-14 |

### 1.2 Tauri app crate and native inputs (`src-tauri/`)

| Path | Purpose | R1 owner | R2 owner |
|---|---|---|---|
| `src-tauri/Cargo.toml` (workspace root + crate `outlooker`), `src-tauri/Cargo.lock` | Workspace members `crates/pff`, `crates/outlooker-core`, `crates/outlooker-loadgen`; app deps of Part 01 §3; feature `e2e` | R1-01 | R2-13 |
| `src-tauri/rust-toolchain.toml` | `channel = "1.99.1"` | R1-01 | R2-13 |
| `src-tauri/build.rs`, `src-tauri/tauri.conf.json`, `src-tauri/icons/**` | Tauri config: identifier, CSP (Part 03 §5.6). **No `bundle.resources` in Round 1** (R2-13 adds it) | R1-01 | R2-13 |
| `src-tauri/capabilities/default.json` | Includes the dialog plugin `open` permission and log permission | R1-01 | R2-13 |
| `src-tauri/.cargo/config.toml` | `TS_RS_EXPORT_DIR`, `TS_RS_LARGE_INT` (Part 03 §1.1) | R1-01 | R2-13 |
| `src-tauri/src/main.rs`, `src-tauri/src/lib.rs` | App Shell | R1-01 (plugins + empty handler only) | R2-13 |
| `src-tauri/src/commands/{mod,app,search,sql,import,settings}.rs` | Command Layer | kickoff | R2-13 |
| `src-tauri/src/events.rs` | Event Emitter (Tauri `EventSink`, throttling) | kickoff | R2-13 |
| `src-tauri/src/mailimg.rs` | `mailimg` URI scheme handler glue | kickoff | R2-13 |
| `src-tauri/schemas/condition-group.json` | DSL JSON schema (Part 03 §5.1) | R1-01 | R2-11 |
| `src-tauri/native/libpff/**` (incl. `ORIGIN`) | Vendored prepared libpff sdist tree (fork commit `bb5bcc0`, CI run `32393676269`) | R1-02 | R2-09 |
| `src-tauri/native/postgresql.sha256` | Pinned sha256 per target triple (theseus-rs 18.6.1, `.tar.gz` assets) | R1-03 | R2-08 |
| `src-tauri/resources/postgresql/` | Fetched + pruned PG 18.6 (gitignored; produced by `scripts/fetch-postgres.mjs`) | R1-03 | R2-08 |

### 1.3 Rust crates (`src-tauri/crates/`)

| Path | Purpose | R1 owner | R2 owner |
|---|---|---|---|
| `crates/pff/**` | libpff build (`build.rs` with `cc`, configure in a copy of the tree), `ffi.rs` (~45 `extern "C"`), safe `!Send` wrapper, tests | R1-02 | R2-09 |
| `core/Cargo.toml` | Every core dependency of Part 01 §5, §7, §8, §9 pinned; `jsonschema` dev-dependency; feature `test-support` | R1-01 | R2-13 |
| `core/src/lib.rs` | Module declarations | R1-01 | R2-13 |
| `core/src/dto/**` | All Part 03 types (`error`, `app`, `search`, `sql`, `import`, `settings`, `events`, `commands` = `COMMANDS` const) + ts-rs export tests; leaf module | R1-01 | R2-13 |
| `core/src/sink.rs` | `EventSink` trait, `CoreEvent`, `RecordingSink`, event name consts | R1-01 | R2-13 |
| `core/src/app/**` | `start_services` / `shutdown` composition (Part 02 §2.2 steps 8–9, §2.5) | kickoff | R2-13 |
| `core/src/gate/**` | Work Gate (Part 02 §3.1) | R1-01 | R2-08 |
| `core/src/paths/**` | Paths (Part 02 §8) | R1-03 | R2-08 |
| `core/src/lifecycle/mod.rs`, `core/src/lifecycle/pg.rs` | PG Controller (Part 02 §2.2 steps 4–5, §2.3 steps 4–6) | R1-03 | R2-08 |
| `core/src/lifecycle/{supervisor,init,start,health,move_dir,status}.rs` | Lifecycle Supervisor | kickoff | R2-08 |
| `core/src/settings/**` | Settings Store | kickoff | R2-08 |
| `core/src/db/**` (incl. `db/migrations/V1__schema.sql`, `V2__roles_views.sql`, `db/credentials.rs`, `db/indexes.rs`) | Schema, roles, views, pools, credentials, search-index DDL routines, `TSV_EXPR` | R1-04 | R2-10 |
| `core/src/test_support/**` | `TestCluster` helper (feature `test-support`) | R1-04 | R2-10 |
| `core/src/importer/{mod,types}.rs` | Importer module root, `RawItem` / `NormalizedItem` | R1-05 | R2-09 |
| `core/src/importer/normalize/**`, `core/src/importer/{webp,dedupe}.rs` | Normalize, WebP, dedupe key | R1-05 | R2-09 |
| `core/src/importer/{queue,pst_reader,inflight,progress}.rs` | Queue, reader thread, poison-item marker, throughput/ETA | kickoff | R2-09 |
| `core/src/importer/{writer,remove}.rs` | Bulk writer (incl. disk guard), PST removal | kickoff | R2-10 |
| `core/src/search/**` | Search Service (compile, cursor, execute, filter, mail, image) | kickoff | R2-11 |
| `core/src/sql_console/**` | SQL Console Service | kickoff | R2-12 |
| `core/src/maintenance/**` | Maintenance Service | kickoff | R2-12 |
| `core/tests/pg_lifecycle.rs` | PG Controller integration tests | R1-03 | R2-08 |
| `core/tests/db_schema.rs` | Migration, self-check, constraint, role tests | R1-04 | R2-10 |
| `core/tests/golden/**` | Sanitizer / normalize golden files | R1-05 | R2-09 |
| `core/tests/lifecycle.rs` | Supervisor integration tests | — | R2-08 |
| `core/tests/app_compose.rs` | Start / shutdown composition tests | — | R2-13 |
| `core/tests/import_pipeline.rs` | Import tests on `outlook.pst` + copies + synthetic `RawItem`s; Outlook-fixture tests behind feature `outlook-fixtures` | — | R2-09 |
| `core/tests/{writer,removal}.rs` | Writer and removal tests (no PST needed) | — | R2-10 |
| `core/tests/search.rs` | Search Service tests on seeded rows | — | R2-11 |
| `core/tests/{sql_console,maintenance}.rs` | Read-only bypass + maintenance tests | — | R2-12 |
| `core/examples/import_bench.rs` | Import throughput bench (Part 02 §9) | — | R2-09 |
| `crates/outlooker-loadgen/**` | 1M-message generator, `seed` subcommand for E2E, perf harness (`perf_` ignored tests) (Part 04 §12.3, Part 02 §9) | kickoff (empty `main.rs` + `Cargo.toml`) | R2-10 |

### 1.4 Scripts, fixtures, logs

| Path | Purpose | R1 owner | R2 owner |
|---|---|---|---|
| `scripts/fetch-postgres.mjs` | Download (`fetch`), verify (`node:crypto`), extract (system `tar`), prune PG 18.6 (Part 01 §3, §6) | R1-03 | R2-08 |
| `scripts/vendor-libpff.sh` | `gh run download 32393676269 -R jhangyu/libpff -n cibw-sdist` → extract into `src-tauri/native/libpff/` → write `ORIGIN` (commit + run id); no autotools | R1-02 | R2-09 |
| `scripts/fixtures/make-pst.ps1` | Outlook COM fixture generator (Part 04 §12.1) — runs only in the Windows phase | R1-02 | R2-09 |
| `tests/fixtures/external/outlook.pst`, `tests/fixtures/external/NOTICE` | libyal testdata `pst/outlook.pst` (commit `c3631ff`), CC-BY-4.0, "Copyright libyal contributors", source URL | R1-02 | R2-09 |
| `tests/fixtures/pst/*.pst` | The 8 Outlook-generated fixtures of Part 04 §12.1 (created in the Windows phase, committed by the leader) | R1-02 (script author) | R2-09 |
| `docs/logs/review-round1.md` | Round 1 review findings | R1-REV | — |
| `docs/logs/review-round2.md` | Round 2 review findings | — | R2-REV |
| `docs/logs/loadtest-<date>.md`, `docs/logs/perf-<date>.md` | Load test / perf results | — | R2-10 (harness writes them; run by the gate-runner) |

---

## 2. Shared contracts and how parallel tasks proceed

### 2.1 Kickoff commit (R1-01, step 1, before any other task commits)

R1-01 lands one commit that contains:
1. `src-tauri/Cargo.toml` (workspace + app crate), `src-tauri/rust-toolchain.toml`, `core/Cargo.toml`, `crates/pff/Cargo.toml` (deps only; R1-02 then owns the crate), `crates/outlooker-loadgen/Cargo.toml` + empty `main.rs`, with **every** dependency of Part 01 pinned, plus `Cargo.lock`.
2. `package.json` with every npm dependency of Part 01 §2, §3, §9 pinned exactly and `engines.node`, plus `package-lock.json`.
3. `tauri.conf.json` **without** `bundle.resources`, so `cargo build --workspace` passes on a fresh checkout before PostgreSQL is fetched.
4. `core/src/lib.rs` declaring every top-level module, and every Rust file of §1.3 marked "kickoff" or owned by another R1 task as an empty file (each `mod.rs` already declares its submodules), so `cargo build --workspace --locked` passes.
5. Typed TS stubs that compile, so the R1-06 router and the R1-07 screens can import each other before either lands: `src/screens/{search,advanced-sql,import,settings,onboarding}/index.tsx` each export `<Name>Screen` returning `null`; `src/components/common/index.ts` exports `EmptyState` and `formatAppError` with the §2.2 signatures; `src/app/App.tsx` exports `App` (rendered by `src/main.tsx`) and `src/app/AppStatusContext.tsx` exports `useAppStatus`. Every locale namespace file exists as `{}`.

After the kickoff commit, each file belongs only to its §1 owner. Other tasks may start coding before the kickoff lands, but commit only after it. Until C1 lands, frontend tasks use the kickoff stubs and the generated bindings only; they do not commit TS copies of DTO types.

### 2.2 Cross-task interfaces (frozen by the detail planners; producer task in brackets)

Names below are binding. Exact parameter and return types are written in the producer's Produces block by its detail planner (§8) and must match spec Part 03 type names.

| Id | Interface | Producer | Consumers |
|---|---|---|---|
| C1 | DTO types: every Rust type of Part 03 §1.2, §4–§9 under `outlooker_core::dto::{error,app,search,sql,import,settings,events}`, names identical to Part 03; `dto::commands::COMMANDS: &[&str]` (the 24 command names: Part 03 §3 plus `app_retry`) | R1-01 | all |
| C2 | `outlooker_core::sink::{EventSink, CoreEvent, RecordingSink}`; `trait EventSink: Send + Sync { fn emit(&self, event: CoreEvent); }`; `CoreEvent` has one variant per Part 03 §9 event; `CoreEvent::name(&self) -> &'static str` | R1-01 | R2-08, R2-09, R2-10, R2-12, R2-13 |
| C3 | Work Gate: `gate::Gate::try_enter(&self, Activity) -> Result<GateGuard, AppError>` (`AppError::Busy`), `Activity { Import, Removal, Maintenance, DataDirMove }`, guard releases on drop; conflict rules of Part 03 §7.2, §8 | R1-01 | R2-08, R2-09, R2-12 |
| C4 | TS client: `api.<camelCaseName>(…)` per command in `src/api/commands.ts` (incl. `api.appRetry()`); `onEvent(name, handler)` in `src/api/events.ts`; rejections re-thrown as `AppError` | R1-01 | R1-06, R1-07, R2-14 |
| C5 | `pff` safe wrapper: `File`, `Folder`, `Message`, `Recipient`, `Attachment` types and methods (open, root folder, sub-folders, sub-messages, identifier, MAPI property getters, three bodies, recipients, attachments incl. content id and data bytes) | R1-02 | R2-09 |
| C6 | `paths::Paths` (data root, pgdata, bin, credentials, marker, inflight marker, log dir) | R1-03 | R1-04, R2-08, R2-09, R2-10, R2-13 |
| C7 | `lifecycle::pg::PgController { init, start, stop, status }` incl. locale candidates + self-check, `StopMode { Fast, Immediate }` | R1-03 | R1-04, R2-08, R2-10 |
| C8 | `db::Db` (shared, cloneable handle to the running cluster: `app_pool()`, `ro_pool()`, `connect_admin()`, `connect_app()`, all `Err(dbNotReady)` while no cluster is connected; the Supervisor sets and clears it, so services never hold a stale port after a PG restart), `db::{migrate, connect_admin, Pools { app, ro }, cjk_self_check}`, `db::credentials::Credentials`, `db::indexes::{SEARCH_INDEXES, TSV_EXPR, drop_search_indexes, ensure_search_indexes}` (drop / ensure take an `outlooker_admin` connection from `connect_admin`, never the app pool — only the table owner may create or drop indexes, Part 04 §6.4), `test_support::TestCluster` | R1-04 | R2-08 … R2-13 |
| C9 | `importer::types::{RawItem, NormalizedItem, ImageIn, ImageOut}`, `normalize::normalize`, `webp::convert`, `dedupe::key` | R1-05 | R2-09, R2-10 |
| C10 | Frontend shared: `EmptyState` props, `formatAppError(err: AppError, t: TFunction): string`, `useAppStatus(): AppStatus` (phase from `app:status-changed`, counts from `library:changed`), screen entry names `SearchScreen`, `AdvancedSqlScreen`, `ImportScreen`, `SettingsScreen`, `OnboardingScreen` | R1-06 | R1-07, R2-14 |
| C11 | `importer::writer::{Writer::spawn, WriterHandle::{submit, finish}}`, `Checkpoint`, `WriterLimits` (test override of batch limits and free-space threshold), disk-guard error `AppError::FileAccess{reason:"insufficientSpace"}` | R2-10 | R2-09, R2-10 (loadgen) |
| C12 | `importer::remove::run(pst_id)` (removal job body) | R2-10 | R2-09 |
| C13 | `search::hydrate_rows(ids: &[i64]) -> Vec<MailRow>` (same hydrate SQL as `search_mail`, Part 04 §7.4) | R2-11 | R2-12 |
| C14 | Service facades called by `core::app` and the Command Layer: `Supervisor` (incl. `retry`), `SettingsStore` (R2-08); `ImportManager` incl. `recover_after_crash`, `shutdown` (R2-09); `SearchService` incl. `image(id)` (R2-11); `SqlConsole` incl. `cancel_running`, `Maintenance` incl. `current() -> Option<MaintenanceState>` (passed into `Supervisor::db_status`, R2-12); method names per Part 02 §3.1 "Interface" column | R2-08, R2-09, R2-11, R2-12 | R2-13 |
| C15 | `Supervisor::phase_watch(&self) -> tokio::sync::watch::Receiver<AppPhase>`; `ImportManager` takes the receiver at construction, pauses (`pause_for_db`) on a non-`Ready` phase and resumes from the checkpoint on `Ready` | R2-08 | R2-09, R2-13 (passes it in) |

### 2.3 Land order inside a round

- Round 1: R1-01 kickoff → (everyone else in parallel). R1-01 lands C1–C4 as its second commit. R1-03 lands C7 `init`/`start`/`stop` before R1-04 needs `TestCluster`; until then R1-04 writes migrations and unit tests. R1-06 lands C10 before R1-07's last commit.
- Round 2: R2-10 lands C11/C12 first; R2-11 lands C13 first; R2-08 lands C15 and `lifecycle/move_dir.rs` (`free_space`) first — R2-10's writer disk guard calls `free_space`, so R2-10 commits that step only after R2-08's commit is on main. R2-08 does not import `importer` or `sql_console`; `core::app` (R2-13) composes the start sequence (Part 02 §2.2 steps 8–9) and the exit sequence (§2.5). R2-13 lands last among Rust tasks. R2-14 writes and dry-runs its E2E specs against a mocked core early, then runs them for real after R2-13 lands.
- A consumer that needs a change to a frozen interface asks the leader; the producer changes it and re-runs every consumer's tests.

---

## 3. Round 1 tasks (7 implementers + 1 reviewer)

All `cargo` commands run in `src-tauri/`; all `npm` commands run at the repo root. Rust integration tests that need PostgreSQL require `node scripts/fetch-postgres.mjs` first and fail with "run scripts/fetch-postgres.mjs" when the binaries are missing.

### R1-01 — Kickoff skeleton, App Shell base, API contract, Work Gate
- **Scope:** Land the kickoff commit (§2.1). Then: Tauri app crate that opens one window with tauri-plugin-single-instance (registered first), dialog and log plugins, identifier `us.jhangy.outlooker`, CSP of Part 03 §5.6, capabilities with dialog `open`, empty command handler. All Part 03 DTOs in `core/src/dto/` with serde + ts-rs attributes exactly as Part 03 (plus `app_retry`, which takes and returns nothing), `COMMANDS` const (24 names), ts-rs export into `src/api/bindings/` plus `commands.json` / `events.json` writers. `sink.rs` (C2). Work Gate (C3) with the conflict matrix of Part 03 §7.2 / §8 and unit tests. `src/api/{call,commands,events}.ts` (C4) and Vitest drift tests that compare wrapper names with `commands.json` / `events.json`. `src-tauri/schemas/condition-group.json` and a core unit test that validates the serde output of the Part 03 §5.1 worked example against it with `jsonschema::validator_for` (draft 2020-12).
- **Owns:** see §5 rows marked R1-01.
- **Depends on:** spec only.
- **Acceptance:**
  - `cargo build --workspace --locked` exits 0 at the kickoff commit and at the final commit, on a checkout where `src-tauri/resources/` does not exist.
  - `rustc --version` run in `src-tauri/` prints `1.99.1`; `node --version` at the repo root (after `mise trust`) prints `v24.22.1`.
  - `grep -nE '"[\^~]' package.json` finds nothing; `cargo tree -p outlooker-core --depth 1` lists every Part 01 §5 / §7 / §8 crate at its Part 01 version (or a newer patch recorded in the commit message).
  - `cargo test -p outlooker-core dto::` and `cargo test -p outlooker-core gate::` exit 0; after `cargo test --workspace`, `git diff --exit-code src/api/bindings` exits 0.
  - `src/api/bindings/commands.json` contains exactly the 24 names (Part 03 §3 plus `app_retry`); `events.json` exactly the 6 names of Part 03 §9.
  - `grep -rE "crate::(lifecycle|importer|search|sql_console|maintenance|settings|db|gate|paths|app)" src-tauri/crates/outlooker-core/src/dto` finds nothing (leaf module).
  - `npm run typecheck` and `npm test -- src/api` exit 0.
  - `grep -rE "webDarkTheme|teamsDarkTheme|theme-one-dark|prefers-color-scheme: *dark" package.json src src-tauri/tauri.conf.json` finds nothing.
- **Spec:** Part 01 §1 (incl. toolchain), §2 (deps), §3, §8; Part 02 §2.6 layer 1, §3 (App Shell base, Work Gate, Typed API Client); Part 03 §1, §1.1, §1.2, all type blocks of §4–§9, §5.1 JSON schema.

### R1-02 — libpff vendoring, FFI crate, PST fixture tooling
- **Scope:** **Deadline: the vendored tree must be committed before 2026-11-18** (the `cibw-sdist` artifact of fork CI run `32393676269` expires `2026-11-18T16:44:21Z`; fallback after expiry: re-run the fork's `build_wheels` workflow via `workflow_dispatch`). `scripts/vendor-libpff.sh` downloads that artifact with `gh run download`, extracts it into `src-tauri/native/libpff/`, writes `ORIGIN` (commit `bb5bcc0` + run id); no autotools. `crates/pff/build.rs`: copies `native/libpff/` into `OUT_DIR`; on Unix runs `sh configure --disable-nls --disable-shared-libs --with-zlib=no` **in that copy** (the sdist ships a Linux `common/config.h` that a quoted include would otherwise pick up); on MSVC no configure, defines as `_build.py`; compiles `libpff/*.c` + the 17 local libs with `cc`; no zlib. Hand-written `extern "C"` for the ~45 functions; safe wrapper (C5) maps `-1` to `Result`, frees `libpff_error_t` and handles in `Drop`, is `!Send`. Never calls `libpff_set_codepage`. Commit `tests/fixtures/external/outlook.pst` (copy of the verified file `scripts/tmp/outlook.pst`, libyal testdata commit `c3631ff`) with `NOTICE` (CC-BY-4.0, attribution, source URL). `scripts/fixtures/make-pst.ps1` generates the 8 fixtures of Part 04 §12.1 and exits non-zero if Outlook rejects a property (run only in the Windows phase).
- **Owns:** see §5 rows marked R1-02.
- **Depends on:** R1-01 kickoff (workspace).
- **Acceptance:**
  - `grep -q bb5bcc0 src-tauri/native/libpff/ORIGIN` exits 0; `src-tauri/native/libpff/include/libpff.h`, `common/types.h` and `libpff/libpff_definitions.h` exist.
  - `cargo test -p pff` exits 0 on macOS; tests assert `pff::version() == "20260625"`, opening a non-PST file returns `Err`, and on `tests/fixtures/external/outlook.pst`: the folder list includes `Inbox` and `Sent Items`; two walks give identical `(folder path, item id)` lists; exactly two `IPM.Note` items (Inbox and `Recovered_Group2`), each with subject "Welcome to Microsoft Office Outlook 2003" and sender "Outlook 2003 Team" read through `Item::string`, an HTML body and attachment metadata.
  - `otool -L` of the pff test binary does not list `libz` (no hidden zlib dependency).
  - `grep -rn libpff_set_codepage src-tauri/crates` finds nothing.
  - A `compile_fail` doc test proves `pff::File` is not `Send` (`cargo test -p pff --doc` exits 0).
  - `tests/fixtures/external/NOTICE` contains `CC-BY-4.0` and `c3631ff`.
  - `pwsh -NoProfile -Command "[ScriptBlock]::Create((Get-Content -Raw scripts/fixtures/make-pst.ps1)) | Out-Null"` exits 0 (syntax check; PowerShell on macOS).
- **Spec:** Part 01 §4 (all, with the "configure in a copy of the tree" correction), §9 PST fixtures; Part 02 §3.1 `pff`; Part 04 §12.1.

### R1-03 — Bundled PostgreSQL runtime: fetch script, Paths, PG Controller
- **Scope:** `scripts/fetch-postgres.mjs`: download the theseus-rs 18.6.1 `.tar.gz` asset for the host (or given) target triple with `fetch`, verify sha256 with `node:crypto` against `src-tauri/native/postgresql.sha256` (6 triples), extract with the system `tar` (`child_process.execFileSync`; bsdtar on macOS, `tar.exe` on Windows 10+), prune (`include/`, `lib/bitcode/`, `lib/libLLVM*`, `lib/llvmjit*`, `lib/*.a`, `lib/pgxs`, `lib/Python`), write `src-tauri/resources/postgresql/`. `core::paths` per Part 02 §8. `core::lifecycle::pg` PG Controller: `initdb` with `-E UTF8 --locale-provider=libc --locale=<candidate>`, explicit `LC_ALL`, candidate lists per OS, per-candidate start + `CREATE EXTENSION pg_trgm` + CJK self-check, temp pwfile deleted; `postgresql.auto.conf` with Part 02 §2.3 step 5 values and Part 04 §6.6 values; `pg_hba.conf` single SCRAM line; port selection with ≤ 3 retries; `pg_ctl start/stop/status`; stale, orphan and PID-reuse `postmaster.pid` handling of Part 02 §2.2 step 4.
- **Owns:** see §5 rows marked R1-03.
- **Depends on:** R1-01 kickoff; C1 (`AppError`).
- **Acceptance:**
  - `node scripts/fetch-postgres.mjs` exits 0; `src-tauri/resources/postgresql/bin/postgres` and `lib/pg_trgm.*` exist; `lib/libLLVM*` does not; `node scripts/fetch-postgres.mjs --self-test` proves that a wrong hash makes the script exit non-zero.
  - `cargo test -p outlooker-core --test pg_lifecycle` exits 0 with tests: fresh init passes the CJK self-check and records the chosen locale; a `C`-locale candidate is rejected by the self-check; stop/start round trip; orphan postmaster stopped and restarted; stale `postmaster.pid` with a dead PID; `postmaster.pid` naming a live non-postgres PID (the test process's own PID) → the executable is checked, the file deleted, start succeeds; port already bound → retry succeeds; `SHOW` returns the Part 04 §6.6 values for `wal_level`, `max_wal_senders`, `plan_cache_mode`, `jit`, `shared_buffers` (formula) and `timezone`.
- **Spec:** Part 01 §3 (native paths), §6; Part 02 §1 (postgres row), §2.2 steps 4–6, §2.3 steps 4–6, §7.3, §8.1, §8.2; Part 04 §1, §6.6.

### R1-04 — Database schema, roles, views, pools, test cluster helper
- **Scope:** Refinery migrations embedded in the core: `V1__schema.sql` = Part 04 §2 DDL + §3 indexes; `V2__roles_views.sql` = roles `outlooker_app` / `outlooker_ro` with the grants of Part 04 §1 and §8.1, the six views of §8.2, `REVOKE CREATE ON SCHEMA public FROM PUBLIC`, `REVOKE TEMP`. `db::credentials` (random 32-byte passwords, file mode 0600 on Unix) and password setting after migration. `db::migrate`, `db::connect_admin`, `db::Pools` (deadpool, `outlooker_app` and `outlooker_ro`), `db::cjk_self_check`. `db::indexes`: the six search-index DDL constants identical to §3, `TSV_EXPR` (the generation expression of the stored column `messages.body_tsv`; the index and the compiler use the column), `drop_search_indexes`, idempotent `ensure_search_indexes` on an admin connection (with `maintenance_work_mem` and parallel workers of §6.4, progress from `pg_stat_progress_create_index`). `test_support::TestCluster` (temp dir, production initdb flags via C7, migrated, both pools, admin connection).
- **Owns:** see §5 rows marked R1-04.
- **Depends on:** R1-01 kickoff; C6, C7 from R1-03 (for `TestCluster`).
- **Acceptance:**
  - `cargo test -p outlooker-core --features test-support --test db_schema` exits 0 with tests: migrations apply on a fresh cluster; `show_trgm('郵件系統')` contains `郵件系`; one rejecting row per CHECK / UNIQUE / FK of §2 (e.g. second running job → SQLSTATE `23505`, occurrence folder from another PST → `23503`); `outlooker_ro` can `SELECT` each view and gets `42501` on `SELECT * FROM messages`; `outlooker_app` can `CREATE TEMP TABLE` and `VACUUM (ANALYZE) messages`, and gets `42501` on `CREATE INDEX` on `messages`; `outlooker_admin` owns every table; `drop_search_indexes` then `ensure_search_indexes` leaves the six indexes valid (`pg_index.indisvalid`), and a second `ensure_search_indexes` is a no-op.
  - A unit test asserts `SEARCH_INDEXES` has exactly the six names of Part 04 §6.4 and that each DDL string (incl. `messages_body_fts ON messages USING gin (body_tsv)`) and the `body_tsv` generation expression `TSV_EXPR` occur in `V1__schema.sql`.
- **Spec:** Part 01 §7, §9 (test helper); Part 02 §2.3 step 3 (credentials), §3.1 Db, §7.2 (role and grants), §7.3; Part 04 §1, §2, §3, §6.4 (DDL routines), §8.1 (roles), §8.2, §12.2 (migration + constraint rows).

### R1-05 — Mail normalization: charset, RTF de-encapsulation, sanitizer, search text, WebP, dedupe key
- **Scope:** `importer::types` (C9: `RawItem` as produced by the reader per Part 04 §5.1/§5.4/§5.8, `NormalizedItem` as consumed by the writer). `normalize/`: body selection of Part 01 §5 / Part 04 §5.5 (HTML → RTF `\fromhtml1` de-encapsulation → plain text), charset decode with encoding_rs and chardetng, in-house MS-OXRTFEX tokenizer (~200 lines), ammonia policy of Part 04 §5.6 and Part 02 §7.1 layer 1 (cid → `outlooker-inline:<ordinal>`, remote `img src` removed, CSS `url()` removed), `body_text` via html2text, snippet (200 chars, whitespace collapsed), `sender_search` / `recipients_search` / `to_preview` per §5.4. `webp.rs`: BLAKE3 source hash, known-hash skip, decode with `image`, size limits and resize per §5.7, lossy encode with `webp` at a quality value, undecodable image → error result, message kept. `dedupe.rs`: Part 04 §4.1 key. All work is pure Rust (no DB) and panic-safe per item (`catch_unwind` wrapper).
- **Owns:** see §5 rows marked R1-05.
- **Depends on:** R1-01 kickoff (deps in `core/Cargo.toml`).
- **Acceptance:**
  - `cargo test -p outlooker-core importer::` exits 0, including: RTF de-encapsulation of a `\fromhtml1` sample equals its golden HTML; Big5 and GBK bytes decode to the expected Unicode; the hostile corpus (`<script>`, `onerror=`, `javascript:`, CSS `url(`, `<iframe`, `<form`, `<base`, `<meta http-equiv`, `position:fixed`, `<img src="https://…">`) produces output where `grep -E '<script|on[a-z]+=|javascript:|url\(|<iframe|<form|position|src="http'` finds nothing; golden-file comparison for the plain and inline samples; 1×1 pixel image → `<img>` without `src`; the same image bytes give one BLAKE3 hash and the second occurrence skips encoding; an image larger than 2,048 px is resized to 2,048 px; images over the 40 MP / 25 MB limits are rejected; dedupe key table tests (brackets, whitespace, CRLF vs LF equal; one changed character differs).
  - Invariants test over every golden sample: `snippet` ≤ 200 chars; every stored email equals its lowercase form; `body_html` is `None` exactly for body kinds `text` / `rtf_text` / `none`; dedupe keys are 32 bytes (the R1-04 CHECK constraints).
- **Spec:** Part 01 §5; Part 02 §3.1 normalize / webp / dedupe, §6.1 rows "undecodable charset", "bad inline image", "panic in a rayon task", §7.1 layer 1; Part 04 §4.1, §5.4–§5.8, §12.2 rows Normalization, Sanitizer, Images.

### R1-06 — Frontend: App Frame, shared UI, Onboarding, Import, Settings screens
- **Scope:** `src/app/`: one `FluentProvider theme={webLightTheme}`, i18next init with namespaces and zh-TW / en (language from `app_status`, switch via `settings_update` + `changeLanguage`), 48 px TitleBar, NavRail, screen router, boot sequence (subscribe all events → `app_status` → `pst_list`), `AppStatus` context (phase from `app:status-changed`, counts from `library:changed`), blocking screens for `InitializingCluster` / `StartingDb` / `StoppingDb`, Failed screen with details disclosure and a Retry button that calls `api.appRetry()`, Toaster. `components/common` (C10). Onboarding wizard (language → data location via dialog plugin → `app_initialize` progress → "import your first PST"). Import screen (imported PST DataGrid, queue cards with ProgressBar and phase, Cancel / Resume / Retry / Remove, removal Dialog that states the PST name and its `messageCount` and that messages also found in other PST files are kept, error log via `import_errors` paging, live `import:progress` / `import:status-changed`). Settings screen (data location + move dialog fed by `data-dir:move-progress`, language RadioGroup, WebP quality Slider + SpinButton, index maintenance with DataGrid from `db_status` and `maintenance_run`, DB status with `databaseSizeBytes`). Empty states for Import and Settings. Visual reference: `docs/design/variant-1/` (`#import`, `#settings`, `#onboarding`, `:empty`) **minus the out-of-scope controls of Global Constraints** (no pause / pause all, no worker-count setting, no re-encode, no usage breakdown, no exact delete/keep preview, no drag-and-drop).
- **Owns:** see §5 rows marked R1-06.
- **Depends on:** C1 bindings and C4 from R1-01 (kickoff stubs until they land).
- **Acceptance:**
  - `npm run typecheck`, `npm test -- src/app src/components/common src/screens/onboarding src/screens/import src/screens/settings` exit 0 with a mocked `src/api/commands` (vi.mock), including: boot subscribes to every event before calling `app_status`; after a mocked `library:changed` with `messages > 0`, `useAppStatus().counts.messages` updates; Failed screen Retry calls `appRetry`; each screen renders its empty state; removal dialog shows `messageCount` and calls `pstRemove` only after confirm; WebP quality outside 1–100 is not sent.
  - Locale parity test: for every namespace directory, `zh-TW` and `en` key sets are equal.
  - No-literal test: each screen rendered with i18next language `cimode` shows only translation keys (no text node outside the key pattern).
  - No-dark test: no source file under `src/` contains `webDarkTheme`, `teamsDarkTheme` or `prefers-color-scheme`.
  - Out-of-scope-controls test: no locale namespace contains a key for pause, pause-all, workers, re-encode, format, or highlight (key-name grep list fixed in the test), and the Import and Settings screens render no element with those labels.
- **Spec:** requirements screens 3, 4, 5; Part 01 §2 (state rule, i18n); Part 02 §3.1 App Frame, Screens, §6.1 ("Fatal screen with details and retry"); Part 03 §1 Localization, §1.2 (error mapping), §2, §4 (incl. `app_retry`), §7, §8, §9 boot sequence; `docs/design/variant-1/NOTES.md`.

### R1-07 — Frontend: mail components, Search screen, Advanced SQL screen
- **Scope:** `components/mail`: `MailList` with `useInfiniteQuery` + `useVirtualizer`, fixed 32 px rows, loader row, next page when within 50 rows of the end, hard stop at 100,000 rows with the `search.resultCap` row, month headers, selected-row brand bar; `ReadingPane` (Persona, To/Cc, occurrence Badges, attachment Cards with disabled download Button + Tooltip, plain-text fallback when `bodyHtml` is null); `MailPreview` as `<iframe sandbox="" srcdoc referrerpolicy="no-referrer">` that replaces `srcdoc` on change. `components/dsl`: builder producing `ConditionGroup` (all node types of Part 03 §5.1, AND/OR toggles, nested groups, depth ≤ 5, ≤ 50 leaves, folder tree and PST picker from `filter_options`, local-day → RFC 3339 instant conversion). `components/sql-editor`: in-house CodeMirror 6 wrapper with `sql({ dialect: PostgreSQL, schema })` and a static schema of the six views. Search screen: refine pane, SearchBox with 300 ms debounce and sequence tagging, `searchCount` shown as a locale-formatted number or "10,000+", short-query MessageBar, "new mail imported — refresh" bar and `filter_options` refetch on `library:changed`, empty and zero-result states. Advanced SQL screen: CSS grid `1fr 3fr 1fr 3fr`, saved query CRUD, Run / Cancel with a UUID `executionId` and `maxRows: 10000`, "first 10,000 rows" MessageBar when `truncated`, MailList mode vs virtualized generic table (numeric right-aligned), table mode: clicking a row shows its column/value list in column 4, `sqlError.position` marker, read-only Badge, empty states. Visual reference: `docs/design/variant-1/` (`#search`, `#sql`, `#sql:table`, `:empty`, `:short`, `:zero`) **minus the out-of-scope controls of Global Constraints** (no SQL Format button, no search-term highlighting).
- **Owns:** see §5 rows marked R1-07.
- **Depends on:** C1 bindings and C4 from R1-01; C10 from R1-06 (kickoff stubs until it lands).
- **Acceptance:**
  - `npm run typecheck`, `npm test -- src/components/mail src/components/dsl src/components/sql-editor src/screens/search src/screens/advanced-sql` exit 0 with mocked commands, including: the preview iframe has `sandbox=""` and no `allow-` token; with 600 mock pages of 200 rows the list stops requesting after 100,000 rows and shows the cap row; mocked `searchCount` `{count: 10000, capped: true}` renders "10,000+" and `{count: 9999, capped: false}` renders "9,999"; DSL builder output for the worked example of Part 03 §5.1 deep-equals the JSON in the spec; a stale search response (older sequence) is dropped; `library:changed` triggers a `filterOptions` refetch and not a list reload; `sqlExecute` is called with `maxRows: 10000`; `truncated: true` shows the cap MessageBar; Advanced SQL shows MailList for a result with an `id` column and a table otherwise; clicking a table row shows its key/value list in column 4; Run is disabled while executing.
  - The locale-parity, no-literal, no-dark and out-of-scope-controls tests of R1-06 pass for the `search` and `sql` namespaces (the tests live in R1-06's `src/app/` and iterate all namespaces).
- **Spec:** requirements screens 1, 2, 5; Part 01 §2 (CodeMirror decision, preview rendering, virtualized lists); Part 02 §3.1 Mail Preview Frame, §7.1 layer 2; Part 03 §2, §5.1–§5.6 (frontend side), §6, §9 (`library:changed` behavior); `docs/design/variant-1/NOTES.md` Advanced SQL row (column 4 row detail).

### R1-REV — Round 1 reviewer
- **Scope:** Review each R1 task's commits as they land (incremental, not at round end) against: the task's Spec sections, Global Constraints, §5 ownership (`git show --stat` paths ⊆ the task's owned paths), pinned versions vs Part 01, and the acceptance commands. Write findings to `docs/logs/review-round1.md` as blocker / should-fix / nit with `file:line`. Blockers go to the leader for routing to the owner; at most two review passes per task; should-fix and nit go to the parking lot.
- **Owns:** `docs/logs/review-round1.md` only. Edits no code.
- **Acceptance:** `docs/logs/review-round1.md` has one section per task R1-01 … R1-07 with verdict `PASS` or `BLOCKED` and the commit hashes reviewed; no `BLOCKED` remains at round close.
- **Spec:** all sections listed by R1-01 … R1-07; Part 02 §7 security for R1-04, R1-05, R1-07.

---

## 4. Round 2 tasks (7 implementers + 1 reviewer)

Round 2 starts only after the Round 1 macOS gate (§7.1) passes. Round 2 closes on the macOS gate (§7.2) with the **minimal fixture set**: `tests/fixtures/external/outlook.pst`, a byte copy and a corrupted copy generated by the tests, synthetic `RawItem` pipeline tests, and an E2E library seeded by `outlooker-loadgen`. The 8 Outlook-generated fixtures are produced in the Windows phase (§7.3); until then their tests (feature `outlook-fixtures`) are a recorded known limitation of Round 2 and a release blocker.

### R2-08 — Lifecycle Supervisor and Settings Store
- **Scope:** `lifecycle::supervisor` owns `AppPhase` and its transitions (Part 02 §2.1), emits `app:status-changed`, publishes C15 `phase_watch`, returns `dbNotReady` unless `Ready`. `start` = Part 02 §2.2 steps 2–7 (bootstrap file, marker and `PG_VERSION` checks, stale-pid recovery, port, health check, migrations, CJK self-check, pools). `init` = §2.3 (validate target, adopt an existing data root, write settings, marker, credentials, initdb via C7, cleanup on failure). `retry` (`app_retry`): only when `phase == Failed` (else `invalidInput{field:"phase", reason:"notFailed"}`); re-runs `start` when a data root exists, or returns to `NeedsSetup` when the failed first-run attempt deleted its pgdata; resets the restart budget. `health` = §2.4 (5 s probe, 2 failures → restart, budget 3 restarts / 10 min, probe interval configurable for tests). `shutdown_db` = §2.5 steps 3–4. `move_dir` = §8.3 / Part 03 §8.2 with `data-dir:move-progress` and a test hook that fails the verify step. `status` = `db_status` SQL of Part 04 §9.2. `settings::SettingsStore`: bootstrap `settings.json` (atomic temp + rename) merged with the DB `settings` row; `settings_update` with only `language` works before `Ready`; `webp_quality` patch before `Ready` → `dbNotReady`. Does not import `importer` or `sql_console` (§2.3).
- **Owns:** see §6 rows marked R2-08.
- **Depends on:** C1–C3, C6–C8.
- **Acceptance:** `cargo test -p outlooker-core --features test-support --test lifecycle` exits 0 with tests: first run → `NeedsSetup → InitializingCluster → StartingDb → Ready` recorded by `RecordingSink` and mirrored on `phase_watch`; adopt an existing data root; `PG_VERSION` ≠ 18 → `Failed`; init failure deletes only the pgdata it created; `Failed` → `app_retry` → `Ready`; `app_retry` while `Ready` → `invalidInput{field:"phase"}`; postmaster killed with `pg_ctl stop -m immediate` → phase returns to `Ready` after restart; a 4th crash inside the 10-minute window (100 ms probe interval in the test) → `Failed` with the last PG log line in `detail`; data dir move to an empty temp dir succeeds and the old root is deleted; verify step forced to fail → phase returns to `Ready` on the old root, the old root is unchanged (file count + bytes), the target is removed, and `data-dir:move-progress` ends with `failed`; move into a non-empty dir → `invalidInput{field:"target"}`; move while the gate holds `Import` → `busy`; `settings.json` survives a simulated crash between temp write and rename (old or new content, never partial).
- **Spec:** Part 02 §2.1–§2.4, §2.5 steps 3–4, §3.1 Lifecycle Supervisor / PG Controller / Settings Store, §6.1 rows "PostgreSQL fails to start", "CJK self-check fails", "Data dir move fails", §8.3; Part 03 §4 (incl. `app_retry`), §8.1, §8.2, §8.4; Part 04 §9.2.

### R2-09 — Import Manager: queue, PST reader, pipeline orchestration
- **Scope:** `importer::ImportManager` serving `pst_list` (status derivation of Part 04 §2.1), `import_enqueue` (path checks, `!BDN` magic, `alreadyImported`), `import_cancel`, `import_resume`, `pst_remove` (cancels a queued import job of that PST and queues the removal job in one transaction; body = C12), `import_errors` (keyset), `recover_after_crash`, `pause_for_db` driven by C15, `shutdown`. `queue`: durable `import_jobs` state machine (Part 03 §7.1), one job at a time, Work Gate `Import` / `Removal`, file size + mtime check (changed → checkpoint reset), index policy of Part 04 §6.4 (deferred when remaining items ≥ 50% of `messages`, else incremental) calling C8 on an admin connection, rebuild at the end of every job that dropped indexes (completed, cancelled or failed), after-job `VACUUM (ANALYZE)` (§6.5); `webp_quality` read from the DB `settings` row at job start through the C8 app pool (no `importer → settings` import). `pst_reader`: reader `std::thread` owning the `pff::File`, folder scan (§5.2) → `items_total`, deterministic depth-first order, `seq` numbering, skip `seq ≤ cursor`, message-class filter (§5.3), property / body / recipient / attachment / inline-image extraction into `RawItem`, `import-inflight` marker with its attempts counter (Part 02 §6.3). Pipeline wiring: bounded channels of 256 + 256 MB byte budget, rayon stages from R1-05, results into C11 writer, cancel flag, maps the C11 disk-guard error to job `Failed` (stage `store`), `import:progress` (throughput / ETA in `progress.rs`), `import:status-changed`, `library:changed`. `examples/import_bench.rs`. A test entry point feeds synthetic `RawItem`s into the pipeline after the reader.
- **Owns:** see §6 rows marked R2-09.
- **Depends on:** C1–C3, C5, C6, C8, C9; C11 and C12 from R2-10 (land first); C15 from R2-08 (lands first).
- **Acceptance:** `cargo test -p outlooker-core --features test-support --test import_pipeline` exits 0 with tests:
  - On `outlook.pst`: folder paths; the Welcome mail (stored twice: Inbox and `Recovered_Group2`) gives 1 message and 2 occurrences, with HTML body and attachment metadata; two scans give identical `(seq, pst_item_id)` lists.
  - `outlook.pst` + its byte copy (no Message-ID) → 1 message, 4 occurrences (fallback-hash cross-PST dedupe); removing one keeps the message, removing both deletes it and its orphans.
  - Corrupted copy (one data block zeroed by the test) → the outcome pre-registered in the test (`item_skipped` and job `Completed`, or `job_failed` with stage `scan`) is the one observed.
  - Synthetic `RawItem` tests from the reader boundary on: CJK (Big5 + UTF-8 HTML) stored text equals expected Unicode; cid inline images → `outlooker-inline:n` rows in `inline_images`, a repeated logo stored once; attachment edge cases (embedded message, missing MIME tag, long file name); hostile HTML stored output passes the R1-05 grep; Message-ID dedupe across two synthetic sources (each shared Message-ID stored once with one occurrence per source; a mail copied into a second folder of one source adds one occurrence).
  - Index policy: first import into an empty library drops the six search indexes and after `Completed` all six exist and are valid; the same after a job that is cancelled and after one that fails through the free-space override; an import whose remaining items are < 50% of `messages` issues no `DROP INDEX` (statement log); after a job, `pg_stat_user_tables.last_vacuum` (or `last_autovacuum`) for `messages` is set.
  - Resume: cancel then resume gives the same `dedupe_key` set and counts as one uninterrupted run; kill after batch 1 (test hook) then resume gives identical rows; touching the PST mtime between cancel and resume resets the cursor and the final counts stay correct; poison item (attempts rule, Part 02 §6.3): an inflight marker for this job with `attempts = 1` → the item is retried and imported, no error row; a marker with `attempts >= 2` → the item is skipped with exactly one `parse` error row; a marker of another job is ignored.
  - PG crash: during an import, `pg_ctl stop -m immediate` then restart via C7 with the test driving the C15 watch (`StartingDb` → `Ready`) → the job ends `Completed`, `dedupe_key` set and counts equal one uninterrupted run, no duplicate occurrence rows.
  - `recover_after_crash`: rows left `Running` / `Cancelling` become `Queued` / `Cancelled`; the `Queued` one resumes from `cursor_seq`.
  - `pst_list` status derivation: a table test over every job kind/state row of Part 04 §2.1, including `Removing`; `pst_remove` on a PST with a queued (not started) import job → no `23505`, the PST is gone.
  - `cargo build -p outlooker-core --example import_bench` exits 0.
  - With feature `outlook-fixtures` (Windows phase, §7.3): the Part 04 §12.1 fixture assertions (`basic`, `cjk`, `inline`, `attach`, `dup-a` + `dup-b` → `messages` = 50 and `message_occurrences` = 81, `hostile`, `corrupt`).
- **Spec:** Part 02 §2.2 step 8, §2.5 step 1, §3.1 Import Manager (queue, pst_reader), §4 steps 1–5 and 8, §6.1 (import rows, PG crash row), §6.2, §6.3, §10 Fault injection; Part 03 §7; Part 04 §2.1, §5.1–§5.3, §5.9, §6.4 (policy), §6.5, §12.2 rows Dedupe, Resume, Deterministic order.

### R2-10 — Bulk writer, PST removal, 1M load generator, perf harness
- **Scope:** `importer::writer` (C11): single tokio task with its own connection (`synchronous_commit = off` on that connection only), reorder buffer by `seq`, batch at 1,000 items / 64 MB / 1 s (`WriterLimits` overridable in tests), one transaction per batch with binary COPY into staging, set-based upsert and merge of Part 04 §4.2 / §6.1–§6.3, `import_errors` rows, checkpoint row; the disk guard (free space on the data-root volume before each batch commit, 2 GiB, Part 04 §5.1) is owned here and returns the C11 error. `importer::remove` (C12): Part 04 §4.3 set-based removal incl. orphan blobs / addresses, `first_pst_id` NULL on survivors, idempotent after crash, then `VACUUM (ANALYZE)`. Owner of `db/**` and `test_support/**` in Round 2 (schema fixes requested by other tasks go here). `crates/outlooker-loadgen`: generator of Part 04 §12.3 (1M messages, 10 sources, duplicate rates, EN / ZH / mixed corpora, image pool) feeding the real writer; `seed --data-root <dir> --messages <n>` builds a complete data root (marker per Part 02 §8.2, credentials, pgdata via C6–C8) for the R2-14 E2E; `#[ignore]` `perf_` tests, one per gate row of Part 04 §12.3 and one per Part 02 §9 row that §12.3 lacks (next page p95 ≤ 100 ms, `mail_get` + first `mailimg` image p95 ≤ 100 ms, first page p50 ≤ 150 ms), writing `docs/logs/loadtest-<date>.md` / `perf-<date>.md` with the commit hash; each gate is a separate test so each run fits the foreground time limit. The binding query set is Part 04 §12.3's 30 canned queries.
- **Owns:** see §6 rows marked R2-10.
- **Depends on:** C1, C2, C6–C9.
- **Acceptance:**
  - `cargo test -p outlooker-core --features test-support --test writer --test removal` exits 0 with tests: items submitted out of `seq` order are stored with the checkpoint at the highest contiguous `seq`; a duplicate Message-ID from a second source adds one occurrence and no message; two sources whose items have no Message-ID and equal sender, time, subject and body (one CRLF, one LF) → 1 message, 2 occurrences; batch limits cut a batch at 1,000 items, at the byte limit and at the time limit (overridden values); `SHOW synchronous_commit` is `off` on the writer connection and `on` on an app-pool connection; a forced error mid-batch rolls back data and checkpoint together; free space below the overridden threshold → the C11 disk-guard error and nothing committed; removal keeps shared messages, deletes non-shared ones and orphan blobs / addresses, and a second run is a no-op.
  - `cargo test -p outlooker-loadgen` (non-ignored smoke: 10,000 messages) exits 0 and prints row counts within the configured duplicate rates; `cargo run -p outlooker-loadgen -- seed --data-root <tmp> --messages 2000` exits 0 and the data root starts with C7.
  - `cargo test -p outlooker-loadgen --release -- --ignored --list` lists one `perf_` test per gate row of Part 04 §12.3 plus the three Part 02 §9 rows above.
- **Spec:** Part 02 §3.1 writer / remove, §4 steps 6–7, §6.1 rows disk space / PG crash, §9; Part 04 §4.2, §4.3, §5.1 (disk guard), §6.1–§6.3, §6.7, §12.2 row Removal, §12.3.

### R2-11 — Search Service
- **Scope:** `search::compile`: simple query → `and(or(sender, recipient, subject, body))` per term (≤ 10 terms), DSL tree → parameterized predicate exactly as Part 04 §7.3 (escape `%`, `_`, `\`; ASCII-letter rule for the tsvector branch on the stored column `m.body_tsv`; `kinds` subset recheck; folder exact / prefix; `sourcePst`), limits (depth ≤ 5, ≤ 50 leaves, value 1..=500 after trim) → `invalidInput{field:<json path>}`, short-query detection by Unicode scalar count. `search::cursor`: base64url JSON `(sort_at, id)` with `-infinity` as `null`. `search::execute`: two-phase execution of §7.4 with window W as a parameter (production constant 5,000, spec Part 04 §7.4), `RESET` of planner settings after phase B, `page_size + 1`, `statement_timeout` 10 s / 20 s, capped count (`LIMIT 10001`), `filter_options`, `hydrate_rows` (C13). `search::mail`: `mail_get` with `outlooker-inline:<ordinal>` → `mailimg://localhost/<id>` (macOS / Linux) or `http://mailimg.localhost/<id>` (Windows). `search::image`: `image(id) -> Option<Bytes>`. Owner of `condition-group.json` in Round 2.
- **Owns:** see §6 rows marked R2-11.
- **Depends on:** C1, C8.
- **Acceptance:** `cargo test -p outlooker-core --features test-support --test search` and `cargo test -p outlooker-core search::` exit 0 with tests:
  - Compiler: for each condition type the SQL text contains no user value and the parameters equal the escaped value; `%`, `_`, `\` match literally.
  - Semantics on seeded rows: `報價` → `shortQuery: true`, `報價單` matches, `invoice` matches `invoices`, partial sender and recipient match, the Part 03 §5.1 worked example returns the expected ids; count of 10,001+ matches → `{count: 10000, capped: true}`; `mail_get` of unknown id → `notFound{entity:"mail"}`; image URLs use the platform form.
  - Two-phase (test W = 20): a dense term runs phase A only and the page equals a reference query `ORDER BY sort_at DESC, id DESC LIMIT n`; a sparse term runs phase B and the merged page equals the same reference; pages 1–3 across a phase-A/phase-B boundary have no gap and no overlap; rows with NULL `sent_at` come last and page correctly; after phase B, `SHOW enable_indexscan` on the same connection returns `on`.
  - Index use (after `ANALYZE`, `enable_seqscan = off` in the test): `EXPLAIN` of `ILIKE '%報價單%'` contains `messages_body_trgm`; an English body term contains `messages_body_fts`; sender contains `messages_sender_trgm`; recipient contains `messages_recipients_trgm`.
- **Spec:** Part 02 §3.1 Search Service, §5 (search, preview); Part 03 §5.1–§5.6 (core side); Part 04 §3 (shared tsvector expression), §7.1–§7.6, §12.2 rows Search compiler, Search semantics.

### R2-12 — SQL Console Service and Maintenance Service
- **Scope:** `sql_console`: saved query CRUD on the app pool (name 1..=200, sql 1..=100,000, duplicate name → `invalidInput{field:"name", reason:"duplicate"}`); `execute` on the `outlooker_ro` pool per Part 04 §8.1 (`max_rows` default 1,000, max 10,000; `BEGIN READ ONLY`, `SET LOCAL statement_timeout = '30s'`, single-statement check, mode detection from prepared column metadata, `DECLARE … CURSOR` + `FETCH FORWARD max_rows + 1`, text cells cut at 2,000 chars, `ROLLBACK`, `DISCARD ALL`), MailList hydration through C13, one running execution at a time, `execution_id → CancelToken` map, `cancel` idempotent, `cancel_running` for shutdown, SQLSTATE mapping (`25006` / `42501` → `readOnlyViolation`, `57014` → `cancelled` or `queryTimeout`). `maintenance`: `analyze`, `vacuum` (+ orphan cleanup), `reindex` (ensure + drop `_ccnew*` leftovers + `REINDEX INDEX CONCURRENTLY` one at a time) per Part 04 §9.1 on an admin connection, Work Gate `Maintenance`, `maintenance:status-changed`.
- **Owns:** see §6 rows marked R2-12.
- **Depends on:** C1–C3, C8; C13 from R2-11 (lands first).
- **Acceptance:** `cargo test -p outlooker-core --features test-support --test sql_console --test maintenance` exits 0 with tests: every bypass of Part 02 §10 fails (`INSERT/UPDATE/DELETE`, `SET TRANSACTION READ WRITE` then a write, `SET ROLE outlooker_app`, `CREATE TEMP TABLE`, `COPY … TO 'copy-probe.txt'`, `pg_read_file`, `DO $$…$$` with a write, `nextval`, `CREATE FUNCTION`, `ALTER ROLE`); Part 04 §12.2 row Read-only role (`DELETE FROM v_mail` → `readOnlyViolation`, `SELECT * FROM messages` → `sqlError 42501`, two statements → `invalidInput multipleStatements`, `SELECT pg_sleep(40)` → `queryTimeout`); `sql_cancel` during `pg_sleep(10)` → `cancelled`; `SELECT id FROM v_mail` → `MailList`, `SELECT 1 AS x` → `Table`; `maxRows` 10,001 → `invalidInput{field:"maxRows"}`; omitted `maxRows` → at most 1,000 rows; `max_rows` + 1 rows → `truncated: true`; maintenance refused while the gate holds `Import`; `vacuum` removes one seeded orphan blob and one seeded orphan address; `reindex` leaves every index valid.
- **Spec:** Part 02 §2.5 step 2, §3.1 SQL Console / Maintenance, §5 (Advanced SQL), §6.1 rows Advanced SQL, §7.2; Part 03 §6, §8.3; Part 04 §8.1, §9.1.

### R2-13 — App composition, Tauri App Shell, Command Layer, Event Emitter, `mailimg` scheme, release build
- **Scope:** `core::app`: `start_services` (Supervisor start → on `Ready`: `ImportManager::recover_after_crash`, then `ensure_search_indexes` in the background under gate `Import`, then the queue worker; passes C15 into `ImportManager`) and `shutdown` (Part 02 §2.5 order: importer shutdown ≤ 15 s with job status `Queued` → `SqlConsole::cancel_running` → close pools → `pg_ctl stop -m fast -w -t 30`, then `-m immediate` on timeout). `src-tauri/src/lib.rs`: plugin order (single-instance first), managed state, calls `core::app`, exit handler on `RunEvent::ExitRequested` with `prevent_exit()`, `register_asynchronous_uri_scheme_protocol("mailimg", …)`. `commands/*.rs`: one thin `#[tauri::command]` per command (24, names identical to `COMMANDS`). `events.rs`: Tauri `EventSink` with per-key throttles (`import:progress` 250 ms per PST plus immediate emit on each phase change, `library:changed` 5 s, `data-dir:move-progress` 250 ms) and trailing flush. `mailimg.rs`: GET only, integer id, `200 image/webp` + `Cache-Control: max-age=31536000, immutable`, `400` / `404` / `405`. Final CSP; adds `bundle.resources` (`resources/postgresql`); cargo feature `e2e` adds `tauri-plugin-wdio-webdriver`, never in release. Owner of `dto/**`, bindings, `lib.rs`, toolchain and Cargo manifests in Round 2.
- **Owns:** see §6 rows marked R2-13.
- **Depends on:** C1–C15 (lands last among Rust tasks).
- **Acceptance:**
  - `cargo test -p outlooker-core --features test-support --test app_compose` exits 0 with tests: `shutdown` during a running import → job status `Queued` (not `Cancelled`), checkpoint committed, `pg_ctl status` reports no server, within the 15 s + 30 s budgets; `start_services` after a simulated kill → `Running` jobs become `Queued`, `Cancelling` become `Cancelled`, then `ensure_search_indexes`, then the queue starts (order recorded by `RecordingSink`); start with one search index missing → after `Ready` it exists and is valid, and the queue worker started only after that.
  - `cargo test -p outlooker` exits 0 with tests: the `generate_handler!` list equals `COMMANDS`; the single-instance plugin is the first `.plugin(` call in `lib.rs` (source check); throttle emits ≤ 4 events per second per key, always emits the trailing value, and emits a phase change immediately inside the 250 ms window; `mailimg` handler returns 405 for POST, 400 for `abc`, 404 for an unknown id, 200 with `image/webp` for a seeded id.
  - `cargo build --release -p outlooker --locked` exits 0, and `cargo tree -p outlooker -e normal | grep -c wdio` prints `0` (default features exclude `e2e`; judge the printed count — grep exits 1 when the count is 0).
  - `npm run tauri build` exits 0 on macOS and produces a `.dmg`; its size is recorded in the R2 gate log.
- **Spec:** Part 01 §3; Part 02 §1, §2.2 steps 1, 8–9, §2.5, §2.6 layer 1, §3.1 App Shell / Command Layer / Event Emitter, §7.1 layer 3; Part 03 §1, §3, §5.6 (scheme, CSP), §9; Part 04 §10 (cross-check).

### R2-14 — Frontend integration and E2E
- **Scope:** Owner of all frontend files in Round 2. Write and dry-run the E2E specs against a mocked core early; after R2-13 lands, run the app against the real core, fix frontend defects found. WebdriverIO E2E with `@wdio/tauri-service` `driverProvider: 'embedded'` (Part 02 §10): (1) onboarding → import `tests/fixtures/external/outlook.pst` → search English ("Welcome") → preview → Advanced SQL mail-list mode → remove PST; (2) on a library seeded with `outlooker-loadgen seed`: Chinese search and the inline-image preview through `mailimg`; (3) the scroll test that loads 100,000 rows in the real webview (WKWebView on macOS) and reaches the cap row.
- **Owns:** see §6 rows marked R2-14.
- **Depends on:** R2-13 landed (real E2E runs only after it); R2-10 `seed`.
- **Acceptance:**
  - `npm run typecheck && npm test && npm run build` exit 0.
  - `npm run e2e` exits 0 on macOS with the three E2E groups above.
- **Spec:** requirements screens 1–5 (end-to-end); Part 01 §9 E2E; Part 02 §10 rows E2E, Per-platform release acceptance (macOS part); Part 03 §5.2 virtualized list cap, §5.6 (image scheme in the webview).

### R2-REV — Round 2 reviewer
- **Scope:** As R1-REV for R2-08 … R2-14, plus a security pass over Part 02 §7 and Part 04 §11 (SQL built only from constant fragments; read-only boundary; sanitizer + sandbox + CSP; path re-validation) and a regression pass: re-run the Round 1 gate commands on the Round 2 head.
- **Owns:** `docs/logs/review-round2.md` only. Edits no code.
- **Acceptance:** `docs/logs/review-round2.md` has one section per task R2-08 … R2-14 plus "Security" and "Round 1 regression", each `PASS` or `BLOCKED` with commit hashes; no `BLOCKED` remains at round close.
- **Spec:** all sections listed by R2-08 … R2-14; Part 02 §7; Part 04 §11.

---

## 5. Round 1 ownership table

Each path (file or directory with everything below it) has exactly one owner in Round 1. No path appears twice. A directory row covers its subtree except paths listed in another row. Reviewer R1-REV owns only its log. Files marked "kickoff" in §1 are created by R1-01 in the kickoff commit and then left untouched in Round 1; they appear here under R1-01 because no other R1 task edits them.

| Owner | Paths |
|---|---|
| R1-01 | `mise.toml`, `package.json`, `package-lock.json`, `vite.config.ts`, `tsconfig.json`, `index.html`, `.gitignore`, `src/main.tsx`, `src/test/setup.ts`, `src/api/**`; `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, `src-tauri/rust-toolchain.toml`, `src-tauri/build.rs`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/**`, `src-tauri/icons/**`, `src-tauri/.cargo/config.toml`, `src-tauri/src/**`, `src-tauri/schemas/**`; `core/Cargo.toml`, `core/src/lib.rs`, `core/src/dto/**`, `core/src/sink.rs`, `core/src/gate/**`; kickoff-only: `core/src/app/**`, `core/src/lifecycle/{supervisor,init,start,health,move_dir,status}.rs`, `core/src/settings/**`, `core/src/importer/{queue,pst_reader,inflight,progress,writer,remove}.rs`, `core/src/search/**`, `core/src/sql_console/**`, `core/src/maintenance/**`, `crates/outlooker-loadgen/**` |
| R1-02 | `crates/pff/**` (after the kickoff `Cargo.toml`), `src-tauri/native/libpff/**`, `scripts/vendor-libpff.sh`, `scripts/fixtures/**`, `tests/fixtures/**` |
| R1-03 | `scripts/fetch-postgres.mjs`, `src-tauri/native/postgresql.sha256`, `src-tauri/resources/**` (generated, gitignored), `core/src/paths/**`, `core/src/lifecycle/mod.rs`, `core/src/lifecycle/pg.rs`, `core/tests/pg_lifecycle.rs` |
| R1-04 | `core/src/db/**`, `core/src/test_support/**`, `core/tests/db_schema.rs` |
| R1-05 | `core/src/importer/mod.rs`, `core/src/importer/types.rs`, `core/src/importer/normalize/**`, `core/src/importer/webp.rs`, `core/src/importer/dedupe.rs`, `core/tests/golden/**` |
| R1-06 | `src/app/**`, `src/components/common/**`, `src/screens/onboarding/**`, `src/screens/import/**`, `src/screens/settings/**`, `src/locales/{zh-TW,en}/{common,onboarding,import,settings}.json` |
| R1-07 | `src/components/mail/**`, `src/components/dsl/**`, `src/components/sql-editor/**`, `src/screens/search/**`, `src/screens/advanced-sql/**`, `src/locales/{zh-TW,en}/{search,sql}.json` |
| R1-REV | `docs/logs/review-round1.md` |

Overlap check (mechanical rule for the detail planners): no R1 path string is a prefix of another owner's path, except the documented carve-outs `core/src/importer/**` (R1-05 owns `mod.rs`, `types.rs`, `normalize/**`, `webp.rs`, `dedupe.rs`; R1-01 owns the six kickoff-only files), `core/src/lifecycle/**` (R1-03 owns `mod.rs`, `pg.rs`; R1-01 owns the six kickoff-only files) and `crates/pff/Cargo.toml` (created in the kickoff, owned by R1-02 afterwards). Kickoff stub files under `src/screens/**`, `src/components/common/**`, `src/app/AppStatusContext.tsx` and `src/locales/**` are owned by the R1-06 / R1-07 row that lists them; R1-01 only creates them in the kickoff commit.

## 6. Round 2 ownership table

| Owner | Paths |
|---|---|
| R2-08 | `core/src/lifecycle/**`, `core/src/settings/**`, `core/src/paths/**`, `core/src/gate/**`, `scripts/fetch-postgres.mjs`, `src-tauri/native/postgresql.sha256`, `src-tauri/resources/**`, `core/tests/pg_lifecycle.rs`, `core/tests/lifecycle.rs` |
| R2-09 | `core/src/importer/mod.rs`, `core/src/importer/{types,queue,pst_reader,inflight,progress,webp,dedupe}.rs`, `core/src/importer/normalize/**`, `crates/pff/**`, `src-tauri/native/libpff/**`, `scripts/vendor-libpff.sh`, `scripts/fixtures/**`, `tests/fixtures/**`, `core/tests/golden/**`, `core/tests/import_pipeline.rs`, `core/examples/**` |
| R2-10 | `core/src/importer/writer.rs`, `core/src/importer/remove.rs`, `core/src/db/**`, `core/src/test_support/**`, `core/tests/db_schema.rs`, `core/tests/writer.rs`, `core/tests/removal.rs`, `crates/outlooker-loadgen/**`, `docs/logs/loadtest-*.md`, `docs/logs/perf-*.md` |
| R2-11 | `core/src/search/**`, `src-tauri/schemas/**`, `core/tests/search.rs` |
| R2-12 | `core/src/sql_console/**`, `core/src/maintenance/**`, `core/tests/sql_console.rs`, `core/tests/maintenance.rs` |
| R2-13 | `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, `src-tauri/rust-toolchain.toml`, `src-tauri/build.rs`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/**`, `src-tauri/icons/**`, `src-tauri/.cargo/config.toml`, `src-tauri/src/**`, `core/Cargo.toml`, `core/src/lib.rs`, `core/src/dto/**`, `core/src/sink.rs`, `core/src/app/**`, `core/tests/app_compose.rs`, `src/api/bindings/**` |
| R2-14 | `mise.toml`, `package.json`, `package-lock.json`, `vite.config.ts`, `tsconfig.json`, `index.html`, `.gitignore`, `wdio.conf.ts`, `tests/e2e/**`, `src/**` except `src/api/bindings/**` |
| R2-REV | `docs/logs/review-round2.md` |

Overlap check: carve-outs are `core/src/importer/**` (R2-09 vs R2-10 split by file as listed) and `src/**` vs `src/api/bindings/**` (bindings are generated by `cargo test` from R2-13's DTOs). A dependency change in a manifest goes through the manifest owner (R2-13 for Cargo, R2-14 for npm).

---

## 7. Verification gates

Gates run on the round's head commit by a dedicated gate-runner team member (runs commands, reports raw output; the leader judges). Each command's exit code is captured right after the command (`cmd > log 2>&1; echo "RC=$?" >> log`). Writers broadcast SETTLED before the gate window opens; the window has zero writes. Commands that exceed the foreground time limit are split, never moved to the background. All gates in §7.1 and §7.2 run on the macOS dev machine and do not depend on any Windows step.

### 7.1 Round 1 gate (macOS)

| # | Command (from repo root unless noted) | Pass |
|---|---|---|
| G1.1 | `node scripts/fetch-postgres.mjs` | RC=0, `src-tauri/resources/postgresql/bin/postgres` exists |
| G1.2 | `grep -q bb5bcc0 src-tauri/native/libpff/ORIGIN && test -f tests/fixtures/external/outlook.pst && grep -q CC-BY-4.0 tests/fixtures/external/NOTICE` | RC=0 |
| G1.3 | `cd src-tauri && cargo build --workspace --locked` | RC=0 |
| G1.4 | `cd src-tauri && cargo test --workspace --locked --features outlooker-core/test-support` | RC=0; the per-task test names of §3 appear in the output as `ok` (checked by name, not by count) |
| G1.5 | `git diff --exit-code src/api/bindings` (after G1.4) | RC=0 |
| G1.6 | `npm ci && npm run typecheck && npm test && npm run build` | RC=0 |
| G1.7 | `grep -rEl "webDarkTheme\|teamsDarkTheme\|theme-one-dark\|prefers-color-scheme: *dark" package.json src src-tauri/tauri.conf.json` | no file listed (grep RC=1; RC=2 = instrument failure) |
| G1.8 | `grep -rEn "T[B]D\|T[O]DO" src src-tauri/src src-tauri/crates --include=*.rs --include=*.ts --include=*.tsx` | no match (grep RC=1) |
| G1.9 | R1-REV log has no `BLOCKED` | `grep -c BLOCKED docs/logs/review-round1.md` prints 0 |

### 7.2 Round 2 gate (macOS)

| # | Command | Pass |
|---|---|---|
| G2.1 | G1.1 … G1.8 on the Round 2 head | as in §7.1 |
| G2.2 | `cd src-tauri && cargo test --workspace --locked --features outlooker-core/test-support` | RC=0; R2 test names present as `ok` |
| G2.3 | `cd src-tauri && cargo build --release -p outlooker --locked && cargo tree -p outlooker -e normal \| grep -c wdio` | build RC=0; printed count `0` (grep RC=1 here is expected, not a failure) |
| G2.4 | `npm run e2e` | RC=0 |
| G2.5 | `npm run tauri build` | RC=0; `.dmg` exists; size recorded (spec §0.3 installer-size risk) |
| G2.6 | **1M-message load test** (Part 04 §12.3, Part 02 §9): `cd src-tauri && cargo test -p outlooker-loadgen --release -- --ignored perf_generate`, then each `perf_<gate>` test separately | every threshold row of Part 04 §12.3 and every Part 02 §9 row met; results in `docs/logs/loadtest-<date>.md` with commit hash and `EXPLAIN (ANALYZE, BUFFERS)` per canned query. Thresholds are pre-registered in the spec and are not changed after a run. |
| G2.7 | Import throughput on a real large PST (user-provided, not in git): `cargo run --release -p outlooker-core --features test-support --example import_bench -- <path>` | ≥ 150 messages/s sustained, peak RSS ≤ 1.5 GB (Part 02 §9); result in `docs/logs/perf-<date>.md` |
| G2.8 | Single instance: launch the built `Outlooker.app` binary twice | `pgrep -x Outlooker` counts 1, the first window is focused, one postmaster runs (`pgrep -x postgres` lists one postmaster parent) |
| G2.9 | R2-REV log has no `BLOCKED` | `grep -c BLOCKED docs/logs/review-round2.md` prints 0 |

The load test (G2.6) runs after R2-10 and R2-09 have landed and before round close; it is the longest gate (index rebuild alone may take up to 30 min) and the only gate that may take hours, so the gate-runner starts it first and checks first liveness (log growing, generation step advanced) within 5 minutes. A failed threshold fails Round 2; there is no third round without the user's approval (convergence contract round budget).

Round 2 known limitation recorded at close: the 8 Outlook-generated fixtures (Part 04 §12.1) do not exist yet, so libpff extraction of real `cid:` content ids, Big5 / GBK codepage properties and real Message-ID properties is verified only in the Windows phase (W2).

### 7.3 Windows phase (via SSH, after both macOS gates pass)

The user opens SSH to a Windows x64 machine only after all macOS-side work is done. Every Windows step is here; nothing earlier depends on it. Multi-step remote commands use `powershell -Command` with `;` separators (not cmd `&`). Prerequisites on the machine: VS 2022 Build Tools (MSVC), Rust 1.99.1, Node 24.22.x, `gh`, WebView2 runtime; Outlook desktop for W2. Defects found here are reported to the leader with the failing command output; fixes are routed to the Round 2 owner of the path and need the user's approval (outside the 2-round budget).

| # | Step | Pass |
|---|---|---|
| W1 | First Windows build + Windows locale self-check (spec §0.3, Part 01 §6): `node scripts/fetch-postgres.mjs`; `cd src-tauri; cargo build --workspace --locked` (MSVC builds pff with no configure); `cargo test -p outlooker-core --locked --features test-support --test pg_lifecycle` | RC=0 each; the test log names the winning initdb locale (`en-US` or `English_United States.65001`) |
| W2 | Outlook fixtures (Part 04 §12.1): `pwsh scripts/fixtures/make-pst.ps1 -Out tests/fixtures/pst`; copy the 8 files back to the dev machine; the leader commits them; then on both machines `cargo test -p outlooker-core --locked --features test-support,outlook-fixtures --test import_pipeline` | script RC=0, 8 files each < 5 MB; test RC=0 on macOS and Windows |
| W3 | Windows release acceptance (Part 02 §10 last row): `cargo test --workspace --locked --features outlooker-core/test-support`; `npm ci; npm run e2e` (includes the inline image through `http://mailimg.localhost` and the 100k-row scroll test on WebView2); `npm run tauri build` (NSIS installer, size recorded); manual checklist: data dir move across volumes; launch twice → `Get-Process Outlooker` counts 1 and the first window is focused | RC=0 each; checklist items ticked in `docs/logs/windows-phase-<date>.md` |

W1, W2 and W3 are release blockers. They are not Round 1 or Round 2 close criteria.

---

## 8. Detail-fill split (3 planners, parallel, no shared text)

Each planner writes the full Stage 1 block (Files / Interfaces / Behavior / Constraints / Acceptance criteria) and then the Stage 2 steps for its tasks, in its own file. Task ids, owned paths and acceptance bullets of §3–§6 are the floor: a detail file may add acceptance bullets but may not remove one or change an owned path (path changes go to planner-1, who edits this outline).

| Planner | File (exclusive writer) | Tasks | Produces blocks it must freeze first |
|---|---|---|---|
| planner-1 | this outline `docs/superpowers/plans/2026-10-21-outlooker-plan.md` + `docs/superpowers/plans/2026-10-21-outlooker-plan-foundation.md` | R1-01, R1-02, R1-03, R1-04, R2-08, R1-REV, R2-REV, Windows phase steps | C1–C8, C14 (Supervisor, SettingsStore), C15 |
| planner-2 | `docs/superpowers/plans/2026-10-21-outlooker-plan-backend.md` | R1-05, R2-09, R2-10, R2-11, R2-12 | C9, C11, C12, C13, C14 (ImportManager, SearchService, SqlConsole, Maintenance) |
| planner-3 | `docs/superpowers/plans/2026-10-21-outlooker-plan-frontend.md` | R1-06, R1-07, R2-13, R2-14 | C10, `core::app` signatures, the Command Layer handler list and event throttle keys |

Sequence:
1. Each planner writes the Interfaces → Produces blocks of its tasks first and sends the file path + section heading to the other two planners and the leader.
2. Each planner checks every Consumes line of its tasks against the producer's block, and reports mismatches to the producer (the producer edits its own file; nobody edits another planner's file).
3. Stage 2 steps per task; then the skill's Self-Review (spec coverage, placeholder scan, type consistency) per file.
4. planner-1 updates §10 coverage of this outline if a detail file moves a spec section between tasks.

---

## 9. Resolved questions (leader rulings after architect review)

1. Locale layout `src/locales/<lang>/<namespace>.json`, `<lang>` exactly `zh-TW` and `en` — approved.
2. `core/src/dto/` as a leaf module, grep-checked in R1-01 — approved.
3. JSON-schema test with `jsonschema = { version = "=0.59.0", default-features = false }`, `outlooker-core` dev-dependency only — approved.
4. Fixtures: commit libyal `outlook.pst` with a CC-BY-4.0 NOTICE; Round 2 closes on the minimal fixture set (§4 intro); Outlook-generated fixtures are a release blocker and a Round 2 known limitation (§7.2, W2).
5. Windows: one Windows phase via SSH after the macOS gates (§7.3).
6. No CI workflow file; local gates only (§7).
7. Failed-screen retry: new command `app_retry()` served by the Lifecycle Supervisor; 24 commands in total (spec Part 03 §4 is updated in parallel by its owner).
8. PST removal dialog shows the PST's message count only; out-of-scope prototype controls are listed in Global Constraints.
9. libpff vendoring deadline 2026-11-18 (R1-02); `build.rs` configures in a copy of the tree (spec erratum applied in parallel).

---

## 10. Coverage table

### 10.1 Spec sections → tasks

| Spec section | Task(s) |
|---|---|
| §0.2 user decisions | Global Constraints; R1-01 (identifier, ts-rs), R1-07 (10,000+ / 100,000 / 10,000-row caps in UI), R2-11 (count cap), R2-12 (30 s / 10,000 rows), R2-13 (event rate) |
| §0.3 known risks | W1 (Windows locale), G2.6 (sizes, throughput, latency), W2 (fixtures / Message-ID), R1-05 (RTF converter), R2-14 + W3 (webview scroll height), G2.5 + W3 (installer size) |
| Part 01 §1 stack, toolchain, §8 other crates | R1-01 |
| Part 01 §2 frontend | R1-01 (deps), R1-06, R1-07 |
| Part 01 §3 shell | R1-01, R2-13 |
| Part 01 §4 PST parsing | R1-02 |
| Part 01 §5 mail content processing | R1-05 |
| Part 01 §6 bundled PostgreSQL | R1-03, W1 |
| Part 01 §7 database access | R1-04 |
| Part 01 §9 test tooling | R1-04 (test helper), R1-02 (PST fixtures), R1-06 / R1-07 (Vitest), R2-14 (E2E), R2-13 (`e2e` feature) |
| Part 02 §1 process model | R2-13 (process), R2-09 (reader thread, rayon), R1-03 (postgres child) |
| Part 02 §2.1 state machine | R2-08 |
| Part 02 §2.2 app start | R2-13 (step 1, steps 8–9 in `core::app`), R2-08 (steps 2–7), R1-03 (steps 4–5 primitives), R2-09 (step 8 recovery) |
| Part 02 §2.3 first run | R2-08, R1-03 (initdb, conf, hba), R1-04 (credentials, migrations) |
| Part 02 §2.4 health monitor | R2-08 |
| Part 02 §2.5 app exit | R2-13 (`core::app::shutdown`), R2-09 (step 1), R2-12 (step 2), R2-08 (steps 3–4) |
| Part 02 §2.6 locking | R1-01 / R2-13 (single instance, G2.8, W3), R1-03 (postmaster.pid), R1-01 (Work Gate) |
| Part 02 §3 / §3.1 components | per component: App Frame / Screens R1-06, R1-07; Mail Preview Frame R1-07; Typed API Client R1-01; App Shell / Command Layer / Event Emitter R2-13; Paths R1-03; Settings Store R2-08; Lifecycle Supervisor R2-08; PG Controller R1-03; Db R1-04; Work Gate R1-01; Import Manager R2-09 (queue, pst_reader), R1-05 (normalize, webp, dedupe), R2-10 (writer, remove); pff R1-02; Search Service R2-11; SQL Console R2-12; Maintenance R2-12 |
| Part 02 §4 import data flow | R2-09 (steps 1–5, 8), R2-10 (steps 6–7) |
| Part 02 §5 search, preview, Advanced SQL flow | R2-11, R2-12, R2-13 (scheme routing), R1-07 |
| Part 02 §6.1 error table | R2-09 (incl. PG crash during import via C15), R2-10 (disk space), R1-05 (charset, image, panic rows), R2-08 (PG start, self-check, move rows), R2-12 (SQL rows) |
| Part 02 §6.2 cancel / resume | R2-09, R2-10 (checkpoint in batch) |
| Part 02 §6.3 poison item | R2-09 |
| Part 02 §7.1 untrusted HTML | R1-05 (layer 1), R1-07 (layer 2), R2-13 (layer 3 CSP, scheme) |
| Part 02 §7.2 read-only SQL | R1-04 (role, grants), R2-12 |
| Part 02 §7.3 local DB access | R1-03, R1-04 |
| Part 02 §8 storage layout | R1-03 (paths), R2-08 (relocate), R2-10 (`seed` data root) |
| Part 02 §9 performance budget | R2-10 (harness incl. rows missing from Part 04 §12.3), R2-09 (`import_bench`), G2.6, G2.7 |
| Part 02 §10 testing strategy | all tasks' acceptance; R2-09 (fault injection), R2-14 (E2E), W3 |
| Part 03 §1, §1.1 conventions, type generation | R1-01, R2-13 |
| Part 03 §1.2 error type | R1-01, R1-06 (`formatAppError`) |
| Part 03 §2 screen → commands map | R1-06, R1-07 |
| Part 03 §3 command index (24 incl. `app_retry`) | R2-13 (handlers), R1-01 (`COMMANDS`) |
| Part 03 §4 app status, first run, `app_retry` | R2-08, R1-06 |
| Part 03 §5.1 query model + JSON schema | R1-01 (types, schema), R2-11 (compiler, limits), R1-07 (DSL builder) |
| Part 03 §5.2 `search_mail` | R2-11, R1-07 |
| Part 03 §5.3 `search_count` | R2-11, R1-07 |
| Part 03 §5.4 `filter_options` | R2-11, R1-07 |
| Part 03 §5.5 `mail_get` | R2-11, R1-07 |
| Part 03 §5.6 preview, images, CSP | R2-11 (URL rewrite, image), R2-13 (scheme, CSP), R1-07 (iframe) |
| Part 03 §6 Advanced SQL | R2-12, R1-07 |
| Part 03 §7 Import management | R2-09, R2-10 (removal), R1-06 |
| Part 03 §8.1 settings | R2-08, R1-06 |
| Part 03 §8.2 data dir move | R2-08, R1-06 |
| Part 03 §8.3 maintenance | R2-12, R1-06 |
| Part 03 §8.4 db status | R2-08, R1-06 |
| Part 03 §9 events | R1-01 (types, names), R2-13 (emitter, throttle), R2-08 / R2-09 / R2-12 (emit), R1-06 / R1-07 (listen) |
| Part 04 §1 cluster requirements | R1-03 (locale), R1-04 (roles) |
| Part 04 §2 schema, §2.1 status derivation | R1-04; R2-09 (§2.1) |
| Part 04 §3 indexes | R1-04 (incl. `TSV_EXPR`) |
| Part 04 §4.1 dedupe key | R1-05 |
| Part 04 §4.2 concurrency-safe upsert | R2-10 |
| Part 04 §4.3 removing a PST | R2-10 |
| Part 04 §5.1–§5.3 stages, folders, message selection | R2-09; R2-10 (disk guard of §5.1) |
| Part 04 §5.4–§5.8 properties, bodies, sanitize, WebP, attachments | R1-05 (transform), R2-09 (extraction in reader) |
| Part 04 §5.9 checkpoint and resume | R2-09 (reader skip, queue, mtime reset), R2-10 (checkpoint write) |
| Part 04 §6.1–§6.3 bulk write path | R2-10 |
| Part 04 §6.4 index policy | R1-04 (DDL routines), R2-09 (policy and tests), R2-13 (startup self-heal test) |
| Part 04 §6.5 after each job | R2-09 |
| Part 04 §6.6 PostgreSQL configuration | R1-03 |
| Part 04 §6.7 expected size | G2.6 (DB size gate) |
| Part 04 §7.1–§7.6 search | R2-11 |
| Part 04 §8.1 role and session limits | R1-04 (role), R2-12 (session) |
| Part 04 §8.2 views | R1-04 |
| Part 04 §9.1 maintenance tasks | R2-12 |
| Part 04 §9.2 `db_status` | R2-08 |
| Part 04 §10 command → table map | R2-13 (cross-check), R2-REV |
| Part 04 §11 security notes | R2-REV (security pass), R2-11, R2-12 |
| Part 04 §12.1 fixtures | R1-02 (`outlook.pst`, script), R2-09 (minimal set), W2 |
| Part 04 §12.2 automated tests | R1-04, R1-05, R2-09, R2-10, R2-11, R2-12 (rows as listed in each task) |
| Part 04 §12.3 load generator and gates | R2-10, G2.6 |
| Part 04 §13 verification sources | reference only (no implementation) |

### 10.2 Requirements screens → tasks

| Screen (requirements.md) | UI task | Backend task(s) | E2E |
|---|---|---|---|
| 1. Search (search box, DSL blocks, results list + preview) | R1-07 | R2-11 | R2-14 |
| 2. Advanced SQL (1:3:1:3, saved queries, editor, mail list / table, preview / row detail) | R1-07 | R2-12, R2-11 (hydrate, `mail_get`) | R2-14 |
| 3. Import management (PST list, queue, progress, cancel, resume, remove) | R1-06 | R2-09, R2-10 | R2-14 |
| 4. Settings (storage location, language, WebP quality, index maintenance, DB status) | R1-06 | R2-08, R2-12 | R2-14; W3 manual (move across volumes) |
| 5a. Empty states for each screen | R1-06 (common `EmptyState`, Import, Settings), R1-07 (Search, Advanced SQL) | R2-08 (`counts`), R2-09 (`pst_list`, `library:changed`) | R2-14 |
| 5b. First-run onboarding (and Failed screen retry) | R1-06 | R2-08 | R2-14 |

---

## 11. Review resolution

Review files: `scripts/tmp/review-outline-arch-stack.md` (stack), `review-outline-arch-system.md` (system), `review-outline-arch-fe-api.md` (fe-api), `review-outline-arch-backend.md` (backend).

| Review item | Kind | Fixed in |
|---|---|---|
| stack B1 — kickoff `bundle.resources` breaks build before PG fetch | blocker | §1.2 `tauri.conf.json` row, §2.1 item 3, R1-01 acceptance, R2-13 scope |
| stack S1 — configure must run in a copy of the tree; no hidden zlib | should-fix | §1.3 pff row, R1-02 scope + `otool -L` acceptance |
| stack S2 — vendor script via `gh run download`; deadline 2026-11-18; ORIGIN / header checks | should-fix | §1.4 script row, R1-02 scope (deadline) + acceptance, G1.2 |
| stack S3 — toolchain pins | should-fix | Global Constraints "Toolchain", §1.1 / §1.2 rows, §2.1 item 1, R1-01 acceptance, §5 / §6 tables |
| stack S4 — mechanical pin checks | should-fix | Global Constraints "Versions", R1-01 acceptance (exact-pin grep, `--locked`, `cargo tree`) |
| stack N1 — no `libpff_set_codepage` | nit | R1-02 acceptance |
| stack N2 — wdio grep RC | nit | G2.3, R2-13 acceptance |
| stack N3 — system `tar` in fetch script | nit | Global Constraints forbidden deps, §1.4, R1-03 scope |
| stack N4 — single-instance check | nit | G2.8, W3 |
| system B1 — PG crash during import: no interface, no test | blocker | C15, R2-08 scope, R2-09 scope + "PG crash" acceptance, R2-13 scope |
| system S1 — exit / start composition untested in Tauri crate | should-fix | Decisions, `core::app` (§1.3, §6 R2-13), R2-13 `app_compose` acceptance |
| system S2 — PID-reuse branch; restart budget | should-fix | R1-03 acceptance; R2-08 scope + acceptance |
| system S3 — data dir move failure path | should-fix | R2-08 scope + acceptance |
| system S4 — job-status reset at start | should-fix | R2-09 `recover_after_crash` acceptance, R2-13 `app_compose` acceptance |
| system S5 — single instance acceptance | should-fix | G2.8, W3, R2-13 plugin-order source check |
| system S6 — Part 02 §9 rows missing from gates | should-fix | R2-10 scope + acceptance, G2.6 |
| system S7 — `webp_quality` dependency | should-fix | R2-09 scope |
| system Q2 — dto leaf condition | ruling | Decisions, R1-01 acceptance grep |
| system N2 — query set | nit | R2-10 scope (Part 04 §12.3 set binding) |
| system N3 — E2E dry-run early | nit | §2.3, R2-14 scope |
| fe-api B1 — Failed-screen retry has no command | blocker | `app_retry` in C1 / C4 / C14, R1-01, R1-06, R2-08, R2-13 (24 commands) |
| fe-api B2 — removal dialog counts not served | blocker | Global Constraints out-of-scope list, R1-06 scope + acceptance |
| fe-api S1 — counts from `library:changed` | should-fix | C10, R1-06 scope + acceptance |
| fe-api S2 — Advanced SQL 10,000-row cap | should-fix | Global Constraints limits, R1-07 scope + acceptance, R2-12 scope + acceptance |
| fe-api S3 — "10,000+" acceptance | should-fix | R1-07 acceptance |
| fe-api S4 — prototype controls without commands | should-fix | Global Constraints out-of-scope list, R1-06 / R1-07 scope, R1-06 out-of-scope-controls test |
| fe-api S5 — Advanced SQL column 4 row detail | should-fix | R1-07 scope + acceptance |
| fe-api N1 — no TS DTO copies before C1 | nit | Global Constraints "Type rule", §2.1 last paragraph, R1-06 depends |
| fe-api N2 — dialog permission in capabilities | nit | §1.2 capabilities row, R1-01 scope |
| fe-api N3 — immediate emit on phase change | nit | R2-13 scope + acceptance |
| fe-api N4 — refetch `filter_options` on `library:changed` | nit | R1-07 scope + acceptance |
| backend B1 — index-drop policy untested | blocker | R2-09 "Index policy" acceptance, R2-13 `app_compose` self-heal test |
| backend B2 — two-phase search not exercised | blocker | Global Constraints "Test seams", R2-11 scope (W parameter) + "Two-phase" acceptance |
| backend S1 — app-role privileges | should-fix | R1-04 acceptance |
| backend S2 — 64 MB / 1 s batch limits, `synchronous_commit` | should-fix | C11 `WriterLimits`, R2-10 scope + acceptance |
| backend S3 — fallback-hash dedupe without PST | should-fix | R2-10 acceptance |
| backend S4 — read back §6.6 settings | should-fix | R1-03 acceptance |
| backend S5 — index use asserted; shared `TSV_EXPR` | should-fix | C8, R1-04 scope + acceptance, R2-11 "Index use" acceptance |
| backend S6 — mtime reset; status derivation | should-fix | R2-09 scope + acceptance |
| backend S7 — remove PST with queued job | should-fix | R2-09 scope + acceptance |
| backend S8 — `NormalizedItem` DB invariants | should-fix | R1-05 invariants acceptance |
| backend S9 — single disk-guard owner | should-fix | C11, R2-10 scope + acceptance, R2-09 scope (maps error only) |
| backend N1 — remote image in hostile grep | nit | R1-05 acceptance |
| backend N2 — WebP resize / limits / known-hash skip | nit | R1-05 scope + acceptance |
| backend N3 — after-job VACUUM check | nit | R2-09 acceptance |
| backend N4 — vacuum orphan cleanup test | nit | R2-12 acceptance |
| backend N5 — index DDL on admin connection | nit | C8, R2-09 scope, R2-12 scope |
| backend §9 Q4 — fixture license and minimal set | ruling | §1.4, R1-02, §4 intro, R2-09 acceptance, R2-14 scope, §7.2 known limitation, W2 |

Not applied (parking lot per leader instruction): stack parking lot (macOS notarization of bundled PG, CI file, extended forbidden-dependency grep); system parking lot (Windows Job Object, hosted Windows CI); fe-api parking lot (`pst_remove_preview`, pause and other out-of-scope controls, highlighting, drag-and-drop); backend parking lot items; system N1 and fe-api §9 Q1 spec-wording updates (spec owners' work).
