# Frontend test cleanup plan (round 2, 2026-10-23)

Base: HEAD `8cee9f9`, `git status --porcelain src tests` empty. Authority: `contract-test-suite-pruning.md` criteria 2, 5, 5a. Input: `test-inventory-frontend.md`.
Timings in this file come from `scripts/tmp/fe-plan/run1.json` (a full `vitest run` on an idle machine, 79/79 green, wall 5.1–6.3 s). They are about half of the inventory's numbers, which were taken under load. The ranking is the same.

Decision rule (user directive): keep a UI test only when it guards logic whose failure costs the user data or a function (state machines, query building, API contracts, destructive-action gates, security attributes). Delete tests that only check click→callback wiring, label text, styling, or React internals. A test over 1 s is a defect: rewrite it if it guards core logic, delete it if not.

## 0. Facts that differ from the prompt or the inventory

1. **Plan file name.** Contract criterion 2 names `docs/logs/2026-10-23/test-cleanup-plan.md`. This file is `test-cleanup-plan-frontend.md`, as the prompt asks. The lead must merge it with the Rust plan, or link both from `test-cleanup-plan.md`, before criterion 2 can pass.
2. **The column-resize work is committed now** (`690df2d`, `af2afa3`, `b9d5230`), and the working tree is clean, so **no file is a conflict**. The lead directed: decide its tests like any other in-scope test. They are in scope now: 79 tests in total.
3. **Only one test is over 1 s on an idle machine:** `pages.test.tsx:32` (1255 ms). `AppFrame.test.tsx:103` is 950 ms (1014 ms when run alone), `global.test.tsx:61` is 810 ms, and `settings.test.tsx:23` is 702 ms. All four still get a decision, as the prompt asks.
4. **Correction to the inventory:** it lists `AppFrame.test.tsx:80` (onboarding while setup is needed) as a duplicate of `onboarding.test.tsx:16`. That is wrong. `onboarding.test.tsx` renders `OnboardingScreen` directly. `AppFrame:80` is the only test that the App routes phase `needsSetup` to onboarding. KEEP.
5. **"Cold first test" cost:** the first rendering test in each file pays roughly 200–500 ms for Fluent styles and the i18n setup. When you delete that test, the cost moves to the next test. It does not disappear. For example, `settings:23` is 376 ms alone and 554–702 ms in a full run.
6. **Toast cost:** a Fluent toast takes about 500 ms to show up in jsdom. Measured: in `AppFrame:103`, `startMove` takes 241 ms and the toast `findByText` takes 523 ms. That wait is the whole reason the test is slow.

## 1. Decision table (vitest)

D = DELETE, K = KEEP, RW = REWRITE, M = MERGE. ms = run1.

| file:line | test | ms | decision | reason |
|---|---|---|---|---|
| src/api/api.test.ts:15 | wraps exactly the registered commands | 2 | K | contract drift guard against bindings/commands.json |
| src/api/api.test.ts:19 | names exactly the emitted events | 0 | K | contract drift guard against bindings/events.json |
| src/api/api.test.ts:23 | passes camelCase arguments and returns the payload | 1 | K | IPC argument contract |
| src/api/api.test.ts:29 | re-throws AppError rejections unchanged and wraps others | 1 | K | error normalisation contract |
| src/api/dialog.test.ts:9 | normalizes the PST picker result | 2 | K | null/string/array → string[]; a wrong shape breaks import |
| src/api/dialog.test.ts:18 | returns the chosen folder or null | 0 | D | `typeof chosen === "string"` pass-through; no logic |
| src/app/AppFrame.test.tsx:42 | library:changed … updates counts.messages | 310 | K | event → global counts state |
| src/app/AppFrame.test.tsx:49 | Failed screen Retry calls appRetry | 47 | D | click→callback wiring (`onRetry={() => api.appRetry()}`) |
| src/app/AppFrame.test.tsx:56 | shows the blocking screen while the database starts and the frame when ready | 34 | K | phase state machine |
| src/app/AppFrame.test.tsx:65 | keeps the SQL text and the search text when switching screens | 292 | K | user input survives navigation (regression) |
| src/app/AppFrame.test.tsx:80 | shows onboarding while setup is needed | 39 | K | only App-level needsSetup → onboarding route (see 0.4) |
| src/app/AppFrame.test.tsx:103 | keeps the move dialog through stoppingDb, startingDb and ready and toasts done | 950 | RW | core multi-phase move state machine. The slow part is the cosmetic toast wait (0.6). Fix: drop the toast `findByText`, assert dialog closed and `title-bar` present. Retitle "…and closes it when done". Measured 162 ms (solo) |
| src/app/AppFrame.test.tsx:118 | shows a failed move's error after the database is back in the old folder | 170 | K | failure branch of the move machine; the error text is the user's only signal |
| src/app/boot.test.ts:26 | boot subscribes to every event before calling app_status | 4 | K | ordering invariant (lost events) |
| src/app/boot.test.ts:46 | does not call pst_list before the database is ready | 1 | K | gating |
| src/app/boot.test.ts:53 | routes events to the sink and the pstList cache | 2 | K | event routing + cache patch |
| src/app/global.test.tsx:52 | locale files: zh-TW and en have equal key sets per namespace | 5 | K | a missing key shows a raw key to users of one language; 5 ms |
| src/app/global.test.tsx:61 | screens rendered in cimode show only keys | 810 | D | UI-COSMETIC (hard-coded string detector), renders 6 screens; no data or function at risk |
| src/app/global.test.tsx:80 | each screen renders its empty state | 134 | D | empty-state text; dedupe (see section 2) |
| src/app/global.test.tsx:91 | no dark theme tokens in src | 1 | D | style rule as a grep, no runtime behaviour; move it to the prevention mechanism if the user wants the rule (section 6) |
| src/components/common/common.test.tsx:26 | formatAppError maps every AppError kind to a localized string | 11 | K | every error kind gives user-readable text in both languages |
| src/components/common/common.test.tsx:40 | toAppError keeps AppError values and wraps anything else | 1 | K | error normalisation |
| src/components/common/common.test.tsx:45 | display helpers format numbers and bytes, also in cimode | 26 | K | formatter logic (units, rounding) |
| src/components/common/display.test.ts:7 | reuses one Intl.DateTimeFormat per (locale, options) | 18 | D | memoisation implementation detail; delete the file |
| src/components/common/columnWidths.test.tsx:12 | writes a resized width to localStorage | 19 | K | persistence: a resized width survives a restart |
| src/components/common/columnWidths.test.tsx:18 | restores a stored width on remount and keeps unknown ids | 4 | K | persistence read + merge; does not drop other grids' widths |
| src/components/common/columnWidths.test.tsx:31 | falls back to defaults on malformed storage | 5 | K | robustness at the localStorage boundary (corrupt data must not break a grid) |
| src/components/common/columnWidths.test.tsx:40 | spread onto DataGrid renders the stored width | 116 | D | UI-COSMETIC: inline width style of a Fluent DataGrid; the hook logic is tested by :12/:18/:31 |
| src/components/dsl/dsl.test.tsx:31 | DSL builder output for the Part 03 §5.1 worked example deep-equals the spec JSON | 4 | K | query building vs spec |
| src/components/dsl/dsl.test.tsx:45 | drops incomplete conditions and empty groups, null for empty root | 1 | K | query building |
| src/components/dsl/dsl.test.tsx:54 | measures depth and builds the folder tree | 1 | K | limits + tree logic |
| src/components/dsl/dsl.test.tsx:67 | Add group adds a group with one empty condition | 269 | D | button→onChange wiring |
| src/components/dsl/dsl.test.tsx:78 | disables Add condition at the leaf budget and hides Add group at depth 4 | 187 | D | button visibility. The limits themselves are tested by depthOf/countLeaves (:31, :54) and by composeQuery `tooManyConditions` (compose.test.ts:9) |
| src/components/mail/mail.test.tsx:29 | applies the stored sender width … compact mode | 190 | D | UI-COSMETIC: `gridTemplateColumns` style and header visibility |
| src/components/mail/mail.test.tsx:39 | preview iframe has sandbox="" and no allow- token | 24 | K | security: untrusted mail HTML isolation |
| src/components/mail/mail.test.tsx:49 | preview replaces srcdoc on the same iframe | 14 | D | React reconciliation detail |
| src/components/mail/mail.test.tsx:58 | reading pane shows the header, attachments and the plain-text fallback | 227 | RW | keep only the body-branch logic: an HTML mail goes to the sandboxed preview (srcdoc), a mail without HTML shows plain text and no iframe. Drop the header, address, folder and download-button label checks. Retitle "reading pane renders HTML in the sandboxed preview and plain text when the mail has no HTML" |
| src/components/mail/mail.test.tsx:70 | reading pane renders HTML through MailPreview and shows the pick state without a mail | 53 | M → :58 | its HTML-branch assert moves into :58; the "Select a message" label check is dropped |
| src/components/mail/mail.test.tsx:79 | buildEntries inserts month headers and an undated group | 1 | K | list-model transform |
| src/components/mail/mail.test.tsx:83 | ArrowDown on the list selects the next mail | 32 | D | keyboard UI interaction; a failure is visible at once and the mouse still works |
| src/components/mail/mail.test.tsx:92 | requests the next page only when the end is within 50 rows | 108 | K | paging trigger; if it breaks, results past page 1 are unreachable |
| src/components/mail/mail.test.tsx:107 | restores scrollTop after the list is hidden and shown again | 156 | K | regression guard for custom restore code (Activity hide/show) |
| src/components/mail/pages.test.tsx:22 | nextPageParam stops at the last cursor and at the cap | 3 | K | cap logic |
| src/components/mail/pages.test.tsx:32 | with 600 mock pages … stops requesting after 100,000 rows and shows the cap row | 1255 | RW | core 100k cap of the hook. Fix: seed the QueryClient with 500 full pages (`setQueryData`) instead of 500 serial `fetchNextPage` round-trips. Then assert capped/hasMore=false/rows=100,000, assert one more `fetchNextPage` makes no `searchMail` call, and assert the cap row. Measured 136–323 ms. Red check: with 499 pages it fails (`capped` false) |
| src/components/mail/pages.test.tsx:65 | a new seq evicts the superseded searchMail entry | 106 | K | cache eviction (memory, stale results) |
| src/components/mail/pages.test.tsx:80 | keeps the current search pages while no screen observes them | 56 | K | gcTime regression |
| src/components/sql-editor/sql-editor.test.tsx:8 | codePointOffsetToUtf16 converts a 1-based code point offset | 2 | K | offset conversion |
| src/components/sql-editor/sql-editor.test.tsx:17 | renders the document, replaces it from props and runs on Mod-Enter | 220 | K | only real CodeMirror coverage. Prop→doc sync is what loads a saved query, and Mod-Enter runs SQL. Screen tests mock the editor |
| src/components/sql-editor/sql-editor.test.tsx:29 | sqlError position marks the character in the editor | 20 | D | highlight styling; the offset math is tested by :8 |
| src/screens/advanced-sql/advanced-sql.test.tsx:44 | sqlExecute is called with maxRows: 10000 | 283 | K | request contract |
| src/screens/advanced-sql/advanced-sql.test.tsx:52 | truncated: true shows the cap MessageBar | 114 | D | banner text for a backend flag |
| src/screens/advanced-sql/advanced-sql.test.tsx:59 | shows MailList for a result with an id column and a table otherwise | 137 | RW | keep the mode→view switch and the auto-open `mailGet(7)`. Delete the 4 cosmetic lines (avatar absence, subject text, "1 ids matched no message") |
| src/screens/advanced-sql/advanced-sql.test.tsx:79 | clicking a table row shows its key/value list in column 4 | 60 | D | click→detail rendering |
| src/screens/advanced-sql/advanced-sql.test.tsx:93 | applies a stored column width by column name … | 127 | D | UI-COSMETIC: `gridTemplateColumns` style |
| src/screens/advanced-sql/advanced-sql.test.tsx:104 | Run is disabled while executing | 59 | K | run/cancel state machine, `sqlCancel(executionId)` contract |
| src/screens/advanced-sql/advanced-sql.test.tsx:118 | shows sqlError text with its position and the empty states | 79 | RW | keep the SQL error + position display, the user's only feedback on a failed query. Delete the empty-state and idle-state label checks (dedupe). Retitle "shows sqlError text with its position" |
| src/screens/advanced-sql/advanced-sql.test.tsx:129 | saves a new query by name and reports a duplicate name | 177 | K | save flow with duplicate-name retry (data entry) |
| src/screens/import/import.test.tsx:23 | importCache applies status changes, removal and progress without mutation | 5 | K | pure reducers |
| src/screens/import/import.test.tsx:34 | removal dialog shows messageCount and calls pstRemove only after confirm | 513 | K | confirm gate before a destructive call |
| src/screens/import/import.test.tsx:45 | shows the empty state when no PST is imported | 52 | D | empty-state text (dedupe) |
| src/screens/import/import.test.tsx:50 | adds PST files through the picker and toasts rejected paths | 553 | K | enqueue + rejected-file feedback (otherwise a silent drop). Under 1 s; the cost is the toast (0.6) |
| src/screens/import/import.test.tsx:62 | queue card actions call cancel and resume | 54 | D | button→api wiring; progress label is cosmetic |
| src/screens/onboarding/onboarding.test.tsx:16 | onboarding initializes with the chosen dir and language | 230 | K | wizard state machine → appInitialize contract |
| src/screens/onboarding/onboarding.test.tsx:27 | shows the init error and stays on the location step | 72 | K | error branch keeps the user on the step |
| src/screens/onboarding/onboarding.test.tsx:36 | step 4 enqueues the picked PST files and navigates to Import | 102 | D | picker→enqueue→go wiring; e2e flow.e2e.ts:21 covers it for real |
| src/screens/search/compose.test.ts:9 | composeQuery builds simple, dsl and problem results | 4 | K | query building + limits |
| src/screens/search/compose.test.ts:30 | termGroup is the Part 03 §5.1 simple-query rule | 0 | K | query building vs spec |
| src/screens/search/compose.test.ts:43 | refine conditions and the builder become nodes of one and-root | 1 | K | query building |
| src/screens/search/search.test.tsx:48 | searchCount … renders 10,000+ / 9,999 | 414 | D | count label formatting; query composition is tested in compose.test.ts |
| src/screens/search/search.test.tsx:60 | a stale search response (older sequence) is dropped | 161 | K | race handling (wrong results shown) |
| src/screens/search/search.test.tsx:75 | library:changed refetches filterOptions and shows the new-mail bar without reloading the list | 321 | K | cache/refresh state machine |
| src/screens/search/search.test.tsx:94 | leaving Search, a library change and coming back neither refetches nor misses the new-mail bar | 176 | K | hidden-screen state regression |
| src/screens/search/search.test.tsx:112 | keeps the refine pane PST list visible while filterOptions refetches | 87 | K | keep-previous-data regression |
| src/screens/search/search.test.tsx:126 | shows the short-query hint and selects the first row | 65 | D | hint display + convenience auto-select. Auto-open of the first row is still covered in SQL mode by advanced-sql:59 (`mailGet(7)`) |
| src/screens/search/search.test.tsx:135 | shows the empty, idle, zero-result and problem states | 89 | D | empty/idle/zero labels (dedupe); the term limit is tested by compose.test.ts:9 |
| src/screens/settings/settings.test.tsx:23 | WebP quality outside 1-100 is not sent | 702 | RW | validation before send. Fix: test the 0/101/50.5/NaN/1/100 boundaries on the pure `isValidQuality`, then one UI pass ("0" → error text, no `settingsUpdate`; "75" → sent). This drops 2 of the 3 render/findByText cycles. Measured 201 ms alone, 554 ms in a full run (cold first test, 0.5) |
| src/screens/settings/settings.test.tsx:35 | maintenance button runs the task | 86 | D | click→api wiring |
| src/screens/settings/settings.test.tsx:41 | disables maintenance while a task is running | 83 | D | button attribute; the backend rejects a second task with `busy/maintenanceRunning` |
| src/screens/settings/settings.test.tsx:49 | moves the data folder after confirmation | 154 | D | DUPLICATE: `AppFrame.test.tsx` `startMove()` (:94-99) clicks settings-move → Move and asserts `dataDirMove("/Volumes/Big/Outlooker")`, in both move tests |

**Counts (79 in scope):** DELETE 25 / REWRITE 6 / MERGE 1 / KEEP 47.

## 2. Empty-state dedupe

Empty-state or idle text was asserted at global:80 (Import, Settings, Onboarding), import:45, search:135, advanced-sql:118 and (as the onboarding route) AppFrame:80. After the plan, vitest asserts empty-state text **nowhere**. The empty state is UI text, and e2e `flow.e2e.ts:63` still sees "No PST imported yet" after a real removal. AppFrame:80 stays because it tests routing, not text.

## 3. e2e decisions (`npm run e2e`, not in `npm test`)

| spec:line | case | decision | reason |
|---|---|---|---|
| tests/e2e/flow.e2e.ts:10 | onboards into a new library | K | only real test of the onboarding wizard + PostgreSQL cluster creation |
| tests/e2e/flow.e2e.ts:21 | imports outlook.pst to Completed | K | only real PST import through the UI |
| tests/e2e/flow.e2e.ts:30 | finds the Welcome mail and previews it in the sandboxed frame | K | real search + preview in WebKit/WebView2 |
| tests/e2e/flow.e2e.ts:42 | lists the mail in Advanced SQL mail-list mode | K | real SQL against v_mail |
| tests/e2e/flow.e2e.ts:55 | removes the PST | K | real destructive path. The 5 cases form one serial session (each needs the state left by the one before), so splitting or partial deletion saves nothing |
| tests/e2e/seeded.e2e.ts:7 | finds the seeded mail (Chinese search) and serves its inline image via mailimg | K | CJK search + `mailimg` protocol + CSP; no unit equivalent; ~1 min |
| tests/e2e/scroll.e2e.ts:11 | loads pages on scroll and shows the cap row | **D** | Cost: 5–12 min of scrolling + a 120,000-message loadgen seed + its own app-data dir. Unique signal: real-webview virtual scroll up to a 3.2 M px container. The cap logic and the cap row are tested by pages:22 + rewritten pages:32, and the on-scroll page trigger by mail:92. The 3.2 M px size is a fixed property checked once in Part 03 §5.2. The test has no failure history in docs/logs. Delete the spec and its `scroll` group in `tests/e2e/prepare.mjs` (`APP_DATA.big`, the `big-root` 120k library, the `GROUPS`/`GROUP_APP_DATA` entries) |

## 4. Work package for round 3 (one sonnet member)

**Owned files** (no other file may change):
`src/api/api.test.ts`, `src/api/dialog.test.ts`, `src/app/AppFrame.test.tsx`, `src/app/boot.test.ts`, `src/components/common/columnWidths.test.tsx`, `src/components/common/common.test.tsx`, `src/screens/search/compose.test.ts`, `src/test/setup.ts`, `src/app/global.test.tsx`, `src/app/testing.tsx`, `src/components/common/display.test.ts` (deleted), `src/components/dsl/dsl.test.tsx`, `src/components/mail/mail.test.tsx`, `src/components/mail/pages.test.tsx`, `src/components/sql-editor/sql-editor.test.tsx`, `src/screens/advanced-sql/advanced-sql.test.tsx`, `src/screens/import/import.test.tsx`, `src/screens/onboarding/onboarding.test.tsx`, `src/screens/search/search.test.tsx`, `src/screens/settings/settings.test.tsx`, `tests/e2e/prepare.mjs`, `tests/e2e/scroll.e2e.ts` (deleted).
Not touched: all production code, `vite.config.ts`.

**The edit script:** `scripts/tmp/fe-plan/apply.py` (gitignored scratch) makes every edit in sections 1, 3 and 7, plus the dead-helper removal below. Every anchor is asserted to occur exactly once, so drift aborts the script instead of producing a wrong edit. Verified on a scratch copy of every test file (`apply.py scratch`): 53/53 green, `tsc --noEmit` clean (including the edited `testing.tsx`). Round 3 must re-check this itself and not trust this note.

Ordered steps (repo root):
1. Precondition: `git status --porcelain src tests` is empty, and `git diff --stat 8cee9f9 HEAD -- <owned files>` is empty. If either is not, STOP and report BLOCKED. The anchors may have moved.
2. Run `python3 -I scripts/tmp/fe-plan/apply.py src`. Expect 24 `edited` lines (some files are edited twice) and 2 `deleted` lines. If an AssertionError occurs, STOP and report it. Do not edit by hand to make it pass.
3. Run `grep -c '^  it(' <file>` on every `src/**/*.test.*`. Expected total = 53: api 4, dialog 1, AppFrame 6, boot 3, global 1, columnWidths 3, common 3, dsl 3, mail 5, pages 4, sql-editor 2, advanced-sql 5, import 3, onboarding 2, compose 3, search 4, settings 1.
4. Run the acceptance commands below.
5. Commit with explicit pathspecs (deleted files included): `git add -- <owned files>` then `git commit -F <msg file> -- <owned files>`. No `git add -A`, no stash/reset/checkout --/clean.

**Dead test-only helpers removed by step 2** (they become dead after the deletions):
- `src/app/testing.tsx` `renderWithProviders`: option `lng` (only `cimode` caller was global:61), returned fields `queryClient` (only global:61 `settle`), `go` (only search:135, onboarding:36), `i18n` (no caller); imports `type i18n`, `type Mock`; the `onGo={go}` prop.
- Inside the edited tests: `columnWidths.test.tsx` Fluent DataGrid imports, `render`, `screen`; `global.test.tsx` mocks, screen imports, `SOURCES`, `KEY`, `NUMERIC`, `SCREENS`, `settle`; `dsl.test.tsx` `OPTIONS`, `DslBuilder`, `GroupDraft`, `fireEvent/screen/vi/renderWithProviders` imports; `pages.test.tsx` `SearchRequest` import; `onboarding.test.tsx` `pickPstFiles` import; `settings.test.tsx` `pickFolder`, `makeDbStatus` imports; `dialog.test.ts` `pickFolder` import.
- e2e: `APP_DATA.big`, the 120k `big-root` library entry, the `scroll` group.
- Not removed (still used): `makeDbStatus` (inside `mockApiDefaults`), `stubViewport`, `makeMailDetail`, `isValidQuality` (production use too), `FailedScreen` export (used by `App.tsx`).
- Already dead before this plan (parking lot, not applied): `renderWithProviders` option `screen` has no caller. The local gitignored data `scripts/tmp/e2e/big`, `big-root` can be deleted by hand.

**Acceptance commands** (each must exit 0; capture `RC=$?` right after the command):
- `mise exec -- npm test` → 53 tests, 17 files, all passed; no single test ≥ 1000 ms (`mise exec -- npx vitest run --reporter=json --outputFile=scripts/tmp/fe-plan/r3.json` then check that the max `assertionResults[].duration` < 1000).
- `mise exec -- npm run typecheck` (covers `tsc -p .` and `tsc -p tests/e2e`).
- `git status --porcelain` after the commit shows none of the owned files.

## 5. Predicted after-state (measured on the scratch simulation, same machine, back-to-back runs)

| | before (HEAD 8cee9f9) | after (scratch) |
|---|---|---|
| vitest tests / files | 79 / 18 | 53 / 17 |
| vitest wall (`vitest run`, back-to-back pairs) | 5.36, 5.11, 4.73, 4.51 s | 4.00, 3.86 s (sections 1–4), 3.33, 3.33 s (with section 7) |
| sum of per-test durations | 10.9 s | 4.7 s |
| tests ≥ 1 s | 1 (pages:32) | 0 |
| slowest test | pages:32 1255 ms | 539 ms (settings:23, cold first test) |
| tests > 300 ms | 9 | ≤ 5 (import:34, import:50, search:60, settings:23, pages:32 rewritten) |
| e2e cases / groups | 7 / 3 | 6 / 2 (−5–12 min and −120k seed per full run) |

Under load (as during the inventory) expect about 2x these numbers. Even so, no test should reach 1 s: the max under load would be about 1.1 s only if settings:23 doubles, which is the main risk in section 6.

## 6. Risks and user decisions

1. **Coverage given up on purpose (accepted by the decision rule):** hard-coded UI strings (global:61), the light-theme-only rule (global:91), the capped count label "10,000+" (search:48; the e2e scroll spec also checked it), keyboard ArrowDown selection (mail:83), and the DSL builder's button limits (dsl:78). If one of these regresses, only a human sees it.
2. **User call — dark-theme rule:** global:91 costs 1 ms and enforces a product rule ("no dark theme"). The plan deletes it as a style rule. If the user wants the rule kept, policy-opus's prevention mechanism (a grep check) is the right home, not a vitest case.
3. **Column-resize tests:** decided by the same rule (lead direction). The 3 styling tests are DELETE, and the 3 persistence tests are KEEP.
4. **User call — e2e scroll spec deletion:** this is the only real-webview check of a 100k-row virtual list. The plan deletes it (section 3) because the cost is 5–12 min plus a 120k seed and the logic has unit coverage. The user can veto this.
5. **Timing variance:** the "first rendering test of a file" cost (0.5) is 200–500 ms, and it doubles under load. settings:23 is now the slowest test, at 554 ms in a parallel run. If the round-3 max-duration check fails under load, rerun it on an idle machine. Do not change the test to pass.
6. **Anchors drift:** `apply.py` aborts if any owned file changes after `8cee9f9`. The other session committed recently, so check step 1 first.

## 7. Shared expensive setups (user decision: do expensive setup once and share it)

Measured with single-file runs (vitest `Duration` breakdown) and a micro-probe (`scripts/tmp/fe-plan/probe/cost.test.tsx`).

| setup | where | measured cost | verdict / fix | saving |
|---|---|---|---|---|
| jsdom environment for files that never touch the DOM | src/api/api.test.ts:1, src/api/dialog.test.ts:1, src/app/boot.test.ts:1, src/app/global.test.tsx:1 (after the rewrite), src/components/common/common.test.tsx:1, src/screens/search/compose.test.ts:1 | ~250 ms of environment setup per file (common.test.tsx alone: 904 ms, of which 29 % is environment and 2 % is tests) | **FIX:** add `// @vitest-environment node` as line 1 of these 6 files. Make `src/test/setup.ts:6,11,12` node-safe: `globalThis.localStorage?.clear()` and `typeof Range !== "undefined"` guards. Both are in apply.py | measured wall 3.86–4.00 s → 3.33 s (≈0.5 s; about 1.5 s summed CPU) |
| Fluent UI + React module import per file | every rendering test file (import is 31–63 % of a single-file run, about 500 ms) | ~500 ms per file | **No fix.** `isolate: false` (one module graph per worker): 3.57 → 3.43 s, within noise, and module-level mocks (`vi.mock`) and `eventsMock` state would leak between files. `deps.optimizer.client` (pre-bundling): no gain and 1 test failed. Both measured on the scratch suite (`v-noiso`, `v-opt` configs) | ≈0.1 s at best; rejected |
| per-test `QueryClient` and `createI18n` in `renderWithProviders` (src/app/testing.tsx:111-112) and in pages.test.tsx | 28 render calls | QueryClient 0.002 ms, createI18n 0.04 ms each (probe) | **No fix.** Sharing them saves < 2 ms in total and would carry cache and language state between tests | < 2 ms |
| cold first render per file (Fluent style injection) | first rendering test of each file (section 0.5) | 200–500 ms per file | **Already shared:** the injected styles stay in the document for the rest of the file. Later full-App renders cost 15–150 ms (AppFrame:56 and :80, 15 ms each). `beforeAll`-shared renders would couple test state for no gain | none |
| large mock dataset | pages.test.tsx:32 (600 mock pages fetched serially) | 1255 ms | **Fixed by the REWRITE in section 1:** 500 pages built once and seeded into the cache. No other test builds more than 200 rows (mail.test.tsx:92,107; `makeMailRow` is trivial) | ≈1.1 s |
| e2e app build | tests/e2e/build.mjs via `npm run e2e` | 2–8 min | **Already once per run** for all groups (package.json `e2e` = build.mjs once, then run.mjs) | none |
| e2e app launch per group | tests/e2e/run.mjs:24-31 (one `wdio run` per group) | one launch each | **No merge possible.** Each group needs its own app-data dir: flow needs an empty library to onboard, and seeded needs a zh-TW 2,000-message library (prepare.mjs:17-21). After the scroll deletion there are 2 launches, the minimum | scroll launch removed (section 3) |
| e2e seeding | tests/e2e/prepare.mjs:36-47 | 120k seed 2–5 min, 2k seed short | **Already once:** a library is reused while its `outlooker-data.json` marker exists (prepare.mjs:38). The 120k library goes away with the scroll group | 120k seed removed (section 3) |
