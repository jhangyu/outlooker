import {
  Badge, Body1, Button, Caption1, Card, DataGrid, DataGridBody, DataGridCell, DataGridHeader, DataGridHeaderCell,
  DataGridRow, Field, Input, Radio, RadioGroup, Slider, SpinButton, Spinner, Subtitle2, Tab, TabList, Title3,
  createTableColumn, makeStyles, mergeClasses, tokens, type TableColumnDefinition,
} from "@fluentui/react-components";
import { DatabaseRegular, FolderRegular, GlobeRegular, ImageRegular, WrenchRegular } from "@fluentui/react-icons";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useRef, useState, type JSX, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../../api/commands";
import { pickFolder } from "../../api/dialog";
import { dbStatusQuery } from "../../api/queries";
import type { IndexStatus } from "../../api/bindings/IndexStatus";
import type { Language } from "../../api/bindings/Language";
import type { MaintenanceTask } from "../../api/bindings/MaintenanceTask";
import { useAppStatus } from "../../app/AppStatusContext";
import {
  Splitter, formatBytes, formatDateTime, formatNumber, useAppToast, useColumnWidths, useLanguageSwitch, useSplit,
  type ColumnDefaults, type SplitDefaults,
} from "../../components/common";
import { MoveDialog } from "./MoveDialog";

export function isValidQuality(v: number): boolean {
  return Number.isInteger(v) && v >= 1 && v <= 100;
}

type SectionId = "storage" | "language" | "content" | "db" | "index";
const SECTIONS: SectionId[] = ["storage", "language", "content", "db", "index"];
const INDEX_COLUMNS: ColumnDefaults = { name: { width: 260 }, table: { width: 180 }, size: { width: 96 }, state: { width: 120 } };

const SPLITS: SplitDefaults = {
  nav: { size: 220, min: 160, max: 400 },
  c1: { min: 160, minOther: 320 },
  c2: { min: 160, minOther: 320 },
  r1: { min: 80, max: 1200 },
  r2: { min: 80, max: 1200 },
};
// Centre a 6 px hit area in the grid gap after a grid area.
const GAP_EDGE = `calc(${tokens.spacingHorizontalL} / -2 - 3px)`;

const useStyles = makeStyles({
  root: { display: "grid", height: "100%", minHeight: 0 },
  nav: {
    position: "relative", backgroundColor: tokens.colorNeutralBackground1, borderRight: `1px solid ${tokens.colorNeutralStroke2}`,
    padding: `${tokens.spacingVerticalM} ${tokens.spacingHorizontalS}`, display: "flex", flexDirection: "column", gap: tokens.spacingVerticalXXS,
  },
  // Mockup `h2 { margin: 0 0 8px 8px }`: the title lines up with the tab labels.
  navTitle: { marginLeft: tokens.spacingHorizontalS },
  tabs: { marginTop: tokens.spacingVerticalS },
  body: {
    position: "relative", overflow: "auto", padding: tokens.spacingHorizontalL, display: "grid",
    gridTemplateAreas: '"storage storage db" "language content db" "index index index"',
    gap: tokens.spacingHorizontalL, alignContent: "start",
  },
  card: { display: "flex", flexDirection: "column", gap: tokens.spacingVerticalM, padding: tokens.spacingHorizontalL, minWidth: 0, overflow: "auto" },
  storage: { gridArea: "storage" },
  language: { gridArea: "language" },
  content: { gridArea: "content" },
  db: { gridArea: "db" },
  index: { gridArea: "index" },
  head: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS },
  row: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS, flexWrap: "wrap" },
  sliderRow: { display: "flex", alignItems: "flex-start", gap: tokens.spacingHorizontalS },
  sliderCol: { flex: 1, minWidth: 0, display: "flex", flexDirection: "column" },
  spin: { width: "88px", flexShrink: 0 },
  grow: { flex: 1 },
  mono: { fontFamily: tokens.fontFamilyMonospace },
  muted: { color: tokens.colorNeutralForeground3 },
  facts: { display: "grid", gridTemplateColumns: "max-content 1fr", gap: `${tokens.spacingVerticalXS} ${tokens.spacingHorizontalL}`, margin: 0 },
  dd: { margin: 0, fontVariantNumeric: "tabular-nums" },
  num: { justifyContent: "flex-end", fontVariantNumeric: "tabular-nums" },
  nowrap: { whiteSpace: "nowrap" },
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
  const db = useQuery(dbStatusQuery);
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
      await qc.invalidateQueries({ queryKey: dbStatusQuery.queryKey });
    } catch (e) {
      toast.error(e);
    }
  };
  const chooseTarget = async () => {
    const dir = await pickFolder();
    if (dir) setMoveTarget(dir);
  };

  const card = (id: SectionId, icon: JSX.Element, children: ReactNode) => (
    <Card className={mergeClasses(s.card, s[id])} ref={(el) => void (cards.current[id] = el)} aria-labelledby={`set-${id}`}>
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
  const split = useSplit("settings", SPLITS);
  const cardSize = (id: SectionId, dim: "offsetWidth" | "offsetHeight") => () => cards.current[id]?.[dim] ?? 0;
  const sizing = useColumnWidths("settings-indexes", INDEX_COLUMNS);
  const columns: TableColumnDefinition<IndexStatus>[] = [
    createTableColumn<IndexStatus>({ columnId: "name", renderHeaderCell: () => t("index.name"), renderCell: (x) => <span className={s.mono}>{x.name}</span> }),
    createTableColumn<IndexStatus>({ columnId: "table", renderHeaderCell: () => t("index.table"), renderCell: (x) => x.table }),
    createTableColumn<IndexStatus>({ columnId: "size", renderHeaderCell: () => t("index.size"), renderCell: (x) => formatBytes(x.sizeBytes, lng) }),
    createTableColumn<IndexStatus>({
      columnId: "state",
      renderHeaderCell: () => t("index.state"),
      renderCell: (x) => (
        <Badge appearance="tint" className={s.nowrap} color={x.valid ? "success" : "warning"}>
          {x.valid ? t("index.healthy") : t("index.rebuild")}
        </Badge>
      ),
    }),
  ];

  return (
    <div className={s.root} style={{ gridTemplateColumns: `${split.track("nav", "220px")} minmax(0, 1fr)` }}>
      <nav className={s.nav} aria-label={t("title")}>
        <Title3 as="h1" className={s.navTitle}>{t("title")}</Title3>
        <TabList vertical className={s.tabs} onTabSelect={(_, d) => cards.current[d.value as SectionId]?.scrollIntoView({ block: "start" })}>
          {SECTIONS.map((id) => (
            <Tab key={id} value={id}>
              {t(`section.${id}`)}
            </Tab>
          ))}
        </TabList>
        <Splitter orientation="vertical" {...split.props("nav")} style={{ top: 0, bottom: 0, right: -3 }} />
      </nav>
      <div
        className={s.body}
        style={{
          gridTemplateColumns: `${split.track("c1", "minmax(0, 1fr)")} ${split.track("c2", "minmax(0, 1fr)")} minmax(0, 1fr)`,
          gridTemplateRows: `${split.track("r1", "auto")} ${split.track("r2", "auto")} auto`,
        }}
      >
        <Splitter orientation="vertical" {...split.props("c1")} measure={cardSize("language", "offsetWidth")} style={{ gridColumn: 1, gridRow: 2, top: 0, bottom: 0, right: GAP_EDGE }} />
        <Splitter orientation="vertical" {...split.props("c2")} measure={cardSize("content", "offsetWidth")} style={{ gridColumn: 2, gridRow: "1 / 3", top: 0, bottom: 0, right: GAP_EDGE }} />
        <Splitter orientation="horizontal" {...split.props("r1")} measure={cardSize("storage", "offsetHeight")} style={{ gridColumn: "1 / 3", gridRow: 1, left: 0, right: 0, bottom: GAP_EDGE }} />
        <Splitter orientation="horizontal" {...split.props("r2")} measure={cardSize("language", "offsetHeight")} style={{ gridColumn: "1 / -1", gridRow: 2, left: 0, right: 0, bottom: GAP_EDGE }} />
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
              <div className={s.sliderRow}>
                <div className={s.sliderCol}>
                  <Slider
                    min={1}
                    max={100}
                    step={1}
                    value={isValidQuality(quality) ? quality : 80}
                    onChange={(_, d) => setDraft(d.value)}
                  />
                  <div className={s.ends}>
                    <Caption1 className={s.muted}>{t("webp.smaller")}</Caption1>
                    <Caption1 className={s.muted}>{t("webp.sharper")}</Caption1>
                  </div>
                </div>
                <SpinButton
                  className={s.spin}
                  min={1}
                  max={100}
                  value={Number.isFinite(quality) ? quality : null}
                  onChange={(_, d) => setDraft(d.value ?? (d.displayValue !== undefined ? Number(d.displayValue) : Number.NaN))}
                />
              </div>
            </Field>
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
              {status.counts.messages === 0 ? <Caption1 className={s.muted}>{t("db.emptyBody")}</Caption1> : null}
            </>
          ) : (
            <Body1 className={s.muted}>{db.isError ? t("db.unavailable") : ""}</Body1>
          ),
        )}
        {card(
          "index",
          <WrenchRegular />,
          <>
            <DataGrid {...sizing} items={db.data?.indexes ?? []} columns={columns} getRowId={(x) => x.name}>
              <DataGridHeader>
                <DataGridRow>
                  {({ renderHeaderCell, columnId }) => (
                    <DataGridHeaderCell className={columnId === "size" ? s.num : undefined} button={{ className: columnId === "size" ? s.num : undefined }}>
                      {renderHeaderCell()}
                    </DataGridHeaderCell>
                  )}
                </DataGridRow>
              </DataGridHeader>
              <DataGridBody<IndexStatus>>
                {({ item, rowId }) => (
                  <DataGridRow<IndexStatus> key={rowId}>
                    {({ renderCell, columnId }) => <DataGridCell className={columnId === "size" ? s.num : undefined}>{renderCell(item)}</DataGridCell>}
                  </DataGridRow>
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
        )}
      </div>
      {moveTarget ? <MoveDialog target={moveTarget} onClose={() => setMoveTarget(null)} /> : null}
    </div>
  );
}
