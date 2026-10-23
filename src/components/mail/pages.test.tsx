import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api/commands");
vi.mock("../../api/events", async () => (await import("../../app/testing")).eventsMock);

import { api } from "../../api/commands";
import type { SearchPage } from "../../api/bindings/SearchPage";
import type { SearchQuery } from "../../api/bindings/SearchQuery";
import type { SearchRequest } from "../../api/bindings/SearchRequest";
import { mockApiDefaults, renderWithProviders } from "../../app/testing";
import { MAX_LIST_ROWS, MailList, PAGE_SIZE, nextPageParam, useMailPages } from ".";
import { makeMailRow, stubViewport } from "./fixtures";

const QUERY: SearchQuery = { kind: "simple", text: "loadgen" };

describe("mail paging", () => {
  beforeEach(() => mockApiDefaults());

  it("nextPageParam stops at the last cursor and at the cap", () => {
    const rows = Array.from({ length: PAGE_SIZE }, (_, i) => makeMailRow(i + 1));
    const page = (next: string | null): SearchPage => ({ rows, nextCursor: next, shortQuery: false, elapsedMs: 1 });
    expect(nextPageParam(page("c1"), [page("c1")])).toBe("c1");
    expect(nextPageParam(page(null), [page(null)])).toBeUndefined();
    const full = Array.from({ length: 500 }, () => page("more"));
    expect(nextPageParam(full[499]!, full)).toBeUndefined();
    expect(nextPageParam(full[498]!, full.slice(0, 499))).toBe("more");
  });

  it("with 600 mock pages of 200 rows the list stops requesting after 100,000 rows and shows the cap row", async () => {
    vi.mocked(api.searchMail).mockImplementation(async (req: SearchRequest) => {
      const n = req.cursor === null ? 0 : Number(req.cursor);
      return {
        rows: Array.from({ length: PAGE_SIZE }, (_, i) => makeMailRow(n * PAGE_SIZE + i + 1)),
        nextCursor: n < 599 ? String(n + 1) : null,
        shortQuery: false,
        elapsedMs: 1,
      };
    });
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
    const { result } = renderHook(() => useMailPages(QUERY, 1), { wrapper });
    await waitFor(() => expect(result.current.rows.length).toBe(PAGE_SIZE));
    // More calls than pages: the hook itself must stop at the cap.
    for (let i = 0; i < 600; i++) await act(() => result.current.fetchNextPage());
    await waitFor(() => expect(result.current.capped).toBe(true));
    expect(api.searchMail).toHaveBeenCalledTimes(500);
    expect(vi.mocked(api.searchMail).mock.calls[0]![0]).toEqual({ query: QUERY, cursor: null, pageSize: 200 });
    expect(result.current.rows.length).toBe(MAX_LIST_ROWS);
    expect(result.current.hasMore).toBe(false);

    const restore = stubViewport(800);
    renderWithProviders(
      <MailList rows={result.current.rows} selectedId={null} onSelect={vi.fn()} hasMore={result.current.hasMore}
        capped={result.current.capped} loadingMore={false} onEndReached={vi.fn()} ariaLabel="results" />,
    );
    expect(screen.getByTestId("result-cap-row").textContent).toBe("Showing the first 100,000 results — refine the search");
    expect(screen.queryByTestId("list-loader-row")).toBeNull();
    expect(screen.getAllByTestId("mail-row").length).toBeLessThan(60);
    restore();
  }, 60_000);

  it("a new seq evicts the superseded searchMail entry", async () => {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
    const { result, rerender } = renderHook(({ seq, q }) => useMailPages(q, seq), {
      wrapper,
      initialProps: { seq: 1, q: QUERY },
    });
    await waitFor(() => expect(result.current.isLoading).toBe(false));
    rerender({ seq: 2, q: { kind: "simple", text: "other" } });
    await waitFor(() => expect(result.current.isLoading).toBe(false));
    const entries = qc.getQueryCache().findAll({ queryKey: ["searchMail"] });
    expect(entries.length).toBe(1);
    expect(entries[0]!.queryKey[1]).toBe(2);
  });

  it("keeps the current search pages while no screen observes them", async () => {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
    const { result, unmount } = renderHook(() => useMailPages(QUERY, 1), { wrapper });
    await waitFor(() => expect(result.current.isLoading).toBe(false));
    vi.useFakeTimers(); // before unmount, so the cache's gc timer is a fake one
    try {
      unmount();
      vi.advanceTimersByTime(6 * 60_000);
    } finally {
      vi.useRealTimers();
    }
    expect(qc.getQueryCache().find({ queryKey: ["searchMail", 1, QUERY] })).toBeDefined();
  });
});
