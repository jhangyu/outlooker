import { afterEach, describe, expect, it, vi } from "vitest";
import { formatDay, formatMonth } from "./display";

describe("display formatters", () => {
  afterEach(() => vi.restoreAllMocks());

  it("reuses one Intl.DateTimeFormat per (locale, options)", () => {
    const spy = vi.spyOn(Intl, "DateTimeFormat");
    formatDay("2026-10-01T08:00:00Z", "fr");
    formatDay("2026-10-02T08:00:00Z", "fr");
    expect(spy).toHaveBeenCalledTimes(1);
    expect(formatMonth("2026-10-01T08:00:00Z", "en")).toContain("2026");
  });
});
