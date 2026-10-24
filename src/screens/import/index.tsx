import {
  Badge, Body1Strong, Button, Caption1, DataGrid, DataGridBody, DataGridCell, DataGridHeader, DataGridHeaderCell, DataGridRow,
  ProgressBar, Subtitle2, Switch, TableCellLayout, createTableColumn, makeStyles, mergeClasses, tokens, buttonClassNames,
  type BadgeProps, type TableColumnDefinition, type TableColumnId,
} from "@fluentui/react-components";
import {
  AddRegular, ArrowSyncRegular, CheckmarkCircleRegular, DeleteRegular, DismissCircleRegular, DocumentRegular, HourglassRegular,
  LockClosedRegular, MailInboxRegular, PauseCircleRegular,
} from "@fluentui/react-icons";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useState, type ReactElement } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../../api/commands";
import { dbStatusQuery, pstListQuery, syncStatusQuery } from "../../api/queries";
import type { PstSource } from "../../api/bindings/PstSource";
import { useAppStatus } from "../../app/AppStatusContext";
import {
  EmptyState, ellipsis, formatBytes, formatDateTime, formatNumber, useAppToast, useColumnWidths, useRegisterPst,
} from "../../components/common";
import { RemoveDialog } from "./RemoveDialog";
import { ActivityCard, ErrorLogCard, ScheduleCard, SidePanel } from "./SidePanel";
import { rowState, type RowState } from "./syncState";

const BADGE: Record<RowState, { color: BadgeProps["color"]; appearance: BadgeProps["appearance"]; icon: ReactElement }> = {
  completed: { color: "success", appearance: "tint", icon: <CheckmarkCircleRegular /> },
  queued: { color: "informative", appearance: "outline", icon: <HourglassRegular /> },
  running: { color: "brand", appearance: "tint", icon: <ArrowSyncRegular /> },
  cancelling: { color: "warning", appearance: "tint", icon: <ArrowSyncRegular /> },
  locked: { color: "warning", appearance: "tint", icon: <LockClosedRegular /> },
  failed: { color: "danger", appearance: "tint", icon: <DismissCircleRegular /> },
  cancelled: { color: "subtle", appearance: "tint", icon: <DismissCircleRegular /> },
  removing: { color: "warning", appearance: "tint", icon: <DeleteRegular /> },
  disabled: { color: "subtle", appearance: "tint", icon: <PauseCircleRegular /> },
};
type ColumnKey = "name" | "size" | "messages" | "new" | "lastSync" | "nextSync" | "status" | "enabled" | "actions";
const COLUMNS: Record<ColumnKey, { width: number }> = {
  name: { width: 260 }, size: { width: 88 }, messages: { width: 88 }, new: { width: 88 }, lastSync: { width: 150 },
  nextSync: { width: 150 }, status: { width: 230 }, enabled: { width: 72 }, actions: { width: 84 },
};
const NUMERIC: ReadonlySet<TableColumnId> = new Set<ColumnKey>(["size", "messages", "new"]);
const column = (columnId: ColumnKey, def: Omit<Parameters<typeof createTableColumn<PstSource>>[0], "columnId">) =>
  createTableColumn<PstSource>({ ...def, columnId });

const useStyles = makeStyles({
  root: {
    display: "grid", gridTemplateRows: "auto 1fr", gridTemplateColumns: "minmax(0, 1fr) 360px", height: "100%", minHeight: 0,
  },
  cmdbar: {
    gridColumn: "1 / -1", display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS,
    padding: `${tokens.spacingVerticalS} ${tokens.spacingHorizontalL}`, backgroundColor: tokens.colorNeutralBackground1,
    borderBottom: `1px solid ${tokens.colorNeutralStroke2}`,
  },
  spacer: { flex: 1 },
  title: { marginRight: tokens.spacingHorizontalM },
  stat: {
    display: "flex", flexDirection: "column", padding: `0 ${tokens.spacingHorizontalM}`,
    borderLeft: `1px solid ${tokens.colorNeutralStroke2}`,
  },
  statValue: { fontWeight: tokens.fontWeightSemibold, fontSize: tokens.fontSizeBase400, fontVariantNumeric: "tabular-nums" },
  muted: { color: tokens.colorNeutralForeground3 },
  pane: {
    display: "flex", flexDirection: "column", minWidth: 0, minHeight: 0, overflow: "hidden",
    backgroundColor: tokens.colorNeutralBackground1,
  },
  side: { backgroundColor: tokens.colorNeutralBackground2, borderLeft: `1px solid ${tokens.colorNeutralStroke2}` },
  paneHead: {
    display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS, minHeight: "44px", boxSizing: "border-box",
    padding: `0 ${tokens.spacingHorizontalM}`, borderBottom: `1px solid ${tokens.colorNeutralStroke2}`,
  },
  scroll: { flex: 1, overflow: "auto", minHeight: 0 },
  cell: { whiteSpace: "nowrap" },
  num: { justifyContent: "flex-end", fontVariantNumeric: "tabular-nums" },
  numButton: { justifyContent: "flex-end" },
  rowSelected: { backgroundColor: tokens.colorBrandBackground2, ":hover": { backgroundColor: tokens.colorBrandBackground2 } },
  rowOff: { color: tokens.colorNeutralForeground4 },
  path: {
    fontFamily: tokens.fontFamilyMonospace, fontSize: tokens.fontSizeBase200, color: tokens.colorNeutralForeground3,
    ...ellipsis, display: "block",
  },
  when: { display: "flex", flexDirection: "column", lineHeight: "16px" },
  statusCell: { display: "flex", flexDirection: "column", gap: tokens.spacingVerticalXS, minWidth: 0, whiteSpace: "normal", alignItems: "flex-start", width: "100%" },
  badge: { flexShrink: 0, whiteSpace: "nowrap" },
  cap: { color: tokens.colorNeutralForeground3 },
  capWarn: { color: tokens.colorStatusWarningForeground1 },
  capErr: { color: tokens.colorStatusDangerForeground1, overflowWrap: "anywhere", userSelect: "text" },
  newPill: { color: tokens.colorStatusSuccessForeground1, fontWeight: tokens.fontWeightSemibold },
  acts: { display: "flex", alignItems: "center" },
  danger: {
    ":enabled": { color: tokens.colorStatusDangerForeground1 },
    ":enabled:hover,:enabled:hover:active,:enabled:active:focus-visible": {
      color: tokens.colorStatusDangerForeground1, [`& .${buttonClassNames.icon}`]: { color: tokens.colorStatusDangerForeground1 },
    },
  },
});

export function ImportScreen() {
  const s = useStyles();
  const { t, i18n } = useTranslation("import");
  const lng = i18n.language;
  const status = useAppStatus();
  const qc = useQueryClient();
  const toast = useAppToast();
  const psts = useQuery(pstListQuery);
  const sync = useQuery(syncStatusQuery);
  const db = useQuery(dbStatusQuery);
  const [selected, setSelected] = useState<number | null>(null);
  const [removing, setRemoving] = useState<PstSource | null>(null);
  const list = psts.data ?? [];
  const sizing = useColumnWidths("import-paths", COLUMNS);

  const register = useRegisterPst();
  const refresh = () =>
    Promise.all([
      qc.invalidateQueries({ queryKey: pstListQuery.queryKey }),
      qc.invalidateQueries({ queryKey: syncStatusQuery.queryKey }),
    ]);
  const act = (f: () => Promise<unknown>) => async () => {
    try {
      await f();
      await refresh();
    } catch (e) {
      toast.error(e);
    }
  };
  const addPaths = act(async () => {
    await register();
  });
  const nextRunAt = sync.data?.nextRunAt ?? null;
  const enabledCount = list.filter((p) => !p.paused).length;
  const live = (p: PstSource) => p.status === "running" || p.status === "queued" || p.status === "cancelling" || p.status === "removing";
  const when = (iso: string | null, sub?: string) => (
    <span className={s.when}>
      <span>{iso ? formatDateTime(iso, lng) : "—"}</span>
      {sub ? <Caption1 className={s.muted}>{sub}</Caption1> : null}
    </span>
  );

  const columns: TableColumnDefinition<PstSource>[] = [
    column("name", {
      compare: (a, b) => a.displayName.localeCompare(b.displayName),
      renderHeaderCell: () => t("col.name"),
      renderCell: (p) => (
        <TableCellLayout truncate media={<DocumentRegular />} description={{ className: s.path, title: p.path, children: p.path }}>
          <strong>{p.displayName}</strong>
        </TableCellLayout>
      ),
    }),
    column("size", {
      compare: (a, b) => a.fileSizeBytes - b.fileSizeBytes,
      renderHeaderCell: () => t("col.size"),
      renderCell: (p) => formatBytes(p.fileSizeBytes, lng),
    }),
    column("messages", {
      compare: (a, b) => a.messageCount - b.messageCount,
      renderHeaderCell: () => t("col.messages"),
      renderCell: (p) => formatNumber(p.messageCount, lng),
    }),
    column("new", {
      compare: (a, b) => a.newMessageCount - b.newMessageCount,
      renderHeaderCell: () => t("col.new"),
      renderCell: (p) =>
        p.newMessageCount > 0 ? <span className={s.newPill}>+{formatNumber(p.newMessageCount, lng)}</span> : formatNumber(0, lng),
    }),
    column("lastSync", {
      compare: (a, b) => (a.lastSyncedAt ?? "").localeCompare(b.lastSyncedAt ?? ""),
      renderHeaderCell: () => t("col.lastSync"),
      renderCell: (p) => when(p.lastSyncedAt),
    }),
    column("nextSync", {
      renderHeaderCell: () => t("col.nextSync"),
      renderCell: (p) => {
        const st = rowState(p);
        if (st === "running") return when(null, t("when.syncing"));
        if (st === "queued") return when(null, t("when.thisRun"));
        if (st === "disabled" || st === "removing") return when(null, t("when.notScheduled"));
        return nextRunAt ? when(nextRunAt, st === "locked" ? t("when.retrying") : undefined) : when(null, t("when.notScheduled"));
      },
    }),
    column("status", {
      compare: (a, b) => rowState(a).localeCompare(rowState(b)),
      renderHeaderCell: () => t("col.status"),
      renderCell: (p) => {
        const st = rowState(p);
        const pr = p.progress;
        const pct = pr?.itemsTotal ? Math.round((pr.itemsDone / pr.itemsTotal) * 100) : null;
        return (
          <span className={s.statusCell}>
            <Badge appearance={BADGE[st].appearance} color={BADGE[st].color} icon={BADGE[st].icon} className={s.badge} data-testid="pst-status" data-state={st}>
              {t(`status.${st}`)}
              {st === "running" && pct !== null ? ` · ${pct}%` : ""}
            </Badge>
            {st === "running" ? (
              <>
                <ProgressBar value={pct === null ? undefined : pct / 100} aria-label={p.displayName} />
                {pr ? (
                  <Caption1 className={s.cap}>
                    {t(`phase.${pr.phase}`)}
                    {pr.itemsTotal ? ` · ${formatNumber(pr.itemsDone, lng)} / ${formatNumber(pr.itemsTotal, lng)}` : ""}
                  </Caption1>
                ) : null}
              </>
            ) : null}
            {st === "queued" ? <Caption1 className={s.cap}>{t("cap.queued")}</Caption1> : null}
            {st === "locked" ? <Caption1 className={s.capWarn}>{t("cap.locked")}</Caption1> : null}
            {st === "failed" ? <Caption1 className={s.capErr}>{p.failureReason ?? t("cap.failed")}</Caption1> : null}
            {st === "cancelled" ? <Caption1 className={s.cap}>{t("cap.cancelled")}</Caption1> : null}
            {st === "disabled" ? <Caption1 className={s.cap}>{t("cap.disabled")}</Caption1> : null}
          </span>
        );
      },
    }),
    column("enabled", {
      renderHeaderCell: () => t("col.enabled"),
      renderCell: (p) => (
        <Switch
          checked={!p.paused}
          aria-label={t("row.enable", { name: p.displayName })}
          data-testid="pst-enabled"
          onChange={(_, d) => void act(() => api.pstSetPaused(p.id, !d.checked))()}
        />
      ),
    }),
    column("actions", {
      renderHeaderCell: () => "",
      renderCell: (p) => (
        <span className={s.acts}>
          <Button
            appearance="subtle"
            icon={<ArrowSyncRegular />}
            aria-label={t("row.syncNow", { name: p.displayName })}
            title={t(p.status === "failed" || p.status === "locked" ? "row.retry" : "row.syncOnce")}
            disabled={live(p)}
            data-testid="pst-sync"
            onClick={(e) => {
              e.stopPropagation();
              void act(() => api.syncNow(p.id))();
            }}
          />
          <Button
            appearance="subtle"
            className={s.danger}
            icon={<DeleteRegular />}
            aria-label={t("row.remove", { name: p.displayName })}
            disabled={p.status === "running" || p.status === "cancelling" || p.status === "removing"}
            data-testid="pst-remove"
            onClick={(e) => {
              e.stopPropagation();
              setRemoving(p);
            }}
          />
        </span>
      ),
    }),
  ];

  const colClass = (id: TableColumnId) => mergeClasses(s.cell, NUMERIC.has(id) && s.num);
  const stat = (value: string, label: string) => (
    <span className={s.stat}>
      <span className={s.statValue}>{value}</span>
      <Caption1 className={s.muted}>{label}</Caption1>
    </span>
  );
  const sel = list.find((p) => p.id === selected) ?? null;
  const empty = psts.isSuccess && list.length === 0;

  return (
    <div className={s.root}>
      <div className={s.cmdbar}>
        <Subtitle2 as="h2" className={s.title}>
          {t("title")}
        </Subtitle2>
        <Button appearance="primary" icon={<AddRegular />} onClick={addPaths} data-testid="import-add">
          {t("add")}
        </Button>
        {empty ? null : (
          <Button icon={<ArrowSyncRegular />} onClick={act(() => api.syncNow(null))} data-testid="sync-all">
            {t("syncAll")}
          </Button>
        )}
        <span className={s.spacer} />
        {stat(formatNumber(status.counts.messages, lng), t("stats.messages"))}
        {stat(formatNumber(status.counts.psts, lng), t("stats.psts"))}
        {stat(db.data ? formatBytes(db.data.databaseSizeBytes, lng) : t("stats.unknown"), t("stats.size"))}
      </div>
      <section className={s.pane} aria-label={t("paths.title")}>
        {empty ? (
          <EmptyState
            icon={<MailInboxRegular />}
            title={t("empty.title")}
            body={t("empty.body")}
            action={
              <Button appearance="primary" icon={<AddRegular />} onClick={addPaths}>
                {t("add")}
              </Button>
            }
          />
        ) : (
          <>
            <div className={s.paneHead}>
              <Body1Strong as="h3">{t("paths.title")}</Body1Strong>
              <Caption1 className={s.muted}>{t("paths.summary", { n: formatNumber(list.length, lng), enabled: formatNumber(enabledCount, lng) })}</Caption1>
              <span className={s.spacer} />
              <Caption1 className={s.muted}>{t("paths.note")}</Caption1>
            </div>
            <div className={s.scroll}>
              <DataGrid {...sizing} items={list} columns={columns} getRowId={(p) => p.id} sortable>
                <DataGridHeader>
                  <DataGridRow>
                    {({ renderHeaderCell, columnId }) => (
                      <DataGridHeaderCell className={colClass(columnId)} button={{ className: NUMERIC.has(columnId) ? s.numButton : undefined }}>
                        {renderHeaderCell()}
                      </DataGridHeaderCell>
                    )}
                  </DataGridRow>
                </DataGridHeader>
                <DataGridBody<PstSource>>
                  {({ item, rowId }) => (
                    <DataGridRow<PstSource>
                      key={rowId}
                      data-testid="pst-row"
                      data-pst-name={item.displayName}
                      aria-selected={item.id === selected}
                      className={mergeClasses(item.id === selected && s.rowSelected, rowState(item) === "disabled" && s.rowOff)}
                      onClick={() => setSelected(item.id)}
                    >
                      {({ renderCell, columnId }) => <DataGridCell className={colClass(columnId)}>{renderCell(item)}</DataGridCell>}
                    </DataGridRow>
                  )}
                </DataGridBody>
              </DataGrid>
            </div>
          </>
        )}
      </section>
      <aside className={mergeClasses(s.pane, s.side)} aria-label={t("sched.title")}>
          <div className={s.scroll}>
            <SidePanel>
              {sync.data ? <ScheduleCard key={JSON.stringify(sync.data.schedule)} status={sync.data} /> : null}
              <ActivityCard list={list} status={sync.data} onCancel={(id) => void act(() => api.pstSetPaused(id, true))()} />
              {sel && (sel.errorCount > 0 || sel.status === "failed") ? <ErrorLogCard key={sel.id} pst={sel} /> : null}
            </SidePanel>
          </div>
      </aside>
      {removing ? <RemoveDialog pst={removing} onClose={() => setRemoving(null)} /> : null}
    </div>
  );
}
