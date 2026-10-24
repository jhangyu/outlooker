import {
  Body1, Body1Strong, Body2, Button, Caption1, Card, MessageBar, MessageBarBody, ProgressBar, Subtitle2, Title3,
  makeStyles, mergeClasses, shorthands, tokens,
} from "@fluentui/react-components";
import {
  CheckmarkCircleRegular, CheckmarkRegular, CircleRegular, CopyRegular, ImageRegular, LockClosedRegular, SearchRegular,
} from "@fluentui/react-icons";
import { useState, type JSX } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../../api/commands";
import { pickFolder } from "../../api/dialog";
import type { AppError } from "../../api/bindings/AppError";
import type { Language } from "../../api/bindings/Language";
import { useAppStatus, useNavigation } from "../../app/AppStatusContext";
import { BrandMark } from "../../components/brand/BrandMark";
import { formatAppError, toAppError, useRegisterPst, useLanguageSwitch } from "../../components/common";

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
  steps: { listStyle: "none", margin: 0, padding: 0, display: "flex", flexDirection: "column" },
  stepItem: {
    display: "grid", gridTemplateColumns: "28px 1fr", gap: tokens.spacingHorizontalM, alignItems: "center",
    padding: `${tokens.spacingVerticalS} 0`, color: tokens.colorNeutralForeground3,
  },
  stepCurrent: { color: tokens.colorNeutralForeground1, fontWeight: tokens.fontWeightSemibold },
  num: {
    width: "28px", height: "28px", boxSizing: "border-box", borderRadius: tokens.borderRadiusCircular, display: "grid", placeItems: "center",
    border: `1px solid ${tokens.colorNeutralStroke1}`, fontSize: tokens.fontSizeBase200, backgroundColor: tokens.colorNeutralBackground1,
  },
  numDone: { backgroundColor: tokens.colorBrandBackground, color: tokens.colorNeutralForegroundOnBrand, ...shorthands.borderColor("transparent") },
  numCurrent: { border: `2px solid ${tokens.colorBrandStroke1}`, color: tokens.colorBrandForeground1 },
  body: { display: "flex", flexDirection: "column", gap: tokens.spacingVerticalXL, padding: "32px 48px", overflow: "auto" },
  lead: { color: tokens.colorNeutralForeground2, maxWidth: "760px" },
  choices: { display: "grid", gridTemplateColumns: "repeat(3, minmax(0, 1fr))", gap: tokens.spacingHorizontalL, maxWidth: "1200px" },
  choice: { padding: tokens.spacingHorizontalL, gap: tokens.spacingVerticalXS },
  // Card draws its border on ::after; thicken it there so the tile does not shift.
  choiceOn: {
    backgroundColor: tokens.colorBrandBackground2,
    ":hover": { backgroundColor: tokens.colorBrandBackground2 },
    "::after": { ...shorthands.borderColor(tokens.colorBrandStroke1), ...shorthands.borderWidth(tokens.strokeWidthThick) },
  },
  hint: { color: tokens.colorNeutralForeground3 },
  mono: { fontFamily: tokens.fontFamilyMonospace, color: tokens.colorNeutralForeground3, wordBreak: "break-all" },
  checklist: { listStyle: "none", margin: 0, padding: 0, display: "flex", flexDirection: "column", gap: tokens.spacingVerticalS },
  check: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS },
  done: { color: tokens.colorStatusSuccessForeground1 },
  foot: {
    display: "flex", gap: tokens.spacingHorizontalS, marginTop: "auto", paddingTop: tokens.spacingVerticalL,
    borderTop: `1px solid ${tokens.colorNeutralStroke2}`,
  },
  spacer: { flex: 1 },
  brandRow: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS },
  feat: { display: "grid", gridTemplateColumns: "36px 1fr", gap: `${tokens.spacingVerticalXS} ${tokens.spacingHorizontalM}` },
  featIcon: {
    gridRow: "span 2", width: "36px", height: "36px", borderRadius: tokens.borderRadiusLarge, display: "grid",
    placeItems: "center", backgroundColor: tokens.colorBrandBackground2, color: tokens.colorBrandForeground1, fontSize: "20px",
  },
});

export function OnboardingScreen() {
  const s = useStyles();
  const { t, i18n } = useTranslation("onboarding");
  const status = useAppStatus();
  const { go } = useNavigation();
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
  const register = useRegisterPst();
  const pickPst = async () => {
    if (await register()) go("import");
  };

  const choice = (testId: string, selected: boolean, onClick: () => void, title: string, hint: string, extra?: JSX.Element) => (
    <Card
      className={mergeClasses(s.choice, selected && s.choiceOn)}
      onClick={onClick}
      aria-pressed={selected}
      data-testid={testId}
      appearance="outline"
    >
      <Body1Strong>{title}</Body1Strong>
      <Caption1 className={s.hint}>{hint}</Caption1>
      {extra}
    </Card>
  );

  let content: JSX.Element;
  if (step === 0)
    content = (
      <>
        <Title3 as="h2">{t("language.heading")}</Title3>
        <Body2 className={s.lead}>{t("language.body")}</Body2>
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
        <Body2 className={s.lead}>{t("location.body")}</Body2>
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
        <Body2 className={s.lead}>{t("database.body")}</Body2>
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
        <Body2 className={s.lead}>{t("import.body")}</Body2>
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
      <Caption1 className={s.hint}>{body}</Caption1>
    </div>
  );

  return (
    <div className={s.root}>
      <aside className={mergeClasses(s.side, s.left)}>
        <div className={s.brandRow}>
          <BrandMark size={28} />
          <strong>{t("common:app.name")}</strong>
        </div>
        <Title3 as="h1">{t("title")}</Title3>
        <ol className={s.steps}>
          {STEP_KEYS.map((key, i) => (
            <li key={key} className={mergeClasses(s.stepItem, i === step && s.stepCurrent)} aria-current={i === step ? "step" : undefined}>
              <span className={mergeClasses(s.num, i < step && s.numDone, i === step && s.numCurrent)}>
                {i < step ? <CheckmarkRegular /> : i + 1}
              </span>
              <span>{t(`step.${key}`)}</span>
            </li>
          ))}
        </ol>
        <Caption1 className={s.hint}>{t("note")}</Caption1>
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
        <Subtitle2 as="h2">{t("feature.title")}</Subtitle2>
        {feature(<SearchRegular />, t("feature.search"), t("feature.searchBody"))}
        {feature(<CopyRegular />, t("feature.dedupe"), t("feature.dedupeBody"))}
        {feature(<ImageRegular />, t("feature.render"), t("feature.renderBody"))}
        {feature(<LockClosedRegular />, t("feature.local"), t("feature.localBody"))}
      </aside>
    </div>
  );
}
