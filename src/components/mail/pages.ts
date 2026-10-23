import { useInfiniteQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo } from "react";
import { api } from "../../api/commands";
import type { AppError } from "../../api/bindings/AppError";
import type { MailRow } from "../../api/bindings/MailRow";
import type { SearchPage } from "../../api/bindings/SearchPage";
import type { SearchQuery } from "../../api/bindings/SearchQuery";
import { toAppError } from "../common";

export const PAGE_SIZE = 200;
/** Part 03 §5.2: 500 pages of 200; the scroll container stays ≤ 3.2M px. */
export const MAX_LIST_ROWS = 100_000;

export function loadedRows(pages: SearchPage[]): number {
  return pages.reduce((n, p) => n + p.rows.length, 0);
}

export function nextPageParam(last: SearchPage, pages: SearchPage[]): string | undefined {
  if (last.nextCursor === null || loadedRows(pages) >= MAX_LIST_ROWS) return undefined;
  return last.nextCursor;
}

export interface MailPages {
  rows: MailRow[];
  hasMore: boolean;
  capped: boolean;
  shortQuery: boolean;
  isLoading: boolean;
  isFetchingNextPage: boolean;
  error: AppError | null;
  fetchNextPage(): Promise<unknown>;
  reset(): void;
}

/** `seq` is the Search screen's sequence number: a response for an older sequence lands in another cache entry and is never shown. */
export function useMailPages(query: SearchQuery | null, seq: number): MailPages {
  const qc = useQueryClient();
  const q = useInfiniteQuery({
    queryKey: ["searchMail", seq, query],
    queryFn: ({ pageParam }) => api.searchMail({ query: query as SearchQuery, cursor: pageParam, pageSize: PAGE_SIZE }),
    initialPageParam: null as string | null,
    getNextPageParam: (last, pages) => nextPageParam(last, pages),
    enabled: query !== null,
    staleTime: Infinity, // changes only through seq or reset()
    gcTime: Infinity, // a hidden screen has no observer, and the effect below keeps one entry
  });
  // Each new composed query gets a new seq; drop the superseded entries so their pages do not pile up.
  useEffect(() => {
    qc.removeQueries({ queryKey: ["searchMail"], predicate: (x) => x.queryKey[1] !== seq });
  }, [qc, seq]);
  const pages = q.data?.pages;
  const derived = useMemo(() => {
    const list = pages ?? [];
    const all = list.flatMap((p) => p.rows);
    const last = list[list.length - 1];
    return {
      rows: all.length > MAX_LIST_ROWS ? all.slice(0, MAX_LIST_ROWS) : all,
      capped: all.length >= MAX_LIST_ROWS && last !== undefined && last.nextCursor !== null,
      shortQuery: list[0]?.shortQuery ?? false,
    };
  }, [pages]);
  return {
    ...derived,
    hasMore: q.hasNextPage,
    isLoading: q.isLoading,
    isFetchingNextPage: q.isFetchingNextPage,
    error: q.error ? toAppError(q.error) : null,
    // cancelRefetch: false → a call while a page is loading joins it instead of restarting it.
    fetchNextPage: () => q.fetchNextPage({ cancelRefetch: false }),
    reset: () => void qc.resetQueries({ queryKey: ["searchMail", seq] }),
  };
}
