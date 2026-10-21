# Outlooker Frontend + App Shell Implementation Plan (R1-06, R1-07, R2-13, R2-14)

> **For agentic workers:** REQUIRED SUB-SKILL: Use podium:team-spawn (recommended) or podium:team-fable to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the React UI (App Frame, five screens, mail / DSL / SQL-editor components), the Tauri App Shell with Command Layer, Event Emitter and `mailimg` scheme, the `core::app` start / shutdown composition, and the WebdriverIO E2E suite of Outlooker.

**Architecture:** The UI is one Vite + React 19 app with one `FluentProvider theme={webLightTheme}`, i18next (namespaces per screen, `zh-TW` + `en`), TanStack Query for every command result, and React context only for app status and navigation. It calls the core only through the typed client `src/api/` (C4). The Tauri crate `outlooker` is thin: `lib.rs` builds Tauri, registers plugins, 24 commands, the `mailimg` scheme and the exit handler; all composition lives in `outlooker_core::app`.

**Tech Stack:** React 19.3.1, TypeScript 7.0.2, Vite 8.3.5, @fluentui/react-components 9.74.10, @fluentui/react-icons 2.0.344, @tanstack/react-query 5.104.5, @tanstack/react-virtual 3.14.21 (virtual-core 3.19.6), codemirror 6.0.2 + @codemirror/lang-sql 6.10.0 + @codemirror/state 6.7.6 + @codemirror/view 6.43.15, i18next 26.5.4 + react-i18next 17.1.0, @tauri-apps/api 2.12.4, @tauri-apps/plugin-dialog 2.8.3, Vitest 5.0.5 + Testing Library 16.3.3 + jsdom 30.1.3; Rust 1.99.1, tauri 2.12.5, tauri-plugin-single-instance 2.5.4, tauri-plugin-dialog 2.8.3, tauri-plugin-log 2.10.1, tauri-plugin-wdio-webdriver 1.5.0 (feature `e2e` only), tokio 1.54.0; WebdriverIO 9.32.0 + @wdio/tauri-service 1.5.0.

**Decisions (hard-to-reverse only):**
- **Search query composition in the frontend.** The refine pane, the search box and the DSL builder compose one `SearchQuery`: search text alone → `simple`; anything else → `dsl` whose root is `and(<one or(sender,recipient,subject,body) group per term>, <refine conditions>, <builder group>)`. Alternatives: (a) send only the builder tree and ignore the box when the builder is open; (b) a new command with three parts. Drivers: the requirements put box + builder + refine on one screen; Part 03 §5.1 has only `simple | dsl`; no contract change. Drawback: the term expansion repeats the core's simple-query rule (one function `termGroup`, tested against the Part 03 §5.1 rule text).
- **Fixed-height virtual rows with month headers as rows.** Each mail row and each month header is one 32 px virtual item; the cap row and the loader row sit after the virtual block. Alternative: variable-height rows like the prototype's 3-line item. Driver: Part 03 §5.2 fixes 32 px rows and the 3.2M px height budget. Drawback: one line per mail (sender · subject — snippet · date) instead of the prototype's three lines.
- **E2E test seams are compile-time gated.** The app data dir override (`OUTLOOKER_APP_DATA_DIR`) is read only with cargo feature `e2e`; the native-picker bypass is read only when Vite builds with `VITE_E2E=1`. Alternative: `browser.tauri.mock` from `tauri-plugin-wdio`, which Part 01 does not list. Driver: WebDriver cannot drive native file dialogs, and the E2E must not touch the user's real data dir. Drawback: the E2E binary is a separate debug build (`--features e2e`, `VITE_E2E=1`).

Plan gate: `/Users/jhangyu/.claude/plugins/cache/jhangyu/podium/1.1.0/protocols/review.md` §L0 Plan gate.

## Global Constraints

The outline's Global Constraints (`docs/superpowers/plans/2026-10-21-outlooker-plan.md`, "Global Constraints") apply to every task here without exception. The lines below repeat the ones every frontend and shell implementer hits, with exact values.

- **Versions:** exact pins of spec Part 01 (above). No `^` / `~` in `package.json`. No new dependency without leader approval. Forbidden: `@uiw/react-codemirror`, `@codemirror/theme-one-dark`, `webDarkTheme`, `teamsDarkTheme`, zustand, tauri-specta, `@fluentui/react-list`, `@fluentui/react-datepicker-compat` (not in Part 01: use native `<input type="date">` inside Fluent `Input` and a plain virtualized `div` list).
- **Theme:** one `FluentProvider theme={webLightTheme}` in `src/app/App.tsx`. No `prefers-color-scheme`, no dark tokens, no theme switch.
- **i18n interpolation:** never use `count` as an interpolation variable (it triggers i18next plural lookup); numbers are passed pre-formatted as `n`.
- **i18n:** every user-visible string comes from `src/locales/<lang>/<namespace>.json` (`<lang>` exactly `zh-TW` and `en`; namespaces `common`, `onboarding`, `import`, `settings`, `search`, `sql`). Both languages have identical key sets. Strings the core sends (`AppError.detail`, `SqlError.message`, `ImportErrorEntry.message`, mail content, file paths) are shown verbatim and are not localized (Part 03 §1).
- **Forbidden key words:** no locale key path contains `pause`, `pauseAll`, `worker`, `reencode`, `format` or `highlight` (case-insensitive), because the out-of-scope test greps for them. Use `dateTime` / `display` instead of `format` in key names.
- **Out-of-scope UI controls (user ruling):** no exact deleted / kept preview in the removal dialog; no pause / pause all; no converter worker count; no re-encode of existing images; no disk-usage breakdown (show `databaseSizeBytes` only); no SQL Format button; no search-term highlighting; no drag-and-drop import; also not carried over from the prototype: the "Sort by relevance" select (ordering is fixed newest first, Part 03 §5.2), "Restart database", "Open folder", "Open log files", "Copy link", "Open in Outlook", the onboarding "PST files found in common locations" list and free-space badges (no command serves them).
- **Layout:** target 1920×1080, full-height grids, only the 48 px title bar as header, panes scroll on their own. Tokens and the component map: `docs/design/variant-1/NOTES.md`. Styling uses Griffel `makeStyles` with Fluent `tokens`; no global CSS file except `src/app/app.css` (body margin 0, `html, body, #root { height: 100% }`).
- **Type rule:** no hand-written TS copy of a DTO. Import every DTO type from `src/api/bindings/<TypeName>` (ts-rs output, one file per type).
- **Tauri APIs:** only `src/api/**` (R1-01) imports `@tauri-apps/*`. Screens call `api.*`, `onEvent`, `pickPstFiles`, `pickFolder` (C4).
- **Commands from React:** every `api.*` call runs inside TanStack Query (`useQuery`, `useInfiniteQuery`, `useMutation`) or inside an event handler that reports failure through the Toaster; no unhandled promise.
- **Optional DTO fields:** send every `Option<T>` request field explicitly (`null` when unset), for example `{ language: 'en', webpQuality: null }`, so the call works whether ts-rs emits `T | null` or `T?`.
- **Test selectors:** components carry the `data-testid` values listed in each task's Produces block; R2-14's E2E relies on them. `data-testid` is not visible text.
- **Tests:** Vitest with jsdom; `vi.mock('<relative path>/api/commands')` (automock) and the events fake of R1-06 (`eventsMock`). No fixed sleeps; use Testing Library `findBy*` / `waitFor` and `vi.useFakeTimers()` for the debounce. E2E uses WebdriverIO condition waits with explicit timeouts (`waitForDisplayed`, `waitUntil`); `browser.pause` is forbidden.
- **Git (shared tree):** `git add <own paths>` then `git commit -F <msgfile> -- <own paths>`; never `git add -A`, `stash` (incl. `stash list`), `reset`, `checkout --`, `clean`. Run `git rev-parse --abbrev-ref HEAD` before and `git status --porcelain` after each commit. Commit message files go to `scripts/tmp/` (gitignored).
- **Commands:** `npm` commands run at the repo root; `cargo` commands run in `src-tauri/`.
- **Paths:** `core/` = `src-tauri/crates/outlooker-core/` (same shorthand as the foundation file); `crates/` = `src-tauri/crates/`.

---

## Task R1-06: App Frame, shared UI, Onboarding, Import, Settings screens

**Files:**
- Create: `src/app/App.tsx` (providers + `AppFrame`), `src/app/app.css`, `src/app/i18n.ts`, `src/app/AppStatusContext.tsx` (replaces the kickoff stub), `src/app/boot.ts`, `src/app/AppFrame.tsx` (TitleBar, NavRail, router, blocking screens, Failed screen, Toaster), `src/app/testing.tsx`
- Create (tests): `src/app/boot.test.ts`, `src/app/AppFrame.test.tsx`, `src/app/global.test.tsx`
- Create: `src/components/common/EmptyState.tsx`, `src/components/common/formatAppError.ts`, `src/components/common/display.ts`, `src/components/common/index.ts` (replaces the kickoff stub), `src/components/common/common.test.tsx`
- Create: `src/screens/onboarding/index.tsx` (replaces stub), `src/screens/onboarding/onboarding.test.tsx`
- Create: `src/screens/import/index.tsx` (replaces stub), `src/screens/import/RemoveDialog.tsx`, `src/screens/import/importCache.ts`, `src/screens/import/import.test.tsx`
- Create: `src/screens/settings/index.tsx` (replaces stub), `src/screens/settings/MoveDialog.tsx`, `src/screens/settings/moveSession.ts` (frame-level move state, created with AppFrame in step 12), `src/screens/settings/settings.test.tsx`
- Create: `src/locales/zh-TW/{common,onboarding,import,settings}.json`, `src/locales/en/{common,onboarding,import,settings}.json`

**Interfaces:**
- Consumes (C1, R1-01): types from `src/api/bindings/`: `AppStatus`, `AppPhase`, `Language`, `LibraryCounts`, `AppError`, `BusyReason`, `InitializeRequest`, `PstSource`, `ImportStatus`, `ImportProgress`, `ImportPhase`, `EnqueueResult`, `ImportErrorsRequest`, `ImportErrorPage`, `ImportErrorEntry`, `ImportErrorStage`, `ImportErrorSeverity`, `ImportStatusChanged`, `LibraryChanged`, `Settings`, `SettingsPatch`, `DataDirMoveProgress`, `MaintenanceTask`, `MaintenanceState`, `DbStatus`, `IndexStatus`.
- Consumes (C4, R1-01): `import { api } from '../../api/commands'` with `api.appStatus(): Promise<AppStatus>`, `api.appInitialize(req: InitializeRequest): Promise<null>`, `api.appRetry(): Promise<null>`, `api.pstList(): Promise<PstSource[]>`, `api.importEnqueue(paths: string[]): Promise<EnqueueResult[]>`, `api.importCancel(pstId: number): Promise<null>`, `api.importResume(pstId: number): Promise<null>`, `api.pstRemove(pstId: number): Promise<null>`, `api.importErrors(req: ImportErrorsRequest): Promise<ImportErrorPage>`, `api.settingsGet(): Promise<Settings>`, `api.settingsUpdate(patch: SettingsPatch): Promise<Settings>`, `api.dataDirMove(target: string): Promise<null>`, `api.maintenanceRun(task: MaintenanceTask): Promise<null>`, `api.dbStatus(): Promise<DbStatus>`; `import { onEvent } from '../../api/events'` with `onEvent<N extends EventName>(name: N, handler: (payload: EventPayload<N>) => void): Promise<() => void>` where `EventName` is the union of the six Part 03 §9 names (`'app:status-changed' | 'import:progress' | 'import:status-changed' | 'library:changed' | 'data-dir:move-progress' | 'maintenance:status-changed'`); `import { pickPstFiles, pickFolder } from '../../api/dialog'` with `pickPstFiles(): Promise<string[]>` (empty array on cancel) and `pickFolder(): Promise<string | null>`. Rejections of `api.*` are `AppError` objects.
- Produces (C10, consumed by R1-07, R2-14):
  - `src/components/common/index.ts` exports:
    - `interface EmptyStateProps { icon: JSX.Element; title: string; body?: string; action?: ReactNode; testId?: string }` and `function EmptyState(props: EmptyStateProps): JSX.Element` (centered column: 72 px circle with `tokens.colorBrandBackground2` holding the icon, `Subtitle1` title, `Body1` body in `tokens.colorNeutralForeground3`, then `action`; root `data-testid={testId ?? 'empty-state'}`).
    - `function formatAppError(err: AppError, t: TFunction): string` and `function toAppError(e: unknown): AppError` (an object with a string `kind` is returned as is; anything else becomes `{ kind: 'internal', detail: String(e) }`).
    - `function useAppToast(): { error(e: unknown): void; notify(text: string, intent?: 'success' | 'error' | 'info'): void }` (dispatches to the `app` Toaster; `error` formats with `formatAppError(toAppError(e))`) and `function useLanguageSwitch(): (lng: Language) => Promise<void>` (`i18n.changeLanguage(lng)` then `api.settingsUpdate({ language: lng, webpQuality: null })`; a rejection is toasted).
    - `function formatNumber(n: number, lng: string): string` (`Intl.NumberFormat(lng)`; every display helper treats the i18next pseudo-language `cimode` as `en`), `function formatBytes(n: number, lng: string): string` (B / KB / MB / GB / TB, base 1024, one decimal from KB up; unit letters are not translated), `function formatDateTime(iso: string | null, lng: string): string` (`Intl.DateTimeFormat(lng, { dateStyle: 'medium', timeStyle: 'short' })`, local time; `''` for `null`), `function formatDay(iso: string | null, lng: string): string` (`dateStyle: 'short'`; `''` for `null`).
  - `src/app/AppStatusContext.tsx` exports:
    - `type ScreenId = 'search' | 'advancedSql' | 'import' | 'settings'`
    - `function useAppStatus(): AppStatus` (phase and error from `app:status-changed` or `app_status`, counts from `library:changed` or `app_status`).
    - `function useNavigation(): { screen: ScreenId; go(screen: ScreenId): void }`
    - `interface AppStatusApi { setStatus(s: AppStatus): void; setCounts(c: LibraryCounts): void }` and `function AppStatusProvider(props: { initial: AppStatus; initialScreen?: ScreenId; onGo?: (s: ScreenId) => void; apiRef?: MutableRefObject<AppStatusApi | null>; children: ReactNode }): JSX.Element` (holds status and screen state; `apiRef` lets `boot` routing push updates; `onGo` is the test hook).
  - Screen entry names (named exports, no props): `SearchScreen` (`src/screens/search/index.tsx`), `AdvancedSqlScreen` (`src/screens/advanced-sql/index.tsx`), `ImportScreen`, `SettingsScreen`, `OnboardingScreen`.
  - `src/app/testing.tsx` exports (test-only helpers, imported only by `*.test.ts(x)`):
    - `function renderWithProviders(ui: ReactElement, opts?: { lng?: 'en' | 'zh-TW' | 'cimode'; status?: Partial<AppStatus>; screen?: ScreenId }): RenderResult & { queryClient: QueryClient; go: Mock; i18n: i18n }` — wraps `FluentProvider(webLightTheme)` + `I18nextProvider(createI18n(lng ?? 'en'))` + a fresh `QueryClient({ defaultOptions: { queries: { retry: false } } })` + `AppStatusProvider` (status = `makeStatus(opts.status)`, `onGo` = the returned `go` mock) + `<Toaster toasterId="app" />`.
    - `const eventsMock: { onEvent: Mock; emit(name: string, payload: unknown): void; reset(): void }` — use as `vi.mock('<rel>/api/events', async () => (await import('<rel>/app/testing')).eventsMock)`.
    - `function mockApiDefaults(): void` — after `vi.mock('<rel>/api/commands')`, sets every `api.*` mock to resolve with an empty or default value (`appStatus` → `makeStatus()`, `pstList` → `[]`, `filterOptions` → `{ psts: [], folders: [] }`, `searchMail` → `{ rows: [], nextCursor: null, shortQuery: false, elapsedMs: 1 }`, `searchCount` → `{ count: 0, capped: false }`, `savedQueryList` → `[]`, `settingsGet` → `{ language: 'en', webpQuality: 80, dataDir: '/data' }`, `dbStatus` → `makeDbStatus()`, `importErrors` → `{ rows: [], nextCursor: null }`, every `() -> ()` command → `null`).
    - `function makeStatus(p?: Partial<AppStatus>): AppStatus` (default `{ phase: 'ready', dataDir: '/data', language: 'en', counts: { messages: 0, psts: 0 }, error: null }`), `function makeDbStatus(p?: Partial<DbStatus>): DbStatus`, `function makePst(p?: Partial<PstSource>): PstSource`.
  - `src/app/i18n.ts` exports `const NAMESPACES = ['common', 'onboarding', 'import', 'settings', 'search', 'sql'] as const`, `type Namespace = (typeof NAMESPACES)[number]`, `const RESOURCES: Record<'zh-TW' | 'en', Record<Namespace, Record<string, unknown>>>` and `function createI18n(lng: string): i18n` (synchronous init, `react-i18next` bound).
  - `src/app/App.tsx` exports `function App(): JSX.Element` (mounted by `src/main.tsx`, R1-01).
  - `data-testid` values (for R2-14): `title-bar`, `nav-search`, `nav-advancedSql`, `nav-import`, `nav-settings`, `blocking-screen`, `failed-screen`, `failed-retry`, `onboarding-lang-zh-TW`, `onboarding-lang-en`, `onboarding-next`, `onboarding-back`, `onboarding-dir-default`, `onboarding-dir-other`, `onboarding-db-ready`, `onboarding-pick-pst`, `onboarding-skip`, `import-add`, `pst-row` (one per imported PST, with `data-pst-name`), `pst-status` (inside the row), `pst-remove`, `remove-confirm`, `queue-card` (with `data-pst-name`), `queue-cancel`, `queue-resume`, `queue-retry`, `settings-language-en`, `settings-language-zh-TW`, `settings-webp-apply`, `settings-move`, `maint-analyze`, `maint-vacuum`, `maint-reindex`.

**Behavior:**
- **Boot (`boot.ts`).** `boot(sink)` first awaits `onEvent` for all six events (in Part 03 §9 order), then calls `api.appStatus()`, then — only when `phase === 'ready'` — `api.pstList()`; a `pstList` failure is ignored (the Import screen refetches). It returns `{ status, psts, unlisten }`. Event routing through `sink`: `app:status-changed` → `sink.status(payload)`; `library:changed` → `sink.counts(payload.counts)`; `import:status-changed` / `import:progress` → `importCache` patch of the `['pstList']` query; `maintenance:status-changed` → invalidate `['dbStatus']`; `data-dir:move-progress` → `setQueryData(['moveProgress'], payload)`. The status handler keeps the current `counts` when the payload phase is not `ready` (the core sends zero counts outside `ready`, Part 03 §4).
- **App (`App.tsx`).** Creates one `QueryClient` and the i18n instance (`createI18n('en')`), runs `boot` in an effect, renders a full-window Fluent `Spinner` until boot resolves; then `changeLanguage(status.language)`, seeds `['pstList']` with the boot list, and renders `AppStatusProvider` + `AppFrame`. If `boot` throws (no command handler yet in Round 1, or the core is down), it renders the Failed screen with the formatted error and a Retry button that re-runs `boot`.
- **AppFrame routing.** `failed` → Failed screen (title, body, `error` formatted, an `Accordion` "Details" item with `error.detail` when `kind === 'internal'` else the error JSON, Retry → `api.appRetry()`; a Retry rejection shows in a `MessageBar intent="error"`). `needsSetup`, or onboarding started in this session and not finished → `OnboardingScreen` (no nav rail). `initializingCluster` / `startingDb` / `stoppingDb` outside onboarding → blocking screen (`Spinner` + `common:phase.<phase>` + `common:phase.hint`), except while a data-folder move session is open (`useMoveSession() !== null`): the move takes the phase through `stoppingDb → startingDb → ready` (Part 03 §8.2), and its own blocking dialog on the still-mounted Settings screen shows the progress and the result. `ready` → title bar + nav rail + the screen of `useNavigation().screen` (default `search`).
- **Title bar (48 px, `tokens.colorBrandBackground`, white text).** Logo square, "Outlooker" (`common:app.name`), crumb `· <screen name>`, spacer, import status badge when a PST in `['pstList']` has status `running` (`common:titlebar.importing` with `percent` = `round(itemsDone / itemsTotal × 100)`, or `common:titlebar.scanning` while `itemsTotal` is null), and the language `ToggleButton` pair which calls `i18n.changeLanguage(lng)` and `api.settingsUpdate({ language: lng, webpQuality: null })` (failure → toast with `formatAppError`).
- **Nav rail (68 px, vertical `TabList`, icon 24 px + label 10/14).** Search (`Search24Regular`), SQL (`Database24Regular`), Import (`ArrowDownload24Regular`), flexible gap, Settings (`Settings24Regular`). Selected tab = `useNavigation().screen`.
- **Toaster.** One `<Toaster toasterId="app" position="bottom-end" />` in `AppFrame`; screens dispatch with `useToastController('app')` and `<Toast><ToastTitle>…</ToastTitle></Toast>`.
- **Onboarding (prototype `#onboarding`; grid `340px 1fr 440px`).** Left: step list (1 language, 2 data location, 3 prepare database, 4 import first PST; done steps show a check icon; current step `aria-current="step"`) and the privacy note. Middle: step body + footer (Back / Next; step 4: Import later / Choose PST files). Right: four feature blocks (`onboarding:feature.*`). Step 1: two selectable `Card`s (`common:language.zh-TW` / `common:language.en`); selecting calls `i18n.changeLanguage` and `api.settingsUpdate({ language, webpQuality: null })` (works before `ready`, Part 03 §8.1). Step 2: `Card` "Default location" showing `status.dataDir` (proposed default), `Card` "Another location…" → `pickFolder()`; the chosen path shows in mono text; Next is enabled when a path is chosen. Next on step 2 → `api.appInitialize({ dataDir, language })`; a rejection shows `formatAppError` in a `MessageBar` and stays on step 2. Step 3: `ProgressBar` (indeterminate until `ready`) + checklist "Create database cluster" (done when the phase has left `initializingCluster`) and "Start database service" (done at `ready`); `failed` is handled by AppFrame. Next is enabled at `ready` (`data-testid="onboarding-db-ready"` appears then). Step 4: primary `Button` "Choose PST files…" → `pickPstFiles()` (empty → stay) → `api.importEnqueue(paths)` → each `rejected` result is toasted with `formatAppError(result.error)` → finish onboarding and `go('import')`; "Import later" finishes and `go('search')`.
- **Import screen (prototype `#import`; grid rows `auto 1fr`, columns `1.6fr 1fr 1fr`).** Command bar (spans all columns): title, primary `Button` "Add PST files…" (`pickPstFiles` → `importEnqueue` → toast per rejected path, invalidate `['pstList']`), spacer, three stats: messages (`counts.messages`), PST files (`counts.psts`), database size (`formatBytes(dbStatus.databaseSizeBytes)`; `—` while unknown). Column 1 "Imported PST files": `DataGrid` (sortable) over `pstList` rows with status `completed`, `failed`, `cancelled` or `removing`; columns name + path (mono, ellipsis, `title` = full path), size (`formatBytes(fileSizeBytes)`), messages (`messageCount`), new (`newMessageCount`), imported (`formatDateTime(finishedAt ?? queuedAt)`), status `Badge` (`pst-status`, text `import:status.<status>`) plus a warning icon with the error count when `errorCount > 0`, remove icon `Button` (disabled while `removing`). Column 2 "Import queue": scroll list of cards for rows with status `queued`, `running`, `cancelling`, `cancelled`, `failed`: name, status `Badge`, phase label (`import:phase.<phase>`), `ProgressBar` (`value` = `itemsDone / itemsTotal`; no value = indeterminate while `itemsTotal` is null and running), counts line `itemsDone / itemsTotal · pct%`, rate `formatBytes(bytesPerSec)/s`, ETA in minutes, error count with a warning icon; actions by status: `running` and `queued` → Cancel; `cancelling` → disabled Cancel; `cancelled` → Resume + Remove; `failed` → Retry (= `importResume`) + Remove. Clicking a card selects it. Column 3 "Error log": `import_errors` of the selected PST via `useInfiniteQuery(['importErrors', pstId])` (`limit: 100`, cursor = `nextCursor`, a "Load more" `Button` while `hasNextPage`); each entry: time, severity icon (`jobFailed` → error, else warning), stage label (`import:stage.<stage>`), `message` verbatim, folder path + item id; `import:log.pick` when nothing is selected, `import:log.none` when empty. Empty state: when `pstList` is empty, the screen body shows `EmptyState` (`import:empty.title`, `import:empty.body` — no drag-and-drop wording — action "Add PST files…").
- **Import live updates (`importCache.ts`).** `applyStatusChanged(list: PstSource[], ev: ImportStatusChanged): PstSource[]` — `removed: true` → drop the row; `source` present → replace the row with the same id or append it; otherwise set `status` on the row with `pstId`. `applyProgress(list: PstSource[], p: ImportProgress): PstSource[]` → set `progress` on the row with `p.pstId`; unknown id → list unchanged. Both pure, return a new array, never mutate. `applyToCache(qc: QueryClient, f: (l: PstSource[]) => PstSource[])` applies to `['pstList']` when the cache has data.
- **Removal dialog (`RemoveDialog.tsx`).** Modal `Dialog` titled `import:remove.title` (`name` = `displayName`), body `import:remove.body` with `n` = `formatNumber(messageCount)` stating that messages also found in other PST files are kept and the PST file is not changed; Cancel and Remove. Remove calls `api.pstRemove(pst.id)` only after the confirm click; rejection → `MessageBar` inside the dialog with `formatAppError`; success → close and toast `import:remove.done`. No exclusive / shared split (out of scope).
- **Settings screen (prototype `#settings`; grid `220px 1fr`).** Left: vertical `TabList` of the five sections; selecting scrolls the card into view (`scrollIntoView`). Body: card grid `repeat(3, 1fr)`, gap 16 px. Cards: (1, spans 2) Data storage: read-only mono `Input` with `settings.dataDir`, `Button` "Change…" → `pickFolder()` → `MoveDialog`; help text. (2) Language: `RadioGroup` zh-TW / en; change = `changeLanguage` + `settingsUpdate({ language, webpQuality: null })`. (3) Mail content and images: `Slider` min 1 max 100 step 1 + `SpinButton` min 1 max 100, one shared draft value; Apply `Button` sends `settingsUpdate({ language: null, webpQuality })` only when `isValidQuality(draft)` (integer in 1..=100); otherwise shows `settings:webp.invalid` under the field and sends nothing; help `settings:webp.help`. (4) Database status: facts from `db_status`: status (`common:phase.<phase>`), engine (`settings:db.engine` with `version`), messages, PST files, inline images (`imageCount`), attachments (`attachmentCount`), size (`formatBytes(databaseSizeBytes)`), last vacuum, last analyze. (5, spans 3) Index maintenance: `DataGrid` of `indexes` (name mono, table, size, state `Badge` valid → `settings:index.healthy` / invalid → `settings:index.rebuild`); buttons ANALYZE / VACUUM / Rebuild all indexes → `api.maintenanceRun('analyze' | 'vacuum' | 'reindex')`; while `dbStatus.maintenance?.state === 'running'` the three are disabled and a `Spinner` with `settings:maintenance.running` shows; `busy` rejection → toast `formatAppError`. Empty state: when `counts.messages === 0` the Database status card shows `EmptyState` `settings:db.emptyTitle` under the facts.
- **Move dialog (`MoveDialog.tsx`).** Opens with the target path; confirm → `api.dataDirMove(target)`; then a non-dismissable state with `ProgressBar` (`bytesCopied / bytesTotal`) and `settings:move.phase.<phase>`; `done` → close + toast `settings:move.done`; `failed` → `MessageBar intent="error"` with `formatAppError(progress.error)` and Close. Reads progress from `['moveProgress']`. "Started" is the frame-level move session (`src/screens/settings/moveSession.ts`: `interface MoveSession { target: string }`, `useMoveSession(): MoveSession | null`, `setMoveSession(qc, s | null)`, TanStack key `['moveSession']`): confirm sets it before calling `dataDirMove` (the phase may leave `ready` before the call returns); a rejection clears it; closing the dialog (after `done`, or Close / Escape after `failed`) clears it. A rejection of `dataDirMove` (busy / invalid target) shows in the dialog before any progress.
- **i18n.** `createI18n(lng)`: `i18next.createInstance()`, `.use(initReactI18next)`, `init({ lng, fallbackLng: 'en', ns: NAMESPACES, defaultNS: 'common', resources: RESOURCES, interpolation: { escapeValue: false }, returnNull: false })` (resources given → synchronous). `RESOURCES` statically imports all 12 JSON files.
- **Global tests (`global.test.tsx`).** Locale parity (flattened key sets of `zh-TW/<ns>` and `en/<ns>` equal for each of the 6 namespaces; both directories contain exactly the 6 files). No-literal (each of the 5 screens and the Failed screen rendered with `lng: 'cimode'` after `mockApiDefaults()`; every non-empty text node matches `/^[A-Za-z0-9_.:\-]+$/` or `/^[\d\s.,:%+\-—·/]+$/`). No-dark (recursive read of every file under `src/`). Out-of-scope (no flattened key path in any namespace matches `/pause|worker|reencode|format|highlight/i`; the Import and Settings screens rendered in `en` contain no element whose text content or `aria-label` matches `/pause|worker|re-?encode|format|highlight/i`).

**Constraints:**
- Layer: `src/app` may import `src/screens/*`, `src/components/*`, `src/api/*`; screens import `src/components/*`, `src/app/AppStatusContext`, `src/api/*`; `src/components/common` imports React, Fluent, i18next and `src/api/*` only (never `src/app` or `src/screens`).
- No global state library: status and navigation in `AppStatusProvider`, server data in TanStack Query (keys `['pstList']`, `['importErrors', pstId]`, `['settings']`, `['dbStatus']`, `['moveProgress']`, `['moveSession']`).
- `['dbStatus']` uses `refetchInterval: 10_000` only while the Settings or Import screen is mounted (query refresh against the core; tests never wait for it).
- Visual mapping (variant-1 → Fluent v9): title bar = `.header`; nav rail = `.rail` as vertical `TabList`; cards = `Card`; pane heads 44 px with `Subtitle2`; colors and spacing only from `tokens`.

**Acceptance criteria:**
- [ ] `npm run typecheck` exits 0.
- [ ] `npm test -- src/app src/components/common src/screens/onboarding src/screens/import src/screens/settings` exits 0 with a mocked `src/api/commands` (`vi.mock`), including the named tests:
  - `boot subscribes to every event before calling app_status` (`src/app/boot.test.ts`)
  - `library:changed with messages > 0 updates useAppStatus().counts.messages` (`src/app/AppFrame.test.tsx`)
  - `Failed screen Retry calls appRetry` (`src/app/AppFrame.test.tsx`)
  - `each screen renders its empty state` (Import: `pstList` `[]` → `import:empty.title`; Settings: `counts.messages` 0 → `settings:db.emptyTitle`; Onboarding renders step 1; `src/app/global.test.tsx`)
  - `removal dialog shows messageCount and calls pstRemove only after confirm` (`src/screens/import/import.test.tsx`)
  - `WebP quality outside 1-100 is not sent` (0, 101 and 50.5 are not sent; 75 is sent as `{ language: null, webpQuality: 75 }`; `src/screens/settings/settings.test.tsx`)
- [ ] Locale parity test: for every namespace directory, `zh-TW` and `en` key sets are equal (`locale files: zh-TW and en have equal key sets per namespace`).
- [ ] No-literal test: each screen rendered with i18next language `cimode` shows only translation keys (no text node outside the key pattern) (`screens rendered in cimode show only keys`).
- [ ] No-dark test: no source file under `src/` contains `webDarkTheme`, `teamsDarkTheme` or `prefers-color-scheme` (`no dark theme tokens in src`).
- [ ] Out-of-scope-controls test: no locale namespace contains a key for pause, pause-all, workers, re-encode, format, or highlight (key-name grep list fixed in the test), and the Import and Settings screens render no element with those labels (`no out-of-scope controls`).
- [ ] Added: `importCache applies status changes, removal and progress without mutation` (`src/screens/import/import.test.tsx`).
- [ ] Added: `formatAppError maps every AppError kind to a non-empty localized string` (all 12 kinds and the 4 busy reasons, in `en` and `zh-TW`; `src/components/common/common.test.tsx`).
- [ ] Added: `keeps the move dialog through stoppingDb, startingDb and ready and toasts done` and `shows a failed move's error after the database is back in the old folder` (`src/app/AppFrame.test.tsx`, describe `data-folder move`): `blocking-screen` is never rendered during the move, the byte progress stays visible, the done toast and the failure message appear.
- [ ] Added: `onboarding initializes with the chosen dir and language` — Next on step 2 calls `appInitialize({ dataDir: '/data', language: 'en' })` (`src/screens/onboarding/onboarding.test.tsx`).
- [ ] Added: `grep -rln "@tauri-apps" src/app src/components src/screens` lists nothing (grep exit 1).

**Steps:**

Order: steps 1–5 land C10 in the first commit (R1-07 waits for it); later commits add boot / frame, Import, Settings, Onboarding and the global tests. Before step 1 read `docs/design/variant-1/index.html` sections `#screen-import`, `#screen-settings`, `#screen-onboarding` and `docs/design/variant-1/NOTES.md`.

- [ ] **Step 1: Write the four `en` locale namespaces.**

`src/locales/en/common.json`:
```json
{
  "app": { "name": "Outlooker", "loading": "Starting Outlooker…" },
  "nav": { "main": "Main navigation", "search": "Search", "advancedSql": "SQL", "import": "Import", "settings": "Settings" },
  "language": { "label": "Language", "zh-TW": "繁體中文", "en": "English" },
  "titlebar": { "importing": "Importing {{percent}}%", "scanning": "Scanning PST…" },
  "phase": {
    "needsSetup": "Setup required",
    "initializingCluster": "Creating the database…",
    "startingDb": "Starting the database…",
    "ready": "Running",
    "stoppingDb": "Stopping the database…",
    "failed": "Failed",
    "hint": "Search is available again when the database is ready."
  },
  "failed": {
    "title": "Outlooker could not start its database",
    "body": "Your mail data was not changed. Try again. If it keeps failing, open the details.",
    "details": "Details",
    "retry": "Retry"
  },
  "error": {
    "dbNotReady": "The database is not ready yet.",
    "notFound": "The item no longer exists.",
    "invalidInput": "The value of {{field}} is not valid.",
    "queryTimeout": "The query ran longer than {{seconds}} seconds and was stopped.",
    "cancelled": "The operation was cancelled.",
    "readOnlyViolation": "This connection is read-only. Data cannot be changed here.",
    "busy": {
      "importRunning": "An import is running. Try again when it finishes.",
      "maintenanceRunning": "Database maintenance is running. Try again when it finishes.",
      "dataDirMoving": "The data folder is being moved.",
      "pstRemoving": "A PST file is being removed."
    },
    "fileAccess": "Cannot access {{path}}.",
    "insufficientSpace": "Not enough free disk space at {{path}}.",
    "notAPst": "{{path}} is not an Outlook PST file.",
    "alreadyImported": "This PST file is already imported.",
    "internal": "Something went wrong. The app log has the details."
  },
  "action": { "cancel": "Cancel", "close": "Close", "goImport": "Go to Import" }
}
```

`src/locales/en/onboarding.json`:
```json
{
  "title": "Welcome to Outlooker",
  "note": "All data stays on this computer. No account, no upload.",
  "step": { "language": "Choose language", "location": "Data location", "database": "Prepare database", "import": "Import first PST" },
  "language": { "heading": "Pick your interface language", "body": "You can change this later in Settings.", "zhHint": "Traditional Chinese (Taiwan)", "enHint": "English" },
  "location": {
    "heading": "Where should mail data be stored?",
    "body": "Imported mail, indexes and images take about 60–80% of the original PST size. Pick a disk with enough free space.",
    "default": "Default location",
    "defaultHint": "Your user folder — right for most people",
    "other": "Another location…",
    "otherHint": "For example an external or larger drive"
  },
  "database": { "heading": "Preparing the local database", "body": "This happens once and takes about 20 seconds.", "create": "Create database cluster", "start": "Start database service", "ready": "The database is ready." },
  "import": { "heading": "Import your first PST", "body": "Import runs in the background. You can search what is already imported while it runs.", "pick": "Choose PST files…", "later": "Import later" },
  "nav": { "back": "Back", "next": "Next" },
  "feature": {
    "title": "What Outlooker does",
    "search": "Search a million messages in seconds",
    "searchBody": "Find mail by sender, recipient or a fragment of the body, in Chinese and English.",
    "dedupe": "Duplicates are stored once",
    "dedupeBody": "A message found in several PST files is stored once, with every place it came from.",
    "render": "Mail looks like the original",
    "renderBody": "HTML is cleaned, and inline images are kept as WebP.",
    "local": "Fully local",
    "localBody": "The database starts and stops with the app. No account and no network."
  }
}
```

`src/locales/en/import.json`:
```json
{
  "title": "Import",
  "add": "Add PST files…",
  "stats": { "messages": "Messages", "psts": "PST files", "size": "Database size", "unknown": "—" },
  "imported": { "title": "Imported PST files", "name": "File", "size": "Size", "messages": "Messages", "new": "New", "date": "Imported", "status": "Status", "remove": "Remove {{name}}" },
  "queue": {
    "title": "Import queue",
    "progress": "{{done}} / {{total}} · {{percent}}%",
    "rate": "{{rate}}/s",
    "eta": "About {{minutes}} min left",
    "errors": "{{n}} errors",
    "cancel": "Cancel",
    "resume": "Resume",
    "retry": "Retry from checkpoint",
    "remove": "Remove",
    "empty": "The queue is empty",
    "emptyBody": "Added PST files show their progress here."
  },
  "status": { "queued": "Queued", "running": "Importing", "cancelling": "Cancelling", "cancelled": "Cancelled", "failed": "Failed", "completed": "Completed", "removing": "Removing" },
  "phase": { "scanning": "Scanning folders", "importing": "Importing mail", "finalizing": "Building search indexes" },
  "log": { "title": "Error log", "pick": "Select a PST file to see its log.", "none": "No errors or warnings for this PST file.", "more": "Load more", "item": "Item {{id}}" },
  "stage": { "open": "Open", "scan": "Scan", "parse": "Parse", "html": "Content", "image": "Image", "store": "Store" },
  "empty": { "title": "No PST imported yet", "body": "Choose “Add PST files” to import Outlook PST files. Large files work: the import runs in the background." },
  "remove": {
    "title": "Remove “{{name}}”?",
    "body": "This PST file brought {{n}} messages. Messages that exist only in this PST file are deleted. Messages also found in other PST files are kept. The PST file itself is not changed.",
    "confirm": "Remove",
    "done": "Removing {{name}}"
  },
  "enqueue": { "rejected": "{{path}}: {{reason}}" }
}
```

`src/locales/en/settings.json`:
```json
{
  "title": "Settings",
  "section": { "storage": "Data storage", "language": "Language", "content": "Mail content and images", "db": "Database status", "index": "Index maintenance" },
  "storage": { "folder": "Folder", "change": "Change…", "help": "Changing the location stops the database, moves the data and restarts it. Search is unavailable while moving." },
  "language": { "help": "Applies immediately. No restart needed." },
  "webp": { "label": "Inline image WebP quality", "apply": "Apply", "invalid": "Enter a whole number from 1 to 100.", "help": "Affects future imports only.", "saved": "WebP quality saved.", "smaller": "Smaller files", "sharper": "Sharper images" },
  "db": {
    "status": "Status", "engine": "Engine", "engineValue": "PostgreSQL {{version}}", "messages": "Messages", "psts": "PST files",
    "images": "Inline images", "attachments": "Attachments", "size": "Database size", "lastVacuum": "Last VACUUM", "lastAnalyze": "Last ANALYZE",
    "never": "Never", "emptyTitle": "No mail imported yet", "emptyBody": "Counts fill in after the first import.",
    "unavailable": "Database status is unavailable while the database is not running."
  },
  "index": {
    "name": "Index", "table": "Table", "size": "Size", "state": "State", "healthy": "Healthy", "rebuild": "Rebuild advised",
    "analyze": "Update statistics (ANALYZE)", "vacuum": "Reclaim space (VACUUM)", "reindex": "Rebuild all indexes",
    "help": "Search is slower during a rebuild but stays available."
  },
  "maintenance": { "running": "Running {{task}}…", "started": "{{task}} started.", "task": { "analyze": "ANALYZE", "vacuum": "VACUUM", "reindex": "REINDEX" } },
  "move": {
    "title": "Move the data folder",
    "body": "Move all mail data to {{target}}? The database stops during the move.",
    "confirm": "Move",
    "progress": "{{copied}} of {{total}}",
    "done": "The data folder was moved.",
    "phase": { "stoppingDb": "Stopping the database…", "copying": "Copying data…", "verifying": "Verifying the copy…", "startingDb": "Starting the database…", "done": "Done", "failed": "The move failed. The data stays in the old folder." }
  }
}
```

- [ ] **Step 2: Write the four `zh-TW` locale namespaces (same keys).**

`src/locales/zh-TW/common.json`:
```json
{
  "app": { "name": "Outlooker", "loading": "正在啟動 Outlooker…" },
  "nav": { "main": "主要導覽", "search": "搜尋", "advancedSql": "進階 SQL", "import": "匯入", "settings": "設定" },
  "language": { "label": "語言", "zh-TW": "繁體中文", "en": "English" },
  "titlebar": { "importing": "匯入中 {{percent}}%", "scanning": "正在掃描 PST…" },
  "phase": {
    "needsSetup": "需要設定",
    "initializingCluster": "正在建立資料庫…",
    "startingDb": "正在啟動資料庫…",
    "ready": "執行中",
    "stoppingDb": "正在停止資料庫…",
    "failed": "失敗",
    "hint": "資料庫就緒後即可再次搜尋。"
  },
  "failed": {
    "title": "Outlooker 無法啟動資料庫",
    "body": "郵件資料沒有被修改。請再試一次；若持續失敗，請展開詳細資料。",
    "details": "詳細資料",
    "retry": "重試"
  },
  "error": {
    "dbNotReady": "資料庫尚未就緒。",
    "notFound": "此項目已不存在。",
    "invalidInput": "{{field}} 的值無效。",
    "queryTimeout": "查詢超過 {{seconds}} 秒，已停止。",
    "cancelled": "作業已取消。",
    "readOnlyViolation": "這是唯讀連線，無法修改資料。",
    "busy": {
      "importRunning": "匯入正在進行，請在完成後再試。",
      "maintenanceRunning": "資料庫維護正在進行，請在完成後再試。",
      "dataDirMoving": "資料夾正在搬移中。",
      "pstRemoving": "正在移除 PST 檔案。"
    },
    "fileAccess": "無法存取 {{path}}。",
    "insufficientSpace": "{{path}} 的磁碟可用空間不足。",
    "notAPst": "{{path}} 不是 Outlook PST 檔案。",
    "alreadyImported": "這個 PST 檔案已經匯入。",
    "internal": "發生錯誤，詳細內容在應用程式紀錄中。"
  },
  "action": { "cancel": "取消", "close": "關閉", "goImport": "前往匯入" }
}
```

`src/locales/zh-TW/onboarding.json`:
```json
{
  "title": "歡迎使用 Outlooker",
  "note": "所有資料只存在這台電腦上，不需要帳號，也不會上傳。",
  "step": { "language": "選擇語言", "location": "資料儲存位置", "database": "準備資料庫", "import": "匯入第一個 PST" },
  "language": { "heading": "先選擇介面語言", "body": "之後可以隨時在「設定」中變更。", "zhHint": "繁體中文（台灣）", "enHint": "英文" },
  "location": {
    "heading": "要把郵件資料存在哪裡？",
    "body": "匯入後的郵件、索引與圖片大約是原始 PST 的 60–80%。建議選擇剩餘空間足夠的磁碟。",
    "default": "預設位置",
    "defaultHint": "使用者資料夾，適合大多數人",
    "other": "其他位置…",
    "otherHint": "例如外接或容量較大的磁碟"
  },
  "database": { "heading": "正在準備本機資料庫", "body": "只需要進行一次，大約 20 秒。", "create": "建立資料庫叢集", "start": "啟動資料庫服務", "ready": "資料庫已就緒。" },
  "import": { "heading": "匯入你的第一個 PST", "body": "匯入在背景進行，進行中也可以搜尋已匯入的郵件。", "pick": "選擇 PST 檔案…", "later": "稍後再匯入" },
  "nav": { "back": "上一步", "next": "下一步" },
  "feature": {
    "title": "Outlooker 能做什麼",
    "search": "秒級搜尋百萬封郵件",
    "searchBody": "依寄件者、收件者或內文片段搜尋，中文與英文都能部分比對。",
    "dedupe": "重複郵件只存一次",
    "dedupeBody": "同一封郵件出現在多個 PST 中只存一次，並記錄每個來源。",
    "render": "郵件原貌呈現",
    "renderBody": "HTML 經過清理，內嵌圖片以 WebP 保存。",
    "local": "完全在本機",
    "localBody": "資料庫隨程式啟動與關閉，不需要帳號，也不連線網路。"
  }
}
```

`src/locales/zh-TW/import.json`:
```json
{
  "title": "匯入管理",
  "add": "加入 PST 檔案…",
  "stats": { "messages": "郵件", "psts": "PST 檔案", "size": "資料庫大小", "unknown": "—" },
  "imported": { "title": "已匯入的 PST", "name": "檔案", "size": "大小", "messages": "郵件", "new": "新增", "date": "匯入時間", "status": "狀態", "remove": "移除 {{name}}" },
  "queue": {
    "title": "匯入佇列",
    "progress": "{{done}} / {{total}} · {{percent}}%",
    "rate": "{{rate}}/秒",
    "eta": "剩餘約 {{minutes}} 分鐘",
    "errors": "{{n}} 個錯誤",
    "cancel": "取消",
    "resume": "繼續",
    "retry": "從檢查點重試",
    "remove": "移除",
    "empty": "佇列是空的",
    "emptyBody": "加入的 PST 會在這裡顯示進度。"
  },
  "status": { "queued": "等待中", "running": "匯入中", "cancelling": "取消中", "cancelled": "已取消", "failed": "失敗", "completed": "完成", "removing": "移除中" },
  "phase": { "scanning": "掃描資料夾", "importing": "匯入郵件", "finalizing": "建立搜尋索引" },
  "log": { "title": "錯誤紀錄", "pick": "選取 PST 檔案以檢視錯誤紀錄。", "none": "這個 PST 檔案沒有錯誤或警告。", "more": "載入更多", "item": "項目 {{id}}" },
  "stage": { "open": "開啟", "scan": "掃描", "parse": "解析", "html": "內容", "image": "圖片", "store": "儲存" },
  "empty": { "title": "尚未匯入任何 PST", "body": "按「加入 PST 檔案」匯入 Outlook PST 檔案。大型檔案也可以，匯入在背景進行。" },
  "remove": {
    "title": "移除「{{name}}」？",
    "body": "這個 PST 檔案帶入了 {{n}} 封郵件。只存在於這個 PST 的郵件會被刪除；也存在於其他 PST 的郵件會保留。PST 檔案本身不會被修改。",
    "confirm": "移除",
    "done": "正在移除 {{name}}"
  },
  "enqueue": { "rejected": "{{path}}：{{reason}}" }
}
```

`src/locales/zh-TW/settings.json`:
```json
{
  "title": "設定",
  "section": { "storage": "資料儲存位置", "language": "語言", "content": "郵件內容與圖片", "db": "資料庫狀態", "index": "索引維護" },
  "storage": { "folder": "資料夾", "change": "變更…", "help": "變更位置時會先停止資料庫、搬移資料，再重新啟動。搬移期間無法搜尋。" },
  "language": { "help": "立即套用，不需重新啟動。" },
  "webp": { "label": "內嵌圖片 WebP 品質", "apply": "套用", "invalid": "請輸入 1 到 100 的整數。", "help": "只影響之後的匯入。", "saved": "已儲存 WebP 品質。", "smaller": "檔案較小", "sharper": "畫質較佳" },
  "db": {
    "status": "狀態", "engine": "引擎", "engineValue": "PostgreSQL {{version}}", "messages": "郵件", "psts": "PST 檔案",
    "images": "內嵌圖片", "attachments": "附件", "size": "資料庫大小", "lastVacuum": "上次 VACUUM", "lastAnalyze": "上次 ANALYZE",
    "never": "從未", "emptyTitle": "尚未匯入任何郵件", "emptyBody": "第一次匯入後就會顯示統計數字。",
    "unavailable": "資料庫未執行時無法取得狀態。"
  },
  "index": {
    "name": "索引", "table": "資料表", "size": "大小", "state": "狀態", "healthy": "良好", "rebuild": "建議重建",
    "analyze": "更新統計資料 (ANALYZE)", "vacuum": "清理空間 (VACUUM)", "reindex": "重建所有索引",
    "help": "重建期間搜尋會變慢，但仍可使用。"
  },
  "maintenance": { "running": "正在執行 {{task}}…", "started": "已開始 {{task}}。", "task": { "analyze": "ANALYZE", "vacuum": "VACUUM", "reindex": "REINDEX" } },
  "move": {
    "title": "搬移資料夾",
    "body": "要把所有郵件資料搬到 {{target}} 嗎？搬移期間資料庫會停止。",
    "confirm": "搬移",
    "progress": "{{copied}} / {{total}}",
    "done": "資料夾已搬移。",
    "phase": { "stoppingDb": "正在停止資料庫…", "copying": "正在複製資料…", "verifying": "正在驗證複本…", "startingDb": "正在啟動資料庫…", "done": "完成", "failed": "搬移失敗，資料仍在原本的資料夾。" }
  }
}
```

- [ ] **Step 3: Write `src/app/i18n.ts`.**

`src/app/i18n.ts`:
```ts
import i18next, { type i18n } from "i18next";
import { initReactI18next } from "react-i18next";
import zhCommon from "../locales/zh-TW/common.json";
import zhOnboarding from "../locales/zh-TW/onboarding.json";
import zhImport from "../locales/zh-TW/import.json";
import zhSettings from "../locales/zh-TW/settings.json";
import zhSearch from "../locales/zh-TW/search.json";
import zhSql from "../locales/zh-TW/sql.json";
import enCommon from "../locales/en/common.json";
import enOnboarding from "../locales/en/onboarding.json";
import enImport from "../locales/en/import.json";
import enSettings from "../locales/en/settings.json";
import enSearch from "../locales/en/search.json";
import enSql from "../locales/en/sql.json";

export const NAMESPACES = ["common", "onboarding", "import", "settings", "search", "sql"] as const;
export type Namespace = (typeof NAMESPACES)[number];

export const RESOURCES: Record<"zh-TW" | "en", Record<Namespace, Record<string, unknown>>> = {
  "zh-TW": { common: zhCommon, onboarding: zhOnboarding, import: zhImport, settings: zhSettings, search: zhSearch, sql: zhSql },
  en: { common: enCommon, onboarding: enOnboarding, import: enImport, settings: enSettings, search: enSearch, sql: enSql },
};

/** Synchronous init: resources are bundled, so `init` loads them before it returns. */
export function createI18n(lng: string): i18n {
  const instance = i18next.createInstance();
  void instance.use(initReactI18next).init({
    lng,
    fallbackLng: "en",
    ns: [...NAMESPACES],
    defaultNS: "common",
    resources: RESOURCES,
    interpolation: { escapeValue: false },
    returnNull: false,
  });
  return instance;
}
```

- [ ] **Step 4: Write the failing common-component test** `src/components/common/common.test.tsx`:
```tsx
import { describe, expect, it } from "vitest";
import type { AppError } from "../../api/bindings/AppError";
import { createI18n } from "../../app/i18n";
import { formatAppError, formatBytes, formatNumber, toAppError } from ".";

const ERRORS: AppError[] = [
  { kind: "dbNotReady" },
  { kind: "notFound", entity: "mail", id: "7" },
  { kind: "invalidInput", field: "dataDir", reason: "notEmpty" },
  { kind: "queryTimeout", timeoutMs: 30000 },
  { kind: "cancelled" },
  { kind: "sqlError", sqlstate: "42601", message: "syntax error at or near \"SELEC\"", position: 1 },
  { kind: "readOnlyViolation" },
  { kind: "busy", reason: "importRunning" },
  { kind: "busy", reason: "maintenanceRunning" },
  { kind: "busy", reason: "dataDirMoving" },
  { kind: "busy", reason: "pstRemoving" },
  { kind: "fileAccess", path: "/x", reason: "notWritable" },
  { kind: "fileAccess", path: "/x", reason: "insufficientSpace" },
  { kind: "notAPst", path: "/x.pst" },
  { kind: "alreadyImported", pstId: 3 },
  { kind: "internal", detail: "boom" },
];

describe("common", () => {
  it("formatAppError maps every AppError kind to a non-empty localized string", () => {
    for (const lng of ["en", "zh-TW"]) {
      const t = createI18n(lng).t;
      for (const err of ERRORS) {
        const text = formatAppError(err, t);
        expect(text.length, `${lng} ${JSON.stringify(err)}`).toBeGreaterThan(0);
        expect(text.startsWith("error."), `${lng} ${err.kind} is untranslated`).toBe(false);
      }
    }
    const en = createI18n("en").t;
    expect(formatAppError(ERRORS[5]!, en)).toBe('syntax error at or near "SELEC"');
    expect(formatAppError({ kind: "queryTimeout", timeoutMs: 30000 }, en)).toContain("30");
  });

  it("toAppError keeps AppError values and wraps anything else", () => {
    expect(toAppError({ kind: "cancelled" })).toEqual({ kind: "cancelled" });
    expect(toAppError(new Error("x"))).toEqual({ kind: "internal", detail: "Error: x" });
  });

  it("display helpers format numbers and bytes, also in cimode", () => {
    expect(formatNumber(10000, "en")).toBe("10,000");
    expect(formatNumber(9999, "zh-TW")).toBe("9,999");
    expect(formatBytes(512, "en")).toBe("512 B");
    expect(formatBytes(1536, "en")).toBe("1.5 KB");
    expect(formatBytes(50_331_648, "cimode")).toBe("48.0 MB");
  });
});
```
Run: `npx vitest run src/components/common`
Expected: FAIL — against the kickoff stub `formatBytes` / `formatNumber` return the wrong text and `formatAppError` returns the untranslated key for `busy` (`en busy is untranslated`).

- [ ] **Step 5: Implement the shared components.**

`src/components/common/display.ts`:
```ts
/** i18next's test pseudo-language "cimode" is not a BCP 47 tag; Intl would throw. */
const loc = (lng: string) => (lng === "cimode" ? "en" : lng);
const UNITS = ["B", "KB", "MB", "GB", "TB"] as const;

export function formatNumber(n: number, lng: string): string {
  return new Intl.NumberFormat(loc(lng)).format(n);
}

export function formatBytes(n: number, lng: string): string {
  let value = n;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = unit === 0 ? 0 : 1;
  const text = new Intl.NumberFormat(loc(lng), { minimumFractionDigits: digits, maximumFractionDigits: digits }).format(value);
  return `${text} ${UNITS[unit]}`;
}

export function formatDateTime(iso: string | null, lng: string): string {
  return iso ? new Intl.DateTimeFormat(loc(lng), { dateStyle: "medium", timeStyle: "short" }).format(new Date(iso)) : "";
}

export function formatDay(iso: string | null, lng: string): string {
  return iso ? new Intl.DateTimeFormat(loc(lng), { dateStyle: "short" }).format(new Date(iso)) : "";
}
```

`src/components/common/formatAppError.ts`:
```ts
import type { TFunction } from "i18next";
import type { AppError } from "../../api/bindings/AppError";
import { isAppError } from "../../api/call";

export function toAppError(e: unknown): AppError {
  return isAppError(e) ? e : { kind: "internal", detail: String(e) };
}

/** Maps AppError.kind to a localized sentence (Part 03 §1 Localization). SqlError.message is shown verbatim. */
export function formatAppError(err: AppError, t: TFunction): string {
  switch (err.kind) {
    case "busy":
      return t(`common:error.busy.${err.reason}`);
    case "sqlError":
      return err.message;
    case "queryTimeout":
      return t("common:error.queryTimeout", { seconds: Math.round(err.timeoutMs / 1000) });
    case "fileAccess":
      return t(err.reason === "insufficientSpace" ? "common:error.insufficientSpace" : "common:error.fileAccess", { path: err.path });
    case "notAPst":
      return t("common:error.notAPst", { path: err.path });
    case "invalidInput":
      return t("common:error.invalidInput", { field: err.field });
    default:
      return t(`common:error.${err.kind}`);
  }
}
```

`src/components/common/EmptyState.tsx`:
```tsx
import { Body1, Subtitle1, makeStyles, tokens } from "@fluentui/react-components";
import type { JSX, ReactNode } from "react";

export interface EmptyStateProps {
  icon: JSX.Element;
  title: string;
  body?: string;
  action?: ReactNode;
  testId?: string;
}

const useStyles = makeStyles({
  root: {
    display: "flex",
    flexDirection: "column",
    alignItems: "center",
    justifyContent: "center",
    gap: tokens.spacingVerticalS,
    padding: tokens.spacingVerticalXXL,
    textAlign: "center",
    height: "100%",
    boxSizing: "border-box",
  },
  art: {
    width: "72px",
    height: "72px",
    borderRadius: tokens.borderRadiusCircular,
    display: "grid",
    placeItems: "center",
    backgroundColor: tokens.colorBrandBackground2,
    color: tokens.colorBrandForeground1,
    fontSize: "36px",
  },
  body: { color: tokens.colorNeutralForeground3, maxWidth: "420px" },
});

export function EmptyState({ icon, title, body, action, testId }: EmptyStateProps) {
  const s = useStyles();
  return (
    <div className={s.root} data-testid={testId ?? "empty-state"}>
      <div className={s.art} aria-hidden>
        {icon}
      </div>
      <Subtitle1>{title}</Subtitle1>
      {body ? <Body1 className={s.body}>{body}</Body1> : null}
      {action}
    </div>
  );
}
```

`src/components/common/hooks.tsx`:
```tsx
import { Toast, ToastTitle, useToastController } from "@fluentui/react-components";
import { useCallback, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../../api/commands";
import type { Language } from "../../api/bindings/Language";
import { formatAppError, toAppError } from "./formatAppError";

export function useAppToast() {
  const { dispatchToast } = useToastController("app");
  const { t } = useTranslation();
  return useMemo(() => {
    const notify = (text: string, intent: "success" | "error" | "info" = "success") =>
      dispatchToast(
        <Toast>
          <ToastTitle>{text}</ToastTitle>
        </Toast>,
        { intent },
      );
    return { notify, error: (e: unknown) => notify(formatAppError(toAppError(e), t), "error") };
  }, [dispatchToast, t]);
}

export function useLanguageSwitch(): (lng: Language) => Promise<void> {
  const { i18n } = useTranslation();
  const toast = useAppToast();
  return useCallback(
    async (lng: Language) => {
      await i18n.changeLanguage(lng);
      try {
        await api.settingsUpdate({ language: lng, webpQuality: null });
      } catch (e) {
        toast.error(e);
      }
    },
    [i18n, toast],
  );
}
```

`src/components/common/index.ts`:
```ts
export { EmptyState, type EmptyStateProps } from "./EmptyState";
export { formatAppError, toAppError } from "./formatAppError";
export { formatBytes, formatDateTime, formatDay, formatNumber } from "./display";
export { useAppToast, useLanguageSwitch } from "./hooks";
```

Run: `npx vitest run src/components/common`
Expected: PASS (3 tests).

- [ ] **Step 6: Write `src/app/AppStatusContext.tsx` (replaces the kickoff stub; its behavior is tested through `AppFrame` in step 10).**

`src/app/AppStatusContext.tsx`:
```tsx
import { createContext, useContext, useImperativeHandle, useMemo, useState, type ReactNode, type RefObject } from "react";
import type { AppStatus } from "../api/bindings/AppStatus";
import type { LibraryCounts } from "../api/bindings/LibraryCounts";

export type ScreenId = "search" | "advancedSql" | "import" | "settings";

export interface AppStatusApi {
  setStatus(s: AppStatus): void;
  setCounts(c: LibraryCounts): void;
}

interface Ctx {
  status: AppStatus;
  screen: ScreenId;
  onboarding: boolean;
  go(screen: ScreenId): void;
}

const StatusContext = createContext<Ctx | null>(null);

export function AppStatusProvider(props: {
  initial: AppStatus;
  initialScreen?: ScreenId;
  onGo?: (s: ScreenId) => void;
  apiRef?: RefObject<AppStatusApi | null>;
  children: ReactNode;
}) {
  const [status, setStatus] = useState(props.initial);
  const [screen, setScreen] = useState<ScreenId>(props.initialScreen ?? "search");
  const [onboarding, setOnboarding] = useState(props.initial.phase === "needsSetup");
  const { onGo } = props;

  useImperativeHandle(
    props.apiRef,
    () => ({
      setStatus: (next) => {
        if (next.phase === "needsSetup") setOnboarding(true);
        // The core sends zero counts outside Ready (Part 03 §4); keep the last known counts.
        setStatus((prev) => (next.phase === "ready" ? next : { ...next, counts: prev.counts }));
      },
      setCounts: (counts) => setStatus((prev) => ({ ...prev, counts })),
    }),
    [],
  );

  const value = useMemo<Ctx>(
    () => ({
      status,
      screen,
      onboarding,
      go: (s) => {
        setOnboarding(false); // navigating is how onboarding finishes
        setScreen(s);
        onGo?.(s);
      },
    }),
    [status, screen, onboarding, onGo],
  );
  return <StatusContext.Provider value={value}>{props.children}</StatusContext.Provider>;
}

function useCtx(): Ctx {
  const ctx = useContext(StatusContext);
  if (!ctx) throw new Error("AppStatusProvider is missing");
  return ctx;
}

export function useAppStatus(): AppStatus {
  return useCtx().status;
}

export function useNavigation(): { screen: ScreenId; go(screen: ScreenId): void } {
  const { screen, go } = useCtx();
  return { screen, go };
}

/** R1-06 internal: true while the first-run wizard owns the window. */
export function useOnboardingActive(): boolean {
  return useCtx().onboarding;
}
```

- [ ] **Step 7: Write the test helpers `src/app/testing.tsx`.**
```tsx
import { FluentProvider, Toaster, webLightTheme } from "@fluentui/react-components";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, type RenderResult } from "@testing-library/react";
import type { i18n } from "i18next";
import type { ReactElement } from "react";
import { I18nextProvider } from "react-i18next";
import { vi, type Mock } from "vitest";
import { api } from "../api/commands";
import type { AppStatus } from "../api/bindings/AppStatus";
import type { DbStatus } from "../api/bindings/DbStatus";
import type { PstSource } from "../api/bindings/PstSource";
import { AppStatusProvider, type ScreenId } from "./AppStatusContext";
import { createI18n } from "./i18n";

export function makeStatus(p: Partial<AppStatus> = {}): AppStatus {
  return { phase: "ready", dataDir: "/data", language: "en", counts: { messages: 0, psts: 0 }, error: null, ...p };
}

export function makeDbStatus(p: Partial<DbStatus> = {}): DbStatus {
  return {
    phase: "ready",
    postgresVersion: "18.6",
    dataDir: "/data",
    databaseSizeBytes: 50_331_648,
    counts: { messages: 0, psts: 0 },
    imageCount: 0,
    attachmentCount: 0,
    indexes: [],
    lastVacuumAt: null,
    lastAnalyzeAt: null,
    maintenance: null,
    ...p,
  };
}

export function makePst(p: Partial<PstSource> = {}): PstSource {
  return {
    id: 1,
    path: "/mail/archive.pst",
    displayName: "archive",
    fileSizeBytes: 1_048_576,
    status: "completed",
    progress: null,
    messageCount: 120,
    newMessageCount: 100,
    errorCount: 0,
    queuedAt: "2026-10-01T08:00:00Z",
    finishedAt: "2026-10-01T08:10:00Z",
    ...p,
  };
}

type Handler = (payload: unknown) => void;
const handlers = new Map<string, Set<Handler>>();

/** Replacement module for src/api/events in tests: vi.mock("<rel>/api/events", async () => (await import("<rel>/app/testing")).eventsMock). */
export const eventsMock = {
  onEvent: vi.fn(async (name: string, handler: Handler) => {
    const set = handlers.get(name) ?? new Set<Handler>();
    set.add(handler);
    handlers.set(name, set);
    return () => set.delete(handler);
  }),
  emit(name: string, payload: unknown) {
    handlers.get(name)?.forEach((h) => h(payload));
  },
  reset() {
    handlers.clear();
    eventsMock.onEvent.mockClear();
  },
};

/** Call after vi.mock("<rel>/api/commands"): every command resolves with an empty or default value. */
export function mockApiDefaults(): void {
  const m = vi.mocked(api);
  m.appStatus.mockResolvedValue(makeStatus());
  m.appInitialize.mockResolvedValue(null);
  m.appRetry.mockResolvedValue(null);
  m.searchMail.mockResolvedValue({ rows: [], nextCursor: null, shortQuery: false, elapsedMs: 1 });
  m.searchCount.mockResolvedValue({ count: 0, capped: false });
  m.filterOptions.mockResolvedValue({ psts: [], folders: [] });
  m.mailGet.mockRejectedValue({ kind: "notFound", entity: "mail", id: "0" });
  m.savedQueryList.mockResolvedValue([]);
  m.savedQueryCreate.mockResolvedValue({ id: 1, name: "q", sql: "SELECT 1", updatedAt: "2026-10-01T00:00:00Z" });
  m.savedQueryUpdate.mockResolvedValue({ id: 1, name: "q", sql: "SELECT 1", updatedAt: "2026-10-01T00:00:00Z" });
  m.savedQueryDelete.mockResolvedValue(null);
  m.sqlExecute.mockResolvedValue({ mode: "command", tag: "SELECT 0", elapsedMs: 1 });
  m.sqlCancel.mockResolvedValue(null);
  m.pstList.mockResolvedValue([]);
  m.importEnqueue.mockResolvedValue([]);
  m.importCancel.mockResolvedValue(null);
  m.importResume.mockResolvedValue(null);
  m.pstRemove.mockResolvedValue(null);
  m.importErrors.mockResolvedValue({ rows: [], nextCursor: null });
  m.settingsGet.mockResolvedValue({ language: "en", webpQuality: 80, dataDir: "/data" });
  m.settingsUpdate.mockResolvedValue({ language: "en", webpQuality: 80, dataDir: "/data" });
  m.dataDirMove.mockResolvedValue(null);
  m.maintenanceRun.mockResolvedValue(null);
  m.dbStatus.mockResolvedValue(makeDbStatus());
}

export function renderWithProviders(
  ui: ReactElement,
  opts: { lng?: "en" | "zh-TW" | "cimode"; status?: Partial<AppStatus>; screen?: ScreenId } = {},
): RenderResult & { queryClient: QueryClient; go: Mock; i18n: i18n } {
  const instance = createI18n(opts.lng ?? "en");
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  const go = vi.fn();
  const result = render(
    <FluentProvider theme={webLightTheme}>
      <I18nextProvider i18n={instance}>
        <QueryClientProvider client={queryClient}>
          <AppStatusProvider initial={makeStatus(opts.status)} initialScreen={opts.screen} onGo={go}>
            {ui}
            <Toaster toasterId="app" />
          </AppStatusProvider>
        </QueryClientProvider>
      </I18nextProvider>
    </FluentProvider>,
  );
  return { ...result, queryClient, go, i18n: instance };
}
```
If `npm run typecheck` reports a field mismatch in `makeDbStatus` / `makePst`, the generated binding in `src/api/bindings/` is the truth: align the factory, never the binding.

- [ ] **Step 8: Typecheck and land C10 (first commit).**
Run: `npm run typecheck`
Expected: exit 0.
Run: `git rev-parse --abbrev-ref HEAD` (expected `main`), then
```bash
printf 'feat(ui): shared frontend contract C10, locales and test helpers\n\nCo-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>\n' > scripts/tmp/msg-r106-1.txt
git add src/app/i18n.ts src/app/AppStatusContext.tsx src/app/testing.tsx src/components/common src/locales/en/common.json src/locales/en/onboarding.json src/locales/en/import.json src/locales/en/settings.json src/locales/zh-TW/common.json src/locales/zh-TW/onboarding.json src/locales/zh-TW/import.json src/locales/zh-TW/settings.json
git commit -F scripts/tmp/msg-r106-1.txt -- src/app/i18n.ts src/app/AppStatusContext.tsx src/app/testing.tsx src/components/common src/locales/en/common.json src/locales/en/onboarding.json src/locales/en/import.json src/locales/en/settings.json src/locales/zh-TW/common.json src/locales/zh-TW/onboarding.json src/locales/zh-TW/import.json src/locales/zh-TW/settings.json
git status --porcelain
```
Expected: the commit succeeds; `git status --porcelain` lists none of these paths. Tell the leader "C10 landed" (R1-07 waits for it).

- [ ] **Step 9: Write the failing boot test** `src/app/boot.test.ts`:
```ts
import { QueryClient } from "@tanstack/react-query";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api/commands");
vi.mock("../api/events", async () => (await import("./testing")).eventsMock);

import { api } from "../api/commands";
import { boot } from "./boot";
import { eventsMock, makePst, makeStatus, mockApiDefaults } from "./testing";

const ALL_EVENTS = [
  "app:status-changed",
  "import:progress",
  "import:status-changed",
  "library:changed",
  "data-dir:move-progress",
  "maintenance:status-changed",
];

describe("boot", () => {
  beforeEach(() => {
    eventsMock.reset();
    mockApiDefaults();
  });

  it("boot subscribes to every event before calling app_status", async () => {
    const order: string[] = [];
    eventsMock.onEvent.mockImplementation(async (name: string) => {
      order.push(name);
      return () => {};
    });
    vi.mocked(api.appStatus).mockImplementation(async () => {
      order.push("app_status");
      return makeStatus();
    });
    vi.mocked(api.pstList).mockImplementation(async () => {
      order.push("pst_list");
      return [];
    });
    const sink = { status: vi.fn(), counts: vi.fn() };
    await boot(new QueryClient(), sink);
    expect(order.slice(0, 6).sort()).toEqual([...ALL_EVENTS].sort());
    expect(order.slice(6)).toEqual(["app_status", "pst_list"]);
  });

  it("does not call pst_list before the database is ready", async () => {
    vi.mocked(api.appStatus).mockResolvedValue(makeStatus({ phase: "needsSetup" }));
    const r = await boot(new QueryClient(), { status: vi.fn(), counts: vi.fn() });
    expect(api.pstList).not.toHaveBeenCalled();
    expect(r.psts).toBeNull();
  });

  it("routes events to the sink and the pstList cache", async () => {
    const qc = new QueryClient();
    const sink = { status: vi.fn(), counts: vi.fn() };
    await boot(qc, sink);
    qc.setQueryData(["pstList"], [makePst({ id: 4, status: "running" })]);
    eventsMock.emit("library:changed", { counts: { messages: 5, psts: 1 } });
    eventsMock.emit("app:status-changed", makeStatus({ phase: "startingDb" }));
    eventsMock.emit("import:status-changed", { pstId: 4, status: "completed", removed: false, source: null });
    expect(sink.counts).toHaveBeenCalledWith({ messages: 5, psts: 1 });
    expect(sink.status).toHaveBeenCalledWith(expect.objectContaining({ phase: "startingDb" }));
    expect(qc.getQueryData<{ status: string }[]>(["pstList"])?.[0]?.status).toBe("completed");
  });
});
```
Run: `npx vitest run src/app/boot.test.ts`
Expected: FAIL — `Failed to resolve import "./boot"`.

- [ ] **Step 10: Write `src/screens/import/importCache.ts` and `src/app/boot.ts`.**

`src/screens/import/importCache.ts`:
```ts
import type { QueryClient } from "@tanstack/react-query";
import type { ImportProgress } from "../../api/bindings/ImportProgress";
import type { ImportStatusChanged } from "../../api/bindings/ImportStatusChanged";
import type { PstSource } from "../../api/bindings/PstSource";

export function applyStatusChanged(list: PstSource[], ev: ImportStatusChanged): PstSource[] {
  if (ev.removed) return list.filter((p) => p.id !== ev.pstId);
  const source = ev.source;
  if (source) {
    return list.some((p) => p.id === source.id) ? list.map((p) => (p.id === source.id ? source : p)) : [...list, source];
  }
  return list.map((p) => (p.id === ev.pstId ? { ...p, status: ev.status } : p));
}

export function applyProgress(list: PstSource[], progress: ImportProgress): PstSource[] {
  if (!list.some((p) => p.id === progress.pstId)) return list;
  return list.map((p) => (p.id === progress.pstId ? { ...p, progress } : p));
}

export function applyToCache(qc: QueryClient, f: (list: PstSource[]) => PstSource[]): void {
  const current = qc.getQueryData<PstSource[]>(["pstList"]);
  if (current) qc.setQueryData(["pstList"], f(current));
}
```

`src/app/boot.ts`:
```ts
import type { QueryClient } from "@tanstack/react-query";
import { api } from "../api/commands";
import { onEvent } from "../api/events";
import type { AppStatus } from "../api/bindings/AppStatus";
import type { LibraryCounts } from "../api/bindings/LibraryCounts";
import type { PstSource } from "../api/bindings/PstSource";
import { applyProgress, applyStatusChanged, applyToCache } from "../screens/import/importCache";

export interface BootSink {
  status(s: AppStatus): void;
  counts(c: LibraryCounts): void;
}

export interface BootResult {
  status: AppStatus;
  psts: PstSource[] | null;
  unlisten(): void;
}

/** Part 03 §9: subscribe to every event first, then take the snapshot, so no transition is missed. */
export async function boot(qc: QueryClient, sink: BootSink): Promise<BootResult> {
  const unlisteners = await Promise.all([
    onEvent("app:status-changed", (s) => sink.status(s)),
    onEvent("import:progress", (p) => applyToCache(qc, (l) => applyProgress(l, p))),
    onEvent("import:status-changed", (e) => applyToCache(qc, (l) => applyStatusChanged(l, e))),
    onEvent("library:changed", (e) => sink.counts(e.counts)),
    onEvent("data-dir:move-progress", (p) => qc.setQueryData(["moveProgress"], p)),
    onEvent("maintenance:status-changed", () => void qc.invalidateQueries({ queryKey: ["dbStatus"] })),
  ]);
  const unlisten = () => unlisteners.forEach((u) => u());
  try {
    const status = await api.appStatus();
    const psts = status.phase === "ready" ? await api.pstList().catch(() => null) : null;
    return { status, psts, unlisten };
  } catch (e) {
    unlisten();
    throw e;
  }
}
```
Run: `npx vitest run src/app/boot.test.ts`
Expected: PASS (3 tests).

- [ ] **Step 11: Write the failing frame test** `src/app/AppFrame.test.tsx`:
```tsx
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api/commands");
vi.mock("../api/dialog");
vi.mock("../api/events", async () => (await import("./testing")).eventsMock);
vi.mock("../screens/search", async () => {
  const { useAppStatus } = await import("./AppStatusContext");
  return { SearchScreen: () => <span data-testid="probe">{String(useAppStatus().counts.messages)}</span> };
});

import { api } from "../api/commands";
import { App } from "./App";
import { eventsMock, makeStatus, mockApiDefaults } from "./testing";

describe("App frame", () => {
  beforeEach(() => {
    eventsMock.reset();
    mockApiDefaults();
  });

  it("library:changed with messages > 0 updates useAppStatus().counts.messages", async () => {
    render(<App />);
    expect((await screen.findByTestId("probe")).textContent).toBe("0");
    eventsMock.emit("library:changed", { counts: { messages: 5, psts: 1 } });
    await waitFor(() => expect(screen.getByTestId("probe").textContent).toBe("5"));
  });

  it("Failed screen Retry calls appRetry", async () => {
    vi.mocked(api.appStatus).mockResolvedValue(makeStatus({ phase: "failed", error: { kind: "internal", detail: "x" } }));
    render(<App />);
    fireEvent.click(await screen.findByTestId("failed-retry"));
    await waitFor(() => expect(api.appRetry).toHaveBeenCalledTimes(1));
  });

  it("shows the blocking screen while the database starts and the frame when ready", async () => {
    vi.mocked(api.appStatus).mockResolvedValue(makeStatus({ phase: "startingDb" }));
    render(<App />);
    expect(await screen.findByTestId("blocking-screen")).toBeTruthy();
    eventsMock.emit("app:status-changed", makeStatus({ phase: "ready" }));
    expect(await screen.findByTestId("title-bar")).toBeTruthy();
    expect(screen.getByTestId("nav-import")).toBeTruthy();
  });

  it("shows onboarding while setup is needed", async () => {
    vi.mocked(api.appStatus).mockResolvedValue(makeStatus({ phase: "needsSetup" }));
    render(<App />);
    expect(await screen.findByTestId("onboarding-lang-en")).toBeTruthy();
  });
});
```
Run: `npx vitest run src/app/AppFrame.test.tsx`
Expected: FAIL — the kickoff `App` renders `null` (`Unable to find an element by: [data-testid="probe"]`).

- [ ] **Step 12: Write `src/app/app.css`, `src/app/AppFrame.tsx` and `src/app/App.tsx`.**

`src/app/app.css`:
```css
html, body, #root { height: 100%; margin: 0; }
```

`src/app/AppFrame.tsx`:
```tsx
import {
  Accordion, AccordionHeader, AccordionItem, AccordionPanel, Badge, Body1, Button, Caption1, MessageBar, MessageBarBody,
  Spinner, Tab, TabList, Title2, Toaster, ToggleButton, makeStyles, tokens,
} from "@fluentui/react-components";
import {
  ArrowDownload24Regular, Database24Regular, ErrorCircleRegular, Search24Regular, Settings24Regular,
} from "@fluentui/react-icons";
import { useQuery } from "@tanstack/react-query";
import { useState, type JSX } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../api/commands";
import type { AppError } from "../api/bindings/AppError";
import type { AppPhase } from "../api/bindings/AppPhase";
import { formatAppError, toAppError, useLanguageSwitch } from "../components/common";
import { AdvancedSqlScreen } from "../screens/advanced-sql";
import { ImportScreen } from "../screens/import";
import { OnboardingScreen } from "../screens/onboarding";
import { SearchScreen } from "../screens/search";
import { SettingsScreen } from "../screens/settings";
import { useMoveSession } from "../screens/settings/moveSession";
import { useAppStatus, useNavigation, useOnboardingActive, type ScreenId } from "./AppStatusContext";

const useStyles = makeStyles({
  app: { display: "grid", gridTemplateRows: "48px 1fr", gridTemplateColumns: "68px 1fr", height: "100vh", overflow: "hidden" },
  titleBar: {
    gridColumn: "1 / -1", display: "flex", alignItems: "center", gap: tokens.spacingHorizontalM,
    paddingRight: tokens.spacingHorizontalL, backgroundColor: tokens.colorBrandBackground, color: tokens.colorNeutralForegroundOnBrand,
  },
  brand: { width: "68px", display: "grid", placeItems: "center" },
  logo: {
    width: "28px", height: "28px", borderRadius: tokens.borderRadiusMedium, display: "grid", placeItems: "center",
    backgroundColor: tokens.colorNeutralBackground1, color: tokens.colorBrandForeground1, fontWeight: tokens.fontWeightBold,
  },
  appName: { fontSize: tokens.fontSizeBase400, fontWeight: tokens.fontWeightSemibold },
  crumb: { opacity: 0.9 },
  spacer: { flex: 1 },
  langButton: { color: tokens.colorNeutralForegroundOnBrand },
  rail: {
    backgroundColor: tokens.colorNeutralBackground3, borderRight: `1px solid ${tokens.colorNeutralStroke2}`,
    padding: `${tokens.spacingVerticalS} ${tokens.spacingHorizontalXS}`, display: "flex", flexDirection: "column", gap: tokens.spacingVerticalXS,
  },
  railTab: { flexDirection: "column", height: "60px", justifyContent: "center", gap: "2px", fontSize: tokens.fontSizeBase100 },
  railGap: { flex: 1 },
  main: { minWidth: 0, minHeight: 0, overflow: "hidden", backgroundColor: tokens.colorNeutralBackground3 },
  center: {
    height: "100vh", display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center",
    gap: tokens.spacingVerticalM, padding: tokens.spacingHorizontalXXL, boxSizing: "border-box", textAlign: "center",
  },
  failedIcon: { fontSize: "48px", color: tokens.colorStatusDangerForeground1 },
  details: { width: "min(720px, 100%)", textAlign: "left" },
  pre: { whiteSpace: "pre-wrap", fontFamily: tokens.fontFamilyMonospace, fontSize: tokens.fontSizeBase200, margin: 0 },
});

const SCREENS: Record<ScreenId, () => JSX.Element | null> = {
  search: SearchScreen,
  advancedSql: AdvancedSqlScreen,
  import: ImportScreen,
  settings: SettingsScreen,
};

export function FailedScreen({ error, onRetry }: { error: AppError | null; onRetry: () => Promise<unknown> }) {
  const s = useStyles();
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  const [retryError, setRetryError] = useState<AppError | null>(null);
  const retry = async () => {
    setBusy(true);
    setRetryError(null);
    try {
      await onRetry();
    } catch (e) {
      setRetryError(toAppError(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className={s.center} data-testid="failed-screen">
      <ErrorCircleRegular className={s.failedIcon} aria-hidden />
      <Title2>{t("failed.title")}</Title2>
      <Body1>{t("failed.body")}</Body1>
      {error ? <Body1>{formatAppError(error, t)}</Body1> : null}
      {retryError ? (
        <MessageBar intent="error">
          <MessageBarBody>{formatAppError(retryError, t)}</MessageBarBody>
        </MessageBar>
      ) : null}
      {error ? (
        <Accordion collapsible className={s.details}>
          <AccordionItem value="details">
            <AccordionHeader>{t("failed.details")}</AccordionHeader>
            <AccordionPanel>
              <pre className={s.pre}>{error.kind === "internal" ? error.detail : JSON.stringify(error, null, 2)}</pre>
            </AccordionPanel>
          </AccordionItem>
        </Accordion>
      ) : null}
      <Button appearance="primary" onClick={retry} disabled={busy} data-testid="failed-retry">
        {t("failed.retry")}
      </Button>
    </div>
  );
}

function BlockingScreen({ phase }: { phase: AppPhase }) {
  const s = useStyles();
  const { t } = useTranslation();
  return (
    <div className={s.center} data-testid="blocking-screen">
      <Spinner size="large" label={t(`phase.${phase}`)} />
      <Caption1>{t("phase.hint")}</Caption1>
    </div>
  );
}

function TitleBar() {
  const s = useStyles();
  const { t, i18n } = useTranslation();
  const { screen } = useNavigation();
  const setLanguage = useLanguageSwitch();
  const psts = useQuery({ queryKey: ["pstList"], queryFn: () => api.pstList() });
  const running = psts.data?.find((p) => p.status === "running");
  const total = running?.progress?.itemsTotal ?? null;
  const importing = running
    ? total
      ? t("titlebar.importing", { percent: Math.round(((running.progress?.itemsDone ?? 0) / total) * 100) })
      : t("titlebar.scanning")
    : null;
  return (
    <header className={s.titleBar} data-testid="title-bar">
      <div className={s.brand}>
        <div className={s.logo} aria-hidden>
          O
        </div>
      </div>
      <span className={s.appName}>{t("app.name")}</span>
      <span className={s.crumb}>· {t(`nav.${screen}`)}</span>
      <span className={s.spacer} />
      {importing ? <Badge appearance="filled" color="informative">{importing}</Badge> : null}
      <div role="group" aria-label={t("language.label")}>
        {(["zh-TW", "en"] as const).map((lng) => (
          <ToggleButton
            key={lng}
            size="small"
            appearance="transparent"
            className={s.langButton}
            checked={i18n.language === lng}
            onClick={() => void setLanguage(lng)}
          >
            {t(`language.${lng}`)}
          </ToggleButton>
        ))}
      </div>
    </header>
  );
}

function NavRail() {
  const s = useStyles();
  const { t } = useTranslation();
  const { screen, go } = useNavigation();
  const tab = (id: ScreenId, icon: JSX.Element) => (
    <Tab value={id} icon={icon} className={s.railTab} data-testid={`nav-${id}`}>
      {t(`nav.${id}`)}
    </Tab>
  );
  return (
    <TabList
      vertical
      selectedValue={screen}
      onTabSelect={(_, d) => go(d.value as ScreenId)}
      aria-label={t("nav.main")}
      className={s.rail}
    >
      {tab("search", <Search24Regular />)}
      {tab("advancedSql", <Database24Regular />)}
      {tab("import", <ArrowDownload24Regular />)}
      <div className={s.railGap} />
      {tab("settings", <Settings24Regular />)}
    </TabList>
  );
}

export function AppFrame() {
  const s = useStyles();
  const status = useAppStatus();
  const onboarding = useOnboardingActive();
  const { screen } = useNavigation();
  // A data-folder move drives the phase through stoppingDb → startingDb → ready (Part 03 §8.2). Its own blocking
  // dialog on the Settings screen shows the progress, so the frame stays mounted until that dialog is closed.
  const moving = useMoveSession() !== null;
  const Screen = SCREENS[screen];
  let body: JSX.Element;
  if (status.phase === "failed") body = <FailedScreen error={status.error} onRetry={() => api.appRetry()} />;
  else if (onboarding) body = <OnboardingScreen />;
  else if (status.phase !== "ready" && !moving) body = <BlockingScreen phase={status.phase} />;
  else
    body = (
      <div className={s.app}>
        <TitleBar />
        <NavRail />
        <main className={s.main}>
          <Screen />
        </main>
      </div>
    );
  return (
    <>
      {body}
      <Toaster toasterId="app" position="bottom-end" />
    </>
  );
}
```

`src/screens/settings/moveSession.ts` (frame-level move state; the TanStack cache outlives every screen):
```ts
import { useQuery, type QueryClient } from "@tanstack/react-query";

/** A move the user confirmed and whose dialog is still open; null otherwise. Set before `data_dir_move` is called. */
export interface MoveSession {
  target: string;
}

const KEY = ["moveSession"] as const;

export function useMoveSession(): MoveSession | null {
  return useQuery<MoveSession | null>({ queryKey: KEY, queryFn: () => null, enabled: false, initialData: null, gcTime: Infinity }).data ?? null;
}

export function setMoveSession(qc: QueryClient, session: MoveSession | null): void {
  qc.setQueryData(KEY, session);
}
```

`src/app/App.tsx`:
```tsx
import { FluentProvider, Spinner, webLightTheme } from "@fluentui/react-components";
import { QueryClient, QueryClientProvider, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { I18nextProvider, useTranslation } from "react-i18next";
import type { AppError } from "../api/bindings/AppError";
import type { AppStatus } from "../api/bindings/AppStatus";
import type { LibraryCounts } from "../api/bindings/LibraryCounts";
import { toAppError } from "../components/common";
import { AppStatusProvider, type AppStatusApi } from "./AppStatusContext";
import { AppFrame, FailedScreen } from "./AppFrame";
import { boot, type BootSink } from "./boot";
import { createI18n } from "./i18n";
import "./app.css";

const i18n = createI18n("en");

type BootState = { kind: "loading" } | { kind: "ready"; status: AppStatus } | { kind: "error"; error: AppError };

function Boot() {
  const qc = useQueryClient();
  const { t, i18n: inst } = useTranslation();
  const apiRef = useRef<AppStatusApi | null>(null);
  const pending = useRef<{ status?: AppStatus; counts?: LibraryCounts }>({});
  const [state, setState] = useState<BootState>({ kind: "loading" });
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let cancelled = false;
    let unlisten = () => {};
    // Events can arrive before AppStatusProvider mounts: buffer them, newest wins.
    const sink: BootSink = {
      status: (s) => (apiRef.current ? apiRef.current.setStatus(s) : (pending.current.status = s)),
      counts: (c) => (apiRef.current ? apiRef.current.setCounts(c) : (pending.current.counts = c)),
    };
    boot(qc, sink).then(
      async (r) => {
        unlisten = r.unlisten;
        if (cancelled) return r.unlisten();
        let status = pending.current.status ?? r.status;
        if (pending.current.counts) status = { ...status, counts: pending.current.counts };
        if (r.psts) qc.setQueryData(["pstList"], r.psts);
        await inst.changeLanguage(status.language);
        setState({ kind: "ready", status });
      },
      (e: unknown) => {
        if (!cancelled) setState({ kind: "error", error: toAppError(e) });
      },
    );
    return () => {
      cancelled = true;
      unlisten();
    };
  }, [attempt, qc, inst]);

  if (state.kind === "loading") return <Spinner size="large" label={t("app.loading")} style={{ height: "100vh" }} />;
  if (state.kind === "error")
    return (
      <FailedScreen
        error={state.error}
        onRetry={async () => {
          setState({ kind: "loading" });
          setAttempt((a) => a + 1);
        }}
      />
    );
  return (
    <AppStatusProvider initial={state.status} apiRef={apiRef}>
      <AppFrame />
    </AppStatusProvider>
  );
}

export function App() {
  const [queryClient] = useState(
    () => new QueryClient({ defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } } }),
  );
  return (
    <FluentProvider theme={webLightTheme} style={{ height: "100%" }}>
      <I18nextProvider i18n={i18n}>
        <QueryClientProvider client={queryClient}>
          <Boot />
        </QueryClientProvider>
      </I18nextProvider>
    </FluentProvider>
  );
}
```
Run: `npx vitest run src/app/AppFrame.test.tsx src/app/boot.test.ts`
Expected: `boot.test.ts` 3 PASS; `AppFrame.test.tsx` 3 PASS and `shows onboarding while setup is needed` FAIL (`Unable to find an element by: [data-testid="onboarding-lang-en"]`) — the onboarding stub renders nothing before step 21, which turns it green.

- [ ] **Step 13: Commit boot and frame.**
```bash
printf 'feat(ui): boot sequence, app frame, failed and blocking screens\n\nCo-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>\n' > scripts/tmp/msg-r106-2.txt
git add src/app/app.css src/app/App.tsx src/app/AppFrame.tsx src/app/boot.ts src/app/boot.test.ts src/app/AppFrame.test.tsx src/screens/import/importCache.ts src/screens/settings/moveSession.ts
git commit -F scripts/tmp/msg-r106-2.txt -- src/app/app.css src/app/App.tsx src/app/AppFrame.tsx src/app/boot.ts src/app/boot.test.ts src/app/AppFrame.test.tsx src/screens/import/importCache.ts src/screens/settings/moveSession.ts
git status --porcelain
```
Expected: commit succeeds; none of these paths listed afterwards.

- [ ] **Step 14: Write the failing Import screen test** `src/screens/import/import.test.tsx`:
```tsx
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api/commands");
vi.mock("../../api/dialog");
vi.mock("../../api/events", async () => (await import("../../app/testing")).eventsMock);

import { api } from "../../api/commands";
import { pickPstFiles } from "../../api/dialog";
import type { ImportProgress } from "../../api/bindings/ImportProgress";
import { makePst, mockApiDefaults, renderWithProviders } from "../../app/testing";
import { ImportScreen } from ".";
import { applyProgress, applyStatusChanged } from "./importCache";

const progress = (p: Partial<ImportProgress> = {}): ImportProgress => ({
  pstId: 2, phase: "importing", itemsTotal: 200, itemsDone: 50, stored: 40, duplicates: 5, errors: 5,
  skippedNonMail: 0, imagesConverted: 3, bytesPerSec: 1_048_576, etaSeconds: 120, ...p,
});

describe("Import screen", () => {
  beforeEach(() => mockApiDefaults());

  it("importCache applies status changes, removal and progress without mutation", () => {
    const list = [makePst({ id: 1 }), makePst({ id: 2, status: "running" })];
    const frozen = JSON.stringify(list);
    expect(applyStatusChanged(list, { pstId: 2, status: "completed", removed: false, source: null })[1]!.status).toBe("completed");
    expect(applyStatusChanged(list, { pstId: 1, status: "completed", removed: true, source: null }).map((p) => p.id)).toEqual([2]);
    expect(applyStatusChanged(list, { pstId: 3, status: "queued", removed: false, source: makePst({ id: 3, status: "queued" }) }).length).toBe(3);
    expect(applyProgress(list, progress())[1]!.progress?.itemsDone).toBe(50);
    expect(applyProgress(list, progress({ pstId: 99 }))).toBe(list);
    expect(JSON.stringify(list)).toBe(frozen);
  });

  it("removal dialog shows messageCount and calls pstRemove only after confirm", async () => {
    vi.mocked(api.pstList).mockResolvedValue([makePst({ id: 7, displayName: "archive-2019", messageCount: 1234 })]);
    renderWithProviders(<ImportScreen />);
    fireEvent.click(await screen.findByTestId("pst-remove"));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(/1,234/)).toBeTruthy();
    expect(api.pstRemove).not.toHaveBeenCalled();
    fireEvent.click(within(dialog).getByTestId("remove-confirm"));
    await waitFor(() => expect(api.pstRemove).toHaveBeenCalledWith(7));
  });

  it("shows the empty state when no PST is imported", async () => {
    renderWithProviders(<ImportScreen />);
    expect(await screen.findByText("No PST imported yet")).toBeTruthy();
  });

  it("adds PST files through the picker and toasts rejected paths", async () => {
    vi.mocked(pickPstFiles).mockResolvedValue(["/a.pst", "/b.txt"]);
    vi.mocked(api.importEnqueue).mockResolvedValue([
      { result: "queued", pst: makePst({ id: 9, status: "queued" }) },
      { result: "rejected", path: "/b.txt", error: { kind: "notAPst", path: "/b.txt" } },
    ]);
    renderWithProviders(<ImportScreen />);
    fireEvent.click(await screen.findByTestId("import-add"));
    await waitFor(() => expect(api.importEnqueue).toHaveBeenCalledWith(["/a.pst", "/b.txt"]));
    expect(await screen.findByText(/is not an Outlook PST file/)).toBeTruthy();
  });

  it("queue card actions call cancel and resume", async () => {
    vi.mocked(api.pstList).mockResolvedValue([
      makePst({ id: 2, displayName: "big", status: "running", progress: progress() }),
      makePst({ id: 3, displayName: "old", status: "cancelled" }),
    ]);
    renderWithProviders(<ImportScreen />);
    fireEvent.click(await screen.findByTestId("queue-cancel"));
    await waitFor(() => expect(api.importCancel).toHaveBeenCalledWith(2));
    fireEvent.click(screen.getByTestId("queue-resume"));
    await waitFor(() => expect(api.importResume).toHaveBeenCalledWith(3));
    expect(screen.getByText("50 / 200 · 25%")).toBeTruthy();
  });
});
```
Run: `npx vitest run src/screens/import`
Expected: FAIL — the kickoff `ImportScreen` renders `null` (first failing assertion: `Unable to find an element by: [data-testid="pst-remove"]`; the `importCache` test passes).

- [ ] **Step 15: Write `src/screens/import/RemoveDialog.tsx` and `src/screens/import/index.tsx`.**

`src/screens/import/RemoveDialog.tsx`:
```tsx
import {
  Button, Dialog, DialogActions, DialogBody, DialogContent, DialogSurface, DialogTitle, MessageBar, MessageBarBody,
} from "@fluentui/react-components";
import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../../api/commands";
import type { AppError } from "../../api/bindings/AppError";
import type { PstSource } from "../../api/bindings/PstSource";
import { formatAppError, formatNumber, toAppError, useAppToast } from "../../components/common";

/** Only the PST's own message count is shown: the exact deleted / kept split is out of scope (user ruling). */
export function RemoveDialog({ pst, onClose }: { pst: PstSource; onClose(): void }) {
  const { t, i18n } = useTranslation("import");
  const qc = useQueryClient();
  const toast = useAppToast();
  const [error, setError] = useState<AppError | null>(null);
  const [busy, setBusy] = useState(false);
  const confirm = async () => {
    setBusy(true);
    setError(null);
    try {
      await api.pstRemove(pst.id);
      await qc.invalidateQueries({ queryKey: ["pstList"] });
      toast.notify(t("remove.done", { name: pst.displayName }));
      onClose();
    } catch (e) {
      setError(toAppError(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Dialog open modalType="modal" onOpenChange={(_, d) => !d.open && onClose()}>
      <DialogSurface>
        <DialogBody>
          <DialogTitle>{t("remove.title", { name: pst.displayName })}</DialogTitle>
          <DialogContent>
            <p>{t("remove.body", { n: formatNumber(pst.messageCount, i18n.language) })}</p>
            {error ? (
              <MessageBar intent="error">
                <MessageBarBody>{formatAppError(error, t)}</MessageBarBody>
              </MessageBar>
            ) : null}
          </DialogContent>
          <DialogActions>
            <Button onClick={onClose}>{t("common:action.cancel")}</Button>
            <Button appearance="primary" onClick={confirm} disabled={busy} data-testid="remove-confirm">
              {t("remove.confirm")}
            </Button>
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
}
```

`src/screens/import/index.tsx`:
```tsx
import {
  Badge, Body1, Button, Caption1, DataGrid, DataGridBody, DataGridCell, DataGridHeader, DataGridHeaderCell, DataGridRow,
  ProgressBar, Subtitle2, TableCellLayout, Title3, createTableColumn, makeStyles, mergeClasses, tokens,
  type BadgeProps, type TableColumnDefinition,
} from "@fluentui/react-components";
import { AddRegular, DeleteRegular, DocumentRegular, ErrorCircleRegular, MailInboxRegular, WarningRegular } from "@fluentui/react-icons";
import { useInfiniteQuery, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../../api/commands";
import { pickPstFiles } from "../../api/dialog";
import type { ImportStatus } from "../../api/bindings/ImportStatus";
import type { PstSource } from "../../api/bindings/PstSource";
import { useAppStatus } from "../../app/AppStatusContext";
import {
  EmptyState, formatAppError, formatBytes, formatDateTime, formatNumber, useAppToast,
} from "../../components/common";
import { RemoveDialog } from "./RemoveDialog";

const IMPORTED: ImportStatus[] = ["completed", "failed", "cancelled", "removing"];
const QUEUED: ImportStatus[] = ["queued", "running", "cancelling", "cancelled", "failed"];
const BADGE: Record<ImportStatus, BadgeProps["color"]> = {
  queued: "informative", running: "brand", cancelling: "warning", cancelled: "subtle",
  failed: "danger", completed: "success", removing: "warning",
};

const useStyles = makeStyles({
  root: { display: "grid", gridTemplateRows: "auto 1fr", gridTemplateColumns: "1.6fr 1fr 1fr", height: "100%", minHeight: 0 },
  cmdbar: {
    gridColumn: "1 / -1", display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS,
    padding: `${tokens.spacingVerticalS} ${tokens.spacingHorizontalL}`, backgroundColor: tokens.colorNeutralBackground1,
    borderBottom: `1px solid ${tokens.colorNeutralStroke2}`,
  },
  spacer: { flex: 1 },
  stat: { display: "flex", flexDirection: "column", alignItems: "flex-end", paddingLeft: tokens.spacingHorizontalL },
  statValue: { fontWeight: tokens.fontWeightSemibold, fontSize: tokens.fontSizeBase400 },
  pane: {
    display: "flex", flexDirection: "column", minWidth: 0, minHeight: 0, overflow: "hidden",
    backgroundColor: tokens.colorNeutralBackground1, borderLeft: `1px solid ${tokens.colorNeutralStroke2}`,
  },
  paneHead: {
    display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS, minHeight: "44px", boxSizing: "border-box",
    padding: `0 ${tokens.spacingHorizontalM}`, borderBottom: `1px solid ${tokens.colorNeutralStroke2}`,
  },
  scroll: { flex: 1, overflow: "auto", minHeight: 0 },
  full: { gridColumn: "1 / -1" },
  path: { fontFamily: tokens.fontFamilyMonospace, color: tokens.colorNeutralForeground3, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", display: "block", maxWidth: "280px" },
  card: {
    display: "flex", flexDirection: "column", gap: tokens.spacingVerticalXS, padding: tokens.spacingHorizontalM,
    borderBottom: `1px solid ${tokens.colorNeutralStroke3}`, cursor: "pointer", position: "relative",
  },
  selected: { backgroundColor: tokens.colorBrandBackground2, boxShadow: `inset 3px 0 0 ${tokens.colorCompoundBrandStroke}` },
  row: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS },
  name: { flex: 1, fontWeight: tokens.fontWeightSemibold, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" },
  sub: { display: "flex", gap: tokens.spacingHorizontalM, color: tokens.colorNeutralForeground3, fontSize: tokens.fontSizeBase200 },
  warn: { color: tokens.colorStatusWarningForeground1 },
  err: { color: tokens.colorStatusDangerForeground1 },
  logEntry: {
    display: "grid", gridTemplateColumns: "auto 20px 1fr", gap: `${tokens.spacingVerticalXS} ${tokens.spacingHorizontalS}`,
    padding: `${tokens.spacingVerticalS} ${tokens.spacingHorizontalM}`, borderBottom: `1px solid ${tokens.colorNeutralStroke3}`,
  },
  logLoc: { gridColumn: "3", color: tokens.colorNeutralForeground3, fontSize: tokens.fontSizeBase200 },
});

export function ImportScreen() {
  const s = useStyles();
  const { t, i18n } = useTranslation("import");
  const lng = i18n.language;
  const status = useAppStatus();
  const qc = useQueryClient();
  const toast = useAppToast();
  const psts = useQuery({ queryKey: ["pstList"], queryFn: () => api.pstList() });
  const db = useQuery({ queryKey: ["dbStatus"], queryFn: () => api.dbStatus(), refetchInterval: 10_000 });
  const [selected, setSelected] = useState<number | null>(null);
  const [removing, setRemoving] = useState<PstSource | null>(null);
  const list = psts.data ?? [];

  const refresh = () => qc.invalidateQueries({ queryKey: ["pstList"] });
  const addFiles = async () => {
    const paths = await pickPstFiles();
    if (paths.length === 0) return;
    try {
      const results = await api.importEnqueue(paths);
      for (const r of results)
        if (r.result === "rejected") toast.notify(t("enqueue.rejected", { path: r.path, reason: formatAppError(r.error, t) }), "error");
      await refresh();
    } catch (e) {
      toast.error(e);
    }
  };
  const run = (f: (id: number) => Promise<unknown>, id: number) => async () => {
    try {
      await f(id);
      await refresh();
    } catch (e) {
      toast.error(e);
    }
  };

  const columns: TableColumnDefinition<PstSource>[] = [
    createTableColumn<PstSource>({
      columnId: "name",
      compare: (a, b) => a.displayName.localeCompare(b.displayName),
      renderHeaderCell: () => t("imported.name"),
      renderCell: (p) => (
        <TableCellLayout media={<DocumentRegular />}>
          <strong>{p.displayName}</strong>
          <span className={s.path} title={p.path}>
            {p.path}
          </span>
        </TableCellLayout>
      ),
    }),
    createTableColumn<PstSource>({
      columnId: "size",
      compare: (a, b) => a.fileSizeBytes - b.fileSizeBytes,
      renderHeaderCell: () => t("imported.size"),
      renderCell: (p) => formatBytes(p.fileSizeBytes, lng),
    }),
    createTableColumn<PstSource>({
      columnId: "messages",
      compare: (a, b) => a.messageCount - b.messageCount,
      renderHeaderCell: () => t("imported.messages"),
      renderCell: (p) => formatNumber(p.messageCount, lng),
    }),
    createTableColumn<PstSource>({
      columnId: "new",
      compare: (a, b) => a.newMessageCount - b.newMessageCount,
      renderHeaderCell: () => t("imported.new"),
      renderCell: (p) => formatNumber(p.newMessageCount, lng),
    }),
    createTableColumn<PstSource>({
      columnId: "date",
      compare: (a, b) => (a.finishedAt ?? a.queuedAt).localeCompare(b.finishedAt ?? b.queuedAt),
      renderHeaderCell: () => t("imported.date"),
      renderCell: (p) => formatDateTime(p.finishedAt ?? p.queuedAt, lng),
    }),
    createTableColumn<PstSource>({
      columnId: "status",
      compare: (a, b) => a.status.localeCompare(b.status),
      renderHeaderCell: () => t("imported.status"),
      renderCell: (p) => (
        <span className={s.row}>
          <Badge appearance="tint" color={BADGE[p.status]} data-testid="pst-status">
            {t(`status.${p.status}`)}
          </Badge>
          {p.errorCount > 0 ? (
            <WarningRegular className={s.warn} aria-label={t("queue.errors", { n: formatNumber(p.errorCount, lng) })} />
          ) : null}
        </span>
      ),
    }),
    createTableColumn<PstSource>({
      columnId: "remove",
      renderHeaderCell: () => "",
      renderCell: (p) => (
        <Button
          appearance="subtle"
          icon={<DeleteRegular />}
          aria-label={t("imported.remove", { name: p.displayName })}
          disabled={p.status === "removing"}
          data-testid="pst-remove"
          onClick={(e) => {
            e.stopPropagation();
            setRemoving(p);
          }}
        />
      ),
    }),
  ];

  const stat = (value: string, label: string) => (
    <span className={s.stat}>
      <span className={s.statValue}>{value}</span>
      <Caption1>{label}</Caption1>
    </span>
  );

  return (
    <div className={s.root}>
      <div className={s.cmdbar}>
        <Title3 as="h2">{t("title")}</Title3>
        <Button appearance="primary" icon={<AddRegular />} onClick={addFiles} data-testid="import-add">
          {t("add")}
        </Button>
        <span className={s.spacer} />
        {stat(formatNumber(status.counts.messages, lng), t("stats.messages"))}
        {stat(formatNumber(status.counts.psts, lng), t("stats.psts"))}
        {stat(db.data ? formatBytes(db.data.databaseSizeBytes, lng) : t("stats.unknown"), t("stats.size"))}
      </div>
      {psts.isSuccess && list.length === 0 ? (
        <div className={mergeClasses(s.pane, s.full)}>
          <EmptyState
            icon={<MailInboxRegular />}
            title={t("empty.title")}
            body={t("empty.body")}
            action={
              <Button appearance="primary" icon={<AddRegular />} onClick={addFiles}>
                {t("add")}
              </Button>
            }
          />
        </div>
      ) : (
        <>
          <section className={s.pane} aria-label={t("imported.title")}>
            <div className={s.paneHead}>
              <Subtitle2 as="h3">{t("imported.title")}</Subtitle2>
            </div>
            <div className={s.scroll}>
              <DataGrid items={list.filter((p) => IMPORTED.includes(p.status))} columns={columns} getRowId={(p) => p.id} sortable>
                <DataGridHeader>
                  <DataGridRow>{({ renderHeaderCell }) => <DataGridHeaderCell>{renderHeaderCell()}</DataGridHeaderCell>}</DataGridRow>
                </DataGridHeader>
                <DataGridBody<PstSource>>
                  {({ item, rowId }) => (
                    <DataGridRow<PstSource>
                      key={rowId}
                      data-testid="pst-row"
                      data-pst-name={item.displayName}
                      onClick={() => setSelected(item.id)}
                    >
                      {({ renderCell }) => <DataGridCell>{renderCell(item)}</DataGridCell>}
                    </DataGridRow>
                  )}
                </DataGridBody>
              </DataGrid>
            </div>
          </section>
          <section className={s.pane} aria-label={t("queue.title")}>
            <div className={s.paneHead}>
              <Subtitle2 as="h3">{t("queue.title")}</Subtitle2>
            </div>
            <div className={s.scroll} role="listbox" aria-label={t("queue.title")}>
              {list.filter((p) => QUEUED.includes(p.status)).length === 0 ? (
                <EmptyState icon={<MailInboxRegular />} title={t("queue.empty")} body={t("queue.emptyBody")} />
              ) : (
                list
                  .filter((p) => QUEUED.includes(p.status))
                  .map((p) => (
                    <QueueCard
                      key={p.id}
                      pst={p}
                      selected={selected === p.id}
                      onSelect={() => setSelected(p.id)}
                      onCancel={run(api.importCancel, p.id)}
                      onResume={run(api.importResume, p.id)}
                      onRemove={() => setRemoving(p)}
                    />
                  ))
              )}
            </div>
          </section>
          <ErrorLog pst={list.find((p) => p.id === selected) ?? null} />
        </>
      )}
      {removing ? <RemoveDialog pst={removing} onClose={() => setRemoving(null)} /> : null}
    </div>
  );
}

function QueueCard(props: {
  pst: PstSource;
  selected: boolean;
  onSelect(): void;
  onCancel(): void;
  onResume(): void;
  onRemove(): void;
}) {
  const s = useStyles();
  const { t, i18n } = useTranslation("import");
  const lng = i18n.language;
  const { pst } = props;
  const p = pst.progress;
  const total = p?.itemsTotal ?? null;
  const done = p?.itemsDone ?? 0;
  const percent = total ? Math.round((done / total) * 100) : 0;
  const running = pst.status === "running";
  return (
    <div
      role="option"
      aria-selected={props.selected}
      tabIndex={0}
      data-testid="queue-card"
      data-pst-name={pst.displayName}
      className={mergeClasses(s.card, props.selected && s.selected)}
      onClick={props.onSelect}
      onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && props.onSelect()}
    >
      <div className={s.row}>
        <DocumentRegular />
        <span className={s.name}>{pst.displayName}</span>
        <Badge appearance="tint" color={BADGE[pst.status]}>
          {t(`status.${pst.status}`)}
        </Badge>
      </div>
      {running && p ? <Caption1>{t(`phase.${p.phase}`)}</Caption1> : null}
      {running || p ? (
        <ProgressBar
          value={running && total === null ? undefined : total ? done / total : 0}
          color={pst.status === "failed" ? "error" : "brand"}
          aria-label={pst.displayName}
        />
      ) : null}
      <div className={s.sub}>
        {total ? <span>{t("queue.progress", { done: formatNumber(done, lng), total: formatNumber(total, lng), percent })}</span> : null}
        {running && p?.bytesPerSec ? <span>{t("queue.rate", { rate: formatBytes(p.bytesPerSec, lng) })}</span> : null}
        {running && p?.etaSeconds ? <span>{t("queue.eta", { minutes: Math.max(1, Math.round(p.etaSeconds / 60)) })}</span> : null}
        {pst.errorCount > 0 ? (
          <span className={s.warn}>
            <WarningRegular /> {t("queue.errors", { n: formatNumber(pst.errorCount, lng) })}
          </span>
        ) : null}
      </div>
      <div className={s.row} onClick={(e) => e.stopPropagation()}>
        {pst.status === "running" || pst.status === "queued" || pst.status === "cancelling" ? (
          <Button size="small" disabled={pst.status === "cancelling"} onClick={props.onCancel} data-testid="queue-cancel">
            {t("queue.cancel")}
          </Button>
        ) : null}
        {pst.status === "cancelled" ? (
          <Button size="small" appearance="primary" onClick={props.onResume} data-testid="queue-resume">
            {t("queue.resume")}
          </Button>
        ) : null}
        {pst.status === "failed" ? (
          <Button size="small" appearance="primary" onClick={props.onResume} data-testid="queue-retry">
            {t("queue.retry")}
          </Button>
        ) : null}
        {pst.status === "cancelled" || pst.status === "failed" ? (
          <Button size="small" appearance="subtle" icon={<DeleteRegular />} onClick={props.onRemove}>
            {t("queue.remove")}
          </Button>
        ) : null}
      </div>
    </div>
  );
}

function ErrorLog({ pst }: { pst: PstSource | null }) {
  const s = useStyles();
  const { t, i18n } = useTranslation("import");
  const errors = useInfiniteQuery({
    queryKey: ["importErrors", pst?.id ?? null],
    queryFn: ({ pageParam }) => api.importErrors({ pstId: pst!.id, cursor: pageParam, limit: 100 }),
    initialPageParam: null as string | null,
    getNextPageParam: (last) => last.nextCursor ?? undefined,
    enabled: pst !== null,
  });
  const rows = errors.data?.pages.flatMap((p) => p.rows) ?? [];
  return (
    <section className={s.pane} aria-label={t("log.title")}>
      <div className={s.paneHead}>
        <Subtitle2 as="h3">{t("log.title")}</Subtitle2>
        {pst ? <Caption1>— {pst.displayName}</Caption1> : null}
      </div>
      <div className={s.scroll}>
        {pst === null ? (
          <EmptyState icon={<DocumentRegular />} title={t("log.pick")} />
        ) : errors.isSuccess && rows.length === 0 ? (
          <EmptyState icon={<DocumentRegular />} title={t("log.none")} />
        ) : (
          rows.map((r) => (
            <div key={r.id} className={s.logEntry}>
              <Caption1>{formatDateTime(r.occurredAt, i18n.language)}</Caption1>
              {r.severity === "jobFailed" ? <ErrorCircleRegular className={s.err} /> : <WarningRegular className={s.warn} />}
              <Body1>
                {t(`stage.${r.stage}`)} · {r.message}
              </Body1>
              <span className={s.logLoc}>
                {[r.folderPath, r.pstItemId !== null ? t("log.item", { id: r.pstItemId }) : null].filter(Boolean).join(" · ")}
              </span>
            </div>
          ))
        )}
        {errors.hasNextPage ? (
          <Button appearance="subtle" onClick={() => void errors.fetchNextPage()}>
            {t("log.more")}
          </Button>
        ) : null}
      </div>
    </section>
  );
}
```
Run: `npx vitest run src/screens/import`
Expected: PASS (5 tests).

- [ ] **Step 16: Commit the Import screen.**
```bash
printf 'feat(ui): import screen with queue, error log and removal dialog\n\nCo-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>\n' > scripts/tmp/msg-r106-3.txt
git add src/screens/import
git commit -F scripts/tmp/msg-r106-3.txt -- src/screens/import
git status --porcelain
```
Expected: commit succeeds; no `src/screens/import` path listed afterwards.

- [ ] **Step 17: Write the failing Settings test** `src/screens/settings/settings.test.tsx`:
```tsx
import { fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api/commands");
vi.mock("../../api/dialog");
vi.mock("../../api/events", async () => (await import("../../app/testing")).eventsMock);

import { api } from "../../api/commands";
import { pickFolder } from "../../api/dialog";
import { makeDbStatus, mockApiDefaults, renderWithProviders } from "../../app/testing";
import { SettingsScreen, isValidQuality } from ".";

async function typeQuality(value: string) {
  const input = await screen.findByRole("spinbutton");
  fireEvent.change(input, { target: { value } });
  fireEvent.blur(input);
  fireEvent.click(screen.getByTestId("settings-webp-apply"));
}

describe("Settings screen", () => {
  beforeEach(() => mockApiDefaults());

  it("WebP quality outside 1-100 is not sent", async () => {
    renderWithProviders(<SettingsScreen />);
    for (const bad of ["0", "101", "50.5"]) {
      await typeQuality(bad);
      expect(await screen.findByText("Enter a whole number from 1 to 100.")).toBeTruthy();
    }
    expect(api.settingsUpdate).not.toHaveBeenCalled();
    await typeQuality("75");
    await waitFor(() => expect(api.settingsUpdate).toHaveBeenCalledWith({ language: null, webpQuality: 75 }));
    expect(isValidQuality(1) && isValidQuality(100) && !isValidQuality(Number.NaN)).toBe(true);
  });

  it("maintenance buttons run tasks and are disabled while one runs", async () => {
    renderWithProviders(<SettingsScreen />);
    fireEvent.click(await screen.findByTestId("maint-vacuum"));
    await waitFor(() => expect(api.maintenanceRun).toHaveBeenCalledWith("vacuum"));
  });

  it("disables maintenance while a task is running", async () => {
    vi.mocked(api.dbStatus).mockResolvedValue(
      makeDbStatus({ maintenance: { task: "reindex", state: "running", startedAt: "2026-10-01T00:00:00Z", finishedAt: null, error: null } }),
    );
    renderWithProviders(<SettingsScreen />);
    await waitFor(() => expect((screen.getByTestId("maint-analyze") as HTMLButtonElement).disabled).toBe(true));
  });

  it("moves the data folder after confirmation", async () => {
    vi.mocked(pickFolder).mockResolvedValue("/Volumes/Big/Outlooker");
    renderWithProviders(<SettingsScreen />);
    fireEvent.click(await screen.findByTestId("settings-move"));
    fireEvent.click(await screen.findByRole("button", { name: "Move" }));
    await waitFor(() => expect(api.dataDirMove).toHaveBeenCalledWith("/Volumes/Big/Outlooker"));
  });
});
```
Run: `npx vitest run src/screens/settings`
Expected: FAIL — `isValidQuality` is not exported by the kickoff stub / `Unable to find role="spinbutton"`.

Then add the frame-level move tests to `src/app/AppFrame.test.tsx` (from step 11). Change its first import line to `import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";`, add `import { pickFolder } from "../api/dialog";` after `import { api } from "../api/commands";`, and append at the end of the file:
```tsx
describe("data-folder move", () => {
  beforeEach(() => {
    eventsMock.reset();
    mockApiDefaults();
    vi.mocked(pickFolder).mockResolvedValue("/Volumes/Big/Outlooker");
  });

  async function startMove() {
    render(<App />);
    fireEvent.click(await screen.findByTestId("nav-settings"));
    fireEvent.click(await screen.findByTestId("settings-move"));
    fireEvent.click(await screen.findByRole("button", { name: "Move" }));
    await waitFor(() => expect(api.dataDirMove).toHaveBeenCalledWith("/Volumes/Big/Outlooker"));
  }
  const progress = (phase: string, error: unknown = null) => ({ phase, bytesCopied: 512, bytesTotal: 1024, error });

  it("keeps the move dialog through stoppingDb, startingDb and ready and toasts done", async () => {
    await startMove();
    act(() => eventsMock.emit("app:status-changed", makeStatus({ phase: "stoppingDb" })));
    act(() => eventsMock.emit("data-dir:move-progress", progress("copying")));
    expect(await screen.findByText("512 B of 1.0 KB")).toBeTruthy();
    expect(screen.queryByTestId("blocking-screen")).toBeNull();
    act(() => eventsMock.emit("app:status-changed", makeStatus({ phase: "startingDb" })));
    expect(screen.queryByTestId("blocking-screen")).toBeNull();
    expect(screen.getByText("512 B of 1.0 KB")).toBeTruthy();
    act(() => eventsMock.emit("app:status-changed", makeStatus({ phase: "ready" })));
    act(() => eventsMock.emit("data-dir:move-progress", progress("done")));
    expect(await screen.findByText("The data folder was moved.")).toBeTruthy();
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("shows a failed move's error after the database is back in the old folder", async () => {
    await startMove();
    act(() => eventsMock.emit("app:status-changed", makeStatus({ phase: "stoppingDb" })));
    act(() =>
      eventsMock.emit("data-dir:move-progress", progress("failed", { kind: "fileAccess", path: "/Volumes/Big/Outlooker", reason: "notWritable" })),
    );
    act(() => eventsMock.emit("app:status-changed", makeStatus({ phase: "startingDb" })));
    expect(screen.queryByTestId("blocking-screen")).toBeNull();
    act(() => eventsMock.emit("app:status-changed", makeStatus({ phase: "ready" })));
    expect(await screen.findByText("Cannot access /Volumes/Big/Outlooker.")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(screen.getByTestId("title-bar")).toBeTruthy();
  });
});
```
Run: `npx vitest run src/app/AppFrame.test.tsx -t "data-folder move"`
Expected: FAIL — the kickoff Settings stub renders nothing (`Unable to find an element by: [data-testid="settings-move"]`). Against a `MoveDialog` that does not set the move session, the frame would swap in `blocking-screen` at `stoppingDb` and `findByText("512 B of 1.0 KB")` would fail.

- [ ] **Step 18: Write `src/screens/settings/MoveDialog.tsx` and `src/screens/settings/index.tsx`.**

`src/screens/settings/MoveDialog.tsx`:
```tsx
import {
  Body1, Button, Caption1, Dialog, DialogActions, DialogBody, DialogContent, DialogSurface, DialogTitle, MessageBar,
  MessageBarBody, ProgressBar,
} from "@fluentui/react-components";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../../api/commands";
import type { AppError } from "../../api/bindings/AppError";
import type { DataDirMoveProgress } from "../../api/bindings/DataDirMoveProgress";
import { formatAppError, formatBytes, toAppError, useAppToast } from "../../components/common";
import { setMoveSession, useMoveSession } from "./moveSession";

/**
 * Progress arrives as `data-dir:move-progress`; boot writes it into the ["moveProgress"] query (Part 03 §8.2).
 * "Started" is the frame-level move session, so AppFrame keeps this dialog mounted while the phase leaves ready.
 */
export function MoveDialog({ target, onClose }: { target: string; onClose(): void }) {
  const { t, i18n } = useTranslation("settings");
  const lng = i18n.language;
  const qc = useQueryClient();
  const toast = useAppToast();
  const started = useMoveSession() !== null;
  const [error, setError] = useState<AppError | null>(null);
  const progress = useQuery<DataDirMoveProgress | null>({
    queryKey: ["moveProgress"],
    queryFn: () => null,
    enabled: false,
    initialData: null,
  });
  const p = started ? progress.data : null;

  const close = useCallback(() => {
    setMoveSession(qc, null);
    onClose();
  }, [qc, onClose]);

  useEffect(() => {
    if (p?.phase === "done") {
      toast.notify(t("move.done"));
      void qc.invalidateQueries({ queryKey: ["settings"] });
      close();
    }
  }, [p?.phase, toast, t, qc, close]);

  const confirm = async () => {
    qc.setQueryData(["moveProgress"], null);
    setError(null);
    // Set before the call: the phase may leave ready before data_dir_move returns.
    setMoveSession(qc, { target });
    try {
      await api.dataDirMove(target);
    } catch (e) {
      setMoveSession(qc, null);
      setError(toAppError(e));
    }
  };
  const blocking = started && p?.phase !== "failed";

  return (
    <Dialog open modalType={blocking ? "alert" : "modal"} onOpenChange={(_, d) => !d.open && !blocking && close()}>
      <DialogSurface>
        <DialogBody>
          <DialogTitle>{t("move.title")}</DialogTitle>
          <DialogContent>
            {!started ? <Body1>{t("move.body", { target })}</Body1> : null}
            {started ? (
              <>
                <Body1>{t(`move.phase.${p?.phase ?? "stoppingDb"}`)}</Body1>
                <ProgressBar value={p && p.bytesTotal > 0 ? p.bytesCopied / p.bytesTotal : undefined} />
                {p && p.bytesTotal > 0 ? (
                  <Caption1>{t("move.progress", { copied: formatBytes(p.bytesCopied, lng), total: formatBytes(p.bytesTotal, lng) })}</Caption1>
                ) : null}
              </>
            ) : null}
            {error ? (
              <MessageBar intent="error">
                <MessageBarBody>{formatAppError(error, t)}</MessageBarBody>
              </MessageBar>
            ) : null}
            {p?.phase === "failed" && p.error ? (
              <MessageBar intent="error">
                <MessageBarBody>{formatAppError(p.error, t)}</MessageBarBody>
              </MessageBar>
            ) : null}
          </DialogContent>
          <DialogActions>
            {!started ? (
              <>
                <Button onClick={onClose}>{t("common:action.cancel")}</Button>
                <Button appearance="primary" onClick={confirm}>
                  {t("move.confirm")}
                </Button>
              </>
            ) : null}
            {started && p?.phase === "failed" ? <Button onClick={close}>{t("common:action.close")}</Button> : null}
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
}
```

`src/screens/settings/index.tsx`:
```tsx
import {
  Badge, Body1, Button, Caption1, Card, DataGrid, DataGridBody, DataGridCell, DataGridHeader, DataGridHeaderCell,
  DataGridRow, Field, Input, Radio, RadioGroup, Slider, SpinButton, Spinner, Subtitle2, Tab, TabList, Title3,
  createTableColumn, makeStyles, mergeClasses, tokens, type TableColumnDefinition,
} from "@fluentui/react-components";
import { DatabaseRegular, FolderRegular, GlobeRegular, ImageRegular, MailInboxRegular, WrenchRegular } from "@fluentui/react-icons";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useRef, useState, type JSX, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../../api/commands";
import { pickFolder } from "../../api/dialog";
import type { IndexStatus } from "../../api/bindings/IndexStatus";
import type { Language } from "../../api/bindings/Language";
import type { MaintenanceTask } from "../../api/bindings/MaintenanceTask";
import { useAppStatus } from "../../app/AppStatusContext";
import {
  EmptyState, formatBytes, formatDateTime, formatNumber, useAppToast, useLanguageSwitch,
} from "../../components/common";
import { MoveDialog } from "./MoveDialog";

export function isValidQuality(v: number): boolean {
  return Number.isInteger(v) && v >= 1 && v <= 100;
}

type SectionId = "storage" | "language" | "content" | "db" | "index";
const SECTIONS: SectionId[] = ["storage", "language", "content", "db", "index"];

const useStyles = makeStyles({
  root: { display: "grid", gridTemplateColumns: "220px 1fr", height: "100%", minHeight: 0 },
  nav: {
    backgroundColor: tokens.colorNeutralBackground1, borderRight: `1px solid ${tokens.colorNeutralStroke2}`,
    padding: `${tokens.spacingVerticalM} ${tokens.spacingHorizontalS}`, display: "flex", flexDirection: "column", gap: tokens.spacingVerticalS,
  },
  body: {
    overflow: "auto", padding: tokens.spacingHorizontalL, display: "grid", gridTemplateColumns: "repeat(3, 1fr)",
    gap: tokens.spacingHorizontalL, alignContent: "start",
  },
  card: { display: "flex", flexDirection: "column", gap: tokens.spacingVerticalM, padding: tokens.spacingHorizontalL },
  wide2: { gridColumn: "span 2" },
  wide3: { gridColumn: "1 / -1" },
  head: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS },
  row: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS, flexWrap: "wrap" },
  grow: { flex: 1 },
  mono: { fontFamily: tokens.fontFamilyMonospace },
  muted: { color: tokens.colorNeutralForeground3 },
  facts: { display: "grid", gridTemplateColumns: "max-content 1fr", gap: `${tokens.spacingVerticalXS} ${tokens.spacingHorizontalL}`, margin: 0 },
  dd: { margin: 0, fontVariantNumeric: "tabular-nums" },
  ends: { display: "flex", justifyContent: "space-between" },
});

export function SettingsScreen() {
  const s = useStyles();
  const { t, i18n } = useTranslation("settings");
  const lng = i18n.language;
  const status = useAppStatus();
  const qc = useQueryClient();
  const toast = useAppToast();
  const setLanguage = useLanguageSwitch();
  const settings = useQuery({ queryKey: ["settings"], queryFn: () => api.settingsGet() });
  const db = useQuery({ queryKey: ["dbStatus"], queryFn: () => api.dbStatus(), refetchInterval: 10_000 });
  const [draft, setDraft] = useState<number | null>(null);
  const [invalid, setInvalid] = useState(false);
  const [moveTarget, setMoveTarget] = useState<string | null>(null);
  const cards = useRef<Partial<Record<SectionId, HTMLElement | null>>>({});
  const quality = draft ?? settings.data?.webpQuality ?? 80;
  const running = db.data?.maintenance?.state === "running" ? db.data.maintenance : null;

  const applyQuality = async () => {
    if (!isValidQuality(quality)) {
      setInvalid(true);
      return;
    }
    setInvalid(false);
    try {
      qc.setQueryData(["settings"], await api.settingsUpdate({ language: null, webpQuality: quality }));
      setDraft(null);
      toast.notify(t("webp.saved"));
    } catch (e) {
      toast.error(e);
    }
  };
  const runTask = async (task: MaintenanceTask) => {
    try {
      await api.maintenanceRun(task);
      toast.notify(t("maintenance.started", { task: t(`maintenance.task.${task}`) }));
      await qc.invalidateQueries({ queryKey: ["dbStatus"] });
    } catch (e) {
      toast.error(e);
    }
  };
  const chooseTarget = async () => {
    const dir = await pickFolder();
    if (dir) setMoveTarget(dir);
  };

  const card = (id: SectionId, icon: JSX.Element, children: ReactNode, wide?: string) => (
    <Card className={mergeClasses(s.card, wide)} ref={(el) => void (cards.current[id] = el)} aria-labelledby={`set-${id}`}>
      <div className={s.head}>
        {icon}
        <Subtitle2 as="h2" id={`set-${id}`}>
          {t(`section.${id}`)}
        </Subtitle2>
      </div>
      {children}
    </Card>
  );
  const fact = (label: string, value: string) => (
    <>
      <dt className={s.muted}>{label}</dt>
      <dd className={s.dd}>{value}</dd>
    </>
  );
  const columns: TableColumnDefinition<IndexStatus>[] = [
    createTableColumn<IndexStatus>({ columnId: "name", renderHeaderCell: () => t("index.name"), renderCell: (x) => <span className={s.mono}>{x.name}</span> }),
    createTableColumn<IndexStatus>({ columnId: "table", renderHeaderCell: () => t("index.table"), renderCell: (x) => x.table }),
    createTableColumn<IndexStatus>({ columnId: "size", renderHeaderCell: () => t("index.size"), renderCell: (x) => formatBytes(x.sizeBytes, lng) }),
    createTableColumn<IndexStatus>({
      columnId: "state",
      renderHeaderCell: () => t("index.state"),
      renderCell: (x) => (
        <Badge appearance="tint" color={x.valid ? "success" : "warning"}>
          {x.valid ? t("index.healthy") : t("index.rebuild")}
        </Badge>
      ),
    }),
  ];

  return (
    <div className={s.root}>
      <nav className={s.nav} aria-label={t("title")}>
        <Title3 as="h1">{t("title")}</Title3>
        <TabList vertical onTabSelect={(_, d) => cards.current[d.value as SectionId]?.scrollIntoView({ block: "start" })}>
          {SECTIONS.map((id) => (
            <Tab key={id} value={id}>
              {t(`section.${id}`)}
            </Tab>
          ))}
        </TabList>
      </nav>
      <div className={s.body}>
        {card(
          "storage",
          <FolderRegular />,
          <>
            <Field label={t("storage.folder")}>
              <div className={s.row}>
                <Input className={mergeClasses(s.grow, s.mono)} readOnly value={settings.data?.dataDir ?? status.dataDir} />
                <Button onClick={chooseTarget} data-testid="settings-move">
                  {t("storage.change")}
                </Button>
              </div>
            </Field>
            <Caption1 className={s.muted}>{t("storage.help")}</Caption1>
          </>,
          s.wide2,
        )}
        {card(
          "language",
          <GlobeRegular />,
          <>
            <RadioGroup value={lng === "zh-TW" ? "zh-TW" : "en"} onChange={(_, d) => void setLanguage(d.value as Language)}>
              <Radio value="zh-TW" label={t("common:language.zh-TW")} data-testid="settings-language-zh-TW" />
              <Radio value="en" label={t("common:language.en")} data-testid="settings-language-en" />
            </RadioGroup>
            <Caption1 className={s.muted}>{t("language.help")}</Caption1>
          </>,
        )}
        {card(
          "content",
          <ImageRegular />,
          <>
            <Field label={t("webp.label")} validationMessage={invalid ? t("webp.invalid") : undefined} validationState={invalid ? "error" : "none"}>
              <div className={s.row}>
                <Slider
                  className={s.grow}
                  min={1}
                  max={100}
                  step={1}
                  value={isValidQuality(quality) ? quality : 80}
                  onChange={(_, d) => setDraft(d.value)}
                />
                <SpinButton
                  min={1}
                  max={100}
                  value={Number.isFinite(quality) ? quality : null}
                  onChange={(_, d) => setDraft(d.value ?? (d.displayValue !== undefined ? Number(d.displayValue) : Number.NaN))}
                />
              </div>
            </Field>
            <div className={s.ends}>
              <Caption1 className={s.muted}>{t("webp.smaller")}</Caption1>
              <Caption1 className={s.muted}>{t("webp.sharper")}</Caption1>
            </div>
            <div className={s.row}>
              <Button onClick={applyQuality} data-testid="settings-webp-apply">
                {t("webp.apply")}
              </Button>
              <Caption1 className={s.muted}>{t("webp.help")}</Caption1>
            </div>
          </>,
        )}
        {card(
          "db",
          <DatabaseRegular />,
          db.data ? (
            <>
              <dl className={s.facts}>
                {fact(t("db.status"), t(`common:phase.${db.data.phase}`))}
                {fact(t("db.engine"), t("db.engineValue", { version: db.data.postgresVersion }))}
                {fact(t("db.messages"), formatNumber(db.data.counts.messages, lng))}
                {fact(t("db.psts"), formatNumber(db.data.counts.psts, lng))}
                {fact(t("db.images"), formatNumber(db.data.imageCount, lng))}
                {fact(t("db.attachments"), formatNumber(db.data.attachmentCount, lng))}
                {fact(t("db.size"), formatBytes(db.data.databaseSizeBytes, lng))}
                {fact(t("db.lastVacuum"), db.data.lastVacuumAt ? formatDateTime(db.data.lastVacuumAt, lng) : t("db.never"))}
                {fact(t("db.lastAnalyze"), db.data.lastAnalyzeAt ? formatDateTime(db.data.lastAnalyzeAt, lng) : t("db.never"))}
              </dl>
              {status.counts.messages === 0 ? (
                <EmptyState icon={<MailInboxRegular />} title={t("db.emptyTitle")} body={t("db.emptyBody")} />
              ) : null}
            </>
          ) : (
            <Body1 className={s.muted}>{db.isError ? t("db.unavailable") : ""}</Body1>
          ),
        )}
        {card(
          "index",
          <WrenchRegular />,
          <>
            <DataGrid items={db.data?.indexes ?? []} columns={columns} getRowId={(x) => x.name}>
              <DataGridHeader>
                <DataGridRow>{({ renderHeaderCell }) => <DataGridHeaderCell>{renderHeaderCell()}</DataGridHeaderCell>}</DataGridRow>
              </DataGridHeader>
              <DataGridBody<IndexStatus>>
                {({ item, rowId }) => (
                  <DataGridRow<IndexStatus> key={rowId}>{({ renderCell }) => <DataGridCell>{renderCell(item)}</DataGridCell>}</DataGridRow>
                )}
              </DataGridBody>
            </DataGrid>
            <div className={s.row}>
              {(["analyze", "vacuum", "reindex"] as const).map((task) => (
                <Button key={task} disabled={running !== null} onClick={() => void runTask(task)} data-testid={`maint-${task}`}>
                  {t(`index.${task}`)}
                </Button>
              ))}
              {running ? <Spinner size="tiny" label={t("maintenance.running", { task: t(`maintenance.task.${running.task}`) })} /> : null}
              <Caption1 className={s.muted}>{t("index.help")}</Caption1>
            </div>
          </>,
          s.wide3,
        )}
      </div>
      {moveTarget ? <MoveDialog target={moveTarget} onClose={() => setMoveTarget(null)} /> : null}
    </div>
  );
}
```
Run: `npx vitest run src/screens/settings src/app/AppFrame.test.tsx`
Expected: `settings.test.tsx` 4 PASS; `AppFrame.test.tsx` 5 PASS (incl. both `data-folder move` tests) and `shows onboarding while setup is needed` still FAIL until step 21.

- [ ] **Step 19: Commit the Settings screen.**
```bash
printf 'feat(ui): settings screen with move dialog, WebP quality and maintenance\n\nCo-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>\n' > scripts/tmp/msg-r106-4.txt
git add src/screens/settings src/app/AppFrame.test.tsx
git commit -F scripts/tmp/msg-r106-4.txt -- src/screens/settings src/app/AppFrame.test.tsx
git status --porcelain
```
Expected: commit succeeds; no `src/screens/settings` path listed afterwards.

- [ ] **Step 20: Write the failing Onboarding test** `src/screens/onboarding/onboarding.test.tsx`:
```tsx
import { fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api/commands");
vi.mock("../../api/dialog");
vi.mock("../../api/events", async () => (await import("../../app/testing")).eventsMock);

import { api } from "../../api/commands";
import { pickPstFiles } from "../../api/dialog";
import { mockApiDefaults, renderWithProviders } from "../../app/testing";
import { OnboardingScreen } from ".";

describe("Onboarding", () => {
  beforeEach(() => mockApiDefaults());

  it("onboarding initializes with the chosen dir and language", async () => {
    renderWithProviders(<OnboardingScreen />, { status: { phase: "needsSetup", dataDir: "/data" } });
    fireEvent.click(await screen.findByTestId("onboarding-lang-en"));
    await waitFor(() => expect(api.settingsUpdate).toHaveBeenCalledWith({ language: "en", webpQuality: null }));
    fireEvent.click(screen.getByTestId("onboarding-next"));
    fireEvent.click(await screen.findByTestId("onboarding-dir-default"));
    fireEvent.click(screen.getByTestId("onboarding-next"));
    await waitFor(() => expect(api.appInitialize).toHaveBeenCalledWith({ dataDir: "/data", language: "en" }));
    expect(await screen.findByText("Create database cluster")).toBeTruthy();
  });

  it("shows the init error and stays on the location step", async () => {
    vi.mocked(api.appInitialize).mockRejectedValue({ kind: "invalidInput", field: "dataDir", reason: "notEmpty" });
    renderWithProviders(<OnboardingScreen />, { status: { phase: "needsSetup", dataDir: "/data" } });
    fireEvent.click(await screen.findByTestId("onboarding-next"));
    fireEvent.click(screen.getByTestId("onboarding-next"));
    expect(await screen.findByText("The value of dataDir is not valid.")).toBeTruthy();
    expect(screen.getByTestId("onboarding-dir-default")).toBeTruthy();
  });

  it("step 4 enqueues the picked PST files and navigates to Import", async () => {
    vi.mocked(pickPstFiles).mockResolvedValue(["/mail/a.pst"]);
    const r = renderWithProviders(<OnboardingScreen />, { status: { phase: "ready", dataDir: "/data" } });
    // phase is ready: the wizard starts at step 3 and Next is enabled
    fireEvent.click(await screen.findByTestId("onboarding-next"));
    fireEvent.click(await screen.findByTestId("onboarding-pick-pst"));
    await waitFor(() => expect(api.importEnqueue).toHaveBeenCalledWith(["/mail/a.pst"]));
    await waitFor(() => expect(r.go).toHaveBeenCalledWith("import"));
  });
});
```
Run: `npx vitest run src/screens/onboarding`
Expected: FAIL — the kickoff stub renders `null` (`Unable to find an element by: [data-testid="onboarding-lang-en"]`).

- [ ] **Step 21: Write `src/screens/onboarding/index.tsx`.**
```tsx
import {
  Body1, Button, Caption1, Card, MessageBar, MessageBarBody, ProgressBar, Subtitle1, Title2, Title3,
  makeStyles, mergeClasses, tokens,
} from "@fluentui/react-components";
import {
  CheckmarkCircleRegular, CircleRegular, CopyRegular, ImageRegular, LockClosedRegular, SearchRegular,
} from "@fluentui/react-icons";
import { useState, type JSX } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../../api/commands";
import { pickFolder, pickPstFiles } from "../../api/dialog";
import type { AppError } from "../../api/bindings/AppError";
import type { Language } from "../../api/bindings/Language";
import { useAppStatus, useNavigation } from "../../app/AppStatusContext";
import { formatAppError, toAppError, useAppToast, useLanguageSwitch } from "../../components/common";

type Step = 0 | 1 | 2 | 3;
const STEP_KEYS = ["language", "location", "database", "import"] as const;

const useStyles = makeStyles({
  root: { display: "grid", gridTemplateColumns: "340px 1fr 440px", height: "100vh", backgroundColor: tokens.colorNeutralBackground1 },
  side: {
    backgroundColor: tokens.colorNeutralBackground2, padding: tokens.spacingHorizontalXXL, display: "flex",
    flexDirection: "column", gap: tokens.spacingVerticalL, overflow: "auto",
  },
  left: { borderRight: `1px solid ${tokens.colorNeutralStroke2}` },
  right: { borderLeft: `1px solid ${tokens.colorNeutralStroke2}` },
  steps: { listStyle: "none", margin: 0, padding: 0, display: "flex", flexDirection: "column", gap: tokens.spacingVerticalS },
  stepItem: { display: "grid", gridTemplateColumns: "28px 1fr", gap: tokens.spacingHorizontalM, alignItems: "center", color: tokens.colorNeutralForeground3 },
  stepCurrent: { color: tokens.colorNeutralForeground1, fontWeight: tokens.fontWeightSemibold },
  num: {
    width: "28px", height: "28px", borderRadius: tokens.borderRadiusCircular, display: "grid", placeItems: "center",
    border: `1px solid ${tokens.colorNeutralStroke1}`, fontSize: tokens.fontSizeBase200,
  },
  numCurrent: { backgroundColor: tokens.colorBrandBackground, color: tokens.colorNeutralForegroundOnBrand, border: "none" },
  body: { display: "flex", flexDirection: "column", gap: tokens.spacingVerticalL, padding: "48px", overflow: "auto" },
  lead: { color: tokens.colorNeutralForeground3, maxWidth: "720px" },
  choices: { display: "grid", gridTemplateColumns: "repeat(2, minmax(0, 360px))", gap: tokens.spacingHorizontalL },
  choice: { padding: tokens.spacingHorizontalL, gap: tokens.spacingVerticalXS },
  mono: { fontFamily: tokens.fontFamilyMonospace, color: tokens.colorNeutralForeground3, wordBreak: "break-all" },
  checklist: { listStyle: "none", margin: 0, padding: 0, display: "flex", flexDirection: "column", gap: tokens.spacingVerticalS },
  check: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS },
  done: { color: tokens.colorStatusSuccessForeground1 },
  foot: { display: "flex", gap: tokens.spacingHorizontalS, marginTop: "auto", maxWidth: "740px" },
  spacer: { flex: 1 },
  feat: { display: "grid", gridTemplateColumns: "40px 1fr", gap: `2px ${tokens.spacingHorizontalM}` },
  featIcon: {
    gridRow: "span 2", width: "40px", height: "40px", borderRadius: tokens.borderRadiusLarge, display: "grid",
    placeItems: "center", backgroundColor: tokens.colorBrandBackground2, color: tokens.colorBrandForeground1, fontSize: "20px",
  },
});

export function OnboardingScreen() {
  const s = useStyles();
  const { t, i18n } = useTranslation("onboarding");
  const status = useAppStatus();
  const { go } = useNavigation();
  const toast = useAppToast();
  const setLanguage = useLanguageSwitch();
  const [step, setStep] = useState<Step>(status.phase === "needsSetup" ? 0 : 2);
  const [dirChoice, setDirChoice] = useState<"default" | "other">("default");
  const [otherDir, setOtherDir] = useState<string | null>(null);
  const [initError, setInitError] = useState<AppError | null>(null);
  const [busy, setBusy] = useState(false);
  const lang: Language = i18n.language === "zh-TW" ? "zh-TW" : "en";
  const dataDir = dirChoice === "default" ? status.dataDir : otherDir;
  const created = !["needsSetup", "initializingCluster"].includes(status.phase);
  const ready = status.phase === "ready";

  const next = async () => {
    if (step === 0) setStep(1);
    else if (step === 1 && dataDir) {
      setBusy(true);
      setInitError(null);
      try {
        await api.appInitialize({ dataDir, language: lang });
        setStep(2);
      } catch (e) {
        setInitError(toAppError(e));
      } finally {
        setBusy(false);
      }
    } else if (step === 2 && ready) setStep(3);
  };
  const chooseOther = async () => {
    const dir = await pickFolder();
    if (dir) {
      setOtherDir(dir);
      setDirChoice("other");
    }
  };
  const pickPst = async () => {
    const paths = await pickPstFiles();
    if (paths.length === 0) return;
    try {
      const results = await api.importEnqueue(paths);
      for (const r of results)
        if (r.result === "rejected") toast.notify(t("import:enqueue.rejected", { path: r.path, reason: formatAppError(r.error, t) }), "error");
      go("import");
    } catch (e) {
      toast.error(e);
    }
  };

  const choice = (testId: string, selected: boolean, onClick: () => void, title: string, hint: string, extra?: JSX.Element) => (
    <Card
      className={s.choice}
      onClick={onClick}
      aria-pressed={selected}
      data-testid={testId}
      appearance={selected ? "filled-alternative" : "outline"}
    >
      <Subtitle1>{title}</Subtitle1>
      <Caption1>{hint}</Caption1>
      {extra}
    </Card>
  );

  let content: JSX.Element;
  if (step === 0)
    content = (
      <>
        <Title3 as="h2">{t("language.heading")}</Title3>
        <Body1 className={s.lead}>{t("language.body")}</Body1>
        <div className={s.choices}>
          {choice("onboarding-lang-zh-TW", lang === "zh-TW", () => void setLanguage("zh-TW"), t("common:language.zh-TW"), t("language.zhHint"))}
          {choice("onboarding-lang-en", lang === "en", () => void setLanguage("en"), t("common:language.en"), t("language.enHint"))}
        </div>
      </>
    );
  else if (step === 1)
    content = (
      <>
        <Title3 as="h2">{t("location.heading")}</Title3>
        <Body1 className={s.lead}>{t("location.body")}</Body1>
        <div className={s.choices}>
          {choice("onboarding-dir-default", dirChoice === "default", () => setDirChoice("default"), t("location.default"), t("location.defaultHint"),
            <span className={s.mono}>{status.dataDir}</span>)}
          {choice("onboarding-dir-other", dirChoice === "other", () => void chooseOther(), t("location.other"), t("location.otherHint"),
            otherDir ? <span className={s.mono}>{otherDir}</span> : undefined)}
        </div>
        {initError ? (
          <MessageBar intent="error">
            <MessageBarBody>{formatAppError(initError, t)}</MessageBarBody>
          </MessageBar>
        ) : null}
      </>
    );
  else if (step === 2)
    content = (
      <>
        <Title3 as="h2">{t("database.heading")}</Title3>
        <Body1 className={s.lead}>{t("database.body")}</Body1>
        <ProgressBar value={ready ? 1 : undefined} thickness="large" />
        <ul className={s.checklist}>
          {[["database.create", created], ["database.start", ready]].map(([key, ok]) => (
            <li key={String(key)} className={s.check}>
              {ok ? <CheckmarkCircleRegular className={s.done} /> : <CircleRegular />}
              <Body1>{t(String(key))}</Body1>
            </li>
          ))}
        </ul>
        {ready ? <Body1 data-testid="onboarding-db-ready">{t("database.ready")}</Body1> : null}
      </>
    );
  else
    content = (
      <>
        <Title3 as="h2">{t("import.heading")}</Title3>
        <Body1 className={s.lead}>{t("import.body")}</Body1>
        <div>
          <Button appearance="primary" size="large" onClick={pickPst} data-testid="onboarding-pick-pst">
            {t("import.pick")}
          </Button>
        </div>
      </>
    );

  const feature = (icon: JSX.Element, title: string, body: string) => (
    <div className={s.feat}>
      <span className={s.featIcon} aria-hidden>
        {icon}
      </span>
      <strong>{title}</strong>
      <Caption1>{body}</Caption1>
    </div>
  );

  return (
    <div className={s.root}>
      <aside className={mergeClasses(s.side, s.left)}>
        <Title2 as="h1">{t("title")}</Title2>
        <ol className={s.steps}>
          {STEP_KEYS.map((key, i) => (
            <li key={key} className={mergeClasses(s.stepItem, i === step && s.stepCurrent)} aria-current={i === step ? "step" : undefined}>
              <span className={mergeClasses(s.num, i === step && s.numCurrent)}>
                {i < step ? <CheckmarkCircleRegular /> : i + 1}
              </span>
              <span>{t(`step.${key}`)}</span>
            </li>
          ))}
        </ol>
        <Caption1>{t("note")}</Caption1>
      </aside>
      <main className={s.body}>
        {content}
        <div className={s.foot}>
          {step === 1 ? (
            <Button onClick={() => setStep(0)} data-testid="onboarding-back">
              {t("nav.back")}
            </Button>
          ) : null}
          <span className={s.spacer} />
          {step === 3 ? (
            <Button appearance="subtle" onClick={() => go("search")} data-testid="onboarding-skip">
              {t("import.later")}
            </Button>
          ) : (
            <Button
              appearance="primary"
              onClick={next}
              disabled={busy || (step === 1 && !dataDir) || (step === 2 && !ready)}
              data-testid="onboarding-next"
            >
              {t("nav.next")}
            </Button>
          )}
        </div>
      </main>
      <aside className={mergeClasses(s.side, s.right)}>
        <Subtitle1 as="h2">{t("feature.title")}</Subtitle1>
        {feature(<SearchRegular />, t("feature.search"), t("feature.searchBody"))}
        {feature(<CopyRegular />, t("feature.dedupe"), t("feature.dedupeBody"))}
        {feature(<ImageRegular />, t("feature.render"), t("feature.renderBody"))}
        {feature(<LockClosedRegular />, t("feature.local"), t("feature.localBody"))}
      </aside>
    </div>
  );
}
```
Back is shown only on step 1: after `app_initialize` the cluster exists, so going back to the language or location step would offer choices that no longer apply.

Run: `npx vitest run src/screens/onboarding src/app/AppFrame.test.tsx`
Expected: PASS (3 + 6 tests).

- [ ] **Step 22: Write the global tests** `src/app/global.test.tsx`:
```tsx
import { screen, waitFor } from "@testing-library/react";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import type { ReactElement } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api/commands");
vi.mock("../api/dialog");
vi.mock("../api/events", async () => (await import("./testing")).eventsMock);

import { api } from "../api/commands";
import type { AppStatus } from "../api/bindings/AppStatus";
import { AdvancedSqlScreen } from "../screens/advanced-sql";
import { ImportScreen } from "../screens/import";
import { OnboardingScreen } from "../screens/onboarding";
import { SearchScreen } from "../screens/search";
import { SettingsScreen } from "../screens/settings";
import { FailedScreen } from "./AppFrame";
import { NAMESPACES, RESOURCES } from "./i18n";
import { eventsMock, makeDbStatus, makePst, mockApiDefaults, renderWithProviders } from "./testing";

function flatten(obj: Record<string, unknown>, prefix = ""): string[] {
  return Object.entries(obj).flatMap(([k, v]) =>
    v !== null && typeof v === "object" ? flatten(v as Record<string, unknown>, `${prefix}${k}.`) : [`${prefix}${k}`],
  );
}

function files(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const p = join(dir, name);
    return statSync(p).isDirectory() ? files(p) : [p];
  });
}

const KEY = /^[A-Za-z0-9_.:-]+$/;
const NUMERIC = /^[\d\s.,:%+\-—·/]*(?:\s?(?:B|KB|MB|GB|TB)(?:\/s)?)?$/;
const SCREENS: [string, ReactElement, Partial<AppStatus>][] = [
  ["search", <SearchScreen />, {}],
  ["advancedSql", <AdvancedSqlScreen />, {}],
  ["import", <ImportScreen />, {}],
  ["settings", <SettingsScreen />, {}],
  ["onboarding", <OnboardingScreen />, { phase: "needsSetup" }],
  ["failed", <FailedScreen error={{ kind: "dbNotReady" }} onRetry={async () => null} />, { phase: "failed" }],
];

async function settle(r: ReturnType<typeof renderWithProviders>) {
  await waitFor(() => expect(r.queryClient.isFetching()).toBe(0));
}

describe("global UI rules", () => {
  beforeEach(() => {
    eventsMock.reset();
    mockApiDefaults();
  });

  it("locale files: zh-TW and en have equal key sets per namespace", () => {
    const expected = NAMESPACES.map((n) => `${n}.json`).sort();
    expect(readdirSync("src/locales/zh-TW").sort()).toEqual(expected);
    expect(readdirSync("src/locales/en").sort()).toEqual(expected);
    for (const ns of NAMESPACES) {
      expect(flatten(RESOURCES["zh-TW"][ns]).sort(), ns).toEqual(flatten(RESOURCES.en[ns]).sort());
    }
  });

  it("screens rendered in cimode show only keys", async () => {
    for (const [name, el, status] of SCREENS) {
      const r = renderWithProviders(el, { lng: "cimode", status });
      await settle(r);
      const bad: string[] = [];
      const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
      for (let n = walker.nextNode(); n; n = walker.nextNode()) {
        const parent = n.parentElement?.tagName;
        if (parent === "STYLE" || parent === "SCRIPT") continue;
        const text = (n.textContent ?? "").trim();
        if (text && !KEY.test(text) && !NUMERIC.test(text)) bad.push(text);
      }
      expect(bad, name).toEqual([]);
      r.unmount();
    }
  });

  it("each screen renders its empty state", async () => {
    const imp = renderWithProviders(<ImportScreen />);
    expect(await screen.findByText("No PST imported yet")).toBeTruthy();
    imp.unmount();
    const set = renderWithProviders(<SettingsScreen />, { status: { counts: { messages: 0, psts: 0 } } });
    expect(await screen.findByText("No mail imported yet")).toBeTruthy();
    set.unmount();
    renderWithProviders(<OnboardingScreen />, { status: { phase: "needsSetup" } });
    expect(await screen.findByText("Pick your interface language")).toBeTruthy();
  });

  it("no dark theme tokens in src", () => {
    const banned = ["web" + "DarkTheme", "teams" + "DarkTheme", "prefers-color-" + "scheme"];
    const hits = files("src").filter((f) => {
      const text = readFileSync(f, "utf8");
      return banned.some((b) => text.includes(b));
    });
    expect(hits).toEqual([]);
  });

  it("no out-of-scope controls", async () => {
    const bannedKey = /^(pause|pauseAll|workers?|reencode|reEncode|format|highlight)/i;
    for (const lng of ["zh-TW", "en"] as const)
      for (const ns of NAMESPACES) {
        const keys = flatten(RESOURCES[lng][ns]).filter((k) => k.split(".").some((seg) => bannedKey.test(seg)));
        expect(keys, `${lng}/${ns}`).toEqual([]);
      }
    vi.mocked(api.pstList).mockResolvedValue([
      makePst({ id: 1, status: "running", displayName: "a" }),
      makePst({ id: 2, status: "cancelled", displayName: "b" }),
      makePst({ id: 3, status: "failed", displayName: "c" }),
      makePst({ id: 4, status: "completed", displayName: "d" }),
    ]);
    vi.mocked(api.dbStatus).mockResolvedValue(
      makeDbStatus({ indexes: [{ name: "messages_body_trgm", table: "messages", sizeBytes: 1024, valid: true }] }),
    );
    const bannedLabel = /\b(pause|pause all|workers?|re-?encode|format|highlight)\b/i;
    for (const el of [<ImportScreen />, <SettingsScreen />]) {
      const r = renderWithProviders(el);
      await settle(r);
      const offenders = [...document.body.querySelectorAll("*")].filter((node) => {
        const own = [...node.childNodes].filter((c) => c.nodeType === Node.TEXT_NODE).map((c) => c.textContent ?? "").join(" ");
        return bannedLabel.test(own) || bannedLabel.test(node.getAttribute("aria-label") ?? "");
      });
      expect(offenders.map((o) => o.outerHTML.slice(0, 80))).toEqual([]);
      r.unmount();
    }
  });
});
```
Run: `npx vitest run src/app/global.test.tsx`
Expected: PASS (5 tests). Until R1-07 lands its screens, `SearchScreen` / `AdvancedSqlScreen` are the kickoff stubs (render nothing); after R1-07 lands, this file covers them too — R1-07 runs it as its own acceptance.

- [ ] **Step 23: Full verification for the task.**
Run: `npm run typecheck`
Expected: exit 0.
Run: `npm test -- src/app src/components/common src/screens/onboarding src/screens/import src/screens/settings`
Expected: every test file passes (common 3, boot 3, AppFrame 6, global 5, import 5, settings 4, onboarding 3).
Run: `grep -rln "@tauri-apps" src/app src/components src/screens; echo "rc=$?"`
Expected: no file listed, `rc=1`.

- [ ] **Step 24: Commit onboarding and the global tests.**
```bash
printf 'feat(ui): onboarding wizard and global locale, no-literal, no-dark and scope tests\n\nCo-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>\n' > scripts/tmp/msg-r106-5.txt
git add src/screens/onboarding src/app/global.test.tsx
git commit -F scripts/tmp/msg-r106-5.txt -- src/screens/onboarding src/app/global.test.tsx
git status --porcelain
```
Expected: commit succeeds; `git status --porcelain` lists no R1-06 path.


## Task R1-07: Mail components, Search screen, Advanced SQL screen

**Files:**
- Create: `src/components/mail/pages.ts`, `src/components/mail/MailList.tsx`, `src/components/mail/ReadingPane.tsx`, `src/components/mail/MailPreview.tsx`, `src/components/mail/AttachmentCard.tsx`, `src/components/mail/index.ts`, `src/components/mail/fixtures.ts` (test helpers), `src/components/mail/pages.test.tsx`, `src/components/mail/mail.test.tsx`
- Create: `src/components/dsl/model.ts`, `src/components/dsl/DslBuilder.tsx`, `src/components/dsl/index.ts`, `src/components/dsl/dsl.test.tsx`
- Create: `src/components/sql-editor/SqlEditor.tsx`, `src/components/sql-editor/schema.ts`, `src/components/sql-editor/index.ts`, `src/components/sql-editor/sql-editor.test.tsx`
- Create: `src/screens/search/index.tsx` (replaces stub), `src/screens/search/RefinePane.tsx`, `src/screens/search/compose.ts`, `src/screens/search/compose.test.ts`, `src/screens/search/search.test.tsx`
- Create: `src/screens/advanced-sql/index.tsx` (replaces stub), `src/screens/advanced-sql/SavedQueries.tsx`, `src/screens/advanced-sql/ResultTable.tsx`, `src/screens/advanced-sql/advanced-sql.test.tsx`
- Create: `src/locales/zh-TW/{search,sql}.json`, `src/locales/en/{search,sql}.json`

**Interfaces:**
- Consumes (C1): `SearchQuery`, `ConditionGroup`, `ConditionNode`, `GroupOp`, `RecipientKind`, `SearchRequest`, `SearchPage`, `SearchCount`, `MailRow`, `Address`, `FilterOptions`, `PstRef`, `FolderRef`, `MailDetail`, `AttachmentMeta`, `Occurrence`, `SavedQuery`, `SavedQueryInput`, `SqlExecuteRequest`, `SqlResult`, `ColumnMeta`, `AppError`, `LibraryChanged` from `src/api/bindings/`. `CellValue` is `string | null` (ts-rs output of `Option<String>`).
- Consumes (C4): `api.searchMail(req: SearchRequest): Promise<SearchPage>`, `api.searchCount(query: SearchQuery): Promise<SearchCount>`, `api.filterOptions(): Promise<FilterOptions>`, `api.mailGet(id: number): Promise<MailDetail>`, `api.savedQueryList(): Promise<SavedQuery[]>`, `api.savedQueryCreate(input: SavedQueryInput): Promise<SavedQuery>`, `api.savedQueryUpdate(id: number, input: SavedQueryInput): Promise<SavedQuery>`, `api.savedQueryDelete(id: number): Promise<null>`, `api.sqlExecute(req: SqlExecuteRequest): Promise<SqlResult>`, `api.sqlCancel(executionId: string): Promise<null>`; `onEvent('library:changed', handler)`.
- Consumes (C10, R1-06): `EmptyState`, `formatAppError`, `toAppError`, `formatNumber`, `formatDateTime`, `formatDay`, `formatBytes` from `src/components/common`; `useAppStatus`, `useNavigation` from `src/app/AppStatusContext`; test helpers `renderWithProviders`, `eventsMock`, `mockApiDefaults`, `makeStatus` from `src/app/testing`.
- Consumes (R1-01, planner-1 request P1-7): `src/test/setup.ts` stubs `Range.prototype.getClientRects` / `getBoundingClientRect` when jsdom lacks them (CodeMirror measures text through Range rects; `global.test.tsx` and `sql-editor.test.tsx` mount the real editor).
- Produces (internal to R1-07; R2-14 may import none of these, it uses the `data-testid`s):
  - `src/components/mail/pages.ts`: `const PAGE_SIZE = 200`, `const MAX_LIST_ROWS = 100_000`, `function loadedRows(pages: SearchPage[]): number`, `function nextPageParam(last: SearchPage, pages: SearchPage[]): string | undefined` (undefined when `last.nextCursor` is null or `loadedRows(pages) >= MAX_LIST_ROWS`), `function useMailPages(query: SearchQuery | null, seq: number): MailPages` with `interface MailPages { rows: MailRow[]; hasMore: boolean; capped: boolean; shortQuery: boolean; isLoading: boolean; isFetchingNextPage: boolean; error: AppError | null; fetchNextPage(): Promise<unknown>; reset(): void }` (`fetchNextPage` calls TanStack's with `cancelRefetch: false`, so a call during a running page fetch joins it` (`rows` is cut at `MAX_LIST_ROWS`; `capped` = `loadedRows >= MAX_LIST_ROWS` and the last page still had a `nextCursor`).
  - `src/components/mail/MailList.tsx`: `const ROW_HEIGHT = 32`, `const PREFETCH_ROWS = 50`, `type Entry = { kind: 'month'; key: string; label: string } | { kind: 'mail'; key: string; row: MailRow }`, `function buildEntries(rows: MailRow[], lng: string, undatedLabel: string): Entry[]`, `interface MailListProps { rows: MailRow[]; selectedId: number | null; onSelect(id: number): void; hasMore: boolean; capped: boolean; loadingMore: boolean; onEndReached(): void; ariaLabel: string; monthHeaders?: boolean }`, `function MailList(p: MailListProps): JSX.Element`.
  - `src/components/mail/ReadingPane.tsx`: `function ReadingPane(p: { mailId: number | null }): JSX.Element`.
  - `src/components/mail/MailPreview.tsx`: `function MailPreview(p: { html: string; title: string }): JSX.Element`.
  - `src/components/mail/AttachmentCard.tsx`: `function AttachmentCard(p: { attachment: AttachmentMeta }): JSX.Element`.
  - `src/components/mail/fixtures.ts` (test-only): `function stubViewport(height: number): () => void` (defines `clientHeight` / `clientWidth` on `HTMLElement.prototype` for jsdom so the virtualizer renders rows; returns the restore function), `function makeMailRow(id: number, p?: Partial<MailRow>): MailRow`, `function makeMailDetail(p?: Partial<MailDetail>): MailDetail`.
  - `src/components/dsl/model.ts`: `type FieldKind = 'sender' | 'recipient' | 'subject' | 'body' | 'dateRange' | 'folder' | 'hasAttachment' | 'sourcePst'`, `interface CondDraft { kind: 'cond'; id: string; field: FieldKind; text: string; kinds: RecipientKind[]; fromDay: string; toDay: string; folderPath: string; includeSubfolders: boolean; flag: boolean; pstIds: number[] }`, `interface GroupDraft { kind: 'group'; id: string; op: GroupOp; children: Array<CondDraft | GroupDraft> }`, `const FIELDS: readonly FieldKind[]`, `const MAX_BUILDER_DEPTH = 4`, `const MAX_LEAVES = 50`, `function newGroup(op?: GroupOp, children?: Array<CondDraft | GroupDraft>): GroupDraft` (default children: one empty sender condition), `function newCond(field?: FieldKind): CondDraft`, `function isComplete(c: CondDraft): boolean`, `function countLeaves(g: GroupDraft): number` (complete conditions only), `function depthOf(g: GroupDraft): number`, `function localDayStart(day: string): Date`, `function toInstant(d: Date): string` (`YYYY-MM-DDTHH:MM:SSZ`), `function toConditionGroup(g: GroupDraft, dayStart?: (day: string) => Date): ConditionGroup | null`, `function updateNode(g: GroupDraft, id: string, f: (n: CondDraft | GroupDraft) => CondDraft | GroupDraft | null): GroupDraft`, `interface FolderTreeNode { path: string; name: string; count: number; children: FolderTreeNode[] }`, `function buildFolderTree(folders: FolderRef[]): FolderTreeNode[]`.
  - `src/components/dsl/DslBuilder.tsx`: `function DslBuilder(p: { value: GroupDraft; onChange(next: GroupDraft): void; options: FilterOptions; leafBudget: number }): JSX.Element`.
  - `src/components/sql-editor/schema.ts`: `const VIEW_COLUMNS: Record<string, ReadonlyArray<readonly [name: string, pgType: string]>>` (the six views of Part 04 §8.2 with their columns), `const SQL_SCHEMA: Record<string, string[]>` (derived: view → column names, the `@codemirror/lang-sql` `SQLNamespace`).
  - `src/components/sql-editor/SqlEditor.tsx`: `interface SqlEditorProps { value: string; onChange(sql: string): void; onRun(): void; errorPosition: number | null; ariaLabel: string }`, `function SqlEditor(p: SqlEditorProps): JSX.Element`, `function codePointOffsetToUtf16(text: string, position1: number): number`.
  - `src/screens/search/compose.ts`: `interface RefineState { pstId: number | null; folderPath: string | null; hasAttachment: boolean; last90Days: boolean }`, `const EMPTY_REFINE: RefineState`, `function termGroup(term: string): ConditionNode`, `type ComposeResult = { query: SearchQuery | null; leaves: number; problem: null | 'tooManyTerms' | 'tooManyConditions' }`, `function composeQuery(text: string, refine: RefineState, builder: ConditionGroup | null, now?: Date): ComposeResult`, `function refineNodes(r: RefineState, now: Date): ConditionNode[]`, `function splitTerms(text: string): string[]`.
  - `data-testid` values (for R2-14): `search-box` (the `input`), `dsl-toggle`, `result-count`, `mail-list`, `mail-row` (with `data-mail-id`), `result-cap-row`, `list-loader-row`, `reading-pane`, `mail-preview` (the iframe), `short-query-hint`, `new-mail-bar`, `new-mail-refresh`, `sql-editor` (the CodeMirror `.cm-content`), `sql-run`, `sql-cancel`, `sql-save`, `sql-new`, `sql-result` (with `data-mode` = `mailList` | `table` | `command`), `sql-table-row`, `sql-row-detail`, `sql-truncated`, `saved-query` (with `data-name`).

**Behavior:**
- **Mail paging (`pages.ts`).** `useInfiniteQuery({ queryKey: ['searchMail', seq, query], queryFn: ({ pageParam }) => api.searchMail({ query, cursor: pageParam, pageSize: PAGE_SIZE }), initialPageParam: null as string | null, getNextPageParam: nextPageParam, enabled: query !== null })`. 500 pages × 200 rows = 100,000, so the hook stops requesting exactly at the cap (Part 03 §5.2). `shortQuery` = the first page's `shortQuery`. `reset()` = `queryClient.resetQueries({ queryKey: ['searchMail', seq] })`.
- **MailList (prototype `.mail-item`, one line at 32 px).** Scroll container `role="listbox"` with `aria-label`; `useVirtualizer({ count: entries.length, estimateSize: () => ROW_HEIGHT, overscan: 10, getScrollElement })`; each virtual item is absolutely positioned (`transform: translateY(start)`, height 32). Month header entries (when `monthHeaders` is true, default) show the local month (`Intl.DateTimeFormat(lng, { year: 'numeric', month: 'long' })`; rows with `sentAt === null` go under `search:list.undated`), style `Caption1Strong` on `tokens.colorNeutralBackground2`. Mail entry (`role="option"`, `aria-selected`): grid `24px minmax(120px,22%) 1fr auto`: 24 px `Avatar` (initials of `from.name ?? from.address`), sender (semibold, ellipsis), subject (`tokens.colorBrandForeground2`) + ` — ` + snippet (`tokens.colorNeutralForeground3`) in one ellipsis line, paperclip icon when `hasAttachments`, date (`formatDay`), `occurrenceCount > 1` → small `CounterBadge`. Selected row: `tokens.colorBrandBackground2` and a 3 px left bar `tokens.colorCompoundBrandStroke`. Keyboard: ArrowUp / ArrowDown on the listbox select the previous / next mail and scroll it into view (`scrollToIndex`). After the virtual block: a loader row (`list-loader-row`, `Spinner size="tiny"`) while `hasMore` and not `capped`; the cap row (`result-cap-row`, text `search:resultCap`) when `capped`. Fetch trigger: an effect on the last virtual item index — when `lastIndex >= entries.length - 1 - PREFETCH_ROWS && hasMore && !loadingMore && !capped` → `onEndReached()`. Rows are never re-sorted by the client.
- **ReadingPane (prototype `.preview`).** `useQuery({ queryKey: ['mailGet', mailId], queryFn: () => api.mailGet(mailId), enabled: mailId !== null })`. `mailId === null` → `EmptyState` (`search:preview.pickTitle`, `search:preview.pickBody`). Error → `MessageBar intent="error"` with `formatAppError`. Loaded: header block (max 45 % of the pane, own scroll): subject (`Title3`; `search:preview.noSubject` when empty), meta row (`Persona` with a 40 px avatar, `from.name` and `from.address` as secondary text, date `formatDateTime(sentAt)`), To / Cc / Bcc lines (`name <address>` joined by `, `; a line only when non-empty), "Found in" `Badge`s, one per occurrence (`pstDisplayName · folderPath`), attachments as `AttachmentCard`s (small `Card`: file-type tile with the upper-cased extension, file name, `formatBytes(sizeBytes)` · `mimeType`, a disabled download `Button` wrapped in `Tooltip` `search:preview.downloadLater` — the Tooltip wraps a `span` because disabled buttons get no pointer events). Body fills the rest: `MailPreview` with `bodyHtml` when not null, else a `pre` (`white-space: pre-wrap`, `fontFamilyBase`) with `bodyText`. Root `data-testid="reading-pane"`.
- **MailPreview.** `<iframe data-testid="mail-preview" sandbox="" srcDoc={html} referrerPolicy="no-referrer" title={title} />` filling the area (border 0, white background). The element keeps a stable React key, so changing `html` replaces `srcdoc` on the same iframe (no new element). Never adds an `allow-*` token.
- **DSL model.** Field → node: `sender|subject|body` → `{ type, value: text.trim() }`; `recipient` → `{ type: 'recipient', value, kinds }`; `dateRange` → `{ type: 'dateRange', from: fromDay ? toInstant(dayStart(fromDay)) : null, to: toDay ? toInstant(dayStart(toDay + 1 day)) : null }` (the UI "to" day is inclusive; the contract `to` is exclusive); `folder` → `{ type: 'folder', folderPath, includeSubfolders }`; `hasAttachment` → `{ type: 'hasAttachment', value: flag }`; `sourcePst` → `{ type: 'sourcePst', pstIds }`. Incomplete conditions (empty text, both days empty, empty folder, no PST) and groups left empty are dropped; a root with no children → `null`. Text inputs have `maxLength={500}`. `dayStart` defaults to `localDayStart` (`new Date(y, m - 1, d)`), so local calendar days become RFC 3339 instants (Part 03 §1 Timestamps).
- **DslBuilder (prototype `.dsl`, `.dsl-group`, `.cond`).** Root bar: `search:dsl.between` + AND/OR `ToggleButton` pair (root `op`), "Add group", "Clear all". Each group = `Card appearance="outline"` with a 3 px brand left border: header "Group n · Match" + AND/OR toggle + remove icon; children separated by an AND/OR label; condition row grid `150px 92px 1fr 28px`: field `Dropdown` (8 fields), operator caption (`search:dsl.op.contains|between|is|in`), value control(s), remove icon. Value controls: text → `Input`; recipient → `Input` + three small `ToggleButton`s To / Cc / Bcc (none pressed = all kinds); dateRange → two `Input type="date"`; folder → `Dropdown` of distinct folder paths (indented by depth) + `Checkbox` "include subfolders"; hasAttachment → `Dropdown` Yes / No; sourcePst → `Dropdown multiselect` of PSTs (`displayName`). "Add condition" is disabled when `countLeaves(root) >= leafBudget`; "Add group" inside a group is shown only while that group's depth `< MAX_BUILDER_DEPTH` (root depth 1; the composed query adds one level, so the sent tree never exceeds depth 5).
- **Query composition (`compose.ts`).** `terms` = `text.trim().split(/\s+/)` without empty strings. More than 10 terms → `problem: 'tooManyTerms'`, `query: null`. Refine conditions: `pstId` → `sourcePst [pstId]`; `folderPath` → `folder { folderPath, includeSubfolders: true }`; `hasAttachment` → `hasAttachment true`; `last90Days` → `dateRange { from: toInstant(localDayStart(today − 90 days)), to: null }`. No refine condition and no builder tree: `terms` empty → `query: null`; else `{ kind: 'simple', text: text.trim() }`. Otherwise `{ kind: 'dsl', root: { op: 'and', children: [...terms.map(termGroup), ...refineNodes, ...(builder ? [{ type: 'group', op: builder.op, children: builder.children }] : [])] } }`. `termGroup(t)` = `{ type: 'group', op: 'or', children: [sender t, recipient t kinds [], subject t, body t] }` (the Part 03 §5.1 simple-query rule). `leaves` = 4 × terms + refine count + builder leaves; `> 50` → `problem: 'tooManyConditions'`, `query: null`.
- **Search screen (prototype `#search`; grid `260px 1fr`).** Refine pane (`Subtitle2` "Refine"; `Tree`: "All mail" + one item per PST from `filterOptions.psts`; folder tree from `buildFolderTree(filterOptions.folders)` with `CounterBadge` = summed `messageCount`; quick filters `Checkbox` "Has attachment", "Last 90 days"). Main column: search bar row (`SearchBox` placeholder `search:box.placeholder`, max width 880 px; `ToggleButton` "Conditions" + `CounterBadge` (builder leaves) + chevron; summary `search:dsl.summary`), collapsible DSL panel (`DslBuilder`, `leafBudget = 50 − 4 × terms − refine count`), hint row (`MessageBar intent="warning"` `search:shortQueryHint` when the first page has `shortQuery: true`; `MessageBar intent="error"` for `problem` keys `search:problem.tooManyTerms|tooManyConditions`; `MessageBar intent="info"` "New mail imported" with Refresh when `newMail`), results grid `minmax(420px, 34%) 1fr` (list pane with head "Results" + count, then `MailList`; reading pane). Typing is debounced 300 ms; each change of the composed query (compared by `JSON.stringify`) increments a local sequence number `seq`; both `searchMail` (via `useMailPages`) and `useQuery(['searchCount', seq, query], () => api.searchCount(query))` use it, so a response for an older sequence is never shown. Count text: `capped` → `search:countCapped` with `n` = `formatNumber(10000)` (renders "10,000+ messages"), else `search:count` with `n` = `formatNumber(count)`; while loading or after a `searchCount` error (e.g. `queryTimeout`) no count is shown and the list still works. The first row is selected automatically when a result arrives and nothing in it is selected. States: `counts.messages === 0` → `EmptyState` `search:empty.title` / `search:empty.body` + "Go to Import" (`go('import')`) in the results area; `query === null` and no problem → `EmptyState` `search:idle.title`; zero rows after load → `EmptyState` `search:zero.title` / `search:zero.body` + "Clear search and conditions" (resets text, refine, builder); `searchMail` error → `MessageBar intent="error"` `formatAppError`. `library:changed` (subscribed with `onEvent` in an effect, unsubscribed on unmount) → `invalidateQueries({ queryKey: ['filterOptions'] })` and, when a query is shown, `newMail = true`; it never refetches `searchMail`. Refresh → `pages.reset()` and `newMail = false`.
- **Advanced SQL screen (prototype `#sql`, `#sql:table`; CSS grid `1fr 3fr 1fr 3fr`).** Column 1: head "Saved queries" + "New query" icon `Button` (`sql-new`: clears editor, deselects); list of saved queries (`useQuery(['savedQueries'], api.savedQueryList)`, sorted by the core; selecting loads its SQL into the editor); `Accordion` "Schema" listing `VIEW_COLUMNS` (view name, then `column type` lines). Column 2: `Toolbar` with Run (`sql-run`, primary, `Play` icon, disabled while executing or the editor is blank), Cancel (`sql-cancel`, enabled only while executing → `api.sqlCancel(executionId)`), Save (`sql-save`: selected saved query → `savedQueryUpdate(id, { name, sql })`; none → a `Dialog` asking for the name → `savedQueryCreate`; `invalidInput` with `field: 'name', reason: 'duplicate'` → `sql:save.duplicate` in the dialog), Delete (only when a saved query is selected; confirm `Dialog` → `savedQueryDelete`), spacer, read-only `Badge` (lock icon, `sql:readOnly`); `SqlEditor` (Mod-Enter runs); status bar (`Caption1`): success → `sql:status.ok` with rows and `elapsedMs`, plus the mode label. Run: `executionId = crypto.randomUUID()`, `api.sqlExecute({ executionId, sql, maxRows: 10000 })` through `useMutation`. Errors: `sqlError` → `MessageBar intent="error"` with `message` verbatim and `sql:error.position` when `position` is set; the editor marks that character; `cancelled`, `queryTimeout`, `readOnlyViolation`, `invalidInput` → `MessageBar` with `formatAppError`. Column 3 (`sql-result`, `data-mode`): `mailList` → head "Mail list" + count, `MailList` (no month headers, `hasMore` false), `missingIds > 0` → caption `sql:result.missing`; `table` → head "Result table" + count, `ResultTable`; `command` → `MessageBar intent="success"` `sql:result.command` with `tag`. `truncated: true` → `MessageBar intent="warning"` `sql-truncated` `sql:result.truncated` with `n` = `formatNumber(10000)` above the list/table. Column 4: mail mode → `ReadingPane` of the selected mail (first row auto-selected); table mode → head "Row detail" + `#n`, a `dl` grid (`max-content 1fr`) of column name (mono) → value (`null` → italic `sql:result.null`), `sql-row-detail`; nothing selected → `EmptyState` `sql:detail.pick`. Empty states: before the first run → column 3 `EmptyState` `sql:empty.noRunTitle` / `sql:empty.noRunBody` and column 4 `EmptyState` `search:preview.pickTitle`; when `counts.messages === 0` and no result yet → column 3 `EmptyState` `sql:empty.noMailTitle` + "Go to Import".
- **ResultTable.** Header row (sticky, `Caption1Strong`, column `name`, `pgType` as tooltip `title`) + virtualized body (`useVirtualizer`, 32 px rows, `role="row"`, `data-testid="sql-table-row"`); grid template `repeat(n, minmax(120px, 1fr))` with horizontal scroll; cells ellipsis with `title` = full value; numeric right-aligned when `pgType` ∈ {`int2`, `int4`, `int8`, `float4`, `float8`, `numeric`}; `null` → italic `sql:result.null`; clicking or Enter / Space on a row selects it (brand background).
- **SqlEditor.** One `EditorView` created in a `useEffect` with `parent` = a div ref: extensions `basicSetup` (package `codemirror`), `sql({ dialect: PostgreSQL, schema: SQL_SCHEMA, upperCaseKeywords: true })`, `Prec.highest(keymap.of([{ key: 'Mod-Enter', run: () => { onRunRef.current(); return true } }]))`, `EditorView.updateListener.of(u => u.docChanged && onChangeRef.current(u.state.doc.toString()))`, `EditorView.contentAttributes.of({ 'aria-label': ariaLabel, 'data-testid': 'sql-editor' })`, a light `EditorView.theme` (height 100 %, monospace 13 px, `.cm-sql-error` wavy red underline), and a `StateField` holding the error marker decoration (set through a `StateEffect`, mapped through edits so it never points past the document). A `value` prop that differs from the document replaces the whole document. `errorPosition` (PostgreSQL 1-based character offset) → `codePointOffsetToUtf16` → mark one character (or the last character when the offset is past the end); `null` clears the mark. The view is destroyed on unmount.
- **Locale keys required by the spec:** `search:shortQueryHint` (en "Terms shorter than 3 characters are not indexed and search slowly"), `search:resultCap` (en "Showing the first 100,000 results — refine the search").

**Constraints:**
- `@tanstack/react-virtual` `useVirtualizer` only; no other list library. The list never holds more than 100,000 mail rows.
- `MailPreview` is the only place that renders mail HTML; no `dangerouslySetInnerHTML` anywhere (`grep -rn dangerouslySetInnerHTML src` finds nothing).
- No search-term highlighting; no SQL Format button; no relevance sort select.
- CodeMirror: only `codemirror`, `@codemirror/lang-sql`, `@codemirror/state`, `@codemirror/view` imports; the wrapper is in-house.
- Advanced SQL always sends `maxRows: 10000`.
- jsdom tests that need virtual rows call `stubViewport(800)` in `beforeEach` and its restore function in `afterEach`.

**Acceptance criteria:**
- [ ] `npm run typecheck` exits 0.
- [ ] `npm test -- src/components/mail src/components/dsl src/components/sql-editor src/screens/search src/screens/advanced-sql` exits 0 with mocked commands, including the named tests:
  - `preview iframe has sandbox="" and no allow- token` (`mail.test.tsx`)
  - `with 600 mock pages of 200 rows the list stops requesting after 100,000 rows and shows the cap row` (`pages.test.tsx`: exactly 500 `searchMail` calls, `rows.length === 100000`, `capped`, and a rendered `MailList` shows `result-cap-row`)
  - `searchCount {count: 10000, capped: true} renders 10,000+ and {count: 9999, capped: false} renders 9,999` (`search.test.tsx`)
  - `DSL builder output for the Part 03 §5.1 worked example deep-equals the spec JSON` (`dsl.test.tsx`)
  - `a stale search response (older sequence) is dropped` (`search.test.tsx`)
  - `library:changed triggers a filterOptions refetch and not a list reload` (`search.test.tsx`)
  - `sqlExecute is called with maxRows: 10000` (`advanced-sql.test.tsx`)
  - `truncated: true shows the cap MessageBar` (`advanced-sql.test.tsx`)
  - `Advanced SQL shows MailList for a result with an id column and a table otherwise` (`advanced-sql.test.tsx`)
  - `clicking a table row shows its key/value list in column 4` (`advanced-sql.test.tsx`)
  - `Run is disabled while executing` (`advanced-sql.test.tsx`)
- [ ] The locale-parity, no-literal, no-dark and out-of-scope-controls tests of R1-06 pass for the `search` and `sql` namespaces (the tests live in R1-06's `src/app/` and iterate all namespaces): `npm test -- src/app/global.test.tsx` exits 0 after this task's locale files land.
- [ ] Added: `composeQuery builds simple, dsl and problem results` (`compose.test.ts`: text only → simple; text + hasAttachment → dsl with one `termGroup` per term; 11 terms → `tooManyTerms`; 10 terms + a builder group of 11 conditions = 51 leaves → `tooManyConditions`).
- [ ] Added: `preview replaces srcdoc on the same iframe` (`mail.test.tsx`).
- [ ] Added: `sqlError position marks the character in the editor` (`sql-editor.test.tsx`).
- [ ] Added: `grep -rn "dangerouslySetInnerHTML" src` finds nothing.

**Steps:**

Order: locales → DSL model and builder → query composition (commit 1) → mail components (commit 2) → SQL editor (commit 3) → Search screen (commit 4) → Advanced SQL screen + task verification (commit 5). C10 of R1-06 must have landed (its step 8) before step 3; until then the kickoff stubs of `src/components/common` and `src/app/AppStatusContext` compile but `renderWithProviders` does not exist, so do not start step 3 before the leader relays "C10 landed". Before step 1 read `docs/design/variant-1/index.html` sections `#screen-search` (regions `#facets` refine pane, `#q` search box, `#dslToggle` / `#dsl` / `#dslGroups` builder, `#hint` hint row, `#resCount` / `#resList` result list, `#searchPreview` reading pane) and `#screen-sql` (`#savedList` + `#schema` column 1, `#runSql` toolbar + `#editor` column 2, `#sqlCol3` result column, `#sqlCol4` detail column), `docs/design/variant-1/app.js` (`renderSql`, `mailItem`, `renderPreview`, `listNav`) and `docs/design/variant-1/NOTES.md` (Advanced SQL row). The prototype's SQL Format button (`#i-format`), the highlight overlay (`#hl`) and the relevance sort select are out of scope and are not built.

- [ ] **Step 1: Write the two `en` locale namespaces** (replace the kickoff `{}` files).

`src/locales/en/search.json`:
```json
{
  "title": "Search",
  "box": { "placeholder": "Search sender, recipients, subject or body", "label": "Search mail" },
  "dsl": {
    "toggle": "Conditions",
    "summary": "{{n}} conditions active",
    "between": "Between groups",
    "and": "AND",
    "or": "OR",
    "addGroup": "Add group",
    "addCond": "Add condition",
    "clear": "Clear all",
    "group": "Group {{n}}",
    "match": "Match",
    "remove": "Remove",
    "fieldLabel": "Field",
    "value": "Value",
    "fromDay": "From",
    "toDay": "To (inclusive)",
    "pickFolder": "Choose a folder",
    "pickPst": "Choose PST files",
    "includeSubfolders": "Include subfolders",
    "yes": "Yes",
    "no": "No",
    "op": { "contains": "contains", "between": "between", "is": "is", "in": "in" },
    "field": {
      "sender": "Sender", "recipient": "Recipient", "subject": "Subject", "body": "Body",
      "dateRange": "Sent date", "folder": "Folder", "hasAttachment": "Has attachment", "sourcePst": "Source PST"
    },
    "kinds": { "to": "To", "cc": "Cc", "bcc": "Bcc" }
  },
  "shortQueryHint": "Terms shorter than 3 characters are not indexed and search slowly",
  "resultCap": "Showing the first 100,000 results — refine the search",
  "count": "{{n}} messages",
  "countCapped": "{{n}}+ messages",
  "problem": {
    "tooManyTerms": "Use at most 10 search words.",
    "tooManyConditions": "The search has more than 50 conditions. Remove some words or conditions."
  },
  "newMail": "New mail was imported.",
  "refresh": "Refresh",
  "results": "Results",
  "refine": "Refine",
  "allMail": "All mail",
  "sources": "PST files",
  "folders": "Folders",
  "quick": "Quick filters",
  "hasAttachment": "Has attachment",
  "last90Days": "Last 90 days",
  "empty": { "title": "No mail to search yet", "body": "Import a PST file to start searching." },
  "idle": { "title": "Search your mail", "body": "Type a word, a name or an address, or add conditions." },
  "zero": { "title": "No messages match", "body": "Try fewer words or remove a condition.", "clear": "Clear search and conditions" },
  "list": { "undated": "No date", "label": "Search results" },
  "preview": {
    "pickTitle": "Select a message",
    "pickBody": "Its content shows here.",
    "noSubject": "(no subject)",
    "to": "To",
    "cc": "Cc",
    "bcc": "Bcc",
    "foundIn": "Found in",
    "attachments": "Attachments",
    "downloadLater": "Downloading attachments is not available yet",
    "download": "Download {{name}}",
    "date": "Sent"
  }
}
```

`src/locales/en/sql.json`:
```json
{
  "title": "Advanced SQL",
  "saved": "Saved queries",
  "savedEmpty": "No saved queries yet.",
  "new": "New query",
  "schema": "Schema",
  "run": "Run",
  "cancel": "Cancel",
  "save": {
    "action": "Save",
    "title": "Save query",
    "name": "Name",
    "duplicate": "A saved query with this name already exists.",
    "done": "Saved “{{name}}”."
  },
  "delete": {
    "action": "Delete",
    "title": "Delete “{{name}}”?",
    "body": "Only the saved query is deleted. Your mail is not changed.",
    "confirm": "Delete"
  },
  "readOnly": "Read-only",
  "editor": "SQL editor",
  "status": {
    "ok": "{{rows}} rows · {{ms}} ms",
    "running": "Running…",
    "idle": "Read-only connection. Press Ctrl+Enter or ⌘+Enter to run."
  },
  "mode": { "mailList": "Mail list", "table": "Result table", "command": "Command" },
  "result": {
    "rows": "{{n}} rows",
    "missing": "{{n}} ids matched no message",
    "truncated": "Showing the first {{n}} rows",
    "command": "Statement completed: {{tag}}",
    "null": "NULL"
  },
  "detail": { "title": "Row detail", "row": "Row {{n}}", "pick": "Select a row to see all its columns." },
  "error": { "position": "at character {{position}}" },
  "empty": {
    "noRunTitle": "Run a query",
    "noRunBody": "Write a SELECT over the views in Schema and press Run. Results with an id column open as mail.",
    "noMailTitle": "No mail to query yet",
    "noMailBody": "Import a PST file first."
  }
}
```

- [ ] **Step 2: Write the two `zh-TW` locale namespaces (same keys).**

`src/locales/zh-TW/search.json`:
```json
{
  "title": "搜尋",
  "box": { "placeholder": "搜尋寄件者、收件者、主旨或內文", "label": "搜尋郵件" },
  "dsl": {
    "toggle": "條件",
    "summary": "已套用 {{n}} 個條件",
    "between": "群組之間",
    "and": "AND",
    "or": "OR",
    "addGroup": "新增群組",
    "addCond": "新增條件",
    "clear": "全部清除",
    "group": "群組 {{n}}",
    "match": "符合",
    "remove": "移除",
    "fieldLabel": "欄位",
    "value": "值",
    "fromDay": "起",
    "toDay": "迄（含當日）",
    "pickFolder": "選擇資料夾",
    "pickPst": "選擇 PST 檔",
    "includeSubfolders": "包含子資料夾",
    "yes": "是",
    "no": "否",
    "op": { "contains": "包含", "between": "介於", "is": "是", "in": "屬於" },
    "field": {
      "sender": "寄件者", "recipient": "收件者", "subject": "主旨", "body": "內文",
      "dateRange": "寄件日期", "folder": "資料夾", "hasAttachment": "有附件", "sourcePst": "來源 PST"
    },
    "kinds": { "to": "收件", "cc": "副本", "bcc": "密件副本" }
  },
  "shortQueryHint": "少於 3 個字元的詞不會使用索引，搜尋較慢",
  "resultCap": "僅顯示前 100,000 筆結果，請縮小搜尋範圍",
  "count": "{{n}} 封郵件",
  "countCapped": "{{n}}+ 封郵件",
  "problem": {
    "tooManyTerms": "搜尋詞最多 10 個。",
    "tooManyConditions": "搜尋條件超過 50 個，請移除部分詞或條件。"
  },
  "newMail": "已匯入新郵件。",
  "refresh": "重新整理",
  "results": "結果",
  "refine": "篩選",
  "allMail": "所有郵件",
  "sources": "PST 檔",
  "folders": "資料夾",
  "quick": "快速篩選",
  "hasAttachment": "有附件",
  "last90Days": "最近 90 天",
  "empty": { "title": "還沒有可搜尋的郵件", "body": "匯入 PST 檔後即可開始搜尋。" },
  "idle": { "title": "搜尋您的郵件", "body": "輸入字詞、姓名或地址，或新增條件。" },
  "zero": { "title": "沒有符合的郵件", "body": "試著減少字詞或移除條件。", "clear": "清除搜尋與條件" },
  "list": { "undated": "無日期", "label": "搜尋結果" },
  "preview": {
    "pickTitle": "請選擇一封郵件",
    "pickBody": "郵件內容會顯示在這裡。",
    "noSubject": "（無主旨）",
    "to": "收件者",
    "cc": "副本",
    "bcc": "密件副本",
    "foundIn": "出現於",
    "attachments": "附件",
    "downloadLater": "目前尚無法下載附件",
    "download": "下載 {{name}}",
    "date": "寄件時間"
  }
}
```

`src/locales/zh-TW/sql.json`:
```json
{
  "title": "進階 SQL",
  "saved": "已儲存的查詢",
  "savedEmpty": "尚未儲存任何查詢。",
  "new": "新查詢",
  "schema": "結構描述",
  "run": "執行",
  "cancel": "取消",
  "save": {
    "action": "儲存",
    "title": "儲存查詢",
    "name": "名稱",
    "duplicate": "已有同名的查詢。",
    "done": "已儲存「{{name}}」。"
  },
  "delete": {
    "action": "刪除",
    "title": "要刪除「{{name}}」嗎？",
    "body": "只會刪除這個已儲存的查詢，郵件不受影響。",
    "confirm": "刪除"
  },
  "readOnly": "唯讀",
  "editor": "SQL 編輯器",
  "status": {
    "ok": "{{rows}} 列 · {{ms}} 毫秒",
    "running": "執行中…",
    "idle": "唯讀連線。按 Ctrl+Enter 或 ⌘+Enter 執行。"
  },
  "mode": { "mailList": "郵件清單", "table": "結果表格", "command": "指令" },
  "result": {
    "rows": "{{n}} 列",
    "missing": "{{n}} 個 id 找不到郵件",
    "truncated": "僅顯示前 {{n}} 列",
    "command": "陳述式已完成：{{tag}}",
    "null": "NULL"
  },
  "detail": { "title": "列詳細資料", "row": "第 {{n}} 列", "pick": "選擇一列以查看所有欄位。" },
  "error": { "position": "位於第 {{position}} 個字元" },
  "empty": {
    "noRunTitle": "執行查詢",
    "noRunBody": "針對結構描述中的檢視撰寫 SELECT 並按執行。含 id 欄位的結果會以郵件開啟。",
    "noMailTitle": "還沒有可查詢的郵件",
    "noMailBody": "請先匯入 PST 檔。"
  }
}
```
Run: `npx vitest run src/app/global.test.tsx -t "equal key sets"`
Expected: PASS (`locale files: zh-TW and en have equal key sets per namespace`) — `search` and `sql` now have identical non-empty key sets.

- [ ] **Step 3: Write the failing DSL test** `src/components/dsl/dsl.test.tsx`:
```tsx
import { fireEvent, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { ConditionGroup } from "../../api/bindings/ConditionGroup";
import { renderWithProviders } from "../../app/testing";
import {
  DslBuilder, buildFolderTree, countLeaves, depthOf, newCond, newGroup, toConditionGroup, toInstant,
  type CondDraft, type GroupDraft,
} from ".";

/** Part 03 §5.1 worked example, `req.query.root`, copied verbatim from the spec. */
const SPEC_ROOT: ConditionGroup = {
  op: "and",
  children: [
    { type: "group", op: "or", children: [
      { type: "sender", value: "contoso.com" },
      { type: "recipient", value: "alice", kinds: [] },
    ] },
    { type: "dateRange", from: "2022-12-31T16:00:00Z", to: "2023-12-31T16:00:00Z" },
    { type: "body", value: "報價" },
    { type: "hasAttachment", value: true },
    { type: "sourcePst", pstIds: [3, 7] },
  ],
};

const cond = (p: Partial<CondDraft>): CondDraft => ({ ...newCond(p.field), ...p });
/** The spec's user is in UTC+8; the test pins that offset instead of depending on the machine time zone. */
const utc8 = (day: string) => new Date(`${day}T00:00:00+08:00`);
const OPTIONS = { psts: [{ id: 3, displayName: "2023", path: "/p/2023.pst" }], folders: [] };

describe("DSL builder", () => {
  it("DSL builder output for the Part 03 §5.1 worked example deep-equals the spec JSON", () => {
    const draft = newGroup("and", [
      newGroup("or", [cond({ field: "sender", text: " contoso.com " }), cond({ field: "recipient", text: "alice" })]),
      cond({ field: "dateRange", fromDay: "2023-01-01", toDay: "2023-12-31" }),
      cond({ field: "body", text: "報價" }),
      cond({ field: "hasAttachment", flag: true }),
      cond({ field: "sourcePst", pstIds: [3, 7] }),
      cond({ field: "subject", text: "   " }),
      newGroup("or", [cond({ field: "folder" })]),
    ]);
    expect(toConditionGroup(draft, utc8)).toEqual(SPEC_ROOT);
    expect(countLeaves(draft)).toBe(6);
  });

  it("drops incomplete conditions and empty groups, and returns null for an empty root", () => {
    expect(toConditionGroup(newGroup("and", [cond({ field: "sender" }), newGroup("or", [])]))).toBeNull();
    expect(toConditionGroup(newGroup("or", [cond({ field: "dateRange", toDay: "2024-02-29" })]), utc8)).toEqual({
      op: "or",
      children: [{ type: "dateRange", from: null, to: "2024-02-29T16:00:00Z" }],
    });
    expect(toInstant(new Date(Date.UTC(2023, 0, 2, 3, 4, 5, 678)))).toBe("2023-01-02T03:04:05Z");
  });

  it("measures depth and builds the folder tree", () => {
    const deep = newGroup("and", [newGroup("or", [newGroup("and", [newGroup("or")])])]);
    expect(depthOf(deep)).toBe(4);
    const tree = buildFolderTree([
      { folderPath: "Inbox", pstId: 1, messageCount: 5 },
      { folderPath: "Inbox/Projects", pstId: 1, messageCount: 2 },
      { folderPath: "Inbox", pstId: 2, messageCount: 3 },
      { folderPath: "Sent Items", pstId: 2, messageCount: 4 },
    ]);
    expect(tree.map((n) => [n.path, n.count])).toEqual([["Inbox", 8], ["Sent Items", 4]]);
    expect(tree[0]!.children.map((n) => [n.path, n.name, n.count])).toEqual([["Inbox/Projects", "Projects", 2]]);
  });

  it("Add group adds a group with one empty condition", () => {
    const onChange = vi.fn();
    renderWithProviders(<DslBuilder value={newGroup("and", [])} onChange={onChange} options={OPTIONS} leafBudget={50} />);
    fireEvent.click(screen.getByRole("button", { name: "Add group" }));
    const next = onChange.mock.calls[0]![0] as GroupDraft;
    expect(next.children).toHaveLength(1);
    const group = next.children[0] as GroupDraft;
    expect(group.kind).toBe("group");
    expect(group.children).toEqual([expect.objectContaining({ kind: "cond", field: "sender", text: "" })]);
  });

  it("disables Add condition at the leaf budget and hides Add group at depth 4", () => {
    const value = newGroup("and", [
      newGroup("or", [cond({ field: "sender", text: "a@b" }), newGroup("and", [newGroup("or", [cond({ field: "body", text: "x" })])])]),
    ]);
    renderWithProviders(<DslBuilder value={value} onChange={vi.fn()} options={OPTIONS} leafBudget={2} />);
    for (const b of screen.getAllByRole("button", { name: "Add condition" })) expect((b as HTMLButtonElement).disabled).toBe(true);
    // root bar + groups at depth 2 and 3; the group at depth 4 has none.
    expect(screen.getAllByRole("button", { name: "Add group" })).toHaveLength(3);
  });
});
```
Run: `npx vitest run src/components/dsl`
Expected: FAIL — `Failed to resolve import "."` (no `src/components/dsl/index.ts` yet).

- [ ] **Step 4: Write `src/components/dsl/model.ts`.**
```ts
import type { ConditionGroup } from "../../api/bindings/ConditionGroup";
import type { ConditionNode } from "../../api/bindings/ConditionNode";
import type { FolderRef } from "../../api/bindings/FolderRef";
import type { GroupOp } from "../../api/bindings/GroupOp";
import type { RecipientKind } from "../../api/bindings/RecipientKind";

export type FieldKind = "sender" | "recipient" | "subject" | "body" | "dateRange" | "folder" | "hasAttachment" | "sourcePst";
export const FIELDS: readonly FieldKind[] = ["sender", "recipient", "subject", "body", "dateRange", "folder", "hasAttachment", "sourcePst"];

export interface CondDraft {
  kind: "cond";
  id: string;
  field: FieldKind;
  text: string;
  kinds: RecipientKind[];
  fromDay: string; // "YYYY-MM-DD" from <input type="date">, "" when unset
  toDay: string; // inclusive in the UI
  folderPath: string;
  includeSubfolders: boolean;
  flag: boolean;
  pstIds: number[];
}

export interface GroupDraft {
  kind: "group";
  id: string;
  op: GroupOp;
  children: Array<CondDraft | GroupDraft>;
}

/** Builder root = depth 1; composeQuery wraps the builder in one more group, so the sent tree stays within depth 5 (Part 03 §5.1). */
export const MAX_BUILDER_DEPTH = 4;
export const MAX_LEAVES = 50;

let lastId = 0;
const nextId = () => `n${++lastId}`;

export function newCond(field: FieldKind = "sender"): CondDraft {
  return {
    kind: "cond", id: nextId(), field, text: "", kinds: [], fromDay: "", toDay: "",
    folderPath: "", includeSubfolders: true, flag: true, pstIds: [],
  };
}

export function newGroup(op: GroupOp = "and", children: Array<CondDraft | GroupDraft> = [newCond()]): GroupDraft {
  return { kind: "group", id: nextId(), op, children };
}

export function isComplete(c: CondDraft): boolean {
  switch (c.field) {
    case "sender":
    case "recipient":
    case "subject":
    case "body":
      return c.text.trim().length > 0;
    case "dateRange":
      return c.fromDay !== "" || c.toDay !== "";
    case "folder":
      return c.folderPath !== "";
    case "hasAttachment":
      return true;
    case "sourcePst":
      return c.pstIds.length > 0;
  }
}

export function countLeaves(g: GroupDraft): number {
  return g.children.reduce((n, c) => n + (c.kind === "group" ? countLeaves(c) : isComplete(c) ? 1 : 0), 0);
}

export function depthOf(g: GroupDraft): number {
  return 1 + Math.max(0, ...g.children.map((c) => (c.kind === "group" ? depthOf(c) : 0)));
}

function ymd(day: string): [number, number, number] {
  const [y, m, d] = day.split("-").map(Number);
  return [y!, m!, d!];
}

/** Local midnight of a calendar day (Part 03 §1 Timestamps: the UI converts local days to instants). */
export function localDayStart(day: string): Date {
  const [y, m, d] = ymd(day);
  return new Date(y, m - 1, d);
}

function nextDay(day: string): string {
  const [y, m, d] = ymd(day);
  return new Date(Date.UTC(y, m - 1, d + 1)).toISOString().slice(0, 10);
}

/** RFC 3339 UTC instant without milliseconds, e.g. 2022-12-31T16:00:00Z. */
export function toInstant(d: Date): string {
  return d.toISOString().replace(/\.\d{3}Z$/, "Z");
}

function toNode(c: CondDraft, dayStart: (day: string) => Date): ConditionNode {
  const value = c.text.trim();
  switch (c.field) {
    case "sender":
      return { type: "sender", value };
    case "subject":
      return { type: "subject", value };
    case "body":
      return { type: "body", value };
    case "recipient":
      return { type: "recipient", value, kinds: c.kinds };
    case "dateRange":
      // The UI "to" day is inclusive; the contract's `to` is exclusive, so send the start of the next day.
      return {
        type: "dateRange",
        from: c.fromDay ? toInstant(dayStart(c.fromDay)) : null,
        to: c.toDay ? toInstant(dayStart(nextDay(c.toDay))) : null,
      };
    case "folder":
      return { type: "folder", folderPath: c.folderPath, includeSubfolders: c.includeSubfolders };
    case "hasAttachment":
      return { type: "hasAttachment", value: c.flag };
    case "sourcePst":
      return { type: "sourcePst", pstIds: c.pstIds };
  }
}

/** Incomplete conditions and groups left empty are dropped; an empty root gives null. */
export function toConditionGroup(g: GroupDraft, dayStart: (day: string) => Date = localDayStart): ConditionGroup | null {
  const children = g.children.flatMap((c): ConditionNode[] => {
    if (c.kind === "cond") return isComplete(c) ? [toNode(c, dayStart)] : [];
    const sub = toConditionGroup(c, dayStart);
    return sub ? [{ type: "group", op: sub.op, children: sub.children }] : [];
  });
  return children.length > 0 ? { op: g.op, children } : null;
}

/** Replaces (or, when f returns null, removes) the node with `id` anywhere below `g`. */
export function updateNode(
  g: GroupDraft,
  id: string,
  f: (n: CondDraft | GroupDraft) => CondDraft | GroupDraft | null,
): GroupDraft {
  const children = g.children.flatMap((c) => {
    if (c.id === id) {
      const next = f(c);
      return next ? [next] : [];
    }
    return [c.kind === "group" ? updateNode(c, id, f) : c];
  });
  return { ...g, children };
}

export interface FolderTreeNode {
  path: string;
  name: string;
  count: number; // messages directly in this folder path, summed over PSTs
  children: FolderTreeNode[];
}

/** Part 03 §5.4: folders of all PSTs merged by their "/"-separated path. */
export function buildFolderTree(folders: FolderRef[]): FolderTreeNode[] {
  const root: FolderTreeNode = { path: "", name: "", count: 0, children: [] };
  for (const f of folders) {
    const parts = f.folderPath.split("/");
    let node = root;
    parts.forEach((name, i) => {
      const path = parts.slice(0, i + 1).join("/");
      let child = node.children.find((c) => c.path === path);
      if (!child) {
        child = { path, name, count: 0, children: [] };
        node.children.push(child);
      }
      if (i === parts.length - 1) child.count += f.messageCount;
      node = child;
    });
  }
  return root.children;
}
```

- [ ] **Step 5: Write `src/components/dsl/DslBuilder.tsx` and `src/components/dsl/index.ts`.**

`src/components/dsl/DslBuilder.tsx`:
```tsx
import {
  Button, Caption1, Caption1Strong, Card, Checkbox, Dropdown, Input, Option, ToggleButton, makeStyles, tokens,
} from "@fluentui/react-components";
import { AddRegular, DismissRegular } from "@fluentui/react-icons";
import { Fragment, useMemo } from "react";
import { useTranslation } from "react-i18next";
import type { FilterOptions } from "../../api/bindings/FilterOptions";
import type { GroupOp } from "../../api/bindings/GroupOp";
import type { RecipientKind } from "../../api/bindings/RecipientKind";
import {
  FIELDS, MAX_BUILDER_DEPTH, countLeaves, newCond, newGroup, updateNode,
  type CondDraft, type FieldKind, type GroupDraft,
} from "./model";

const OP: Record<FieldKind, "contains" | "between" | "is" | "in"> = {
  sender: "contains", recipient: "contains", subject: "contains", body: "contains",
  dateRange: "between", folder: "is", hasAttachment: "is", sourcePst: "in",
};
const KINDS: readonly RecipientKind[] = ["to", "cc", "bcc"];

const useStyles = makeStyles({
  root: { display: "flex", flexDirection: "column", gap: tokens.spacingVerticalS },
  bar: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS },
  group: {
    display: "flex", flexDirection: "column", gap: tokens.spacingVerticalXS, padding: tokens.spacingHorizontalM,
    borderLeft: `3px solid ${tokens.colorBrandStroke1}`,
  },
  head: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS },
  spacer: { flex: 1 },
  join: { color: tokens.colorNeutralForeground3, paddingLeft: tokens.spacingHorizontalS },
  cond: { display: "grid", gridTemplateColumns: "150px 92px 1fr 28px", alignItems: "center", gap: tokens.spacingHorizontalS },
  value: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalXS, minWidth: 0 },
  grow: { flex: 1, minWidth: 0 },
  foot: { display: "flex", gap: tokens.spacingHorizontalS },
});

interface Ctx {
  set(id: string, f: (n: CondDraft | GroupDraft) => CondDraft | GroupDraft | null): void;
  full: boolean;
  options: FilterOptions;
}

function OpToggle({ op, onChange }: { op: GroupOp; onChange(op: GroupOp): void }) {
  const { t } = useTranslation("search");
  return (
    <span>
      <ToggleButton size="small" checked={op === "and"} onClick={() => onChange("and")}>{t("dsl.and")}</ToggleButton>
      <ToggleButton size="small" checked={op === "or"} onClick={() => onChange("or")}>{t("dsl.or")}</ToggleButton>
    </span>
  );
}

function ValueControl({ cond, ctx }: { cond: CondDraft; ctx: Ctx }) {
  const { t } = useTranslation("search");
  const s = useStyles();
  const patch = (p: Partial<CondDraft>) => ctx.set(cond.id, (c) => ({ ...(c as CondDraft), ...p }));
  const folderPaths = useMemo(
    () => [...new Set(ctx.options.folders.map((f) => f.folderPath))].sort(),
    [ctx.options.folders],
  );
  switch (cond.field) {
    case "sender":
    case "subject":
    case "body":
      return (
        <Input className={s.grow} value={cond.text} maxLength={500} aria-label={t("dsl.value")}
          onChange={(_, d) => patch({ text: d.value })} />
      );
    case "recipient":
      return (
        <span className={s.value}>
          <Input className={s.grow} value={cond.text} maxLength={500} aria-label={t("dsl.value")}
            onChange={(_, d) => patch({ text: d.value })} />
          {KINDS.map((k) => (
            <ToggleButton key={k} size="small" checked={cond.kinds.includes(k)}
              onClick={() => patch({ kinds: cond.kinds.includes(k) ? cond.kinds.filter((x) => x !== k) : [...cond.kinds, k] })}>
              {t(`dsl.kinds.${k}`)}
            </ToggleButton>
          ))}
        </span>
      );
    case "dateRange":
      return (
        <span className={s.value}>
          <Input type="date" value={cond.fromDay} aria-label={t("dsl.fromDay")} onChange={(_, d) => patch({ fromDay: d.value })} />
          <Input type="date" value={cond.toDay} aria-label={t("dsl.toDay")} onChange={(_, d) => patch({ toDay: d.value })} />
        </span>
      );
    case "folder":
      return (
        <span className={s.value}>
          <Dropdown className={s.grow} placeholder={t("dsl.pickFolder")} aria-label={t("dsl.pickFolder")}
            value={cond.folderPath} selectedOptions={cond.folderPath ? [cond.folderPath] : []}
            onOptionSelect={(_, d) => patch({ folderPath: d.optionValue ?? "" })}>
            {folderPaths.map((p) => (
              <Option key={p} value={p} text={p}>
                <span style={{ paddingLeft: `${(p.split("/").length - 1) * 12}px` }}>{p.split("/").pop()}</span>
              </Option>
            ))}
          </Dropdown>
          <Checkbox checked={cond.includeSubfolders} label={t("dsl.includeSubfolders")}
            onChange={(_, d) => patch({ includeSubfolders: d.checked === true })} />
        </span>
      );
    case "hasAttachment":
      return (
        <Dropdown className={s.grow} aria-label={t("dsl.value")} value={t(cond.flag ? "dsl.yes" : "dsl.no")}
          selectedOptions={[cond.flag ? "yes" : "no"]} onOptionSelect={(_, d) => patch({ flag: d.optionValue === "yes" })}>
          <Option value="yes">{t("dsl.yes")}</Option>
          <Option value="no">{t("dsl.no")}</Option>
        </Dropdown>
      );
    case "sourcePst":
      return (
        <Dropdown className={s.grow} multiselect placeholder={t("dsl.pickPst")} aria-label={t("dsl.pickPst")}
          value={ctx.options.psts.filter((p) => cond.pstIds.includes(p.id)).map((p) => p.displayName).join(", ")}
          selectedOptions={cond.pstIds.map(String)}
          onOptionSelect={(_, d) => patch({ pstIds: d.selectedOptions.map(Number) })}>
          {ctx.options.psts.map((p) => (
            <Option key={p.id} value={String(p.id)}>{p.displayName}</Option>
          ))}
        </Dropdown>
      );
  }
}

function CondRow({ cond, ctx }: { cond: CondDraft; ctx: Ctx }) {
  const { t } = useTranslation("search");
  const s = useStyles();
  return (
    <div className={s.cond} data-testid="dsl-cond">
      <Dropdown aria-label={t("dsl.fieldLabel")} value={t(`dsl.field.${cond.field}`)} selectedOptions={[cond.field]}
        onOptionSelect={(_, d) => d.optionValue && ctx.set(cond.id, () => ({ ...newCond(d.optionValue as FieldKind), id: cond.id }))}>
        {FIELDS.map((f) => (
          <Option key={f} value={f}>{t(`dsl.field.${f}`)}</Option>
        ))}
      </Dropdown>
      <Caption1>{t(`dsl.op.${OP[cond.field]}`)}</Caption1>
      <ValueControl cond={cond} ctx={ctx} />
      <Button appearance="subtle" size="small" icon={<DismissRegular />} aria-label={t("dsl.remove")}
        onClick={() => ctx.set(cond.id, () => null)} />
    </div>
  );
}

function GroupCard({ group, index, depth, ctx }: { group: GroupDraft; index: number; depth: number; ctx: Ctx }) {
  const { t } = useTranslation("search");
  const s = useStyles();
  const append = (node: CondDraft | GroupDraft) =>
    ctx.set(group.id, (g) => ({ ...(g as GroupDraft), children: [...(g as GroupDraft).children, node] }));
  return (
    <Card appearance="outline" className={s.group}>
      <div className={s.head}>
        <Caption1Strong>{t("dsl.group", { n: index })}</Caption1Strong>
        <Caption1>{t("dsl.match")}</Caption1>
        <OpToggle op={group.op} onChange={(op) => ctx.set(group.id, (g) => ({ ...(g as GroupDraft), op }))} />
        <span className={s.spacer} />
        <Button appearance="subtle" size="small" icon={<DismissRegular />} aria-label={t("dsl.remove")}
          onClick={() => ctx.set(group.id, () => null)} />
      </div>
      {group.children.map((c, i) => (
        <Fragment key={c.id}>
          {i > 0 ? <Caption1 className={s.join}>{t(`dsl.${group.op}`)}</Caption1> : null}
          {c.kind === "group" ? <GroupCard group={c} index={i + 1} depth={depth + 1} ctx={ctx} /> : <CondRow cond={c} ctx={ctx} />}
        </Fragment>
      ))}
      <div className={s.foot}>
        <Button size="small" icon={<AddRegular />} disabled={ctx.full} onClick={() => append(newCond())}>{t("dsl.addCond")}</Button>
        {depth < MAX_BUILDER_DEPTH ? (
          <Button size="small" icon={<AddRegular />} disabled={ctx.full} onClick={() => append(newGroup())}>{t("dsl.addGroup")}</Button>
        ) : null}
      </div>
    </Card>
  );
}

/** Prototype `#dsl` / `#dslGroups`: root bar, then one outline Card per group (brand left border). */
export function DslBuilder({ value, onChange, options, leafBudget }: {
  value: GroupDraft;
  onChange(next: GroupDraft): void;
  options: FilterOptions;
  leafBudget: number;
}) {
  const { t } = useTranslation("search");
  const s = useStyles();
  const ctx: Ctx = {
    set: (id, f) => onChange(updateNode(value, id, f)),
    full: countLeaves(value) >= leafBudget,
    options,
  };
  return (
    <div className={s.root} data-testid="dsl-builder">
      <div className={s.bar}>
        <Caption1>{t("dsl.between")}</Caption1>
        <OpToggle op={value.op} onChange={(op) => onChange({ ...value, op })} />
        <Button size="small" icon={<AddRegular />} disabled={ctx.full}
          onClick={() => onChange({ ...value, children: [...value.children, newGroup()] })}>
          {t("dsl.addGroup")}
        </Button>
        <span className={s.spacer} />
        <Button size="small" appearance="subtle" onClick={() => onChange({ ...value, children: [] })}>{t("dsl.clear")}</Button>
      </div>
      {value.children.map((c, i) =>
        c.kind === "group" ? (
          <GroupCard key={c.id} group={c} index={i + 1} depth={2} ctx={ctx} />
        ) : (
          <CondRow key={c.id} cond={c} ctx={ctx} />
        ),
      )}
    </div>
  );
}
```

`src/components/dsl/index.ts`:
```ts
export { DslBuilder } from "./DslBuilder";
export {
  FIELDS, MAX_BUILDER_DEPTH, MAX_LEAVES, buildFolderTree, countLeaves, depthOf, isComplete, localDayStart, newCond, newGroup,
  toConditionGroup, toInstant, updateNode,
  type CondDraft, type FieldKind, type FolderTreeNode, type GroupDraft,
} from "./model";
```
Run: `npx vitest run src/components/dsl`
Expected: PASS (5 tests).

- [ ] **Step 6: Write the failing composition test** `src/screens/search/compose.test.ts`:
```ts
import { describe, expect, it } from "vitest";
import type { ConditionGroup } from "../../api/bindings/ConditionGroup";
import { toInstant } from "../../components/dsl";
import { EMPTY_REFINE, composeQuery, termGroup } from "./compose";

const words = (n: number) => Array.from({ length: n }, (_, i) => `word${i}`).join(" ");

describe("query composition", () => {
  it("composeQuery builds simple, dsl and problem results", () => {
    expect(composeQuery("  quarterly   report ", EMPTY_REFINE, null)).toEqual({
      query: { kind: "simple", text: "quarterly   report" },
      leaves: 8,
      problem: null,
    });
    const r = composeQuery("quarterly report", { ...EMPTY_REFINE, hasAttachment: true }, null);
    expect(r.query).toEqual({
      kind: "dsl",
      root: { op: "and", children: [termGroup("quarterly"), termGroup("report"), { type: "hasAttachment", value: true }] },
    });
    expect(r.leaves).toBe(9);
    expect(composeQuery(words(11), EMPTY_REFINE, null)).toEqual({ query: null, leaves: 44, problem: "tooManyTerms" });
    const builder: ConditionGroup = {
      op: "or",
      children: Array.from({ length: 11 }, (_, i) => ({ type: "subject" as const, value: `s${i}` })),
    };
    expect(composeQuery(words(10), EMPTY_REFINE, builder)).toEqual({ query: null, leaves: 51, problem: "tooManyConditions" });
    expect(composeQuery("   ", EMPTY_REFINE, null).query).toBeNull();
  });

  it("termGroup is the Part 03 §5.1 simple-query rule", () => {
    expect(termGroup("alice")).toEqual({
      type: "group",
      op: "or",
      children: [
        { type: "sender", value: "alice" },
        { type: "recipient", value: "alice", kinds: [] },
        { type: "subject", value: "alice" },
        { type: "body", value: "alice" },
      ],
    });
  });

  it("refine conditions and the builder become nodes of one and-root", () => {
    const now = new Date(2026, 9, 22, 15, 30);
    const builder: ConditionGroup = { op: "or", children: [{ type: "body", value: "報價" }] };
    const r = composeQuery("", { pstId: 4, folderPath: "Inbox", hasAttachment: false, last90Days: true }, builder, now);
    expect(r.query).toEqual({
      kind: "dsl",
      root: {
        op: "and",
        children: [
          { type: "sourcePst", pstIds: [4] },
          { type: "folder", folderPath: "Inbox", includeSubfolders: true },
          { type: "dateRange", from: toInstant(new Date(2026, 6, 24)), to: null },
          { type: "group", op: "or", children: [{ type: "body", value: "報價" }] },
        ],
      },
    });
    expect(r.leaves).toBe(4);
  });
});
```
Run: `npx vitest run src/screens/search/compose.test.ts`
Expected: FAIL — `Failed to resolve import "./compose"`.

- [ ] **Step 7: Write `src/screens/search/compose.ts`.**
```ts
import type { ConditionGroup } from "../../api/bindings/ConditionGroup";
import type { ConditionNode } from "../../api/bindings/ConditionNode";
import type { SearchQuery } from "../../api/bindings/SearchQuery";
import { MAX_LEAVES, toInstant } from "../../components/dsl";

export interface RefineState {
  pstId: number | null;
  folderPath: string | null;
  hasAttachment: boolean;
  last90Days: boolean;
}

export const EMPTY_REFINE: RefineState = { pstId: null, folderPath: null, hasAttachment: false, last90Days: false };
const MAX_TERMS = 10;

export type ComposeResult = { query: SearchQuery | null; leaves: number; problem: null | "tooManyTerms" | "tooManyConditions" };

/** Part 03 §5.1 simple-query rule: a term matches any of sender, recipients, subject, body. */
export function termGroup(term: string): ConditionNode {
  return {
    type: "group",
    op: "or",
    children: [
      { type: "sender", value: term },
      { type: "recipient", value: term, kinds: [] },
      { type: "subject", value: term },
      { type: "body", value: term },
    ],
  };
}

function leafCount(nodes: ConditionNode[]): number {
  return nodes.reduce((n, c) => n + (c.type === "group" ? leafCount(c.children) : 1), 0);
}

export function refineNodes(r: RefineState, now: Date): ConditionNode[] {
  const out: ConditionNode[] = [];
  if (r.pstId !== null) out.push({ type: "sourcePst", pstIds: [r.pstId] });
  if (r.folderPath !== null) out.push({ type: "folder", folderPath: r.folderPath, includeSubfolders: true });
  if (r.hasAttachment) out.push({ type: "hasAttachment", value: true });
  if (r.last90Days) {
    // Local midnight 90 calendar days ago (the Date constructor normalizes the negative day).
    out.push({ type: "dateRange", from: toInstant(new Date(now.getFullYear(), now.getMonth(), now.getDate() - 90)), to: null });
  }
  return out;
}

export function splitTerms(text: string): string[] {
  return text.trim().split(/\s+/).filter((t) => t.length > 0);
}

/** Search box + refine pane + builder → one SearchQuery (plan Decision "Search query composition in the frontend"). */
export function composeQuery(text: string, refine: RefineState, builder: ConditionGroup | null, now: Date = new Date()): ComposeResult {
  const terms = splitTerms(text);
  if (terms.length > MAX_TERMS) return { query: null, leaves: 4 * terms.length, problem: "tooManyTerms" };
  const refined = refineNodes(refine, now);
  const leaves = 4 * terms.length + refined.length + (builder ? leafCount(builder.children) : 0);
  if (leaves > MAX_LEAVES) return { query: null, leaves, problem: "tooManyConditions" };
  if (refined.length === 0 && builder === null) {
    return { query: terms.length > 0 ? { kind: "simple", text: text.trim() } : null, leaves, problem: null };
  }
  const children: ConditionNode[] = [
    ...terms.map(termGroup),
    ...refined,
    ...(builder ? [{ type: "group" as const, op: builder.op, children: builder.children }] : []),
  ];
  return { query: { kind: "dsl", root: { op: "and", children } }, leaves, problem: null };
}
```
Run: `npx vitest run src/screens/search/compose.test.ts src/components/dsl`
Expected: PASS (3 + 5 tests).

- [ ] **Step 8: Commit locales, DSL and composition.**
Run: `npm run typecheck` — expected exit 0. Run `git rev-parse --abbrev-ref HEAD` (expected `main`), then
```bash
printf 'feat(ui): search and sql locales, DSL builder and query composition\n\nCo-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>\n' > scripts/tmp/msg-r107-1.txt
git add src/locales/en/search.json src/locales/en/sql.json src/locales/zh-TW/search.json src/locales/zh-TW/sql.json src/components/dsl src/screens/search/compose.ts src/screens/search/compose.test.ts
git commit -F scripts/tmp/msg-r107-1.txt -- src/locales/en/search.json src/locales/en/sql.json src/locales/zh-TW/search.json src/locales/zh-TW/sql.json src/components/dsl src/screens/search/compose.ts src/screens/search/compose.test.ts
git status --porcelain
```
Expected: the commit succeeds; `git status --porcelain` lists none of these paths.

- [ ] **Step 9: Write the mail test fixtures** `src/components/mail/fixtures.ts` (imported only by `*.test.tsx`):
```ts
import type { MailDetail } from "../../api/bindings/MailDetail";
import type { MailRow } from "../../api/bindings/MailRow";

/**
 * jsdom has no layout, so the virtualizer sees a 0 px viewport and renders no rows. virtual-core reads
 * element.clientHeight / clientWidth (and skips ResizeObserver when jsdom lacks it); this defines both on
 * HTMLElement.prototype and returns the restore function.
 */
export function stubViewport(height: number): () => void {
  const proto = HTMLElement.prototype;
  const saved = (["clientHeight", "clientWidth"] as const).map((k) => [k, Object.getOwnPropertyDescriptor(proto, k)] as const);
  Object.defineProperty(proto, "clientHeight", { configurable: true, get: () => height });
  Object.defineProperty(proto, "clientWidth", { configurable: true, get: () => 1000 });
  return () => {
    for (const [k, d] of saved) {
      if (d) Object.defineProperty(proto, k, d);
      else Reflect.deleteProperty(proto, k);
    }
  };
}

export function makeMailRow(id: number, p: Partial<MailRow> = {}): MailRow {
  return {
    id,
    sentAt: "2026-10-01T08:00:00Z",
    from: { name: "Ann Lee", address: "ann@contoso.example" },
    toPreview: "Bob",
    recipientCount: 1,
    subject: `Subject ${id}`,
    snippet: "snippet",
    hasAttachments: false,
    occurrenceCount: 1,
    ...p,
  };
}

export function makeMailDetail(p: Partial<MailDetail> = {}): MailDetail {
  return {
    id: 5,
    messageId: "<q1@contoso.example>",
    subject: "Quote Q1",
    sentAt: "2026-10-01T08:00:00Z",
    receivedAt: "2026-10-01T08:01:00Z",
    from: { name: "Ann Lee", address: "ann@contoso.example" },
    to: [{ name: "Bob", address: "bob@x.example" }],
    cc: [],
    bcc: [],
    bodyHtml: "<p>Hello</p>",
    bodyText: "Hello",
    attachments: [{ id: 1, fileName: "quote.pdf", sizeBytes: 2048, mimeType: "application/pdf" }],
    occurrences: [{ pstId: 1, pstDisplayName: "archive", pstPath: "/p/archive.pst", folderPath: "Inbox/Projects", pstItemId: 77 }],
    ...p,
  };
}
```
If `npm run typecheck` reports a field mismatch here, the generated binding in `src/api/bindings/` is the truth: align the fixture, never the binding.

- [ ] **Step 10: Write the failing paging test** `src/components/mail/pages.test.tsx`:
```tsx
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api/commands");
vi.mock("../../api/events", async () => (await import("../../app/testing")).eventsMock);

import { api } from "../../api/commands";
import type { SearchPage } from "../../api/bindings/SearchPage";
import type { SearchQuery } from "../../api/bindings/SearchQuery";
import type { SearchRequest } from "../../api/bindings/SearchRequest";
import { mockApiDefaults, renderWithProviders } from "../../app/testing";
import { MAX_LIST_ROWS, MailList, PAGE_SIZE, nextPageParam, useMailPages } from ".";
import { makeMailRow, stubViewport } from "./fixtures";

const QUERY: SearchQuery = { kind: "simple", text: "loadgen" };

describe("mail paging", () => {
  beforeEach(() => mockApiDefaults());

  it("nextPageParam stops at the last cursor and at the cap", () => {
    const rows = Array.from({ length: PAGE_SIZE }, (_, i) => makeMailRow(i + 1));
    const page = (next: string | null): SearchPage => ({ rows, nextCursor: next, shortQuery: false, elapsedMs: 1 });
    expect(nextPageParam(page("c1"), [page("c1")])).toBe("c1");
    expect(nextPageParam(page(null), [page(null)])).toBeUndefined();
    const full = Array.from({ length: 500 }, () => page("more"));
    expect(nextPageParam(full[499]!, full)).toBeUndefined();
    expect(nextPageParam(full[498]!, full.slice(0, 499))).toBe("more");
  });

  it("with 600 mock pages of 200 rows the list stops requesting after 100,000 rows and shows the cap row", async () => {
    vi.mocked(api.searchMail).mockImplementation(async (req: SearchRequest) => {
      const n = req.cursor === null ? 0 : Number(req.cursor);
      return {
        rows: Array.from({ length: PAGE_SIZE }, (_, i) => makeMailRow(n * PAGE_SIZE + i + 1)),
        nextCursor: n < 599 ? String(n + 1) : null,
        shortQuery: false,
        elapsedMs: 1,
      };
    });
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
    const { result } = renderHook(() => useMailPages(QUERY, 1), { wrapper });
    await waitFor(() => expect(result.current.rows.length).toBe(PAGE_SIZE));
    // More calls than pages: the hook itself must stop at the cap.
    for (let i = 0; i < 600; i++) await act(() => result.current.fetchNextPage());
    await waitFor(() => expect(result.current.capped).toBe(true));
    expect(api.searchMail).toHaveBeenCalledTimes(500);
    expect(vi.mocked(api.searchMail).mock.calls[0]![0]).toEqual({ query: QUERY, cursor: null, pageSize: 200 });
    expect(result.current.rows.length).toBe(MAX_LIST_ROWS);
    expect(result.current.hasMore).toBe(false);

    const restore = stubViewport(800);
    renderWithProviders(
      <MailList rows={result.current.rows} selectedId={null} onSelect={vi.fn()} hasMore={result.current.hasMore}
        capped={result.current.capped} loadingMore={false} onEndReached={vi.fn()} ariaLabel="results" />,
    );
    expect(screen.getByTestId("result-cap-row").textContent).toBe("Showing the first 100,000 results — refine the search");
    expect(screen.queryByTestId("list-loader-row")).toBeNull();
    expect(screen.getAllByTestId("mail-row").length).toBeLessThan(60);
    restore();
  }, 60_000);
});
```
Run: `npx vitest run src/components/mail/pages.test.tsx`
Expected: FAIL — `Failed to resolve import "."` (no `src/components/mail/index.ts`).

- [ ] **Step 11: Write the failing mail component test** `src/components/mail/mail.test.tsx`:
```tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api/commands");
vi.mock("../../api/events", async () => (await import("../../app/testing")).eventsMock);

import { api } from "../../api/commands";
import { mockApiDefaults, renderWithProviders } from "../../app/testing";
import { MailList, MailPreview, ReadingPane, buildEntries } from ".";
import { makeMailDetail, makeMailRow, stubViewport } from "./fixtures";

const ROWS = [
  makeMailRow(1, { sentAt: "2026-10-03T08:00:00Z" }),
  makeMailRow(2, { sentAt: "2026-10-02T08:00:00Z" }),
  makeMailRow(3, { sentAt: "2026-09-15T08:00:00Z" }),
  makeMailRow(4, { sentAt: null }),
];
const listProps = { selectedId: null, onSelect: vi.fn(), hasMore: false, capped: false, loadingMore: false, onEndReached: vi.fn(), ariaLabel: "results" };

describe("mail components", () => {
  let restore: () => void;
  beforeEach(() => {
    mockApiDefaults();
    restore = stubViewport(800);
  });
  afterEach(() => restore());

  it('preview iframe has sandbox="" and no allow- token', () => {
    render(<MailPreview html="<p>Hi</p>" title="Hello" />);
    const frame = screen.getByTestId("mail-preview");
    expect(frame.tagName).toBe("IFRAME");
    expect(frame.getAttribute("sandbox")).toBe("");
    expect(frame.outerHTML).not.toMatch(/allow-/);
    expect(frame.getAttribute("referrerpolicy")).toBe("no-referrer");
    expect(frame.getAttribute("srcdoc")).toBe("<p>Hi</p>");
  });

  it("preview replaces srcdoc on the same iframe", () => {
    const r = render(<MailPreview html="<p>one</p>" title="a" />);
    const first = screen.getByTestId("mail-preview");
    r.rerender(<MailPreview html="<p>two</p>" title="b" />);
    const second = screen.getByTestId("mail-preview");
    expect(second).toBe(first);
    expect(second.getAttribute("srcdoc")).toBe("<p>two</p>");
  });

  it("reading pane shows the header, attachments and the plain-text fallback", async () => {
    vi.mocked(api.mailGet).mockResolvedValue(makeMailDetail({ bodyHtml: null, bodyText: "Plain body" }));
    renderWithProviders(<ReadingPane mailId={5} />);
    expect(await screen.findByText("Plain body")).toBeTruthy();
    expect(api.mailGet).toHaveBeenCalledWith(5);
    expect(screen.queryByTestId("mail-preview")).toBeNull();
    expect(screen.getByText("Quote Q1")).toBeTruthy();
    expect(screen.getByText("Bob <bob@x.example>")).toBeTruthy();
    expect(screen.getByText("archive · Inbox/Projects")).toBeTruthy();
    expect((screen.getByRole("button", { name: "Download quote.pdf" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("reading pane renders HTML through MailPreview and shows the pick state without a mail", async () => {
    vi.mocked(api.mailGet).mockResolvedValue(makeMailDetail());
    const r = renderWithProviders(<ReadingPane mailId={5} />);
    expect((await screen.findByTestId("mail-preview")).getAttribute("srcdoc")).toBe("<p>Hello</p>");
    r.unmount();
    renderWithProviders(<ReadingPane mailId={null} />);
    expect(screen.getByText("Select a message")).toBeTruthy();
  });

  it("buildEntries inserts month headers and an undated group", () => {
    expect(buildEntries(ROWS, "en", "No date").map((e) => e.kind)).toEqual(["month", "mail", "mail", "month", "mail", "month", "mail"]);
  });

  it("ArrowDown on the list selects the next mail", () => {
    const onSelect = vi.fn();
    renderWithProviders(<MailList {...listProps} rows={ROWS} selectedId={1} onSelect={onSelect} />);
    expect(screen.getAllByTestId("mail-row")).toHaveLength(4);
    expect(screen.getByText("No date")).toBeTruthy();
    fireEvent.keyDown(screen.getByRole("listbox"), { key: "ArrowDown" });
    expect(onSelect).toHaveBeenCalledWith(2);
  });

  it("requests the next page only when the end is within 50 rows", () => {
    const far = vi.fn();
    const r = renderWithProviders(
      <MailList {...listProps} rows={Array.from({ length: 200 }, (_, i) => makeMailRow(i + 1))} hasMore monthHeaders={false} onEndReached={far} />,
    );
    expect(far).not.toHaveBeenCalled();
    expect(screen.getByTestId("list-loader-row")).toBeTruthy();
    r.unmount();
    const near = vi.fn();
    renderWithProviders(
      <MailList {...listProps} rows={Array.from({ length: 60 }, (_, i) => makeMailRow(i + 1))} hasMore monthHeaders={false} onEndReached={near} />,
    );
    expect(near).toHaveBeenCalled();
  });
});
```
Run: `npx vitest run src/components/mail`
Expected: FAIL — `Failed to resolve import "."`.

- [ ] **Step 12: Write the mail components.**

`src/components/mail/pages.ts`:
```ts
import { useInfiniteQuery, useQueryClient } from "@tanstack/react-query";
import { useMemo } from "react";
import { api } from "../../api/commands";
import type { AppError } from "../../api/bindings/AppError";
import type { MailRow } from "../../api/bindings/MailRow";
import type { SearchPage } from "../../api/bindings/SearchPage";
import type { SearchQuery } from "../../api/bindings/SearchQuery";
import { toAppError } from "../common";

export const PAGE_SIZE = 200;
/** Part 03 §5.2: 500 pages of 200; the scroll container stays ≤ 3.2M px. */
export const MAX_LIST_ROWS = 100_000;

export function loadedRows(pages: SearchPage[]): number {
  return pages.reduce((n, p) => n + p.rows.length, 0);
}

export function nextPageParam(last: SearchPage, pages: SearchPage[]): string | undefined {
  if (last.nextCursor === null || loadedRows(pages) >= MAX_LIST_ROWS) return undefined;
  return last.nextCursor;
}

export interface MailPages {
  rows: MailRow[];
  hasMore: boolean;
  capped: boolean;
  shortQuery: boolean;
  isLoading: boolean;
  isFetchingNextPage: boolean;
  error: AppError | null;
  fetchNextPage(): Promise<unknown>;
  reset(): void;
}

/** `seq` is the Search screen's sequence number: a response for an older sequence lands in another cache entry and is never shown. */
export function useMailPages(query: SearchQuery | null, seq: number): MailPages {
  const qc = useQueryClient();
  const q = useInfiniteQuery({
    queryKey: ["searchMail", seq, query],
    queryFn: ({ pageParam }) => api.searchMail({ query: query as SearchQuery, cursor: pageParam, pageSize: PAGE_SIZE }),
    initialPageParam: null as string | null,
    getNextPageParam: (last, pages) => nextPageParam(last, pages),
    enabled: query !== null,
  });
  const pages = q.data?.pages;
  const derived = useMemo(() => {
    const list = pages ?? [];
    const all = list.flatMap((p) => p.rows);
    const last = list[list.length - 1];
    return {
      rows: all.length > MAX_LIST_ROWS ? all.slice(0, MAX_LIST_ROWS) : all,
      capped: all.length >= MAX_LIST_ROWS && last !== undefined && last.nextCursor !== null,
      shortQuery: list[0]?.shortQuery ?? false,
    };
  }, [pages]);
  return {
    ...derived,
    hasMore: q.hasNextPage,
    isLoading: q.isLoading,
    isFetchingNextPage: q.isFetchingNextPage,
    error: q.error ? toAppError(q.error) : null,
    // cancelRefetch: false → a call while a page is loading joins it instead of restarting it.
    fetchNextPage: () => q.fetchNextPage({ cancelRefetch: false }),
    reset: () => void qc.resetQueries({ queryKey: ["searchMail", seq] }),
  };
}
```

`src/components/mail/MailList.tsx`:
```tsx
import { Avatar, Caption1Strong, CounterBadge, Spinner, makeStyles, mergeClasses, tokens } from "@fluentui/react-components";
import { AttachRegular } from "@fluentui/react-icons";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useMemo, useRef, type KeyboardEvent } from "react";
import { useTranslation } from "react-i18next";
import type { MailRow } from "../../api/bindings/MailRow";
import { formatDay } from "../common";

export const ROW_HEIGHT = 32;
export const PREFETCH_ROWS = 50;

export type Entry = { kind: "month"; key: string; label: string } | { kind: "mail"; key: string; row: MailRow };

export interface MailListProps {
  rows: MailRow[];
  selectedId: number | null;
  onSelect(id: number): void;
  hasMore: boolean;
  capped: boolean;
  loadingMore: boolean;
  onEndReached(): void;
  ariaLabel: string;
  monthHeaders?: boolean;
}

/** Rows arrive newest first (Part 03 §5.2); a header entry starts each local month, undated mail last. */
export function buildEntries(rows: MailRow[], lng: string, undatedLabel: string): Entry[] {
  const fmt = new Intl.DateTimeFormat(lng === "cimode" ? "en" : lng, { year: "numeric", month: "long" });
  const out: Entry[] = [];
  let current: string | null = null;
  for (const row of rows) {
    const d = row.sentAt ? new Date(row.sentAt) : null;
    const month = d ? `${d.getFullYear()}-${d.getMonth()}` : "undated";
    if (month !== current) {
      out.push({ kind: "month", key: `m-${month}-${out.length}`, label: d ? fmt.format(d) : undatedLabel });
      current = month;
    }
    out.push({ kind: "mail", key: `r-${row.id}`, row });
  }
  return out;
}

const useStyles = makeStyles({
  scroll: { height: "100%", overflowY: "auto", position: "relative", outlineStyle: "none" },
  block: { position: "relative", width: "100%" },
  item: { position: "absolute", top: 0, left: 0, width: "100%", height: `${ROW_HEIGHT}px`, boxSizing: "border-box" },
  month: {
    display: "flex", alignItems: "center", padding: `0 ${tokens.spacingHorizontalM}`,
    backgroundColor: tokens.colorNeutralBackground2, color: tokens.colorNeutralForeground3,
  },
  row: {
    display: "grid", gridTemplateColumns: "24px minmax(120px, 22%) 1fr auto", alignItems: "center",
    gap: tokens.spacingHorizontalS, padding: `0 ${tokens.spacingHorizontalM}`, cursor: "pointer",
    borderBottom: `1px solid ${tokens.colorNeutralStroke3}`,
    ":hover": { backgroundColor: tokens.colorNeutralBackground1Hover },
  },
  selected: {
    backgroundColor: tokens.colorBrandBackground2,
    boxShadow: `inset 3px 0 0 ${tokens.colorCompoundBrandStroke}`,
    ":hover": { backgroundColor: tokens.colorBrandBackground2 },
  },
  ellipsis: { overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", minWidth: 0 },
  sender: { fontWeight: tokens.fontWeightSemibold },
  subject: { color: tokens.colorBrandForeground2 },
  snippet: { color: tokens.colorNeutralForeground3 },
  meta: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalXS, color: tokens.colorNeutralForeground3, fontSize: tokens.fontSizeBase200 },
  footer: { display: "flex", alignItems: "center", justifyContent: "center", height: `${ROW_HEIGHT}px`, color: tokens.colorNeutralForeground3 },
});

/** Prototype `#resList` `.mail-item`, one line per mail at 32 px (plan Decision "Fixed-height virtual rows"). */
export function MailList(p: MailListProps) {
  const { t, i18n } = useTranslation("search");
  const s = useStyles();
  const ref = useRef<HTMLDivElement>(null);
  const lng = i18n.language;
  const undated = t("list.undated");
  const entries = useMemo<Entry[]>(
    () =>
      p.monthHeaders === false
        ? p.rows.map((row) => ({ kind: "mail", key: `r-${row.id}`, row }))
        : buildEntries(p.rows, lng, undated),
    [p.rows, p.monthHeaders, lng, undated],
  );
  const v = useVirtualizer({ count: entries.length, getScrollElement: () => ref.current, estimateSize: () => ROW_HEIGHT, overscan: 10 });
  const items = v.getVirtualItems();
  const lastIndex = items.length > 0 ? items[items.length - 1]!.index : -1;
  const { hasMore, loadingMore, capped, onEndReached } = p;

  useEffect(() => {
    if (entries.length > 0 && lastIndex >= entries.length - 1 - PREFETCH_ROWS && hasMore && !loadingMore && !capped) onEndReached();
  }, [lastIndex, entries.length, hasMore, loadingMore, capped, onEndReached]);

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    e.preventDefault();
    const mailIdx: number[] = [];
    entries.forEach((en, i) => en.kind === "mail" && mailIdx.push(i));
    if (mailIdx.length === 0) return;
    const cur = mailIdx.findIndex((i) => (entries[i] as { row: MailRow }).row.id === p.selectedId);
    const step = e.key === "ArrowDown" ? 1 : -1;
    const next = cur === -1 ? 0 : Math.min(Math.max(cur + step, 0), mailIdx.length - 1);
    const idx = mailIdx[next]!;
    p.onSelect((entries[idx] as { row: MailRow }).row.id);
    v.scrollToIndex(idx);
  };

  return (
    <div ref={ref} className={s.scroll} role="listbox" aria-label={p.ariaLabel} tabIndex={0} onKeyDown={onKeyDown} data-testid="mail-list">
      <div className={s.block} style={{ height: `${v.getTotalSize()}px` }}>
        {items.map((it) => {
          const en = entries[it.index]!;
          const style = { transform: `translateY(${it.start}px)` };
          if (en.kind === "month") {
            return (
              <div key={en.key} className={mergeClasses(s.item, s.month)} style={style} role="presentation">
                <Caption1Strong>{en.label}</Caption1Strong>
              </div>
            );
          }
          const row = en.row;
          const selected = row.id === p.selectedId;
          const name = row.from.name ?? row.from.address ?? "";
          return (
            <div key={en.key} role="option" aria-selected={selected} data-testid="mail-row" data-mail-id={row.id}
              className={mergeClasses(s.item, s.row, selected && s.selected)} style={style} onClick={() => p.onSelect(row.id)}>
              <Avatar size={24} name={name} aria-hidden />
              <span className={mergeClasses(s.ellipsis, s.sender)}>{name}</span>
              <span className={s.ellipsis}>
                <span className={s.subject}>{row.subject || t("preview.noSubject")}</span>
                {row.snippet ? <span className={s.snippet}>{` — ${row.snippet}`}</span> : null}
              </span>
              <span className={s.meta}>
                {row.hasAttachments ? <AttachRegular aria-hidden /> : null}
                {row.occurrenceCount > 1 ? <CounterBadge count={row.occurrenceCount} size="small" appearance="ghost" /> : null}
                <span>{formatDay(row.sentAt, lng)}</span>
              </span>
            </div>
          );
        })}
      </div>
      {hasMore && !capped ? (
        <div className={s.footer} data-testid="list-loader-row">
          <Spinner size="tiny" />
        </div>
      ) : null}
      {capped ? (
        <div className={s.footer} data-testid="result-cap-row">
          {t("resultCap")}
        </div>
      ) : null}
    </div>
  );
}
```

`src/components/mail/MailPreview.tsx`:
```tsx
import { makeStyles, tokens } from "@fluentui/react-components";

const useStyles = makeStyles({
  frame: { display: "block", width: "100%", height: "100%", border: "0", backgroundColor: tokens.colorNeutralBackground1 },
});

/**
 * The only place that renders mail HTML (Part 03 §5.6): empty sandbox — no scripts, no same-origin, no forms,
 * no popups, no navigation. Never add an allow-* token. The element is not keyed by mail, so a new `html`
 * replaces srcdoc on the same iframe.
 */
export function MailPreview({ html, title }: { html: string; title: string }) {
  const s = useStyles();
  return <iframe className={s.frame} data-testid="mail-preview" sandbox="" srcDoc={html} referrerPolicy="no-referrer" title={title} />;
}
```

`src/components/mail/AttachmentCard.tsx`:
```tsx
import { Body1Strong, Button, Caption1, Card, Tooltip, makeStyles, tokens } from "@fluentui/react-components";
import { ArrowDownloadRegular } from "@fluentui/react-icons";
import { useTranslation } from "react-i18next";
import type { AttachmentMeta } from "../../api/bindings/AttachmentMeta";
import { formatBytes } from "../common";

const useStyles = makeStyles({
  card: { display: "flex", flexDirection: "row", alignItems: "center", gap: tokens.spacingHorizontalS, width: "260px" },
  tile: {
    width: "36px", height: "36px", display: "grid", placeItems: "center", flexShrink: 0,
    borderRadius: tokens.borderRadiusMedium, backgroundColor: tokens.colorBrandBackground2, color: tokens.colorBrandForeground1,
    fontSize: tokens.fontSizeBase100, fontWeight: tokens.fontWeightSemibold,
  },
  text: { display: "flex", flexDirection: "column", minWidth: 0, flex: 1 },
  name: { overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" },
});

/** Metadata only; download comes in a later phase (requirements: attachments are re-read from the PST later). */
export function AttachmentCard({ attachment: a }: { attachment: AttachmentMeta }) {
  const { t, i18n } = useTranslation("search");
  const s = useStyles();
  const ext = a.fileName.includes(".") ? (a.fileName.split(".").pop() ?? "").toUpperCase().slice(0, 4) : "";
  return (
    <Card size="small" className={s.card}>
      <span className={s.tile} aria-hidden>
        {ext}
      </span>
      <div className={s.text}>
        <Body1Strong className={s.name} title={a.fileName}>{a.fileName}</Body1Strong>
        <Caption1>{`${formatBytes(a.sizeBytes, i18n.language)} · ${a.mimeType}`}</Caption1>
      </div>
      {/* Disabled buttons get no pointer events, so the Tooltip wraps a span. */}
      <Tooltip content={t("preview.downloadLater")} relationship="description">
        <span>
          <Button appearance="subtle" disabled icon={<ArrowDownloadRegular />} aria-label={t("preview.download", { name: a.fileName })} />
        </span>
      </Tooltip>
    </Card>
  );
}
```

`src/components/mail/ReadingPane.tsx`:
```tsx
import {
  Badge, Caption1, MessageBar, MessageBarBody, Persona, Spinner, Title3, makeStyles, tokens,
} from "@fluentui/react-components";
import { MailRegular } from "@fluentui/react-icons";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api } from "../../api/commands";
import type { Address } from "../../api/bindings/Address";
import { EmptyState, formatAppError, formatDateTime, toAppError } from "../common";
import { AttachmentCard } from "./AttachmentCard";
import { MailPreview } from "./MailPreview";

const useStyles = makeStyles({
  root: { display: "flex", flexDirection: "column", height: "100%", minHeight: 0, backgroundColor: tokens.colorNeutralBackground1 },
  head: {
    maxHeight: "45%", overflowY: "auto", flexShrink: 0, display: "flex", flexDirection: "column", gap: tokens.spacingVerticalS,
    padding: `${tokens.spacingVerticalM} ${tokens.spacingHorizontalL}`, borderBottom: `1px solid ${tokens.colorNeutralStroke2}`,
  },
  meta: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalM },
  spacer: { flex: 1 },
  line: { display: "grid", gridTemplateColumns: "48px 1fr", gap: tokens.spacingHorizontalS, color: tokens.colorNeutralForeground2 },
  chips: { display: "flex", flexWrap: "wrap", gap: tokens.spacingHorizontalXS, alignItems: "center" },
  body: { flex: 1, minHeight: 0, overflow: "auto" },
  text: { margin: 0, padding: tokens.spacingHorizontalL, whiteSpace: "pre-wrap", fontFamily: tokens.fontFamilyBase },
  center: { display: "grid", placeItems: "center", height: "100%" },
});

const fmt = (a: Address) => (a.name && a.address ? `${a.name} <${a.address}>` : (a.name ?? a.address ?? ""));

/** Prototype `#searchPreview` (`renderPreview`). */
export function ReadingPane({ mailId }: { mailId: number | null }) {
  const { t, i18n } = useTranslation("search");
  const s = useStyles();
  const q = useQuery({ queryKey: ["mailGet", mailId], queryFn: () => api.mailGet(mailId as number), enabled: mailId !== null });

  let content;
  if (mailId === null) {
    content = <EmptyState icon={<MailRegular />} title={t("preview.pickTitle")} body={t("preview.pickBody")} />;
  } else if (q.error) {
    content = (
      <MessageBar intent="error">
        <MessageBarBody>{formatAppError(toAppError(q.error), t)}</MessageBarBody>
      </MessageBar>
    );
  } else if (!q.data) {
    content = (
      <div className={s.center}>
        <Spinner />
      </div>
    );
  } else {
    const m = q.data;
    const lines = (["to", "cc", "bcc"] as const).filter((k) => m[k].length > 0);
    content = (
      <>
        <div className={s.head}>
          <Title3>{m.subject || t("preview.noSubject")}</Title3>
          <div className={s.meta}>
            <Persona name={m.from.name ?? m.from.address ?? ""} secondaryText={m.from.address ?? ""} avatar={{ size: 40 }} />
            <span className={s.spacer} />
            <Caption1 title={t("preview.date")}>{formatDateTime(m.sentAt, i18n.language)}</Caption1>
          </div>
          {lines.map((k) => (
            <div key={k} className={s.line}>
              <Caption1>{t(`preview.${k}`)}</Caption1>
              <span>{m[k].map(fmt).join(", ")}</span>
            </div>
          ))}
          <div className={s.chips}>
            <Caption1>{t("preview.foundIn")}</Caption1>
            {m.occurrences.map((o) => (
              <Badge key={`${o.pstId}-${o.pstItemId}`} appearance="tint" color="informative">
                {`${o.pstDisplayName} · ${o.folderPath}`}
              </Badge>
            ))}
          </div>
          {m.attachments.length > 0 ? (
            <div className={s.chips}>
              {m.attachments.map((a) => (
                <AttachmentCard key={a.id} attachment={a} />
              ))}
            </div>
          ) : null}
        </div>
        <div className={s.body}>
          {m.bodyHtml !== null ? (
            <MailPreview html={m.bodyHtml} title={m.subject || t("preview.noSubject")} />
          ) : (
            <pre className={s.text}>{m.bodyText}</pre>
          )}
        </div>
      </>
    );
  }
  return (
    <div className={s.root} data-testid="reading-pane">
      {content}
    </div>
  );
}
```

`src/components/mail/index.ts`:
```ts
export { AttachmentCard } from "./AttachmentCard";
export { MailList, PREFETCH_ROWS, ROW_HEIGHT, buildEntries, type Entry, type MailListProps } from "./MailList";
export { MailPreview } from "./MailPreview";
export { MAX_LIST_ROWS, PAGE_SIZE, loadedRows, nextPageParam, useMailPages, type MailPages } from "./pages";
export { ReadingPane } from "./ReadingPane";
```
Run: `npx vitest run src/components/mail`
Expected: PASS (`pages.test.tsx` 2, `mail.test.tsx` 7). `grep -rn "dangerouslySetInnerHTML" src` finds nothing.

- [ ] **Step 13: Commit the mail components.**
Run: `npm run typecheck` — expected exit 0. Run `git rev-parse --abbrev-ref HEAD` (expected `main`), then
```bash
printf 'feat(ui): virtualized mail list with 100k cap, reading pane and sandboxed preview\n\nCo-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>\n' > scripts/tmp/msg-r107-2.txt
git add src/components/mail
git commit -F scripts/tmp/msg-r107-2.txt -- src/components/mail
git status --porcelain
```
Expected: commit succeeds; `git status --porcelain` lists no `src/components/mail` path.

- [ ] **Step 14: Write the failing SQL editor test** `src/components/sql-editor/sql-editor.test.tsx`:
```tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { SQL_SCHEMA, SqlEditor, VIEW_COLUMNS, codePointOffsetToUtf16 } from ".";

const props = { onChange: vi.fn(), onRun: vi.fn(), errorPosition: null, ariaLabel: "SQL editor" };

describe("SQL editor", () => {
  it("codePointOffsetToUtf16 converts a 1-based code point offset", () => {
    const text = "SELECT 😀 x"; // 10 code points, 11 UTF-16 units
    expect(codePointOffsetToUtf16(text, 1)).toBe(0);
    expect(codePointOffsetToUtf16(text, 8)).toBe(7);
    expect(codePointOffsetToUtf16(text, 9)).toBe(9);
    expect(codePointOffsetToUtf16(text, 10)).toBe(10);
    expect(codePointOffsetToUtf16(text, 99)).toBe(11);
  });

  it("the static schema lists the six views of Part 04 §8.2", () => {
    expect(Object.keys(VIEW_COLUMNS).sort()).toEqual(["v_attachments", "v_folders", "v_mail", "v_occurrences", "v_pst_sources", "v_recipients"]);
    expect(SQL_SCHEMA.v_mail).toContain("body_text");
    expect(SQL_SCHEMA.v_occurrences).toEqual(["message_id", "pst_id", "pst_path", "pst_name", "folder_path", "pst_item_id"]);
  });

  it("renders the document, replaces it from props and runs on Mod-Enter", () => {
    const onRun = vi.fn();
    const r = render(<SqlEditor {...props} value="SELECT 1" onRun={onRun} />);
    const content = screen.getByTestId("sql-editor");
    expect(content.getAttribute("aria-label")).toBe("SQL editor");
    expect(content.textContent).toBe("SELECT 1");
    fireEvent.keyDown(content, { key: "Enter", ctrlKey: true });
    expect(onRun).toHaveBeenCalledTimes(1);
    r.rerender(<SqlEditor {...props} value="SELECT 2" onRun={onRun} />);
    expect(screen.getByTestId("sql-editor").textContent).toBe("SELECT 2");
  });

  it("sqlError position marks the character in the editor", () => {
    const r = render(<SqlEditor {...props} value="SELECT 😀 x" errorPosition={10} />);
    expect(r.container.querySelector(".cm-sql-error")?.textContent).toBe("x");
    r.rerender(<SqlEditor {...props} value="SELECT 😀 x" errorPosition={8} />);
    expect(r.container.querySelector(".cm-sql-error")?.textContent).toBe("😀");
    r.rerender(<SqlEditor {...props} value="SELECT 😀 x" errorPosition={null} />);
    expect(r.container.querySelector(".cm-sql-error")).toBeNull();
  });
});
```
This test relies on the jsdom `Range` layout stub in `src/test/setup.ts` (planner-1 foundation file, request P1-7): CodeMirror measures text through `Range.getClientRects`, which jsdom may not implement.
Run: `npx vitest run src/components/sql-editor`
Expected: FAIL — `Failed to resolve import "."`.

- [ ] **Step 15: Write `src/components/sql-editor/schema.ts`, `SqlEditor.tsx` and `index.ts`.**

`src/components/sql-editor/schema.ts`:
```ts
/** The six read-only views of Part 04 §8.2 with their PostgreSQL types; the only schema Advanced SQL users see (Part 03 §6.2). */
export const VIEW_COLUMNS: Record<string, ReadonlyArray<readonly [name: string, pgType: string]>> = {
  v_mail: [
    ["id", "int8"], ["message_id", "text"], ["subject", "text"], ["sent_at", "timestamptz"], ["received_at", "timestamptz"],
    ["sender_name", "text"], ["sender_email", "text"], ["recipients", "text"], ["recipient_count", "int4"],
    ["to_preview", "text"], ["has_attachments", "bool"], ["body_text", "text"], ["snippet", "text"],
    ["body_source", "text"], ["body_tsv", "tsvector"], ["imported_at", "timestamptz"],
  ],
  v_recipients: [["message_id", "int8"], ["ordinal", "int4"], ["kind", "text"], ["display_name", "text"], ["email", "text"]],
  v_occurrences: [
    ["message_id", "int8"], ["pst_id", "int8"], ["pst_path", "text"], ["pst_name", "text"], ["folder_path", "text"], ["pst_item_id", "int8"],
  ],
  v_attachments: [["id", "int8"], ["message_id", "int8"], ["file_name", "text"], ["size_bytes", "int8"], ["mime_type", "text"]],
  v_folders: [["id", "int8"], ["pst_id", "int8"], ["name", "text"], ["path", "text"], ["message_count", "int8"]],
  v_pst_sources: [["id", "int8"], ["path", "text"], ["display_name", "text"], ["file_size_bytes", "int8"], ["created_at", "timestamptz"]],
};

/** `@codemirror/lang-sql` SQLNamespace: view → column names. */
export const SQL_SCHEMA: Record<string, string[]> = Object.fromEntries(
  Object.entries(VIEW_COLUMNS).map(([view, cols]) => [view, cols.map(([name]) => name)]),
);
```

`src/components/sql-editor/SqlEditor.tsx`:
```tsx
import { PostgreSQL, sql } from "@codemirror/lang-sql";
import { EditorState, Prec, StateEffect, StateField } from "@codemirror/state";
import { Decoration, EditorView, keymap, type DecorationSet } from "@codemirror/view";
import { makeStyles } from "@fluentui/react-components";
import { basicSetup } from "codemirror";
import { useEffect, useRef } from "react";
import { SQL_SCHEMA } from "./schema";

export interface SqlEditorProps {
  value: string;
  onChange(sql: string): void;
  onRun(): void;
  errorPosition: number | null;
  ariaLabel: string;
}

/** PostgreSQL `position` is a 1-based character (code point) offset; CodeMirror positions are UTF-16 offsets. */
export function codePointOffsetToUtf16(text: string, position1: number): number {
  let i = 0;
  for (let cp = 1; cp < position1 && i < text.length; cp++) i += (text.codePointAt(i) ?? 0) > 0xffff ? 2 : 1;
  return i;
}

type Mark = { from: number; to: number } | null;

function errorRange(doc: string, position: number | null): Mark {
  if (position === null || doc.length === 0) return null;
  let from = Math.min(codePointOffsetToUtf16(doc, position), doc.length - 1);
  const unit = doc.charCodeAt(from);
  if (from > 0 && unit >= 0xdc00 && unit <= 0xdfff) from -= 1; // never split a surrogate pair
  return { from, to: from + ((doc.codePointAt(from) ?? 0) > 0xffff ? 2 : 1) };
}

const setError = StateEffect.define<Mark>();
const errorMark = Decoration.mark({ class: "cm-sql-error" });
/** The marker maps through edits, so typing after an error never leaves it on a stale range. */
const errorField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(deco, tr) {
    let next = deco.map(tr.changes);
    for (const e of tr.effects) if (e.is(setError)) next = e.value ? Decoration.set([errorMark.range(e.value.from, e.value.to)]) : Decoration.none;
    return next;
  },
  provide: (f) => EditorView.decorations.from(f),
});

const theme = EditorView.theme({
  "&": { height: "100%", fontSize: "13px" },
  ".cm-scroller": { fontFamily: "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace" },
  ".cm-sql-error": { textDecoration: "underline wavy #c50f1f", textUnderlineOffset: "3px" },
});

const useStyles = makeStyles({ host: { height: "100%", minHeight: 0, overflow: "hidden" } });

/** In-house CodeMirror 6 wrapper (Part 01 §2: no @uiw/react-codemirror). */
export function SqlEditor({ value, onChange, onRun, errorPosition, ariaLabel }: SqlEditorProps) {
  const s = useStyles();
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const onRunRef = useRef(onRun);
  const onChangeRef = useRef(onChange);
  useEffect(() => {
    onRunRef.current = onRun;
    onChangeRef.current = onChange;
  });

  useEffect(() => {
    const v = new EditorView({
      parent: host.current as HTMLDivElement,
      state: EditorState.create({
        doc: value,
        extensions: [
          basicSetup,
          sql({ dialect: PostgreSQL, schema: SQL_SCHEMA, upperCaseKeywords: true }),
          Prec.highest(keymap.of([{ key: "Mod-Enter", run: () => (onRunRef.current(), true) }])),
          EditorView.updateListener.of((u) => {
            if (u.docChanged) onChangeRef.current(u.state.doc.toString());
          }),
          EditorView.contentAttributes.of({ "aria-label": ariaLabel, "data-testid": "sql-editor" }),
          theme,
          errorField,
        ],
      }),
    });
    view.current = v;
    return () => {
      v.destroy();
      view.current = null;
    };
    // The view is created once per mount; `value` and `errorPosition` are synced by the effects below.
  }, [ariaLabel]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    const v = view.current;
    if (!v) return;
    const doc = v.state.doc.toString();
    if (doc !== value) v.dispatch({ changes: { from: 0, to: doc.length, insert: value } });
  }, [value]);

  useEffect(() => {
    const v = view.current;
    if (!v) return;
    v.dispatch({ effects: setError.of(errorRange(v.state.doc.toString(), errorPosition)) });
  }, [errorPosition, value]);

  return <div ref={host} className={s.host} />;
}
```

`src/components/sql-editor/index.ts`:
```ts
export { SQL_SCHEMA, VIEW_COLUMNS } from "./schema";
export { SqlEditor, codePointOffsetToUtf16, type SqlEditorProps } from "./SqlEditor";
```
Run: `npx vitest run src/components/sql-editor`
Expected: PASS (4 tests).

- [ ] **Step 16: Commit the SQL editor.**
Run: `npm run typecheck` — expected exit 0. Run `grep -rhoE "from \"@codemirror/[a-z-]+\"|from \"codemirror\"" src | sort -u`
Expected exactly: `from "@codemirror/lang-sql"`, `from "@codemirror/state"`, `from "@codemirror/view"`, `from "codemirror"`. Then
```bash
printf 'feat(ui): in-house CodeMirror SQL editor with view schema and error marker\n\nCo-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>\n' > scripts/tmp/msg-r107-3.txt
git add src/components/sql-editor
git commit -F scripts/tmp/msg-r107-3.txt -- src/components/sql-editor
git status --porcelain
```
Expected: commit succeeds; `git status --porcelain` lists no `src/components/sql-editor` path.

- [ ] **Step 17: Write the failing Search screen test** `src/screens/search/search.test.tsx`:
```tsx
import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api/commands");
vi.mock("../../api/events", async () => (await import("../../app/testing")).eventsMock);

import { api } from "../../api/commands";
import type { MailRow } from "../../api/bindings/MailRow";
import type { SearchPage } from "../../api/bindings/SearchPage";
import { eventsMock, mockApiDefaults, renderWithProviders } from "../../app/testing";
import { makeMailRow, stubViewport } from "../../components/mail/fixtures";
import { SearchScreen } from ".";

const READY = { status: { counts: { messages: 10, psts: 1 } } };
const page = (rows: MailRow[], shortQuery = false): SearchPage => ({ rows, nextCursor: null, shortQuery, elapsedMs: 1 });

function deferred<T>() {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

/** Types into the search box and lets the 300 ms debounce elapse. */
function type(text: string) {
  fireEvent.change(screen.getByTestId("search-box"), { target: { value: text } });
  act(() => {
    vi.advanceTimersByTime(300);
  });
}

describe("Search screen", () => {
  let restore: () => void;
  beforeEach(() => {
    eventsMock.reset();
    mockApiDefaults();
    restore = stubViewport(800);
    vi.useFakeTimers({ shouldAdvanceTime: true });
  });
  afterEach(() => {
    vi.useRealTimers();
    restore();
  });

  it("searchCount {count: 10000, capped: true} renders 10,000+ and {count: 9999, capped: false} renders 9,999", async () => {
    vi.mocked(api.searchMail).mockResolvedValue(page([makeMailRow(1)]));
    vi.mocked(api.searchCount).mockResolvedValue({ count: 10000, capped: true });
    renderWithProviders(<SearchScreen />, READY);
    type("alpha");
    expect((await screen.findByTestId("result-count")).textContent).toBe("10,000+ messages");
    vi.mocked(api.searchCount).mockResolvedValue({ count: 9999, capped: false });
    type("beta");
    await waitFor(() => expect(screen.getByTestId("result-count").textContent).toBe("9,999 messages"));
    expect(api.searchCount).toHaveBeenLastCalledWith({ kind: "simple", text: "beta" });
  });

  it("a stale search response (older sequence) is dropped", async () => {
    const older = deferred<SearchPage>();
    const newer = deferred<SearchPage>();
    vi.mocked(api.searchMail).mockReturnValueOnce(older.promise).mockReturnValueOnce(newer.promise);
    renderWithProviders(<SearchScreen />, READY);
    type("alpha");
    await waitFor(() => expect(api.searchMail).toHaveBeenCalledTimes(1));
    type("beta");
    await waitFor(() => expect(api.searchMail).toHaveBeenCalledTimes(2));
    await act(async () => newer.resolve(page([makeMailRow(2, { subject: "newer result" })])));
    await act(async () => older.resolve(page([makeMailRow(1, { subject: "older result" })])));
    expect(await screen.findByText("newer result")).toBeTruthy();
    expect(screen.queryByText("older result")).toBeNull();
  });

  it("library:changed triggers a filterOptions refetch and not a list reload", async () => {
    vi.mocked(api.searchMail).mockResolvedValue(page([makeMailRow(1, { subject: "first" })]));
    renderWithProviders(<SearchScreen />, READY);
    type("alpha");
    expect(await screen.findByText("first")).toBeTruthy();
    await waitFor(() => expect(api.filterOptions).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(eventsMock.onEvent).toHaveBeenCalledWith("library:changed", expect.any(Function)));
    act(() => eventsMock.emit("library:changed", { counts: { messages: 11, psts: 1 } }));
    await waitFor(() => expect(api.filterOptions).toHaveBeenCalledTimes(2));
    expect(api.searchMail).toHaveBeenCalledTimes(1);
    expect(screen.getByTestId("new-mail-bar")).toBeTruthy();
    fireEvent.click(screen.getByTestId("new-mail-refresh"));
    await waitFor(() => expect(api.searchMail).toHaveBeenCalledTimes(2));
    expect(screen.queryByTestId("new-mail-bar")).toBeNull();
  });

  it("shows the short-query hint and selects the first row", async () => {
    vi.mocked(api.searchMail).mockResolvedValue(page([makeMailRow(1), makeMailRow(2)], true));
    renderWithProviders(<SearchScreen />, READY);
    type("報價");
    expect(await screen.findByTestId("short-query-hint")).toBeTruthy();
    await waitFor(() => expect(api.mailGet).toHaveBeenCalledWith(1));
    expect(screen.getAllByTestId("mail-row")[0]!.getAttribute("aria-selected")).toBe("true");
  });

  it("shows the empty, idle, zero-result and problem states", async () => {
    const empty = renderWithProviders(<SearchScreen />);
    expect(screen.getByText("No mail to search yet")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Go to Import" }));
    expect(empty.go).toHaveBeenCalledWith("import");
    empty.unmount();

    renderWithProviders(<SearchScreen />, READY);
    expect(screen.getByText("Search your mail")).toBeTruthy();
    type("nothing");
    expect(await screen.findByText("No messages match")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Clear search and conditions" }));
    expect((screen.getByTestId("search-box") as HTMLInputElement).value).toBe("");
    type("a b c d e f g h i j k");
    expect(screen.getByText("Use at most 10 search words.")).toBeTruthy();
    expect(api.searchMail).toHaveBeenCalledTimes(1);
  });
});
```
Run: `npx vitest run src/screens/search`
Expected: FAIL — the kickoff `SearchScreen` renders `null` (`Unable to find an element by: [data-testid="search-box"]`); `compose.test.ts` still passes.

- [ ] **Step 18: Write `src/screens/search/RefinePane.tsx` and `src/screens/search/index.tsx`.**

`src/screens/search/RefinePane.tsx`:
```tsx
import {
  Caption1Strong, Checkbox, CounterBadge, Subtitle2, Tree, TreeItem, TreeItemLayout, makeStyles, mergeClasses, tokens,
} from "@fluentui/react-components";
import { DocumentRegular, FolderRegular, MailInboxRegular } from "@fluentui/react-icons";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import type { FilterOptions } from "../../api/bindings/FilterOptions";
import { buildFolderTree, type FolderTreeNode } from "../../components/dsl";
import type { RefineState } from "./compose";

const useStyles = makeStyles({
  root: {
    display: "flex", flexDirection: "column", gap: tokens.spacingVerticalS, minHeight: 0, overflowY: "auto",
    padding: tokens.spacingHorizontalM, backgroundColor: tokens.colorNeutralBackground2, borderRight: `1px solid ${tokens.colorNeutralStroke2}`,
  },
  section: { marginTop: tokens.spacingVerticalM, color: tokens.colorNeutralForeground3 },
  picked: { backgroundColor: tokens.colorBrandBackground2, fontWeight: tokens.fontWeightSemibold },
});

function FolderItems({ nodes, value, onPick }: { nodes: FolderTreeNode[]; value: RefineState; onPick(path: string): void }) {
  const s = useStyles();
  return (
    <>
      {nodes.map((n) => (
        <TreeItem key={n.path} itemType={n.children.length > 0 ? "branch" : "leaf"} value={`folder:${n.path}`}>
          <TreeItemLayout iconBefore={<FolderRegular />} onClick={() => onPick(n.path)}
            className={mergeClasses(value.folderPath === n.path && s.picked)}
            aside={<CounterBadge count={n.count} overflowCount={9_999_999} size="small" appearance="ghost" />}>
            {n.name}
          </TreeItemLayout>
          {n.children.length > 0 ? (
            <Tree>
              <FolderItems nodes={n.children} value={value} onPick={onPick} />
            </Tree>
          ) : null}
        </TreeItem>
      ))}
    </>
  );
}

/** Prototype `#facets`: All mail, PST files, folder tree, quick filters. */
export function RefinePane({ value, onChange, options }: { value: RefineState; onChange(next: RefineState): void; options: FilterOptions | undefined }) {
  const { t } = useTranslation("search");
  const s = useStyles();
  const folders = useMemo(() => buildFolderTree(options?.folders ?? []), [options]);
  const all = value.pstId === null && value.folderPath === null;
  return (
    <aside className={s.root} aria-label={t("refine")}>
      <Subtitle2>{t("refine")}</Subtitle2>
      <Tree aria-label={t("sources")}>
        <TreeItem itemType="leaf" value="all">
          <TreeItemLayout iconBefore={<MailInboxRegular />} className={mergeClasses(all && s.picked)}
            onClick={() => onChange({ ...value, pstId: null, folderPath: null })}>
            {t("allMail")}
          </TreeItemLayout>
        </TreeItem>
        {(options?.psts ?? []).map((p) => (
          <TreeItem key={p.id} itemType="leaf" value={`pst:${p.id}`}>
            <TreeItemLayout iconBefore={<DocumentRegular />} className={mergeClasses(value.pstId === p.id && s.picked)}
              onClick={() => onChange({ ...value, pstId: value.pstId === p.id ? null : p.id })}>
              {p.displayName}
            </TreeItemLayout>
          </TreeItem>
        ))}
      </Tree>
      <Caption1Strong className={s.section}>{t("folders")}</Caption1Strong>
      <Tree aria-label={t("folders")}>
        <FolderItems nodes={folders} value={value}
          onPick={(path) => onChange({ ...value, folderPath: value.folderPath === path ? null : path })} />
      </Tree>
      <Caption1Strong className={s.section}>{t("quick")}</Caption1Strong>
      <Checkbox label={t("hasAttachment")} checked={value.hasAttachment} onChange={(_, d) => onChange({ ...value, hasAttachment: d.checked === true })} />
      <Checkbox label={t("last90Days")} checked={value.last90Days} onChange={(_, d) => onChange({ ...value, last90Days: d.checked === true })} />
    </aside>
  );
}
```

`src/screens/search/index.tsx`:
```tsx
import {
  Button, Caption1, CounterBadge, MessageBar, MessageBarActions, MessageBarBody, SearchBox, Spinner, Subtitle2, ToggleButton,
  makeStyles, tokens,
} from "@fluentui/react-components";
import { ChevronDownRegular, ChevronUpRegular, MailInboxRegular, SearchRegular } from "@fluentui/react-icons";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../../api/commands";
import { onEvent } from "../../api/events";
import { useAppStatus, useNavigation } from "../../app/AppStatusContext";
import { EmptyState, formatAppError, formatNumber } from "../../components/common";
import { DslBuilder, countLeaves, newGroup, toConditionGroup, type GroupDraft } from "../../components/dsl";
import { MailList, ReadingPane, useMailPages } from "../../components/mail";
import { EMPTY_REFINE, composeQuery, refineNodes, splitTerms, type RefineState } from "./compose";
import { RefinePane } from "./RefinePane";

const DEBOUNCE_MS = 300;
const COUNT_CAP = 10_000;
/** Non-fresh object: passes Fluent's slot type, carries the E2E selector to the native <input>. */
const SEARCH_INPUT = { "data-testid": "search-box", maxLength: 500 };

const useStyles = makeStyles({
  root: { display: "grid", gridTemplateColumns: "260px 1fr", height: "100%", minHeight: 0 },
  main: { display: "flex", flexDirection: "column", minWidth: 0, minHeight: 0 },
  bar: {
    display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS, padding: `${tokens.spacingVerticalS} ${tokens.spacingHorizontalL}`,
    borderBottom: `1px solid ${tokens.colorNeutralStroke2}`, backgroundColor: tokens.colorNeutralBackground1,
  },
  box: { flex: 1, maxWidth: "880px" },
  summary: { color: tokens.colorNeutralForeground3 },
  dsl: {
    padding: `${tokens.spacingVerticalS} ${tokens.spacingHorizontalL}`, maxHeight: "40%", overflowY: "auto",
    borderBottom: `1px solid ${tokens.colorNeutralStroke2}`, backgroundColor: tokens.colorNeutralBackground2,
  },
  hints: { display: "flex", flexDirection: "column", gap: tokens.spacingVerticalXS, padding: `0 ${tokens.spacingHorizontalL}` },
  results: { flex: 1, minHeight: 0, display: "grid", gridTemplateColumns: "minmax(420px, 34%) 1fr" },
  listPane: { display: "flex", flexDirection: "column", minHeight: 0, borderRight: `1px solid ${tokens.colorNeutralStroke2}` },
  paneHead: {
    display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS, minHeight: "44px", boxSizing: "border-box",
    padding: `0 ${tokens.spacingHorizontalM}`, borderBottom: `1px solid ${tokens.colorNeutralStroke2}`,
  },
  count: { color: tokens.colorNeutralForeground3 },
  fill: { flex: 1, minHeight: 0 },
  center: { display: "grid", placeItems: "center", height: "100%" },
});

interface Inputs {
  text: string;
  refine: RefineState;
  draft: GroupDraft;
}

/** Prototype `#screen-search`. */
export function SearchScreen() {
  const { t, i18n } = useTranslation("search");
  const s = useStyles();
  const qc = useQueryClient();
  const status = useAppStatus();
  const { go } = useNavigation();
  const lng = i18n.language;

  const [inputs, setInputs] = useState<Inputs>(() => ({ text: "", refine: EMPTY_REFINE, draft: newGroup("and", []) }));
  const [debounced, setDebounced] = useState(inputs);
  useEffect(() => {
    const h = setTimeout(() => setDebounced(inputs), DEBOUNCE_MS);
    return () => clearTimeout(h);
  }, [inputs]);
  const [dslOpen, setDslOpen] = useState(false);
  const [newMail, setNewMail] = useState(false);

  const composed = useMemo(
    () => composeQuery(debounced.text, debounced.refine, toConditionGroup(debounced.draft)),
    [debounced],
  );
  const hasMail = status.counts.messages > 0;
  const query = hasMail ? composed.query : null;

  // Part 03 §5.2 stale requests: every new composed query gets the next sequence number.
  const key = JSON.stringify(query);
  const [seqState, setSeqState] = useState({ key, seq: 0 });
  if (seqState.key !== key) setSeqState({ key, seq: seqState.seq + 1 });
  const seq = seqState.seq;

  const pages = useMailPages(query, seq);
  const count = useQuery({
    queryKey: ["searchCount", seq, query],
    queryFn: () => api.searchCount(query!),
    enabled: query !== null,
  });
  const filters = useQuery({ queryKey: ["filterOptions"], queryFn: () => api.filterOptions() });

  const [picked, setPicked] = useState<{ seq: number; id: number } | null>(null);
  const firstId = pages.rows[0]?.id ?? null;
  const selectedId = picked && picked.seq === seq ? picked.id : firstId;

  const showingRef = useRef(false);
  useEffect(() => {
    showingRef.current = query !== null;
  }, [query]);
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let alive = true;
    void onEvent("library:changed", () => {
      void qc.invalidateQueries({ queryKey: ["filterOptions"] });
      if (showingRef.current) setNewMail(true); // never refetches searchMail (Part 03 §9)
    }).then((u) => {
      if (alive) unlisten = u;
      else u();
    });
    return () => {
      alive = false;
      unlisten?.();
    };
  }, [qc]);

  const terms = splitTerms(inputs.text).length;
  const builderLeaves = countLeaves(inputs.draft);
  const leafBudget = 50 - 4 * terms - refineNodes(inputs.refine, new Date()).length;

  const clearAll = () => {
    const cleared = { text: "", refine: EMPTY_REFINE, draft: newGroup("and", []) };
    setInputs(cleared);
    setDebounced(cleared);
  };
  const refresh = () => {
    pages.reset();
    void qc.invalidateQueries({ queryKey: ["searchCount", seq] });
    setNewMail(false);
  };

  let countText: string | null = null;
  if (query !== null && count.data) {
    countText = count.data.capped
      ? t("countCapped", { n: formatNumber(COUNT_CAP, lng) })
      : t("count", { n: formatNumber(count.data.count, lng) });
  }

  let results;
  if (!hasMail) {
    results = (
      <EmptyState icon={<MailInboxRegular />} title={t("empty.title")} body={t("empty.body")}
        action={<Button appearance="primary" onClick={() => go("import")}>{t("common:action.goImport")}</Button>} />
    );
  } else if (query === null) {
    results = <EmptyState icon={<SearchRegular />} title={t("idle.title")} body={t("idle.body")} />;
  } else if (pages.error) {
    results = (
      <MessageBar intent="error">
        <MessageBarBody>{formatAppError(pages.error, t)}</MessageBarBody>
      </MessageBar>
    );
  } else if (pages.isLoading) {
    results = (
      <div className={s.center}>
        <Spinner />
      </div>
    );
  } else if (pages.rows.length === 0) {
    results = (
      <EmptyState icon={<SearchRegular />} title={t("zero.title")} body={t("zero.body")}
        action={<Button onClick={clearAll}>{t("zero.clear")}</Button>} />
    );
  } else {
    results = (
      <div className={s.results}>
        <section className={s.listPane}>
          <div className={s.paneHead}>
            <Subtitle2>{t("results")}</Subtitle2>
            {countText ? <Caption1 className={s.count} data-testid="result-count">{countText}</Caption1> : null}
          </div>
          <div className={s.fill}>
            <MailList rows={pages.rows} selectedId={selectedId} onSelect={(id) => setPicked({ seq, id })}
              hasMore={pages.hasMore} capped={pages.capped} loadingMore={pages.isFetchingNextPage}
              onEndReached={() => void pages.fetchNextPage()} ariaLabel={t("list.label")} />
          </div>
        </section>
        <ReadingPane mailId={selectedId} />
      </div>
    );
  }

  return (
    <div className={s.root}>
      <RefinePane value={inputs.refine} onChange={(refine) => setInputs((p) => ({ ...p, refine }))} options={filters.data} />
      <div className={s.main}>
        <div className={s.bar}>
          <SearchBox className={s.box} input={SEARCH_INPUT} placeholder={t("box.placeholder")} aria-label={t("box.label")}
            value={inputs.text} onChange={(_, d) => setInputs((p) => ({ ...p, text: d.value }))} />
          <ToggleButton data-testid="dsl-toggle" checked={dslOpen} onClick={() => setDslOpen((o) => !o)}
            icon={dslOpen ? <ChevronUpRegular /> : <ChevronDownRegular />} iconPosition="after">
            {t("dsl.toggle")}
            <CounterBadge count={builderLeaves} size="small" />
          </ToggleButton>
          {builderLeaves > 0 ? <Caption1 className={s.summary}>{t("dsl.summary", { n: builderLeaves })}</Caption1> : null}
        </div>
        {dslOpen ? (
          <div className={s.dsl}>
            <DslBuilder value={inputs.draft} onChange={(draft) => setInputs((p) => ({ ...p, draft }))}
              options={filters.data ?? { psts: [], folders: [] }} leafBudget={leafBudget} />
          </div>
        ) : null}
        <div className={s.hints}>
          {pages.shortQuery ? (
            <MessageBar intent="warning" data-testid="short-query-hint">
              <MessageBarBody>{t("shortQueryHint")}</MessageBarBody>
            </MessageBar>
          ) : null}
          {composed.problem ? (
            <MessageBar intent="error">
              <MessageBarBody>{t(`problem.${composed.problem}`)}</MessageBarBody>
            </MessageBar>
          ) : null}
          {newMail ? (
            <MessageBar intent="info" data-testid="new-mail-bar">
              <MessageBarBody>{t("newMail")}</MessageBarBody>
              <MessageBarActions>
                <Button size="small" data-testid="new-mail-refresh" onClick={refresh}>{t("refresh")}</Button>
              </MessageBarActions>
            </MessageBar>
          ) : null}
        </div>
        <div className={s.fill}>{results}</div>
      </div>
    </div>
  );
}
```
Typing is debounced as one `inputs` object (search text, refine, builder), so every change waits the same 300 ms and produces at most one sequence step. "Clear search and conditions" resets both the live and the debounced inputs at once.

Run: `npx vitest run src/screens/search`
Expected: PASS (`compose.test.ts` 3, `search.test.tsx` 5).

- [ ] **Step 19: Commit the Search screen.**
Run: `npm run typecheck` — expected exit 0. Run `git rev-parse --abbrev-ref HEAD` (expected `main`), then
```bash
printf 'feat(ui): search screen with refine pane, DSL panel, sequenced paging and live-update bar\n\nCo-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>\n' > scripts/tmp/msg-r107-4.txt
git add src/screens/search
git commit -F scripts/tmp/msg-r107-4.txt -- src/screens/search
git status --porcelain
```
Expected: commit succeeds; `git status --porcelain` lists no `src/screens/search` path.

- [ ] **Step 20: Write the failing Advanced SQL test** `src/screens/advanced-sql/advanced-sql.test.tsx`:
```tsx
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api/commands");
vi.mock("../../api/events", async () => (await import("../../app/testing")).eventsMock);
// The CodeMirror wrapper has its own test; here a textarea stands in so the screen logic runs without layout.
vi.mock("../../components/sql-editor", async () => {
  const { createElement } = await import("react");
  return {
    VIEW_COLUMNS: { v_mail: [["id", "int8"]] },
    SqlEditor: (p: { value: string; onChange(v: string): void; ariaLabel: string }) =>
      createElement("textarea", { "data-testid": "sql-editor", "aria-label": p.ariaLabel, value: p.value, onChange: (e: { target: { value: string } }) => p.onChange(e.target.value) }),
  };
});

import { api } from "../../api/commands";
import type { SqlResult } from "../../api/bindings/SqlResult";
import { mockApiDefaults, renderWithProviders } from "../../app/testing";
import { makeMailRow, stubViewport } from "../../components/mail/fixtures";
import { AdvancedSqlScreen } from ".";

const READY = { status: { counts: { messages: 10, psts: 1 } } };
const TABLE: SqlResult = {
  mode: "table",
  columns: [{ name: "kind", pgType: "text" }, { name: "n", pgType: "int8" }, { name: "note", pgType: "text" }],
  rows: [["to", "12", null], ["cc", "3", "x"]],
  truncated: false,
  elapsedMs: 4,
};

function runSql(sql: string) {
  fireEvent.change(screen.getByTestId("sql-editor"), { target: { value: sql } });
  fireEvent.click(screen.getByTestId("sql-run"));
}

describe("Advanced SQL screen", () => {
  let restore: () => void;
  beforeEach(() => {
    mockApiDefaults();
    restore = stubViewport(800);
  });
  afterEach(() => restore());

  it("sqlExecute is called with maxRows: 10000", async () => {
    renderWithProviders(<AdvancedSqlScreen />, READY);
    runSql("SELECT 1");
    await waitFor(() =>
      expect(api.sqlExecute).toHaveBeenCalledWith({ executionId: expect.stringMatching(/^[0-9a-f-]{36}$/), sql: "SELECT 1", maxRows: 10000 }),
    );
  });

  it("truncated: true shows the cap MessageBar", async () => {
    vi.mocked(api.sqlExecute).mockResolvedValue({ ...TABLE, truncated: true });
    renderWithProviders(<AdvancedSqlScreen />, READY);
    runSql("SELECT kind, n, note FROM t");
    expect((await screen.findByTestId("sql-truncated")).textContent).toContain("Showing the first 10,000 rows");
  });

  it("Advanced SQL shows MailList for a result with an id column and a table otherwise", async () => {
    vi.mocked(api.sqlExecute).mockResolvedValueOnce({
      mode: "mailList", rows: [makeMailRow(7, { subject: "from sql" })], missingIds: 1, truncated: false, elapsedMs: 2,
    });
    renderWithProviders(<AdvancedSqlScreen />, READY);
    runSql("SELECT id, subject FROM v_mail");
    await waitFor(() => expect(screen.getByTestId("sql-result").getAttribute("data-mode")).toBe("mailList"));
    expect(screen.getAllByTestId("mail-row")).toHaveLength(1);
    expect(screen.getByText("1 ids matched no message")).toBeTruthy();
    await waitFor(() => expect(api.mailGet).toHaveBeenCalledWith(7));
    vi.mocked(api.sqlExecute).mockResolvedValueOnce(TABLE);
    runSql("SELECT kind, n, note FROM t");
    await waitFor(() => expect(screen.getByTestId("sql-result").getAttribute("data-mode")).toBe("table"));
    expect(screen.getAllByTestId("sql-table-row")).toHaveLength(2);
    expect(screen.queryByTestId("mail-row")).toBeNull();
  });

  it("clicking a table row shows its key/value list in column 4", async () => {
    vi.mocked(api.sqlExecute).mockResolvedValue(TABLE);
    renderWithProviders(<AdvancedSqlScreen />, READY);
    runSql("SELECT kind, n, note FROM t");
    const rows = await screen.findAllByTestId("sql-table-row");
    expect(screen.queryByTestId("sql-row-detail")).toBeNull();
    fireEvent.click(rows[0]!);
    const detail = await screen.findByTestId("sql-row-detail");
    expect(within(detail).getByText("kind")).toBeTruthy();
    expect(within(detail).getByText("12")).toBeTruthy();
    expect(within(detail).getByText("NULL")).toBeTruthy();
    expect(screen.getByText("Row 1")).toBeTruthy();
  });

  it("Run is disabled while executing", async () => {
    let finish!: (r: SqlResult) => void;
    vi.mocked(api.sqlExecute).mockReturnValue(new Promise<SqlResult>((r) => (finish = r)));
    renderWithProviders(<AdvancedSqlScreen />, READY);
    runSql("SELECT pg_sleep(10)");
    await waitFor(() => expect((screen.getByTestId("sql-run") as HTMLButtonElement).disabled).toBe(true));
    const sent = vi.mocked(api.sqlExecute).mock.calls[0]![0];
    fireEvent.click(screen.getByTestId("sql-cancel"));
    await waitFor(() => expect(api.sqlCancel).toHaveBeenCalledWith(sent.executionId));
    finish({ mode: "command", tag: "SELECT", elapsedMs: 1 });
    await waitFor(() => expect((screen.getByTestId("sql-run") as HTMLButtonElement).disabled).toBe(false));
    expect((screen.getByTestId("sql-cancel") as HTMLButtonElement).disabled).toBe(true);
  });

  it("shows sqlError text with its position and the empty states", async () => {
    const empty = renderWithProviders(<AdvancedSqlScreen />);
    expect(screen.getByText("No mail to query yet")).toBeTruthy();
    empty.unmount();
    vi.mocked(api.sqlExecute).mockRejectedValue({ kind: "sqlError", sqlstate: "42601", message: 'syntax error at or near "SELEC"', position: 1 });
    renderWithProviders(<AdvancedSqlScreen />, READY);
    expect(screen.getByText("Run a query")).toBeTruthy();
    runSql("SELEC 1");
    expect(await screen.findByText('syntax error at or near "SELEC" at character 1')).toBeTruthy();
  });

  it("saves a new query by name and reports a duplicate name", async () => {
    vi.mocked(api.savedQueryCreate).mockRejectedValueOnce({ kind: "invalidInput", field: "name", reason: "duplicate" });
    renderWithProviders(<AdvancedSqlScreen />, READY);
    fireEvent.change(screen.getByTestId("sql-editor"), { target: { value: "SELECT 1" } });
    fireEvent.click(screen.getByTestId("sql-save"));
    const dialog = await screen.findByRole("dialog");
    fireEvent.change(within(dialog).getByRole("textbox"), { target: { value: "ones" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    expect(await within(dialog).findByText("A saved query with this name already exists.")).toBeTruthy();
    fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(api.savedQueryCreate).toHaveBeenLastCalledWith({ name: "ones", sql: "SELECT 1" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });
});
```
Run: `npx vitest run src/screens/advanced-sql`
Expected: FAIL — the kickoff `AdvancedSqlScreen` renders `null` (`Unable to find an element by: [data-testid="sql-editor"]`).

- [ ] **Step 21: Write `ResultTable.tsx`, `SavedQueries.tsx` and `index.tsx` under `src/screens/advanced-sql/`.**

`src/screens/advanced-sql/ResultTable.tsx`:
```tsx
import { makeStyles, mergeClasses, tokens } from "@fluentui/react-components";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useRef } from "react";
import { useTranslation } from "react-i18next";
import type { ColumnMeta } from "../../api/bindings/ColumnMeta";
import { ROW_HEIGHT } from "../../components/mail";

/** Part 03 §6.2: numeric cells are right-aligned and never parsed. */
export const NUMERIC_TYPES = new Set(["int2", "int4", "int8", "float4", "float8", "numeric"]);

const useStyles = makeStyles({
  scroll: { height: "100%", overflow: "auto", position: "relative" },
  head: {
    display: "grid", position: "sticky", top: 0, zIndex: 1, height: `${ROW_HEIGHT}px`, alignItems: "center",
    backgroundColor: tokens.colorNeutralBackground2, borderBottom: `1px solid ${tokens.colorNeutralStroke2}`,
    fontSize: tokens.fontSizeBase200, fontWeight: tokens.fontWeightSemibold,
  },
  row: {
    display: "grid", position: "absolute", top: 0, left: 0, width: "100%", height: `${ROW_HEIGHT}px`, alignItems: "center",
    boxSizing: "border-box", borderBottom: `1px solid ${tokens.colorNeutralStroke3}`, cursor: "pointer",
    ":hover": { backgroundColor: tokens.colorNeutralBackground1Hover },
  },
  selected: { backgroundColor: tokens.colorBrandBackground2, ":hover": { backgroundColor: tokens.colorBrandBackground2 } },
  cell: { padding: `0 ${tokens.spacingHorizontalS}`, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" },
  num: { textAlign: "right", fontVariantNumeric: "tabular-nums" },
  nul: { fontStyle: "italic", color: tokens.colorNeutralForeground4 },
});

export function ResultTable({ columns, rows, selected, onSelect }: {
  columns: ColumnMeta[];
  rows: Array<Array<string | null>>;
  selected: number | null;
  onSelect(index: number): void;
}) {
  const { t } = useTranslation("sql");
  const s = useStyles();
  const ref = useRef<HTMLDivElement>(null);
  const v = useVirtualizer({ count: rows.length, getScrollElement: () => ref.current, estimateSize: () => ROW_HEIGHT, overscan: 10 });
  const grid = { gridTemplateColumns: `repeat(${columns.length}, minmax(120px, 1fr))`, minWidth: `${columns.length * 120}px` };
  return (
    <div ref={ref} className={s.scroll} role="table" aria-rowcount={rows.length + 1}>
      <div role="row" className={s.head} style={grid}>
        {columns.map((c, j) => (
          <span key={j} role="columnheader" title={c.pgType} className={mergeClasses(s.cell, NUMERIC_TYPES.has(c.pgType) && s.num)}>
            {c.name}
          </span>
        ))}
      </div>
      <div style={{ height: `${v.getTotalSize()}px`, position: "relative", minWidth: grid.minWidth }}>
        {v.getVirtualItems().map((it) => {
          const row = rows[it.index]!;
          const pick = () => onSelect(it.index);
          return (
            <div key={it.index} role="row" tabIndex={0} aria-selected={selected === it.index} data-testid="sql-table-row"
              className={mergeClasses(s.row, selected === it.index && s.selected)}
              style={{ ...grid, transform: `translateY(${it.start}px)` }}
              onClick={pick}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  pick();
                }
              }}>
              {row.map((cell, j) => (
                <span key={j} role="cell" title={cell ?? undefined}
                  className={mergeClasses(s.cell, NUMERIC_TYPES.has(columns[j]?.pgType ?? "") && s.num, cell === null && s.nul)}>
                  {cell ?? t("result.null")}
                </span>
              ))}
            </div>
          );
        })}
      </div>
    </div>
  );
}
```

`src/screens/advanced-sql/SavedQueries.tsx`:
```tsx
import {
  Accordion, AccordionHeader, AccordionItem, AccordionPanel, Button, Caption1, Subtitle2, makeStyles, mergeClasses, tokens,
} from "@fluentui/react-components";
import { AddRegular, DocumentRegular } from "@fluentui/react-icons";
import { useTranslation } from "react-i18next";
import type { SavedQuery } from "../../api/bindings/SavedQuery";
import { VIEW_COLUMNS } from "../../components/sql-editor";

const useStyles = makeStyles({
  root: { display: "flex", flexDirection: "column", minHeight: 0, borderRight: `1px solid ${tokens.colorNeutralStroke2}`, backgroundColor: tokens.colorNeutralBackground2 },
  head: { display: "flex", alignItems: "center", minHeight: "44px", padding: `0 ${tokens.spacingHorizontalM}`, borderBottom: `1px solid ${tokens.colorNeutralStroke2}` },
  spacer: { flex: 1 },
  list: { flex: 1, overflowY: "auto", minHeight: 0, padding: tokens.spacingHorizontalXS },
  item: { width: "100%", justifyContent: "flex-start", fontWeight: tokens.fontWeightRegular },
  picked: { backgroundColor: tokens.colorBrandBackground2, fontWeight: tokens.fontWeightSemibold },
  empty: { padding: tokens.spacingHorizontalM, color: tokens.colorNeutralForeground3 },
  schema: { maxHeight: "45%", overflowY: "auto", borderTop: `1px solid ${tokens.colorNeutralStroke2}` },
  col: { display: "flex", gap: tokens.spacingHorizontalS, fontFamily: tokens.fontFamilyMonospace, fontSize: tokens.fontSizeBase200, paddingLeft: tokens.spacingHorizontalM },
  type: { color: tokens.colorNeutralForeground3 },
  view: { fontFamily: tokens.fontFamilyMonospace, fontWeight: tokens.fontWeightSemibold },
});

/** Prototype `#savedList` + `#schema` (column 1). */
export function SavedQueries({ items, selectedId, onPick, onNew }: {
  items: SavedQuery[];
  selectedId: number | null;
  onPick(q: SavedQuery): void;
  onNew(): void;
}) {
  const { t } = useTranslation("sql");
  const s = useStyles();
  return (
    <section className={s.root}>
      <div className={s.head}>
        <Subtitle2>{t("saved")}</Subtitle2>
        <span className={s.spacer} />
        <Button appearance="subtle" icon={<AddRegular />} aria-label={t("new")} title={t("new")} data-testid="sql-new" onClick={onNew} />
      </div>
      <div className={s.list}>
        {items.length === 0 ? <Caption1 className={s.empty}>{t("savedEmpty")}</Caption1> : null}
        {items.map((q) => (
          <Button key={q.id} appearance="subtle" icon={<DocumentRegular />} data-testid="saved-query" data-name={q.name}
            aria-current={q.id === selectedId ? "true" : undefined}
            className={mergeClasses(s.item, q.id === selectedId && s.picked)} onClick={() => onPick(q)}>
            {q.name}
          </Button>
        ))}
      </div>
      <Accordion collapsible className={s.schema}>
        <AccordionItem value="schema">
          <AccordionHeader>{t("schema")}</AccordionHeader>
          <AccordionPanel>
            {Object.entries(VIEW_COLUMNS).map(([view, cols]) => (
              <div key={view}>
                <div className={s.view}>{view}</div>
                {cols.map(([name, type]) => (
                  <div key={name} className={s.col}>
                    <span>{name}</span>
                    <span className={s.type}>{type}</span>
                  </div>
                ))}
              </div>
            ))}
          </AccordionPanel>
        </AccordionItem>
      </Accordion>
    </section>
  );
}
```

`src/screens/advanced-sql/index.tsx`:
```tsx
import {
  Badge, Button, Caption1, Dialog, DialogActions, DialogBody, DialogContent, DialogSurface, DialogTitle, Field, Input,
  MessageBar, MessageBarBody, Subtitle2, Toolbar, ToolbarButton, makeStyles, tokens,
} from "@fluentui/react-components";
import { DatabaseRegular, DeleteRegular, LockClosedRegular, PlayRegular, SaveRegular, StopRegular, TableRegular } from "@fluentui/react-icons";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../../api/commands";
import type { AppError } from "../../api/bindings/AppError";
import type { SavedQuery } from "../../api/bindings/SavedQuery";
import type { SqlExecuteRequest } from "../../api/bindings/SqlExecuteRequest";
import type { SqlResult } from "../../api/bindings/SqlResult";
import { useAppStatus, useNavigation } from "../../app/AppStatusContext";
import { EmptyState, formatAppError, formatNumber, toAppError, useAppToast } from "../../components/common";
import { MailList, ReadingPane } from "../../components/mail";
import { SqlEditor } from "../../components/sql-editor";
import { ResultTable } from "./ResultTable";
import { SavedQueries } from "./SavedQueries";

/** Part 03 §6.2 maximum; the screen always asks for it. */
export const MAX_ROWS = 10_000;
const noop = () => {};

const useStyles = makeStyles({
  root: { display: "grid", gridTemplateColumns: "1fr 3fr 1fr 3fr", height: "100%", minHeight: 0 },
  col: { display: "flex", flexDirection: "column", minWidth: 0, minHeight: 0, borderRight: `1px solid ${tokens.colorNeutralStroke2}` },
  toolbar: { borderBottom: `1px solid ${tokens.colorNeutralStroke2}`, gap: tokens.spacingHorizontalXS },
  spacer: { flex: 1 },
  editor: { flex: 1, minHeight: 0 },
  status: {
    display: "flex", gap: tokens.spacingHorizontalM, padding: `${tokens.spacingVerticalXS} ${tokens.spacingHorizontalM}`,
    borderTop: `1px solid ${tokens.colorNeutralStroke2}`, color: tokens.colorNeutralForeground3,
  },
  head: {
    display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS, minHeight: "44px", boxSizing: "border-box",
    padding: `0 ${tokens.spacingHorizontalM}`, borderBottom: `1px solid ${tokens.colorNeutralStroke2}`,
  },
  muted: { color: tokens.colorNeutralForeground3 },
  fill: { flex: 1, minHeight: 0 },
  detail: { display: "grid", gridTemplateColumns: "max-content 1fr", gap: `${tokens.spacingVerticalXS} ${tokens.spacingHorizontalM}`, margin: 0, padding: tokens.spacingHorizontalM, overflow: "auto" },
  key: { fontFamily: tokens.fontFamilyMonospace, color: tokens.colorNeutralForeground3 },
  value: { margin: 0, overflowWrap: "anywhere", whiteSpace: "pre-wrap" },
  nul: { fontStyle: "italic", color: tokens.colorNeutralForeground4 },
});

function SaveDialog({ onSave, onClose }: { onSave(name: string): Promise<void>; onClose(): void }) {
  const { t } = useTranslation("sql");
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const submit = async () => {
    try {
      await onSave(name.trim());
      onClose();
    } catch (e) {
      const err = toAppError(e);
      setError(err.kind === "invalidInput" && err.field === "name" && err.reason === "duplicate" ? t("save.duplicate") : formatAppError(err, t));
    }
  };
  return (
    <Dialog open modalType="modal" onOpenChange={(_, d) => !d.open && onClose()}>
      <DialogSurface>
        <DialogBody>
          <DialogTitle>{t("save.title")}</DialogTitle>
          <DialogContent>
            <Field label={t("save.name")} validationMessage={error ?? undefined} validationState={error ? "error" : "none"}>
              <Input value={name} maxLength={200} onChange={(_, d) => setName(d.value)} />
            </Field>
          </DialogContent>
          <DialogActions>
            <Button onClick={onClose}>{t("cancel")}</Button>
            <Button appearance="primary" disabled={name.trim() === ""} onClick={() => void submit()}>{t("save.action")}</Button>
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
}

function DeleteDialog({ query, onConfirm, onClose }: { query: SavedQuery; onConfirm(): void; onClose(): void }) {
  const { t } = useTranslation("sql");
  return (
    <Dialog open modalType="modal" onOpenChange={(_, d) => !d.open && onClose()}>
      <DialogSurface>
        <DialogBody>
          <DialogTitle>{t("delete.title", { name: query.name })}</DialogTitle>
          <DialogContent>{t("delete.body")}</DialogContent>
          <DialogActions>
            <Button onClick={onClose}>{t("cancel")}</Button>
            <Button appearance="primary" onClick={onConfirm}>{t("delete.confirm")}</Button>
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
}

/** Prototype `#screen-sql` / `#sql:table`: saved queries | editor | result | detail. */
export function AdvancedSqlScreen() {
  const { t, i18n } = useTranslation("sql");
  const s = useStyles();
  const qc = useQueryClient();
  const toast = useAppToast();
  const status = useAppStatus();
  const { go } = useNavigation();
  const lng = i18n.language;

  const [sql, setSql] = useState("");
  const [savedId, setSavedId] = useState<number | null>(null);
  const [result, setResult] = useState<SqlResult | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [executionId, setExecutionId] = useState<string | null>(null);
  const [mailId, setMailId] = useState<number | null>(null);
  const [rowIndex, setRowIndex] = useState<number | null>(null);
  const [dialog, setDialog] = useState<"save" | "delete" | null>(null);

  const saved = useQuery({ queryKey: ["savedQueries"], queryFn: () => api.savedQueryList() });
  const current = saved.data?.find((q) => q.id === savedId) ?? null;

  const run = useMutation({
    mutationFn: (req: SqlExecuteRequest) => api.sqlExecute(req),
    onMutate: (req) => {
      setExecutionId(req.executionId);
      setError(null);
    },
    onSuccess: (r) => {
      setResult(r);
      setRowIndex(null);
      setMailId(r.mode === "mailList" ? (r.rows[0]?.id ?? null) : null);
    },
    onError: (e) => setError(toAppError(e)),
    onSettled: () => setExecutionId(null),
  });
  const running = executionId !== null;
  const doRun = () => {
    if (running || sql.trim() === "") return;
    run.mutate({ executionId: crypto.randomUUID(), sql, maxRows: MAX_ROWS });
  };
  const cancel = () => {
    if (executionId) api.sqlCancel(executionId).catch(toast.error);
  };

  const refreshSaved = () => void qc.invalidateQueries({ queryKey: ["savedQueries"] });
  const saveAs = async (name: string) => {
    const q = await api.savedQueryCreate({ name, sql });
    setSavedId(q.id);
    refreshSaved();
    toast.notify(t("save.done", { name: q.name }));
  };
  const save = () => {
    if (!current) return setDialog("save");
    api.savedQueryUpdate(current.id, { name: current.name, sql })
      .then((q) => {
        refreshSaved();
        toast.notify(t("save.done", { name: q.name }));
      })
      .catch(toast.error);
  };
  const remove = () => {
    if (!current) return;
    setDialog(null);
    api.savedQueryDelete(current.id)
      .then(() => {
        setSavedId(null);
        refreshSaved();
      })
      .catch(toast.error);
  };
  const pick = (q: SavedQuery) => {
    setSavedId(q.id);
    setSql(q.sql);
    setError(null);
  };
  const newQuery = () => {
    setSavedId(null);
    setSql("");
    setError(null);
  };
  const edit = (next: string) => {
    setSql(next);
    if (error?.kind === "sqlError") setError(null);
  };

  const errorText = error
    ? formatAppError(error, t) + (error.kind === "sqlError" && error.position !== null ? ` ${t("error.position", { position: error.position })}` : "")
    : null;

  let statusLine;
  if (running) statusLine = <span>{t("status.running")}</span>;
  else if (result) {
    const n = result.mode === "command" ? 0 : result.rows.length;
    statusLine = (
      <>
        <span>{t("status.ok", { rows: formatNumber(n, lng), ms: formatNumber(result.elapsedMs, lng) })}</span>
        <span className={s.spacer} />
        <span>{t(`mode.${result.mode}`)}</span>
      </>
    );
  } else statusLine = <span>{t("status.idle")}</span>;

  let column3;
  let column4;
  if (!result) {
    column3 = status.counts.messages === 0 ? (
      <EmptyState icon={<DatabaseRegular />} title={t("empty.noMailTitle")} body={t("empty.noMailBody")}
        action={<Button appearance="primary" onClick={() => go("import")}>{t("common:action.goImport")}</Button>} />
    ) : (
      <EmptyState icon={<DatabaseRegular />} title={t("empty.noRunTitle")} body={t("empty.noRunBody")} />
    );
    column4 = <EmptyState icon={<TableRegular />} title={t("search:preview.pickTitle")} />;
  } else {
    const truncated = result.mode !== "command" && result.truncated ? (
      <MessageBar intent="warning" data-testid="sql-truncated">
        <MessageBarBody>{t("result.truncated", { n: formatNumber(MAX_ROWS, lng) })}</MessageBarBody>
      </MessageBar>
    ) : null;
    if (result.mode === "mailList") {
      column3 = (
        <>
          <div className={s.head}>
            <Subtitle2>{t("mode.mailList")}</Subtitle2>
            <Caption1 className={s.muted}>{t("result.rows", { n: formatNumber(result.rows.length, lng) })}</Caption1>
          </div>
          {truncated}
          {result.missingIds > 0 ? <Caption1 className={s.muted}>{t("result.missing", { n: formatNumber(result.missingIds, lng) })}</Caption1> : null}
          <div className={s.fill}>
            <MailList rows={result.rows} selectedId={mailId} onSelect={setMailId} hasMore={false} capped={false} loadingMore={false}
              onEndReached={noop} ariaLabel={t("mode.mailList")} monthHeaders={false} />
          </div>
        </>
      );
      column4 = <ReadingPane mailId={mailId} />;
    } else if (result.mode === "table") {
      const row = rowIndex === null ? null : result.rows[rowIndex];
      column3 = (
        <>
          <div className={s.head}>
            <Subtitle2>{t("mode.table")}</Subtitle2>
            <Caption1 className={s.muted}>{t("result.rows", { n: formatNumber(result.rows.length, lng) })}</Caption1>
          </div>
          {truncated}
          <div className={s.fill}>
            <ResultTable columns={result.columns} rows={result.rows} selected={rowIndex} onSelect={setRowIndex} />
          </div>
        </>
      );
      column4 = row ? (
        <>
          <div className={s.head}>
            <Subtitle2>{t("detail.title")}</Subtitle2>
            <Caption1 className={s.muted}>{t("detail.row", { n: (rowIndex ?? 0) + 1 })}</Caption1>
          </div>
          <dl className={s.detail} data-testid="sql-row-detail">
            {result.columns.map((c, j) => (
              <div key={j} style={{ display: "contents" }}>
                <dt className={s.key}>{c.name}</dt>
                <dd className={s.value}>{row[j] ?? <span className={s.nul}>{t("result.null")}</span>}</dd>
              </div>
            ))}
          </dl>
        </>
      ) : (
        <EmptyState icon={<TableRegular />} title={t("detail.pick")} />
      );
    } else {
      column3 = (
        <MessageBar intent="success">
          <MessageBarBody>{t("result.command", { tag: result.tag })}</MessageBarBody>
        </MessageBar>
      );
      column4 = <EmptyState icon={<TableRegular />} title={t("detail.pick")} />;
    }
  }

  return (
    <div className={s.root}>
      <SavedQueries items={saved.data ?? []} selectedId={savedId} onPick={pick} onNew={newQuery} />
      <section className={s.col}>
        <Toolbar className={s.toolbar}>
          <ToolbarButton appearance="primary" icon={<PlayRegular />} data-testid="sql-run" disabled={running || sql.trim() === ""} onClick={doRun}>
            {t("run")}
          </ToolbarButton>
          <ToolbarButton icon={<StopRegular />} data-testid="sql-cancel" disabled={!running} onClick={cancel}>{t("cancel")}</ToolbarButton>
          <ToolbarButton icon={<SaveRegular />} data-testid="sql-save" disabled={sql.trim() === ""} onClick={save}>{t("save.action")}</ToolbarButton>
          {current ? (
            <ToolbarButton icon={<DeleteRegular />} onClick={() => setDialog("delete")}>{t("delete.action")}</ToolbarButton>
          ) : null}
          <span className={s.spacer} />
          <Badge appearance="tint" color="informative" icon={<LockClosedRegular />}>{t("readOnly")}</Badge>
        </Toolbar>
        {errorText ? (
          <MessageBar intent="error">
            <MessageBarBody>{errorText}</MessageBarBody>
          </MessageBar>
        ) : null}
        <div className={s.editor}>
          <SqlEditor value={sql} onChange={edit} onRun={doRun} ariaLabel={t("editor")}
            errorPosition={error?.kind === "sqlError" ? error.position : null} />
        </div>
        <Caption1 className={s.status}>{statusLine}</Caption1>
      </section>
      <section className={s.col} data-testid="sql-result" data-mode={result?.mode ?? "none"}>
        {column3}
      </section>
      <section className={s.col}>{column4}</section>
      {dialog === "save" ? <SaveDialog onSave={saveAs} onClose={() => setDialog(null)} /> : null}
      {dialog === "delete" && current ? <DeleteDialog query={current} onConfirm={remove} onClose={() => setDialog(null)} /> : null}
    </div>
  );
}
```
`doRun` reads the latest `sql` because `SqlEditor` keeps `onRun` in a ref updated after every render, so Mod-Enter always runs the current text.

Run: `npx vitest run src/screens/advanced-sql`
Expected: PASS (7 tests).

- [ ] **Step 22: Full verification for the task.**
Run: `npm run typecheck`
Expected: exit 0.
Run: `npm test -- src/components/mail src/components/dsl src/components/sql-editor src/screens/search src/screens/advanced-sql`
Expected: every file passes (dsl 5, mail: pages 2 + mail 7, sql-editor 4, search: compose 3 + search 5, advanced-sql 7).
Run: `npm test -- src/app/global.test.tsx`
Expected: PASS (5 tests) — now with the real `SearchScreen` and `AdvancedSqlScreen` rendered in `cimode` and the `search` / `sql` namespaces in the parity and key-name checks.
Run: `grep -rn "dangerouslySetInnerHTML" src; echo "rc=$?"`
Expected: no line printed, `rc=1`.
Run: `grep -rln "@tauri-apps" src/components src/screens; echo "rc=$?"`
Expected: no file listed, `rc=1`.

- [ ] **Step 23: Commit the Advanced SQL screen.**
```bash
printf 'feat(ui): advanced SQL screen with saved queries, mail-list and table modes, row detail\n\nCo-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>\n' > scripts/tmp/msg-r107-5.txt
git add src/screens/advanced-sql
git commit -F scripts/tmp/msg-r107-5.txt -- src/screens/advanced-sql
git status --porcelain
```
Expected: commit succeeds; `git status --porcelain` lists no R1-07 path.

## Task R2-13: App composition, Tauri App Shell, Command Layer, Event Emitter, `mailimg` scheme, release build

**Files:**
- Create: `core/src/app/mod.rs` (kickoff file, empty until now), `core/tests/app_compose.rs`
- Modify: `src-tauri/src/lib.rs`, `src-tauri/src/commands/{mod,app,search,sql,import,settings}.rs`, `src-tauri/src/events.rs`, `src-tauri/src/mailimg.rs`, `src-tauri/tauri.conf.json` (`bundle.resources`)
- Modify only for defects or requests routed by the leader: `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, `core/Cargo.toml`, `core/src/lib.rs`, `core/src/dto/**`, `core/src/sink.rs`, `src/api/bindings/**` (regenerated by `cargo test`), `src-tauri/capabilities/**`, `src-tauri/rust-toolchain.toml`, `src-tauri/build.rs`, `src-tauri/icons/**`, `src-tauri/.cargo/config.toml`
- Test: unit test modules inside `src-tauri/src/lib.rs`, `src-tauri/src/events.rs`, `src-tauri/src/mailimg.rs`; `core/tests/app_compose.rs`

**Interfaces:**
- Consumes (C1–C3, planner-1): `dto::*` (incl. `dto::commands::COMMANDS: [&str; 24]`), `sink::{EventSink, CoreEvent, RecordingSink}` with `CoreEvent::{name, payload}`, `gate::Gate`.
- Consumes (C6–C8, planner-1): `paths::{Paths, dev_pg_dir}` (`Paths::new(app_local_data_dir, app_log_dir, pg_dir)`), `lifecycle::pg::{PgController, PgStatus}`, `db::Db`, `db::indexes::missing_or_invalid`, `test_support::TestCluster` is not used here (the tests build real `Services`).
- Consumes (C14 / C15, planner-1, R2-08): `settings::SettingsStore::{load(PathBuf), get(&Db), update(&Db, SettingsPatch)}`; `lifecycle::supervisor::{Supervisor, SupervisorConfig}` with `Supervisor::new(paths, settings, gate, db, sink, config)`, `phase()`, `phase_watch()`, `status()`, `boot()`, `initialize(req)`, `retry()`, `move_data_dir(target)`, `db_status(Option<MaintenanceState>)`, `shutdown_db()`.
- Consumes (C14, planner-2 — confirmed against the Produces blocks of `docs/superpowers/plans/2026-10-21-outlooker-plan-backend.md` headings "Task R2-09", "Task R2-11", "Task R2-12"; planner-1's Supervisor / SettingsStore / Paths in `…-plan-foundation.md` "Task R2-08" and "Task R1-03"):
  - `importer::{ImportManager, ImportTuning, RecoveryReport}`: `ImportManager::new(db: Db, gate: Gate, paths: Paths, sink: Arc<dyn EventSink>, phase: watch::Receiver<AppPhase>, tuning: ImportTuning) -> ImportManager` (Clone); `list() -> Result<Vec<PstSource>, AppError>`; `enqueue(paths: Vec<String>) -> Result<Vec<EnqueueResult>, AppError>`; `cancel(pst_id: i64)`, `resume(pst_id: i64)`, `remove(pst_id: i64) -> Result<(), AppError>`; `errors(req: ImportErrorsRequest) -> Result<ImportErrorPage, AppError>`; `recover_after_crash() -> Result<RecoveryReport, AppError>` with `RecoveryReport { requeued: Vec<i64>, cancelled: Vec<i64> }` (pst ids); `self_heal_indexes() -> Result<(), AppError>` (holds gate `Import`, runs `ensure_search_indexes` on an admin connection); `start_worker()` (sync, idempotent); `shutdown(timeout: Duration) -> Result<(), AppError>` (running job → `Queued`, last batch + checkpoint committed); `job_events() -> tokio::sync::broadcast::Receiver<(i64, ImportStatus)>` (terminal job outcomes; used by `app_compose` to wait without polling); `ImportTuning { writer: WriterLimits, crash_after_batches: Option<i64>, hold_reader_after: Option<i64>, on_batch: Option<Arc<dyn Fn(i64) + Send + Sync>> }`, `Default` = production (all seams `None`); `hold_reader_after: Some(n)` blocks the reader after `n` items until the job is cancelled (user cancel, shutdown or DB pause).
  - `search::SearchService`: `new(db: Db) -> SearchService` (Clone); `search(req: SearchRequest) -> Result<SearchPage, AppError>`; `count(query: SearchQuery) -> Result<SearchCount, AppError>`; `filter_options() -> Result<FilterOptions, AppError>`; `mail_get(id: i64) -> Result<MailDetail, AppError>`; `image(id: i64) -> Result<Option<Vec<u8>>, AppError>`.
  - `sql_console::SqlConsole`: `new(db: Db) -> SqlConsole` (Clone); `saved_list() -> Result<Vec<SavedQuery>, AppError>`; `saved_create(input: SavedQueryInput) -> Result<SavedQuery, AppError>`; `saved_update(id: i64, input: SavedQueryInput) -> Result<SavedQuery, AppError>`; `saved_delete(id: i64) -> Result<(), AppError>`; `execute(req: SqlExecuteRequest) -> Result<SqlResult, AppError>`; `cancel(execution_id: String) -> Result<(), AppError>`; `cancel_running()` (async, infallible).
  - `maintenance::Maintenance`: `new(db: Db, gate: Gate, sink: Arc<dyn EventSink>) -> Maintenance` (Clone); `run(task: MaintenanceTask) -> Result<(), AppError>` (returns once started); `current() -> Option<MaintenanceState>` (sync).
- Produces (`outlooker_core::app`, consumed by `src-tauri/src/lib.rs` and the tests):
  ```rust
  pub struct AppDirs { pub app_local_data_dir: PathBuf, pub app_log_dir: PathBuf, pub pg_dir: PathBuf }
  #[derive(Default)] pub struct BuildOptions { pub supervisor: SupervisorConfig, pub import_tuning: ImportTuning }
  #[derive(Clone)]
  pub struct Services {
      pub paths: Paths, pub db: Db, pub gate: Gate, pub sink: Arc<dyn EventSink>,
      pub settings: SettingsStore, pub supervisor: Supervisor, pub importer: ImportManager,
      pub search: SearchService, pub sql: SqlConsole, pub maintenance: Maintenance,
  }
  pub fn build(dirs: AppDirs, sink: Arc<dyn EventSink>, opts: BuildOptions) -> Result<Services, AppError>;
  #[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum StartStep { Recovered, IndexesEnsured, WorkerStarted }
  #[derive(Debug, Clone, PartialEq)] pub struct StartReport { pub steps: Vec<StartStep>, pub recovery: RecoveryReport }
  pub async fn start_services(s: &Services) -> Result<StartReport, AppError>;
  #[derive(Debug, Clone, Copy)] pub struct ShutdownLimits { pub import_drain: Duration }   // Default: 15 s
  #[derive(Debug, Clone, PartialEq, Eq)] pub struct ShutdownReport { pub import_drained: bool, pub db_stopped: bool }
  pub async fn shutdown(s: &Services, limits: ShutdownLimits) -> ShutdownReport;
  ```
- Produces (Command Layer, `src-tauri/src/commands/*.rs`; one `#[tauri::command] pub async fn` per command, first parameter `s: tauri::State<'_, Services>`, parameter names = the C4 argument keys in snake_case, return `Result<T, AppError>`):

  | File | Commands (name → service call) |
  |---|---|
  | `commands/app.rs` | `app_status() -> AppStatus` → `supervisor.status()`; `app_initialize(req: InitializeRequest) -> ()` → `supervisor.initialize(req)`; `app_retry() -> ()` → `supervisor.retry()`; `data_dir_move(target: String) -> ()` → `supervisor.move_data_dir(target)`; `db_status() -> DbStatus` → `supervisor.db_status(maintenance.current())` |
  | `commands/search.rs` | `search_mail(req: SearchRequest) -> SearchPage`; `search_count(query: SearchQuery) -> SearchCount`; `filter_options() -> FilterOptions`; `mail_get(id: i64) -> MailDetail` → `search.*` |
  | `commands/sql.rs` | `saved_query_list() -> Vec<SavedQuery>`; `saved_query_create(input: SavedQueryInput) -> SavedQuery`; `saved_query_update(id: i64, input: SavedQueryInput) -> SavedQuery`; `saved_query_delete(id: i64) -> ()`; `sql_execute(req: SqlExecuteRequest) -> SqlResult`; `sql_cancel(execution_id: String) -> ()` → `sql.*` |
  | `commands/import.rs` | `pst_list() -> Vec<PstSource>`; `import_enqueue(paths: Vec<String>) -> Vec<EnqueueResult>`; `import_cancel(pst_id: i64) -> ()`; `import_resume(pst_id: i64) -> ()`; `pst_remove(pst_id: i64) -> ()`; `import_errors(req: ImportErrorsRequest) -> ImportErrorPage` → `importer.*` |
  | `commands/settings.rs` | `settings_get() -> Settings` → `settings.get(&db)`; `settings_update(patch: SettingsPatch) -> Settings` → `settings.update(&db, patch)`; `maintenance_run(task: MaintenanceTask) -> ()` → `maintenance.run(task)` |

- Produces (Event Emitter, `src-tauri/src/events.rs`):
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)] pub enum ThrottleKey { ImportProgress(i64), LibraryChanged, DataDirMoveProgress }
  pub const IMPORT_PROGRESS_WINDOW: Duration = Duration::from_millis(250);
  pub const LIBRARY_CHANGED_WINDOW: Duration = Duration::from_secs(5);
  pub const MOVE_PROGRESS_WINDOW: Duration = Duration::from_millis(250);
  pub fn throttle_key(e: &CoreEvent) -> Option<(ThrottleKey, Duration)>;
  #[derive(Default)] pub struct Throttle { /* HashMap<ThrottleKey, Slot> */ }
  impl Throttle {
      pub fn offer(&mut self, e: CoreEvent, now: Instant) -> Vec<CoreEvent>;   // events to emit now
      pub fn due(&mut self, now: Instant) -> Vec<CoreEvent>;                   // trailing values whose window ended
      pub fn next_deadline(&self) -> Option<Instant>;
  }
  pub struct TauriSink { /* AppHandle + Arc<Mutex<Throttle>> + Arc<Notify> */ }
  impl TauriSink { pub fn new(app: tauri::AppHandle) -> Arc<TauriSink>; }   // spawns the trailing-flush task
  impl EventSink for TauriSink { .. }
  ```
- Produces (`src-tauri/src/mailimg.rs`):
  ```rust
  pub async fn respond<F, Fut>(method: &tauri::http::Method, path: &str, load: F) -> tauri::http::Response<Vec<u8>>
  where F: FnOnce(i64) -> Fut, Fut: Future<Output = Result<Option<Vec<u8>>, AppError>>;
  pub fn unavailable() -> tauri::http::Response<Vec<u8>>;   // 503, used before Services exist
  ```
- Produces (for R2-14): cargo feature `e2e` registers `tauri_plugin_wdio_webdriver::init()` and makes the app read `OUTLOOKER_APP_DATA_DIR` (absolute path) as `app_local_data_dir`; the debug E2E binary is `src-tauri/target/debug/outlooker` (`outlooker.exe` on Windows); `bundle.resources` = `["resources/postgresql/"]`, so the PostgreSQL tree resolves at `<resource dir>/resources/postgresql` in both the bundle and `target/<profile>/`.

**Behavior:**
- **`build`.** `Paths::new(dirs…)`, `SettingsStore::load(paths.bootstrap_file())`, `Gate::new()`, `Db::new()`, `Supervisor::new(paths.clone(), settings.clone(), gate.clone(), db.clone(), sink.clone(), opts.supervisor)`, `ImportManager::new(db, gate, paths, sink, supervisor.phase_watch(), opts.import_tuning)`, `SearchService::new(db)`, `SqlConsole::new(db)`, `Maintenance::new(db, gate, sink)`. No I/O except loading the bootstrap file; no task is spawned.
- **`start_services`** (Part 02 §2.2 steps 2–9): `supervisor.boot().await` (its error is logged, not returned: the phase is then `Failed` and `app_retry` can recover); then wait on `phase_watch()` until the phase is `Ready` (this also covers first run: the wait ends after onboarding's `app_initialize`); then in order `importer.recover_after_crash()` → push `Recovered`; `importer.self_heal_indexes()` → push `IndexesEnsured` (an error is logged and the worker still starts, because search works without the indexes); `importer.start_worker()` → push `WorkerStarted`. Returns the report. It runs once per process: later `Ready` transitions (PG restart, data dir move) are handled inside `ImportManager` through C15.
- **`shutdown`** (Part 02 §2.5): `importer.shutdown(limits.import_drain)` (`import_drained` = `Ok`; a timeout or error is logged); `sql.cancel_running()`; `supervisor.shutdown_db()` (closes pools, `StoppingDb`, `pg_ctl stop -m fast -w -t 30`, immediate on timeout — inside C7) → `db_stopped` = `Ok`. Never panics.
- **App Shell (`lib.rs`).** Plugin order: `tauri_plugin_single_instance::init` first (focus + unminimize `main`), then dialog, log, and with feature `e2e` `tauri_plugin_wdio_webdriver::init()`. `setup`: resolve `AppDirs` (`app_local_data_dir` from `OUTLOOKER_APP_DATA_DIR` under feature `e2e` when set, else `path().app_local_data_dir()`; `app_log_dir()`; `path().resolve("resources/postgresql", BaseDirectory::Resource)`), `TauriSink::new`, `app::build`, `app.manage(services.clone())`, `tauri::async_runtime::spawn(start_services)`. `invoke_handler(generate_handler![…24 commands…])`. `register_asynchronous_uri_scheme_protocol("mailimg", …)`: spawn on `tauri::async_runtime`, read `Services` with `try_state` (missing → `unavailable()`), call `mailimg::respond(request.method(), request.uri().path(), |id| search.image(id))`, then `responder.respond(response)`. `build(generate_context!())` then `run(handler)`: on `RunEvent::ExitRequested { api, code, .. }` — if the shutdown already finished, do nothing (exit proceeds); if `code` is `None` (window close, Cmd+Q, logoff): `api.prevent_exit()`, start the shutdown once (atomic flag) on the async runtime, then `app_handle.exit(0)`; if `code` is `Some` (exit requested through `AppHandle::exit`, where `prevent_exit` is ignored) and the shutdown has not run: run it with `tauri::async_runtime::block_on`.
- **Throttle (`events.rs`).** `throttle_key`: `ImportProgress(p)` → `(ImportProgress(p.pst_id), 250 ms)`; `LibraryChanged` → `(LibraryChanged, 5 s)`; `DataDirMoveProgress` → `(DataDirMoveProgress, 250 ms)`; the other three events → `None` (emitted at once). `offer(e, now)`: unkeyed → `[e]`, and if `e` is `ImportStatusChanged` with `removed` or status `Completed | Failed | Cancelled | Queued`, the pending `LibraryChanged` (if any) is flushed with it ("plus once when a job ends", Part 03 §9). Keyed: emit now when the slot never emitted, or `now − last_emit ≥ window`, or the event's phase differs from the last emitted phase of the slot (`ImportProgress.phase`, `DataDirMoveProgress.phase`; Part 03 §9 "plus once at each phase change"); emitting now clears the slot's pending value and sets `last_emit = now`. Otherwise the event replaces the slot's pending value (trailing value). `due(now)`: every slot with a pending value and `now ≥ last_emit + window` emits it (`last_emit = now`). `next_deadline`: min over pending slots of `last_emit + window`.
- **TauriSink.** `emit` locks the throttle, calls `offer(e, Instant::now())`, emits each result with `app.emit(e.name(), e.payload())` (errors logged), then `notify_one()`; the flush task loops: `timeout_at(next_deadline, notify.notified())` when a deadline exists, else `notify.notified().await`; then emits `due(Instant::now())`. A deadline timer, not a polling loop.
- **mailimg (`respond`).** Method ≠ `GET` → `405`; `path` without the leading `/` not a positive decimal `i64` → `400`; `load(id)`: `Ok(Some(bytes))` → `200`, `Content-Type: image/webp`, `Cache-Control: max-age=31536000, immutable`; `Ok(None)` → `404`; `Err(DbNotReady)` → `503`; other `Err` → `500` (logged). Bodies of non-200 responses are empty. No file path is ever read.
- **`tauri.conf.json`.** Adds `"resources": ["resources/postgresql/"]` under `bundle`; CSP stays as R1-01 wrote it (`default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: mailimg: http://mailimg.localhost; frame-src 'self'; connect-src ipc: http://ipc.localhost; font-src 'self' data:`); `bundle.targets` changes from R1-01's `["dmg", "nsis"]` to `"all"` (dmg + app on macOS, nsis + msi on Windows; agreed with planner-1, request P1-6). From this commit on, `node scripts/fetch-postgres.mjs` must run before any `cargo build` of the app crate (tauri-build copies the resource dir and fails when it is missing).

**Constraints:**
- Command functions contain no logic beyond one service call (`grep -c "await" src-tauri/src/commands/*.rs` equals the number of commands per file, checked in review).
- `core::app` imports every service; no service imports `core::app`.
- `tauri-plugin-wdio-webdriver` only behind feature `e2e`; default features stay empty.
- `mailimg` and the exit handler never block the main thread except the documented `block_on` fallback.
- No `sleep` in tests: throttle tests drive `Throttle` with explicit `Instant`s; `app_compose` waits with `tokio::time::timeout` around awaited futures (a bound, not a poll).

**Acceptance criteria:**
- [ ] `cargo test -p outlooker-core --features test-support --test app_compose` exits 0 with tests: `shutdown_during_import_requeues_job` (`shutdown` during a running import → job status `Queued` (not `Cancelled`), checkpoint committed, `pg_ctl status` reports no server, within the 15 s + 30 s budgets), `start_after_kill_recovers_jobs_then_heals_then_starts_worker` (`start_services` after a simulated kill → `Running` jobs become `Queued`, `Cancelling` become `Cancelled`, then `ensure_search_indexes`, then the queue starts; order recorded in `StartReport.steps` and the recovery in `StartReport.recovery`, and every worker `import:status-changed` of the requeued PST with a status other than `queued` is emitted while all search indexes are valid — the test sink records index validity at each such event, the list is non-empty and all `true`; "simulated kill" = `supervisor.shutdown_db()` without `importer.shutdown`, so job rows stay `running` / `cancelling`), `missing_index_is_rebuilt_before_worker` (start with one search index missing → after `Ready` it exists and is valid (`missing_or_invalid` empty), and the queue worker started only after that).
- [ ] `cargo test -p outlooker` exits 0 with tests: `generate_handler_list_equals_commands` (the `generate_handler!` list equals `COMMANDS`), `single_instance_plugin_is_first` (the single-instance plugin is the first `.plugin(` call in `lib.rs`, source check), `throttle_at_most_four_per_second_per_key`, `throttle_always_emits_trailing_value`, `throttle_emits_phase_change_immediately` (inside the 250 ms window), `job_end_flushes_library_changed`, `mailimg_post_is_405`, `mailimg_non_integer_is_400` (`abc`), `mailimg_unknown_id_is_404`, `mailimg_seeded_id_is_200_webp` (a `load` closure over a seeded `HashMap<i64, Vec<u8>>`, headers checked), `mailimg_db_not_ready_is_503`.
- [ ] `cargo build --release -p outlooker --locked` exits 0, and `cargo tree -p outlooker -e normal | grep -c wdio` prints `0` (default features exclude `e2e`; judge the printed count — grep exits 1 when the count is 0).
- [ ] `npm run tauri build` exits 0 on macOS and produces a `.dmg`; its size is recorded in the R2 gate log (`ls -l src-tauri/target/release/bundle/dmg/*.dmg`).
- [ ] Added: `cargo build -p outlooker --features e2e --locked` exits 0 (the E2E binary of R2-14 builds).
- [ ] Added: after `cargo test --workspace --locked`, `git diff --exit-code src/api/bindings` exits 0.

**Steps:**

Paths: `core/` = `src-tauri/crates/outlooker-core/`. Every `cargo` command runs in `src-tauri/`; every `npm` / `node` command at the repo root. Commit protocol of Global Constraints: `git rev-parse --abbrev-ref HEAD` before, `git add <listed paths>` then `git commit -F <msgfile> -- <listed paths>` (message file under `scripts/tmp/`, no `;` in the text), `git status --porcelain` after (no entry of your own paths). Test commands that need PostgreSQL require `node scripts/fetch-postgres.mjs` once (it is idempotent).

Order: Steps 1–2 land first (the E2E binary that R2-14's smoke dry run needs). Steps 3–12 (Event Emitter, `mailimg`) need no other Round 2 task. Steps 13–17 (`core::app`) need R2-08, R2-09, R2-10, R2-11, R2-12 landed. Steps 18–24 (Command Layer, App Shell, resources) need Step 16. Step 25 is the release check.

- [ ] **Step 1: Register the WebDriver plugin under feature `e2e`.** Replace `src-tauri/src/lib.rs` (kickoff content, foundation R1-01 Step 4) with:

```rust
mod commands;
mod events;
mod mailimg;

use tauri::Manager;

pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_log::Builder::new().build());
    // ponytail: E2E-only WebDriver server; never compiled into the default (release) build.
    #[cfg(feature = "e2e")]
    let builder = builder.plugin(tauri_plugin_wdio_webdriver::init());
    builder
        .run(tauri::generate_context!())
        .expect("error while running Outlooker");
}
```

- [ ] **Step 2: Build both variants and commit.**

Run: `cargo build -p outlooker --features e2e --locked`
Expected: exit 0 (`Finished` line).
Run: `cargo tree -p outlooker -e normal --features e2e | grep -c wdio`
Expected: a number ≥ 1.
Run: `cargo tree -p outlooker -e normal | grep -c wdio`
Expected: prints `0` (grep exits 1 when the count is 0; judge the printed number).
Commit `src-tauri/src/lib.rs` with message `feat(shell): register tauri-plugin-wdio-webdriver under cargo feature e2e`.

- [ ] **Step 3: Write the failing throttle tests.** Write `src-tauri/src/events.rs` (kickoff file, empty) with only this test module (the types do not exist yet, so it fails to compile):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use outlooker_core::dto::app::LibraryCounts;
    use outlooker_core::dto::events::{DataDirMoveProgress, ImportStatusChanged, LibraryChanged};
    use outlooker_core::dto::import::ImportProgress;

    fn progress(pst_id: i64, phase: ImportPhase, items_done: i64) -> CoreEvent {
        CoreEvent::ImportProgress(ImportProgress {
            pst_id,
            phase,
            items_total: Some(1000),
            items_done,
            stored: items_done,
            duplicates: 0,
            errors: 0,
            skipped_non_mail: 0,
            images_converted: 0,
            bytes_per_sec: None,
            eta_seconds: None,
        })
    }

    fn library(messages: i64) -> CoreEvent {
        CoreEvent::LibraryChanged(LibraryChanged { counts: LibraryCounts { messages, psts: 1 } })
    }

    fn status(pst_id: i64, status: ImportStatus) -> CoreEvent {
        CoreEvent::ImportStatusChanged(ImportStatusChanged { pst_id, status, removed: false, source: None })
    }

    fn moving(phase: DataDirMovePhase) -> CoreEvent {
        CoreEvent::DataDirMoveProgress(DataDirMoveProgress { phase, bytes_copied: 0, bytes_total: 10, error: None })
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn throttle_at_most_four_per_second_per_key() {
        let t0 = Instant::now();
        let mut th = Throttle::default();
        let mut emitted: Vec<Duration> = Vec::new();
        for i in 0..100u64 {
            let now = t0 + ms(i * 10);
            for _ in th.offer(progress(1, ImportPhase::Importing, i as i64), now) {
                emitted.push(now - t0);
            }
            for _ in th.due(now) {
                emitted.push(now - t0);
            }
        }
        assert!(emitted.len() >= 4, "too few events: {emitted:?}");
        for &start in &emitted {
            let n = emitted.iter().filter(|&&t| t >= start && t < start + Duration::from_secs(1)).count();
            assert!(n <= 4, "{n} events in the 1 s window starting at {start:?}: {emitted:?}");
        }
    }

    #[test]
    fn throttle_always_emits_trailing_value() {
        let t0 = Instant::now();
        let mut th = Throttle::default();
        assert_eq!(th.offer(progress(1, ImportPhase::Importing, 1), t0).len(), 1);
        assert!(th.offer(progress(1, ImportPhase::Importing, 2), t0 + ms(10)).is_empty());
        assert!(th.offer(progress(1, ImportPhase::Importing, 3), t0 + ms(20)).is_empty());
        assert!(th.due(t0 + ms(100)).is_empty());
        assert_eq!(th.next_deadline(), Some(t0 + ms(250)));
        assert_eq!(th.due(t0 + ms(250)), vec![progress(1, ImportPhase::Importing, 3)]);
        assert_eq!(th.next_deadline(), None);
        assert_eq!(th.offer(library(1), t0).len(), 1);
        assert!(th.offer(library(2), t0 + ms(1000)).is_empty());
        assert!(th.due(t0 + ms(4999)).is_empty());
        assert_eq!(th.due(t0 + Duration::from_secs(5)), vec![library(2)]);
    }

    #[test]
    fn throttle_emits_phase_change_immediately() {
        let t0 = Instant::now();
        let mut th = Throttle::default();
        assert_eq!(th.offer(progress(1, ImportPhase::Scanning, 0), t0).len(), 1);
        assert!(th.offer(progress(1, ImportPhase::Scanning, 0), t0 + ms(10)).is_empty());
        assert_eq!(
            th.offer(progress(1, ImportPhase::Importing, 1), t0 + ms(50)),
            vec![progress(1, ImportPhase::Importing, 1)]
        );
        assert!(th.offer(progress(1, ImportPhase::Importing, 2), t0 + ms(60)).is_empty());
        assert_eq!(th.offer(progress(1, ImportPhase::Finalizing, 3), t0 + ms(70)).len(), 1);
        assert_eq!(th.offer(progress(2, ImportPhase::Importing, 1), t0 + ms(70)).len(), 1, "each PST has its own key");
        assert_eq!(th.offer(moving(DataDirMovePhase::Copying), t0).len(), 1);
        assert!(th.offer(moving(DataDirMovePhase::Copying), t0 + ms(5)).is_empty());
        assert_eq!(th.offer(moving(DataDirMovePhase::Verifying), t0 + ms(6)).len(), 1);
    }

    #[test]
    fn job_end_flushes_library_changed() {
        let t0 = Instant::now();
        let mut th = Throttle::default();
        assert_eq!(th.offer(library(1), t0).len(), 1);
        assert!(th.offer(library(2), t0 + ms(1000)).is_empty());
        assert_eq!(
            th.offer(status(1, ImportStatus::Running), t0 + ms(1500)),
            vec![status(1, ImportStatus::Running)],
            "a running job does not flush"
        );
        assert_eq!(
            th.offer(status(1, ImportStatus::Completed), t0 + ms(2000)),
            vec![status(1, ImportStatus::Completed), library(2)]
        );
        assert!(th.due(t0 + Duration::from_secs(60)).is_empty(), "the flushed value is not emitted twice");
        assert_eq!(th.offer(status(1, ImportStatus::Completed), t0 + ms(2100)).len(), 1);
    }
}
```

- [ ] **Step 4: Run the tests to see them fail.**

Run: `cargo test -p outlooker --locked events::`
Expected: compile error `cannot find type `Throttle` in this scope` (plus unresolved `ImportPhase`, `CoreEvent`).

- [ ] **Step 5: Write the throttle and the Tauri sink.** Insert above the test module in `src-tauri/src/events.rs`:

```rust
//! Event Emitter (spec Part 02 §3.1, Part 03 §9): per-key throttle and the Tauri `EventSink`.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use outlooker_core::dto::events::DataDirMovePhase;
use outlooker_core::dto::import::{ImportPhase, ImportStatus};
use outlooker_core::sink::{CoreEvent, EventSink};
use tauri::{AppHandle, Emitter};
use tokio::sync::Notify;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThrottleKey {
    ImportProgress(i64),
    LibraryChanged,
    DataDirMoveProgress,
}

pub const IMPORT_PROGRESS_WINDOW: Duration = Duration::from_millis(250);
pub const LIBRARY_CHANGED_WINDOW: Duration = Duration::from_secs(5);
pub const MOVE_PROGRESS_WINDOW: Duration = Duration::from_millis(250);

/// Throttled events and their windows; every other event is emitted at once.
pub fn throttle_key(e: &CoreEvent) -> Option<(ThrottleKey, Duration)> {
    match e {
        CoreEvent::ImportProgress(p) => Some((ThrottleKey::ImportProgress(p.pst_id), IMPORT_PROGRESS_WINDOW)),
        CoreEvent::LibraryChanged(_) => Some((ThrottleKey::LibraryChanged, LIBRARY_CHANGED_WINDOW)),
        CoreEvent::DataDirMoveProgress(_) => Some((ThrottleKey::DataDirMoveProgress, MOVE_PROGRESS_WINDOW)),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Import(ImportPhase),
    Move(DataDirMovePhase),
}

fn phase(e: &CoreEvent) -> Option<Phase> {
    match e {
        CoreEvent::ImportProgress(p) => Some(Phase::Import(p.phase)),
        CoreEvent::DataDirMoveProgress(p) => Some(Phase::Move(p.phase)),
        _ => None,
    }
}

/// Part 03 §9 "plus once when a job ends": these status changes flush a pending `library:changed`.
fn ends_job(e: &CoreEvent) -> bool {
    matches!(e, CoreEvent::ImportStatusChanged(c)
        if c.removed
            || matches!(c.status, ImportStatus::Completed | ImportStatus::Failed | ImportStatus::Cancelled | ImportStatus::Queued))
}

struct Slot {
    window: Duration,
    last_emit: Option<Instant>,
    last_phase: Option<Phase>,
    pending: Option<CoreEvent>,
}

impl Slot {
    fn window_over(&self, now: Instant) -> bool {
        self.last_emit.is_none_or(|t| now.duration_since(t) >= self.window)
    }

    fn mark(&mut self, now: Instant, e: &CoreEvent) {
        self.pending = None;
        self.last_emit = Some(now);
        self.last_phase = phase(e);
    }
}

/// Pure throttle state; time is passed in, so tests drive it with explicit `Instant`s.
#[derive(Default)]
pub struct Throttle {
    slots: HashMap<ThrottleKey, Slot>,
}

impl Throttle {
    /// Events to emit now for `e` offered at `now`.
    pub fn offer(&mut self, e: CoreEvent, now: Instant) -> Vec<CoreEvent> {
        let Some((key, window)) = throttle_key(&e) else {
            let flush = ends_job(&e);
            let mut out = vec![e];
            if flush {
                if let Some(slot) = self.slots.get_mut(&ThrottleKey::LibraryChanged) {
                    if let Some(pending) = slot.pending.take() {
                        slot.mark(now, &pending);
                        out.push(pending);
                    }
                }
            }
            return out;
        };
        let slot = self.slots.entry(key).or_insert(Slot { window, last_emit: None, last_phase: None, pending: None });
        if slot.window_over(now) || phase(&e) != slot.last_phase {
            slot.mark(now, &e);
            vec![e]
        } else {
            slot.pending = Some(e);
            Vec::new()
        }
    }

    /// Trailing values whose window ended at `now`.
    pub fn due(&mut self, now: Instant) -> Vec<CoreEvent> {
        let mut out = Vec::new();
        for slot in self.slots.values_mut() {
            if slot.pending.is_some() && slot.window_over(now) {
                let pending = slot.pending.take().expect("checked above");
                slot.mark(now, &pending);
                out.push(pending);
            }
        }
        out
    }

    /// Earliest instant at which `due` returns something.
    pub fn next_deadline(&self) -> Option<Instant> {
        self.slots
            .values()
            .filter(|s| s.pending.is_some())
            .filter_map(|s| s.last_emit.map(|t| t + s.window))
            .min()
    }
}

/// The core's `EventSink` inside the app: throttles, then `app.emit(name, payload)`.
pub struct TauriSink {
    app: AppHandle,
    throttle: Arc<Mutex<Throttle>>,
    notify: Arc<Notify>,
}

fn emit_now(app: &AppHandle, e: &CoreEvent) {
    if let Err(err) = app.emit(e.name(), e.payload()) {
        log::warn!("emit {} failed: {err}", e.name());
    }
}

impl TauriSink {
    /// Spawns the trailing-flush task: it sleeps on a deadline timer (or on `notify` when nothing is pending).
    pub fn new(app: AppHandle) -> Arc<TauriSink> {
        let sink = Arc::new(TauriSink {
            app: app.clone(),
            throttle: Arc::new(Mutex::new(Throttle::default())),
            notify: Arc::new(Notify::new()),
        });
        let (throttle, notify) = (sink.throttle.clone(), sink.notify.clone());
        tauri::async_runtime::spawn(async move {
            loop {
                let deadline = throttle.lock().unwrap().next_deadline();
                match deadline {
                    Some(d) => {
                        let _ = tokio::time::timeout_at(tokio::time::Instant::from_std(d), notify.notified()).await;
                    }
                    None => notify.notified().await,
                }
                let due = throttle.lock().unwrap().due(Instant::now());
                for e in &due {
                    emit_now(&app, e);
                }
            }
        });
        sink
    }
}

impl EventSink for TauriSink {
    fn emit(&self, e: CoreEvent) {
        let out = self.throttle.lock().unwrap().offer(e, Instant::now());
        for e in &out {
            emit_now(&self.app, e);
        }
        self.notify.notify_one();
    }
}
```

(`notify_one` stores one permit when the flush task is not waiting, so a new pending value always re-arms the deadline. `Throttle` is unused until Step 21 wires `TauriSink`; the `dead_code` warnings until then are expected.)

- [ ] **Step 6: Run the throttle tests.**

Run: `cargo test -p outlooker --locked events::`
Expected: `test events::tests::throttle_at_most_four_per_second_per_key ... ok`, `throttle_always_emits_trailing_value ... ok`, `throttle_emits_phase_change_immediately ... ok`, `job_end_flushes_library_changed ... ok`; `test result: ok. 4 passed`.

- [ ] **Step 7: Commit.** Commit `src-tauri/src/events.rs` with message `feat(shell): event emitter with per-key throttle and trailing flush`.

- [ ] **Step 8: Write the failing `mailimg` tests.** Write `src-tauri/src/mailimg.rs` (kickoff file, empty) with only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tauri::http::{header, Method, StatusCode};

    const WEBP: &[u8] = b"RIFF\x0c\x00\x00\x00WEBPVP8 ";

    fn store() -> HashMap<i64, Vec<u8>> {
        HashMap::from([(7, WEBP.to_vec())])
    }

    async fn call(method: &Method, path: &str) -> Response<Vec<u8>> {
        let s = store();
        respond(method, path, |id| {
            let hit = s.get(&id).cloned();
            async move { Ok(hit) }
        })
        .await
    }

    #[tokio::test]
    async fn mailimg_post_is_405() {
        let r = call(&Method::POST, "/7").await;
        assert_eq!(r.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert!(r.body().is_empty());
    }

    #[tokio::test]
    async fn mailimg_non_integer_is_400() {
        for path in ["/abc", "/0", "/-1", "/", "/7.5", "/+7", "/7/x", "/99999999999999999999"] {
            assert_eq!(call(&Method::GET, path).await.status(), StatusCode::BAD_REQUEST, "path {path}");
        }
    }

    #[tokio::test]
    async fn mailimg_unknown_id_is_404() {
        let r = call(&Method::GET, "/8").await;
        assert_eq!(r.status(), StatusCode::NOT_FOUND);
        assert!(r.body().is_empty());
    }

    #[tokio::test]
    async fn mailimg_seeded_id_is_200_webp() {
        let r = call(&Method::GET, "/7").await;
        assert_eq!(r.status(), StatusCode::OK);
        assert_eq!(r.headers()[header::CONTENT_TYPE], "image/webp");
        assert_eq!(r.headers()[header::CACHE_CONTROL], "max-age=31536000, immutable");
        assert_eq!(r.body().as_slice(), WEBP);
    }

    #[tokio::test]
    async fn mailimg_db_not_ready_is_503() {
        let r = respond(&Method::GET, "/7", |_| async { Err(AppError::DbNotReady) }).await;
        assert_eq!(r.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(unavailable().status(), StatusCode::SERVICE_UNAVAILABLE);
        let r = respond(&Method::GET, "/7", |_| async { Err(AppError::internal("boom")) }).await;
        assert_eq!(r.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(r.body().is_empty());
    }
}
```

- [ ] **Step 9: Run them to see them fail.**

Run: `cargo test -p outlooker --locked mailimg::`
Expected: compile error `cannot find function `respond` in this scope`.

- [ ] **Step 10: Write `respond` and `unavailable`.** Insert above the test module in `src-tauri/src/mailimg.rs`:

```rust
//! `mailimg` URI scheme (spec Part 03 §5.6): serves stored WebP images by integer id. No file path is ever read.
use std::future::Future;

use outlooker_core::dto::error::AppError;
use tauri::http::{header, Method, Response, StatusCode};

fn empty(status: StatusCode) -> Response<Vec<u8>> {
    let mut r = Response::new(Vec::new());
    *r.status_mut() = status;
    r
}

/// 503 for requests that arrive before the services exist.
pub fn unavailable() -> Response<Vec<u8>> {
    empty(StatusCode::SERVICE_UNAVAILABLE)
}

/// `path` is the URI path (`/<id>`): `mailimg://localhost/<id>` on macOS, `http://mailimg.localhost/<id>` on Windows.
fn parse_id(path: &str) -> Option<i64> {
    let s = path.strip_prefix('/').unwrap_or(path);
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse::<i64>().ok().filter(|id| *id > 0)
}

pub async fn respond<F, Fut>(method: &Method, path: &str, load: F) -> Response<Vec<u8>>
where
    F: FnOnce(i64) -> Fut,
    Fut: Future<Output = Result<Option<Vec<u8>>, AppError>>,
{
    if *method != Method::GET {
        return empty(StatusCode::METHOD_NOT_ALLOWED);
    }
    let Some(id) = parse_id(path) else {
        return empty(StatusCode::BAD_REQUEST);
    };
    match load(id).await {
        Ok(Some(bytes)) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "image/webp")
            .header(header::CACHE_CONTROL, "max-age=31536000, immutable")
            .body(bytes)
            .expect("static status and headers are valid"),
        Ok(None) => empty(StatusCode::NOT_FOUND),
        Err(AppError::DbNotReady) => empty(StatusCode::SERVICE_UNAVAILABLE),
        Err(e) => {
            log::error!("mailimg {id}: {e}");
            empty(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
```

- [ ] **Step 11: Run the `mailimg` tests.**

Run: `cargo test -p outlooker --locked mailimg::`
Expected: `mailimg_post_is_405`, `mailimg_non_integer_is_400`, `mailimg_unknown_id_is_404`, `mailimg_seeded_id_is_200_webp`, `mailimg_db_not_ready_is_503` each `... ok`; `test result: ok. 5 passed`.

- [ ] **Step 12: Commit.** Commit `src-tauri/src/mailimg.rs` with message `feat(shell): mailimg scheme handler (GET, integer id, webp, cache headers)`.

- [ ] **Step 13: Write the failing composition tests** `core/tests/app_compose.rs`:

```rust
//! R2-13 composition tests on a real PostgreSQL (run `node scripts/fetch-postgres.mjs` first).
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use outlooker_core::app::{self, AppDirs, BuildOptions, Services, ShutdownLimits, ShutdownReport, StartStep};
use outlooker_core::db::indexes::missing_or_invalid;
use outlooker_core::db::Db;
use outlooker_core::dto::app::{AppPhase, InitializeRequest, Language};
use outlooker_core::dto::import::{EnqueueResult, ImportPhase, ImportStatus};
use outlooker_core::importer::{ImportTuning, RecoveryReport};
use outlooker_core::lifecycle::pg::{PgController, PgStatus};
use outlooker_core::lifecycle::SupervisorConfig;
use outlooker_core::paths::{dev_pg_dir, Paths};
use outlooker_core::sink::{CoreEvent, EventSink, NullSink, RecordingSink};
use outlooker_core::test_support::import_fixtures::new_source;
use tokio::sync::mpsc;
use tokio::time::timeout;

const LONG: Duration = Duration::from_secs(120);
const STEPS: [StartStep; 3] = [StartStep::Recovered, StartStep::IndexesEnsured, StartStep::WorkerStarted];

/// Records every event (RecordingSink), forwards it to a channel for event waits, and for one watched PST records,
/// at the moment each `import:status-changed` (status other than `queued`) is emitted, whether all six search
/// indexes were valid. `queued` is excluded: `recover_after_crash` may report it before the heal.
struct TestSink {
    rec: RecordingSink,
    tx: mpsc::UnboundedSender<CoreEvent>,
    watched: Mutex<Option<(i64, Db)>>,
    probes: Mutex<Vec<(ImportStatus, bool)>>,
}

impl TestSink {
    fn new() -> (Arc<TestSink>, mpsc::UnboundedReceiver<CoreEvent>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let sink = TestSink { rec: RecordingSink::default(), tx, watched: Mutex::new(None), probes: Mutex::new(Vec::new()) };
        (Arc::new(sink), rx)
    }

    fn watch(&self, pst_id: i64, db: Db) {
        *self.watched.lock().unwrap() = Some((pst_id, db));
    }

    fn probes(&self) -> Vec<(ImportStatus, bool)> {
        self.probes.lock().unwrap().clone()
    }
}

impl EventSink for TestSink {
    fn emit(&self, e: CoreEvent) {
        if let CoreEvent::ImportStatusChanged(c) = &e {
            let watched = self.watched.lock().unwrap().clone();
            if let Some((pst_id, db)) = watched {
                if c.pst_id == pst_id && c.status != ImportStatus::Queued {
                    // multi_thread test runtime: block this worker thread for one catalog query
                    let valid = tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current().block_on(async {
                            let admin = db.connect_admin().await.expect("admin connection");
                            missing_or_invalid(&admin).await.expect("index check").is_empty()
                        })
                    });
                    self.probes.lock().unwrap().push((c.status, valid));
                }
            }
        }
        self.rec.emit(e.clone());
        let _ = self.tx.send(e);
    }
}

fn dirs(root: &Path) -> AppDirs {
    assert!(dev_pg_dir().join("bin").exists(), "PostgreSQL binaries missing: run node scripts/fetch-postgres.mjs");
    AppDirs { app_local_data_dir: root.join("app"), app_log_dir: root.join("logs"), pg_dir: dev_pg_dir() }
}

fn data_root(root: &Path) -> PathBuf {
    root.join("data")
}

fn opts(import_tuning: ImportTuning) -> BuildOptions {
    let supervisor = SupervisorConfig { probe_interval: Duration::from_millis(200), ..SupervisorConfig::default() };
    BuildOptions { supervisor, import_tuning }
}

async fn wait_phase(s: &Services, want: AppPhase) {
    let mut w = s.supervisor.phase_watch();
    timeout(LONG, w.wait_for(|p| *p == want)).await.expect("phase timeout").expect("supervisor dropped");
}

/// Onboarding through the Supervisor only (no worker): returns Ready services on `<root>/data`.
async fn first_run(root: &Path) -> Services {
    let s = app::build(dirs(root), Arc::new(NullSink), opts(ImportTuning::default())).unwrap();
    s.supervisor.boot().await.unwrap();
    let req = InitializeRequest { data_dir: data_root(root).display().to_string(), language: Language::En };
    s.supervisor.initialize(req).await.unwrap();
    wait_phase(&s, AppPhase::Ready).await;
    s
}

/// What a killed process leaves behind: the importer never ran its shutdown, so job rows keep their state.
/// The cluster is stopped through the Supervisor (phase `StoppingDb`), so the old health monitor stays idle and
/// cannot restart PostgreSQL under the next `Services`.
async fn stop_like_a_kill(s: Services) {
    s.supervisor.shutdown_db().await.unwrap();
}

async fn job_state(admin: &tokio_postgres::Client, pst_id: i64) -> String {
    admin
        .query_one("SELECT state FROM import_jobs WHERE pst_id = $1 ORDER BY id DESC LIMIT 1", &[&pst_id])
        .await
        .unwrap()
        .get(0)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shutdown_during_import_requeues_job() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../tests/fixtures/external/outlook.pst");
    assert!(fixture.exists(), "missing fixture {}", fixture.display());
    let tmp = tempfile::tempdir().unwrap();
    stop_like_a_kill(first_run(tmp.path()).await).await;

    let (sink, mut events) = TestSink::new();
    let tuning = ImportTuning { hold_reader_after: Some(0), ..ImportTuning::default() };
    let s = app::build(dirs(tmp.path()), sink, opts(tuning)).unwrap();
    let report = timeout(LONG, app::start_services(&s)).await.expect("start timeout").unwrap();
    assert_eq!(report.steps, STEPS);
    let mut job_events = s.importer.job_events();
    let path = fixture.canonicalize().unwrap().display().to_string();
    let pst_id = match &s.importer.enqueue(vec![path]).await.unwrap()[..] {
        [EnqueueResult::Queued { pst }] => pst.id,
        other => panic!("enqueue: {other:?}"),
    };
    timeout(LONG, async {
        loop {
            match events.recv().await.expect("sink closed") {
                CoreEvent::ImportProgress(p) if p.pst_id == pst_id && p.phase == ImportPhase::Importing => break,
                _ => {}
            }
        }
    })
    .await
    .expect("the job never reached importing");

    let started = Instant::now();
    let report = app::shutdown(&s, ShutdownLimits::default()).await;
    let elapsed = started.elapsed();
    assert_eq!(report, ShutdownReport { import_drained: true, db_stopped: true });
    assert!(elapsed < Duration::from_secs(45), "shutdown took {elapsed:?} (budget 15 s + 30 s)");
    assert_eq!(job_events.try_recv().unwrap(), (pst_id, ImportStatus::Queued), "Queued, not Cancelled");
    let paths = Paths::new(tmp.path().join("app"), tmp.path().join("logs"), dev_pg_dir()).with_data_root(data_root(tmp.path()));
    assert_eq!(PgController::new(paths.clone()).status(&paths.pgdata()).await.unwrap(), PgStatus::Stopped);

    // Reopen the library without starting the worker: the job row and its checkpoint were committed.
    let s2 = app::build(dirs(tmp.path()), Arc::new(NullSink), opts(ImportTuning::default())).unwrap();
    s2.supervisor.boot().await.unwrap();
    wait_phase(&s2, AppPhase::Ready).await;
    let admin = s2.db.connect_admin().await.unwrap();
    assert_eq!(job_state(&admin, pst_id).await, "queued");
    let checkpoints: i64 = admin
        .query_one("SELECT count(*) FROM import_checkpoints WHERE pst_id = $1", &[&pst_id])
        .await
        .unwrap()
        .get(0);
    assert_eq!(checkpoints, 1, "checkpoint row committed");
    drop(admin);
    s2.supervisor.shutdown_db().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn start_after_kill_recovers_jobs_then_heals_then_starts_worker() {
    let tmp = tempfile::tempdir().unwrap();
    let s1 = first_run(tmp.path()).await;
    let admin = s1.db.connect_admin().await.unwrap();
    // The requeued job points at a missing file, so the worker's first event for it is `failed` (no index drop).
    let gone = tmp.path().join("gone");
    let (running_pst, _) = new_source(&admin, &gone.join("running.pst").display().to_string()).await;
    let (cancelling_pst, cancelling_job) = new_source(&admin, &gone.join("cancelling.pst").display().to_string()).await;
    admin.execute("UPDATE import_jobs SET state = 'cancelling' WHERE id = $1", &[&cancelling_job]).await.unwrap();
    admin.batch_execute("DROP INDEX messages_body_trgm").await.unwrap();
    assert_eq!(missing_or_invalid(&admin).await.unwrap(), vec!["messages_body_trgm"]);
    drop(admin);
    stop_like_a_kill(s1).await;

    let (sink, mut events) = TestSink::new();
    let s2 = app::build(dirs(tmp.path()), sink.clone(), opts(ImportTuning::default())).unwrap();
    sink.watch(running_pst, s2.db.clone());
    let report = timeout(LONG, app::start_services(&s2)).await.expect("start timeout").unwrap();
    assert_eq!(report.steps, STEPS);
    assert_eq!(report.recovery, RecoveryReport { requeued: vec![running_pst], cancelled: vec![cancelling_pst] });

    let ended = timeout(LONG, async {
        loop {
            match events.recv().await.expect("sink closed") {
                CoreEvent::ImportStatusChanged(c) if c.pst_id == running_pst && c.status != ImportStatus::Queued => break c.status,
                _ => {}
            }
        }
    })
    .await
    .expect("the requeued job never ran");
    assert_eq!(ended, ImportStatus::Failed);
    let probes = sink.probes();
    assert!(!probes.is_empty(), "no worker event for the requeued PST; events: {:?}", sink.rec.names());
    assert!(probes.iter().all(|(_, valid)| *valid), "worker touched the job before the indexes were healed: {probes:?}");

    let admin = s2.db.connect_admin().await.unwrap();
    assert_eq!(job_state(&admin, cancelling_pst).await, "cancelled");
    assert!(missing_or_invalid(&admin).await.unwrap().is_empty());
    drop(admin);
    app::shutdown(&s2, ShutdownLimits::default()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn missing_index_is_rebuilt_before_worker() {
    let tmp = tempfile::tempdir().unwrap();
    let s1 = first_run(tmp.path()).await;
    let admin = s1.db.connect_admin().await.unwrap();
    admin.batch_execute("DROP INDEX messages_subject_trgm").await.unwrap();
    assert_eq!(missing_or_invalid(&admin).await.unwrap(), vec!["messages_subject_trgm"]);
    drop(admin);
    stop_like_a_kill(s1).await;

    let (sink, _events) = TestSink::new();
    let s2 = app::build(dirs(tmp.path()), sink, opts(ImportTuning::default())).unwrap();
    let report = timeout(LONG, app::start_services(&s2)).await.expect("start timeout").unwrap();
    // IndexesEnsured is pushed only when the heal succeeded, and always before WorkerStarted.
    assert_eq!(report.steps, STEPS);
    assert_eq!(report.recovery, RecoveryReport::default());
    let admin = s2.db.connect_admin().await.unwrap();
    assert!(missing_or_invalid(&admin).await.unwrap().is_empty());
    let valid: bool = admin
        .query_one(
            "SELECT i.indisvalid FROM pg_index i JOIN pg_class c ON c.oid = i.indexrelid WHERE c.relname = 'messages_subject_trgm'",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(valid);
    drop(admin);
    app::shutdown(&s2, ShutdownLimits::default()).await;
}
```

(`outlooker_core::test_support` compiles in `cargo test --workspace` because `outlooker-loadgen` enables `test-support` and Cargo unifies features across the workspace; the acceptance command names the feature explicitly.)

- [ ] **Step 14: Run the tests to see them fail.**

Run: `cargo test -p outlooker-core --locked --features test-support --test app_compose`
Expected: compile error `unresolved imports `outlooker_core::app::AppDirs`, `outlooker_core::app::BuildOptions`` (the kickoff `core/src/app/mod.rs` is empty).

- [ ] **Step 15: Write `core/src/app/mod.rs`.**

```rust
//! App composition (spec Part 02 §2.2 steps 2–9, §2.5): builds every service, starts them in order, stops them.
//! The only module that imports every service; no service imports this module.
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::db::Db;
use crate::dto::app::AppPhase;
use crate::dto::error::AppError;
use crate::gate::Gate;
use crate::importer::{ImportManager, ImportTuning, RecoveryReport};
use crate::lifecycle::{Supervisor, SupervisorConfig};
use crate::maintenance::Maintenance;
use crate::paths::Paths;
use crate::search::SearchService;
use crate::settings::SettingsStore;
use crate::sink::EventSink;
use crate::sql_console::SqlConsole;

pub struct AppDirs {
    pub app_local_data_dir: PathBuf,
    pub app_log_dir: PathBuf,
    pub pg_dir: PathBuf,
}

#[derive(Default)]
pub struct BuildOptions {
    pub supervisor: SupervisorConfig,
    pub import_tuning: ImportTuning,
}

#[derive(Clone)]
pub struct Services {
    pub paths: Paths,
    pub db: Db,
    pub gate: Gate,
    pub sink: Arc<dyn EventSink>,
    pub settings: SettingsStore,
    pub supervisor: Supervisor,
    pub importer: ImportManager,
    pub search: SearchService,
    pub sql: SqlConsole,
    pub maintenance: Maintenance,
}

/// Wires every service. No I/O except reading the bootstrap file; spawns nothing.
pub fn build(dirs: AppDirs, sink: Arc<dyn EventSink>, opts: BuildOptions) -> Result<Services, AppError> {
    let paths = Paths::new(dirs.app_local_data_dir, dirs.app_log_dir, dirs.pg_dir);
    let settings = SettingsStore::load(paths.bootstrap_file())?;
    let gate = Gate::new();
    let db = Db::new();
    let supervisor = Supervisor::new(paths.clone(), settings.clone(), gate.clone(), db.clone(), sink.clone(), opts.supervisor);
    let importer = ImportManager::new(
        db.clone(),
        gate.clone(),
        paths.clone(),
        sink.clone(),
        supervisor.phase_watch(),
        opts.import_tuning,
    );
    let search = SearchService::new(db.clone());
    let sql = SqlConsole::new(db.clone());
    let maintenance = Maintenance::new(db.clone(), gate.clone(), sink.clone());
    Ok(Services { paths, db, gate, sink, settings, supervisor, importer, search, sql, maintenance })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartStep {
    Recovered,
    IndexesEnsured,
    WorkerStarted,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StartReport {
    pub steps: Vec<StartStep>,
    pub recovery: RecoveryReport,
}

/// Runs once per process. Later `Ready` transitions (PG restart, data dir move) are handled inside
/// `ImportManager` through the phase watch (C15).
pub async fn start_services(s: &Services) -> Result<StartReport, AppError> {
    if let Err(e) = s.supervisor.boot().await {
        // Phase is now Failed; `app_retry` recovers, and the wait below ends when it does.
        log::error!("boot failed: {e}");
    }
    // First run: this wait ends after onboarding's `app_initialize` reached Ready.
    s.supervisor
        .phase_watch()
        .wait_for(|p| *p == AppPhase::Ready)
        .await
        .map_err(|_| AppError::internal("supervisor dropped"))?;
    let mut steps = Vec::new();
    let recovery = s.importer.recover_after_crash().await?;
    steps.push(StartStep::Recovered);
    match s.importer.self_heal_indexes().await {
        Ok(()) => steps.push(StartStep::IndexesEnsured),
        // Search works without the indexes (slower), so the queue still starts.
        Err(e) => log::error!("search index self-heal failed: {e}"),
    }
    s.importer.start_worker();
    steps.push(StartStep::WorkerStarted);
    Ok(StartReport { steps, recovery })
}

#[derive(Debug, Clone, Copy)]
pub struct ShutdownLimits {
    pub import_drain: Duration,
}

impl Default for ShutdownLimits {
    fn default() -> Self {
        ShutdownLimits { import_drain: Duration::from_secs(15) }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShutdownReport {
    pub import_drained: bool,
    pub db_stopped: bool,
}

/// Part 02 §2.5: importer (job → Queued, ≤ 15 s), running SQL cancelled, then the database
/// (pools closed, `pg_ctl stop -m fast -w -t 30`, immediate on timeout — inside the Supervisor). Never panics.
pub async fn shutdown(s: &Services, limits: ShutdownLimits) -> ShutdownReport {
    let import_drained = match s.importer.shutdown(limits.import_drain).await {
        Ok(()) => true,
        Err(e) => {
            log::warn!("import shutdown: {e}");
            false
        }
    };
    s.sql.cancel_running().await;
    let db_stopped = match s.supervisor.shutdown_db().await {
        Ok(()) => true,
        Err(e) => {
            log::warn!("database shutdown: {e}");
            false
        }
    };
    ShutdownReport { import_drained, db_stopped }
}
```

- [ ] **Step 16: Run the composition tests.**

Run: `node scripts/fetch-postgres.mjs` (repo root; exits 0, idempotent), then in `src-tauri/`: `cargo test -p outlooker-core --locked --features test-support --test app_compose`
Expected: `test shutdown_during_import_requeues_job ... ok`, `test start_after_kill_recovers_jobs_then_heals_then_starts_worker ... ok`, `test missing_index_is_rebuilt_before_worker ... ok`; `test result: ok. 3 passed`.
Run: `grep -rnE "crate::app|outlooker_core::app" src-tauri/crates/outlooker-core/src --include=*.rs | grep -v "^src-tauri/crates/outlooker-core/src/app/"`
Expected: no output (no service imports `core::app`).

- [ ] **Step 17: Commit.** Commit `src-tauri/crates/outlooker-core/src/app/mod.rs` and `src-tauri/crates/outlooker-core/tests/app_compose.rs` with message `feat(core): app composition (build, start_services, shutdown) with composition tests`.

- [ ] **Step 18: Write the Command Layer.** Each function is one service call (Part 03 §3; argument names are the C4 keys in snake_case — Tauri maps `executionId` → `execution_id`, `pstId` → `pst_id`).

`src-tauri/src/commands/app.rs`:
```rust
use outlooker_core::app::Services;
use outlooker_core::dto::app::{AppStatus, InitializeRequest};
use outlooker_core::dto::error::AppError;
use outlooker_core::dto::settings::DbStatus;
use tauri::State;

#[tauri::command]
pub async fn app_status(s: State<'_, Services>) -> Result<AppStatus, AppError> {
    Ok(s.supervisor.status().await)
}

#[tauri::command]
pub async fn app_initialize(s: State<'_, Services>, req: InitializeRequest) -> Result<(), AppError> {
    s.supervisor.initialize(req).await
}

#[tauri::command]
pub async fn app_retry(s: State<'_, Services>) -> Result<(), AppError> {
    s.supervisor.retry().await
}

#[tauri::command]
pub async fn data_dir_move(s: State<'_, Services>, target: String) -> Result<(), AppError> {
    s.supervisor.move_data_dir(target).await
}

#[tauri::command]
pub async fn db_status(s: State<'_, Services>) -> Result<DbStatus, AppError> {
    s.supervisor.db_status(s.maintenance.current()).await
}
```

`src-tauri/src/commands/search.rs`:
```rust
use outlooker_core::app::Services;
use outlooker_core::dto::error::AppError;
use outlooker_core::dto::search::{FilterOptions, MailDetail, SearchCount, SearchPage, SearchQuery, SearchRequest};
use tauri::State;

#[tauri::command]
pub async fn search_mail(s: State<'_, Services>, req: SearchRequest) -> Result<SearchPage, AppError> {
    s.search.search(req).await
}

#[tauri::command]
pub async fn search_count(s: State<'_, Services>, query: SearchQuery) -> Result<SearchCount, AppError> {
    s.search.count(query).await
}

#[tauri::command]
pub async fn filter_options(s: State<'_, Services>) -> Result<FilterOptions, AppError> {
    s.search.filter_options().await
}

#[tauri::command]
pub async fn mail_get(s: State<'_, Services>, id: i64) -> Result<MailDetail, AppError> {
    s.search.mail_get(id).await
}
```

`src-tauri/src/commands/sql.rs`:
```rust
use outlooker_core::app::Services;
use outlooker_core::dto::error::AppError;
use outlooker_core::dto::sql::{SavedQuery, SavedQueryInput, SqlExecuteRequest, SqlResult};
use tauri::State;

#[tauri::command]
pub async fn saved_query_list(s: State<'_, Services>) -> Result<Vec<SavedQuery>, AppError> {
    s.sql.saved_list().await
}

#[tauri::command]
pub async fn saved_query_create(s: State<'_, Services>, input: SavedQueryInput) -> Result<SavedQuery, AppError> {
    s.sql.saved_create(input).await
}

#[tauri::command]
pub async fn saved_query_update(s: State<'_, Services>, id: i64, input: SavedQueryInput) -> Result<SavedQuery, AppError> {
    s.sql.saved_update(id, input).await
}

#[tauri::command]
pub async fn saved_query_delete(s: State<'_, Services>, id: i64) -> Result<(), AppError> {
    s.sql.saved_delete(id).await
}

#[tauri::command]
pub async fn sql_execute(s: State<'_, Services>, req: SqlExecuteRequest) -> Result<SqlResult, AppError> {
    s.sql.execute(req).await
}

#[tauri::command]
pub async fn sql_cancel(s: State<'_, Services>, execution_id: String) -> Result<(), AppError> {
    s.sql.cancel(execution_id).await
}
```

`src-tauri/src/commands/import.rs`:
```rust
use outlooker_core::app::Services;
use outlooker_core::dto::error::AppError;
use outlooker_core::dto::import::{EnqueueResult, ImportErrorPage, ImportErrorsRequest, PstSource};
use tauri::State;

#[tauri::command]
pub async fn pst_list(s: State<'_, Services>) -> Result<Vec<PstSource>, AppError> {
    s.importer.list().await
}

#[tauri::command]
pub async fn import_enqueue(s: State<'_, Services>, paths: Vec<String>) -> Result<Vec<EnqueueResult>, AppError> {
    s.importer.enqueue(paths).await
}

#[tauri::command]
pub async fn import_cancel(s: State<'_, Services>, pst_id: i64) -> Result<(), AppError> {
    s.importer.cancel(pst_id).await
}

#[tauri::command]
pub async fn import_resume(s: State<'_, Services>, pst_id: i64) -> Result<(), AppError> {
    s.importer.resume(pst_id).await
}

#[tauri::command]
pub async fn pst_remove(s: State<'_, Services>, pst_id: i64) -> Result<(), AppError> {
    s.importer.remove(pst_id).await
}

#[tauri::command]
pub async fn import_errors(s: State<'_, Services>, req: ImportErrorsRequest) -> Result<ImportErrorPage, AppError> {
    s.importer.errors(req).await
}
```

`src-tauri/src/commands/settings.rs`:
```rust
use outlooker_core::app::Services;
use outlooker_core::dto::error::AppError;
use outlooker_core::dto::settings::{MaintenanceTask, Settings, SettingsPatch};
use tauri::State;

#[tauri::command]
pub async fn settings_get(s: State<'_, Services>) -> Result<Settings, AppError> {
    s.settings.get(&s.db).await
}

#[tauri::command]
pub async fn settings_update(s: State<'_, Services>, patch: SettingsPatch) -> Result<Settings, AppError> {
    s.settings.update(&s.db, patch).await
}

#[tauri::command]
pub async fn maintenance_run(s: State<'_, Services>, task: MaintenanceTask) -> Result<(), AppError> {
    s.maintenance.run(task).await
}
```
`src-tauri/src/commands/mod.rs` stays `pub mod app; pub mod import; pub mod search; pub mod settings; pub mod sql;`.

- [ ] **Step 19: Check the Command Layer and commit.**

Run: `cargo build -p outlooker --locked`
Expected: exit 0 (`dead_code` warnings for the not-yet-registered commands are expected).
Run (repo root): `grep -c "await" src-tauri/src/commands/app.rs src-tauri/src/commands/search.rs src-tauri/src/commands/sql.rs src-tauri/src/commands/import.rs src-tauri/src/commands/settings.rs`
Expected: `src-tauri/src/commands/app.rs:5`, `search.rs:4`, `sql.rs:6`, `import.rs:6`, `settings.rs:3` (one service call per command, 24 in total).
Commit the five files `src-tauri/src/commands/{app,search,sql,import,settings}.rs` with message `feat(shell): command layer, one thin tauri command per contract command`.

- [ ] **Step 20: Write the failing App Shell source tests.** Append to `src-tauri/src/lib.rs` (the Step 1 version):

```rust
#[cfg(test)]
mod tests {
    use outlooker_core::dto::commands::COMMANDS;

    const SRC: &str = include_str!("lib.rs");

    /// Command names inside the `generate_handler![…]` list of `run()` (path prefix stripped).
    fn handler_names() -> Vec<String> {
        let open = "generate_handler![";
        let start = SRC.find(open).expect("lib.rs has a generate_handler! list") + open.len();
        let end = start + SRC[start..].find(']').expect("closing bracket");
        SRC[start..end]
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.rsplit("::").next().unwrap().to_owned())
            .collect()
    }

    #[test]
    fn generate_handler_list_equals_commands() {
        let mut got = handler_names();
        got.sort();
        let mut want: Vec<String> = COMMANDS.iter().map(|c| (*c).to_owned()).collect();
        want.sort();
        assert_eq!(got, want);
    }

    #[test]
    fn single_instance_plugin_is_first() {
        let first = SRC.find(".plugin(").expect("lib.rs registers plugins");
        assert!(
            SRC[first..].starts_with(".plugin(tauri_plugin_single_instance::init("),
            "first plugin call: {}",
            &SRC[first..first + 60]
        );
    }
}
```
(The test module stays the last item of `lib.rs`, so the first occurrences of `generate_handler![` and `.plugin(` are the ones in `run()`.)

- [ ] **Step 21: Run them to see the handler test fail.**

Run: `cargo test -p outlooker --locked tests::`
Expected: `tests::single_instance_plugin_is_first ... ok` (and the 9 `events::` / `mailimg::` tests `ok`, the filter matches them too); `tests::generate_handler_list_equals_commands ... FAILED` with `lib.rs has a generate_handler! list` (the Step 1 shell registers no handler); `test result: FAILED. 10 passed; 1 failed`.

- [ ] **Step 22: Write the full App Shell.** Replace everything above the test module in `src-tauri/src/lib.rs` with:

```rust
mod commands;
mod events;
mod mailimg;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};

use outlooker_core::app::{self as core_app, AppDirs, BuildOptions, Services, ShutdownLimits};
use tauri::path::BaseDirectory;
use tauri::{Manager, RunEvent};

/// Exit shutdown state: 0 idle, 1 running, 2 done.
static SHUTDOWN: AtomicU8 = AtomicU8::new(0);
const IDLE: u8 = 0;
const RUNNING: u8 = 1;
const DONE: u8 = 2;

/// E2E seam (R2-14): the app data dir comes from `OUTLOOKER_APP_DATA_DIR` (absolute) only in `--features e2e` builds.
#[cfg(feature = "e2e")]
fn app_data_override() -> Option<PathBuf> {
    std::env::var_os("OUTLOOKER_APP_DATA_DIR").map(PathBuf::from).filter(|p| p.is_absolute())
}

#[cfg(not(feature = "e2e"))]
fn app_data_override() -> Option<PathBuf> {
    None
}

async fn run_shutdown(services: &Services) {
    let report = core_app::shutdown(services, ShutdownLimits::default()).await;
    log::info!("shutdown: {report:?}");
    SHUTDOWN.store(DONE, Ordering::SeqCst);
}

pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_log::Builder::new().build());
    // ponytail: E2E-only WebDriver server; never compiled into the default (release) build.
    #[cfg(feature = "e2e")]
    let builder = builder.plugin(tauri_plugin_wdio_webdriver::init());
    builder
        .setup(|app| {
            let app_local_data_dir = match app_data_override() {
                Some(dir) => dir,
                None => app.path().app_local_data_dir()?,
            };
            let dirs = AppDirs {
                app_local_data_dir,
                app_log_dir: app.path().app_log_dir()?,
                pg_dir: app.path().resolve("resources/postgresql", BaseDirectory::Resource)?,
            };
            let sink = events::TauriSink::new(app.handle().clone());
            let services = core_app::build(dirs, sink, BuildOptions::default())?;
            app.manage(services.clone());
            tauri::async_runtime::spawn(async move {
                match core_app::start_services(&services).await {
                    Ok(report) => log::info!("services started: {:?}", report.steps),
                    Err(e) => log::error!("start_services failed: {e}"),
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app::app_status,
            commands::app::app_initialize,
            commands::app::app_retry,
            commands::search::search_mail,
            commands::search::search_count,
            commands::search::filter_options,
            commands::search::mail_get,
            commands::sql::saved_query_list,
            commands::sql::saved_query_create,
            commands::sql::saved_query_update,
            commands::sql::saved_query_delete,
            commands::sql::sql_execute,
            commands::sql::sql_cancel,
            commands::import::pst_list,
            commands::import::import_enqueue,
            commands::import::import_cancel,
            commands::import::import_resume,
            commands::import::pst_remove,
            commands::import::import_errors,
            commands::settings::settings_get,
            commands::settings::settings_update,
            commands::app::data_dir_move,
            commands::settings::maintenance_run,
            commands::app::db_status
        ])
        .register_asynchronous_uri_scheme_protocol("mailimg", |ctx, request, responder| {
            let handle = ctx.app_handle().clone();
            tauri::async_runtime::spawn(async move {
                let search = handle.try_state::<Services>().map(|s| s.search.clone());
                let response = match search {
                    None => mailimg::unavailable(),
                    Some(search) => {
                        mailimg::respond(request.method(), request.uri().path(), |id| async move { search.image(id).await })
                            .await
                    }
                };
                responder.respond(response);
            });
        })
        .build(tauri::generate_context!())
        .expect("error while building Outlooker")
        .run(|handle, event| {
            let RunEvent::ExitRequested { api, code, .. } = event else { return };
            if SHUTDOWN.load(Ordering::SeqCst) == DONE {
                return;
            }
            let Some(services) = handle.try_state::<Services>().map(|s| (*s).clone()) else { return };
            match code {
                // Window close, Cmd+Q, logoff: keep running, shut down once, then exit.
                None => {
                    api.prevent_exit();
                    if SHUTDOWN.compare_exchange(IDLE, RUNNING, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
                        let handle = handle.clone();
                        tauri::async_runtime::spawn(async move {
                            run_shutdown(&services).await;
                            handle.exit(0);
                        });
                    }
                }
                // `AppHandle::exit` from elsewhere: prevent_exit is ignored, so finish the shutdown here.
                Some(_) => {
                    if SHUTDOWN.compare_exchange(IDLE, RUNNING, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
                        // ponytail: the one documented main-thread block (exit already decided by the caller).
                        tauri::async_runtime::block_on(run_shutdown(&services));
                    }
                }
            }
        });
}
```

(The command order in `generate_handler!` is the `COMMANDS` order; the test compares sorted lists. `run_shutdown` sets `DONE` before `handle.exit(0)`, so the second `ExitRequested` (code `Some(0)`) returns at once.)

- [ ] **Step 23: Add the PostgreSQL resources to the bundle.** In `src-tauri/tauri.conf.json`, the `bundle` object becomes (CSP and every other key unchanged):

```json
  "bundle": {
    "active": true,
    "targets": "all",
    "resources": ["resources/postgresql/"],
    "icon": ["icons/32x32.png", "icons/128x128.png", "icons/128x128@2x.png", "icons/icon.icns", "icons/icon.ico"]
  }
```
From this commit on, `node scripts/fetch-postgres.mjs` must run before any `cargo build` of the `outlooker` crate (tauri-build copies `resources/postgresql/` into `target/<profile>/` and fails when it is missing).

- [ ] **Step 24: Run the App Shell tests and commit.**

Run (repo root): `node scripts/fetch-postgres.mjs` — exit 0.
Run: `cargo test -p outlooker --locked`
Expected: 11 tests `ok`: `tests::generate_handler_list_equals_commands`, `tests::single_instance_plugin_is_first`, `events::tests::throttle_at_most_four_per_second_per_key`, `events::tests::throttle_always_emits_trailing_value`, `events::tests::throttle_emits_phase_change_immediately`, `events::tests::job_end_flushes_library_changed`, `mailimg::tests::mailimg_post_is_405`, `mailimg::tests::mailimg_non_integer_is_400`, `mailimg::tests::mailimg_unknown_id_is_404`, `mailimg::tests::mailimg_seeded_id_is_200_webp`, `mailimg::tests::mailimg_db_not_ready_is_503`; `test result: ok. 11 passed`.
Run: `test -x src-tauri/target/debug/resources/postgresql/bin/postgres; echo "rc=$?"` (repo root)
Expected: `rc=0` (dev builds resolve the resource dir next to the binary).
Run: `grep -c '"resources/postgresql/"' src-tauri/tauri.conf.json`
Expected: `1`.
Commit `src-tauri/src/lib.rs` and `src-tauri/tauri.conf.json` with message `feat(shell): app shell (setup, 24 commands, mailimg scheme, exit shutdown, e2e data dir seam) and bundled PostgreSQL resources`.

- [ ] **Step 25: Release, E2E-binary and bindings checks.** `npm run tauri build` and the release `cargo build` are long-running: if one exceeds the foreground time limit, report `BLOCKED` with the command and hand it to the round's gate-runner (never move it to the background).

Run: `cargo build --release -p outlooker --locked`
Expected: exit 0.
Run: `cargo tree -p outlooker -e normal | grep -c wdio`
Expected: prints `0` (grep exits 1 at count 0; judge the printed number).
Run: `cargo build -p outlooker --features e2e --locked`
Expected: exit 0; `test -x src-tauri/target/debug/outlooker` (repo root) exits 0.
Run (repo root): `npm run tauri build`
Expected: exit 0; the output lists a `.dmg` under `src-tauri/target/release/bundle/dmg/`.
Run (repo root): `ls -l src-tauri/target/release/bundle/dmg/*.dmg > scripts/tmp/r2-13-dmg-size.txt; echo "RC=$?" >> scripts/tmp/r2-13-dmg-size.txt; cat scripts/tmp/r2-13-dmg-size.txt`
Expected: one `.dmg` line and `RC=0`; quote the line in the sign-off report so the gate-runner records the size in the R2 gate log.
Run: `cargo test --workspace --locked` (in `src-tauri/`, needs the fetched PostgreSQL and `tests/fixtures/external/outlook.pst`), then (repo root) `git diff --exit-code src/api/bindings; echo "rc=$?"`
Expected: the test run exits 0 and `rc=0` (the ts-rs output is unchanged).
Run (repo root): `grep -rn "sleep" src-tauri/src src-tauri/crates/outlooker-core/src/app src-tauri/crates/outlooker-core/tests/app_compose.rs`
Expected: no output.
No commit in this step (no file changes); `git status --porcelain` shows none of this task's paths.

## Task R2-14: Frontend integration and E2E

**Files:**
- Create: `wdio.conf.ts`, `tests/e2e/build.mjs`, `tests/e2e/tauri.e2e.conf.json`, `tests/e2e/support.ts`, `tests/e2e/prepare.mjs`, `tests/e2e/run.mjs`, `tests/e2e/smoke.e2e.ts`, `tests/e2e/flow.e2e.ts`, `tests/e2e/seeded.e2e.ts`, `tests/e2e/scroll.e2e.ts`, `tests/e2e/tsconfig.json`
- Modify: `package.json` (scripts `e2e`, `e2e:build`; devDependencies `webdriverio`, `@wdio/cli`, `@wdio/local-runner`, `@wdio/mocha-framework`, `@wdio/spec-reporter` at `9.32.0` and `@wdio/tauri-service` at `1.5.0` — exact pins, all listed in spec Part 01 §9), `package-lock.json`, `tsconfig.json` (exclude `tests/e2e`), any file under `src/**` except `src/api/bindings/**` to fix defects found by the real-core runs
- Test: the four E2E spec files; the full Vitest suite

**Interfaces:**
- Consumes: every `data-testid` of R1-06 and R1-07 (lists in their Produces blocks); R2-13's feature `e2e`, `OUTLOOKER_APP_DATA_DIR`, binary path `src-tauri/target/debug/outlooker[.exe]`; C4 `pickPstFiles` / `pickFolder` E2E bypass (planner-1, `src/api/dialog.ts`): when built with `VITE_E2E=1`, each call shifts the next entry from `window.__outlookerE2ePicks` (an array of `string[]` or `string | null` set by the test with `browser.execute`) instead of opening the native dialog; R2-10 `outlooker-loadgen seed --data-root <dir> --messages <n>` (a complete data root; planner-2 cross-check: the seed contains one fixed message with subject `E2E 報價單 inline` whose HTML body has one inline image, and every generated sender address contains `loadgen`); R2-08 bootstrap file format `{ "data_dir": "<abs>", "language": "zh-TW" | "en" }` at `<app_local_data_dir>/settings.json`.
- Produces: `npm run e2e` = `node tests/e2e/build.mjs && node tests/e2e/run.mjs` (builds the E2E binary, then runs the groups `smoke`, `flow`, `seeded`, `scroll` in that order, one `wdio run` per group, preparing each group's library first); `npm run e2e -- --spec tests/e2e/smoke.e2e.ts` runs only the smoke group (dry run; needs no loadgen seed).

**Behavior:**
- **Build (`tests/e2e/build.mjs`).** Spawns `npx tauri build --debug --no-bundle --features e2e --config tests/e2e/tauri.e2e.conf.json` with `env: { ...process.env, VITE_E2E: '1' }`, `stdio: 'inherit'`, `shell: process.platform === 'win32'`; exits with the child's code. `tauri.e2e.conf.json` = `{ "app": { "security": { "capabilities": ["default", { "identifier": "e2e", "windows": ["main"], "permissions": ["wdio-webdriver:default"] }] } } }` (the e2e capability exists only in this build, so the release build never references the plugin's permission).
- **Runner (`tests/e2e/run.mjs`).** Why: `@wdio/tauri-service` 1.5.0 in embedded mode starts one app process per capability, all at once on one port, and the single-instance plugin ends every process after the first; so each run has exactly one capability. `run.mjs` runs the groups in order; before each it calls `prepareGroup(group)` from `prepare.mjs` (the `fresh/` dir is recreated empty before `smoke` and `flow`), then spawns `npx wdio run wdio.conf.ts --spec tests/e2e/<group>.e2e.ts` with env `OUTLOOKER_E2E_GROUP=<group>`; it stops at the first failing group and exits with that run's code. `--spec <file>` limits the run to that group.
- **Prepare (`tests/e2e/prepare.mjs`).** Exports `prepareGroup(group)`; run directly (`node tests/e2e/prepare.mjs`) it prepares every group. Creates `scripts/tmp/e2e/` (gitignored) with three app-data dirs: `fresh/` (empty — onboarding flow), `seeded/` (bootstrap `settings.json` → `data_dir` = `scripts/tmp/e2e/seeded-root`, language `zh-TW`), `big/` (bootstrap → `scripts/tmp/e2e/big-root`, language `en`); runs `cargo run --release -p outlooker-loadgen -- seed --data-root <abs seeded-root> --messages 2000` and `… --data-root <abs big-root> --messages 120000` in `src-tauri/` only when the root has no `outlooker-data.json` yet (re-runs reuse it); `fresh/` is deleted and recreated empty before every group that uses it. Fails (exit 1, message names the step) on any child failure.
- **wdio.conf.ts.** `runner: 'local'`, `framework: 'mocha'` (`timeout: 600_000`), `reporters: ['spec']`, `maxInstances: 1`, `services: [['@wdio/tauri-service', { driverProvider: 'embedded', captureBackendLogs: false, captureFrontendLogs: false }]]`, exactly one capability per run, chosen by `OUTLOOKER_E2E_GROUP` (`specs` = that group's file): the capability `{ browserName: 'tauri', 'tauri:options': { application: <abs binary path> }, 'wdio:tauriServiceOptions': { env: { OUTLOOKER_APP_DATA_DIR: <abs app-data dir> } } }`; `smoke` and `flow` use `fresh/`, `seeded` uses `seeded/`, `scroll` uses `big/`. No `browser.tauri.*` calls (the service's backend plugin `tauri-plugin-wdio` is not in Part 01); everything goes through the DOM and `browser.execute`.
- **E2E (1) `flow.e2e.ts`** (Part 02 §10 E2E row): onboarding — click `onboarding-lang-en`, `onboarding-next`, `onboarding-dir-default`, `onboarding-next`, wait for `onboarding-db-ready` (timeout 120 s), `onboarding-next`; set picks `[[<abs tests/fixtures/external/outlook.pst>]]` and click `onboarding-pick-pst`; Import screen: wait until the `pst-row` with `data-pst-name="outlook"` shows `pst-status` text `Completed` (timeout 120 s); search — click `nav-search`, type `Welcome` into `search-box`, wait for a `mail-row`, click it, `reading-pane` shows "Welcome to Microsoft Office Outlook 2003", `mail-preview` has `sandbox=""`; Advanced SQL — `nav-advancedSql`, click `sql-new`, type `SELECT id, subject FROM v_mail` into `sql-editor`, click `sql-run`, `sql-result` gets `data-mode="mailList"` with one `mail-row`; remove PST — `nav-import`, click `pst-remove`, `remove-confirm`, wait until no `pst-row` exists (timeout 60 s) and `import:empty.title` text ("No PST imported yet") shows.
- **E2E (2) `seeded.e2e.ts`**: app starts `Ready` on the seeded root (zh-TW); type `報價單` into `search-box`, click the `mail-row` whose text contains `E2E 報價單 inline`; the `mail-preview` `srcdoc` contains an `<img>` whose `src` starts with `mailimg://localhost/` on macOS / `http://mailimg.localhost/` on Windows; `browser.execute` loads that URL with `new Image()` in the main document (same CSP) and resolves `naturalWidth > 0`, proving the scheme serves the WebP through the CSP. The driver cannot switch into the frame: `tauri-plugin-wdio-webdriver` enters frames through `contentDocument`, which is null for the `sandbox=""` (opaque-origin) iframe. Known limit: this does not prove the sandboxed frame itself paints the image.
- **E2E (3) `scroll.e2e.ts`** (Part 03 §5.2, spec §0.3 webview scroll risk): app on the 120,000-message root (en); type `loadgen` into `search-box`; wait for `result-count` text `10,000+`; then repeat: `browser.execute` sets the `mail-list` element's `scrollTop = scrollHeight`, and `waitUntil` the `mail-list` `scrollHeight` grows (32 px per loaded row) or `result-cap-row` is displayed (each wait ≤ 10 s; the loop ends when the cap row shows; hard cap 600 iterations → fail); finally assert `result-cap-row` is displayed, `mail-list` `scrollHeight` ≥ 3,200,000 px and the last `mail-row` `data-mail-id` is a number.
- **Smoke (`smoke.e2e.ts`)** — the early dry run: the app window shows `title-bar` or `blocking-screen` or `failed-screen` or the onboarding (`onboarding-lang-en`) within 60 s. It runs against whatever core exists (Round 1 shell: commands missing → the Failed screen) and proves the build, the embedded driver and the env override.
- **Integration fixes.** After R2-13 lands, run the app against the real core (`npm run tauri dev`) through all five screens and the three E2E groups; every defect fixed in `src/**` gets a Vitest regression test in the file of the touched component.

**Constraints:**
- `webdriverio` / `@wdio/*` at `9.32.0` (`@wdio/tauri-service` `1.5.0`); never webdriverio 10.x.
- No `browser.pause`; condition waits with explicit timeouts only.
- E2E data lives under `scripts/tmp/e2e/` (gitignored); the user's real app data dir is never touched (`OUTLOOKER_APP_DATA_DIR` always set).
- `npm run e2e` is a long-running command: only the round's gate-runner runs the full suite; implementers run single specs.

**Acceptance criteria:**
- [ ] `npm run typecheck && npm test && npm run build` exit 0.
- [ ] `npm run e2e` exits 0 on macOS with the three E2E groups above (`flow.e2e.ts`, `seeded.e2e.ts`, `scroll.e2e.ts`) plus `smoke.e2e.ts`, each run as its own `wdio run` by `tests/e2e/run.mjs` and listed as passed in the spec reporter output.
- [ ] Added: `npm run e2e -- --spec tests/e2e/smoke.e2e.ts` exits 0 (dry run, before and after R2-13 lands).
- [ ] Added: `grep -rn "browser.pause" tests/e2e` finds nothing.

**Steps:**

Every `npm` / `node` command runs at the repo root; `cargo` in `src-tauri/`. Commit protocol of Global Constraints (`git rev-parse --abbrev-ref HEAD` before; `git add <listed paths>` then `git commit -F scripts/tmp/<msgfile> -- <listed paths>`; `git status --porcelain` after). `npm run e2e` (all four groups) is long-running (debug build, a 120,000-message seed, 500 page loads): only the round's gate-runner runs it; implementers run single groups with `node tests/e2e/run.mjs --spec tests/e2e/<group>.e2e.ts`.

Design facts this task relies on (verified in the package sources under `scripts/tmp/p3/`):
- `@wdio/tauri-service` 1.5.0 in embedded mode spawns **one app process per capability** at launch, all on the same WebDriver port (`dist/esm/index.js:2333-2365`), and the single-instance plugin would end every second instance. So each `wdio run` has exactly one capability; `tests/e2e/run.mjs` runs the four groups one after another (one app process, one app-data dir per group).
- The capability's `wdio:tauriServiceOptions.env` is merged into the spawned app's environment (`index.js:1158-1166`), which carries `OUTLOOKER_APP_DATA_DIR`.
- `tauri-plugin-wdio-webdriver` 1.5.0: element send-keys uses the native value setter plus an `input` event for `input` / `textarea` and `document.execCommand('insertText')` for contenteditable (`src/platform/executor.rs:831-875`), so `setValue` / `addValue` work for React inputs, CJK text and the CodeMirror editor. `execute` awaits a returned promise (`executor.rs` around line 1220). Frame switching reads `contentDocument` from the parent (`executor.rs:2202-2222`), which is `null` for the `sandbox=""` preview (opaque origin): the seeded test therefore reads the iframe's `srcdoc` attribute and loads its image URL from the main document.

- [ ] **Step 1: Pin the E2E dev dependencies.** `webdriverio`, `@wdio/cli`, `@wdio/mocha-framework` (`9.32.0`) and `@wdio/tauri-service` (`1.5.0`) are already in `package.json` (R1-01). Add the two packages the WebdriverIO test runner needs (spec Part 01 §9, exact pins `9.32.0`):

Run: `npm install --save-dev --save-exact @wdio/local-runner@9.32.0 @wdio/spec-reporter@9.32.0`
Expected: exit 0; `package.json` `devDependencies` gains `"@wdio/local-runner": "9.32.0"` and `"@wdio/spec-reporter": "9.32.0"`.
Then set the `scripts` entries `e2e` and `e2e:build` (replacing R1-01's `"e2e": "wdio run wdio.conf.ts"`):
```json
    "e2e": "node tests/e2e/build.mjs && node tests/e2e/run.mjs",
    "e2e:build": "node tests/e2e/build.mjs",
```
Run: `grep -nE '"[\^~]' package.json; echo "rc=$?"`
Expected: `rc=1` (no range specifiers).
Run: `node -e "const d=require('./package.json').devDependencies;const w=['webdriverio','@wdio/cli','@wdio/local-runner','@wdio/mocha-framework','@wdio/spec-reporter'];process.exit([...w.map(k=>d[k]==='9.32.0'),d['@wdio/tauri-service']==='1.5.0'].every(Boolean)?0:1)"; echo "rc=$?"`
Expected: `rc=0`.

- [ ] **Step 2: Keep the E2E sources out of the app's TypeScript project.** `tsconfig.json` (R1-01) gets one key after `"include": ["src"]`:
```json
  "exclude": ["tests/e2e"]
```
Create `tests/e2e/tsconfig.json` (editor support for the WebdriverIO globals; the runner transpiles the specs itself):
```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "skipLibCheck": true,
    "allowJs": true,
    "types": ["node", "@wdio/globals/types", "@wdio/mocha-framework"]
  },
  "include": ["./**/*.ts", "./**/*.mjs", "../../wdio.conf.ts"]
}
```
Run: `npm run typecheck`
Expected: exit 0 (unchanged app project).

- [ ] **Step 3: Write the E2E build.** `tests/e2e/tauri.e2e.conf.json` (merged over `src-tauri/tauri.conf.json` by `--config`; the e2e capability exists only in this build, so the release build never references the plugin's permission):
```json
{
  "app": {
    "security": {
      "capabilities": ["default", { "identifier": "e2e", "windows": ["main"], "permissions": ["wdio-webdriver:default"] }]
    }
  }
}
```
`tests/e2e/build.mjs`:
```js
// Builds the E2E binary src-tauri/target/debug/outlooker[.exe]: debug, no bundle, cargo feature e2e
// (embedded WebDriver + OUTLOOKER_APP_DATA_DIR seam), frontend built with VITE_E2E=1 (native picker bypass).
import { spawnSync } from "node:child_process";

const shell = process.platform === "win32";

function run(cmd, args, env = process.env) {
  const r = spawnSync(cmd, args, { stdio: "inherit", env, shell });
  if (r.status !== 0) {
    console.error(`build: ${cmd} ${args.join(" ")} failed (exit ${r.status ?? r.signal})`);
    process.exit(r.status || 1);
  }
}

// tauri-build copies src-tauri/resources/postgresql/ and fails when it is missing (idempotent fetch).
run("node", ["scripts/fetch-postgres.mjs"]);
run(
  "npx",
  ["tauri", "build", "--debug", "--no-bundle", "--features", "e2e", "--config", "tests/e2e/tauri.e2e.conf.json"],
  { ...process.env, VITE_E2E: "1" },
);
```

- [ ] **Step 4: Build it.**

Run: `npm run e2e:build`
Expected: exit 0; then `test -x src-tauri/target/debug/outlooker; echo "rc=$?"` prints `rc=0`.
Run: `grep -c "__outlookerE2ePicks" dist/assets/*.js`
Expected: a count ≥ 1 in one asset file (the `VITE_E2E=1` branch of `src/api/dialog.ts` is in the bundle).

- [ ] **Step 5: Write the library preparation module** `tests/e2e/prepare.mjs` (a module used by `run.mjs` and `wdio.conf.ts`, and a CLI that prepares every group):
```js
// E2E app-data dirs and seeded libraries under scripts/tmp/e2e/ (gitignored). The user's real app data dir is
// never used: every run sets OUTLOOKER_APP_DATA_DIR to one of these dirs.
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

export const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
export const E2E_ROOT = path.join(REPO_ROOT, "scripts", "tmp", "e2e");
export const APP_DATA = {
  fresh: path.join(E2E_ROOT, "fresh"),
  seeded: path.join(E2E_ROOT, "seeded"),
  big: path.join(E2E_ROOT, "big"),
};
export const GROUPS = ["smoke", "flow", "seeded", "scroll"];
export const GROUP_APP_DATA = { smoke: APP_DATA.fresh, flow: APP_DATA.fresh, seeded: APP_DATA.seeded, scroll: APP_DATA.big };

const LIBRARIES = {
  [APP_DATA.seeded]: { root: path.join(E2E_ROOT, "seeded-root"), language: "zh-TW", messages: 2000 },
  [APP_DATA.big]: { root: path.join(E2E_ROOT, "big-root"), language: "en", messages: 120000 },
};

function fail(step, detail) {
  console.error(`prepare: ${step} failed: ${detail}`);
  process.exit(1);
}

/** Seeds the library once (re-runs reuse a root that has its marker) and writes the bootstrap file. */
function prepareLibrary(appData) {
  const lib = LIBRARIES[appData];
  if (!fs.existsSync(path.join(lib.root, "outlooker-data.json"))) {
    fs.rmSync(lib.root, { recursive: true, force: true });
    const args = ["run", "--release", "--locked", "-p", "outlooker-loadgen", "--", "seed",
      "--data-root", lib.root, "--messages", String(lib.messages)];
    const r = spawnSync("cargo", args, { cwd: path.join(REPO_ROOT, "src-tauri"), stdio: "inherit", shell: process.platform === "win32" });
    if (r.status !== 0) fail(`seed ${path.basename(lib.root)}`, `exit ${r.status ?? r.signal}`);
  }
  fs.mkdirSync(appData, { recursive: true });
  const bootstrap = { data_dir: lib.root, language: lib.language };
  fs.writeFileSync(path.join(appData, "settings.json"), JSON.stringify(bootstrap, null, 2) + "\n");
}

/** Makes the app-data dir of `group` ready: `fresh/` is emptied on every call (onboarding starts from zero). */
export function prepareGroup(group) {
  const appData = GROUP_APP_DATA[group];
  if (!appData) fail("group", `unknown group ${group}`);
  fs.mkdirSync(E2E_ROOT, { recursive: true });
  if (appData === APP_DATA.fresh) {
    fs.rmSync(appData, { recursive: true, force: true });
    fs.mkdirSync(appData, { recursive: true });
  } else {
    prepareLibrary(appData);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  for (const group of GROUPS) prepareGroup(group);
}
```

- [ ] **Step 6: Check the fresh-dir handling** (no seeding needed).

Run: `mkdir -p scripts/tmp/e2e/fresh/leftover`, then `node -e "import('./tests/e2e/prepare.mjs').then(m => { m.prepareGroup('smoke'); const fs = require('node:fs'); process.exit(fs.readdirSync(m.APP_DATA.fresh).length === 0 ? 0 : 1); })"; echo "rc=$?"`
Expected: `rc=0` (the leftover entry was removed).
Run: `git check-ignore -q scripts/tmp/e2e/fresh; echo "rc=$?"`
Expected: `rc=0` (gitignored).

- [ ] **Step 7: Write the runner** `tests/e2e/run.mjs`:
```js
// Runs the E2E groups one after another, one app process and one app-data dir per `wdio run`
// (the tauri-service launcher starts one app per capability at once). `--spec tests/e2e/<group>.e2e.ts` runs one group.
import { spawnSync } from "node:child_process";
import path from "node:path";
import { GROUPS, prepareGroup } from "./prepare.mjs";

const at = process.argv.indexOf("--spec");
const only = at >= 0 ? path.basename(process.argv[at + 1] ?? "").replace(/\.e2e\.ts$/, "") : null;
if (only !== null && !GROUPS.includes(only)) {
  console.error(`run: unknown spec ${process.argv[at + 1]} (groups: ${GROUPS.join(", ")})`);
  process.exit(1);
}

for (const group of only ? [only] : GROUPS) {
  prepareGroup(group);
  console.log(`run: group ${group}`);
  const r = spawnSync("npx", ["wdio", "run", "wdio.conf.ts", "--spec", `tests/e2e/${group}.e2e.ts`], {
    stdio: "inherit",
    env: { ...process.env, OUTLOOKER_E2E_GROUP: group },
    shell: process.platform === "win32",
  });
  if (r.status !== 0) {
    console.error(`run: group ${group} failed (exit ${r.status ?? r.signal})`);
    process.exit(r.status || 1);
  }
}
```

- [ ] **Step 8: Write the WebdriverIO config** `wdio.conf.ts`:
```ts
import path from "node:path";
import { GROUP_APP_DATA, REPO_ROOT } from "./tests/e2e/prepare.mjs";

const group: string = process.env.OUTLOOKER_E2E_GROUP ?? "smoke";
const appData: string | undefined = (GROUP_APP_DATA as Record<string, string>)[group];
if (!appData) throw new Error(`unknown OUTLOOKER_E2E_GROUP ${group}`);
const binary = path.join(REPO_ROOT, "src-tauri", "target", "debug", process.platform === "win32" ? "outlooker.exe" : "outlooker");

export const config = {
  runner: "local",
  specs: [`./tests/e2e/${group}.e2e.ts`],
  maxInstances: 1,
  capabilities: [
    {
      browserName: "tauri",
      "tauri:options": { application: binary },
      "wdio:tauriServiceOptions": { env: { OUTLOOKER_APP_DATA_DIR: appData } },
    },
  ],
  services: [["@wdio/tauri-service", { driverProvider: "embedded", captureBackendLogs: false, captureFrontendLogs: false }]],
  framework: "mocha",
  mochaOpts: { ui: "bdd", timeout: 600_000 },
  reporters: ["spec"],
  logLevel: "warn",
  waitforTimeout: 10_000,
};
```

- [ ] **Step 9: Write the shared helpers** `tests/e2e/support.ts`:
```ts
/** `[data-testid="<id>"]` (R1-06 / R1-07 Produces lists). */
export const tid = (id: string): string => `[data-testid="${id}"]`;

/** Queues native-picker results for the VITE_E2E=1 bypass of src/api/dialog.ts. */
export async function setPicks(picks: Array<string[] | string | null>): Promise<void> {
  await browser.execute((p: Array<string[] | string | null>) => {
    (window as unknown as { __outlookerE2ePicks: Array<string[] | string | null> }).__outlookerE2ePicks = p;
  }, picks);
}

/** Visible text of the first match, "" when nothing matches (safe inside waitUntil across re-renders). */
export async function textOf(selector: string): Promise<string> {
  const el = await $(selector);
  return (await el.isExisting()) ? el.getText() : "";
}

/** Attribute of the first match, null when nothing matches. */
export async function attrOf(selector: string, name: string): Promise<string | null> {
  const el = await $(selector);
  return (await el.isExisting()) ? el.getAttribute(name) : null;
}

export async function count(selector: string): Promise<number> {
  return (await $$(selector)).length;
}
```

- [ ] **Step 10: Write the smoke spec** `tests/e2e/smoke.e2e.ts` (the dry run: works against whatever core exists — the Round 1 shell shows the Failed screen because its commands are missing):
```ts
import { tid } from "./support";

describe("smoke", () => {
  it("shows the app frame, a blocking or failed screen, or onboarding", async () => {
    const anyFrame = [tid("title-bar"), tid("blocking-screen"), tid("failed-screen"), tid("onboarding-lang-en")].join(", ");
    await $(anyFrame).waitForDisplayed({ timeout: 60_000, timeoutMsg: "no app frame within 60 s" });
  });
});
```

- [ ] **Step 11: See the smoke run fail without the embedded driver.** Build the app crate without feature `e2e` into the same binary path, then run the smoke group:

Run (in `src-tauri/`): `cargo build -p outlooker --locked`; then (repo root): `node tests/e2e/run.mjs --spec tests/e2e/smoke.e2e.ts; echo "rc=$?"`
Expected: non-zero `rc`; the output contains `Embedded WebDriver server did not become ready` (no plugin in a default build: proves the run depends on the `e2e` feature).

- [ ] **Step 12: Run the smoke dry run green.**

Run: `npm run e2e -- --spec tests/e2e/smoke.e2e.ts; echo "rc=$?"`
Expected: `rc=0`; the spec reporter lists `smoke` → `✓ shows the app frame, a blocking or failed screen, or onboarding` and the launcher summary `Spec Files: 1 passed, 1 total`.

- [ ] **Step 13: Commit the harness.** Commit `package.json`, `package-lock.json`, `tsconfig.json`, `wdio.conf.ts`, `tests/e2e/tsconfig.json`, `tests/e2e/tauri.e2e.conf.json`, `tests/e2e/build.mjs`, `tests/e2e/prepare.mjs`, `tests/e2e/run.mjs`, `tests/e2e/support.ts`, `tests/e2e/smoke.e2e.ts` with message `test(e2e): webdriverio harness with embedded tauri driver and smoke dry run`.

- [ ] **Step 14: Write the flow spec** `tests/e2e/flow.e2e.ts` (Part 02 §10 E2E row; runs after R2-13 lands):
```ts
import assert from "node:assert/strict";
import path from "node:path";
import { REPO_ROOT } from "./prepare.mjs";
import { attrOf, count, setPicks, textOf, tid } from "./support";

const PST = path.join(REPO_ROOT, "tests", "fixtures", "external", "outlook.pst");
const OUTLOOK_ROW = `${tid("pst-row")}[data-pst-name="outlook"]`;

describe("flow: onboarding, import, search, preview, Advanced SQL, removal", () => {
  it("onboards into a new library", async () => {
    await $(tid("onboarding-lang-en")).waitForClickable({ timeout: 60_000 });
    await $(tid("onboarding-lang-en")).click();
    await $(tid("onboarding-next")).click();
    await $(tid("onboarding-dir-default")).click();
    await $(tid("onboarding-next")).waitForEnabled();
    await $(tid("onboarding-next")).click();
    await $(tid("onboarding-db-ready")).waitForDisplayed({ timeout: 120_000 });
    await $(tid("onboarding-next")).click();
  });

  it("imports outlook.pst to Completed", async () => {
    await setPicks([[PST]]);
    await $(tid("onboarding-pick-pst")).click();
    await browser.waitUntil(async () => (await textOf(`${OUTLOOK_ROW} ${tid("pst-status")}`)) === "Completed", {
      timeout: 120_000,
      timeoutMsg: "outlook.pst did not reach Completed",
    });
  });

  it("finds the Welcome mail and previews it in the sandboxed frame", async () => {
    await $(tid("nav-search")).click();
    await $(tid("search-box")).setValue("Welcome");
    await $(tid("mail-row")).waitForDisplayed({ timeout: 30_000 });
    await $(tid("mail-row")).click();
    await browser.waitUntil(
      async () => (await textOf(tid("reading-pane"))).includes("Welcome to Microsoft Office Outlook 2003"),
      { timeout: 30_000, timeoutMsg: "reading pane did not show the Welcome subject" },
    );
    assert.equal(await attrOf(tid("mail-preview"), "sandbox"), "");
  });

  it("lists the mail in Advanced SQL mail-list mode", async () => {
    await $(tid("nav-advancedSql")).click();
    await $(tid("sql-new")).click();
    await $(tid("sql-editor")).click();
    await $(tid("sql-editor")).addValue("SELECT id, subject FROM v_mail");
    await $(tid("sql-run")).click();
    await browser.waitUntil(async () => (await attrOf(tid("sql-result"), "data-mode")) === "mailList", {
      timeout: 30_000,
      timeoutMsg: "sql-result never reached data-mode=mailList",
    });
    assert.equal(await count(`${tid("sql-result")} ${tid("mail-row")}`), 1);
  });

  it("removes the PST", async () => {
    await $(tid("nav-import")).click();
    await $(`${OUTLOOK_ROW} ${tid("pst-remove")}`).click();
    await $(tid("remove-confirm")).click();
    await browser.waitUntil(async () => (await count(tid("pst-row"))) === 0, {
      timeout: 60_000,
      timeoutMsg: "pst-row still present after removal",
    });
    await browser.waitUntil(async () => (await textOf("body")).includes("No PST imported yet"), {
      timeout: 10_000,
      timeoutMsg: "import empty state not shown",
    });
  });
});
```

- [ ] **Step 15: Write the seeded spec** `tests/e2e/seeded.e2e.ts` (R2-10 seed: one message with subject `E2E 報價單 inline` and one inline image):
```ts
import assert from "node:assert/strict";
import { attrOf, tid } from "./support";

const IMAGE_PREFIX = process.platform === "win32" ? "http://mailimg.localhost/" : "mailimg://localhost/";

describe("seeded library (zh-TW): Chinese search and inline image", () => {
  it("finds the seeded mail and serves its inline image through mailimg", async () => {
    await $(tid("search-box")).waitForDisplayed({ timeout: 120_000 });
    await $(tid("search-box")).setValue("報價單");
    const row = $(`//*[@data-testid="mail-row"][contains(., "E2E 報價單 inline")]`);
    await row.waitForDisplayed({ timeout: 30_000 });
    await row.click();
    await browser.waitUntil(async () => ((await attrOf(tid("mail-preview"), "srcdoc")) ?? "").includes("<img"), {
      timeout: 30_000,
      timeoutMsg: "preview has no image",
    });
    // The sandbox="" frame has an opaque origin, so WebDriver cannot enter it: read the URL from srcdoc and load
    // it in the main document, which has the same CSP (img-src includes the mailimg scheme).
    const srcdoc = (await attrOf(tid("mail-preview"), "srcdoc")) ?? "";
    const src = /<img[^>]*\ssrc="([^"]+)"/.exec(srcdoc)?.[1] ?? "";
    assert.ok(src.startsWith(IMAGE_PREFIX), `image src ${src}`);
    const width = await browser.execute(async (url: string) => {
      const img = new Image();
      img.src = url;
      try {
        await img.decode();
      } catch {
        return 0;
      }
      return img.naturalWidth;
    }, src);
    assert.ok(width > 0, `naturalWidth ${width}`);
  });
});
```
Known limit (leader ruling): this proves the `mailimg` scheme serves the WebP through the CSP, not that the opaque-origin `sandbox=""` frame renders it. Rendering inside the frame stays covered by the R1-07 `MailPreview` unit test and the manual macOS / Windows release check.

- [ ] **Step 16: Write the scroll spec** `tests/e2e/scroll.e2e.ts` (Part 03 §5.2 cap; spec §0.3 webview scroll risk):
```ts
import assert from "node:assert/strict";
import { textOf, tid } from "./support";

const MAX_ROUNDS = 600; // 500 pages of 200 reach the cap; more means the list never stops

async function scrollHeight(): Promise<number> {
  return browser.execute((sel: string) => document.querySelector(sel)?.scrollHeight ?? 0, tid("mail-list"));
}

describe("scroll to the 100,000-row cap in the real webview", () => {
  it("loads pages on scroll and shows the cap row", async () => {
    await $(tid("search-box")).waitForDisplayed({ timeout: 120_000 });
    await $(tid("search-box")).setValue("loadgen");
    await browser.waitUntil(async () => (await textOf(tid("result-count"))).includes("10,000+"), {
      timeout: 60_000,
      timeoutMsg: "result-count never showed 10,000+",
    });
    await $(tid("mail-row")).waitForDisplayed({ timeout: 30_000 });
    let rounds = 0;
    while (!(await $(tid("result-cap-row")).isDisplayed())) {
      rounds += 1;
      assert.ok(rounds <= MAX_ROUNDS, `cap row not reached after ${MAX_ROUNDS} scrolls`);
      const before = await scrollHeight();
      await browser.execute((sel: string) => {
        const list = document.querySelector(sel);
        if (list) list.scrollTop = list.scrollHeight;
      }, tid("mail-list"));
      // Each loaded page grows the virtual total size (32 px rows); the cap row ends the loop.
      await browser.waitUntil(
        async () => (await scrollHeight()) > before || (await $(tid("result-cap-row")).isDisplayed()),
        { timeout: 10_000, timeoutMsg: `no new page within 10 s at round ${rounds}` },
      );
    }
    assert.ok((await scrollHeight()) >= 3_200_000, `scrollHeight ${await scrollHeight()}`);
    const rows = await $$(tid("mail-row"));
    const lastId = await rows[rows.length - 1]!.getAttribute("data-mail-id");
    assert.ok(/^\d+$/.test(lastId ?? ""), `last data-mail-id ${lastId}`);
  });
});
```

- [ ] **Step 17: Run the three real-core groups** (after R2-13 and R2-10 landed; first `npm run e2e:build` so the binary contains the real core).

Run: `npm run e2e:build` — exit 0.
Run: `node tests/e2e/run.mjs --spec tests/e2e/flow.e2e.ts; echo "rc=$?"`
Expected: `rc=0`; reporter shows 5 `✓` under `flow: onboarding, import, search, preview, Advanced SQL, removal` and `Spec Files: 1 passed, 1 total`.
Run: `node tests/e2e/run.mjs --spec tests/e2e/seeded.e2e.ts; echo "rc=$?"`
Expected: `rc=0`; `Spec Files: 1 passed, 1 total`. The first run seeds `scripts/tmp/e2e/seeded-root` (2,000 messages) before the app starts.
Run: `node tests/e2e/run.mjs --spec tests/e2e/scroll.e2e.ts; echo "rc=$?"` — long-running (120,000-message seed on the first run, then 500 page loads): the gate-runner runs it if it exceeds the foreground limit.
Expected: `rc=0`; `Spec Files: 1 passed, 1 total`.
A failing group is a defect in the app or the core, never in the waits: do not raise a timeout to make it pass; go to Step 18.

- [ ] **Step 18: Integration fixes (real core).** Run `npm run tauri dev` and walk all five screens (Onboarding on an empty app data dir, Import, Search, Advanced SQL, Settings) against the real core, plus every failure from Step 17. For each frontend defect, in this order:
  1. Write a Vitest test in the test file of the touched component (for example `src/screens/search/search.test.tsx`) that reproduces it with the R1-06 `eventsMock` and the automocked `src/api/commands`; run `npx vitest run <that file>` and see it fail with the defect's symptom.
  2. Fix the component under `src/**` (never `src/api/bindings/**`; a contract defect goes to the leader for R2-13).
  3. Run `npx vitest run <that file>` green, then the failing E2E group green.
  4. Append one line to `scripts/tmp/r2-14-defects.md`: `<symptom> | <file:line fixed> | <test name>`, and commit the component and its test (`git add <the two files>`, `git commit -F scripts/tmp/<msgfile> -- <the two files>`, message `fix(ui): <symptom>`).
  A defect in the core or the App Shell is reported to the leader with the failing E2E output; it is not patched here.

- [ ] **Step 19: Commit the specs.** Commit `tests/e2e/flow.e2e.ts`, `tests/e2e/seeded.e2e.ts`, `tests/e2e/scroll.e2e.ts` with message `test(e2e): onboarding-import-search-sql-remove flow, seeded mailimg preview, 100k scroll cap`.

- [ ] **Step 20: Final checks.**

Run, one after another: `npm run typecheck; echo "rc=$?"`, `npm test; echo "rc=$?"`, `npm run build; echo "rc=$?"`
Expected: `rc=0` for each (the acceptance chain `npm run typecheck && npm test && npm run build`).
Run: `grep -rn "browser.pause" tests/e2e; echo "rc=$?"`
Expected: `rc=1` (nothing found).
Run: `npm run e2e -- --spec tests/e2e/smoke.e2e.ts; echo "rc=$?"`
Expected: `rc=0` (the dry run still passes on the real core).
Gate-runner only (long-running): `npm run e2e > scripts/tmp/e2e-full.log 2>&1; echo "RC=$?" >> scripts/tmp/e2e-full.log`
Expected: `RC=0`; `grep -c "run: group" scripts/tmp/e2e-full.log` prints `4` (smoke, flow, seeded, scroll) and `grep -cE "Spec Files:.*1 passed, 1 total" scripts/tmp/e2e-full.log` prints `4`; the spec reporter lists every test of the four files with `✓`.

<!-- PLAN-END -->
