// @vitest-environment node
import { describe, expect, it } from "vitest";
import type { AppError } from "../../api/bindings/AppError";
import { createI18n } from "../../app/i18n";
import { formatAppError, formatBytes, formatNumber, toAppError } from ".";

const ERRORS: AppError[] = [
  { kind: "dbNotReady" },
  { kind: "notFound", entity: "mail", id: "7" },
  { kind: "invalidInput", field: "dataDir", reason: "notEmpty" },
  { kind: "queryTimeout", timeoutMs: 30000 },
  { kind: "cancelled" },
  { kind: "sqlError", sqlstate: "42601", message: "syntax error at or near \"SELEC\"", position: 1 },
  { kind: "readOnlyViolation" },
  { kind: "busy", reason: "importRunning" },
  { kind: "busy", reason: "maintenanceRunning" },
  { kind: "busy", reason: "dataDirMoving" },
  { kind: "busy", reason: "pstRemoving" },
  { kind: "fileAccess", path: "/x", reason: "notWritable" },
  { kind: "fileAccess", path: "/x", reason: "insufficientSpace" },
  { kind: "notAPst", path: "/x.pst" },
  { kind: "alreadyImported", pstId: 3 },
  { kind: "incompatibleDatabase", detail: "migration failed" },
  { kind: "internal", detail: "boom" },
];

describe("common", () => {
  it("formatAppError maps every AppError kind to a non-empty localized string", () => {
    for (const lng of ["en", "zh-TW"]) {
      const t = createI18n(lng).t;
      for (const err of ERRORS) {
        const text = formatAppError(err, t);
        expect(text.length, `${lng} ${JSON.stringify(err)}`).toBeGreaterThan(0);
        expect(text.startsWith("error."), `${lng} ${err.kind} is untranslated`).toBe(false);
      }
    }
    const en = createI18n("en").t;
    expect(formatAppError(ERRORS[5]!, en)).toBe('syntax error at or near "SELEC"');
    expect(formatAppError({ kind: "queryTimeout", timeoutMs: 30000 }, en)).toContain("30");
  });

  it("toAppError keeps AppError values and wraps anything else", () => {
    expect(toAppError({ kind: "cancelled" })).toEqual({ kind: "cancelled" });
    expect(toAppError(new Error("x"))).toEqual({ kind: "internal", detail: "Error: x" });
  });

  it("display helpers format numbers and bytes, also in cimode", () => {
    expect(formatNumber(10000, "en")).toBe("10,000");
    expect(formatNumber(9999, "zh-TW")).toBe("9,999");
    expect(formatBytes(512, "en")).toBe("512 B");
    expect(formatBytes(1536, "en")).toBe("1.5 KB");
    expect(formatBytes(50_331_648, "cimode")).toBe("48.0 MB");
  });
});
