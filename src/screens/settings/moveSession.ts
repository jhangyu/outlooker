import { useQuery, type QueryClient } from "@tanstack/react-query";

/** A move the user confirmed and whose dialog is still open; null otherwise. Set before `data_dir_move` is called. */
export interface MoveSession {
  target: string;
}

const KEY = ["moveSession"] as const;

export function useMoveSession(): MoveSession | null {
  return useQuery<MoveSession | null>({ queryKey: KEY, queryFn: () => null, enabled: false, initialData: null, gcTime: Infinity }).data ?? null;
}

export function setMoveSession(qc: QueryClient, session: MoveSession | null): void {
  qc.setQueryData(KEY, session);
}
