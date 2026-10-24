# Restructure plan FINAL (lead merge of plan-a + plan-b, 2026-10-24)

Contract: `docs/logs/2026-10-24-restructure-contract.md`. Base: `main` at `d655f42`, no worktrees, no remotes, no running PostgreSQL.

**Base = plan-a** (`docs/logs/restructure/plan-a.md`): its §1 tree, §2 move table and §3 reference-fix inventory are adopted as written, except for the deltas below. Read plan-a §2-§3 for your package's rows.

## Why plan-a's skeleton
- `app/` is the npm root and the Tauri project root at the same time, so app/ ↔ app/backend has the same relation as today's root ↔ src-tauri. The Tauri CLI discovery risk (plan-b R1) and the WDIO path-base risk (plan-b R4) do not exist.
- Depth is preserved for every `../..` path in Rust, e2e and scripts.
- `docs/mockups/` names the mockup area the user asked for (plan-b kept `docs/design`).

## Deltas taken from plan-b
1. `.gitignore`: also add `.claude/settings.local.json`, `.claude/worktrees/`, `.claude/tmp/`.
2. `.claude/CLAUDE.md`: add one line "Design specs go in `docs/specs/`, mockups in `docs/mockups/` (never `docs/superpowers/` or `docs/design/`)."
3. `README.md`: add a short "Layout" section (top-level tree: `app/frontend`, `app/backend`, `app/scripts`, `app/tests`, `docs/specs`, `docs/mockups`, `docs/brand`, `docs/logs`). All npm commands are written `cd app && mise exec -- npm …`.
4. Gate order (plan-b R2-A): warm `cargo build --workspace` first in the 150 s build cap (repeat until it finishes), then clippy, then `cargo test` split per target under the 40 s cap.

## Lead decisions on open points
- **Agent scratch dir** = `app/scripts/tmp/` (code already writes e2e data there via ROOT). R1-D creates `.claude/settings.json` with `{"env": {"CLAUDE_PROJECT_TMP_DIR": "app/scripts/tmp"}}` so the global tmp-guard hook (`~/.claude/hooks/global-tmp-guard:42`) points agents there. CLAUDE.md test-policy text says `app/scripts/tmp/`.
- **Live docs are fixed, history is not.** Acceptance 6 excludes only `docs/logs/`. So R1-E also rewrites old paths in `docs/specs/*` and `docs/mockups/*` (including the repo-tree section around `2026-10-21-outlooker-design.md:410`) to the new layout. Paths in `docs/specs/2026-10-23-ui-density-and-resize-design.md:15-16` that point into untracked `scripts/tmp/` become `app/scripts/tmp/`.
- `tests/fixtures` stays under `app/tests/fixtures` (plan-a). `scripts/fixtures/make-pst.ps1` stays as is (no flattening).
- Memory files outside the repo (`~/.claude/projects/.../memory/*.md`) are updated by the lead after round 2.

## Acceptance-6 grep (used by every package on its own paths, and by R2-D over the whole tree)
```
git grep -nE '(^|[^a-z/._-])src-tauri|(^|[^a-z/._-])(scripts|tests)/(build|fetch|vendor|cargo-test|vitest|test-time|fixtures|tmp|e2e)|"/src/|src/test/setup|src/api/bindings|"include": \["src"\]|docs/(design|superpowers)|(^|[^a-z/])outlooker-logo\.svg|(^|[^/a-z.])mise\.toml' -- . ':!docs/logs' ':!app/backend/native' ':!*.lock' ':!app/package-lock.json'
```
Expected: 0 lines. A hit that is correct under the new layout (for example a crate-relative `tests/` path) is reported with file:line and a reason, not silenced by changing the pattern.

## Round 1 — moves + reference fixes (5 members, parallel, disjoint files)
Same packages as plan-a §5 Round 1 (R1-A backend, R1-B frontend + shell, R1-C scripts + tests, R1-D root meta, R1-E docs), with these changes:
- R1-D additionally owns `.claude/settings.json` (new) and the deltas 1-3 above.
- R1-E additionally rewrites live doc paths in `docs/specs/*` and `docs/mockups/*` (decision above).
- No cargo / npm / python builds in round 1. Static checks only (`git grep`, `node --check`, `python3 -m py_compile`).
- Shared git index: if a git command fails on `index.lock`, wait for your next turn and retry; never delete the lock file.

## Round 2 — gates + fixes (4 members, one compile slot at a time)
Slot order: R2-A → R2-B → R2-C. A member passes the slot by sending `SLOT_DONE` to the next member and to the lead. Between slots, members only read or edit.

| Member | Owns | Gate commands (from the dir named) | Acceptance |
|---|---|---|---|
| R2-A backend | `app/backend/**` | `app/backend`: `cargo build --workspace` (repeat until done), `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace --lib`, then each `--test <name>` target, then `cargo build --release` (app only) + `grep -c '^warning:'` | contract 2 and 5 (release step); `git status --porcelain app/frontend/api/bindings` empty after cargo test |
| R2-B frontend | `app/frontend/**`, `app/*.{json,ts,html}` | `app`: `mise exec -- npm test`, `mise exec -- npm run build` | contract 3 |
| R2-C desktop + e2e | `app/scripts/**`, `app/tests/**` | `app`: `mise exec -- npm run e2e:build`; repo root: `CI=true python3 app/scripts/build.py` | contract 4 and 5 |
| R2-D acceptance sweep | `README.md`, `.claude/**`, `.config/**`, `.gitignore`, `docs/**` except `docs/logs/` | no compile: contract 1, the acceptance-6 grep, `cd app && mise config ls` lists `.config/mise.toml`, `cd app && mise exec -- node --version` = `v24.22.1`, `git log --follow --oneline` on 3 moved files shows pre-move history, contract 8 last | contract 1, 6, 8 |

Every gate step writes the exact command, `RC=$?` captured right after it, and `git rev-parse HEAD` into `app/scripts/tmp/restructure-gate-<member>.log`.
Contract 7 (`.claude/CLAUDE.md` is loaded) is a live check by the lead or user in a fresh session.
