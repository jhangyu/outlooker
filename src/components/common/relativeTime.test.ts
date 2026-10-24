// @vitest-environment node
import { describe, expect, it } from "vitest";
import { formatRelative } from "./relativeTime";

const NOW = Date.parse("2026-01-01T12:00:00Z");
const at = (ms: number) => new Date(NOW + ms).toISOString();
const MIN = 60_000;

describe("formatRelative", () => {
  it("picks minute/hour/day units at the boundaries, past and future", () => {
    expect(formatRelative(at(-3 * MIN), "en", NOW)).toMatch(/^3 min\.? ago$/);
    expect(formatRelative(at(5 * MIN), "en", NOW)).toMatch(/^in 5 min/);
    expect(formatRelative(at(59 * MIN), "en", NOW)).toMatch(/59 min/);
    expect(formatRelative(at(60 * MIN), "en", NOW)).toMatch(/1 hr/);
    expect(formatRelative(at(-23 * 60 * MIN), "en", NOW)).toMatch(/23 hr/);
    expect(formatRelative(at(-48 * 60 * MIN), "en", NOW)).toMatch(/2 days ago/);
  });
  it("follows the app language", () => {
    expect(formatRelative(at(-3 * MIN), "zh-TW", NOW)).toBe("3 分鐘前");
    expect(formatRelative(at(5 * MIN), "zh-TW", NOW)).toBe("5 分鐘後");
  });
});
