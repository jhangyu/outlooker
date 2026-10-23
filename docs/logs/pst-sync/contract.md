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
