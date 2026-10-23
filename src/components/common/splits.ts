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
 * One boundary: `size` is the px size of the sized pane (undefined = content/fr, no stored default), `min` its minimum,
 * `max` an optional hard maximum. Boundaries sharing a `group` lie along one container axis, in key order: each is
 * also limited by the container minus the panes before it and the mins of those after it (last member's `minOther`
 * is the room kept for the trailing pane, default 240). Without `group` a boundary is its own group.
 */
export type SplitDef = { size?: number; min: number; max?: number; minOther?: number; group?: string };
export type SplitDefaults = Record<string, SplitDef>;

const groupOf = (id: string, d: SplitDef) => d.group ?? id;

/**
 * Persisted sizes for the boundaries of one surface. `defaults` must be referentially stable (module constant).
 * `sizes[id]` is the stored size (else default) clamped to the measured container, recomputed on every resize, so a
 * smaller window never hides a pane (the stored value is kept and returns when the window grows).
 * `props(id)` feeds <Splitter>, which reports the container size.
 */
export function useSplit(surfaceKey: string, defaults: SplitDefaults) {
  const key = PREFIX + surfaceKey;
  const [stored, setStored] = useState(() => readNumbers(key));
  const [extents, setExtents] = useState<Record<string, number>>({});

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

  const setExtent = useCallback((group: string, px: number) => {
    setExtents((m) => (m[group] === px ? m : { ...m, [group]: px }));
  }, []);

  const sizes: Record<string, number | undefined> = {};
  const limits: Record<string, number | undefined> = {};
  const reserves: Record<string, number> = {};
  const ids = Object.keys(defaults);
  const used: Record<string, number> = {};
  for (const id of ids) {
    const d = defaults[id];
    const g = groupOf(id, d);
    const members = ids.filter((o) => groupOf(o, defaults[o]) === g);
    const after = members.slice(members.indexOf(id) + 1);
    const tail = defaults[after[after.length - 1] ?? id].minOther ?? DEFAULT_MIN_OTHER;
    reserves[id] = after.reduce((n, o) => n + defaults[o].min, 0) + tail;
    const ext = extents[g] || undefined;
    const room = ext === undefined ? Infinity : Math.max(d.min, ext - (used[g] ?? 0) - reserves[id]);
    const hard = Math.min(d.max ?? Infinity, room);
    limits[id] = Number.isFinite(hard) ? Math.max(d.min, hard) : undefined;
    const v = stored[id] ?? d.size;
    sizes[id] = v === undefined ? undefined : Math.min(Math.max(v, d.min), limits[id] ?? Infinity);
    used[g] = (used[g] ?? 0) + (sizes[id] ?? d.min);
  }

  /** CSS track for a grid template: `fallback` while the size is unset, else px (CSS-clamped until the container is measured). */
  const track = (id: string, fallback: string): string => {
    const v = sizes[id];
    if (v === undefined) return fallback;
    const d = defaults[id];
    return extents[groupOf(id, d)] ? `${v}px` : `clamp(${d.min}px, ${v}px, calc(100% - ${reserves[id]}px))`;
  };

  const props = (id: string) => {
    const d = defaults[id];
    return {
      value: sizes[id],
      min: d.min,
      max: limits[id],
      onExtent: (px: number) => setExtent(groupOf(id, d), px),
      onChange: (px: number) => set(id, Math.min(Math.max(px, d.min), limits[id] ?? Infinity)),
      onReset: () => set(id, undefined),
    };
  };

  return { sizes, track, props };
}
