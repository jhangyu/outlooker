import {
  Badge, Body1, Button, Caption1, DataGrid, DataGridBody, DataGridCell, DataGridHeader, DataGridHeaderCell, DataGridRow,
  Body1Strong, ProgressBar, Subtitle2, TableCellLayout, createTableColumn, makeStyles, mergeClasses, tokens,
  buttonClassNames, type BadgeProps, type CreateTableColumnOptions, type TableColumnDefinition, type TableColumnId,
} from "@fluentui/react-components";
import {
  AddRegular, DeleteRegular, DismissRegular, DocumentRegular, ErrorCircleRegular, MailInboxRegular, PlayRegular, WarningRegular,
} from "@fluentui/react-icons";
import { useInfiniteQuery, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../../api/commands";
import { dbStatusQuery, pstListQuery } from "../../api/queries";
import type { ImportStatus } from "../../api/bindings/ImportStatus";
import type { PstSource } from "../../api/bindings/PstSource";
import { useAppStatus } from "../../app/AppStatusContext";
import {
  EmptyState, ellipsis, formatBytes, formatDateTime, formatNumber, useAppToast, useColumnWidths, useEnqueuePst,
} from "../../components/common";
import { RemoveDialog } from "./RemoveDialog";

const IMPORTED: ImportStatus[] = ["completed", "failed", "cancelled", "removing"];
const QUEUED: ImportStatus[] = ["queued", "running", "cancelling", "cancelled", "failed"];
const BADGE: Record<ImportStatus, BadgeProps["color"]> = {
  queued: "informative", running: "brand", cancelling: "warning", cancelled: "subtle",
  failed: "danger", completed: "success", removing: "warning",
};
type ColumnKey = "name" | "size" | "messages" | "new" | "date" | "status" | "remove";
const FILE_COLUMNS: Record<ColumnKey, { width: number }> = {
  name: { width: 220 }, size: { width: 88 }, messages: { width: 88 }, new: { width: 72 },
  date: { width: 192 }, status: { width: 128 }, remove: { width: 48 },
};
const NUMERIC: ReadonlySet<TableColumnId> = new Set<ColumnKey>(["size", "messages", "new"]);
const column = (columnId: ColumnKey, def: Omit<CreateTableColumnOptions<PstSource>, "columnId">) =>
  createTableColumn<PstSource>({ ...def, columnId });

const useStyles = makeStyles({
  root: { display: "grid", gridTemplateRows: "auto 1fr", gridTemplateColumns: "1.6fr 1fr 1fr", height: "100%", minHeight: 0 },
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
    backgroundColor: tokens.colorNeutralBackground1, borderLeft: `1px solid ${tokens.colorNeutralStroke2}`,
    // The rail already draws the border left of the first pane (mockup `.pane + .pane`).
    ":first-of-type": { borderLeft: "none" },
  },
  paneHead: {
    display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS, minHeight: "44px", boxSizing: "border-box",
    padding: `0 ${tokens.spacingHorizontalM}`, borderBottom: `1px solid ${tokens.colorNeutralStroke2}`,
  },
  paneTitle: { flexShrink: 0 },
  paneFor: { ...ellipsis, color: tokens.colorNeutralForeground3 },
  scroll: { flex: 1, overflow: "auto", minHeight: 0 },
  full: { gridColumn: "1 / -1", borderLeft: "none" },
  cell: { whiteSpace: "nowrap" },
  num: { justifyContent: "flex-end", fontVariantNumeric: "tabular-nums" },
  // The sortable header label sits in a full-width flex button; right-align inside it too.
  numButton: { justifyContent: "flex-end" },
  rowSelected: { backgroundColor: tokens.colorBrandBackground2, ":hover": { backgroundColor: tokens.colorBrandBackground2 } },
  badge: { flexShrink: 0, whiteSpace: "nowrap" },
  path: {
    fontFamily: tokens.fontFamilyMonospace, fontSize: tokens.fontSizeBase200, color: tokens.colorNeutralForeground3,
    ...ellipsis, display: "block",
  },
  card: {
    display: "flex", flexDirection: "column", gap: tokens.spacingVerticalXS, padding: tokens.spacingHorizontalM,
    borderBottom: `1px solid ${tokens.colorNeutralStroke3}`, cursor: "pointer", position: "relative",
  },
  selected: { backgroundColor: tokens.colorBrandBackground2, boxShadow: `inset 3px 0 0 ${tokens.colorCompoundBrandStroke}` },
  row: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS },
  acts: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalXS },
  danger: {
    ":enabled": { color: tokens.colorStatusDangerForeground1 },
    ":enabled:hover,:enabled:hover:active,:enabled:active:focus-visible": {
      color: tokens.colorStatusDangerForeground1, [`& .${buttonClassNames.icon}`]: { color: tokens.colorStatusDangerForeground1 },
    },
  },
  name: { flex: 1, fontWeight: tokens.fontWeightSemibold, ...ellipsis },
  sub: {
    display: "flex", flexWrap: "wrap", gap: tokens.spacingHorizontalM, rowGap: "2px", color: tokens.colorNeutralForeground3,
    fontSize: tokens.fontSizeBase200, "& > span": { whiteSpace: "nowrap" },
  },
  warn: { color: tokens.colorStatusWarningForeground1 },
  warnInline: { display: "inline-flex", alignItems: "center", gap: tokens.spacingHorizontalXXS },
  err: { color: tokens.colorStatusDangerForeground1 },
  logEntry: {
    // 168px fits the longest formatDateTime output at 12px mono ("2026年10月22日 中午12:30", "Oct 22, 2026, 12:30 PM").
    display: "grid", gridTemplateColumns: "168px 20px 1fr", gap: `${tokens.spacingVerticalXS} ${tokens.spacingHorizontalS}`,
    padding: `${tokens.spacingVerticalS} ${tokens.spacingHorizontalM}`, borderBottom: `1px solid ${tokens.colorNeutralStroke3}`,
  },
  logTime: { fontFamily: tokens.fontFamilyMonospace, color: tokens.colorNeutralForeground3, whiteSpace: "nowrap" },
  logMsg: { minWidth: 0, overflowWrap: "anywhere" },
  logLoc: {
    gridColumn: "3", fontFamily: tokens.fontFamilyMonospace, wordBreak: "break-all", color: tokens.colorNeutralForeground3,
    fontSize: tokens.fontSizeBase200,
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
  const db = useQuery(dbStatusQuery);
  const [selected, setSelected] = useState<number | null>(null);
  const [removing, setRemoving] = useState<PstSource | null>(null);
  const list = psts.data ?? [];
  const sizing = useColumnWidths("import-files", FILE_COLUMNS);

  const enqueue = useEnqueuePst();
  const refresh = () => qc.invalidateQueries({ queryKey: pstListQuery.queryKey });
  const addFiles = async () => {
    try {
      if (await enqueue()) await refresh();
    } catch (e) {
      toast.error(e);
    }
  };
  const run = (f: (id: number) => Promise<unknown>, id: number) => async () => {
    try {
      await f(id);
      await refresh();
    } catch (e) {
      toast.error(e);
    }
  };

  const columns: TableColumnDefinition<PstSource>[] = [
    column("name", {
      compare: (a, b) => a.displayName.localeCompare(b.displayName),
      renderHeaderCell: () => t("imported.name"),
      renderCell: (p) => (
        <TableCellLayout truncate media={<DocumentRegular />} description={{ className: s.path, title: p.path, children: p.path }}>
          <strong>{p.displayName}</strong>
        </TableCellLayout>
      ),
    }),
    column("size", {
      compare: (a, b) => a.fileSizeBytes - b.fileSizeBytes,
      renderHeaderCell: () => t("imported.size"),
      renderCell: (p) => formatBytes(p.fileSizeBytes, lng),
    }),
    column("messages", {
      compare: (a, b) => a.messageCount - b.messageCount,
      renderHeaderCell: () => t("imported.messages"),
      renderCell: (p) => formatNumber(p.messageCount, lng),
    }),
    column("new", {
      compare: (a, b) => a.newMessageCount - b.newMessageCount,
      renderHeaderCell: () => t("imported.new"),
      renderCell: (p) => formatNumber(p.newMessageCount, lng),
    }),
    column("date", {
      compare: (a, b) => (a.finishedAt ?? a.queuedAt).localeCompare(b.finishedAt ?? b.queuedAt),
      renderHeaderCell: () => t("imported.date"),
      renderCell: (p) => formatDateTime(p.finishedAt ?? p.queuedAt, lng),
    }),
    column("status", {
      compare: (a, b) => a.status.localeCompare(b.status),
      renderHeaderCell: () => t("imported.status"),
      renderCell: (p) => (
        <span className={s.row}>
          <Badge appearance="tint" color={BADGE[p.status]} className={s.badge} data-testid="pst-status">
            {t(`status.${p.status}`)}
          </Badge>
          {p.errorCount > 0 ? (
            <WarningRegular className={s.warn} aria-label={t("queue.errors", { n: formatNumber(p.errorCount, lng) })} />
          ) : null}
        </span>
      ),
    }),
    column("remove", {
      renderHeaderCell: () => "",
      renderCell: (p) => (
        <Button
          appearance="subtle"
          className={s.danger}
          icon={<DeleteRegular />}
          aria-label={t("imported.remove", { name: p.displayName })}
          disabled={p.status === "removing"}
          data-testid="pst-remove"
          onClick={(e) => {
            e.stopPropagation();
            setRemoving(p);
          }}
        />
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

  return (
    <div className={s.root}>
      <div className={s.cmdbar}>
        <Subtitle2 as="h2" className={s.title}>
          {t("title")}
        </Subtitle2>
        <Button appearance="primary" icon={<AddRegular />} onClick={addFiles} data-testid="import-add">
          {t("add")}
        </Button>
        <span className={s.spacer} />
        {stat(formatNumber(status.counts.messages, lng), t("stats.messages"))}
        {stat(formatNumber(status.counts.psts, lng), t("stats.psts"))}
        {stat(db.data ? formatBytes(db.data.databaseSizeBytes, lng) : t("stats.unknown"), t("stats.size"))}
      </div>
      {psts.isSuccess && list.length === 0 ? (
        <div className={mergeClasses(s.pane, s.full)}>
          <EmptyState
            icon={<MailInboxRegular />}
            title={t("empty.title")}
            body={t("empty.body")}
            action={
              <Button appearance="primary" icon={<AddRegular />} onClick={addFiles}>
                {t("add")}
              </Button>
            }
          />
        </div>
      ) : (
        <>
          <section className={s.pane} aria-label={t("imported.title")}>
            <div className={s.paneHead}>
              <Body1Strong as="h3">{t("imported.title")}</Body1Strong>
            </div>
            <div className={s.scroll}>
              <DataGrid
                {...sizing}
                items={list.filter((p) => IMPORTED.includes(p.status))}
                columns={columns}
                getRowId={(p) => p.id}
                sortable
              >
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
                      className={item.id === selected ? s.rowSelected : undefined}
                      onClick={() => setSelected(item.id)}
                    >
                      {({ renderCell, columnId }) => <DataGridCell className={colClass(columnId)}>{renderCell(item)}</DataGridCell>}
                    </DataGridRow>
                  )}
                </DataGridBody>
              </DataGrid>
            </div>
          </section>
          <section className={s.pane} aria-label={t("queue.title")}>
            <div className={s.paneHead}>
              <Body1Strong as="h3">{t("queue.title")}</Body1Strong>
            </div>
            <div className={s.scroll} role="listbox" aria-label={t("queue.title")}>
              {list.filter((p) => QUEUED.includes(p.status)).length === 0 ? (
                <EmptyState icon={<MailInboxRegular />} title={t("queue.empty")} body={t("queue.emptyBody")} />
              ) : (
                list
                  .filter((p) => QUEUED.includes(p.status))
                  .map((p) => (
                    <QueueCard
                      key={p.id}
                      pst={p}
                      selected={selected === p.id}
                      onSelect={() => setSelected(p.id)}
                      onCancel={run(api.importCancel, p.id)}
                      onResume={run(api.importResume, p.id)}
                      onRemove={() => setRemoving(p)}
                    />
                  ))
              )}
            </div>
          </section>
          <ErrorLog pst={list.find((p) => p.id === selected) ?? null} />
        </>
      )}
      {removing ? <RemoveDialog pst={removing} onClose={() => setRemoving(null)} /> : null}
    </div>
  );
}

function QueueCard(props: {
  pst: PstSource;
  selected: boolean;
  onSelect(): void;
  onCancel(): void;
  onResume(): void;
  onRemove(): void;
}) {
  const s = useStyles();
  const { t, i18n } = useTranslation("import");
  const lng = i18n.language;
  const { pst } = props;
  const p = pst.progress;
  const total = p?.itemsTotal ?? null;
  const done = p?.itemsDone ?? 0;
  const percent = total ? Math.round((done / total) * 100) : 0;
  const running = pst.status === "running";
  return (
    <div
      role="option"
      aria-selected={props.selected}
      tabIndex={0}
      data-testid="queue-card"
      data-pst-name={pst.displayName}
      className={mergeClasses(s.card, props.selected && s.selected)}
      onClick={props.onSelect}
      onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && props.onSelect()}
    >
      <div className={s.row}>
        <DocumentRegular />
        <span className={s.name}>{pst.displayName}</span>
        <Badge appearance="tint" color={BADGE[pst.status]}>
          {t(`status.${pst.status}`)}
        </Badge>
      </div>
      {(running && p) || total ? (
        <div className={s.sub}>
          {running && p ? <span>{t(`phase.${p.phase}`)}</span> : null}
          <span className={s.spacer} />
          {total ? <span>{t("queue.progress", { done: formatNumber(done, lng), total: formatNumber(total, lng), percent })}</span> : null}
        </div>
      ) : null}
      {running || p ? (
        <ProgressBar
          value={running && total === null ? undefined : total ? done / total : 0}
          color={pst.status === "failed" ? "error" : "brand"}
          aria-label={pst.displayName}
        />
      ) : null}
      <div className={s.sub}>
        {running && p?.bytesPerSec ? <span>{t("queue.rate", { rate: formatBytes(p.bytesPerSec, lng) })}</span> : null}
        {running && p?.etaSeconds ? <span>{t("queue.eta", { minutes: Math.max(1, Math.round(p.etaSeconds / 60)) })}</span> : null}
        {pst.errorCount > 0 ? (
          <span className={mergeClasses(s.warn, s.warnInline)}>
            <WarningRegular />
            {t("queue.errors", { n: formatNumber(pst.errorCount, lng) })}
          </span>
        ) : null}
      </div>
      <div className={s.acts} onClick={(e) => e.stopPropagation()}>
        {pst.status === "running" || pst.status === "queued" || pst.status === "cancelling" ? (
          <Button size="small" icon={<DismissRegular />} disabled={pst.status === "cancelling"} onClick={props.onCancel} data-testid="queue-cancel">
            {t("queue.cancel")}
          </Button>
        ) : null}
        {pst.status === "cancelled" ? (
          <Button size="small" appearance="primary" icon={<PlayRegular />} onClick={props.onResume} data-testid="queue-resume">
            {t("queue.resume")}
          </Button>
        ) : null}
        {pst.status === "failed" ? (
          <Button size="small" appearance="primary" icon={<PlayRegular />} onClick={props.onResume} data-testid="queue-retry">
            {t("queue.retry")}
          </Button>
        ) : null}
        {pst.status === "cancelled" || pst.status === "failed" ? (
          <Button size="small" appearance="subtle" className={s.danger} icon={<DeleteRegular />} onClick={props.onRemove}>
            {t("queue.remove")}
          </Button>
        ) : null}
      </div>
    </div>
  );
}

function ErrorLog({ pst }: { pst: PstSource | null }) {
  const s = useStyles();
  const { t, i18n } = useTranslation("import");
  const errors = useInfiniteQuery({
    queryKey: ["importErrors", pst?.id ?? null],
    queryFn: ({ pageParam }) => api.importErrors({ pstId: pst!.id, cursor: pageParam, limit: 100 }),
    initialPageParam: null as string | null,
    getNextPageParam: (last) => last.nextCursor ?? undefined,
    enabled: pst !== null,
  });
  const rows = errors.data?.pages.flatMap((p) => p.rows) ?? [];
  return (
    <section className={s.pane} aria-label={t("log.title")}>
      <div className={s.paneHead}>
        <Body1Strong as="h3" className={s.paneTitle}>
          {t("log.title")}
        </Body1Strong>
        {pst ? (
          <Caption1 className={s.paneFor} title={pst.displayName}>
            — {pst.displayName}
          </Caption1>
        ) : null}
      </div>
      <div className={s.scroll}>
        {pst === null ? (
          <EmptyState icon={<DocumentRegular />} title={t("log.pick")} />
        ) : errors.isSuccess && rows.length === 0 ? (
          <EmptyState icon={<DocumentRegular />} title={t("log.none")} />
        ) : (
          rows.map((r) => (
            <div key={r.id} className={s.logEntry}>
              <Caption1 className={s.logTime}>{formatDateTime(r.occurredAt, i18n.language)}</Caption1>
              {r.severity === "jobFailed" ? <ErrorCircleRegular className={s.err} /> : <WarningRegular className={s.warn} />}
              <Body1 className={s.logMsg}>
                {t(`stage.${r.stage}`)} · {r.message}
              </Body1>
              <span className={s.logLoc}>
                {[r.folderPath, r.pstItemId !== null ? t("log.item", { id: r.pstItemId }) : null].filter(Boolean).join(" · ")}
              </span>
            </div>
          ))
        )}
        {errors.hasNextPage ? (
          <Button appearance="subtle" onClick={() => void errors.fetchNextPage()}>
            {t("log.more")}
          </Button>
        ) : null}
      </div>
    </section>
  );
}
