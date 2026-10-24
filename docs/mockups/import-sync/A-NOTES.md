# Import + PST sync — direction A (evolutionary)

Both keep the current shell (brand title bar, nav rail), the 48 px command bar with stats, 44 px pane heads, 32 px controls and the Fluent 2 webLight tokens from `docs/mockups/variant-1`. Open the HTML directly. The title bar switch toggles "已登錄路徑" (sample data) vs "空狀態" (empty).

## A1 — side-panel schedule
Concept: the current 3-pane page becomes 2 panes. Left: one wide sortable grid of registered paths (one row per path). Right (360 px, docked): a "同步排程" card that is always visible, and a "目前活動" card below it. You see the schedule, the per-path state and the run progress together, with no extra clicks.
- Schedule: radio group 關閉 / 固定間隔 (SpinButton + 分鐘/小時 dropdown) / 每日固定時間 (time + 7 round weekday chips). A pane-head badge and a "下次同步" line follow the edits. Info note: runs only while the app is open, a missed run catches up once on start, an unchanged file is skipped. Save and revert buttons.
- States: the status column has a badge and a caption line. 已是最新 (green), 等待中 (neutral outline + why it waits), 同步中 62% (brand badge + ProgressBar + phase counts), 已鎖定 (warning + "等 Outlook 關閉後，下次排程會自動重試"), 失敗 (danger + reason), 已停用 (grey badge, dimmed row, switch off, next = 不排程).
- Per-row: an enable Switch, plus icon buttons for sync now / remove. Sync now is disabled while that path syncs. Global "全部立即同步" is in the command bar.
- Activity card: run summary (2 / 5 paths, 1 skipped as unchanged, +1,422 new) and the current item (3-phase bar, rate, ETA, cancel). It also lists the queued and locked paths of this run.

## A2 — command-bar schedule popover, rich rows + detail
Concept: the schedule is set once and then stays out of the way. It sits in the command bar as a "排程：每 30 分鐘 ⌄" button that opens a Popover. The page puts state first: a full-width run strip under the command bar, a path list grouped by attention, and a detail pane for the selected path.
- Schedule popover: a segmented tab control (關閉 / 固定間隔 / 每日). Interval mode has a SpinButton and a unit. Daily mode has a time field, a joined 7-day toggle bar and 平日 / 每天 presets. A preview line shows the next runs. The same info note, then 取消 / 儲存. The button label follows the chosen mode.
- States: rows are grouped 需要注意 (locked, failed) → 進行中 (syncing, queued) → 已是最新 → 已停用. Locked and failed rows show an inline warning or error bar with the reason and a 立即重試 button. A syncing row has an inline ProgressBar. A queued row shows its place in line. A disabled row is dimmed, its next sync is 不排程, and it offers 同步一次.
- Each row shows: name, full path (mono), size, messages, 上次新增, 上次同步, 下次同步, a Switch, and remove. The detail pane shows the metadata, sync history (trigger: 排程 / 手動 / 啟動時補跑, results such as 略過 unchanged or 已鎖定) and the error log. The error log is the current page's third pane, moved here.
- Activity: a brand run strip with overall progress, the current file and phase, rate, ETA, new mail this run and 取消本輪. In the empty state it becomes a grey idle strip.

## Fluent UI React v9 mapping
| Element | Component |
|---|---|
| Command bar | `Toolbar`, `Button appearance="primary"`, `ToolbarDivider`; A2 schedule trigger = `Popover` + `PopoverTrigger` + `PopoverSurface` (or `MenuButton`-styled `Button` with `ChevronDownRegular`) |
| A1 path grid | `DataGrid` (sortable) + `TableCellLayout` (media icon, mono description) |
| A2 path list | `List`/`ListItem` (`@fluentui/react-list`) with sticky group headers, or `DataGrid` with custom row render |
| Status | `Badge appearance="tint"` color success/brand/informative/warning/danger/subtle; `Spinner size="extra-tiny"` or a rotating `ArrowSyncRegular` |
| Inline lock / error | `MessageBar intent="warning"/"error"` + `MessageBarActions` |
| Progress | `ProgressBar` (value or indeterminate), `Field` with validationMessage for the phase caption |
| Enable | `Switch` |
| Schedule mode | A1 `RadioGroup`/`Radio`; A2 `TabList appearance="subtle-circular"` or a `ToggleButton` group |
| Interval | `SpinButton` + `Dropdown`/`Option` (分鐘/小時) |
| Daily time | v9 has no TimePicker in the core package; use `@fluentui/react-timepicker-compat` `TimePicker`, or an `Input type="time"` |
| Weekdays | `ToggleButton` ×7 (A1 `shape="circular"`, A2 square in a joined group) |
| Info note | `MessageBar intent="info"` (A1 card body / A2 popover) |
| A1 side cards | `Card` + `CardHeader` |
| Remove | existing `RemoveDialog` (`Dialog`) unchanged |
| Empty | existing `EmptyState` |
