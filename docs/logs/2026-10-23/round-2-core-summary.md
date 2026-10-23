# Round 2 — squad core summary

Branch `techdebt/r2-core`, base `a832785`, code head `e9aafed`. Plan: `round-2-core-plan.md`. Implementer 1 report: `round-2-core-impl1-report.md`.

## Commits
- `632058a` C5 `as_db`/`from_db` for ImportPhase, ImportErrorStage, ImportErrorSeverity, RecipientKind and new JobKind/JobState (one `db_text!` macro, `dto/import.rs`); C18 `emit_status` returns the fetched source; C12 data-dir move rollback; A13 `version` removed from `tauri.conf.json` (fallback confirmed in tauri-utils 2.10.2 `config.rs:4108`).
- `616dd92` C8 one `fsutil::write_atomic(path, bytes, private)`; C9 poison-tolerant locks at every non-test site (core, app `events.rs`, loadgen `images.rs`); C11 `maintenance::REINDEX_LIST_SQL` replaces `REINDEX_INDEXES`; C19 `test_support::{count, png}` + `TestCluster::count`.
- `01ffc72`, `e9aafed` reviewer nits: `write_atomic` removes its `.tmp` on failure and closes the file before rename/remove (Windows); reindex test rejects `messages_dedupe_once` and requires exactly 17 indexes.

## Items
Fixed: C5, C18, C12, A13, C8 (writer part), C9, C11, C19 (integration-test part).
Not worth fixing:
- C8 `AppError::io`: argument sugar over 18 sites, no drift risk.
- C19 `importer/webp.rs` unit-test png: kept so lib unit tests do not depend on the `test-support` feature (only enabled through loadgen's feature unification).

## Visible behavior changes (for the round report)
- An unknown text in `import_jobs.kind/state`, `import_checkpoints.phase`, `import_errors.stage/severity` or `recipients.kind` now returns `Internal` instead of a plausible wrong value (one bad row fails the whole import list).
- Failed data-dir move into a pre-existing empty target: the target is emptied and kept (was deleted). Failed old-root removal after a successful move and every rollback error are logged.
- The initdb password file is written via tmp + rename (0600); a failed write leaves no tmp.

## Gate artifacts (all in `r2-core/tmp/verify/`, untracked)
- Baseline at `a832785`: `base-*.txt`, 20/20 RC=0.
- Full gate at `616dd92`: `gate-00..20-*.txt`, 20/20 RC=0 (lib 11+121+7, lifecycle 18, import_pipeline 26/26, sql_console 8/8, smoke 2/2), no orphan postmasters.
- Targeted re-gate at `e9aafed` (only `fsutil.rs` and `tests/maintenance.rs` changed since `616dd92`): `regate-01..04-*.txt`, clippy, lib 11+122+7, maintenance 3, pg_lifecycle 8, all RC=0.

## Known limitation
- `tests/import_pipeline.rs:991` (`outlook-fixtures` feature, `tc.count` edit) is never compiled by the allowed cargo commands; it belongs to the pending Windows phase W2.

## Round 3 (core)
See `round-2-core-plan.md` "Round 3 roadmap": P1 opus C2 + C20 + C13 rest; P2 sonnet C10, A9, A11 + A8 core part; C6/C7/C17/C22 not worth fixing without an `import_bench` measurement.
