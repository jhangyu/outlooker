# Round 2 — squad core handoff

Branch `techdebt/r2-core`, base `a832785`, code head `e9aafed`, summary `bdcfebb`.
Detail: `round-2-core-plan.md` (verified evidence, slice), `round-2-core-summary.md`, `round-2-core-impl1-report.md`.

## Completed work and evidence

| item | commit | evidence |
|---|---|---|
| C5 DB text ↔ enum, no silent fallback | 632058a | `db_text!` macro (`dto/import.rs`); 6 round-trip unit tests (phase, stage, severity, recipient kind, job kind, job state); `grep "fn (phase_of\|phase_db\|stage_db\|severity_db\|kind_db)"` → 0 |
| C18 enqueue runs `LIST_SQL` once per path | 632058a | `emit_status` returns `Option<PstSource>`; `source()` deleted |
| C12 data-dir move rollback | 632058a | integration test `lifecycle::move_rollback_keeps_pre_existing_empty_target`; `grep "let _ =" lifecycle/move_dir.rs` → 0 |
| A13 version only in `Cargo.toml` | 632058a | `tauri.conf.json` has no `version`; fallback stated in tauri-utils 2.10.2 `config.rs:4108` |
| C8 one atomic file writer | 616dd92, 01ffc72, e9aafed | `fsutil::write_atomic(path, bytes, private)`; unit tests `private_file_is_owner_only_and_stale_tmp_mode_does_not_leak`, `failed_rename_removes_tmp` |
| C9 poison-tolerant locks | 616dd92 | `grep -rnE "\.(lock\|read\|write)\(\)\.unwrap\(\)" src-tauri/src src-tauri/crates/*/src` → only `importer/inflight.rs:64` (test) |
| C11 run-time reindex list | 616dd92, 01ffc72 | `maintenance::REINDEX_LIST_SQL`; `tests/maintenance.rs` asserts known names present, no `*_pkey`, no `messages_dedupe_once`, exactly 17 |
| C19 shared test helpers | 616dd92 | `test_support::{count, png}`, `TestCluster::count`; `grep "fn count(" tests/` → 0 |

Gates (from `src-tauri`, shared `CARGO_TARGET_DIR`; artifacts in `r2-core/tmp/verify/`, untracked, lost on worktree removal):
- Baseline at `a832785`: `base-*.txt`, 20/20 RC=0.
- Full gate at `616dd92`: `gate-00..20-*.txt`, 20/20 RC=0, no orphan postmasters.
- Targeted re-gate at `e9aafed` (changed since `616dd92`: `fsutil.rs`, `tests/maintenance.rs`): `regate-01..04-*.txt` — clippy, lib 11+122+7, maintenance 3, pg_lifecycle 8, all RC=0.

## Interface contracts the next round depends on

- DB error mapping: unchanged from `round-1-core-handoff.md` (`db_err`, `sqlstate_err`, `pool_err`, `Db::app_client`/`ro_client`, writer-only 53100 mapping).
- DB text ↔ enum: `db_text!(Type, "what", { Variant => "text", ... })` in `dto/import.rs` is the only mapping. It generates `as_db`, `from_db` (unknown → `AppError::internal("unknown <what> <text>")`) and a `#[cfg(test)] ALL`. Applied to `ImportPhase`, `ImportErrorStage`, `ImportErrorSeverity`, `RecipientKind`, and crate-internal `importer::queue::{JobKind, JobState}`. SQL text literals for kind/state inside SQL strings stay. `status_of(JobKind, JobState)` is exhaustive (no `_` arm for imports).
- File writes: `crate::fsutil::write_atomic(path, bytes, private)` is the only atomic writer (settings, init marker, credentials, initdb password file). It removes a stale `.tmp`, creates with `create_new` (0600 when `private` on Unix), fsyncs, closes, renames, and removes the `.tmp` on any failure.
- Lock poisoning: every lock uses `unwrap_or_else(|p| p.into_inner())`. No `lock().unwrap()` outside test modules.
- Reindex: `maintenance::REINDEX_LIST_SQL` (valid non-constraint indexes in `public`, `quote_ident`, ordered by name). `db::indexes::SEARCH_INDEXES` and its migration test are unchanged.
- Test helpers: `test_support::{count, png}` and `TestCluster::count` (feature `test-support`). Lib unit tests must not use `test_support` (the feature is on only through loadgen's feature unification); `importer/webp.rs` keeps its own test png for that reason.

## Known limitations

- `tests/import_pipeline.rs:991` (`outlook-fixtures` feature) was changed to `tc.count(...)` by inspection. No allowed cargo command compiles it; it belongs to the pending Windows phase W2.
- An unknown DB enum text fails the whole `import_list` (one bad row → `Internal`). This is the intended no-fallback policy; the schema CHECK constraints make it a schema-drift signal only.
- Round-1 limitations still hold (no UI string for disk-full outside the writer; `smoke_10k`, `outlook_pst`, `sql_console read_only_role_rows` run near the 40 s cap).

## Visible behavior changes (round report)

- Unknown DB enum text → `Internal` instead of a plausible wrong state.
- Failed data-dir move into a pre-existing empty target: the target is emptied and its mode restored, not deleted. Failure to remove the old root after a successful move and every rollback error are logged (`log::warn!`/`log::error!`); the UI still shows Done/Failed as before.
- The initdb password file is written via tmp + rename (0600); no leftover file on failure.

## Not worth fixing (reasons)

- C8 `AppError::io(path, err)`: argument sugar over 18 `file_access(p, &e.to_string())` sites; no drift risk.
- C19 `importer/webp.rs` unit-test png: kept so lib unit tests do not depend on the `test-support` feature.
- C6 (ammonia Builder per message), C7 (snippet/clean_text allocations, `ImageIn` copy), C17 (poison marker writes), C22 (`free_space` per batch): costs are inferred from code shape, never measured; an `import_bench` run needs a `--release` build that does not fit the 150 s cap, and C22's unknown-volume → 0 is a deliberate fail-closed guard (`importer/writer.rs:431`).
- Round-1 list unchanged: C3 step 2, C21, A10, A12, A14, A16, A17, A7 (keep `outlook-fixtures` and `make-pst.ps1` for W2), C13 refuted parts (`WriterHandle::submit`, `NullSink`), C15 rustfmt claim.

## Next round: core slice for round 3

After round 3 the remaining core items are all fixed or listed above. Re-verify line numbers at round start.

Package 1 — opus (judgment: C2 split without behavior change, C20 decision table). Exclusive files (core = `src-tauri/crates/outlooker-core/`):
- core `src/importer/queue.rs`, new `src/importer/runner.rs`, new `src/importer/pipeline.rs`, `src/importer/mod.rs`
- core `src/importer/writer.rs`, `src/test_support/import_fixtures.rs`, `src/paths/mod.rs`
- core `src/lifecycle/move_dir.rs`, and `src/sys.rs` AFTER Package 2 sends SETTLED for C10 (ownership transfers then)
- core `tests/import_pipeline.rs`, `tests/writer.rs` (only if a call site must follow)

Items:
1. C2: split `queue.rs` (1084+ lines) into `queue.rs` (commands, list, errors, `JobKind`/`JobState`, `JobCancel`), `runner.rs` (worker loop, `run_job`, `run_import`, `run_removal`, end-of-attempt decision) and `pipeline.rs` (reader + rayon + writer handle of one attempt). Extract the `attempt` steps into named functions; gather the test seams (`crash_after_batches`, `hold_reader_after`, `on_batch`, `hold_used`, `AttemptEnd::Crashed`, `Wake::Crash`, the cfg synthetic reader) into one struct. No behavior change: the 26 `import_pipeline` tests are the net.
2. C20: the end-of-attempt decision `(AttemptEnd, Option<CancelReason>, Option<JobFailure>) → (JobState, ImportStatus)` (today inline around `queue.rs:813-815`) becomes a pure function with one table-driven unit test.
3. C13 rest: delete the `Writer` unit struct, `WriterHandle::spawn` replaces `Writer::spawn` (callers `queue.rs`, `test_support/import_fixtures.rs:114`); `Paths::new` uses `default_data_root` instead of repeating `"data"`.
4. C10 part b (after Package 2's SETTLED): move `free_space` from `lifecycle/move_dir.rs` into `sys.rs`; update `importer/writer.rs:21` and `move_dir.rs`.

Acceptance: `wc -l importer/queue.rs` well under 1084 and no function over ~80 lines in the three files; C20 table test passes; `grep -rn "struct Writer;\|Writer::spawn" src-tauri/crates` → 0; `grep -rn "move_dir::free_space" src-tauri/crates` → 0; full gate exits 0 (import_pipeline 26/26).

Package 2 — sonnet (mechanical, 4 items). Exclusive files:
- core `src/lifecycle/pg.rs`, `src/db/mod.rs`, `src/db/pools.rs`, `src/db/indexes.rs`, new `src/sys.rs` (until SETTLED), `src/lib.rs` (module lines)
- core `src/lifecycle/{health,start,supervisor,init}.rs` and `src/test_support/mod.rs` only for `use` lines that follow the moves
- core `tests/pg_lifecycle.rs`, `tests/app_compose.rs` (`use` lines), loadgen `src/root.rs`, `src/load.rs` (`use` lines)
- core `src/sink.rs` (+ `src/sink/throttle.rs` if a submodule is chosen), `src-tauri/src/events.rs`
- loadgen `Cargo.toml`, `src/lib.rs`, `src/perf.rs` (deleted), `tests/perf.rs` (+ `tests/common/` if needed), `src-tauri/Cargo.lock` (cargo-written only)
- `src-tauri/crates/pff/build.rs`

Items:
1. C10 part a: move `connect`, `ADMIN_ROLE`, `CJK_SELF_CHECK` from `lifecycle/pg.rs` into `db/` (delete the `pub use crate::lifecycle::pg::connect` re-export at `db/mod.rs:6`); one base `tokio_postgres::Config` builder shared by `connect` and `db/pools.rs`; `total_ram_bytes` into new `sys.rs` (`db/indexes.rs:2` and `pg.rs` use it from there). After this, `grep -rn "lifecycle::pg" src-tauri/crates/outlooker-core/src/db` → 0.
2. A9: move `Throttle`, `ThrottleKey` and their pure helpers (`phase`, `ends_job`) with their 4 tests from `src-tauri/src/events.rs` into core `sink`; `TauriSink` (emit + notify loop) stays in the app crate.
3. A11 + A8 core part: move loadgen `src/perf.rs` under `tests/` (it is used only by `tests/perf.rs`); `tokio-postgres` becomes a loadgen dev-dependency if no other loadgen src file uses it (verify with grep first); messages at `src/perf.rs:41` and `tests/perf.rs:2,94` stop telling users `-p`/`--features` (per root `CLAUDE.md`: `cargo test --workspace --release --test perf -- --ignored ...`, `cargo run --release --example import_bench`; check the example still builds since loadgen unifies `test-support` on core).
4. pff `build.rs:27`: the missing-vendor message names the ui squad's ported script (`scripts/vendor-libpff.sh` → the `.py` name the ui squad commits). Do this only after the ui squad's port has landed on integration; take the exact name from that commit. If the port does not land in round 3, leave the message and report it.

Acceptance: `grep -rn "lifecycle::pg" src-tauri/crates/outlooker-core/src/db` → 0; `grep -rn "fn total_ram_bytes" src-tauri/crates` → 1 hit in `sys.rs`; `grep -n "struct Throttle" src-tauri/src/events.rs` → 0 and the throttle tests run under `cargo test --workspace --lib`; `ls src-tauri/crates/outlooker-loadgen/src/perf.rs` → missing; `grep -rnE "\-p outlooker|--features" src-tauri/crates/outlooker-loadgen` → 0; `grep -c "vendor-libpff.sh" src-tauri/crates/pff/build.rs` → 0 (if the port landed); full gate exits 0.

Ordering: Package 2 commits C10 part a first and sends SETTLED; then Package 1 does C10 part b. C2 does not touch Package 2's files, so both packages start at once.

Gate commands: the 20 commands in `round-1-core-plan.md` "Gate commands". If A11 moves `tests/perf.rs` helpers, command 20 still selects `--test perf`.

## Environment findings (embed in round 3 prompts)

- Same as `round-1-core-handoff.md` "Environment findings". A cold worktree needed 3 runs of `cargo test --workspace --no-run` and 2 clippy runs at the 40 s cap; progress persisted.
- zsh does not word-split `$var`: a `for a in "x y"; set -- $a` loop silently passes one argument (it produced a false RC=101 in the baseline). Write one command per call.
- The SendMessage cap is 1500 characters.
