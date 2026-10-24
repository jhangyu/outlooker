import {
  Body1, Button, Caption1, Card, Dropdown, Field, Input, Badge, MessageBar, MessageBarBody, Option, Radio, RadioGroup,
  SpinButton, ToggleButton, makeStyles, mergeClasses, tokens,
} from "@fluentui/react-components";
import {
  CalendarRegular, ClockRegular, DocumentRegular, ErrorCircleRegular, HourglassRegular, LockClosedRegular, ArrowSyncRegular, WarningRegular,
} from "@fluentui/react-icons";
import { useInfiniteQuery, useQueryClient } from "@tanstack/react-query";
import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../../api/commands";
import { syncStatusQuery } from "../../api/queries";
import type { PstSource } from "../../api/bindings/PstSource";
import type { SyncStatus } from "../../api/bindings/SyncStatus";
import { EmptyState, formatBytes, formatDateTime, formatNumber, formatRelative, useAppToast, useNow } from "../../components/common";
import { toDraft, toSchedule, type ScheduleDraft } from "./syncState";

const useStyles = makeStyles({
  side: { display: "flex", flexDirection: "column", gap: tokens.spacingVerticalM, padding: tokens.spacingHorizontalM },
  card: { display: "flex", flexDirection: "column", gap: tokens.spacingVerticalM },
  heading: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS },
  grow: { flex: 1 },
  muted: { color: tokens.colorNeutralForeground3 },
  sub: { display: "flex", flexDirection: "column", gap: tokens.spacingVerticalS, marginLeft: tokens.spacingHorizontalXXL },
  inline: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS },
  chips: { display: "flex", flexWrap: "wrap", gap: tokens.spacingHorizontalXS },
  chip: { minWidth: "32px", width: "32px", height: "32px", padding: 0 },
  nextLine: {
    display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS, padding: `${tokens.spacingVerticalS} ${tokens.spacingHorizontalM}`,
    borderRadius: tokens.borderRadiusMedium, backgroundColor: tokens.colorBrandBackground2, color: tokens.colorBrandForeground2,
  },
  foot: { display: "flex", justifyContent: "flex-end", gap: tokens.spacingHorizontalS },
  item: { display: "flex", flexDirection: "column", gap: tokens.spacingVerticalXS, paddingTop: tokens.spacingVerticalS, borderTop: `1px solid ${tokens.colorNeutralStroke3}` },
  meta: { display: "flex", gap: tokens.spacingHorizontalM, color: tokens.colorNeutralForeground3, fontSize: tokens.fontSizeBase200 },
  phases: { display: "flex", gap: "2px" },
  phase: { flex: 1, height: "4px", borderRadius: "2px", backgroundColor: tokens.colorNeutralBackground5 },
  phaseOn: { backgroundColor: tokens.colorBrandBackground },
  newPill: { color: tokens.colorStatusSuccessForeground1, fontWeight: tokens.fontWeightSemibold },
  warn: { color: tokens.colorStatusWarningForeground1 },
  err: { color: tokens.colorStatusDangerForeground1 },
  logEntry: {
    display: "grid", gridTemplateColumns: "20px 1fr", gap: `${tokens.spacingVerticalXS} ${tokens.spacingHorizontalS}`,
    paddingTop: tokens.spacingVerticalS, borderTop: `1px solid ${tokens.colorNeutralStroke3}`,
  },
  logTime: { gridColumn: "2", fontFamily: tokens.fontFamilyMonospace, color: tokens.colorNeutralForeground3 },
  logMsg: { minWidth: 0, overflowWrap: "anywhere", userSelect: "text" },
  logLoc: {
    gridColumn: "2", fontFamily: tokens.fontFamilyMonospace, wordBreak: "break-all", color: tokens.colorNeutralForeground3,
    fontSize: tokens.fontSizeBase200,
  },
});

const PHASES = ["scanning", "importing", "finalizing"] as const;

export function ScheduleCard({ status }: { status: SyncStatus }) {
  const s = useStyles();
  const { t, i18n } = useTranslation("import");
  const lng = i18n.language;
  const now = useNow();
  const qc = useQueryClient();
  const toast = useAppToast();
  const [draft, setDraft] = useState<ScheduleDraft>(() => toDraft(status.schedule));
  const saved = toDraft(status.schedule);
  const schedule = toSchedule(draft);
  const dirty = JSON.stringify(schedule) !== JSON.stringify(toSchedule(saved));
  const set = (p: Partial<ScheduleDraft>) => setDraft((d) => ({ ...d, ...p }));
  const day = (iso: number, style: "narrow" | "long") => new Intl.DateTimeFormat(lng, { weekday: style }).format(new Date(2024, 0, iso));
  const save = async () => {
    if (!schedule) return;
    try {
      const next = await api.syncScheduleSet(schedule);
      qc.setQueryData(syncStatusQuery.queryKey, next);
      setDraft(toDraft(next.schedule));
    } catch (e) {
      toast.error(e);
    }
  };
  return (
    <Card className={s.card} role="group" aria-labelledby="sync-sched-h">
      <div className={s.heading}>
        <ClockRegular />
        <Body1 id="sync-sched-h" as="h4" style={{ margin: 0, fontWeight: tokens.fontWeightSemibold }}>
          {t("sched.title")}
        </Body1>
        <span className={s.grow} />
        <Caption1 className={s.muted}>{t("sched.appliesTo")}</Caption1>
      </div>
      <RadioGroup value={draft.mode} onChange={(_, d) => set({ mode: d.value as ScheduleDraft["mode"] })} aria-labelledby="sync-sched-h">
        <Radio value="off" label={t("sched.off")} />
        <Radio value="interval" label={t("sched.interval")} />
        {draft.mode === "interval" ? (
          <div className={s.sub}>
            <Field label={t("sched.every")}>
              <div className={s.inline}>
                <SpinButton
                  min={1}
                  max={999}
                  value={draft.n}
                  aria-label={t("sched.everyValue")}
                  onChange={(_, d) => set({ n: d.value ?? Number(d.displayValue) })}
                />
                <Dropdown
                  aria-label={t("sched.unit")}
                  value={t(`sched.${draft.unit}`)}
                  selectedOptions={[draft.unit]}
                  onOptionSelect={(_, d) => set({ unit: d.optionValue as ScheduleDraft["unit"] })}
                  style={{ minWidth: 0, width: "120px" }}
                >
                  <Option value="minutes">{t("sched.minutes")}</Option>
                  <Option value="hours">{t("sched.hours")}</Option>
                </Dropdown>
              </div>
            </Field>
          </div>
        ) : null}
        <Radio value="daily" label={t("sched.daily")} />
        {draft.mode === "daily" ? (
          <div className={s.sub}>
            <Field label={t("sched.time")}>
              {/* Fluent v9 core has no TimePicker and the timepicker package is a new dependency: native time input. */}
              <Input type="time" value={draft.time} aria-label={t("sched.time")} onChange={(_, d) => set({ time: d.value })} style={{ width: "140px" }} />
            </Field>
            <Field label={t("sched.weekdays")}>
              <div className={s.chips} role="group" aria-label={t("sched.weekdays")}>
                {[1, 2, 3, 4, 5, 6, 7].map((iso) => (
                  <ToggleButton
                    key={iso}
                    shape="circular"
                    className={s.chip}
                    checked={draft.days.includes(iso)}
                    aria-label={day(iso, "long")}
                    onClick={() => set({ days: draft.days.includes(iso) ? draft.days.filter((x) => x !== iso) : [...draft.days, iso] })}
                  >
                    {day(iso, "narrow")}
                  </ToggleButton>
                ))}
              </div>
            </Field>
          </div>
        ) : null}
      </RadioGroup>
      {status.schedule.mode !== "off" && status.nextRunAt ? (
        <div className={s.nextLine} data-testid="next-sync">
          <CalendarRegular />
          <span title={formatDateTime(status.nextRunAt, lng)}>{t("sched.next", { time: formatRelative(status.nextRunAt, lng, now) })}</span>
        </div>
      ) : null}
      <MessageBar intent="info">
        <MessageBarBody>{t("sched.note")}</MessageBarBody>
      </MessageBar>
      <div className={s.foot}>
        <Button disabled={!dirty} onClick={() => setDraft(saved)}>
          {t("sched.revert")}
        </Button>
        <Button appearance="primary" disabled={!dirty || !schedule} onClick={() => void save()} data-testid="sched-save">
          {t("sched.save")}
        </Button>
      </div>
    </Card>
  );
}

export function ActivityCard({ list, status, onPause }: { list: PstSource[]; status: SyncStatus | undefined; onPause(id: number): void }) {
  const s = useStyles();
  const { t, i18n } = useTranslation("import");
  const lng = i18n.language;
  const now = useNow();
  const running = list.filter((p) => p.status === "running");
  const queued = list.filter((p) => p.status === "queued");
  const locked = list.filter((p) => p.status === "locked");
  const busy = running.length + queued.length + locked.length > 0;
  return (
    <Card className={s.card} role="region" aria-labelledby="sync-act-h">
      <div className={s.heading}>
        <ArrowSyncRegular />
        <Body1 id="sync-act-h" as="h4" style={{ margin: 0, fontWeight: tokens.fontWeightSemibold }}>
          {t("activity.title")}
        </Body1>
        <span className={s.grow} />
        {status?.lastRunAt ? <Caption1 className={s.muted} title={formatDateTime(status.lastRunAt, lng)}>{t("activity.lastRun", { time: formatRelative(status.lastRunAt, lng, now) })}</Caption1> : null}
      </div>
      {!busy ? (
        <Caption1 className={s.muted}>{t("activity.idle")}</Caption1>
      ) : (
        <>
          {running.map((p) => {
            const pr = p.progress;
            const total = pr?.itemsTotal ?? null;
            const done = pr?.itemsDone ?? 0;
            const cur = pr ? PHASES.indexOf(pr.phase) : -1;
            return (
              <div key={p.id} className={s.item} data-testid="activity-running">
                <div className={s.inline}>
                  <ArrowSyncRegular />
                  <Body1 style={{ flex: 1, fontWeight: tokens.fontWeightSemibold }}>{p.displayName}</Body1>
                  <Badge appearance="tint" color="brand">
                    {t("status.running")}
                  </Badge>
                </div>
                <div className={s.phases} aria-label={pr ? t(`phase.${pr.phase}`) : undefined}>
                  {PHASES.map((ph, i) => (
                    <i key={ph} className={mergeClasses(s.phase, i <= cur && s.phaseOn)} />
                  ))}
                </div>
                <div className={s.meta}>
                  <span>{pr ? t(`phase.${pr.phase}`) : ""}</span>
                  <span className={s.grow} />
                  {total ? <span>{t("progress", { done: formatNumber(done, lng), total: formatNumber(total, lng), percent: Math.round((done / total) * 100) })}</span> : null}
                </div>
                <div className={s.meta}>
                  {pr?.bytesPerSec ? <span>{t("rate", { rate: formatBytes(pr.bytesPerSec, lng) })}</span> : null}
                  {pr?.etaSeconds ? <span>{t("eta", { minutes: Math.max(1, Math.round(pr.etaSeconds / 60)) })}</span> : null}
                  <span className={s.grow} />
                  <Button size="small" appearance="subtle" disabled={p.paused} onClick={() => onPause(p.id)} data-testid="activity-pause">
                    {t("activity.pause")}
                  </Button>
                </div>
              </div>
            );
          })}
          {queued.map((p) => (
            <div key={p.id} className={s.item}>
              <div className={s.inline}>
                <HourglassRegular className={s.muted} />
                <Body1 style={{ flex: 1, fontWeight: tokens.fontWeightSemibold }}>{p.displayName}</Body1>
                <Badge appearance="outline">{t("status.queued")}</Badge>
              </div>
            </div>
          ))}
          {locked.map((p) => (
            <div key={p.id} className={s.item}>
              <div className={s.inline}>
                <LockClosedRegular className={s.warn} />
                <Body1 style={{ flex: 1, fontWeight: tokens.fontWeightSemibold }}>{p.displayName}</Body1>
                <Badge appearance="tint" color="warning">
                  {t("activity.lockedSkipped")}
                </Badge>
              </div>
              <div className={s.meta}>{t("activity.retryNext")}</div>
            </div>
          ))}
        </>
      )}
    </Card>
  );
}

/** Error log of one PST (via importErrors), shown under the other cards when the selected path has errors. */
export function ErrorLogCard({ pst }: { pst: PstSource }) {
  const s = useStyles();
  const { t, i18n } = useTranslation("import");
  const errors = useInfiniteQuery({
    queryKey: ["importErrors", pst.id],
    queryFn: ({ pageParam }) => api.importErrors({ pstId: pst.id, cursor: pageParam, limit: 100 }),
    initialPageParam: null as string | null,
    getNextPageParam: (last) => last.nextCursor ?? undefined,
  });
  const rows = errors.data?.pages.flatMap((p) => p.rows) ?? [];
  return (
    <Card className={s.card} role="region" aria-label={t("log.title")}>
      <div className={s.heading}>
        <DocumentRegular />
        <Body1 as="h4" style={{ margin: 0, fontWeight: tokens.fontWeightSemibold }}>
          {t("log.title")}
        </Body1>
        <Caption1 className={s.muted} title={pst.displayName}>
          — {pst.displayName}
        </Caption1>
      </div>
      {errors.isSuccess && rows.length === 0 ? <EmptyState icon={<DocumentRegular />} title={t("log.none")} /> : null}
      {rows.map((r) => (
        <div key={r.id} className={s.logEntry}>
          {r.severity === "jobFailed" ? <ErrorCircleRegular className={s.err} /> : <WarningRegular className={s.warn} />}
          <Body1 className={s.logMsg}>
            {t(`stage.${r.stage}`)} · {r.message}
          </Body1>
          <Caption1 className={s.logTime}>{formatDateTime(r.occurredAt, i18n.language)}</Caption1>
          <span className={s.logLoc}>
            {[r.folderPath, r.pstItemId !== null ? t("log.item", { id: r.pstItemId }) : null].filter(Boolean).join(" · ")}
          </span>
        </div>
      ))}
      {errors.hasNextPage ? (
        <Button appearance="subtle" onClick={() => void errors.fetchNextPage()}>
          {t("log.more")}
        </Button>
      ) : null}
    </Card>
  );
}

export function SidePanel({ children }: { children: ReactNode }) {
  return <div className={useStyles().side}>{children}</div>;
}
