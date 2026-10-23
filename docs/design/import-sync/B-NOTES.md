# Import page, direction B ("Sync sources" hub)

Both mockups turn the page from an import queue into a list of registered PST sources that sync on one global schedule. The shell (brand title bar, left nav rail, Fluent 2 webLight tokens) is copied from `docs/design/variant-1/`. Open the HTML directly. The title bar switch "範例資料 / 空狀態" shows the empty state.

## B1: source cards
- Layout: a hero header (page title, overall status badges, global "排程 / 加入 PST 路徑… / 全部立即同步"), a schedule summary strip (mode, last run, next run, new mail, the "only while the app is open, catch-up on start" line), a "目前活動" activity card, then one card per PST. The schedule editor is an inline end drawer that the "排程" toggle button opens and closes. It is open by default.
- Card: a colored state band at the top (icon, state text, time), name, enabled Switch, full path in mono, a 3×2 stats grid (size, messages, new last sync, last sync, next sync, plus one state-specific fact), and a footer with sync now, open folder and remove.
- Best for: a few sources (1–8). You see every state at a glance.

## B2: source list with a detail pane (master–detail)
- Layout: the left pane (320 px) has "全部立即同步 / 加入 PST 路徑…", a pinned "同步排程" item, then the source list (state icon, name, state + short reason, message count, mini progress bar while syncing). The right pane shows the selected source, or a settings-style schedule page when "同步排程" is selected. A full-width status bar at the bottom shows current import activity on every view.
- Source detail: header (name, state Badge, mono path + open folder, "納入排程" Switch, sync now/retry, remove), a state MessageBar or live progress, 5 stat tiles (size, messages, new last sync, last sync, next sync), and tabs for "同步紀錄" (per-run history table: result, new, duration, trigger: schedule / manual / catch-up) and "錯誤紀錄".
- Best for: many sources, and seeing why a source keeps getting skipped (the history shows repeated locked runs).

## How each state is shown (both)
| State | B1 card band / B2 list icon + detail | Action |
|---|---|---|
| Up to date | green check, "已是最新", time of the last sync | 立即同步 |
| Queued | neutral hourglass, "等待中 · 佇列第 1 位", indeterminate gray bar | 移出佇列 |
| Syncing | brand sync icon, % + ETA, ProgressBar + phase segments (check changes, scan, import, index) | 取消. Remove is disabled |
| Locked | warning lock, "已鎖定 · 等待 Outlook 關閉". The MessageBar says it retries at the next run (14:35) and shows the locked-run count | 重試 |
| Failed | danger icon + reason ("找不到檔案：磁碟 E: 未連線"), consecutive failures, link to the error log | 重試, 變更路徑… |
| Disabled | gray slash, Switch off, dimmed body, next sync "—（已停用）"; manual sync still allowed | 立即同步 |
| Empty | EmptyState with "加入 PST 路徑…" (B2 also has "先設定排程"); the activity area says nothing is running | |

Schedule editor (both): mode = off / interval (SpinButton + 分鐘/小時 Dropdown, presets in B1) / daily (time input + weekday ToggleButton chips); a next-run preview (B1 one line, B2 the next 5 runs); an info MessageBar for "only while open; one catch-up run on start"; a note that unchanged files are skipped and locked files are deferred.

## Fluent UI React v9 mapping
| Region | Components |
|---|---|
| Page actions | `Toolbar`, `Button appearance="primary"`, `ToggleButton` (B1 排程) |
| Status summary | `Badge` / `CounterBadge`, `Text`, `Divider` |
| B1 source card | `Card` + `CardHeader` + `CardFooter`, `Switch`, `ProgressBar`, `MessageBar intent="warning/error"`, `Button` icon-only + `Tooltip` |
| B2 list / detail | `List` (selectable, `@fluentui/react-list`) or `TabList vertical`; detail `TabList` + `DataGrid` (history), `InfoLabel` |
| Schedule | B1 `InlineDrawer position="end"` / `OverlayDrawer` on narrow widths; B2 settings `Card`s; `RadioGroup`, `SpinButton`, `Dropdown`, `TimePicker` (`@fluentui/react-timepicker-compat`) or `Input type="time"`, `ToggleButton` weekday chips, `MessageBar intent="info"` |
| Activity | `ProgressBar`, `Spinner size="tiny"`; B2 status bar = plain flex `div` |
| Remove / add | `Dialog` (existing `RemoveDialog`), native file picker via Tauri dialog |
