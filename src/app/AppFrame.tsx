import {
  Accordion, AccordionHeader, AccordionItem, AccordionPanel, Badge, Body1, Button, Caption1, MessageBar, MessageBarBody,
  Spinner, Tab, TabList, Title2, Toaster, ToggleButton, makeStyles, mergeClasses, tabClassNames, tokens, useId,
} from "@fluentui/react-components";
import {
  ArrowDownload24Regular, Database24Regular, ErrorCircleRegular, Search24Regular, Settings24Regular,
} from "@fluentui/react-icons";
import { useQuery } from "@tanstack/react-query";
import { Activity, useState, type JSX } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../api/commands";
import { pstListQuery } from "../api/queries";
import type { AppError } from "../api/bindings/AppError";
import type { AppPhase } from "../api/bindings/AppPhase";
import { formatAppError, toAppError, useLanguageSwitch } from "../components/common";
import { AdvancedSqlScreen } from "../screens/advanced-sql";
import { ImportScreen } from "../screens/import";
import { OnboardingScreen } from "../screens/onboarding";
import { SearchScreen } from "../screens/search";
import { SettingsScreen } from "../screens/settings";
import { useMoveSession } from "../screens/settings/moveSession";
import { BrandMark } from "../components/brand/BrandMark";
import { useAppStatus, useNavigation, useOnboardingActive, type ScreenId } from "./AppStatusContext";

const useStyles = makeStyles({
  app: { display: "grid", gridTemplateRows: "48px 1fr", gridTemplateColumns: "68px 1fr", height: "100vh", overflow: "hidden" },
  titleBar: {
    gridColumn: "1 / -1", display: "flex", alignItems: "center", gap: tokens.spacingHorizontalM,
    paddingRight: tokens.spacingHorizontalL, backgroundColor: tokens.colorBrandBackground, color: tokens.colorNeutralForegroundOnBrand,
  },
  brand: { width: "68px", display: "grid", placeItems: "center" },
  appName: { fontSize: tokens.fontSizeBase400, fontWeight: tokens.fontWeightSemibold },
  crumb: { opacity: 0.9 },
  langLabel: { fontSize: tokens.fontSizeBase200, opacity: 0.95 },
  spacer: { flex: 1 },
  seg: {
    display: "inline-flex", border: `${tokens.strokeWidthThin} solid ${tokens.colorNeutralStrokeOnBrand2}`,
    borderRadius: tokens.borderRadiusMedium, overflow: "hidden",
  },
  langButton: {
    color: tokens.colorNeutralForegroundOnBrand, minWidth: "auto", padding: `2px ${tokens.spacingHorizontalM}`,
    fontSize: tokens.fontSizeBase200, fontWeight: tokens.fontWeightRegular, borderRadius: 0, border: "none",
    ":hover": { color: tokens.colorNeutralForegroundOnBrand, backgroundColor: tokens.colorBrandBackgroundHover },
    ":hover:active": { color: tokens.colorNeutralForegroundOnBrand, backgroundColor: tokens.colorBrandBackgroundPressed },
  },
  langOn: {
    backgroundColor: tokens.colorNeutralBackground1, color: tokens.colorBrandForeground2, fontWeight: tokens.fontWeightSemibold,
    ":hover": { backgroundColor: tokens.colorNeutralBackground1, color: tokens.colorBrandForeground2 },
    ":hover:active": { backgroundColor: tokens.colorNeutralBackground1, color: tokens.colorBrandForeground2 },
  },
  rail: {
    backgroundColor: tokens.colorNeutralBackground3, borderRight: `1px solid ${tokens.colorNeutralStroke2}`,
    padding: `${tokens.spacingVerticalS} ${tokens.spacingHorizontalXS}`, display: "flex", flexDirection: "column", gap: tokens.spacingVerticalXS,
  },
  // Fluent Tab is a grid that puts icon and label in one row; stack them like the mockup rail button.
  railTab: {
    gridAutoFlow: "row", justifyItems: "center", justifyContent: "center", rowGap: "2px",
    padding: `${tokens.spacingVerticalSNudge} 0`, height: "auto",
  },
  railTabOn: {
    backgroundColor: tokens.colorNeutralBackground1, boxShadow: tokens.shadow2,
    ":enabled:hover": { backgroundColor: tokens.colorNeutralBackground1 },
    [`& .${tabClassNames.content}`]: { color: tokens.colorBrandForeground1 },
    [`:enabled:hover .${tabClassNames.content}`]: { color: tokens.colorBrandForeground1 },
    [`:enabled:active .${tabClassNames.content}`]: { color: tokens.colorBrandForeground1 },
  },
  railIcon: { gridColumnStart: 1, gridRowStart: 1, fontSize: "24px", width: "24px", height: "24px" },
  railLabel: {
    gridColumnStart: 1, gridRowStart: 2, fontSize: tokens.fontSizeBase100, lineHeight: tokens.lineHeightBase100,
    padding: 0, whiteSpace: "nowrap",
  },
  railGap: { flex: 1 },
  main: { minWidth: 0, minHeight: 0, overflow: "hidden", backgroundColor: tokens.colorNeutralBackground3 },
  center: {
    height: "100vh", display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center",
    gap: tokens.spacingVerticalM, padding: tokens.spacingHorizontalXXL, boxSizing: "border-box", textAlign: "center",
  },
  failedIcon: { fontSize: "48px", color: tokens.colorStatusDangerForeground1 },
  details: { width: "min(720px, 100%)", textAlign: "left" },
  pre: { whiteSpace: "pre-wrap", fontFamily: tokens.fontFamilyMonospace, fontSize: tokens.fontSizeBase200, margin: 0, userSelect: "text" },
});

// Every screen stays mounted so it keeps its state. <Activity> hides inactive ones and pauses their effects.
const SCREENS: Record<ScreenId, () => JSX.Element | null> = {
  search: SearchScreen,
  advancedSql: AdvancedSqlScreen,
  import: ImportScreen,
  settings: SettingsScreen,
};

export function FailedScreen({ error, onRetry }: { error: AppError | null; onRetry: () => Promise<unknown> }) {
  const s = useStyles();
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  const [retryError, setRetryError] = useState<AppError | null>(null);
  const retry = async () => {
    setBusy(true);
    setRetryError(null);
    try {
      await onRetry();
    } catch (e) {
      setRetryError(toAppError(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className={s.center} data-testid="failed-screen">
      <ErrorCircleRegular className={s.failedIcon} aria-hidden />
      <Title2>{t("failed.title")}</Title2>
      <Body1>{t("failed.body")}</Body1>
      {error ? <Body1>{formatAppError(error, t)}</Body1> : null}
      {retryError ? (
        <MessageBar intent="error">
          <MessageBarBody>{formatAppError(retryError, t)}</MessageBarBody>
        </MessageBar>
      ) : null}
      {error ? (
        <Accordion collapsible className={s.details}>
          <AccordionItem value="details">
            <AccordionHeader>{t("failed.details")}</AccordionHeader>
            <AccordionPanel>
              <pre className={s.pre}>{error.kind === "internal" ? error.detail : JSON.stringify(error, null, 2)}</pre>
            </AccordionPanel>
          </AccordionItem>
        </Accordion>
      ) : null}
      <Button appearance="primary" onClick={retry} disabled={busy} data-testid="failed-retry">
        {t("failed.retry")}
      </Button>
    </div>
  );
}

function BlockingScreen({ phase }: { phase: AppPhase }) {
  const s = useStyles();
  const { t } = useTranslation();
  return (
    <div className={s.center} data-testid="blocking-screen">
      <Spinner size="large" label={t(`phase.${phase}`)} />
      <Caption1>{t("phase.hint")}</Caption1>
    </div>
  );
}

function TitleBar() {
  const s = useStyles();
  const { t, i18n } = useTranslation();
  const { screen } = useNavigation();
  const setLanguage = useLanguageSwitch();
  const langLabelId = useId("lang-label");
  const psts = useQuery(pstListQuery);
  const running = psts.data?.find((p) => p.status === "running");
  const total = running?.progress?.itemsTotal ?? null;
  const importing = running
    ? total
      ? t("titlebar.importing", { percent: Math.round(((running.progress?.itemsDone ?? 0) / total) * 100) })
      : t("titlebar.scanning")
    : null;
  return (
    <header className={s.titleBar} data-testid="title-bar">
      <div className={s.brand}>
        <BrandMark size={28} chip />
      </div>
      <span className={s.appName}>{t("app.name")}</span>
      <span className={s.crumb}>· {t(`nav.${screen}`)}</span>
      <span className={s.spacer} />
      {importing ? <Badge appearance="filled" color="informative">{importing}</Badge> : null}
      <span id={langLabelId} className={s.langLabel}>{t("language.label")}</span>
      <div role="group" aria-labelledby={langLabelId} className={s.seg}>
        {(["zh-TW", "en"] as const).map((lng) => (
          <ToggleButton
            key={lng}
            size="small"
            appearance="transparent"
            className={mergeClasses(s.langButton, i18n.language === lng && s.langOn)}
            checked={i18n.language === lng}
            onClick={() => void setLanguage(lng)}
          >
            {t(`language.${lng}`)}
          </ToggleButton>
        ))}
      </div>
    </header>
  );
}

function NavRail() {
  const s = useStyles();
  const { t } = useTranslation();
  const { screen, go } = useNavigation();
  const tab = (id: ScreenId, icon: JSX.Element) => (
    <Tab
      value={id}
      icon={{ children: icon, className: s.railIcon }}
      content={{ className: s.railLabel }}
      className={mergeClasses(s.railTab, screen === id && s.railTabOn)}
      data-testid={`nav-${id}`}
    >
      {t(`nav.${id}`)}
    </Tab>
  );
  return (
    <TabList
      vertical
      selectedValue={screen}
      onTabSelect={(_, d) => go(d.value as ScreenId)}
      aria-label={t("nav.main")}
      className={s.rail}
    >
      {tab("search", <Search24Regular />)}
      {tab("advancedSql", <Database24Regular />)}
      {tab("import", <ArrowDownload24Regular />)}
      <div className={s.railGap} />
      {tab("settings", <Settings24Regular />)}
    </TabList>
  );
}

export function AppFrame() {
  const s = useStyles();
  const [visited, setVisited] = useState<ReadonlySet<ScreenId>>(new Set());
  const status = useAppStatus();
  const onboarding = useOnboardingActive();
  const { screen } = useNavigation();
  // A data-folder move drives the phase through stoppingDb → startingDb → ready (Part 03 §8.2). Its own blocking
  // dialog on the Settings screen shows the progress, so the frame stays mounted until that dialog is closed.
  const moving = useMoveSession() !== null;
  // A screen mounts on its first visit only, so start-up does not pay for screens the user has not opened.
  if (!visited.has(screen)) setVisited(new Set(visited).add(screen));
  let body: JSX.Element;
  if (status.phase === "failed") body = <FailedScreen error={status.error} onRetry={() => api.appRetry()} />;
  else if (onboarding) body = <OnboardingScreen />;
  else if (status.phase !== "ready" && !moving) body = <BlockingScreen phase={status.phase} />;
  else
    body = (
      <div className={s.app}>
        <TitleBar />
        <NavRail />
        <main className={s.main}>
          {(Object.entries(SCREENS) as [ScreenId, () => JSX.Element | null][]).filter(([id]) => visited.has(id) || id === screen).map(([id, Screen]) => (
            <Activity key={id} mode={screen === id ? "visible" : "hidden"}>
              <Screen />
            </Activity>
          ))}
        </main>
      </div>
    );
  return (
    <>
      {body}
      <Toaster toasterId="app" position="bottom-end" />
    </>
  );
}
