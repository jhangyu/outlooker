import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useColumnWidths, type ColumnDefaults } from "./columnWidths";

const DEFAULTS: ColumnDefaults = { a: { width: 100 }, b: { width: 120 } };
const KEY = "outlooker.columnWidths.t";

describe("useColumnWidths", () => {
  it("writes a resized width to localStorage", () => {
    const { result } = renderHook(() => useColumnWidths("t", DEFAULTS));
    act(() => result.current.onColumnResize(undefined, { columnId: "a", width: 300.4 }));
    expect(JSON.parse(localStorage.getItem(KEY)!)).toEqual({ a: 300 });
  });

  it("restores a stored width on remount and keeps unknown ids", () => {
    const first = renderHook(() => useColumnWidths("t", DEFAULTS));
    act(() => first.result.current.onColumnResize(undefined, { columnId: "a", width: 300 }));
    first.unmount();
    const second = renderHook(() => useColumnWidths("t", DEFAULTS));
    expect(second.result.current.columnSizingOptions.a.defaultWidth).toBe(300);
    expect(second.result.current.columnSizingOptions.b.defaultWidth).toBe(120);
    expect(second.result.current.columnSizingOptions.b.minWidth).toBe(48);
    localStorage.setItem(KEY, JSON.stringify({ a: 300, gone: 77 }));
    act(() => second.result.current.onColumnResize(undefined, { columnId: "b", width: 150 }));
    expect(JSON.parse(localStorage.getItem(KEY)!)).toEqual({ a: 300, gone: 77, b: 150 });
  });

  it("falls back to defaults on malformed storage", () => {
    for (const bad of ["{", "[1]", '{"a":"x","b":null}']) {
      localStorage.setItem(KEY, bad);
      const { result } = renderHook(() => useColumnWidths("t", DEFAULTS));
      expect(result.current.columnSizingOptions.a.defaultWidth).toBe(100);
      expect(result.current.columnSizingOptions.b.defaultWidth).toBe(120);
    }
  });
});
