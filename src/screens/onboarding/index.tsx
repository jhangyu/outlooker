import {
  Body1, Button, Caption1, Card, MessageBar, MessageBarBody, ProgressBar, Subtitle1, Title2, Title3,
  makeStyles, mergeClasses, tokens,
} from "@fluentui/react-components";
import {
  CheckmarkCircleRegular, CircleRegular, CopyRegular, ImageRegular, LockClosedRegular, SearchRegular,
} from "@fluentui/react-icons";
import { useState, type JSX } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../../api/commands";
import { pickFolder, pickPstFiles } from "../../api/dialog";
import type { AppError } from "../../api/bindings/AppError";
import type { Language } from "../../api/bindings/Language";
import { useAppStatus, useNavigation } from "../../app/AppStatusContext";
import { formatAppError, toAppError, useAppToast, useLanguageSwitch } from "../../components/common";

type Step = 0 | 1 | 2 | 3;
const STEP_KEYS = ["language", "location", "database", "import"] as const;

const useStyles = makeStyles({
  root: { display: "grid", gridTemplateColumns: "340px 1fr 440px", height: "100vh", backgroundColor: tokens.colorNeutralBackground1 },
  side: {
    backgroundColor: tokens.colorNeutralBackground2, padding: tokens.spacingHorizontalXXL, display: "flex",
    flexDirection: "column", gap: tokens.spacingVerticalL, overflow: "auto",
  },
  left: { borderRight: `1px solid ${tokens.colorNeutralStroke2}` },
  right: { borderLeft: `1px solid ${tokens.colorNeutralStroke2}` },
  steps: { listStyle: "none", margin: 0, padding: 0, display: "flex", flexDirection: "column", gap: tokens.spacingVerticalS },
  stepItem: { display: "grid", gridTemplateColumns: "28px 1fr", gap: tokens.spacingHorizontalM, alignItems: "center", color: tokens.colorNeutralForeground3 },
  stepCurrent: { color: tokens.colorNeutralForeground1, fontWeight: tokens.fontWeightSemibold },
  num: {
    width: "28px", height: "28px", borderRadius: tokens.borderRadiusCircular, display: "grid", placeItems: "center",
    border: `1px solid ${tokens.colorNeutralStroke1}`, fontSize: tokens.fontSizeBase200,
  },
  numCurrent: { backgroundColor: tokens.colorBrandBackground, color: tokens.colorNeutralForegroundOnBrand, border: "none" },
  body: { display: "flex", flexDirection: "column", gap: tokens.spacingVerticalL, padding: "48px", overflow: "auto" },
  lead: { color: tokens.colorNeutralForeground3, maxWidth: "720px" },
  choices: { display: "grid", gridTemplateColumns: "repeat(2, minmax(0, 360px))", gap: tokens.spacingHorizontalL },
  choice: { padding: tokens.spacingHorizontalL, gap: tokens.spacingVerticalXS },
  mono: { fontFamily: tokens.fontFamilyMonospace, color: tokens.colorNeutralForeground3, wordBreak: "break-all" },
  checklist: { listStyle: "none", margin: 0, padding: 0, display: "flex", flexDirection: "column", gap: tokens.spacingVerticalS },
  check: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS },
  done: { color: tokens.colorStatusSuccessForeground1 },
  foot: { display: "flex", gap: tokens.spacingHorizontalS, marginTop: "auto", maxWidth: "740px" },
  spacer: { flex: 1 },
  feat: { display: "grid", gridTemplateColumns: "40px 1fr", gap: `2px ${tokens.spacingHorizontalM}` },
  featIcon: {
    gridRow: "span 2", width: "40px", height: "40px", borderRadius: tokens.borderRadiusLarge, display: "grid",
    placeItems: "center", backgroundColor: tokens.colorBrandBackground2, color: tokens.colorBrandForeground1, fontSize: "20px",
  },
});

export function OnboardingScreen() {
  const s = useStyles();
  const { t, i18n } = useTranslation("onboarding");
  const status = useAppStatus();
  const { go } = useNavigation();
  const toast = useAppToast();
  const setLanguage = useLanguageSwitch();
  const [step, setStep] = useState<Step>(status.phase === "needsSetup" ? 0 : 2);
  const [dirChoice, setDirChoice] = useState<"default" | "other">("default");
  const [otherDir, setOtherDir] = useState<string | null>(null);
  const [initError, setInitError] = useState<AppError | null>(null);
  const [busy, setBusy] = useState(false);
  const lang: Language = i18n.language === "zh-TW" ? "zh-TW" : "en";
  const dataDir = dirChoice === "default" ? status.dataDir : otherDir;
  const created = !["needsSetup", "initializingCluster"].includes(status.phase);
  const ready = status.phase === "ready";

  const next = async () => {
    if (step === 0) setStep(1);
    else if (step === 1 && dataDir) {
      setBusy(true);
      setInitError(null);
      try {
        await api.appInitialize({ dataDir, language: lang });
        setStep(2);
      } catch (e) {
        setInitError(toAppError(e));
      } finally {
        setBusy(false);
      }
    } else if (step === 2 && ready) setStep(3);
  };
  const chooseOther = async () => {
    const dir = await pickFolder();
    if (dir) {
      setOtherDir(dir);
      setDirChoice("other");
    }
  };
  const pickPst = async () => {
    const paths = await pickPstFiles();
    if (paths.length === 0) return;
    try {
      const results = await api.importEnqueue(paths);
      for (const r of results)
        if (r.result === "rejected") toast.notify(t("import:enqueue.rejected", { path: r.path, reason: formatAppError(r.error, t) }), "error");
      go("import");
    } catch (e) {
      toast.error(e);
    }
  };

  const choice = (testId: string, selected: boolean, onClick: () => void, title: string, hint: string, extra?: JSX.Element) => (
    <Card
      className={s.choice}
      onClick={onClick}
      aria-pressed={selected}
      data-testid={testId}
      appearance={selected ? "filled-alternative" : "outline"}
    >
      <Subtitle1>{title}</Subtitle1>
      <Caption1>{hint}</Caption1>
      {extra}
    </Card>
  );

  let content: JSX.Element;
  if (step === 0)
    content = (
      <>
        <Title3 as="h2">{t("language.heading")}</Title3>
        <Body1 className={s.lead}>{t("language.body")}</Body1>
        <div className={s.choices}>
          {choice("onboarding-lang-zh-TW", lang === "zh-TW", () => void setLanguage("zh-TW"), t("common:language.zh-TW"), t("language.zhHint"))}
          {choice("onboarding-lang-en", lang === "en", () => void setLanguage("en"), t("common:language.en"), t("language.enHint"))}
        </div>
      </>
    );
  else if (step === 1)
    content = (
      <>
        <Title3 as="h2">{t("location.heading")}</Title3>
        <Body1 className={s.lead}>{t("location.body")}</Body1>
        <div className={s.choices}>
          {choice("onboarding-dir-default", dirChoice === "default", () => setDirChoice("default"), t("location.default"), t("location.defaultHint"),
            <span className={s.mono}>{status.dataDir}</span>)}
          {choice("onboarding-dir-other", dirChoice === "other", () => void chooseOther(), t("location.other"), t("location.otherHint"),
            otherDir ? <span className={s.mono}>{otherDir}</span> : undefined)}
        </div>
        {initError ? (
          <MessageBar intent="error">
            <MessageBarBody>{formatAppError(initError, t)}</MessageBarBody>
          </MessageBar>
        ) : null}
      </>
    );
  else if (step === 2)
    content = (
      <>
        <Title3 as="h2">{t("database.heading")}</Title3>
        <Body1 className={s.lead}>{t("database.body")}</Body1>
        <ProgressBar value={ready ? 1 : undefined} thickness="large" />
        <ul className={s.checklist}>
          {[["database.create", created], ["database.start", ready]].map(([key, ok]) => (
            <li key={String(key)} className={s.check}>
              {ok ? <CheckmarkCircleRegular className={s.done} /> : <CircleRegular />}
              <Body1>{t(String(key))}</Body1>
            </li>
          ))}
        </ul>
        {ready ? <Body1 data-testid="onboarding-db-ready">{t("database.ready")}</Body1> : null}
      </>
    );
  else
    content = (
      <>
        <Title3 as="h2">{t("import.heading")}</Title3>
        <Body1 className={s.lead}>{t("import.body")}</Body1>
        <div>
          <Button appearance="primary" size="large" onClick={pickPst} data-testid="onboarding-pick-pst">
            {t("import.pick")}
          </Button>
        </div>
      </>
    );

  const feature = (icon: JSX.Element, title: string, body: string) => (
    <div className={s.feat}>
      <span className={s.featIcon} aria-hidden>
        {icon}
      </span>
      <strong>{title}</strong>
      <Caption1>{body}</Caption1>
    </div>
  );

  return (
    <div className={s.root}>
      <aside className={mergeClasses(s.side, s.left)}>
        <Title2 as="h1">{t("title")}</Title2>
        <ol className={s.steps}>
          {STEP_KEYS.map((key, i) => (
            <li key={key} className={mergeClasses(s.stepItem, i === step && s.stepCurrent)} aria-current={i === step ? "step" : undefined}>
              <span className={mergeClasses(s.num, i === step && s.numCurrent)}>
                {i < step ? <CheckmarkCircleRegular /> : i + 1}
              </span>
              <span>{t(`step.${key}`)}</span>
            </li>
          ))}
        </ol>
        <Caption1>{t("note")}</Caption1>
      </aside>
      <main className={s.body}>
        {content}
        <div className={s.foot}>
          {step === 1 ? (
            <Button onClick={() => setStep(0)} data-testid="onboarding-back">
              {t("nav.back")}
            </Button>
          ) : null}
          <span className={s.spacer} />
          {step === 3 ? (
            <Button appearance="subtle" onClick={() => go("search")} data-testid="onboarding-skip">
              {t("import.later")}
            </Button>
          ) : (
            <Button
              appearance="primary"
              onClick={next}
              disabled={busy || (step === 1 && !dataDir) || (step === 2 && !ready)}
              data-testid="onboarding-next"
            >
              {t("nav.next")}
            </Button>
          )}
        </div>
      </main>
      <aside className={mergeClasses(s.side, s.right)}>
        <Subtitle1 as="h2">{t("feature.title")}</Subtitle1>
        {feature(<SearchRegular />, t("feature.search"), t("feature.searchBody"))}
        {feature(<CopyRegular />, t("feature.dedupe"), t("feature.dedupeBody"))}
        {feature(<ImageRegular />, t("feature.render"), t("feature.renderBody"))}
        {feature(<LockClosedRegular />, t("feature.local"), t("feature.localBody"))}
      </aside>
    </div>
  );
}
