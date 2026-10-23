import {
  Badge, Button, Caption1, Dialog, DialogActions, DialogBody, DialogContent, DialogSurface, DialogTitle, Field, Input,
  Body1Strong, MessageBar, MessageBarBody, Toolbar, ToolbarButton, makeStyles, tokens,
} from "@fluentui/react-components";
import { DatabaseRegular, DeleteRegular, LockClosedRegular, PlayRegular, SaveRegular, StopRegular, TableRegular } from "@fluentui/react-icons";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../../api/commands";
import type { AppError } from "../../api/bindings/AppError";
import type { SavedQuery } from "../../api/bindings/SavedQuery";
import type { SqlExecuteRequest } from "../../api/bindings/SqlExecuteRequest";
import type { SqlResult } from "../../api/bindings/SqlResult";
import { useAppStatus, useNavigation } from "../../app/AppStatusContext";
import {
  EmptyState, Splitter, ellipsis, formatAppError, formatNumber, toAppError, useAppToast, useSplit, type SplitDefaults,
} from "../../components/common";
import { MailList, ReadingPane } from "../../components/mail";
import { SqlEditor } from "../../components/sql-editor";
import { ResultTable } from "./ResultTable";
import { SavedQueries } from "./SavedQueries";

/** Part 03 §6.2 maximum; the screen always asks for it. */
export const MAX_ROWS = 10_000;
const noop = () => {};

// Sized pane = the left column of each border; the default is the 1:3:1:3 ratio (fr tracks).
const SPLITS: SplitDefaults = { c1: { min: 160, group: "cols" }, c2: { min: 240, group: "cols" }, c3: { min: 160, group: "cols" } };

const useStyles = makeStyles({
  root: { position: "relative", display: "grid", height: "100%", minHeight: 0 },
  // `:last-of-type`, not `:last-child`: an open dialog's portal anchor can follow the last column.
  col: {
    display: "flex", flexDirection: "column", minWidth: 0, minHeight: 0, borderRight: `1px solid ${tokens.colorNeutralStroke2}`,
    ":last-of-type": { borderRight: "none" },
  },
  toolbar: { borderBottom: `1px solid ${tokens.colorNeutralStroke2}`, gap: tokens.spacingHorizontalXS, overflow: "hidden" },
  tool: { minWidth: "auto" },
  badge: { flexShrink: 0, whiteSpace: "nowrap" },
  spacer: { flex: 1 },
  editor: { flex: 1, minHeight: 0 },
  status: {
    display: "flex", gap: tokens.spacingHorizontalM, padding: `${tokens.spacingVerticalXS} ${tokens.spacingHorizontalM}`,
    borderTop: `1px solid ${tokens.colorNeutralStroke2}`, alignItems: "center", backgroundColor: tokens.colorNeutralBackground2,
    color: tokens.colorNeutralForeground2, whiteSpace: "nowrap", overflow: "hidden",
  },
  statusText: { ...ellipsis },
  head: {
    display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS, minHeight: "44px", boxSizing: "border-box",
    padding: `0 ${tokens.spacingHorizontalM}`, borderBottom: `1px solid ${tokens.colorNeutralStroke2}`,
  },
  title: { ...ellipsis },
  count: { flexShrink: 0, whiteSpace: "nowrap", color: tokens.colorNeutralForeground3 },
  note: { display: "block", color: tokens.colorNeutralForeground3, padding: `${tokens.spacingVerticalXS} ${tokens.spacingHorizontalM}` },
  inset: { margin: tokens.spacingHorizontalM },
  fill: { flex: 1, minHeight: 0 },
  detail: {
    display: "grid", gridTemplateColumns: "max-content 1fr", gap: `${tokens.spacingVerticalXS} ${tokens.spacingHorizontalL}`, margin: 0,
    padding: `${tokens.spacingVerticalL} ${tokens.spacingHorizontalXL}`, overflow: "auto",
  },
  key: { fontFamily: tokens.fontFamilyMonospace, fontSize: tokens.fontSizeBase200, color: tokens.colorNeutralForeground3 },
  value: { margin: 0, overflowWrap: "anywhere", whiteSpace: "pre-wrap" },
  nul: { fontStyle: "italic", color: tokens.colorNeutralForeground4 },
});

function SaveDialog({ onSave, onClose }: { onSave(name: string): Promise<void>; onClose(): void }) {
  const { t } = useTranslation("sql");
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const submit = async () => {
    try {
      await onSave(name.trim());
      onClose();
    } catch (e) {
      const err = toAppError(e);
      setError(err.kind === "invalidInput" && err.field === "name" && err.reason === "duplicate" ? t("save.duplicate") : formatAppError(err, t));
    }
  };
  return (
    <Dialog open modalType="modal" onOpenChange={(_, d) => !d.open && onClose()}>
      <DialogSurface>
        <DialogBody>
          <DialogTitle>{t("save.title")}</DialogTitle>
          <DialogContent>
            <Field label={t("save.name")} validationMessage={error ?? undefined} validationState={error ? "error" : "none"}>
              <Input value={name} maxLength={200} onChange={(_, d) => setName(d.value)} />
            </Field>
          </DialogContent>
          <DialogActions>
            <Button onClick={onClose}>{t("cancel")}</Button>
            <Button appearance="primary" disabled={name.trim() === ""} onClick={() => void submit()}>{t("save.action")}</Button>
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
}

function DeleteDialog({ query, onConfirm, onClose }: { query: SavedQuery; onConfirm(): void; onClose(): void }) {
  const { t } = useTranslation("sql");
  return (
    <Dialog open modalType="modal" onOpenChange={(_, d) => !d.open && onClose()}>
      <DialogSurface>
        <DialogBody>
          <DialogTitle>{t("delete.title", { name: query.name })}</DialogTitle>
          <DialogContent>{t("delete.body")}</DialogContent>
          <DialogActions>
            <Button onClick={onClose}>{t("cancel")}</Button>
            <Button appearance="primary" onClick={onConfirm}>{t("delete.confirm")}</Button>
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
}

/** Prototype `#screen-sql` / `#sql:table`: saved queries | editor | result | detail. */
export function AdvancedSqlScreen() {
  const { t, i18n } = useTranslation("sql");
  const s = useStyles();
  const split = useSplit("advanced-sql", SPLITS);
  const root = useRef<HTMLDivElement>(null);
  const paneWidth = (i: number) => root.current?.querySelectorAll<HTMLElement>(":scope > section")[i]?.offsetWidth ?? 0;
  const qc = useQueryClient();
  const toast = useAppToast();
  const status = useAppStatus();
  const { go } = useNavigation();
  const lng = i18n.language;

  const [sql, setSql] = useState("");
  const [savedId, setSavedId] = useState<number | null>(null);
  const [result, setResult] = useState<SqlResult | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [executionId, setExecutionId] = useState<string | null>(null);
  const [mailId, setMailId] = useState<number | null>(null);
  const [rowIndex, setRowIndex] = useState<number | null>(null);
  const [dialog, setDialog] = useState<"save" | "delete" | null>(null);

  const saved = useQuery({ queryKey: ["savedQueries"], queryFn: () => api.savedQueryList() });
  const current = saved.data?.find((q) => q.id === savedId) ?? null;

  const run = useMutation({
    mutationFn: (req: SqlExecuteRequest) => api.sqlExecute(req),
    onMutate: (req) => {
      setExecutionId(req.executionId);
      setError(null);
    },
    onSuccess: (r) => {
      setResult(r);
      setRowIndex(null);
      setMailId(r.mode === "mailList" ? (r.rows[0]?.id ?? null) : null);
    },
    onError: (e) => setError(toAppError(e)),
    onSettled: () => setExecutionId(null),
  });
  const running = executionId !== null;
  const doRun = () => {
    if (running || sql.trim() === "") return;
    run.mutate({ executionId: crypto.randomUUID(), sql, maxRows: MAX_ROWS });
  };
  const cancel = () => {
    if (executionId) api.sqlCancel(executionId).catch(toast.error);
  };

  const refreshSaved = () => void qc.invalidateQueries({ queryKey: ["savedQueries"] });
  const saveAs = async (name: string) => {
    const q = await api.savedQueryCreate({ name, sql });
    setSavedId(q.id);
    refreshSaved();
    toast.notify(t("save.done", { name: q.name }));
  };
  const save = () => {
    if (!current) return setDialog("save");
    api.savedQueryUpdate(current.id, { name: current.name, sql })
      .then((q) => {
        refreshSaved();
        toast.notify(t("save.done", { name: q.name }));
      })
      .catch(toast.error);
  };
  const remove = () => {
    if (!current) return;
    setDialog(null);
    api.savedQueryDelete(current.id)
      .then(() => {
        setSavedId(null);
        refreshSaved();
      })
      .catch(toast.error);
  };
  const pick = (q: SavedQuery) => {
    setSavedId(q.id);
    setSql(q.sql);
    setError(null);
  };
  const newQuery = () => {
    setSavedId(null);
    setSql("");
    setError(null);
  };
  const edit = (next: string) => {
    setSql(next);
    if (error?.kind === "sqlError") setError(null);
  };

  const errorText = error
    ? formatAppError(error, t) + (error.kind === "sqlError" && error.position !== null ? ` ${t("error.position", { position: error.position })}` : "")
    : null;

  let statusLine;
  if (running) statusLine = <span className={s.statusText}>{t("status.running")}</span>;
  else if (result) {
    const n = result.mode === "command" ? 0 : result.rows.length;
    statusLine = (
      <>
        <span className={s.statusText}>{t("status.ok", { rows: formatNumber(n, lng), ms: formatNumber(result.elapsedMs, lng) })}</span>
        <span className={s.spacer} />
        <span>{t(`mode.${result.mode}`)}</span>
      </>
    );
  } else statusLine = <span className={s.statusText}>{t("status.idle")}</span>;

  let column3;
  let column4;
  const head3 = (title: string) => (
    <div className={s.head}>
      <Body1Strong className={s.title}>{title}</Body1Strong>
    </div>
  );
  if (!result) {
    const empty = status.counts.messages === 0 ? (
      <EmptyState icon={<DatabaseRegular />} title={t("empty.noMailTitle")} body={t("empty.noMailBody")}
        action={<Button appearance="primary" size="small" onClick={() => go("import")}>{t("common:action.goImport")}</Button>} />
    ) : (
      <EmptyState icon={<DatabaseRegular />} title={t("empty.noRunTitle")} body={t("empty.noRunBody")} />
    );
    column3 = (
      <>
        {head3(t("mode.mailList"))}
        {empty}
      </>
    );
    column4 = <EmptyState icon={<TableRegular />} title={t("search:preview.pickTitle")} />;
  } else {
    const truncated = result.mode !== "command" && result.truncated ? (
      <MessageBar intent="warning" className={s.inset} data-testid="sql-truncated">
        <MessageBarBody>{t("result.truncated", { n: formatNumber(MAX_ROWS, lng) })}</MessageBarBody>
      </MessageBar>
    ) : null;
    if (result.mode === "mailList") {
      column3 = (
        <>
          <div className={s.head}>
            <Body1Strong className={s.title}>{t("mode.mailList")}</Body1Strong>
            <Caption1 className={s.count}>{t("result.rows", { n: formatNumber(result.rows.length, lng) })}</Caption1>
          </div>
          {truncated}
          {result.missingIds > 0 ? <Caption1 className={s.note}>{t("result.missing", { n: formatNumber(result.missingIds, lng) })}</Caption1> : null}
          <div className={s.fill}>
            <MailList rows={result.rows} selectedId={mailId} onSelect={setMailId} hasMore={false} capped={false} loadingMore={false}
              onEndReached={noop} ariaLabel={t("mode.mailList")} monthHeaders={false} compact />
          </div>
        </>
      );
      column4 = <ReadingPane mailId={mailId} />;
    } else if (result.mode === "table") {
      const row = rowIndex === null ? null : result.rows[rowIndex];
      column3 = (
        <>
          <div className={s.head}>
            <Body1Strong className={s.title}>{t("mode.table")}</Body1Strong>
            <Caption1 className={s.count}>{t("result.rows", { n: formatNumber(result.rows.length, lng) })}</Caption1>
          </div>
          {truncated}
          <div className={s.fill}>
            <ResultTable columns={result.columns} rows={result.rows} selected={rowIndex} onSelect={setRowIndex} />
          </div>
        </>
      );
      column4 = row ? (
        <>
          <div className={s.head}>
            <Body1Strong className={s.title}>{t("detail.title")}</Body1Strong>
            <Caption1 className={s.count}>{t("detail.row", { n: (rowIndex ?? 0) + 1 })}</Caption1>
          </div>
          <dl className={s.detail} data-testid="sql-row-detail">
            {result.columns.map((c, j) => (
              <div key={j} style={{ display: "contents" }}>
                <dt className={s.key}>{c.name}</dt>
                <dd className={s.value}>{row[j] ?? <span className={s.nul}>{t("result.null")}</span>}</dd>
              </div>
            ))}
          </dl>
        </>
      ) : (
        <EmptyState icon={<TableRegular />} title={t("detail.pick")} />
      );
    } else {
      column3 = (
        <>
          {head3(t("mode.command"))}
          <MessageBar intent="success" className={s.inset}>
            <MessageBarBody>{t("result.command", { tag: result.tag })}</MessageBarBody>
          </MessageBar>
        </>
      );
      column4 = <EmptyState icon={<TableRegular />} title={t("detail.pick")} />;
    }
  }

  return (
    <div
      ref={root}
      className={s.root}
      style={{ gridTemplateColumns: `${split.track("c1", "minmax(0, 1fr)")} ${split.track("c2", "minmax(0, 3fr)")} ${split.track("c3", "minmax(0, 1fr)")} minmax(0, 3fr)` }}
    >
      {(["c1", "c2", "c3"] as const).map((id, i) => (
        <Splitter key={id} orientation="vertical" {...split.props(id)} measure={() => paneWidth(i)} style={{ gridColumn: i + 1, gridRow: 1, top: 0, bottom: 0, right: -3 }} />
      ))}
      <SavedQueries items={saved.data ?? []} selectedId={savedId} onPick={pick} onNew={newQuery} />
      <section className={s.col}>
        <Toolbar size="small" className={s.toolbar}>
          <ToolbarButton className={s.tool} appearance="primary" icon={<PlayRegular />} data-testid="sql-run" disabled={running || sql.trim() === ""} onClick={doRun}>
            {t("run")}
          </ToolbarButton>
          <ToolbarButton className={s.tool} icon={<StopRegular />} data-testid="sql-cancel" disabled={!running} onClick={cancel}>{t("cancel")}</ToolbarButton>
          <ToolbarButton className={s.tool} icon={<SaveRegular />} data-testid="sql-save" disabled={sql.trim() === ""} onClick={save}>{t("save.action")}</ToolbarButton>
          {current ? (
            <ToolbarButton className={s.tool} icon={<DeleteRegular />} onClick={() => setDialog("delete")}>{t("delete.action")}</ToolbarButton>
          ) : null}
          <span className={s.spacer} />
          <Badge className={s.badge} appearance="tint" color="warning" icon={<LockClosedRegular />}>{t("readOnly")}</Badge>
        </Toolbar>
        {errorText ? (
          <MessageBar intent="error">
            <MessageBarBody>{errorText}</MessageBarBody>
          </MessageBar>
        ) : null}
        <div className={s.editor}>
          <SqlEditor value={sql} onChange={edit} onRun={doRun} ariaLabel={t("editor")}
            errorPosition={error?.kind === "sqlError" ? error.position : null} />
        </div>
        <Caption1 className={s.status}>{statusLine}</Caption1>
      </section>
      <section className={s.col} data-testid="sql-result" data-mode={result?.mode ?? "none"}>
        {column3}
      </section>
      <section className={s.col}>{column4}</section>
      {dialog === "save" ? <SaveDialog onSave={saveAs} onClose={() => setDialog(null)} /> : null}
      {dialog === "delete" && current ? <DeleteDialog query={current} onConfirm={remove} onClose={() => setDialog(null)} /> : null}
    </div>
  );
}
