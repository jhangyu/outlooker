# Merged fix list — UI layout fix

Sources: `review-frame-search.md` (FS-n), `review-tools.md` (TL-n). Each item's details (app file:line, mockup reference, symptom, fix) are in the source file under its ID.
Base commit: 13f208e.

## Lead decisions
- Onboarding stays full screen, outside the app frame (database is not running during first run, so a rail with other screens would lead to dead screens). Not a fix point. Parking lot.
- FS-6 global nowrap rule lives in `src/app/app.css`, owned by impl-frame-search-opus only.
- TL-1 (MailList `compact` prop) and FS-26 (MailList month header) go to the SQL owner, who owns `MailList.tsx` and the caller in `advanced-sql/index.tsx`.

## Round 1 — all blockers and should-fix items (nits wait for round 2)

| Owner | Files (exclusive, incl. adjacent `*.test.tsx`) | Fix points |
|---|---|---|
| impl-frame-search-opus | `src/app/AppFrame.tsx`, `src/app/app.css`, `src/app/AppFrame.test.tsx`, `src/app/global.test.tsx`, `src/screens/search/*`, `src/screens/onboarding/*`, `src/locales/*/common.json`, `src/locales/*/search.json`, `src/locales/*/onboarding.json` | FS-1, FS-2, FS-5, FS-6, FS-7, FS-8, FS-9, FS-10, FS-11, FS-12, FS-13, FS-14, FS-15, FS-23, FS-24, FS-25 |
| impl-dsl-mail-opus | `src/components/dsl/*`, `src/components/mail/AttachmentCard.tsx`, `src/components/mail/ReadingPane.tsx`, `src/components/mail/MailPreview.tsx`, `src/components/mail/mail.test.tsx`, `src/components/common/*` | FS-3, FS-4, FS-16, FS-17, FS-18, FS-19, FS-20, FS-21, FS-22 |
| impl-sql-opus | `src/screens/advanced-sql/*`, `src/components/sql-editor/*`, `src/components/mail/MailList.tsx`, `src/components/mail/pages.test.tsx`, `src/locales/*/sql.json` | TL-1, TL-2, TL-3, TL-4, TL-5, TL-6, TL-7, TL-8, TL-9, TL-13, TL-14, FS-26 |
| impl-import-settings-opus | `src/screens/import/*`, `src/screens/settings/*`, `src/locales/*/import.json`, `src/locales/*/settings.json` | TL-15, TL-16, TL-17, TL-18, TL-19, TL-20, TL-21, TL-22, TL-23, TL-27, TL-28, TL-29, TL-33 |

## Round 2 — nits plus round-1 reviewer leftovers
One implementer (impl-round2-opus), all files above, serial after round 1 is signed off:
FS-27, FS-28, FS-29, FS-30, FS-31, FS-32, FS-33, FS-34, TL-10, TL-11, TL-12, TL-24, TL-25, TL-26, TL-30, TL-31, TL-32, plus any round-1 reviewer should-fix items moved here.

## Parking lot
- Onboarding inside the app frame (mockup) vs full screen (current) — user decision if they want the mockup behaviour.
- No rendered screenshots: the app needs the Tauri backend. Visual confirmation needs a real app run.
- From impl-dsl-mail-opus (round 1): the selected AND/OR toggle uses Fluent primary hover and pressed colors, while the mockup uses a flat color. Nit, round 2.
- From impl-import-settings-opus (round 1): right-aligned numeric column headers in the imported-PST DataGrid may still show the label left-aligned inside the sortable header button. Check in round 2.
