# Cargo invocation rule (run from `src-tauri/`)

Canonical commands, no other flags on feature selection:

- Test: `cargo test --workspace [--test NAME | --lib | <test-name filter>]`
- Lint: `cargo clippy --workspace --all-targets -- -D warnings`
- Build: `cargo build --workspace`

Never use `-p` / `--package`, per-package `--features`, `--all-features`, or `cargo check` (use clippy).
Why: cargo unifies features only over the selected packages and `check` and `clippy` hash differently, so every
variant (`-p X`, `--features Y`, check vs clippy) recompiles shared deps (serde_json, url, time) and downstream crates
as a full extra copy in `target/`. Narrow to one test with `--test NAME` or a name filter, which does not change the
resolved features. `docs/**` plans that show `cargo ... -p ...` are superseded by this rule.

Allowed exceptions (separate artifacts, different profile or binary config):
- `tests/e2e/build.mjs`: `tauri build --debug --no-bundle --features e2e` (e2e + tauri custom-protocol binary).
- `--release` perf runs (`outlooker-loadgen/tests/perf.rs`, `import_bench` example): target/release, not target/debug.
