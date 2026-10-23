import { FluentProvider, Toaster, webLightTheme } from "@fluentui/react-components";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, type RenderResult } from "@testing-library/react";
import type { ReactElement } from "react";
import { I18nextProvider } from "react-i18next";
import { vi } from "vitest";
import { api } from "../api/commands";
import type { AppStatus } from "../api/bindings/AppStatus";
import type { DbStatus } from "../api/bindings/DbStatus";
import type { PstSource } from "../api/bindings/PstSource";
import { AppStatusProvider, type ScreenId } from "./AppStatusContext";
import { createI18n } from "./i18n";

export function makeStatus(p: Partial<AppStatus> = {}): AppStatus {
  return { phase: "ready", dataDir: "/data", language: "en", counts: { messages: 0, psts: 0 }, error: null, ...p };
}

export function makeDbStatus(p: Partial<DbStatus> = {}): DbStatus {
  return {
    phase: "ready",
    postgresVersion: "18.6",
    dataDir: "/data",
    databaseSizeBytes: 50_331_648,
    counts: { messages: 0, psts: 0 },
    imageCount: 0,
    attachmentCount: 0,
    indexes: [],
    lastVacuumAt: null,
    lastAnalyzeAt: null,
    maintenance: null,
    ...p,
  };
}

export function makePst(p: Partial<PstSource> = {}): PstSource {
  return {
    id: 1,
    path: "/mail/archive.pst",
    displayName: "archive",
    fileSizeBytes: 1_048_576,
    status: "completed",
    progress: null,
    messageCount: 120,
    newMessageCount: 100,
    errorCount: 0,
    queuedAt: "2026-10-01T08:00:00Z",
    finishedAt: "2026-10-01T08:10:00Z",
    syncEnabled: true,
    lastCheckedAt: "2026-10-01T08:10:00Z",
    lastSyncedAt: "2026-10-01T08:10:00Z",
    failureReason: null,
    ...p,
  };
}

type Handler = (payload: unknown) => void;
const handlers = new Map<string, Set<Handler>>();

/** Replacement module for src/api/events in tests: vi.mock("<rel>/api/events", async () => (await import("<rel>/app/testing")).eventsMock). */
const register = async (name: string, handler: Handler) => {
  const set = handlers.get(name) ?? new Set<Handler>();
  set.add(handler);
  handlers.set(name, set);
  return () => {
    set.delete(handler);
  };
};

export const eventsMock = {
  onEvent: vi.fn(register),
  emit(name: string, payload: unknown) {
    handlers.get(name)?.forEach((h) => h(payload));
  },
  reset() {
    handlers.clear();
    eventsMock.onEvent.mockReset();
    eventsMock.onEvent.mockImplementation(register);
  },
};

/** Call after vi.mock("<rel>/api/commands"): every command resolves with an empty or default value. */
export function mockApiDefaults(): void {
  const m = vi.mocked(api);
  m.appStatus.mockResolvedValue(makeStatus());
  m.appInitialize.mockResolvedValue(null);
  m.appRetry.mockResolvedValue(null);
  m.searchMail.mockResolvedValue({ rows: [], nextCursor: null, shortQuery: false, elapsedMs: 1 });
  m.searchCount.mockResolvedValue({ count: 0, capped: false });
  m.filterOptions.mockResolvedValue({ psts: [], folders: [] });
  m.mailGet.mockRejectedValue({ kind: "notFound", entity: "mail", id: "0" });
  m.savedQueryList.mockResolvedValue([]);
  m.savedQueryCreate.mockResolvedValue({ id: 1, name: "q", sql: "SELECT 1", updatedAt: "2026-10-01T00:00:00Z" });
  m.savedQueryUpdate.mockResolvedValue({ id: 1, name: "q", sql: "SELECT 1", updatedAt: "2026-10-01T00:00:00Z" });
  m.savedQueryDelete.mockResolvedValue(null);
  m.sqlExecute.mockResolvedValue({ mode: "command", tag: "SELECT 0", elapsedMs: 1 });
  m.sqlCancel.mockResolvedValue(null);
  m.pstList.mockResolvedValue([]);
  m.pstRegister.mockResolvedValue([]);
  m.pstSetEnabled.mockRejectedValue({ kind: "notFound", entity: "pst", id: "0" });
  m.syncNow.mockResolvedValue(null);
  m.syncStatus.mockResolvedValue({ schedule: { mode: "interval", minutes: 60 }, lastRunAt: null, nextRunAt: null });
  m.syncScheduleSet.mockResolvedValue({ schedule: { mode: "interval", minutes: 60 }, lastRunAt: null, nextRunAt: null });
  m.importCancel.mockResolvedValue(null);
  m.pstRemove.mockResolvedValue(null);
  m.importErrors.mockResolvedValue({ rows: [], nextCursor: null });
  m.settingsGet.mockResolvedValue({ language: "en", webpQuality: 80, dataDir: "/data" });
  m.settingsUpdate.mockResolvedValue({ language: "en", webpQuality: 80, dataDir: "/data" });
  m.dataDirMove.mockResolvedValue(null);
  m.maintenanceRun.mockResolvedValue(null);
  m.dbStatus.mockResolvedValue(makeDbStatus());
}

export function renderWithProviders(
  ui: ReactElement,
  opts: { status?: Partial<AppStatus>; screen?: ScreenId } = {},
): RenderResult {
  const instance = createI18n("en");
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  return render(
    <FluentProvider theme={webLightTheme}>
      <I18nextProvider i18n={instance}>
        <QueryClientProvider client={queryClient}>
          <AppStatusProvider initial={makeStatus(opts.status)} initialScreen={opts.screen}>
            {ui}
            <Toaster toasterId="app" />
          </AppStatusProvider>
        </QueryClientProvider>
      </I18nextProvider>
    </FluentProvider>,
  );
}
