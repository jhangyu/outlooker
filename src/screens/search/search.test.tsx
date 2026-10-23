import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api/commands");
vi.mock("../../api/dialog");
vi.mock("../../api/events", async () => (await import("../../app/testing")).eventsMock);

import { api } from "../../api/commands";
import type { MailRow } from "../../api/bindings/MailRow";
import type { SearchPage } from "../../api/bindings/SearchPage";
import { App } from "../../app/App";
import { eventsMock, makeStatus, mockApiDefaults, renderWithProviders } from "../../app/testing";
import { makeMailRow, stubViewport } from "../../components/mail/fixtures";
import { SearchScreen } from ".";

const READY = { status: { counts: { messages: 10, psts: 1 } } };
const page = (rows: MailRow[], shortQuery = false): SearchPage => ({ rows, nextCursor: null, shortQuery, elapsedMs: 1 });

function deferred<T>() {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

/** Types into the search box and lets the 300 ms debounce elapse. */
function type(text: string) {
  fireEvent.change(screen.getByTestId("search-box"), { target: { value: text } });
  act(() => {
    vi.advanceTimersByTime(300);
  });
}

describe("Search screen", () => {
  let restore: () => void;
  beforeEach(() => {
    eventsMock.reset();
    mockApiDefaults();
    restore = stubViewport(800);
    vi.useFakeTimers({ shouldAdvanceTime: true });
  });
  afterEach(() => {
    vi.useRealTimers();
    restore();
  });

  it("searchCount {count: 10000, capped: true} renders 10,000+ and {count: 9999, capped: false} renders 9,999", async () => {
    vi.mocked(api.searchMail).mockResolvedValue(page([makeMailRow(1)]));
    vi.mocked(api.searchCount).mockResolvedValue({ count: 10000, capped: true });
    renderWithProviders(<SearchScreen />, READY);
    type("alpha");
    expect((await screen.findByTestId("result-count")).textContent).toBe("10,000+ messages");
    vi.mocked(api.searchCount).mockResolvedValue({ count: 9999, capped: false });
    type("beta");
    await waitFor(() => expect(screen.getByTestId("result-count").textContent).toBe("9,999 messages"));
    expect(api.searchCount).toHaveBeenLastCalledWith({ kind: "simple", text: "beta" });
  });

  it("a stale search response (older sequence) is dropped", async () => {
    const older = deferred<SearchPage>();
    const newer = deferred<SearchPage>();
    vi.mocked(api.searchMail).mockReturnValueOnce(older.promise).mockReturnValueOnce(newer.promise);
    renderWithProviders(<SearchScreen />, READY);
    type("alpha");
    await waitFor(() => expect(api.searchMail).toHaveBeenCalledTimes(1));
    type("beta");
    await waitFor(() => expect(api.searchMail).toHaveBeenCalledTimes(2));
    await act(async () => newer.resolve(page([makeMailRow(2, { subject: "newer result" })])));
    await act(async () => older.resolve(page([makeMailRow(1, { subject: "older result" })])));
    expect(await screen.findByText("newer result")).toBeTruthy();
    expect(screen.queryByText("older result")).toBeNull();
  });

  it("library:changed refetches filterOptions and shows the new-mail bar without reloading the list", async () => {
    vi.mocked(api.appStatus).mockResolvedValue(makeStatus({ counts: { messages: 10, psts: 1 } }));
    vi.mocked(api.searchMail).mockResolvedValue(page([makeMailRow(1, { subject: "first" })]));
    render(<App />);
    await screen.findByTestId("search-box");
    type("alpha");
    expect(await screen.findByText("first")).toBeTruthy();
    await waitFor(() => expect(api.filterOptions).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(eventsMock.onEvent).toHaveBeenCalledWith("library:changed", expect.any(Function)));
    expect(screen.queryByTestId("new-mail-bar")).toBeNull();
    act(() => eventsMock.emit("library:changed", { counts: { messages: 11, psts: 1 } }));
    await waitFor(() => expect(api.filterOptions).toHaveBeenCalledTimes(2));
    expect(api.searchMail).toHaveBeenCalledTimes(1);
    expect(screen.getByTestId("new-mail-bar")).toBeTruthy();
    fireEvent.click(screen.getByTestId("new-mail-refresh"));
    await waitFor(() => expect(api.searchMail).toHaveBeenCalledTimes(2));
    expect(screen.queryByTestId("new-mail-bar")).toBeNull();
  });

  it("leaving Search, a library change and coming back neither refetches nor misses the new-mail bar", async () => {
    vi.mocked(api.appStatus).mockResolvedValue(makeStatus({ counts: { messages: 10, psts: 1 } }));
    vi.mocked(api.searchMail).mockResolvedValue(page([makeMailRow(1, { subject: "first" })]));
    render(<App />);
    await screen.findByTestId("search-box");
    type("invoice");
    expect(await screen.findByText("first")).toBeTruthy();
    await waitFor(() => expect(api.searchCount).toHaveBeenCalled());
    const mails = vi.mocked(api.searchMail).mock.calls.length;
    const counts = vi.mocked(api.searchCount).mock.calls.length;
    fireEvent.click(screen.getByTestId("nav-import"));
    act(() => eventsMock.emit("library:changed", { counts: { messages: 11, psts: 1 } }));
    fireEvent.click(screen.getByTestId("nav-search"));
    expect(await screen.findByTestId("new-mail-bar")).toBeTruthy();
    expect(api.searchMail).toHaveBeenCalledTimes(mails);
    expect(api.searchCount).toHaveBeenCalledTimes(counts);
  });

  it("keeps the refine pane PST list visible while filterOptions refetches after a library change", async () => {
    vi.mocked(api.appStatus).mockResolvedValue(makeStatus({ counts: { messages: 10, psts: 1 } }));
    const opts = { psts: [{ id: 1, displayName: "ArchivePST" }], folders: [] } as never;
    vi.mocked(api.filterOptions).mockResolvedValueOnce(opts);
    let release: (v: unknown) => void = () => {};
    vi.mocked(api.filterOptions).mockImplementationOnce(() => new Promise((r) => (release = r as never)) as never);
    render(<App />);
    expect(await screen.findByText("ArchivePST")).toBeTruthy();
    act(() => eventsMock.emit("library:changed", { counts: { messages: 11, psts: 1 } }));
    await waitFor(() => expect(api.filterOptions).toHaveBeenCalledTimes(2));
    expect(screen.queryByText("ArchivePST")).not.toBeNull();
    await act(async () => release(opts));
  });

  it("shows the short-query hint and selects the first row", async () => {
    vi.mocked(api.searchMail).mockResolvedValue(page([makeMailRow(1), makeMailRow(2)], true));
    renderWithProviders(<SearchScreen />, READY);
    type("報價");
    expect(await screen.findByTestId("short-query-hint")).toBeTruthy();
    await waitFor(() => expect(api.mailGet).toHaveBeenCalledWith(1));
    expect(screen.getAllByTestId("mail-row")[0]!.getAttribute("aria-selected")).toBe("true");
  });

  it("shows the empty, idle, zero-result and problem states", async () => {
    const empty = renderWithProviders(<SearchScreen />);
    expect(screen.getByText("No mail to search yet")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Go to Import" }));
    expect(empty.go).toHaveBeenCalledWith("import");
    empty.unmount();

    renderWithProviders(<SearchScreen />, READY);
    expect(screen.getByText("Search your mail")).toBeTruthy();
    type("nothing");
    expect(await screen.findByText("No messages match")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Clear search and conditions" }));
    expect((screen.getByTestId("search-box") as HTMLInputElement).value).toBe("");
    type("a b c d e f g h i j k");
    expect(screen.getByText("Use at most 10 search words.")).toBeTruthy();
    expect(api.searchMail).toHaveBeenCalledTimes(1);
  });
});
