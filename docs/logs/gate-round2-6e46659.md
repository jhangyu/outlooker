# Round 2 gate — 6e46659 (+ lint fix 0c64266), run 2026-10-22 04:27–04:35 CST by gate-r2, judged by team-lead

Raw logs: scripts/tmp/gate2/ and scripts/tmp/fix-g2-clippy.log (gitignored).

| Gate | Result | Evidence |
|---|---|---|
| G2.1 Round 1 gates on Round 2 head | PASS | fetch RC=0, postgres executable; ORIGIN/NOTICE RC=0; workspace build RC=0; bindings diff RC=0; npm ci RC=0, typecheck RC=0, vitest 16 files / 68 tests, vite build RC=0; dark-theme grep RC=1; TBD/TODO grep RC=1 |
| G2.2 cargo tests (split per target, --test-threads 4) | PASS | lib 11+111+7; app_compose 3, db_schema 8, lifecycle 17, maintenance 3, pg_lifecycle 8, removal 2, search 16, writer 10, sql_console 7 + read_only_role_rows 1, import_pipeline 24+2, pff 6; doc RC=0; loadgen lib 7, seeded_root_starts ok, smoke_10k ok alone (21.6 s, fix-g2; killed at 40 s when run together with other loadgen tests) |
| G2.2 clippy -D warnings | PASS after 0c64266 | first run RC=101 (pff/tests/outlook_pst.rs:59); lint-only fixes in 3 files; final RC=0 |
| G2.3 release build, no wdio | PASS | RC=0; `grep -c wdio` on normal dependency tree = 0 |
| G2.4 E2E | PARTIAL | smoke 1, flow 5, seeded 1 passing; scroll NOT RUN (120k seed + 500 pages exceed the 40 s cap → needs user long-run approval) |
| G2.5 tauri build | PASS | CI=true; Outlooker_0.1.0_aarch64.dmg 42,445,233 bytes |
| G2.6 1M load test | NOT RUN | exceeds foreground caps → needs user long-run approval |
| G2.7 real PST throughput | NOT RUN | needs a user-supplied large PST |
| G2.8 single instance | PASS | two `open` → `pgrep -x outlooker` = 1; quit RC=0 |
| G2.9 review log | PASS | `grep -c BLOCKED docs/logs/review-round2.md` = 0 |

Housekeeping: 5 orphan postmasters of this repo's test/E2E data dirs were stopped by team-lead with `pg_ctl -D <dir> stop -m fast` (all under scripts/tmp/e2e/ or a test temp dir).

Verdict: Round 2 macOS gate PASS for every item that can run within the foreground caps (contract-phase2 A2 partial: G2.4 scroll and G2.6 pending user approval; G2.7 pending user PST). A3 PASS.
