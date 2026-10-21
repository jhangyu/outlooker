import type { AppStatus } from "../api/bindings/AppStatus";

const INITIAL: AppStatus = {
  phase: "needsSetup",
  dataDir: "",
  language: "zh-TW",
  counts: { messages: 0, psts: 0 },
  error: null,
};

export function useAppStatus(): AppStatus {
  return INITIAL;
}

export type ScreenId = "search" | "advancedSql" | "import" | "settings";

export function useNavigation(): { screen: ScreenId; go(screen: ScreenId): void } {
  return { screen: "search", go: () => {} };
}
