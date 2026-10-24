# CI build contract (frozen 2026-10-24; only the user may change it)

## End state
A Python CI build script compiles the Outlooker desktop app for Linux, macOS and Windows; a guard proves the CI path runs no tests; the build was run green locally (Linux amd64 container, macOS host, Windows host over SSH); the code is pushed to a new public GitHub repo `jhangyu/outlooker`; GitHub Actions builds all three platforms green; and pushing a `v*` tag publishes a GitHub Release with each platform's bundles.

## User decisions
- Repo: new **public** repo `jhangyu/outlooker` (gh account jhangyu).
- Release: only on `v*` tag push. Plain pushes build and upload workflow artifacts only.
- Windows host `User@192.168.26.209` (`ssh -i ~/.ssh/id_ed25519_win`, default shell cmd.exe, use `powershell -Command` with `;`): members may install rustup (per `app/backend/rust-toolchain.toml`) and mise (Node 24.22.1) in user scope. Do not modify Visual Studio. Work dir `C:\Users\User\outlooker-ci`.
- Linux local check: Docker `--platform linux/amd64` (emulated on this arm64 Mac), same as the GitHub ubuntu runner.

## In scope
1. Python CI build entry under `app/scripts/` (extend the existing `app/scripts/build.py`; one build module, no parallel second build path).
2. A test guard: a Python check that fails when the CI path (workflow files + CI build script) invokes any test.
3. `.github/workflows/` workflow: 3-platform matrix build, artifact upload, release job on `v*` tags.
4. Local verification: Linux amd64 container, macOS host, Windows host via SSH (harness lives in gitignored scratch `app/scripts/tmp/`).
5. Push to GitHub, run the CI loop until green, tag-triggered release verified.

## Out of scope
- Code signing / notarization (bundles ship unsigned).
- Running any test (cargo test, npm test, vitest, e2e, perf) in CI.
- Changes to app source code, except a minimal fix that a platform build proves is required (report to lead first).

## Acceptance criteria
1. `python3 app/scripts/build.py --ci --out <dir>` (exact flags fixed by the build-script owner and recorded below) builds release bundles and copies them into `<dir>` on Linux, macOS and Windows.
2. `python3 app/scripts/ci_guard.py` exits 0 on the committed tree, and exits non-zero (red observed) when a test command is injected into a workflow file or into the CI build script.
3. The workflow runs the guard before building, builds on ubuntu, macos and windows runners, uploads each platform's bundles, and on a `v*` tag creates a GitHub Release with all bundles attached. No step runs a test.
4. Local green builds with evidence records (command, self-captured RC, bundle file list, code state): Linux amd64 container, macOS host, Windows host.
5. Before the first push: secret scan of the tree to be published (no credentials, tokens, private keys); repo created public; pushed.
6. CI green on all three platforms for the pushed commit (run URL recorded).
7. A `v*` tag run is green and the GitHub Release lists bundles for all three platforms (release URL recorded).

## CI loop rule (from the user's Halcyon CI loop spec)
Main agent does not fix or debug; it only checks CI status and assigns work. CI not green → one **opus** debugger confirms the root cause → a **different opus** implementer writes the fix and re-pushes. Main agent runs `date` and checks the run every **8 minutes** (never longer). The same error appearing a third time → stop and report the failure trace to the user.

## Round budget
2 rounds (round 1: build script, guard, workflow, Windows toolchain, Linux container base; round 2: three local green builds, push, CI loop, release). Each round may get at most 2 review→fix cycles.

## Interface (filled by build-script owner)
Frozen by lead before round 1 so all members can work in parallel:
- CLI: `python3 app/scripts/build.py --ci --out <dir>` (Windows: `python app\scripts\build.py --ci --out <dir>`), run from the repo root or any cwd.
- `--ci` means: on Linux, install the Tauri system packages with apt (`sudo` when not root); always `npm ci`; release build (no `--debug`); never run tests.
- `--out <dir>`: after the build, copy each bundle file into `<dir>` flat (create it): `.dmg`, `.app` (as `.app.tar.gz`), `.msi`, NSIS `-setup.exe`, `.deb`, `.rpm`, `.AppImage`. Print each copied path. Exit non-zero when zero bundles were copied.
- Prerequisites the caller provides: `mise` on PATH (Node from `.config/mise.toml`), rustup/cargo on PATH (toolchain from `app/backend/rust-toolchain.toml`), Python 3.

## Parking lot
(empty)

## Round log
- Round 1 (build script, guard, workflow, Windows toolchain, Linux container base): all signed off; reviewer verdict CONFIRMED, no blocker (`app/scripts/tmp/ci/review-round1.md`). The 4 should-fix findings (macOS `python`, release re-run, guard misses npm/cargo aliases, `--out` mixing debug and release bundles) all hit acceptance criteria 1-3 and 7, so the round 2 fix step takes them before the real builds.
