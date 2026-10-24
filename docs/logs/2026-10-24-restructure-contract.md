# Directory restructure contract (frozen 2026-10-24; only the user can change it)

## End state
The repo uses one merged directory layout for docs, mockups, frontend and backend, and the repo root holds exactly one regular file: `README.md`.

## In scope
- Phase 1 (design): two independent opus plans, `docs/logs/restructure/plan-a.md` and `plan-b.md`.
- Phase 2 (merge): lead writes `docs/logs/restructure/plan-final.md` with the best parts of both, plus the work-package split for 2 rounds of 4-5 implementers.
- Phase 3 (migration): 2 rounds, 4-5 implementers each, file-ownership boundaries per work package.

## Out of scope
- Behavior changes, renames of Rust crates/modules or TS identifiers beyond what a path move forces.
- New features, new tests (except a path that a moved test needs to keep working).
- Rewriting historical docs/logs content (only moving files and fixing live references).

## Acceptance (all must pass on main after round 2)
1. `find . -maxdepth 1 -type f ! -name '.*'` prints only `./README.md`. Dot-entries (`.gitignore`, `.claude/`, `.config/`, `.git/`) are allowed at the root.
2. `cargo test --workspace` green, `cargo clippy --workspace --all-targets -- -D warnings` green, `cargo build --workspace` green (run from the new backend dir).
3. `npm test` green and `npm run build` (typecheck + vite) green, run from wherever package.json lives.
4. `npm run e2e:build` green (debug tauri binary with the e2e feature builds).
5. `python3 <new path>/build.py` desktop build green (or its `cargo build --release` app-only step with zero `^warning:` lines).
6. A grep for every old path in the final plan's move table, over tracked live files (code, config, CLAUDE.md, README, scripts; docs/logs history excluded), returns zero hits.
7. The project CLAUDE.md is still loaded by Claude Code at its new location, and its commands are correct for the new layout.
8. `git status --porcelain` is empty after the last commit; history is preserved with `git mv`.

## Round budget
Design: 1 round. Migration: 2 rounds (user decision). Budget used up with acceptance not passed → stop and report.

## Result (HEAD 5f78798)
Acceptance 1-6 and 8 pass (logs `app/scripts/tmp/restructure-gate-r2{a,b,c,d}.log`). Acceptance 7 (`.claude/CLAUDE.md` loaded) needs a fresh session.

## Parking lot
- Under heavy machine load, the full `npm test` and `cargo test --test it` exceed the 40 s foreground cap. Round 2 ran them split by directory or module.
- `tauri info` reports the JS package `@tauri-apps/plugin-fs` as not installed. No repo file references it (checked at d655f42), so this is not caused by the move.
- The vite build warns about a 1,595 kB index chunk. This predates the move.
