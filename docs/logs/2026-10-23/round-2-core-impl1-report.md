# Round 2 core — implementer 1 report (Package 1: C5, C18, C12, A13)

Code commit `632058a` (branch `techdebt/r2-core`).

## Conclusions
- C5: one `db_text!` macro (`dto/import.rs:6-24`) generates `as_db` / `from_db` (plus a test-only `ALL`) next to each enum:
  - `ImportPhase`, `ImportErrorStage`, `ImportErrorSeverity` (`dto/import.rs`);
  - `RecipientKind` (`dto/search.rs`);
  - new crate-internal `JobKind` / `JobState` (`importer/queue.rs:150-171`).
  - An unknown DB value returns `AppError::internal("unknown <type> <value>")`.
  - Deleted: `phase_of`, `phase_db`, `stage_db`, `severity_db`, `kind_db`, the writer checkpoint phase parser, and the queue stage/severity closures.
  - `search/mail.rs` and `search/compile.rs` now use the enum.
  - `status_of(JobKind, JobState)` is an exhaustive match with no fallback arm.
  - Enum comparisons replace string comparisons in: `list`, `newest_job_for_update`, `cancel`, `resume`, `remove`, `next_job`, `run_job`, `set_final`, `run_import`, `recover_after_crash`. SQL literals are unchanged.
  - The macro marks `as_db` with `#[allow(dead_code)]`. Reason: `JobKind::as_db` is used only by the test, because SQL literals write the kind. The line carries a ponytail comment.
- C18: `emit_status` returns `Option<PstSource>` (`queue.rs:257`). `enqueue` uses that value, so each path runs one `LIST_SQL`. `fn source` is deleted.
- C12 (`lifecycle/move_dir.rs`):
  - Before the copy, the move records the target's permissions. `None` means the move created the target.
  - New `undo_target`: if the move created the target, delete it. If the target existed before, empty it and restore its original mode (`copy_tree` sets the mode to 0700).
  - Success path: a failed old-root removal is logged with `log::warn!` (path + error). The move still reports Done.
  - Rollback: errors of stop, clean, `set_data_dir` and `start_cluster` are logged with `log::error!`. The early restart after a failed stop is also logged. The Failed event still carries the original error.
- A13: Context7 returned "monthly quota exceeded", so I checked the locked source instead:
  - tauri-utils 2.10.2 `src/config.rs:4106-4108`: "App version ... If removed the version number from `Cargo.toml` is used."
  - tauri-codegen 2.7.4 `src/context.rs:255-259` falls back to `env!("CARGO_PKG_VERSION")`.
  - `"version"` is deleted from `tauri.conf.json`. The file still parses as JSON.

## Files changed (632058a)
- core `src/dto/import.rs`, `src/dto/search.rs`
- core `src/importer/queue.rs`, `src/importer/writer.rs`
- core `src/search/mail.rs`, `src/search/compile.rs`
- core `src/lifecycle/move_dir.rs`, `tests/lifecycle.rs`
- `src-tauri/tauri.conf.json`

## Acceptance
- P1-1 PASS: 0 hits.
- P1-2 PASS: 0 hits.
- P1-3 PASS: the only hits are the `db_text!` invocations at `queue.rs:156,169-170`, which are the `as_db` / `from_db` definitions.
- P1-4 PASS: 6 round-trip tests (phase, stage, severity, recipient kind, job kind, job state) plus `status_table`. `cargo test --workspace --lib` RC=0, 11+121+7 tests (`tmp/impl1-lib.txt`).
- P1-5 PASS: the 3 grep hits are `insert_source(inner` (substring match). `fn source` no longer exists.
- P1-6 PASS: 0 `let _ =`. `--test lifecycle move_`: 6/6, RC=0, including the new `move_rollback_keeps_pre_existing_empty_target` (`tmp/impl1-lifecycle.txt`).
  - I did not run this test red. The old code ran `remove_dir_all(target)`, which would fail the test's `is_dir` assert.
- P1-7 PASS: 0.
- P1-8 PENDING: the gate runner must run the full gate.
  - `--test search`: 16/16, RC=0.
  - Full-workspace clippy fails now at `tests/import_pipeline.rs:388` (`count` not found). That file is impl-2's work in progress.
  - Before that error, clippy reported nothing in my files.

## Behavior changes
- An unknown DB text now returns Internal. This covers job kind/state, checkpoint phase, error stage/severity and recipient kind. Before, the value silently became Completed / Finalizing / Store / JobFailed / To.
- These functions can now return that error: `list` (so every status emit), `errors`, `mail_get`, `next_job`, `recover_after_crash`. In `recover_after_crash`, the UPDATE is already applied when the error is raised.
- A failed move into a pre-existing empty target now keeps the directory, empty, with its original mode.
- The move paths write new log lines.
- The app version now comes from Cargo.toml (0.1.0, the same value as before).
- `status_of` changed from `pub` to `pub(crate)`. It had no external callers.
- Changed test assertions: `status_table` cases now use enums. The 9 expected results are the same. No other assertion changed.
- Serde/TS names are unchanged.

## Uncertain
- None, except the pending gate result.
