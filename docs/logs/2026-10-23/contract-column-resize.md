# Contract: user-resizable, persisted table columns

## End state
Every UI surface that shows tabular/result columns lets the user drag-resize each column, and each width survives an app restart.

## In scope
- Every table / result grid / column-header list in `src/` (inventory: `docs/logs/2026-10-23/column-inventory.md`).
- One shared resize + persistence mechanism used by all of them.
- MailList gets a new header row with resizable columns (user decision: grid plan §4 option A).
- Base: implementation starts only after `techdebt/integration` is merged into `main` (user decision).

## Out of scope
- Column reorder, hide/show, sort changes.
- Backend (`src-tauri/`) changes unless the architects prove the frontend cannot persist on its own.

## Acceptance criteria
1. Every surface listed in the inventory has draggable column borders (mouse drag changes the column width).
2. Widths persist per surface per column and are restored after a full app restart.
3. One shared module implements resize + persistence; no per-surface copies.
4. A vitest test fails if the width is not saved or not restored.
5. `mise exec -- npm run build` and `mise exec -- npm test` are green.

## Round budget
3 rounds.

## Parking lot
(empty)
