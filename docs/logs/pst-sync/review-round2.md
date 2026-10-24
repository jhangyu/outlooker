# PST scheduled sync — adversarial review, round 2

Scope: `git diff d103e9c..HEAD -- src-tauri src` at HEAD df000bc. Dimensions: correctness and data integrity, concurrency, security.

Verdict: **merge OK** — no blocker. 2 should-fix, 4 nits.

Tests run (src-tauri/, canonical commands, TMPDIR=scripts/tmp/t-rev2), all green:
- `cargo test --workspace --test it import_pipeline` — 29 passed (incl. pause_resume_continues_from_checkpoint,
  unregister_keeps_mail_and_reregister_adds_no_duplicates, register_same_path_rejected with the case variant on macOS,
  sync_repeated_errors_keep_one_row, sync_changed_failure_reason_writes_one_new_row, sync_blocked_probe_does_not_delay_other_paths,
  pst_remove_with_queued_job)
- `cargo test --workspace --lib importer` — 45 passed (incl. windows_verbatim_prefix_dropped, end_of_attempt_table)
- Probe (temporary test appended to a cp backup of tests/it/import_pipeline.rs, file restored from the backup, `git status` clean):
  pause mid-job, change the file, resume → error rows `before=1 after=2 listed=2` (finding 1).
- Scratch check: macOS `realpath(3)` (what `std::fs::canonicalize` calls) maps `CAFÉ.PST`, NFC `café.pst` and NFD `café.pst`
  to the one on-disk name, so case and Unicode-form variants register once on macOS.

Checked and found sound (no finding):
- Concurrency without sync_lock: `sync_one` re-locks the source row and the newest job (`newest_job_for_update`, queue.rs:576-586)
  and acts only when the newest job id is unchanged (queue.rs:492-495); two syncs of one path cannot both insert
  (and `import_jobs_one_active_per_pst` backs it). A Delete or register that adds a job makes the stale sync skip.
- Lock order: every writer takes the source row before the job row (set_paused, unregister, remove, sync_one, insert_source,
  set_final via the import_errors FK); no lock cycle found.
- Pause vs job start: the job is registered in memory before its row turns `running` (runner.rs:102-117). A pause committed first
  makes the UPDATE match 0 rows; one that commits later (including the EvalPlanQual case where the UPDATE's subquery still sees
  the old `paused = false`) finds the registered job and sets Hold. The worker skips queued jobs of paused/unregistered paths
  (runner.rs:84-90), so a held job never blocks other paths. Hold during finalize is ignored (EndAction already chosen), the
  job completes and the path stays paused: no corruption. Hold after the last item requeues; Resume re-runs from the checkpoint
  and finalizes.
- Resume: same job, CP_KEEP when the identity is unchanged, CP_RESET plus known-id skip when the file changed (test covers it).
- Unregister / re-register: mail, folders and occurrences kept; search and `mail_get` work (test); re-register reuses the row
  under `FOR UPDATE`, resumes a held job instead of adding one, concurrent re-registers serialize to one AlreadyImported.
- Delete of a queued job: the job row is deleted, its checkpoint cascades (`import_checkpoints.job_id ON DELETE CASCADE`),
  error rows get `job_id NULL` (`ON DELETE SET NULL`); the removal deletes the source and everything under it. A worker that
  picked that job finds 0 rows. Crash recovery requeues a running removal (idempotent). A failed removal maps to `Failed`
  (queue.rs:173), so Delete can be retried and `sync_one` skips the path (round-1 finding 1 holds).
- Poison skip across syncs: keyed by `pst_item_id > 0`, not by job, so the dedupe move of rows does not hide it.
- Fresh failure reason: same reason checked against the newest failed job's own row; new reason → one job + one row; the
  moved-row dedupe keeps `job_failed` rows at one per distinct reason, so alternating reasons do not grow error rows.
- `plain_path`: drive and UNC verbatim prefixes stripped, `\\?\Volume{..}` and long paths kept; canonical form is the same for
  every spelling, so identity holds whether or not the prefix is stripped.
- Frontend: Pause/Resume → `pst_set_paused`, Unregister → `pst_unregister` with a confirm dialog, Delete → existing
  RemoveDialog confirm; Sync now disabled while paused; zh-TW/en `import.json` key sets identical.
- Security: new commands take only an id and a bool; canonicalize output only feeds the existing probe (absolute path, PST magic).

---

### [should-fix] Error rows duplicate inside one job: every Pause/Resume, restart or DB pause adds them again

Location: `src-tauri/crates/outlooker-core/src/importer/writer.rs:291-296` (`e.job_id IS DISTINCT FROM $2`), `src-tauri/crates/outlooker-core/src/importer/pipeline.rs:215` (scan rows written on every attempt)
Dimension: Correctness / data integrity
Rule: Correctness
Severity: Medium (partial impact: error rows and the listed error count grow and never shrink, no mail lost; likelihood likely — Outlook rewrites the PST mtime almost every session, so a resumed job usually resets its cursor)
Origin: introduced
Tags: none
Register: none, Ticket: TBD, Owner: TBD

Evidence:
The dedupe rule matches only rows of an *earlier* job ("rows of this job never match"). But one job now has many attempts:
Pause then Resume (new in round 2), app restart during a sync (Shutdown requeue), crash recovery, DB pause. Each attempt
(a) re-runs `record_scan`, so every degraded folder writes its `scan` row again under the same job id, and (b) after a
cursor reset (file changed while held) re-reads every unreadable item, whose rows already carry this job id, so they are
inserted again. Probe run: pause after batch 1, write the file, resume → `before=1 after=2 listed=2`. The next sync job then
moves both copies (UPDATE matches each), so the duplicates are permanent and grow by one copy per cycle.

Impact:
Contract R1 ("one error row per distinct error, the count does not grow") breaks for any path the user pauses and resumes,
or that is mid-sync at app quit; the error log and the `errorCount` badge fill with copies.

Recommended Fix:
Dedupe against this job's rows too, except where identical rows are legitimately distinct (item errors without an id):
in `same_error!` replace `e.job_id IS DISTINCT FROM $2` with
`(e.job_id IS DISTINCT FROM $2 OR n.pst_item_id > 0 OR n.stage = 'scan')`.
Extend `pause_resume_continues_from_checkpoint` (or `sync_repeated_errors_keep_one_row`) with one unreadable item and assert
one error row after the changed-file resume.

### [should-fix] A stalled probe still stops every later scheduled sync and app quit

Location: `src-tauri/crates/outlooker-core/src/sync/mod.rs:123` (scheduler awaits `sync(All)`), `src-tauri/crates/outlooker-core/src/sync/mod.rs:58` (`stop` awaits the task), `src-tauri/crates/outlooker-core/src/importer/queue.rs:552-572` (`sync` waits for every path)
Dimension: Concurrency
Rule: Correctness (contract override 9)
Severity: Medium (partial impact: no scheduled sync for any path and a hanging Quit while one share is dead; likelihood possible — network-share PSTs are the case override 9 names; a hard SMB/NFS mount can block `stat` for minutes)
Origin: introduced (the await is unchanged, but override 9 and R4 are this round's promise and only hold inside one run)
Tags: none
Register: none, Ticket: TBD, Owner: TBD

Evidence:
`sync` joins all per-path tasks before it returns, and the scheduler loop awaits it before it computes the next slot. Within
one run the fast path proceeds (R4 test), but while path A's `spawn_blocking` stat hangs, the scheduler task never reaches the
next slot, so paths B..N are not synced at their next interval either. `Scheduler::stop` (`h.await`) waits for the same
future, so app shutdown waits for the OS stat timeout.

Impact:
One dead share silently suspends the schedule for every PST and can hang Quit; the UI keeps showing the old next-sync time.

Recommended Fix:
Do not let the scheduler wait on a path's file I/O: in `step`, spawn `importer.sync(SyncTarget::All)` and keep only the handle
(skip a path whose previous probe is still in flight with a per-path in-flight `HashSet<i64>` in `queue::Inner`, so slots do not
pile up blocking threads), and make `stop` stop waiting for that handle (blocking threads are left to finish on their own).

---

### [nit] A Pause or Unregister that lands during the probe still queues a job

Location: `src-tauri/crates/outlooker-core/src/importer/queue.rs:492-495`
Evidence: the recheck compares only the newest job id; `paused`/`unregistered` were read before the probe (queue.rs:543-550 in `sync`). Pause does not add a job, so the stale sync inserts a queued job on a paused (or unregistered) path. The worker never runs it (runner.rs:88), so no mail is touched; the row shows "Queued" under "Paused" and the job runs at once on Resume or re-register.
Fix: in the transaction read `SELECT paused OR unregistered FROM pst_sources WHERE id = $1` (the row is already locked) and return `Skipped` when true.

### [nit] Delete is enabled while a sync runs, then fails with "import running"

Location: `src/screens/import/index.tsx:261`
Evidence: the button is disabled only for `removing`; `queue::remove` returns `Busy(ImportRunning)` for a running job (queue.rs:595), so the user gets an error toast and has to pause first.
Fix: `disabled={p.status === "removing" || p.status === "running"}`, or have `remove` hold the running job and queue the removal after it.

### [nit] "Retry" on a failed removal does nothing

Location: `src/screens/import/index.tsx` (Sync now button, `title` "row.retry" for status failed); `src-tauri/crates/outlooker-core/src/importer/queue.rs:477`
Evidence: a failed removal lists as `failed`, the row offers Retry (Sync now), but `sync_one` skips every path whose newest job is a removal, silently. Only Delete retries it.
Fix: expose the newest job kind (or a `removeFailed` flag) in `PstSource` and offer Delete instead of Retry there.

### [nit] The `\\?\` strip measures bytes, not UTF-16 units

Location: `src-tauri/crates/outlooker-core/src/importer/queue.rs:295`
Evidence: `plain.len()` is UTF-8 bytes; a CJK path of 100 characters is ~300 bytes and keeps the `\\?\` prefix in the stored and displayed path. Identity is unaffected (same canonical form every time).
Fix: `plain.encode_utf16().count() < 260`.

---

## Parking-lot suggestions (outside the contract acceptance; no severity)

1. A deterministically failing file (PST magic fine, libpff cannot open it) gets a new queued job every slot (`Failed` newest + `Probe::Ready`, queue.rs:499-501): error rows stay at one, but `import_jobs` grows by one row per slot and the file is opened each time.
2. `JobCancel::clear_pause` (queue.rs:78-84, pre-existing) resets the stop flag even when Hold arrived between the reason check and the clear (runner.rs:245-248); that attempt then runs to the end before it requeues. Clear the flag only when the reason became `None`.
3. The dedupe move rewrites `occurred_at` (writer.rs:283); a sync running while the user pages the error log (keyset on `occurred_at, id`) can skip or repeat rows across pages.
4. Re-registering an unregistered path that now holds a *different* PST reuses the source row and treats colliding PST item ids as known (not read). Same assumption as a scheduled sync of a replaced file (arch-2 item-id stability); worth a size/identity guard if file replacement is expected.
5. Blocking-pool probes cannot be cancelled; even with finding 2 fixed, the runtime may wait for them at process exit.
