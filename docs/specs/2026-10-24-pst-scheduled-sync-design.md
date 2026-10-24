# PST scheduled sync — requirements

Status: requirements frozen by the user 2026-10-24. Architecture and Import screen design are produced by the team
(architect proposals in `docs/logs/pst-sync/`, mockups in `docs/mockups/import-sync/`) and chosen by the user.

## End state
The user registers one or more PST paths. The app syncs each path on a schedule and on demand, adding new mail to
the database, and shows clearly when a PST is locked by Outlook and will be retried.

## Decisions (user)
1. **Append-only sync.** A sync adds messages that are not in the database yet. Messages deleted or moved in Outlook
   stay in the database. Nothing is removed by a sync.
2. **Locked PST = detect and defer.** If the PST cannot be opened because Outlook (or anything else) holds it, the
   sync of that path is skipped, the path shows a "locked, waiting for Outlook to close" state, and the next scheduled
   run or a manual trigger retries. No VSS / shadow copy, no admin rights, no service.
3. **Schedule = one of two modes**, plus a "Sync now" button:
   - interval: every N minutes or N hours;
   - daily: at a fixed time (HH:MM), on selected weekdays.
4. **Only while the app runs.** No tray, no autostart. On app start, if a scheduled run was missed while the app was
   closed, run once (one catch-up run, not one per missed slot).
5. **Multiple paths, one global schedule** (default, user did not object). Each path can be enabled/disabled.

## Constraints from the current code
- PST identity today is `pst_sources (path, file_size_bytes, file_mtime)` UNIQUE; a changed file re-imports as a new
  source row. Sync must keep **one entry per registered path** in the Import list, not one row per sync run.
- Cross-PST dedupe (`importer/dedupe.rs`) already makes re-writing known messages a no-op; the cost to watch is
  re-parsing a large PST (20 GB target) on every run. Unchanged file (size + mtime) = skip without opening.
- Import jobs go through the existing queue, Work Gate and resume machinery (`importer/queue.rs`, `gate/`); sync must
  reuse them, not add a second pipeline.
- Platform policy: no per-platform branches unless unreachable. Lock detection must be the same code path on macOS
  and Windows (e.g. open failure classification), verified on Windows over SSH in the Windows phase.
- UI: Fluent 2 (`@fluentui/react-components` 9.74.10, `webLightTheme` only), zh-TW and en locale files with identical
  keys.
- Tests: only for core logic (schedule next-run computation, lock classification, sync state machine, identity
  handling) per `.claude/CLAUDE.md` test policy.

## Out of scope
VSS / shadow copy, mirror sync (deleting mail), per-path schedules, background sync while the app is closed, tray,
autostart, filesystem watchers.
