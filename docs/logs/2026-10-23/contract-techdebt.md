# Contract — Tech-debt cleanup (3 rounds)

## End state
The codebase carries no high-ROI tech debt left: every item scored worth fixing in the scan inventory is fixed or explicitly judged not worth fixing, and the full gate is green on `main`.

## In scope
- Phase 0: three read-only scans (Rust core crate; Tauri app + loadgen + pff + scripts + build config; frontend + e2e tests + docs) producing a scored inventory.
- Rounds 1–3: each round, two squads. Each squad = 1 opus architect (squad lead, plans and verifies) + 2 sonnet implementers. One opus reviewer per round over the combined diff.
- Doc fixes (stale/incorrect docs, superseded plans) are applied directly as part of the rounds.

## Out of scope
- New features or behavior changes visible to users (except fixing an outright bug found as debt, reported in the round report).
- Changing existing account passwords; touching `src-tauri/resources/postgresql/`, vendored third-party trees, `src-tauri/gen/`.
- Windows-only verification (done later over SSH by the user).

## Acceptance criteria (every round, on the merged integration state)
1. `cd src-tauri && cargo clippy --workspace --all-targets -- -D warnings` exits 0.
2. `cd src-tauri && cargo test --workspace -- --test-threads 4` exits 0 (split per `--test NAME` if over the foreground cap).
3. `mise exec -- npm run build` (tsc + vite) exits 0; frontend unit tests (if `npm test` exists) exit 0.
4. Every inventory item assigned to the round is either fixed (commit hash) or marked "not worth fixing" with a one-line reason in the round handoff.
5. Round reviewer verdict: CONFIRMED with no open blocker/should-fix.

## Parking lot
None by user decision: reviewer findings are fixed in-round or judged not worth fixing; items left over after round 3 are reported as "remaining, not worth fixing" with reasons.

## Round budget
3 rounds. Stop early when the remaining inventory is not worth fixing.
