# Round 1 core: impl-1 report (Package 1)

Commits on `techdebt/r1-core`:
- `fe101cf`: items 1–5.
- `3431a42`: item 6, the comment fix in `search/mod.rs`.

All files touched are in the Package 1 file list.

## Conclusions
All paths below are in `src-tauri/crates/outlooker-core/src/` unless stated.

- **One error policy in `db/mod.rs`:**
  - `db_err` (:23): an error without SQLSTATE goes to `no_sqlstate_err` (:31). Every other error goes to `sqlstate_err`.
  - `sqlstate_err` (:41) is a pure function on (code, message):
    - 57P01, 57P02 and 57P03 → `DbNotReady`.
    - 53100 → `FileAccess`, path `""`, reason `insufficientSpace`.
    - Any other code → `Internal "<code>: <message>"`.
  - `pool_err` (:50):
    - `Closed` → `DbNotReady`.
    - `Backend` → `db_err`.
    - Any other pool error → `Internal "pool: …"`.
  - `library_counts` (:58) is the one copy of the 2-count query.
  - The unit tests are at the end of the file.
- **`db/pools.rs`:**
  - `Db::app_client` and `Db::ro_client` apply `pool_err` to the pool checkout.
  - `is_connected` is deleted.
  - `app_pool` stays because `lifecycle/health.rs:37` and the tests use it.
- **Deleted private mappers:**
  - search: `pool_err` and `db_err`.
  - queue: `pool_err`, `db_err` and `app()`.
  - sql_console: `pool_err`.
  - settings: `pool_client`.
  - writer: `pg`.
  - remove: `pg`.
  - the old shared `pg_err`.
- **`importer/writer.rs`** keeps a private `map_db_error(e, data_root)`. It wraps `db_err` and only puts the real data-root path into the disk-full error. Both of its callers pass `cfg.data_root`.
- **`sql_console/execute.rs`:** the no-SQLSTATE arm of `map_sql_error` now calls `db::no_sqlstate_err`. The console's own SQLSTATE table is unchanged.
- **Statement timeout:**
  - `search/execute.rs:77` and `:124` now send `SET LOCAL statement_timeout = {t}`, with `t` in milliseconds.
  - In `sql_console/mod.rs` the `SET_TIMEOUT` constant is deleted. Both call sites build the statement from `TIMEOUT_MS`.
- **Mtime truncation:**
  - `importer/queue.rs` now calls `writer::truncate_to_micros` at its 3 call sites.
  - The queue unit test `mtime_truncated_to_micros` now tests `truncate_to_micros`.
  - I added one assertion for a time before 2000-01-01. It fails if the Unix-epoch version comes back.

## Acceptance
- **A1 PASS:** the only definitions are `db_err` and `pool_err` in `db/mod.rs`. `pg_err` has 0 hits.
- **A2 PASS:** 0 hits.
- **A3 FAIL as written:** 2 hits, at `db/mod.rs:60` and `lifecycle/status.rs:15`.
  - The second hit is the 4-count query, which item 2 says to keep. The grep text also matches it.
  - Lead decides: accept it as is, or split the status query into `library_counts` plus a separate image_blobs/attachments query. The split costs one extra round trip.
- **A4 PASS:** 0 hits.
- **A5 PASS:** 0 hits.
- **A6 PASS (my local runs):**
  - `db::tests::pool_closed_is_not_ready` and `db::tests::sqlstate_policy` pass.
  - The new test `db_schema` `settings_get_uses_cached_quality_after_pool_closed` passes. `db_schema` is 9/9 (`.claude/tmp/impl1-dbschema.log`).
  - Red check: I copied `db/mod.rs` to a backup and changed `Closed` to map to `Internal`. The test failed (`.claude/tmp/impl1-red.log`). I restored the file from the backup.
- **A7 PENDING:** the full gate has not run yet. Local clippy exits 0 (`.claude/tmp/impl1-clippy2.log`).

## Error-code changes
No existing test assertion was changed.

| Where | Error | Before | After |
|---|---|---|---|
| Queue and SQL console pools | Backend connect error that has a SQLSTATE (for example 28P01, bad password) | `DbNotReady` | `Internal` |
| Queue `db_err`; no-SQLSTATE arm of the SQL console | Error without SQLSTATE that is not a closed connection or I/O (for example wrong row count, type conversion) | `DbNotReady` | `Internal` |
| Settings, supervisor counts, `db_status`, all old `pg_err` sites (indexes, start, migrate, maintenance, saved queries) | Closed pool, closed connection, I/O error | `Internal` | `DbNotReady` |
| All paths except the SQL console's own SQLSTATE table | 57P01, 57P02, 57P03 | `Internal` | `DbNotReady` |
| `pg_err` paths and search | 53100 (disk full) | `Internal` | `FileAccess insufficientSpace` |

Notes on the table:
- Second row: in the queue, such an error now fails the job instead of pausing it.
- Third row: this fixes the bug where settings did not fall back to the cached quality.
- Fifth row: the path is `""` everywhere except the writer, which passes the data root. The writer and remove paths already had this mapping.

## Uncertain
- **Pg-crash tests:** the second row of the table could appear as a failed import in these tests if a crash gives an error that is neither a closed connection nor I/O. Gate commands 14–16 cover this.
- **Mtime before 2000-01-01:** the queue's truncation now rounds toward 2000. This matches what PostgreSQL stores.
