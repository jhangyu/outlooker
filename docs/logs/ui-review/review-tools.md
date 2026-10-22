# UI review — Advanced SQL, Import, Settings (vs. docs/design/variant-1)

Method: static comparison only. The app cannot render in a plain browser (every screen calls Tauri commands through `src/api/call`; there is no browser mock), so no app screenshots were taken.
Width math uses the real window limits: `src-tauri/tauri.conf.json:14` (default 1920x1080, minWidth 1280) and the 68 px rail (`src/app/AppFrame.tsx:24`). Content width = W − 68.
Fluent v9 facts used below were read from `node_modules/@fluentui/*` (react-components 9.74.10):
- Button: `minWidth: 96px` (medium), `64px` (small), and **no `white-space: nowrap`**, so its text wraps (`react-button/.../useButtonStyles.styles.raw.js:47,245`).
- ToolbarButton: size medium, appearance subtle by default (`react-toolbar/.../useToolbarButton.js:9-13`).
- Badge: fixed `height: 20px`, no nowrap (`react-badge/.../useBadgeStyles.styles.raw.js:19-21`). Wrapped badge text spills out of the badge.
- DataGrid uses flex layout. Every cell is `flex: 1 1 0px; min-width: 0` (`react-table/.../useTableCellStyles.styles.raw.js:26-29`), so all columns get the same width. TableCellLayout truncates only with the `truncate` prop.
- Slider `minWidth: 120px` (`react-slider/.../useSliderStyles.styles.raw.js:38`). The SpinButton input is `width: 100%` of an intrinsically sized root.
- `src/app/app.css` is one line (`html, body, #root { height: 100%; margin: 0; }`). No finding touches it.

Severity: blocker = visibly broken layout or wrapping; should-fix = deviation from the mockup; nit = small polish.

---

## Advanced SQL

### TL-1 — blocker — the mail-list rows in SQL column 3 overflow at every supported width (shared component)
- App: `src/components/mail/MailList.tsx:51-52` (row grid `24px minmax(120px, 22%) 1fr auto`), used by `src/screens/advanced-sql/index.tsx:227-228`.
- Mockup: `app.js:322` renders `mailItem(m, …, true)`, the compact variant. `styles.css:232-233` `.mail-item.compact { grid-template-columns: 1fr auto }`: line 1 is sender plus date, line 2 is subject, line 3 is snippet, and there is no avatar.
- Symptom: column 3 is 1/8 of the content width: 151 px at 1280, 231 px at 1920. A row needs 24 + 120 + 3×8 gaps + date ≈ 240 px plus 24 px padding, so the subject track collapses to 0. The date is pushed past the column edge, and the list (`overflowY: auto`, so overflow-x is auto too) shows a horizontal scrollbar. This happens in both languages, whenever a query returns an `id` column.
- Fix: add a `compact?: boolean` prop to `MailList`. When it is set:
  - Each row uses `gridTemplateColumns: "1fr auto"`, `gridTemplateRows: "20px 20px 16px"`, and padding `8px 8px 8px 12px` (mockup `.mail-item.compact`).
  - The avatar is dropped.
  - Sender (semibold, ellipsis) and date (12px, Fg3, nowrap) go on row 1. Subject (Brand fg2, ellipsis, `gridColumn: "1 / 3"`) goes on row 2. Snippet (12px, Fg3, ellipsis, `gridColumn: "1 / 3"`) goes on row 3.
  - The row height is a separate constant (≈ 72 px), used as `estimateSize` and as the absolute row `height`.
  - `index.tsx:227` passes `compact`.
  - **Shared file: `src/components/mail/MailList.tsx` belongs to the frame/search reviewer's scope.** The lead should assign it once.

### TL-2 — blocker — the editor toolbar overflows into column 3 below about 1380 px when a saved query is selected
- App: `src/screens/advanced-sql/index.tsx:279-290` (medium `ToolbarButton`s Run / Cancel / Save / Delete, then a Badge).
- Mockup: `index.html:125-134` uses `.btn.small` buttons (`styles.css:177`: 24 px high, 12 px font, padding 0 8) and `.toolbar` (`styles.css:308`: gap 4, padding 4 8, min-height 44). The badge is `.badge` with `white-space: nowrap` (`styles.css:185`).
- Symptom: column 2 is 3/8 of the content width, 454 px at 1280. Four medium buttons at their 96 px minimum, plus gaps, plus the Read-only badge (≈ 75 px), plus 16 px padding ≈ 490 px. The badge spills past the column border into column 3, and in zh-TW it can wrap "唯讀" inside the 20 px high badge. Without a selected saved query (no Delete button) the toolbar still fits.
- Fix:
  - Use `<Toolbar size="small" …>`, which makes the ToolbarButtons small: 24 px high, min-width 64, 12 px font. This matches the mockup.
  - Add `minWidth: "auto"` to the toolbar buttons through a class.
  - Give the badge a class with `flexShrink: 0, whiteSpace: "nowrap"`.
  - Optionally add `overflow: "hidden"` to the `toolbar` style (line 28) as a guard.

### TL-3 — blocker — the column 3 and column 4 headers wrap in English at 1280
- App: `src/screens/advanced-sql/index.tsx:35-38` (`head`), `:220-223` and `:237-240` (Subtitle2 title plus Caption1 "N rows").
- Mockup: `styles.css:162-164`. `.pane-head h3` is 14 px semibold, and `.count` is 12 px Fg3 showing **only the number** (`app.js:321,327`: `<span class="count">${rows}</span>`).
- Symptom: column 3 is 151 px at 1280. "Result table" at 16 px (≈ 92 px) + "10,000 rows" (≈ 65 px) + 8 px gap + 24 px padding ≈ 189 px, so the header wraps to two lines and grows past 44 px. "Mail list" is borderline.
- Fix:
  - Title: replace `<Subtitle2>` with a 14 px semibold text (`<Body1Strong>` or Text `weight="semibold" size={300}`). Add a class with `minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap"`.
  - Count: add a class with `flexShrink: 0, whiteSpace: "nowrap"`.
  - Show the count as the bare formatted number, as the mockup does. Or keep the `result.rows` string; nowrap with ellipsis on the title still prevents the wrap.

### TL-4 — blocker — column 1 grows wider than 1/8 and breaks the 1:3:1:3 grid
- App: `src/screens/advanced-sql/SavedQueries.tsx:10` (`root` has no `minWidth: 0` and no `overflow: hidden`). Driving content: `:18` and `:56-60` (schema rows are a flex row of two spans with no nowrap or ellipsis) and `:14`/`:42-46` (saved-query Buttons).
- Mockup: `styles.css:160` `.pane { min-width: 0; overflow: hidden }`, applied to `<aside class="pane">` at `index.html:116`. `styles.css:296` `1fr 3fr 1fr 3fr`.
- Symptom: a grid item without `min-width: 0` cannot shrink below its min-content width. With the schema accordion open, a row such as `received_at timestamptz` (≈ 22 mono chars ≈ 160 px) + 8 px gap + 12 px indent + panel padding is wider than 151 px. Column 1 then widens and squeezes columns 2–4. The same happens with a long unbreakable saved-query name. Seen at window widths below about 1600 px.
- Fix:
  - Line 10 `root`: add `minWidth: 0, overflow: "hidden"`.
  - Line 18 `col`: add `whiteSpace: "nowrap"`. Wrap the name span in a class with `overflow: "hidden", textOverflow: "ellipsis", minWidth: 0`, and give the type span `flexShrink: 0`.
  - Line 17 `schema`: `maxHeight: "42%"` (mockup `styles.css:303`).

### TL-5 — blocker — saved-query names wrap onto several lines instead of truncating
- App: `src/screens/advanced-sql/SavedQueries.tsx:14`, `:42-46`. These are Fluent subtle `Button`s, which have no nowrap.
- Mockup: `styles.css:297-301`. `.saved-item` is a full-width block with padding 8 12 and a bottom border Stroke3. `.t` has `overflow:hidden; text-overflow:ellipsis; white-space:nowrap`. The selected item has the Brand2 background plus a 3 px left brand bar.
- Symptom: in a 151 px column, a name such as "Client mail with attachments, 30 days" wraps to 3 lines, and rows get uneven heights. This happens in both languages.
- Fix:
  - Render the label as `<span className={s.itemText}>{q.name}</span>` with `itemText: { minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }`.
  - Line 14 `item`: add `minWidth: 0` (overrides the 96 px), `justifyContent: "flex-start"`, `borderRadius: 0`, padding `8px 12px`, `borderBottom: 1px solid Stroke3`.
  - `picked` (line 15): add `boxShadow: inset 3px 0 0 ${tokens.colorCompoundBrandStroke}`.
  - Line 13 `list`: padding 0 (the mockup list has no inset).
  - Add `title={q.name}` so the full name shows on hover.

### TL-6 — blocker — the "已儲存的查詢" header wraps at 1280 in zh-TW
- App: `src/screens/advanced-sql/SavedQueries.tsx:11`, `:34-37` (Subtitle2 16 px), plus the string `src/locales/zh-TW/sql.json:3` "已儲存的查詢".
- Mockup: `index.html:117` "已存查詢" at 14 px (`styles.css:163`).
- Symptom: 6 CJK characters × 16 px = 96 px, plus the 32 px icon button, plus 24 px padding = 152 px. Column 1 at 1280 is 151 px minus its border, so the title wraps to "已儲存的查 / 詢".
- Fix:
  - Title: 14 px semibold text (`Body1Strong`) with `minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap"`.
  - Head (line 11): add `gap: tokens.spacingHorizontalS`.
  - Optionally change the zh-TW string to the mockup's "已存查詢". The locale file is owned with this group.

### TL-7 — should-fix — column 3 has no header before a run and in command mode, so the header rules do not line up across columns
- App: `src/screens/advanced-sql/index.tsx:203-209` (EmptyState only) and `:266-270` (MessageBar only).
- Mockup: `app.js:311` always renders `<div class="pane-head"><h3>郵件清單</h3></div>` above the empty state.
- Symptom: columns 1 and 2 have a 44 px header line with a bottom border. Column 3 starts directly with the empty state or the MessageBar, so the border is missing and the MessageBar sits flush at the top.
- Fix: always render `<div className={s.head}>` in column 3. Use the title `t("mode.mailList")` before a run and `t("mode.command")` in command mode, then put the EmptyState or MessageBar below it. Give the command MessageBar a `margin: tokens.spacingHorizontalM`.

### TL-8 — should-fix — the status bar style differs from the mockup
- App: `src/screens/advanced-sql/index.tsx:31-34`, `:300`.
- Mockup: `styles.css:325` `.statusbar` has `align-items:center`, padding 4 12, `background: Bg2`, `color: Fg2`, 12 px.
- Symptom: no background tint and Fg3 color, so the bar does not read as a status bar. In en at narrow widths the idle text ("Read-only connection. Press Ctrl+Enter or ⌘+Enter to run.", ≈ 330 px) wraps to two lines when column 2 gets narrower than that.
- Fix:
  - `status`: add `alignItems: "center", backgroundColor: tokens.colorNeutralBackground2, color: tokens.colorNeutralForeground2, whiteSpace: "nowrap", overflow: "hidden"`.
  - Give the first span `overflow: "hidden", textOverflow: "ellipsis", minWidth: 0`.

### TL-9 — should-fix — the "N ids matched no message" caption touches the column edges
- App: `src/screens/advanced-sql/index.tsx:225`.
- Mockup: none. Every pane text in the mockup has 12 px horizontal padding (`styles.css:162`).
- Symptom: the text sits flush against the left border, directly under the header.
- Fix: a class with `padding: \`${tokens.spacingVerticalXS} ${tokens.spacingHorizontalM}\``, also applied to the `truncated` MessageBar (lines 212-216) as a margin.

### TL-10 — nit — the row-detail key/value list spacing differs
- App: `src/screens/advanced-sql/index.tsx:41-42`.
- Mockup: `styles.css:327-328` `.kv`: gap 4 16, padding 16 20, dt mono 12 px Fg3.
- Fix: `detail`: `gap: \`${tokens.spacingVerticalXS} ${tokens.spacingHorizontalL}\``, `padding: \`${tokens.spacingVerticalL} ${tokens.spacingHorizontalXL}\``. `key`: add `fontSize: tokens.fontSizeBase200`.

### TL-11 — nit — the read-only badge color
- App: `src/screens/advanced-sql/index.tsx:289` (`color="informative"`).
- Mockup: `index.html:133` `.badge.warn` (warning foreground, background and border).
- Fix: `color="warning"` with `appearance="tint"`. The nowrap part is covered by TL-2.

### TL-12 — nit — the right-most column has an extra border
- App: `src/screens/advanced-sql/index.tsx:27` (`borderRight` on every column, including column 4).
- Mockup: `styles.css:161` `.pane + .pane { border-left }`, so there is no outer border.
- Fix: `col: { …, ":last-child": { borderRight: "none" } }`, or switch to `borderLeft` and drop it on the first item.

### TL-13 — should-fix — the result table header and cells differ from the mockup grid
- App: `src/screens/advanced-sql/ResultTable.tsx:13-17` (header background Bg2, border Stroke2) and `:18-24` (cells inherit 14 px).
- Mockup: `styles.css:215-216` and `:419`. The header background is Bg1 with a `border-bottom: Stroke1`. In `#sqlCol3` the cells are 12 px with 8 px side padding. Header cells are `mono` (`app.js:328`).
- Fix:
  - `head`: `backgroundColor: tokens.colorNeutralBackground1`, `borderBottom: 1px solid ${tokens.colorNeutralStroke1}`, `fontFamily: tokens.fontFamilyMonospace`.
  - `row`: add `fontSize: tokens.fontSizeBase200`.

### TL-14 — should-fix — the saved-queries pane background
- App: `src/screens/advanced-sql/SavedQueries.tsx:10` (`backgroundColor: colorNeutralBackground2`).
- Mockup: `styles.css:160` `.pane` is Bg1 (white).
- Fix: drop `backgroundColor` or use `tokens.colorNeutralBackground1`.

---

## Import

### TL-15 — blocker — the imported-PST DataGrid squeezes 7 equal columns into the pane, so text overlaps and wraps
- App: `src/screens/import/index.tsx:100-170` (column definitions), `:211-227` (DataGrid), `:47` (`path` has `maxWidth: 280px`), `:106-111` (TableCellLayout without `truncate`).
- Mockup: `app.js:347-350` renders a native `table.grid`. `styles.css:214-216`: cells are `white-space: nowrap` and columns auto-size to content. The name cell has `max-width:280px` and the path is a block with an ellipsis. Size, Messages and Unique use `.num` (right aligned, tabular, `styles.css:220`). The table sits in a `.scroll` that scrolls horizontally.
- Symptom: pane 1 is 1.6/3.6 of the content width, 539 px at 1280. Every Fluent flex-layout cell is `flex: 1 1 0`, so each of the 7 columns gets about 77 px, including the icon-only Remove column. Effects, in both languages:
  - The path span has a 280 px min-content (`maxWidth: 280px` + nowrap), so it overflows the 77 px name cell and draws over the Size and Messages cells.
  - The date `formatDateTime(..., dateStyle medium, timeStyle short)` ("Oct 22, 2026, 2:30 PM" / "2026年10月22日 下午2:30") wraps to 2–3 lines.
  - The status badge "Completed" plus the warning icon does not fit, and the badge text wraps inside its 20 px height.
- Fix:
  - Give the columns fixed flex bases. Add a map, for example `const COL = { name: "1 0 220px", size: "0 0 88px", messages: "0 0 88px", new: "0 0 72px", date: "0 0 176px", status: "0 0 120px", remove: "0 0 44px" }`.
  - Apply it in both render functions: `<DataGridHeaderCell style={{ flex: COL[columnId] }}>` and `<DataGridCell style={{ flex: COL[columnId] }}>` (the render callback receives the column, `({ renderCell, columnId })`). Or use the equivalent makeStyles classes.
  - Give the DataGrid `style={{ minWidth: 808 }}` so the existing `s.scroll` (line 45, `overflow: auto`) scrolls horizontally instead of squeezing.
  - Name cell: `<TableCellLayout truncate media={<DocumentRegular />} description={<span className={s.path} title={p.path}>{p.path}</span>}>{p.displayName}</TableCellLayout>`. Change `path` to `{ fontFamily: mono, fontSize: tokens.fontSizeBase200, color: Fg3, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", display: "block", minWidth: 0 }` and remove `maxWidth: 280px`.
  - Add a cell class with `whiteSpace: "nowrap"` for all cells and header cells.
  - Size, messages and new cells: `justifyContent: "flex-end", fontVariantNumeric: "tabular-nums"` (mockup `.num`).
  - Status Badge: `flexShrink: 0, whiteSpace: "nowrap"`.

### TL-16 — blocker — the queue card progress line wraps while a file is importing
- App: `src/screens/import/index.tsx:55` (`sub`: flex with no wrap and no nowrap on children), `:297-314`.
- Mockup: `app.js:376-378`. Line A is phase · size on the left, a spacer, then "done / total · pct%" on the right. Line B is rate, ETA and error count. `styles.css:345` `.q-item .sub`.
- Symptom: the queue pane is 1/3.6 of the content width, 337 px at 1280, minus 24 px padding. One running item puts "12,345 / 64,000 · 19%" (≈ 120 px), "1.6 MB/s", "About 3 min left" and "⚠ 3 errors" on one line with 12 px gaps, ≈ 360 px. The spans shrink and wrap internally ("About 3 / min left"). This happens in en and in zh-TW ("剩餘約 3 / 分鐘").
- Fix: split into the mockup's two lines.
  - Line A (`s.sub`): `<Caption1>{t(\`phase.${p.phase}\`)}</Caption1>` (moved from line 297), a `<span className={s.spacer}/>`, then the progress text.
  - Then the ProgressBar.
  - Line B: rate, ETA and errors.
  - Add `flexWrap: "wrap", rowGap: "2px"` to `sub`, and `whiteSpace: "nowrap"` to every child span (for example `sub: { …, "& > span": { whiteSpace: "nowrap" } }`).

### TL-17 — should-fix — the warning icon in the queue card is misaligned with its text
- App: `src/screens/import/index.tsx:310-312` (`<span className={s.warn}><WarningRegular /> text</span>`, an inline span).
- Mockup: `app.js:378` (icon plus count). `svg.ic` is `flex:none` inside the flex `.sub` (`styles.css:119,345`).
- Symptom: the icon sits on the text baseline, about 3 px too low, and the separator is a literal space.
- Fix: change `warn` usage here to a class with `display: "inline-flex", alignItems: "center", gap: tokens.spacingHorizontalXXS` and drop the space character.

### TL-18 — should-fix — the error-log time column is `auto` per row, so columns do not line up between entries
- App: `src/screens/import/index.tsx:58-61` (`gridTemplateColumns: "auto 20px 1fr"`), `:366` (Caption1 full date and time).
- Mockup: `styles.css:352` `.log-entry { grid-template-columns: 64px 20px 1fr }`. The time is `mono muted` (`app.js:383`).
- Symptom: each entry is its own grid, so the icon and message start at a different x whenever the formatted time width changes ("上午9:05" vs "下午12:30", proportional digits). The message column looks ragged.
- Fix:
  - `gridTemplateColumns: "136px 20px 1fr"` (fits "2026/10/22 下午12:30" at 12 px mono). Size it from the longest zh-TW `formatDateTime` output if you change the format.
  - Give the time Caption1 a class with `fontFamily: tokens.fontFamilyMonospace, color: tokens.colorNeutralForeground3, whiteSpace: "nowrap"`.

### TL-19 — should-fix — long folder paths in the error log widen the grid track
- App: `src/screens/import/index.tsx:62` (`logLoc`), `:368-373`.
- Mockup: `styles.css:356` `.log-entry .loc { font-family: mono; word-break: break-all }`.
- Symptom: a folder path has no break opportunity, so the `1fr` track grows to its min-content. The entry then overflows the pane and is clipped, because the pane has `overflow: hidden`.
- Fix: `logLoc`: add `fontFamily: tokens.fontFamilyMonospace, wordBreak: "break-all"`. Give the message `Body1` a class with `minWidth: 0, overflowWrap: "anywhere"`.

### TL-20 — should-fix — the error-log header PST name does not truncate
- App: `src/screens/import/index.tsx:356` (`<Caption1>— {pst.displayName}</Caption1>`).
- Mockup: `index.html:163` `.count` in `.pane-head` (12 px Fg3).
- Symptom: a long file name (no spaces) cannot wrap and is clipped by the pane with no ellipsis, or it pushes the title.
- Fix: give the Caption1 a class with `minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", color: tokens.colorNeutralForeground3` and `title={pst.displayName}`. Give the title `flexShrink: 0`.

### TL-21 — should-fix — the Import command bar title and stats differ from the mockup
- App: `src/screens/import/index.tsx:182` (`Title3`, 20 px), `:35-36` and `:172-177` (stats right-aligned, `paddingLeft` 16, no divider, label in Fg1).
- Mockup: `styles.css:335`. `.cmdbar h2` is 16 px semibold with `margin-right: 12px`. `styles.css:336-338` `.stat`: column, `padding: 0 12px`, `border-left: 1px solid Stroke2`, value 16 px tabular, label 12 px Fg3, left aligned.
- Fix:
  - Title: `<Subtitle2 as="h2">` (16/22 semibold) with `marginRight: tokens.spacingHorizontalM`.
  - `stat`: `{ display: "flex", flexDirection: "column", padding: \`0 ${tokens.spacingHorizontalM}\`, borderLeft: \`1px solid ${tokens.colorNeutralStroke2}\` }` (remove `alignItems: "flex-end"`).
  - `statValue`: add `fontVariantNumeric: "tabular-nums"`.
  - Label Caption1: `color: tokens.colorNeutralForeground3`.

### TL-22 — should-fix — the Import pane headers use 16 px instead of 14 px
- App: `src/screens/import/index.tsx:208`, `:232`, `:355` (`Subtitle2`).
- Mockup: `styles.css:163` (`.pane-head h3` 14 px semibold).
- Fix: replace with `Body1Strong as="h3"` (14/20 semibold). Optional nit: add the mockup `.count` (`index.html:155,159`) as a muted Caption1 with the item count.

### TL-23 — should-fix — the selected imported-PST row has no highlight
- App: `src/screens/import/index.tsx:217-224` (the row `onClick` sets `selected`, but there is no visual state).
- Mockup: `styles.css:219` `table.grid tbody tr[aria-selected="true"] { background: Brand2 }`.
- Symptom: clicking a row changes the error log on the right, but nothing in the table shows which row is active.
- Fix: `className={item.id === selected ? s.rowSelected : undefined}` and `aria-selected={item.id === selected}` with `rowSelected: { backgroundColor: tokens.colorBrandBackground2, ":hover": { backgroundColor: tokens.colorBrandBackground2 } }`.

### TL-24 — nit — the first Import pane has a double left border
- App: `src/screens/import/index.tsx:39` (`borderLeft` on every pane). The rail already has a right border (`AppFrame`).
- Mockup: `styles.css:161` (`.pane + .pane` only).
- Fix: `pane: { …, ":first-of-type": { borderLeft: "none" } }`, or add the border only to panes 2 and 3.

### TL-25 — nit — the remove button is not danger-colored and the queue action buttons have no icons
- App: `src/screens/import/index.tsx:157-167`, `:317-334`.
- Mockup: `app.js:350` (`btn small subtle danger`), `app.js:356-363` (icons play / x / trash on the action buttons), `styles.css:346` `.acts` gap 4 px.
- Fix: give the remove `Button` a class with `color: tokens.colorStatusDangerForeground1`. Add `icon={<DismissRegular/>}` to Cancel and `icon={<PlayRegular/>}` to Resume and Retry. Use an actions row gap of `tokens.spacingHorizontalXS`.

### TL-26 — nit — the Remove dialog width and long titles
- App: `src/screens/import/RemoveDialog.tsx:35-37` (Fluent default `maxWidth: 600px`).
- Mockup: `styles.css:413` `dialog { width: 520px }`.
- Fix: `<DialogSurface style={{ maxWidth: 520 }}>`, or a class. Give `DialogTitle` `overflowWrap: "anywhere"` so a long unbroken PST name wraps instead of overflowing.

---

## Settings

### TL-27 — should-fix — the card placement does not follow the mockup grid areas and leaves an empty cell
- App: `src/screens/settings/index.tsx:34-40` (`repeat(3, 1fr)`, `wide2` / `wide3`), and the call sites `:141-156` (storage, `s.wide2`), `:157`, `:168`, `:202`, `:226-251` (`s.wide3`).
- Mockup: `styles.css:365-367`, `grid-template-areas: "storage storage db" "lang webp db" "index index index"`. The DB card spans two rows on the right.
- Symptom: auto placement gives row 1 = storage (2 columns) + language, and row 2 = content + db + an **empty third cell**. The DB card does not span two rows, and the three cards on rows 1–2 have mismatched heights.
- Fix:
  - `body`: `gridTemplateColumns: "repeat(3, minmax(0, 1fr))"`, `gridTemplateAreas: '"storage storage db" "language content db" "index index index"'`.
  - Replace `wide2` / `wide3` with area classes: `storage: { gridArea: "storage" }`, `language: { gridArea: "language" }`, `content: { gridArea: "content" }`, `db: { gridArea: "db" }`, `index: { gridArea: "index" }`. Have `card()` (line 96) pick the class from `id`, and remove the `wide` parameter.
  - `card` (line 38): add `minWidth: 0` (mockup `.card { min-width: 0 }`, `styles.css:202`).

### TL-28 — blocker — the WebP SpinButton wraps under the slider at 1280
- App: `src/screens/settings/index.tsx:173-188` (`s.row` has `flexWrap: "wrap"`, line 42), with Slider `className={s.grow}` and a SpinButton with no width.
- Mockup: `index.html:199-203`. The range input is full width, and the value is shown in the label (`：80`).
- Symptom: at 1280 the content card is ≈ 309 px wide (≈ 277 px inside the padding). Slider (min 120 px) + 8 px gap + SpinButton (intrinsic input width ≈ 150+ px) is more than 277 px, so the SpinButton drops to a second line. This happens in both languages.
- Fix: give the SpinButton a class with `width: "88px", flexShrink: 0`, and give the slider row its own class with `flexWrap: "nowrap"` instead of `s.row`.

### TL-29 — should-fix — the "Smaller files / Sharper images" captions do not line up with the slider ends
- App: `src/screens/settings/index.tsx:48`, `:190-193` (`ends` spans the full card width, but the slider is shortened by the SpinButton).
- Mockup: `index.html:201-202` (the captions sit directly under the full-width slider).
- Symptom: "畫質較佳 / Sharper images" is right-aligned under the SpinButton instead of under the slider end.
- Fix: wrap the Slider and the `ends` row in `<div className={s.sliderCol}>` with `sliderCol: { flex: 1, minWidth: 0, display: "flex", flexDirection: "column" }`, placed inside the slider row next to the SpinButton. Then the captions span exactly the slider width.

### TL-30 — nit — the settings title is not indented like the mockup
- App: `src/screens/settings/index.tsx:30-33` (`gap: spacingVerticalS`), `:131`.
- Mockup: `styles.css:361-363`. The `h2` has `margin: 0 0 8px 8px`, the links have padding 6 12, and the nav gap is 2 px.
- Symptom: the title starts about 12 px left of the tab labels, while in the mockup they are about 4 px apart.
- Fix: give Title3 a class with `marginLeft: tokens.spacingHorizontalS`. Set the nav `gap: tokens.spacingVerticalXXS` and give the TabList `marginTop: tokens.spacingVerticalS`.

### TL-31 — nit — the empty-database art inside the DB card
- App: `src/screens/settings/index.tsx:218-220` (full `EmptyState`: 72 px art, 24 px padding, inside a card).
- Mockup: `app.js:432-434`. In the empty state the facts list just shows zeros, with no art.
- Fix: replace it with `<Caption1 className={s.muted}>{t("db.emptyBody")}</Caption1>`.

### TL-32 — nit — the index grid size column is not right-aligned, and the state badge can wrap
- App: `src/screens/settings/index.tsx:116`, `:120-124`.
- Mockup: `app.js:435-436` (`num` class on size, `styles.css:220`). The badge has nowrap (`styles.css:185`).
- Fix: size cell content in a span with `marginLeft: "auto", fontVariantNumeric: "tabular-nums"` (or a cell class with `justifyContent: "flex-end"`). Badge class with `whiteSpace: "nowrap"`.

### TL-33 — should-fix — the Move dialog content has no vertical rhythm, and long paths overflow
- App: `src/screens/settings/MoveDialog.tsx:65-86`. `DialogContent` is not a flex container (its Fluent styles set only padding and overflow). `Body1` is an inline span, followed directly by the ProgressBar, Caption1 and MessageBar with 0 spacing. `{{target}}` is an unbroken path inside `move.body` (`src/locales/*/settings.json` `move.body`).
- Mockup: `styles.css:413-416` (dialog padding 24, h2 margin-bottom 8, actions margin-top 20). The body paragraphs have default margins.
- Symptom: the ProgressBar touches the phase text, and the error MessageBar touches the body. A deep Windows path makes the dialog content scroll horizontally.
- Fix:
  - `<DialogContent className={s.content}>` with `content: { display: "flex", flexDirection: "column", gap: tokens.spacingVerticalS }`.
  - Give the body `Body1` `overflowWrap: "anywhere"`.
  - Optional nit: `DialogSurface` `maxWidth: 520px` to match the mockup. The same `maxWidth` nit applies to `SaveDialog` and `DeleteDialog` at `src/screens/advanced-sql/index.tsx:62`, `:84`.

---

## Counts
- blocker: 9 (TL-1, TL-2, TL-3, TL-4, TL-5, TL-6, TL-15, TL-16, TL-28)
- should-fix: 15 (TL-7, TL-8, TL-9, TL-13, TL-14, TL-17, TL-18, TL-19, TL-20, TL-21, TL-22, TL-23, TL-27, TL-29, TL-33)
- nit: 9 (TL-10, TL-11, TL-12, TL-24, TL-25, TL-26, TL-30, TL-31, TL-32)

## Proposed file ownership groups (no file appears in two groups)
- **Group SQL-main** — `src/screens/advanced-sql/index.tsx`: TL-2, TL-3, TL-7, TL-8, TL-9, TL-10, TL-11, TL-12, plus the dialog-width nit in TL-33 for SaveDialog/DeleteDialog. After Group Shared-mail lands, it also passes `compact` at line 227 (the TL-1 caller side).
- **Group SQL-side** — `src/screens/advanced-sql/SavedQueries.tsx`, `src/screens/advanced-sql/ResultTable.tsx`, `src/locales/zh-TW/sql.json` (optional string change): TL-4, TL-5, TL-6, TL-13, TL-14. Can be merged with Group SQL-main into one implementer.
- **Group Import** — `src/screens/import/index.tsx`, `src/screens/import/RemoveDialog.tsx`: TL-15, TL-16, TL-17, TL-18, TL-19, TL-20, TL-21, TL-22, TL-23, TL-24, TL-25, TL-26.
- **Group Settings** — `src/screens/settings/index.tsx`, `src/screens/settings/MoveDialog.tsx`: TL-27, TL-28, TL-29, TL-30, TL-31, TL-32, TL-33 (MoveDialog part).
- **Group Shared-mail (cross-reviewer)** — `src/components/mail/MailList.tsx`: TL-1 (the `compact` prop). This file is in the frame/search reviewer's scope. The lead should give it to whoever owns MailList, and SQL-main adds the one-line caller change after it lands.
- `src/app/app.css`: no findings.
