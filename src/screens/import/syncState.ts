import type { PstSource } from "../../api/bindings/PstSource";
import type { SyncSchedule } from "../../api/bindings/SyncSchedule";

/** What the row shows. `paused` sits on top of every status except `removing` (a paused path may still be stopping its current item). */
export type RowState = "completed" | "queued" | "running" | "locked" | "failed" | "removing" | "paused";

export function rowState(p: Pick<PstSource, "status" | "paused">): RowState {
  return p.paused && p.status !== "removing" ? "paused" : p.status;
}

/** Editable form of a schedule. `n`/`unit` only matter for interval, `time`/`days` only for daily. */
export interface ScheduleDraft {
  mode: SyncSchedule["mode"];
  n: number;
  unit: "minutes" | "hours";
  time: string;
  days: number[];
}

export function toDraft(s: SyncSchedule): ScheduleDraft {
  const base: ScheduleDraft = { mode: s.mode, n: 1, unit: "hours", time: "08:30", days: [1, 2, 3, 4, 5] };
  if (s.mode === "interval") {
    return s.minutes % 60 === 0 ? { ...base, n: s.minutes / 60, unit: "hours" } : { ...base, n: s.minutes, unit: "minutes" };
  }
  return s.mode === "daily" ? { ...base, time: s.time, days: s.weekdays } : base;
}

/** null = the draft is not savable (empty weekdays, no time, interval below 1). */
export function toSchedule(d: ScheduleDraft): SyncSchedule | null {
  if (d.mode === "off") return { mode: "off" };
  if (d.mode === "interval") {
    return Number.isInteger(d.n) && d.n >= 1 ? { mode: "interval", minutes: d.unit === "hours" ? d.n * 60 : d.n } : null;
  }
  return d.days.length > 0 && /^\d{2}:\d{2}$/.test(d.time) ? { mode: "daily", time: d.time, weekdays: [...d.days].sort() } : null;
}
