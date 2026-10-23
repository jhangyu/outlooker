# Frontend / settings / Tauri command layer — code-reading brief (for PST scheduled sync + Import redesign)

Read-only brief. All claims were read in code (paths relative to repo root, `src-tauri/crates/outlooker-core/src` abbreviated `core/`).
Importer pipeline, gate and DB schema are covered by another member.

## 1. Import page today (`src/screens/import/index.tsx`, 467 lines)

Screen is a CSS grid, `gridTemplateRows: auto 1fr` (:42). Columns come from `useSplit("import", SPLITS)` (:116, :230): `p1` (min 320) and `p2` (min 200), both in group `cols` (:39). Defaults `1.6fr 1fr 1fr` (:230). Two `Splitter`s sit on the p1 and p2 borders (:322-324). Splits persist in localStorage `outlooker.splits.*` (splits.ts:33 `PREFIX`).

```
┌ cmdbar (:232) ───────────────────────────────────────────────────────────┐
│ "Import" [+ Add PST files…]            (spacer)  Messages | PST files | Database size │
├────────────────────────┬───────────────────────┬─────────────────────────┤
│ Imported PST files     │ Import queue          │ Error log — <pst name>  │
│ DataGrid (sortable,    │ role=listbox of       │ rows: time | icon |     │
│ resizable columns)     │ QueueCard             │ "stage · message" +     │
│ name|size|msgs|new|    │  name + status Badge  │ folder path · item id   │
│ date|status|[trash]    │  phase / n of N · %   │ [Load more]             │
│ rows: completed,failed,│  ProgressBar          │ empty: "Select a PST…"  │
│ cancelled, removing    │  rate / ETA / errors  │ or "No errors…"         │
│ click row = select     │  Cancel/Resume/Retry/ │                         │
│                        │  Remove               │                         │
└────────────────────────┴───────────────────────┴─────────────────────────┘
No PSTs at all (list empty after load): panes replaced by one full-width EmptyState + "Add PST files…" button (:244-256).
```

Controls and states:
- Command bar: title (`Subtitle2 h2`), primary "Add PST files…" (`data-testid=import-add`, :236), three stat blocks: messages and PST count from `useAppStatus().counts` (:240-241), DB size from `dbStatusQuery` or "—" (:242).
- Imported grid (:259-296): items filtered to `IMPORTED = completed|failed|cancelled|removing` (:23, :266). Columns and widths (:30-33): name 220 (icon, bold name, mono path in `description`), size 88, messages 88, new 72, date 192 (`finishedAt ?? queuedAt`), status 128 (tint `Badge`, `data-testid=pst-status`, plus a warning icon if `errorCount>0`), remove 48 (trash button `pst-remove`, disabled when `removing`, opens RemoveDialog). Numeric columns right-aligned (:34). All but remove are sortable (`compare`). Widths come from `useColumnWidths("import-files", …)` (:134).
- Queue pane (:297-320): items filtered to `QUEUED = queued|running|cancelling|cancelled|failed` (:24). Note `cancelled` and `failed` appear in BOTH panes. Empty: `EmptyState` queue.empty.
- QueueCard (:332-415): `role=option`, selectable (also by Enter/Space). Shows name, status Badge, phase label (`phase.scanning|importing|finalizing`, only while running), `done/total · percent` if `itemsTotal`, `ProgressBar` (indeterminate when running and total unknown, error colour when failed), bytes/s, ETA in minutes, error count. Buttons: Cancel (running/queued/cancelling; disabled while cancelling; `queue-cancel`), Resume (cancelled; `queue-resume`), Retry from checkpoint (failed; `queue-retry`), Remove (cancelled/failed).
- Status Badge colours (:25-28): queued informative, running brand, cancelling warning, cancelled subtle, failed danger, completed success, removing warning.
- ErrorLog (:417-467): `useInfiniteQuery(["importErrors", pstId])` → `api.importErrors({pstId, cursor, limit:100})`, enabled only when a row is selected; severity `jobFailed` = red error icon, else yellow warning icon; stage label from `stage.*`.
- RemoveDialog (`RemoveDialog.tsx:20-64`): modal, shows `messageCount`, confirm calls `api.pstRemove` → invalidates `pstList`, toast "Removing {{name}}" (`remove.done`); error shown in a `MessageBar`.
- Local state: `selected` id, `removing` PST (:131-132). Error-log pane follows `selected`.

Data and events:
- Queries: `pstListQuery` (`queries.ts:5`, key `["pstList"]`) and `dbStatusQuery` (`queries.ts:7`, refetch every 10 s). No other query.
- The Import screen itself subscribes to NO events. All events are registered once in `boot()` (`src/app/boot.ts:23-30`) and mutate the `pstList` cache via `importCache.ts` (`applyStatusChanged` :7, `applyProgress` :16, `applyToCache` :21, cache is only patched if already loaded).
- The page is always mounted after first visit (`AppFrame.tsx:83-89, 241-245`, `<Activity>` keeps state). Title bar badge "Importing N%" reads `pstList` for the first running PST (`AppFrame.tsx:152-159`).
- Mutations call `refresh()` = `invalidateQueries(pstList)` after add/cancel/resume (:137-152).

Facts relevant to sync (what the page cannot express today):
- `PstSource` DTO has no schedule, lock, last-sync or enabled field (`core/dto/import.rs:30-42`: id, path, displayName, fileSizeBytes, status, progress, messageCount, newMessageCount, errorCount, queuedAt, finishedAt). `ImportStatus` has 7 values, no "locked" / "waiting" (`dto/import.rs:47-55`); the status `Record<ImportStatus,…>` (:25) and `status.*` locale keys must be extended in step.
- One row per PST *source* (path+size+mtime unique, `db/migrations/V1__schema.sql:14`); a changed file would add a second row, the Import list has no concept of "registered path".

## 2. How a PST is added today

1. UI: `useEnqueuePst()` (`components/common/hooks.tsx:42-58`), used by Import (:136) and Onboarding (`screens/onboarding/index.tsx:107-109`; on success it navigates to Import).
2. `pickPstFiles()` (`api/dialog.ts:16-23`): `@tauri-apps/plugin-dialog` `open({multiple:true, directory:false, filters:[pst]})`. E2E build (`VITE_E2E=1`) reads `window.__outlookerE2ePicks` instead (:9-13). `pickFolder()` (:26) is the folder variant (used by Settings move).
3. `api.importEnqueue(paths)` (`api/commands.ts:39`) → `invoke("import_enqueue", { paths })` via `call()` (`api/call.ts:12-18`, errors normalised to `AppError`).
4. Rust: `commands::import::import_enqueue` (`src-tauri/src/commands/import.rs:12`) → `s.importer.enqueue(paths)` returns `Vec<EnqueueResult>` (`dto/import.rs:84-90`: `queued{pst}` | `rejected{path,error}`).
5. Back in the hook: each `rejected` row is toasted with `import:enqueue.rejected` (`hooks.tsx:50-51`); returns true when the call succeeded; Import then refetches `pstList` (:140). Progress then arrives by events (§4).
Other import commands: `pst_list`, `import_cancel`, `import_resume`, `pst_remove`, `import_errors` (`lib.rs:84-89`; wrappers `commands.ts:38-43`).

## 3. Settings

Storage is split in two places:
- `<app_local_data_dir>/settings.json` ("bootstrap", written atomically): `Bootstrap { data_dir: Option<String>, language }` (`core/settings/mod.rs:11-16`, path `core/paths/mod.rs:23-25`). Loaded at `build` (`core/app/mod.rs:49`). Lives outside the PostgreSQL data dir because it is needed before the DB starts (data dir location).
- PostgreSQL singleton table `settings` (`webp_quality smallint`, `updated_at`; `core/db/migrations/V1__schema.sql:176-181`). `SettingsStore::webp_quality` (`settings/mod.rs:54`) reads it; a cached last value (`state.1`, default 80) is returned while the DB is not ready (:62-66).
- DTOs (`core/dto/settings.rs:7-22`), ts-rs exported to `src/api/bindings/` (`TS_RS_EXPORT_DIR` in `src-tauri/.cargo/config.toml:2`): `Settings { language, webpQuality, dataDir }`, `SettingsPatch { language?: Language, webpQuality?: number }` — patch semantics = `Option` per field, `null` = leave unchanged (UI sends `{language:null, webpQuality:75}`, `settings.test.tsx:29`, `hooks.tsx:32`).
- Commands: `settings_get`, `settings_update(patch)` → returns the full `Settings` (`src-tauri/src/commands/settings.rs:7-14`); UI wrappers `commands.ts:44-45`.
- Settings validation is in `SettingsStore::update` (`settings/mod.rs:71-85`, error `AppError::invalid("webpQuality","outOfRange")`).

Adding a setting end-to-end (as the existing pattern implies; verified by reading each hop, not by doing it):
1. Storage: bootstrap file field (`Bootstrap`) if needed before DB, else new column in `settings` table via a new migration file (`core/db/migrations/` has V1, V2 only).
2. Rust DTO: add field to `Settings` and `SettingsPatch` (`dto/settings.rs`), populate in `SettingsStore::get` (:68) and apply in `update` (:71).
3. Regenerate bindings: ts-rs `#[ts(export)]` writes `src/api/bindings/*.ts` during `cargo test` (`events.json`/`commands.json` come from tests `dto/events.rs:73-77`, `dto/commands.rs:14-21`).
4. New command (if needed): fn in `src-tauri/src/commands/*.rs`; add to `COMMANDS` (`dto/commands.rs:2`, array length is hard-coded `24` at :2 and :19), to `generate_handler![]` (`lib.rs:70-95`; test `lib.rs:160-167` requires both lists equal), and to `api` in `src/api/commands.ts` (test `api.test.ts:15` requires exact match with `commands.json`).
5. UI: `settingsGet` query is inline in the screen, key `["settings"]` (`screens/settings/index.tsx:85`); writes do `qc.setQueryData(["settings"], await api.settingsUpdate(...))` (:101). Mock default for tests at `src/app/testing.tsx` (`settingsGet` / `settingsUpdate` returns `{language,webpQuality,dataDir}`) must gain the new field.
6. Locale keys in both `settings.json` files (parity test, §5).

Settings screen structure (`screens/settings/index.tsx`, 303 lines): left nav (`TabList vertical`, scrolls to card, :158-169) + body grid of 5 `Card`s via template areas `"storage storage db" "language content db" "index index index"` (:52-55): storage (data folder, Change… → `pickFolder` → `MoveDialog`), language (RadioGroup), content (WebP quality Slider + SpinButton + Apply), db (status facts), index (DataGrid + ANALYZE/VACUUM/REINDEX). Splits `nav, c1, c2, r1, r2` (:31-38). There is no schedule/sync card today. Section list is `SECTIONS` (:27-28) + `section.*` keys.

## 4. Event system

- Names (`core/dto/events.rs:9-20`, TS mirror `src/api/events.ts:9-16`): `app:status-changed` (AppStatus), `import:progress` (ImportProgress), `import:status-changed` (ImportStatusChanged `{pstId,status,removed,source?}`, `events.rs:22-30`), `library:changed` (`{counts}`), `data-dir:move-progress`, `maintenance:status-changed` (MaintenanceState). Adding an event means: `CoreEvent` variant + `name()` + `payload()` (`core/sink.rs:18-60`), `names::ALL` (array length 6 hard-coded, `dto/events.rs:16`), `EVENTS` + `EventPayloads` in `src/api/events.ts`, `boot()` subscription; `api.test.ts:19-21` checks `EVENTS` equals `events.json`.
- Emitter: `TauriSink` (`src-tauri/src/events.rs:10-58`) runs events through core `Throttle` (`core/sink/throttle.rs`), then `app.emit(name, payload)`; a spawned task flushes trailing events at deadlines (:31-45).
- Throttle (`throttle.rs:17-27`): `import:progress` per PST id every 250 ms (a phase change is always passed, `phase()` :31-36); `library:changed` every 5 s, and flushed once when a job ends — `ends_job` = status change to completed/failed/cancelled/queued or removed (:40-45); `data-dir:move-progress` 250 ms. All other events (incl. `import:status-changed`, `app:status-changed`, `maintenance:status-changed`) are emitted immediately. A new "sync/lock state changed" event would be unthrottled unless added to `throttle_key`.
- Frontend: `onEvent` wrapper (`events.ts:29-31`); `boot()` subscribes to all six first and then takes `appStatus` + `pstList` snapshot (`boot.ts:21-39`). Handling:
  - progress / status-changed → patch the `["pstList"]` cache in place (no refetch): `boot.ts:25-26`, `importCache.ts`. `status-changed` with `source` replaces or appends the whole PstSource; with only `status` patches the status; `removed` deletes.
  - `library:changed` → `AppStatusContext.setCounts` (`AppStatusContext.tsx:41`).
  - `data-dir:move-progress` → `setQueryData(moveProgressQuery)` (pseudo-query, `queries.ts:10-15`).
  - `maintenance:status-changed` → `invalidateQueries(dbStatus)` (`boot.ts:29`).
  - Only user actions use `invalidateQueries(pstList)` (Import `refresh`, RemoveDialog :32).
- Consequence for sync: any new per-path state shown on the Import list must either travel inside `PstSource` (arrives via `source` in `import:status-changed`) or a new event + cache patch; the current cache patcher only knows `PstSource`.

## 5. i18n and Fluent 2 conventions

i18n:
- react-i18next, resources bundled and synchronous (`src/app/i18n.ts:16-36`), namespaces `common, onboarding, import, settings, search, sql` (:16), fallback `en`, default ns `common`. Language is set from `AppStatus.language` after boot (`App.tsx:43`) and switched via `useLanguageSwitch` (`hooks.tsx:25-39`, persists with `settingsUpdate`).
- Files: `src/locales/{zh-TW,en}/<ns>.json`; keys camelCase, nested objects by area (`imported.*`, `queue.*`, `status.*`, `phase.*`, `log.*`, `stage.*`, `empty.*`, `remove.*`, `enqueue.*` in `import.json`); interpolation `{{name}}`; enum-valued keys looked up dynamically (`t(\`status.${p.status}\`)`, `screens/import/index.tsx:190`), so a new `ImportStatus` or phase value needs a key in both languages. Cross-namespace via `t("common:action.cancel")`.
- Parity enforced: `src/app/global.test.tsx:15-23` (same files and equal flattened key sets, per namespace). Adding a namespace requires editing `i18n.ts` (NAMESPACES/RESOURCES).
- Number/date/size formatting: never `toLocaleString`/`toFixed`; use `formatNumber`, `formatBytes`, `formatDateTime`, `formatDay`, `formatMonth` from `components/common/display.ts` (`Intl` with cached formatters, `cimode`→`en`), passing `i18n.language`; pass the formatted string into `t(..., {n: formatNumber(...)})` (e.g. `index.tsx:193`). Note there is no relative-time or duration formatter ("About {{minutes}} min left" does rounding inline, :383); schedule/next-run text would need a new helper there. Bytes use B/KB/MB/GB/TB base-1024 (`display.ts:5`).
- Errors: `formatAppError(error, t)` maps every `AppError` kind (`common.test.tsx:27` checks all kinds are localized); `AppError` kinds incl. `busy.*`, `fileAccess`, `alreadyImported` are in `common.json` `error.*`. There is no "file locked" kind today (`locales/en/common.json` error block) — lock classification would need a new `AppError` variant or a distinct status.

Fluent 2:
- `@fluentui/react-components` v9 (spec says 9.74.10), `FluentProvider theme={webLightTheme}` only (`App.tsx:79`; no dark theme anywhere). Styling: `makeStyles` + `tokens` + `mergeClasses` per file (`index.tsx:41-112`), no CSS files beyond `src/app/app.css` (global user-select rule and `white-space: nowrap` for Button/Badge). Icons `@fluentui/react-icons` Regular variants. Components in use on Import: Button, Badge, DataGrid family, ProgressBar, Dialog, MessageBar, Body1/Body1Strong/Caption1/Subtitle2. Settings uses Card, Field, Input, RadioGroup, Slider, SpinButton, TabList, Spinner (settings/index.tsx:1-5). Toasts via `useAppToast` (`hooks.tsx:10-23`, toaster id "app", mounted in `AppFrame.tsx:252`).
- Shared building blocks to reuse: `EmptyState` (icon+title+body+action, `common/EmptyState.tsx`), `Splitter` + `useSplit` (px/fr tracks, persisted, min/max/group, `common/splits.ts`, `Splitter.tsx`), `useColumnWidths` / `useGridColumnSizing` (`columnWidths.ts`), `ellipsis` style helper (`display.ts:2`).
- Design intent: `docs/design/variant-1/NOTES.md` — "sibling of new Outlook": brand-blue title bar, left rail, neutral surfaces, 32 px controls, 14/20 body, full-height multi-column grids with independently scrolling panes, light only (:6); its component map for Import (:33) says Toolbar + primary Button, DataGrid, List of Cards with ProgressBar and Pause/Cancel/Resume/Retry/Remove, error log List, modal Remove dialog. Explicitly NOT implemented per user (:4): Pause, worker-count setting, deleted/kept preview, drag-and-drop zone. Text selection is disabled app-wide (`app.css`, uncommitted); content to copy opts in with `userSelect:"text"`.
- Shell: `AppFrame.tsx` grid 48 px title bar + 68 px nav rail; screens ids `search|advancedSql|import|settings` (`AppStatusContext.tsx:5`); Import screen fills `main` (overflow hidden), so a redesign must scroll inside its own panes.

## 6. Existing frontend tests to update if Import is redesigned

- `src/screens/import/import.test.tsx` (56 lines, 3 tests): (a) `importCache` apply functions — pure, survives a redesign unless `PstSource`/cache logic changes; (b) removal dialog via `pst-remove` testid → `dialog` → `remove-confirm` → expects `api.pstRemove(7)` and "1,234" in the dialog; (c) add flow via `import-add` testid → `pickPstFiles` + `importEnqueue` mocks → toast "is not an Outlook PST file". Hooks on testids `import-add`, `pst-remove`, `remove-confirm`; text from `import.json`.
- `src/app/AppFrame.test.tsx:64` clicks `nav-import` only for state-keeping (no Import content). `src/app/boot.test.ts:54-64` tests the `pstList` cache routing (depends on `PstSource.status` patching). `src/screens/search/search.test.tsx` and `src/api/dialog.test.ts` mention import/pick but do not assert Import UI. `src/api/api.test.ts` checks `api` keys == `commands.json` and `EVENTS` == `events.json` — fails the moment a command/event is added without regenerating bindings (needs the ts-rs test run to rewrite json).
- `src/app/global.test.tsx` (locale parity) will fail if one language lacks a key.
- `src/components/common/common.test.tsx` (formatAppError all kinds, display helpers) — update if a new `AppError` kind is added.
- `src/screens/settings/settings.test.tsx` (1 test, WebP validation; mocked `settingsUpdate` called with `{language:null, webpQuality:75}`) — must be updated if `SettingsPatch` gains fields that the UI always sends (the mock expects an exact object).
- `src/app/testing.tsx` helpers (`makePst` :35, `mockApiDefaults` :78, `eventsMock`) must be extended for any new PstSource field / command (the `api` mock is automock of `commands.ts`; every new command needs a default).
- E2E (opt-in, `tests/e2e/flow.e2e.ts:7,24,56-61`) uses testids `pst-row` (+`data-pst-name`), `pst-status` ("Completed" text), `pst-remove`, `nav-import`; `seeded.e2e.ts` exists too (not read).
- Test policy (CLAUDE.md): UI layout/copy/wiring tests are not to be added; only core logic (schedule next-run, lock classification, sync state) deserves committed tests.

## 7. Uncommitted working tree relevant to Import (`git diff`, read-only)

- Only one line touches the Import screen: `src/screens/import/index.tsx:107` adds `userSelect: "text"` to the error-log message style (`logMsg`) so error messages can be copied. No structural Import change in flight.
- Rest of the diff is app-wide: `src/app/app.css` disables text selection globally (opt-in via `userSelect:"text"`); `Splitter.tsx`/`splits.ts`/`splits.test.tsx` make drags update once per animation frame and write localStorage only on release (`onChange(px, live?)` new second parameter); small edits in mail ReadingPane/MessageHeader, advanced-sql ResultTable/index, AppFrame `pre` style. Untracked: the requirements spec. HEAD commit 17da584 already pinned grouped splitters (relevant: the 3-pane Import layout uses group `cols`).

## Uncertainties
- Where the importer/queue (`s.importer`) and supervisor emit `import:status-changed` and fill `PstSource.progress` was not read (other member's scope); only the DTO and frontend consumers were verified.
- Which bindings file would carry new DTOs: confirmed export dir and `#[ts(export)]` but not run (no builds allowed); the claim "bindings are regenerated by `cargo test`" is inferred from the test fns writing json (`events.rs:73-77`, `commands.rs:14-21`) and ts-rs derive, not observed.
- `seeded.e2e.ts` and `MoveDialog.tsx` were not read. Onboarding's Import step beyond `useEnqueuePst` was not read.
