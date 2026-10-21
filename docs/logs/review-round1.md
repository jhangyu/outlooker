# Round 1 review log (R1-REV)

Reviewer: rev-r1. Plan: `docs/superpowers/plans/2026-10-21-outlooker-plan.md` §3, §5; procedure `docs/superpowers/plans/2026-10-21-outlooker-plan-foundation.md` "R1-REV / R2-REV steps"; spec `docs/superpowers/specs/2026-10-21-outlooker-design.md`.
Finding format: `- [blocker|should-fix|nit] file:line — finding — evidence`. Verdict words appear only in section headings.

## R1-01 — IN REVIEW

Commits reviewed: bc20348 (kickoff).

Pass 1 (bc20348):
- Ownership: all 108 paths are R1-01 rows or kickoff stubs of §1 / §5 (`git show --stat bc20348`). OK.
- Versions: `cargo tree -p outlooker-core --depth 1 -e normal --locked --offline` RC=0; every Part 01 §5 / §7 / §8 crate at its pin (ammonia 4.2.4, base64 0.23.1, blake3 1.8.7, chardetng 1.0.0, deadpool-postgres 0.14.2, encoding_rs 0.8.42, getrandom 0.4.3, html2text 0.17.4, image 0.25.10, log 0.4.34, rayon 1.12.0, refinery 0.10.0, serde 1.0.229, serde_json 1.0.154, sysinfo 0.39.6, thiserror 2.0.21, tokio 1.54.0, tokio-postgres 0.7.18, ts-rs 12.0.1, webp 0.3.1). App crate: tauri =2.12.5, tauri-build =2.7.4, dialog =2.8.3, log =2.10.1, single-instance =2.5.4, wdio-webdriver =1.5.0 optional behind feature `e2e`. `grep -nE '"[\^~]' package.json` RC=1.
- Toolchain: `rustc --version` in src-tauri prints `rustc 1.99.1`; `mise exec -- node --version` prints `v24.22.1`; `engines.node` is `>=24.22 <25`. OK.
- Forbidden deps: none in `Cargo.lock` (sqlx, bindgen, testcontainers, postgresql_embedded, tauri-specta, mail-parser: grep RC=1). npm `extract-zip` / `tar-stream` are transitive only (`npm ls`: @wdio/cli via @puppeteer/browsers, webdriverio via archiver), not used by the fetch script. OK.
- No-dark grep over package.json, src, tauri.conf.json: RC=1. OK.
- tauri.conf.json: identifier `us.jhangy.outlooker`, no `bundle.resources`, CSP `script-src 'self'`, `img-src 'self' data: mailimg: http://mailimg.localhost`, `frame-src 'self'` (spec Part 03 §5.6). Capabilities: `dialog:allow-open`, `log:default`. OK.
- Findings:
  - [blocker] package.json:31-46 — `@wdio/local-runner` and `@wdio/spec-reporter` (9.32.0; spec Part 01 §9 "Required" / "declare it directly") are missing; plan §2.1 item 2 requires every Part 01 §9 npm dependency pinned in the kickoff. Evidence: `grep -c 'local-runner\|spec-reporter' package.json` prints 0. Fix: `mise exec -- npm install --save-dev --save-exact @wdio/local-runner@9.32.0 @wdio/spec-reporter@9.32.0`, commit package.json and package-lock.json. Sent to team-lead.

## R1-02 — IN REVIEW

Commits reviewed: f22cae5 (vendor).

Pass 1 (f22cae5):
- Ownership: 1076 paths under `src-tauri/native/libpff/` plus `scripts/vendor-libpff.sh`; all R1-02. OK.
- `src-tauri/native/libpff/ORIGIN` has `commit=bb5bcc0`, `run_id=32393676269` and the sdist sha256; `include/libpff.h`, `common/types.h`, `libpff/libpff_definitions.h` exist. Vendored before the 2026-11-18 artifact expiry. OK.
- `scripts/vendor-libpff.sh`: `gh run download <run> -R jhangyu/libpff -n cibw-sdist`, system tar, no autotools, writes ORIGIN. OK.

## R1-03 — NOT STARTED

Commits reviewed: none yet.

## R1-04 — NOT STARTED

Commits reviewed: none yet.

## R1-05 — NOT STARTED

Commits reviewed: none yet.

## R1-06 — NOT STARTED

Commits reviewed: none yet.

## R1-07 — NOT STARTED

Commits reviewed: none yet.

## Parking lot

- [nit] R1-01 bc20348 commit message — "registry re-check deferred to cargo and npm resolution" records no registry check (Global Constraints "Versions"). All pins equal Part 01, so no wrong version resulted.
- [nit] R1-01 bc20348 — `npm run typecheck` fails at the kickoff commit (stubs import `src/api/bindings/*`, generated in the contract commit; stated in the commit message). Foundation plan R1-01 Behavior says typecheck passes after the kickoff. Transient; re-checked at the final R1-01 commit.
