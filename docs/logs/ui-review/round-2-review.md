VERDICT: CONFIRMED

# Round 2 review: UI layout fix (variant 1 fidelity)

Scope: `git diff 919e8bc..07b92f8 -- src/` (commits 22e3373, 07b92f8), 19 files. HEAD = 07b92f8; `git status --porcelain src/` was empty.
Method: static. Each point was checked against `docs/design/variant-1/` (styles.css, app.js, index.html) and the Fluent sources in `node_modules/@fluentui`, not only against `scripts/tmp/round2-report.md`.

Gate (run by this reviewer, RC captured right after each command):
- `npm run typecheck`: RC=0 (`scripts/tmp/gate-r2-typecheck.txt`)
- `npm test`: RC=0, 16 files, 68 tests passed (`scripts/tmp/gate-r2-vitest.txt`)

Counts: blocker 0, should-fix 0, nit 3.

## Correctness against the round-2 list

All 26 items plus the 2 extras are fixed as described. Evidence for the ones that depend on Fluent internals or mockup values:
- R1-1: Card draws its outline on `::after` with longhand `borderStyle/borderWidth` and `outline` sets `borderColor` Stroke1 (`react-card/lib/components/Card/useCardStyles.styles.raw.js:38-48`, `:214-219`). The root `border`/`borderLeft` are gone (`src/components/dsl/DslBuilder.tsx:24-35`); `::after` is recolored to Stroke2 + 3 px brand left, which matches `.dsl-group` (`styles.css:280`). The nested-group change (brand edge on `::after` too) removes the same double border there. Deviation accepted. See R2-1 for a content-inset side effect.
- R1-2: `boxSizing: border-box` on `num`, `numDone` uses `shorthands.borderColor("transparent")` (`src/screens/onboarding/index.tsx:35-38`) — all circles are 28 px, as `styles.css:396-398` with `* { box-sizing: border-box }` (`:101`).
- R1-3: 20+20+16 + 2×8 padding + 1 border = 73 (`MailList.tsx:12`, `:64-65`).
- R1-4: `estimateSize` returns 73 only for `kind === "mail"` in compact mode; month entries use `s.item` 32 px; the footer is not a virtual entry (`MailList.tsx:99`, `:51`, `:125-135`). Comment updated (`:83-86`).
- R1-5: `git grep textOverflow 07b92f8 -- src` has one hit, `src/components/common/display.ts:2`. Single source confirmed. Where a site previously lacked `minWidth: 0` or `whiteSpace: nowrap` (AttachmentCard name, ReadingPane, ResultTable cell, SavedQueries name, SQL statusText), the parent already had nowrap or the addition is harmless.
- R1-6: `column(id: ColumnKey, …)` + `basis: Record<ColumnKey, string>` — a missing key is a compile error; `NUMERIC` is the one list for cells and headers (`src/screens/import/index.tsx:28-31`, `:213-217`).
- FS-27..FS-34, TL-10, TL-11, TL-26, TL-30: values match `styles.css:134`, `:208`, `:283-284`, `:204` (`.field` gap XS), `:251`, `:387-390`, `index.html:232`, `:264`, `:327-328`, `index.html:133`, `:413`, `:361-363`. TL-26: DialogSurface base `maxWidth: 600px` is overridden; the small-screen `maxWidth: 100vw` media rule still applies (`react-dialog/.../useDialogSurfaceStyles.styles.raw.js:26`, `:37-39`).
- TL-12: `section:last-of-type` is column 4. Children of the SQL root are `SavedQueries`, three `<section>`s, then the dialogs (`src/screens/advanced-sql/index.tsx:304-335`), so `:last-of-type` is correct regardless of what the dialog renders inline.
- TL-24: Import root children are `div.cmdbar` then `<section>` panes, so `section.pane:first-of-type` is the imported pane. The full-width empty pane is a `<div>` (not first-of-type because of the cmdbar), and `full` sets `borderLeft: none` explicitly, merged after `pane` (`src/screens/import/index.tsx:241`). Correct.
- TL-25: Fluent subtle has no rest icon color, so the icon inherits the danger color; hover/pressed icon colors are overridden with a higher-specificity `:enabled:hover` selector (`react-button/.../useButtonStyles.styles.raw.js:150-180`). Disabled state is excluded by `:enabled`. See R2-2.
- TL-31: `db.emptyTitle` has no remaining user (`git grep emptyTitle 07b92f8 -- src` is empty).
- TL-32 and parking "numeric headers": `TableHeaderCell` always renders the `button` slot (a `div` when not sortable) with `width: 100%; display: flex` (`react-table/.../useTableHeaderCell.js:29-41`, `useTableHeaderCellStyles.styles.raw.js:62-72`), so `justifyContent: flex-end` on the button slot is the right place. Applied in Import (sortable) and Settings (not sortable).
- Parking "AND/OR toggle": `segOn` uses the exact selector keys of Fluent's checked-primary styles (`:hover`, `:hover:active,:active:focus-visible`; `react-button/.../useToggleButtonStyles.styles.raw.js:73-87`) and is merged last, so it wins. Matches `.toggle button[aria-pressed="true"]` (`styles.css:290`).
- Extra (a): the mockup has no SQL empty state with this button (only search, `app.js:222`), so `size="small"` on this one button is a local fit fix with no mockup conflict. Fit is an estimate (column 3 ≈ 151 px at 1280, content ≈ 102 px; "Go to Import" small ≈ 88 px, "前往匯入" ≈ 64 px).
- Extra (b): `muted` → `note` in `src/screens/advanced-sql/index.tsx:49`, `:250`; no `s.muted` left in that file.

## Regression scan
- Attribute counts base → head over `src/`: `data-testid` 53/53, `aria-label` 32/32, `aria-selected` 6/6, `aria-current` 2/2, `role=` 12/12, `aria-hidden` 7/8 (+1 on the onboarding logo).
- New i18n keys used: `search:preview.attachments` and `common:app.name` exist in en and zh-TW.
- Unused makeStyles keys in the six most-changed files: none. Removed imports (`EmptyState`, `MailInboxRegular` in settings) have no remaining use.
- `src/app/global.test.tsx:86`: the assertion now looks for `db.emptyBody`, which renders only when `counts.messages === 0` (`src/screens/settings/index.tsx:231`). It still fails if the Settings empty-state branch breaks. Intent preserved.
- Event handlers, keyboard handler, and virtualization logic are unchanged apart from R1-4.

## Findings

### R2-1: nit. DSL group content sits 3 px closer to the brand stripe than the mockup
- File: `src/components/dsl/DslBuilder.tsx:24-35`
- Problem: the border now lives on Card's absolutely positioned `::after`, which overlays the root padding instead of adding to it. Top-level group content starts 8 px from the outer left edge (5 px past the 3 px stripe); the mockup `.dsl-group` (border-box, `border-left: 3px` + `padding: 8px`, `styles.css:280`) starts it at 11 px. Top/right/bottom are 1 px tighter too. Nested groups lose 3 px the same way.
- Fix: add the border widths to the padding, e.g. `topGroup: { padding: \`calc(${tokens.spacingHorizontalS} + 1px)\`, paddingLeft: \`calc(${tokens.spacingHorizontalS} + 3px)\` }` and `group: { paddingLeft: \`calc(${tokens.spacingHorizontalM} + 3px)\` }`.

### R2-2: nit. Remove buttons lose the danger color during a keyboard press
- File: `src/screens/import/index.tsx:87-93`
- Problem: Fluent subtle also styles `:active:focus-visible` (`useButtonStyles.styles.raw.js:168-180`); `danger` covers `:enabled:hover` and `:enabled:hover:active` only, so Space/Enter on a focused remove button flashes neutral Foreground2Pressed text and brand icon.
- Fix: extend the selector to `":enabled:hover,:enabled:hover:active,:enabled:active:focus-visible"`.

### R2-3: nit. Visible "Language" label is not tied to the group
- File: `src/app/AppFrame.tsx:171-172`
- Rule: Accessibility.
- Problem: the new visible label and the group's `aria-label` carry the same string, so screen readers announce it twice and the label text is not programmatically the group's name.
- Fix: give the span an `id` (`useId()`) and replace `aria-label` with `aria-labelledby={id}` on the group.

## Unverifiable (static only)
- Rendered appearance: Card `::after` recolor, flat AND/OR toggle colors, header button alignment, button fit in SQL column 3, danger colors. All reasoned from Fluent sources, not seen.
- Font-metric estimates for the "Go to Import" fit.

## Parking lot
- `src/components/mail/ReadingPane.tsx` `chips` gap is XS (4 px); mockup `.atts` gap is S (8 px, `styles.css:247`). Pre-existing, not a round-2 point.
- Search idle-state reading pane (user decision pending, carried from round 1).
