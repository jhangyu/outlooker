# Outlooker

Desktop app (Tauri 2 + React + Rust) that imports Outlook PST files into a bundled PostgreSQL and searches them.

## Prerequisites
- [mise](https://mise.jdx.dev) (pins node 24.22.1 via `mise.toml`) and a Rust toolchain (`src-tauri/rust-toolchain.toml`).
- Bundled PostgreSQL: `node scripts/fetch-postgres.mjs`

## Commands
- Build the frontend (typecheck + vite): `mise exec -- npm run build`
- Frontend unit tests: `mise exec -- npm test`
- End-to-end tests: `mise exec -- npm run e2e` (groups `flow`, `seeded`, `scroll`; `scroll` seeds 120,000 messages and is long)
- One e2e group: `mise exec -- node tests/e2e/run.mjs --spec tests/e2e/flow.e2e.ts`
- Desktop build: `python3 scripts/build.py`
- Cargo test, lint and build commands: see `CLAUDE.md`.

## Docs
- Design spec: `docs/superpowers/specs/2026-10-21-outlooker-design.md`
- UI reference: `docs/design/variant-1/NOTES.md`
