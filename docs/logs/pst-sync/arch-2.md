# PST scheduled sync — architecture proposal 2

Author: architect-2. Input: `docs/superpowers/specs/2026-10-24-pst-scheduled-sync-design.md` (frozen requirements),
`reader-backend.md`, `reader-frontend.md`. Paths are relative to `src-tauri/crates/outlooker-core/src/` unless they
start with `src/` (frontend), `src-tauri/` or `docs/`. Every brief claim used here was re-read in code; no brief error
found. One addition the briefs missed: `index_policy` would drop all search indexes on every sync (item 2).

## Summary (one paragraph)
One `pst_sources` row per path (UNIQUE `path`). A sync is the existing import job on that same row: the checkpoint
already resets on a changed file. The reader skips items whose PST item id already has a row in `message_occurrences`
for this path, so a changed 20 GB file costs a folder scan plus one id lookup per item, not a full parse. One shared
probe function (stat, then open and read 4 bytes, then classify raw OS codes 32/33 as "locked") runs before a job is
queued and again at job start. A locked path gets a new terminal job state `locked`: no finalize, no VACUUM, no error
row, one job row for each locked streak. A small scheduler service (`core/sync/`) computes the next run with `jiff`
(local zone, DST), stores the schedule and last-run time in the DB `settings` row, and calls `ImportManager::sync`.

## 1. Identity model
- **Schema**: `pst_sources_same_file_once UNIQUE (path, file_size_bytes, file_mtime)` (`db/migrations/V1__schema.sql:14`)
  becomes `pst_sources_path_once UNIQUE (path)`. Drop the column `pst_sources.file_mtime`. The file identity of record is
  `import_checkpoints (pst_file_size, pst_file_mtime)` (`V1__schema.sql:136-151`). `Checkpoint::begin` already compares
  against it and resets the cursor and counters on change (`importer/writer.rs:131-140`). Keep `file_size_bytes` for
  display. Every attempt sets it to the current size (one `UPDATE` in `pipeline.rs:76-81`, before `Checkpoint::begin`).
- New columns on `pst_sources`: `sync_enabled boolean NOT NULL DEFAULT true`, `last_checked_at timestamptz` (the last
  time a sync step looked at the path, including "unchanged" and "locked").
- **No snapshot rows are ever created again**, so the remove hazard (`importer/remove.rs:15-29` deletes messages whose
  only occurrence is under the removed `pst_id`) cannot be triggered by sync. Sync never calls `remove`.
- Messages deleted in Outlook keep their occurrence under the same `pst_id` (rows are never deleted by a job), so the
  append-only rule holds by construction. `messages.first_pst_id` (`V1__schema.sql:61`) stays correct.
- **Register** (replaces enqueue, `importer/queue.rs:317-341`): probe the path (item 3). `Ready` → insert row + queued
  job (today's `insert_source`, `queue.rs:292-315`, with `ON CONFLICT ON CONSTRAINT pst_sources_path_once`). `Locked` →
  insert row + one job in state `locked` (registering while Outlook is open is the common case; the magic check runs
  at the first unlocked probe). The path already exists → `Rejected{AlreadyImported{pst_id}}` (kind kept, copy becomes
  "already registered"). Other probe errors → `Rejected` as today.
- **Remove a registered path** = the existing `pst_remove` (`queue.rs:397-417`): it deletes the row and every message
  that only this path holds, including mail that was deleted in Outlook and kept by append-only. The confirm dialog
  must say so. **Disable** (`sync_enabled = false`) is the non-destructive choice: data stays, scheduler and "Sync all"
  skip the path. No third "unregister but keep data" action (disable covers it).
- **Edit V1, no V3** (and update spec Part 04 tables `docs/superpowers/specs/2026-10-21-outlooker-design.md:1355,1519,
  1550,1574` in the same change, because V1 says "generated from spec, do not hand-edit", `V1__schema.sql:1`). Why: a V3
  that adds UNIQUE(path) must first merge the duplicate-path rows that existing dev DBs may hold. That is compat code,
  and the policy forbids it. New columns and the changed CHECKs are covered by V1's `GRANT ... ON ALL TABLES`
  (`V1__schema.sql:205`). Cost: refinery aborts on an edited, already-applied migration (`abort_divergent: true`,
  refinery-core 0.10.0 `runner.rs:240`; runs every boot, `lifecycle/start.rs:47`), so existing dev data dirs must be
  deleted. See item 10, question 1.

## 2. Re-parse cost
- **Unchanged** (stat size+mtime == checkpoint identity AND newest job is `import/completed`): no job, no open. Only
  `last_checked_at` is updated. This uses only existing data (`LIST_SQL` lateral newest job, `queue.rs:185-193`).
- **Changed** (or last run not completed): normal job. `Checkpoint::begin` resets cursor and counters
  (`writer.rs:131-140`), so `stored` = new messages of this sync and `newMessageCount` keeps its meaning
  (`queue.rs:238-239`).
- **Per-item skip (decided: yes)**. Outlook rewrites the active PST's mtime almost every session, so "unchanged" rarely
  holds for the main PST. Without a skip, every sync re-parses 20 GB (bodies, RTF, attachments ≤25 MB, WebP;
  `pst_reader.rs:168-234`, `normalize/mod.rs:28-60`) only to throw it away in SQL dedupe.
  - `attempt` loads `SELECT pst_item_id FROM message_occurrences WHERE pst_id=$1` into `Arc<HashSet<i64>>` (about
    16 MB for 500k items). It passes the set in a new `ReaderParams.known` field (`pst_reader.rs:42`).
  - Reader: after `sub_message(index)` and `identifier()` (`pst_reader.rs:300`), an id in `known` yields the new
    `RawContent::Known` (`types.rs:27-31`) without `read_mail`. Normalize maps it to the new `Outcome::Known`
    (`normalize/mod.rs:49`, `types.rs:198`). The writer writes no rows for it and counts it as a duplicate (`&(mails -
    stored + known)` in `writer.rs:419-434`). The item still has a seq, so the gap-free cursor commit is unchanged.
    `spawn_synthetic` honours `known` the same way.
  - Non-mail items are re-read, but only their class property (`pst_reader.rs:169-171`), which is cheap. Items that
    errored last time have no occurrence, so they are retried, which is wanted.
  - **index_policy fix**: `remaining = items_total - cursor_seq - known.len()` (clamped at 0) in `pipeline.rs:98`.
    Today `2*remaining >= messages` (`pipeline.rs:211`) would drop every search index on each sync of a big PST and
    rebuild them in finalize.
  - **finalize**: skip `AFTER_JOB_VACUUM` when the run's `stored == 0` (`runner.rs:295`). Keep `ensure_search_indexes`
    (idempotent).
- **Stability assumption and how to verify** (libpff identifier = PST node id; the code does not rely on it today):
  over SSH on Windows with real Outlook, import `test.pst` into DB A with the skip on. Then do in Outlook: add 20 mails,
  delete 5, move 5 to another folder, run File > Compact Now, add 5 more. Sync DB A. Import the final file into a fresh
  DB B with `known` forced empty. Pass = `SELECT dedupe_key FROM messages` of B is a subset of A, and A ⊇ B holds again
  after a second round. Fail → set `known` to empty (one line in `attempt`). Correctness then holds at the full-parse cost.
- Known limitation (append-only): a moved message whose id is unchanged keeps its old `folder_id` occurrence. If the id
  did change, it appears in both folders.

## 3. Lock detection
- New `importer/probe.rs`, which replaces `check_file` (`queue.rs:265-282`) everywhere:
  - `pub enum Probe { Ready { size: i64, mtime: SystemTime }, Locked, Unavailable(AppError) }`
  - `pub fn probe(path: &str) -> Probe`: not absolute → `fileAccess notAbsolute`. `std::fs::metadata` error → lock
    check, else `notFound`. Not a file → `notAFile`. Then `std::fs::File::open` + `read_exact(4)` error → lock check,
    else `unreadable`. Magic mismatch → `NotAPst`. Otherwise `Ready` (mtime via `truncate_to_micros`, `writer.rs:89`).
  - `fn is_lock(e: &io::Error) -> bool { matches!(e.raw_os_error(), Some(32 | 33)) }` (ERROR_SHARING_VIOLATION /
    ERROR_LOCK_VIOLATION). **No cfg.** Proof that it is inert on macOS: errno 32 = EPIPE and 33 = EDOM are not in the
    error lists of open(2), read(2) or stat(2) for a regular file, so the same code is correct on both platforms.
- **Where it runs**: (a) in each sync step before any job is created (item 4). (b) in `attempt` before
  `Checkpoint::begin` (`pipeline.rs:76-81`), so a lock that appeared while the job waited in the queue does not reset
  the checkpoint. (c) after a libpff open failure (`pipeline.rs:88-93`, `ReaderMsg::Failed{stage: Open}`) the probe is
  re-run. If it now says `Locked`, the attempt ends `Locked`. This classifies libpff's string-only error
  (`crates/pff/src/lib.rs:19-21,43-51`) without parsing localized text.
- **New state**: `import_jobs.state` gets `'locked'` (terminal). V1 CHECKs (`V1__schema.sql:127-133`) become: state
  list + `locked`; `(state IN ('cancelled','failed','completed','locked')) = (finished_at IS NOT NULL)`;
  `kind = 'import' OR state NOT IN ('cancelling','cancelled','locked')`. `JobState::Locked` (`queue.rs:158-169`),
  `ImportStatus::Locked` (`dto/import.rs:47-55`), `status_of(Import, Locked) => Locked` (`queue.rs:172-183`).
- **Runner**: `AttemptEnd::Locked` → `EndAction::Locked` (`runner.rs:203-222`). It calls `set_final(Locked, None)`:
  no `finalize`, no `job_failed` row (`runner.rs:266-281` is skipped), then `emit_status`.
- **Streaks without noise**: a sync step that probes `Locked` when the newest job is already `locked` only runs
  `UPDATE import_jobs SET finished_at = now()` on that row. Otherwise it inserts one `locked` job. The same rule
  applies to `Unavailable` (missing file and similar): one `failed` job + one `job_failed` row (stage `open`) at
  streak start only.
- **Windows verification (later, over SSH; use powershell, semicolons)**. P = the PST path.
  1. Outlook open: `powershell -Command "$p='P'; try { [IO.File]::Open($p,'Open','Read','ReadWrite,Delete').Close(); 'OPEN' } catch { '{0:X}' -f $_.Exception.HResult }; try { [IO.File]::Open($p,'Open','Read','ReadWrite').Close(); 'OPEN2' } catch { '{0:X}' -f $_.Exception.HResult }; (Get-Item $p).Length"`.
     The first open uses Rust std's share mode (read, write, delete), the second libcfile's (read, write;
     `native/libpff/libcfile/libcfile_file.c:336-352`). Expect `80070020` (= 32) twice and a size (stat works).
  2. Outlook closed: same command. Expect `OPEN`, `OPEN2`.
  3. Real app (the user runs it, not an agent): register P with Outlook open → status `locked`, no error-log row. Close
     Outlook, click Sync now → `completed`. Reopen Outlook, wait for one scheduled run → still one `locked` job row
     (`SELECT count(*) FROM import_jobs WHERE state='locked'` in the SQL console, row count does not grow).
  If step 1 prints `OPEN` while Outlook runs, Outlook allows readers. The design still works (no locked state; libpff
  reads a live file; append-only makes the next sync fix it). Record this result in `docs/logs/pst-sync/`.

## 4. Scheduler
- **Module**: `core/sync/mod.rs` (`Scheduler` service) + `core/sync/schedule.rs` (pure next-run logic). Built in
  `app::build` (`app/mod.rs:47-65`) with `db`, `importer`, `sink`, `supervisor.phase_watch()`. Started in
  `start_services` after `start_worker` (`app/mod.rs:101`), with new `StartStep::SchedulerStarted` (`app/mod.rs:67-72`,
  test `tests/it/app_compose.rs:22`). Stopped first in `app::shutdown`, before `importer.shutdown`
  (`app/mod.rs:126`), so nothing is enqueued during the drain.
- **Task** (one `tokio::spawn`, guarded like `start_worker`, `queue.rs:509-514`):
  ```
  loop {
    wait phase == Ready (select shutdown)                         // same gating as runner.rs:41
    (schedule, last) = SELECT sync_* FROM settings
    next = match last { None => now, Some(t) => schedule::next_run(&schedule, t, &TimeZone::system()) }
    emit sync:status-changed { schedule, last, next }
    select {
      sleep(min(next - now, 60 s)) => if now >= next { importer.sync(All); UPDATE settings SET sync_last_run_at = now() },
      wake.notified()  => {}   // schedule changed
      phase.changed()  => {}
      shutdown         => return
    }
  }
  ```
  Wall clock (`jiff::Timestamp::now()`) is re-read at least every 60 s. This covers laptop sleep and clock or zone
  changes, which a monotonic `tokio` sleep alone does not.
- **Catch-up**: `next_run` returns the first slot strictly after `last`. If the app was closed across one or more slots,
  `next <= now` at start → exactly one run, then `last = now`. `last = NULL` (never ran) counts as overdue → one run.
- **next_run** (`fn next_run(s: &SyncSchedule, last: Timestamp, tz: &TimeZone) -> Timestamp`):
  interval → `last + minutes`. Daily → for local dates d = date(last in tz) … +7: candidate =
  `d.at(hh, mm, 0, 0).to_zoned(tz.clone())` (jiff 0.2.38 `civil/datetime.rs:1514`, compatible rule: a time inside a DST
  gap moves forward, an ambiguous time takes the earlier instant); first candidate with weekday in set and instant >
  `last`. `TimeZone::system()` (`tz/timezone.rs:325`): Unix `/etc/localtime`, Windows `GetDynamicTimeZoneInformation` →
  CLDR mapping, all inside the crate. One code path for both platforms.
- **ImportManager::sync(target: SyncTarget)**, new in `importer/queue.rs`, public via `importer/mod.rs`.
  `SyncTarget::All` = enabled paths. `SyncTarget::One(pst_id)` = that path, even if disabled (explicit user action).
  It is serialized by a `tokio::sync::Mutex<()>` in `Inner` (`queue.rs:93-107`) so the scheduler and a manual click
  never interleave. Per path: (1) skip if the newest job is active (queued, running or cancelling); this is the "no
  overlapping runs" rule, also enforced by `import_jobs_one_active_per_pst`, `V1__schema.sql:184`, via
  `INSERT … ON CONFLICT (pst_id) WHERE state IN ('queued','running','cancelling') DO NOTHING`. Skip also when the
  newest job is `removing`. (2) unchanged → touch `last_checked_at`. (3) `probe` → `Ready` inserts a queued job;
  `Locked`/`Unavailable` → streak rule (item 3). (4) `emit_status` for changed rows. (5) one `kick` at the end
  (`queue.rs:339`).
- **Work Gate**: neither the scheduler nor `sync` touches the gate. Jobs wait for it in the worker as today
  (`runner.rs:55-63`): maintenance and data-dir moves delay a sync, they never overlap it. During a data-dir move the
  phase is not Ready, so the scheduler waits. "Busy" means the job stays queued; it is not an error.
- **Manual trigger**: `sync_now(pst_id?)`: `None` → `sync(All)`, `Some(id)` → `sync(One(id))`. It does not change
  `sync_last_run_at` (the schedule is unaffected). Cancel stays `import_cancel` (`queue.rs:356-377`).
- **Disabled paths**: excluded from `All`. Disabling does not cancel a queued or running job (the user has Cancel).

## 5. Settings storage
DB `settings` singleton row (`V1__schema.sql:176-181`, edited in V1), with new columns:
`sync_mode text NOT NULL DEFAULT 'interval' CHECK (sync_mode IN ('interval','daily'))`,
`sync_interval_minutes integer NOT NULL DEFAULT 60 CHECK (sync_interval_minutes BETWEEN 5 AND 10080)`,
`sync_daily_minute smallint NOT NULL DEFAULT 540 CHECK (sync_daily_minute BETWEEN 0 AND 1439)`,
`sync_weekdays smallint NOT NULL DEFAULT 127 CHECK (sync_weekdays BETWEEN 1 AND 127)` (bit 0 = Monday),
`sync_last_run_at timestamptz`. Registered paths live in `pst_sources` (item 1).
Why not `settings.json` (`settings/mod.rs:11-16`): that file exists only for values needed before the DB starts. The
scheduler runs only in `Ready`, and last-run must move with the data dir and agree with the job rows. Why not a new
table: one row, already exists, already granted. Both modes' fields are kept, so switching modes keeps the other mode's
values.

## 6. API surface
**DTOs** (ts-rs, `#[ts(export)]`):
- `PstSource` (`dto/import.rs:30-42`) adds `syncEnabled: boolean`, `lastCheckedAt: string | null`,
  `lastSyncedAt: string | null` (finished_at of the newest `import/completed` job, extra lateral in `LIST_SQL`),
  `failureReason: string | null` (message of the newest `job_failed` row of the newest job when status is failed).
  Existing fields keep their meaning: `newMessageCount` = new in the last real sync, `messageCount` = mail items seen in
  the PST at the last sync, `queuedAt`/`finishedAt` = newest job (for `locked`: locked since / last seen locked).
- `ImportStatus` adds `locked`.
- New `dto/sync.rs`: `SyncSchedule` = `{ mode: "interval", minutes: number } | { mode: "daily", time: "HH:MM",
  weekdays: number[] }` (serde `tag = "mode"`, ISO weekdays 1 = Monday … 7, non-empty, unique; invalid →
  `AppError::invalid("schedule", "outOfRange")`). `SyncStatus { schedule: SyncSchedule, lastRunAt: string | null,
  nextRunAt: string | null }` (RFC 3339 UTC like other timestamps, `db/mod.rs:14`; the UI formats them in local time).
**Commands** (`dto/commands.rs:2` array becomes `[&str; 27]`, length 27 also at `:19`; `src-tauri/src/lib.rs:70-95`;
`src/api/commands.ts`; the tests `lib.rs:160-167` and `api.test.ts:15` must stay green):
- removed: `import_enqueue`, `import_resume`.
- added: `pst_register(paths: string[]) -> EnqueueResult[]`, `pst_set_enabled(pstId, enabled) -> PstSource`,
  `sync_now(pstId: number | null) -> void`, `sync_status() -> SyncStatus`, `sync_schedule_set(schedule) -> SyncStatus`.
- unchanged: `pst_list`, `import_cancel`, `pst_remove`, `import_errors`.
**Events** (`dto/events.rs:16` `ALL` becomes `[&str; 7]`; `sink.rs:18-60` variant + name + payload; `src/api/events.ts`
`EVENTS`/`EventPayloads`; `src/app/boot.ts:23-30` subscription; not throttled, `sink/throttle.rs:17-27` untouched):
- added: `sync:status-changed` → `SyncStatus`, emitted on schedule change, after each run, and on every next-run
  recompute.
- `import:status-changed` keeps its shape; the extended `PstSource` carries all per-path state through the existing
  cache patcher (`src/app/importCache.ts`).
**UI state table** (what the Import screen derives):
| UI state | Source |
|---|---|
| idle / up to date | `status = completed` |
| queued | `status = queued` |
| syncing + progress | `status = running`, `progress` (phase, itemsDone/itemsTotal, ETA) |
| locked, waiting for Outlook | `status = locked` (+ `finishedAt` = last seen locked) |
| failed + reason | `status = failed`, `failureReason`, error log via `import_errors` |
| cancelled by user | `status = cancelled` |
| disabled | `syncEnabled = false` (shown on top of the last status) |
| last sync / checked | `lastSyncedAt`, `lastCheckedAt` |
| next sync | `SyncStatus.nextRunAt` when `syncEnabled` |
| new in last sync | `newMessageCount` |

## 7. Deleted / kept
Deleted: `check_file` (→ `probe`, `queue.rs:265-282`); `queue::resume` + `ImportManager::resume` + `import_resume`
command (`queue.rs:379-395`, `importer/mod.rs:68-70`, `src-tauri/src/commands/import.rs`); `import_enqueue` command
(renamed to `pst_register`; `queue::enqueue` → `queue::register`); `pst_sources.file_mtime` and
`pst_sources_same_file_once`, plus the identity lookup in `insert_source` (`queue.rs:304-309`); the Resume / "Retry from
checkpoint" buttons and the separate queue concept in the Import screen (`src/screens/import/index.tsx:24,332-415`;
the mockup decides the replacement); `useEnqueuePst` → `useRegisterPst` (`src/components/common/hooks.tsx:42-58`).
Callers to update: `test_support/import_fixtures.rs:18`, `examples/import_bench.rs`, `tests/it/import_pipeline.rs`
(enqueue/resume at :226,257,302,378,451,499,539), `tests/it/app_compose.rs:117`.
Kept: the onboarding PST step (`src/screens/onboarding/index.tsx:107-109,179`); the first run needs a first registered
path, and it uses the same hook. `pst_remove`, `import_cancel`, `import_errors`, the queue/worker/gate/resume machinery
(a sync is a job).

## 8. Tests
Committed (each ≤ 1 s; all are core logic, data integrity or lock classification per `CLAUDE.md`):
1. `sync/schedule.rs` unit: interval; daily later today; weekday roll-over across a week; DST gap (America/New_York
   2026-03-08 02:30 → 03:30 −04:00); DST fold (2026-11-01 01:30 → −04:00 instant); last run long ago → first slot after
   it (catch-up returns a past instant, exactly one).
2. `importer/probe.rs` unit: `is_lock` for `io::Error::from_raw_os_error(32|33)` true, 2/5/13 false; directory →
   `notAFile`; wrong magic → `NotAPst`.
3. `tests/it/import_pipeline.rs` (shared cluster, synthetic sources):
   a. `sync_changed_file_keeps_one_row_and_skips_known`: import 20, change file + add 5 items, `sync(One)` → one
      `pst_sources` row, `newMessageCount == 5`, the 20 known items counted as duplicates (reader seam counts
      `read_mail` calls == 5).
   b. `sync_unchanged_creates_no_job`: completed + unchanged → job count unchanged, `last_checked_at` set.
   c. `sync_locked_no_error_row_no_finalize`: test-support lock seam (synthetic registry flag read by `probe` under
      `cfg(feature = "test-support")`, same pattern as `queue.rs:529-552`) → status `locked`; two runs → one job row,
      zero `import_errors`. Unlock → `completed`.
   d. `register_same_path_rejected`: second register → `AlreadyImported`.
   Existing resume tests (:371-378, :437-451, :488-508) switch to `sync(One)` and keep their oracles.
One-off (scripts/tmp, not committed): Windows lock procedure (item 3); identifier-stability procedure (item 2); real-app
schedule check by the user (daily time = now + 2 min fires once; set `sync_last_run_at` to yesterday and restart → one
catch-up run); 20 GB timing of a changed-file sync with skip (record in `docs/logs/pst-sync/`).

## 9. Implementation split
| | Deliverable | Owns (exclusive) | Depends on |
|---|---|---|---|
| A | Sync core: identity, probe, skip, locked state | `db/migrations/V1__schema.sql`, design-spec Part 04 rows, `importer/{probe.rs (new), queue.rs, pipeline.rs, runner.rs, writer.rs, pst_reader.rs, types.rs, normalize/mod.rs, mod.rs}`, `dto/import.rs`, `test_support/import_fixtures.rs`, `examples/import_bench.rs`, `tests/it/import_pipeline.rs` | none |
| B | Scheduler | `sync/{mod.rs, schedule.rs}` (new), `dto/sync.rs` (new), `dto/mod.rs`, `lib.rs` (core), `app/mod.rs`, `dto/events.rs`, `sink.rs`, core `Cargo.toml`, `tests/it/app_compose.rs` | A's `ImportManager::sync` + settings columns (signatures frozen above); `schedule.rs` can start at once |
| C | Commands + frontend plumbing | `src-tauri/src/commands/{import.rs, sync.rs (new), mod.rs}`, `src-tauri/src/lib.rs`, `dto/commands.rs`, `src/api/{commands.ts, events.ts, bindings/*}`, `src/app/{boot.ts, testing.tsx}`, `src/components/common/hooks.tsx`, `src/screens/onboarding/*`, `src/locales/*/{import,common}.json` | TS side at once against item 6; Rust side after A and B DTOs land |
| D | Import screen (Fluent 2 mockup) | `src/screens/import/*` | item 6 contract only (mocks); final run after C |
Acceptance (run from `src-tauri/` with the canonical commands; frontend from the repo root):
- A: `cargo test --workspace --test it import_pipeline` green including 3a–3d; `cargo test --workspace --lib probe`
  green; `cargo clippy --workspace --all-targets -- -D warnings` clean; `grep -rnwE "check_file|import_resume|pst_sources_same_file_once" crates src` → 0 hits.
- B: `cargo test --workspace --lib schedule` green (item 8.1); `cargo test --workspace --test it app_compose` green with
  4 steps; `grep -n '^jiff = "=0.2.38"' crates/outlooker-core/Cargo.toml` → 1 hit, and `Cargo.lock` still lists only
  jiff 0.2.38; clippy clean.
- C: `cargo test --workspace` regenerates `commands.json`/`events.json` with 27 / 7 names; `npm test` green (includes
  `api.test.ts`, the locale parity test `global.test.tsx`, and the onboarding test); `grep -rnE "importEnqueue|importResume|useEnqueuePst" src` → 0.
- D: `npm test` and `npm run build` green; the screen uses only `pstList`, `pstRegister`, `pstSetEnabled`, `syncNow`,
  `syncStatus`, `syncScheduleSet`, `importCancel`, `pstRemove`, `importErrors` and the event `sync:status-changed`.

## 10. Risks and open questions for the user
1. **Data dir reset (needs your decision).** Editing V1 makes any existing dev database fail to start (refinery
   rejects a changed applied migration). You would delete the data folder and re-import. The alternative is a V3
   migration that merges duplicate-path rows, which is compat code and more work. My recommendation: edit V1 and reset.
2. **Outlook start during a sync.** While a sync reads the PST, the app holds the file open (read, sharing
   read+write). If you start Outlook at that moment, Outlook can report the PST as "in use". The per-item skip keeps
   syncs short (scan + id checks); the 20 GB timing run will show how short. Accept, or ask for a different rule
   (for example, no automatic runs, Sync now only).
3. **Item-id stability** (item 2) is unverified until the Windows procedure runs. If it fails, the fallback is a full
   re-parse of a changed PST on every sync (correct but slow, possibly an hour for 20 GB).
4. **New dependency `jiff` 0.2.38** in `outlooker-core` (already in `Cargo.lock` through an optional `serde_with` edge,
   not compiled today; `chrono` 0.4.45 is in the same situation). Needed because "daily at HH:MM" needs the system zone
   and DST rules; std has neither, and reading the zone by hand is per-platform code (registry vs `/etc/localtime`).
