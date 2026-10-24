import { queryOptions } from "@tanstack/react-query";
import { api } from "./commands";
import type { DataDirMoveProgress } from "./bindings/DataDirMoveProgress";

export const pstListQuery = queryOptions({ queryKey: ["pstList"], queryFn: () => api.pstList() });

export const syncStatusQuery = queryOptions({ queryKey: ["syncStatus"], queryFn: () => api.syncStatus() });

export const dbStatusQuery = queryOptions({ queryKey: ["dbStatus"], queryFn: () => api.dbStatus(), refetchInterval: 10_000 });

/** Pseudo-query: boot writes `data-dir:move-progress` events into it. */
export const moveProgressQuery = queryOptions({
  queryKey: ["moveProgress"],
  queryFn: (): DataDirMoveProgress | null => null,
  enabled: false,
  initialData: null as DataDirMoveProgress | null,
});
