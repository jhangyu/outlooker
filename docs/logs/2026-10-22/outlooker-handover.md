# Outlooker — session handover (2026-10-22, written by team-lead at ~46% context)

## Goal
User (asleep, gave standing authorization): build the Outlooker PST search desktop app per the approved spec and plan, follow the best route until all tasks are done. Stop only for: Windows phase (needs user SSH), real large PST for G2.7 (user-provided), destructive/irreversible actions, scope growth, or budget exhaustion.

## Must-read entry points
1. docs/logs/contract-phase2.md — frozen contract for implementation (end state, acceptance A1–A4, budget, parking lot, "Needs the user").
2. docs/superpowers/plans/2026-10-21-outlooker-plan.md — outline: §4 Round 2 tasks, §2.3 land order, §6 ownership, §7.2 gates, §7.3 Windows phase.
3. Detail files: -foundation.md (R2-08, reviewers, Windows steps), -backend.md (R2-09…R2-12), -frontend.md (R2-13, R2-14).
4. scripts/tmp/round-rules.md — rules pasted into every implementer prompt (git, hook, long commands, report format). gitignored; recreate from this file's "Rules" if lost.
5. Spec docs/superpowers/specs/2026-10-21-outlooker-design.md; requirements docs/specs/requirements.md; chosen UI docs/design/variant-1/.

## State at handover (verify with git) — UPDATED at close of Round 2
- Round 2: DONE on macOS. Review PASS for R2-08…R2-14 (docs/logs/review-round2.md); gate record docs/logs/gate-round2-6e46659.md (lint fix 0c64266). HEAD after that: see `git log`.
- Pending user: (1) approve-long-run for E2E scroll (`mise exec -- node -e "import('./tests/e2e/prepare.mjs').then(m=>m.prepareGroup('scroll'))"`, then `mise exec -- node tests/e2e/run.mjs --spec tests/e2e/scroll.e2e.ts`) and the G2.6 1M load test (`cd src-tauri; cargo test -p outlooker-loadgen --release -- --ignored perf_generate`, then each perf_<gate>); (2) a real large PST for G2.7 (`OUTLOOKER_PERF_PST`); (3) SSH to the Windows box for W1–W3.
- All team members are shut down.

### Original state at first handover
- Branch main, HEAD fa20e00 (round 1 gate record). Untracked only docs/design/variant-2/ and variant-3/ (unchosen prototypes, user may delete).
- Round 1: DONE. All 7 tasks reviewed PASS (docs/logs/review-round1.md), gate PASS (docs/logs/gate-round1-b3607fc.md).
- In flight: impl-r103 fixing one nit in src-tauri/crates/outlooker-core/src/lifecycle/pg.rs (remove leftover .initdb-pw before creating it 0600). Check `git log --oneline -3` for its commit; then shut it down.
- Round 2: see "Round 2 launch" below for status.

## Environment facts (verified)
- Rust 1.99.1 via rustup (src-tauri/rust-toolchain.toml). Node 24.22.1 via mise: run `mise exec -- npm|node ...` (plain node is 25.9.0).
- PostgreSQL 18.6.1 binaries at src-tauri/resources/postgresql (fetched).
- Bash hook: denies `&`, heredoc, "unt"+"il", sleep, literal /tmp in command text; 2 denials → circuit breaker. Files only via Write/Edit. Foreground caps 40 s / 150 s (build) — re-run cargo/npm to progress; never background.
- Gate command fixes for G2.1: run pff tests without `--features outlooker-core/test-support`; quote `--include=*.rs` patterns (zsh).
- wait-member-exit: `python3 ~/.claude/scripts/wait-member-exit.py <names>`.

## Round 2 launch (plan §4, §2.3)
Tasks and planned members (sonnet implementers, opus reviewer, haiku gate-runner): impl-r208 (R2-08 lifecycle + settings), impl-r209 (R2-09 importer), impl-r210 (R2-10 writer/removal/loadgen), impl-r211 (R2-11 search), impl-r212 (R2-12 SQL console + maintenance), impl-r213 (R2-13 app shell, commands, events, mailimg, core::app, release build — lands last among Rust tasks), impl-r214 (R2-14 frontend integration + E2E), rev-r2 (R2-REV, log docs/logs/review-round2.md), gate-r2 (G2.1–G2.6, G2.8, G2.9 after review closes).
Land order: R2-08 lands C15 + lifecycle/move_dir.rs (free_space) first; R2-10 lands C11/C12 first (writer disk guard step only after R2-08's commit); R2-11 lands C13 first; R2-13 last; R2-14 dry-runs early (smoke spec), real E2E after R2-13.
Known risks: G2.6 1M-message load test may exceed foreground caps (needs splitting or a user approve-long-run token → report to user); G2.7 needs a user PST; import_bench measured 119 msg/s on tiny outlook.pst (target ≥150 on real data).

## Decisions already made (do not reopen)
UI variant 1; app id us.jhangy.outlooker; caps: count exact to 10,000 then "10,000+", list ≤100,000 rows, Advanced SQL 30 s / ≤10,000 rows (default 1,000); out-of-scope prototype controls excluded; Windows via user SSH after macOS work; poison item = two-attempt rule; paused import waits only for Ready; stored body_tsv + phase-A window 5,000; global no-literal test skips `.cm-editor`.

## Parking lot
docs/logs/contract-phase1.md, docs/logs/contract-phase2.md, docs/logs/review-round1.md (17 items), scripts/tmp/review-plan-*.md.

## First action for a new session
Read this file and contract-phase2.md; run `git log --oneline -15` and `git status --porcelain`; check which Round 2 members exist (`python3 ~/.claude/scripts/wait-member-exit.py <names>` prints ALIVE/EXITED); continue from the first unfinished Round 2 item.
