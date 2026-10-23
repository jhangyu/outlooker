# Round brief (shared by every round of the tech-debt cleanup)

Contract (read first, quoted verbatim in kickoffs): `docs/logs/2026-10-23/contract-techdebt.md`.
Inventories: `docs/logs/2026-10-23/scan-core.md` (C*), `scan-app.md` (A*), `scan-ui.md` (U*, D*).

## User decisions (final)
- U1: FIX. Switching screens must keep each screen's state (SQL text/result/selection, search inputs/selection/scroll). Keep all screens mounted and hide inactive ones, or lift the state above `AppFrame` — the architect picks the smaller correct change.
- D11: USE `outlooker-logo.svg` as the in-app logo: move it under `src/assets/`, replace the text "O" boxes in the title bar (`AppFrame.tsx`) and onboarding screen. Do not change the app icon.
- D8: done (prototype dirs deleted).
- A5 (narrow Tauri capabilities): user APPROVED one full `mise exec -- npm run e2e` run as runtime proof at round 2 close (long run, orchestrator schedules it).
- A7: keep the `outlook-fixtures` feature and `make-pst.ps1` (pending Windows phase).
- No parking lot. Every finding is fixed in its round or judged "not worth fixing" with a one-line reason in the handoff.
- Doc fixes are applied directly (no separate approval).

## Squads and exclusive file ownership (same split every round)
- Squad `core` — everything under `src-tauri/` EXCEPT `src-tauri/capabilities/`. Includes all Cargo.toml / Cargo.lock, build.rs, crates, Tauri commands. Worktree `/Users/jhangyu/project/outlooker-wt/r{N}-core`.
- Squad `ui` — `src/`, `tests/`, `docs/` (except `docs/logs/`), `scripts/`, root `package.json`, `package-lock.json`, `vite.config.ts`, `tsconfig.json`, `wdio.conf.ts`, `mise.toml`, `index.html`, `src-tauri/capabilities/`, `outlooker-logo.svg`. Worktree `/Users/jhangyu/project/outlooker-wt/r{N}-ui`.
- `docs/logs/` belongs to the orchestrator; squads write only their own `round-{N}-{squad}-*.md` files there.
- Cross-squad items (e.g. a Tauri command signature change that the frontend must follow): the frontend↔Tauri command contract must not change in this cleanup unless both architects agree and tell the orchestrator first.

## Environment (dev Mac) — embed in every implementer / test-runner prompt
- Cargo rule (root `CLAUDE.md`): run from `<worktree>/src-tauri`; only `cargo test --workspace [--test NAME | --lib | <filter>]`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --workspace`. Never `-p`, per-package `--features`, `--all-features`, or `cargo check`.
- In worktrees always export `CARGO_TARGET_DIR=/Users/jhangyu/project/outlooker/src-tauri/target` so registry deps are reused (a fresh target dir needs a full rebuild that exceeds the foreground cap).
- Rust tests: add `-- --test-threads 4` (macOS SysV shm limit; each PostgreSQL cluster holds one segment).
- Foreground caps: 40 s for tests, 150 s for builds. A test killed by the cap leaves its PostgreSQL postmaster running (ppid 1): stop it with `src-tauri/resources/postgresql/bin/pg_ctl -D <dir> stop -m fast` after confirming the data dir belongs to this repo/worktree; never blanket-kill or ipcrm. Split long runs by `--test NAME` / name filter; never background or extend timeouts. A command that cannot fit = report BLOCKED.
- Node: always `mise exec -- npm ...` / `mise exec -- npx ...` (project pins node 24). Frontend gates: `mise exec -- npm run build` (tsc + vite), `mise exec -- npm test` (vitest).
- `src-tauri/resources/postgresql` (core worktree) and `node_modules` (ui worktree) are symlinks into the main tree, excluded from git. Never add, delete, or modify them.
- No sleep, no polling loops (hook-blocked). Wait for messages by ending the turn.
- Write files with Write/Edit, not heredocs. Do not put `;` or `&` inside `git commit -m` text.

## Git red lines (verbatim, every member)
- NEVER run `git stash` (including `git stash list`), `git reset`, `git checkout --`, or `git clean` — teammates' uncommitted work being present in the tree is normal.
- Commit ONLY with explicit `git add <your-own-files>`. Never `git add -A` / `git add .`.
- Commit with a pathspec: `git commit -- <your-own-files>`, never a bare `git commit`. For `git mv`/deletions list both old and new paths in the pathspec.
- Never touch files outside your ownership list. Never force-push. Never switch branches in a worktree.
- Commit message trailer: `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>`.

## Engineering rules
- Unreleased project: no backward compatibility, no shims. Superseded code/docs/tests are deleted entirely. Similar logic converges into one module.
- Minimum diff, no speculative abstractions. Behavior-preserving refactors keep tests green before and after; a test that was red before you started = stop and report.
- Before calling any third-party API you have not seen used in this repo, check the locked version in Cargo.lock / package-lock.json and query context7 docs for that version.
- Every non-trivial change leaves one check that fails if the logic breaks (existing test or a new small one).
