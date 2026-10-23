# Frontend test inventory (read-only, 2026-10-23)

Totals: **79 vitest `it` cases in 18 files** (73 timed at HEAD d32b4e0 + 6 from the in-flight, uncommitted column-resize work, marked `NEW*`) and **7 WDIO e2e `it` cases in 3 specs**. CANDIDATE count: **vitest 26 (23 at HEAD + 3 NEW*), e2e 1** (details below).
Line numbers are from the working tree at inventory time (mail.test.tsx and advanced-sql.test.tsx already shifted by the in-flight column-resize edit). Raw timings: `scripts/tmp/test-inventory/vitest.json`.

Categories: CORE, UI-COSMETIC, UI-INTERACTION, GLUE, DUPLICATE, SLOW (>1 s), E2E.
Cost note: durations below are per-test wall ms as reported by vitest (jsdom + Fluent UI render dominates; no test is slow in logic). A per-file environment/transform overhead sits on top (see runtime section).

## 1. Summary per file

| File | Tests | Mix | File time (ms) | CANDIDATE |
|---|---|---|---|---|
| src/api/api.test.ts | 4 | 4 CORE | 6 | 0 |
| src/api/dialog.test.ts | 2 | 2 CORE | 5 | 0 |
| src/app/AppFrame.test.tsx | 7 | 3 CORE, 2 UI-INTERACTION, 1 UI-COSMETIC, 1 SLOW | 3078 | 2 |
| src/app/boot.test.ts | 3 | 3 CORE | 10 | 0 |
| src/app/global.test.tsx | 4 | 1 CORE, 1 SLOW+UI-COSMETIC, 1 UI-COSMETIC/DUPLICATE, 1 GLUE | 1731 | 3 |
| src/components/common/common.test.tsx | 3 | 3 CORE | 371 | 0 |
| src/components/common/display.test.ts | 1 | 1 GLUE (perf-cache impl detail) | 47 | 1 |
| src/components/common/columnWidths.test.tsx (NEW*) | 4 | 3 CORE-ish (persistence), 1 UI-COSMETIC | not timed | 1 |
| src/components/dsl/dsl.test.tsx | 5 | 3 CORE, 2 UI-INTERACTION | 942 | 2 |
| src/components/mail/mail.test.tsx | 9 (1 NEW*) | 2 CORE, 3 UI-COSMETIC, 2 UI-INTERACTION, 1 CORE-security, 1 NEW* UI-COSMETIC | 1332 | 4 |
| src/components/mail/pages.test.tsx | 4 | 3 CORE, 1 SLOW | 2910 | 1 |
| src/components/sql-editor/sql-editor.test.tsx | 3 | 1 CORE, 1 UI-INTERACTION, 1 UI-COSMETIC | 645 | 1 |
| src/screens/advanced-sql/advanced-sql.test.tsx | 8 (1 NEW*) | 3 CORE, 3 UI-INTERACTION/COSMETIC, 1 SLOW, 1 NEW* UI-COSMETIC | 1945 | 4 |
| src/screens/import/import.test.tsx | 5 | 2 CORE, 2 UI-INTERACTION, 1 UI-COSMETIC | 1883 | 2 |
| src/screens/onboarding/onboarding.test.tsx | 3 | 2 CORE (flow), 1 UI-INTERACTION | 861 | 1 |
| src/screens/search/compose.test.ts | 3 | 3 CORE | 4 | 0 |
| src/screens/search/search.test.tsx | 7 | 4 CORE, 3 UI-COSMETIC | 2427 | 3 |
| src/screens/settings/settings.test.tsx | 4 | 1 CORE, 3 UI-INTERACTION | 2296 | 2 |
| tests/e2e/flow.e2e.ts | 5 | E2E | n/a | 0 |
| tests/e2e/scroll.e2e.ts | 1 | E2E, SLOW | n/a | 1 |
| tests/e2e/seeded.e2e.ts | 1 | E2E | n/a | 0 |

(Per-file CANDIDATE column reflects section 2 flags; sum = 26.)

## 2. Full list (vitest)

Format: file:line | name | asserts | category | ms | flag

### src/api/api.test.ts
- :15 wraps exactly the registered commands | `api` keys (snake-cased) == generated bindings/commands.json | CORE (contract drift guard) | 4 | KEEP
- :19 names exactly the emitted events | EVENTS values == bindings/events.json | CORE (contract drift guard) | 0 | KEEP
- :23 passes camelCase arguments and returns the payload | `importCancel(7)` → invoke("import_cancel",{pstId:7}) | CORE (API client contract) | 1 | KEEP
- :29 re-throws AppError rejections unchanged and wraps others | error normalisation | CORE | 1 | KEEP

### src/api/dialog.test.ts
- :9 normalizes the PST picker result | null/string/array → string[] | CORE (data transform) | 4 | KEEP
- :18 returns the chosen folder or null | pass-through | CORE (tiny) | 1 | KEEP

### src/app/AppFrame.test.tsx
- :42 library:changed with messages > 0 updates counts.messages | event → context counts | CORE | 501 | KEEP
- :49 Failed screen Retry calls appRetry | click `failed-retry` → api.appRetry | UI-INTERACTION | 82 | CANDIDATE: pure click→callback wiring, no logic
- :56 shows the blocking screen while the database starts and the frame when ready | phase→screen mapping | CORE (phase state machine) | 36 | KEEP
- :65 keeps the SQL text and the search text when switching screens | screen state survives nav | CORE (state retention regression) | 515 | KEEP
- :80 shows onboarding while setup is needed | phase needsSetup → onboarding-lang-en visible | DUPLICATE of :56 phase routing + onboarding.test.tsx:16 | 44 | CANDIDATE: same phase→screen mapping as :56
- :103 keeps the move dialog through stoppingDb, startingDb and ready and toasts done | data-dir move state machine across app status phases | CORE, SLOW | 1633 | KEEP (unique multi-phase state machine; slow only from `startMove()` full App render + Fluent dialog). Consider trimming, not deleting.
- :118 shows a failed move's error after the database is back in the old folder | failure branch of the same machine | CORE | 266 | KEEP (could MERGE with :103 to save one `startMove()`)

### src/app/boot.test.ts
- :26 boot subscribes to every event before calling app_status | ordering invariant | CORE | 4 | KEEP
- :46 does not call pst_list before the database is ready | gating | CORE | 1 | KEEP
- :53 routes events to the sink and the pstList cache | event routing + cache patch | CORE | 5 | KEEP

### src/app/global.test.tsx
- :52 locale files: zh-TW and en have equal key sets per namespace | key parity | CORE (cheap guard) | 4 | KEEP
- :61 screens rendered in cimode show only keys | renders 6 screens in cimode and walks the DOM for non-key text | UI-COSMETIC, SLOW | 1470 | CANDIDATE: slowest global test; renders every screen only to catch hard-coded strings; the key-parity test (:52) plus code review/lint cover most of it
- :80 each screen renders its empty state | Import/Settings/Onboarding empty text | UI-COSMETIC, DUPLICATE | 255 | CANDIDATE: empty state text already asserted in import.test.tsx:45, search.test.tsx:135, advanced-sql.test.tsx:118, AppFrame.test.tsx:80
- :91 no dark theme tokens in src | greps all sources for banned tokens | GLUE (static lint-in-test) | 1 | CANDIDATE: a style rule, not behaviour; fast but guards nothing that can regress at runtime. Keep only if the user wants the rule (move to grep/lint otherwise)

### src/components/common/common.test.tsx
- :26 formatAppError maps every AppError kind to a localized string | CORE (error→text, both languages) | 80 | KEEP
- :40 toAppError keeps AppError values and wraps anything else | CORE | 1 | KEEP
- :45 display helpers format numbers and bytes, also in cimode | CORE (formatters) | 290 | KEEP

### src/components/common/display.test.ts
- :7 reuses one Intl.DateTimeFormat per (locale, options) | spies on Intl constructor call count | GLUE (implementation-detail perf cache) | 47 | CANDIDATE: asserts a memoisation detail, not output behaviour

### src/components/common/columnWidths.test.tsx (NEW*, uncommitted, not timed)
- :12 writes a resized width to localStorage | persistence write | CORE-ish | – | KEEP
- :18 restores a stored width on remount and keeps unknown ids | persistence read + merge | CORE-ish | – | KEEP
- :31 falls back to defaults on malformed storage | robustness | CORE-ish | – | KEEP
- :40 spread onto DataGrid renders the stored width | renders Fluent DataGrid, checks inline width style | UI-COSMETIC | – | CANDIDATE: column-width styling; the hook tests above already cover the logic

### src/components/dsl/dsl.test.tsx
- :31 DSL builder output for the Part 03 §5.1 worked example deep-equals the spec JSON | draft→ConditionGroup | CORE | 3 | KEEP
- :45 drops incomplete conditions and empty groups, null for empty root | CORE | 1 | KEEP
- :54 measures depth and builds the folder tree | CORE | 1 | KEEP
- :67 Add group adds a group with one empty condition | click "Add group" → onChange(draft) | UI-INTERACTION | 590 | CANDIDATE: button→callback wiring; slow because full Fluent render
- :78 disables Add condition at the leaf budget and hides Add group at depth 4 | button disabled/hidden by limits | UI-INTERACTION (limit enforcement, light logic) | 348 | CANDIDATE: depth/budget logic already covered by depthOf/countLeaves in :31/:54; this asserts button visibility

### src/components/mail/mail.test.tsx
- :29 (NEW*) applies the stored sender width to header and rows, hides the header in compact mode | gridTemplateColumns style | UI-COSMETIC | – | CANDIDATE: column widths / layout styling
- :39 preview iframe has sandbox="" and no allow- token | security attrs of preview iframe | CORE (security) | 223 | KEEP
- :49 preview replaces srcdoc on the same iframe | iframe identity retained on rerender | UI-COSMETIC | 37 | CANDIDATE: React reconciliation detail
- :58 reading pane shows the header, attachments and the plain-text fallback | text/label rendering + disabled download | UI-COSMETIC | 549 | CANDIDATE: asserts rendered labels
- :70 reading pane renders HTML through MailPreview and shows pick state without a mail | UI-COSMETIC | 147 | CANDIDATE: renders text/label; partially DUPLICATE of :58 and :39
- :79 buildEntries inserts month headers and an undated group | list-model transform | CORE | 1 | KEEP
- :83 ArrowDown on the list selects the next mail | keyboard nav → onSelect | UI-INTERACTION | 85 | KEEP (cheap, real keyboard logic)
- :92 requests the next page only when the end is within 50 rows | infinite scroll threshold | CORE | 118 | KEEP
- :107 restores scrollTop after the list is hidden and shown again | Activity hide/show regression | CORE (regression) | 172 | KEEP

### src/components/mail/pages.test.tsx
- :22 nextPageParam stops at the last cursor and at the cap | CORE | 3 | KEEP
- :32 with 600 mock pages of 200 rows the list stops requesting after 100,000 rows and shows the cap row | 500 sequential page fetches via hook + cap row render | CORE, SLOW | 2729 | CANDIDATE: slowest test in suite (2.7 s of ~12 s); hook cap logic is unit-covered by :22 and cap row is covered by e2e scroll.e2e.ts; trim to a small page count (parametrise cap) or drop
- :65 a new seq evicts the superseded searchMail entry | cache eviction | CORE | 115 | KEEP
- :80 keeps the current search pages while no screen observes them | gcTime regression | CORE | 63 | KEEP

### src/components/sql-editor/sql-editor.test.tsx
- :8 codePointOffsetToUtf16 converts a 1-based code point offset | CORE | 2 | KEEP
- :17 renders the document, replaces it from props and runs on Mod-Enter | CodeMirror wrapper | UI-INTERACTION | 469 | KEEP (only real CodeMirror coverage; screen tests mock the editor)
- :29 sqlError position marks the character in the editor | error highlight class `.cm-sql-error` | UI-COSMETIC (but exercises offset logic) | 175 | KEEP (borderline; the offset conversion is already tested at :8)

### src/screens/advanced-sql/advanced-sql.test.tsx
- :44 sqlExecute is called with maxRows: 10000 | request contract | CORE | 662 | KEEP
- :52 truncated: true shows the cap MessageBar | renders message text | UI-COSMETIC | 164 | CANDIDATE: asserts a banner string
- :59 Advanced SQL shows MailList for a result with an id column and a table otherwise | mode→view switch (+ avatar-absence assertion) | CORE with UI-COSMETIC bits | 304 | KEEP (trim the `.fui-Avatar` and text assertions)
- :79 clicking a table row shows its key/value list in column 4 | row-detail rendering | UI-INTERACTION/COSMETIC | 81 | CANDIDATE: click→detail text rendering, no logic
- :93 (NEW*) applies a stored column width by column name to header and rows | style | UI-COSMETIC | – | CANDIDATE: column widths
- :104 Run is disabled while executing | run/cancel state machine incl. sqlCancel(executionId) | CORE | 65 | KEEP
- :118 shows sqlError text with its position and the empty states | error text + empty-state labels | UI-COSMETIC | 129 | CANDIDATE: text/label; empty state duplicated in global.test.tsx:80 and search.test.tsx:135
- :129 saves a new query by name and reports a duplicate name | save dialog duplicate-name retry | CORE | 540 | KEEP

### src/screens/import/import.test.tsx
- :23 importCache applies status changes, removal and progress without mutation | pure reducers | CORE | 3 | KEEP
- :34 removal dialog shows messageCount and calls pstRemove only after confirm | confirm gate before destructive call | CORE | 982 | KEEP
- :45 shows the empty state when no PST is imported | label | UI-COSMETIC, DUPLICATE (global.test.tsx:80 asserts the same text) | 136 | CANDIDATE: duplicate empty-state text
- :50 adds PST files through the picker and toasts rejected paths | enqueue + rejected toast | CORE (partly UI) | 594 | KEEP
- :62 queue card actions call cancel and resume | click→api.importCancel/Resume + progress text | UI-INTERACTION | 168 | CANDIDATE: button→callback wiring; "50 / 200 · 25%" is cosmetic

### src/screens/onboarding/onboarding.test.tsx
- :16 onboarding initializes with the chosen dir and language | wizard state machine → appInitialize | CORE | 624 | KEEP
- :27 shows the init error and stays on the location step | error branch | CORE | 85 | KEEP
- :36 step 4 enqueues the picked PST files and navigates to Import | picker → importEnqueue → go("import") | UI-INTERACTION (light flow) | 152 | CANDIDATE: click→callback wiring; e2e flow.e2e.ts:21 covers the same path for real

### src/screens/search/compose.test.ts
- :9 composeQuery builds simple, dsl and problem results | CORE | 3 | KEEP
- :30 termGroup is the Part 03 §5.1 simple-query rule | CORE | 0 | KEEP
- :43 refine conditions and the builder become nodes of one and-root | CORE | 1 | KEEP

### src/screens/search/search.test.tsx
- :48 searchCount renders 10,000+ / 9,999 | count label formatting with cap | UI-COSMETIC (cap flag logic is backend) | 829 | CANDIDATE: label text; also assert of searchCount call arg is the only logic
- :60 a stale search response (older sequence) is dropped | race handling | CORE | 154 | KEEP
- :75 library:changed refetches filterOptions and shows the new-mail bar without reloading the list | CORE | 704 | KEEP
- :94 leaving Search, a library change and coming back neither refetches nor misses the new-mail bar | CORE | 272 | KEEP
- :112 keeps the refine pane PST list visible while filterOptions refetches | cache keep-previous regression | CORE | 166 | KEEP
- :126 shows the short-query hint and selects the first row | hint element + auto-select first mail | UI-COSMETIC (+ auto-select) | 108 | CANDIDATE: hint display; auto-select is the only logic
- :135 shows the empty, idle, zero-result and problem states | empty/idle/zero/problem text, Clear button, 10-word limit | UI-COSMETIC (problem gate is in compose.test.ts) | 196 | CANDIDATE: label text; term limit duplicated by compose.test.ts:9

### src/screens/settings/settings.test.tsx
- :23 WebP quality outside 1-100 is not sent | validation before send | CORE | 1475 | KEEP (slow because each `typeQuality` loop awaits findByText; could be trimmed)
- :35 maintenance button runs the task | click→api.maintenanceRun("vacuum") | UI-INTERACTION | 82 | CANDIDATE: pure wiring
- :41 disables maintenance while a task is running | button disabled by status | UI-INTERACTION | 107 | CANDIDATE: attribute assertion
- :49 moves the data folder after confirmation | pick→confirm→api.dataDirMove | UI-INTERACTION | 632 | KEEP (confirmation gate on destructive move; also the entry for AppFrame:103)

Flag tally (vitest CANDIDATE): AppFrame 2, global 3, display 1, columnWidths 1*, dsl 2, mail 4 (1*), pages 1, advanced-sql 4 (1*), import 2, onboarding 1, search 3, settings 2 = **26** (* = NEW, provisional; 23 at HEAD).

## 3. e2e (WDIO + tauri-service, static analysis only, not run)

Config: `wdio.conf.ts` (one group per run via `OUTLOOKER_E2E_GROUP`, mocha, timeout 600 s, maxInstances 1; spawns the built debug binary). `npm run e2e` = `node tests/e2e/build.mjs && node tests/e2e/run.mjs` (builds `tauri build --debug --no-bundle --features e2e`, seeds data). Not part of `npm test`.
Timing info found in docs/logs/2026-10-23 (the e2e A5 round plan): total 15-35 min expected; debug build 2-8 min incremental; 120k seed 2-5 min; flow ≈ 2 min; seeded ≈ 1 min; scroll ≈ 5-12 min. These are estimates, no measured logs located.

| Spec | Case | Covers | Flag |
|---|---|---|---|
| flow.e2e.ts:10 | onboards into a new library | real onboarding wizard + DB cluster creation | KEEP |
| flow.e2e.ts:21 | imports outlook.pst to Completed | real PST import | KEEP |
| flow.e2e.ts:30 | finds the Welcome mail and previews it in the sandboxed frame | search + preview + sandbox attr (duplicates mail.test.tsx:39 attribute) | KEEP |
| flow.e2e.ts:42 | lists the mail in Advanced SQL mail-list mode | SQL mailList mode against real DB (duplicates advanced-sql.test.tsx:59 with mocks) | KEEP |
| flow.e2e.ts:55 | removes the PST | real removal | KEEP |
| scroll.e2e.ts:11 | loads pages on scroll and shows the cap row | 500 pages scroll to 100k cap in real webview (≈5-12 min) | CANDIDATE (E2E, SLOW): longest test in repo; overlaps pages.test.tsx:32 (hook cap) and virtual-scroll; keep only if the real-webview scroll risk matters |
| seeded.e2e.ts:7 | finds the seeded mail (Chinese search) and serves inline image via mailimg | CJK search + mailimg protocol, no unit equivalent | KEEP |

Note: docs/logs finding U18 mentions a `smoke.e2e.ts`; it no longer exists (only flow, scroll, seeded present).

## 4. Runtime

Command: `mise exec -- npx vitest run --reporter=json` (jsdom, config `vite.config.ts:8-12`, setup `src/test/setup.ts`, include `src/**/*.test.{ts,tsx}`). Package script: `npm test` = `vitest run`.
Wall-clock: **~12.2 s** of test execution across 17 files / 73 tests at HEAD (all pass; the 6 NEW* tests added mid-inventory were not in this timed run). Sum of per-test durations ≈ 18 s (files run in parallel workers). Full-run foreground wall (including vitest startup/transform) was not separately captured; the sum is dominated by Fluent UI/jsdom rendering, not business logic.

Slowest 15 tests (ms):
1. pages.test.tsx:32 cap after 600 mock pages — 2729
2. AppFrame.test.tsx:103 move dialog phases — 1633
3. settings.test.tsx:23 WebP quality — 1475
4. global.test.tsx:61 cimode all screens — 1470
5. import.test.tsx:34 removal dialog — 982
6. search.test.tsx:48 searchCount label — 829
7. search.test.tsx:75 library:changed refetch — 704
8. advanced-sql.test.tsx:44 maxRows — 662
9. settings.test.tsx:49 move folder — 632
10. onboarding.test.tsx:16 initialize — 624
11. import.test.tsx:50 add PST files — 594
12. dsl.test.tsx:67 Add group — 590
13. mail.test.tsx:58 reading pane — 549
14. advanced-sql.test.tsx:129 save query — 540
15. AppFrame.test.tsx:65 keep SQL/search text — 515

Slowest files (ms): AppFrame 3078, pages 2910, search 2427, settings 2296, advanced-sql 1945, import 1883, global 1731, mail 1332, dsl 942, onboarding 861, sql-editor 645, common 371, display 47, others <11.
Tests >1 s: 4 (pages:32, AppFrame:103, settings:23, global:61) ≈ 7.3 s of the ≈ 18 s sum. Removing the CANDIDATEs would save roughly 6-8 s of the sum (≈ 4 s of the dominated critical path), mostly global:61, pages:32, search:48, mail:58, dsl:67.

## 5. Notes / uncertain
- The working tree changed during inventory: another member's column-resize work (untracked `columnWidths.test.tsx`, edits to `mail.test.tsx`, `advanced-sql.test.tsx`, `src/test/setup.ts`) added 6 tests. They are flagged NEW*; their candidate status is provisional because the feature is being authored.
- Timing is a single run; durations vary ±20%.
- CORE vs UI-INTERACTION boundary is a judgment call; items marked "borderline" are KEEP by default.
- Prior audit docs already flagged test hygiene issues (docs/logs/2026-10-23/scan-app.md U18).
