# Rust test cleanup plan (round 2, v2, 2026-10-23)

Authority: `contract-test-suite-pruning.md` criteria 2, 5, 5a (no default-run test > 1 s), plus three binding user decisions relayed by the lead: (1) one PostgreSQL startup for the whole `cargo test --workspace` run, non-core tests deleted not migrated; (2) every repeated expensive setup is shared; (3) the per-test boot function is not public (`test-policy-design.md` §2 "Expensive setup is shared", §3 "Compiler visibility").
Input: `test-inventory-rust.md` (273 = 224 fn + 49 ts-rs). Overlap claims were re-checked by reading both test bodies; "inventory overturned" marks claims that did not hold.

Counts: DELETE 26 / REWRITE 22 / MERGE 21 / KEEP 204 (KEEP includes 49 ts-rs codegen, 14 `#[ignore]` perf gates, 7 `outlook-fixtures` tests).
Default run after: about 203 executed tests (250 today; the inventory totals differ by 2), 14 ignored.

## 1. Root cause
Measured with the bundled PostgreSQL 18.6 on this Mac, idle (`scripts/tmp/pgprobe/`):
| step | time |
|---|---|
| `initdb` | 0.45–0.75 s |
| `pg_ctl start` / `stop -m fast` | 0.10 s / 0.10–0.20 s |
| `CREATE DATABASE … STRATEGY FILE_COPY` | 0.01–0.10 s |
| copy a 39 MB / 997-file pgdata: `cp -R` / APFS clone | 1.70 s / 0.36 s |

`TestCluster::start()` runs initdb, a self-check start/stop, a start and the migrations for every test (`test_support/mod.rs:38-52`, `lifecycle/pg.rs:197-271`). Lifecycle, pg_lifecycle and app_compose tests run the same initdb through `Supervisor::initialize` or `PgController::init` just to get a running cluster. With 4 threads the CPU-heavy initdb runs contend: `pst_list_status_derivation` does no import and still took 5.5 s.

## 2. Design: exactly one PostgreSQL startup per run (Option A)

### 2.1 Why Option A (one merged test binary)
Cargo builds each `tests/*.rs` file into its own binary and runs the binaries one after another, so a process-wide fixture boots once per binary. I considered and rejected Option B, a cluster shared across binaries (the first binary boots it and later binaries find it through a lock file and port file). No process knows it is the last one, so the cluster either leaks after the run or needs a stop hook that cargo does not provide. Option A: every DB test lives in one binary `it`, so one process means one boot. The cluster starts once for the process and is stopped by an exit hook (§2.3). No Cargo.toml edit is needed: `crates/outlooker-core/Cargo.toml` has no `[[test]]` entries and autotests are on, so cargo discovers `tests/it/main.rs` as target `it` and `tests/cluster_lifecycle.rs` as target `cluster_lifecycle`.
Per run, `it` starts PostgreSQL once and `cluster_lifecycle` starts it only for the exceptions of §2.6. The lib unit tests, pff, loadgen and the root crate start none.

### 2.2 Files
- New `crates/outlooker-core/tests/it/main.rs` (cargo target `it`), exactly:
  ```rust
  #![cfg(feature = "test-support")]
  mod app_compose; mod db_schema; mod import_pipeline; mod lifecycle; mod maintenance;
  mod pg_lifecycle; mod removal; mod search; mod sql_console; mod writer;
  ```
- `git mv` these files into `tests/it/` (both paths in the pathspec): app_compose, db_schema, import_pipeline, lifecycle, maintenance, pg_lifecycle, removal, search, sql_console, writer. Their own `#![cfg(feature = "test-support")]` lines stay valid as module attributes. Relative fixture paths change: `import_pipeline.rs:25 OUTLOOK_PST` and `app_compose.rs:122` use `env!("CARGO_MANIFEST_DIR")`-relative paths, so they need no edit. `golden/` stays at `tests/golden` (read via `CARGO_MANIFEST_DIR`).
- New `crates/outlooker-core/tests/cluster_lifecycle.rs`: the explicit exceptions of §2.6. These are the only tests that start their own clusters, because starting, stopping and killing PostgreSQL is the behavior under test.
- Lib inline `#[cfg(test)]` tests: none uses PostgreSQL (grep of `src/` for `TestCluster|test_support` hits only the module declaration at `src/lib.rs:16`). No change needed.
- Other users of `TestCluster::start`: `examples/import_bench.rs:61` (a `--release` example, one process, keeps working) and `outlooker-loadgen/tests/smoke.rs` (deleted, §3).

### 2.3 Fixture (`crates/outlooker-core/src/test_support/`)
**Template data root**, new file `template.rs`:
- `pub(crate) fn template_root() -> &'static Path` (`OnceLock<PathBuf>`), stored at `std::env::temp_dir()/outlooker-test-pg/<key>`. `key` = the first 16 hex chars of a blake3 hash over the sorted (relative path, bytes) of every file under `concat!(env!("CARGO_MANIFEST_DIR"), "/src/db")` and `/src/lifecycle`, plus the byte length of `dev_pg_dir()/bin/postgres`. Add a `// ponytail:` comment with the ceiling: a data-root layout change outside db/ and lifecycle/ does not invalidate the cache (clear it with `rm -rf $TMPDIR/outlooker-test-pg`).
- Build, only when `<dir>/data/outlooker-data.json` is missing, into `<dir>.tmp-<pid>` through the real production path: `SettingsStore::load`, then `Supervisor::new(.., SupervisorConfig::default())`, `boot()`, `initialize(InitializeRequest { data_dir: tmp/data, language: En })`, wait for `Ready` on `phase_watch()`, `shutdown_db()`. Then remove `tmp/app`, `tmp/logs` and `tmp/data/pgdata/log`, and `rename(tmp, dir)`. If the rename fails because another process won, `remove_dir_all(tmp)`. Last, remove sibling key directories.
- `pub fn seeded_root(dst: &Path)`: copies `template_root()/data` to `dst` with `crate::lifecycle::move_dir::copy_tree` (make it `pub(crate)`, `move_dir.rs:31`). `std::fs::copy` clones on APFS. This fn copies files and boots nothing. Only `cluster_lifecycle.rs` uses it.

**Shared cluster**, rewrite of `mod.rs`:
- `static RUN: OnceLock<Run>` with `Run { dir, port, creds }` (start once per process, no refcount), plus `static SEEDS: Mutex<HashSet<&'static str>>`. Lock with `unwrap_or_else(|p| p.into_inner())`.
- `fn boot_shared() -> Run`, private and not `pub` (the policy's visibility rule). It is called only through `RUN.get_or_init(boot_shared)`. After the start it registers an exit hook once. The hook needs no new dependency, because std already links the C runtime: `unsafe extern "C" { safe fn atexit(cb: extern "C" fn()) -> std::ffi::c_int; }`. The callback `stop_shared` runs `pg_ctl stop -m immediate -w -D <run>/pgdata` through `std::process::Command`, then `remove_dir_all(run.dir)`. libtest ends with `std::process::exit` or a normal return from main, and both run atexit handlers on Unix and in the Windows CRT. A SIGKILL (for example the 40 s cap) skips the hook; the next boot's sweep (step 1) removes the leftover. Honest cost: until that next run, a killed run leaves one idle postmaster (1 SysV segment, ~150 MB of mapped memory).
  Steps:
  1. Sweep: for each `temp_dir()/outlooker-test-pg-run-<pid>-*` whose pid is not ours and not alive (`sysinfo` `refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, ProcessRefreshKind::nothing())`, then `process(pid).is_none()`), run `pg_ctl stop -m immediate -w -D <d>/pgdata` and `remove_dir_all`.
  2. `seeded_root(dir)` with `dir = temp_dir()/outlooker-test-pg-run-<pid>-<gen>`.
  3. Append `TEST_CONF` to `dir/pgdata/postgresql.auto.conf`.
  4. `PgController::start` via `block_on` (below).
  5. As admin on database `postgres`: `ALTER ROLE outlooker_ro CONNECTION LIMIT -1`.
  `TEST_CONF = "fsync = off\nfull_page_writes = off\nmax_connections = 400\nshared_buffers = '128MB'\n"`. Do not override `synchronous_commit` (`writer.rs:112` asserts `on`) or any role setting (`sql_console.rs:131` asserts the role default `work_mem`).
- `fn block_on<F: Future + Send>(f: F) -> F::Output where F::Output: Send`: `std::thread::scope` spawns a thread that runs `f` on its own current-thread runtime. The shared state holds no clients or pools, so no tokio task is tied to one test's runtime.
- `pub async fn TestCluster::start() -> TestCluster` (signature unchanged, so the ~60 call sites need no edit). This is the only public way in, and it hands out a fresh database:
  1. `let run = RUN.get_or_init(boot_shared);` (the first caller boots; concurrent callers block on the OnceLock).
  2. `dbname = format!("t{}", NEXT.fetch_add(1))`. On `postgres`: `CREATE DATABASE {dbname} TEMPLATE outlooker STRATEGY FILE_COPY`, then `UPDATE pg_database SET datacl = (SELECT datacl FROM pg_database WHERE datname = 'outlooker') WHERE datname = $1`. CREATE DATABASE does not copy database grants; the migrations set them at `V2__roles_views.sql:9,47`, and `db_schema.rs:54,70` guard them.
  3. Per-test `TempDir`, `paths.with_data_root(dir/data)`, `create_dir_all(data_root)`. This is required: the writer's free-space guard fails closed on a missing path (`importer/writer.rs:385`).
  4. `creds.save(paths.credentials())`, then `db.set(Some(self.db_conn()))`.
- `pub async fn TestCluster::from_seed(name: &'static str, seed: fn(&TestCluster) -> BoxFuture<'_, ()>) -> TestCluster`. Under the `SEEDS` lock, if `name` is missing: create database `seed_{name}` from `outlooker`, run `seed` on a handle to it inside `block_on`, drop every connection to it, and insert `name`. Then create this test's database with `TEMPLATE seed_{name}` instead of `outlooker`. Reads share the seeded rows; writes stay private.
- Public methods: `admin()/app()/ro()`, which connect to `self.dbname`; `count()`; `db_conn() -> DbConn`, which builds new pools for this database (used to attach a Supervisor, §2.5); `cut_db()`, which replaces `kill()`; `restore_db()`, which replaces `restart()` (§2.6). Remove `kill`, `restart`, `controller()` and `locale`; all are dead after the rewrite (grep).
  - `cut_db`: `db.set(None)`, then on `postgres`: `ALTER DATABASE {db} ALLOW_CONNECTIONS false; SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{db}'`.
  - `restore_db`: `ALTER DATABASE {db} ALLOW_CONNECTIONS true`, then `db.set(Some(db_conn()))`.
- `Drop`: only `db.set(None)` (closes this test's pools). The per-test database stays until the exit hook deletes the run directory (peak disk ≈ tests × ~8 MB). The cluster never restarts during a run, at any thread count.
- How the merged binary's modules reach the fixture: each module file keeps `use outlooker_core::test_support::{TestCluster, count, png, import_fixtures::…};`. `it` links the lib as an external crate, so it sees only `pub` items: `TestCluster::start`, `TestCluster::from_seed`, the handle methods and the row helpers. `boot_shared`, `RUN`, `template_root` and `block_on` are private or `pub(crate)`, so a call from `tests/` fails with E0603. Policy check R3 (`test-policy-design.md:193`) must call `boot_shared`, not `TestCluster::start`: `start` no longer boots, it hands out a database.

**Production seams** (the contract allows "test-support seams a REWRITE needs"):
- E1 database name: `db/pools.rs` changes to `pool(port, user, pw, dbname, max)`, `Pools::build(port, creds, dbname: &str)`, and `DbConn { .., pub dbname: String }`. `Db::connect_admin` and `Db::connect_app` use `c.dbname` (`pools.rs:12,74,78`). The production callers pass `DB_NAME`: `lifecycle/start.rs:45-46` and `outlooker-loadgen/src/root.rs:29`.
- E2 attach (precedent: `fail_next_move_verify` at `supervisor.rs:150` is already `#[cfg(any(test, feature = "test-support"))]`): a new `Inner.attached: Mutex<Option<DbConn>>` and `#[cfg(feature = "test-support")] pub fn attach_test_db(&self, c: DbConn)`. In `start.rs:try_start`, as the first statement: `#[cfg(feature = "test-support")] if let Some(c) = attached.clone() { return Ok(c) }`. In `supervisor.rs:shutdown_db`, after `db.set(None)`: `#[cfg(feature = "test-support")] if attached.is_some() { return Ok(()) }`. Effect: `boot`, `start_services`, `shutdown` and move validation run against the test's own database on the shared cluster, and PostgreSQL itself is never started or stopped. `boot()` still requires the marker file, so the helper writes `{data_root}/outlooker-data.json` (`{}`) and the bootstrap settings.

### 2.4 Isolation guarantees and limits
- Each test owns: its database (tables, sequences, indexes, `pg_stat_user_tables`, catalogs such as pg_index and pg_class), its data root and its credentials file. `CREATE INDEX` / `REINDEX CONCURRENTLY` and VACUUM wait only on their own database.
- All tests share: roles and role settings, `pg_stat_activity`, the port and the postmaster. The only current violator of the rule below is `sql_console.rs:225` (fixed in §3).
- **Shared-cluster rule (for the policy implementer, verbatim):** "A test on the shared PostgreSQL cluster never runs ALTER ROLE, ALTER SYSTEM, or DROP/ALTER of another test's database, and it terminates or cancels backends only with `datname = current_database()`."
- `outlooker_ro CONNECTION LIMIT 2` (`V1__schema.sql:3`) applies cluster-wide, so the shared clone lifts it. No test asserts the limit (grep `53300|connlimit` finds nothing). `max_connections = 20` (`lifecycle/pg.rs:65`) becomes 400 for up to 28 tests × (app pool 8 + ro 2 + writer + admin).

### 2.5 SysV shared memory and thread count
`kern.sysv.shmmni = 32`, `hw.ncpu = 28`. Each postmaster holds one SysV segment. After the change, `it` runs one cluster and `cluster_lifecycle` runs at most 4 (one per test), so a full run uses at most 5 segments. The shm limit no longer constrains `--test-threads`. Recommendation: drop `-- --test-threads 4` and use the libtest default (28 here); `TEST_CONF max_connections = 400` covers it. The `outlooker-test-env-traps` memory entry and any gate prompts that pass `--test-threads 4` should be updated.

### 2.6 Postmaster-kill tests and the exception list
| test | core? | decision |
|---|---|---|
| import_pipeline.rs:755 pg_crash_mid_import_resumes | yes (crash recovery) | MERGE → 779 |
| import_pipeline.rs:779 pg_crash_failed_then_retry_resumes | yes | REWRITE on the shared cluster. `tc.cut_db()` replaces `kill()`: the importer sees what a PostgreSQL crash looks like to it (connections terminated with 57P01, new connections refused, `db_err` → `DbNotReady`, `db/mod.rs:41-45`). `tc.restore_db()` replaces `restart()`. The phase sequence is unchanged. |
| import_pipeline.rs:583 resume_after_kill_equals_uninterrupted | yes | no postmaster kill: it uses the importer crash seam `crash_after_batches`. Stays shared. |
| lifecycle.rs:175/185 health restart, restart budget | yes (auto-restart after a DB crash, then stop retrying) | EXCEPTION X2: the behavior is "the health monitor sees a dead postmaster and starts a new one", so a real postmaster must die |
| lifecycle.rs:94/107/143 first run, adopt, retry from Failed | yes (onboarding, reinstall) | EXCEPTION X1: initdb and cluster start are the behavior |
| lifecycle.rs:199/229/248 data-dir move, rollback | yes (moves the user's library; rollback must not lose data) | EXCEPTION X3: stop, copy, start of a real pgdata |
| pg_lifecycle.rs:57/93/103/117 orphan postmaster, pid reuse, port conflict, live conf | yes (start after an app crash or a reboot) | EXCEPTION X4 |
| app_compose.rs:120/174 | yes | REWRITE on the shared cluster via seam E2 (no exception) |

#### Exception list for the user (verbatim-ready)
These tests check that the app itself can create, start, restart, move and take over its database. Those actions cannot happen on the one shared database server without breaking every other test, so each of these tests starts its own small PostgreSQL. Each takes 1–3 s, above the 1 s cap.
| # | test | what it protects | why the shared server cannot serve it | runtime | if deleted |
|---|---|---|---|---|---|
| X1 | first_run_reaches_ready (absorbs adopt and retry) | first launch creates a working library; a reinstall adopts the old library; after an unsupported-version error, "retry" recovers | creating a new database server (initdb) is the behavior under test | ~2–2.5 s | first-run setup, adopt and retry are untested; a broken onboarding would show up only for real users |
| X2 | restart_budget_exhaustion_fails (absorbs health restart) | when the database process dies, the app restarts it, and after repeated deaths it stops and reports an error | the test must kill a real database process; killing the shared one kills every test | ~1–1.5 s | the automatic crash restart is untested |
| X3 | move_data_dir_succeeds (absorbs 2 rollback tests) | moving the library to another folder works, and a failed move restores the old one without data loss | the move stops the server, copies its files and starts it elsewhere | ~2–3 s | a library move or rollback could lose user data unnoticed |
| X4 | orphan_postmaster_is_restarted (absorbs pid reuse, port conflict, live config) | after a crash or a reboot the app can start its database even when an old process, a reused process id or a busy port is in the way | it needs a stopped data folder and real server starts | ~1 s | starting after a crash or reboot is untested |
Not an exception: C-locale rejection is rewritten onto the shared server (a C-locale database fails the CJK self-check). It loses one assert, "a rejected setup leaves no folder behind". To keep that assert it would become exception X5 (one initdb, ~1 s).

Exception binary `tests/cluster_lifecycle.rs`, 4 tests. **User approval required.** In plain words: these four tests check that the app itself can create, start, restart, move and take over a PostgreSQL database. Faking the database there would test nothing, so each starts its own small PostgreSQL. Each takes 1–3 s, above the 1 s cap, so the policy runner needs a higher cap for this one binary (proposal: 3 s via `RUST_TEST_TIME_INTEGRATION` for `cluster_lifecycle` in `scripts/cargo-test-runner.mjs`).
- X1 `first_run_reaches_ready` (merges adopt 107 and retry 143): run the real `initialize` (initdb), assert the phases and the marker, `shutdown_db`; a second rig adopts the same root → Ready, `shutdown_db`; write PG_VERSION 17 → the third rig's `boot` fails; write 18 → `retry` → Ready → `shutdown_db`. One initdb plus 2 starts, ~2–2.5 s.
- X2 `restart_budget_exhaustion_fails` (merges 175): `seeded_root`, `SupervisorConfig { restart_budget: 1, probe_interval: 100 ms }`. Kill → StartingDb → Ready (this is the restart proof), kill → Failed (`health.rs:57`: `len > budget`). ~1–1.5 s.
- X3 `move_data_dir_succeeds` (merges 229 and 248): `seeded_root`, boot. First, a move with `fail_next_move_verify` into a pre-existing empty target → Failed, the target is kept and empty. Second, a move with fail-verify into a fresh target → the target is removed and `auth/` is unchanged. Third, a real move → Done, then the marker, bootstrap and old-root asserts. ~2–3 s.
- X4 `orphan_postmaster_is_restarted` (merges 93, 103, 117): `seeded_root(paths.data_root())`, admin password from `Credentials::load`. (a) Fake a pid file with our own live pid → start succeeds; the SHOW checks of 117 run there. (b) Leave it running; a second `PgController` starts → the orphan is stopped and replaced. (c) Stop; `with_ports([busy])` → a different port is used. Stop. ~1 s.
- If the user rejects the exception, the alternative is DELETE of X1–X4. Onboarding, crash restart, data-dir move and start recovery would then be untested.

## 3. Decision table
Paths are relative to `src-tauri/crates/`; line numbers are pre-move. "fixture" means the body is unchanged and the speedup comes from §2.

### outlooker-core/tests/import_pipeline.rs → tests/it/import_pipeline.rs
| line | test | decision | reason |
|---|---|---|---|
| 222, 256, 288 | outlook_pst_import, byte_copy_dedupes_and_removal, corrupt_copy_outcome | KEEP (fixture) | real-PST import, end-to-end remove job plus content-hash dedupe of byte copies, corrupt item skip |
| 323 | synthetic_cjk | DELETE | Big5 via codepage 950 = `normalize/charset.rs:94`; `normalize/mod.rs:80` only passes it through; stored CJK found by search = `search.rs:68` |
| 352 | synthetic_inline_images | KEEP (fixture) | only test of blob dedupe across mails plus dangling refs in the DB |
| 397 | synthetic_attachments | MERGE → `normalize/attachments.rs:77` attachment_rules | move the one unique assert: `assert_eq!(long.mime_type, "application/octet-stream")` |
| 426 | synthetic_hostile | DELETE | the same `hostile.html` through the same sanitizer = `sanitize.rs:182`. Move `hostile_violations` into `mod fixtures`, or clippy flags it as dead code. |
| 437 | synthetic_message_id_dedupe | DELETE | = `writer.rs:46` + `writer.rs:165` + `dedupe.rs:110` |
| 480, 505, 536 | index_policy_* | KEEP (fixture) | three distinct rules; 536 is a MERGE target |
| 550 | after_job_vacuum_ran | MERGE → 536 | inventory overturned: `maintenance.rs:48` tests the Maintenance VACUUM, not the importer's after-job VACUUM. Assert the SQL count is 0 before and 1 after the first import of 536. |
| 562, 583 | resume_after_cancel / resume_after_kill_equals_uninterrupted | REWRITE | replace `reference()` (a second full import) with a pure `expected(n)`: the message-id set `{m{i}@synthetic.example : i%10 != 0}` plus counts. ITEMS 40→20, hold 20→10. |
| 606 | mtime_change_resets_cursor | KEEP (fixture) | |
| 688, 696, 708, 715 | poison_marker_* | KEEP (fixture) | one small import each; merging them would put 4 imports in one test |
| 725 | retry_rewrites_marker_before_reading | KEEP | 0.04 s |
| 755 | pg_crash_mid_import_resumes | MERGE → 779 | same kill → StartingDb → restart → Ready path; move the duplicate-occurrence assert into 779 |
| 779 | pg_crash_failed_then_retry_resumes | REWRITE | `cut_db`/`restore_db` (§2.6), `expected(20)`, hold 15, `wait_batch(.., 2)`, plus the dup assert |
| 801 | recover_after_crash_states | KEEP (fixture) | |
| 840 | pst_list_status_derivation | REWRITE | inventory DELETE overturned: `queue.rs:561` covers the 9 single-job rows but not "newest job wins" or the list order. Keep only the two-job row and the order block. |
| 898, 913 | pst_remove_with_queued_job, enqueue_validation | KEEP (fixture) | the 8 s was contention |
| 966–1028 | fixtures_* (7) | KEEP | `outlook-fixtures` feature, not in the default run; they move with the file into `it` |

Remove the `reference()` helper; `Snapshot.keys` becomes the message-id set.

### writer.rs, sql_console.rs, search.rs, db_schema.rs, maintenance.rs, removal.rs → tests/it/
| file:line | test | decision | reason |
|---|---|---|---|
| writer.rs:59 | content_hash_crlf_vs_lf | DELETE | CRLF == LF key = `dedupe.rs:110`; the conflict path = writer.rs:46/165 |
| writer.rs:70/83/96 | batch_cut_by_items/bytes/time | KEEP (fixture) | inventory overturned: the cut rule sits inline in the async loop (`importer/writer.rs:364-378`) |
| sql_console.rs:65 | read_only_role_rows | REWRITE | it really waits 30 s (`SET LOCAL statement_timeout = 30000`, `sql_console/mod.rs:154`). Replace the `pg_sleep(40)` block with `SELECT current_setting('statement_timeout') AS t` → `Table [[Some("30s")]]`; 57014 → `QueryTimeout{30000}` = `execute.rs:100`. No production change. |
| sql_console.rs:170 | one_execution_at_a_time | REWRITE | `pg_sleep(10)`, then after the `alreadyRunning` assert `cancel("a1")` → `Err(Cancelled)`, then `a3` ok |
| sql_console.rs:213 | killed_backend_is_replaced_by_a_fresh_connection | REWRITE | add `AND datname = current_database()` (line 225) |
| sql_console.rs:88 | cancel_during_sleep | KEEP (fixture) | the statement is cancelled at once |
| search.rs:174 | count_capped | REWRITE | seed by SQL: `INSERT INTO messages (dedupe_key, dedupe_kind, body_source, subject, sent_at) SELECT decode(lpad(to_hex(g),64,'0'),'hex'), 'content_hash', 'none', 'capword ' \|\| g, to_timestamp(1600000000+g) FROM generate_series(1,10001) g` (the count reads only `messages`, `search/execute.rs:51`) |
| search.rs:232, 242, 252, 269, 291 | two_phase_* (3), null_sent_at_last_and_paged, reset_after_phase_b | REWRITE | `TestCluster::from_seed("window", seed_window_set)` replaces `start()` + `seed_window_set` (§4) |
| search.rs other 10 | | KEEP (fixture) | |
| db_schema.rs:106 | db_handle_not_ready_when_unset | DELETE | unset Db = `lifecycle.rs:304`; closed pool = `db_schema.rs:117` |
| db_schema.rs other 8, maintenance.rs (3) | | KEEP (fixture) | `maintenance.rs:48` already seeds only 2 rows; the inventory REWRITE reason did not hold |
| removal.rs:61 | second_run_is_noop | MERGE → removal.rs:37 | same setup; append the second-run and remove-b asserts |

### app_compose.rs → tests/it/app_compose.rs (seam E2)
| line | test | decision | reason |
|---|---|---|---|
| 120 | shutdown_during_import_requeues_job | REWRITE | `tc = TestCluster::start()`; a helper writes the marker plus bootstrap for `tc.paths.data_root()`; `app::build`, `s.supervisor.attach_test_db(tc.db_conn())`, `start_services`. The flow is the same up to `app::shutdown`. Verify through `tc.admin()`: the job is `queued` and the checkpoint row exists. Drop the `PgStatus::Stopped` assert; the real stop is covered by X1/X3. |
| 174 | start_after_kill_recovers_jobs_then_heals_then_starts_worker | REWRITE, MERGE target | mutate through `tc.admin()` (no first boot), attach, `start_services`, same asserts; add 220's `indisvalid` assert |
| 220 | missing_index_is_rebuilt_before_worker | MERGE → 174 | 174 already drops an index and asserts heal-before-worker |

Delete the `first_run` and `stop_like_a_kill` helpers.

### lifecycle.rs → tests/it/lifecycle.rs (non-PG and attached) plus cluster_lifecycle.rs (X1–X3)
| line | test | decision | reason |
|---|---|---|---|
| 94 | first_run_reaches_ready | REWRITE → X1 (exception) | absorbs 107 and 143 |
| 107 | adopt_existing_data_root | MERGE → X1 | |
| 120 | wrong_pg_version_fails | REWRITE, stays in `it` | no PostgreSQL needed: write `outlooker-data.json` and `pgdata/PG_VERSION = 17` into a temp root; `boot` fails before any start (`start.rs:28-35`) |
| 133, 158, 165, 295, 304, 312 | | KEEP | no PostgreSQL; stay in `it` |
| 143 | retry_from_failed_reaches_ready | MERGE → X1 | |
| 175 | health_restarts_killed_postmaster | MERGE → X2 | |
| 185 | restart_budget_exhaustion_fails | REWRITE → X2 (exception) | |
| 199 | move_data_dir_succeeds | REWRITE → X3 (exception) | absorbs 229 and 248 |
| 229, 248 | move_verify_failure_rolls_back, move_rollback_keeps_pre_existing_empty_target | MERGE → X3 | |
| 263 | move_into_non_empty_is_invalid | REWRITE → `move_rejections` in `it` | the checks run before any stop (`move_dir.rs:82-99`), so an attached Ready Supervisor (seam E2) is enough |
| 274, 285 | move_into_own_subfolder_is_invalid, move_while_import_is_busy | MERGE → move_rejections | |

`rig_at` gains a `SupervisorConfig` parameter. The X tests use `seeded_root`.

### pg_lifecycle.rs → tests/it/pg_lifecycle.rs (c_locale only) plus X4
| line | test | decision | reason |
|---|---|---|---|
| 24 | init_passes_cjk_self_check | DELETE | X1 reaches Ready through the same `PgController::init` (self-check in `pg.rs:255-267`) |
| 35 | c_locale_is_rejected | REWRITE on the shared cluster | `CREATE DATABASE c_loc TEMPLATE template0 LOCALE 'C'`, `CREATE EXTENSION pg_trgm`, `CJK_SELF_CHECK` → false (pg_trgm classifies characters with the database's LC_CTYPE). Lost: the assert that a rejected candidate leaves no pgdata. The rejection classifier itself is `pg.rs:388`. |
| 43 | stop_start_round_trip | DELETE | covered by X4 |
| 57 | orphan_postmaster_is_restarted | REWRITE → X4 (exception) | |
| 77 | stale_pid_with_dead_process | DELETE | the code path is `PidOwner::Dead => Ok(())` (`pg.rs:275`); PostgreSQL itself clears the stale lock, so the test checks PostgreSQL, not our code |
| 93, 103, 117 | pid_reused…, port_conflict_retries, auto_conf_values_are_live | MERGE → X4 | inventory "duplicate" for 117 overturned: it checks the live server; it runs on X4's seeded clone |

### outlooker-core/src lib unit tests (no PostgreSQL)
| file:line | test | decision | reason |
|---|---|---|---|
| db/mod.rs:114 | pool_closed_is_not_ready | DELETE | one-line mapping; = `db_schema.rs:117` |
| dto/app.rs:60, dto/error.rs:64 | language_and_phase_wire_values, app_error_wire_shape | DELETE | the wire shape is pinned by the committed ts-rs bindings (same serde attributes) and by `npm run typecheck` |
| dto/search.rs:211 | worked_example_matches_spec_json | DELETE | same reason; behavior = `compile.rs:404`, `search.rs:126`; keep the `worked_example()` helper |
| dto/import.rs:159 | import_phase_db_round_trip | KEEP, rename to `db_text_round_trips`, MERGE target | |
| dto/import.rs:164/169, dto/search.rs:174 | stage, severity, recipient_kind round-trips | MERGE → db_text_round_trips | one helper, one test |
| gate/mod.rs:116 | guard_drop_releases | MERGE → gate/mod.rs:106 | append `current() == None` and a re-enter assert |
| importer/progress.rs:108 | throughput_and_eta | DELETE | UI-only numbers |
| importer/queue.rs:580/588 | job_kind/job_state round-trips | MERGE → queue.rs:561 status_table | |
| lifecycle/pg.rs:375 | last_log_line_skips_startup_log | DELETE | error-text cosmetics |
| paths/mod.rs:65 | paths_layout | DELETE | path joins; every DB test exercises them |
| sink.rs:83 | recording_sink_keeps_order_and_names | DELETE | tests a test helper |
| queue.rs:604, writer.rs:547, init.rs:109, execute.rs:68, execute.rs:79, attachments.rs:77, commands.rs:15, events.rs:74 and all other unit tests | | KEEP | core logic or codegen, < 0.1 s |

### ts-rs `export_bindings_*` (49)
KEEP. They are declaration-derived (`#[ts(export)]`) and cost ~0.1 s total. Their write to `src/api/bindings` is the drift signal. An explicit export command would need a hand-kept list of 49 types.

### outlooker-loadgen, pff, root crate
| file:line | test | decision | reason |
|---|---|---|---|
| outlooker-loadgen/tests/smoke.rs:8, :27 | smoke_10k, seeded_root_starts | DELETE (delete the file) | 10k load is perf work (the ignored suite); the e2e seed runs in every e2e run (`tests/e2e/prepare.mjs:45`) |
| outlooker-loadgen/src/generate.rs:264, :299; corpus.rs:92; images.rs:76; rng.rs:83, :99 | 6 loadgen self-tests | DELETE | tooling, not product logic (264 took 9.4 s) |
| outlooker-loadgen/tests/perf/support.rs:155 | percentile_nearest_rank_and_canned_set | KEEP | wrong percentile math would pass the perf gates silently |
| outlooker-loadgen/tests/perf.rs (14) | perf_* | KEEP `#[ignore]` | the user's perf gates |
| pff/tests/outlook_pst.rs:52 | version_is_20260625 | DELETE | constant string |
| pff/tests/outlook_pst.rs:101 | filetime_conversion | DELETE | `pst_reader.rs:375` tests the same `pff::filetime_to_unix_micros` |
| pff others (4), src/lib.rs (2), src/mailimg.rs (5) | | KEEP | |

## 4. Shared expensive setups
| setup | where it repeats | today | shared design | saved |
|---|---|---|---|---|
| PostgreSQL initdb + start + self-check + migrate | every `TestCluster::start` (~60 call sites in 7 files) and ~25 lifecycle/pg_lifecycle/app_compose tests | ~1.3–1.6 s each, plus contention | template data root, built once per source change (§2.3); one shared cluster per run; a fresh FILE_COPY database per test (0.01–0.1 s) | ~85 boots → 1 (+ exceptions); ~120 s CPU, ~100 s wall |
| migrations per test | inside the row above | part of it | migrated once into the template database `outlooker` | included |
| second "reference" import | `import_pipeline.rs:191 reference()` used by 562, 583, 755, 779 | a full cluster plus a 40-item import each | pure `expected(n)` computed from the corpus | 4 boots + 4 imports |
| the same 100-mail seed | `search.rs:51 seed_window_set` used by 232, 242, 252, 269, 291 | 5 writer runs | `TestCluster::from_seed("window", ..)`: seeded once into `seed_window`, each test gets a FILE_COPY of it (read-only tests share the rows, writes stay private) | 4 writer runs (~0.1–0.2 s each) |
| 10k-row seed | `search.rs:174 count_capped` (10,001 writer items); `smoke.rs:8 smoke_10k` | 3.2 s; 25.7 s | count_capped: one `generate_series` INSERT (the test deletes rows, so it keeps its own database); smoke_10k deleted | ~3 s; ~26 s |
| Supervisor initialize to get a ready data root | lifecycle.rs (12 tests), app_compose.rs (3), pg_lifecycle.rs (7) | one initdb each | `seeded_root` clone of the template (X2–X4); attach seam E2 for app_compose and move validation | ~20 initdb runs |
| external `pg_ctl` / `initdb` launches | every boot above | 4–6 launches per test | 1 start per run in `it`; the exceptions only | included above |
| outlook.pst parse | import_pipeline 222, 256, 288, 688–725, 913; pff tests | pff's 6 tests total 0.01 s, so a parse costs a few ms | no change: parsing is the behavior under test, and sharing would save milliseconds | ~0 |
| writer / ImportManager runtime per test | writer.rs `start_writer`, import_pipeline `manager()` | microseconds to milliseconds | no change: per-test state is the isolation | ~0 |
| tokio runtime per test | every `#[tokio::test]` | ~1 ms | no change | ~0 |
| ts-rs exports | 49 generated tests | 0.1 s total | no change | ~0 |
| perf corpus generation | perf.rs (ignored, `--release` only) | not in the default run | out of scope | — |

## 5. Work packages (round 3, two sonnet members, disjoint file ownership)
Order: A1 must land before B2. A sends `FIXTURE_READY <hash>` to B and to the lead. Commit in compile-green steps, because both packages build the same core lib.
Binary merge ownership: no Cargo.toml changes (autodiscovery, §2.1). Every `crates/outlooker-core/tests/*.rs` file is moved by Package A in step A1 and by nobody else: 10 `git mv` into `tests/it/` plus `tests/it/main.rs`. Once A1 is committed, ownership of `tests/it/{lifecycle,pg_lifecycle,app_compose}.rs` passes to B, and A never edits them again. B creates `tests/cluster_lifecycle.rs`. `tests/golden/` is not moved. Outside outlooker-core, `outlooker-loadgen/tests/smoke.rs` (B deletes it), `outlooker-loadgen/tests/perf*` and `pff/tests/outlook_pst.rs` start no PostgreSQL or none in the default run, and they stay in their own crates.

### Package A — fixture, seams, merged binary, DB test files
Owned files:
- `outlooker-core/src/test_support/{mod.rs, template.rs}`
- `outlooker-core/src/db/pools.rs`
- `outlooker-core/src/lifecycle/{start.rs, supervisor.rs, move_dir.rs}`
- `outlooker-loadgen/src/root.rs`
- `outlooker-core/tests/it/main.rs`
- `outlooker-core/tests/it/{import_pipeline,writer,sql_console,search,db_schema,maintenance,removal}.rs`

Steps:
1. A1: seams E1 and E2; `copy_tree` `pub(crate)`; `template.rs`; `mod.rs` per §2.3. Create `tests/it/main.rs` as in §2.2. `git mv` all 10 files into `tests/it/`, including B's three (lifecycle, pg_lifecycle, app_compose); A moves them without editing their content. Fix `sql_console.rs:225`. Gate: `cargo test --workspace --test it` green. B's moved lifecycle tests still boot their own clusters at this point, which is expected until B2. Commit, then send `FIXTURE_READY`.
2. sql_console 65, 170; search 174 + the 5 `from_seed` tests; writer 59; db_schema 106; removal 61 → 37.
3. import_pipeline rows of §3.

Acceptance (from `src-tauri/`):
- `cargo test --workspace --test it` green.
- Budget: `zsh ../scripts/tmp/test-inventory/rt.sh crates/outlooker-core it`, then `awk '/ ok </{t=$NF; gsub(/[<>s]/,"",t); if (t+0 > 1.0) print}' ../scripts/tmp/test-inventory/rt-it.txt` prints nothing. Judge the second run (warm template).
- One boot: `grep -rn "TestCluster::start\|from_seed" crates --include=*.rs | grep -v "tests/it/\|src/test_support\|examples/import_bench"` prints nothing, so no other binary uses the fixture. After `cargo test --workspace --test it`: `pgrep -fl outlooker-test-pg-run` prints nothing (the exit hook stopped the cluster) and `ls "$TMPDIR" | grep -c outlooker-test-pg-run` is 0. For the one-start proof, add a temporary `eprintln!("SHARED_BOOT")` in `boot_shared`: `cargo test --workspace --test it -- --nocapture 2>&1 | grep -c SHARED_BOOT` = 1. Remove it before commit.
- Visibility: create a temporary `tests/zz_boot.rs` containing `fn main(){}` and `#[test] fn t(){ outlooker_core::test_support::boot_shared(); }`. `cargo test --workspace --test zz_boot` must fail with E0603 (private) or E0425. Then delete the file and leave it uncommitted.
- Absent: `grep -rnE "fn (synthetic_cjk|synthetic_hostile|synthetic_message_id_dedupe|synthetic_attachments|after_job_vacuum_ran|pg_crash_mid_import_resumes|content_hash_crlf_vs_lf|db_handle_not_ready_when_unset|second_run_is_noop)\b|pg_sleep\(40\)|pg_sleep\(2\)|fn reference\(" crates/outlooker-core/tests` prints nothing.

### Package B — exceptions binary, lifecycle/app_compose modules, lib unit tests, loadgen, pff
Owned files:
- `outlooker-core/tests/it/{lifecycle,pg_lifecycle,app_compose}.rs` (after A1 moves them)
- `outlooker-core/tests/cluster_lifecycle.rs` (new)
- the test modules of `outlooker-core/src/{db/mod.rs, dto/app.rs, dto/error.rs, dto/import.rs, dto/search.rs, gate/mod.rs, importer/progress.rs, importer/queue.rs, importer/normalize/attachments.rs, lifecycle/pg.rs, paths/mod.rs, sink.rs}`
- the test modules of `outlooker-loadgen/src/{generate,corpus,images,rng}.rs`
- `outlooker-loadgen/tests/smoke.rs` (`git rm`)
- `pff/tests/outlook_pst.rs`

Steps:
1. B1 (independent): the lib unit-test, loadgen and pff rows. Gate: `cargo test --workspace --lib` and `cargo test --workspace --test outlook_pst` green. Commit.
2. Wait for `FIXTURE_READY`: end the turn, no polling.
3. B2: `cluster_lifecycle.rs` X1–X4; strip the moved PG tests out of `tests/it/lifecycle.rs` and `tests/it/pg_lifecycle.rs`; the `it/lifecycle.rs` rows (wrong_pg_version, move_rejections, KEEPs); `pg_lifecycle.rs` c_locale; the `app_compose.rs` rows.

Acceptance (from `src-tauri/`):
- `cargo test --workspace --lib` green. `cargo test --workspace --test it`, `--test cluster_lifecycle`, `--test outlook_pst` and `--test perf` (1 passed, 14 ignored) green.
- Budget via `rt.sh`: `it` has no test > 1.0 s; `cluster_lifecycle` has no test > 3.0 s (pending the exception approval).
- Absent: `grep -rnE "fn (language_and_phase_wire_values|app_error_wire_shape|worked_example_matches_spec_json|recipient_kind_db_round_trip|import_error_stage_db_round_trip|import_error_severity_db_round_trip|guard_drop_releases|throughput_and_eta|job_kind_db_round_trip|job_state_db_round_trip|last_log_line_skips_startup_log|paths_layout|recording_sink_keeps_order_and_names|pool_closed_is_not_ready|deterministic_items_and_duplicate_plan|e2e_message_in_seed_config|word_lists_and_text|deterministic_and_known_skips_encoding|deterministic_and_bounded|zipf_rank_zero_is_most_frequent|version_is_20260625|filetime_conversion|stop_start_round_trip|init_passes_cjk_self_check|stale_pid_with_dead_process|health_restarts_killed_postmaster|adopt_existing_data_root|retry_from_failed_reaches_ready|move_verify_failure_rolls_back|move_rollback_keeps_pre_existing_empty_target|move_into_own_subfolder_is_invalid|move_while_import_is_busy|pid_reused_by_live_non_postgres_process|port_conflict_retries|auto_conf_values_are_live|missing_index_is_rebuilt_before_worker)\b" crates` prints nothing. `test -e crates/outlooker-loadgen/tests/smoke.rs` fails.
- `grep -c "#\[tokio::test\|#\[test\]" crates/outlooker-core/tests/cluster_lifecycle.rs` = 4.

### Final gate (lead or gate-runner, after both)
`cargo test --workspace` green; `cargo clippy --workspace --all-targets -- -D warnings` green; rt.sh second run within budget; A's leak check.

## 6. Predicted after-state
- Test binaries that touch PostgreSQL: `it` (1 cluster) and `cluster_lifecycle` (the 4 approved exceptions). PostgreSQL startups per warm run: 1 shared + ~9 in the exceptions (X1 initdb + 2 starts, X2 2, X3 4, X4 3). Today: ~85 initdb + ~170 starts.
- About 203 tests executed by default, 14 ignored.
- `cargo test --workspace` test-execution wall-clock (warm template, excluding compile). Today: ~150–180 s.
  - default threads (28 here): 6–10 s. `it` ~3–5 s (~70 DB tests at 0.1–0.4 s plus a one-time 0.5 s boot: clone 0.36 s + start 0.1 s), `cluster_lifecycle` ~3 s (4 tests in parallel, the longest is X3), core lib ~0.5 s, the rest < 0.1 s each.
  - `-- --test-threads 4` (the current gate habit): 9–14 s. `it` ~5–8 s (~70 × 0.25 s / 4 plus the boot), `cluster_lifecycle` ~3–4 s.
  - A cold template (after a db/ or lifecycle/ source change) adds ~1.5 s once.
  - Process exit: the exit hook's `pg_ctl stop -m immediate` adds ~0.1 s per run.
- These numbers are estimates from the PostgreSQL step timings of §1, not measured runs (core build artifacts were missing from `target/`).

## 7. Risks and user decisions
1. USER DECISION: the exception binary `cluster_lifecycle` (§2.6). Four tests start their own PostgreSQL because starting it is what they test. Each runs 1–3 s, so they need a 3 s cap for that binary. The alternative is to delete them.
2. USER DECISION / policy coupling: a cold template build (~1.5 s, only after a db/ or lifecycle/ source change) lands inside whichever test first needs the cluster, and the 1 s `--ensure-time` cap would fail that test once. Proposed fix for policy-opus: before running the `it` and `cluster_lifecycle` binaries, the runner script (`scripts/cargo-test-runner.mjs`) runs `<bin> --ignored --exact warm_template` once without a time cap. This needs a `#[test] #[ignore] fn warm_template()` in `tests/it/main.rs` that calls a `pub fn warm_template()` in test_support; that fn builds files only and boots nothing.
3. Production seams E1 (database name, `start.rs`, loadgen `root.rs`) and E2 (attach, `start.rs`, `supervisor.rs`, test-support feature only) touch production files. The contract's "test-support seams" clause covers them; listed so the user sees them.
4. `cut_db`/`restore_db` simulate a crash for one database. A connection that was already open during a real crash gets an I/O error instead of 57P01; both map to `DbNotReady` (`db/mod.rs:31-48`), so the importer path is the same.
5. `UPDATE pg_database SET datacl` is a direct catalog write (superuser). Fallback: the two GRANT/REVOKE statements of `V2__roles_views.sql:9,47`; `db_schema.rs:54,70` catch either failure.
6. Windows: NTFS copies files instead of cloning them, so the boot in `it` and the X tests are slower there (estimated +0.5–1.5 s each). The budget is enforced on the macOS gate.
7. Future tests must not touch cluster-global state (§2.4). This rule goes to policy-opus.
8. The exit hook spawns `pg_ctl` from an `atexit` callback. This is not yet proven on this toolchain. If it misbehaves (for example on Windows), the fallback is the boot-time sweep alone, which leaves one idle postmaster per run until the next run starts. The warm boot (~0.5 s) is charged to the first test that needs it, which stays under 1 s.
9. Losses accepted by rewrites: the `PgStatus::Stopped` assert after app shutdown (the real stop is covered by X1/X3), and the "rejected locale leaves no pgdata" assert (c_locale).

## 8. Parking lot
- `lifecycle/init.rs:13 rfc3339_utc` and `importer/dedupe.rs:80 format_utc_seconds` are two implementations of the same formatter (production duplication).
- Production `try_candidate` uses the default WAL_LOG strategy for CREATE DATABASE; FILE_COPY is faster on first run.
- `target/debug/deps` held no outlooker-core artifacts at 12:38, so round 3 begins with a full core rebuild that exceeds the 40 s cap (use the 150 s build allowance or a gate-runner).
