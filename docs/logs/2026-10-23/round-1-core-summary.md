# Round 1 — squad core summary

Branch `techdebt/r1-core`, head `d139fbf` (base `c1965dd`). Plan: `round-1-core-plan.md`.

## Commits
- `71a2a89` A2 unused serde/serde_json removed; C4 image without default codecs; A3 `[workspace.dependencies]`; A1 pff build.rs always re-prepares; C15 one newest-server-log scan (+ unit test).
- `56b5acb`, `edb14f5` F4 orphaned plan IDs replaced in 12 comment-only files.
- `fe101cf` C1 one `db_err` + `pool_err`, `Db::app_client`/`ro_client`; C3 step 1 `db::library_counts`; C14 timeouts from constants; C16 one `truncate_to_micros`; C13 `is_connected` deleted (+ 3 unit tests, 1 integration test).
- `3431a42` F4 last plan ID (`search/mod.rs`).
- `d139fbf` reviewer R-C1: SQLSTATE 53100 → `insufficientSpace` only in the writer with the real data root; elsewhere `Internal("53100: …")` (+ unit test).

## Visible behavior changes (bug fixes, for the round report)
- Closed pool / I/O error → `DbNotReady` everywhere; fixes settings `get` fallback (was `Internal`).
- 57P01/57P02/57P03 → `DbNotReady` (an import no longer fails on PostgreSQL restart; it pauses).
- Pool connect with SQLSTATE (bad password, too many connections) and non-closed non-I/O client errors → `Internal` (were `DbNotReady`, which paused the queue forever).
- 53100 outside the writer's batch/session path → `Internal` with the PostgreSQL message (was `FileAccess` with an empty path).
- Health detail (`last_log_line`) no longer picks `startup.log`.
- Pre-2000 mtimes in the queue now truncate like PostgreSQL stores them.

## Checks
- B2 corrected: `moxcms` stays in Cargo.lock (non-optional dependency of image 0.25.10, Cargo.lock:2222).
- A3 grep also matches the 4-count status query that the plan keeps (`lifecycle/status.rs:15`); accepted.

## Gate artifacts
- Full gate, 20/20 RC=0 at `3431a42`: `/Users/jhangyu/project/outlooker-wt/r1-core/tmp/verify/20261022T1831*`–`1837*`.
- Targeted re-gate after R-C1, 9/9 RC=0 at `d139fbf` (clippy, lib 133, db_schema, removal, writer, import_pipeline A/B/C, smoke seeded): `tmp/verify/*-r2-*.txt`.

## Refuted / not worth fixing / decisions
See plan sections "Refuted items" and "Not worth fixing". A7: keep `outlook-fixtures` and `make-pst.ps1` (pending Windows phase W2).
