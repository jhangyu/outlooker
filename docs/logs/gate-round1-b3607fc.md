# Round 1 gate — b3607fc (run 2026-10-22 03:27:33–03:28:53 CST, gate-runner gate-r1, judged by team-lead)

Raw logs: scripts/tmp/gate1/<id>.log (gitignored). HEAD moved to bbe6f83 during the window by a reviewer-log-only commit (docs/logs/review-round1.md); no code changed.

| Gate | Result | Evidence |
|---|---|---|
| G1.1 fetch PostgreSQL | PASS | RC=0; `test -x src-tauri/resources/postgresql/bin/postgres` RC=0 |
| G1.2 libpff origin + fixture | PASS | three checks RC=0/0/0 |
| G1.3 cargo build --workspace --locked | PASS | RC=0 |
| G1.4 cargo test (split by target) | PASS | --no-run RC=0; --lib 88 passed; db_schema 8 passed; pg_lifecycle 8 passed; pff outlook_pst 6 passed; doc RC=0; 0 FAILED lines in all logs; named tests present as ok: outlook_pst_welcome_message, ddl_matches_migration, ro_role_sees_only_views, pwfile_is_owner_only, init_passes_cjk_self_check, auto_conf_clamps_shared_buffers, paths_layout, big5_and_gbk, fromhtml_golden, hostile_corpus_is_clean, golden_plain, golden_inline |
| G1.5 bindings drift | PASS | `git diff --exit-code src/api/bindings` RC=0 |
| G1.6 npm ci / typecheck / test / build | PASS | ci RC=0 (under 40 s); typecheck RC=0; vitest 16 files / 68 tests passed; vite build RC=0 |
| G1.7 no dark theme | PASS | grep RC=1 (no match) |
| G1.8 no TBD/TODO | PASS | grep RC=1 (no match) |
| G1.9 review log | PASS | `grep -c BLOCKED docs/logs/review-round1.md` printed 0 |

Gate-command deviations (instrument fixes, not code fixes):
- G1.4: `-p pff --features outlooker-core/test-support` is rejected by cargo ("package 'pff' does not contain this feature"); the pff test target was run without the feature flag (6 passed).
- G1.8: zsh expands `--include=*.rs`; the include patterns were quoted.
- Both are recorded for Round 2's G2.1 command list.

Verdict: Round 1 gate PASS (contract-phase2 A1, A3).
