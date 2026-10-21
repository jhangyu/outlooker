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

Commits reviewed: f22cae5 (vendor), 9dc6196 (FFI wrapper, fixture, generator).

Pass 1 (f22cae5, 9dc6196):
- Ownership: f22cae5 = 1076 paths under `src-tauri/native/libpff/` + `scripts/vendor-libpff.sh`; 9dc6196 = `crates/pff/{build.rs,src/*,tests/outlook_pst.rs}`, `scripts/fixtures/make-pst.ps1`, `tests/fixtures/external/{NOTICE,outlook.pst}`. All R1-02. OK.
- C5 Produces (foundation plan R1-02): every listed signature present in `crates/pff/src/lib.rs` (version, Error, Result, check_signature, File::{open,root_folder,message_store}, Item::{identifier … attached_item}, AttachmentType, filetime_to_unix_micros) and `src/tags.rs` (all 20 tags, values match). Only platform branch is `#[cfg(windows)]` UTF-16 open (lib.rs:112-135, 147-156), which the Stage 1 Behavior defines. OK.
- build.rs: copies the tree to `$OUT_DIR/libpff-src`, Unix `sh configure --disable-nls --disable-shared-libs --with-zlib=no` in the copy, MSVC defines `_CRT_SECURE_NO_WARNINGS` / `UNICODE` / `WINVER=0x0501`, `HAVE_LOCAL_<LIB>` for the 17 libs, no zlib. Matches spec Part 01 §4.3. OK.
- Acceptance (my runs):
  - `grep -q bb5bcc0 src-tauri/native/libpff/ORIGIN` + three `test -f`: RC=0.
  - `cargo test -p pff --locked`: RC=0; `version_is_20260625`, `non_pst_is_error`, `outlook_pst_folders`, `outlook_pst_walk_is_deterministic`, `outlook_pst_welcome_message`, `filetime_conversion` ok; doctest `lib.rs - (line 4) - compile fail ... ok`.
  - `otool -L target/debug/deps/outlook_pst-8158793779263bfb`: only `/usr/lib/libSystem.B.dylib`; libz count 0.
  - `grep -rn libpff_set_codepage src-tauri/crates`: RC=1.
  - NOTICE contains `CC-BY-4.0` and `c3631ff`; `shasum -a 256` of `tests/fixtures/external/outlook.pst` equals `scripts/tmp/outlook.pst` (df01707f…4e24).
  - `pwsh -NoProfile -Command "[ScriptBlock]::Create(...)"`: not runnable, `zsh: command not found: pwsh` (RC=127). Instrument missing, not a pass. Sent to team-lead for a decision (install PowerShell or move the check to the Windows phase).
- Findings:
  - [blocker] scripts/fixtures/make-pst.ps1:38 — `$ns.AddStoreEx($path, 3)   # olStoreUnicode`: Outlook `OlStoreType` is olStoreDefault=1, olStoreUnicode=2, olStoreANSI=3, so 3 creates ANSI (97-2002) PSTs, against spec Part 04 §12.1 (`AddStoreEx(path, olStoreUnicode)`). ANSI stores keep strings in a codepage, which invalidates the cjk.pst charset / trigram fixtures. Fix: change `3` to `2`. Sent to team-lead.

## R1-03 — NOT STARTED

Commits reviewed: none yet.

## R1-04 — NOT STARTED

Commits reviewed: none yet.

## R1-05 — NOT STARTED

Commits reviewed: none yet.

## R1-06 — IN REVIEW

Commits reviewed: 11e381e (C10 shared contract, locales, test helpers).

Pass 1 (11e381e, partial task):
- Ownership: `src/app/{AppStatusContext.tsx,i18n.ts,testing.tsx}`, `src/components/common/*`, `src/locales/{zh-TW,en}/{common,import,onboarding,settings}.json`. All R1-06. OK.
- C10: `EmptyState` props (EmptyState.tsx:4-10), `formatAppError(err, t)` (formatAppError.ts:10), `useAppStatus()` (AppStatusContext.tsx:68) present with the §2.2 / frontend-plan shapes; `useLanguageSwitch` calls `changeLanguage` then `settingsUpdate({ language, webpQuality: null })` (hooks.tsx:23-36) as planned.
- Locale parity (python key-set diff over the working tree): common 41/41, import 53/53, onboarding 36/36, settings 57/57 equal; no key containing pause / worker / re-encode / format / highlight / drag / drop.
- Typecheck not judged yet: depends on R1-01 bindings and `src/api/call.ts` (`isAppError`), not yet on main. Full acceptance re-run at the final R1-06 commit.

## R1-07 — NOT STARTED

Commits reviewed: none yet.

## Parking lot

- [nit] R1-01 bc20348 commit message — "registry re-check deferred to cargo and npm resolution" records no registry check (Global Constraints "Versions"). All pins equal Part 01, so no wrong version resulted.
- [nit] R1-01 bc20348 — `npm run typecheck` fails at the kickoff commit (stubs import `src/api/bindings/*`, generated in the contract commit; stated in the commit message). Foundation plan R1-01 Behavior says typecheck passes after the kickoff. Transient; re-checked at the final R1-01 commit.
- [should-fix] src-tauri/crates/pff/build.rs:31 — the `$OUT_DIR/libpff-src/.prepared` marker is never invalidated: after a re-vendor (`ORIGIN` changes, build.rs reruns) the stale copy is compiled. Fix: write the `ORIGIN` content into `.prepared` and re-copy when it differs.
- [should-fix] src-tauri/crates/pff/src/lib.rs:388-394 — `attachment_data` returns `Ok(Some(buf))` truncated when `libpff_attachment_data_read_buffer` returns 0 before `size` bytes are read; a short read looks like success (silent data loss). Fix: return `Err` when `filled < size`.
- [nit] scripts/fixtures/make-pst.ps1:29-31 — PT_SYSTIME read-back is compared as strings (`"$back" -ne "$value"`); Outlook may return the time with a different DateTimeKind / local offset, which would throw a false "Outlook did not keep property". Fix: compare `[datetime]` values via `.ToUniversalTime()` for 0x0040 tags. Windows-phase risk only.
- [nit] scripts/fixtures/make-pst.ps1:194-200 — corrupt.pst zeroes 512 bytes at 60 % of the file; spec Part 04 §12.1 wants "one item's data block". The R2-09 test pre-registers both outcomes, so this is acceptable; a comment noting that the offset is not proven to be inside an item block would help.
