import { QueryClient } from "@tanstack/react-query";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api/commands");
vi.mock("../api/events", async () => (await import("./testing")).eventsMock);

import { api } from "../api/commands";
import { boot } from "./boot";
import { eventsMock, makePst, makeStatus, mockApiDefaults } from "./testing";

const ALL_EVENTS = [
  "app:status-changed",
  "import:progress",
  "import:status-changed",
  "library:changed",
  "data-dir:move-progress",
  "maintenance:status-changed",
];

describe("boot", () => {
  beforeEach(() => {
    eventsMock.reset();
    mockApiDefaults();
  });

  it("boot subscribes to every event before calling app_status", async () => {
    const order: string[] = [];
    eventsMock.onEvent.mockImplementation(async (name: string) => {
      order.push(name);
      return () => {};
    });
    vi.mocked(api.appStatus).mockImplementation(async () => {
      order.push("app_status");
      return makeStatus();
    });
    vi.mocked(api.pstList).mockImplementation(async () => {
      order.push("pst_list");
      return [];
    });
    const sink = { status: vi.fn(), counts: vi.fn() };
    await boot(new QueryClient(), sink);
    expect(order.slice(0, 6).sort()).toEqual([...ALL_EVENTS].sort());
    expect(order.slice(6)).toEqual(["app_status", "pst_list"]);
  });

  it("does not call pst_list before the database is ready", async () => {
    vi.mocked(api.appStatus).mockResolvedValue(makeStatus({ phase: "needsSetup" }));
    const r = await boot(new QueryClient(), { status: vi.fn(), counts: vi.fn() });
    expect(api.pstList).not.toHaveBeenCalled();
    expect(r.psts).toBeNull();
  });

  it("routes events to the sink and the pstList cache", async () => {
    const qc = new QueryClient();
    const sink = { status: vi.fn(), counts: vi.fn() };
    await boot(qc, sink);
    qc.setQueryData(["pstList"], [makePst({ id: 4, status: "running" })]);
    eventsMock.emit("library:changed", { counts: { messages: 5, psts: 1 } });
    eventsMock.emit("app:status-changed", makeStatus({ phase: "startingDb" }));
    eventsMock.emit("import:status-changed", { pstId: 4, status: "completed", removed: false, source: null });
    expect(sink.counts).toHaveBeenCalledWith({ messages: 5, psts: 1 });
    expect(sink.status).toHaveBeenCalledWith(expect.objectContaining({ phase: "startingDb" }));
    expect(qc.getQueryData<{ status: string }[]>(["pstList"])?.[0]?.status).toBe("completed");
  });
});
