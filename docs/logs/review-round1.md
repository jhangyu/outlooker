# Round 1 review log (R1-REV)

Reviewer: rev-r1. Plan: `docs/superpowers/plans/2026-10-21-outlooker-plan.md` §3, §5; procedure `docs/superpowers/plans/2026-10-21-outlooker-plan-foundation.md` "R1-REV / R2-REV steps"; spec `docs/superpowers/specs/2026-10-21-outlooker-design.md`.
Finding format: `- [blocker|should-fix|nit] file:line — finding — evidence`. Verdict words appear only in section headings.

## R1-01 — PASS

Commits reviewed: bc20348 (kickoff), 70a8f42 (contract: DTOs, sink, gate, TS client, schema), 8ab36de (ResizeObserver stub in setup.ts), e72c88d (wdio local-runner and spec-reporter).

Pass 1 (bc20348):
- Ownership: all 108 paths are R1-01 rows or kickoff stubs of §1 / §5 (`git show --stat bc20348`). OK.
- Versions: `cargo tree -p outlooker-core --depth 1 -e normal --locked --offline` RC=0; every Part 01 §5 / §7 / §8 crate at its pin (ammonia 4.2.4, base64 0.23.1, blake3 1.8.7, chardetng 1.0.0, deadpool-postgres 0.14.2, encoding_rs 0.8.42, getrandom 0.4.3, html2text 0.17.4, image 0.25.10, log 0.4.34, rayon 1.12.0, refinery 0.10.0, serde 1.0.229, serde_json 1.0.154, sysinfo 0.39.6, thiserror 2.0.21, tokio 1.54.0, tokio-postgres 0.7.18, ts-rs 12.0.1, webp 0.3.1). App crate: tauri =2.12.5, tauri-build =2.7.4, dialog =2.8.3, log =2.10.1, single-instance =2.5.4, wdio-webdriver =1.5.0 optional behind feature `e2e`. `grep -nE '"[\^~]' package.json` RC=1.
- Toolchain: `rustc --version` in src-tauri prints `rustc 1.99.1`; `mise exec -- node --version` prints `v24.22.1`; `engines.node` is `>=24.22 <25`. OK.
- Forbidden deps: none in `Cargo.lock` (sqlx, bindgen, testcontainers, postgresql_embedded, tauri-specta, mail-parser: grep RC=1). npm `extract-zip` / `tar-stream` are transitive only (`npm ls`: @wdio/cli via @puppeteer/browsers, webdriverio via archiver), not used by the fetch script. OK.
- No-dark grep over package.json, src, tauri.conf.json: RC=1. OK.
- tauri.conf.json: identifier `us.jhangy.outlooker`, no `bundle.resources`, CSP `script-src 'self'`, `img-src 'self' data: mailimg: http://mailimg.localhost`, `frame-src 'self'` (spec Part 03 §5.6). Capabilities: `dialog:allow-open`, `log:default`. Single-instance plugin registered first (lib.rs:9). OK.
- Findings:
  - [blocker, closed by lead ruling and e72c88d] package.json:31-46 — `@wdio/local-runner` / `@wdio/spec-reporter` 9.32.0 (spec Part 01 §9) were missing from the kickoff. Lead ruling: not a blocker (the final plan adds them in R2-14 Step 1); impl-r101 added them in e72c88d anyway. Plan-wording note in the parking lot.

Pass 1 (70a8f42):
- Ownership: `core/src/dto/*.rs`, `core/src/{sink.rs,gate/mod.rs}`, `src-tauri/schemas/condition-group.json`, `src/api/**`. All R1-01. OK.
- C1: every type of Part 03 §1.2 and §4–§9 present with the spec fields and serde attributes (internally tagged `AppError` / `SearchQuery` / `ConditionNode` / `SqlResult` / `EnqueueResult`, camelCase fields, `Language` "zh-TW" / "en"); `SqlResult::Table.rows` is `Vec<Vec<Option<String>>>` as the foundation plan rule 4 states. `COMMANDS` = 24 names in Part 03 §3 order with `app_retry` third (commands.rs:2-10); `names::ALL` = 6 events.
- C2 `EventSink` / `CoreEvent::{name,payload}` / `RecordingSink` / `NullSink` (sink.rs); C3 `Gate` single-slot with `busy_reason` mapping and `released()` via `Notify` (no polling) (gate/mod.rs); C4 `call` re-throws `AppError` and wraps other rejections as `internal`, 24 `api.*` wrappers, `EVENTS`, `onEvent`, `pickPstFiles` / `pickFolder` with the `VITE_E2E` bypass.
- Leaf rule: `grep -rE "crate::(lifecycle|importer|search|sql_console|maintenance|settings|db|gate|paths|app)" src-tauri/crates/outlooker-core/src/dto` RC=1; no `From<…> for AppError` in `dto`.
- Acceptance (my runs): `cargo test -p outlooker-core --locked --lib -- dto:: gate:: sink::` RC=0 (59 tests incl. `worked_example_matches_spec_json`, `worked_example_root_validates_against_schema`, `every_activity_blocks_every_activity`, `released_resolves_after_drop`, `recording_sink_keeps_order_and_names`); then `git diff --exit-code src/api/bindings` RC=0; `commands.json` 24 entries incl. `app_retry`, `events.json` 6 entries; `mise exec -- npx vitest run src/api` RC=0 (2 files, 6 tests); `npm run typecheck` RC=0 at 79fc2f3 (detached worktree); `cargo build --workspace --locked` RC=0.
- Pass 2 (8ab36de, e72c88d):
  - 8ab36de: `src/test/setup.ts` ResizeObserver stub, made by impl-r101 (the owner) at the lead's request (lead confirmation). Ownership OK.
  - e72c88d: the package.json diff is only the two devDependencies at exact `9.32.0` plus a reformatted `engines` block; the caret/tilde grep on package.json gives RC=1; lockfile and `npm ls --depth=0` show both at 9.32.0. At e72c88d (detached worktree): `npm run typecheck` RC=0, `npx vitest run` RC=0 (16 files, 68 tests).
  - Missing `.invoke_handler(tauri::generate_handler![])` (lib.rs:8-18): lead ruling, parking lot (R2-13 owns the 24 handlers).

## R1-02 — PASS

Commits reviewed: f22cae5 (vendor), 9dc6196 (FFI wrapper, fixture, generator), 5120f28 (fix: olStoreUnicode = 2).

Pass 1 (f22cae5, 9dc6196):
- Ownership: f22cae5 = 1076 paths under `src-tauri/native/libpff/` + `scripts/vendor-libpff.sh`; 9dc6196 = `crates/pff/{build.rs,src/*,tests/outlook_pst.rs}`, `scripts/fixtures/make-pst.ps1`, `tests/fixtures/external/{NOTICE,outlook.pst}`. All R1-02. OK.
- C5 Produces (foundation plan R1-02): every listed signature present in `crates/pff/src/lib.rs` (version, Error, Result, check_signature, File::{open,root_folder,message_store}, Item::{identifier … attached_item}, AttachmentType, filetime_to_unix_micros) and `src/tags.rs` (all 20 tags, values match). Only platform branch is the `#[cfg(windows)]` UTF-16 open (lib.rs:112-135, 147-156), which the Stage 1 Behavior defines. OK.
- build.rs: copies the tree to `$OUT_DIR/libpff-src`, Unix `sh configure --disable-nls --disable-shared-libs --with-zlib=no` in the copy, MSVC defines `_CRT_SECURE_NO_WARNINGS` / `UNICODE` / `WINVER=0x0501`, `HAVE_LOCAL_<LIB>` for the 17 libs, no zlib (spec Part 01 §4.3). OK.
- Acceptance (my runs):
  - `grep -q bb5bcc0 src-tauri/native/libpff/ORIGIN` + three `test -f`: RC=0.
  - `cargo test -p pff --locked`: RC=0; `version_is_20260625`, `non_pst_is_error`, `outlook_pst_folders`, `outlook_pst_walk_is_deterministic`, `outlook_pst_welcome_message`, `filetime_conversion` ok; doctest `lib.rs - (line 4) - compile fail ... ok`.
  - `otool -L target/debug/deps/outlook_pst-8158793779263bfb`: only `/usr/lib/libSystem.B.dylib`; libz count 0.
  - `grep -rn libpff_set_codepage src-tauri/crates`: RC=1.
  - NOTICE contains `CC-BY-4.0` and `c3631ff`; sha256 of `tests/fixtures/external/outlook.pst` equals `scripts/tmp/outlook.pst` (df01707f…4e24).
  - pwsh syntax check: the first attempt could not run (`pwsh` not on PATH, RC=127). Rerun with PowerShell 7.6.7 at `scripts/tmp/pwsh/pwsh` (lead instruction): `scripts/tmp/pwsh/pwsh -NoProfile -Command "[ScriptBlock]::Create((Get-Content -Raw scripts/fixtures/make-pst.ps1)) | Out-Null"` RC=0; control `[ScriptBlock]::Create('if (')` RC=1 ("Missing condition in if statement"), so the check can fail. Closed; the real fixture run stays in the Windows phase (W2).
- Findings (pass 1):
  - [blocker] scripts/fixtures/make-pst.ps1:38 — `AddStoreEx($path, 3)` creates ANSI PSTs (OlStoreType: olStoreUnicode=2, olStoreANSI=3), against spec Part 04 §12.1. Fixed in 5120f28.

Pass 2 (5120f28): diff is exactly `AddStoreEx($path, 3)` → `AddStoreEx($path, 2)`; path `scripts/fixtures/make-pst.ps1` (R1-02). Blocker closed. Remaining items are in the parking lot.

## R1-03 — PASS

Commits reviewed: 1d6019c (fetch script, Paths, PG Controller, tests), f378648 (clippy fixes in pg.rs).

Pass 1:
- Ownership: `scripts/fetch-postgres.mjs`, `src-tauri/native/postgresql.sha256`, `core/src/paths/mod.rs`, `core/src/lifecycle/pg.rs`, `core/tests/pg_lifecycle.rs`. All R1-03. OK.
- postgresql.sha256: 6 triples; aarch64-apple-darwin and x86_64-pc-windows-msvc compared with the upstream `.sha256` release assets of theseus-rs 18.6.1 (curl): equal.
- fetch-postgres.mjs: `fetch` + `node:crypto` sha256 + system `tar` via `execFileSync` + prune of `include`, `lib/bitcode`, `lib/pgxs`, `lib/Python`, `lib/libLLVM*`, `lib/llvmjit*`, `lib/*.a` (spec Part 01 §3, §6). No npm archive package. OK.
- pg.rs: `auto_conf` equals spec Part 04 §6.6 + Part 02 §2.3 step 5 (all 25 settings); `HBA_CONF` is the single `host all all 127.0.0.1/32 scram-sha-256` line; `unix_socket_directories = ''`; initdb `-E UTF8 --locale-provider=libc --locale=<c>` with `LC_ALL` set and `LANG`/`PGDATA`/`PGPORT`/`PGTZ` removed; candidates per OS are the spec-defined platform branch (pg.rs:19-22); pwfile removed after initdb (pg.rs:236); port retry on "could not bind" (pg.rs:299-303); PID-file recovery never signals a non-postgres process (pg.rs:259-273).
- Acceptance (my runs): `mise exec -- node scripts/fetch-postgres.mjs` RC=0 ("already present", stamp equals the pinned hash); `--self-test` RC=0 ("wrong hash rejected"); `resources/postgresql/bin/postgres` and `lib/pg_trgm.dylib` exist, no `libLLVM*` / `llvmjit*` / `*.a`; `cargo test -p outlooker-core --locked --test pg_lifecycle` RC=0 with `init_passes_cjk_self_check`, `c_locale_is_rejected`, `stop_start_round_trip`, `orphan_postmaster_is_restarted`, `stale_pid_with_dead_process`, `pid_reused_by_live_non_postgres_process`, `port_conflict_retries`, `auto_conf_values_are_live` ok.
- Findings: none blocking; see parking lot.

## R1-04 — PASS

Commits reviewed: 635fcbf (migrations, roles, views, pools, Db handle, TestCluster), 8529112 (cfg gate on db_schema tests).

Pass 1:
- Ownership: `core/src/db/{mod,pools,credentials,indexes}.rs`, `core/src/db/migrations/V{1,2}__*.sql`, `core/src/test_support/mod.rs`, `core/tests/db_schema.rs`. All R1-04. OK.
- Security (Part 02 §7.2, §7.3; Part 04 §1, §8.1): V1 creates `outlooker_app` / `outlooker_ro` with `NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOREPLICATION NOBYPASSRLS`, ro `CONNECTION LIMIT 2`; V2 has every `ALTER ROLE outlooker_ro SET …` line, `REVOKE ALL ON SCHEMA public FROM PUBLIC`, `REVOKE TEMP … FROM PUBLIC`, `GRANT TEMP` to app, `SELECT, INSERT, UPDATE, DELETE, MAINTAIN` to app, `SELECT` on the six views only to ro; view bodies equal Part 04 §8.2; views owned by `outlooker_app`. Role passwords are 64-hex values set after migration (db/mod.rs:38-44). Credentials file 0600 on Unix (credentials.rs:36-40). OK.
- Acceptance (my runs): `cargo test -p outlooker-core --locked --features test-support --test db_schema` RC=0 with the 8 named tests ok; `cargo test -p outlooker-core --locked --lib db::indexes` RC=0 (`ddl_matches_migration`).
- Test change judged: `ro_role_sees_only_views` sets `default_transaction_read_only = off` before `CREATE TEMP TABLE` (db_schema.rs:61-64). It still tests the spec requirement and is the stronger test: a user can run that SET inside an Advanced SQL session, so `REVOKE TEMP` (Part 04 §8.1) must hold on its own, and the 42501 proves it does.
- Plan deviation accepted: `show_trgm('郵件系統')` prints multibyte trigrams as hashes, so the test asserts 5 trigrams and overlap with `show_trgm('郵件系')` (db_schema.rs:16-20) instead of the literal `郵件系`.
- Findings: none blocking; see parking lot.

## R1-05 — PASS

Commits reviewed: 0e626e8 (normalize, sanitizer, RTF, charset, WebP, dedupe, goldens).

Pass 1:
- Ownership: `core/src/importer/{types,webp,dedupe}.rs`, `core/src/importer/normalize/*.rs`, `core/tests/golden/*`. All R1-05. OK.
- Sanitizer (Part 04 §5.6, Part 02 §7.1 layer 1): allowed tags, clean-content tags, generic and per-tag attributes and the CSS property list in sanitize.rs:9-44 equal the spec lists item by item; `url_relative(Deny)`, `link_rel("noopener noreferrer")`, `strip_comments(true)`; `a href` keeps only http/https/mailto, `img src` keeps only cid-in-map / data:image (png|jpeg|gif|bmp|webp) rewritten to `outlooker-inline:<n>`, remote src removed; style values with `url(`, `expression(`, `javascript:`, `@import`, `behavior`, `\`, `/*`, `<` removed. `cid` and `data` in `url_schemes` (sanitize.rs:35) is safe: the only URL attributes allowed are `a href` and `img src`, and the filter rewrites or drops both.
- Hostile corpus `tests/golden/hostile.html` covers every acceptance item (`<script>`, `onerror=`, `javascript:`, `vbscript:`, CSS `url(`, `<iframe`, `<form`, `<base`, `<meta http-equiv`, `position:fixed`, remote `<img>`, `expression(`, `behavior`, `<svg><script>`).
- Dedupe key equals Part 04 §4.1 (prefixes, U+001F joins, Message-ID rules 1-5, body whitespace and zero-width deletion). WebP: 25 MB / 40 MP limits, ≤2 px skip, 2,048 px Lanczos3 resize, BLAKE3 of source bytes before decode.
- Acceptance (my run): `cargo test -p outlooker-core --locked --lib importer::` RC=0, 25 tests ok incl. `fromhtml_golden`, `big5_and_gbk`, `hostile_corpus_is_clean`, `golden_plain`, `golden_inline`, `one_pixel_image_loses_src`, `known_hash_skips_encode`, `resize_to_2048`, `limits_rejected`, `key_table`, `invariants_over_goldens`.
- Mutation check (lead request; isolated worktree, cp backup, restored from the backup): adding `"position"` to `STYLE_PROPERTIES` (sanitize.rs:43) made `hostile_corpus_is_clean` fail (RC=101, `left: ["position"]`, output `<div style="position:fixed;…">`); after the restore the 3 sanitizer tests pass (RC=0). The hostile test can turn red.
- Findings: none blocking; see parking lot.

## R1-06 — PASS

Commits reviewed: 11e381e (C10 shared contract, locales, test helpers), 26beebd (boot, app frame, failed / blocking screens), 60c5d1b (import screen, removal dialog), 32f5580 (settings, onboarding, global tests), f7399fb (global test fixes).

Pass 1:
- Deviations judged: `import.meta.glob` instead of `node:fs` in global.test.tsx (no @types/node; the no-dark test asserts more than 20 source files, so an empty glob cannot pass); the no-literal test skips text inside `.cm-editor` (lead ruling: CodeMirror chrome and user SQL are not UI strings); `eventsMock.reset` restores its implementation. All three keep the tests meaningful.
- Ownership: every path is under `src/app/**`, `src/components/common/**`, `src/screens/{onboarding,import,settings}/**` or `src/locales/{zh-TW,en}/{common,onboarding,import,settings}.json`. All R1-06. OK.
- C10: `EmptyState` props (EmptyState.tsx:4-10), `formatAppError(err, t)` (formatAppError.ts:10), `useAppStatus()` (AppStatusContext.tsx:68), screen entry names exported. OK.
- Theme: one `<FluentProvider theme={webLightTheme}>` (App.tsx:78); no dark token anywhere in `src` (global test + grep).
- Out-of-scope controls: removal dialog shows only the PST's `messageCount` and the "kept if found in other PST files" sentence, no deleted / kept split (RemoveDialog.tsx:12, 39); no pause / worker / re-encode / drag-and-drop control or key (grep over `src/screens`, `src/components`, `src/app`).
- i18n: no hard-coded JSX text or `title=` / `label=` / `placeholder=` / `aria-label=` literal in non-test `.tsx` (grep); locale key sets equal for all 6 namespaces.
- Acceptance (my run in a detached worktree at 79fc2f3 with `node_modules` linked): `mise exec -- npm run typecheck` RC=0; `mise exec -- npx vitest run` RC=0, 16 files / 68 tests. Named tests present and passing: `boot subscribes to every event before calling app_status`, `library:changed with messages > 0 updates useAppStatus().counts.messages`, `Failed screen Retry calls appRetry`, `each screen renders its empty state`, `removal dialog shows messageCount and calls pstRemove only after confirm`, `WebP quality outside 1-100 is not sent`, locale parity, cimode no-literal, no-dark, no-out-of-scope-controls (global.test.tsx:53-128).
- Findings: none blocking; see parking lot.

## R1-07 — PASS

Commits reviewed: ff548dd (search / sql locales, DSL builder, query composition, mail components, SQL editor), 25d984a (search and advanced SQL screens).

Pass 1:
- Ownership: every path is under `src/components/{mail,dsl,sql-editor}/**`, `src/screens/{search,advanced-sql}/**` or `src/locales/{zh-TW,en}/{search,sql}.json`. All R1-07. OK.
- Security (Part 02 §7.1 layer 2, Part 03 §5.6): `MailPreview` is the only HTML sink: `<iframe sandbox="" srcDoc={html} referrerPolicy="no-referrer">` (MailPreview.tsx:14); no `dangerouslySetInnerHTML`, `innerHTML`, `eval`, `new Function` or `allow-` token in non-test `src` (grep).
- Type rule: no `interface` / `type` named like a `src/api/bindings` type outside the bindings directory (loop over all 47 binding names).
- Limits: `MAX_LIST_ROWS = 100_000` (pages.ts:12), `COUNT_CAP = 10_000` (search/index.tsx:19), `sqlExecute` with `maxRows: MAX_ROWS` = 10,000 and a `crypto.randomUUID()` execution id (advanced-sql/index.tsx:22, 137).
- Acceptance (same run as R1-06): all R1-07 named tests present and passing — sandbox / no `allow-`, 600 pages × 200 rows stop at 100,000 with the cap row, `10,000+` / `9,999`, worked example deep-equals spec JSON, stale sequence dropped, `library:changed` refetches `filterOptions` without a list reload, `maxRows: 10000`, truncated MessageBar, MailList vs table mode, column-4 key/value list, Run disabled while executing; the R1-06 global tests iterate the `search` and `sql` namespaces.
- Findings: none blocking; see parking lot.

## Parking lot

- [nit] R1-01 bc20348 commit message — "registry re-check deferred to cargo and npm resolution" records no registry check (Global Constraints "Versions"). All pins equal Part 01, so no wrong version resulted.
- [nit] R1-01 bc20348 — `npm run typecheck` fails at the kickoff commit (stubs import `src/api/bindings/*`, generated in the contract commit; stated in the commit message). Foundation plan R1-01 Behavior says typecheck passes after the kickoff. Transient; re-checked at the final R1-01 commit.
- [should-fix] src-tauri/crates/pff/build.rs:31 — the `$OUT_DIR/libpff-src/.prepared` marker is never invalidated: after a re-vendor (`ORIGIN` changes, build.rs reruns) the stale copy is compiled. Fix: write the `ORIGIN` content into `.prepared` and re-copy when it differs.
- [should-fix] src-tauri/crates/pff/src/lib.rs:388-394 — `attachment_data` returns `Ok(Some(buf))` truncated when `libpff_attachment_data_read_buffer` returns 0 before `size` bytes are read; a short read looks like success (silent data loss). Fix: return `Err` when `filled < size`.
- [nit] scripts/fixtures/make-pst.ps1:29-31 — PT_SYSTIME read-back is compared as strings (`"$back" -ne "$value"`); Outlook may return the time with a different DateTimeKind / local offset, which would throw a false "Outlook did not keep property". Fix: compare `[datetime]` values via `.ToUniversalTime()` for 0x0040 tags. Windows-phase risk only.
- [nit] scripts/fixtures/make-pst.ps1:194-200 — corrupt.pst zeroes 512 bytes at 60 % of the file; spec Part 04 §12.1 wants "one item's data block". The R2-09 test pre-registers both outcomes, so this is acceptable; a comment noting that the offset is not proven to be inside an item block would help.
- [should-fix] scripts/fetch-postgres.mjs:75,85 — the archive is extracted under `os.tmpdir()` and then `renameSync(top, DEST)`; when the repo is on another volume than the temp dir (common on Windows: %TEMP% on C:, repo on D:) rename fails with EXDEV. Fix: `mkdtempSync(join(dirname(DEST), ".pg-"))` so the extract dir is on the destination volume.
- [should-fix] src-tauri/crates/outlooker-core/src/lifecycle/pg.rs:227-228 — the initdb pwfile (admin password) is written with the default umask (typically 0644) until it is removed after initdb. Fix: on Unix open it with `OpenOptionsExt::mode(0o600)`, as credentials.rs does.
- [should-fix] src-tauri/crates/outlooker-core/src/lifecycle/pg.rs:122,271 — a live same-user `postgres` whose executable path differs from the bundled one (e.g. an orphan from a previous app location after macOS App Translocation or an app move) is classified `Foreign` and start fails with `fileAccess{inUseByOtherUser}`, although spec Part 02 §2.2 step 4 reserves that error for another user's postgres. Fix: compare process owner UID (sysinfo `Process::user_id`) with the current user before choosing `Foreign`; for same-user postgres run `pg_ctl stop -D pgdata` as for an orphan.
- [nit] src-tauri/crates/outlooker-core/src/paths/mod.rs:55-57 — `dev_pg_dir()` embeds the build machine's absolute `CARGO_MANIFEST_DIR` into every build, release included. Fix: gate it with `#[cfg(any(test, feature = "test-support"))]`.
- [nit] scripts/fetch-postgres.mjs:50-58 — `--self-test` exercises `verify()` only and exits 0; it does not run the download path with a wrong pin. Acceptance wording ("a wrong hash makes the script exit non-zero") is met only through the shared `verify` + exit code 2 at line 95. Fix: let `--self-test` call `main` logic with an injected wrong pin and assert exit code 2.
- [should-fix] src-tauri/crates/outlooker-core/tests/db_schema.rs:36-44 — 7 rejecting rows for about 60 CHECK / UNIQUE / FK constraints of `V1__schema.sql`; plan outline R1-04 acceptance asks one per constraint of Part 04 §2 (the foundation plan lists only the test name). Untested examples: `import_jobs` state/finished_at CHECK (V1:132), `folders` parent self-reference CHECK (V1:26), `saved_queries_name_once` (V1:173), `image_blobs` width/height (V1:94-95). Fix: extend the `cases` table, one row per constraint.
- [nit] src-tauri/crates/outlooker-core/tests/db_schema.rs:100 — "second `ensure_search_indexes` is a no-op" is asserted only as "does not error". Fix: record `pg_class.oid` of the six indexes before and after the second call and assert equality.
- [nit] src-tauri/crates/outlooker-core/src/db/credentials.rs:7 — `Credentials` derives `Debug`, and `DbConn` (pools.rs:35) holds it, so any `{:?}` log of either prints the three passwords. Fix: hand-written `Debug` that redacts the fields.
- [nit] src-tauri/crates/outlooker-core/src/importer/normalize/mod.rs:81-92 — an HTML body over 16 MB falls through to the RTF `\fromhtml1` path; spec Part 04 §5.5 says it falls through to rule 3 (plain text). Fix: skip the RTF-HTML branch when the HTML body was rejected for size.
- [nit] src/screens/import/index.tsx:73 and src/screens/settings/index.tsx:60 — `db_status` is refetched every 10 s (`refetchInterval: 10_000`) while the screen is open; it runs `pg_database_size` and index queries each time, and spec Part 03 §8.3 / §9 deliver maintenance state through `maintenance:status-changed`. Fix: refetch `dbStatus` on that event and on `library:changed` instead of on a timer.
- [nit] plan §2.1 item 2 wording — the kickoff omitted `@wdio/local-runner` / `@wdio/spec-reporter` (spec Part 01 §9) because the full plan adds them in R2-14 Step 1; added early in e72c88d. Align the §2.1 "every npm dependency" wording with that step.
- [nit] src-tauri/src/lib.rs:8-18 — no `.invoke_handler(tauri::generate_handler![])` (foundation plan R1-01 Behavior). Lead ruling: R2-13 adds the 24 handlers.
