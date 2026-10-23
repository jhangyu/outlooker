# Test policy and recurrence prevention: design (2026-10-23)

Contract: `docs/logs/2026-10-23/contract-test-suite-pruning.md`, criteria 3, 4, 5a.
Scratch proofs: `scripts/tmp/policy-design/` (`vt/` = vitest, `rt/` and `ws/` = cargo). Nothing in the repo or in `~/.claude` was changed.

## 1. Root cause

Agents add micro tests because every instruction they read turns "verify" into "add a committed test". No instruction says that verification can be a run that is not committed. No instruction sets a runtime budget.

| # | Source (file:line) | Text (short) | Effect |
|---|---|---|---|
| 1 | ponytail SessionStart hook, `~/.claude/plugins/cache/ponytail/ponytail/4.9.0/hooks/ponytail-instructions.js:72` (same text in that plugin's `AGENTS.md:30` and the injected SKILL) | "Lazy code without its check is unfinished: non-trivial logic leaves ONE runnable check behind (... one small test file)" | Injected into every session. "A branch" counts as non-trivial, so almost every UI change qualifies. Agents read "check" as "committed test". |
| 2 | `~/.claude/reference/delegation-templates.md:64` (T2 implementation template, acceptance 2) | "沒有現成測試 → 留下一個最小可執行檢查（test 或 assert self-check）並實跑過" | Every implementation brief built from T2 asks the worker to leave a test behind. |
| 3 | `~/.claude/rules/judgment-rubrics.md:62` (R5.1) | "一個會在邏輯壞掉時失敗的可執行檢查（測試、assert self-check、或實跑一次關鍵路徑）" | Allows a one-off run, but it does not say where a one-off check lives or that it is not committed. The test is the default reading. |
| 4 | `~/.claude/rules/lessons-learned.md:26-27` | "先寫檢查，再注入違規，證明檢查會變紅" / "測試須親眼看過紅燈，再看到綠燈" | Correct for mechanisms. Briefs copy it as "add a test that is red on the base commit", so each fix becomes a permanent test. |
| 5 | `~/.claude/plugins/cache/jhangyu/podium/1.1.0/skills/test-driven-development/SKILL.md:18-34` | "Always: New features, Bug fixes, Refactoring, Behavior changes" / "NO PRODUCTION CODE WITHOUT A FAILING TEST FIRST" | No exception for UI wiring or cosmetics. The only exceptions need human permission. |
| 6 | Lead briefs and contracts in this repo, which copy 1-4 | `docs/logs/2026-10-23/impl-brief-column-resize.md:40` "Every non-trivial change leaves one test that fails if the logic breaks."; `contract-column-resize.md:20` "A vitest test fails if the width is not saved or not restored."; `round-1-ui-plan.md:62-63` "New test in AppFrame.test.tsx ... New test in mail.test.tsx"; `round-3-ui-plan.md:58`; `review-round2.md:31` "Each acceptance bullet maps to a named test" | Each acceptance criterion becomes a test that stays in the suite. |
| 7 | No runtime budget anywhere. Search: `vite.config.ts:8-12` has no timeout. `src-tauri/.cargo/config.toml` has no runner. `cargo-nextest` is not installed. No git hooks (`.git/hooks` has only samples, `core.hooksPath` is unset). | - | A slow test costs the author nothing. 51 Rust tests take >2 s, the slowest takes 31.5 s (`test-inventory-rust.md`, "Slowest 20"). 4 vitest tests take >1 s (`test-inventory-frontend.md` §4). |

Evidence from the repo: the last 40 commits that touched test files include 1 test file per feature commit (`b9d5230`, `af2afa3`, `690df2d`: column-resize, each with its own UI test), and 19 test files in one cleanup merge (`d32b4e0`). The frontend inventory flags 26 of 79 vitest cases as UI wiring, cosmetic, or duplicate.

Conclusion: fix the words in two places and add one mechanical cap.
- Project: `CLAUDE.md` carries the policy (it loads on every task, and section 2 of the global CLAUDE.md ranks it above plugin text).
- Global: the user decides on the proposals in section 4.
- Mechanical: a per-test time cap in both runners. No mechanism can judge "test value", so value is enforced by policy text and review only.

## 2. Policy text (paste into `/Users/jhangyu/project/outlooker/CLAUDE.md`, after the cargo section)

```markdown
# Test policy

Verifying a change and adding a test are different things. Verify every change by running it. Commit a new test only when it guards one of these:
- core logic: import pipeline, dedupe, search compile and semantics, SQL console policy, lifecycle state machines, data transforms
- data integrity: schema and migrations, resume and crash recovery, atomic writes, removal
- security: credentials, roles and read-only SQL, sanitizing, sandboxing
- a bug that actually happened (one regression test for each bug)

Do not commit tests for: layout, styling, labels and copy, click-to-callback wiring, prop pass-through, enum or serde round-trips already covered by the type system or another test, or anything an existing test already covers. A finished UI is rarely touched again, so its tests run thousands of times and guard nothing.

Expensive setup is shared, never per test. A database cluster boot, a fixture import, a large seed dataset, or an app or runtime launch runs once per test run. All tests that need it share it. Each test gets cheap isolation: a fresh database or schema, or a reset. No test boots its own. In Rust, every DB test gets its database from the shared fixture in `test_support`. The per-test boot function is not public, so it cannot be called outside that module.

One-off checks go in `scripts/tmp/` (gitignored, never committed). This includes red/green proofs, probes, and "does it render" runs. An acceptance criterion says "behavior X holds (run output attached)". It does not say "add a test for X", unless X is in the list above.

Runtime budget: a typical test takes a few hundred ms. The hard cap is 1 s per test, including setup hooks. `vite.config.ts` (testTimeout/hookTimeout) and `scripts/cargo-test-runner.mjs` (libtest --ensure-time on debug test binaries) enforce the cap. A test that hits the cap is rewritten to be smaller (smaller input, shared fixture, test the function rather than the whole app) or deleted. Never raise the cap. Never move a test out of the default run to escape the cap.

Before you add a test, search for an existing test of the same behavior and extend it. e2e (`npm run e2e`) and `--release` perf tests are opt-in. Do not add to them unless the user asks.
```

(~20 lines. English to match the existing CLAUDE.md.)

## 3. Enforcement mechanisms

Two mechanisms, one per runner. Both run on the canonical commands (`npm test`, `cargo test --workspace`), so agents cannot skip them by using the normal workflow.

### M1. vitest per-test cap: `vite.config.ts`
Add `testTimeout: 1000, hookTimeout: 1000` to the `test` block. vitest 5.0.5 (package.json) supports both. Native, one line.
- `hookTimeout` is required. Proven: a cheap test body with a 1.5 s `beforeEach` passes with only `testTimeout` set (RC=0, `scripts/tmp/policy-design/vt/out-hook-default.txt`). It fails with `hookTimeout: 1000` ("Hook timed out in 1000ms", RC=1, `out-hook-1000.txt`).
- Scratch red proof (`vt/out.txt`, RC=1): an async 1.5 s test fails with "Test timed out in 1000ms", and a synchronous 1.5 s busy loop also fails (1555ms, same message). A fast test passes.
- Rejected: `slowTestThreshold`. It only colors the reporter output and fails nothing. Its default is already 300 ms.

### M2. Rust per-test cap: cargo target runner + libtest `--ensure-time`
- Rejected: cargo-nextest. It is not installed (`which cargo-nextest`: not found), so it adds a tool. It also runs each test in its own process, which defeats the shared PostgreSQL template cluster that the slow-test REWRITEs need (each PG test boots a cluster, ~1.3 s, `test-inventory-rust.md`). It would also change the canonical command.
- Rejected: a timing guard inside `TestCluster`. It misses the non-PG tests (for example the 9.4 s loadgen `deterministic_items_and_duplicate_plan`). It also collides with the cleanup rewrite of `test_support/mod.rs`.
- Chosen: libtest has a native per-test limit. `--ensure-time` fails any test above the critical threshold from `RUST_TEST_TIME_UNIT` / `RUST_TEST_TIME_INTEGRATION` (`"warn_ms,critical_ms"`). On the stable 1.99.1 toolchain it is unstable: it fails with "the option `Z` is only accepted on the nightly compiler" (`rt/B0.txt`) and works with `RUSTC_BOOTSTRAP=1` set at runtime (`rt/B.txt`: `slow_unit ... FAILED (time limit exceeded) <1.500s>`, RC=101).
- To keep `cargo test --workspace` unchanged, a cargo `target.runner` (applies to every binary that cargo runs) calls a small node script. The script adds `-Z unstable-options --ensure-time` and `RUSTC_BOOTSTRAP=1` only for binaries under `debug/deps/` (test binaries). `cargo run` binaries and `--release` perf binaries pass through unchanged.
- Proven in scratch workspace `ws/`:
  - red: RC=101, `r3.txt`.
  - `-- --list` still works: RC=0, `r4.txt`.
  - adding the config recompiles 0 crates: `grep -c Compiling r6.txt` = 0, so there is no extra copy in `target/`, which satisfies the CLAUDE.md cargo rule.
  - an env override `RUST_TEST_TIME_UNIT=2000,3000` turns it green: RC=0, `r7.txt`.
  - `cargo run --bin hello -- x y` passes args and exit code 3 through and does not see `RUSTC_BOOTSTRAP`.
- Path pitfall, proven: cargo starts the runner with cwd = package dir, so a relative script argument fails ("Cannot find module .../crates/a/scripts/...", `r1.txt`). Fix: pass the script path through `[env]` with `relative = true` and load it with `node -e`.
- Known limits:
  - (a) It depends on an unstable libtest flag through `RUSTC_BOOTSTRAP` (test binaries only, never rustc). If a future toolchain drops `--ensure-time`, every test binary fails loudly with an "unrecognized option" error. It never goes silently green.
  - (b) Every `cargo run` (including `tauri dev`) goes through one extra node process. Args, stdio, and exit code pass through. Signal behavior under `tauri dev` restarts was not tested.
  - (c) Windows (path separator handled with `path.sep`) is not run yet. Verify it in the Windows SSH phase.

### Shared-setup rule: enforced by M1/M2 plus Rust visibility; no grep check
- The 1 s cap already fires on a per-test boot. A PG cluster boot costs ~1.3 s (`test-inventory-rust.md`), so a test that boots its own cluster fails M2. A full app render plus a fixture import per test hits M1 the same way.
- Compiler visibility: the shared fixture that plan-rust-opus designs in `crates/outlooker-core/src/test_support/` must keep the per-test boot function non-`pub`. Every caller in `tests/*.rs` is a separate crate, so a direct call fails to compile. This needs no new code. The cleanup owns that file; this design only adds an acceptance check.
- Rejected: a grep check. Cluster boots also happen in production code (`lifecycle/{init,start,pg,move_dir,health,supervisor}.rs`, loadgen `root.rs`, `examples/import_bench.rs`), and the app-level lifecycle tests reach them through the production API. A grep for `PgController::new` / `TestCluster::start` would need an allowlist of production files. It would duplicate what visibility plus the time cap already enforce.

### Rejected: test-count ratchet and commit hook
- A count ratchet: agents bump the number in the same commit, and the count does not measure value.
- A pre-commit hook: the Rust suite takes minutes, it can be skipped with `--no-verify`, and the repo has no hook infrastructure.
- The value side stays with the policy text plus the global prompt fixes in section 4.

## 4. Proposals for global rules (NOT applied; user decides)

Back up each file to `~/.claude/backups/` before you edit it (global CLAUDE.md §7).

P1. `~/.claude/CLAUDE.md`, section 8 工程標準: append a new block after line 77 (end of 未釋出開發政策):
```markdown
**測試政策**
- 驗證不等於加測試。每個改動都要實跑驗證；只有守護核心邏輯、資料完整性、安全、或曾實際發生的 bug 時才 commit 新測試。
- 不 commit：版面、樣式、文案、點擊→callback 接線、純轉發、型別系統已保證或既有測試已涵蓋的行為。
- 一次性驗證（紅綠證明、探測、渲染檢查）放專案的 scratch 目錄（如 scripts/tmp/），不 commit。
- 單一測試常態數百 ms，上限 1 s；超限就重寫或刪除，不放寬上限。
- 昂貴的 setup（資料庫叢集啟動、fixture 匯入、大量種子資料、app 或 runtime 啟動）每次測試執行只做一次，所有測試共用；每個測試用便宜的隔離（新的 database 或 schema、或 reset），不得自行啟動。
- plugin/skill（ponytail「留一個 runnable check」、TDD 鐵律）要求留測試時，以本政策為準：check 可以是不 commit 的一次性實跑。
```

P2. `~/.claude/reference/delegation-templates.md:64`: replace acceptance item 2 with:
```
2. 執行 {測試命令} 全綠，並實跑一次改動的關鍵路徑（附輸出）。只有改動屬核心邏輯／資料完整性／安全／已發生 bug 時才新增 commit 測試；一次性檢查放 scratch 目錄，不 commit。單一測試不得超過 1 s。
```

P3. `~/.claude/rules/judgment-rubrics.md:62` (R5.1): replace with:
```
1. 一個會在邏輯壞掉時失敗的可執行檢查：預設是一次性實跑（scratch 目錄，不 commit）；只有核心邏輯、資料完整性、安全、已發生 bug 才留下 commit 的測試（≤1 s）。平凡 one-liner 免。
```
Also append to the ✗ example at line 68: `UI 接線／文案改動新增 render 測試 → 過度，實跑一次即可。`

P4. `~/.claude/rules/lessons-learned.md`, section 「先預註冊判準，先看過紅燈，再開工」 (after line 27): add
```
  - 紅綠證明可在 scratch 目錄做。證明完成不代表該測試要 commit；commit 與否依測試政策。
```

P5. podium plugin (the user's own; source repo, not the cache copy) `skills/test-driven-development/SKILL.md:24-27`: add to the Exceptions list
```
- UI layout, styling, copy and click-to-callback wiring (verify by running once; do not commit a test)
- Red/green runs done in a scratch dir that is not committed, when the project test policy says the behavior does not earn a permanent test
```
The ponytail plugin (`hooks/ponytail-instructions.js:72`) is third-party. Do not edit it. P1 overrides it through global CLAUDE.md §2 precedence.

## 5. Implementation spec for policy-impl-opus (round 3)

### Owned files (disjoint from cleanup)
| File | Action | Shared? |
|---|---|---|
| `CLAUDE.md` (repo root) | append section 2 policy text verbatim | not edited by cleanup |
| `vite.config.ts` | add `testTimeout: 1000, hookTimeout: 1000` in `test` | FLAG: confirm with the lead that plan-frontend-opus does not edit it |
| `src-tauri/.cargo/config.toml` | add the `[env]` key and the `[target]` runner below | FLAG: confirm that plan-rust-opus does not edit it |
| `scripts/cargo-test-runner.mjs` | new | new file |

Not touched: any `*.test.ts(x)`, `src/test/setup.ts`, Rust test files, `test_support/`, fixtures.

### Content
`src-tauri/.cargo/config.toml` (keep the existing `[env]` lines and add one key; add the target table):
```toml
[env]
TS_RS_EXPORT_DIR = { value = "../src/api/bindings", relative = true }
TS_RS_LARGE_INT = "number"
CARGO_TEST_RUNNER_JS = { value = "../scripts/cargo-test-runner.mjs", relative = true }

# Per-test time cap (CLAUDE.md "Test policy"); the script passes non-test binaries through unchanged.
[target.'cfg(all())']
runner = ["node", "-e", "import(require('node:url').pathToFileURL(process.env.CARGO_TEST_RUNNER_JS))"]
```

`scripts/cargo-test-runner.mjs`:
```js
// Cargo target runner (src-tauri/.cargo/config.toml): debug test binaries get libtest's per-test time cap
// (1 s critical, CLAUDE.md "Test policy"); `cargo run` and --release binaries pass through unchanged.
import { spawnSync } from "node:child_process";
import { sep } from "node:path";

// Loaded through `node -e`, so argv[1] is the binary.
const [bin, ...args] = process.argv.slice(1);
const env = { ...process.env };
if (bin.includes(`${sep}debug${sep}deps${sep}`)) {
  // ponytail: --ensure-time is unstable libtest; RUSTC_BOOTSTRAP is set for the test binary only, never for rustc.
  env.RUSTC_BOOTSTRAP = "1";
  env.RUST_TEST_TIME_UNIT ??= "500,1000";
  env.RUST_TEST_TIME_INTEGRATION ??= "500,1000";
  args.unshift("-Z", "unstable-options", "--ensure-time");
}
const r = spawnSync(bin, args, { stdio: "inherit", env });
process.exit(r.status ?? 1);
```
Use the Write tool, not a heredoc. The safety hook blocks heredocs that contain timer or sleep words.

### Order
1. Write the 4 files.
2. Run the red proofs below.
3. Do NOT commit until the cleanup members report that their suites are green. Before cleanup, 51 Rust tests and 4 vitest tests exceed the cap, so committing early turns every member's run red.
4. The lead's final gate run (gate-runner) runs with the mechanisms on. That run is the evidence for criterion 5a.
5. Commit the 4 paths with a pathspec.

### Acceptance commands (each with RC captured right after the command)
Frontend (from the repo root):
- R1 red: create the temporary file `src/zz-timecap.test.ts` with two cases: `it` with a 1.5 s busy loop, and a `describe` with a 1.5 s busy-loop `beforeEach` plus an empty `it`. Run `mise exec -- npx vitest run src/zz-timecap.test.ts > scripts/tmp/policy-impl/fe-red.txt 2>&1; echo RC=$?`. Expect RC≠0, "Test timed out in 1000ms" and "Hook timed out in 1000ms" present. Then delete the file.
- R1 green: after cleanup, `mise exec -- npm test` is green 3 runs in a row. Any test failing on the timeout is a REWRITE item for cleanup. Do not raise the timeout.

Rust (from `src-tauri/`):
- R2 red: create the temporary file `crates/outlooker-core/tests/zz_timecap.rs` with one `#[test]` that busy-loops 1.5 s (`Instant` loop, no `thread::sleep`). Run `cargo test --workspace --test zz_timecap > ../scripts/tmp/policy-impl/rs-red.txt 2>&1; echo RC=$?`. Expect RC=101 and "time limit exceeded".
- R2 knob: `RUST_TEST_TIME_INTEGRATION=2000,3000 cargo test --workspace --test zz_timecap`. Expect RC=0. This proves the cap is what failed the test. Then delete the file.
- R2 no rebuild: run `cargo test --workspace --lib` once before and once after the config edit. The second run's output has 0 `Compiling outlooker` lines, apart from crates that other members edited (state which).
- R2 green: after cleanup, the gate-runner's full `cargo test --workspace` (with `-- --test-threads 4` per memory) is green with the runner on. `cargo clippy --workspace --all-targets -- -D warnings` stays green, because the runner is not involved in clippy.
- Passthrough: `cargo run`-style passthrough was proven in scratch only (`scripts/tmp/policy-design/ws`). Optional: one `npm run tauri dev` launch and quit, to check that the node hop does not break dev.

Shared setup (after the plan-rust-opus fixture lands):
- R3 red: create the temporary file `crates/outlooker-core/tests/zz_boot.rs`. It calls the per-test boot function directly (today `TestCluster::start`; use the name from the final fixture). Run `cargo test --workspace --test zz_boot > ../scripts/tmp/policy-impl/boot-red.txt 2>&1; echo RC=$?`. Expect RC=101 and a privacy error (E0624 private function, or E0603 private item). Then delete the file. If it compiles, the fixture's boot is still `pub`: report to the lead. Do not edit `test_support` yourself.

Policy:
- `grep -n "^# Test policy" CLAUDE.md` returns 1 line, and the section text equals section 2 of this file.

## Uncertain / needs a decision
- The cap measures wall time under the normal parallel run (vitest workers; cargo `--test-threads 4`). Tests near 0.8 s (for example `search.test.tsx:48` 829 ms) can flake past 1 s. The policy says rewrite such tests. It does not say raise the cap.
- User decision (2026-10-23): all DB tests share ONE PostgreSQL startup per run (plan-rust-opus owns the design). The R3 check depends on that fixture keeping its boot function non-`pub`.
- `outlook-fixtures` tests (Windows phase, real PSTs) are debug test binaries, so the cap applies to them. `RUST_TEST_TIME_INTEGRATION=...` overrides it for that run if needed.
