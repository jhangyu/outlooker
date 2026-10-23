import { Avatar, Caption1Strong, CounterBadge, Spinner, makeStyles, mergeClasses, tokens } from "@fluentui/react-components";
import { AttachRegular } from "@fluentui/react-icons";
import { useVirtualizer } from "@tanstack/react-virtual";
import { Fragment, useEffect, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent, type PointerEvent, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { MailRow } from "../../api/bindings/MailRow";
import { ellipsis, formatDay, formatMonth, Splitter, useSplit, type SplitDefaults } from "../common";
import { moveColumn, handleSide, readOrder, writeOrder, type ColId } from "./columnOrder";

export const ROW_HEIGHT = 32;
/** Prototype `.mail-item.compact`: sender + date, subject, snippet on three lines. */
// 56 px of text lines + 2 × 8 px padding + 1 px bottom border (border-box).
const COMPACT_ROW_HEIGHT = 73;
const PREFETCH_ROWS = 50;
const STORE = "mail-list";
// Subject is the 1fr remainder wherever it sits. Sender and date have stored widths (one `outlooker.splits.mail-list` map, order included).
// Both share one group: each is limited by the room left after the other, 24 px avatar, 3 × 8 gaps and the subject minimum.
const SUBJECT_MIN = 96;
const FIXED = 24 + 3 * 8 + SUBJECT_MIN;
const COLUMN_SPLITS: SplitDefaults = {
  sender: { size: 110, min: 96, group: "cols" },
  date: { size: 80, min: 72, group: "cols", minOther: FIXED },
};
const DRAG_THRESHOLD = 4;
const LABELS: Record<ColId, string> = { sender: "list.col.from", subject: "list.col.subject", date: "list.col.date" };

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
  head: {
    display: "grid", alignItems: "center", gap: tokens.spacingHorizontalS, height: "24px", flexShrink: 0,
    padding: `0 ${tokens.spacingHorizontalM}`, overflow: "hidden", scrollbarGutter: "stable",
    backgroundColor: tokens.colorNeutralBackground2, color: tokens.colorNeutralForeground2,
    borderBottom: `1px solid ${tokens.colorNeutralStroke2}`,
  },
  headCell: { position: "relative", minWidth: 0, outlineStyle: "none", cursor: "grab", touchAction: "none" },
  headEnd: { display: "flex", justifyContent: "flex-end" },
  sr: { position: "absolute", width: "1px", height: "1px", overflow: "hidden", clipPath: "inset(50%)" },
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
  meta: { display: "flex", justifySelf: "end", whiteSpace: "nowrap", alignItems: "center", gap: tokens.spacingHorizontalXS, color: tokens.colorNeutralForeground3, fontSize: tokens.fontSizeBase200 },
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
  const { track, props } = useSplit(STORE, COLUMN_SPLITS);
  const storeKey = "outlooker.splits." + STORE;
  const [order, setOrder] = useState(() => readOrder(storeKey));
  const [announce, setAnnounce] = useState("");
  const headRef = useRef<HTMLDivElement>(null);
  const cells = useRef<Partial<Record<ColId, HTMLElement | null>>>({});
  const refocus = useRef<ColId | null>(null);
  const press = useRef<{ id: ColId; x: number } | null>(null);
  useLayoutEffect(() => {
    if (refocus.current) cells.current[refocus.current]?.focus();
    refocus.current = null;
  }, [order]);
  const reorder = (id: ColId, to: number) => {
    const next = moveColumn(order, id, to);
    if (next.join() === order.join()) return;
    writeOrder(storeKey, next);
    setOrder(next);
    setAnnounce(`${t(LABELS[id])} ${next.indexOf(id) + 1}/${next.length}`);
  };
  const tracks: Record<ColId, string> = { sender: track("sender", "110px"), subject: `minmax(${SUBJECT_MIN}px, 1fr)`, date: track("date", "80px") };
  const grid = { gridTemplateColumns: `24px ${order.map((c) => tracks[c]).join(" ")}` };
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

  const ownEvent = (e: { target: EventTarget; currentTarget: EventTarget }) => e.target === e.currentTarget;
  const onCellKeyDown = (id: ColId) => (e: KeyboardEvent<HTMLElement>) => {
    if (!ownEvent(e) || !e.altKey || (e.key !== "ArrowLeft" && e.key !== "ArrowRight")) return;
    e.preventDefault();
    refocus.current = id;
    reorder(id, order.indexOf(id) + (e.key === "ArrowLeft" ? -1 : 1));
  };
  const onCellDown = (id: ColId) => (e: PointerEvent<HTMLElement>) => {
    if (e.button !== 0 || (e.target as HTMLElement).closest('[role="separator"]')) return;
    e.currentTarget.setPointerCapture?.(e.pointerId);
    press.current = { id, x: e.clientX };
  };
  const onCellUp = (e: PointerEvent<HTMLElement>) => {
    const down = press.current;
    press.current = null;
    if (!down || Math.abs(e.clientX - down.x) < DRAG_THRESHOLD) return;
    // Drop position = number of other columns whose centre lies left of the pointer.
    const to = order.filter((c) => c !== down.id).filter((c) => {
      const r = cells.current[c]?.getBoundingClientRect();
      return r ? r.left + r.width / 2 < e.clientX : false;
    }).length;
    refocus.current = down.id;
    reorder(down.id, to);
  };
  const head = (id: ColId): ReactNode => {
    const resizable = id !== "subject";
    const side = resizable ? handleSide(order, id) : null;
    return (
      <span key={id} ref={(el) => void (cells.current[id] = el)} className={mergeClasses(s.headCell, id === "date" && s.headEnd)} tabIndex={0}
        onKeyDown={onCellKeyDown(id)} onPointerDown={onCellDown(id)} onPointerUp={onCellUp} onPointerCancel={() => (press.current = null)}>
        <Caption1Strong className={s.ellipsis}>{t(LABELS[id])}</Caption1Strong>
        {resizable && side ? (
          <Splitter orientation="vertical" reverse={side === "left"} {...props(id)} container={() => headRef.current}
            style={side === "left" ? { left: "-7px", top: 0, height: "100%" } : { right: "-7px", top: 0, height: "100%" }} />
        ) : null}
      </span>
    );
  };
  const cellOf = (id: ColId, row: MailRow, name: string, meta: ReactNode): ReactNode =>
    id === "sender" ? (
      <span key={id} className={mergeClasses(s.ellipsis, s.sender)}>{name}</span>
    ) : id === "subject" ? (
      <span key={id} className={s.ellipsis}>
        <span className={s.subject}>{row.subject || t("preview.noSubject")}</span>
        {row.snippet ? <span className={s.snippet}>{` — ${row.snippet}`}</span> : null}
      </span>
    ) : (
      <Fragment key={id}>{meta}</Fragment>
    );
  return (
    <div className={s.root}>
      {p.compact ? null : (
        <div ref={headRef} className={s.head} style={grid} role="presentation" data-testid="mail-list-header">
          <span />
          {order.map(head)}
          <span className={s.sr} role="status" aria-live="polite">{announce}</span>
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
              {order.map((c) => cellOf(c, row, name, meta))}
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
