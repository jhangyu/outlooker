# Contract: UI density and resizable layout (frozen 2026-10-23)

Spec: `docs/superpowers/specs/2026-10-23-ui-density-and-resize-design.md` (commit bbe164d). Approved mockups: `scripts/tmp/ui-redesign/header-b.html`, `scripts/tmp/ui-redesign/hint-c.html`.

## End state
Every pane split on Search, Advanced SQL, Import and Settings (including Settings card rows) and every search result column is draggable and persisted; the reading-pane header is one 49 px token row (81 px two rows below 900 px); search hints live in status segments attached to the 條件 button with no hints row.

## In scope (one owner each)
- D1 splits module + Settings: `src/components/common/{splits.ts,Splitter.tsx,columnWidths.ts,columnWidths.test.tsx,splits.test.tsx,index.ts}`, `src/screens/settings/index.tsx` — owner impl-splits-sonnet
- D2 apply splits to Advanced SQL + Import: `src/screens/advanced-sql/{index.tsx,SavedQueries.tsx}`, `src/screens/import/index.tsx` — owner impl-splits-sonnet (after D1)
- D3 reading-pane header: `src/components/mail/{ReadingPane.tsx,AttachmentCard.tsx,index.ts}` + new files under `src/components/mail/` — owner impl-header-sonnet
- D4 hints segments + Search screen splits: `src/screens/search/index.tsx` + new files under `src/screens/search/`, `src/screens/search/search.test.tsx`, `tests/e2e/flow.e2e.ts` — owner impl-hints-sonnet (splits part after D1 API handoff)
- D5 result-list columns: `src/components/mail/MailList.tsx` — owner impl-columns-2-sonnet (round 2)
- Shared, lock via main before editing: `src/components/mail/mail.test.tsx`, `src/locales/{zh-TW,en}/search.json`, `src/locales/{zh-TW,en}/common.json`, `src/locales/{zh-TW,en}/settings.json`, `src/locales/{zh-TW,en}/sql.json`, `src/locales/{zh-TW,en}/import.json`

## Out of scope
Onboarding screen, dark-theme tuning, resize grips/icons, existing DataGrid column resizing in Settings/Import, new e2e or perf tests.

## Acceptance criteria
1. `npm run typecheck` exits 0.
2. `npm test` exits 0 (run by test-runner-haiku in the final gate window).
3. Each boundary listed in spec §1 table renders a `role="separator"` element with `aria-orientation`; dragging or arrow keys change the stored size under `outlooker.splits.<surface>`; a reload restores it; double-click restores default (D1 test covers clamp + round-trip; per-screen wiring verified by grep + one run).
4. MailList (changed by the user 2026-10-23, round 2):
   - The subject column takes the remaining width (1fr). Every other column (sender, date) has a stored width and a drag handle on its boundary.
   - Header cells can be dragged to reorder the sender, subject and date columns. The keyboard also reorders them: Alt+ArrowLeft and Alt+ArrowRight on a focused header cell.
   - The rows follow the header order.
   - Order and widths persist across reloads through one storage path.
   - With nothing stored, a 420 px list pane shows all columns unclipped.
5. ReadingPane: no `Persona`, no per-kind recipient grid lines, no attachment-card block in the header; header is a Fluent `Toolbar` with `Overflow`; each token opens a Popover with the full list; data-testid `reading-pane` kept.
6. Search: the `hints` stack is gone; status segments sit inside a `role="group"` with the 條件 button; data-testids `short-query-hint`, `new-mail-bar`, `new-mail-refresh` are kept on the new elements; new-mail segment refreshes in one click; each `problem.*` code has a `problem.short.*` key in zh-TW and en.
7. No new npm dependency (`package.json` dependencies unchanged).

## Round budget
3 rounds. Round 1 is used up (review cycles exhausted). Round 2 started 2026-10-23, approved by the user.

## Parking lot
- S4: every splitter has the same generic name "Resize". Per-boundary names need locale keys.
- S5: a collapsed SavedQueries schema keeps its stored height.
- S7: the narrow-window warning segment has no Tooltip. The full text is still in the popover.
- Visual check in the running app: overflow, the two-row header, and dragging every splitter. No agent can render.
- F1: the result-list date column is a fixed 80 px. With a duplicate-count badge it needs about 83 to 99 px, so the date can spill over the subject. Needs a visual check.
- F2: result-list header cells have no visible focus ring, yet they are the Alt+Arrow reorder target.
- F3: the reorder shortcut is not exposed through aria-keyshortcuts.
- F4: MailList.tsx hard-codes the "outlooker.splits." prefix, which duplicates the private constant in splits.ts.
- F5: the 40 % cap on the search condition panel resolves against the new wrapper, not the main column.
- F6: useSplit host() caches by group only, so the axis is ignored after the first call.
- Another session's uncommitted MailPreview.tsx change makes 2 mail.test.tsx srcdoc tests fail. That session must update the tests when it commits.

## Outcome
Round 2 closed 2026-10-24. Reviewer verdict CONFIRMED (scripts/tmp/review-ui-density-round2b.md). Gate at 646dfc0: typecheck RC=0, npm test 60/62, and both failures come from the other session's MailPreview.tsx change.
