# Plan: column resize for hand-rolled grids (S3 SQL ResultTable, S4 MailList)

Owner: architect-grid (planning only). Base: main @ c1965dd. The persistence design and S1/S2 belong to architect-persist-opus.

## 1. Shared drag-logic decision: reuse Fluent, write no drag code

- Fluent `DataGrid` resizes with `useTableColumnSizing_unstable` (`node_modules/@fluentui/react-table/lib/components/DataGrid/useDataGrid.js:41`). S1/S2 get it through `resizableColumns`.
- Hand-rolled grids can call the same hook without DataGrid: `useTableFeatures({ columns, items: [] }, [useTableColumnSizing_unstable(params)])` (`dist/index.d.ts:1086,1106`). The hook does not read `items`.
- What the hook gives: `columnSizing_unstable.getColumnWidths()` returns the px width of each column (`lib/hooks/useTableColumnSizing.js:72`). `getTableHeaderCellProps(id).aside` is a `TableResizeHandle` element that already handles mouse drag, touch drag, keyboard resizing and the ARIA separator role (`useTableColumnSizing.js:85-98`, `useKeyboardResizing.js:115-126`). `enableKeyboardMode(id)` switches on keyboard resize mode (`useTableColumnSizing.js:56-60`).
- Mouse drag: listeners go on `targetDocument` from `useFluent`. The width changes once per animation frame, and `onColumnResize` fires on every change (`useTableColumnResizeMouseHandler.js:13-25,56-75`). The app root and `renderWithProviders` (`src/app/testing.tsx:115`) both supply a FluentProvider. Vitest's jsdom has `requestAnimationFrame` because `pretendToBeVisual` defaults to true (`node_modules/vitest/dist/chunks/index.CK-t0WMg.js:1293`).
- Result: **one drag engine for all 4 surfaces** (Fluent's). The project adds one shared module that holds persistence plus a small grid adapter. Nobody writes pointer-event code.

### Use `autoFitColumns: false` for S3/S4
With `autoFitColumns: true` (the default, `useTableColumnSizing.js:27`), Fluent rescales every width to fit the container (`useTableColumnResizeState.js:9-13,41-43`). The stored width and the rendered width would then differ. `false` renders exactly the stored px values. The table then scrolls horizontally when it is wider than its pane. This matches today's `minWidth: N*120px` (`ResultTable.tsx:40`).

### Fluent quirks the shared module must respect
- A new `columnSizingOptions` reference resets widths only when the options carry `idealWidth` (`useTableColumnResizeState.js:76-82`, `utils/columnResizeUtils.js:20-30`). The shared module sets only `defaultWidth` and `minWidth`, so a new reference resets nothing (see §2).
- Columns that are not yet in the state take `defaultWidth ?? idealWidth(150)` and `minWidth(100)` (`columnResizeUtils.js:1-2,34-41`). Pass `minWidth` explicitly.
- The handle's English `aria-label` "Resize column" is hard-coded (`useKeyboardResizing.js:120`). This is an i18n gap → parking lot.

## 2. Agreed shared API (FINAL, owned by architect-persist-opus)

Module: `src/components/common/columnWidths.ts` (WP-SHARED owns it, plus its test, the `index.ts` re-export and the `src/test/setup.ts` `localStorage.clear()` line).

```ts
useColumnWidths(surfaceKey: string, defaults: Record<string, { width: number; minWidth?: number }>):
  { resizableColumns: true; resizableColumnsOptions: { autoFitColumns: false };
    columnSizingOptions: TableColumnSizingOptions; onColumnResize(e: unknown, d: { columnId: TableColumnId; width: number }): void }
useGridColumnSizing(surfaceKey, defaults): { widths; total; headerProps(id): { aside; onKeyDown } }   // same file, built on useColumnWidths
```
- Storage: localStorage `outlooker.columnWidths.<surfaceKey>`. Writes are synchronous, with no debounce and no pruning. `minWidth` defaults to 48. Surface keys: `sql-result` (S3), `mail-list` (S4), `settings-indexes`, `import-files`.
- `columnSizingOptions` carries only `defaultWidth` (stored ?? default) and `minWidth`. With no `idealWidth`, a new options reference does not reset the widths of existing columns (`columnResizeUtils.js:20-30`). The module memoizes on the `defaults` reference, so **callers memoize `defaults`**.
- S1/S2 spread the result onto the grid: `<DataGrid {...sizing}>`.
- Adapter implementation notes (sent to persist and accepted as non-blocking):
  - Pass `{ ...sizing, autoFitColumns: false }` to `useTableColumnSizing_unstable`. The hook reads `autoFitColumns` at the top level (`useTableColumnSizing.js:27`). `resizableColumnsOptions` is a DataGrid-only prop.
  - Take `widths` and `total` from `getColumnWidths()`. Fluent's state is the live source during a drag.
  - Build columns with `useMemo(ids → createTableColumn({ columnId }))`.
- `headerProps(id).onKeyDown`: Enter or Space calls `enableKeyboardMode(id)` (cast needed: Fluent types the event as Mouse|Touch, but the code only calls `preventDefault` and `nativeEvent.stopPropagation`, `useTableColumnSizing.js:56-60`). Arrow keys then resize in 20 px steps, 5 px with Shift. Enter or Esc leaves the mode, and focus goes back to the header cell (`useKeyboardResizing.js:21-50,72-80`).
- Write rate: `onColumnResize` fires once per animation frame while dragging. The synchronous localStorage write per frame is acceptable.

## 3. S3 design: `src/screens/advanced-sql/ResultTable.tsx`

Today: `gridTemplateColumns: repeat(N, minmax(120px, 1fr))`, and the same `grid` object goes on the header (`:43`), every virtual row (`:57`) and the spacer (`:50`). Header cells are keyed by index (`:45`).

Change (about 15 lines):
1. Column ids come from the column names, so a name like `id` restores its width in any later query. Fluent needs unique ids, so the 2nd and later occurrence of a duplicate name use the key `name#j`:
   `const ids = useMemo(() => columns.map((c, j) => columns.findIndex((x) => x.name === c.name) === j ? c.name : `${c.name}#${j}`), [columns]);`
   `const defaults = useMemo(() => Object.fromEntries(ids.map((id) => [id, { width: 160 }])), [ids]);`
2. `const { widths, total, headerProps } = useGridColumnSizing("sql-result", defaults);`
3. `const grid = { gridTemplateColumns: ids.map((id) => `${widths[id]}px`).join(" "), minWidth: `${total}px` };` The header, rows and spacer stay unchanged because they already share `grid`.
4. Header cell (`:45`): add `position: "relative"` and `tabIndex={0}` to the outer `columnheader` span, and spread `onKeyDown`. Move the text into an inner span that carries `s.cell` ellipsis. The handle's -8px margin overhangs the cell edge (handle styles: absolute, right 0, 16 px wide). Render `{headerProps(id).aside}` after the text.
- Ceiling: storage gets one key per distinct SQL column name that a user ever resized. Each key is a few bytes, so there is no cap. `// ponytail:` comment; add an LRU cap if the stored blob ever matters.
- Ceiling: during a drag, each frame re-renders the visible rows (about 30 + overscan 10, `:39`). Rows are cheap spans. Optimize only if dragging lags.

## 4. S4 options: `src/components/mail/MailList.tsx`

Today: a non-compact row is `24px minmax(120px, 22%) 1fr auto` (`:58`), which is avatar | sender | subject+snippet | meta. The list has no header. The root is `role="listbox"` and is also the scroll element (`:123`). Compact mode (`:63-66,145-154`, used by Advanced SQL `index.tsx:252-253`) has no separable columns.

### Option A: compact header row, sender column resizable (recommended only if the user wants it)
- Render a header only when `!p.compact`. MailList returns a wrapper `<div>` (flex column, height 100%) that holds a header `<div role="presentation">` (labels "From" | "Subject" | "Date", 24 px, `Caption1Strong`) and the existing listbox div with `flex: 1; minHeight: 0`. The header must stay outside the listbox: a listbox may contain only options and groups.
- Only the **sender** boundary is resizable: `useGridColumnSizing("mail-list", useMemo(() => ({ sender: { width: 200, minWidth: 96 } }), []))` (or a module-level const). The row template becomes `24px ${widths.sender}px 1fr auto`, set inline on the header and on each non-compact row, which replaces `:58`. Subject stays `1fr`, because it fills the remaining space and a stored width means nothing for it. Date/meta stays `auto`, because it holds a variable icon, badge and day. Resizing these two adds nothing, since the right edge of a `1fr` column is the left edge of `auto`.
- Alignment: Windows WebView2 shows classic scrollbars, which narrow the rows but not the header. Put `scrollbarGutter: "stable"` on the listbox and on the header (with `overflow: hidden`). The tracks then line up. macOS overlay scrollbars are 0 px wide.
- New i18n keys: `list.col.from`, `list.col.subject`, `list.col.date` in the `search` namespace (en + zh-TW).
- Cost: about 25 lines in MailList, 6 i18n strings, and 1 test. Risk: search screen tests that count children or use `getByRole("listbox")` (`mail.test.tsx:77`) keep working, because the listbox is unchanged.
- `techdebt/integration` overlap: that branch edits the root listbox `<div>` line (adds `onScroll`) and adds `lastTop` + `useLayoutEffect` after `:89`. Wrapping the listbox in a new div re-indents that line → **one textual conflict on one line**. The fix keeps both edits. Merge order: whichever lands second resolves it. The implementer must not touch the `useRef` block near `:89`.

### Option B: leave MailList out
- The list is a message list with a single visual column of text. It is not a data table (the inventory marks it "NO header", `column-inventory.md` S4). The contract covers "every table / result grid / column-header list". MailList is none of these today.
- Cost: 0. A user who needs columns uses Advanced SQL table mode (S3).

**Recommendation: B.** A one-boundary header adds new UI and an i18n surface for little gain. The pane is already narrow (`search/index.tsx:41` `minmax(420px, 34%)`). **The user decides.** If the user picks A, WP-S4 below applies.

## 5. Test plan (vitest, `src/test/setup.ts` already stubs ResizeObserver)

- **Shared module** (WP-SHARED, persist plan): seed storage → render a probe → assert the restored width. Drag or call `onColumnResize` → assert the storage value. This test meets contract item 4.
- **S3** (`src/screens/advanced-sql/advanced-sql.test.tsx`, new cases):
  1. Restore: seed storage for key `outlooker.columnWidths.sql-result`, column `kind` = 240 → run SQL with `TABLE` (`:22-28`) → the header `role="row"` style `gridTemplateColumns` starts with `240px`.
  2. Drag saves: `fireEvent.mouseDown(handle, { clientX: 100, button: 0 })`, with `handle = header.querySelector(".fui-TableResizeHandle")` (the handle is aria-hidden until keyboard mode), then `fireEvent.mouseMove(document, { clientX: 160 })` and `fireEvent.mouseUp(document)`. `await waitFor` the rAF, then assert that the stored width of `kind` = 220 and the template starts with `220px`.
  3. Keyboard: focus the `columnheader`, press Enter, press ArrowRight → the stored width grows by 20.
  4. Existing `sql-table-row` assertions (`:75,:83`) stay green unchanged.
- **S4 (only if A)**, `src/components/mail/mail.test.tsx`: seed `outlooker.columnWidths.mail-list` with `{"sender":300}` → the first `mail-row` style contains `300px`. With `compact`, no header renders.
- Prove the tests can fail: an implementer runs each restore/save test once with the persist write commented out and sees red before green (lessons-learned rule).
- Gate: `mise exec -- npm test` and `mise exec -- npm run build`.

## 6. Work packages (exclusive file ownership)

| WP | Owner files | Depends on |
|---|---|---|
| WP-SHARED (persist plan) | `src/components/common/columnWidths.ts` + test, `src/components/common/index.ts`, `src/test/setup.ts` (localStorage.clear line) | none, lands first |
| WP-S3 | `src/screens/advanced-sql/ResultTable.tsx`, `src/screens/advanced-sql/advanced-sql.test.tsx` | WP-SHARED API (can code against the signature in §2 in parallel, then integrate) |
| WP-S4 (only if Option A) | `src/components/mail/MailList.tsx`, `src/components/mail/mail.test.tsx`, `src/locales/{en,zh-TW}/search.json` | WP-SHARED; coordinate the merge order with `techdebt/integration` |
| WP-S1/S2 | persist architect's plan | WP-SHARED |

`useGridColumnSizing` lives in WP-SHARED, so S3 and S4 contain no drag or persist code.

## 7. Agreement record
architect-persist-opus, FINAL (quoted): "FINAL: src/components/common/columnWidths.ts — useColumnWidths(surfaceKey: string, defaults: Record<string, { width: number; minWidth?: number }>): { resizableColumns: true; resizableColumnsOptions: { autoFitColumns: false }; columnSizingOptions: TableColumnSizingOptions; onColumnResize(e: unknown, d: { columnId: TableColumnId; width: number }): void } + useGridColumnSizing(surfaceKey, defaults): { widths; total; headerProps(id): { aside; onKeyDown } } (same file, built on useColumnWidths)."
architect-grid replied AGREED with no blocker, plus the two non-blocking implementation notes in §2.
architect-persist-opus confirmed FINAL and recorded both notes in `plan-column-resize-persist.md` §7. Surface keys: sql-result, mail-list, settings-indexes, import-files.

## Parking lot
- The Fluent handle `aria-label` "Resize column" is English-only (`useKeyboardResizing.js:120`).
- Pane splitters (`search/index.tsx:41`) are out of scope per the inventory.
