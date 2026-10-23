# Test cleanup plan (index)

- Rust: [test-cleanup-plan-rust.md](test-cleanup-plan-rust.md) (v3)
- Frontend: [test-cleanup-plan-frontend.md](test-cleanup-plan-frontend.md)
- Prevention: [test-policy-design.md](test-policy-design.md)

## User decisions (2026-10-23)
1. The 4 own-cluster tests (first run, crash restart, data-dir move, start after crash) stay in `tests/cluster_lifecycle.rs` with a 3 s per-test cap for that binary only.
2. Rust per-test cap: enforce automatically (runner + libtest --ensure-time via RUSTC_BOOTSTRAP; 1 s, 3 s for cluster_lifecycle).
3. Delete `tests/e2e/scroll.e2e.ts` with its seed data, and delete the dark-theme source check (`src/app/global.test.tsx:91`).
4. Apply global-rule proposals P1-P5 from test-policy-design.md section 4.
5. Lead decisions: C-locale test moves onto the shared cluster (no 5th exception); the cold template build is pre-warmed by the test runner script.
