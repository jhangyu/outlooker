import {
  Badge, Body1, Button, Caption1, Card, DataGrid, DataGridBody, DataGridCell, DataGridHeader, DataGridHeaderCell,
  DataGridRow, Field, Input, Radio, RadioGroup, Slider, SpinButton, Spinner, Subtitle2, Tab, TabList, Title3,
  createTableColumn, makeStyles, mergeClasses, tokens, type TableColumnDefinition,
} from "@fluentui/react-components";
import { DatabaseRegular, FolderRegular, GlobeRegular, ImageRegular, MailInboxRegular, WrenchRegular } from "@fluentui/react-icons";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useRef, useState, type JSX, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../../api/commands";
import { pickFolder } from "../../api/dialog";
import type { IndexStatus } from "../../api/bindings/IndexStatus";
import type { Language } from "../../api/bindings/Language";
import type { MaintenanceTask } from "../../api/bindings/MaintenanceTask";
import { useAppStatus } from "../../app/AppStatusContext";
import {
  EmptyState, formatBytes, formatDateTime, formatNumber, useAppToast, useLanguageSwitch,
} from "../../components/common";
import { MoveDialog } from "./MoveDialog";

export function isValidQuality(v: number): boolean {
  return Number.isInteger(v) && v >= 1 && v <= 100;
}

type SectionId = "storage" | "language" | "content" | "db" | "index";
const SECTIONS: SectionId[] = ["storage", "language", "content", "db", "index"];

const useStyles = makeStyles({
  root: { display: "grid", gridTemplateColumns: "220px 1fr", height: "100%", minHeight: 0 },
  nav: {
    backgroundColor: tokens.colorNeutralBackground1, borderRight: `1px solid ${tokens.colorNeutralStroke2}`,
    padding: `${tokens.spacingVerticalM} ${tokens.spacingHorizontalS}`, display: "flex", flexDirection: "column", gap: tokens.spacingVerticalS,
  },
  body: {
    overflow: "auto", padding: tokens.spacingHorizontalL, display: "grid", gridTemplateColumns: "repeat(3, 1fr)",
    gap: tokens.spacingHorizontalL, alignContent: "start",
  },
  card: { display: "flex", flexDirection: "column", gap: tokens.spacingVerticalM, padding: tokens.spacingHorizontalL },
  wide2: { gridColumn: "span 2" },
  wide3: { gridColumn: "1 / -1" },
  head: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS },
  row: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS, flexWrap: "wrap" },
  grow: { flex: 1 },
  mono: { fontFamily: tokens.fontFamilyMonospace },
  muted: { color: tokens.colorNeutralForeground3 },
  facts: { display: "grid", gridTemplateColumns: "max-content 1fr", gap: `${tokens.spacingVerticalXS} ${tokens.spacingHorizontalL}`, margin: 0 },
  dd: { margin: 0, fontVariantNumeric: "tabular-nums" },
  ends: { display: "flex", justifyContent: "space-between" },
});

export function SettingsScreen() {
  const s = useStyles();
  const { t, i18n } = useTranslation("settings");
  const lng = i18n.language;
  const status = useAppStatus();
  const qc = useQueryClient();
  const toast = useAppToast();
  const setLanguage = useLanguageSwitch();
  const settings = useQuery({ queryKey: ["settings"], queryFn: () => api.settingsGet() });
  const db = useQuery({ queryKey: ["dbStatus"], queryFn: () => api.dbStatus(), refetchInterval: 10_000 });
  const [draft, setDraft] = useState<number | null>(null);
  const [invalid, setInvalid] = useState(false);
  const [moveTarget, setMoveTarget] = useState<string | null>(null);
  const cards = useRef<Partial<Record<SectionId, HTMLElement | null>>>({});
  const quality = draft ?? settings.data?.webpQuality ?? 80;
  const running = db.data?.maintenance?.state === "running" ? db.data.maintenance : null;

  const applyQuality = async () => {
    if (!isValidQuality(quality)) {
      setInvalid(true);
      return;
    }
    setInvalid(false);
    try {
      qc.setQueryData(["settings"], await api.settingsUpdate({ language: null, webpQuality: quality }));
      setDraft(null);
      toast.notify(t("webp.saved"));
    } catch (e) {
      toast.error(e);
    }
  };
  const runTask = async (task: MaintenanceTask) => {
    try {
      await api.maintenanceRun(task);
      toast.notify(t("maintenance.started", { task: t(`maintenance.task.${task}`) }));
      await qc.invalidateQueries({ queryKey: ["dbStatus"] });
    } catch (e) {
      toast.error(e);
    }
  };
  const chooseTarget = async () => {
    const dir = await pickFolder();
    if (dir) setMoveTarget(dir);
  };

  const card = (id: SectionId, icon: JSX.Element, children: ReactNode, wide?: string) => (
    <Card className={mergeClasses(s.card, wide)} ref={(el) => void (cards.current[id] = el)} aria-labelledby={`set-${id}`}>
      <div className={s.head}>
        {icon}
        <Subtitle2 as="h2" id={`set-${id}`}>
          {t(`section.${id}`)}
        </Subtitle2>
      </div>
      {children}
    </Card>
  );
  const fact = (label: string, value: string) => (
    <>
      <dt className={s.muted}>{label}</dt>
      <dd className={s.dd}>{value}</dd>
    </>
  );
  const columns: TableColumnDefinition<IndexStatus>[] = [
    createTableColumn<IndexStatus>({ columnId: "name", renderHeaderCell: () => t("index.name"), renderCell: (x) => <span className={s.mono}>{x.name}</span> }),
    createTableColumn<IndexStatus>({ columnId: "table", renderHeaderCell: () => t("index.table"), renderCell: (x) => x.table }),
    createTableColumn<IndexStatus>({ columnId: "size", renderHeaderCell: () => t("index.size"), renderCell: (x) => formatBytes(x.sizeBytes, lng) }),
    createTableColumn<IndexStatus>({
      columnId: "state",
      renderHeaderCell: () => t("index.state"),
      renderCell: (x) => (
        <Badge appearance="tint" color={x.valid ? "success" : "warning"}>
          {x.valid ? t("index.healthy") : t("index.rebuild")}
        </Badge>
      ),
    }),
  ];

  return (
    <div className={s.root}>
      <nav className={s.nav} aria-label={t("title")}>
        <Title3 as="h1">{t("title")}</Title3>
        <TabList vertical onTabSelect={(_, d) => cards.current[d.value as SectionId]?.scrollIntoView({ block: "start" })}>
          {SECTIONS.map((id) => (
            <Tab key={id} value={id}>
              {t(`section.${id}`)}
            </Tab>
          ))}
        </TabList>
      </nav>
      <div className={s.body}>
        {card(
          "storage",
          <FolderRegular />,
          <>
            <Field label={t("storage.folder")}>
              <div className={s.row}>
                <Input className={mergeClasses(s.grow, s.mono)} readOnly value={settings.data?.dataDir ?? status.dataDir} />
                <Button onClick={chooseTarget} data-testid="settings-move">
                  {t("storage.change")}
                </Button>
              </div>
            </Field>
            <Caption1 className={s.muted}>{t("storage.help")}</Caption1>
          </>,
          s.wide2,
        )}
        {card(
          "language",
          <GlobeRegular />,
          <>
            <RadioGroup value={lng === "zh-TW" ? "zh-TW" : "en"} onChange={(_, d) => void setLanguage(d.value as Language)}>
              <Radio value="zh-TW" label={t("common:language.zh-TW")} data-testid="settings-language-zh-TW" />
              <Radio value="en" label={t("common:language.en")} data-testid="settings-language-en" />
            </RadioGroup>
            <Caption1 className={s.muted}>{t("language.help")}</Caption1>
          </>,
        )}
        {card(
          "content",
          <ImageRegular />,
          <>
            <Field label={t("webp.label")} validationMessage={invalid ? t("webp.invalid") : undefined} validationState={invalid ? "error" : "none"}>
              <div className={s.row}>
                <Slider
                  className={s.grow}
                  min={1}
                  max={100}
                  step={1}
                  value={isValidQuality(quality) ? quality : 80}
                  onChange={(_, d) => setDraft(d.value)}
                />
                <SpinButton
                  min={1}
                  max={100}
                  value={Number.isFinite(quality) ? quality : null}
                  onChange={(_, d) => setDraft(d.value ?? (d.displayValue !== undefined ? Number(d.displayValue) : Number.NaN))}
                />
              </div>
            </Field>
            <div className={s.ends}>
              <Caption1 className={s.muted}>{t("webp.smaller")}</Caption1>
              <Caption1 className={s.muted}>{t("webp.sharper")}</Caption1>
            </div>
            <div className={s.row}>
              <Button onClick={applyQuality} data-testid="settings-webp-apply">
                {t("webp.apply")}
              </Button>
              <Caption1 className={s.muted}>{t("webp.help")}</Caption1>
            </div>
          </>,
        )}
        {card(
          "db",
          <DatabaseRegular />,
          db.data ? (
            <>
              <dl className={s.facts}>
                {fact(t("db.status"), t(`common:phase.${db.data.phase}`))}
                {fact(t("db.engine"), t("db.engineValue", { version: db.data.postgresVersion }))}
                {fact(t("db.messages"), formatNumber(db.data.counts.messages, lng))}
                {fact(t("db.psts"), formatNumber(db.data.counts.psts, lng))}
                {fact(t("db.images"), formatNumber(db.data.imageCount, lng))}
                {fact(t("db.attachments"), formatNumber(db.data.attachmentCount, lng))}
                {fact(t("db.size"), formatBytes(db.data.databaseSizeBytes, lng))}
                {fact(t("db.lastVacuum"), db.data.lastVacuumAt ? formatDateTime(db.data.lastVacuumAt, lng) : t("db.never"))}
                {fact(t("db.lastAnalyze"), db.data.lastAnalyzeAt ? formatDateTime(db.data.lastAnalyzeAt, lng) : t("db.never"))}
              </dl>
              {status.counts.messages === 0 ? (
                <EmptyState icon={<MailInboxRegular />} title={t("db.emptyTitle")} body={t("db.emptyBody")} />
              ) : null}
            </>
          ) : (
            <Body1 className={s.muted}>{db.isError ? t("db.unavailable") : ""}</Body1>
          ),
        )}
        {card(
          "index",
          <WrenchRegular />,
          <>
            <DataGrid items={db.data?.indexes ?? []} columns={columns} getRowId={(x) => x.name}>
              <DataGridHeader>
                <DataGridRow>{({ renderHeaderCell }) => <DataGridHeaderCell>{renderHeaderCell()}</DataGridHeaderCell>}</DataGridRow>
              </DataGridHeader>
              <DataGridBody<IndexStatus>>
                {({ item, rowId }) => (
                  <DataGridRow<IndexStatus> key={rowId}>{({ renderCell }) => <DataGridCell>{renderCell(item)}</DataGridCell>}</DataGridRow>
                )}
              </DataGridBody>
            </DataGrid>
            <div className={s.row}>
              {(["analyze", "vacuum", "reindex"] as const).map((task) => (
                <Button key={task} disabled={running !== null} onClick={() => void runTask(task)} data-testid={`maint-${task}`}>
                  {t(`index.${task}`)}
                </Button>
              ))}
              {running ? <Spinner size="tiny" label={t("maintenance.running", { task: t(`maintenance.task.${running.task}`) })} /> : null}
              <Caption1 className={s.muted}>{t("index.help")}</Caption1>
            </div>
          </>,
          s.wide3,
        )}
      </div>
      {moveTarget ? <MoveDialog target={moveTarget} onClose={() => setMoveTarget(null)} /> : null}
    </div>
  );
}
