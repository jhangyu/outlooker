# Round 3 (final) — squad `ui` plan

Worktree `/Users/jhangyu/project/outlooker-wt/r3-ui`, branch `techdebt/r3-ui`, base `bf65fcd`.
Contract: `contract-techdebt.md` (end state: no high-ROI debt left). Binding brief: `round-brief.md` (incl. the D11 revert).

## Baseline (bf65fcd, 2026-10-23T03:26Z, logs `tmp/base-*.log`, untracked)

- `mise exec -- npm run build` RC=0 (includes the e2e typecheck).
- `mise exec -- npm test` RC=0, 16 files / 72 tests.
- `mise exec -- npx tsc --noEmit -p tests/e2e` RC=0.

## Re-verification of the slice (current code)

| item | still present | evidence |
|---|---|---|
| U4 | yes | `import/index.tsx:129-140` and `onboarding/index.tsx:113-124` copy pick → `api.importEnqueue` → toast rejected → `toast.error`. Tails differ: `refresh()` vs `go("import")`. |
| U7 | yes | 12 literal keys: `boot.ts:27,28`, `App.tsx:41`, `AppFrame.tsx:156`, `import/index.tsx:122,123,128`, `importCache.ts:21,22`, `RemoveDialog.tsx:31`, `settings/index.tsx:72,98`, `MoveDialog.tsx:32,53`. `dbStatus` with `refetchInterval: 10_000` is defined twice. |
| U12 | yes | `call.ts:12-14` and `formatAppError.ts:5-7` both wrap into `{kind:"internal"}`. |
| U21 | yes | `settings.test.tsx:35`. |
| D11 revert | in scope | `cdde7dc` added `src/assets/outlooker-logo.svg` and `<img>` in `AppFrame.tsx:12` and `onboarding/index.tsx:15`. |
| U2 | partly | No `gcTime` anywhere. **Adjusted:** a plain `gcTime: 0` on `searchMail` would break U1. A hidden `<Activity>` screen unsubscribes its observers, so the current search would be dropped on each screen switch and the deep-scrolled infinite list would reload from page 1. Fix only the real leak: superseded `searchMail` entries (each new query gets a new `seq`) and `mailGet` bodies. `searchCount` entries are one number each → not worth fixing. |
| U3 | yes | `display.ts:9,20,25,29` and `MailList.tsx:33` construct `Intl` formatters on every call. |
| U5 | yes | `search/index.tsx:111` `50 - 4 * terms`, `compose.ts:55,57` repeat `4`. |
| U8 | yes | `fixtures.ts:15-24` (`hadRO`, R1-07 comment). `setup.ts:11-17` installs the stub globally. |
| U9 comment | yes | `SqlEditor.tsx:91`. |
| U15 | adjusted | `ROW_HEIGHT` IS used through the barrel (`advanced-sql/ResultTable.tsx:7`) → keep. Drop `isComplete`, `localDayStart` (dsl) and `loadedRows`, `PREFETCH_ROWS` (mail). |
| U19 | yes | `sql-editor.test.tsx:17-21`. |
| U20 | yes | `global.test.tsx:101-~125` ("no out-of-scope controls"). Keep `:92` dark-theme test. |
| A7 | yes | `scripts/vendor-libpff.sh` hardcodes run id `32393676269`. References: `scripts/build.py:39`, spec `:437`, `src-tauri/crates/pff/build.rs:27` (core squad). |

## Packages (exclusive files; nobody else edits them this round)

Counting rule: P1 = 1 judgment item + 3 mechanical groups; P2 = 1 judgment item + 3 mechanical groups.

### Package 1 — `r3-ui-impl-1-sonnet` (sonnet): query keys, enqueue hook, error wrap, logo revert

Files: `src/api/queries.ts` (new), `src/api/call.ts`, `src/components/common/{hooks.tsx,formatAppError.ts,index.ts,common.test.tsx}`, `src/app/{App.tsx,boot.ts,AppFrame.tsx,AppFrame.test.tsx}`, `src/assets/outlooker-logo.svg` (delete), `src/screens/import/**`, `src/screens/onboarding/**`, `src/screens/settings/**`.

1. **U7 (judgment).** `src/api/queries.ts` exports `queryOptions` objects (TanStack `queryOptions`, check the locked `@tanstack/react-query` version with context7) for `pstList`, `dbStatus` (with `refetchInterval: 10_000`) and `moveProgress` (keep its current pseudo-query options exactly: `enabled:false`, `queryFn: () => null`, `initialData` as today). Every `useQuery`, `getQueryData`, `setQueryData`, `invalidateQueries` site uses `<x>.queryKey` or the options object. Do not touch `moveSession.ts` (U13, not worth fixing).
   - A: `grep -rn '\["pstList"\]\|\["dbStatus"\]\|\["moveProgress"\]' src | grep -v '\.test\.' | grep -v src/api/queries.ts` → no output.
   - A: `grep -c "refetchInterval" -r src --include='*.ts*'` total across non-test files = 1 (in `queries.ts`).
2. **U4 + U12 (mechanical).** `useEnqueuePst()` in `common/hooks.tsx`: pick → `api.importEnqueue` → toast each rejected row (`import:enqueue.rejected`) → `toast.error` on throw; returns `Promise<boolean>` (true when the enqueue call resolved). Import screen: `if (await enqueue()) await refresh()` keeping the refresh error path under `toast.error`. Onboarding: `if (await enqueue()) go("import")`. Move `toAppError` into `src/api/call.ts`; `call` throws `toAppError(e)`; `common/index.ts` re-exports it from `../../api/call`; `formatAppError.ts` no longer defines it.
   - A: `grep -c "importEnqueue\|pickPstFiles" src/screens/import/index.tsx src/screens/onboarding/index.tsx` → 0 each.
   - A: `grep -rc "function toAppError" src | grep -v ":0"` → exactly `src/api/call.ts:1`.
   - A: import and onboarding tests unchanged and green.
3. **D11 revert (mechanical).** Restore the pre-`cdde7dc` logo: `git show cdde7dc^:src/app/AppFrame.tsx` and `git show cdde7dc^:src/screens/onboarding/index.tsx` are the source for the `.logo` style and markup only (text "O", `aria-hidden`; title bar `colorNeutralBackground1` + `colorBrandForeground1`; onboarding `colorBrandBackground` + `colorNeutralForegroundOnBrand`). Remove `logoImg` styles and the svg imports. `git rm src/assets/outlooker-logo.svg`. Keep every U1 change from `cdde7dc` (Activity, AppFrame.test.tsx U1 test). Do not touch the untracked root `outlooker-logo.svg`.
   - A: `grep -rn "outlooker-logo\|logoImg\|<img" src` → no output. `test -e src/assets/outlooker-logo.svg` RC=1. `test -e outlooker-logo.svg` (root, in main tree) unchanged.
   - A: `git diff cdde7dc^ HEAD -- src/app/AppFrame.tsx src/screens/onboarding/index.tsx | grep -i "logo"` → no output (logo regions byte-identical to before cdde7dc).
   - A: `grep -c "Activity" src/app/AppFrame.tsx` ≥ 1 (U1 kept).
4. **U21 (mechanical).** Rename `settings.test.tsx:35` to `"maintenance button runs the task"`.
   - A: `grep -c "maintenance button runs the task" src/screens/settings/settings.test.tsx` → 1.

### Package 2 — `r3-ui-impl-2-sonnet` (sonnet): memory, formatters, constants, sweep, script port

Files: `src/components/mail/**`, `src/components/common/{display.ts,display.test.ts (new)}`, `src/screens/search/**`, `src/components/dsl/**`, `src/components/sql-editor/**`, `src/app/global.test.tsx`, `scripts/vendor-libpff.sh` (delete), `scripts/vendor-libpff.py` (new), `scripts/build.py`, `docs/superpowers/specs/2026-10-21-outlooker-design.md`, `README.md`.

1. **U2 (judgment).** In `useMailPages` (`pages.ts`), after the query hook, evict superseded entries: `useEffect(() => { qc.removeQueries({ queryKey: ["searchMail"], predicate: (x) => x.queryKey[1] !== seq }) }, [qc, seq])` (or equivalent). In `ReadingPane.tsx` add `gcTime: 30_000` to the `mailGet` query. Do not set `gcTime: 0` on `searchMail`/`searchCount`, do not change `App.tsx` QueryClient defaults (P1 file).
   - A: new test in `pages.test.tsx`: render the hook with seq 1 then seq 2 (different query) → `qc.getQueryCache().findAll({ queryKey: ["searchMail"] }).length === 1` and its key has seq 2. Seen red without the effect, green with it.
   - A: existing U1 tests stay green (`AppFrame.test.tsx`, `search.test.tsx`, `mail.test.tsx`).
2. **U3 + U5 (mechanical).** `display.ts`: one module `Map` cache keyed by `` `${kind}|${locale}|${JSON.stringify(options)}` `` returning `Intl.NumberFormat` / `Intl.DateTimeFormat`. Add `formatMonth(iso, lng)` there and use it in `MailList.tsx:33` (import from `../common/display` directly, `common/index.ts` is P1's). U5: export `LEAVES_PER_TERM = 4` from `compose.ts`, use it at `compose.ts:55,57`, and in `search/index.tsx:111` use `MAX_LEAVES - LEAVES_PER_TERM * terms - ...`.
   - A: `grep -c "new Intl" src/components/common/display.ts` ≤ 2 (one per constructor kind, inside the cache). `grep -rn "new Intl" src --include='*.tsx' | grep -v test` → no output.
   - A: new `display.test.ts`: same `(lng, options)` twice → the same formatter instance (or spy on `Intl.DateTimeFormat` called once for two `formatDay` calls).
   - A: `grep -n "50 - 4\|4 \* terms" src/screens/search/index.tsx src/screens/search/compose.ts` → no output.
3. **Sweep (mechanical).** U8: delete the `ResizeObserver` block and its restore line in `fixtures.ts` (keep the clientHeight/clientWidth stub). U9: drop only the `// eslint-disable-line ...` comment at `SqlEditor.tsx:91`. U15: drop `isComplete`, `localDayStart` from `dsl/index.ts` and `loadedRows`, `PREFETCH_ROWS` from `mail/index.ts` (keep `ROW_HEIGHT`). U19: delete the test at `sql-editor.test.tsx:17-21` (and now-unused imports). U20: delete "no out-of-scope controls" in `global.test.tsx` plus helpers/imports only it uses; keep "no dark theme tokens".
   - A: `grep -c "eslint" src/components/sql-editor/SqlEditor.tsx` → 0. `grep -c "R1-07\|hadRO\|ResizeObserver" src/components/mail/fixtures.ts` → 0. `grep -c "isComplete\|localDayStart" src/components/dsl/index.ts` → 0. `grep -c "loadedRows\|PREFETCH_ROWS" src/components/mail/index.ts` → 0. `grep -c "out-of-scope" src/app/global.test.tsx` → 0.
   - A: test count = 72 − 2 deleted + new tests (U2 1, U3 ≥1, P1 adds none) — reported exactly.
4. **A7 port (mechanical, line by line).** `scripts/vendor-libpff.py`, stdlib only (`argparse`, `subprocess`, `tarfile`, `hashlib`, `shutil`, `tempfile`, `pathlib`). Required positional argument: run id or path to `libpff_python-*.tar.gz`; no default, no hardcoded run id, no expiry date. Same behavior as each `.sh` line: `gh run download <id> -R jhangyu/libpff -n cibw-sdist -D <tmp>/dl`; extract; check the 3 header files exist; replace `src-tauri/native/libpff`; write `ORIGIN` with the same 6 keys in the same order (`repo, commit, run_id, artifact, sdist, sha256`); temp dir always removed. Docstring says how to get a new run id (re-run the fork's `build_wheels` workflow at bb5bcc0). Delete the `.sh` in the same commit; update `scripts/build.py:39` and spec `:437` to `vendor-libpff.py`; README if it names the script.
   - A: `test -e scripts/vendor-libpff.sh` RC=1. `python3 scripts/vendor-libpff.py --help` RC=0. `python3 scripts/vendor-libpff.py` (no arg) RC≠0. `grep -rn "vendor-libpff.sh" scripts docs/superpowers README.md` → no output. `grep -c "32393676269\|2026-11-18" scripts/vendor-libpff.py` → 0.
   - A: local-tarball dry check, without touching the real vendored tree: run the port with `--dest <tmpdir>` (add an optional `--dest`, default `src-tauri/native/libpff`) against a tarball built by `tar -czf` from the current `src-tauri/native/libpff` contents placed under a `libpff_python-x/` top dir → `<tmpdir>/ORIGIN` has the 6 keys and `run_id=local-tarball`; `diff -r` of the copied tree vs the source minus ORIGIN is empty. Must not write into `src-tauri/native/libpff`.
   - Cross-squad: `src-tauri/crates/pff/build.rs:27` text is the core squad's (orchestrator routes).

## Gate (test-runner `r3-ui-test-haiku`, after both implementers send SETTLED; window = zero writes)

1. `mise exec -- npm run build` RC=0.
2. `mise exec -- npm test` RC=0, file/test counts recorded.
3. `mise exec -- npx tsc --noEmit -p tests/e2e` RC=0.
4. All "A:" greps above, raw output recorded.
5. `git diff bf65fcd HEAD --stat -- package-lock.json` → empty.
Artifact: `tmp/gate-r3.log` (untracked), start/end timestamps and HEAD hash inside.

## Final ledger (every U/D/A item owned by ui)

Status after round 3 planning; the summary updates the round 3 commits.

| item | status | where |
|---|---|---|
| U1 keep screen state | fixed r1 | cdde7dc, 73e2d15, 9bd59bf, a9b32da |
| U2 query memory | r3 P2 (searchMail eviction + mailGet gcTime); `searchCount` part not worth fixing: one number per entry | — |
| U3 Intl formatter cache | r3 P2 | — |
| U4 enqueue hook | r3 P1 | — |
| U5 leaf budget constants | r3 P2. `COUNT_CAP`/`MAX_ROWS` mirror Rust caps: not worth fixing, no shared source across the IPC boundary without a contract change | — |
| U6 hand-rolled seq | not worth fixing: works, guarded by `search.test.tsx`; the U2 eviction now also keys on it | — |
| U7 query keys single source | r3 P1 | — |
| U8 duplicate ResizeObserver stub | r3 P2 | — |
| U9 eslint comment | r3 P2. Behavior part (undo lost on language switch): not worth fixing, rare event | — |
| U10 plan-ID comments | fixed r1 (7e02d16, cdde7dc); last hit `fixtures.ts` removed by U8 in r3 P2 | — |
| U11 test-only props | not worth fixing: zero runtime cost | — |
| U12 toAppError duplicate | r3 P1 | — |
| U13 pseudo-query stores | not worth fixing: works, tested through the move flow (riskiest flow) | — |
| U14 casts in DslBuilder | not worth fixing: internal casts in one component | — |
| U15 dead barrel exports | r3 P2 (`ROW_HEIGHT` refuted: used by ResultTable) | — |
| U16 mutation mirror state | not worth fixing: works and tested; refactor risk > value | — |
| U17 WebP default 80 | not worth fixing: removing it changes loading UX for a value that has not drifted | — |
| U18 smoke e2e cannot fail | fixed r2 (group deleted) | de9c18b |
| U19 tautology test | r3 P2 | — |
| U20 vocabulary test | r3 P2 | — |
| U21 test name | r3 P1 | — |
| D1 superseded plans | fixed r1 | 7e02d16 |
| D2 root CLAUDE.md sentence | orchestrator-owned (not ui) | — |
| D3 handover doc | orchestrator-owned (not ui) | — |
| D4+D5 spec status, stale refs | fixed r2 | 63acf37 |
| D6 spec layout | fixed r2 | 63acf37, 3425e6f |
| D7 NOTES exclusions | fixed r2 | 63acf37, ca04132 |
| D8 prototype dirs | fixed (untracked dirs deleted, no commit) | — |
| D9 README | fixed r2 | 63acf37, ca04132, 150978c |
| D10 docs/logs policy | orchestrator-owned (not ui) | — |
| D11 in-app logo | fixed r1 (cdde7dc), reverted by user decision → r3 P1 restores the text "O" boxes | — |
| D12 requirements / variant-1 | refuted: no drift found in the scan | — |
| A4 unused plugin-log | fixed r1 | 7e02d16 |
| A5 narrow capabilities | fixed r2, runtime proof full e2e PASS at 1b13f56 | 63acf37 |
| A6 e2e typecheck | fixed r2 | de9c18b |
| A7 vendor-libpff.sh → .py | r3 P2 (ui half). `make-pst.ps1` + `outlook-fixtures` kept by user decision (Windows phase) | — |
| A8 no `-p` in e2e prepare | fixed r2 (+ CARGO_TARGET_DIR) | de9c18b |
| A15 dead `--self-test` | fixed r2 | 63acf37 |
| E1 (r2 finding) stop app PG after each e2e group | fixed r2 | de9c18b |

Not ui-owned (core / orchestrator): A1–A3, A9–A14, A16, A17, C*.
