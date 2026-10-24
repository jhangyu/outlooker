# Restructure plan A (architect A, 2026-10-24)

Base: `main` at `d655f42`, one worktree, no unmerged branches (`git worktree list`, `git branch --no-merged main`).

## 0. Core idea (one sentence)

Put the whole product (one npm package + one Cargo workspace + its tooling and tests) under a single `app/` directory, and keep only repo-level material at the root (`README.md`, `docs/`, dot-config).

Why `app/` and not top-level `frontend/` + `backend/` siblings:

- The npm package root must be an ancestor of everything that resolves `node_modules`: the frontend, `tests/e2e/*.ts` (`@wdio/*` types via `tests/e2e/tsconfig.json:9`), `scripts/vitest-capped.mjs:7` (`import ... from "vite"`) and `wdio.conf.ts`. Node and TypeScript resolve `node_modules` only by walking up. With siblings, every one of these needs a custom resolution config.
- The Tauri CLI finds `tauri.conf.json` by searching down from its cwd (the npm package dir). Siblings would need `TAURI_APP_PATH` (present in the CLI binary, `strings node_modules/@tauri-apps/cli-darwin-arm64/*.node`), set per invocation, and not portable to the Windows `cmd` shell that npm scripts use there.
- Depth is preserved. Today the crates reach the repo root with `../../..` and the e2e code with `../..`. Under `app/` those same strings resolve to `app/`, and `tests/`, `scripts/`, `dist` move into `app/` too. So most relative paths stay byte-identical (proof in §3, column "unchanged").

Inside `app/`, `src/` becomes `frontend/` and `src-tauri/` becomes `backend/`: the user asked for named frontend and backend areas, the Tauri CLI does not depend on the name `src-tauri`, and the rename is at equal depth, so relative links keep working.

## 1. Target tree

```
README.md                      the only root regular file
.gitignore
.claude/CLAUDE.md              project instructions (Claude Code loads .claude/CLAUDE.md)
.config/mise.toml              node pin (mise reads .config/mise.toml)
docs/
  specs/                       product + design specs (requirements.md, dated design specs)
  mockups/                     clickable design HTML (was docs/design)
    variant-1/                 main UI reference prototype
    import-sync/               import/sync page mockups A1/A2/B1/B2
  brand/outlooker-logo.svg     brand master artwork (not shipped, not referenced by code)
  logs/                        session logs, history (untouched)
app/                           the product: npm package root = Tauri project root
  package.json, package-lock.json, index.html, vite.config.ts, tsconfig.json, wdio.conf.ts
  node_modules/  dist/         (gitignored, generated)
  frontend/                    React + TS (was src/)
    api/ app/ components/ screens/ locales/ test/ main.tsx
  backend/                     Cargo workspace + Tauri shell (was src-tauri/)
    src/ crates/{outlooker-core,outlooker-loadgen,pff}/ native/ schemas/ capabilities/ icons/
    .cargo/config.toml  tauri.conf.json  Cargo.toml  Cargo.lock  rust-toolchain.toml  build.rs
  scripts/                     build.py, fetch-postgres.mjs, vendor-libpff.py, test runners, fixtures/make-pst.ps1
    tmp/                       scratch (gitignored): probes, e2e app data, fetch work dir
  tests/
    e2e/                       WebdriverIO e2e (build/run/prepare + specs)
    fixtures/external/         outlook.pst + NOTICE (shared by Rust tests and e2e)
```

Root after migration: `README.md`, `.gitignore`, `.git/`, `.claude/`, `.config/`, `docs/`, `app/`. `find . -maxdepth 1 -type f ! -name '.*'` → `./README.md`.

## 2. Move table (source: `git ls-files`)

| Old path | New path | Files | Notes |
|---|---|---|---|
| `src/` | `app/frontend/` | 141 | internal `../../api/...` imports unchanged |
| `src-tauri/` | `app/backend/` | 1225 | incl. `native/libpff` (1076), `.cargo/`, `crates/` |
| `index.html` | `app/index.html` | 1 | vite root = package dir |
| `package.json` | `app/package.json` | 1 | scripts unchanged (all relative to package dir) |
| `package-lock.json` | `app/package-lock.json` | 1 | |
| `vite.config.ts` | `app/vite.config.ts` | 1 | |
| `tsconfig.json` | `app/tsconfig.json` | 1 | |
| `wdio.conf.ts` | `app/wdio.conf.ts` | 1 | its `./tests/e2e/...` paths unchanged |
| `scripts/` | `app/scripts/` | 7 | `build.py`, `cargo-test-runner.mjs`, `fetch-postgres.mjs`, `test-time-judge.mjs`, `vendor-libpff.py`, `vitest-capped.mjs`, `fixtures/make-pst.ps1` |
| `tests/` | `app/tests/` | 10 | `e2e/` (8), `fixtures/external/` (2) |
| `mise.toml` | `.config/mise.toml` | 1 | |
| `CLAUDE.md` | `.claude/CLAUDE.md` | 1 | |
| `outlooker-logo.svg` | `docs/brand/outlooker-logo.svg` | 1 | unreferenced by code (`git grep outlooker-logo` → only `docs/superpowers/specs/2026-10-21-outlooker-design.md:416`) |
| `docs/design/` | `docs/mockups/` | 11 | `import-sync/` (6), `variant-1/` (5) |
| `docs/superpowers/specs/*` | `docs/specs/` | 3 | the `superpowers/` level is a tool name, not a topic |
| `docs/specs/requirements.md` | (stays) | 1 | |
| `docs/logs/` | (stays) | 68 | history, content not rewritten |
| `README.md`, `.gitignore` | (stay) | 2 | edited |

Untracked/ignored state that must follow (not in git):

| Old | New | How |
|---|---|---|
| `src-tauri/target/` (13 GB), `src-tauri/resources/postgresql/`, `src-tauri/gen/` | under `app/backend/` | `git mv src-tauri app/backend` renames the directory on disk, so ignored content should come along; verify with `ls app/backend/target app/backend/resources/postgresql/bin` after the move (see §6) |
| `scripts/tmp/` (e2e app data, fetch work dir) | `app/scripts/tmp/` | same: rides along with `git mv scripts app/scripts` |
| `node_modules/` (756 MB) | `app/node_modules/` | plain `mv node_modules app/` (`.bin` links are relative) |
| `dist/` | removed | regenerated by `vite build` into `app/dist` |

## 3. Reference-fix inventory (all verified by grep on `d655f42`)

Paths in the "New" column are the paths after the move. "unchanged" = the literal stays correct because depth is preserved.

### Backend (`src-tauri/` → `app/backend/`)

| File:line | Today | Action |
|---|---|---|
| `src-tauri/.cargo/config.toml:2` | `TS_RS_EXPORT_DIR = "../src/api/bindings"` | → `"../frontend/api/bindings"` |
| `src-tauri/.cargo/config.toml:4` | `CARGO_TEST_RUNNER_JS = "../scripts/cargo-test-runner.mjs"` | unchanged (`app/backend/../scripts`) |
| `src-tauri/tauri.conf.json:6-9` | `beforeDevCommand npm run dev`, `devUrl :1420`, `beforeBuildCommand npm run build`, `frontendDist ../dist` | unchanged (`app/backend/../dist` = `app/dist`; npm cwd = `app/`) |
| `src-tauri/tauri.conf.json:22-23` | `resources/postgresql/`, `icons/...` | unchanged (backend-relative) |
| `src-tauri/crates/pff/build.rs:24` | `manifest.join("../../native/libpff")` | unchanged |
| `src-tauri/crates/pff/build.rs:27` | msg `src-tauri/native/libpff ... scripts/vendor-libpff.py` | → `app/backend/native/libpff ... app/scripts/vendor-libpff.py` |
| `src-tauri/crates/pff/tests/outlook_pst.rs:6` | `../../../tests/fixtures/external/outlook.pst` | unchanged (= `app/tests/fixtures/...`) |
| `src-tauri/crates/outlooker-core/src/dto/search.rs:204` | `include_str!("../../../../schemas/condition-group.json")` | unchanged (schemas stays in backend) |
| `src-tauri/crates/outlooker-core/src/paths/mod.rs:57` | `../../resources/postgresql` | unchanged |
| `src-tauri/crates/outlooker-core/src/dto/events.rs:66` | msg `set by src-tauri/.cargo/config.toml` | → `app/backend/.cargo/config.toml` |
| `src-tauri/crates/outlooker-core/tests/it/app_compose.rs:110` | `../../../tests/fixtures/external/outlook.pst` | unchanged |
| `src-tauri/crates/outlooker-core/tests/it/import_pipeline.rs:30,33,1143-1144` | `../../../tests/fixtures/...`, `REPO_ROOT = "../../.."`, `tests/fixtures/pst/` | values unchanged (now resolve under `app/`); line 31 comment `Repository root` → `app/ root` |
| `src-tauri/crates/outlooker-loadgen/tests/perf/support.rs:23-25` | `repo_root() = MANIFEST/../../..`, comment `src-tauri/crates/outlooker-loadgen` | → `../../../..` (true repo root, still needed for `docs/logs` at :65-66 and `git rev-parse` at :50); comment → `app/backend/crates/outlooker-loadgen` |
| `.../perf/support.rs:28,30` | `src-tauri/target/perf-root` | → `app/backend/target/perf-root` |
| `.../outlooker-loadgen/tests/perf.rs:98` | `repo_root().join("src-tauri")` | → `"app/backend"` |
| `.../outlooker-loadgen/tests/perf.rs:102` | `repo_root().join("src-tauri/target")` | → `"app/backend/target"` |
| `.../outlooker-loadgen/tests/perf.rs:3`, `perf/support.rs:17,19` | `docs/logs/...` | unchanged (repo-root relative, fixed by :25) |
| `.../outlooker-core/examples/import_bench.rs:4` | `Run from src-tauri/` | → `app/backend/` |
| `.../outlooker-core/examples/test_pg_server.rs:1`, `src/test_support/mod.rs:2` | `scripts/cargo-test-runner.mjs` | → `app/scripts/cargo-test-runner.mjs` |
| `.../test_support/template.rs:56`, `tests/cluster_lifecycle.rs:4,43,246`, `tests/it/app_compose.rs:2,77`, `tests/it/lifecycle.rs:27` | msg `run node scripts/fetch-postgres.mjs` | → `node app/scripts/fetch-postgres.mjs` |

Not touched: `crates/outlooker-core/src/db/indexes.rs:79`, `importer/normalize/*.rs` golden paths, `test_support/template.rs:43`, `loadgen/src/corpus.rs:27-28`, `src/lib.rs:149` (all crate-internal).

### Frontend + package shell (→ `app/`)

| File:line | Today | Action |
|---|---|---|
| `index.html:6` | `href="/src/components/brand/mark.svg"` | → `/frontend/components/brand/mark.svg` |
| `index.html:11` | `src="/src/main.tsx"` | → `/frontend/main.tsx` |
| `vite.config.ts:11` | `setupFiles: ["src/test/setup.ts"]` | → `frontend/test/setup.ts` |
| `vite.config.ts:12` | `include: ["src/**/*.test.{ts,tsx}"]` | → `frontend/**/...` |
| `tsconfig.json:16` (`"include": ["src"]`) | | → `["frontend"]` |
| `package.json:12-15` | `-p tests/e2e`, `scripts/vitest-capped.mjs`, `tests/e2e/build.mjs`, `tests/e2e/run.mjs` | unchanged (package-relative) |
| `wdio.conf.ts:2,11` | `./tests/e2e/prepare.mjs`, `./tests/e2e/${group}.e2e.ts` | unchanged |
| `src/**` relative imports (`../../api/...`) | | unchanged (moved as one tree) |

### Scripts + tests (→ `app/scripts/`, `app/tests/`)

`ROOT` in the scripts becomes `app/` (`Path(__file__).parent.parent`, `join(dirname(...), "..")`), and `REPO_ROOT` in `prepare.mjs:8` (`../..`) becomes `app/` too.

| File:line | Today | Action |
|---|---|---|
| `scripts/build.py:2` | docstring `python3 scripts/build.py` | → `python3 app/scripts/build.py` |
| `scripts/build.py:11,12,13` | `ROOT / "src-tauri" / ...` (PG dir, libpff ORIGIN, bundle dir) | `"src-tauri"` → `"backend"` |
| `scripts/build.py:39` | msg `src-tauri/native/libpff ... scripts/vendor-libpff.py` | → `app/backend/native/libpff ... app/scripts/vendor-libpff.py` |
| `scripts/build.py:42,43-44` | `node scripts/fetch-postgres.mjs`, `ROOT/node_modules`, `npm ci` with `cwd=ROOT` (:22) | unchanged (cwd = `app/`) |
| `scripts/fetch-postgres.mjs:3` | usage `node scripts/fetch-postgres.mjs` | → `node app/scripts/fetch-postgres.mjs` |
| `scripts/fetch-postgres.mjs:12,13` | `join(ROOT, "src-tauri", ...)` | → `"backend"` |
| `scripts/fetch-postgres.mjs:78` | `join(ROOT, "scripts", "tmp")` | unchanged |
| `scripts/vendor-libpff.py:2` | docstring `src-tauri/native/libpff` | → `app/backend/native/libpff` |
| `scripts/vendor-libpff.py:32` | `ROOT / "src-tauri/native/libpff"` | → `"backend/native/libpff"` |
| `scripts/cargo-test-runner.mjs:1` | comment `src-tauri/.cargo/config.toml` | → `app/backend/.cargo/config.toml` |
| `scripts/vitest-capped.mjs:11,14` | `"vite.config.ts"`, `node_modules/vitest/vitest.mjs` (cwd-relative) | unchanged (npm cwd = `app/`) |
| `tests/e2e/prepare.mjs:14` | `path.resolve(REPO_ROOT, "src-tauri", CARGO_TARGET_DIR ?? "target")` | → `"backend"` |
| `tests/e2e/prepare.mjs:38` | `cwd = path.join(REPO_ROOT, "src-tauri")` | → `"backend"` |
| `tests/e2e/prepare.mjs:1,9` | `scripts/tmp/e2e/` | unchanged (`app/scripts/tmp/e2e`) |
| `tests/e2e/run.mjs:19` | `path.join(REPO_ROOT, "src-tauri", "resources", ...)` | → `"backend"` |
| `tests/e2e/run.mjs:2,27` | `tests/e2e/...`, `wdio.conf.ts` | unchanged |
| `tests/e2e/build.mjs:15` | comment `src-tauri/resources/postgresql/` | → `backend/resources/postgresql/` |
| `tests/e2e/build.mjs:16,19` | `scripts/fetch-postgres.mjs`, `--config tests/e2e/tauri.e2e.conf.json` | unchanged |
| `tests/e2e/flow.e2e.ts:6` | `REPO_ROOT/tests/fixtures/external/outlook.pst` | unchanged |
| `tests/e2e/support.ts:4` | comment `src/api/dialog.ts` | → `frontend/api/dialog.ts` |
| `tests/e2e/tsconfig.json:11` | `../../wdio.conf.ts` | unchanged |
| `tests/fixtures/external/NOTICE:1` | `tests/fixtures/external/outlook.pst` | → `app/tests/fixtures/external/outlook.pst` |

### Root meta

| File:line | Action |
|---|---|
| `CLAUDE.md:1` | `run from src-tauri/` → `run from app/backend/` |
| `CLAUDE.md:16` | `tests/e2e/build.mjs` → `app/tests/e2e/build.mjs` |
| `CLAUDE.md:17` | `outlooker-loadgen/tests/perf.rs` (crate-relative) unchanged |
| `CLAUDE.md:29` | `scripts/cargo-test-runner.mjs` → `app/scripts/...` |
| `CLAUDE.md:33` | `tests/cluster_lifecycle.rs` (crate-relative) unchanged |
| `CLAUDE.md:35` | `scripts/tmp/` → `app/scripts/tmp/` |
| `CLAUDE.md:37` | `scripts/vitest-capped.mjs`, `scripts/cargo-test-runner.mjs`, `scripts/test-time-judge.mjs` → `app/scripts/...`; `vite.config.ts` → `app/vite.config.ts`; `src-tauri/.cargo/config.toml` → `app/backend/.cargo/config.toml`; add "`npm test` runs from `app/`" |
| `README.md:6` | `mise.toml` → `.config/mise.toml`; `src-tauri/rust-toolchain.toml` → `app/backend/rust-toolchain.toml` |
| `README.md:7,13,14` | commands: prefix `cd app &&` (or `scripts/`→`app/scripts/` for build.py); one e2e group: `cd app && mise exec -- node tests/e2e/run.mjs --spec tests/e2e/flow.e2e.ts` |
| `README.md:9-12` | npm commands: say "run from `app/`" |
| `README.md:18,19` | → `docs/specs/2026-10-21-outlooker-design.md`, `docs/mockups/variant-1/NOTES.md` |
| `.gitignore:1` | `scripts/tmp/` → `app/scripts/tmp/` (a pattern with an inner `/` is anchored to the .gitignore dir) |
| `.gitignore:6,7,8` | `src-tauri/...` → `app/backend/...` (`target/`, `resources/postgresql/`, `gen/`) |
| `.gitignore:2-5` | `node_modules/`, `target/`, `dist/`, `.DS_Store` unanchored → unchanged |

### Docs cross-links (live docs only, not `docs/logs/`)

| File:line | Action |
|---|---|
| `docs/design/import-sync/A-NOTES.md:3`, `B1.html:34`, `B2.html:34` | `docs/design/variant-1` → `docs/mockups/variant-1` |
| `docs/design/variant-1/NOTES.md:4` | `docs/superpowers/specs/...` → `docs/specs/...` |
| `docs/superpowers/specs/2026-10-24-pst-scheduled-sync-design.md:4` | `docs/design/import-sync/` → `docs/mockups/import-sync/` |
| `docs/superpowers/specs/2026-10-21-outlooker-design.md` (34 hits incl. the §~410 repo tree) and `2026-10-23-ui-density-and-resize-design.md:15-16` (`scripts/tmp/...` mockups that are not in git) | leave as is: they describe the layout at the time of writing. Recommend acceptance 6 excludes `docs/` except the cross-links above |

External (outside the repo, lead/user action, not a work package): `~/.claude/projects/-Users-jhangyu-project-outlooker/memory/outlooker-test-env-traps.md` (mentions `scripts/cargo-test-runner.mjs`, `src-tauri/.cargo/config.toml`, `src-tauri/resources/postgresql/bin/pg_ctl`, `scripts/tmp/e2e`, "Worktree `tmp/` is NOT gitignored (only `scripts/tmp/`)", `cargo build --release` from `src-tauri/`) and `MEMORY.md` index line.

## 4. Root-only-README: how each root tool config is placed

| Item | Placement | Evidence |
|---|---|---|
| package.json, package-lock.json, node_modules | `app/` (package root) | npm runs scripts with cwd = package dir; everything Node lives below it |
| index.html, vite.config.ts, tsconfig.json, wdio.conf.ts | `app/` | vite root defaults to cwd = `app/`; `vitest-capped.mjs:11` loads `vite.config.ts` from cwd |
| mise.toml | `.config/mise.toml` | mise 2026.10.17 binary lists `.config/mise.toml`, `.config/mise/config.toml`, `.mise.toml` among project config names (`strings ~/.local/bin/mise`). mise walks up from cwd, so `app/` subdirs get node 24.22.1. Trust: `~/.local/state/mise/trusted-configs/project-outlooker-…` is a symlink to the repo dir (dir-level trust); whether it covers `.config/mise.toml` is NOT verified (my probe in a temp dir was denied). Check: `mise config ls` from the repo root lists `~/project/outlooker/.config/mise.toml` with no trust error, and `mise exec -- node --version` from `app/` prints `v24.22.1` |
| CLAUDE.md | `.claude/CLAUDE.md` | Claude Code binary contains `.claude/CLAUDE.md` (23 occurrences, `strings $(readlink -f $(command -v claude))`); documented as an equivalent project memory location. Memory dir key is the cwd path, unchanged |
| outlooker-logo.svg | `docs/brand/` | not referenced by code or config; the shipped mark is `src/components/brand/mark.svg` (differs from the logo, `cmp`) |
| rust-toolchain.toml | `app/backend/` (moves with workspace) | rustup reads it walking up from cargo's cwd = `app/backend` |
| Agent scratch `scripts/tmp/` | `app/scripts/tmp/` | code already writes there relative to ROOT (`fetch-postgres.mjs:78`, `prepare.mjs:9`). Note: `~/.claude/hooks/global-tmp-guard:44` suggests the first existing of `scripts/tmp`, `tmp`, `.claude/tmp` under the project dir, so after the move it suggests `.claude/tmp`. Option: add `.claude/settings.json` with `{"env":{"CLAUDE_PROJECT_TMP_DIR":"app/scripts/tmp"}}` (hook line 42 honours it). Lead decides; not required by the contract |

## 5. Work packages

Rules for every package: `mkdir -p app docs/brand .claude .config` before any `git mv` (git mv fails on a missing parent); never `git mv` onto an existing destination (it would nest the source inside it); `git mv` pathspecs list old and new paths; commit with `git commit -- <old paths> <new paths>`; no stash/reset/checkout --/clean/`add -A`. No cargo/npm/python builds inside packages; builds run only in the lead-granted slot at round end.

Precondition (lead, before round 1): no running PostgreSQL with a data dir inside the repo (`pgrep -fl postgres` and match `-D` paths against `scripts/tmp` and `src-tauri`); no other session has the repo open for builds.

### Round 1: moves + reference fixes (5 packages, parallel edits, disjoint files)

| Pkg | Owns (old → new) | Edits | Mechanical acceptance |
|---|---|---|---|
| R1-A backend | `src-tauri/**` → `app/backend/**` | every row of §3 "Backend" | `git ls-files src-tauri \| wc -l` = 0; `git ls-files app/backend \| wc -l` = 1225; `ls app/backend/target app/backend/resources/postgresql/bin/pg_ctl` exist; `git grep -nE 'src-tauri\|(^\|[^/[:alnum:]])scripts/\|\.\./src/' -- app/backend ':!app/backend/native'` = 0 hits |
| R1-B frontend + shell | `src/**` → `app/frontend/**`; `index.html package.json package-lock.json vite.config.ts tsconfig.json wdio.conf.ts` → `app/`; `mv node_modules app/`; `rm -rf dist` | §3 "Frontend + package shell" | `git ls-files src index.html package.json package-lock.json vite.config.ts tsconfig.json wdio.conf.ts` empty; `git grep -nE '"/?src/' -- app/index.html app/vite.config.ts app/tsconfig.json` = 0; `test -x app/node_modules/.bin/vite` |
| R1-C scripts + tests | `scripts/**` → `app/scripts/**`; `tests/**` → `app/tests/**` | §3 "Scripts + tests" | `git ls-files scripts tests` empty; `git grep -n 'src-tauri' -- app/scripts app/tests` = 0; `git grep -nE '"src/\|src/api' -- app/tests` = 0; `python3 -m py_compile app/scripts/build.py app/scripts/vendor-libpff.py`; `node --check` on each `.mjs` |
| R1-D root meta | `CLAUDE.md` → `.claude/CLAUDE.md`; `mise.toml` → `.config/mise.toml`; edits `README.md`, `.gitignore` | §3 "Root meta" | `git ls-files CLAUDE.md mise.toml` empty; `git grep -nE 'src-tauri\|(^\|[^/[:alnum:]])(scripts\|tests)/\|docs/(design\|superpowers)\|(^\|[^.])mise\.toml' -- README.md .claude/CLAUDE.md .gitignore` = 0 (CLAUDE.md:17,33 crate-relative lines are written as `outlooker-loadgen/tests/perf.rs` and `outlooker-core/tests/cluster_lifecycle.rs` so the pattern does not hit); `git check-ignore app/scripts/tmp/x app/backend/target/x app/backend/gen/x app/backend/resources/postgresql/x` prints all 4 |
| R1-E docs | `docs/design/**` → `docs/mockups/**`; `docs/superpowers/specs/*` → `docs/specs/`; `outlooker-logo.svg` → `docs/brand/` | §3 "Docs cross-links" | `git ls-files docs/design docs/superpowers outlooker-logo.svg` empty; `git ls-files docs/mockups \| wc -l` = 11, `docs/specs` = 4; `git grep -nE 'docs/(design\|superpowers)' -- docs/mockups docs/specs/2026-10-24-pst-scheduled-sync-design.md` = 0 |

Ordering inside round 1: none needed. Destinations are disjoint (`app/backend`, `app/frontend` + `app/*` files, `app/scripts` + `app/tests`, `.claude` + `.config`, `docs/*`). R1-A edits `.cargo/config.toml` to `../frontend/...` without needing R1-B done.

Round-1 gate (lead slot, serial, after all 5 report): from `app/backend`: `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --workspace`, `cargo test --workspace` (split per target if over the foreground cap: `--lib`, `--test it`, …). From `app/`: `mise exec -- npm test`, `mise exec -- npm run build`. This is where ts-rs export (`TS_RS_EXPORT_DIR`) is proven: `git status --porcelain app/frontend/api/bindings` must be empty after `cargo test` (bindings regenerate identical).

### Round 2: gate fixes + full acceptance (4 packages, gates run serially in slots)

| Pkg | Owns | Does | Mechanical acceptance (contract item) |
|---|---|---|---|
| R2-A backend gate | `app/backend/**` | fix anything the round-1 cargo gate reported; run the warning-free release check `cargo build --release` from `app/backend` (app package only) | (2) all three cargo commands RC 0; release log `grep -c '^warning:'` = 0 (5) |
| R2-B frontend gate | `app/frontend/**`, `app/*.{json,ts,html}` | fix npm gate failures | (3) `npm test` RC 0, `npm run build` RC 0 from `app/` |
| R2-C desktop + e2e | `app/scripts/**`, `app/tests/**` | `mise exec -- npm run e2e:build` from `app/`, then `python3 app/scripts/build.py` from the repo root; fix | (4) e2e:build RC 0 and `app/backend/target/debug/outlooker` exists; (5) build.py RC 0 and prints a `.app`/`.dmg` under `app/backend/target/release/bundle` |
| R2-D acceptance sweep | `README.md`, `.claude/**`, `.config/**`, `.gitignore`, `docs/**` (not `docs/logs/`) | run items 1, 6, 7, 8; fix stragglers in its own files, report hits in others to the owner | (1) `find . -maxdepth 1 -type f ! -name '.*'` = `./README.md`; (6) `git grep -nE 'src-tauri\|(^\|[^/[:alnum:]_-])(scripts\|tests)/(build\|fetch\|vendor\|cargo-test\|vitest\|test-time\|fixtures\|tmp\|e2e)\|"/src/\|src/test/setup\|src/api/bindings\|docs/(design\|superpowers)\|outlooker-logo\.svg' -- ':!docs/logs' ':!docs/specs/2026-10-2[13]*' ':!app/backend/native'` = 0; (7) `claude -p "Quote the first heading of the project instructions"` run from the repo root returns `Cargo invocation rule (run from \`app/backend/\`)`; `mise exec -- node --version` from `app/` = `v24.22.1`; (8) `git status --porcelain` empty; `git log --follow --oneline app/backend/Cargo.toml \| wc -l` > 1 (history follows) |

Slot order for round 2: R2-A cargo → R2-B npm → R2-C e2e:build → R2-C build.py (each compiles; never two at once). R2-D's checks need no compile and can run any time after R2-A..C commit; its item 8 runs last.

## 6. Risks and unverified items

1. `git mv <dir>` moving ignored content (`target/`, `resources/postgresql/`, `gen/`, `scripts/tmp/`) along: I expect a directory rename, not verified on this git version. If the old dirs remain, `mv` them by hand (same filesystem, instant) before the gate; never `rm` `resources/postgresql` (refetch needs network).
2. Cargo will rebuild workspace crates after the move (path fingerprints, and `crates/pff/build.rs` emits an absolute `rerun-if-changed`, see memory note), so the first round-1 cargo command includes the >40 s libpff C compile. Run `cargo build --workspace` first in its own 150 s slot to warm, then clippy and test.
3. mise trust for `.config/mise.toml`: dir-level trust symlink exists, coverage not verified. Fallback: user runs `mise trust .config/mise.toml` once.
4. `.claude/CLAUDE.md` loading: verified only by string presence in the binary, not by a live session. R2-D item 7 proves it.
5. Tauri CLI behaviour (search down for `tauri.conf.json`, `beforeBuildCommand` cwd = parent of the tauri dir): relationship between `app/` and `app/backend/` equals today's root/`src-tauri/`, so I expect no change; context7 quota was exhausted, so not doc-verified. The round-1 gate does not cover it; R2-C does.
6. `scripts/test-time-judge.mjs`, `cargo-test-runner.mjs` contain no path literals except comments (grep). Not re-read in full.
7. Windows (user's box over SSH): `build.py`, `fetch-postgres.mjs`, e2e paths use `Path`/`path.join`, so separators are fine; Windows run not part of this contract.
8. Identifier `REPO_ROOT` in `import_pipeline.rs:33` and `prepare.mjs:8` now means `app/`. Renaming is out of scope; comments clarify.
9. `docs/specs/2026-10-21-outlooker-design.md` keeps 34 old-path mentions as history. If the lead wants acceptance 6 to cover `docs/specs`, add a package to rewrite its §~410 tree (scope growth; ask the user).
