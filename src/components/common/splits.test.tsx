import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useSplit, type SplitDefaults } from "./splits";

const DEFS: SplitDefaults = { nav: { size: 220, min: 160, max: 400 }, row: { min: 80, max: 600 } };
const KEY = "outlooker.splits.t";

describe("useSplit", () => {
  it("clamps malformed and out-of-range stored sizes", () => {
    for (const [stored, nav, row] of [
      ["{", 220, undefined],
      ['{"nav":"x","row":null}', 220, undefined],
      ['{"nav":50,"row":9999}', 160, 600],
      ['{"nav":9999,"row":1}', 400, 80],
    ] as const) {
      localStorage.setItem(KEY, stored);
      const { result } = renderHook(() => useSplit("t", DEFS));
      expect(result.current.sizes).toEqual({ nav, row });
    }
  });

  it("round-trips through storage and reset restores the default", () => {
    localStorage.removeItem(KEY);
    const first = renderHook(() => useSplit("t", DEFS));
    act(() => first.result.current.props("nav").onChange(301.4));
    act(() => first.result.current.props("row").onChange(5));
    expect(JSON.parse(localStorage.getItem(KEY)!)).toEqual({ nav: 301, row: 80 });
    first.unmount();
    const second = renderHook(() => useSplit("t", DEFS));
    expect(second.result.current.sizes).toEqual({ nav: 301, row: 80 });
    act(() => second.result.current.props("nav").onReset());
    expect(second.result.current.sizes.nav).toBe(220);
    expect(JSON.parse(localStorage.getItem(KEY)!)).toEqual({ row: 80 });
  });

  it("keeps every other pane at its min when the container is smaller than the stored sizes", () => {
    const defs: SplitDefaults = { a: { min: 100, group: "g" }, b: { min: 150, group: "g", minOther: 200 } };
    localStorage.setItem(KEY, '{"a":900,"b":900}');
    const { result } = renderHook(() => useSplit("t", defs));
    act(() => result.current.props("a").onExtent(600));
    const { a, b } = result.current.sizes as { a: number; b: number };
    expect(a).toBe(600 - 150 - 200);
    expect(a + b + 200).toBeLessThanOrEqual(600);
    expect(b).toBeGreaterThanOrEqual(150);
    expect(result.current.props("a").max).toBe(250);
    act(() => result.current.props("a").onExtent(2000));
    expect(result.current.sizes.a).toBe(900);
  });
});
