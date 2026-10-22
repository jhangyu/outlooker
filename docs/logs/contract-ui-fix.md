# Contract — UI layout fix (variant 1 fidelity)

End state: every screen of the React app matches the chosen mockup `docs/design/variant-1/` in layout, with no misaligned elements and no unintended line wrapping, in both zh-TW and English.

## In scope
- Phase R (review): two reviewers compare `src/` (app frame, screens, components, CSS) against `docs/design/variant-1/` (index.html, styles.css, app.js, NOTES.md) and write fix lists.
- Phase F (fix): implementers fix the merged fix list in 2 rounds (4–5 implementers total, file-ownership split).

## Out of scope
- Controls the mockup has but the product excludes (see handover: "out-of-scope prototype controls excluded").
- Dark theme, new features, behaviour changes, backend (`src-tauri/`).
- Variants 2 and 3.

## Acceptance
- R1. `docs/logs/ui-review/review-frame-search.md` and `docs/logs/ui-review/review-tools.md` exist; every fix point has ID, app file:line, mockup reference (file:line or selector), symptom, concrete fix.
- R2. `docs/logs/ui-review/fix-list.md` (merged by lead) assigns every accepted fix point to a round and an owner file set.
- F1. Every fix point assigned to the round is fixed or reported as BLOCKED with reason.
- F2. `mise exec -- npm run typecheck` RC=0 and `mise exec -- npm test` RC=0 after each round.
- F3. Round reviewer verdict confirms no open blocker for the round.

## Round budget
Phase R: 1 round. Phase F: 2 rounds. Max 2 review→fix cycles per round.
