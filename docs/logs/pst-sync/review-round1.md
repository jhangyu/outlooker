# PST scheduled sync — adversarial review, round 1

Scope: `git diff 806eae0..HEAD -- src-tauri src` (commit 4c15645 excluded). Dimensions: correctness / data integrity, security.
Base HEAD: fdf4f8f.

Verdict: **merge OK** — no blocker. 3 should-fix, all introduced by this change.

Tests run (src-tauri/, canonical commands), all green:
- `cargo test --workspace --lib schedule` — 9 passed
- `cargo test --workspace --test it import_pipeline` — 23 passed (incl. sync_changed_file_keeps_one_row_and_skips_known, sync_unchanged_creates_no_job, sync_locked_no_error_row_no_finalize, register_same_path_rejected)
- `cargo test --workspace --test it app_compose` — 3 passed
- `cargo test --workspace --lib probe` — 2 passed

Checked and found sound (no finding): append-only (no sync path deletes occurrences, messages or folders; sync never calls remove);
Disable only flips `sync_enabled`; Remove deletes only messages with no occurrence under another pst_id (`importer/remove.rs:15-29`);
known-item skip with CP_KEEP / CP_RESET (cancel, crash and changed-file resume); duplicate counting `mails - stored + known`;
locked run (no finalize, no VACUUM, no error row, one row per streak, probe before `Checkpoint::begin`); `is_lock` has no cfg;
scheduler overlap (single task + `sync_lock` + row `FOR UPDATE`), catch-up once, off mode, DST gap/fold, weekday wrap,
clock moved back, shutdown order (scheduler stopped before importer drain); `time` 0.3.55 `local_offset_at` on Unix has no
single-thread guard (`sys/local_offset_at/unix.rs`), so the local zone is really used; schedule and path validation at the command boundary.

---

### [should-fix] A failed removal is undone by the next scheduled sync

Location: `src-tauri/crates/outlooker-core/src/importer/queue.rs:921-935`
Dimension: Correctness / data integrity
Rule: Correctness
Severity: Medium (High impact: mail the user asked to delete comes back and the source row stays; likelihood possible — removal fails on any non-DbNotReady DB error, `runner.rs:147-153`)
Origin: introduced
Tags: none
Register: none, Ticket: TBD, Owner: TBD

Evidence:
`sync_one` skips only `Queued | Running | Cancelling`. A removal that ended `(Remove, Failed)` is not skipped, `unchanged` is
false (it requires `(Import, Completed)`), the probe says Ready, and a new `import` job is inserted. `remove::run` deletes
occurrences in autocommitted 5,000-row chunks, so a failed removal has already deleted part of the occurrences (and the
messages only this path held). Those items are no longer "known", so the re-import reads and stores them again.
arch-2 §4 says "Skip also when the newest job is `removing`", but `status_of(Remove, Failed)` is `Failed`, not `Removing`.

Impact:
User clicks Remove, the removal fails half-way, and within one schedule interval (default 60 min) the scheduler re-imports
the deleted mail into the path the user wanted gone. Silent; the UI then shows "completed".

Recommended Fix:
In `sync_one`, skip every newest job of kind Remove, whatever its state:
`if kind == JobKind::Remove || matches!(state, JobState::Queued | JobState::Running | JobState::Cancelling) { return Ok(SyncStep::Skipped); }`
(the user retries Remove; Sync now on that path then also stays a no-op until the removal succeeds).

### [should-fix] A poison item (one that crashes the process) is re-read on every changed-file sync

Location: `src-tauri/crates/outlooker-core/src/importer/pipeline.rs:428-430`, `src-tauri/crates/outlooker-core/src/importer/pst_reader.rs:290-304`
Dimension: Correctness / data integrity
Rule: Correctness
Severity: Medium (High impact: the app process dies twice per sync; likelihood possible — the poison marker exists because libpff can kill the process on a malformed item)
Origin: introduced
Tags: none
Register: none, Ticket: TBD, Owner: TBD

Evidence:
The skip set is only `SELECT pst_item_id FROM message_occurrences WHERE pst_id = $1`. A poison item gets an
`Unreadable{POISON_MESSAGE}` error row but no occurrence. The poison marker is honoured only for the same `job_id`
(`pipeline.rs:166` filters `r.job_id == job_id`), and every sync is a new job. So each new job reads the item again:
crash, restart, recovery requeues the same job, attempts = 2, crash again, then skip.
Outlook rewrites the PST mtime almost every session (arch-2 §2), so "changed" is the normal case and the default schedule is hourly.
Before this change a re-read happened only on a user-initiated import; now the scheduler drives it unattended.

Impact:
For a PST with one such item, the app crashes twice on (nearly) every scheduled sync while Outlook is in use.

Recommended Fix:
Add the ids of earlier poison skips to the known set in `attempt`:
`SELECT pst_item_id FROM message_occurrences WHERE pst_id = $1 UNION SELECT pst_item_id FROM import_errors WHERE pst_id = $1 AND message = $2 AND pst_item_id > 0`
with `$2 = POISON_MESSAGE` (better: a dedicated stage/flag instead of matching the message text). Residual: a crash before the
item id was read (marker `pst_item_id = 0`) cannot be keyed by id; note it as a known limitation.

### [should-fix] Error rows and the per-path error count grow with every sync

Location: `src-tauri/crates/outlooker-core/src/importer/queue.rs:193` (count over all jobs), `src-tauri/crates/outlooker-core/src/importer/pipeline.rs:206` (scan errors per attempt), `src-tauri/crates/outlooker-core/src/importer/queue.rs:921-935` (Failed newest + Ready → new job)
Dimension: Correctness / data integrity
Rule: Correctness
Severity: Medium (partial impact: misleading count and unbounded table growth, no mail lost; likelihood likely for any PST with an unreadable item or folder)
Origin: introduced
Tags: none
Register: none, Ticket: TBD, Owner: TBD

Evidence:
With one source row per path, every changed-file sync is a new job on the same `pst_id`. Unreadable items have no occurrence,
so they are re-read and write a new `item_skipped` row each sync (arch-2 §2 chose the retry, but not the de-duplication of
its error rows); degraded folders write a new `scan` row each attempt (`pipeline.rs:206`). A deterministic job failure
(e.g. libpff cannot parse the file but the magic is fine) is re-queued on every slot because a `Failed` newest job plus
`Probe::Ready` inserts a new job, adding one `job_failed` row per run. `errorCount` in `LIST_SQL` counts all rows of the
path across all jobs; the Import screen shows the error log when `errorCount > 0` (`src/screens/import/index.tsx:340`).
Before this change each re-import was a new source row, so counts were per snapshot.

Impact:
A PST with 50 unreadable items gains about 1,200 error rows a day at the default interval; the shown count and the error log
fill with duplicates and stop telling the user what is wrong now.

Recommended Fix:
Pick one: (a) count only the newest job's rows in `LIST_SQL` (`AND e.job_id = j.id`, plus the newest completed job when the
newest is locked); or (b) before writing an item/scan error, skip it when the same `(pst_id, stage, pst_item_id or folder_path)`
already has a row (e.g. a partial unique index + `ON CONFLICT DO NOTHING` in `INS_ERRORS`). (a) is one line and keeps history.

---

## Parking-lot suggestions (outside the contract acceptance; no severity)

1. User Cancel lasts only until the next scheduled run: a `Cancelled` newest job plus `Probe::Ready` re-queues it (`queue.rs:931-935`).
   The only way to stop a long sync for good is Disable. Decide whether that is wanted and say it in the UI.
2. Failed streak keeps a stale reason: `Probe::Unavailable` with a `Failed` newest job does nothing (`queue.rs:945`), so a path whose
   last job failed for another reason (e.g. disk full) and whose file is now missing still shows the old `failureReason`.
3. `sync_one` does blocking file I/O (`std::fs::metadata`, `probe`) on a tokio worker while holding the `pst_sources` row lock and
   `sync_lock` (`queue.rs:928-932`). A PST on a slow network share can stall the worker thread and the import worker's
   `UPDATE pst_sources` for the length of an SMB timeout. `spawn_blocking` around the probe would avoid it.
4. Path identity is the raw string (`UNIQUE (path)` with `COLLATE "C"`): on Windows `C:\Mail\a.pst` and `c:\mail\A.PST` register as two
   paths (two list entries, double parse cost; mail is still deduped). Canonicalize at `pst_register` if this matters.
5. `sync/mod.rs:17-18` says re-reading the wall clock also covers zone changes. On Unix `localtime_r` is not required to re-read the
   zone (it may not call `tzset`); unverified whether a zone change while the app runs is picked up before restart.
