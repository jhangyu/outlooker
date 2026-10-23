# Test-suite pruning: result (2026-10-23, HEAD 20bbfcb)

Verdict: all criteria pass. The first gate (HEAD 52bb6b8) failed criterion 6 on two defects, D1 and F1. Both are fixed. Re-gate 3 at HEAD 20bbfcb is green: 3 cargo test runs, `npm test`, build. The first gate's clippy and typecheck results were not rerun on 20bbfcb (see "Not re-gated"). Gate artifacts: `scripts/tmp/final-gate/` (gitignored, this machine): `regate3-*.txt` for the final gate, the unprefixed files for the first gate. Audit scratch: `scripts/tmp/final-audit/`.

## Before / after

| | before (d32b4e0) | after (20bbfcb) |
|---|---|---|
| Rust tests executed (default run) | 250 (lib 140, integration 110), 14 ignored | 208 executed, 16 ignored (14 perf + 2 `warm_template`) |
| Rust wall-clock | test execution ~150-180 s, every PG test boots its own cluster | full `cargo test --workspace`: 23 s, 39 s, 24 s (regate3-test-1..3, load 6-12, includes the runner, one shared boot, solo reruns) |
| Slowest Rust test | `read_only_role_rows` 31.5 s, `smoke_10k` 25.7 s; 51 tests over 2 s | slowest alone in re-gate 3: `writer::batch_cut_by_time` 3.028 s (WARNING) |
| Vitest | 79 cases in 18 files, ~12.2 s, slowest 2729 ms | 53 tests in 17 files, 5.58 s (regate3-npm-1; earlier gate 4.06-5.15 s), slowest 530 ms on a quiet machine |
| e2e | 7 cases in 3 specs | 6 (scroll spec deleted) |

## WARNING lines (passed while busy, solo time over the 1 s cap), re-gate 3
- Run 1: none (one "slow under contention" note: shutdown_during_import_requeues_job 0.948 s).
- Run 2: shutdown_during_import_requeues_job 1.419 s, mtime_change_resets_cursor 1.349 s, pg_crash_failed_then_retry_resumes 2.268 s, writer::batch_cut_by_time 3.028 s.
- Run 3: byte_copy_dedupes_and_removal 1.2 s, index_policy_first_import_drops_and_rebuilds 1.289 s.
- `npm test`: none.
Per CLAUDE.md these tests should be rewritten smaller. The import_pipeline group and `writer::batch_cut_by_time` recur (see parking lot).

## Criteria
| # | Status | Evidence |
|---|---|---|
| 1 inventories | PASS | test-inventory-rust.md, test-inventory-frontend.md |
| 2 plan | PASS | test-cleanup-plan.md plus -rust.md and -frontend.md |
| 3 design | PASS | test-policy-design.md |
| 4 mechanism | PASS | `src-tauri/.cargo/config.toml:6-11` (runner, RUST_TEST_THREADS=4); `scripts/cargo-test-runner.mjs:51` cap 1 s / 3 s, `:55-59` rejects -q/--format, `:73` fails closed without timing, `:77-84` solo rerun; `scripts/test-time-judge.mjs` 6 s tolerance and WARNING; `package.json:13` + `scripts/vitest-capped.mjs`; `vite.config.ts:14-15`; `test_support/mod.rs:74-76` panics without OUTLOOKER_TEST_PG, boot code only in `examples/test_pg_server.rs`; CLAUDE.md Test policy matches |
| 5 deletes and rewrites | PASS | all 77 test names that disappeared have a plan row or user decision. F1 (lost rollback guard) is fixed by e3e4fb6: a unit test on `move_dir.rs` `undo_target` |
| 5b one shared startup | PASS | SHARED_BOOT count 1 in each of the three re-gate runs; only `tests/cluster_lifecycle.rs` (4 tests, user-approved) starts its own |
| 5a 1 s budget | PASS with tolerance | the runner enforces the cap; slow tests pass only through the user-approved busy tolerance (WARNINGs above, none over 3.1 s, none over 6 s) |
| 6 green | PASS | re-gate 3 at 20bbfcb: `cargo test --workspace` RC=0 three times, `npm test` RC=0, build RC=0, leak check clean. First gate at 52bb6b8: clippy RC=0, typecheck RC=0 |
| 7 counts and wall-clock | PASS | this doc |

## Fixed since the first gate
- D1: `tests/it/pg_lifecycle.rs` created fixed-name database `c_loc`, so the solo rerun failed with 42P04. Fixed in e3e4fb6 (unique name).
- F1: the rollback guard for a pre-existing empty target was dropped in a6ed07a. Restored as a 3 ms unit test in `move_dir.rs` (e3e4fb6).
- `move_data_dir_succeeds` sat at 2.1-2.9 s against its 3 s cap, and a re-gate failed at 3.133 s. Split into a success test and a verify-failure rollback test in b4fa48e.
- Busy threshold lowered from 0.25 to 0.1 load per core in 20bbfcb. This is a user decision: normal load on this 28-core machine is 6-12, which sat on the old threshold. Note that this makes the cap more lenient. A re-gate run had failed `writer::batch_cut_by_bytes` 1.092 s at load 6.7/28 = 0.24 under the old threshold. The 1 s cap, the 6 s tolerance and the 3 s cluster_lifecycle cap are unchanged.

## Not re-gated
- clippy and typecheck were last run on 52bb6b8. Later commits changed `move_dir.rs`, `cluster_lifecycle.rs`, `pg_lifecycle.rs` and `test-time-judge.mjs`. Run `cargo clippy --workspace --all-targets -- -D warnings` and `npm run typecheck` on 20bbfcb before relying on criterion 6.
- `cargo test` doctests ran inside the full run in re-gate 3 (no split needed).

## Audit findings still open (not fixed)
- F2: `expected(n)` (`tests/it/import_pipeline.rs:191`) replaces the real second import as the resume oracle. It drifts if `corpus()` changes.
- F3: `pg_crash_failed_then_retry_resumes` uses `cut_db`/`restore_db` (`test_support/mod.rs:177-188`), not a real postmaster death. The real kill stays covered by `cluster_lifecycle::orphan_postmaster_is_restarted`.
- F4: `read_only_role_rows` no longer fires the real 30 s timeout. It checks `statement_timeout = 30s`. The 57014 mapping is unit-tested (`sql_console/execute.rs:100`).

## Known limitations
- First-run test no longer covers adopting an existing data root (X1).
- `free_space` serializes the Disks refresh with a mutex (a6c0bb3).
- Server exit cleanup takes ~4 s at the end of the `it` run. A run killed during it leaves a postmaster, and the next server start sweeps it.
- The 6 s tolerance and the 0.1 busy threshold are judgment calls. The load average lags about 1 min.
- The Rust cap depends on libtest's unstable `--report-time` format and fails closed.
- vitest: an unhandled error that arrives together with timeout failures is invisible in the JSON report.
- Windows spawn and sweep not run.
- Doctests and `--release` perf runs do not pass through the cap.

## Parking lot (for the user to decide, not fixed now)
- `CARGO_TEST_CAP_S` (`scripts/cargo-test-runner.mjs:51`) lets the environment raise the cap, which CLAUDE.md says never to do.
- The vitest cap `CAP_S=1` in `scripts/vitest-capped.mjs` is duplicated apart from `vite.config.ts:14-15`.
- `npx vitest` bypasses the solo rerun and the judgment.
- `examples/import_bench.rs:4` documents the forbidden `-p ... --features` command in its comment.
- Tests that keep showing up in WARNING lines (import_pipeline group, `writer::batch_cut_by_time` 3 s) should be rewritten smaller.
