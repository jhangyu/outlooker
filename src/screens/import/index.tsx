import {
  Badge, Body1, Button, Caption1, DataGrid, DataGridBody, DataGridCell, DataGridHeader, DataGridHeaderCell, DataGridRow,
  ProgressBar, Subtitle2, TableCellLayout, Title3, createTableColumn, makeStyles, mergeClasses, tokens,
  type BadgeProps, type TableColumnDefinition,
} from "@fluentui/react-components";
import { AddRegular, DeleteRegular, DocumentRegular, ErrorCircleRegular, MailInboxRegular, WarningRegular } from "@fluentui/react-icons";
import { useInfiniteQuery, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../../api/commands";
import { pickPstFiles } from "../../api/dialog";
import type { ImportStatus } from "../../api/bindings/ImportStatus";
import type { PstSource } from "../../api/bindings/PstSource";
import { useAppStatus } from "../../app/AppStatusContext";
import {
  EmptyState, formatAppError, formatBytes, formatDateTime, formatNumber, useAppToast,
} from "../../components/common";
import { RemoveDialog } from "./RemoveDialog";

const IMPORTED: ImportStatus[] = ["completed", "failed", "cancelled", "removing"];
const QUEUED: ImportStatus[] = ["queued", "running", "cancelling", "cancelled", "failed"];
const BADGE: Record<ImportStatus, BadgeProps["color"]> = {
  queued: "informative", running: "brand", cancelling: "warning", cancelled: "subtle",
  failed: "danger", completed: "success", removing: "warning",
};

const useStyles = makeStyles({
  root: { display: "grid", gridTemplateRows: "auto 1fr", gridTemplateColumns: "1.6fr 1fr 1fr", height: "100%", minHeight: 0 },
  cmdbar: {
    gridColumn: "1 / -1", display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS,
    padding: `${tokens.spacingVerticalS} ${tokens.spacingHorizontalL}`, backgroundColor: tokens.colorNeutralBackground1,
    borderBottom: `1px solid ${tokens.colorNeutralStroke2}`,
  },
  spacer: { flex: 1 },
  stat: { display: "flex", flexDirection: "column", alignItems: "flex-end", paddingLeft: tokens.spacingHorizontalL },
  statValue: { fontWeight: tokens.fontWeightSemibold, fontSize: tokens.fontSizeBase400 },
  pane: {
    display: "flex", flexDirection: "column", minWidth: 0, minHeight: 0, overflow: "hidden",
    backgroundColor: tokens.colorNeutralBackground1, borderLeft: `1px solid ${tokens.colorNeutralStroke2}`,
  },
  paneHead: {
    display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS, minHeight: "44px", boxSizing: "border-box",
    padding: `0 ${tokens.spacingHorizontalM}`, borderBottom: `1px solid ${tokens.colorNeutralStroke2}`,
  },
  scroll: { flex: 1, overflow: "auto", minHeight: 0 },
  full: { gridColumn: "1 / -1" },
  path: { fontFamily: tokens.fontFamilyMonospace, color: tokens.colorNeutralForeground3, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", display: "block", maxWidth: "280px" },
  card: {
    display: "flex", flexDirection: "column", gap: tokens.spacingVerticalXS, padding: tokens.spacingHorizontalM,
    borderBottom: `1px solid ${tokens.colorNeutralStroke3}`, cursor: "pointer", position: "relative",
  },
  selected: { backgroundColor: tokens.colorBrandBackground2, boxShadow: `inset 3px 0 0 ${tokens.colorCompoundBrandStroke}` },
  row: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS },
  name: { flex: 1, fontWeight: tokens.fontWeightSemibold, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" },
  sub: { display: "flex", gap: tokens.spacingHorizontalM, color: tokens.colorNeutralForeground3, fontSize: tokens.fontSizeBase200 },
  warn: { color: tokens.colorStatusWarningForeground1 },
  err: { color: tokens.colorStatusDangerForeground1 },
  logEntry: {
    display: "grid", gridTemplateColumns: "auto 20px 1fr", gap: `${tokens.spacingVerticalXS} ${tokens.spacingHorizontalS}`,
    padding: `${tokens.spacingVerticalS} ${tokens.spacingHorizontalM}`, borderBottom: `1px solid ${tokens.colorNeutralStroke3}`,
  },
  logLoc: { gridColumn: "3", color: tokens.colorNeutralForeground3, fontSize: tokens.fontSizeBase200 },
});

export function ImportScreen() {
  const s = useStyles();
  const { t, i18n } = useTranslation("import");
  const lng = i18n.language;
  const status = useAppStatus();
  const qc = useQueryClient();
  const toast = useAppToast();
  const psts = useQuery({ queryKey: ["pstList"], queryFn: () => api.pstList() });
  const db = useQuery({ queryKey: ["dbStatus"], queryFn: () => api.dbStatus(), refetchInterval: 10_000 });
  const [selected, setSelected] = useState<number | null>(null);
  const [removing, setRemoving] = useState<PstSource | null>(null);
  const list = psts.data ?? [];

  const refresh = () => qc.invalidateQueries({ queryKey: ["pstList"] });
  const addFiles = async () => {
    const paths = await pickPstFiles();
    if (paths.length === 0) return;
    try {
      const results = await api.importEnqueue(paths);
      for (const r of results)
        if (r.result === "rejected") toast.notify(t("enqueue.rejected", { path: r.path, reason: formatAppError(r.error, t) }), "error");
      await refresh();
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
    createTableColumn<PstSource>({
      columnId: "name",
      compare: (a, b) => a.displayName.localeCompare(b.displayName),
      renderHeaderCell: () => t("imported.name"),
      renderCell: (p) => (
        <TableCellLayout media={<DocumentRegular />}>
          <strong>{p.displayName}</strong>
          <span className={s.path} title={p.path}>
            {p.path}
          </span>
        </TableCellLayout>
      ),
    }),
    createTableColumn<PstSource>({
      columnId: "size",
      compare: (a, b) => a.fileSizeBytes - b.fileSizeBytes,
      renderHeaderCell: () => t("imported.size"),
      renderCell: (p) => formatBytes(p.fileSizeBytes, lng),
    }),
    createTableColumn<PstSource>({
      columnId: "messages",
      compare: (a, b) => a.messageCount - b.messageCount,
      renderHeaderCell: () => t("imported.messages"),
      renderCell: (p) => formatNumber(p.messageCount, lng),
    }),
    createTableColumn<PstSource>({
      columnId: "new",
      compare: (a, b) => a.newMessageCount - b.newMessageCount,
      renderHeaderCell: () => t("imported.new"),
      renderCell: (p) => formatNumber(p.newMessageCount, lng),
    }),
    createTableColumn<PstSource>({
      columnId: "date",
      compare: (a, b) => (a.finishedAt ?? a.queuedAt).localeCompare(b.finishedAt ?? b.queuedAt),
      renderHeaderCell: () => t("imported.date"),
      renderCell: (p) => formatDateTime(p.finishedAt ?? p.queuedAt, lng),
    }),
    createTableColumn<PstSource>({
      columnId: "status",
      compare: (a, b) => a.status.localeCompare(b.status),
      renderHeaderCell: () => t("imported.status"),
      renderCell: (p) => (
        <span className={s.row}>
          <Badge appearance="tint" color={BADGE[p.status]} data-testid="pst-status">
            {t(`status.${p.status}`)}
          </Badge>
          {p.errorCount > 0 ? (
            <WarningRegular className={s.warn} aria-label={t("queue.errors", { n: formatNumber(p.errorCount, lng) })} />
          ) : null}
        </span>
      ),
    }),
    createTableColumn<PstSource>({
      columnId: "remove",
      renderHeaderCell: () => "",
      renderCell: (p) => (
        <Button
          appearance="subtle"
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

  const stat = (value: string, label: string) => (
    <span className={s.stat}>
      <span className={s.statValue}>{value}</span>
      <Caption1>{label}</Caption1>
    </span>
  );

  return (
    <div className={s.root}>
      <div className={s.cmdbar}>
        <Title3 as="h2">{t("title")}</Title3>
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
              <Subtitle2 as="h3">{t("imported.title")}</Subtitle2>
            </div>
            <div className={s.scroll}>
              <DataGrid items={list.filter((p) => IMPORTED.includes(p.status))} columns={columns} getRowId={(p) => p.id} sortable>
                <DataGridHeader>
                  <DataGridRow>{({ renderHeaderCell }) => <DataGridHeaderCell>{renderHeaderCell()}</DataGridHeaderCell>}</DataGridRow>
                </DataGridHeader>
                <DataGridBody<PstSource>>
                  {({ item, rowId }) => (
                    <DataGridRow<PstSource>
                      key={rowId}
                      data-testid="pst-row"
                      data-pst-name={item.displayName}
                      onClick={() => setSelected(item.id)}
                    >
                      {({ renderCell }) => <DataGridCell>{renderCell(item)}</DataGridCell>}
                    </DataGridRow>
                  )}
                </DataGridBody>
              </DataGrid>
            </div>
          </section>
          <section className={s.pane} aria-label={t("queue.title")}>
            <div className={s.paneHead}>
              <Subtitle2 as="h3">{t("queue.title")}</Subtitle2>
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
      {running && p ? <Caption1>{t(`phase.${p.phase}`)}</Caption1> : null}
      {running || p ? (
        <ProgressBar
          value={running && total === null ? undefined : total ? done / total : 0}
          color={pst.status === "failed" ? "error" : "brand"}
          aria-label={pst.displayName}
        />
      ) : null}
      <div className={s.sub}>
        {total ? <span>{t("queue.progress", { done: formatNumber(done, lng), total: formatNumber(total, lng), percent })}</span> : null}
        {running && p?.bytesPerSec ? <span>{t("queue.rate", { rate: formatBytes(p.bytesPerSec, lng) })}</span> : null}
        {running && p?.etaSeconds ? <span>{t("queue.eta", { minutes: Math.max(1, Math.round(p.etaSeconds / 60)) })}</span> : null}
        {pst.errorCount > 0 ? (
          <span className={s.warn}>
            <WarningRegular /> {t("queue.errors", { n: formatNumber(pst.errorCount, lng) })}
          </span>
        ) : null}
      </div>
      <div className={s.row} onClick={(e) => e.stopPropagation()}>
        {pst.status === "running" || pst.status === "queued" || pst.status === "cancelling" ? (
          <Button size="small" disabled={pst.status === "cancelling"} onClick={props.onCancel} data-testid="queue-cancel">
            {t("queue.cancel")}
          </Button>
        ) : null}
        {pst.status === "cancelled" ? (
          <Button size="small" appearance="primary" onClick={props.onResume} data-testid="queue-resume">
            {t("queue.resume")}
          </Button>
        ) : null}
        {pst.status === "failed" ? (
          <Button size="small" appearance="primary" onClick={props.onResume} data-testid="queue-retry">
            {t("queue.retry")}
          </Button>
        ) : null}
        {pst.status === "cancelled" || pst.status === "failed" ? (
          <Button size="small" appearance="subtle" icon={<DeleteRegular />} onClick={props.onRemove}>
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
        <Subtitle2 as="h3">{t("log.title")}</Subtitle2>
        {pst ? <Caption1>— {pst.displayName}</Caption1> : null}
      </div>
      <div className={s.scroll}>
        {pst === null ? (
          <EmptyState icon={<DocumentRegular />} title={t("log.pick")} />
        ) : errors.isSuccess && rows.length === 0 ? (
          <EmptyState icon={<DocumentRegular />} title={t("log.none")} />
        ) : (
          rows.map((r) => (
            <div key={r.id} className={s.logEntry}>
              <Caption1>{formatDateTime(r.occurredAt, i18n.language)}</Caption1>
              {r.severity === "jobFailed" ? <ErrorCircleRegular className={s.err} /> : <WarningRegular className={s.warn} />}
              <Body1>
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
