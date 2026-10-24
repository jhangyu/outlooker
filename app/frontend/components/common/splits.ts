import { useCallback, useEffect, useRef, useState } from "react";

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
 * `scroll` = the group lies on a scrolling axis: no container bound, only `max` applies (needs no host).
 */
export type SplitDef = { size?: number; min: number; max?: number; minOther?: number; group?: string; scroll?: true };
export type SplitDefaults = Record<string, SplitDef>;

const groupOf = (id: string, d: SplitDef) => d.group ?? id;
const isDev = import.meta.env.DEV;

type Axis = "x" | "y";
/** Tracks laid out by the host along the axis: px tokens of the resolved grid template, else flex children (not separators). */
function trackCount(el: HTMLElement, cs: CSSStyleDeclaration, axis: Axis): number {
  if (cs.display.endsWith("grid")) return ((axis === "x" ? cs.gridTemplateColumns : cs.gridTemplateRows).match(/[\d.]+px/g) ?? []).length;
  return [...el.children].filter((c) => c.getAttribute("role") !== "separator").length;
}
/** Content size of the host available to its tracks (padding and the gaps between tracks removed). 0 = not laid out (jsdom). */
function inner(el: HTMLElement, axis: Axis): number {
  const cs = getComputedStyle(el);
  const x = axis === "x";
  const pad = (parseFloat(x ? cs.paddingLeft : cs.paddingTop) || 0) + (parseFloat(x ? cs.paddingRight : cs.paddingBottom) || 0);
  const size = (x ? el.clientWidth : el.clientHeight) - pad;
  if (!(size > 0)) return 0;
  const gap = parseFloat(x ? cs.columnGap : cs.rowGap) || 0;
  return Math.max(0, size - Math.max(0, trackCount(el, cs, axis) - 1) * gap);
}
/**
 * Dev check against N1 (host = the sized pane): a grid host needs at least two tracks on the axis; a flex host must run
 * along the axis and must not hold the separator itself (a pane's own splitter is its direct child).
 */
function assertHost(el: HTMLElement, axis: Axis, group: string) {
  const cs = getComputedStyle(el);
  const ok = cs.display.endsWith("grid")
    ? trackCount(el, cs, axis) >= 2
    : cs.display.endsWith("flex") && cs.flexDirection.startsWith(axis === "x" ? "row" : "column") && ![...el.children].some((c) => c.getAttribute("role") === "separator");
  if (!ok) console.error(`useSplit: host for "${group}" is not the layout container of the group's panes along ${axis}`, el);
}

/**
 * Persisted sizes for the boundaries of one surface. `defaults` must be referentially stable (module constant).
 * `sizes[id]` is the stored size (else default) clamped to the measured host, recomputed on every resize, so a
 * smaller window never hides a pane (the stored value is kept and returns when the window grows).
 * `host(group, axis)` is the callback ref for the grid/flex element laying out the group's panes (one per non-scroll group);
 * `props(id)` feeds <Splitter>; `track`/`rest` build the grid template so CSS reserves the mins the clamp assumes.
 */
export function useSplit(surfaceKey: string, defaults: SplitDefaults) {
  const key = PREFIX + surfaceKey;
  const [stored, setStored] = useState(() => readNumbers(key));
  const [extents, setExtents] = useState<Record<string, number>>({});
  const hosts = useRef(new Map<string, (el: HTMLElement | null) => (() => void) | undefined>());
  const attached = useRef(new Set<string>());

  /** `live` (mid-drag) updates state only; storage is written once the value is final. */
  const set = useCallback(
    (id: string, px: number | undefined, live?: boolean) => {
      if (!live) writeNumber(key, id, px);
      setStored((m) => {
        const next = { ...m };
        if (px === undefined) delete next[id];
        else next[id] = Math.round(px);
        return next;
      });
    },
    [key],
  );

  const host = (group: string, axis: Axis) => {
    let ref = hosts.current.get(group);
    if (!ref) {
      ref = (el) => {
        if (!el) return;
        attached.current.add(group);
        const read = () => {
          const px = inner(el, axis);
          setExtents((m) => (m[group] === px ? m : { ...m, [group]: px }));
        };
        read();
        if (isDev && inner(el, axis) > 0) assertHost(el, axis, group);
        const ro = typeof ResizeObserver === "undefined" ? undefined : new ResizeObserver(read);
        ro?.observe(el);
        return () => {
          ro?.disconnect();
          attached.current.delete(group);
        };
      };
      hosts.current.set(group, ref);
    }
    return ref;
  };

  const sizes: Record<string, number | undefined> = {};
  const limits: Record<string, number | undefined> = {};
  const reserves: Record<string, number> = {};
  const tails: Record<string, number> = {};
  const ids = Object.keys(defaults);
  const used: Record<string, number> = {};
  for (const id of ids) {
    const d = defaults[id];
    const g = groupOf(id, d);
    const members = ids.filter((o) => groupOf(o, defaults[o]) === g);
    const after = members.slice(members.indexOf(id) + 1);
    const tail = defaults[after[after.length - 1] ?? id].minOther ?? DEFAULT_MIN_OTHER;
    tails[g] = tail;
    reserves[id] = after.reduce((n, o) => n + defaults[o].min, 0) + tail;
    const ext = d.scroll ? undefined : extents[g] || undefined;
    const room = ext === undefined ? Infinity : Math.max(d.min, ext - (used[g] ?? 0) - reserves[id]);
    const hard = Math.min(d.max ?? Infinity, room);
    limits[id] = Number.isFinite(hard) ? Math.max(d.min, hard) : undefined;
    const v = stored[id] ?? d.size;
    sizes[id] = v === undefined ? undefined : Math.min(Math.max(v, d.min), limits[id] ?? Infinity);
    used[g] = (used[g] ?? 0) + (sizes[id] ?? d.min);
  }

  // Dev: a non-scroll group whose splitters render without a host silently loses its container bound (End, aria-valuemax).
  const rendered = useRef(new Set<string>());
  rendered.current.clear();
  useEffect(() => {
    if (!isDev) return;
    for (const g of rendered.current) if (!attached.current.has(g)) console.error(`useSplit("${surfaceKey}"): group "${g}" has no host()`);
  });

  /** CSS track for a grid template: `fallback` (kept at least `min`) while the size is unset, else px (CSS-clamped until the host is measured). */
  const track = (id: string, fallback: string): string => {
    const v = sizes[id];
    const d = defaults[id];
    if (v === undefined) return d.scroll ? fallback : `minmax(${d.min}px, ${fallback})`;
    return d.scroll || extents[groupOf(id, d)] ? `${v}px` : `clamp(${d.min}px, ${v}px, calc(100% - ${reserves[id]}px))`;
  };
  /** Trailing track of a group: keeps the room the clamp reserves for it. */
  const rest = (group: string, fallback: string): string => `minmax(${tails[group] ?? DEFAULT_MIN_OTHER}px, ${fallback})`;

  const props = (id: string) => {
    const d = defaults[id];
    if (!d.scroll) rendered.current.add(groupOf(id, d));
    return {
      value: sizes[id],
      min: d.min,
      max: limits[id],
      onChange: (px: number, live?: boolean) => set(id, Math.min(Math.max(px, d.min), limits[id] ?? Infinity), live),
      onReset: () => set(id, undefined),
    };
  };

  return { sizes, track, rest, props, host };
}
