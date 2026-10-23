# Round 2 handoff — squad `ui`

Branch `techdebt/r2-ui`, signed-off head `150978c` (base `a832785`). Plan: `round-2-ui-plan.md`. Summary and gate record: `round-2-ui-summary.md`. Reviewer verdict: CONFIRMED (report `r2-ui/tmp/review/round-2-review-ui.md`, untracked). Findings 1–3 fixed in `150978c`. Finding 4 waits for the e2e run.

## Completed work and evidence

| item | commit | evidence |
|---|---|---|
| A6 e2e typecheck in build | de9c18b | `package.json:11-12`. Before the fix: build RC=1 at `scroll.e2e.ts(36,31)`. After: `npx tsc --noEmit -p tests/e2e` RC=0 (baseline RC=1). |
| A8 no `-p`, honor `CARGO_TARGET_DIR` | de9c18b | `prepare.mjs` exports `TARGET_DIR`. Seeding builds `--workspace --bin outlooker-loadgen`, then runs the binary. `wdio.conf.ts` uses `TARGET_DIR/debug`. The reviewer checked that the import prints both target dirs. |
| U18 smoke group deleted | de9c18b | `GROUPS = flow,seeded,scroll`. `run.mjs --spec smoke.e2e.ts` → "unknown spec", RC=1. |
| E1 (new) stop the app's PG after each group | de9c18b | `run.mjs` `stopCluster` runs `pg_ctl -D <root>/pgdata stop -m fast`. `groupDataRoot` matches `outlooker-core/src/paths/mod.rs:14,44`. |
| A5 `core:default` → `core:event:default` | 63acf37 | Static proof: the frontend uses only `invoke`/`listen`/dialog `open`. build.rs has no app manifest. The wdio plugin declares no permissions. |
| D4+D5 spec status, §0.3 outcomes, stale paths | 63acf37 | grep `pending user review\|scripts/tmp/\|^Owner:` → 0. §0.3 has 6 rows with 3 cells each. |
| D6 spec §3 layout | 63acf37, 3425e6f | Regenerated. The 12 required names are present. |
| D7+D9 NOTES exclusion list, README | 63acf37, ca04132, 150978c | NOTES lists all 8 items from spec §0.2. `README.md` has 19 lines, and every node command runs through mise. |
| A15 dead `--self-test` | 63acf37 | grep `self-test` → 0. `node --check` RC=0. |

Gate at ca04132: build RC=0, `npm test` RC=0 (16 files / 72 tests), e2e tsc RC=0, all 8 greps as expected, package-lock unchanged. Re-gate at 150978c (docs only): build RC=0, test RC=0 (16 / 72).

## Known limitations

- **e2e run pending.** The approved `mise exec -- npm run e2e` on the merged integration state is the only runtime proof for A5, A8 (seeding path, `TARGET_DIR`), U18 and E1. Pass criteria are in the plan section "A5 runtime proof". Until it runs, these items rest on static proof.
- **Scroll group never run.** Spec §0.3 scroll row says "WKWebView covered by e2e group `scroll`". If the run fails or skips scroll, change that row to "open" (reviewer finding 4, routed by `r2-ui-architect` on the run result).
- **Round 1 limits still apply.** The AppFrame move-error test flake did not recur: 4 full suites this round were green (baseline, gate, re-gate, reviewer). The real-webview scroll restore after switching screens is still checked by hand only.
- **Seeding cost.** Seeding now builds `outlooker-loadgen` in `target/release` (allowed release exception). The first run on a cold release dir adds a few minutes.

## Not worth fixing (unchanged)

- U6: the hand-rolled `seq` works and is guarded by `search.test.tsx`. A rewrite touches 3 files and changes no behavior.
- U9 (behavior part): a language switch loses undo history only, and the event is rare.
- U11: the test-only props cost nothing at runtime.
- U13: the pseudo-query stores work and are tested through the move flow, the riskiest flow.
- U14: internal casts in one component, no external blast radius.
- U16: the mutation-mirror state works and is tested. The refactor risk is greater than the value.
- U17: removing the UI default changes the loading UX for a value that has not drifted.

## Interface notes

- The frontend↔Tauri command and event contract is unchanged: 24 commands, 6 events.
- The capability is now `core:event:default` + `dialog:allow-open`. Any new frontend import of a Tauri `window`/`app`/`path`/`webview` API needs its permission added to `src-tauri/capabilities/default.json`. Without it, the call rejects at runtime and only e2e catches it.
- `npm run build` and `npm run typecheck` now type-check `tests/e2e` and `wdio.conf.ts`.
- e2e: `prepare.mjs` exports `TARGET_DIR`, `GROUPS`, `GROUP_APP_DATA` and `groupDataRoot`. `wdio.conf.ts` needs `OUTLOOKER_E2E_GROUP` (it throws when the variable is unset). Run e2e only through `run.mjs`.

## Round 3 — ui slice (covers every remaining ui item)

**Package 1 — query/enqueue single source (sonnet, 2 judgment + 2 mechanical).**
Exclusive files: `src/api/queries.ts` (new), `src/api/call.ts`, `src/components/common/{hooks.tsx,formatAppError.ts,index.ts,common.test.tsx}`, `src/app/{App.tsx,boot.ts,AppFrame.tsx,AppFrame.test.tsx}`, `src/screens/import/**`, `src/screens/onboarding/**`, `src/screens/settings/**`.
- U4: `useEnqueuePst()` in `common/hooks.tsx` (pick → `api.importEnqueue` → toast rejected → toast error, returns `Promise<boolean>`). Used by `import/index.tsx` and `onboarding/index.tsx`. Acceptance: `grep -c "importEnqueue" src/screens/import/index.tsx src/screens/onboarding/index.tsx` → 0 each. Existing import and onboarding tests stay green.
- U7: `src/api/queries.ts` with `queryOptions` for `pstList`, `dbStatus` (incl. `refetchInterval`) and `moveProgress`. Every `useQuery`/`getQueryData`/`setQueryData`/`invalidateQueries` site uses them. Acceptance: `grep -rn '\["pstList"\]\|\["dbStatus"\]\|\["moveProgress"\]' src --include=*.ts --include=*.tsx | grep -v test | grep -v src/api/queries.ts` → no match.
- U12: move `toAppError` into `src/api/call.ts`. `call` throws `toAppError(e)`. `common/index.ts` re-exports it from `api/call`, and `formatAppError.ts` stops defining it. Acceptance: `grep -c "function toAppError" -r src` → 1.
- U21: rename `settings.test.tsx:36` to "maintenance button runs the task".

**Package 2 — perf, single constants, dead code, script port (sonnet, 4 mechanical groups).**
Exclusive files: `src/components/mail/**`, `src/components/common/display.ts`, `src/screens/search/**`, `src/components/dsl/**`, `src/components/sql-editor/**`, `src/app/global.test.tsx`, `scripts/vendor-libpff.sh` (delete), `scripts/vendor-libpff.py` (new), `scripts/build.py`, `docs/superpowers/specs/2026-10-21-outlooker-design.md`, `README.md`.
- U2+U3: set `gcTime` per query in `pages.ts` (`searchMail`), `search/index.tsx` (`searchCount`) and `ReadingPane.tsx` (`mailGet`). Do not change the QueryClient defaults in `App.tsx` (P1 owns that file). Cache `Intl` formatters in `display.ts` (a `Map` keyed by locale and options). MailList imports any new helper from `../common/display` directly, because `common/index.ts` belongs to P1. Acceptance: `grep -c "new Intl" src/components/common/display.ts` ≤ 1 per formatter kind, inside the cache. A new display test asserts the same instance for the same key. Gates green.
- U5: export the leaf budget from `compose.ts` and use `MAX_LEAVES` in `search/index.tsx`. Acceptance: `grep -n "50 - 4" src/screens/search/index.tsx` → no match.
- Sweep: U8 (delete the `fixtures.ts:14-22` ResizeObserver stub and its restore), U9 (drop the `SqlEditor.tsx:91` eslint comment only), U15 (drop unused barrel names in `dsl/index.ts` and `mail/index.ts`), U19 (delete the tautology test in `sql-editor.test.tsx:17-21`), U20 (delete the vocabulary test in `global.test.tsx:96-123`, keep the dark-theme test). Acceptance: `grep -c "eslint" src/components/sql-editor/SqlEditor.tsx` → 0. `grep -c "R1-07\|hadRO" src/components/mail/fixtures.ts` → 0. The test count drops by exactly the deleted tests, and all others stay green.
- A7 first half: port `scripts/vendor-libpff.sh` to `scripts/vendor-libpff.py` (stdlib only, run id or tarball path as a required argument, no hardcoded run id or expiry note), delete the `.sh`, and update the references in `scripts/build.py:39`, spec §3 layout `:437` and README if it names the script. Acceptance: `test -e scripts/vendor-libpff.sh` RC=1. `python3 scripts/vendor-libpff.py --help` RC=0. `grep -rn "vendor-libpff.sh" scripts docs/superpowers README.md` → no match.
  - Cross-squad: `src-tauri/crates/pff/build.rs:27` also names `vendor-libpff.sh`. The core squad changes that message in round 3. The orchestrator routes it.

After round 3, every U, D and A item owned by ui is either fixed or listed above as not worth fixing.
