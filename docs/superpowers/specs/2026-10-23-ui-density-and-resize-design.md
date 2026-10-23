# UI density and resizable layout

Date: 2026-10-23. Status: approved design, pending implementation plan.

## Goal

Every split between panes and every list column the user can see can be dragged and is remembered across restarts.
The reading-pane mail header takes one 49 px row (two rows on a narrow pane). Search hints take no row of their own.

## Scope

In scope:
1. Draggable, persisted pane splits on the Search, Advanced SQL, Import and Settings screens.
2. Draggable, persisted column widths in the search result list.
3. Reading-pane header redesign (approved mockup: `scripts/tmp/ui-redesign/header-b.html`).
4. Search hints moved next to the 條件 button (approved mockup: `scripts/tmp/ui-redesign/hint-c.html`).

Out of scope: onboarding screen, dark-theme tuning, new resize UI chrome (no grips, no icons), and changes to the existing DataGrid column resizing in Settings and Import.

## 1. Pane splits

### Behaviour
- A split boundary is an invisible 6 px hit area centred on the existing 1 px border.
- On hover the cursor becomes `col-resize` or `row-resize`. There is no other visual change.
- Drag with the pointer (pointer capture) to change the size. Double-click restores the default size.
- Keyboard: the boundary is a focusable `role="separator"` with `aria-orientation`, `aria-valuenow/min/max`. Arrow keys move it 16 px. Home and End jump to min and max.
- Each pane has a min size. The drag clamps so no pane goes below its min. The stored size is clamped again on load and on window resize, so a smaller window never hides a pane.
- The size persists in `localStorage` per surface key and is read once on mount.

### Unit
One generic module in `src/components/common/` (`splits.ts` + `Splitter.tsx`):
- `useSplit(surfaceKey, defaults)` returns the current sizes (px) and props for each boundary.
- `<Splitter>` renders the hit area and handles pointer and keyboard input.
- One sized pane per boundary is stored in px. Its neighbour takes `1fr`, so the layout fills the window.

The `localStorage` read, validation and write in `columnWidths.ts` (`readStored`, the write in `onColumnResize`) move into one shared helper that both `columnWidths.ts` and `splits.ts` use. The key prefixes stay separate (`outlooker.columnWidths.`, `outlooker.splits.`).

### Boundaries per screen

| Screen | Boundary | Sized pane, default |
|---|---|---|
| Search | refine pane \| main | refine pane width, 260 px |
| Search | condition-builder panel \| results (horizontal; only while the panel is open) | panel height, content height capped at 40 % of the main column |
| Search | mail list \| reading pane | list width, 34 % of the results area, min 420 px |
| Advanced SQL | each of the three column borders | the left column of each border; the current 1:3:1:3 ratio is the default |
| Advanced SQL | saved-query list \| schema (horizontal, in `SavedQueries.tsx`) | schema height, 42 % |
| Import | each of the two pane borders | the left pane of each border; the current 1.6:1:1 ratio is the default |
| Settings | nav \| body | nav width, 220 px |
| Settings | the two column borders of the card grid | the card-grid columns; equal thirds by default |
| Settings | the two row borders of the card grid (horizontal) | the height of each of the three card rows; content height by default |

Settings card rows:
- The grid has three rows: storage + db, language + content + db, and index. The db card spans rows 1 and 2.
- Row 1 | row 2 boundary: its hit area covers only the columns that row 1 and row 2 do not share, which are the storage card's columns. Dragging it resizes row 1. The db card stretches over both rows.
- Row 2 | row 3 boundary: its hit area spans the full width.
- Until the user drags a row, its height follows its content (`auto`). After a drag, the height is stored in px.
- A card whose content is taller than its row scrolls inside the card (`overflow: auto`). The settings body still scrolls as a whole when the rows are taller than the window.
- Double-click on a row boundary returns that row to content height.

The reading-pane header \| body split is removed. The new header has a fixed height (section 3).

## 2. Search result columns

`MailList.tsx` uses `useGridColumnSizing`. Today only the sender column has a resize handle. The change adds a handle on the subject column, so both the sender \| subject and the subject \| date boundaries can be dragged. The date column takes the remaining width (`1fr`) with a minimum width, so all three columns can be resized. The avatar column stays fixed at 24 px. Widths persist under the existing `mail-list` key.

## 3. Reading-pane header (from header-b.html)

### Layout
- One Fluent `Toolbar` row of 28 px tokens, in this order: subject (flexible, ellipsis), sender token (20 px avatar + name + address), 收件者 To, 副本 Cc, 密件副本 Bcc, 出現於 Found-in, 📎 attachments, then the date on the right.
- A recipient or location token shows its label, the first value and "+N". Cc, Bcc and 📎 tokens are not rendered when their lists are empty.
- Height: 49 px for one row. When the pane is narrower than 900 px (measured with a ResizeObserver or container query), the header is 81 px in two rows: row 1 is subject + date, and row 2 is the token toolbar. In the two-row layout the sender token shows only avatar + name, and the address moves into its popover.
- Overflow: Fluent `Overflow` + `OverflowItem priority` + `useOverflowMenu`. Tokens that do not fit move into a trailing "⋯ +N" token. They collapse in this order, first to last: Bcc (1), Found-in and 📎 (2), Cc (3), To (4), sender (5).

### Popovers
Each token opens a `Popover` on click, Enter or Space.
- Sender: full subject, name, address and full date.
- To, Cc, Bcc: a scrollable list of "Name address" rows and a "複製全部" button, which copies `Name <addr>, …`.
- Found-in: one row per occurrence, PST display name and folder path.
- 📎: compact rows with icon, name, size, and 開啟 / 另存 buttons. These replace the attachment cards. The actions are the same as `AttachmentCard`.
- "⋯ +N": the hidden token groups, rendered from data.

Nothing is dropped. Every value is reachable from a token popover. Tooltips are a convenience only.

### Accessibility
- Tab enters the toolbar once. The ←/→ arrow keys move between tokens, and Home/End jump to the first and last token.
- Esc closes a popover and returns focus to its token.
- Each token has an explicit `aria-label`, for example "收件者：陳愛麗 與另外 39 位", plus `aria-haspopup="dialog"` and `aria-expanded`.
- Counts are text, not color.

The `Persona`, the per-kind grid lines, the badge list and the attachment-card block are deleted from `ReadingPane.tsx`.

## 4. Search hints (from hint-c.html)

### Layout
- The `hints` stack is deleted.
- Status segments attach to the right of the 條件 `ToggleButton`. They share its outline, overlapping by −1 px, with outer corners rounded like a SplitButton.
- The button and the segments are wrapped in a `role="group"`.
- A segment exists only while its message is active. Segments are ordered by severity: error, then warning, then new mail.
- When an error and a warning are both active, only the error segment shows, and its popover lists both messages.

### The three messages

| Message | Segment | Style | Click | Search box | Announced as |
|---|---|---|---|---|---|
| Short-query warning | "短詞較慢", warning icon | light amber (`colorStatusWarning*` tokens) | Popover with the full `shortQueryHint` text | thin amber underline | `role="status"` |
| Query error `problem.*` | short bold label per code (≤ 6 zh characters / ≤ 14 en), error icon | light red (`colorStatusDanger*` tokens) | Popover with the full `t("problem.*")` text | red underline and `aria-invalid` | `role="alert"`, once on appearance |
| New mail | "已匯入新郵件 ｜ 重新整理", mail icon | brand tint | Refreshes the list in one click | — | `role="status"` |

- Each `problem.*` code gets a new short-label i18n key, `problem.short.*`, in both zh-TW and en.
- When the window is narrow, the warning segment shrinks to its icon, and the full text stays in a `Tooltip` and in the popover.
- Tab order: search box, 條件, then each segment. Esc closes a popover.

## Testing

Per the project test policy:
- **Committed test, split clamping and persistence:** extend `columnWidths.test.tsx` for the shared storage helper, and add one `useSplit` test. Malformed or out-of-range stored values clamp to min and max, and the sizes round-trip through storage.
- **Updated tests:** existing tests that select `short-query-hint`, `new-mail-bar` or `new-mail-refresh`, or the reading-pane header DOM, are updated to the new elements. Their behaviour is unchanged, and no new tests are added for it.
- **Verified by running, not committed:** layout, header token overflow and segment styling. Each is checked once in the running app (`npm run dev` / tauri dev), and the run notes go in `scripts/tmp/`.
- **Gates:** `npm run typecheck` and `npm test` pass.
