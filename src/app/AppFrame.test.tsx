import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api/commands");
vi.mock("../api/dialog");
vi.mock("../api/events", async () => (await import("./testing")).eventsMock);
vi.mock("../screens/search", async () => {
  const { useAppStatus } = await import("./AppStatusContext");
  return { SearchScreen: () => <span data-testid="probe">{String(useAppStatus().counts.messages)}</span> };
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
