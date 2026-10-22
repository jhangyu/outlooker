# Round 1 — squad core handoff

Branch `techdebt/r1-core`, code head `d139fbf` (reviewer verdict CONFIRMED), docs commit `ba97464`. Base `c1965dd`.
Detail: `round-1-core-plan.md` (verified evidence, refuted items, roadmap), `round-1-core-summary.md`, `round-1-core-impl1-report.md`.

## Completed work and evidence

| item | commit | evidence |
|---|---|---|
| A2 unused serde/serde_json in root crate | 71a2a89 | clippy clean; root `[dependencies]` has no serde |
| C4 image without default codecs | 71a2a89 | exr, ravif, rav1e, tiff, qoi gone from Cargo.lock; `moxcms` stays (non-optional dependency of image 0.25.10) |
| A3 `[workspace.dependencies]` | 71a2a89 | no exact pin repeated across manifests |
| A1 pff build.rs `.prepared` skip | 71a2a89 | `grep prepared crates/pff/build.rs` → 0 |
| C15 one newest-server-log scan | 71a2a89 | unit test `last_log_line_skips_startup_log` |
| F4 orphaned plan-ID comments | 56b5acb, edb14f5, 3431a42 | grep `R[12]-NN` / "named in the plan" over src-tauri `*.rs` → 0 |
| C1 one DB error policy | fe101cf, d139fbf | unit tests `pool_closed_is_not_ready`, `sqlstate_policy`, `disk_full_names_the_data_root`; integration test `db_schema::settings_get_uses_cached_quality_after_pool_closed` |
| C3 step 1 `db::library_counts` | fe101cf | 2-count query written once (`db/mod.rs`) |
| C14 timeouts from constants | fe101cf | no `statement_timeout = '…'` literal in Rust sources |
| C16 one `truncate_to_micros` | fe101cf | unit test incl. pre-2000 case |
| C13 part: `Db::is_connected` deleted | fe101cf | grep → 0 |
| R-C1 (reviewer) 53100 mapping | d139fbf | see contract below |

Gates (run from `src-tauri`, shared `CARGO_TARGET_DIR`):
- Full gate 20/20 RC=0 at `3431a42`: `r1-core/tmp/verify/20261022T1831*`–`20261022T1837*` (each file stamps HEAD, RC, duration).
- Targeted re-gate 9/9 RC=0 at `d139fbf` (clippy, lib 133 tests, db_schema, removal, writer, import_pipeline A/B/C, smoke seeded): `r1-core/tmp/verify/*-r2-*.txt`.
- `tmp/verify/` is untracked in the worktree; it does not survive a worktree removal.

## Interface contracts the next round depends on

DB error mapping (`crates/outlooker-core/src/db/mod.rs`), the only mappers in the crate:
- `db_err(tokio_postgres::Error)`:
  - no SQLSTATE and (closed or I/O source) → `DbNotReady`;
  - no SQLSTATE otherwise → `Internal(e.to_string())`;
  - SQLSTATE → `sqlstate_err`.
- `sqlstate_err(code, message)`: `57P01`/`57P02`/`57P03` → `DbNotReady`; everything else, including `53100`, → `Internal("<code>: <message>")`.
- `no_sqlstate_err(&Error)`: the no-SQLSTATE arm, reused by `sql_console/execute.rs map_sql_error`.
- `pool_err(PoolError)`: `Closed` → `DbNotReady`; `Backend(e)` → `db_err(e)`; other → `Internal("pool: …")`.
- `Db::app_client()` / `Db::ro_client()` (`db/pools.rs`): the only pool checkout path, `pool_err` applied. `lifecycle/health.rs:39` probe and tests still use `app_pool()`.
- Writer only (`importer/writer.rs` private `map_db_error`, used at session setup and batch commit): `53100` → `FileAccess{data_root, insufficientSpace}`, else `db_err`.
- Domain tables layer on top and fall through to `db_err`:
  - search: 57014 → `QueryTimeout`, 22007/22008 → invalid cursor;
  - SQL console: its SQLSTATE table;
  - saved queries: unique violation → duplicate name.
- Queue policy depends on this: `DbNotReady` pauses the job and waits; anything else fails it. New code must map transient DB states to `DbNotReady` only through `db_err`/`pool_err`, never locally.

Other: `db::library_counts(&Client)`; `importer::writer::truncate_to_micros` is the single timestamp truncation (PostgreSQL epoch).

## Known limitations

- Error codes the UI can see changed (bug fixes; listed in `round-1-core-summary.md`). There is no UI string for a disk-full error outside the writer: it shows as Internal with the PostgreSQL message.
- Writer checkpoint/folder/known-hash queries map 53100 to Internal (only batch commit and session setup carry the data root).
- `smoke_10k` (28–38 s) and `outlook_pst` (30–31 s, mostly compile) run close to the 40 s cap under load.

## Not worth fixing (reasons)

- C3 step 2 count cache: adds state for an unmeasured cost.
- C21 test seams on `PgController`/`Supervisor`: one uncontended lock; cfg-gating needs feature plumbing for an integration test.
- A10 source-text handler test: works; a rewrite is churn.
- A12 loadgen argv loop: internal tool.
- A14 `bundle.targets`: no Linux build exists.
- A16 Drop ignores close results: unactionable in Drop.
- A17 CI: new infrastructure, out of scope.
- A7: keep `outlook-fixtures` and ui's `make-pst.ps1`. They belong to the pending Windows phase W2 (spec `docs/superpowers/specs/2026-10-21-outlooker-design.md:2423-2428`). Running them needs `--features outlook-fixtures`, which the root CLAUDE.md cargo rule does not list; the user decides that when W2 runs.
- C13 rest refuted: `WriterHandle::submit` is used by `tests/writer.rs`; `NullSink` is needed by integration tests.

## Next round: recommended core slice for round 2

C2 (`importer/queue.rs` split) is NOT unblocked yet: C5 and C18 still edit `queue.rs`. Land them in round 2; C2 + C20 go to round 3.

Package 1 — opus (judgment: enum round-trips, rollback safety). Exclusive files:
- core `src/importer/queue.rs`, `src/importer/writer.rs`, `src/importer/types.rs`
- core `src/dto/import.rs`, `src/dto/search.rs`
- core `src/lifecycle/move_dir.rs`, `src/dto/events.rs`

Items:
- C5: `as_db/from_db` next to `ImportPhase`, `ImportErrorStage`, `ImportErrorSeverity`, `RecipientKind`, plus new `JobKind`/`JobState`. An unknown DB value becomes `AppError::internal`, no silent fallback. Add one round-trip unit test per enum.
- C18: `emit_status` returns the `PstSource` it fetched; enqueue runs 1 query per path instead of 2.
- C12: `move_dir.rs` changes:
  - log the swallowed `remove_dir_all` failure of the old root;
  - on rollback, delete only what the move created, never a pre-existing empty target;
  - log rollback errors.
  - Add one test for the pre-existing target case.

Package 2 — sonnet (mechanical). Exclusive files:
- core `src/settings/mod.rs`, `src/db/credentials.rs`, `src/lifecycle/pg.rs`, `src/dto/error.rs`
- core `src/lifecycle/supervisor.rs`, `src/lifecycle/health.rs`, `src/sink.rs`, `src/importer/webp.rs`, `src/importer/normalize/sanitize.rs`
- core `src/importer/inflight.rs` (1 C9 site), core `src/maintenance/mod.rs`, `tests/maintenance.rs`
- one new util module

Items:
- C8: one `write_atomic(path, bytes, mode)` plus `AppError::io(path, err)`; replace the 3 implementations.
- C9: one poison-tolerant lock helper at every non-test `lock().unwrap()`/`read().unwrap()`/`write().unwrap()` site. Recount with grep before editing.
- C11: the reindex task reads non-constraint public indexes from `pg_index` at run time; delete `REINDEX_INDEXES` and its consistency test.

C9 sites in `queue.rs`/`writer.rs`: none (the queue already uses `unwrap_or_else(into_inner)`). The two packages share no files.

Round 3 core: C2 split + C20 table test (opus). Remaining, by priority:
- C10 layering (db/lifecycle cycle);
- C19 shared test helpers;
- C13 rest (`Writer` → `WriterHandle::spawn`, `Paths::new` uses `default_data_root`);
- A11 loadgen `perf.rs` → tests (tokio-postgres becomes a dev-dep);
- A8 core part (`loadgen/tests/perf.rs:2,94` and `src/perf.rs` text without `-p`/`--features`);
- A13 drop `version` from `tauri.conf.json` (check the Tauri 2.12 fallback in docs first);
- A9 `Throttle` into core `sink`.

C6, C7, C17 and C22 only if an `import_bench` measurement shows a gain; else not worth fixing. Round 3 is heavy: put C2 first, and judge the tail items not worth fixing if the budget runs out.

## Environment findings (embed in round 2 prompts)

- Hook caps: clippy and `cargo test` get 40 s, `cargo build` 150 s. A new worktree needs several identical 40 s reruns of clippy and `cargo test --workspace --no-run`. Each rerun keeps its progress (path crates recompile per worktree path). Run the `--no-run` warm-up before the gate. After a warm build it takes 9–18 s.
- Commands near the cap: `sql_console read_only_role_rows` (≈32 s, waits for a 30 s statement timeout), `smoke_10k` (28–38 s), `outlook_pst` (≈31 s). Keep them as separate gate commands. The 20-command split in `round-1-core-plan.md` "Gate commands" covers every test (import_pipeline 26/26, sql_console 8/8, smoke 2/2).
- `--test NAME` built one extra variant of libwebp-sys/jsonschema/webp/outlooker-core on first use (about 40 s, once). The orchestrator ruled: keep using `--test NAME`.
- A test killed by the cap leaves a postmaster with ppid 1 in a `$TMPDIR/.tmpXXXX/data/pgdata` dir. Stop it with `pg_ctl -D <dir> stop -m fast` (3 such cases in the round-1 baseline).
- `/tmp` redirection is hook-blocked; use `<worktree>/tmp/verify/` or `.claude/tmp/` (both untracked; never `git add`).
- Run cargo with cwd `src-tauri`. `--manifest-path` from another cwd picks the wrong toolchain (rustc 1.94 vs pinned 1.99).
- The SendMessage cap is 1500 chars; longer reports go to a file.
