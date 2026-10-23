# Backend code-reading brief for PST scheduled sync

Read-only. Paths below are relative to `src-tauri/crates/outlooker-core/src/` ("core/") unless stated. Everything here was read in code; items marked UNVERIFIED were not run.

## 0. Contradictions with the requirements doc and framing

- The reader does NOT open the PST with `std::fs::File::open`. It calls `pff::File::open` (libpff FFI): `core/importer/pst_reader.rs:251`, `crates/pff/src/lib.rs:143-159`. `std::fs::File::open` appears only in `check_file` (`importer/queue.rs:276`), which reads 4 magic bytes. So there are two separate opens (see §2).
- Requirements doc says "Unchanged file (size + mtime) = skip without opening". Today that is true only at enqueue: `check_file` stats the file and opens it for 4 bytes, then the UNIQUE key returns `AlreadyImported` (`queue.rs:303-310`). libpff is never opened in that case. A resume of an existing job (same size and mtime) does re-open with libpff and re-scan the folder tree.

## 1. Import lifecycle end to end

**Enqueue** (`importer/queue.rs:317-341`): at most 100 paths. For each path: `check_file` (`:265-282`), then `insert_source` (`:292-315`).
- `check_file` requires: absolute path, `metadata` ok, is a file, first 4 bytes are `!BDN` (`:36`). Any open or read failure maps to `FileAccess{reason:"unreadable"}` and the OS error kind is lost (`:276`). It returns size and mtime truncated to microseconds (`importer/writer.rs:89-95`).
- `insert_source`: in ONE transaction it inserts `pst_sources` with `ON CONFLICT ON CONSTRAINT pst_sources_same_file_once DO NOTHING`, then `import_jobs(kind='import', state='queued')`. If the insert conflicted, it looks up the existing id and returns `AlreadyImported{pst_id}` (`:303-310`). The AlreadyImported identity rule is exactly `(path, file_size_bytes, file_mtime)` (`db/migrations/V1__schema.sql:14`).
- After the loop, `inner.kick.notify_one()` wakes the worker (`:339`).

**Job states**: `import_jobs.state` is queued, running, cancelling, cancelled, failed, completed (`queue.rs:158-169`, `V1__schema.sql:127`). `kind` is import or remove. Status shown in the UI comes from `status_of` (`queue.rs:172-183`) of the NEWEST job of the PST (`LIST_SQL`, `:185-193`).
- Schema guards: partial unique `import_jobs_one_active_per_pst` (`V1__schema.sql:184`, at most one queued/running/cancelling job per pst) and `import_jobs_one_running` (`:185`, at most one running job in the whole database).

**Runner** (`importer/runner.rs`): one worker task, spawned once by `start_worker` (`queue.rs:509-514`). Loop (`runner.rs:35-81`): waits until `AppPhase::Ready` (`:41`), selects the oldest queued job (`next_job`, `:83-93`, `ORDER BY id LIMIT 1`), takes the Work Gate (`:55`), runs the job (`run_job`, `:96-126`: queued to running with `started_at`), drops the gate. It sleeps on `kick`, phase change or shutdown (`:75-79`).

**Pipeline stages** (`importer/pipeline.rs:70-134`, `attempt`):
1. Read pst path, size and mtime from `pst_sources`; `file_identity` re-stats the file (`:136-143`). A stat failure gives `JobFailure{stage: Open, "cannot open PST file: ..."}`.
2. `Checkpoint::begin` (`writer.rs:131-140`): if the checkpoint's `(pst_file_size, pst_file_mtime)` equals the file's current values it keeps cursor and counters (`CP_KEEP`), else it RESETS cursor and counters to 0 (`CP_RESET`). The reset happens before the libpff open.
3. Reader thread (`pst_reader.rs:250-316`) opens the PST with libpff, scans the whole folder tree, sends `Scanned`, then reads every message with `seq > cursor_seq` (`:272`). `seq` is the position in a deterministic folder walk (`:268-273`).
4. `record_scan` upserts folders, writes scan error rows, sets phase `importing` with `items_total` (`pipeline.rs:193-205`).
5. Index policy: search indexes are dropped when remaining items are at least half of existing messages (`:209-217`).
6. Normalize on a rayon pool, writer task with a dedicated connection (`:100-110`, `spawn_pipeline` `:249-293`). The writer commits a batch (default max 1000 items, 64 MB, 1 s) in one transaction together with the checkpoint (`writer.rs:25-36`, `:398-444`).
7. Finish: `finalize` (indexes plus `VACUUM ANALYZE` of 7 tables, `runner.rs:284-298`), then `set_final` (`runner.rs:159-179`).

**Resume after crash or restart**:
- `start_services` calls `recover_after_crash` (`queue.rs:479-501`, `app/mod.rs:94`): running to queued, cancelling to cancelled.
- A graceful shutdown cancels with reason Shutdown and the job goes back to `queued` (`runner.rs:218`, `:253-263`). On the next start the same job resumes from `cursor_seq`, provided size and mtime are unchanged.
- Manual `resume` (`queue.rs:379-395`) only works when the newest job is import and cancelled or failed. It calls `check_file` first (`:389`) and inserts a new queued job for the SAME `pst_id`.
- A DB loss mid-job is a Paused attempt that waits and re-runs from the checkpoint (`runner.rs:184-199`, `:225-247`).
- Poison-item marker file `import-inflight` (`importer/inflight.rs`, `paths/mod.rs:41`) handles libpff crashes; it is per job id and is ignored after a checkpoint reset (`pipeline.rs:158`).

**Cancellation** (`queue.rs:356-377`): queued job goes straight to cancelled. A running job goes to `cancelling` and the in-memory cancel flag is set with reason User (`JobCancel`, `:42-84`). The reader checks the flag per item (`pst_reader.rs:275`). End decision table: `end_of_attempt` (`runner.rs:214-222`, unit test `:304-333`). A failure beats any cancel reason.

**Completion stats / "new messages"**:
- Per batch, `stored` = row count returned by `INS_MESSAGES` (`ON CONFLICT (dedupe_key) DO NOTHING`), i.e. messages actually inserted (`writer.rs:407`, `:220-234`). `duplicates = mails - stored` (`:434`). Also `errors`, `skipped_non_mail`, `images_converted` (`:419-437`).
- Counters accumulate in `import_checkpoints` via `CP_BATCH` (`:77-85`). `PstSource.new_message_count = progress.stored`, `message_count = stored + duplicates` (`queue.rs:238-239`).
- The checkpoint is keyed by `pst_id` (one row, `V1__schema.sql:136-151`). If the same `pst_id` runs a second job with unchanged identity, `CP_KEEP` keeps the old counters, so "new this run" cannot be read from the checkpoint without a reset or a delta.

## 2. How the PST is opened and what a lock looks like today

Four places touch the file before or during an import:
1. `check_file` at enqueue and at manual resume: `std::fs::metadata` plus `std::fs::File::open` plus `read_exact(4)` (`queue.rs:271-276`). Any failure on the open or read maps to `FileAccess{path, reason:"unreadable"}`. A missing file is `notFound`; wrong magic is `NotAPst`. No lock-specific class exists.
2. `file_identity`: `std::fs::metadata` only, at the start of every attempt (`pipeline.rs:138`). Error becomes `JobFailure{Open}`.
3. Reader: `pff::File::open(&p.path)` (`pst_reader.rs:251`). Failure sends `ReaderMsg::Failed{stage: Open, message: e.message}` (`:253-255`).
4. `pff::File::open` builds a message string only: `"libpff_file_open: <libpff_error_sprint text>"` (`crates/pff/src/lib.rs:43-51`, `:150`). `pff::Error` has only `message: String` (`:19-21`); no OS error code, no kind.

Windows detail from the vendored C (`src-tauri/native/libpff/libcfile/libcfile_file.c:336-352`, `:410-430`, `:440-465`): read-only open uses `CreateFileA` with `FILE_SHARE_READ | FILE_SHARE_WRITE` and `OPEN_EXISTING`. A sharing violation is not a special case there; it falls into the default branch ("unable to open file: <path>" with the Win32 error code attached via `libcerror_system_set_error`). UNVERIFIED on a real Windows box: the exact text, and whether Outlook's own share mode makes this open fail at all (depends on Outlook's handle mode). Note FILE_SHARE_WRITE is requested, so if Outlook holds the file with write access and denies readers, the open fails; if Outlook allows readers, the open succeeds and libpff reads a file that is being written.

macOS/Unix: libpff uses plain `open(2)` (only `fcntl.h` included for flags; no `flock`/`lockf`/`fcntl` locking calls in `libcfile_file.c`). So a lock can never occur on macOS. A same-code-path lock classification can only be exercised on macOS by a permission-denied file, not a real lock. This is a test-design constraint.

What an open error becomes today (`pipeline.rs:86-94`, `first_message` `:179-190`):
- Reader failure becomes `JobFailure{Open, message}` becomes `AttemptEnd::Failed` (`runner.rs:217`) becomes `Finalize(Failed)`. Before the final state is set, `finalize` still runs (set phase, `ensure_search_indexes`, `VACUUM ANALYZE` of 7 tables; `runner.rs:270`, `:284-298`). `set_final` then writes one `import_errors` row (`severity job_failed`, `stage open`, `runner.rs:163-171`) and sets `failed` (`:172-178`). `emit_status` shows `ImportStatus::Failed`.
- Consequences for a retry-every-N-minutes lock skip: each locked attempt costs a finalize (admin connection plus VACUUM) and leaves one more `job_failed` row. It would also reset the checkpoint first if the file changed (`pipeline.rs:81`, before the open). Lock handling needs its own early branch before `Checkpoint::begin`, or a probe before enqueueing a job.
- Error rows have no `retry` or `locked` concept. `ImportErrorStage::Open` exists (`dto/import.rs:125-132`), `V1__schema.sql:158` CHECK allows `open`.
- `AppError` variants relevant: `FileAccess{path,reason}` (`dto/error.rs:25`), `Busy{reason}` (`:23`), `AlreadyImported{pst_id}` (`:29`).

## 3. Dedupe, occurrences, re-import after change, cost

**Rows written per message per pst_source** (all in one batch transaction, `writer.rs:398-444`):
- `folders` rows are written at scan time, once per `(pst_id, pst_folder_id)` via upsert (`writer.rs:151-165`, UNIQUE `folders_pst_folder_once` `V1__schema.sql:24`). Re-scan of the same pst_id updates in place.
- `messages` row only if new (`ON CONFLICT (dedupe_key) DO NOTHING`, `writer.rs:220-234`); with `first_pst_id = this pst`. Recipients, attachments, inline images are inserted only for newly stored messages (they join `stage_new_ids`, `:236-264`).
- `message_occurrences (pst_id, pst_item_id, message_id, folder_id)` for EVERY mail item, new or duplicate (`:266-271`, `ON CONFLICT (pst_id, pst_item_id) DO NOTHING`). PK `(pst_id, pst_item_id)` (`V1__schema.sql:85`). Non-mail items and skipped items get no occurrence (only `Outcome::Mail` is staged for messages; see `writer.rs:419-421`).
- Dedupe key: Message-ID (BLAKE3 of normalized id) else a content hash of sender, sent time, subject, body (`importer/dedupe.rs:12-39`). Dedupe is decided in SQL only (`messages_dedupe_once`, `V1__schema.sql:63`).

**Same path imported again after the file changed**: size or mtime differs, so `insert_source` creates a NEW `pst_sources` row with a new id (`queue.rs:297`, `V1__schema.sql:14`). Result:
- Import list gets a second entry for the same path (this is what the requirements doc calls out).
- New `folders` rows (a full copy of the folder tree under the new pst_id) and new `message_occurrences` rows for every mail item (duplicates included), a fresh checkpoint starting at `seq` 0, new job. The old row, its folders, occurrences, checkpoint and errors remain.
- `stored` counts only messages whose dedupe_key is new, so it is exactly the "new messages this run" for a new-row design.
- Reusing the SAME `pst_id` for a changed file is also possible in code but is a different path: `Checkpoint::begin` detects a size/mtime change and resets cursor and counters (`writer.rs:131-140`, log line `pipeline.rs:83`). But `pst_sources.file_size_bytes`/`file_mtime` are NOT updated anywhere (only `import_checkpoints` is), and the UNIQUE constraint is on those columns. No code path updates `pst_sources` identity after insert (grep of `UPDATE pst_sources`: none in `core/`). Reusing one row per registered path would require a new UPDATE of those two columns (and the constraint would then hold per current identity).

**Cost of re-parsing**:
- Unchanged file: free at enqueue (see §0). Resumed with unchanged identity: items with `seq <= cursor_seq` are not read at all (`pst_reader.rs:272-274`), but the folder scan runs again (`:258`).
- Changed file: no per-item skip exists. For every item after the cursor the reader reads all properties, bodies (html, rtf, plain), recipients and attachment metadata, plus attachment bytes for content-id attachments up to 25 MB (`pst_reader.rs:168-234`, `MAX_ATTACHMENT_DATA` `:20`). Then `normalize` runs on the rayon pool: HTML sanitize, RTF, charset, WebP conversion (`importer/normalize/mod.rs:28-60`). Dedupe happens only afterwards in SQL. `known` image hashes only avoid re-encoding duplicate images (`pipeline.rs:101`, `:109`), not parsing.
- Nothing reads item modification time or checks "already have this `pst_item_id`" before parsing. `message_occurrences` already holds `pst_item_id` (libpff identifier, `pst_reader.rs:300`) per `pst_id`, so a "skip items whose identifier is already stored for this registered path" design is possible with data we already have. UNVERIFIED whether libpff identifiers are stable across PST edits/compaction/repair and whether an identifier can be reused after delete; the code does not rely on it.
- `seq` is the position in the folder walk, so it shifts when mail is inserted mid-walk. That is why a changed file resets the cursor instead of reusing it.

## 4. Removal (`importer/remove.rs`) and shared messages

- Remove job: `queue::remove` (`queue.rs:397-417`) refuses when the newest job is running or cancelling (`Busy`) or already removing; cancels a queued import and inserts a `remove` job. Runs through the same worker and the Removal gate activity (`runner.rs:54`, `:128-157`).
- `remove::run` (`remove.rs:38-63`): loops 5000-occurrence chunks of this pst, deleting occurrences, then deleting a message only if no occurrence of another pst remains (`CHUNK`, `:15-29`). Then, in one transaction, deletes orphan image blobs and addresses and the `pst_sources` row (`:50-60`, `:31-35`), then `VACUUM`. `pst_sources` delete cascades to folders, import_jobs, checkpoints, errors, occurrences (`V1__schema.sql:19,83,125,137,155`); `messages.first_pst_id` is `ON DELETE SET NULL` (`:61`).
- Design hazard: a message that exists only in an old snapshot row (deleted in Outlook before the newer snapshot) has an occurrence only under the old `pst_id`. Removing that old source row deletes the message. So the append-only rule is broken if a design cleans up old sources after a changed-file sync. Either keep old rows, or reuse one row per path and never run remove from sync.
- Removing a registered path's row removes messages that only it holds, which is correct for the explicit "remove PST" action, but a "disable/unregister path" action must not call it.

## 5. Work Gate and queue concurrency

- `gate/mod.rs:8-13` activities: Import, Removal, Maintenance, DataDirMove. One slot only: `try_enter` fails with `AppError::Busy{reason}` for ANY held activity, including another Import (`:36-43`, unit test `:90-103`). `released()` waits for the slot to free (`:50-58`).
- Users of the gate: worker per job (`runner.rs:54-64`), `self_heal_indexes` at startup (`queue.rs:504`), maintenance (`maintenance/mod.rs:80`), data-dir move (`lifecycle/move_dir.rs:84`).
- Queue: strictly one job at a time (gate slot, plus DB index `import_jobs_one_running`). Order is by job id (FIFO).
- A scheduler does not need to hold the gate itself. It only needs to enqueue a queued `import_jobs` row and `kick` the worker; the worker takes the gate, and if the gate is busy (Maintenance, DataDirMove) it waits on `released()` and retries (`runner.rs:55-63`). Note `enqueue` itself never checks the gate. `import_enqueue` through `ImportManager::enqueue` is the existing entry; `ImportManager.inner.kick` is private (`importer/mod.rs:52`), so a scheduler inside `importer` or a new public method is needed.
- Scheduler-side gaps: a lock-skip that runs as an ordinary job would hold the gate (Import) for a short time and also blocks maintenance during that time; a pre-open probe before creating the job avoids the gate and job row entirely. Per-pst active-job uniqueness (`import_jobs_one_active_per_pst`) already prevents double-enqueue for the same pst_id when a manual and a scheduled trigger collide (second insert would error; today `resume` does not catch that conflict explicitly: `queue.rs:390`).

## 6. Startup / shutdown hooks and timer patterns

- Process start: Tauri `setup` builds services (`src/lib.rs:50-60`) and spawns `start_services` (`src/lib.rs:63-67`). `app::build` spawns nothing (`app/mod.rs:46-65`). `start_services` (`app/mod.rs:82-104`): boot DB, wait for `Ready` (`:88-92`, first run waits for onboarding), `recover_after_crash`, `self_heal_indexes`, `start_worker`. `StartStep` enum (`:67-72`) and `StartReport` list the steps. Natural scheduler start point: after `start_worker` (`:101`) (add a step). Catch-up check ("missed while closed") also belongs there: it needs persisted last-run time (nothing stores one today).
- Shutdown: Tauri exit calls `run_shutdown` (`src/lib.rs:29-33`) which calls `app::shutdown` (`app/mod.rs:125-142`): `importer.shutdown(15 s)`, cancel SQL, stop DB. A scheduler must stop BEFORE `importer.shutdown` so it cannot enqueue during drain. The importer's shutdown flag (`queue.rs:136-139`) does not stop `enqueue`.
- Timer patterns already in code: (a) health monitor `tokio::spawn` with `tokio::time::interval` and `MissedTickBehavior::Delay`, started once guarded by an `AtomicBool` (`lifecycle/health.rs:12-34`, `lifecycle/supervisor.rs:40,66`); (b) the import worker waits on `Notify` + `watch<AppPhase>` + shutdown `watch<bool>` (`runner.rs:35-81`, `queue.rs:93-139`); (c) `tokio::time::timeout` for pauses (`runner.rs:192`). No cron or wall-clock-based scheduler exists (no `chrono`/cron grep needed; the only periodic task is the 5 s DB probe, `supervisor.rs:26`). Wall-clock "daily at HH:MM on weekdays" needs a time/zone crate decision (not checked here; check `Cargo.toml` before design).
- Phase gating: every worker action is skipped unless `AppPhase::Ready` (`runner.rs:41`); a scheduler should use the same phase watch (`supervisor.phase_watch()`, `app/mod.rs:58`).
- Persistence available for schedule state: `settings.json` bootstrap holds only `data_dir` and `language` (`settings/mod.rs:11-16`, atomic write via `fsutil.rs:10`); DB `settings` is a one-row table with only `webp_quality` (`V1__schema.sql:176-181`). Frontend/command layer coverage belongs to the other reader.

## 7. Reusable pieces and blockers

Reusable:
- `check_file` (stat plus magic) for registration validation; `insert_source`/`enqueue` for the job; `kick` for waking the worker; `cancel`, `errors`, `list`, `emit_status` and `ImportStatusChanged` events for UI; gate and worker for exclusion; `import_jobs.created_at/started_at/finished_at` for "last run" if one job per sync; `Checkpoint`/`CP_*` for resume; `write_atomic` (`fsutil.rs:10`) for any file state.
- `ImportManager::job_events()` broadcast `(pst_id, ImportStatus)` for terminal outcomes (`importer/mod.rs:96`) so a scheduler can learn when a sync finished without polling.

Blockers / constraints a design must handle:
1. `pst_sources` UNIQUE `(path, file_size_bytes, file_mtime)` (`V1__schema.sql:14`) makes identity per snapshot. Either add a registered-paths table that points at the latest snapshot, or allow an `UPDATE` of size/mtime on one row (then the unique key means "current identity" and a no-op skip stays `AlreadyImported`). Nothing in code updates those columns today.
2. `import_jobs_one_active_per_pst` (`V1__schema.sql:184`): a second trigger for the same pst while queued/running fails at insert.
3. `Checkpoint::begin` resets counters when identity changed (`writer.rs:131-140`); `stored` accumulates for an unchanged identity. "New messages this run" needs a baseline or a reset.
4. Open errors are an unstructured string (`pff::Error.message`) and `check_file` flattens everything to `unreadable`. Lock detection must be added; it can use `std::fs::OpenOptions` and `io::Error::raw_os_error()` before queuing a job, which is the same code on both OS (`ERROR_SHARING_VIOLATION` 32 only occurs on Windows, so macOS can only test the classifier with a permission-denied or injected error).
5. A failed-open job today runs `finalize` (index check plus `VACUUM`) and writes a `job_failed` row (`runner.rs:270`, `:163-171`); repeating that on every lock retry is expensive and noisy.
6. `import_errors.stage` CHECK values (`V1__schema.sql:158`) and `JobState`/`ImportStatus` enums are closed sets with DB CHECKs (`:127`, `:131`); a new "locked/waiting" state needs a migration (migrations live in `core/db/migrations/`, currently V1 and V2) and TS bindings (`#[ts(export)]` in `dto/import.rs`). The spec says no forward compatibility is needed, so V1 can be edited in place if the team lead allows it.
7. The `remove` hazard in §4 (never clean old snapshot rows after a sync).
8. `ImportManager.inner.kick` and `Inner` are crate-private (`importer/mod.rs:52`, `queue.rs:93`); new public methods go on `ImportManager`.
9. Role privileges: `outlooker_app` has `SELECT, INSERT, UPDATE, DELETE, MAINTAIN ON ALL TABLES` (`V1__schema.sql:205`), but it is `ON ALL TABLES` at migration time, and no `ALTER DEFAULT PRIVILEGES` exists in V1/V2 (`V2__roles_views.sql:46-50`, grep verified). A table added later needs its own GRANT; a table added by editing V1 is covered by V1's grant.
10. Cursor reuse: a changed-file design cannot reuse `cursor_seq` (positions shift); skip-by-`pst_item_id` is the only per-item skip candidate available from current data (UNVERIFIED stability).

## Spec cross-check

`docs/superpowers/specs/2026-10-21-outlooker-design.md`: §6.2 cancel/resume (`:539-546`), §6.3 poison item (`:548-558`), §5.9 checkpoint (`:1933-1960`) match the code read above (changed file means reset, resume re-runs scan, dedupe makes repeat harmless). No spec text mentions sharing violations or locked files (grep for "sharing|locked|in use" found only unrelated CSP lines).
