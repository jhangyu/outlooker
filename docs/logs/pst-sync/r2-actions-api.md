# PST sync round 2 — per-path actions API (Pause / Unregister / Delete)

Contract overrides 7 and 8. Branch `pst-sync-r2-actions`. Command count stays 27.

## Commands
| Command (TS `api.*`) | Args | Returns | Effect |
|---|---|---|---|
| removed: `pst_set_enabled` (`pstSetEnabled`) | | | replaced by `pst_set_paused` |
| removed: `import_cancel` (`importCancel`) | | | replaced by `pst_set_paused(id, true)` |
| `pst_set_paused` (`pstSetPaused(pstId, paused)`) | `pstId: number, paused: boolean` | `PstSource` | `true` = Pause: a running sync stops after the current item, its checkpoint is committed and the job goes back to `queued`; queued jobs of a paused path are not started; scheduled sync and `sync_now(pstId)` do nothing for it. `false` = Resume: the queued job (if any) continues from the checkpoint. Idempotent. Errors: `notFound` (unknown or unregistered id), `busy{pstRemoving}` while a removal is queued/running. |
| `pst_unregister` (`pstUnregister(pstId)`) | `pstId: number` | `null` | Path leaves `pst_list` and is never synced again; its mail stays stored, searchable and readable. A running sync stops (checkpoint kept). Emits `import:status-changed` with `removed: true, source: null`. Errors: `notFound`, `busy{pstRemoving}`. |
| `pst_remove` (`pstRemove(pstId)`) — unchanged = **Delete** | `pstId: number` | `null` | Deletes the path and every message only this path holds. Works on paused paths. |
| `pst_register` (`pstRegister(paths)`) — changed | `paths: string[]` | `EnqueueResult[]` | Each path is canonicalized first (`std::fs::canonicalize`: absolute, `.`/`..` resolved, separators normalized, on-disk letter case; Windows `\\?\` prefix stripped for drive and UNC paths). A path equal to an **unregistered** one is registered again on the same source row (no duplicate messages; an interrupted sync resumes from its checkpoint). Equal to a registered one → `rejected{alreadyImported}`. `queued.pst.path` is the canonical form; `rejected.path` is the input string. |
| `sync_now(pstId \| null)` — changed | | `null` | `null`: every path that is not paused. `pstId` of a paused path: no-op. |
| `pst_list`, `import_errors`, `sync_status`, `sync_schedule_set` | | | unchanged (`pst_list` omits unregistered paths) |

## DTO changes
- `PstSource.syncEnabled: boolean` → **`PstSource.paused: boolean`** (true = Paused; show on top of `status`).
- `ImportStatus` values unchanged: `queued | running | cancelling | cancelled | failed | completed | removing | locked`.
  With the cancel command gone, `cancelling` / `cancelled` are not reached by a listed path (kept in the type until a cleanup).
- UI derivation for a paused path: `paused && status === "running"` = pausing (stops after the current item);
  `paused && status === "queued"` = paused mid-sync (Resume continues); `paused` with any other status = paused, idle.

## Schema (V1 edited in place)
`pst_sources.sync_enabled` → `paused boolean NOT NULL DEFAULT false` plus `unregistered boolean NOT NULL DEFAULT false`.
