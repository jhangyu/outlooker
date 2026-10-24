# Outlooker

Desktop app (Tauri 2 + React + Rust) that imports Outlook PST files into a bundled PostgreSQL and searches them.

## Prerequisites
- [mise](https://mise.jdx.dev) (pins node 24.22.1 via `.config/mise.toml`) and a Rust toolchain (`app/backend/rust-toolchain.toml`).
- Bundled PostgreSQL: `cd app && mise exec -- node scripts/fetch-postgres.mjs`

## Layout
- `app/frontend`: React UI
- `app/backend`: Rust workspace and Tauri app
- `app/scripts`, `app/tests`: build, fetch and test tooling (npm root is `app/`)
- `docs/specs`: design specs, `docs/mockups`: UI mockups, `docs/brand`: logo, `docs/logs`: work logs

## Commands
- Build the frontend (typecheck + vite): `cd app && mise exec -- npm run build`
- Frontend unit tests: `cd app && mise exec -- npm test`
- End-to-end tests: `cd app && mise exec -- npm run e2e` (groups `flow`, `seeded`, `scroll`; `scroll` seeds 120,000 messages and is long)
- One e2e group: `cd app && mise exec -- npm run e2e:build`, then `cd app && mise exec -- node tests/e2e/run.mjs --spec tests/e2e/flow.e2e.ts`
- Desktop build: `python3 app/scripts/build.py`
- Cargo test, lint and build commands: see `.claude/CLAUDE.md`.

## Docs
- Design spec: `docs/specs/2026-10-21-outlooker-design.md`
- UI reference: `docs/mockups/variant-1/NOTES.md`
