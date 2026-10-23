# Implementation brief: column resize (shared by all 4 implementers)

Repo: /Users/jhangyu/project/outlooker, branch `main`, shared working tree (no worktrees). HEAD at kickoff: d32b4e0.

Read first, in order:
1. Contract: docs/logs/2026-10-23/contract-column-resize.md
2. Persist plan: docs/logs/2026-10-23/plan-column-resize-persist.md (shared API §2, S1/S2 §4, tests §5)
3. Grid plan: docs/logs/2026-10-23/plan-column-resize-grid.md (adapter §2, S3 §3, S4 option A §4 — user chose option A, MailList gets a header row)

The plans were written before the tech-debt merge into main. Line numbers may have moved; re-locate by content. The shared API signature in persist plan §2 / §7 is FROZEN — code against it even before columnWidths.ts lands.

## Work packages (exclusive file ownership)
| Member | Package | Owned files |
|---|---|---|
| impl-shared-sonnet | shared module + S1 settings | src/components/common/columnWidths.ts (new), src/components/common/columnWidths.test.tsx (new), src/components/common/index.ts, src/test/setup.ts, src/screens/settings/index.tsx |
| impl-import-sonnet | S2 import DataGrid | src/screens/import/index.tsx, src/screens/import/import.test.tsx (only if needed) |
| impl-sql-sonnet | S3 SQL ResultTable | src/screens/advanced-sql/ResultTable.tsx, src/screens/advanced-sql/advanced-sql.test.tsx |
| impl-mail-sonnet | S4 MailList header row | src/components/mail/MailList.tsx, src/components/mail/mail.test.tsx, src/locales/en/search.json, src/locales/zh-TW/search.json |

Need a change in a file you do not own = SendMessage the owner (names above), never edit it.

## Gates
- Node: always `mise exec -- npm ...` / `mise exec -- npx ...`.
- Your own check: `mise exec -- npx vitest run <your test file>`; then `mise exec -- npm run build` (tsc + vite). Other members edit in parallel, so a build error in a file you do not own is not yours: report it, do not fix it.
- Foreground cap 40 s for tests, 150 s for build. Never background, never extend timeouts. Cannot fit = BLOCKED.
- No sleep, no polling loops (hook-blocked). Wait for messages by ending the turn.
- Third-party API: check the installed version in node_modules (@fluentui/react-table 9.19.23) before using any API; do not code from memory.

## Git red lines (verbatim)
- NEVER run `git stash` (including `git stash list`), `git reset`, `git checkout --`, or `git clean` — teammates' uncommitted work in the tree is normal.
- Commit ONLY with explicit `git add <your-own-files>`. Never `git add -A` / `git add .`.
- Commit with a pathspec: `git commit -- <your-own-files>`, never a bare `git commit`.
- Run `git rev-parse --abbrev-ref HEAD` (must be main) before commit, `git status --porcelain -- <your files>` after (must be empty).
- No `;` or `&` inside `git commit -m` text. Write files with Write/Edit, not heredocs. Never push, never switch branches.
- Commit message trailer: `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>`
- Do not touch other untracked files (docs/logs/*, outlooker-logo.svg).

## Engineering rules
- Unreleased project: no backward-compat shims; delete superseded code (e.g. S2 flex-basis classes). One shared module, no per-surface drag/persist code.
- Minimum diff. Every non-trivial change leaves one test that fails if the logic breaks.
