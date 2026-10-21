import { describe, expect, it, vi } from "vitest";
import commandNames from "./bindings/commands.json";
import eventNames from "./bindings/events.json";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { api } from "./commands";
import { EVENTS } from "./events";

const toSnake = (s: string) => s.replace(/[A-Z]/g, (c) => `_${c.toLowerCase()}`);

describe("typed API client", () => {
  it("wraps exactly the registered commands", () => {
    expect(Object.keys(api).map(toSnake).sort()).toEqual([...commandNames].sort());
  });

  it("names exactly the emitted events", () => {
    expect(Object.values(EVENTS).sort()).toEqual([...eventNames].sort());
  });

  it("passes camelCase arguments and returns the payload", async () => {
    invoke.mockResolvedValueOnce(null);
    await api.importCancel(7);
    expect(invoke).toHaveBeenCalledWith("import_cancel", { pstId: 7 });
  });

  it("re-throws AppError rejections unchanged and wraps others", async () => {
    invoke.mockRejectedValueOnce({ kind: "dbNotReady" });
    await expect(api.appStatus()).rejects.toEqual({ kind: "dbNotReady" });
    invoke.mockRejectedValueOnce("boom");
    await expect(api.appStatus()).rejects.toEqual({ kind: "internal", detail: "boom" });
  });
});
