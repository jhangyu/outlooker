# Round 3 (final) — squad core summary

Branch `techdebt/r3-core`, base `bf65fcd`, code head `6c0c033`. Plan: `round-3-core-plan.md` (`b03f602`).

## Commits
- `4f5253c` C10: `db` owns `connect`, `base_config`, `ADMIN_ROLE`, `CJK_SELF_CHECK`; new `sys.rs` owns `total_ram_bytes` and `free_space`; one `tokio_postgres::Config::new()`; `pg.rs` line split (C15 rest); pff `build.rs` message names `scripts/vendor-libpff.py`.
- `e148989` A9 `Throttle` and helpers + 4 tests in core `sink/throttle.rs` (`TauriSink` stays in `src-tauri/src/events.rs`); A11 + A8 loadgen perf helpers moved to `tests/perf/support.rs`, `tokio-postgres` dev-dependency only, no `-p`/`--features` in messages or commands, `perf_e2e_import` runs `cargo build --release --workspace --example import_bench` then the binary; C13 `Paths::new` uses `default_data_root`.
- `1b44c47` reviewer nit: the `import_bench` path carries `EXE_SUFFIX`.
- `f6a6d6c`, `e084b6f` C2: `importer/queue.rs` 1105 → 609 lines; `runner.rs` (worker, job run, removal, final state, pause wait, finalize) and `pipeline.rs` (one attempt as named steps; every test-seam read); no function over 80 lines. C20: pure `runner::end_of_attempt` + `end_of_attempt_table` (16 rows).
- `6c0c033` C13: `WriterHandle::spawn` replaces the unit struct `Writer` (5 files incl. loadgen `src/load.rs`).

## Visible behavior changes
None. Behavior-preserving refactors; no test assertion changed; `tests/import_pipeline.rs` untouched.

## Gate (at `6c0c033`)
20/20 RC=0, artifacts `r3-core/scripts/tmp/verify/gate-00..20-*.txt` (gitignored; `gate-00-head.txt` holds the HEAD). Lib 7 + 127 + 6 (= 140, baseline 11 + 122 + 7: 4 throttle tests moved app → core, `end_of_attempt_table` added, loadgen `percentile` test moved into `--test perf`); import_pipeline 9 + 9 + 8 = 26/26; sql_console 7 + 1; smoke 1 + 1; perf 1 passed + 14 ignored. No leftover postmasters. Baseline at `bf65fcd`: `base-00..20-*.txt`, 20/20 RC=0.

## Known limitations
- `perf_e2e_import` and the `perf_*` gates are compiled only; they need a `--release` build and a real PST on the reference machine.
- `tests/import_pipeline.rs:991` (`outlook-fixtures`) is still compiled by no allowed command (Windows phase W2).
- Gate commands near the 40 s cap: `sql_console read_only_role_rows` (31 s). The pff build script restarts from scratch when killed; a fresh worktree needs `cargo build --workspace` (150 s cap) before clippy/tests under load.

## Final ledger (every C*/A* item in core ownership)

| item | status |
|---|---|
| C1 one DB error policy | fixed R1 `fe101cf`, `d139fbf` |
| C2 queue.rs split | fixed R3 `f6a6d6c`, `e084b6f` |
| C3 library counts | step 1 fixed R1 `fe101cf`; step 2 (count cache) not worth fixing: cached state for an unmeasured cost |
| C4 image codecs | fixed R1 `71a2a89` |
| C5 DB text ↔ enum | fixed R2 `632058a` |
| C6 ammonia Builder per message | not worth fixing: cost unmeasured; `import_bench` needs a `--release` build over the 150 s cap |
| C7 snippet/clean_text/ImageIn allocations | not worth fixing: same reason as C6 |
| C8 atomic writer | fixed R2 `616dd92`, `01ffc72`, `e9aafed`; `AppError::io` not worth fixing (argument sugar, no drift risk) |
| C9 poison-tolerant locks | fixed R2 `616dd92` |
| C10 db ↔ lifecycle layering | fixed R3 `4f5253c` |
| C11 reindex list | fixed R2 `616dd92`, `01ffc72` |
| C12 data-dir move rollback | fixed R2 `632058a` |
| C13 dead abstractions | `is_connected` fixed R1 `fe101cf`; `Writer` fixed R3 `6c0c033`; `Paths::new` fixed R3 `e148989`; `WriterHandle::submit` and `NullSink` refuted R1 (used by tests) |
| C14 timeout constants | fixed R1 `fe101cf` |
| C15 newest log scan | fixed R1 `71a2a89`; `pg.rs` two-statement line fixed R3 `4f5253c`; rustfmt gate not worth adding (new infrastructure) |
| C16 truncate_to_micros | fixed R1 `fe101cf` |
| C17 poison marker writes | not worth fixing: unmeasured, marker design deliberate |
| C18 enqueue double LIST_SQL | fixed R2 `632058a` |
| C19 shared test helpers | fixed R2 `616dd92`; `webp.rs` test png kept (lib tests must not need `test-support`) |
| C20 end-of-attempt table test | fixed R3 `e084b6f` |
| C21 test seams on PgController/Supervisor | not worth fixing: one uncontended lock; cfg-gating needs feature plumbing for an integration test |
| C22 free_space per batch | not worth fixing: unmeasured; unknown volume → 0 is a deliberate fail-closed guard |
| A1 pff `.prepared` | fixed R1 `71a2a89` |
| A2 unused serde | fixed R1 `71a2a89` |
| A3 workspace dependencies | fixed R1 `71a2a89` |
| A7 `outlook-fixtures` / `make-pst.ps1` | kept by user decision (Windows phase W2); pff message for the ported `.py` script fixed R3 `4f5253c` |
| A8 banned cargo flags | core part fixed R3 `e148989`, `1b44c47`; `prepare.mjs` part is the ui squad's |
| A9 Throttle into core | fixed R3 `e148989` |
| A10 source-text handler test | not worth fixing: works; a rewrite is churn |
| A11 loadgen perf in src | fixed R3 `e148989` |
| A12 loadgen argv loop | not worth fixing: internal tool |
| A13 version in tauri.conf.json | fixed R2 `632058a` |
| A14 `bundle.targets: all` | not worth fixing: no Linux build exists; shipped platforms are a user decision |
| A16 Drop ignores close results | not worth fixing: unactionable in `Drop` |
| A17 no CI | not worth fixing in this cleanup: new infrastructure (reported to the user) |
| A4, A5, A6, A15 | ui squad ownership — in the ui ledger |
