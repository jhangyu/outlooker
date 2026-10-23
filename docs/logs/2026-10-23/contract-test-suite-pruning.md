# Contract: test-suite pruning + recurrence prevention (frozen 2026-10-23)

## End state
The default test runs (`cargo test --workspace`, `npm test`) contain only tests that guard core/business logic or real regression risk; low-value tests (UI cosmetics, trivial getters/wiring, duplicated coverage, slow tests with no unique signal) are deleted; and a mechanism stops AI agents from re-adding such tests.

## In scope
- Rust tests under `src-tauri/src`, `src-tauri/crates/*` (inline `#[cfg(test)]` and `tests/`).
- Frontend tests: `src/**/*.test.*` (vitest), `tests/e2e/*` (wdio).
- Prevention: project-level rules (CLAUDE.md / docs) and, if justified, one mechanical check.

## Out of scope
- `src-tauri/native/libpff/tests` (vendored upstream C library).
- `~/.claude/` global rule files (changes there are proposed to the user, not applied).
- Production code changes, except removing test-only helpers that become dead after test deletion, and test-support seams a REWRITE needs.

## Acceptance criteria
1. Inventory files exist: `docs/logs/2026-10-23/test-inventory-rust.md`, `docs/logs/2026-10-23/test-inventory-frontend.md`; each lists every test (file:line, name, category, runtime where measurable) and flags non-core / slow candidates.
2. Cleanup plan exists: `docs/logs/2026-10-23/test-cleanup-plan.md` with a per-test DELETE / KEEP / MERGE / REWRITE decision and a reason for each DELETE and REWRITE.
3. Prevention design exists: `docs/logs/2026-10-23/test-policy-design.md`.
4. Prevention mechanism implemented and committed per the design.
5. Every DELETE in the plan is applied and committed, and no test outside the plan is deleted. Every REWRITE is applied: the rewritten test still guards the same core behavior.
5b. Shared database (user directive): all database tests share ONE PostgreSQL startup per test run. No test boots its own cluster. Any exception needs explicit user approval. The same rule applies to every other expensive setup that tests repeat (fixture import, large seed data, runtime or app launch): it runs once per test run and is shared, with cheap per-test isolation.
5a. Runtime budget (user directive): no single test in the default runs takes more than 1 s, and the typical test takes a few hundred ms. A slow test that guards core logic is REWRITTEN to fit the budget. A slow test that does not guard core logic is DELETED.
6. After cleanup: `cargo test --workspace` green, `cargo clippy --workspace --all-targets -- -D warnings` green, `npm test` green, `npm run typecheck` green.
7. Before/after test count and wall-clock recorded in `docs/logs/2026-10-23/test-cleanup-result.md`.

## Round budget
3 rounds. Round 1 = inventory. Round 2 = plan + prevention design. Round 3 = implement + delete.

## Parking lot
(empty)
