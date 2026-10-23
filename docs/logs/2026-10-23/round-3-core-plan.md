# Round 3 (final) — squad core plan

Branch `techdebt/r3-core`, base `bf65fcd` (integration). Worktree `/Users/jhangyu/project/outlooker-wt/r3-core`.
Contract: `contract-techdebt.md`. Binding rules: `round-brief.md`. Paths below: core = `src-tauri/crates/outlooker-core/`, loadgen = `src-tauri/crates/outlooker-loadgen/`.

## Re-verified slice (at bf65fcd)

| item | evidence today |
|---|---|
| C2 | `importer/queue.rs` 1105 lines; `attempt` 199 lines (`queue.rs:848-1046`, flagged by `scripts/tmp/fnlen.py 80`); commands :188-577, runner :579-1050 |
| C20 | end-of-attempt decision inline in `run_import` `queue.rs:792-828` (Crashed / Shutdown→Queued / finalize / Failed-Cancelled-Completed) |
| C13 rest | `pub struct Writer;` `writer.rs:307`, `Writer::spawn` callers: `queue.rs:955`, `test_support/import_fixtures.rs:114`, loadgen `src/load.rs:79` (missed by the round-2 handoff), `tests/writer.rs:21` (+ comment `:115`, `writer.rs:203`, `import_fixtures.rs:101`); `"data"` twice in `paths/mod.rs:14,26` |
| C10 | `db/mod.rs:6` re-exports `lifecycle::pg::connect`; `db/mod.rs:11` imports `ADMIN_ROLE`, `CJK_SELF_CHECK`; `db/indexes.rs:2` imports `total_ram_bytes`; `writer.rs:21` imports `move_dir::free_space`; `tokio_postgres::Config::new()` twice (`pg.rs:110`, `db/pools.rs:12`) |
| C15 rest | `pg.rs:279` two statements on one line (the only part of C15 left) |
| A9 | `Throttle`, `ThrottleKey`, `throttle_key`, `phase`, `ends_job`, `Slot` + 4 tests in `src-tauri/src/events.rs` |
| A11 | loadgen `src/perf.rs` (166 lines) used only by `tests/perf.rs:17`; it is the only loadgen src file using `tokio_postgres` |
| A8 core part | `tests/perf.rs:2` and `src/perf.rs:41` say `cargo test -p outlooker-loadgen`; `tests/perf.rs:94` runs `cargo run --release -p outlooker-core --features test-support --example import_bench` |
| pff message | `crates/pff/build.rs:27` names `scripts/vendor-libpff.sh`; fixed decision: `scripts/vendor-libpff.py` |

## Decisions

- C2 test seams: they already sit in one public struct, `ImportTuning` (`importer/mod.rs:26`; 31 references in tests/app). It is not restructured. The split moves `Inner::hold_used` and every seam read (`hold_reader_after`, `crash_after_batches`, `on_batch`, `Wake::Crash`, `AttemptEnd::Crashed`) into `pipeline.rs`, so the seams live in one file next to the code they bend. The cfg synthetic reader (`SYNTHETIC`, `synthetic_items`, `enqueue_synthetic` helpers) stays together in `queue.rs` (it feeds `enqueue`).
- C2 module boundaries: `queue.rs` = state (`Inner`, `JobCancel`, `CancelReason`, `RunningJob`), `JobKind`/`JobState`/`status_of`, SQL commands, list, errors, recovery, `start_worker`/`shutdown`, synthetic source. `runner.rs` = `worker`, `next_job`, `run_job`, `run_removal`, `run_import`, `set_final`, `pause_wait`, `finalize`, the C20 decision function. `pipeline.rs` = one attempt: identity, checkpoint, poison marker, reader, scan rows, index policy, writer, normalize thread, wake select, teardown, as named step functions.
- C10 ordering: `sys.rs` is created entirely by Package 2 (both `free_space` and `total_ram_bytes`). The only file both packages need is `importer/writer.rs` (Package 2: the `use` line 21; Package 1: `Writer` deletion). Ownership of `writer.rs` is Package 2's until Package 2 sends SETTLED for C10, then Package 1's. Package 1 starts with C2 (no `writer.rs` edit), so neither package idles.
- Shared tree: both implementers compile the same tree. Keep it compiling between edit bursts. A compile error in a file you do not own = message its owner and retry later; never edit it.

## Packages

### Package 1 — `r3-core-impl-1-opus` (judgment: C2, C20; plus mechanical C13 `Writer`)

Exclusive files: core `src/importer/queue.rs`, new `src/importer/runner.rs`, new `src/importer/pipeline.rs`, `src/importer/mod.rs`, `src/test_support/import_fixtures.rs`, `tests/writer.rs`, `tests/import_pipeline.rs` (`use` lines only, if any), loadgen `src/load.rs`; core `src/importer/writer.rs` only after Package 2's C10 SETTLED.

1. C2: split as in Decisions; no behavior change (same SQL, same event order, same `AttemptEnd` outcomes).
2. C20: the end-of-attempt decision becomes one pure function over `(AttemptEnd kind, Option<CancelReason>, failure present)` returning the outcome (crash drop / requeue / finalize then final `(JobState, ImportStatus)`); a table-driven unit test named `end_of_attempt_table` covers every row.
3. C13: delete `pub struct Writer;`; `impl WriterHandle { pub async fn spawn(..) }` replaces `Writer::spawn`; update the 4 call sites and the 3 comments.

Acceptance:
- `wc -l core/src/importer/queue.rs` ≤ 650.
- `python3 scripts/tmp/fnlen.py 80 core/src/importer/{queue,runner,pipeline}.rs` exits 0 (today it flags `attempt`, 199 lines).
- `cargo test --workspace --lib end_of_attempt_table -- --test-threads 4` → 1 passed.
- `grep -rnE "struct Writer;|Writer::spawn" src-tauri/crates src-tauri/src` → 0.
- `git diff bf65fcd -- core/tests/import_pipeline.rs` shows `use` lines only (or nothing).
- Gate commands 14–16 (import_pipeline 26/26) and the full gate exit 0.

### Package 2 — `r3-core-impl-2-sonnet` (mechanical, 4 items)

Exclusive files: core `src/lifecycle/pg.rs`, `src/lifecycle/move_dir.rs`, `src/db/mod.rs`, `src/db/pools.rs`, `src/db/indexes.rs`, new `src/sys.rs`, `src/lib.rs`, `src/sink.rs`, new `src/sink/throttle.rs`, `src/paths/mod.rs`, `src/importer/writer.rs` (line 21 only, until SETTLED), `tests/pg_lifecycle.rs`; `src-tauri/src/events.rs`; loadgen `Cargo.toml`, `src/lib.rs`, `src/perf.rs` (deleted), `tests/perf.rs`, new `tests/perf/` support module; `src-tauri/Cargo.lock` (cargo-written only); `src-tauri/crates/pff/build.rs`.

1. C10 (do first, commit, send SETTLED): `connect`, `ADMIN_ROLE`, `CJK_SELF_CHECK` move into `db/` (delete the re-export `db/mod.rs:6`; `pg.rs` and `tests/pg_lifecycle.rs` import from `db`); one base `tokio_postgres::Config` builder used by `connect` and `db/pools.rs`; new `sys.rs` holds `total_ram_bytes` and `free_space` (callers `pg.rs`, `db/indexes.rs`, `move_dir.rs`, `writer.rs:21`, `tests/pg_lifecycle.rs`). Split `pg.rs:279` onto two lines (C15 rest).
2. A9: `Throttle`, `ThrottleKey`, `throttle_key`, the window constants, `phase`, `ends_job`, `Slot` and their 4 tests move to core `sink/throttle.rs` (declared `mod throttle` from `sink.rs`, re-exported as needed); `TauriSink` stays in `events.rs`.
3. A11 + A8 core part: loadgen `src/perf.rs` moves under `tests/perf/` as a module of `tests/perf.rs` (gate command 20 still `--test perf`); `tokio-postgres` becomes a loadgen dev-dependency; messages say `cargo test --workspace --release --test perf -- --ignored --test-threads 1 <name>`; `perf_e2e_import` runs `cargo build --release --workspace --example import_bench` and then the built binary (no `-p`, no `--features`). Check the target resolves once with `cargo build --workspace --example import_bench` (debug, ≤150 s).
4. One-liners: `crates/pff/build.rs:27` names `scripts/vendor-libpff.py`; `Paths::new` uses `default_data_root` (C13 rest).

Acceptance:
- `grep -rn "lifecycle::" core/src/db core/src/importer` → 0.
- `grep -rn "fn total_ram_bytes\|fn free_space" src-tauri/crates` → 2 hits, both in `core/src/sys.rs`.
- `grep -rn "Config::new()" core/src` → 1.
- `grep -n "struct Throttle" src-tauri/src/events.rs` → 0; `cargo test --workspace --lib throttle -- --test-threads 4` runs the 4 moved tests, all pass.
- `ls loadgen/src/perf.rs` → missing; `grep -rnE "\-p outlooker|--features" loadgen` → 0; `grep -n tokio-postgres loadgen/Cargo.toml` → only under `[dev-dependencies]`.
- `grep -c "vendor-libpff.sh" crates/pff/build.rs` → 0; `grep -c '"data"' core/src/paths/mod.rs` → 1.
- Full gate exits 0.

Why not 3 packages: C2 + C20 are one coupled refactor (the decision function is cut out of `run_import` during the split); C13 `Writer` is a 4-call-site rename that lands in files the split already owns. Package 1 is at its limit (2 judgment items + 1 small mechanical); `Paths::new` moved to Package 2 for that reason.

## Gate commands

Run from `<worktree>/src-tauri` with `export CARGO_TARGET_DIR=/Users/jhangyu/project/outlooker/src-tauri/target`. Logs to `<worktree>/scripts/tmp/verify/` (gitignored; never `/tmp`, never `git add`). One command per Bash call, each ≤ 40 s. Warm-up: `cargo test --workspace --no-run` until RC=0. Each log ends with `RC=$?` captured right after the command.

1. `cargo clippy --workspace --all-targets -- -D warnings`
2. `cargo test --workspace --lib -- --test-threads 4`
3. `cargo test --workspace --doc`
4. `cargo test --workspace --test app_compose -- --test-threads 4`
5. `cargo test --workspace --test db_schema -- --test-threads 4`
6. `cargo test --workspace --test maintenance -- --test-threads 4`
7. `cargo test --workspace --test removal -- --test-threads 4`
8. `cargo test --workspace --test writer -- --test-threads 4`
9. `cargo test --workspace --test lifecycle -- --test-threads 4`
10. `cargo test --workspace --test pg_lifecycle -- --test-threads 4`
11. `cargo test --workspace --test search -- --test-threads 4`
12. `cargo test --workspace --test sql_console -- --test-threads 4 --skip read_only_role_rows`
13. `cargo test --workspace --test sql_console read_only_role_rows -- --test-threads 4`
14. `cargo test --workspace --test import_pipeline -- --test-threads 4 synthetic_ outlook_pst_import enqueue_validation pst_`
15. `cargo test --workspace --test import_pipeline -- --test-threads 4 poison_marker retry_rewrites mtime_change corrupt_copy byte_copy after_job`
16. `cargo test --workspace --test import_pipeline -- --test-threads 4 index_policy resume_after pg_crash recover_after`
17. `cargo test --workspace --test smoke smoke_10k -- --test-threads 4`
18. `cargo test --workspace --test smoke seeded_root_starts -- --test-threads 4`
19. `cargo test --workspace --test outlook_pst -- --test-threads 4`
20. `cargo test --workspace --test perf -- --test-threads 4` (all ignored; compiles only)

Coverage: 14+15+16 = 26/26 `import_pipeline`, 12+13 = 8/8 `sql_console`, 17+18 = 2/2 `smoke`. Lib test count rises by 4 (throttle moved from the app crate's lib to core's lib; the app crate's lib count drops by 4) plus 1 (`end_of_attempt_table`).
Frontend gates are the ui squad's; core touches no frontend file.

Gate window: implementers commit and send SETTLED to the core squad architect (its name is in the spawn prompt); the lead announces the window to the orchestrator (reviewer runs no cargo during it); `r3-core-test-haiku` runs 1–20 and reports per-command RC; the lead closes the window after the runner confirms.

## Baseline (at bf65fcd)

20/20 RC=0, logs `scripts/tmp/verify/base-00..20-*.txt` (gitignored): clippy clean; lib 11 + 122 + 7; doc 1; app_compose 3; db_schema 9; maintenance 3; removal 2; writer 10; lifecycle 18; pg_lifecycle 8; search 16; sql_console 7 + 1; import_pipeline 9 + 9 + 8 = 26; smoke 1 + 1; outlook_pst 6; perf 14 ignored.

Environment findings (embed in prompts):
- The pff build script copies, configures and compiles libpff from scratch every time it runs (A1). It runs once per worktree for the build/test units and once more for clippy. A run killed by the 40 s cap loses all of its progress, so 40 s retries never converge under load (load average was 23: other sessions compile on this machine). Recipe: `cargo build --workspace` (150 s cap; took 2 min 03 s) first, then `cargo test --workspace --no-run`; clippy passed in 34.5 s once load fell to 11.
- Package 2's edit of `crates/pff/build.rs` (`rerun-if-changed=build.rs`) re-triggers that full pff rebuild in both units. Package 2 lands the pff one-liner first, then runs `cargo build --workspace` once and clippy once, and tells Package 1 when the rebuild has passed.

## Final ledger (every C*/A* item in core ownership)

Status at plan time; `round-3-core-summary.md` carries the final version with round-3 commit hashes.

| item | status |
|---|---|
| C1 one DB error policy | fixed R1 `fe101cf`, `d139fbf` |
| C2 queue.rs split | round 3 Package 1 |
| C3 library counts | step 1 fixed R1 `fe101cf`; step 2 (count cache) not worth fixing: adds cached state for an unmeasured cost |
| C4 image codecs | fixed R1 `71a2a89` |
| C5 DB text ↔ enum | fixed R2 `632058a` |
| C6 ammonia Builder per message | not worth fixing: cost unmeasured; `import_bench` needs a `--release` build over the 150 s cap |
| C7 snippet/clean_text/ImageIn allocations | not worth fixing: same reason as C6 |
| C8 atomic writer | fixed R2 `616dd92`, `01ffc72`, `e9aafed`; `AppError::io` not worth fixing (argument sugar, no drift risk) |
| C9 poison-tolerant locks | fixed R2 `616dd92` |
| C10 db ↔ lifecycle layering | round 3 Package 2 |
| C11 reindex list | fixed R2 `616dd92`, `01ffc72` |
| C12 data-dir move rollback | fixed R2 `632058a` |
| C13 dead abstractions | `is_connected` fixed R1 `fe101cf`; `WriterHandle::submit` and `NullSink` refuted R1 (used by tests); `Writer` round 3 Package 1; `Paths::new` round 3 Package 2 |
| C14 timeout constants | fixed R1 `fe101cf` |
| C15 newest log scan | fixed R1 `71a2a89`; `pg.rs:279` line split round 3 Package 2; rustfmt gate not worth adding (no fmt gate in repo, new infrastructure) |
| C16 truncate_to_micros | fixed R1 `fe101cf` |
| C17 poison marker writes | not worth fixing: unmeasured, marker design deliberate |
| C18 enqueue double LIST_SQL | fixed R2 `632058a` |
| C19 shared test helpers | fixed R2 `616dd92`; `webp.rs` test png kept (lib tests must not need `test-support`) |
| C20 end-of-attempt table test | round 3 Package 1 |
| C21 test seams on PgController/Supervisor | not worth fixing: one uncontended lock; cfg-gating needs feature plumbing for an integration test |
| C22 free_space per batch | not worth fixing: unmeasured; unknown volume → 0 is a deliberate fail-closed guard (`writer.rs:390`) |
| A1 pff `.prepared` | fixed R1 `71a2a89` |
| A2 unused serde | fixed R1 `71a2a89` |
| A3 workspace dependencies | fixed R1 `71a2a89` |
| A7 `outlook-fixtures` / `make-pst.ps1` | kept by user decision (Windows phase W2); sh→py port is the ui squad's; core part = pff message, round 3 Package 2 |
| A8 banned cargo flags | core part (loadgen perf messages, `perf_e2e_import`) round 3 Package 2; `prepare.mjs` part is the ui squad's (now `--workspace`) |
| A9 Throttle into core | round 3 Package 2 |
| A10 source-text handler test | not worth fixing: works; a rewrite is churn |
| A11 loadgen perf in src | round 3 Package 2 |
| A12 loadgen argv loop | not worth fixing: internal tool |
| A13 version in tauri.conf.json | fixed R2 `632058a` |
| A14 `bundle.targets: all` | not worth fixing: no Linux build exists; needs a user decision on shipped platforms |
| A16 Drop ignores close results | not worth fixing: unactionable in `Drop` |
| A17 no CI | not worth fixing in this cleanup: new infrastructure (report to the user) |
| A4, A5, A6, A15 | ui squad ownership (`package.json`, `capabilities/`, `tsconfig`, `scripts/`) — in the ui ledger |
