import { Avatar, Caption1Strong, CounterBadge, Spinner, makeStyles, mergeClasses, tokens } from "@fluentui/react-components";
import { AttachRegular } from "@fluentui/react-icons";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import { useTranslation } from "react-i18next";
import type { MailRow } from "../../api/bindings/MailRow";
import { ellipsis, formatDay, formatMonth, useGridColumnSizing } from "../common";

export const ROW_HEIGHT = 32;
/** Prototype `.mail-item.compact`: sender + date, subject, snippet on three lines. */
// 56 px of text lines + 2 × 8 px padding + 1 px bottom border (border-box).
const COMPACT_ROW_HEIGHT = 73;
const PREFETCH_ROWS = 50;
// Sender and date have stored widths, subject is the 1fr remainder so extra pane width goes to the subject.
// A 420 px pane (the minimum) fits: 24 avatar + 110 + 96 subject floor + 88 + 3 × 8 gaps + 24 padding + ~15 scrollbar.
// Sender gets Fluent's right-edge handle (sender|subject). Date is dragged from its left edge (subject|date),
// which Fluent cannot do (inverted delta), so ponytail: hand-rolled pointer handler that read-merge-writes the
// same `mail-list` storage entry. Move into columnWidths.ts if a second surface needs a left-edge handle.
const COLUMN_DEFAULTS = { sender: { width: 110, minWidth: 96 }, date: { width: 88, minWidth: 72 } };
const DATE_MIN = 72;
const DATE_MAX = 320;
const DATE_STEP = 16;
const STORAGE_KEY = "outlooker.columnWidths.mail-list";

function storeDateWidth(width: number) {
  try {
    const raw: unknown = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "null");
    const map = raw && typeof raw === "object" && !Array.isArray(raw) ? raw : {};
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ ...map, date: width }));
  } catch {
    // Storage unavailable: width stays session-only.
  }
}

export type Entry = { kind: "month"; key: string; label: string } | { kind: "mail"; key: string; row: MailRow };

export interface MailListProps {
  rows: MailRow[];
  selectedId: number | null;
  onSelect(id: number): void;
  hasMore: boolean;
  capped: boolean;
  loadingMore: boolean;
  onEndReached(): void;
  ariaLabel: string;
  monthHeaders?: boolean;
  /** Three-line rows without avatar for narrow columns (prototype `mailItem(…, true)`). */
  compact?: boolean;
}

/** Rows arrive newest first (Part 03 §5.2); a header entry starts each local month, undated mail last. */
export function buildEntries(rows: MailRow[], lng: string, undatedLabel: string): Entry[] {
  const out: Entry[] = [];
  let current: string | null = null;
  for (const row of rows) {
    const d = row.sentAt ? new Date(row.sentAt) : null;
    const month = d ? `${d.getFullYear()}-${d.getMonth()}` : "undated";
    if (month !== current) {
      out.push({ kind: "month", key: `m-${month}-${out.length}`, label: d ? formatMonth(row.sentAt as string, lng) : undatedLabel });
      current = month;
    }
    out.push({ kind: "mail", key: `r-${row.id}`, row });
  }
  return out;
}

const useStyles = makeStyles({
  root: { display: "flex", flexDirection: "column", height: "100%" },
  dateHandle: {
    position: "absolute", top: 0, bottom: 0, left: "-6px", width: "8px", cursor: "col-resize", touchAction: "none",
    ":hover": { boxShadow: `inset -2px 0 0 ${tokens.colorNeutralStroke1Hover}` },
    ":focus-visible": { boxShadow: `inset -2px 0 0 ${tokens.colorCompoundBrandStroke}`, outlineStyle: "none" },
  },
  head: {
    display: "grid", alignItems: "center", gap: tokens.spacingHorizontalS, height: "24px", flexShrink: 0,
    padding: `0 ${tokens.spacingHorizontalM}`, overflow: "hidden", scrollbarGutter: "stable",
    backgroundColor: tokens.colorNeutralBackground2, color: tokens.colorNeutralForeground2,
    borderBottom: `1px solid ${tokens.colorNeutralStroke2}`,
  },
  headCell: { position: "relative", minWidth: 0, outlineStyle: "none" },
  scroll: { flex: 1, minHeight: 0, overflowY: "auto", position: "relative", outlineStyle: "none", scrollbarGutter: "stable" },
  block: { position: "relative", width: "100%" },
  item: { position: "absolute", top: 0, left: 0, width: "100%", height: `${ROW_HEIGHT}px`, boxSizing: "border-box" },
  month: {
    display: "flex", alignItems: "center", padding: `0 ${tokens.spacingHorizontalM}`,
    backgroundColor: tokens.colorNeutralBackground2, color: tokens.colorNeutralForeground2,
    borderBottom: `1px solid ${tokens.colorNeutralStroke2}`,
  },
  row: {
    display: "grid", alignItems: "center",
    gap: tokens.spacingHorizontalS, padding: `0 ${tokens.spacingHorizontalM}`, cursor: "pointer",
    borderBottom: `1px solid ${tokens.colorNeutralStroke3}`,
    ":hover": { backgroundColor: tokens.colorNeutralBackground1Hover },
  },
  compact: {
    gridTemplateColumns: "1fr auto", gridTemplateRows: "20px 20px 16px", gap: `0 ${tokens.spacingHorizontalS}`, height: `${COMPACT_ROW_HEIGHT}px`,
    padding: `${tokens.spacingVerticalS} ${tokens.spacingHorizontalS} ${tokens.spacingVerticalS} ${tokens.spacingHorizontalM}`,
  },
  span: { gridColumn: "1 / 3" },
  small: { fontSize: tokens.fontSizeBase200 },
  selected: {
    backgroundColor: tokens.colorBrandBackground2,
    boxShadow: `inset 3px 0 0 ${tokens.colorCompoundBrandStroke}`,
    ":hover": { backgroundColor: tokens.colorBrandBackground2 },
  },
  ellipsis: { ...ellipsis },
  sender: { fontWeight: tokens.fontWeightSemibold },
  subject: { color: tokens.colorBrandForeground2 },
  snippet: { color: tokens.colorNeutralForeground3 },
  meta: { display: "flex", whiteSpace: "nowrap", alignItems: "center", gap: tokens.spacingHorizontalXS, color: tokens.colorNeutralForeground3, fontSize: tokens.fontSizeBase200 },
  footer: { display: "flex", alignItems: "center", justifyContent: "center", height: `${ROW_HEIGHT}px`, color: tokens.colorNeutralForeground3 },
});

/**
 * Prototype `#resList` `.mail-item`, one line per mail at 32 px.
 * `compact` mail rows are three lines at COMPACT_ROW_HEIGHT; month headers stay 32 px.
 */
export function MailList(p: MailListProps) {
  const { t, i18n } = useTranslation("search");
  const s = useStyles();
  const ref = useRef<HTMLDivElement>(null);
  // A hidden <Activity> (display:none) can lose scrollTop in the webview: remember it and restore on re-show.
  const lastTop = useRef(0);
  useLayoutEffect(() => {
    const el = ref.current;
    if (el && el.scrollTop !== lastTop.current) el.scrollTop = lastTop.current;
  }, []);
  const { widths, headerProps } = useGridColumnSizing("mail-list", COLUMN_DEFAULTS);
  const [dateWidth, setDateWidth] = useState(() => widths.date!);
  const grid = { gridTemplateColumns: `24px ${widths.sender}px minmax(96px, 1fr) ${dateWidth}px` };
  const resizeDate = (w: number) => {
    const next = Math.round(Math.min(Math.max(w, DATE_MIN), DATE_MAX));
    setDateWidth(next);
    storeDateWidth(next);
  };
  const dragDate = (e: PointerEvent<HTMLSpanElement>) => {
    const el = e.currentTarget;
    const x0 = e.clientX;
    const w0 = dateWidth;
    el.setPointerCapture(e.pointerId);
    const move = (m: globalThis.PointerEvent) => resizeDate(w0 + x0 - m.clientX);
    const up = () => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      el.removeEventListener("pointercancel", up);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
    el.addEventListener("pointercancel", up);
  };
  const lng = i18n.language;
  const undated = t("list.undated");
  const entries = useMemo<Entry[]>(
    () =>
      p.monthHeaders === false
        ? p.rows.map((row) => ({ kind: "mail", key: `r-${row.id}`, row }))
        : buildEntries(p.rows, lng, undated),
    [p.rows, p.monthHeaders, lng, undated],
  );
  const v = useVirtualizer({ count: entries.length, getScrollElement: () => ref.current, estimateSize: (i) => (p.compact && entries[i]?.kind === "mail" ? COMPACT_ROW_HEIGHT : ROW_HEIGHT), overscan: 10 });
  const items = v.getVirtualItems();
  const lastIndex = items.length > 0 ? items[items.length - 1]!.index : -1;
  const { hasMore, loadingMore, capped, onEndReached } = p;

  useEffect(() => {
    if (entries.length > 0 && lastIndex >= entries.length - 1 - PREFETCH_ROWS && hasMore && !loadingMore && !capped) onEndReached();
  }, [lastIndex, entries.length, hasMore, loadingMore, capped, onEndReached]);

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    e.preventDefault();
    const mailIdx: number[] = [];
    entries.forEach((en, i) => en.kind === "mail" && mailIdx.push(i));
    if (mailIdx.length === 0) return;
    const cur = mailIdx.findIndex((i) => (entries[i] as { row: MailRow }).row.id === p.selectedId);
    const step = e.key === "ArrowDown" ? 1 : -1;
    const next = cur === -1 ? 0 : Math.min(Math.max(cur + step, 0), mailIdx.length - 1);
    const idx = mailIdx[next]!;
    p.onSelect((entries[idx] as { row: MailRow }).row.id);
    v.scrollToIndex(idx);
  };

  const sender = headerProps("sender");
  return (
    <div className={s.root}>
      {p.compact ? null : (
        <div className={s.head} style={grid} role="presentation" data-testid="mail-list-header">
          <span />
          <span className={s.headCell} tabIndex={0} onKeyDown={sender.onKeyDown}>
            <Caption1Strong className={s.ellipsis}>{t("list.col.from")}</Caption1Strong>
            {sender.aside}
          </span>
          <Caption1Strong className={s.ellipsis}>{t("list.col.subject")}</Caption1Strong>
          <span className={s.headCell}>
            <Caption1Strong className={s.ellipsis}>{t("list.col.date")}</Caption1Strong>
            <span
              className={s.dateHandle} role="separator" aria-orientation="vertical" tabIndex={0}
              aria-valuenow={dateWidth} aria-valuemin={DATE_MIN} aria-valuemax={DATE_MAX} aria-label={t("list.col.date")}
              onPointerDown={dragDate} onDoubleClick={() => resizeDate(COLUMN_DEFAULTS.date.width)}
              onKeyDown={(e) => {
                if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
                e.preventDefault();
                resizeDate(dateWidth + (e.key === "ArrowLeft" ? DATE_STEP : -DATE_STEP));
              }}
            />
          </span>
        </div>
      )}
    <div ref={ref} className={s.scroll} role="listbox" aria-label={p.ariaLabel} tabIndex={0} onKeyDown={onKeyDown} onScroll={(e) => (lastTop.current = e.currentTarget.scrollTop)} data-testid="mail-list">
      <div className={s.block} style={{ height: `${v.getTotalSize()}px` }}>
        {items.map((it) => {
          const en = entries[it.index]!;
          const style = { transform: `translateY(${it.start}px)` };
          if (en.kind === "month") {
            return (
              <div key={en.key} className={mergeClasses(s.item, s.month)} style={style} role="presentation">
                <Caption1Strong>{en.label}</Caption1Strong>
              </div>
            );
          }
          const row = en.row;
          const selected = row.id === p.selectedId;
          const name = row.from.name ?? row.from.address ?? "";
          const meta = (
            <span className={s.meta}>
              {row.hasAttachments ? <AttachRegular aria-hidden /> : null}
              {row.occurrenceCount > 1 ? <CounterBadge count={row.occurrenceCount} size="small" appearance="ghost" /> : null}
              <span>{formatDay(row.sentAt, lng)}</span>
            </span>
          );
          if (p.compact) {
            return (
              <div key={en.key} role="option" aria-selected={selected} data-testid="mail-row" data-mail-id={row.id}
                className={mergeClasses(s.item, s.row, s.compact, selected && s.selected)} style={style} onClick={() => p.onSelect(row.id)}>
                <span className={mergeClasses(s.ellipsis, s.sender)}>{name}</span>
                {meta}
                <span className={mergeClasses(s.ellipsis, s.span, s.subject)}>{row.subject || t("preview.noSubject")}</span>
                <span className={mergeClasses(s.ellipsis, s.span, s.small, s.snippet)}>{row.snippet}</span>
              </div>
            );
          }
          return (
            <div key={en.key} role="option" aria-selected={selected} data-testid="mail-row" data-mail-id={row.id}
              className={mergeClasses(s.item, s.row, selected && s.selected)} style={{ ...style, ...grid }} onClick={() => p.onSelect(row.id)}>
              <Avatar size={24} name={name} aria-hidden />
              <span className={mergeClasses(s.ellipsis, s.sender)}>{name}</span>
              <span className={s.ellipsis}>
                <span className={s.subject}>{row.subject || t("preview.noSubject")}</span>
                {row.snippet ? <span className={s.snippet}>{` — ${row.snippet}`}</span> : null}
              </span>
              {meta}
            </div>
          );
        })}
      </div>
      {hasMore && !capped ? (
        <div className={s.footer} data-testid="list-loader-row">
          <Spinner size="tiny" />
        </div>
      ) : null}
      {capped ? (
        <div className={s.footer} data-testid="result-cap-row">
          {t("resultCap")}
        </div>
      ) : null}
    </div>
    </div>
  );
}
