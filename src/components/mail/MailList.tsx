import { Avatar, Caption1Strong, CounterBadge, Spinner, makeStyles, mergeClasses, tokens } from "@fluentui/react-components";
import { AttachRegular } from "@fluentui/react-icons";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useMemo, useRef, type KeyboardEvent } from "react";
import { useTranslation } from "react-i18next";
import type { MailRow } from "../../api/bindings/MailRow";
import { formatDay } from "../common";

export const ROW_HEIGHT = 32;
export const PREFETCH_ROWS = 50;

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
}

/** Rows arrive newest first (Part 03 §5.2); a header entry starts each local month, undated mail last. */
export function buildEntries(rows: MailRow[], lng: string, undatedLabel: string): Entry[] {
  const fmt = new Intl.DateTimeFormat(lng === "cimode" ? "en" : lng, { year: "numeric", month: "long" });
  const out: Entry[] = [];
  let current: string | null = null;
  for (const row of rows) {
    const d = row.sentAt ? new Date(row.sentAt) : null;
    const month = d ? `${d.getFullYear()}-${d.getMonth()}` : "undated";
    if (month !== current) {
      out.push({ kind: "month", key: `m-${month}-${out.length}`, label: d ? fmt.format(d) : undatedLabel });
      current = month;
    }
    out.push({ kind: "mail", key: `r-${row.id}`, row });
  }
  return out;
}

const useStyles = makeStyles({
  scroll: { height: "100%", overflowY: "auto", position: "relative", outlineStyle: "none" },
  block: { position: "relative", width: "100%" },
  item: { position: "absolute", top: 0, left: 0, width: "100%", height: `${ROW_HEIGHT}px`, boxSizing: "border-box" },
  month: {
    display: "flex", alignItems: "center", padding: `0 ${tokens.spacingHorizontalM}`,
    backgroundColor: tokens.colorNeutralBackground2, color: tokens.colorNeutralForeground3,
  },
  row: {
    display: "grid", gridTemplateColumns: "24px minmax(120px, 22%) 1fr auto", alignItems: "center",
    gap: tokens.spacingHorizontalS, padding: `0 ${tokens.spacingHorizontalM}`, cursor: "pointer",
    borderBottom: `1px solid ${tokens.colorNeutralStroke3}`,
    ":hover": { backgroundColor: tokens.colorNeutralBackground1Hover },
  },
  selected: {
    backgroundColor: tokens.colorBrandBackground2,
    boxShadow: `inset 3px 0 0 ${tokens.colorCompoundBrandStroke}`,
    ":hover": { backgroundColor: tokens.colorBrandBackground2 },
  },
  ellipsis: { overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", minWidth: 0 },
  sender: { fontWeight: tokens.fontWeightSemibold },
  subject: { color: tokens.colorBrandForeground2 },
  snippet: { color: tokens.colorNeutralForeground3 },
  meta: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalXS, color: tokens.colorNeutralForeground3, fontSize: tokens.fontSizeBase200 },
  footer: { display: "flex", alignItems: "center", justifyContent: "center", height: `${ROW_HEIGHT}px`, color: tokens.colorNeutralForeground3 },
});

/** Prototype `#resList` `.mail-item`, one line per mail at 32 px (plan Decision "Fixed-height virtual rows"). */
export function MailList(p: MailListProps) {
  const { t, i18n } = useTranslation("search");
  const s = useStyles();
  const ref = useRef<HTMLDivElement>(null);
  const lng = i18n.language;
  const undated = t("list.undated");
  const entries = useMemo<Entry[]>(
    () =>
      p.monthHeaders === false
        ? p.rows.map((row) => ({ kind: "mail", key: `r-${row.id}`, row }))
        : buildEntries(p.rows, lng, undated),
    [p.rows, p.monthHeaders, lng, undated],
  );
  const v = useVirtualizer({ count: entries.length, getScrollElement: () => ref.current, estimateSize: () => ROW_HEIGHT, overscan: 10 });
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

  return (
    <div ref={ref} className={s.scroll} role="listbox" aria-label={p.ariaLabel} tabIndex={0} onKeyDown={onKeyDown} data-testid="mail-list">
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
          return (
            <div key={en.key} role="option" aria-selected={selected} data-testid="mail-row" data-mail-id={row.id}
              className={mergeClasses(s.item, s.row, selected && s.selected)} style={style} onClick={() => p.onSelect(row.id)}>
              <Avatar size={24} name={name} aria-hidden />
              <span className={mergeClasses(s.ellipsis, s.sender)}>{name}</span>
              <span className={s.ellipsis}>
                <span className={s.subject}>{row.subject || t("preview.noSubject")}</span>
                {row.snippet ? <span className={s.snippet}>{` — ${row.snippet}`}</span> : null}
              </span>
              <span className={s.meta}>
                {row.hasAttachments ? <AttachRegular aria-hidden /> : null}
                {row.occurrenceCount > 1 ? <CounterBadge count={row.occurrenceCount} size="small" appearance="ghost" /> : null}
                <span>{formatDay(row.sentAt, lng)}</span>
              </span>
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
  );
}
