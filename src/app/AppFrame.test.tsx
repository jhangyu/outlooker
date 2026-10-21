import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api/commands");
vi.mock("../api/dialog");
vi.mock("../api/events", async () => (await import("./testing")).eventsMock);
vi.mock("../screens/search", async () => {
  const { useAppStatus } = await import("./AppStatusContext");
  return { SearchScreen: () => <span data-testid="probe">{String(useAppStatus().counts.messages)}</span> };
});

import { api } from "../api/commands";
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
