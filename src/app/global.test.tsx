import { screen, waitFor } from "@testing-library/react";
import type { ReactElement } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api/commands");
vi.mock("../api/dialog");
vi.mock("../api/events", async () => (await import("./testing")).eventsMock);

import { api } from "../api/commands";
import type { AppStatus } from "../api/bindings/AppStatus";
import { AdvancedSqlScreen } from "../screens/advanced-sql";
import { ImportScreen } from "../screens/import";
import { OnboardingScreen } from "../screens/onboarding";
import { SearchScreen } from "../screens/search";
import { SettingsScreen } from "../screens/settings";
import { FailedScreen } from "./AppFrame";
import { NAMESPACES, RESOURCES } from "./i18n";
import { eventsMock, makeDbStatus, makePst, mockApiDefaults, renderWithProviders } from "./testing";

function flatten(obj: Record<string, unknown>, prefix = ""): string[] {
  return Object.entries(obj).flatMap(([k, v]) =>
    v !== null && typeof v === "object" ? flatten(v as Record<string, unknown>, `${prefix}${k}.`) : [`${prefix}${k}`],
  );
}

// Vite globs instead of node:fs: tsconfig `types` does not include @types/node.
const SOURCES = import.meta.glob<string>("/src/**/*.{ts,tsx,css,json,html}", { query: "?raw", import: "default", eager: true });
const LOCALE_FILES = Object.keys(import.meta.glob("../locales/*/*.json"));
const localeFiles = (lng: string) =>
  LOCALE_FILES.filter((p) => p.startsWith(`../locales/${lng}/`)).map((p) => p.split("/").pop()!).sort();

const KEY = /^[A-Za-z0-9_.:-]+$/;
const NUMERIC = /^[\d\s.,:%+\-—·/]*(?:\s?(?:B|KB|MB|GB|TB)(?:\/s)?)?$/;
const SCREENS: [string, ReactElement, Partial<AppStatus>][] = [
  ["search", <SearchScreen />, {}],
  ["advancedSql", <AdvancedSqlScreen />, {}],
  ["import", <ImportScreen />, {}],
  ["settings", <SettingsScreen />, {}],
  ["onboarding", <OnboardingScreen />, { phase: "needsSetup" }],
  ["failed", <FailedScreen error={{ kind: "dbNotReady" }} onRetry={async () => null} />, { phase: "failed" }],
];

async function settle(r: ReturnType<typeof renderWithProviders>) {
  await waitFor(() => expect(r.queryClient.isFetching()).toBe(0));
}

describe("global UI rules", () => {
  beforeEach(() => {
    eventsMock.reset();
    mockApiDefaults();
  });

  it("locale files: zh-TW and en have equal key sets per namespace", () => {
    const expected = NAMESPACES.map((n) => `${n}.json`).sort();
    expect(localeFiles("zh-TW")).toEqual(expected);
    expect(localeFiles("en")).toEqual(expected);
    for (const ns of NAMESPACES) {
      expect(flatten(RESOURCES["zh-TW"][ns]).sort(), ns).toEqual(flatten(RESOURCES.en[ns]).sort());
    }
  });

  it("screens rendered in cimode show only keys", async () => {
    for (const [name, el, status] of SCREENS) {
      const r = renderWithProviders(el, { lng: "cimode", status });
      await settle(r);
      const bad: string[] = [];
      const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
      for (let n = walker.nextNode(); n; n = walker.nextNode()) {
        const parent = n.parentElement?.tagName;
        if (parent === "STYLE" || parent === "SCRIPT") continue;
        // CodeMirror renders user SQL and its own fold-gutter glyphs; neither is a UI string.
        if (n.parentElement?.closest(".cm-editor")) continue;
        const text = (n.textContent ?? "").trim();
        if (text && !KEY.test(text) && !NUMERIC.test(text)) bad.push(text);
      }
      expect(bad, name).toEqual([]);
      r.unmount();
    }
  });

  it("each screen renders its empty state", async () => {
    const imp = renderWithProviders(<ImportScreen />);
    expect(await screen.findByText("No PST imported yet")).toBeTruthy();
    imp.unmount();
    const set = renderWithProviders(<SettingsScreen />, { status: { counts: { messages: 0, psts: 0 } } });
    expect(await screen.findByText("Counts fill in after the first import.")).toBeTruthy();
    set.unmount();
    renderWithProviders(<OnboardingScreen />, { status: { phase: "needsSetup" } });
    expect(await screen.findByText("Pick your interface language")).toBeTruthy();
  });

  it("no dark theme tokens in src", () => {
    const banned = ["web" + "DarkTheme", "teams" + "DarkTheme", "prefers-color-" + "scheme"];
    expect(Object.keys(SOURCES).length).toBeGreaterThan(20);
    const hits = Object.entries(SOURCES)
      .filter(([, text]) => banned.some((b) => text.includes(b)))
      .map(([f]) => f);
    expect(hits).toEqual([]);
  });

  it("no out-of-scope controls", async () => {
    const bannedKey = /^(pause|pauseAll|workers?|reencode|reEncode|format|highlight)/i;
    for (const lng of ["zh-TW", "en"] as const)
      for (const ns of NAMESPACES) {
        const keys = flatten(RESOURCES[lng][ns]).filter((k) => k.split(".").some((seg) => bannedKey.test(seg)));
        expect(keys, `${lng}/${ns}`).toEqual([]);
      }
    vi.mocked(api.pstList).mockResolvedValue([
      makePst({ id: 1, status: "running", displayName: "a" }),
      makePst({ id: 2, status: "cancelled", displayName: "b" }),
      makePst({ id: 3, status: "failed", displayName: "c" }),
      makePst({ id: 4, status: "completed", displayName: "d" }),
    ]);
    vi.mocked(api.dbStatus).mockResolvedValue(
      makeDbStatus({ indexes: [{ name: "messages_body_trgm", table: "messages", sizeBytes: 1024, valid: true }] }),
    );
    const bannedLabel = /\b(pause|pause all|workers?|re-?encode|format|highlight)\b/i;
    for (const el of [<ImportScreen />, <SettingsScreen />]) {
      const r = renderWithProviders(el);
      await settle(r);
      const offenders = [...document.body.querySelectorAll("*")].filter((node) => {
        const own = [...node.childNodes].filter((c) => c.nodeType === Node.TEXT_NODE).map((c) => c.textContent ?? "").join(" ");
        return bannedLabel.test(own) || bannedLabel.test(node.getAttribute("aria-label") ?? "");
      });
      expect(offenders.map((o) => o.outerHTML.slice(0, 80))).toEqual([]);
      r.unmount();
    }
  });
});
