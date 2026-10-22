VERDICT: CONFIRMED

# Round 1 review: UI layout fix (variant 1 fidelity)

Scope: `git diff 13f208e..919e8bc -- src/` (commits 85c1b33, 277145d, 196d3d7, 919e8bc). HEAD checked = 919e8bc, no uncommitted changes under `src/`.
Method: static only. Each fix point was checked against the mockup CSS (`docs/design/variant-1/styles.css`, `app.js`) and the Fluent sources in `node_modules`, not only against the reports.
`mise exec -- npm run typecheck` at HEAD printed no errors. `npm test` was not run by this reviewer.

Counts: blocker 0, should-fix 2, nit 4.

## Correctness against the fix list

All 50 round-1 points are fixed as described or with a stated reason for the deviation:
- Frame/search/onboarding: FS-1, 2, 5–15, 23–25.
- DSL/mail: FS-3, 4, 16–22, 26.
- SQL: TL-1–9, 13, 14.
- Import/settings: TL-15–23, 27–29, 33.

Deviations reviewed and accepted:
- FS-7: the optional `left:-4px` indicator was skipped. The Tab root clips its overflow, so the indicator would not show.
- FS-23: the selected tile thickens Card's `::after` border instead of adding a root border. Verified: `react-card/.../useCardStyles.styles.raw.js:38-48` draws the border on an absolutely positioned `::after`.
- TL-15 and TL-18: the date, status and log-time widths are sized for the real `formatDateTime` output, not for the reviewer's assumed format. The reason is stated in a code comment (`src/screens/import/index.tsx:86`).
- FS-13: both the idle state and the zero state show `ReadingPane mailId={null}`. The no-mail state shows an empty `<div />` (`src/screens/search/index.tsx:230`), which matches `app.js:221-224`.

Regression scan:
- Every `data-testid` and `aria-*` attribute on a removed line is still present at HEAD. Counts are equal between base and head (`aria-label` 27/27, `aria-current` 2/2, and every touched testid 1/1).
- `aria-selected` was added on Import rows (TL-23).
- Event handlers are unchanged. The keyboard handler in MailList is untouched.
- Virtualizer: `estimateSize` follows `COMPACT_ROW_HEIGHT` for compact rows, and the row height class matches (`MailList.tsx:62-65`, `:95`). See R1-4 for the remaining latent mismatch.
- Global nowrap rule (`src/app/app.css:3`): checked the narrow containers.
  - The SQL toolbar now clips instead of wrapping (`overflow: hidden`), and small buttons fit at 1280.
  - Settings maintenance buttons sit in the full-width index card, and `s.row` wraps buttons as whole items.
  - The saved-query buttons ellipsize through `itemText`.
  - No clipped label was found. See the parking lot for one borderline case.

## Findings

### R1-1: should-fix. Top-level DSL group cards draw a double border
- File: `src/components/dsl/DslBuilder.tsx:28-32` (`topGroup`), used at `:171`
- Rule: correctness against FS-19, and a regression from adding a root border to a Fluent Card.
- Problem: `topGroup` adds `border: 1px solid colorNeutralStroke2` to the Card root. `Card appearance="outline"` also draws a 1 px `colorNeutralStroke1` border on `::after`, at `inset: 0` inside the root border (`react-card/lib/components/Card/useCardStyles.styles.raw.js:38-48`, `:214-219`). The result is two nested borders on the top, right and bottom, plus a grey 1 px line inside the 3 px brand left border. The mockup `.dsl-group` (`styles.css:280`) has one 1 px Stroke2 border. The onboarding implementer avoided this same trap in FS-23.
- Fix: in `topGroup`, drop `border` and `borderLeft` from the root and recolor the pseudo-element instead: `"::after": { borderColor: tokens.colorNeutralStroke2, borderLeftWidth: "3px", borderLeftColor: tokens.colorBrandStroke1 }`. Or keep the root borders and add `"::after": { display: "none" }`.
- Severity: Medium. Visible on every top-level group whenever the DSL panel is open. Cosmetic, no functional loss.

### R1-2: should-fix. Onboarding step circles are 28, 30 and 32 px, so they do not line up
- File: `src/screens/onboarding/index.tsx:34-39` (`num`, `numDone`, `numCurrent`)
- Rule: correctness against FS-24 (the contract says "no misaligned elements").
- Problem: the mockup sets `* { box-sizing: border-box }` (`styles.css:101`), so every `.n` is 28 px (`styles.css:396-398`). Done steps keep a transparent 1 px border. The app has no box-sizing on `num`, so it uses content-box:
  - done steps are 28 px (`border: "none"`);
  - pending steps are 30 px (1 px border);
  - the current step is 32 px (2 px border).
  The circles sit in a 28 px grid column, so they overflow it by 0, 2 and 4 px, and their centers drift right. Row heights also differ.
- Fix: add `boxSizing: "border-box"` to `num`. In `numDone`, replace `border: "none"` with `borderColor: "transparent"`.
- Severity: Medium. Visible misalignment on the first-run screen.

### R1-3: nit. Compact mail row content is 1 px taller than the row
- File: `src/components/mail/MailList.tsx:62-65`
- Problem: the row is `boxSizing: border-box` at 72 px. That height holds the 8 + 8 px padding and the 1 px bottom border (from `s.row`), which leaves 55 px of content. `gridTemplateRows: "20px 20px 16px"` needs 56 px, so the snippet line overlaps the bottom border by 1 px.
- Fix: set `COMPACT_ROW_HEIGHT = 73`, or use `gridTemplateRows: "20px 20px 15px"`.

### R1-4: nit. Compact mode estimates 72 px for month-header and footer entries; stale doc comment
- File: `src/components/mail/MailList.tsx:95`, `:81`
- Problem: `estimateSize` returns 72 for every entry when `compact` is set. Month headers stay 32 px (`s.item`), so `compact` together with month headers would leave 40 px gaps. The only caller passes `monthHeaders={false}` (`src/screens/advanced-sql/index.tsx:247`), so the bug is latent. The comment at `:81` still says "one line per mail at 32 px".
- Fix: `estimateSize: (i) => (p.compact && entries[i]?.kind === "mail" ? COMPACT_ROW_HEIGHT : ROW_HEIGHT)`. Update the comment to mention the compact variant.

### R1-5: nit. The ellipsis style block is copied into 8 files
- Files:
  - `src/screens/advanced-sql/SavedQueries.tsx:12`, `:18`
  - `src/screens/advanced-sql/index.tsx:43`
  - `src/screens/search/RefinePane.tsx:24`
  - `src/screens/import/index.tsx:51`, `:69`
  - `src/components/mail/ReadingPane.tsx:22`
  - and others
- Problem: `{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", minWidth: 0 }` appeared 5 times at base and 13 times at head (`git grep -c 'textOverflow: "ellipsis"'`). The policy is "one generic module for similar needs".
- Fix: export `const ellipsis = { … } as const` from `src/components/common/display.ts`, and spread it (`title: { ...ellipsis }`) in each makeStyles block.

### R1-6: nit. Import column classes are looked up by a computed string key
- File: `src/screens/import/index.tsx:202-203`
- Problem: `s[\`col_${String(id)}\` as keyof typeof s]` hides a missing key behind a cast. A new or renamed column silently gets no flex basis and collapses back to `flex: 1 1 0`. The size/messages/new list is also hard-coded a second time.
- Fix: use a typed map, `const COL: Record<ColumnKey, string> = { name: s.col_name, … }` with `ColumnKey` as the union of column ids, or set the class on each column definition. TypeScript then rejects a missing entry.

## Unverifiable (static only)
- Rendered appearance in general: rail label fit (FS-1), language segment colors and hover precedence (FS-2), the Card `::after` override (FS-23), and the R1-1 double border are all reasoned from Fluent sources, not seen.
- Column widths that come from font-metric estimates: import date 192 px, status 128 px, log time 168 px, SpinButton 88 px.
- Whether the right-aligned number header cells (`num` with `justifyContent: flex-end`) right-align the sortable header button (implementer flag).
- `npm test` result at HEAD (not run by this reviewer).

## Parking lot (not tied to a round-1 point or a regression)
- SQL column 3 at 1280 px: the EmptyState "Go to Import" primary button is about 108 px wide with nowrap, and the content box inside the 24 px EmptyState padding is about 103 px. The button now overflows a few px into the padding instead of wrapping. It is not clipped, but a render check is needed.
- Search idle state (no query yet): the list pane shows the idle EmptyState and the reading pane shows the "pick a mail" EmptyState side by side. The mockup has no idle state. Decide whether the right column should stay empty for idle, as it does for no-mail.
- `src/screens/advanced-sql/index.tsx:45`: the `muted` class now also sets `display: block` and padding. It has a single user (the missing-ids caption), but the name no longer describes it. A rename, such as `note`, would help.
