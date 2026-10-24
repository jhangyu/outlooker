# PST scheduled sync — implementation contract (frozen 2026-10-24, only the user may change it)

## End state
Registered PST paths sync on an interval or daily schedule and on "Sync now", append-only, one Import entry per path;
a PST locked by another program shows "locked, waiting" and is retried on the next run; the Import screen follows
mockup `docs/design/import-sync/A1.html`.

## Base design
`docs/logs/pst-sync/arch-2.md` is the design, with these user-approved overrides (they win over arch-2):
1. **Time crate = `time`, not `jiff`.** Use `time = { version = "=0.3.55", features = ["local-offset"] }` in
   `outlooker-core/Cargo.toml` (already locked and compiled with `local-offset`). Next-run algorithm for daily mode and
   DST resolution: `docs/logs/pst-sync/arch-1.md` lines 129-145 (offset function injected, tests pass a fake offset with
   a DST change). `Cargo.lock` gains no new package. No `jiff` anywhere.
2. **Schema: edit `V1__schema.sql`, no new migration.** The dev data folder is reset by the user; no merge/compat code.
3. **Disable keeps mail, Remove deletes mail only this path holds.** No third action.
4. **Outlook starting during a sync may report "in use": accepted.** Automatic runs stay.
5. **Schedule has an "off" mode** in addition to interval and daily: `SyncSchedule` adds `{ mode: "off" }`
   (no automatic runs, no catch-up; Sync now still works; `nextRunAt = null`). Default for a fresh DB: interval 60 min.
6. **Import screen = mockup A1** (`docs/design/import-sync/A1.html`, notes `A-NOTES.md`), built with
   `@fluentui/react-components` 9.74.10 and the existing app conventions.

## Deliverables (file ownership exclusive; see arch-2 §9 for the full owned-file lists)
- **A Sync core** (identity, probe, item skip, locked state, index-policy fix, VACUUM skip) — git worktree branch `pst-sync-core`.
- **B Scheduler** (`core/sync/`, `dto/sync.rs`, app lifecycle wiring, `sync:status-changed` event) — worktree branch `pst-sync-scheduler`.
- **C Commands + frontend plumbing** (Tauri commands, `src/api/*`, boot subscription, onboarding, remove `useEnqueuePst`,
  `src/locales/*/common.json`, onboarding locale) — main tree; Rust side after A and B merge.
- **D Import screen** (`src/screens/import/*`, `src/locales/*/import.json`) — main tree, against the arch-2 §6 contract.
  Keep the in-flight `logMsg` `userSelect: "text"` line.

## Acceptance (all must pass on main after merge; cargo from `src-tauri/` with CLAUDE.md canonical commands)
1. A: `cargo test --workspace --test it import_pipeline` green including arch-2 tests 3a–3d; `cargo test --workspace --lib probe` green;
   `grep -rnwE "check_file|import_resume|pst_sources_same_file_once" src-tauri/crates src-tauri/src` → 0 hits.
2. B: `cargo test --workspace --lib schedule` green (interval, daily, weekday wrap, DST gap and overlap, off mode, catch-up once);
   `cargo test --workspace --test it app_compose` green; `grep -rn jiff src-tauri/crates/*/Cargo.toml src-tauri/Cargo.toml` → 0;
   `git diff 806eae0 -- src-tauri/Cargo.lock` adds no `[[package]]`.
3. C: bindings regenerated, `commands.json`/`events.json` hold 27 / 7 names; `grep -rnE "importEnqueue|importResume|useEnqueuePst" src` → 0.
4. D: Import screen uses only `pstList`, `pstRegister`, `pstSetEnabled`, `syncNow`, `syncStatus`, `syncScheduleSet`,
   `importCancel`, `pstRemove`, `importErrors` and event `sync:status-changed`; zh-TW and en `import.json` have identical key sets.
5. Whole tree on main: `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo test --workspace` green;
   `mise exec -- npm test` green; `mise exec -- npm run build` green; every new test ≤ 1 s.
6. Real-app check by the user (agents do not open app windows): register a PST, Sync now → completed; change schedule → next sync time updates.

## Out of scope / later phases
Windows verification over SSH (arch-2 §3 steps 1-3 and the item-id stability check, arch-2 §2) — separate Windows phase.
VSS, mirror sync, per-path schedules, tray/autostart, file watchers, run-history table, interval presets/limits beyond
validation, "change path" action.

## Round budget
3 rounds. Parking lot: `docs/logs/pst-sync/parking-lot.md`.

## Round 2 (user decisions 2026-10-24, appended by the user's instruction)
End state: the round-1 end state, plus the 3 review fixes, per-path actions Pause / Unregister / Delete, blocking file
I/O split off shared locks, case-insensitive path identity, and the HTML sanitizer keeping image width/height.
Overrides of round 1 (they win):
7. **Per-path actions = Pause, Unregister (zh-TW 取消), Delete.** Pause stops a running sync (checkpoint kept) and stops
   all scheduled and manual syncs until Resume; Resume continues from the checkpoint. Pause replaces both "Disable" and
   the per-run Cancel. Unregister removes the path from the Import list and never syncs it again; its mail stays
   stored and searchable; registering the same path again later works and adds no duplicate messages. Delete = the
   existing remove (it deletes mail only this path holds).
8. **Path identity is case- and form-insensitive** wherever the file system treats paths that way: registering the
   same file via a different case, separators or `..` segments yields one registration. No cfg branches.
9. **Blocking file I/O (metadata, probe) runs off the async workers and holds no DB row lock or sync lock**; a slow
   path (network share) does not delay other paths' syncs or the import worker.
10. **The sanitizer keeps `<img>` width/height** (validated numeric values only); `golden_inline` is updated and green.
Round-2 acceptance (on main after merge):
R1. Review fixes (docs/logs/pst-sync/review-round1.md 1-3): a failed removal is not re-imported by a scheduled sync;
    poison items from earlier syncs are skipped; a repeatedly failing path does not grow error rows/count per slot.
    Each has one regression test (data integrity).
R2. Pause/Resume, Unregister and re-register, Delete each have a test of their data effect; UI shows the three actions;
    zh-TW/en key parity holds.
R3. Path-identity test: case/separator/`..` variants of one existing file register once.
R4. A test or scratch check (output attached) shows a blocked probe on one path does not delay another path's sync.
R5. `golden_inline` green; whole `cargo test --workspace` green; clippy clean; `npm test` and `npm run build` green.
R6. Fresh review round 2 finds no blocker. Then the dev data root is reset (no backup, user-approved) and the
    previously imported PSTs are registered and synced again.
Round-2 additions (user decisions 2026-10-24):
11. **Fresh failure reason**: when a failing path fails for a different reason (e.g. file now missing), a new failed
    job/error row is written and the UI shows the new reason. Same reason → no new row (review finding 3 still holds).
12. **Relative times** on the Import screen for last sync, next sync and last run ("3 分鐘前" / "in 5 min"), with the
    absolute date-time as a tooltip; zh-TW and en.
13. **Resizable side panel** on the Import screen using the existing Splitter / useSplit, like the other screens.
    Real-app resize check by the user (jsdom layout is not evidence).
Kept as-is (user): time-zone change while running needs a restart. Native time input, no per-run counters, error log
as the third side card, no live next-sync preview.
R7. 11: a test shows a changed reason writes one new row and the same reason writes none. 12/13: `npm test` and
    `npm run build` green; relative-time formatting logic has one unit test (data transform).

## Round 3 (last round of the budget; user decisions 2026-10-24, appended by the user's instruction)
End state: the round-2 end state, plus the 2 should-fix findings and 4 nits of `docs/logs/pst-sync/review-round2.md`
fixed, a "Reset database" action on the database-format-error screen, and the dev data root reseeded.
In scope: review-round2 findings (lines 48-125); the reset action; `outlooker-loadgen/src/load.rs`; `scripts/tmp/demo-root`.
Out of scope: Windows verification, VSS, mirror sync, per-path schedules, tray/autostart, parking-lot items.
Overrides of round 2 (they win):
14. **Reset database**: when the database cannot be opened because its format is wrong (migration failure), the
    warning screen offers "Reset database". After a confirmation dialog it deletes only the app's database data
    directory and the crash record `data_root/import-inflight` (user decision 2026-10-24: its old job id would misfire on the
    fresh database), creates a fresh database, and returns to the first-run flow. It deletes nothing else.
15. **Dev data root** `scripts/tmp/demo-root` is deleted without backup (user-approved, random demo mail only) and
    reseeded by the loadgen with 10,000 messages; loadgen fake sources are written as paused.
Round-3 acceptance (on main after merge):
R8. Same-job error rows: a regression test pauses mid-job, changes the file, resumes, and asserts the error row count
    does not grow (seen red, then green), ≤ 1 s.
R9. Stalled probe: a test shows that while one path's probe is blocked, the next tick syncs the other paths and
    `Scheduler::stop` returns at once; one sync per path at a time.
R10. Nits: Pause/Unregister during the probe queues no job; Delete is disabled while a sync runs; a failed removal
    offers Delete instead of Retry; the `\\?\` strip counts UTF-16 units. Each verified (run output); tests only for core logic.
R11. Reset: on a migration failure the warning screen shows the reset button; after confirmation the DB is empty and
    usable and the app is at first run; one test (data integrity/safety: only the data directory is deleted).
    One fresh red-team review of the reset finds no blocker.
R12. `scripts/tmp/demo-root` reseeded with 10,000 messages; Import screen shows the fake sources as Paused, not failed.
R13. Whole tree on main: clippy clean; `cargo test --workspace` (lib, it, cluster_lifecycle) green; `cargo build --release`
    0 `^warning:` lines; `npm test` and `npm run build` green; each gate log holds `RC=$?` and the commit hash.
R14. Real-app check by the user: register `tests/fixtures/external/outlook.pst`, Sync now → completed; schedule change
    updates next sync; splitter resize; Pause/Resume/Unregister/Delete; reading-pane images fit; reset flow.
