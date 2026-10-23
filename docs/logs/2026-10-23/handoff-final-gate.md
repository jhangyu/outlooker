# Handoff: final gate for the test-suite pruning contract (2026-10-23)

From: policy-impl-opus. To: the member who runs the final gate and writes `test-cleanup-result.md`.
HEAD at handoff: a29c6d3 (plus this file). Evidence files are in `scripts/tmp/policy-impl/` (gitignored, on this machine only).

## 1. Contract acceptance criteria (verbatim from contract-test-suite-pruning.md) and status

> 1. Inventory files exist: `docs/logs/2026-10-23/test-inventory-rust.md`, `docs/logs/2026-10-23/test-inventory-frontend.md`; each lists every test (file:line, name, category, runtime where measurable) and flags non-core / slow candidates.

Status: the files exist. They are untracked in git (`git status`: `??`). Commit them with the result doc.

> 2. Cleanup plan exists: `docs/logs/2026-10-23/test-cleanup-plan.md` with a per-test DELETE / KEEP / MERGE / REWRITE decision and a reason for each DELETE and REWRITE.

Status: the files exist (index + `-rust.md` + `-frontend.md`). They are untracked. Commit them with the result doc.

> 3. Prevention design exists: `docs/logs/2026-10-23/test-policy-design.md`.

Status: the file exists and is untracked. Commit it with the result doc. Later decisions superseded parts of its §3 M2: `--report-time` judgment instead of `--ensure-time`, solo rerun with a 6 s busy tolerance, and a runner-booted shared cluster. CLAUDE.md "Test policy" is the current text.

> 4. Prevention mechanism implemented and committed per the design.

Status: DONE.
- Policy text: CLAUDE.md "Test policy" (cdbbdea, a29c6d3).
- vitest cap: vite.config.ts 1000 ms (21df644) plus `scripts/vitest-capped.mjs` as `npm test` (3b9eb3b).
- Rust cap and shared boot: `scripts/cargo-test-runner.mjs`, `scripts/test-time-judge.mjs` and `src-tauri/.cargo/config.toml` (0e317ac); `examples/test_pg_server.rs` and test_support (87e70e1).

Red/green proofs, in `scripts/tmp/policy-impl/`:
| case | Rust | vitest |
|---|---|---|
| 1.5 s test, quiet machine → FAIL | j2-rs-a.txt RC=1 | j-vt-a.txt RC=1; slow hook: j-vt-hook-a.txt RC=1 |
| 1.5 s test, load forced busy → pass with WARNING | j2-rs-b.txt RC=0 | j-vt-b.txt RC=0; slow hook: j-vt-hook-b.txt RC=0 |
| 7 s test, busy → FAIL | j2-rs-c.txt RC=1 | j-vt-c.txt RC=1 |
| assertion failure is never rerun | n/a | j-vt-d.txt (1 vitest run, RC=1); j-vt-d2.txt (assertion + timeout: only the timeout reruns, RC=1) |
| slow first run, solo rerun FAILED → FAIL | j-rs-rerunfail.txt RC=1 | rerun must have status "passed" |
| crate layout `tests/<dir>/main.rs` is capped | b-red-zzdir.txt RC=1 | n/a |
| timing unreadable (format change, `-q`, `--format`) → FAIL | fc-a-broken.txt, fc-b-q.txt, fc-b-json.txt RC=1 | n/a |
| boot code unreachable from tests | s-boot-red.txt: E0425 `boot_shared`, E0603 `shared` | n/a |
| OUTLOOKER_TEST_PG absent → clear panic | s-noenv.txt RC=101 | n/a |
| one boot per run | `grep -c SHARED_BOOT` = 1 (g-full-1..3, gate1-it.txt) | n/a |
| kill → next run sweeps the orphan postmaster | s-kill.txt and the next run, 0 leftovers | n/a |

> 5. Every DELETE in the plan is applied and committed, and no test outside the plan is deleted. Every REWRITE is applied: the rewritten test still guards the same core behavior.

Status: signed off by the lead for both cleanup packages (commits listed in §2). Not re-verified by me.

> 5b. Shared database (user directive): all database tests share ONE PostgreSQL startup per test run. No test boots its own cluster. Any exception needs explicit user approval. The same rule applies to every other expensive setup that tests repeat (fixture import, large seed data, runtime or app launch): it runs once per test run and is shared, with cheap per-test isolation.

Status: DONE for Rust.
- The runner boots one cluster per run for `it`.
- The only exception binary is `tests/cluster_lifecycle.rs`: 4 tests, user-approved, 3 s base cap.
- Evidence: SHARED_BOOT count = 1; boot code is not callable from tests (E0425/E0603 above).

> 5a. Runtime budget (user directive): no single test in the default runs takes more than 1 s, and the typical test takes a few hundred ms. A slow test that guards core logic is REWRITTEN to fit the budget. A slow test that does not guard core logic is DELETED.

Status: enforced mechanically, with the user-approved tolerance described in §3.
- Quiet machine (load ~4, 14:37): vitest 3 runs green, slowest 530 ms (fe-q-1..3.txt).
- Under load, the WARNINGs listed in §4 appear.
- Open: a full 3-run Rust gate on a quiet machine (§3).

> 6. After cleanup: `cargo test --workspace` green, `cargo clippy --workspace --all-targets -- -D warnings` green, `npm test` green, `npm run typecheck` green.

Status:
- clippy RC=0 and typecheck RC=0 on the final tree (final-clippy.txt, final-tc.txt).
- cargo test: run 1 green by target (§4). Runs 2 and 3 are NOT done (§4).
- npm test with the wrapper: not gated yet.

> 7. Before/after test count and wall-clock recorded in `docs/logs/2026-10-23/test-cleanup-result.md`.

Status: NOT DONE. The before numbers are in §6; the after numbers come from your gate run.

## 2. Commits of this effort (oldest first)
828775c, cdbbdea, a4c1a8d, 1e0d4a7, 8e7c8da, 7e60b3d, a6ed07a, cff3456, a6c0bb3, 21df644, ee32003, 87e70e1, 0e317ac, 3b9eb3b, a29c6d3.

## 3. Gate commands and how to judge them
Run from `src-tauri/` for cargo and from the repo root for npm. Run one command per shell call, never two at once: they contend and void the timing. Capture `RC=$?` right after each command and record `uptime` before each run.

Rust, three consecutive runs. The full `cargo test --workspace` takes about 40 s or more under load. `--test it` alone needs ~10–20 s of main run plus its solo reruns. So use a quiet machine (load average under ~7 on this 28-core box) or a gate-runner with a longer timeout. If you must split, run these per run, in order:
1. `cargo test --workspace --test it`
2. `cargo test --workspace --lib --bins --test cluster_lifecycle --test perf --test outlook_pst`
3. `cargo test --workspace --doc` (rustdoc does not go through the runner)

Frontend, three runs: `mise exec -- npm test`.
Also: `cargo clippy --workspace --all-targets -- -D warnings` and `mise exec -- npm run typecheck`.

Judging (same rules on both sides, `scripts/test-time-judge.mjs`):
- RC=0 is green. RC≠0 is red. Read the FAIL line.
- A test over its base cap (1 s; 3 s for cluster_lifecycle) is rerun once, alone. The runner prints one of:
  - `slow under contention: NAME Xs alone <= cap`: pass. Contention only.
  - `WARNING: NAME Xs alone > cap, allowed while busy up to 6 s (load L/28 cores, busy threshold 0.25)`: pass. Record it in the result doc. Per CLAUDE.md, a test that keeps appearing here should be rewritten.
  - `FAIL: ...`: red. Causes: over the cap on a quiet machine, over 6 s, rerun not ok, or no timing data.
- One boot per run: in the `it` output, `grep -c SHARED_BOOT` = 1.
- No leftovers after the run: `pgrep -f outlooker-test-pg-run` prints nothing, and `ls $TMPDIR | grep -c outlooker-test-pg-run` = 0. A run killed by a timeout can leave one postmaster. The next run's server sweeps it. To stop it by hand, use `pg_ctl stop -m immediate -D <dir>/pgdata` on that exact dir after confirming its parent pid is dead.

## 4. Run 1 results (15:21–15:23, by target)
- `--test it`: RC=0, wall 30 s, main run 76 passed and 1 ignored in 9.76 s, SHARED_BOOT=1 (gate1-it.txt). WARNINGs:
  - `app_compose::shutdown_during_import_requeues_job 1.625s alone > 1 s cap (load 31.6/28)`
  - `import_pipeline::index_policy_first_import_drops_and_rebuilds 1.824s alone > 1 s cap (load 29.6/28)`
  - `import_pipeline::mtime_change_resets_cursor 5.002s alone > 1 s cap (load 28.0/28)`
- lib, bins, cluster_lifecycle, perf, outlook_pst: RC=0, wall 23 s (gate1-rest.txt). 4 "slow under contention" notes, all for cluster_lifecycle.
- doc: RC=0, 1 doctest (gate1-doc.txt).
- Run 2 (`--test it`) hit the 40 s shell cap twice: the main run took 14–21 s, then the reruns. At the time syspolicyd was at 648% CPU and Spotlight at ~230%. One of those runs is void, because I ran two gate commands concurrently.
- Earlier datapoints:
  - With the old in-process boot, at load ~4–21: `it` at 4 threads finished in 6.7–10 s.
  - Full workspace wall at 4 threads: 12–17 s (g-full-1/2).

## 5. Known limitations
- X1 `first_run_reaches_ready` no longer covers adopting an existing data root (merged away in the cleanup).
- `free_space` serializes the sysinfo Disks refresh with a mutex (a6c0bb3), to stop a flake under parallel tests.
- Server exit cleanup (pg_ctl stop -m immediate plus remove_dir_all of the per-test databases) takes ~4 s at the end of the `it` run. If a run is killed during it, the run dir is left and the next sweep removes it.
- The 6 s busy tolerance and the busy threshold (1-min load / cores ≥ 0.25, calibrated on 2026-10-23: quiet ~4, busy 15–44 on 28 cores) are judgment calls. The load average lags behind bursts by about 1 minute.
- The Rust cap depends on libtest's unstable `--report-time` line format (`test NAME ... ok <1.234s>`) through RUSTC_BOOTSTRAP on test binaries. It fails closed: no timing means FAIL.
- vitest: an unhandled error that comes together with timeout failures is not visible in the JSON report, so the wrapper could pass it after successful reruns. vitest itself still prints it.
- `examples/import_bench.rs`: its doc comment still shows `-p ... --features` (forbidden by CLAUDE.md). It now needs the runner for its cluster (the runner boots one for `import_bench`). It has not been run since the change.
- Windows: the server spawn and the sweep have not been run there (Windows SSH phase).

## 6. Before numbers (from the inventories)
- Rust (test-inventory-rust.md):
  - 224 fn tests plus 49 ts-rs = 273.
  - The default run executed 250 (lib 140, integration 110), 14 ignored.
  - 51 tests over 2 s; slowest `read_only_role_rows` 31.5 s, `smoke_10k` 25.7 s.
  - Test-execution wall ~150–180 s (test-cleanup-plan-rust.md §6), with each PG test booting its own cluster.
- Frontend (test-inventory-frontend.md):
  - 79 vitest cases in 18 files (73 timed at d32b4e0), plus 7 e2e cases in 3 specs.
  - Execution ~12.2 s across 17 files and 73 tests. Slowest: the mail paging 500-pages test at 2729 ms, the cimode screens test at 1470 ms.
- After, for comparison (measure your own runs):
  - Rust run 1 by target: 205 executed (it 76 + others 129) plus 1 doctest. 16 ignored: 14 perf, plus 2 `warm_template`.
  - Frontend: 53 tests, 3.05–3.91 s duration on a quiet machine (fe-q-1..3.txt).

## Busy threshold changed (2026-10-23)

The busy threshold in `scripts/test-time-judge.mjs` changed from 0.25 to 0.1 (load / cores) by user decision. Normal load on this shared 28-core machine is 6-12, which sat at the old threshold, so tests near the 1 s cap flaked between FAIL and pass. Everything else is unchanged (1 s cap, 3 s for cluster_lifecycle, 6 s tolerance, WARNING lines). Re-proof with TEST_LOAD_OVERRIDE in `scripts/tmp/policy-impl/j3-rs-a/b/c.txt`: load/cores 0.05 → FAIL RC=1, 0.15 → WARNING RC=0, 7 s at 0.15 → FAIL RC=1.
