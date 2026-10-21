import { FluentProvider, Spinner, webLightTheme } from "@fluentui/react-components";
import { QueryClient, QueryClientProvider, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { I18nextProvider, useTranslation } from "react-i18next";
import type { AppError } from "../api/bindings/AppError";
import type { AppStatus } from "../api/bindings/AppStatus";
import type { LibraryCounts } from "../api/bindings/LibraryCounts";
import { toAppError } from "../components/common";
import { AppStatusProvider, type AppStatusApi } from "./AppStatusContext";
import { AppFrame, FailedScreen } from "./AppFrame";
import { boot, type BootSink } from "./boot";
import { createI18n } from "./i18n";
import "./app.css";

const i18n = createI18n("en");

type BootState = { kind: "loading" } | { kind: "ready"; status: AppStatus } | { kind: "error"; error: AppError };

function Boot() {
  const qc = useQueryClient();
  const { t, i18n: inst } = useTranslation();
  const apiRef = useRef<AppStatusApi | null>(null);
  const pending = useRef<{ status?: AppStatus; counts?: LibraryCounts }>({});
  const [state, setState] = useState<BootState>({ kind: "loading" });
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let cancelled = false;
    let unlisten = () => {};
    // Events can arrive before AppStatusProvider mounts: buffer them, newest wins.
    const sink: BootSink = {
      status: (s) => (apiRef.current ? apiRef.current.setStatus(s) : (pending.current.status = s)),
      counts: (c) => (apiRef.current ? apiRef.current.setCounts(c) : (pending.current.counts = c)),
    };
    boot(qc, sink).then(
      async (r) => {
        unlisten = r.unlisten;
        if (cancelled) return r.unlisten();
        let status = pending.current.status ?? r.status;
        if (pending.current.counts) status = { ...status, counts: pending.current.counts };
        if (r.psts) qc.setQueryData(["pstList"], r.psts);
        await inst.changeLanguage(status.language);
        setState({ kind: "ready", status });
      },
      (e: unknown) => {
        if (!cancelled) setState({ kind: "error", error: toAppError(e) });
      },
    );
    return () => {
      cancelled = true;
      unlisten();
    };
  }, [attempt, qc, inst]);

  if (state.kind === "loading") return <Spinner size="large" label={t("app.loading")} style={{ height: "100vh" }} />;
  if (state.kind === "error")
    return (
      <FailedScreen
        error={state.error}
        onRetry={async () => {
          setState({ kind: "loading" });
          setAttempt((a) => a + 1);
        }}
      />
    );
  return (
    <AppStatusProvider initial={state.status} apiRef={apiRef}>
      <AppFrame />
    </AppStatusProvider>
  );
}

export function App() {
  const [queryClient] = useState(
    () => new QueryClient({ defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } } }),
  );
  return (
    <FluentProvider theme={webLightTheme} style={{ height: "100%" }}>
      <I18nextProvider i18n={i18n}>
        <QueryClientProvider client={queryClient}>
          <Boot />
        </QueryClientProvider>
      </I18nextProvider>
    </FluentProvider>
  );
}
