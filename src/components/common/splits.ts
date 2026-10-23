import { useCallback, useState } from "react";

/** Shared localStorage helper for `{ id: number }` maps (column widths and pane splits). */
export function readNumbers(key: string): Record<string, number> {
  const out: Record<string, number> = {};
  try {
    const raw: unknown = JSON.parse(localStorage.getItem(key) ?? "null");
    if (raw && typeof raw === "object" && !Array.isArray(raw)) {
      for (const [k, v] of Object.entries(raw)) if (typeof v === "number" && Number.isFinite(v)) out[k] = v;
    }
  } catch {
    // Malformed JSON or unavailable storage: fall back to defaults.
  }
  return out;
}

/** Write one id (or delete it when `value` is undefined), keeping every other stored id. */
export function writeNumber(key: string, id: string, value: number | undefined): void {
  // ponytail: synchronous write per change (<1 KB), never loses the last value. Unknown ids are never pruned.
  try {
    const map = readNumbers(key);
    if (value === undefined) delete map[id];
    else map[id] = Math.round(value);
    localStorage.setItem(key, JSON.stringify(map));
  } catch {
    // Storage unavailable: values stay session-only.
  }
}

const PREFIX = "outlooker.splits.";
const DEFAULT_MIN_OTHER = 240;

/**
 * One boundary: `size` is the px size of the pane before the border (undefined = content/fr, no stored default),
 * `min` its minimum, `max` an optional hard maximum (otherwise container size - `minOther` at drag time).
 */
export type SplitDef = { size?: number; min: number; max?: number; minOther?: number };
export type SplitDefaults = Record<string, SplitDef>;

const clamp = (v: number, d: SplitDef) => Math.min(Math.max(v, d.min), d.max ?? Infinity);

/**
 * Persisted sizes for the boundaries of one surface. `defaults` must be referentially stable (module constant).
 * `sizes[id]` is the clamped stored size, else the default (possibly undefined). `props(id)` feeds <Splitter>.
 */
export function useSplit(surfaceKey: string, defaults: SplitDefaults) {
  const key = PREFIX + surfaceKey;
  const [stored, setStored] = useState(() => readNumbers(key));

  const set = useCallback(
    (id: string, px: number | undefined) => {
      writeNumber(key, id, px);
      setStored((m) => {
        const next = { ...m };
        if (px === undefined) delete next[id];
        else next[id] = Math.round(px);
        return next;
      });
    },
    [key],
  );

  const size = (id: string): number | undefined => {
    const d = defaults[id];
    const v = stored[id] ?? d.size;
    return v === undefined ? undefined : clamp(v, d);
  };

  const sizes = Object.fromEntries(Object.keys(defaults).map((id) => [id, size(id)])) as Record<string, number | undefined>;

  /**
   * CSS track for a grid template: `fallback` while the size is unset, else px kept inside the container so a
   * smaller window never hides the other pane. Boundaries with a hard `max` are plain px.
   */
  const track = (id: string, fallback: string): string => {
    const v = sizes[id];
    const d = defaults[id];
    if (v === undefined) return fallback;
    return d.max !== undefined ? `${v}px` : `clamp(${d.min}px, ${v}px, calc(100% - ${d.minOther ?? DEFAULT_MIN_OTHER}px))`;
  };

  const props = (id: string) => {
    const d = defaults[id];
    return {
      value: sizes[id],
      min: d.min,
      max: d.max,
      minOther: d.minOther ?? DEFAULT_MIN_OTHER,
      onChange: (px: number) => set(id, clamp(px, d)),
      onReset: () => set(id, undefined),
    };
  };

  return { sizes, track, props };
}
