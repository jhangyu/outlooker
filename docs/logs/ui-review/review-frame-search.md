# UI review — App frame, Search, Onboarding vs variant 1

Scope: `src/app/AppFrame.tsx`, `src/app/app.css`, `src/screens/search/*`, `src/components/mail/*`, `src/components/dsl/*`, `src/components/common/EmptyState.tsx`, `src/screens/onboarding/index.tsx`.
Mockup: `docs/design/variant-1/index.html`, `styles.css`, `app.js`.
Method: static comparison only. The app was not rendered: `App.tsx` boots through Tauri commands, so a plain vite page reaches the Failed screen.
Fluent v9 facts below were read from `node_modules/@fluentui/*/lib/**/*.styles.raw.js` (react-tabs 9.12.6, react-combobox 9.17.8, react-button 9.11.2, react-badge 9.5.7, react-tree 9.16.9).

Excluded on purpose:
- Plan Global Constraints out-of-scope controls (`docs/superpowers/plans/2026-10-21-outlooker-plan.md:38`). Examples: search-term highlighting, prototype data-state toggle.
- The 32 px one-line mail row. The plan decision "Fixed-height virtual rows" (`docs/superpowers/plans/2026-10-21-outlooker-plan-frontend.md:13`) replaces the mockup's 3-line row.
- Controls the app does not have: sort select in the results pane head, Copy button and folder badge in `.preview-tools`. Adding them would be new features.

Severity counts: blocker 6, should-fix 20, nit 8 (34 total).

---

## Blockers

### FS-1 — blocker — Nav rail labels clip or wrap; icon and label sit side by side
- App: `src/app/AppFrame.tsx:42` (`railTab`), `:163`
- Mockup: `styles.css:145-153` (`.rail button`: column flex, 24 px icon, 10 px label, `padding: 6px 0`)
- Symptom (both languages): Fluent `Tab` root is `display: grid; grid-auto-flow: column` (react-tabs `useTabStyles.styles.raw.js:23-30`). `flexDirection: "column"` on a grid has no effect. The icon (col 1) and the label (col 2, `iconBefore` → `gridColumnStart: 2`, `:567-570`) are in one row. The content slot sets `typographyStyles.body1` (14 px, `:548-552`), so `fontSize: Base100` on the root is overridden. The usable width is 68 − 8 (rail padding) − 20 (Tab padding 10+10) = 40 px for a 20 px icon + 6 px gap + 14 px label. The label overflows and is clipped (`overflow: hidden`). Examples: "Settings", "設定", "進階 SQL".
- Fix: in `railTab`, replace `flexDirection/justifyContent` with `gridAutoFlow: "row", justifyItems: "center", justifyContent: "center", rowGap: "2px", padding: "6px 0", height: "auto"`. Pass slot classes:
  `<Tab icon={{ children: icon, className: s.railIcon }} content={{ className: s.railLabel }} …>`
  - `railIcon: { gridColumnStart: 1, gridRowStart: 1, fontSize: "24px", width: "24px", height: "24px" }`
  - `railLabel: { gridColumnStart: 1, gridRowStart: 2, fontSize: tokens.fontSizeBase100, lineHeight: tokens.lineHeightBase100, padding: 0, whiteSpace: "nowrap" }`

  The reserved-space span (`contentReservedSpace`) also takes `state.content.className` (`:678`), so it follows automatically. Keep the `data-testid`.

### FS-2 — blocker — Title bar language buttons: hover text is blue on blue; selected state is invisible
- App: `src/app/AppFrame.tsx:37`, `:140-153`
- Mockup: `styles.css:135-138` (`.seg`: 1 px white-70% border, selected = white background + brand text, semibold)
- Symptom (both languages): the transparent `ToggleButton` hover rule sets `color: colorNeutralForeground2BrandHover` (#115ea3) on the brand bar (react-button `useToggleButtonStyles.styles.raw.js:105-113`; base Button `:hover` color). `.x:hover` is more specific than the app's `langButton` class, so hovered text becomes almost invisible. Checked and unchecked buttons both have a transparent background, so the current language is not visible.
- Fix: wrap the group in `seg: { display: "inline-flex", border: "1px solid rgba(255,255,255,.7)", borderRadius: tokens.borderRadiusMedium, overflow: "hidden" }`.
  - Each button: `color: "#fff"`, `minWidth: "auto"`, `padding: "2px 12px"`, `fontSize: tokens.fontSizeBase200`, `borderRadius: 0`, plus `":hover": { color: "#fff", backgroundColor: tokens.colorBrandBackgroundHover }`.
  - Checked class: `backgroundColor: tokens.colorNeutralBackground1, color: tokens.colorBrandForeground2, fontWeight: semibold`, and the same values under `:hover`.

  Plain `<button aria-pressed>` elements styled like the mockup also work and are simpler.

### FS-3 — blocker — DSL field dropdown overflows its 150 px column into the operator column
- App: `src/components/dsl/DslBuilder.tsx:132` (field `Dropdown`, no className), grid `:31`
- Mockup: `styles.css:283` (`.cond` 150px 92px 1fr 28px), `app.js:139`
- Symptom (both languages): Fluent `Dropdown` root has `minWidth: '250px'` (react-combobox `useDropdownStyles.styles.raw.js:20`). In a 150 px track it spills 100 px over the operator text ("contains" / "包含") and the value input. The value dropdowns are not affected because `s.grow` sets `minWidth: 0`.
- Fix: add `className={s.field}` with `field: { minWidth: 0, width: "100%" }` to the field `Dropdown`.

### FS-4 — blocker — Attachment card subtitle overflows the 260 px card
- App: `src/components/mail/AttachmentCard.tsx:30`
- Mockup: `app.js:210` (shows `mime.split('/')[1].slice(0, 24)`), `styles.css:248-250`
- Symptom: `${size} · ${mimeType}`, for example `application/vnd.openxmlformats-officedocument.wordprocessingml.document`, has no break point. It overflows the fixed-width card and is clipped or pushes the download button, for every Office attachment.
- Fix: give the `Caption1` the same class as the name: `name: { overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }`, plus `title={a.mimeType}`. Optionally show only the subtype, as the mockup does.

### FS-5 — blocker — Refine pane tree labels wrap instead of truncating
- App: `src/screens/search/RefinePane.tsx:26-29`, `:53-63`
- Mockup: `styles.css:265` (`.tree-item > span { overflow:hidden; text-overflow:ellipsis; white-space:nowrap }`)
- Symptom (both languages): the `TreeItemLayout` main slot only sets padding (react-tree `useTreeItemLayoutStyles.styles.raw.js:111-113`). Long PST display names and folder names wrap onto two or three lines. The usable width is 260 − 24 (pane padding) − 24·level indent − 20 icon − count badge. Rows get uneven heights, and the count badge drops out of line.
- Fix: on every `TreeItemLayout`, pass `main={{ className: s.label }}` with `label: { overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", minWidth: 0 }`. Add `title={name}` on the label.

### FS-6 — blocker — Fluent Buttons and Badges do not set `white-space: nowrap` (app-wide)
- App: `src/app/app.css:1` (only global rule)
- Mockup: `styles.css:170` (`.btn { white-space: nowrap }`), `:185` (`.badge { white-space: nowrap }`)
- Symptom: Fluent Button root has `overflow: hidden` and no `white-space` rule (react-button `useButtonStyles.styles.raw.js:20-47`). Fluent Badge has a fixed `height: 20px` and no `white-space` rule (react-badge `useBadgeStyles.styles.raw.js:12-21`). When a flex row shrinks, button labels wrap to two lines and badge text wraps outside its 20 px pill. Search examples: the "條件 / Conditions" toggle (`search/index.tsx:190`), DSL "新增群組 / Add group", "全部清除 / Clear all", and the "Found in" occurrence badges (`ReadingPane.tsx:72`). Settings, Import and SQL have the same root cause.
- Fix: one global rule in `src/app/app.css`: `.fui-Button, .fui-ToggleButton, .fui-Badge { white-space: nowrap; }`. Also give `flexShrink: 0` to the search-bar toggle (`search/index.tsx:190`). Lead: dedupe with the tools reviewer if they also report this.

---

## Should-fix

### FS-7 — Rail selected item style differs from mockup
- App: `src/app/AppFrame.tsx:42`
- Mockup: `styles.css:151-152` (selected = Bg1 white + shadow2 + brand text semibold; 3 px brand bar at `left:-4px; top/bottom:10px`)
- Symptom: Fluent transparent selected Tab has no background. Only the 3 px indicator at `left: 0` shows.
- Fix: add a `railTabSelected` class when `screen === id`: `backgroundColor: tokens.colorNeutralBackground1, boxShadow: tokens.shadow2, color: tokens.colorBrandForeground1`. Optional: move the indicator to `left: -4px`.

### FS-8 — Search bar padding and SearchBox height/fill differ
- App: `src/screens/search/index.tsx:27` (padding 8/16), `:30`, `:188`
- Mockup: `styles.css:271-273` (`.searchbar` padding 12/16; `.searchbox` 36 px high, Bg3 fill, Fg3 icon)
- Fix: `bar` padding `${tokens.spacingVerticalM} ${tokens.spacingHorizontalL}`. Set `appearance="filled-darker"` on `SearchBox`, and add `height: "36px"` to `s.box`.

### FS-9 — DSL summary caption can wrap (zh-TW splits between any CJK characters)
- App: `src/screens/search/index.tsx:31`, `:195`
- Mockup: `styles.css:276`
- Symptom: "已套用 3 個條件" is a shrinkable flex item. When the bar narrows (the toggle is wide, or the window is narrow), it breaks into two lines.
- Fix: `summary: { color: Fg3, whiteSpace: "nowrap", flexShrink: 0 }`.

### FS-10 — Hint MessageBars touch the search bar or DSL panel (no top gap)
- App: `src/screens/search/index.tsx:36`
- Mockup: `styles.css:291` (`.hint-row { padding: 8px 16px 0; background: Bg3 }`)
- Fix: `hints: { …, padding: \`${tokens.spacingVerticalS} ${tokens.spacingHorizontalL} 0\` }`. Render the wrapper only when at least one bar is shown, so the 8 px does not add empty space.

### FS-11 — Results area is flat and edge to edge; mockup is an inset raised card
- App: `src/screens/search/index.tsx:37`
- Mockup: `styles.css:293` (`.results { margin: 8px 8px 0; border-radius: 6px 6px 0 0; overflow: hidden; box-shadow: shadow2 }`) on the Bg3 `main`
- Fix: `results: { …, margin: \`${tokens.spacingVerticalS} ${tokens.spacingHorizontalS} 0\`, borderRadius: \`${tokens.borderRadiusLarge} ${tokens.borderRadiusLarge} 0 0\`, overflow: "hidden", boxShadow: tokens.shadow2, backgroundColor: tokens.colorNeutralBackground1 }`.

### FS-12 — Results pane head uses a 16 px title; mockup uses 14 px semibold
- App: `src/screens/search/index.tsx:39-42`, `:169`
- Mockup: `styles.css:162-163` (`.pane-head` padding 8/12, h3 Base300 semibold)
- Fix: replace `Subtitle2` with `Body1Strong`. Set `paneHead` padding to `${tokens.spacingVerticalS} ${tokens.spacingHorizontalM}`.

### FS-13 — Empty, idle and zero-result states replace both panes; mockup keeps the two-pane grid
- App: `src/screens/search/index.tsx:139-181`
- Mockup: `app.js:220-231` (the empty or zero state is rendered inside `#resList`; the Results pane head and the preview pane stay)
- Symptom: with no results, the list pane, its head and the reading pane disappear, and one centered block fills the full width. The layout jumps when results arrive.
- Fix: always render `s.results` with the list pane and its head. Put `EmptyState`, `Spinner` or the error `MessageBar` in the list pane `s.fill` instead of `MailList`. The right column shows `ReadingPane mailId={null}` for the zero state, and an empty pane for the no-mail state (`app.js:223`). Check that `search.test.tsx` selectors still match.

### FS-14 — Refine pane chrome: grey background, inset padding, no pane head
- App: `src/screens/search/RefinePane.tsx:12-15`, `:50`
- Mockup: `index.html:76-78`, `styles.css:160-166`, `:260-262` (Bg1 white pane; 44 px `.pane-head` with bottom border; list `padding: 8px 0`; full-bleed rows `padding: 5px 12px`; section titles `padding: 8px 12px 4px`, Base200 semibold Fg3)
- Symptom: the pane is Bg2 with 12 px padding on all sides. The selected and hover row backgrounds stop 12 px short of the pane edges. The title is a 16 px `Subtitle2` without a header bar, so it does not line up with the Results pane head row.
- Fix: root `backgroundColor: Bg1`, `padding: 0`, `gap: 0`. Add a head div with the same styles as `paneHead` in FS-12, containing `Body1Strong {t("refine")}`. Wrap the trees in a scroll div with `padding: 8px 0`. Section captions get `padding: 8px 12px 4px` instead of `marginTop`. Checkbox rows get `paddingLeft: 12px`.

### FS-15 — Refine pane has no "PST files" section title before the sources tree
- App: `src/screens/search/RefinePane.tsx:51`
- Mockup: `app.js:113` (`.facet-title` "PST 檔 / sources" above "All mail")
- Fix: render `<Caption1Strong className={s.section}>{t("sources")}</Caption1Strong>` before the first `Tree`. The key already exists.

### FS-16 — DSL AND/OR toggle renders as two separate 64 px buttons, not a joined segment
- App: `src/components/dsl/DslBuilder.tsx:43-51`
- Mockup: `styles.css:288-290` (`.toggle`: one 1 px Stroke1 border, radius 4, `overflow: hidden`, 24 px high; pressed = brand fill + white text)
- Symptom: small ToggleButtons have `minWidth: 64px` (react-button `useButtonStyles.styles.raw.js:245`), separate rounded borders, and no gap, so the two borders touch.
- Fix: `seg: { display: "inline-flex", border: \`1px solid ${tokens.colorNeutralStroke1}\`, borderRadius: tokens.borderRadiusMedium, overflow: "hidden" }` on the wrapper. On each button: `minWidth: "auto", border: "none", borderRadius: 0, padding: "0 8px"`. For the checked state use `appearance="primary"` when `checked`, or a class `backgroundColor: colorBrandBackground, color: "#fff"`.

### FS-17 — Top-level join label missing between DSL groups
- App: `src/components/dsl/DslBuilder.tsx:203-209`
- Mockup: `app.js:154` (`<span class="badge brand dsl-join">AND</span>` between groups), `styles.css:282`
- Symptom: the join appears inside groups (`:163`) but not between root groups. The group cards stack with no connector.
- Fix: in the root map, render `{i > 0 ? <Badge appearance="tint" color="brand" className={s.joinBadge}>{t(\`dsl.${value.op}\`)}</Badge> : null}` before each child, inside a `Fragment` with `key={c.id}`. Use `joinBadge: { alignSelf: "center" }`.

### FS-18 — DSL root bar: "Add group" sits left of the spacer; mockup puts it on the right
- App: `src/components/dsl/DslBuilder.tsx:196-201`
- Mockup: `index.html:91-97` (label, toggle, spacer, Add group, Clear all)
- Fix: move `<span className={s.spacer} />` before the Add group `Button`.

### FS-19 — DSL groups always stack vertically; mockup wraps them side by side
- App: `src/components/dsl/DslBuilder.tsx:22`, `:24-27`, `:203-209`
- Mockup: `styles.css:279-280` (`.dsl-groups { display:flex; flex-wrap:wrap; gap:8px; align-items:stretch }`, `.dsl-group { min-width:520px; flex:1; padding:8px; background:Bg1; border:1px Stroke2; border-left:3px brand; shadow2 }`)
- Symptom: at the 1920 px target, the DSL area is about 1560 px wide. The mockup shows two groups per row; the app shows one full-width card per group, so the panel quickly reaches its 40 % max height and scrolls.
- Fix: wrap the root `value.children` map in `<div className={s.groups}>` with the `.dsl-groups` rules. Pass a `top` flag to depth-2 `GroupCard`s and add `topGroup: { minWidth: "520px", flex: 1, boxShadow: tokens.shadow2, backgroundColor: tokens.colorNeutralBackground1, border: \`1px solid ${tokens.colorNeutralStroke2}\`, borderLeft: \`3px solid ${tokens.colorBrandStroke1}\`, padding: tokens.spacingHorizontalS }`. Nested groups stay full width.

### FS-20 — DSL folder condition: "Include subfolders" label can wrap; recipient To/Cc/Bcc toggles are 64 px each
- App: `src/components/dsl/DslBuilder.tsx:75-79`, `:101-102`
- Mockup: `styles.css:285-286`
- Symptom: in a narrow value column, "包含子資料夾" / "Include subfolders" wraps under the checkbox. The three 64 px minimum-width toggles (192 px total) squeeze the recipient input.
- Fix: Checkbox `label={{ children: …, style: { whiteSpace: "nowrap" } }}`, or a class with `whiteSpace: nowrap; flexShrink: 0`. Recipient kind toggles get `minWidth: "auto"` (same class as FS-16).

### FS-21 — Reading pane header spacing smaller than mockup
- App: `src/components/mail/ReadingPane.tsx:15-18`
- Mockup: `styles.css:239` (`.read { padding: 20px 24px; gap: 16px }`)
- Fix: `head` padding `${tokens.spacingVerticalXL} ${tokens.spacingHorizontalXXL}`, gap `tokens.spacingVerticalL`.

### FS-22 — Reading pane sent date can wrap next to a long sender
- App: `src/components/mail/ReadingPane.tsx:19`, `:59-61`
- Mockup: `styles.css:241`, `:245` (`.meta` grid `40px 1fr auto`; date in the auto column, right aligned)
- Symptom: in the flex row, the Persona (long name + address) and the date shrink together. An en date like "Jun 15, 2024, 3:42 PM" breaks onto two or three lines.
- Fix: date `Caption1` class `{ whiteSpace: "nowrap", flexShrink: 0, color: tokens.colorNeutralForeground3, alignSelf: "flex-start" }`. Persona `className` `{ minWidth: 0 }`. Add `primaryText={{ className: ellipsis }}` and `secondaryText={{ className: ellipsis }}` with nowrap ellipsis.

### FS-23 — Onboarding typography and choice tiles differ from mockup
- App: `src/screens/onboarding/index.tsx:37-39`, `:105-117`, `:192`, `:231`
- Mockup: `styles.css:392` (steps h2 24/32), `:386` (info h3 16), `:401` (lead 16/22 Fg2, max 760), `:402-404` (grid `repeat(3, minmax(0,1fr))`, max 1200; tile title 14 semibold; selected = 2 px brand border + Brand bg2), `app.js:445`
- Symptom: the left heading `Title2` is 28 px. The right heading `Subtitle1` is 20 px. The lead is `Body1` 14 px in Fg3. Tile titles are `Subtitle1` 20 px. The selected tile uses `filled-alternative` (grey), which is not the brand selection look.
- Fix:
  - `:192` → `Title3 as="h1"`
  - `:231` → `Subtitle2 as="h2"`
  - lead → `Body2` with `color: Fg2, maxWidth: "760px"`
  - `:113` → `Body1Strong`
  - `:114` hint gets `color: Fg3`
  - Card: keep `appearance="outline"`. Add `selected && s.choiceOn` with `choiceOn: { border: \`2px solid ${tokens.colorBrandStroke1}\`, backgroundColor: tokens.colorBrandBackground2 }`.
  - `choices` → `gridTemplateColumns: "repeat(3, minmax(0, 1fr))", maxWidth: "1200px"`

### FS-24 — Onboarding stepper: current and done markers are swapped compared with the mockup; done shows a circle inside a circle
- App: `src/screens/onboarding/index.tsx:28-35`, `:194-200`
- Mockup: `styles.css:393-398` (li `padding: 8px 0`, no list gap; done = brand fill + white check; current = 2 px brand border + brand numeral; others = 1 px Stroke1 on Bg1), `app.js:444`
- Symptom: the current step is brand-filled. Done steps render `CheckmarkCircleRegular` (an icon with its own circle) inside the 28 px outlined circle.
- Fix: `numDone: { backgroundColor: colorBrandBackground, color: "#fff", border: "none" }` with `<CheckmarkRegular />`. `numCurrent: { border: \`2px solid ${tokens.colorBrandStroke1}\`, color: tokens.colorBrandForeground1 }`. Base `num` adds `backgroundColor: Bg1`. `steps` gap 0; `stepItem` padding `8px 0`.

### FS-25 — Onboarding footer has no divider; body padding differs
- App: `src/screens/onboarding/index.tsx:36`, `:44`
- Mockup: `styles.css:399` (`.ob-body` padding 32px 48px, gap 20), `:405` (`.ob-foot` `border-top: 1px Stroke2; padding-top: 16px`)
- Fix: `body` padding `"32px 48px"`, gap `tokens.spacingVerticalXL`. `foot` adds `borderTop: \`1px solid ${tokens.colorNeutralStroke2}\`, paddingTop: tokens.spacingVerticalL`. Drop `maxWidth: "740px"`; the mockup footer spans the column.

### FS-26 — Mail list month header style
- App: `src/components/mail/MailList.tsx:47-50`, `:116`
- Mockup: `styles.css:236` (`.list-group`: Bg2, Fg2, Base200 semibold, `border-bottom: 1px Stroke2`, sticky)
- Symptom: the text is Fg3 with no bottom border, so the header blends into the rows. The fixed 32 px height is kept by plan decision.
- Fix: `month: { …, color: tokens.colorNeutralForeground2, borderBottom: \`1px solid ${tokens.colorNeutralStroke2}\` }`.

---

## Nits

### FS-27 — Title bar missing the "語言 / Language" label before the language group
- App: `src/app/AppFrame.tsx:140`; Mockup: `index.html:56`, `styles.css:134`
- Fix: `<span className={s.label}>{t("language.label")}</span>` with `fontSize: Base200, opacity: .95`. The key exists.

### FS-28 — EmptyState illustration has no bottom spacing
- App: `src/components/common/EmptyState.tsx:24-33`; Mockup: `styles.css:208` (`margin-bottom: 8px`)
- Fix: `art: { …, marginBottom: tokens.spacingVerticalS }`.

### FS-29 — DSL operator text left-aligned; mockup centers it in Fg3
- App: `src/components/dsl/DslBuilder.tsx:138`; Mockup: `styles.css:284`
- Fix: class `{ textAlign: "center", color: tokens.colorNeutralForeground3 }`. Condition grid `gap` is 4 px in the mockup (`:283`) vs `spacingHorizontalS` 8 px in the app (`:31`): use `spacingHorizontalXS`.

### FS-30 — Reading pane: no "Attachments" caption above the cards
- App: `src/components/mail/ReadingPane.tsx:77-83`; Mockup: `app.js:209`
- Fix: render `<Caption1 className={fg3}>{t("preview.attachments")}</Caption1>` above the chips row. The key exists.

### FS-31 — Plain-text body padding and width
- App: `src/components/mail/ReadingPane.tsx:24`; Mockup: `styles.css:251` (24 px side padding through `.read`, `max-width: 900px`, line-height 22 px)
- Fix: `text: { …, padding: \`${tokens.spacingVerticalL} ${tokens.spacingHorizontalXXL}\`, maxWidth: "900px", lineHeight: "22px" }`.

### FS-32 — Onboarding feature list sizes and colors
- App: `src/screens/onboarding/index.tsx:46-50`, `:185`; Mockup: `styles.css:387-390` (36 px icon tile, gap 4/12, description Base200 Fg3)
- Fix: `feat` columns `36px 1fr`, gap `4px 12px`. `featIcon` 36×36. Description `Caption1` class `color: Fg3`. Same Fg3 for the left note at `:203` (mockup `index.html:235` `caption muted`).

### FS-33 — Onboarding left column has no logo + "Outlooker" row
- App: `src/screens/onboarding/index.tsx:191-192`; Mockup: `index.html:232`
- Fix: add a row before the heading: a brand-filled 28 px logo "O" + `<strong>{t("common:app.name")}</strong>`.

### FS-34 — Refine pane selected row color
- App: `src/screens/search/RefinePane.tsx:17`; Mockup: `styles.css:264` (Bg1Selected #ebebeb + semibold)
- Fix: `picked: { backgroundColor: tokens.colorNeutralBackground1Selected, fontWeight: semibold }`.

---

## Uncertain
- Onboarding is rendered full screen, without the title bar and rail (`AppFrame.tsx:195`). In the mockup it is a screen inside the frame with a rail item (`index.html:68`, `:230`). The plan does not say which is intended. I did not list it as a fix. Lead or user to decide.
- No rendered screenshot. All findings come from static reading plus Fluent source. FS-5, FS-9 and FS-22 depend on string and window length; FS-1, FS-2, FS-3, FS-4 and FS-6 are deterministic.
- FS-6 is a global rule. The tools reviewer may report the same root cause on their screens.

---

## Proposed file ownership groups

| Group | Files | Fix points |
|---|---|---|
| A — Frame | `src/app/AppFrame.tsx`, `src/app/app.css` | FS-1, FS-2, FS-6 (global CSS part), FS-7, FS-27 |
| B — Search screen | `src/screens/search/index.tsx`, `src/screens/search/RefinePane.tsx` | FS-5, FS-6 (toggle `flexShrink`), FS-8, FS-9, FS-10, FS-11, FS-12, FS-13, FS-14, FS-15, FS-34 |
| C — DSL builder | `src/components/dsl/DslBuilder.tsx` | FS-3, FS-16, FS-17, FS-18, FS-19, FS-20, FS-29 |
| D — Mail components + empty state | `src/components/mail/AttachmentCard.tsx`, `src/components/mail/ReadingPane.tsx`, `src/components/mail/MailList.tsx`, `src/components/common/EmptyState.tsx` | FS-4, FS-21, FS-22, FS-26, FS-28, FS-30, FS-31 |
| E — Onboarding | `src/screens/onboarding/index.tsx` | FS-23, FS-24, FS-25, FS-32, FS-33 |

Notes:
- `MailList.tsx`, `ReadingPane.tsx` and `EmptyState.tsx` are shared with Advanced SQL (and `EmptyState` with Import and Settings). Group D's owner must be the only editor of these files across both review lists.
- `app.css` (FS-6) is global. Merge with the tools list so only Group A edits it.
