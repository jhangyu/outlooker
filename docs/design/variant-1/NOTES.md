# Variant 1 — Outlook-native productivity

## Not implemented (user ruling)
The prototype draws these controls, but the app does not build them (spec `docs/superpowers/specs/2026-10-21-outlooker-design.md` §0.2 "Prototype-only features"): SQL Format, Pause (true pause / pause all), worker-count setting, deleted/kept preview before removing a PST, drag-and-drop import zone.

**Rationale.** The app should look like a sibling of new Outlook and Microsoft 365, so users who know Outlook need no learning time. It uses a brand-blue title bar, a left navigation rail with icons and labels, Fluent 2 neutral surfaces, and standard density (32 px controls, 14/20 body text). The mail reading conventions are the ones Outlook users know: an avatar, sender, date and subject in the list; a selected row with a brand left bar; a reading pane with sender, To/Cc and attachment cards. Every screen is a full-height multi-column grid. Panes scroll on their own, and the only header is a 48 px title bar. Light theme only.
Open `index.html` directly. Deep links for review: `#sql`, `#sql:table`, `#import`, `#settings`, `#onboarding`. You can add `:empty`, `:en`, `:short` or `:zero` (for example `#search:empty:en`). The title bar toggles sample data vs. empty states and zh-TW vs. English.

| Token group | Values (CSS custom properties in `styles.css :root`) |
|---|---|
| Neutrals | Fg1 #242424, Fg2 #424242, Fg3 #616161, Fg4 #707070; Bg1 #fff, Bg2 #fafafa, Bg3 #f5f5f5, Bg4 #f0f0f0, Bg5 #ebebeb; Stroke1 #d1d1d1, Stroke2 #e0e0e0, Stroke3 #f0f0f0 |
| Brand | Background #0f6cbd, Hover #115ea3, Pressed #0c3b5e, Background2 #ebf3fc, Stroke2 #b4d6fa |
| Status | Success #0e700e / #f1faf1, Warning #8a3707 / #fff9f5, Danger #b10e1c / #fdf3f4 |
| Spacing | XXS 2, XS 4, SNudge 6, S 8, MNudge 10, M 12, L 16, XL 20, XXL 24, XXXL 32 px |
| Type ramp | Base100–600 = 10/14, 12/16, 14/20, 16/22, 20/28, 24/32; weights 400/600/700; Segoe UI Variable → Segoe UI → system-ui (+ PingFang TC / JhengHei for CJK) |
| Radius | Small 2, Medium 4, Large 6, XLarge 8, Circular |
| Elevation | shadow2 / 4 / 8 / 16 / 28 / 64 (Fluent two-layer ambient + key) |
| Focus | 2 px #000 outline with an inner 1 px white ring (Fluent strokeFocus1/2) |

Text contrast: Fg3 #616161 on white = 6.2:1; Fg4 #707070 = 4.95:1; white on brand #0f6cbd = 5.2:1.

## Screen → Fluent UI React v9 component mapping
| Screen / region | Components |
|---|---|
| Shell | Title bar: `Toolbar` + `ToggleButton` group (language). Nav rail: `NavDrawer` (inline, rail size) with `NavItem`, or a vertical `TabList` with arrow-key navigation |
| Search — refine pane | `Tree` / `FlatTree` (source PST, folders) with `CounterBadge`; `Checkbox` quick filters |
| Search — query bar | `SearchBox`; `ToggleButton` + `CounterBadge` opens the condition builder |
| Search — DSL blocks | Each group is a `Card` (appearance="outline"). Rows use `Dropdown` (field), `Input` / `DatePicker` (value) and `Button` icon-only remove. AND/OR is a small `ToggleButton` pair. Use `Divider` with an AND/OR label between groups |
| Search — hints / zero result | `MessageBar intent="warning"` for queries under 3 characters. Empty state: illustration + `Subtitle1` + `Button` |
| Mail list | Virtualized `List` (`@fluentui/react-list`) with `Avatar`, `Persona`-style rows, month headers. Selected row = brand left bar |
| Reading pane | `Persona`, `Text`/`Title3`, `Badge` (source PST occurrences), attachment cards = `Card` size="small" with a disabled download `Button` + `Tooltip` |
| Advanced SQL | CSS grid `1fr 3fr 1fr 3fr`. Saved queries: `List` + `Accordion` schema tree. Editor: CodeMirror 6 (`@codemirror/lang-sql`) inside a `Toolbar` (Run / Format / Save) with a read-only `Badge`. Column 3: mail `List`, or `DataGrid` when the result has no `id` column. Column 4: reading pane, or a row-detail key/value list |
| Import | `Toolbar` + `Button appearance="primary"`. Imported PSTs: `DataGrid` (sortable). Queue: `List` of `Card`s with `ProgressBar` (+ phase segments), `Badge` status, `Button`s Pause / Cancel / Resume / Retry / Remove. Error log: `List` with severity icons. Remove PST: `Dialog` (modal) that states how many messages are deleted vs. kept |
| Settings | Left section index = `TabList vertical` (or anchor `Link`s). Cards: `Field` + `Input` (path), `RadioGroup` (language), `Slider` + `SpinButton`/`Dropdown` (WebP quality, workers), `DataGrid` (indexes), status `Badge`/`PresenceBadge` |
| Onboarding | Full-pane wizard: step list (custom stepper on `Text` + `Avatar`-like numerals), choice tiles = `Card` selectable, `ProgressBar` + checklist for DB init, drop zone + `Button`. Right column = feature `Card`s |
| Toast | `Toaster` / `Toast` (`useToastController`) |
