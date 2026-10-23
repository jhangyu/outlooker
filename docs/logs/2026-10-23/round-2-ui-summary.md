# Round 2 — squad `ui` summary

Branch `techdebt/r2-ui`, base `a832785`, head `ca04132` (plus this summary). Plan: `round-2-ui-plan.md`.

## Commits

| commit | items |
|---|---|
| a78d24d | plan |
| de9c18b | A6, A8 (+ CARGO_TARGET_DIR), U18, E1 |
| 63acf37, 3425e6f, ca04132 | A5, D4+D5, D6, D7+D9, A15 |

## Items fixed

- A6: `package.json:11-12` `typecheck` runs root + `tests/e2e`; `build` runs `typecheck`. `scroll.e2e.ts:35` uses `$$(...).getElements()`. Root tsconfig `exclude` removed.
- A8: `prepare.mjs` `TARGET_DIR` honors `CARGO_TARGET_DIR`; seeding = `cargo build --release --locked --workspace --bin outlooker-loadgen` + run the binary. `wdio.conf.ts` uses `TARGET_DIR/debug`.
- U18: smoke group deleted (flow covers the fresh landing). `wdio.conf.ts` no default group.
- E1: `run.mjs` `stopCluster` runs `pg_ctl -D <root>/pgdata stop -m fast` after every `wdio run`; root from `prepare.mjs` `groupDataRoot`.
- A5: `capabilities/default.json` `core:default` → `core:event:default` (static proof in the plan; runtime proof pending, see below).
- D4+D5: spec status "implemented on macOS; Windows phase pending"; §0.3 has an Outcome column (6 rows); `scripts/tmp/` and `Owner:` lines removed.
- D6: spec §3 layout regenerated (13 core modules, loadgen, mailimg, capabilities, assets, scripts, fixtures, README).
- D7+D9: NOTES.md "Not implemented (user ruling)"; new `README.md` (19 lines).
- A15: `--self-test` branch removed from `scripts/fetch-postgres.mjs`.

## Gate (run by r2-ui-test-haiku at ca04132, 2026-10-23T02:48:38Z–02:48:46Z; raw logs `tmp/gate-{1,2,3}.log` in the worktree, untracked)

- `mise exec -- npm run build` RC=0 (now includes the e2e typecheck).
- `mise exec -- npm test` RC=0, 16 files / 72 tests.
- `mise exec -- npx tsc --noEmit -p tests/e2e` RC=0 (baseline RC=1).
- Greps: tsconfig `exclude` 0; prepare.mjs `"-p"` 0; smoke spec absent; `core:default` 0; `core:event:default` 1; spec stale patterns 0; `self-test` 0; package-lock.json unchanged since a832785.
- Red-first evidence for A6: build RC=1 at `scroll.e2e.ts(36,31)` before the fix (impl-1 `tmp/p1-red.log`).

## Pending (orchestrator)

- A5 / A8 / U18 / E1 runtime proof: the approved `mise exec -- npm run e2e` on the merged integration state. The pass criteria are in the plan section "A5 runtime proof". Until it runs, these items rest on static proof.

## Refuted / adjusted / not worth fixing

- U18: the handoff fix (assert per group) did not apply because smoke has one app-data dir. The group was deleted instead.
- A8: extended to `CARGO_TARGET_DIR`.
- Not worth fixing (unchanged): U6, U9 behavior, U11, U13, U14, U16, U17 — reasons in `round-1-ui-handoff.md`.
- Round 3: U4, U7 | U2+U3, U5+U12, sweep (U8, U9 comment, U15, U19, U20, U21), A7 vendor-libpff port. Plan section "Round 3 roadmap".
