# Round 2 — squad core plan

Worktree `/Users/jhangyu/project/outlooker-wt/r2-core`, branch `techdebt/r2-core`, base `a832785` (integration after round 1).
All claims re-checked in code on 2026-10-23 (grep/read). Paths relative to `src-tauri/crates/outlooker-core/` unless stated.

## Verified evidence (current code)

| id | evidence | package |
|---|---|---|
| C5 | Silent-fallback parsers: `importer/queue.rs:151 status_of` (unknown state → Completed `:160`), `:164 phase_of` (unknown → Finalizing `:168`), `:467-479` stage/severity closures (unknown → Store / JobFailed). Writer side: `importer/writer.rs:87 phase_db`, `:95 stage_db`, `:106 severity_db`, `:114 kind_db`, plus a third phase parser `:134` (already errors). Recipient kind parsed again in `search/mail.rs:45-49` (unknown → To) and written again in `search/compile.rs:118-122`. Job kind/state are bare `&str`/`String` (`queue.rs:376-421, 517, 620, 674, 712, 802, 813-815`). | 1 |
| C18 | `queue.rs:344-345`: `emit_status` (runs `LIST_SQL`) then `source()` (runs it again) per enqueued path. | 1 |
| C12 | `lifecycle/move_dir.rs:155` `let _ = remove_dir_all(old root)` after success; rollback `:161` deletes `target` even when the empty target pre-existed (allowed at `:98`); `:160,162,164` rollback errors ignored; `:116` start error after failed stop ignored. | 1 |
| A13 | `src-tauri/tauri.conf.json:4 "version": "0.1.0"` duplicates root `Cargo.toml` version. | 1 |
| C8 | Three file writers: `settings/mod.rs:19 write_atomic` (no mode; also used by `lifecycle/init.rs:35`), `db/credentials.rs:29 save` (tmp+fsync+rename, 0600 via create+truncate — a stale tmp keeps its old mode), `lifecycle/pg.rs:155 write_private` (remove + create_new 0600, not atomic). | 2 |
| C9 | Non-test `lock()/read()/write().unwrap()` sites: core `sink.rs:54,57,63`, `settings/mod.rs:48,53,70,77`, `importer/webp.rs:25,26,27`, `importer/normalize/sanitize.rs:92,115`, `lifecycle/supervisor.rs:83,87,91,106,128`, `lifecycle/pg.rs:196,226`, `lifecycle/health.rs:54`; app `src-tauri/src/events.rs:151,158,170`; loadgen `src/images.rs:52,59,63`. (`importer/inflight.rs:64` is a test.) Crate convention: 36 sites use `unwrap_or_else(|p| p.into_inner())`. | 2 |
| C11 | `maintenance/mod.rs:15 REINDEX_INDEXES` (17 names) duplicates the migration; consistency only via `tests/maintenance.rs:83-98`. | 2 |
| C19 | `count(client, sql)` helper copied in `tests/import_pipeline.rs`, `tests/removal.rs`, `tests/writer.rs`; png generator in `tests/import_pipeline.rs` and `importer/webp.rs` tests. | 2 |

## Slice changes vs the round-1 recommendation

- C8 narrowed: one `write_atomic(path, bytes, mode)`. `AppError::io(path, err)` (18 `file_access(p, &e.to_string())` sites) is **not worth fixing**: argument sugar, no drift risk, and it would put `move_dir.rs` in both packages.
- C9: no new helper module. Use the crate's existing `unwrap_or_else(|p| p.into_inner())` form at every site (stdlib, matches 36 sites).
- C12 moved to Package 1 (opus, data-affecting rollback) so Package 2 stays mechanical; with C8 narrowed, `move_dir.rs` is in one package only.
- Pulled forward from round 3 to keep round 3 reachable: C19 (Package 2), A13 (Package 1, one line).

## Package 1 — enum round-trips, enqueue double query, data-dir move rollback, config version (C5, C18, C12, A13)

Recommended model: **opus**. Judgment-dense: C5, C12. C18 and A13 are one-line riders in files this package already owns.

Exclusive files (core = `src-tauri/crates/outlooker-core/`):
- core `src/dto/import.rs`, `src/dto/search.rs`, `src/dto/events.rs`
- core `src/importer/queue.rs`, `src/importer/writer.rs`, `src/importer/types.rs`
- core `src/search/mail.rs`, `src/search/compile.rs`
- core `src/lifecycle/move_dir.rs`
- core `tests/lifecycle.rs` (C12 test only), `tests/search.rs`, `tests/app_compose.rs` (only if an assertion must follow)
- `src-tauri/tauri.conf.json`

Work:
1. C5: `as_db(self) -> &'static str` and `from_db(&str) -> Result<Self, AppError>` next to `ImportPhase`, `ImportErrorStage`, `ImportErrorSeverity`, `RecipientKind` (pattern: `importer/types.rs AddressType::as_db`). Add `JobKind {Import, Remove}` and `JobState {Queued, Running, Cancelling, Cancelled, Failed, Completed}` (crate-internal, next to the queue; not DTOs) with the same pair. Unknown DB value → `AppError::internal("unknown <type> <value>")`, never a fallback. Replace `phase_of`, `phase_db`, `stage_db`, `severity_db`, `kind_db`, the writer phase parser, the queue stage/severity closures, `search/mail.rs` and `search/compile.rs` string matches. `status_of` takes `(JobKind, JobState)`. Bare string comparisons of kind/state in Rust become enum comparisons; SQL text literals (`'queued'` etc. inside SQL) stay. One round-trip unit test per enum (every variant: `from_db(as_db(v)) == v`; one unknown string → `Err`).
2. C18: `emit_status` returns the `Option<PstSource>` it fetched; `enqueue` uses it (1 `LIST_SQL` per path); `source()` deleted if it has no other caller.
3. C12 in `move_dir.rs`:
   - record whether the move created `target` (it did not exist before the copy);
   - success: a failed `remove_dir_all(old root)` is logged with `log::warn!` (path + error), the move still reports Done;
   - rollback: remove `target` only if the move created it; a pre-existing (empty) target is emptied, not deleted;
   - rollback: log every error of stop / remove / `set_data_dir` / `start_cluster` (`log::error!`); the emitted Failed error stays the original error;
   - test: a failed move (use the existing `fail_next_verify` seam) into a pre-existing empty target leaves the target directory existing and empty.
4. A13: delete `"version"` from `tauri.conf.json` after confirming in context7 (Tauri 2.x, locked version in `Cargo.lock`) that the version then comes from `Cargo.toml`. If the docs do not confirm it, do not change the file and report.

Acceptance (mechanical; run from the worktree root):
- P1-1. `grep -nE "fn (phase_of|phase_db|stage_db|severity_db|kind_db)\b" -r src-tauri/crates/outlooker-core/src` → 0.
- P1-2. `grep -nE '_ => (ImportStatus::Completed|ImportPhase::Finalizing|ImportErrorStage::Store|ImportErrorSeverity::JobFailed|to\.push)' -r src-tauri/crates/outlooker-core/src` → 0.
- P1-3. `grep -nE '"(queued|running|cancelling|cancelled|failed|completed|import|remove)"' src-tauri/crates/outlooker-core/src/importer/queue.rs` → hits only inside the `as_db`/`from_db` bodies and the `#[cfg(test)]` module.
- P1-4. `cargo test --workspace --lib <enum>` round-trip tests exist and pass (≥5 tests: phase, stage, severity, recipient kind, job kind/state).
- P1-5. `grep -n "source(inner" src-tauri/crates/outlooker-core/src/importer/queue.rs` → 0 inside `enqueue`.
- P1-6. `grep -n "let _ =" src-tauri/crates/outlooker-core/src/lifecycle/move_dir.rs` → 0; the new pre-existing-target test passes.
- P1-7. `grep -c '"version"' src-tauri/tauri.conf.json` → 0 (or reported as refuted with the doc quote).
- P1-8. Full gate exits 0. Any changed test assertion is listed with the reason.

## Package 2 — one atomic writer, poison policy, run-time reindex list, shared test helpers (C8, C9, C11, C19)

Recommended model: **sonnet** (mechanical, 4 items).

Exclusive files:
- core `src/fsutil.rs` (new), `src/lib.rs` (module line only), `src/settings/mod.rs`, `src/db/credentials.rs`, `src/lifecycle/pg.rs`, `src/lifecycle/init.rs`
- core `src/sink.rs`, `src/importer/webp.rs`, `src/importer/normalize/sanitize.rs`, `src/lifecycle/supervisor.rs`, `src/lifecycle/health.rs`
- `src-tauri/src/events.rs`, `src-tauri/crates/outlooker-loadgen/src/images.rs`
- core `src/maintenance/mod.rs`, `tests/maintenance.rs`
- core `src/test_support/mod.rs`, `tests/import_pipeline.rs`, `tests/removal.rs`, `tests/writer.rs`

Work:
1. C8: `fsutil::write_atomic(path: &Path, bytes: &[u8], private: bool) -> Result<(), AppError>`: create parent dir; remove a stale `<path>.tmp`; create it with `create_new` (mode 0600 on unix when `private`); write + `sync_all`; rename. Replace `settings::write_atomic` (and its `init.rs` caller), `Credentials::save`, `pg.rs write_private` (its unit test moves to `fsutil`). Delete the three old bodies.
2. C9: every site in the C9 row above → `.unwrap_or_else(|p| p.into_inner())`. Recount with grep first; test modules excluded.
3. C11: the reindex task selects `quote_ident(relname)` of valid non-constraint indexes in schema `public` (the query now in `tests/maintenance.rs:86`), ordered by name, after the leftover drop; delete `REINDEX_INDEXES` and the set-equality part of `reindex_leaves_all_valid`. Keep `SEARCH_INDEXES` and its migration test. Make the query a `pub const` and assert in `tests/maintenance.rs` that it returns `messages_body_fts` and `import_jobs_queue` and no `*_pkey` index.
4. C19: `TestCluster::count(&self, sql) -> i64` (or a free fn in `test_support`) and one test png generator in `test_support`; delete the copies in the three test files and `importer/webp.rs` tests (webp tests only if `test_support` is reachable from unit tests under the current cfg; otherwise keep that one copy and report).

Acceptance (mechanical; run from the worktree root):
- P2-1. `grep -rn "fn write_atomic\|fn write_private\|sync_all" src-tauri/crates/outlooker-core/src` → `write_atomic` and `sync_all` only in `fsutil.rs`; `write_private` → 0.
- P2-2. `grep -rnE "\.(lock|read|write)\(\)\.unwrap\(\)" src-tauri/src src-tauri/crates/*/src` → only lines inside `#[cfg(test)]` modules (list them in the report).
- P2-3. `grep -rn "REINDEX_INDEXES" src-tauri` → 0.
- P2-4. `grep -rnE "^(async )?fn count\(" src-tauri/crates/outlooker-core/tests` → 0.
- P2-5. `fsutil` unit test: private file has mode 0600 (unix) and a stale `.tmp` with mode 0644 does not leak its mode.
- P2-6. Full gate exits 0.

## Shared-worktree protocol

- Both implementers edit one worktree. Each commits only its own files with a pathspec commit, then sends `SETTLED <hash>` to the squad architect (name given in the dispatch prompt). The gate window opens only after both report SETTLED (or one is idle with nothing uncommitted); no writes during the window.
- Implementers run targeted tests only (`--lib`, `--test NAME`); the full gate is run by `r2-core-test-haiku`.

## Gate commands

Unchanged from round 1 (`round-1-core-plan.md` "Gate commands", 20 commands; test targets unchanged: core tests app_compose, db_schema, import_pipeline, lifecycle, maintenance, pg_lifecycle, removal, search, sql_console, writer; loadgen perf, smoke; pff outlook_pst). Run from `<worktree>/src-tauri` with `export CARGO_TARGET_DIR=/Users/jhangyu/project/outlooker/src-tauri/target`, one command per Bash call, logs under `<worktree>/tmp/verify/` (untracked). Warm-up: `cargo test --workspace --no-run` until RC=0.

## Round 3 roadmap (core)

After round 2, the remaining worth-fixing core items are C2, C20, C13 rest, C10, A9, A11, A8 core part. Two packages:
- R3 Package 1 — opus: C2 split `importer/queue.rs` into commands / runner / pipeline, seams in one struct (unblocked: C1, C3, C5, C16, C18 all landed); C20 table test of the end-of-attempt decision; C13 rest (`Writer` unit struct → `WriterHandle::spawn`, fixture caller; `Paths::new` uses `default_data_root`). Owns `importer/{queue,runner,pipeline,mod,writer}.rs`, `test_support/import_fixtures.rs`, `paths/mod.rs`.
- R3 Package 2 — sonnet: C10 (`connect` + constants into `db/`, `free_space` + `total_ram_bytes` into one `sys` module, one base `Config`), A9 (`Throttle` into core `sink`), A11 + A8 core part (loadgen `perf.rs` → `tests/`, `tokio-postgres` dev-dep, text without `-p`/`--features`). C10's one-line `use` change in `importer/writer.rs` is handed to Package 1 (owner of that file) after Package 2 commits, or the R3 architect sequences it.
- Measurement-gated C6, C7, C17, C22: judged in round 3 without new work — not worth fixing unless an `import_bench` number exists (C22's unknown-volume → 0 is a deliberate fail-closed guard, `writer.rs:431`).

## Not worth fixing (this round)

- C8 `AppError::io` sugar: see slice changes.
- Round-1 list unchanged (C3 step 2, C21, A10, A12, A14, A16, A17, A7 keep, C13 refuted parts).

## Baseline gate result

HEAD `a832785`, 2026-10-23 ~10:45–10:55 CST, load average up to 44 (other squad building). Artifacts `r2-core/tmp/verify/base-*.txt` (each stamps HEAD, RC, duration).
All 20 commands RC=0: clippy; lib 133 (11+115+7); doc 1; app_compose 3; db_schema 9; maintenance 3; removal 2; writer 10; lifecycle 17; pg_lifecycle 8; search 16; sql_console 7+1 (31 s); import_pipeline 9+9+8 = 26/26; smoke 1+1 (smoke_10k 22 s); outlook_pst 6 (25 s, mostly compile); perf 0 (14 ignored). **Baseline PASS.**
Cold worktree: `--no-run` needed 3 runs at the 40 s cap, clippy 2 runs (progress persisted, no hang).
