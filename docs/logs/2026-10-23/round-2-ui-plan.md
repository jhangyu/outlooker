# Round 2 — squad `ui` plan

Worktree `/Users/jhangyu/project/outlooker-wt/r2-ui`, branch `techdebt/r2-ui`, base `a832785`.
Contract: `docs/logs/2026-10-23/contract-techdebt.md`. Brief: `docs/logs/2026-10-23/round-brief.md`. Round 1 handoff: `round-1-ui-handoff.md`.

## Selected items (re-verified at a832785)

| id | verified in code | package |
|---|---|---|
| A6 | `npx tsc --noEmit -p tests/e2e` RC=1: `tests/e2e/scroll.e2e.ts(36,31) TS2362` (`rows.length` of a `$$` result). `package.json:11-12` `build`/`typecheck` run only the root tsconfig. Root `tsconfig.json:17` `exclude: ["tests/e2e"]` is redundant (`include: ["src"]`). Confirmed. | P1 |
| A8 | `tests/e2e/prepare.mjs:33` `cargo run --release --locked -p outlooker-loadgen`. Confirmed. **Extended:** `prepare.mjs` and `wdio.conf.ts:7` hardcode `src-tauri/target/...`, but worktrees export `CARGO_TARGET_DIR` (round brief). In a worktree, e2e would launch a missing or stale binary. Same root cause (binary location), fixed together. | P1 |
| U18 | `tests/e2e/smoke.e2e.ts:4-7` passes on any of 4 screens. The `smoke` group always runs on the fresh app-data dir (`prepare.mjs:16`), and `flow.e2e.ts:11` already waits for `onboarding-lang-en` on the same fresh dir. **Adjusted fix:** delete the smoke group (flow is strictly stronger) instead of narrowing it. Saves one app launch per run. | P1 |
| E1 (new) | `tests/e2e/run.mjs` never stops the app's PostgreSQL cluster after a `wdio run`. `docs/logs/review-round2.md:131` saw 2 postmasters left after flow + seeded; `gate-round2-6e46659.md:18` stopped 5 by hand. Orphans hold SysV shm and break the next `cargo test` run (macOS shm limit). Data roots: fresh → `<APP_DATA.fresh>/data` (`outlooker-core/src/paths/mod.rs:14`), seeded/big → `LIBRARIES[...].root`; cluster dir = `<root>/pgdata` (`paths/mod.rs:44`). | P1 |
| A5 | `src-tauri/capabilities/default.json:6` `core:default`. Frontend Tauri surface re-checked: `invoke` (src/api/call.ts:1), `listen` (src/api/events.ts:1), dialog `open` (src/api/dialog.ts:1). No drag region, no `__TAURI__`, no window/app/path import. `tests/e2e/tauri.e2e.conf.json` adds only `wdio-webdriver:default`. Static proof holds. | P2 |
| D4+D5 | spec `:3` "Status: pending user review"; §0.3 `:28-36` has no outcomes; `scripts/tmp/` at `:140,:151`; `Owner:` at `:66,:294`. Confirmed. | P2 |
| D6 | spec §3 layout `:405-428` lists 9 core modules (tree has 13: + app, dto, sink, test_support), misses `crates/outlooker-loadgen`, `src/mailimg.rs`, `capabilities/`, `src/components/common`, `src/assets`, `scripts/{build.py,vendor-libpff.sh,fixtures/}`, `tests/fixtures/external`, `wdio.conf.ts`. Confirmed. | P2 |
| D7+D9 | `docs/design/variant-1/NOTES.md` has no exclusion list; no root `README.md`. Confirmed. Both are short entry-point docs, counted as one item. | P2 |
| A15 | `scripts/fetch-postgres.mjs:61-79` `--self-test` branch; no caller in package.json/scripts/tests/docs (only the usage comment `:3`). Never run by any gate, so it guards nothing. Pulled forward from the round 3 sweep. | P2 |

## Refuted / adjusted

- U18: handoff fix (assert per group) does not apply — smoke has only one app-data dir (fresh). Delete instead.
- A8: handoff fix extended to honor `CARGO_TARGET_DIR` (see above), otherwise the approved A5 e2e run in a worktree tests a wrong binary.

## Package 1 — e2e harness and build config — model: sonnet (mechanical, 4 items)

Exclusive files: `package.json`, `tsconfig.json`, `wdio.conf.ts`, `tests/e2e/**` (incl. deleting `tests/e2e/smoke.e2e.ts`).
Not owned: `package-lock.json` (no dependency change; must stay byte-identical).

- A6: `package.json` scripts: `"typecheck": "tsc --noEmit -p . && tsc --noEmit -p tests/e2e"`, `"build": "npm run typecheck && vite build"`. Fix `scroll.e2e.ts:36` (check webdriverio 9.32.0 docs for the `$$` result type via context7; no cast to `any`). Drop the `exclude` line from `tsconfig.json`.
- A8: `prepare.mjs` exports one `TARGET_DIR = path.resolve(path.join(REPO_ROOT, "src-tauri"), process.env.CARGO_TARGET_DIR ?? "target")`. Seeding: `cargo build --release --locked --workspace --bin outlooker-loadgen` once, then spawn `<TARGET_DIR>/release/outlooker-loadgen[.exe] seed --data-root ... --messages ...`. `wdio.conf.ts` builds the app binary path from the same `TARGET_DIR` (`debug/outlooker[.exe]`).
- U18: `git rm tests/e2e/smoke.e2e.ts`; remove `smoke` from `GROUPS` and `GROUP_APP_DATA`; `wdio.conf.ts:4` default `"smoke"` → no default (the existing unknown-group throw fires when `OUTLOOKER_E2E_GROUP` is unset).
- E1: `run.mjs` stops the group's cluster after every `wdio run`, pass or fail, before any `process.exit`: if `<root>/pgdata/postmaster.pid` exists, run `src-tauri/resources/postgresql/bin/pg_ctl[.exe] -D <root>/pgdata stop -m fast`. The root per group comes from one exported helper in `prepare.mjs` (no second copy of the path table).

Acceptance (P1):
1. Red first: with only the `package.json` script change applied, `mise exec -- npm run build` RC≠0 showing `scroll.e2e.ts(36` (report the line). After the fix: `mise exec -- npm run build` RC=0 and `mise exec -- npx tsc --noEmit -p tests/e2e` RC=0.
2. `grep -c '"exclude"' tsconfig.json` → 0.
3. `grep -c -- '"-p"' tests/e2e/prepare.mjs` → 0; `grep -c "CARGO_TARGET_DIR" tests/e2e/prepare.mjs` → 1; `grep -c "src-tauri\", \"target\"" wdio.conf.ts` → 0.
4. `test -e tests/e2e/smoke.e2e.ts` RC=1; `grep -rn "smoke" tests/e2e wdio.conf.ts` → no match.
5. `grep -c "pg_ctl" tests/e2e/run.mjs` → ≥1; `node --check` RC=0 on `tests/e2e/run.mjs` and `tests/e2e/prepare.mjs`; `node -e "import('./tests/e2e/prepare.mjs').then(m => console.log(m.GROUPS.join(','), m.TARGET_DIR))"` prints `flow,seeded,scroll` and the target dir (run once with and once without `CARGO_TARGET_DIR`; report both).
6. `git diff --exit-code a832785 -- package-lock.json` RC=0.
7. Gates below RC=0. Runtime proof of A8/U18/E1 comes from the A5 long run.

## Package 2 — docs, capability, dead self-test — model: sonnet (mechanical, 4 items)

Exclusive files: `docs/superpowers/specs/2026-10-21-outlooker-design.md`, `docs/design/variant-1/NOTES.md`, `README.md` (new), `src-tauri/capabilities/default.json`, `scripts/fetch-postgres.mjs`.

- A5: `"core:default"` → `"core:event:default"`. Nothing else in the file changes.
- D4+D5: status line → `Status: implemented on macOS; Windows phase pending.` §0.3 title → `Known risks and outcomes`; add an `Outcome` column, one entry per row, each with its source:
  Windows pg_trgm locale → `open: Windows phase`; 1M load / throughput / latency → `open: 1M load test (not run, needs a user-approved long run)`; sample PST → `tests/fixtures/external/outlook.pst` used by import tests and e2e flow; `scripts/fixtures/make-pst.ps1` kept for the Windows phase (user ruling A7); RTF HTML → verify the converter in `src-tauri/crates/outlooker-core/src/importer/` and cite the file; scroll limit → `100k cap implemented; WKWebView covered by e2e group scroll; WebView2 open: Windows phase`; installer size → `macOS dmg 42,445,233 bytes (docs/logs/gate-round2-6e46659.md G2.5); Windows open`.
  `:140` drop "cloned to `scripts/tmp/libpff`"; `:151` drop "Script: `scripts/tmp/sdist/ccbuild.sh`." (keep "verified 2026-10-21 against fork bb5bcc0"-style history). `:66`, `:294` delete `Owner: arch-*.` keep the `Input:` sentence.
- D6: regenerate the §3 layout block from `ls`/`git ls-files` at the current tree. Directory granularity; core modules as one brace list. Must include every path named in the D6 row above plus `README.md`. Do not list gitignored paths except `resources/postgresql/` (already marked gitignored).
- D7+D9: `NOTES.md` gets a top section `## Not implemented (user ruling)` listing SQL Format, Pause, worker count, deleted/kept preview, drag-drop zone, pointing to spec §0.2 (verify each against §0.2 and that `src/` has no such control). `README.md` ≤ 30 lines: what the app is (1 line); prerequisites (mise, node pin from `mise.toml`, `node scripts/fetch-postgres.mjs`); `mise exec -- npm run build`, `mise exec -- npm test`, `mise exec -- npm run e2e` (groups flow, seeded, scroll; scroll seeds 120,000 messages and is long), one group via `node tests/e2e/run.mjs --spec tests/e2e/flow.e2e.ts`; `python3 scripts/build.py`; cargo commands → root `CLAUDE.md`.
- A15: delete the `--self-test` branch (`:61-79`), its mention in the usage comment `:3`, and any import that becomes unused.

Acceptance (P2):
1. `grep -c '"core:default"' src-tauri/capabilities/default.json` → 0; `grep -c '"core:event:default"'` → 1; `node -e "JSON.parse(require('fs').readFileSync('src-tauri/capabilities/default.json','utf8'))"` RC=0.
2. Spec: `grep -c "pending user review\|scripts/tmp/\|^Owner:" <spec>` → 0; `grep -n "^## 0.3 Known risks and outcomes" <spec>` → 1 match; every §0.3 data row has 3 cells.
3. Spec §3 block contains each of: `outlooker-loadgen`, `mailimg.rs`, `capabilities/`, `components/common`, `assets/`, `build.py`, `fixtures/`, `tests/fixtures/external`, `wdio.conf.ts`, `test_support`, `dto`, `sink` (grep within the block's line range, report the range).
4. `grep -c "^## Not implemented (user ruling)" docs/design/variant-1/NOTES.md` → 1; `wc -l README.md` ≤ 30; `grep -c "smoke" README.md` → 0; every command in README exists (`package.json` script names, file paths `test -e`).
5. `grep -c "self-test" scripts/fetch-postgres.mjs` → 0; `node --check scripts/fetch-postgres.mjs` RC=0; `node scripts/fetch-postgres.mjs` RC=0 in the worktree is NOT run (it downloads into `src-tauri/resources/`); the orchestrator's merged e2e run (`build.mjs` calls it) is the runtime check.
6. Gates below RC=0.

## Gate commands (worktree root; test-runner `r2-ui-test-haiku`; window opens only after both implementers send SETTLED to `r2-ui-architect`)

1. `mise exec -- npm run build` → RC=0 (after P1 this includes the e2e typecheck).
2. `mise exec -- npm test` → RC=0 (baseline 16 files / 72 tests).
3. `mise exec -- npx tsc --noEmit -p tests/e2e` → RC=0.
4. The grep checks of both packages.
Each command once; `RC=$?` captured right after it into the artifact; start/end time recorded. Artifact: `docs/logs/2026-10-23/round-2-ui-gate.md`. Raw logs: `tmp/` in the worktree (untracked, not committed).

## A5 runtime proof — approved long run (orchestrator schedules, on the merged integration state)

- Where: `/Users/jhangyu/project/outlooker-wt/integration`, after both squads' round 2 merges (core round 2 may touch the app crate; the run must cover the merged binary).
- Pre-steps: `export CARGO_TARGET_DIR=/Users/jhangyu/project/outlooker/src-tauri/target` (P1 makes e2e honor it); `chmod -R u+w "$CARGO_TARGET_DIR"/*/resources 2>/dev/null` (review-round2.md:23 note); record `pgrep -fl "postgres -D"` before.
- Command (one run, from the worktree root): `mise exec -- npm run e2e > tmp/e2e-a5.log 2>&1; echo "RC=$?" >> tmp/e2e-a5.log`
- Steps inside: `fetch-postgres.mjs` (idempotent) → `tauri build --debug --no-bundle --features e2e` → per group (flow, seeded, scroll): seed if needed (`cargo build --release --workspace --bin outlooker-loadgen`, 2,000 and 120,000 messages) → `wdio run` → cluster stop (E1).
- Expected duration: 15–35 min (e2e debug build 2–8 min incremental; release loadgen build 2–6 min if not cached; 120k seed 2–5 min; flow ~2 min; seeded ~1 min; scroll up to 500 pages ≈ 5–12 min). Hard stop at 60 min.
- First-alive gate (≤ 5 min after start): `tmp/e2e-a5.log` grows and shows `Compiling`/`Finished` from the tauri build. Otherwise treat as dead.
- Pass criteria:
  - A5 (blocking): groups `flow` (5 passing) and `seeded` (1 passing) green. These exercise boot `invoke`, `listen` on `library:changed`/`import:*`, onboarding, import, search, preview, Advanced SQL, removal. A permission problem shows as a stalled UI / rejected IPC in flow.
  - E1: after the run, `pgrep -fl "postgres -D .*scripts/tmp/e2e"` → no match.
  - `scroll` (1 passing): first run ever (gate-round2 G2.4 never ran it). Required for the contract's full e2e but not evidence for or against A5 — scroll uses only `invoke` already covered by flow. A scroll failure is triaged by `r2-ui-architect` as its own finding, not as an A5 revert.
- If flow or seeded fails: capture the failing step from the log; revert A5 only if the failure is IPC-related (`not allowed`/`permission` in the log, or stall at the first `invoke`/`listen`).

## Round 3 roadmap (so nothing worth fixing remains after it)

- P1 (judgment, sonnet): U4 (`useEnqueuePst` hook for import + onboarding), U7 (`src/api/queries.ts` with `queryOptions` for `pstList`/`dbStatus`/`moveProgress`).
- P2 (mechanical, sonnet), 4 grouped items:
  1. U2 + U3: `gcTime` on `searchMail`/`searchCount`/`mailGet`; cached `Intl` formatters in `display.ts`.
  2. U5 + U12: leaf budget from `compose.ts` + `MAX_LEAVES`; one `toAppError` in `api/call.ts`.
  3. Dead code and stale tests: U8 (fixtures.ts ResizeObserver stub), U9 comment only, U15 (barrel exports), U19 (delete tautology test), U20 (drop vocabulary test, keep dark theme), U21 (rename).
  4. A7 first half: `scripts/vendor-libpff.sh` → `scripts/vendor-libpff.py` with the run id as a required argument (no date-rotting default); update spec §3 layout and README if they name the script.
- File overlap check for round 3: U7 and U2 both touch `search/index.tsx` / query call sites → U2's `searchCount` gcTime goes to P1 with U7 if they collide; the architect re-splits at round 3 planning.
- After round 3 the ui inventory is closed: all other items are already fixed or listed as not worth fixing (U6, U9 behavior, U11, U13, U14, U16, U17 — reasons in `round-1-ui-handoff.md`).

## Not worth fixing (unchanged from round 1)

U6, U9 (behavior part), U11, U13, U14, U16, U17 — see `round-1-ui-handoff.md`.

## Baseline gate result (base `a832785`, before any change, 2026-10-23)

- `mise exec -- npm run build` → RC=0 (vite built in 400 ms; only the >500 kB chunk warning).
- `mise exec -- npm test` → RC=0, 16 files / 72 tests, 5.3 s.
- Extra (not a gate): `mise exec -- npx tsc --noEmit -p tests/e2e` → RC=1, `scroll.e2e.ts(36,31) TS2362` (A6, fixed by P1).
