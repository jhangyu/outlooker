# Round 2 review log (R2-REV)

Reviewer: rev-r2. Plan: `docs/superpowers/plans/2026-10-21-outlooker-plan.md` §4, §6; procedure `docs/superpowers/plans/2026-10-21-outlooker-plan-foundation.md` "R1-REV / R2-REV steps"; spec `docs/superpowers/specs/2026-10-21-outlooker-design.md`.
Finding format: `- [blocker|should-fix|nit] file:line — finding — evidence`. Verdict words appear only in section headings.
Runs: detached worktree `scripts/tmp/rev2-wt` (own `target/`; `src-tauri/resources` and `node_modules` linked to the main tree). Logs of my runs: `scripts/tmp/rev2/*.log` (gitignored).
Test threads: on this Mac `kern.sysv.shmmni` = 32 SysV shared-memory IDs system-wide; each cluster and each running initdb holds one. With the default 16 test threads plus other members' clusters, TestCluster initdb fails with `FATAL: could not create shared memory segment: No space left on device` (reproduced: 22 parallel initdb, 2 failures, `scripts/tmp/rev2/initdb-par/`). Integration test runs below use `-- --test-threads 4` where noted; this is an environment limit, not a code defect.

## R2-08 — PASS

Commits reviewed: 1f13f4f (supervisor, C15 phase_watch, free_space, settings store), 98434ae (lifecycle tests, move fixes), e204725 (cfg gate), 32ac9a8 (initdb error detail), 80208cb (move target inside data root), 92dc26f (fetch script: owner-writable files).

Pass 1 (1f13f4f, 98434ae, e204725, 32ac9a8):
- Ownership: `core/src/lifecycle/{health,init,start,status,supervisor,move_dir,mod,pg}.rs`, `core/src/settings/mod.rs`, `core/tests/lifecycle.rs`. All R2-08. OK.
- C15: `Supervisor::phase_watch` (supervisor.rs:74) returns a `watch::Receiver<AppPhase>`; `free_space` (move_dir.rs:48) longest mount-point match on the nearest existing ancestor. No `importer` / `sql_console` import in lifecycle (grep).
- Acceptance (my runs at 32ac9a8): `cargo test -p outlooker-core --locked --features test-support --test lifecycle` RC=0 (16 passed, default threads and 4); `--test pg_lifecycle` RC=0 (8); `--lib lifecycle::` RC=0 incl. `initdb_stderr_classification`. Named tests: first_run_reaches_ready, adopt_existing_data_root, wrong_pg_version_fails, init_failure_removes_only_own_pgdata, retry_from_failed_reaches_ready, retry_when_not_failed_is_invalid, retry_without_cluster_returns_to_setup, health_restarts_killed_postmaster, restart_budget_exhaustion_fails, move_data_dir_succeeds, move_verify_failure_rolls_back, move_into_non_empty_is_invalid, move_while_import_is_busy, settings_language_before_ready, settings_webp_before_ready_is_not_ready, bootstrap_write_is_atomic.
- 32ac9a8 (lead request after the initdb flake): initdb stderr keeps FATAL / ERROR lines (pg.rs:103-106); only locale failures and the CJK self-check try the next locale, any other initdb failure returns at once (pg.rs:97-100, 250-259). Unit test covers the shared-memory text and an invalid locale name.
- Findings (pass 1):
  - [blocker, fixed in 80208cb] lifecycle/move_dir.rs:74-80 — a data-dir move target inside the current data root (e.g. `<root>/backup`, not existing) passed the empty check; copy_tree (move_dir.rs:31-35) creates dst before `read_dir(src)` and recurses into its own copy with PostgreSQL stopped, until ENAMETOOLONG or a full disk. Spec Part 02 §7.3 (frontend paths re-validated in the core).

Pass 2 (80208cb, 92dc26f):
- 80208cb: `resolve_path` (canonical nearest existing ancestor + remaining components) and `starts_with(canonical old root)` → `invalidInput{field:"target", reason:"insideDataRoot"}` (move_dir.rs:67-79, 93-97); new test move_into_own_subfolder_is_invalid (root/backup, root/auth/x/y, root). My run at 3ca849a: `--test lifecycle` RC=0, 17 passed. Blocker closed.
- [blocker, fixed in 92dc26f] scripts/fetch-postgres.mjs — 42 extracted files (dependency-notices/python@3.13/*) were mode 0444; tauri-build copies `bundle.resources` into `target/<profile>/resources` keeping the mode, so the next app build whose build script reruns fails with `Permission denied (os error 13)`. Reproduced at efd4a55: `node tests/e2e/build.mjs` RC=1 and `cargo build --workspace --tests --features outlooker-core/test-support` RC=101 after one earlier app build (this is the G2.2 → G2.4 order).
- 92dc26f: `makeWritable` over the extracted tree and on the "already present" path (fetch-postgres.mjs:48-57, 86, 108); `--self-test` now also checks a 0444 file becomes owner-writable. My runs: `node --check` RC=0; `--self-test` RC=0 ("read-only files made writable"); `find src-tauri/resources/postgresql -type f ! -perm -u+w | wc -l` = 0; at 92dc26f `node tests/e2e/build.mjs` RC=0, then `cargo build --workspace --locked --tests --features outlooker-core/test-support` RC=0. Blocker closed. Gate note: `target/*/resources` copies made before 92dc26f stay read-only; run `chmod -R u+w src-tauri/target/*/resources` once before G2.2.

## R2-09 — PASS

Commits reviewed: d05c80b (import manager, queue, PST reader, inflight, progress, pipeline tests, import_bench), 1c8b21e (folder-cycle guard).

Pass 1 (d05c80b):
- Ownership: `core/src/importer/{mod,queue,pst_reader,inflight,progress}.rs`, `core/tests/import_pipeline.rs`, `core/examples/import_bench.rs`. All R2-09. OK.
- Acceptance (my runs at e204725): `cargo build -p outlooker-core --locked --features test-support --test import_pipeline --example import_bench` RC=0; `cargo test ... --test import_pipeline` RC=101 with default threads (5 TestCluster initdb panics, shared-memory limit, see header), RC=0 with `-- --test-threads 4` (26 passed). Each acceptance bullet maps to a named test: outlook_pst_import, byte_copy_dedupes_and_removal, corrupt_copy_outcome (pre-registered: Completed, ≥1 skipped parse row, 0 messages), synthetic_cjk / _inline_images / _attachments / _hostile / _message_id_dedupe, index_policy_first_import_drops_and_rebuilds, index_policy_rebuilt_after_cancel_and_disk_failure, index_policy_incremental_keeps_indexes, after_job_vacuum_ran, resume_after_cancel_equals_uninterrupted, resume_after_kill_equals_uninterrupted, mtime_change_resets_cursor, poison_marker_attempts_1_retries_item, poison_marker_attempts_2_skips_item, poison_marker_of_other_job_is_ignored, pg_crash_mid_import_resumes, recover_after_crash_states, pst_list_status_derivation, pst_remove_with_queued_job. `outlook-fixtures` feature tests exist (import_pipeline.rs:27) for the Windows phase.
- Security (untrusted PST data, Part 02 §7.3, Part 04 §11): `check_file` (queue.rs:305-322) re-validates absolute path, metadata, regular file, `!BDN` magic; folder scan uses an explicit stack, no recursion; attachment bytes read only for content-id attachments, capped at 25 MB (pst_reader.rs:20, 176); embedded messages read only their subject; all SQL text constant.
- Findings: none blocking. Folder-cycle nit fixed by the owner in 1c8b21e (visited set in `walk_tree`, unit test folder_cycle_is_skipped_with_one_error_row).
- Regression at 6d1afa4: `--test import_pipeline -- --test-threads 4` RC=0, 26 passed.

## R2-10 — PASS

Commits reviewed: 4a1cbaf (writer C11, removal C12, import_fixtures), 8bfd771 (loadgen, seed, perf gates), bbf944e (cfg gate on writer / removal tests).

Pass 1 (4a1cbaf, 8bfd771):
- Ownership: `core/src/importer/{writer,remove}.rs`, `core/src/test_support/**`, `core/tests/{writer,removal}.rs`, `crates/outlooker-loadgen/**`. 8bfd771 also edits `src-tauri/Cargo.lock` (R2-13): dependency edges of outlooker-loadgen only (image, rayon, serde_json, tempfile, tokio-postgres), no new package; manifest pins `=0.7.18`, `=0.25.10`, `=1.12.0`, `=1.0.154`, `=3.27.0` equal the existing lock entries. Lead accepted the lock edit.
- Writer: one transaction per batch, binary COPY into ON COMMIT DELETE ROWS staging, set-based upserts, `synchronous_commit = off` only in the writer session (writer.rs:234), disk guard before each commit (writer.rs:436-440), checkpoint in the same transaction. Removal: 5,000-occurrence chunks, orphan blobs / addresses, `first_pst_id` set NULL by FK `ON DELETE SET NULL` (V1__schema.sql:61), then VACUUM (ANALYZE).
- Acceptance (my runs): `--test writer --test removal` RC=0 at 4a1cbaf (10 + 2: out_of_order_checkpoint_is_contiguous, message_id_duplicate_adds_occurrence, content_hash_crlf_vs_lf, batch_cut_by_items / _bytes / _time, synchronous_commit_off_only_on_writer, error_rolls_back_data_and_checkpoint, disk_guard_blocks_commit, same_key_twice_in_one_batch; keeps_shared_deletes_rest, second_run_is_noop). At 32ac9a8: `cargo test -p outlooker-loadgen --locked` RC=0 (7 unit + smoke_10k + seeded_root_starts); `cargo run -p outlooker-loadgen -- seed --data-root scripts/tmp/rev2/seedroot --messages 2000` RC=0 (1,971 stored, 2,000 occurrences), then `pg_ctl start` / `stop` on it RC=0 / 0; `cargo test -p outlooker-loadgen --release -- --ignored --list` RC=0, 14 `perf_` tests (10 rows of Part 04 §12.3 + next_page, mail_get_first_image, first_page_p50 + generate).
- Mutation check (lead request; worktree, `cp` backup, restored from the backup, 10/10 green after): M1 writer.rs:474 checkpoint never advances → batch_cut_by_items, batch_cut_by_time, out_of_order_checkpoint_is_contiguous FAILED (RC=101); M2 writer.rs:277 `ORDER BY s.dedupe_key, s.seq DESC` (within-batch winner) → same_key_twice_in_one_batch FAILED (RC=101).
- Findings (pass 1):
  - [blocker, fixed in bbf944e] core/tests/writer.rs:1, core/tests/removal.rs:1 — no `#![cfg(feature = "test-support")]`, so any featureless `cargo test -p outlooker-core ...` (R2-11 acceptance) failed to compile (RC=101, `could not find test_support`).

Pass 2 (bbf944e): both files gated; featureless `cargo build -p outlooker-core --locked --tests` RC=0 at 32ac9a8. Blocker closed.

## R2-11 — PASS

Commits reviewed: 08aeaee (C13 hydrate_rows, constant read SQL), 3376681 (compiler, cursor, two-phase execute, mail_get, image), 39bcf13 (tests), 2570547 (cfg gate).

Pass 1 (08aeaee, 3376681, 39bcf13):
- Ownership: `core/src/search/**`, `core/tests/search.rs`. All R2-11. OK.
- Security (Part 04 §7.3, §11): every `format!` in compile.rs:95-200 and execute.rs:26-52 inserts only `$n` placeholders, AND / OR joiners or the compiled predicate; LIKE values escaped (`%`, `_`, `\`); folder exact match bound as a parameter, prefix escaped; tsvector branch on stored `m.body_tsv`.
- Acceptance (my runs at 39bcf13): `--features test-support --test search` first run RC=101 (5 initdb panics, shared-memory limit), re-run RC=0, 16 passed (compiler, semantics, count_capped, two_phase_dense_phase_a_only, two_phase_sparse_runs_phase_b, two_phase_pages_no_gap_no_overlap, null_sent_at_last_and_paged, reset_after_phase_b, explain_uses_indexes, mail_get_unknown_is_not_found, mail_get_image_urls_platform_form, ...); `--lib search::` RC=0.
- Findings (pass 1):
  - [blocker, fixed in 2570547] core/tests/search.rs:1 — acceptance `cargo test -p outlooker-core search::` (no features) did not compile (RC=101, `could not find test_support`).

Pass 2 (2570547 + bbf944e): at 32ac9a8 `cargo test -p outlooker-core --locked search::` RC=0 (26 lib tests, every integration target compiled). Blocker closed. Regression at 6d1afa4: `--test search -- --test-threads 4` RC=0, 16 passed.

## R2-12 — PASS

Commits reviewed: a1630b9 (SQL Console, Maintenance, tests), ea8ae95 (detach a connection that fails to reset).

Pass 1 (a1630b9):
- Ownership: `core/src/sql_console/{mod,execute,saved}.rs`, `core/src/maintenance/mod.rs`, `core/tests/{sql_console,maintenance}.rs`. All R2-12. OK.
- Read-only boundary (Part 02 §7.2, Part 04 §8.1): user SQL runs only on the `outlooker_ro` pool (mod.rs:125) inside `BEGIN READ ONLY` + `SET LOCAL statement_timeout = '30s'` (mod.rs:26-27, 159-160); single statement proved by extended-protocol `prepare` (mod.rs:161); `ROLLBACK` + `DISCARD ALL` after every execution; SELECT-only grants on six views (R1-04). The role can lift its session default (`SET default_transaction_read_only = off`, `SET TRANSACTION READ WRITE`), so the explicit `BEGIN READ ONLY` per execution plus the grants are the boundary; a user `COMMIT` ends only its own execution. SQLSTATE table equals Part 04 §8.1 (execute.rs:28-37).
- Mutation check: mod.rs:26 `BEGIN READ ONLY` → `BEGIN READ WRITE` turns bypass_attempts_all_fail red (RC=101): `ALTER ROLE outlooker_ro SET work_mem = '1GB' must fail, got Ok(Command)`. So the read-only transaction is the only layer that stops the role from changing its own session defaults, and a test observes it. Restored from the `cp` backup; 6/6 green after.
- Maintenance: gate `Maintenance` (maintenance/mod.rs:94), admin connection, constant index names for REINDEX CONCURRENTLY one at a time.
- Acceptance (my runs at 38be932): `--test maintenance` RC=0 (reindex_leaves_all_valid, vacuum_removes_orphans, maintenance_refused_while_import_holds_gate); `--test sql_console -- --skip read_only_role_rows` RC=0 (6: bypass_attempts_all_fail, cancel_during_sleep, max_rows_limits, mode_detection, one_execution_at_a_time, saved_queries_crud); `--lib sql_console::` RC=0 (3).
- Not run by me: read_only_role_rows (its 40 s server-wait row needs > 30 s; my foreground cap is 30 s). Evidence: implementer log `scripts/tmp/r212-test.log` lines 20, 22 at a1630b9 (`test read_only_role_rows ... ok`, 7 passed, 31.74 s). This is implementer-produced evidence; gate G2.2 re-runs it.
- Findings: none blocking (see parking lot). Owner fixed the reset item in ea8ae95.

Pass 2 (ea8ae95): `deadpool_postgres::Object::take` drops the connection when `ROLLBACK` or `DISCARD ALL` fails (deadpool 0.13.1 in Cargo.lock). My run: `--test sql_console -- --skip read_only_role_rows` RC=0, 7 passed incl. killed_backend_is_replaced_by_a_fresh_connection. Regression at 6d1afa4: same RC=0. read_only_role_rows after ea8ae95 rests on the implementer's run and gate G2.2.

## R2-13 — PASS

Commits reviewed: 1443f96 (wdio plugin under feature e2e), 0f581b3 (event emitter, mailimg), f8c8372 (core app composition + app_compose), 9b53d24 (command layer), 38be932 (App Shell, bundle resources), 3ca849a (test and comment fix).

Pass 1:
- Ownership: `src-tauri/src/**`, `src-tauri/tauri.conf.json`, `core/src/app/mod.rs`, `core/tests/app_compose.rs`. All R2-13. OK.
- Security (Part 02 §7.1 layer 3, Part 03 §5.6): CSP (tauri.conf.json:17) `script-src 'self'`, `img-src 'self' data: mailimg: http://mailimg.localhost`, `frame-src 'self'`, `connect-src ipc: http://ipc.localhost`, no remote host; `style-src 'unsafe-inline'` is the accepted deviation C.2-3. mailimg.rs: GET only (else 405), path must be all ASCII digits and > 0 (else 400), 404 when absent, 200 `image/webp` + `max-age=31536000, immutable`, 503 while the DB is not ready; it reads bytes from `image_blobs` only, never a file path. The e2e app-data override exists only under `cfg(feature = "e2e")` (lib.rs:19-27).
- Acceptance (my runs at 3ca849a): `cargo test -p outlooker --locked` RC=0, 11 passed (generate_handler_list_equals_commands, single_instance_plugin_is_first, throttle_at_most_four_per_second_per_key, throttle_always_emits_trailing_value, throttle_emits_phase_change_immediately, job_end_flushes_library_changed, mailimg 405 / 400 / 404 / 200 / 503); `--features test-support --test app_compose` RC=0, 3 passed (shutdown_during_import_requeues_job asserts Queued not Cancelled, < 45 s, `PgStatus::Stopped`, job and checkpoint rows after reopen; start_after_kill_recovers_jobs_then_heals_then_starts_worker; missing_index_is_rebuilt_before_worker); `cargo build --release -p outlooker --locked` RC=0; `cargo tree -p outlooker -e normal | grep -c wdio` printed 0 (control: with `--features e2e` it prints 1); `git diff --exit-code src/api/bindings` after `--lib dto::` RC=0.
- Not run by me: `npm run tauri build` (.dmg) — gate G2.5.
- Deviations judged (lead request):
  1. app_compose.rs:180 drops `import_jobs_one_running` (V1__schema.sql:185) in setup only, to hold one `running` and one `cancelling` job at once; recovery does not depend on that index. The requeued job points at a missing file, so its `Failed` event proves the worker ran, and the probes (app_compose.rs:207-208) prove it ran after the index heal. The test still checks Part 02 §2.2 step 8 order and §2.5 recovery. Accepted; nit in the parking lot.
  2. `.dmg` bundling needs `CI=true` (Finder AppleScript step hangs headless) — gate note for G2.5.
  3. Read-only PostgreSQL files — routed as a blocker to the fetch-script owner (R2-08) and fixed in 92dc26f (see R2-08).
  4. Orphan cleanup by impl-r213: 8 postmasters with PPID 1 got SIGINT; their data dirs were all `/var/folders/.../T/.tmp*/data/pgdata` and their binaries this repo's (3 under `scripts/tmp/pgbin`, 5 under `src-tauri/resources/postgresql`); they include the 5 live clusters I listed at 03:39 (creator PIDs 29128, 32576, 33755, 72774, 77420). PPID 1 means the parent test process was gone, so no running teammate test lost its cluster. About 31 SysV segments with NATTCH 0 were removed with `ipcrm`; no process was attached, but some (ctime 03:16) may belong to other projects and the list was not saved. At 04:08: `pgrep -x postgres` 0, `ipcs -m` 0 segments. Parking-lot item on cleanup practice.

## R2-14 — PASS

Commits reviewed: c924166 (wdio harness, embedded tauri driver, smoke), efd4a55 (flow and seeded groups), 6d1afa4 (scroll spec committed).

Pass 1 (c924166, efd4a55):
- Ownership: `package.json` (scripts only), `tsconfig.json` (exclude tests/e2e), `wdio.conf.ts`, `tests/e2e/**`. All R2-14. No dependency change; `grep -nE '"[\^~]' package.json` RC=1.
- E2E design: `@wdio/tauri-service` `driverProvider: 'embedded'`; app binary built with cargo feature `e2e` and `VITE_E2E=1`; every group uses its own app-data dir under `scripts/tmp/e2e/` through `OUTLOOKER_APP_DATA_DIR` (feature-gated in lib.rs), so the user's real app data is never touched.
- Acceptance (my runs at efd4a55, worktree with linked node_modules): `npm run typecheck` RC=0; `npm test` RC=0 (16 files, 68 tests); `npm run build` RC=0; `tsc --noEmit -p tests/e2e` RC=0. At 92dc26f: `node tests/e2e/build.mjs` RC=0; `node tests/e2e/run.mjs --spec tests/e2e/flow.e2e.ts` RC=0 (5 passing: onboards, imports outlook.pst to Completed, finds Welcome and previews it with `sandbox=""`, Advanced SQL mail-list, removes the PST); `--spec tests/e2e/seeded.e2e.ts` RC=0 (1 passing: 報價單 search, `mailimg://localhost/<id>` image decodes with naturalWidth > 0).
- Findings (pass 1):
  - [blocker, fixed in 6d1afa4] tests/e2e/prepare.mjs:16 — GROUPS lists `scroll` but `tests/e2e/scroll.e2e.ts` was untracked, so `npm run e2e` on the committed tree failed at that group.

Pass 2 (6d1afa4): `tests/e2e/scroll.e2e.ts` committed; `git status --porcelain tests/e2e` clean. Blocker closed.
- Not run (needs-user-decision, lead ruling): E2E group 3, the 100,000-row scroll test on a 120,000-message seed — longer than any member's foreground limit; it needs a user-approved long run (gate G2.4). Until it runs, the third acceptance group of `npm run e2e` is unverified.

## Security — PASS

Scope: Part 02 §7, Part 04 §11 on the Round 2 head 6d1afa4 (+92dc26f).
- SQL from constant fragments: `grep -rnE 'format!\(' core/src` filtered for SQL words lists 14 sites; each inserts only placeholders, compiled predicates, constant index names / DDL (db/indexes.rs:44, 55, 61-62), `saved.rs` constant column list, the integer FETCH count, or (by design, Part 04 §8.1) the user statement after `DECLARE outlooker_c NO SCROLL CURSOR FOR` on the read-only role inside `BEGIN READ ONLY` after a single-statement `prepare`. maintenance/mod.rs:71 interpolates `_ccnew` index names read from the catalog (nit, parking lot).
- Read-only boundary: see R2-12 (ro pool, `BEGIN READ ONLY` each execution, SELECT-only grants, `ROLLBACK` + `DISCARD ALL`, mutation check observed by a test).
- Untrusted mail HTML: sanitizer (`importer/normalize`) and `MailPreview` unchanged in Round 2 (`git diff --stat fa20e00 6d1afa4` empty for both); iframe `sandbox=""`, `srcDoc`, `referrerPolicy="no-referrer"` (MailPreview.tsx:14); CSP and mailimg as in R2-13; `search/mail.rs` rewrites `outlooker-inline:<n>` only to `mailimg://localhost/<integer blob id>`.
- Path re-validation: PST paths (queue.rs:305-322), `app_initialize` target absolute + empty (lifecycle/init.rs:48-54), data-dir move target absolute + empty + not inside the data root (move_dir.rs:90-100).
- Credentials: unchanged from Round 1 (0600 file, admin only for migrations / maintenance / index DDL).

## Round 1 regression — PASS

§7.1 gate commands on the Round 2 head 6d1afa4 in the review worktree (92dc26f for the build after the resource fix):
- G1.1 `node scripts/fetch-postgres.mjs` RC=0 ("already present"); `bin/postgres` exists.
- G1.2 ORIGIN `bb5bcc0`, `outlook.pst`, NOTICE `CC-BY-4.0`: RC 0 / 0 / 0.
- G1.3 `cargo build --workspace --locked` RC=0.
- G1.4 run split per target (foreground cap) with `--workspace --locked --features outlooker-core/test-support`: `--lib --bins` RC=0 (outlooker 11, outlooker-core 111, loadgen 7); `--test outlook_pst --test db_schema --test pg_lifecycle --test writer --test removal -- --test-threads 4` RC=0 (6, 8, 8, 10, 2); `--test import_pipeline --test search` RC=0 (26, 16); `--test lifecycle --test maintenance --test app_compose` RC=0 (17, 3, 3); `--test smoke --test perf --test sql_console -- --skip read_only_role_rows` RC=0 (2, 0 + 14 ignored, 7); `--doc` RC=0. 0 failed in every target. read_only_role_rows: see R2-12.
- G1.5 `git diff --exit-code src/api/bindings` RC=0.
- G1.6 `npm run typecheck`, `npm test`, `npm run build` RC=0 at efd4a55 (6d1afa4 adds only an E2E spec). `npm ci` not run: the worktree links the main tree's node_modules and `npm ci` would delete it; gate G2.1 runs it.
- G1.7 dark-theme grep RC=1 (no file). G1.8 placeholder grep over src, src-tauri/src, src-tauri/crates RC=1 (no match); also none in tests/e2e, wdio.conf.ts.
- Versions: no new Cargo or npm dependency in Round 2 (`git diff --stat fa20e00 6d1afa4` on manifests: package.json scripts only, Cargo.lock loadgen edges only); no forbidden crate in Cargo.lock (exact-name grep RC=1).

## Parking lot

- [should-fix] gate / test infrastructure — TestCluster-heavy test binaries with the default 16 threads exceed macOS `kern.sysv.shmmni` = 32 together with other clusters; initdb then fails. Fix: run G2.2 with `-- --test-threads 4`, or add a process-wide semaphore around `TestCluster::start` (core/src/test_support/mod.rs:20).
- [should-fix] test processes killed by the foreground time limit skip `TestCluster`'s Drop and leave postmasters with PPID 1 plus SysV segments (the 8 orphans of R2-13 item 4). Also an E2E run leaves the app's cluster running after wdio ends: after my flow and seeded runs two postmasters stayed up (`scripts/tmp/rev2-wt/scripts/tmp/e2e/{fresh/data,seeded-root}/pgdata`; I stopped them with `pg_ctl stop -m fast`). Fix: run.mjs stops the group's cluster after each `wdio run`; tests write the cluster dir to a list a cleanup script can stop.
- [should-fix] cleanup practice — removing SysV segments or killing processes needs a saved target list (PID, data dir, segment id, ctime) first; some removed segments may have belonged to other projects.
- [should-fix] core/tests/lifecycle.rs:195 — restart_budget_exhaustion_fails asserts only `error.is_some()`; acceptance asks for the last PG log line in `detail` (health.rs:60 does it). Fix: assert the detail equals `last_log_line` of the pgdata log.
- [should-fix] core/tests/lifecycle.rs:233 — move_verify_failure_rolls_back compares file count + bytes of `auth/` only, not the old root. Fix: compare the whole root excluding `pgdata/log`, `postmaster.*` and stats files.
- [should-fix] core/tests/sql_console.rs:54 — bypass rows assert only `is_err()`; a row can pass for an unrelated error. Fix: assert `ReadOnlyViolation` or `SqlError{42501}` per row.
- [should-fix] core/src/sql_console/mod.rs:173-191 — FETCH returns up to 10,001 full rows into memory before cells are cut to 2,000 chars; a query of huge text values can exhaust memory. Fix: wrap the user query in a select that truncates text columns, or fetch in chunks.
- [should-fix] frontend — no specific locale text for `invalidInput{field:"target", reason:"insideDataRoot"}` (new in 80208cb); check `formatAppError` shows a useful message on the Settings move dialog.
- [nit] core/src/importer/writer.rs:437 — `free_space(..).unwrap_or(0)` reports a volume-probe error as `insufficientSpace`. Fix: return the probe's `fileAccess` error.
- [nit] core/src/importer/remove.rs:36-38 — SQLSTATE 53100 during removal maps to `fileAccess{path:""}`. Fix: pass the data root.
- [nit] core/src/importer/pst_reader.rs:154-156 — HTML / RTF / plain bodies are read whole before the 256 MB byte budget applies. Fix: check the body size property first and skip oversized bodies as `item_skipped`.
- [nit] core/src/importer/pst_reader.rs (walk_tree) — the folder-cycle error row uses stage `parse`; other scan problems use `scan`.
- [nit] core/src/maintenance/mod.rs:71 — `DROP INDEX CONCURRENTLY {name}` with a catalog name; quote it as an identifier.
- [nit] core/src/lifecycle/pg.rs try_candidate — two statements joined on one line (`connect(...).await?;            postgres.batch_execute(...)`); run rustfmt.
- [nit] core/tests/app_compose.rs:180 — the kill test edits the schema (drops `import_jobs_one_running`); two kill scenarios (one running, one cancelling) would avoid it.
- [nit] tests/e2e/seeded.e2e.ts — the image is decoded in the main document, not inside the sandboxed iframe, so the test does not prove the frame itself can load `mailimg`.
- [nit] src-tauri/tauri.conf.json:22 — bundle `targets` changed from `["dmg", "nsis"]` to `"all"`; on Windows this also builds an MSI (WiX).
- [gate note] G2.5 `npm run tauri build` needs `CI=true`. G2.2 / G2.4: run `chmod -R u+w src-tauri/target/*/resources` once for copies made before 92dc26f. G2.4 scroll group needs the user's long-run approval.
