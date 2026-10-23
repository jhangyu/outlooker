# Round 1 handoff — squad `ui`

Branch `techdebt/r1-ui`, final head `09dcada` (merged into `techdebt/integration`). Reviewer verdict: CONFIRMED at `09dcada`.
Plan: `docs/logs/2026-10-23/round-1-ui-plan.md`. Gate record: `docs/logs/2026-10-23/round-1-ui-gate.md`.

## Completed work and evidence

| item | commit | evidence |
|---|---|---|
| D1 superseded plans | 7e02d16 | 4 plan files deleted (25,432 lines). Still-true decisions ported into spec §0.5. The reviewer checked every §0.5 claim against the code. |
| U10 plan-ID comments (ui files) | 7e02d16, cdde7dc | `grep -rn "outlooker-plan\|plan Decision\|R1-0[0-9]\|R2-1[0-9]" src tests scripts docs --exclude-dir=logs` → only `src/components/mail/fixtures.ts:14` (U8, see round 2) |
| A4 unused `@tauri-apps/plugin-log` | 7e02d16 | 0 hits in package.json/package-lock.json. `log:default` removed from `src-tauri/capabilities/default.json`. |
| U1 keep screen state | cdde7dc, 73e2d15, 9bd59bf, a9b32da | Each screen is wrapped in React `<Activity>` and mounts on its first visit (AppFrame.tsx). MailList restores scrollTop. Search results use `staleTime: Infinity`. The new-mail bar is derived from global `status.counts`. `filterOptions` uses `keepPreviousData`. 4 new tests, each seen red then green (AppFrame.test.tsx, mail.test.tsx, search.test.tsx). |
| D11 in-app logo | cdde7dc | `src/assets/outlooker-logo.svg` (the mark only, viewBox `92 74 300 300`) is in the title bar and onboarding boxes. The app icon is unchanged. |
| Review F2/F3 spec fixes | 3dfe7e5 | §0.5 row heights (Search 32 px, Advanced SQL compact 73 px). The frontend log plugin claims are removed. |

Final gate at `a9b32da`, each command run once: `mise exec -- npm run build` RC=0. `mise exec -- npm test` RC=0 (72/72). `mise exec -- npx vitest run src/app src/components/mail src/screens/search` RC=0 (35/35). Baseline was 68 tests, all green.

## Known limitations

- **Flaky test**: in the full suite, `AppFrame.test.tsx` "data-folder move > shows a failed move's error..." failed once at 9bd59bf. It could not find the "Close" button. The test passes alone. After a9b32da it passed 3 runs in a row plus the gate run. The root cause is not confirmed. The test takes about 300 ms, so plain slowness does not explain it. If it recurs, check the event order in that test (status/progress emits versus the MoveDialog `started` state). Do not raise timeouts.
- **Undo history**: while Advanced SQL is hidden, the CodeMirror view is destroyed. It is re-created from the kept `sql` text, so undo history and cursor reset on each switch. The SQL text, result and selection are kept.
- **Scroll restore**: MailList saves `scrollTop` from `onScroll` and restores it on re-show. A jsdom test proves this (it simulates the webview reset). It is not proven in the real WKWebView/WebView2. If the webview fires a scroll event to 0 while hidden, the restore fails. Check it once by hand in the app: scroll the search list, switch screens, come back. The reading pane and SQL grid scroll are not restored (outside U1).

## Not worth fixing (round 1 judgments)

- U6: the hand-rolled `seq` works and is guarded by search.test.tsx. A rewrite touches 3 files and changes no behavior.
- U9 (behavior part): the editor is re-created on a language switch, which loses undo only. The event is rare. The dead `eslint-disable` comment is still on the round 3 sweep.
- U11: the test-only props cost nothing at runtime.
- U13: the pseudo-query stores work and are tested by the move flow. A rewrite would touch the riskiest flow.
- U14: internal casts in one component.
- U16: the mutation-mirror state works and is tested.
- U17: removing the UI default changes the loading UX for a value that has not drifted.

## Interface contracts

- The frontend↔Tauri command and event contract is unchanged: 24 commands and 6 events. The Search screen no longer subscribes to `library:changed` itself. `boot.ts:26` still feeds `counts` globally, and the Search screen reads `useAppStatus().counts`.
- Screens are mounted lazily and kept mounted. A screen must not rely on remounting to reset its state. A hidden screen has its effects torn down (Activity), so any per-screen event listener misses events while the screen is hidden. Derive the state from global context instead.

## Next round (round 2) — recommended ui slice

**Package 1 — e2e and build config (sonnet, mechanical).** Exclusive files: `package.json`, `tsconfig.json`, `tests/e2e/**`, `wdio.conf.ts`, `src-tauri/capabilities/default.json`.
- A6: add `"typecheck:e2e": "tsc --noEmit -p tests/e2e"` and chain it into `typecheck` and `build`. Fix `tests/e2e/scroll.e2e.ts:36` (TS2362, red today). Drop the redundant `exclude` from the root tsconfig. Acceptance: `mise exec -- npx tsc --noEmit -p tests/e2e` RC=0, and `npm run build` runs it.
- A8: in `tests/e2e/prepare.mjs:33`, replace `cargo run --release --locked -p outlooker-loadgen` with `cargo build --release --locked --workspace --bin outlooker-loadgen` and then run `src-tauri/target/release/outlooker-loadgen seed ...`. The workspace has no `default-members`, so a bare `cargo run --bin` does not find the binary. Acceptance: `grep -c " -p" tests/e2e/prepare.mjs` → 0.
- U18: in `tests/e2e/smoke.e2e.ts:4-7`, assert only the expected landing screen per group (`title-bar` seeded, `onboarding-lang-en` fresh).
- A5: change `core:default` to `core:event:default`. Static proof: the frontend's Tauri surface is `invoke` (src/api/call.ts:1; app commands need no permission because build.rs has no `AppManifest`), `listen`/unlisten (src/api/events.ts:1 → `core:event:default`) and dialog `open` (`dialog:allow-open`). There is no drag region and no window/app/path API. `tauri-plugin-wdio-webdriver 1.5.0` declares `permissions = []` and drives the webview natively.
  - **Runtime proof wanted (needs the user's long-run approval):** one full `mise exec -- npm run e2e` (build e2e binary + flow, seeded, scroll groups) on the round 2 merged state, all groups green. The flow group covers boot, onboarding, enqueue (invoke), the `library:changed` and `import:*` events (listen) and navigation. If an IPC permission were missing, `listen` or `invoke` would reject, the UI would stall, and the flow would fail. If the run is not approved, A5 ships on the static proof and the run happens with the user's Windows/SSH phase.

**Package 2 — docs (sonnet, mechanical).** Exclusive files: `docs/superpowers/specs/2026-10-21-outlooker-design.md`, `docs/design/variant-1/NOTES.md`, `README.md` (new, repo root — confirm root README ownership with the orchestrator; it is not in the ui list).
- D4+D5: set the spec status to implemented. Each §0.3 risk row gets its outcome or "open: Windows phase / 1M load test". Drop the `scripts/tmp/...` evidence paths and the `Owner:` lines.
- D6: regenerate the §3 repo layout from the tree.
- D7: add a "Not implemented (user ruling)" list at the top of NOTES.md, pointing to spec §0.2.
- D9: README of about 25 lines (prerequisites: mise node pin, `scripts/fetch-postgres.mjs`; `npm run build/test/e2e`; `python3 scripts/build.py`; cargo rule → root CLAUDE.md).

**Round 3 (unchanged from the plan):** U2, U3, U5, U12 | U4, U7. Then a sweep if budget remains: U8, U9 comment, U15, U19, U20, U21, A15, A7 (first half: port `vendor-libpff.sh` to Python with the run id as an argument).

**A7 outcome:** core keeps the `outlook-fixtures` feature, so `scripts/fixtures/make-pst.ps1` stays (it produces those fixtures, manual Windows + Outlook only). Only the `vendor-libpff.sh` → Python port remains for ui.

## Hand-offs outside ui

- F4 (src-tauri plan-ID comments, e.g. `src-tauri/src/lib.rs:18` R2-14): core squad.
- D2 (root CLAUDE.md plan sentence), D3 (handover doc), D10 (docs/logs ignore policy): orchestrator.
- The untracked `r1-ui/tmp/` (test-runner and reviewer scratch) is not committed. Delete it with the worktree.
