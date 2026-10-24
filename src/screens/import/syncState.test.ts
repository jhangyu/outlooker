import { describe, expect, it } from "vitest";
import { rowState, toDraft, toSchedule } from "./syncState";

describe("sync state derivation", () => {
  it("disabled shows over a resting status but never hides a live job", () => {
    expect(rowState({ status: "completed", paused: true })).toBe("disabled");
    expect(rowState({ status: "locked", paused: true })).toBe("disabled");
    expect(rowState({ status: "running", paused: true })).toBe("running");
    expect(rowState({ status: "locked", paused: false })).toBe("locked");
  });

  it("schedule draft round-trips and rejects unsavable input", () => {
    for (const s of [
      { mode: "off" as const },
      { mode: "interval" as const, minutes: 90 },
      { mode: "interval" as const, minutes: 120 },
      { mode: "daily" as const, time: "07:05", weekdays: [1, 6] },
    ])
      expect(toSchedule(toDraft(s))).toEqual(s);
    expect(toDraft({ mode: "interval", minutes: 120 })).toMatchObject({ n: 2, unit: "hours" });
    expect(toSchedule({ ...toDraft({ mode: "daily", time: "07:05", weekdays: [1] }), days: [] })).toBeNull();
    expect(toSchedule({ ...toDraft({ mode: "interval", minutes: 5 }), n: 0 })).toBeNull();
  });
});
