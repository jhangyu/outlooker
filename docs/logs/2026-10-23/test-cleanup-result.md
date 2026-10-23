# Test-suite pruning: result (2026-10-23, HEAD 52bb6b8)

Verdict: criteria 1-5, 5b and 7 pass. **Criterion 6 FAILS** and 5a is only partly met, because of one reproducible defect (D1 below). Gate artifacts: `scripts/tmp/final-gate/` (gitignored, this machine). Audit scratch: `scripts/tmp/final-audit/`.

## Before / after

| | before (d32b4e0) | after (52bb6b8) |
|---|---|---|
| Rust tests executed (default run) | 250 (lib 140, integration 110), 14 ignored | 206 executed, 16 ignored (14 perf + 2 `warm_template`), plus 1 doctest |
| Rust test-execution wall | ~150-180 s, every PG test boots its own cluster | full `cargo test --workspace` 38 s (test-1, load 12, includes the runner, one shared boot, solo reruns) |
| Slowest Rust test | `read_only_role_rows` 31.5 s, `smoke_10k` 25.7 s; 51 tests over 2 s | no first run over 2.4 s alone (`index_policy_first_import_drops_and_rebuilds`, run 3, load 8) |
| Vitest | 79 cases in 18 files, ~12.2 s, slowest 2729 ms | 53 tests in 17 files, 4.06-5.15 s (npm-1..3), slowest 530 ms on a quiet machine (handoff fe-q-1..3) |
| e2e | 7 cases in 3 specs | 6 (scroll spec deleted) |

## WARNING lines (passed while busy, solo time over the 1 s cap)
Full run test-1 (load 11-16 / 28 cores):
- app_compose::shutdown_during_import_requeues_job 1.579 s
- import_pipeline::byte_copy_dedupes_and_removal 1.295 s
- import_pipeline::index_policy_first_import_drops_and_rebuilds 1.026 s
- import_pipeline::mtime_change_resets_cursor 1.342 s
- import_pipeline::pg_crash_failed_then_retry_resumes 1.519 s
- import_pipeline::resume_after_cancel_equals_uninterrupted 1.53 s
- import_pipeline::resume_after_kill_equals_uninterrupted 1.894 s

Run 3 `it` (incomplete file, 76 passed) adds: index_policy_incremental_keeps_indexes 1.156 s, synthetic_inline_images 1.892 s, lifecycle::move_rejections 1.955 s, and index_policy_first_import 2.4 s. Per CLAUDE.md, tests that keep appearing here (the import_pipeline group) should be made smaller. `npm test`: 0 warnings in 3 runs.

## Criteria
| # | Status | Evidence |
|---|---|---|
| 1 inventories | PASS | test-inventory-rust.md, test-inventory-frontend.md exist (committed with this doc) |
| 2 plan | PASS | test-cleanup-plan.md plus -rust.md and -frontend.md, per-test decisions with reasons |
| 3 design | PASS | test-policy-design.md |
| 4 mechanism | PASS | CLAUDE.md Test policy; `src-tauri/.cargo/config.toml:6-11` (runner, RUST_TEST_THREADS=4); `scripts/cargo-test-runner.mjs:51` cap 1 s / 3 s, `:55-59` rejects -q/--format, `:73` fails closed without timing, `:77-84` solo rerun; `scripts/test-time-judge.mjs:13-19` 6 s busy tolerance and WARNING; `package.json:13` + `scripts/vitest-capped.mjs`; `vite.config.ts:14-15` 1000 ms; `test_support/mod.rs:74-76` panics without OUTLOOKER_TEST_PG, boot code only in `examples/test_pg_server.rs` |
| 5 deletes and rewrites | PASS with one finding | all 77 test names that disappeared have a plan row or user decision (audit §1). Finding F1 |
| 5b one shared startup | PASS | SHARED_BOOT count 1 (test-1.txt:154); only `tests/cluster_lifecycle.rs` (4 tests, user-approved) starts its own; boot code unreachable from tests (handoff E0425/E0603 proof) |
| 5a 1 s budget | PARTIAL | enforced by the runner; the default run passes only through the busy tolerance (WARNINGs above); `c_locale_is_rejected` measured 1.093 s in the main run and then failed its rerun (D1) |
| 6 green | **FAIL** | clippy RC=0, typecheck RC=0, `npm test` x3 RC=0, full `cargo test` run 1 RC=0 (38 s). Run 2 `--test it` RC=1 (D1). Run 3 `--test it` has no RC (killed at the 40 s tool cap twice), so 3 consecutive green Rust runs were not obtained |
| 7 counts and wall-clock | PASS | this doc |

## Defects and findings
- **D1 (blocks 6)**: `tests/it/pg_lifecycle.rs:9` runs `CREATE DATABASE c_loc` with a fixed name on the shared cluster. When the test exceeds the 1 s cap, the runner reruns it alone on the same cluster, and the rerun fails with `database "c_loc" already exists` (42P04). Evidence: `final-gate/test-2-it.txt:49` (1.093 s in the main run), `:98-115` (FAIL). Any test that creates a fixed-name object breaks the rerun the same way. Fix: derive the name from the per-test database name, or drop it in the test. Not fixed here (audit red line).
- **F1 (negative space)**: commit a6ed07a removed from `cluster_lifecycle::move_data_dir_succeeds` the check that a failed move into a pre-existing empty folder keeps the folder and leaves it empty. Plan row 172 said MERGE into X3. `lifecycle/move_dir.rs:63-72` (`undo_target`, `pre_existing` branch) is now untested, and the handoff does not list it. A user-chosen folder could be deleted on rollback without any test noticing.
- F2: `expected(n)` (`tests/it/import_pipeline.rs:191`) replaces the real second import as the resume oracle. It drifts if `corpus()` changes.
- F3: `pg_crash_failed_then_retry_resumes` uses `cut_db`/`restore_db` (`test_support/mod.rs:177-188`), not a real postmaster death. The real kill stays covered by `cluster_lifecycle::orphan_postmaster_is_restarted`.
- F4: `read_only_role_rows` no longer fires the real 30 s timeout. It checks `statement_timeout = 30s`, and the 57014 mapping is unit-tested (`sql_console/execute.rs:100`).
- F5: `CARGO_TEST_CAP_S` (`cargo-test-runner.mjs:51`) lets the environment raise the cap, which CLAUDE.md says never to do. `vitest-capped.mjs` hard-codes `CAP_S=1` apart from `vite.config.ts`. `npx vitest` bypasses the solo rerun.
- F6: `examples/import_bench.rs:4` still documents the forbidden `-p ... --features` command.

## Known limitations (from the handoff plus this audit)
- First-run test no longer covers adopting an existing data root (X1).
- `free_space` serializes the Disks refresh with a mutex (a6c0bb3).
- Server exit cleanup takes ~4 s at the end of the `it` run. A run killed during it leaves a postmaster, and the next server start sweeps it. After the gate one such postmaster remains (pid 58146, parent 1, `$TMPDIR/outlooker-test-pg-run-57790-...`).
- The 6 s tolerance and the 0.25 busy threshold are judgment calls. The load average lags about 1 min.
- The Rust cap depends on libtest's unstable `--report-time` format and fails closed.
- vitest: an unhandled error that arrives together with timeout failures is invisible in the JSON report.
- Windows spawn and sweep not run.
- Doctests and `--release` perf runs do not pass through the cap.
- The full Rust gate needs more than the 40 s tool cap under load, so it was split.
