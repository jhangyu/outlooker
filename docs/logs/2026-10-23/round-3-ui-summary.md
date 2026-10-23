# Round 3 (final) — squad `ui` summary

Branch `techdebt/r3-ui`, base `bf65fcd`. Plan: `round-3-ui-plan.md`.

## Commits

| commit | items |
|---|---|
| 1fd0e20 | plan |
| e43ddd8 | U7, U4, U12, U21, D11 revert |
| 2bcd878 | U2, U3, U5, U8, U9 comment, U15, U19, U20, A7 (ui half) |

## Gate (r3-ui-test-haiku at 2bcd878, 2026-10-23T03:31:51Z–03:32:49Z, `tmp/gate-r3.log` + `tmp/g4-*.log`, untracked)

- `mise exec -- npm run build` RC=0.
- `mise exec -- npm test` RC=0, 17 files / 72 tests (72 − 2 deleted (U19, U20) + 2 new (U2 eviction, U3 cache)).
- `mise exec -- npx tsc --noEmit -p tests/e2e` RC=0.
- Acceptance greps: all as expected. One check hit something else: `fixtures.ts` `R1-07|hadRO|ResizeObserver` count 1. The hit is the doc comment at `fixtures.ts:6`, which describes how virtual-core uses ResizeObserver. The stub and the `R1-07` and `hadRO` lines are gone, so the handoff criterion (`R1-07|hadRO` → 0) holds.
- `vendor-libpff.py`: `--help` RC=0. No argument → RC=2. No `.sh` references remain. package-lock.json is unchanged since bf65fcd.
- Red-first: the U2 eviction test failed without the effect ("expected 2 to be 1") and passed with it (impl-2 report).

## Known limitations

- `vendor-libpff.py` uses `tarfile.extractall(filter="data")`. This needs Python ≥ 3.12, or a 3.8–3.11 security release that backports the filter. The `gh run download` path was not run because it needs the network and a live run id. The local-tarball path was run against a temp `--dest`: ORIGIN had 6 keys in order, and `diff -r` was empty.
- `src-tauri/crates/pff/build.rs:27` still has to name `vendor-libpff.py`. That file belongs to the core squad.
- Round 1 limitations are unchanged: the scroll restore in the real webview is checked by hand only. The undo history in Advanced SQL resets when you switch screens.

## Final ledger (every U/D/A item owned by ui)

| item | status | commit / reason |
|---|---|---|
| U1 keep screen state | fixed r1 | cdde7dc, 73e2d15, 9bd59bf, a9b32da |
| U2 query memory | fixed r3 (superseded `searchMail` entries evicted, `mailGet` gcTime 30 s) | 2bcd878. The `searchCount` part is not worth fixing: each entry is one number. `gcTime: 0` was refuted because it breaks U1 (hidden Activity screens unsubscribe). |
| U3 Intl formatter cache | fixed r3 | 2bcd878 |
| U4 enqueue hook | fixed r3 | e43ddd8 |
| U5 leaf budget constants | fixed r3 | 2bcd878. `COUNT_CAP`/`MAX_ROWS` mirror Rust caps: not worth fixing. A shared source would need a contract change. |
| U6 hand-rolled seq | not worth fixing | It works and is guarded by `search.test.tsx`. The U2 eviction keys on it. |
| U7 query keys single source | fixed r3 | e43ddd8 |
| U8 duplicate ResizeObserver stub | fixed r3 | 2bcd878 |
| U9 eslint comment | fixed r3 | 2bcd878. The behavior part (undo lost on a language switch) is not worth fixing: the event is rare. |
| U10 plan-ID comments | fixed r1 + r3 | 7e02d16, cdde7dc. The last hit was removed with U8 in 2bcd878. |
| U11 test-only props | not worth fixing | Zero runtime cost. |
| U12 toAppError duplicate | fixed r3 | e43ddd8 |
| U13 pseudo-query stores | not worth fixing | They work and are tested through the move flow, the riskiest flow. |
| U14 casts in DslBuilder | not worth fixing | Internal casts in one component. |
| U15 dead barrel exports | fixed r3 | 2bcd878. `ROW_HEIGHT` refuted: ResultTable uses it. |
| U16 mutation mirror state | not worth fixing | It works and is tested. The refactor risk is greater than the value. |
| U17 WebP default 80 | not worth fixing | Removing it changes the loading UX for a value that has not drifted. |
| U18 smoke e2e cannot fail | fixed r2 (group deleted) | de9c18b |
| U19 tautology test | fixed r3 | 2bcd878 |
| U20 vocabulary test | fixed r3 | 2bcd878 |
| U21 test name | fixed r3 | e43ddd8 |
| D1 superseded plans | fixed r1 | 7e02d16 |
| D2 root CLAUDE.md sentence | orchestrator-owned | — |
| D3 handover doc | orchestrator-owned | — |
| D4+D5 spec status, stale refs | fixed r2 | 63acf37 |
| D6 spec layout | fixed r2 (+ script name r3) | 63acf37, 3425e6f, 2bcd878 |
| D7 NOTES exclusions | fixed r2 | 63acf37, ca04132 |
| D8 prototype dirs | fixed (untracked dirs deleted) | no commit |
| D9 README | fixed r2 | 63acf37, ca04132, 150978c |
| D10 docs/logs policy | orchestrator-owned | — |
| D11 in-app logo | fixed r1, then reverted in r3 by user decision (text "O" boxes restored, svg deleted) | cdde7dc → e43ddd8 |
| D12 requirements / variant-1 | refuted | The scan found no drift. |
| A4 unused plugin-log | fixed r1 | 7e02d16 |
| A5 narrow capabilities | fixed r2. Runtime proof: full e2e PASS at 1b13f56. | 63acf37 |
| A6 e2e typecheck | fixed r2 | de9c18b |
| A7 vendor-libpff.sh → .py | fixed r3 (ui half). `make-pst.ps1` and `outlook-fixtures` are kept by user decision (Windows phase). | 2bcd878 |
| A8 no `-p` in e2e prepare | fixed r2 | de9c18b |
| A15 dead `--self-test` | fixed r2 | 63acf37 |
| E1 stop the app's PG after each e2e group | fixed r2 | de9c18b |

Not owned by ui: A1–A3, A9–A14, A16, A17, C*.
