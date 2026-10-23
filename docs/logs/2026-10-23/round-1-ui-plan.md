# Round 1 — squad `ui` plan

Worktree `/Users/jhangyu/project/outlooker-wt/r1-ui`, branch `techdebt/r1-ui`, base `c1965dd`.
Contract: `docs/logs/2026-10-23/contract-techdebt.md` (main tree). Brief: `docs/logs/2026-10-23/round-brief.md` (main tree).

## Selected items (round 1)

| id | verified in code | package |
|---|---|---|
| U1 | `AppFrame.tsx:231,242` mounts only `SCREENS[screen]`; SQL text `advanced-sql/index.tsx:122-129`, search inputs/selection `search/index.tsx:70-100` are `useState` → lost on navigation. Confirmed. | P1 |
| D11 | `AppFrame.tsx:164-166` and `onboarding/index.tsx:212-214` render a text "O" in a 28 px box. Logo file is a 1200x420 lockup (mark + "outlooker" wordmark + tagline on a `#F7FAFF` card). Confirmed. | P1 |
| D1 | 4 plan files, 25,432 lines. Each has a `**Decisions (hard-to-reverse only)**` block (plan.md:20-27, foundation:13-18, backend:11-17, frontend:11-17) and plan.md §9 "Resolved questions" (:468-480). No reference from src/tests/scripts/spec to the plan files by path. Confirmed. | P2 |
| U10 | plan pointers at `AppStatusContext.tsx:77`, `compose.ts:52`, `MailList.tsx:83`, `tests/e2e/support.ts:1`, `fixtures.ts:14` (the last is U8). Confirmed. `MailList.tsx:83` goes to P1 (file owned by P1). | P2 (+P1 for MailList.tsx:83) |
| A4 | `@tauri-apps/plugin-log` only in package.json:28 / lockfile; zero imports in src/tests/scripts. `log:default` in `capabilities/default.json:6`. Rust `tauri_plugin_log` (lib.rs:44) needs no webview permission. Confirmed. | P2 |

## Refuted items

None refuted. Corrections to scanner claims:
- D1: the master plan has no single "Decisions" section with four entries; there are four `Decisions` blocks (one per plan file) plus master §9 "Resolved questions". All still-true entries are ported.
- A6: understated. `mise exec -- npx tsc --noEmit -p tests/e2e` exits 1 today: `tests/e2e/scroll.e2e.ts(36,31): error TS2362`. A latent type error, not just an unchecked config.
- A8: the scanner's fix (`cargo run --release --bin outlooker-loadgen`) does not work: the workspace has no `default-members`, so a bare `cargo run --bin` searches only the root package. Correct fix: `cargo build --release --locked --workspace --bin outlooker-loadgen`, then run `target/release/outlooker-loadgen`.
- A5: provable statically. Frontend Tauri surface = `invoke` (src/api/call.ts:1, app commands, no `AppManifest` in build.rs → no permission needed), `listen` (src/api/events.ts:1 → `core:event:allow-listen/unlisten`), dialog `open` (`dialog:allow-open`). No `data-tauri-drag-region`, no `@tauri-apps/api/window|app|path`. The e2e driver `tauri-plugin-wdio-webdriver 1.5.0` declares `permissions = []` (registry `permissions/default.toml`) and drives the webview natively. So `core:default` → `core:event:default` is safe by static proof; one runtime e2e pass is still wanted (round 2, see roadmap).

## Roadmap rounds 2–3 (tentative)

Round 2
- P1 (e2e / build config, mechanical): A6 (add `typecheck:e2e`, chain into `typecheck`, fix `scroll.e2e.ts:36`, drop redundant `exclude`), A8 (fix above), U18 (smoke asserts only the expected landing screen), A5 (`core:default` → `core:event:default`). Ask the orchestrator for one `npm run e2e` long-gate run (user approval) as A5 runtime proof; if not approved, A5 ships on the static proof above.
- P2 (docs, mechanical): D4+D5 (spec status → implemented; §0.3 rows get outcome or "open: Windows phase / 1M load test"; drop `scripts/tmp/...` paths and `Owner:` lines), D6 (regenerate §3 repo layout from the tree), D7 (NOTES.md "Not implemented (user ruling)" list), D9 (README, ~25 lines).

Round 3
- P1 (frontend perf / single source, mechanical): U2 (`gcTime` on `searchMail`/`searchCount`/`mailGet`), U3 (cache `Intl` formatters in display.ts), U5 (export leaf budget from compose.ts, use `MAX_LEAVES`), U12 (one `toAppError` in api/call.ts).
- P2 (judgment): U4 (`useEnqueuePst` hook), U7 (`src/api/queries.ts` with `queryOptions` for `pstList`/`dbStatus`/`moveProgress`).
- Sweep if budget remains (each a few lines, otherwise "not worth fixing"): U8 (fixtures.ts duplicate ResizeObserver stub), U9 (drop dead `eslint-disable` comment only), U15 (dead barrel exports), U19 (delete tautology test), U20 (drop vocabulary test, keep dark-theme test), U21 (test rename), A15 (delete `--self-test` branch in fetch-postgres.mjs), A7 (port `vendor-libpff.sh` to Python with run id as argument).

Not ui-owned (hand back to orchestrator):
- D2: root `CLAUDE.md` last sentence — orchestrator-owned file; remove after D1 merges.
- D3, D10: under `docs/logs/` — orchestrator-owned.
- A7 second half (`make-pst.ps1` + `outlook-fixtures` tests and cfg in core): needs a core-squad decision; ui can only delete the ps1 if core deletes the feature.
- D8: done by user. D12: no drift, no action.

## Not worth fixing

- U6: the hand-rolled `seq` works and is guarded by `search.test.tsx:52`; the rewrite touches 3 files for no behavior change.
- U9 (behavior part): editor re-creation on language switch loses undo history only; rare event, Compartment rewrite not justified.
- U11: two test-only props cost nothing at runtime.
- U13: pseudo-query stores work and are tested through the move flow; replacing them touches 4 files in the riskiest flow.
- U14: internal casts in one component, no external blast radius.
- U16: mutation-mirror state works and is tested; refactor risk > value.
- U17: removing the UI default changes loading UX for a value that has not drifted.

## Package 1 — keep screens mounted (U1) + in-app logo (D11) — model: sonnet

Judgment-dense items: 2.

Exclusive files: `src/app/AppFrame.tsx`, `src/app/AppFrame.test.tsx`, `src/screens/onboarding/index.tsx`, `src/screens/onboarding/onboarding.test.tsx`, `src/components/mail/MailList.tsx`, `src/components/mail/mail.test.tsx`, `src/components/mail/fixtures.ts`, `src/assets/outlooker-logo.svg` (new), `src/app/global.test.tsx`, plus any other `src/**/*.test.tsx` that breaks because hidden screens stay in the DOM (report each one).

U1 design (decided): wrap each screen in React 19 `<Activity mode={screen === id ? "visible" : "hidden"}>` inside `<main>` (react 19.3.1 is installed; Activity hides with `display:none`, keeps state, and destroys effects while hidden, so hidden screens stop their `useQuery` subscriptions, `refetchInterval` polling and `library:changed` listener). No state lifting. Because a `display:none` subtree can lose `scrollTop` in the webview, `MailList` keeps the last `scrollTop` from its `onScroll` in a ref and restores it in a `useLayoutEffect` on (re)mount when it differs. The CodeMirror view is destroyed/re-created with the preserved `sql` state (undo history reset is accepted; state it in the report).

D11 design (decided): one asset `src/assets/outlooker-logo.svg` derived from `/Users/jhangyu/project/outlooker/outlooker-logo.svg`: keep `<defs>` and the mark group (`<g transform="translate(76 59)" ...>`), remove the background `<rect width="1200" ...>` and the wordmark `<g transform="translate(425 118)">`, set a square `viewBox` around the mark (about `92 74 300 300`; adjust so nothing is clipped). Import it as a URL (`import logo from "../assets/outlooker-logo.svg"`, typed by `vite/client`) and render `<img src={logo} alt="" />` inside the existing 28 px `.logo` boxes in both places (title bar box keeps its white background for contrast on the brand bar). Do not touch `src-tauri/icons/` or `tauri.conf.json`. Root copy stays; the orchestrator deletes it after merge.

Acceptance:
1. New test in `AppFrame.test.tsx`: type SQL text in the Advanced SQL editor, click `nav-import`, click `nav-advancedSql`, the editor still holds the text. Same for the search text box (`nav-search` → `nav-settings` → `nav-search`). Both tests are red on the base commit (run once before the fix, report the red output line) and green after.
2. New test in `mail.test.tsx`: MailList scrolled to N, hidden via an `<Activity mode="hidden">` wrapper, `scrollTop` set to 0 (simulates the webview reset), shown again → `scrollTop === N`. Red without the restore, green with it.
3. `grep -n ">\s*O\s*<\|^\s*O$" src/app/AppFrame.tsx src/screens/onboarding/index.tsx` → no match. `grep -c "outlooker-logo.svg" src/app/AppFrame.tsx src/screens/onboarding/index.tsx` → 1 each.
4. `grep -c "<text" src/assets/outlooker-logo.svg` → 0; `grep -c "width=\"1200\"" src/assets/outlooker-logo.svg` → 0.
5. `grep -n "plan Decision" src/components/mail/MailList.tsx` → no match (U10 part).
6. Gates below exit 0.

## Package 2 — retire superseded plans (D1, U10) + drop unused log plugin (A4) — model: sonnet

Judgment-dense: D1 (decision port). Mechanical: U10, A4.

Exclusive files: `docs/superpowers/plans/2026-10-21-outlooker-plan.md`, `...-plan-foundation.md`, `...-plan-backend.md`, `...-plan-frontend.md` (deleted with `git rm`), `docs/superpowers/specs/2026-10-21-outlooker-design.md`, `src/app/AppStatusContext.tsx`, `src/screens/search/compose.ts`, `tests/e2e/support.ts`, `package.json`, `package-lock.json`, `src-tauri/capabilities/default.json`.

D1: add a section `## 0.5 Implementation decisions` after §0.4 in the spec. Port every entry of the four `**Decisions**` blocks and plan.md §9 that is still true in the code, one bullet each with its rationale (verify each against the tree, e.g. `src/locales/<lang>/<ns>.json`, `core/src/dto/` leaf, 24 commands incl. `app_retry`). Drop entries that are process-only (round/task ownership, kickoff ordering) or no longer true, and list the dropped ones in the report with a reason. Then `git rm` the four plan files and remove the empty `docs/superpowers/plans/` directory.
U10: replace the plan/task pointer in `AppStatusContext.tsx:77`, `compose.ts:52`, `tests/e2e/support.ts:1` with a one-line rationale (or point to the spec §0.5 entry). `fixtures.ts:14` is left to round 3 (U8 deletes those lines).
A4: remove `@tauri-apps/plugin-log` from package.json, update the lockfile with `mise exec -- npm install --package-lock-only` (do NOT run a plain `npm install`/`uninstall`: `node_modules` is a symlink into the main tree), remove `"log:default"` from `capabilities/default.json`.

Acceptance:
1. `ls docs/superpowers/plans 2>/dev/null | wc -l` → 0. `grep -n "^## 0.5" docs/superpowers/specs/2026-10-21-outlooker-design.md` → 1 match.
2. `grep -rn "outlooker-plan\|superpowers/plans\|plan Decision\|R1-0[0-9]\|R2-1[0-9]" src tests scripts docs --exclude-dir=logs` → only `src/components/mail/MailList.tsx` (P1) and `src/components/mail/fixtures.ts` (round 3); zero once P1 lands.
3. `grep -c "plugin-log" package.json package-lock.json` → 0 each. `grep -c "log:default" src-tauri/capabilities/default.json` → 0. `node -e "JSON.parse(require('fs').readFileSync('src-tauri/capabilities/default.json'))"` exits 0.
4. Gates below exit 0. (Capability validity is also checked by `tauri-build` in the orchestrator's merged cargo gate.)

## Gate commands (run in the worktree root; test-runner `r1-ui-test-haiku`)

1. `mise exec -- npm run build` → exit 0 (tsc + vite).
2. `mise exec -- npm test` → exit 0 (vitest; baseline 16 files / 68 tests).
3. Targeted: `mise exec -- npx vitest run src/app src/components/mail` (P1), the grep checks per package.
Record `RC=$?` right after each command inside the artifact. Artifact path: `docs/logs/2026-10-23/round-1-ui-gate.md`.

## Baseline gate result (base `c1965dd`, before any change, 2026-10-23)

- `mise exec -- npm run build` → RC=0 (vite built in 1.47 s; only the >500 kB chunk warning).
- `mise exec -- npm test` → RC=0, 16 files / 68 tests passed, 11.6 s.
- Extra (not a gate): `mise exec -- npx tsc --noEmit -p tests/e2e` → RC=1, `scroll.e2e.ts(36,31) TS2362` (A6, round 2).
