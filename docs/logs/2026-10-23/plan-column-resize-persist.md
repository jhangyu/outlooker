# Plan: column-width persistence (shared module) + Fluent DataGrid wiring (S1, S2)

Contract: `docs/logs/2026-10-23/contract-column-resize.md`. Inventory: `docs/logs/2026-10-23/column-inventory.md`.
Companion plan (S3, S4, grid adapter details): `docs/logs/2026-10-23/plan-column-resize-grid.md` (architect-grid-opus).
Scope of this plan: storage decision, the one shared module API, S1/S2 wiring, test plan, work packages.

## 1. Storage decision: webview `localStorage` (frontend only, no backend change)

Proof that localStorage survives an app restart in this build:
- `src-tauri/tauri.conf.json:13-15`: the only window is declared in config, with no `incognito` and no `dataDirectory`. `src-tauri/src/lib.rs` creates no webview in code (grep `Webview|incognito` = no hits).
- Tauri config default: `tauri-utils-2.10.2/src/config.rs:2447` `incognito: false`.
- macOS: `wry-0.57.0/src/wkwebview/mod.rs:231-244`: with `incognito == false` and no `data_store_identifier`, wry uses `WKWebsiteDataStore::defaultDataStore` (persistent, on disk). `nonPersistentDataStore` is used only for incognito (`:236`).
- Windows: `tauri-2.12.5/src/manager/webview.rs:559-564` forces the WebView2 user data folder to `app_local_data_dir()` (persistent). Private mode is set only from `incognito` (`wry-0.57.0/src/webview2/mod.rs:414`).
- The release origin is fixed (`tauri://localhost` / `http://tauri.localhost`), so the localStorage partition is stable between runs. The dev server (`localhost:1420`) uses its own partition. This is expected.

Rejected option: `settingsGet/settingsUpdate` (`src/api/commands.ts:44-45`). It needs a DTO change in `outlooker-core/src/dto/settings.rs:7-21`, a bootstrap JSON or Postgres write, and ts-rs regeneration. The contract allows a backend change only when the frontend cannot persist; it can. No store or fs plugin is needed: capabilities stay `core:default, dialog:allow-open, log:default`.

Known limit: localStorage belongs to one OS user profile. Deleting the app's WebKit/WebView2 data clears it. Column widths are cosmetic, so this is acceptable.

## 2. Shared module API (agreed, see §7)

File: `src/components/common/columnWidths.ts` (new). Re-exported from `src/components/common/index.ts`.

```ts
import type { TableColumnId, TableColumnSizingOptions } from "@fluentui/react-components";

export type ColumnDefaults = Record<string, { width: number; minWidth?: number }>;

/** Persistence + Fluent sizing params. Spread onto <DataGrid> (S1/S2); fed to useTableColumnSizing_unstable by useGridColumnSizing (S3/S4). */
export function useColumnWidths(surfaceKey: string, defaults: ColumnDefaults): {
  resizableColumns: true;
  resizableColumnsOptions: { autoFitColumns: false };
  columnSizingOptions: TableColumnSizingOptions;
  onColumnResize: (e: unknown, d: { columnId: TableColumnId; width: number }) => void;
};

/** Hand-rolled grid adapter (S3, S4). Spec: plan-column-resize-grid.md §2. */
export function useGridColumnSizing(surfaceKey: string, defaults: ColumnDefaults): {
  widths: Record<string, number>;
  total: number;
  headerProps: (id: string) => { aside: React.ReactNode; onKeyDown: (e: React.KeyboardEvent) => void };
};
```

Surface keys (fixed registry): `settings-indexes` (S1), `import-files` (S2), `sql-result` (S3), `mail-list` (S4, only if the user picks the header option).

Behavior of `useColumnWidths` (implementation contract):
- Storage key `outlooker.columnWidths.<surfaceKey>`. Value: JSON `Record<string, number>`.
- `columnSizingOptions` is built in a `useMemo` on `[surfaceKey, defaults]`. That memo reads storage and sets, per id, `defaultWidth: stored[id] ?? defaults[id].width` and `minWidth: defaults[id].minWidth ?? 48`. **No `idealWidth`.** Reason: a new options reference resets widths only if the options carry `idealWidth` (`@fluentui/react-table/lib/utils/columnResizeUtils.js:20-30`). With only `defaultWidth`, a new reference leaves current widths alone and gives new columns their stored width (`:34-44`). Callers pass a stable `defaults` (module const for S1/S2, `useMemo` on columns for S3). The memo does not depend on stored state, so the reference does not change during a drag.
- `onColumnResize`: read the stored map, set `map[columnId] = Math.round(width)`, write it back. The write is synchronous with no debounce. A setItem under 1 KB once per animation frame is negligible, and a synchronous write cannot lose the last width when the app quits in the middle of a debounce (`ponytail:` comment). Fluent clamps the width to minWidth before the callback (`useTableColumnResizeState.js:88-95`).
- Malformed JSON, non-object values or non-finite numbers are ignored and the default is used. All localStorage access is in try/catch. A failure means defaults, never a crash.
- Columns that no longer exist stay in storage and are never pruned. Stale S1/S2 ids are harmless because those ids are static. For S3, a column name that comes back in a later query gets its width back. `ponytail:` ceiling: one number per distinct SQL column name ever resized. Add an LRU cap if that ever matters.
- Dynamic ids (S3): key = column name, with `#j` appended for duplicates (grid plan §3).
- `resizableColumnsOptions: { autoFitColumns: false }` for every surface: see §3.

## 3. Fluent DataGrid facts (installed `@fluentui/react-table` 9.19.23, verified in node_modules)

- Props: `resizableColumns`, `columnSizingOptions: Record<id, {minWidth, idealWidth, padding, defaultWidth, autoFitColumns}>`, `onColumnResize(e, {columnId, width})`, `resizableColumnsOptions: {autoFitColumns?: boolean}` (`dist/index.d.ts:151,213,217,230-235,548-551`). DataGrid resizes through `useTableColumnSizing_unstable` (`lib/components/DataGrid/useDataGrid.js:41`), the same engine S3/S4 use.
- `autoFitColumns` defaults to `true` (`lib/hooks/useTableColumnResizeState.js:53`). When it is on, Fluent re-stretches the widths to the container on every update (`:10,:22,:41-43`). A restored width then differs from the saved width, and the last column gets no resize handle (`lib/hooks/useTableColumnSizing.js:89`). **Decision: `autoFitColumns: false`** (in the hook's return value). The table then gets `minWidth: fit-content` and scrolls horizontally inside its existing scroll container.
- The default per-column `minWidth` is 100 (`lib/utils/columnResizeUtils.js:2`). That would inflate S2's 44–88 px columns. The hook always passes a minWidth (default 48).
- `defaultWidth` is read when a column first enters the state (`columnResizeUtils.js:36-44`). The hook reads storage synchronously inside the memo, so the stored width is already present on the first render.

## 4. S1 / S2 wiring

S1 `src/screens/settings/index.tsx` (grid `:241`, columns `:125-139`):
```tsx
const INDEX_COLUMNS: ColumnDefaults = { name: { width: 260 }, table: { width: 180 }, size: { width: 96 }, state: { width: 120 } }; // module level
const sizing = useColumnWidths("settings-indexes", INDEX_COLUMNS);
<DataGrid {...sizing} items={...} columns={columns} getRowId={...}>
```
Keep the `s.num` alignment classes.

S2 `src/screens/import/index.tsx` (grid `:260-290`):
```tsx
const FILE_COLUMNS: Record<ColumnKey, { width: number }> = {
  name: { width: 220 }, size: { width: 88 }, messages: { width: 88 }, new: { width: 72 },
  date: { width: 192 }, status: { width: 128 }, remove: { width: 48 },
}; // module level
const sizing = useColumnWidths("import-files", FILE_COLUMNS);
<DataGrid {...sizing} className=... items=... columns=... getRowId=... sortable>
```
- Delete the flex-basis classes `col_name … col_remove` (`:68-74`), the `basis` map (`:213-215`), and `grid: { minWidth: "832px" }` (`:63`) together with `className={s.grid}`. Fluent now sets width, minWidth and maxWidth inline on every cell, and the flex classes would fight it. `colClass` keeps only `s.cell` and `s.num`.
- Keep `sortable`. Fluent renders the resize handle as an `aside` outside the sort button, so a drag does not sort.

## 5. Test plan

Contract criterion 4 is covered by one test owned by WP-SHARED: `src/components/common/columnWidths.test.tsx` (vitest, jsdom localStorage, `renderHook` from `@testing-library/react`).
1. Save: `renderHook(() => useColumnWidths("t", { a: { width: 100 }, b: { width: 120 } }))`. Then `act(() => result.current.onColumnResize(undefined, { columnId: "a", width: 300 }))`. Assert that `JSON.parse(localStorage.getItem("outlooker.columnWidths.t")!)` equals `{ a: 300 }`. **This fails if the width is not saved.**
2. Restore: unmount, then render a new hook with the same surface and defaults. Assert `columnSizingOptions.a.defaultWidth === 300` and `b.defaultWidth === 120`. **This fails if the width is not restored** (fresh component, same storage = restart).
3. Edge cases: corrupt storage (`"{"`) gives defaults with no throw. A stored key that is not in `defaults` stays in storage after another resize. `minWidth` defaults to 48.
4. End to end through Fluent: render `<DataGrid {...useColumnWidths("t2", …)}>` with storage seeded at `a=240`. Assert that the `a` header cell has inline `style.width === "240px"`. This proves that the S1/S2 spread wiring restores the width.
- Prove red first: run 1, 2 and 4 against a stub that ignores storage. They must fail before the real implementation turns them green.
- `src/test/setup.ts`: add `localStorage.clear()` to the existing `afterEach`, so stored widths do not leak between tests (WP-SHARED owns this line).
- S3 drag, keyboard and restore tests: grid plan §5.
- Gate: `mise exec -- npm test` and `mise exec -- npm run build` (contract criterion 5).

## 6. Work packages (exclusive file ownership, 4 implementers)

| WP | Owned files | Depends on |
|---|---|---|
| WP-SHARED + S1 | `src/components/common/columnWidths.ts` (new, both hooks), `src/components/common/columnWidths.test.tsx` (new), `src/components/common/index.ts` (re-export line), `src/test/setup.ts` (one line), `src/screens/settings/index.tsx` | none; lands first |
| WP-S2 | `src/screens/import/index.tsx` (+ `import.test.tsx` only if a check there is needed) | WP-SHARED signature (§2, frozen). Code against it in parallel; build is green once WP-SHARED lands |
| WP-S3 | grid plan §6 (`ResultTable.tsx`, `advanced-sql.test.tsx`) | WP-SHARED |
| WP-S4 | grid plan §6. Exists only if the user picks the MailList header option | WP-SHARED |

S1 is folded into WP-SHARED because it is a 3-line change and makes WP-SHARED's own end-to-end proof real. If the user rules MailList out (grid plan §4 option B), there are 3 implementers.

## 7. API agreement with architect-grid-opus

The signature in §2 is FINAL. The persist architect owns the shared API (lead ruling). architect-grid-opus confirmed it, quoted:

> "AGREED to your FINAL line, with no blocker. Your correction to my quirk 1 is right: existing columns keep their idealWidth when the options carry none (columnResizeUtils.js:20-30)."

Their earlier counter-proposal (`Record<string, number>` defaults, tuple return, options built from `idealWidth`) is superseded by the FINAL line.

Binding implementation notes from that confirmation (no signature change):
1. `useTableColumnSizing_unstable` reads `autoFitColumns` at the top level of its params (`lib/hooks/useTableColumnSizing.js:27`) and ignores `resizableColumnsOptions`. `useGridColumnSizing` therefore calls `useTableFeatures({ columns, items: [] }, [useTableColumnSizing_unstable({ ...sizing, autoFitColumns: false })])`, with `columns = useMemo(Object.keys(defaults).map(id => createTableColumn({ columnId: id })))`.
2. `useGridColumnSizing` takes `widths` and `total` from `columnSizing_unstable.getColumnWidths()`, not from storage, because Fluent's state is the live source during a drag. `headerProps(id) = { aside: getTableHeaderCellProps(id).aside, onKeyDown: Enter/Space → enableKeyboardMode(id)(e as never) }`.
3. S3 memoizes `defaults` on `columns`. Duplicate column names use the key `name#j`.
