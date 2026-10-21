# Convergence Contract — Phase 1 (design + spec + plan)

Frozen 2026-10-21. Only the user may change it.

**End state:** A user-approved design spec, a user-chosen UI design, and an architect-reviewed implementation plan with a 2-round execution plan exist in the repo.

## In scope
1. Architecture sections (4 architects): tech stack, system architecture, frontend API (Tauri command/event interface), backend server (import pipeline + DB schema + search).
2. Three UI design variants (static HTML prototypes) for all screens in docs/specs/requirements.md.
3. Merged spec at docs/superpowers/specs/2026-10-21-outlooker-design.md.
4. Implementation plan at docs/superpowers/plans/ (outline by 1 planner, architect review, then 3 planners fill details + execution plan: 2 rounds, each 5–7 implementers + 1 reviewer).

## Out of scope
Any production code, scaffolding, dependency installation.

## Acceptance
- A1. docs/specs/arch/{01-tech-stack,02-system-architecture,03-frontend-api,04-backend-server}.md exist; each has no "TBD"/"TODO"; every third-party dependency lists a version verified against its registry or repo.
- A2. Cross-section consistency: every Tauri command named in 03 is served by a component in 02/04; every table referenced in 03/04 is defined in the 04 schema.
- A3. docs/design/variant-{1,2,3}/index.html exist, open in a browser, cover the 5 screen groups, use Fluent 2 tokens, contain no dark-mode styles (no `prefers-color-scheme: dark`, no dark theme).
- A4. User selects one design variant.
- A5. Merged spec is committed, and the user approves it.
- A6. Plan outline is approved by an architect review; the final plan contains an execution plan with 2 rounds, each listing 5–7 implementer tasks with file ownership + 1 reviewer.

## Round budget
3 rounds per deliverable.

## Parking lot
- Prototype-only features (user ruled out of scope 2026-10-21): exact delete/keep preview on PST removal; true pause / pause all / converter worker count / re-convert images / disk usage breakdown / SQL format button; search term highlighting; drag-and-drop import. UI must not show these controls.
- macOS signing + notarization of bundled PostgreSQL binaries (release time).
- CI workflow once a git remote exists.
- Grep check for forbidden dependency names.

## User decisions log
- 2026-10-21: Windows x64 machine available; user opens SSH for direct operation after all macOS-side prerequisites are done.
