# Cargo invocation rule (run from `src-tauri/`)

Canonical commands, no other flags on feature selection:

- Test: `cargo test --workspace [--test NAME | --lib | <test-name filter>]`
- Lint: `cargo clippy --workspace --all-targets -- -D warnings`
- Build: `cargo build --workspace`

Never use `-p` / `--package`, per-package `--features`, `--all-features`, or `cargo check` (use clippy).
Why: cargo unifies features only over the selected packages and `check` and `clippy` hash differently, so every
variant (`-p X`, `--features Y`, check vs clippy) recompiles shared deps (serde_json, url, time) and downstream crates
as a full extra copy in `target/`. Narrow to one test with `--test NAME` or a name filter, which does not change the
resolved features.

Allowed exceptions (separate artifacts, different profile or binary config):
- `tests/e2e/build.mjs`: `tauri build --debug --no-bundle --features e2e` (e2e + tauri custom-protocol binary).
- `--release` perf runs (`outlooker-loadgen/tests/perf.rs`, `import_bench` example): target/release, not target/debug.

# Test policy

Verifying a change and adding a test are different things. Verify every change by running it. Commit a new test only when it guards one of these:
- core logic: import pipeline, dedupe, search compile and semantics, SQL console policy, lifecycle state machines, data transforms
- data integrity: schema and migrations, resume and crash recovery, atomic writes, removal
- security: credentials, roles and read-only SQL, sanitizing, sandboxing
- a bug that actually happened (one regression test for each bug)

Do not commit tests for: layout, styling, labels and copy, click-to-callback wiring, prop pass-through, enum or serde round-trips already covered by the type system or another test, or anything an existing test already covers. A finished UI is rarely touched again, so its tests run thousands of times and guard nothing.

Expensive setup is shared, never per test. A database cluster boot, a fixture import, a large seed dataset, or an app or runtime launch runs once per test run. All tests that need it share it. Each test gets cheap isolation: a fresh database or schema, or a reset. No test boots its own. In Rust, every DB test gets its database from the shared fixture in `test_support`. The per-test boot function is not public, so it cannot be called outside that module.

A test on the shared PostgreSQL cluster never runs ALTER ROLE, ALTER SYSTEM, or DROP/ALTER of another test's database, and it terminates or cancels backends only with `datname = current_database()`.

The only exception binary is `tests/cluster_lifecycle.rs` (3 s cap, user-approved); no other test starts its own cluster.

One-off checks go in `scripts/tmp/` (gitignored, never committed). This includes red/green proofs, probes, and "does it render" runs. An acceptance criterion says "behavior X holds (run output attached)". It does not say "add a test for X", unless X is in the list above.

Runtime budget: a typical test takes a few hundred ms. The hard cap is 1 s per test, including setup hooks. `vite.config.ts` (testTimeout/hookTimeout) and `scripts/cargo-test-runner.mjs` (libtest --ensure-time on debug test binaries) enforce the cap. A test that hits the cap is rewritten to be smaller (smaller input, shared fixture, test the function rather than the whole app) or deleted. Never raise the cap. Never move a test out of the default run to escape the cap.

Before you add a test, search for an existing test of the same behavior and extend it. e2e (`npm run e2e`) and `--release` perf tests are opt-in. Do not add to them unless the user asks.
