import { describe, expect, it, vi } from "vitest";

const open = vi.fn();
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: (...a: unknown[]) => open(...a) }));

import { pickFolder, pickPstFiles } from "./dialog";

describe("dialog pickers", () => {
  it("normalizes the PST picker result", async () => {
    open.mockResolvedValueOnce(null);
    expect(await pickPstFiles()).toEqual([]);
    open.mockResolvedValueOnce("a");
    expect(await pickPstFiles()).toEqual(["a"]);
    open.mockResolvedValueOnce(["a", "b"]);
    expect(await pickPstFiles()).toEqual(["a", "b"]);
  });

  it("returns the chosen folder or null", async () => {
    open.mockResolvedValueOnce("/x");
    expect(await pickFolder()).toBe("/x");
    open.mockResolvedValueOnce(null);
    expect(await pickFolder()).toBeNull();
  });
});
