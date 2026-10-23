import {
  Avatar, Button, Caption1, Overflow, OverflowItem, Popover, PopoverSurface, PopoverTrigger, Subtitle2, Text, Toolbar,
  ToolbarButton, makeStyles, mergeClasses, tokens, useIsOverflowItemVisible, useOverflowMenu,
} from "@fluentui/react-components";
import { CopyRegular, FolderRegular, MoreHorizontalRegular, AttachRegular } from "@fluentui/react-icons";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { Address } from "../../api/bindings/Address";
import type { MailDetail } from "../../api/bindings/MailDetail";
import { ellipsis, formatDateTime } from "../common";
import { AttachmentRow } from "./AttachmentRow";

/** Pane width below which the header uses two rows (subject + date, then tokens). */
const TWO_ROW_BELOW = 900;

const useStyles = makeStyles({
  hdr: {
    boxSizing: "border-box", flexShrink: 0, display: "flex", padding: `10px ${tokens.spacingHorizontalL}`,
    borderBottom: `1px solid ${tokens.colorNeutralStroke2}`, backgroundColor: tokens.colorNeutralBackground1,
  },
  one: { height: "49px", alignItems: "center", columnGap: tokens.spacingHorizontalS },
  two: { height: "81px", flexDirection: "column", rowGap: "4px" },
  row: { display: "flex", alignItems: "center", columnGap: tokens.spacingHorizontalS, minWidth: 0, height: "28px" },
  subject: { ...ellipsis, flex: "1 1 0", minWidth: "140px", margin: 0 },
  date: { flexShrink: 0, whiteSpace: "nowrap", color: tokens.colorNeutralForeground3 },
  bar: { flex: "0 1 auto", minWidth: 0, padding: 0, columnGap: "6px", minHeight: "28px", overflow: "hidden" },
  barTwo: { flex: "1 1 auto" },
  token: {
    height: "28px", maxWidth: "230px", minWidth: 0, columnGap: "5px", padding: "0 8px", fontWeight: tokens.fontWeightRegular,
    border: `1px solid ${tokens.colorNeutralStroke1}`, backgroundColor: tokens.colorNeutralBackground1, whiteSpace: "nowrap",
  },
  sender: { maxWidth: "300px", border: "1px solid transparent", paddingLeft: "3px" },
  label: { color: tokens.colorNeutralForeground3, flexShrink: 0 },
  val: { ...ellipsis, minWidth: 0 },
  cnt: { flexShrink: 0, fontWeight: tokens.fontWeightSemibold, backgroundColor: tokens.colorNeutralBackground5, borderRadius: tokens.borderRadiusMedium, padding: "0 5px" },
  addr: { ...ellipsis, color: tokens.colorNeutralForeground3, minWidth: 0 },
  pw: { display: "inline-flex", flexShrink: 0 },
  surface: { width: "380px", maxWidth: "90vw", padding: `${tokens.spacingVerticalS} 0` },
  ph: { display: "flex", alignItems: "center", columnGap: tokens.spacingHorizontalS, padding: `0 ${tokens.spacingHorizontalM} ${tokens.spacingVerticalXS}`, borderBottom: `1px solid ${tokens.colorNeutralStroke2}` },
  grow: { flex: 1 },
  sh: { padding: `${tokens.spacingVerticalS} ${tokens.spacingHorizontalM} 2px`, color: tokens.colorNeutralForeground3, fontWeight: tokens.fontWeightSemibold },
  list: { listStyle: "none", margin: 0, padding: 0, maxHeight: "208px", overflowY: "auto" },
  listAll: { maxHeight: "none" },
  li: { display: "flex", alignItems: "center", columnGap: tokens.spacingHorizontalS, padding: `5px ${tokens.spacingHorizontalM}`, minHeight: "32px" },
  tx: { display: "flex", flexDirection: "column", minWidth: 0, flex: 1 },
  muted: { color: tokens.colorNeutralForeground3 },
  kv: { display: "flex", flexDirection: "column", padding: `6px ${tokens.spacingHorizontalM}`, userSelect: "text" },
});

const fmt = (a: Address) => (a.name && a.address ? `${a.name} <${a.address}>` : (a.name ?? a.address ?? ""));
const nameOf = (a: Address) => a.name ?? a.address ?? "";

function useNarrow() {
  const ref = useRef<HTMLDivElement>(null);
  const [narrow, setNarrow] = useState(false);
  useEffect(() => {
    const el = ref.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(([e]) => setNarrow((e?.contentRect.width ?? TWO_ROW_BELOW) < TWO_ROW_BELOW));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  return [ref, narrow] as const;
}

/** Popover token: ToolbarButton trigger + dialog surface. */
function Token({ id, priority, className, label, children, content }: {
  id: string; priority: number; className?: string; label: string; children: ReactNode; content: ReactNode;
}) {
  const s = useStyles();
  return (
    <OverflowItem id={id} priority={priority}>
      <span className={s.pw}>
        <Popover positioning="below-start">
          <PopoverTrigger disableButtonEnhancement>
            <ToolbarButton className={mergeClasses(s.token, className)} aria-label={label} aria-haspopup="dialog">
              {children}
            </ToolbarButton>
          </PopoverTrigger>
          <PopoverSurface className={s.surface} aria-label={label}>{content}</PopoverSurface>
        </Popover>
      </span>
    </OverflowItem>
  );
}

function Recipients({ list, copyLabel, title }: { list: Address[]; copyLabel: string; title: string }) {
  const s = useStyles();
  const { t } = useTranslation("search");
  return (
    <>
      <div className={s.ph}>
        <Text weight="semibold">{title}</Text>
        <Caption1 className={s.muted}>{t("preview.people", { n: list.length })}</Caption1>
        <span className={s.grow} />
        <Button size="small" icon={<CopyRegular />} onClick={() => void navigator.clipboard.writeText(list.map(fmt).join(", "))}>
          {copyLabel}
        </Button>
      </div>
      <RecipientList list={list} />
    </>
  );
}

function RecipientList({ list, all }: { list: Address[]; all?: boolean }) {
  const s = useStyles();
  return (
    <ul className={mergeClasses(s.list, all && s.listAll)}>
      {list.map((a, i) => (
        <li key={i} className={s.li}>
          <span className={s.tx}>
            <Text className={s.val}>{nameOf(a)}</Text>
            {a.name && a.address ? <Caption1 className={mergeClasses(s.val, s.muted)}>{a.address}</Caption1> : null}
          </span>
        </li>
      ))}
    </ul>
  );
}

function Places({ m, all }: { m: MailDetail; all?: boolean }) {
  const s = useStyles();
  return (
    <ul className={mergeClasses(s.list, all && s.listAll)}>
      {m.occurrences.map((o) => (
        <li key={`${o.pstId}-${o.pstItemId}`} className={s.li}>
          <FolderRegular aria-hidden />
          <span className={s.tx}>
            <Text className={s.val}>{o.pstDisplayName}</Text>
            <Caption1 className={mergeClasses(s.val, s.muted)}>{o.folderPath}</Caption1>
          </span>
        </li>
      ))}
    </ul>
  );
}

function Files({ m }: { m: MailDetail }) {
  const s = useStyles();
  return (
    <div className={s.list}>
      {m.attachments.map((a) => (
        <div key={a.id} className={s.li}>
          <AttachmentRow attachment={a} />
        </div>
      ))}
    </div>
  );
}

/** Renders `children` only while the overflow item `id` is hidden in the toolbar. */
function WhenHidden({ id, title, children }: { id: string; title: string; children: ReactNode }) {
  const s = useStyles();
  const visible = useIsOverflowItemVisible(id);
  if (visible) return null;
  return (
    <>
      <div className={s.sh}>{title}</div>
      {children}
    </>
  );
}

function OverflowToken({ m }: { m: MailDetail }) {
  const { t } = useTranslation("search");
  const s = useStyles();
  const { ref, isOverflowing, overflowCount } = useOverflowMenu<HTMLButtonElement>();
  if (!isOverflowing) return null;
  const groups = [
    { id: "bcc", title: t("preview.bcc"), n: m.bcc.length },
    { id: "found", title: t("preview.foundIn"), n: m.occurrences.length },
    { id: "att", title: t("preview.attachments"), n: m.attachments.length },
  ].filter((g) => g.n > 0);
  const names = groups.map((g) => g.title).join("、");
  const label = t("preview.moreAria", { n: overflowCount, names });
  return (
    <Popover positioning="below-end">
      <PopoverTrigger disableButtonEnhancement>
        <ToolbarButton ref={ref} className={s.token} aria-label={label} aria-haspopup="dialog" icon={<MoreHorizontalRegular />}>
          <span className={s.cnt}>{`+${overflowCount}`}</span>
        </ToolbarButton>
      </PopoverTrigger>
      <PopoverSurface className={s.surface} aria-label={t("preview.moreTitle")}>
        {groups.map((g) => (
          <WhenHidden key={g.id} id={g.id} title={`${g.title} · ${g.n}`}>
            {g.id === "bcc" ? <RecipientList list={m.bcc} all /> : g.id === "found" ? <Places m={m} all /> : <Files m={m} />}
          </WhenHidden>
        ))}
      </PopoverSurface>
    </Popover>
  );
}

/** Spec §3: one Toolbar row of popover tokens (two rows below 900 px). */
export function MessageHeader({ m }: { m: MailDetail }) {
  const { t, i18n } = useTranslation("search");
  const s = useStyles();
  const [ref, narrow] = useNarrow();
  const subject = m.subject || t("preview.noSubject");
  const full = formatDateTime(m.sentAt, i18n.language);
  const sender = nameOf(m.from);
  const recip = (id: "to" | "cc" | "bcc", priority: number) => {
    const list = m[id];
    if (id !== "to" && list.length === 0) return null;
    const label = t(`preview.${id}`);
    const first = list[0] ? nameOf(list[0]) : "";
    const more = list.length - 1;
    return (
      <Token key={id} id={id} priority={priority} label={more > 0 ? t("preview.tokenAriaMore", { label, first, n: more }) : `${label}: ${first}`}
        content={<Recipients list={list} title={label} copyLabel={t("preview.copyAll")} />}>
        <span className={s.label}>{label}</span>
        <span className={s.val}>{first}</span>
        {more > 0 ? <span className={s.cnt}>{`+${more}`}</span> : null}
      </Token>
    );
  };
  const bar = (
    <Overflow minimumVisible={0}>
      <Toolbar size="small" aria-label={t("preview.toolbar")} className={mergeClasses(s.bar, narrow && s.barTwo)}>
        <Token id="sender" priority={5} className={s.sender} label={`${t("preview.from")}: ${sender} ${m.from.address ?? ""}`.trim()}
          content={
            <>
              <div className={s.kv}><Caption1 className={s.muted}>{t("preview.subject")}</Caption1><Text>{subject}</Text></div>
              <div className={s.kv}><Caption1 className={s.muted}>{t("preview.from")}</Caption1><Text>{fmt(m.from)}</Text></div>
              <div className={s.kv}><Caption1 className={s.muted}>{t("preview.date")}</Caption1><Text>{full}</Text></div>
            </>
          }>
          <Avatar size={20} name={sender} color="colorful" />
          <b className={s.val}>{sender}</b>
          {!narrow && m.from.address && m.from.name ? <span className={s.addr}>{m.from.address}</span> : null}
        </Token>
        {recip("to", 4)}
        {recip("cc", 3)}
        {recip("bcc", 1)}
        {m.occurrences.length > 0 ? (
          <Token id="found" priority={2} label={t("preview.foundInAria", { n: m.occurrences.length })}
            content={<><div className={s.ph}><Text weight="semibold">{t("preview.foundIn")}</Text><Caption1 className={s.muted}>{t("preview.places", { n: m.occurrences.length })}</Caption1></div><Places m={m} /></>}>
            <FolderRegular aria-hidden />
            <span className={s.val}>{m.occurrences[0]?.pstDisplayName}</span>
            {m.occurrences.length > 1 ? <span className={s.cnt}>{`+${m.occurrences.length - 1}`}</span> : null}
          </Token>
        ) : null}
        {m.attachments.length > 0 ? (
          <Token id="att" priority={2} label={t("preview.attAria", { n: m.attachments.length })}
            content={<><div className={s.ph}><Text weight="semibold">{t("preview.attachments")}</Text><Caption1 className={s.muted}>{t("preview.files", { n: m.attachments.length })}</Caption1></div><Files m={m} /></>}>
            <AttachRegular aria-hidden />
            <span className={s.cnt}>{`×${m.attachments.length}`}</span>
          </Token>
        ) : null}
        <OverflowToken m={m} />
      </Toolbar>
    </Overflow>
  );
  const subj = <Subtitle2 as="h2" className={s.subject} title={subject}>{subject}</Subtitle2>;
  const date = <Caption1 className={s.date} title={t("preview.date")}>{full}</Caption1>;
  return (
    <div ref={ref} className={mergeClasses(s.hdr, narrow ? s.two : s.one)}>
      {narrow ? (
        <>
          <div className={s.row}>{subj}{date}</div>
          <div className={s.row}>{bar}</div>
        </>
      ) : (
        <>
          {subj}
          {bar}
          {date}
        </>
      )}
    </div>
  );
}
