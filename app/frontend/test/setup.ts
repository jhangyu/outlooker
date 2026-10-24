import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

afterEach(() => {
  cleanup();
  globalThis.localStorage?.clear(); // undefined in the node environment
});

// jsdom has no layout. CodeMirror measures text through Range rects; empty rects make it fall back to defaults.
const emptyRect = { x: 0, y: 0, width: 0, height: 0, top: 0, left: 0, right: 0, bottom: 0, toJSON: () => ({}) } as DOMRect;
if (typeof Range !== "undefined" && !Range.prototype.getClientRects) Range.prototype.getClientRects = () => [] as unknown as DOMRectList;
if (typeof Range !== "undefined" && !Range.prototype.getBoundingClientRect) Range.prototype.getBoundingClientRect = () => emptyRect;

// jsdom has no ResizeObserver; Fluent MessageBar needs one.
if (typeof globalThis.ResizeObserver === "undefined") {
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
}
