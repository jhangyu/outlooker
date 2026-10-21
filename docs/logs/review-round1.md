# Round 1 review log (R1-REV)

Reviewer: rev-r1. Plan: `docs/superpowers/plans/2026-10-21-outlooker-plan.md` §3, §5; procedure `docs/superpowers/plans/2026-10-21-outlooker-plan-foundation.md` "R1-REV / R2-REV steps"; spec `docs/superpowers/specs/2026-10-21-outlooker-design.md`.
Finding format: `- [blocker|should-fix|nit] file:line — finding — evidence`. Verdict words appear only in section headings.

## R1-01 — IN REVIEW

Commits reviewed: bc20348 (kickoff).

Pass 1 (bc20348):
- Ownership: all 108 paths are R1-01 rows or kickoff stubs of §1 / §5 (`git show --stat bc20348`). OK.
- Versions: `cargo tree -p outlooker-core --depth 1 -e normal --locked --offline` RC=0; every Part 01 §5 / §7 / §8 crate at its pin (ammonia 4.2.4, base64 0.23.1, blake3 1.8.7, chardetng 1.0.0, deadpool-postgres 0.14.2, encoding_rs 0.8.42, getrandom 0.4.3, html2text 0.17.4, image 0.25.10, log 0.4.34, rayon 1.12.0, refinery 0.10.0, serde 1.0.229, serde_json 1.0.154, sysinfo 0.39.6, thiserror 2.0.21, tokio 1.54.0, tokio-postgres 0.7.18, ts-rs 12.0.1, webp 0.3.1). App crate: tauri =2.12.5, tauri-build =2.7.4, dialog =2.8.3, log =2.10.1, single-instance =2.5.4, wdio-webdriver =1.5.0 optional behind feature `e2e`. `grep -nE '"[\^~]' package.json` RC=1.
- Toolchain: `rustc --version` in src-tauri prints `rustc 1.99.1`; `mise exec -- node --version` prints `v24.22.1`; `engines.node` is `>=24.22 <25`. OK.
- Forbidden deps: none in `Cargo.lock` (sqlx, bindgen, testcontainers, postgresql_embedded, tauri-specta, mail-parser: grep RC=1). npm `extract-zip` / `tar-stream` are transitive only (`npm ls`: @wdio/cli via @puppeteer/browsers, webdriverio via archiver), not used by the fetch script. OK.
- No-dark grep over package.json, src, tauri.conf.json: RC=1. OK.
- tauri.conf.json: identifier `us.jhangy.outlooker`, no `bundle.resources`, CSP `script-src 'self'`, `img-src 'self' data: mailimg: http://mailimg.localhost`, `frame-src 'self'` (spec Part 03 §5.6). Capabilities: `dialog:allow-open`, `log:default`. OK.
- Findings:
  - [blocker] package.json:31-46 — `@wdio/local-runner` and `@wdio/spec-reporter` (9.32.0; spec Part 01 §9 "Required" / "declare it directly") are missing; plan §2.1 item 2 requires every Part 01 §9 npm dependency pinned in the kickoff. Evidence: `grep -c 'local-runner\|spec-reporter' package.json` prints 0. Fix: `mise exec -- npm install --save-dev --save-exact @wdio/local-runner@9.32.0 @wdio/spec-reporter@9.32.0`, commit package.json and package-lock.json. Sent to team-lead.

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
  - `pwsh -NoProfile -Command "[ScriptBlock]::Create(...)"`: NOT RUN — `zsh: command not found: pwsh` (RC=127). Instrument missing, not a pass. Tagged needs-user-decision and sent to team-lead (install PowerShell on the Mac, or move the check to the Windows phase).
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
- Findings: none blocking; see parking lot.

## R1-06 — IN REVIEW

Commits reviewed: 11e381e (C10 shared contract, locales, test helpers).

Pass 1 (11e381e, partial task):
- Ownership: `src/app/{AppStatusContext.tsx,i18n.ts,testing.tsx}`, `src/components/common/*`, `src/locales/{zh-TW,en}/{common,import,onboarding,settings}.json`. All R1-06. OK.
- C10: `EmptyState` props (EmptyState.tsx:4-10), `formatAppError(err, t)` (formatAppError.ts:10), `useAppStatus()` (AppStatusContext.tsx:68) present with the §2.2 / frontend-plan shapes; `useLanguageSwitch` calls `changeLanguage` then `settingsUpdate({ language, webpQuality: null })` (hooks.tsx:23-36) as planned.
- Locale parity (python key-set diff over the working tree): common 41/41, import 53/53, onboarding 36/36, settings 57/57 equal; no key containing pause / worker / re-encode / format / highlight / drag / drop.
- Typecheck not judged yet: depends on R1-01 bindings and `src/api/call.ts` (`isAppError`), not yet on main. Full acceptance re-run at the final R1-06 commit.

## R1-07 — NOT STARTED

Commits reviewed: none yet.

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
