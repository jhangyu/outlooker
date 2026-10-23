# Column inventory (read-only preflight, main @ c1965dd)

All claims verified by reading the cited lines. `grep` for `<table|<th|colgroup` over src/ found **no native tables**; no `grid-template-columns` header rows other than those below.

## 1. Surfaces that render resizable-candidate columns (4)

### S1. Settings → index-maintenance DataGrid
- `src/screens/settings/index.tsx:241-259` — Fluent `DataGrid` + `DataGridHeader/Body`; columns defined `:125-139` via `createTableColumn` (ids `name`, `table`, `size`, `state`).
- Widths today: none set; Fluent default equal-share auto layout. Only `size` right-aligned (`s.num`, `:58`). No `resizableColumns` prop.
- Static columns (4), data-driven rows (`db.data?.indexes`). Not virtualized. Not sortable.
- Tests: `src/screens/settings/settings.test.tsx` (no column assertions found for this grid via grep of `DataGrid`).

### S2. Import → "imported" PST DataGrid
- `src/screens/import/index.tsx:260-290` — Fluent `DataGrid`, `sortable`; columns `:150-~205` via `column()` helper (`:30-31`); ids `ColumnKey` = name,size,messages,new,date,status,remove (`:28`).
- Widths today: per-column CSS classes with flex basis `:68-74` (name `flex: 1 0 220px`; size/messages 88px; new 72px; date 192px; status 128px; remove 44px), mapped `:213-217` (`colClass`). Grid `minWidth: 832px` (`:63`). No `resizableColumns`.
- Static columns (7). Not virtualized (plain DataGrid). Contains a non-data `remove` button column (44px) and `name` column using `TableCellLayout` with path description.
- Tests: `src/screens/import/import.test.tsx`.

### S3. Advanced SQL → ResultTable (query result grid)
- `src/screens/advanced-sql/ResultTable.tsx:30-77`; mounted `src/screens/advanced-sql/index.tsx:268` (mode `"table"`, `:261-269`).
- Hand-rolled ARIA table (`role="table"/"row"/"columnheader"/"cell"`) using CSS grid; header row `:43-49` sticky; body rows absolutely positioned and virtualized with `@tanstack/react-virtual` `useVirtualizer` (`:39`, rows `:51-72`).
- Widths today: `gridTemplateColumns: repeat(N, minmax(120px, 1fr))` + `minWidth: N*120px` (`:40`), identical `grid` style applied to the header (`:43`) and every row (`:57`), and spacer (`:50`).
- **Dynamic columns**: `columns: ColumnMeta[]` come from the query result (`index.tsx:268`; `ColumnMeta` has `name`, `pgType` per `:45`). Column set changes per query → persistence key must tolerate arbitrary names (and duplicate names; header keyed by index `:45`).
- Tests: `src/screens/advanced-sql/advanced-sql.test.tsx` (`sql-table-row` testid at `:75,:83`).

### S4. MailList (search results + Advanced SQL "mailList" mode) — row "columns", NO header
- `src/components/mail/MailList.tsx:57-67` (`row`/`compact` styles), render `:103-186`.
- Non-compact row: CSS grid `24px minmax(120px, 22%) 1fr auto` (`:58`) = avatar | sender | subject+snippet | meta(attachment, count badge, day). Compact row (`compact` prop): `1fr auto` grid, 3 text lines (`:64-66`, `:149-157`), no separable columns.
- **No column header exists** (no header row component; month headers `:46-49` are group dividers). Virtualized (`useVirtualizer` `:103`).
- Users: `src/screens/search/index.tsx:173` (non-compact, search results; also pane split `:41` `minmax(420px, 34%) 1fr`) and `src/screens/advanced-sql/index.tsx:252-253` (`compact`, so no columns to resize).
- Tests: `src/components/mail/mail.test.tsx`, `pages.test.tsx`, `search.test.tsx:95`, `advanced-sql.test.tsx:66`.
- Design decision needed: resizing here requires adding a header row (new UI) or treating it as not a column surface. Contract says "column-header list", so lead/architects must rule.

## 2. Grids that are layout, not data columns (not surfaces; listed so nobody re-greps)
`AppFrame.tsx:24`, `settings/index.tsx:29,38,56`, `search/index.tsx:24,41`, `advanced-sql/index.tsx:26,53`, `import/index.tsx:34,104`, `onboarding/index.tsx:21,30,42,65`, `ReadingPane.tsx:24` (label|value, 48px), `DslBuilder.tsx:44` (150px 92px 1fr 28px form row: field/op/value/remove; form controls, not result columns). `settings facts` / `advanced-sql :53` / `import :104` are `<dl>` key-value grids. Pane splitters (screen-level column widths) are also layout, out of "table columns" scope unless lead says otherwise.

## 3. Persistence mechanisms today
- **No `localStorage`/`sessionStorage` anywhere in src/** (grep empty).
- Backend settings via Tauri commands: `src/api/commands.ts:44-45` `settingsGet` / `settingsUpdate(patch)`; types `src/api/bindings/Settings.ts` = `{language, webpQuality, dataDir}`, `SettingsPatch.ts` = `{language|null, webpQuality|null}` (ts-rs generated from `src-tauri/crates/outlooker-core/src/dto/settings.rs:7-21`). Consumers: `settings/index.tsx:71,87` (react-query key `["settings"]`), `components/common/hooks.tsx:30` (language).
- Storage backend: `webp_quality` in Postgres `settings` table (`outlooker-core/src/settings/mod.rs:73,94`); language + data_dir in bootstrap JSON `<app_local_data_dir>/settings.json` with atomic write (`settings/mod.rs:11-28`, struct `Bootstrap` `:12-15`).
- Theme: hardcoded `webLightTheme` (`src/app/App.tsx:78`), not persisted. Language is the only UI pref persisted.
- Capabilities `src-tauri/capabilities/default.json`: `core:default, dialog:allow-open, log:default`. No store/fs plugin. `package.json:27-28` has only `@tauri-apps/plugin-dialog`, `plugin-log`; `src-tauri/Cargo.toml:20-23` plugins: dialog, log, single-instance, wdio-webdriver(optional).
- Implication (unverified design claim): frontend-only persistence = webview `localStorage` (WebView data dir survives restart in Tauri; **not verified here, including whether data dir persists across e2e/dmg runs**); backend persistence would require changing `Settings`/`SettingsPatch` + bootstrap JSON + regenerated bindings, which the contract puts out of scope unless proven necessary.

## 4. Libraries (package.json / installed node_modules)
- `@fluentui/react-components` 9.74.10 (`package.json:22`); installed `@fluentui/react-table` 9.19.23 (`node_modules/@fluentui/react-table/package.json:3`). Its `DataGrid` supports `resizableColumns`, `columnSizingOptions` (per-column `defaultWidth/minWidth/idealWidth`), and `onColumnResize(e, {columnId, width})` (`dist/index.d.ts:151,202,213,217`). Covers S1 and S2 natively; persistence must be wired via `columnSizingOptions` + `onColumnResize`.
- `@tanstack/react-virtual` 3.14.21 (`package.json:25`, installed same). Virtualization only; no column sizing.
- `@tanstack/react-query` 5.104.5. No `@tanstack/react-table`, no react-resizable, no zustand/redux (grep of package.json).
- `ResultTable` (S3) is hand-rolled, so Fluent resize hooks do not apply directly (`useTableColumnSizing_unstable` exists in react-table but not evaluated here).

## 5. `techdebt/integration` overlap
`git diff main...techdebt/integration -- MailList.tsx search/index.tsx` (HEAD of branch bf65fcd, 2 files, +21/-30):
- `MailList.tsx`: adds `useLayoutEffect` import, `lastTop` ref + layout effect restoring `scrollTop` (hidden `<Activity>` fix), `onScroll` handler on the `role="listbox"` root `:129`, and a comment edit. **No change to row/grid/columns/header layout.** Textual conflict risk is limited to the root `<div ...>` line (`:129` on main) and the `useRef` block near `:87-90`; both touch lines a resize change to MailList would likely also touch if it edits the root div.
- `search/index.tsx`: removes `onEvent("library:changed")` subscription and `newMail` state, adds `baseline`/`keepPreviousData`/`staleTime`; **no change to layout, grid styles (`:24,:41`) or the `<MailList …>` call at `:173`**.
- Conclusion: no column/header layout overlap; only minor merge-conflict risk in MailList root element.

## Surface count
4 candidate surfaces: S1 settings DataGrid, S2 import DataGrid, S3 SQL ResultTable, S4 MailList (header-less; needs lead ruling).
