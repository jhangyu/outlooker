// @vitest-environment node
import { describe, expect, it } from "vitest";
import { NAMESPACES, RESOURCES } from "./i18n";

function flatten(obj: Record<string, unknown>, prefix = ""): string[] {
  return Object.entries(obj).flatMap(([k, v]) =>
    v !== null && typeof v === "object" ? flatten(v as Record<string, unknown>, `${prefix}${k}.`) : [`${prefix}${k}`],
  );
}

const LOCALE_FILES = Object.keys(import.meta.glob("../locales/*/*.json"));
const localeFiles = (lng: string) =>
  LOCALE_FILES.filter((p) => p.startsWith(`../locales/${lng}/`)).map((p) => p.split("/").pop()!).sort();

describe("global UI rules", () => {
  it("locale files: zh-TW and en have equal key sets per namespace", () => {
    const expected = NAMESPACES.map((n) => `${n}.json`).sort();
    expect(localeFiles("zh-TW")).toEqual(expected);
    expect(localeFiles("en")).toEqual(expected);
    for (const ns of NAMESPACES) {
      expect(flatten(RESOURCES["zh-TW"][ns]).sort(), ns).toEqual(flatten(RESOURCES.en[ns]).sort());
    }
  });
});
