import type { QueryClient } from "@tanstack/react-query";
import { pstListQuery } from "../../api/queries";
import type { ImportProgress } from "../../api/bindings/ImportProgress";
import type { ImportStatusChanged } from "../../api/bindings/ImportStatusChanged";
import type { PstSource } from "../../api/bindings/PstSource";

export function applyStatusChanged(list: PstSource[], ev: ImportStatusChanged): PstSource[] {
  if (ev.removed) return list.filter((p) => p.id !== ev.pstId);
  const source = ev.source;
  if (source) {
    return list.some((p) => p.id === source.id) ? list.map((p) => (p.id === source.id ? source : p)) : [...list, source];
  }
  return list.map((p) => (p.id === ev.pstId ? { ...p, status: ev.status } : p));
}

export function applyProgress(list: PstSource[], progress: ImportProgress): PstSource[] {
  if (!list.some((p) => p.id === progress.pstId)) return list;
  return list.map((p) => (p.id === progress.pstId ? { ...p, progress } : p));
}

export function applyToCache(qc: QueryClient, f: (list: PstSource[]) => PstSource[]): void {
  const current = qc.getQueryData<PstSource[]>(pstListQuery.queryKey);
  if (current) qc.setQueryData(pstListQuery.queryKey, f(current));
}
