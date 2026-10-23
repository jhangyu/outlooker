# PST scheduled sync: architecture proposal 1 (architect-1)

Input: `docs/superpowers/specs/2026-10-24-pst-scheduled-sync-design.md` (frozen). Paths are relative to
`src-tauri/crates/outlooker-core/src/` ("core/") unless they start with `src/` (frontend) or `src-tauri/`.
I checked every cited line in the code on 2026-10-24. Briefs `reader-backend.md` and `reader-frontend.md` matched the code.
I found one gap in them: the "time zone crate" question in reader-backend §6. Section 4 answers it.

## Summary
A registered path is one `pst_sources` row for its whole life. A sync of a changed file reuses that row. The
existing checkpoint reset (`importer/writer.rs:131-140`) restarts the walk, and a per-path "known item" set skips
the expensive read of every item that the path already stored. A pre-job probe with `std::fs::File::open` sorts
open failures into locked / not found / unreadable / not a PST. A locked path never creates a job, so it never
runs finalize or VACUUM and never writes a `job_failed` row. One tokio scheduler task enqueues ordinary import jobs
into the existing queue. The worker and the Work Gate stay as they are. The schedule lives in the DB `settings` singleton.
Local-time math uses the `time` crate. It is already compiled in this workspace with `local-offset`, so it adds no new crate.

## 1. Identity model

Decision: **one `pst_sources` row per registered path.** Add migration `V3__pst_sync.sql`. Do not edit V1.
- Why V3: refinery 0.10.0 aborts on a changed applied migration (`abort_divergent: true` by default,
  `~/.cargo/registry/.../refinery-core-0.10.0/src/runner.rs:240`; runner call `db/mod.rs:91-92`). An edited V1 would
  make every existing data dir fail to boot, and the user's library would need a full re-import. V3 is plain ALTERs.
  It is not compatibility code.
- V3 contents:
  - `pst_sources`: drop `pst_sources_same_file_once` (V1:14). Add `CONSTRAINT pst_sources_path_once UNIQUE (path)`.
    Add `sync_enabled boolean NOT NULL DEFAULT true`, `check_problem text NULL CHECK (check_problem IN ('locked',
    'not_found','unreadable','not_a_pst'))` and `checked_at timestamptz NULL`. Keep `file_size_bytes` and `file_mtime`.
    They now mean "last seen", for display and for the synthetic-source fallback at `importer/pipeline.rs:140`.
  - `settings` (V1:176-181): add `sync_mode text NOT NULL DEFAULT 'interval' CHECK (sync_mode IN ('interval','daily'))`,
    `sync_interval_minutes integer NOT NULL DEFAULT 60 CHECK (BETWEEN 5 AND 10080)`,
    `sync_daily_minute smallint NOT NULL DEFAULT 540 CHECK (BETWEEN 0 AND 1439)`,
    `sync_weekdays smallint NOT NULL DEFAULT 127 CHECK (BETWEEN 1 AND 127)` (bit 0 = Monday) and
    `sync_last_run_at timestamptz` (V3 sets it to `now()`, so the first start does not run a catch-up).
  - V3 adds no table, so it needs no new GRANT. The V1:205 and V2:49 grants cover all columns.
  - The UNIQUE(path) add fails if the DB already holds two snapshot rows of the same path. V3 starts with a `DO` block
    that raises "duplicate PST path rows: remove the older entry in Import first". See §10 Q1.
- **Identity of the snapshot** = the checkpoint's `(pst_file_size, pst_file_mtime)` (V1:142-143), which
  `Checkpoint::begin` already writes at the start of every attempt (`importer/writer.rs:131-140`). "Unchanged" means
  that the current stat equals the checkpoint identity. A file that changes during a run is seen as changed on the next run, so the design corrects itself.
- **Changed file**: the same `pst_id`, the same Import-list entry. `CP_RESET` (`writer.rs:68-74`) zeroes the cursor and
  counters. `folders` upsert in place (`writer.rs:151-165`). Occurrences insert with `ON CONFLICT DO NOTHING`
  (`writer.rs:266-271`). Occurrences of mail that was deleted in Outlook stay. This is the append-only rule (spec decision 1).
- The snapshot-row hazard (reader-backend §4) goes away. Only one row per path exists, so no old row can be cleaned up and nothing deletes mail by accident.
- **Remove a registered path** = the existing remove job, unchanged (`importer/queue.rs:397-417`, `importer/remove.rs:38-63`).
  It deletes the path's row and every message that only this path holds, including mail that was deleted in Outlook and kept by the
  append-only rule. The remove dialog must say this. **Disable** (`sync_enabled=false`) is the action that "stops syncing
  and keeps the mail".
- Registration: `queue::enqueue` (`queue.rs:317-341`) becomes `register`. `insert_source` (`queue.rs:292-315`) inserts
  with `ON CONFLICT ON CONSTRAINT pst_sources_path_once DO NOTHING`. A conflict gives `AlreadyImported{pst_id}` (same
  AppError kind and the same locale key; the meaning is now "path already registered"). If the probe returns Locked at registration,
  the row is still inserted, with `check_problem='locked'` and no job. Other probe errors reject the path as they do today.

## 2. Re-parse cost

- Unchanged file: no job. The probe reads only metadata and the 4 magic bytes (`queue.rs:265-282`). libpff never opens the file.
- Changed file: **per-item skip by `pst_item_id` within the same path**. No full re-parse.
  - `pipeline::attempt` loads `SELECT pst_item_id FROM message_occurrences WHERE pst_id=$1` into a `HashSet<i64>`
    (about 8 bytes per mail) and passes it in `ReaderParams` (`importer/pst_reader.rs:42-53`).
  - Reader (`pst_reader.rs:300`): after `m.identifier()` and before `read_mail`, a known id sends
    `RawContent::Known` (new variant, `importer/types.rs:27-31`). The synthetic reader (`pst_reader.rs:336`) applies the
    same check, so the tests can see it.
  - Normalize (`importer/normalize/mod.rs:49`) maps `Known` to `Outcome::Known` with no work. The writer
    (`writer.rs:419-433`) counts `Known` items into `duplicates` and stages no occurrence, because the occurrence
    already exists. The cursor still moves forward, so resume works unchanged.
  - Result: a changed 20 GB PST costs one folder scan plus one identifier read per item. Only new items pay for
    the property, body and attachment read and the normalize step.
  - Known limits (accepted, append-only): an item that Outlook edits in place (same id, for example a draft) is not
    re-read. Non-mail items have no occurrence (`writer.rs:419-421`), so they are read again on each changed run. That cost is small.
- **Precondition: libpff identifiers must be stable and must not be reused.** This is unverified. The design depends on it.
  Verify it before D2 merges, on Windows with real Outlook, using existing code only:
  1. Copy `x.pst` to `before.pst`. In Outlook, receive or create 5 mails, delete 3 (and empty Deleted Items), move 2
     between folders, then run File > Account Settings > Data Files > Settings > Compact Now. Close Outlook. Copy to `after.pst`.
  2. Import both copies (current build, two sources). Run in SQL console:
     `SELECT count(*) FROM message_occurrences a JOIN message_occurrences b USING (pst_item_id) WHERE a.pst_id=<before>
     AND b.pst_id=<after> AND a.message_id<>b.message_id;` Expected: `0`. Also expected: `after` rows that are missing in
     `before` equal the new plus moved mails.
  3. Any count other than 0 means that ids are reused. The fallback is to remove the known-set lookup (one `if` in the reader).
     Dedupe keeps the data correct, and each changed run becomes a full re-parse. Report this to the user as a performance limit.

## 3. Lock detection

- One probe function serves registration, sync and the start of each job: `check_file` (`queue.rs:265-282`) returns
  `Result<(i64, SystemTime), PathCheck>` with `PathCheck = Locked | NotFound | Unreadable | NotAPst`.
  - `metadata` error: `ErrorKind::NotFound` gives NotFound, any other error gives Unreadable.
  - `File::open` or `read_exact` error goes to `fn classify_open(e: &io::Error) -> PathCheck`:
    `raw_os_error()` 32 (`ERROR_SHARING_VIOLATION`) or 33 (`ERROR_LOCK_VIOLATION`) gives Locked; anything else gives Unreadable.
  - The same code runs on every platform. On macOS and Linux the Locked branch cannot be reached: errno 32 is EPIPE and
    33 is EDOM, and POSIX `open(2)` and `read(2)` on a regular file never return either. libpff on Unix has no locking
    either (reader-backend §2). No `cfg` is needed.
  - Mapping to AppError (callers that still return errors): `FileAccess{reason}` with reason `locked`, `notFound` or
    `unreadable`; `NotAPst`. The existing reasons stay; `locked` is new.
- **Pre-job probe (normal case)**: the sync decision (§4) runs the probe before it inserts a job. Locked means
  `check_problem='locked'`, `checked_at=now()`, one `import:status-changed` event, and no job row. So there is no
  finalize, no VACUUM, no `job_failed` row and no Work Gate use.
- **In-job probe (race: queued, then Outlook opened the file)**: `file_identity` (`pipeline.rs:137-143`) runs
  `check_file` before `Checkpoint::begin` (`pipeline.rs:81`). Locked returns new `AttemptEnd::Locked`, and
  `end_of_attempt` (`importer/runner.rs:214-222`) maps it to new `EndAction::Deferred`:
  `set_final(Cancelled, None)` (no error row), `check_problem='locked'`, then emit. Finalize is skipped. This is safe because
  the checkpoint and the indexes are untouched before step 2 and step 5 (`pipeline.rs:81`, `:98`).
- Remaining window: the probe passes, then Outlook opens the file before `pff::File::open` (`pst_reader.rs:251`), a few ms later.
  The job then fails as today (one `job_failed` row). This is accepted; it cannot repeat on every run, because the next run is
  probed again first.
- A successful probe sets `check_problem=NULL`.
- **Windows verification over SSH (do it later, in the Windows phase)**. Use `powershell -Command` with `;` separators:
  1. The user opens Outlook with the PST attached.
  2. `powershell -Command "try { $f=[IO.File]::Open('C:\path\x.pst','Open','Read','ReadWrite,Delete'); 'OPEN_OK'; $f.Close() } catch { $_.Exception.HResult }"`.
     This share mode equals Rust std's default (read+write+delete). Expected with Outlook open: `-2147024864`
     (0x80070020, which is code 32). Close Outlook and run it again: expected `OPEN_OK`.
  3. Run `cargo test --workspace --lib classify_open` on Windows (the committed unit test, §8).
  4. The user runs the real app: register the PST with Outlook open. Expected: status "Locked", no new rows in `import_jobs` or
     `import_errors`. Close Outlook and click Sync now. Expected: status syncing, then completed.
  5. If step 2 prints `OPEN_OK` while Outlook is open, Outlook shares the file for reading. Stop and report: lock
     detection cannot work, and libpff would read a file that is being written (§10 R2).

## 4. Scheduler

- **Placement**: new module `core/sync/` contains `schedule.rs` (pure next-run math) and `service.rs` (`SyncService`, the task).
  It is built in `app::build` (`app/mod.rs:47-65`) next to `importer`. It starts in `start_services` after `start_worker`
  (`app/mod.rs:101`) as new `StartStep::SchedulerStarted` (`app/mod.rs:68-72`). It stops first in `app::shutdown`, before
  `importer.shutdown` (`app/mod.rs:126`). It uses its own shutdown `watch<bool>` and the task `JoinHandle`, the same
  pattern as `queue::shutdown` (`queue.rs:516-527`). `register` and `sync` also return early when the importer is shutting down (`queue.rs:133`).
- **Task loop** (one `tokio::spawn`, started once with the `start_worker` slot pattern, `queue.rs:509-514`):
  1. Wait for `AppPhase::Ready` through `supervisor.phase_watch()` (same as `runner.rs:41-47`).
  2. Read the schedule and `sync_last_run_at` from `settings`. `next = schedule::next_run(&sched, last_run)`.
  3. If `now >= next`: write `sync_last_run_at = now()` first, emit `sync:state-changed`, then run
     `importer.sync(None, Trigger::Scheduled)`.
  4. Wait in `select!` on: `sleep(min(next - now, 60 s))`, the schedule-changed `Notify`, a phase change, and shutdown. Then loop.
  - The 60 s cap is there because tokio sleeps on a monotonic clock that can stop while the laptop sleeps. Each wake reads the
    wall clock again, so a time-zone or clock change and a DST change are handled with no special code.
  - **Catch-up**: `next_run` is computed from the persisted last run. After the app was closed past one or more slots,
    the first loop iteration finds `now >= next` and runs **once**. Step 3 then moves `last_run` to now, so missed slots
    do not pile up. If the clock went backwards (`last_run > now`), `last_run` is clamped to `now`.
- **next_run** (`sync/schedule.rs`, pure):
  `fn next_run(s: &SyncSchedule, last: OffsetDateTime, offset_at: impl Fn(OffsetDateTime) -> UtcOffset) -> OffsetDateTime`.
  - Interval: `last + minutes`. This is absolute time, so DST does not change it.
  - Daily: for the local dates of `last` up to 8 days ahead, take wall time `HH:MM` on a selected weekday, resolve it to an
    instant, and return the first instant greater than `last`.
  - Resolving a wall time W: `o1 = offset_at(W−1 day)`, `o2 = offset_at(W+1 day)`, `t1 = W−o1`, `t2 = W−o2`. A candidate
    is valid when `t + offset_at(t) == W`. If both are valid (a fold or a normal day), use the earlier one. If one is
    valid, use it. If none is valid (a DST gap), use `t1`, so 02:30 becomes 03:30. These are the same rules as jiff "compatible" disambiguation.
  - Production passes `|t| UtcOffset::local_offset_at(t).unwrap_or(UtcOffset::UTC)`. Tests pass a fake offset function with a DST change.
  - Dependency: `time = { version = "=0.3.55", features = ["local-offset"] }` in `outlooker-core/Cargo.toml`. 0.3.55 is
    already locked (`src-tauri/Cargo.lock:4997`) and already built with `local-offset` and `formatting` (it comes in
    through tauri-plugin-log; `cargo tree -i time -e features` confirms it). Cargo.lock gets no new package. 0.3.55's Unix
    `local_offset_at` has no single-thread restriction (`time-0.3.55/src/sys/local_offset_at/unix.rs:104-108`).
    I rejected `jiff` 0.2.38 (current on crates.io): it is in Cargo.lock only as an optional dependency of serde_with, is not built, and would add 3-4 crates.
- **Per-path sync decision** (pure `fn decide(..) -> Decision` in `importer/queue.rs`, run inside one transaction with
  `newest_job_for_update`, `queue.rs:344-354`, so a manual run and a scheduled run cannot both insert a job):

  | condition, checked in order | result |
  |---|---|
  | newest job queued, running, cancelling or removing | Skip: already active, no overlap |
  | trigger is Scheduled or SyncAll, and `sync_enabled=false` | Skip: disabled |
  | probe fails | record `check_problem`, Skip |
  | no checkpoint, or stat ≠ checkpoint identity | Enqueue: update size, mtime, `checked_at`, clear problem |
  | trigger is Manual (this path or all) and newest job is failed or cancelled | Enqueue: resumes from the cursor |
  | otherwise | Unchanged: set `checked_at` only |

  A scheduled run therefore does not retry a failed or cancelled job on an unchanged file. A corrupt PST does not write
  a `job_failed` row every 15 minutes. "Sync now" retries it.
- **Work Gate and queue**: the scheduler never takes the gate. It only inserts `queued` jobs and calls `kick`
  (`queue.rs:339`). The worker keeps FIFO order, one job at a time, and waits on `gate.released()` while maintenance or a
  data-dir move holds the gate (`runner.rs:54-63`). Overlap is impossible at three levels: the first table row above, the
  `import_jobs_one_active_per_pst` index (V1:184) and `import_jobs_one_running` (V1:185).
- **Manual trigger**: `pst_sync(pstId?)`. `null` means all enabled paths (trigger SyncAll). An id means that path, even when
  it is disabled (trigger Manual). It does not change `sync_last_run_at`: the schedule stays on wall-clock slots.
- **Disabled path**: skipped by scheduled runs and by "sync all". Disabling does not cancel a running job. The user can press Cancel.

## 5. Settings storage

- The schedule and `sync_last_run_at` go in the DB `settings` singleton (V3 columns, §1). Registered paths and their per-path
  flags go in `pst_sources`. Reasons: the scheduler only runs when the DB is Ready. The schedule belongs to the library and moves
  with the data dir. The last-run write is a single SQL statement.
- I rejected `settings.json`: it is the pre-DB bootstrap file (`settings/mod.rs:11-16`, data dir and language only).
- I rejected a new table: the singleton already exists, and a new table would need its own GRANT (reader-backend §7.9).
- SQL lives in `sync/service.rs`. `SettingsStore` (`settings/mod.rs:54-86`) and `Settings`/`SettingsPatch`
  (`dto/settings.rs:7-22`) do not change, so `settings.test.tsx`'s exact-object mock stays valid.

## 6. API surface

DTOs (ts-rs `#[ts(export)]`, camelCase):
```rust
// dto/sync.rs (new)
#[serde(tag = "mode", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SyncSchedule { Interval { minutes: u32 }, Daily { hour: u8, minute: u8, weekdays: Vec<Weekday> } }
pub enum Weekday { Mon, Tue, Wed, Thu, Fri, Sat, Sun }
pub struct SyncState { pub schedule: SyncSchedule, pub last_run_at: Option<String>, pub next_run_at: String } // RFC3339 UTC
// dto/import.rs changes
pub enum PathCheck { Locked, NotFound, Unreadable, NotAPst }
pub enum ImportStatus { /* 7 existing */ Locked, Unavailable }   // Locked = check Locked, Unavailable = other checks
pub struct PstSource { /* existing */ pub enabled: bool, pub check: Option<PathCheck>, pub checked_at: Option<String>,
                       pub queued_at: Option<String> /* was String: a locked registration has no job */ }
pub enum RegisterResult { Registered { pst: PstSource }, Rejected { path: String, error: AppError } } // replaces EnqueueResult
```
- Status priority in `queue::list`: an active job (queued, running, cancelling, removing) comes first. Then `check` Locked
  gives Locked, and any other check gives Unavailable. Then the newest job's terminal status.
  `LIST_SQL` (`queue.rs:185-193`) changes `CROSS JOIN LATERAL` to `LEFT JOIN LATERAL` and selects the new columns.
  `status_of` (`queue.rs:172-183`) takes `Option<(kind, state)>` and the check.
- The statuses the UI needs: idle = Completed; queued = Queued; syncing + progress = Running + `progress` (unchanged);
  locked-waiting = Locked; failed + reason = Failed (the `job_failed` row is in `import_errors`) or Unavailable + `check`;
  disabled = `enabled=false`; last sync = `checkedAt`, last import = `finishedAt`; next sync = `SyncState.nextRunAt`;
  new in last sync = `newMessageCount` (the checkpoint's `stored`, reset per changed snapshot; known items count as
  `duplicates`, so `messageCount` stays the total).
- Commands (`COMMANDS` `dto/commands.rs:2` and the count `:19` go from 24 to 27; `generate_handler!` `src-tauri/src/lib.rs:70-95`;
  `src/api/commands.ts`):
  - removed: `import_enqueue`, `import_resume`
  - added: `pst_register(paths) -> RegisterResult[]`, `pst_sync(pstId: number|null) -> void`,
    `pst_set_enabled(pstId, enabled) -> void`, `sync_schedule_get() -> SyncState`, `sync_schedule_set(schedule) -> SyncState`.
  - `sync_schedule_set` validates minutes 5..=10080, hour < 24, minute < 60 and non-empty weekdays, and returns
    `AppError::invalid("schedule", ...)` on error. It sets `sync_last_run_at = now()`, so a new schedule never fires at once, and wakes the task.
  - kept: `pst_list`, `import_cancel`, `pst_remove`, `import_errors`.
- Events: per-path changes reuse `import:status-changed` with the full `source` (`queue.rs:256-260`). The frontend cache
  patch already replaces the whole `PstSource`. Add **one** event, `sync:state-changed` (`SyncState`). It is emitted on
  schedule set and when a scheduled run starts. Lists to update: `names::ALL` (`dto/events.rs:16`, 6 to 7), `CoreEvent` plus
  `name()`/`payload()` (`sink.rs:17-60`), `src/api/events.ts` `EVENTS`/`EventPayloads`, `src/app/boot.ts:23-30`
  (`setQueryData(["syncState"])`). It is not throttled (`sink/throttle.rs:17-27`). Bindings come from `cargo test`
  (`dto/events.rs:73-77`, `dto/commands.rs:14-21`).

## 7. Deleted, or kept and why

- Delete `queue::resume` (`queue.rs:379-395`), `ImportManager::resume` (`importer/mod.rs:68-70`), the command
  `import_resume` (`src-tauri/src/commands/import.rs:22-23`), `api.importResume` and the Resume/Retry buttons and their locale keys.
  Manual Sync now replaces them (table row 5, §4).
- Replace `import_enqueue`/`queue::enqueue`/`EnqueueResult` with `pst_register`/`register`/`RegisterResult`. Delete the old
  names. `useEnqueuePst` (`src/components/common/hooks.tsx:42-58`) becomes `useRegisterPst`.
- Delete the `(path,size,mtime)` identity lookup in `insert_source` (`queue.rs:303-310`) and the constraint (V3).
- Keep: the onboarding PST step (`src/screens/onboarding/index.tsx:107`). Adding PSTs at first run is still registration;
  only the hook name changes. Keep the remove flow (§1), cancel, the error log, the worker, the gate and resume-after-crash (`queue.rs:479-501`).
- The opt-in e2e specs (`tests/e2e/flow.e2e.ts`) use `pst-status` "Completed" and the add flow. The Import-screen rewrite updates them (not run by default).

## 8. Tests

Committed tests (core logic, data integrity or regressions; each is far below 1 s; they extend existing tests where possible):
1. `sync/schedule.rs` unit tests (new): interval; daily same day before and after the slot; weekday skip across a week;
   catch-up (last run 3 days ago gives one past instant); DST gap (02:30 becomes 03:30) and fold (earlier 01:30) with a
   fake offset function; backward clock clamp.
2. `queue.rs` unit tests: `classify_open` (raw 32 and 33 give Locked; 2, 5 and ErrorKind::PermissionDenied give
   Unreadable); the `decide` table (every row in §4).
3. `runner.rs` `end_of_attempt` test (`runner.rs:304-333`): add the `Locked` → `Deferred` row.
4. `tests/it/import_pipeline.rs`: rewrite `mtime_change_resets_cursor` (`:479`) into **sync of a changed path**.
   - Setup: register a synthetic path with N items, wait for Completed, `touch` the file, `sync(Some(id))`. The second corpus has the same N ids with
     `RawContent::Unreadable` content plus 2 new mails.
   - Assert: one `pst_sources` row, same id; `stored=2`, `duplicates=N`, `errors=0` (proves known items were not read);
     old occurrences kept.
   - Add to the same test: a second `sync` with no change creates no job row.
5. `import_pipeline.rs`: one test for an **unreadable path at sync**. Use a mode-000 file; it stands in for Locked on macOS,
   which uses the same branch except for the raw-code match. Assert `check_problem` is set, the `import_jobs` count and the
   `import_errors` count do not change, and no finalize happens (index OIDs are unchanged, using the existing `index_oids` helper at `:126`).
6. Switch the existing tests that call `resume` (`:378`, `:451`, `:499`) to `sync(Some(id))`, and `enqueue` callers (`:226`, `:257`, `:302`,
   `:539`, `app_compose.rs:117`) to `register`.

One-off checks (scripts/tmp or manual, not committed):
- the identifier-stability check in §2
- the Windows lock check in §3
- the real-app check of scheduled firing: set a 5-minute interval, watch `checkedAt` advance, then close the app past a slot, reopen it,
  and see exactly one catch-up run (one new `checked_at`; `sync_last_run_at` = start time). The user runs this check (headless rule).

## 9. Implementation split

Frozen interfaces between deliverables: the §6 DTOs, the §4 `next_run` signature, and
`ImportManager::{register(Vec<String>), sync(Option<i64>, Trigger), set_enabled(i64,bool)}`, `pub enum Trigger { Scheduled, SyncAll, Manual }`.

| # | deliverable | owns files | depends on |
|---|---|---|---|
| D1 | schedule math + sync DTOs | `core/sync/mod.rs` (new: `pub mod schedule;`), `core/sync/schedule.rs` (new), `core/dto/sync.rs` (new), `core/dto/mod.rs` (1 line), `core/lib.rs` (1 line), `outlooker-core/Cargo.toml`, `src-tauri/Cargo.lock` | none |
| D2 | identity, probe, known-skip, sync decision | `core/db/migrations/V3__pst_sync.sql` (new), `core/importer/{queue,pipeline,runner,pst_reader,types,writer,mod}.rs`, `core/importer/normalize/mod.rs`, `core/dto/import.rs`, `outlooker-core/tests/it/{import_pipeline,app_compose}.rs` | none (the `Trigger` enum lives in `importer/mod.rs`) |
| D3 | scheduler service, commands, events, bindings | `core/sync/service.rs` (new) + 1 line in `core/sync/mod.rs` after D1, `core/app/mod.rs`, `core/dto/{events,commands}.rs`, `core/sink.rs`, `src-tauri/src/commands/{import,mod}.rs`, `src-tauri/src/commands/sync.rs` (new), `src-tauri/src/lib.rs`, `src/api/bindings/*` (generated) | D1 and D2 merged |
| D4 | frontend API layer | `src/api/{commands,events,queries}.ts`, `src/app/boot.ts`, `src/app/boot.test.ts`, `src/app/testing.tsx`, `src/screens/import/importCache.ts`, `src/screens/import/import.test.tsx` (cache part), `src/components/common/hooks.tsx`, `src/screens/onboarding/index.tsx`, `src/locales/{en,zh-TW}/import.json`, plus compile-only fixes in `src/screens/import/index.tsx` (drop Resume/Retry, add status colours) | coding: §6 only; green: D3 bindings |

D1 and D2 run in parallel. D4 starts in parallel against §6 and goes green after D3. The Import-screen rewrite (separate mockup work) starts after D4.

Acceptance (run from `src-tauri/`, canonical commands only, `CARGO_TEST_CAP_S` unset):
All deliverables: `cargo clippy --workspace --all-targets -- -D warnings` is clean.
- D1: `cargo test --workspace --lib schedule` passes with every §8.1 case present (grep the test names).
  `git diff src-tauri/Cargo.lock` adds no `[[package]]` block.
- D2: `cargo test --workspace --test it import_pipeline` and `cargo test --workspace --lib importer` pass with §8.2-8.6
  present. `grep -rn "fn resume\|pst_sources_same_file_once" crates/outlooker-core/src` finds only the V1 file.
- D3: `cargo test --workspace` passes, including `generate_handler_list_equals_commands` (`src-tauri/src/lib.rs:161`)
  and `app_compose` with `STEPS` (`tests/it/app_compose.rs:22`) extended by `SchedulerStarted` after `WorkerStarted`.
  `src/api/bindings/commands.json` has 27 names and `events.json` has 7.
- D4: `npm test` passes (api.test, global.test locale parity, boot.test, import.test). `npm run typecheck` passes.
  `grep -rn "importResume\|importEnqueue\|useEnqueuePst" src` finds nothing.

## 10. Risks and open questions

Questions for the user:
- **Q1 (data)**: the V3 migration fails if your current database holds two Import entries for the same path, which happens when a changed file was imported twice.
  Option A: remove the older entry in Import before upgrading (the default in this proposal).
  Option B: wipe the dev data dir and re-import.
  Do you have such duplicates, and which option do you want?
- **Q2 (semantics)**: Remove deletes every message that only this path holds, including mail you deleted in Outlook and that sync kept.
  Disable keeps the mail. Is "Disable = keep the mail, Remove = delete it" the behaviour you want, or do you need a third action, "unregister but keep the mail"?
  This proposal assumes no third action.

Risks:
- **R1**: libpff identifier stability is unverified (§2). If it fails, a changed 20 GB PST is fully re-parsed on each changed run. The data stays correct; the run is slow.
- **R2**: if Outlook opens the PST in a mode that shares reads, the probe sees no lock. libpff then reads a file that is being written, and scan or parse errors are possible. §3 step 5 detects this before any code ships.
- **R3**: Outlook changes the PST mtime while it is open even when no mail arrives. While Outlook is closed, every scheduled run after Outlook last ran sees "changed" once, then "unchanged". The cost per changed run is a scan plus an identifier read per item (§2).
- **R4**: the daily slot uses the OS offset per instant. A time-zone change while the app runs takes effect when the libc cache refreshes (at the latest, at restart). This is accepted.
