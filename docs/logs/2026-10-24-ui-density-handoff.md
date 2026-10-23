# Handoff: resizable splits do not work on Advanced SQL, Import, Settings cards

From session outlooker-40 to the next owner, 2026-10-24. The previous team is shut down. Nothing is in flight.

## User report (after a fresh build at HEAD 410f05f, real app)
1. On Advanced SQL and Import, no column width can be changed.
2. On Settings, no card width or height can be changed. Only the side nav resizes.

The user has not reported on Search; check it while you are there.

## Contract and spec (still in force)
- Contract: `docs/logs/2026-10-23-ui-density-contract.md`. Criterion 3 covers pane splits. The parking lot lists known should-fix items.
- Spec: `docs/superpowers/specs/2026-10-23-ui-density-and-resize-design.md` §1. It lists every boundary per screen.
- Round budget: 3. Rounds 1 and 2 are used, so this is round 3, the last.

## Code
- `src/components/common/splits.ts`: `useSplit(surface, defaults)`, `host(group, axis)`, group clamping, `track()`/`rest()`. Storage is under `outlooker.splits.<surface>`.
- `src/components/common/Splitter.tsx`: the `role="separator"` element, with pointer and keyboard input and `reverse`.
- Call sites:
  - `src/screens/advanced-sql/{index.tsx,SavedQueries.tsx}`: group `cols`, c1 to c3.
  - `src/screens/import/index.tsx`: group `cols`, p1 and p2.
  - `src/screens/settings/index.tsx`: `nav`, group `cols` c1/c2, scroll group `rows` r1/r2.
  - `src/screens/search/index.tsx`
  - `src/components/mail/MailList.tsx`
- The DataGrid column resizing in Settings and Import tables (`useColumnWidths`) and in the SQL `ResultTable` (`useGridColumnSizing`) predates this work and was out of scope. Ask the user whether "所有欄位寬度" also means those table columns.

## History (do not retry dead routes)
- `77964ee`, `19a447f`, `1326e26`: first version. Each border was clamped only against a fixed max, so panes could be hidden. Review refuted it (B1 and B2).
- `fb1779e`: Splitter measured its parent element. On search refine, list, DSL and settings nav the parent was the sized pane, which created a shrink feedback loop (N1). Review refuted it.
- `646dfc0`: the current fix (plan `scripts/tmp/splits-fix-plan-2.md`). `host()` on the layout container measures the container, minus grid gaps. The reviewer confirmed it, but only in jsdom with stubbed layout (probe `scripts/tmp/splits-probe/`). No agent ever ran it in the real webview.

## What is observed vs not
- Observed by the user: single-member boundaries work (settings nav). The grouped boundaries (`cols`, `rows`) do not.
- Not verified anywhere: real-webview layout values that `host()` reads.

## Hypotheses to test first (unverified)
1. `trackCount()` (`splits.ts:49`) counts `px` tokens in the computed `gridTemplateColumns`/`Rows`. If WKWebView returns another form (for example `repeat()` or `minmax()`), or the host is not the grid, the group extent goes wrong, and the clamp pins each member at its min or max. Then a drag changes nothing.
2. `inner()` returns 0 before layout or for a host with `display: contents`/flex. The group may then treat extent as 0 and clamp everything to min.
3. The host ref is attached to an element that is not the element laying out the group tracks.
4. The separator is rendered but covered (z-index or overflow) or has 0 hit size in grouped layouts, so the pointer never reaches it.

Check these in the running app first: log `inner()`, `trackCount()` and the resolved sizes, and confirm the separator elements are hit-testable. Then fix the root cause in the shared module, not per screen.

## Rules for this repo
- Read `CLAUDE.md` for the test policy and the cargo rules.
- Committed tests are 1 s or less, and only for core logic. Visual and wiring checks run once and are recorded in `scripts/tmp/`.
- Gate: `npm run typecheck` and `npm test` must both exit 0.
