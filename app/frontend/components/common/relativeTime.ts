import { useSyncExternalStore } from "react";

/** "3 min ago" / "in 5 min" in the app language. Units: minutes, hours, days (rounded). */
export function formatRelative(iso: string, lng: string, now: number): string {
  const diff = new Date(iso).getTime() - now;
  const rtf = new Intl.RelativeTimeFormat(lng, { numeric: "auto", style: "short" });
  if (Math.abs(diff) < 60_000) return rtf.format(0, "second");
  const m = Math.round(diff / 60_000);
  if (Math.abs(m) < 60) return rtf.format(m, "minute");
  const h = Math.round(m / 60);
  return Math.abs(h) < 24 ? rtf.format(h, "hour") : rtf.format(Math.round(h / 24), "day");
}

// One shared minute ticker for the whole screen: every useNow() subscriber reads the same clock.
const listeners = new Set<() => void>();
let now = Date.now();
let timer: ReturnType<typeof setInterval> | undefined;

function subscribe(cb: () => void) {
  if (listeners.size === 0) {
    now = Date.now();
    timer = setInterval(() => {
      now = Date.now();
      listeners.forEach((l) => l());
    }, 60_000);
  }
  listeners.add(cb);
  return () => {
    listeners.delete(cb);
    if (listeners.size === 0) clearInterval(timer);
  };
}

export function useNow(): number {
  return useSyncExternalStore(subscribe, () => now);
}
