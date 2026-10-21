# Convergence Contract — Phase 2 (implementation, macOS rounds)

Frozen 2026-10-22 under the user's standing authorization ("after the full execution plan is done, start directly and follow the best route until all tasks are done"). Only the user may change it.

**End state:** Rounds 1 and 2 of docs/superpowers/plans/2026-10-21-outlooker-plan.md are implemented on `main`, and every macOS gate of plan §7.1 and §7.2 passes on the round head commit, except the items listed under "Needs the user".

## In scope
- Round 1 tasks R1-01 … R1-07 + reviewer R1-REV (plan §3, detail in -foundation/-backend/-frontend files).
- Round 2 tasks R2-08 … R2-14 + reviewer R2-REV (plan §4).
- Gate runs G1.1–G1.9 and G2.1–G2.6, G2.8, G2.9 by a dedicated gate-runner.

## Out of scope
- Windows phase W1–W3 (plan §7.3): starts only after the user opens SSH.
- Parking-lot items in docs/logs/contract-phase1.md and the review files scripts/tmp/review-plan-*.md.

## Needs the user (reported at close, not counted as failure)
- G2.7 needs a real large PST supplied by the user.
- W1–W3 need SSH access to the Windows machine.

## Acceptance
- A1. Round 1: G1.1–G1.9 pass on the round head; outputs saved under docs/logs/gate-round1-<hash>.md with RC lines.
- A2. Round 2: G2.1–G2.6, G2.8, G2.9 pass on the round head; outputs saved under docs/logs/gate-round2-<hash>.md; the load test result is in docs/logs/loadtest-<date>.md.
- A3. Reviewer logs docs/logs/review-round1.md and docs/logs/review-round2.md contain no BLOCKED.
- A4. `git status --porcelain` is clean apart from gitignored paths after each round.

## Round budget
2 implementation rounds (plan). Inside a round: at most 2 fix iterations per failing gate before the leader stops and reports.

## Parking lot
(empty)
