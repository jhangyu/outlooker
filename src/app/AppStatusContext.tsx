import { createContext, useContext, useImperativeHandle, useMemo, useState, type ReactNode, type RefObject } from "react";
import type { AppStatus } from "../api/bindings/AppStatus";
import type { LibraryCounts } from "../api/bindings/LibraryCounts";

export type ScreenId = "search" | "advancedSql" | "import" | "settings";

export interface AppStatusApi {
  setStatus(s: AppStatus): void;
  setCounts(c: LibraryCounts): void;
}

interface Ctx {
  status: AppStatus;
  screen: ScreenId;
  onboarding: boolean;
  go(screen: ScreenId): void;
}

const StatusContext = createContext<Ctx | null>(null);

export function AppStatusProvider(props: {
  initial: AppStatus;
  initialScreen?: ScreenId;
  onGo?: (s: ScreenId) => void;
  apiRef?: RefObject<AppStatusApi | null>;
  children: ReactNode;
}) {
  const [status, setStatus] = useState(props.initial);
  const [screen, setScreen] = useState<ScreenId>(props.initialScreen ?? "search");
  const [onboarding, setOnboarding] = useState(props.initial.phase === "needsSetup");
  const { onGo } = props;

  useImperativeHandle(
    props.apiRef,
    () => ({
      setStatus: (next) => {
        if (next.phase === "needsSetup") setOnboarding(true);
        // The core sends zero counts outside Ready (Part 03 §4); keep the last known counts.
        setStatus((prev) => (next.phase === "ready" ? next : { ...next, counts: prev.counts }));
      },
      setCounts: (counts) => setStatus((prev) => ({ ...prev, counts })),
    }),
    [],
  );

  const value = useMemo<Ctx>(
    () => ({
      status,
      screen,
      onboarding,
      go: (s) => {
        setOnboarding(false); // navigating is how onboarding finishes
        setScreen(s);
        onGo?.(s);
      },
    }),
    [status, screen, onboarding, onGo],
  );
  return <StatusContext.Provider value={value}>{props.children}</StatusContext.Provider>;
}

function useCtx(): Ctx {
  const ctx = useContext(StatusContext);
  if (!ctx) throw new Error("AppStatusProvider is missing");
  return ctx;
}

export function useAppStatus(): AppStatus {
  return useCtx().status;
}

export function useNavigation(): { screen: ScreenId; go(screen: ScreenId): void } {
  const { screen, go } = useCtx();
  return { screen, go };
}

/** R1-06 internal: true while the first-run wizard owns the window. */
export function useOnboardingActive(): boolean {
  return useCtx().onboarding;
}
