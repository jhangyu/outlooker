# Round 1 — squad core plan

Worktree `/Users/jhangyu/project/outlooker-wt/r1-core`, branch `techdebt/r1-core`, base `c1965dd`.
All claims below were re-checked in code on 2026-10-23 (grep/read, `cargo tree`). Paths relative to `src-tauri/crates/outlooker-core/src/` unless stated.

## Selected items (round 1)

| id | verified evidence | package |
|---|---|---|
| C1 | 7 error mappers confirmed: `db/mod.rs:20 pg_err`, `search/mod.rs:22 pool_err` + `:94 db_err`, `importer/queue.rs:148 pool_err` + `:160 db_err`, `importer/writer.rs:156 pg` + `:224 map_db_error`, `importer/remove.rs:36 pg`, `sql_console/mod.rs:71 pool_err`, `sql_console/execute.rs:13` None arm, plus `settings/mod.rs:32`, `lifecycle/supervisor.rs:98`, `lifecycle/status.rs:10` mapping every pool error to Internal. Real bug: `settings/mod.rs:80` falls back only on `DbNotReady`, but `pool_client` (`:32`) returns `Internal` for a closed pool. | 1 |
| C3 (step 1 only) | Same 2-count query at `importer/queue.rs:269`, `:940`, `lifecycle/supervisor.rs:100`; `lifecycle/status.rs:14` is a 4-count superset. | 1 |
| C14 | `search/execute.rs:13-14` constants vs SQL literals `:77`, `:125`; `sql_console/mod.rs:24` vs `:27`. | 1 |
| C16 | `importer/queue.rs:297 trunc_micros` (Unix epoch, 3 call sites + unit test `:1102`) vs `importer/writer.rs:124 truncate_to_micros` (PG epoch, the value PostgreSQL stores). The queue value is compared to the stored value, so the writer version is the correct one. | 1 |
| C13 (part) | `db/pools.rs:63 is_connected`: zero callers (grep over src, crates, tests). | 1 |
| A2 | `grep -rn serde src-tauri/src src-tauri/build.rs` → 0 hits; `serde`, `serde_json` in root `Cargo.toml:21-22` unused. | 2 |
| C4 | `cargo tree --workspace -e features -i image`: default features come only from `outlooker-core` and `outlooker-loadgen` (webp enables `image` without defaults — scanner's UNCERTAIN resolved). `Cargo.lock` holds exr, moxcms, qoi, rav1e, ravif, tiff. `importer/webp.rs:48-53` accepts only PNG/JPEG/GIF/BMP; loadgen `images.rs` writes PNG/JPEG. | 2 |
| A3 | No `[workspace.dependencies]`; tokio, serde, serde_json, log, tokio-postgres, image, rayon, tempfile pinned in 2–4 manifests. | 2 |
| A1 | `crates/pff/build.rs:31` skips copy+configure when `OUT_DIR/libpff-src/.prepared` exists; build.rs reruns on `ORIGIN` change (`scripts/vendor-libpff.sh:38` rewrites it), finds `.prepared`, compiles the stale copy. | 2 |
| C15 | `lifecycle/pg.rs:168 newest_server_log` skips `startup.log`; `lifecycle/pg.rs:399 last_log_line` does not (used by `lifecycle/health.rs:60`). | 2 |

## Refuted items

- C13 (rest): `WriterHandle::submit` is not test-fixture-only — `tests/writer.rs` calls it 7 times; keep. `NullSink` is used by integration tests (`tests/app_compose.rs`, `tests/import_pipeline.rs`) that link the lib, so it cannot move under `cfg(test)`; gating it behind `test-support` only adds cfg noise. `default_data_root` duplicate literal is real but one line (`paths/mod.rs:14` vs `:26`) — folded into round 2 with the `Writer` unit-struct removal (C2 touches the same spawn site).
- C15 rustfmt claim (`pg.rs:262` two statements on one line): line numbers have shifted and there is no rustfmt gate; not acted on.
- Scanner line numbers for C15 (`pg.rs:131`) are stale (actual `:168`); content claim holds.

## Roadmap rounds 2–3 (tentative; re-verify each item in code at round start)

Round 2 (needs C1 landed):
- C5 enum `as_db/from_db` + `JobKind/JobState`, unknown value → `AppError::internal` (queue.rs, writer.rs, dto) — judgment.
- C18 `emit_status` returns the `PstSource` (queue.rs) — mechanical.
- C10 move `connect`/constants into `db/`, `free_space` + `total_ram_bytes` into one sys module, one base `Config` — judgment.
- C8 one `fsutil::write_atomic` + `AppError::io` — mechanical.
- C9 one poison-tolerant lock helper at all non-test `lock().unwrap()` sites — mechanical.
- C11 reindex task reads `pg_index` at run time, delete `REINDEX_INDEXES` — mechanical.
- C12 `move_dir.rs` swallowed errors and rollback deleting a pre-existing target — judgment (data-affecting bug).
- C19 shared test helpers (`TestCluster::count`, test png).
- C13 rest: `Writer` unit struct → `WriterHandle::spawn`, `Paths::new` uses `default_data_root`.
- A11 loadgen `perf.rs` → `tests/`, `tokio-postgres` to dev-dep.
- A8 core part: `loadgen/tests/perf.rs:2,94` and `src/perf.rs` text without `-p`/`--features` (the `tests/e2e/prepare.mjs:33` part is the ui squad's file).
- A13 drop `version` from `tauri.conf.json` (verify Tauri 2.12 fallback to Cargo.toml version in docs first).
Round 3 (needs C1, C5, C16, C18 landed in queue.rs):
- C2 split `importer/queue.rs` into commands / runner / pipeline, seams into one struct; then C20 table test of the end-of-attempt decision.
- A9 move `Throttle` from `src/events.rs` into core `sink`.
- C6, C7: only after an `import_bench` measurement shows a gain; else not worth.
- A7 core part: decided KEEP (see Not worth fixing).
- C17, C22: measurement-gated like C6.

## Not worth fixing

- C3 step 2 (cache counts from deltas): adds state for an unmeasured cost; step 1 removes the drift.
- C21 `with_ports`/`fail_next_verify` test seams: one uncontended lock on the start path; cfg-gating needs feature plumbing for an integration test.
- A10 `lib.rs` source-text handler test: works; rewriting it is churn for a guard test.
- A12 loadgen argv loop: internal tool, errors already print usage.
- A14 `bundle.targets`: no Linux build exists; picking targets is speculative.
- A16 Drop impls ignore close results: unactionable in Drop.
- A17 CI: out of scope for debt rounds (new infrastructure).
- A7 `outlook-fixtures` feature (`outlooker-core/Cargo.toml:36`, `tests/import_pipeline.rs:27,955`) and ui's `scripts/fixtures/make-pst.ps1`: KEEP both. They are the pending Windows phase W2 (spec `docs/superpowers/specs/2026-10-21-outlooker-design.md:2423-2428`, user runs Windows steps over SSH), not dead code; Windows verification is out of contract scope. Known conflict: running them needs `--features outlook-fixtures`, which the root CLAUDE.md cargo rule does not list as an allowed exception — the user decides that when W2 runs.

## Package 1 — DB error mapping and shared DB helpers (C1, C3 step 1, C14, C16, C13 part, F4 part)

Recommended model: **opus** (C1 picks one error policy that changes visible error codes; judgment-dense across 20 files).

Exclusive files (core = `src-tauri/crates/outlooker-core/`):
- core `src/db/mod.rs`, `src/db/pools.rs`, `src/db/indexes.rs`
- core `src/search/mod.rs`, `src/search/execute.rs`, `src/search/filter.rs`, `src/search/image.rs`, `src/search/mail.rs`
- core `src/importer/queue.rs`, `src/importer/writer.rs`, `src/importer/remove.rs`
- core `src/sql_console/mod.rs`, `src/sql_console/execute.rs`, `src/sql_console/saved.rs`
- core `src/settings/mod.rs`
- core `src/lifecycle/supervisor.rs`, `src/lifecycle/status.rs`, `src/lifecycle/start.rs`
- core `src/maintenance/mod.rs`
- core `tests/db_schema.rs`
- `src-tauri/crates/outlooker-loadgen/src/load.rs`

Work:
1. C1: in `db/mod.rs` keep exactly two public mappers and delete every private copy:
   - `db_err(e: tokio_postgres::Error) -> AppError` = superset: no SQLSTATE and (`is_closed()` or I/O source) → `DbNotReady`; SQLSTATE `57P01`/`57P02`/`57P03` (server shutting down / starting) → `DbNotReady`; `53100` → `FileAccess{insufficientSpace}`; other SQLSTATE → `Internal("<code>: <message>")`; other → `Internal(e.to_string())`. The writer variant that needs the data root path keeps a thin `map_db_error(e, data_root)` only if a caller passes a real path; otherwise one function.
   - `pool_err(e: PoolError) -> AppError`: `Closed` → `DbNotReady`; `Backend(e)` → `db_err(e)`; other → `Internal("pool: …")`.
   - `Db::app_client()` / `Db::ro_client()` in `db/pools.rs` return the pool object with `pool_err` applied; replace every `app_pool()?.get().await.map_err(..)` / `ro_pool()?…` site.
   - Domain tables stay where they are but fall through to the shared mapper: `search/execute.rs map_err` (57014, 22007/22008), `sql_console/execute.rs map_sql_error` (its no-SQLSTATE arm uses the same closed/I-O test, not a blanket `DbNotReady`), `sql_console/saved.rs write_err`.
   - Rename `pg_err` callers to `db_err` (`indexes.rs`, `start.rs`, `maintenance/mod.rs`, loadgen `load.rs`).
2. C3 step 1: `db::library_counts(client) -> Result<LibraryCounts, AppError>`; use it at `queue.rs:269`, `queue.rs:940`, `supervisor.rs:100`. `status.rs` keeps its 4-count query.
3. C14: build `SET LOCAL statement_timeout = <ms>` from `TIMEOUT_MS`/`SHORT_TIMEOUT_MS` (search) and `TIMEOUT_MS` (sql_console). The `'30s'` role default in `db/migrations/V2__roles_views.sql:3` stays (migration text).
4. C16: delete `queue.rs trunc_micros`, use `writer::truncate_to_micros` at its 3 call sites, move its unit test assertion onto `truncate_to_micros`.
5. C13 part: delete `Db::is_connected`.
6. F4 (review round 1) part: `search/mod.rs:101` doc comment starts with the old plan ID "C13." — drop the ID, keep the rationale.

Acceptance (mechanical):
- A1. `grep -rnE "fn (pg_err|pool_err|db_err|pg)\(" src-tauri/crates/outlooker-core/src` → definitions only in `db/mod.rs` (one `db_err`, one `pool_err`); `grep -rn "pg_err" src-tauri/crates` → 0.
- A2. `grep -rnE "_pool\(\)\?\.get\(\)" src-tauri/crates/outlooker-core/src` → 0 (only `db/pools.rs` calls `.get()`; `lifecycle/health.rs:39` probe excepted).
- A3. `grep -rn "(SELECT count(\*) FROM messages), (SELECT count(\*) FROM pst_sources)" src-tauri/crates/outlooker-core/src` → exactly 1 hit (in `db/`).
- A4. `grep -rn "statement_timeout = '" src-tauri/crates/outlooker-core/src --include=*.rs` → 0.
- A5. `grep -rn "trunc_micros\|is_connected" src-tauri/crates` → 0.
- A6. New check that fails if the mapping breaks: unit tests in `db/mod.rs` for `pool_err(PoolError::Closed) == DbNotReady` and the SQLSTATE branch (factor the SQLSTATE part as a pure fn on `(code, message)` so it is testable without a server); plus one test in `tests/db_schema.rs` that `Settings::get` returns the cached quality (not `Internal`) after the pool is closed.
- A7. Full gate below exits 0. Any existing test assertion change must be listed in the report with the reason (a changed error code is a visible behavior change and goes into the round report).

## Package 2 — Build config, lifecycle log, plan-ID comments (A2, C4, A3, A1, C15, F4)

Recommended model: **sonnet** (mechanical, 3 work items).

Exclusive files:
- `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`
- `src-tauri/crates/outlooker-core/Cargo.toml`, `src-tauri/crates/outlooker-loadgen/Cargo.toml`, `src-tauri/crates/pff/Cargo.toml`
- `src-tauri/crates/pff/build.rs`
- core `src/lifecycle/pg.rs`
- F4 comment-only files: `src-tauri/src/lib.rs`, core `src/dto/commands.rs`, core `src/importer/types.rs`, core `src/importer/normalize/sanitize.rs`, core `examples/import_bench.rs`, core `tests/app_compose.rs`, core `tests/import_pipeline.rs`, core `tests/maintenance.rs`, core `tests/sql_console.rs`, `src-tauri/crates/outlooker-loadgen/src/generate.rs`, `src-tauri/crates/pff/tests/outlook_pst.rs`

Work:
1. Manifests (A2 + C4 + A3): delete `serde`, `serde_json` from root `Cargo.toml`; add `[workspace.dependencies]` in root `Cargo.toml` with the exact existing pins for every dependency used by ≥2 workspace manifests; members use `x = { workspace = true, features = [...] }` keeping each crate's current feature list (resolved features must not change except `image`); `image` in workspace deps = `{ version = "=0.25.10", default-features = false, features = ["png", "jpeg", "gif", "bmp"] }`. Keep the serde_json `float_roundtrip` comment's intent. Lockfile updated only by cargo itself (no hand edits, no version bumps).
2. A1: remove the `.prepared` skip in `crates/pff/build.rs` (always `remove_dir_all` + copy + configure when the script runs; it reruns only on `build.rs`/`ORIGIN` change).
3. C15: `last_log_line` reuses the `newest_server_log` selection (one newest-log function, `startup.log` excluded) and keeps returning the last non-empty line.
4. F4 (review round 1, `r1-ui/tmp/review/round-1-review.md` F4): replace orphaned plan IDs (`R1-02`, `R1-05`, `R2-09`, `R2-10`, `R2-12`, `R2-13`, `R2-14`, the leading `C9` in `importer/types.rs:1`, "named in the plan" at `lifecycle/pg.rs:194`) with a short rationale or spec section reference. Comments only. Do NOT touch `db/migrations/V1__schema.sql` / `V2__roles_views.sql` (refinery checksum).

Acceptance (mechanical):
- B1. `grep -nE "^serde" src-tauri/Cargo.toml` → only inside `[workspace.dependencies]`; root `[dependencies]` has no serde/serde_json.
- B2. `grep -cE '^name = "(rav1e|ravif|exr|tiff|qoi|moxcms)"' src-tauri/Cargo.lock` → 0.
- B3. Every dependency in ≥2 member manifests uses `workspace = true`: `grep -hE '^[a-z_-]+ = .*"=' src-tauri/crates/*/Cargo.toml src-tauri/Cargo.toml` lists no crate name twice.
- B4. `grep -c "prepared" src-tauri/crates/pff/build.rs` → 0.
- B5. `grep -c "fn newest_server_log\|read_dir" src-tauri/crates/outlooker-core/src/lifecycle/pg.rs` shows one log-dir scan; existing `pg_lifecycle`/`lifecycle` tests green, plus one unit test in `pg.rs` that a newer `startup.log` is not chosen by `last_log_line`.
- B7. `grep -rnE "\bR[12]-[0-9]{2}\b|named in the plan|^//! C9 " src-tauri --include=*.rs` → 0 hits (Package 1 owns `search/mod.rs:101`); `git diff --stat` shows no change under `db/migrations/`.
- B6. Full gate below exits 0; `cargo tree --workspace -e features -i image` shows no `default` feature.

Note for package 2: a manifest change recompiles most of the graph; the first `cargo test --workspace --no-run` / clippy may exceed the 40 s cap several times. Each run keeps its progress; rerun the identical command until it finishes (not a hang). If a single crate cannot finish inside 40 s, report BLOCKED.

## Gate commands

Run from `<worktree>/src-tauri` with `export CARGO_TARGET_DIR=/Users/jhangyu/project/outlooker/src-tauri/target`. Write each log under `<worktree>/.claude/tmp/` (never `/tmp`, hook-blocked; never `git add` it). One command per Bash call (each ≤40 s cap). Warm-up first: `cargo test --workspace --no-run` until RC=0.

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
13. `cargo test --workspace --test sql_console read_only_role_rows -- --test-threads 4` (≈32 s: waits for a 30 s statement timeout)
14. `cargo test --workspace --test import_pipeline -- --test-threads 4 synthetic_ outlook_pst_import enqueue_validation pst_`
15. `cargo test --workspace --test import_pipeline -- --test-threads 4 poison_marker retry_rewrites mtime_change corrupt_copy byte_copy after_job`
16. `cargo test --workspace --test import_pipeline -- --test-threads 4 index_policy resume_after pg_crash recover_after`
17. `cargo test --workspace --test smoke smoke_10k -- --test-threads 4` (≈38 s under load — at the cap)
18. `cargo test --workspace --test smoke seeded_root_starts -- --test-threads 4`
19. `cargo test --workspace --test outlook_pst -- --test-threads 4`
20. `cargo test --workspace --test perf -- --test-threads 4` (14 ignored; compiles only)

Coverage check: 14+15+16 = 26 of 26 `import_pipeline` tests (9+9+8); 12+13 = 8 of 8 `sql_console`; 17+18 = 2 of 2 `smoke`. A killed test leaves a postmaster with ppid 1: stop it with `src-tauri/resources/postgresql/bin/pg_ctl -D <dir> stop -m fast` after confirming the data dir is a temp dir started by this run.
`--doc` is outside the CLAUDE.md list but selects no features and adds no artifact variant; it covers the one doctest that plain `cargo test --workspace` would run.

## Baseline gate result

Branch HEAD `c1965dd`, 2026-10-23 02:04–02:19 CST, machine load average 29–46 (other squad building).
All 20 commands RC=0: clippy RC=0; lib 129 passed (11+111+7); doc 1 passed; app_compose 3; db_schema 8; maintenance 3; removal 2; writer 10; lifecycle 17; pg_lifecycle 8; search 16; sql_console 7+1; import_pipeline 9+9+8; smoke 1+1; outlook_pst 6; perf 0 (14 ignored). **Baseline PASS.**
Observed environment facts:
- A cold worktree needs several 40 s runs of clippy and `--no-run` (path crates recompile per worktree path); progress persists between runs.
- `--test NAME` builds one extra variant of libwebp-sys, jsonschema, webp and outlooker-core compared with plain `cargo test --workspace` (one-time ~40 s). The root CLAUDE.md claim that `--test NAME` adds no artifact variant does not hold for these crates; reported to the orchestrator, not changed here.
- `smoke_10k` (38 s) and `outlook_pst` (30 s) are close to the cap under load; if they exceed it, report BLOCKED with the timing (no extension self-granted).
