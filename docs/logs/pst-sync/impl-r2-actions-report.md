# impl-r2-actions report — Pause / Unregister / Delete, path identity (contract overrides 7, 8)

Branch `pst-sync-r2-actions`, commit `c6352e5` (base 4d86726). API: `docs/logs/pst-sync/r2-actions-api.md`.
Paths below are under `src-tauri/crates/outlooker-core/` unless they start with `src/` or `src-tauri/src`.

## 1. Conclusions
- **Schema** (`src/db/migrations/V1__schema.sql:12-13`, spec table mirrored): `sync_enabled` → `paused boolean DEFAULT false`
  + `unregistered boolean DEFAULT false`. The dev DB must be reset (V1 edited in place, as agreed).
- **Pause / Resume** = `pst_set_paused(pstId, paused) -> PstSource` (`src/importer/queue.rs:425`).
  - Running job: `hold_running` (`queue.rs:417`) sets the new cancel reason `Hold` (`queue.rs:43`); the attempt ends,
    the checkpoint is committed and the job goes back to `queued` (`src/importer/runner.rs:230`), same as shutdown.
    The search indexes a first import dropped are rebuilt at that point (`runner.rs:268-278`) so search is not left
    index-less while paused.
  - The worker never starts a queued job of a paused or unregistered source: `next_job` filter (`runner.rs:83-90`) and
    the `queued → running` UPDATE guard (`runner.rs:109`). Race: the job is registered as running before that UPDATE,
    so a pause commits either before it (UPDATE takes 0 rows) or after it (pause finds the flag).
  - `sync(All)` and `sync(One)` skip paused paths (`queue.rs:519-535`; One on a paused path is a silent no-op).
  - Resume clears the flag and kicks the worker: the same queued job continues from its checkpoint.
- **Unregister** = `pst_unregister(pstId)` (`queue.rs:441`): sets `unregistered`, holds a running job, emits
  `import:status-changed {removed: true, source: null}` (the existing cache patcher drops the row). The row, folders,
  occurrences and messages stay, so search and `mail_get` (occurrence with path) keep working. `pst_list` hides it
  (`queue.rs:204`); `sync(One)`, Pause and Unregister answer NotFound.
- **Re-register** (`insert_source`, `queue.rs:309`): a registration whose canonical path equals an unregistered row
  reuses that row (clears both flags, updates size); it queues a job unless a held job is still queued (then that one
  resumes). Same pst_id → known item ids are skipped, no duplicate messages or occurrences.
- **Delete** = existing `pst_remove`, unchanged; works on a paused path. Pause/Unregister during a removal → Busy.
- **Removed**: `pst_set_enabled`, `import_cancel`, `ImportManager::{set_enabled, cancel}`, `CancelReason::User`,
  the `Cancelled` finalize path, `RunningJob.job_id`. Command count stays 27.
- **Path identity** (`canonical` / `plain_path`, `queue.rs:289-305`): `register` canonicalizes every input with
  `std::fs::canonicalize` inside `spawn_blocking` together with the probe (`queue.rs:352-380`). No cfg. A path that
  does not resolve keeps its input form and the probe rejects it. `Rejected.path` stays the input string;
  `Queued.pst.path` is the canonical form.

### Path-identity evidence (Rust 1.99.1 std source, fetched from rust-lang/rust tag 1.99.1, plus a probe here)
- `library/std/src/sys/fs/unix.rs:1998-2008`: `canonicalize` = `libc::realpath`.
  macOS probe (APFS case-insensitive, `scripts/tmp/canon.rs`): `case-probe/sub/mail.pst`, `case-probe/SUB/../Sub//Mail.PST`,
  `case-probe/Sub/./MAIL.pst` → all `…/case-probe/Sub/Mail.PST` (on-disk case, `..`, `.`, `//` resolved).
- `library/std/src/sys/fs/windows.rs:1593-1610`: opens with `access_mode(0)` + `FILE_FLAG_BACKUP_SEMANTICS`, default
  share mode `FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE` (`:206`), then
  `GetFinalPathNameByHandleW(…, VOLUME_NAME_DOS)`. `library/std/src/fs.rs:3071-3087` documents the `\\?\` extended
  path. `plain_path` strips `\\?\` for drive paths and maps `\\?\UNC\` to `\\` when shorter than 260 chars.
- Linux (case-sensitive): different case = different file; the test then expects `notFound` for the case variant.

## 2. Changed files (commit c6352e5)
`src-tauri/crates/outlooker-core/src/db/migrations/V1__schema.sql`, `src/dto/import.rs`, `src/dto/commands.rs`,
`src/importer/{mod,queue,runner}.rs`, `tests/it/import_pipeline.rs`, `src-tauri/src/commands/import.rs`,
`src-tauri/src/lib.rs`, `src/api/{commands.ts,api.test.ts}`, `src/api/bindings/{PstSource.ts,commands.json}` (regenerated
by `cargo test --workspace --lib`), `src/app/testing.tsx`, `src/screens/import/{index.tsx,syncState.ts,syncState.test.ts}`,
`docs/superpowers/specs/2026-10-21-outlooker-design.md` (schema table row), `docs/logs/pst-sync/r2-actions-api.md`.

Import screen compile fixes only (UI redesign left to the UI implementer): `src/screens/import/index.tsx` enable switch
→ `checked={!p.paused}` / `pstSetPaused(p.id, !d.checked)`, activity Cancel button → `pstSetPaused(id, true)`,
`enabledCount` counts `!p.paused`; `syncState.ts` `rowState` reads `paused` (still names the state "disabled").

## 3. Acceptance
1. PASS — tests (solo times): `pause_resume_continues_from_checkpoint` 0.35 s (`tests/it/import_pipeline.rs:442`),
   `unregister_keeps_mail_and_reregister_adds_no_duplicates` 0.27 s (`:888`), `register_same_path_rejected` extended with
   `.`, `..`, `//` and case variants 0.18 s (`:781`), Delete: `pst_remove_with_queued_job` extended (Delete on a paused
   path, Busy for Pause/Unregister during removal, `:760`) plus existing `byte_copy_dedupes_and_removal` and
   `tests/it/removal.rs`; `sync_unchanged_creates_no_job` rewritten for paused (`:860`); unit `windows_verbatim_prefix_dropped`
   (`queue.rs:774`). Red proofs (scratch, reverted from backup): removing the worker's paused filter fails the pause test
   (`left: Completed, right: Queued`); disabling canonicalization fails `register_same_path_rejected`.
2. PASS — `cargo test --workspace --test it`: 81 passed, 0 failed, 1 ignored (the runner's solo reruns of load-slowed
   tests then hit the 40 s foreground cap; machine load 40-80 on 28 cores). `cargo test --workspace --lib`: only
   `golden_inline` fails (other member); with `-- --skip golden_inline` all green (7 + 126 + 0 + 0).
3. MOVED TO MERGED GATE (lead decision): clippy runs on main with its warm target dir. `cargo build --workspace` and
   `cargo build --workspace --tests` compile with no warnings on this branch.
4. PASS — the grep returns 0 hits.
5. PASS — `mise exec -- npm test` 20 files / 64 tests green; `mise exec -- npm run build` green.
6. PASS — commit `c6352e5` on `pst-sync-r2-actions`, explicit pathspec, `git status --porcelain` empty afterwards.

## 4. Uncertainties and notes
- **Windows not run** (separate Windows phase): canonicalize of a PST held by Outlook relies on CreateFile with
  access 0 not hitting a sharing violation; if it does fail, the input path is used (identity then falls back to the
  raw string for that registration). Check in the Windows phase: register a PST while Outlook is open, via two spellings.
- `ImportStatus::Cancelling` / `Cancelled` (and `JobState::Cancelling`) are no longer reached by a listed path (nothing
  sets `cancelling` now; `cancelled` only for the queued job a Delete replaces). Kept to limit scope; parking-lot cleanup.
- A path unregistered while its job was queued keeps that queued job (hidden, never started) until it is registered
  again (then it resumes) — intended; Delete of an unregistered path is not reachable from the UI.
- Search filter by source: unregistered paths disappear from `pst_list`, so a UI source picker built from it cannot
  select them; their mail is still found by every other condition.
- Test hazard found: `test_support/template.rs:57-74` shares `$TMPDIR/outlooker-test-pg` across worktrees and deletes
  templates of other schema keys; I ran tests with `TMPDIR=<worktree>/scripts/tmp/t`. Reported to the lead.
- register vs `probe::probe_blocking` (impl-r2-fixes, 2d7f6e1): register already runs canonicalize + probe + the
  metadata fallback inside one `spawn_blocking` (`queue.rs:355-367`); `probe_blocking` does not exist on this branch.
  At merge, keep this closure (canonicalize must run in the same blocking call).
- Merge overlap with impl-r2-fixes-opus: `queue.rs` `sync()` target query (`queue.rs:519-535`) and `register` (spawn_blocking
  around canonicalize + probe, override 9 style) are near their `sync_one` region; `sync_one` itself is untouched.
