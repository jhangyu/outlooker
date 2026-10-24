import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
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
    act(() => first.result.current.props("nav").onChange(250, true)); // mid-drag: state only, no storage write
    expect(first.result.current.sizes.nav).toBe(250);
    expect(localStorage.getItem(KEY)).toBeNull();
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

  /** A host div laid out by the stubbed computed style: 3 px grid tracks with 16 px column gaps. */
  const gridHost = (width: number) => {
    const el = document.createElement("div");
    Object.defineProperty(el, "clientWidth", { configurable: true, value: width });
    const cs = { display: "grid", gridTemplateColumns: "100px 100px 100px", columnGap: "16px", paddingLeft: "0px", paddingRight: "0px" };
    vi.spyOn(window, "getComputedStyle").mockReturnValue(cs as unknown as CSSStyleDeclaration);
    return el;
  };

  it("keeps every other pane at its min when the host (minus its gaps) is smaller than the stored sizes", () => {
    const defs: SplitDefaults = { a: { min: 100, group: "g" }, b: { min: 150, group: "g", minOther: 200 } };
    localStorage.setItem(KEY, '{"a":900,"b":900}');
    const { result } = renderHook(() => useSplit("t", defs));
    const el = gridHost(632); // 632 − 2 × 16 gaps = 600
    let cleanup: (() => void) | undefined;
    act(() => void (cleanup = result.current.host("g", "x")(el)));
    const { a, b } = result.current.sizes as { a: number; b: number };
    expect(a).toBe(600 - 150 - 200);
    expect(a + b + 200).toBeLessThanOrEqual(600);
    expect(b).toBeGreaterThanOrEqual(150);
    expect(result.current.props("a").max).toBe(250);
    Object.defineProperty(el, "clientWidth", { value: 2032 });
    act(() => {
      cleanup?.();
      result.current.host("g", "x")(el);
    });
    expect(result.current.sizes.a).toBe(900);
    vi.restoreAllMocks();
  });

  it("scroll group ignores the host, unset bounded track keeps its min", () => {
    const defs: SplitDefaults = { r: { min: 80, max: 1200, scroll: true }, c: { min: 160, group: "g" } };
    localStorage.setItem(KEY, '{"r":900}');
    const { result } = renderHook(() => useSplit("t", defs));
    expect(result.current.track("r", "auto")).toBe("900px"); // no calc(100% − …) fallback bounded by the visible height
    const el = gridHost(332);
    act(() => {
      result.current.host("g", "x")(el);
      result.current.host("r", "x")(el); // even a measured 300 px host does not bound a scroll group
    });
    expect(result.current.sizes.r).toBe(900);
    expect(result.current.props("r").max).toBe(1200);
    expect(result.current.track("r", "auto")).toBe("900px");
    expect(result.current.track("c", "1fr")).toBe("minmax(160px, 1fr)");
    expect(result.current.rest("g", "1fr")).toBe("minmax(240px, 1fr)");
    vi.restoreAllMocks();
  });
});
