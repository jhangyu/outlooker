# PST sync round 2: sync-path fixes (review round 1 findings 1-3, contract override 9)

Branch `pst-sync-r2-fixes` (worktree `../outlooker-r2-fixes`), base 4d86726. All paths below are under
`src-tauri/crates/outlooker-core/`.

## Fixes

1. **A failed removal is not re-imported** (`src/importer/queue.rs`, `sync_one`). Every sync step skips a path whose
   newest job is a removal in any state, including `failed`. The user retries Remove; until it succeeds, Sync now on
   that path is a no-op too (as the review recommended).
2. **Poison items from earlier jobs are skipped** (`src/importer/pipeline.rs` `attempt`, `src/importer/pst_reader.rs`).
   `attempt` loads the item ids of earlier poison rows (`stage = 'parse'`, message = `POISON_MESSAGE`,
   `pst_item_id > 0`) and passes them to the reader as a new `ReaderParams::poison` set. The reader (real and
   synthetic) sends such an item as unreadable with the poison message without reading it, so it still counts as an
   error (not as a duplicate) and its error row is the same as before (see 3: it is not duplicated).
   Known limitation: a death before the item id was read leaves a marker with `pst_item_id = 0`; such an item cannot be
   keyed and is retried by a later job (the same-job attempts rule still applies).
3. **Error rows and the error count do not grow per sync** (`src/importer/writer.rs` `insert_errors_sql!`, used by
   the writer batch insert, the scan rows in `pipeline.rs`, the job_failed row in `runner.rs` `set_final` and in
   `sync_one`). An error row whose content (stage, severity, folder path, item id, message) an earlier job of the same
   source already recorded is not inserted again; the existing row moves to the new job (`job_id`, `occurred_at`).
   Effect: one row per distinct error per source; the list count (`LIST_SQL`, unchanged: all rows of the source)
   stays flat; the newest failed job still finds its own `job_failed` row, so `failureReason` keeps working; the error
   log shows when each error was last seen. Rows of the same job never match each other (two items without an id in
   one folder with the same message stay two rows). The writer skips the statement for batches without errors.
4. **Blocking file I/O off the async workers, without locks** (override 9). `sync_one` now: reads the path, newest
   job and checkpoint identity with a plain query; runs stat + probe in `spawn_blocking`; then opens the transaction,
   locks the source row and the newest job (`newest_job_for_update`) and applies the result only if the newest job id
   is still the one it saw (else Skipped; the next sync looks again). `sync` runs the paths concurrently (tokio
   `JoinSet`), emits status and kicks the worker per path as soon as that path queued a job, and returns the first
   error after all paths ran. `sync_lock` is deleted: with the I/O moved out it guarded nothing that the row lock plus
   the job-id re-check does not already guarantee (two concurrent syncs of one path: the second sees a new newest job
   and skips). The two probes in `attempt` (job start, and the re-probe after a libpff open failure) use the new
   `probe::probe_blocking`. Test seam `probe::hold_probe(path)` (feature test-support) blocks `probe(path)` until the
   returned guard drops.

## Tests (tests/it/import_pipeline.rs)

| Test | Guards | Red (fix removed) | Green solo |
|---|---|---|---|
| `sync_skips_path_after_failed_removal` | finding 1 | import jobs `left: 2, right: 1` | 0.39 s |
| `sync_skips_earlier_poison_items` | finding 2 | "poison item was read" rows `left: 1, right: 0` | 0.28 s |
| `sync_repeated_errors_keep_one_row` | finding 3 (item rows + job_failed rows, list count, failureReason) | rows per source `left: (2, 2), right: (1, 1)` | 0.23 s |
| `sync_blocked_probe_does_not_delay_other_paths` | override 9 (acceptance 2 / R4) | fast path completion `left: None, right: Some(Completed)` | 0.42 s |

Red run: the four fixes were mutated away on the files after copying them to `scripts/tmp/bak/` (sync without the
Remove check, empty poison set, dedupe match forced false, `sync` awaiting each path before the next), one test
binary, `cargo test --workspace --test it sync_`: exactly these 4 failed at the assertions above, the 3 older sync
tests passed. Files restored from the backups (grep for the mutation markers: 0 hits), then green.
The override-9 test holds the probe of the path that sorts first and requires the second path to be synced and
imported by the worker within 3 s while the first probe still hangs, and `sync` not to have returned yet.

## Commands (src-tauri/, CARGO_TARGET_DIR=<worktree>/src-tauri/target)

- `cargo test --workspace --test it`: 84 passed, 0 failed, 1 ignored (`warm_template`, pre-existing), 8.16 s.
- `cargo test --workspace --lib --no-fail-fast`: outlooker_lib 7 passed; outlooker_core 125 passed, 1 failed
  (`importer::normalize::tests::golden_inline`, owned by another member); loadgen, pff 0 tests.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean (RC=0).

## Not done / for the lead

- `register` (`queue.rs` around lines 319 and 328) still calls `probe` and `std::fs::metadata` on the async worker.
  It holds no row lock or sync lock there, but a slow path still blocks one tokio worker during registration.
  `register` is owned by impl-r2-actions-opus in this round; the fix is one line per call
  (`probe_blocking(path.clone()).await`, and the metadata call inside the same `spawn_blocking`).
- `LIST_SQL` error count left as is (all rows of the source): with the dedupe it no longer grows per sync. It still
  counts errors that stopped happening; counting only the newest run's rows would need the "newest completed job when
  the newest is locked/unchanged" rule from the review and was not needed for R1.
- The dedupe match uses the `import_errors_page` index (pst_id prefix) and filters the rest; a source with tens of
  thousands of error rows may want an index on `(pst_id, pst_item_id)` (marked `ponytail:` in writer.rs). No schema
  change was made, to avoid touching V1 while another member edits it.
- Merge note: `ReaderParams` gained a `poison` field (the one test that builds `ReaderParams` is updated);
  `queue::sync` now takes `&Arc<Inner>` (only caller: `ImportManager::sync`, unchanged).
