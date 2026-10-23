import type { QueryClient } from "@tanstack/react-query";
import { api } from "../api/commands";
import { dbStatusQuery, moveProgressQuery, syncStatusQuery } from "../api/queries";
import { onEvent } from "../api/events";
import type { AppStatus } from "../api/bindings/AppStatus";
import type { LibraryCounts } from "../api/bindings/LibraryCounts";
import type { PstSource } from "../api/bindings/PstSource";
import { applyProgress, applyStatusChanged, applyToCache } from "../screens/import/importCache";

export interface BootSink {
  status(s: AppStatus): void;
  counts(c: LibraryCounts): void;
}

export interface BootResult {
  status: AppStatus;
  psts: PstSource[] | null;
  unlisten(): void;
}

/** Part 03 §9: subscribe to every event first, then take the snapshot, so no transition is missed. */
export async function boot(qc: QueryClient, sink: BootSink): Promise<BootResult> {
  const unlisteners = await Promise.all([
    onEvent("app:status-changed", (s) => sink.status(s)),
    onEvent("import:progress", (p) => applyToCache(qc, (l) => applyProgress(l, p))),
    onEvent("import:status-changed", (e) => applyToCache(qc, (l) => applyStatusChanged(l, e))),
    onEvent("sync:status-changed", (e) => qc.setQueryData(syncStatusQuery.queryKey, e)),
    onEvent("library:changed", (e) => sink.counts(e.counts)),
    onEvent("data-dir:move-progress", (p) => qc.setQueryData(moveProgressQuery.queryKey, p)),
    onEvent("maintenance:status-changed", () => void qc.invalidateQueries({ queryKey: dbStatusQuery.queryKey })),
  ]);
  const unlisten = () => unlisteners.forEach((u) => u());
  try {
    const status = await api.appStatus();
    const psts = status.phase === "ready" ? await api.pstList().catch(() => null) : null;
    return { status, psts, unlisten };
  } catch (e) {
    unlisten();
    throw e;
  }
}
