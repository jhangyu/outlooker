import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api/commands");
vi.mock("../api/dialog");
vi.mock("../api/events", async () => (await import("./testing")).eventsMock);
vi.mock("../screens/search", async () => {
  const { useAppStatus } = await import("./AppStatusContext");
  const { useState } = await import("react");
  return {
    SearchScreen: () => {
      const [q, setQ] = useState("");
      return (
        <>
          <span data-testid="probe">{String(useAppStatus().counts.messages)}</span>
          <input data-testid="search-box" value={q} onChange={(e) => setQ(e.target.value)} />
        </>
      );
    },
  };
});
vi.mock("../components/sql-editor", async (importOriginal) => {
  const { createElement } = await import("react");
  return {
    ...(await importOriginal<typeof import("../components/sql-editor")>()),
    SqlEditor: (p: { value: string; onChange(v: string): void; ariaLabel: string }) =>
      createElement("textarea", { "data-testid": "sql-editor", "aria-label": p.ariaLabel, value: p.value, onChange: (e: { target: { value: string } }) => p.onChange(e.target.value) }),
  };
});

import { api } from "../api/commands";
import { pickFolder } from "../api/dialog";
import { App } from "./App";
import { eventsMock, makeStatus, mockApiDefaults } from "./testing";

describe("App frame", () => {
  beforeEach(() => {
    eventsMock.reset();
    mockApiDefaults();
  });

  it("library:changed with messages > 0 updates useAppStatus().counts.messages", async () => {
    render(<App />);
    expect((await screen.findByTestId("probe")).textContent).toBe("0");
    eventsMock.emit("library:changed", { counts: { messages: 5, psts: 1 } });
    await waitFor(() => expect(screen.getByTestId("probe").textContent).toBe("5"));
  });

  it("Failed screen Retry calls appRetry", async () => {
    vi.mocked(api.appStatus).mockResolvedValue(makeStatus({ phase: "failed", error: { kind: "internal", detail: "x" } }));
    render(<App />);
    fireEvent.click(await screen.findByTestId("failed-retry"));
    await waitFor(() => expect(api.appRetry).toHaveBeenCalledTimes(1));
  });

  it("shows the blocking screen while the database starts and the frame when ready", async () => {
    vi.mocked(api.appStatus).mockResolvedValue(makeStatus({ phase: "startingDb" }));
    render(<App />);
    expect(await screen.findByTestId("blocking-screen")).toBeTruthy();
    eventsMock.emit("app:status-changed", makeStatus({ phase: "ready" }));
    expect(await screen.findByTestId("title-bar")).toBeTruthy();
    expect(screen.getByTestId("nav-import")).toBeTruthy();
  });

  it("keeps the SQL text and the search text when switching screens", async () => {
    render(<App />);
    await screen.findByTestId("search-box");
    expect(screen.queryByTestId("sql-editor")).toBeNull(); // a screen mounts on its first visit
    fireEvent.click(await screen.findByTestId("nav-advancedSql"));
    fireEvent.change(await screen.findByTestId("sql-editor"), { target: { value: "select 1" } });
    fireEvent.click(screen.getByTestId("nav-import"));
    fireEvent.click(screen.getByTestId("nav-advancedSql"));
    expect((screen.getByTestId("sql-editor") as HTMLTextAreaElement).value).toBe("select 1");
    fireEvent.change(screen.getByTestId("search-box"), { target: { value: "invoice" } });
    fireEvent.click(screen.getByTestId("nav-settings"));
    fireEvent.click(screen.getByTestId("nav-search"));
    expect((screen.getByTestId("search-box") as HTMLInputElement).value).toBe("invoice");
  });

  it("shows onboarding while setup is needed", async () => {
    vi.mocked(api.appStatus).mockResolvedValue(makeStatus({ phase: "needsSetup" }));
    render(<App />);
    expect(await screen.findByTestId("onboarding-lang-en")).toBeTruthy();
  });
});

describe("data-folder move", () => {
  beforeEach(() => {
    eventsMock.reset();
    mockApiDefaults();
    vi.mocked(pickFolder).mockResolvedValue("/Volumes/Big/Outlooker");
  });

  async function startMove() {
    render(<App />);
    fireEvent.click(await screen.findByTestId("nav-settings"));
    fireEvent.click(await screen.findByTestId("settings-move"));
    fireEvent.click(await screen.findByRole("button", { name: "Move" }));
    await waitFor(() => expect(api.dataDirMove).toHaveBeenCalledWith("/Volumes/Big/Outlooker"));
  }
  const progress = (phase: string, error: unknown = null) => ({ phase, bytesCopied: 512, bytesTotal: 1024, error });

  it("keeps the move dialog through stoppingDb, startingDb and ready and toasts done", async () => {
    await startMove();
    act(() => eventsMock.emit("app:status-changed", makeStatus({ phase: "stoppingDb" })));
    act(() => eventsMock.emit("data-dir:move-progress", progress("copying")));
    expect(await screen.findByText("512 B of 1.0 KB")).toBeTruthy();
    expect(screen.queryByTestId("blocking-screen")).toBeNull();
    act(() => eventsMock.emit("app:status-changed", makeStatus({ phase: "startingDb" })));
    expect(screen.queryByTestId("blocking-screen")).toBeNull();
    expect(screen.getByText("512 B of 1.0 KB")).toBeTruthy();
    act(() => eventsMock.emit("app:status-changed", makeStatus({ phase: "ready" })));
    act(() => eventsMock.emit("data-dir:move-progress", progress("done")));
    expect(await screen.findByText("The data folder was moved.")).toBeTruthy();
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("shows a failed move's error after the database is back in the old folder", async () => {
    await startMove();
    act(() => eventsMock.emit("app:status-changed", makeStatus({ phase: "stoppingDb" })));
    act(() =>
      eventsMock.emit("data-dir:move-progress", progress("failed", { kind: "fileAccess", path: "/Volumes/Big/Outlooker", reason: "notWritable" })),
    );
    act(() => eventsMock.emit("app:status-changed", makeStatus({ phase: "startingDb" })));
    expect(screen.queryByTestId("blocking-screen")).toBeNull();
    act(() => eventsMock.emit("app:status-changed", makeStatus({ phase: "ready" })));
    expect(await screen.findByText("Cannot access /Volumes/Big/Outlooker.")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(screen.getByTestId("title-bar")).toBeTruthy();
  });
});
