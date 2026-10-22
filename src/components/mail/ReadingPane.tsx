import {
  Badge, Caption1, MessageBar, MessageBarBody, Persona, Spinner, Title3, makeStyles, tokens,
} from "@fluentui/react-components";
import { MailRegular } from "@fluentui/react-icons";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api } from "../../api/commands";
import type { Address } from "../../api/bindings/Address";
import { EmptyState, ellipsis, formatAppError, formatDateTime, toAppError } from "../common";
import { AttachmentCard } from "./AttachmentCard";
import { MailPreview } from "./MailPreview";

const useStyles = makeStyles({
  root: { display: "flex", flexDirection: "column", height: "100%", minHeight: 0, backgroundColor: tokens.colorNeutralBackground1 },
  head: {
    maxHeight: "45%", overflowY: "auto", flexShrink: 0, display: "flex", flexDirection: "column", gap: tokens.spacingVerticalL,
    padding: `${tokens.spacingVerticalXL} ${tokens.spacingHorizontalXXL}`, borderBottom: `1px solid ${tokens.colorNeutralStroke2}`,
  },
  meta: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalM },
  spacer: { flex: 1 },
  persona: { minWidth: 0 },
  ellipsis: { ...ellipsis },
  date: { whiteSpace: "nowrap", flexShrink: 0, alignSelf: "flex-start", color: tokens.colorNeutralForeground3 },
  line: { display: "grid", gridTemplateColumns: "48px 1fr", gap: tokens.spacingHorizontalS, color: tokens.colorNeutralForeground2 },
  chips: { display: "flex", flexWrap: "wrap", gap: tokens.spacingHorizontalXS, alignItems: "center" },
  body: { flex: 1, minHeight: 0, overflow: "auto" },
  text: {
    margin: 0, padding: `${tokens.spacingVerticalL} ${tokens.spacingHorizontalXXL}`, maxWidth: "900px", lineHeight: "22px",
    whiteSpace: "pre-wrap", fontFamily: tokens.fontFamilyBase,
  },
  atts: { display: "flex", flexDirection: "column", gap: tokens.spacingVerticalXS },
  muted: { color: tokens.colorNeutralForeground3 },
  center: { display: "grid", placeItems: "center", height: "100%" },
});

const fmt = (a: Address) => (a.name && a.address ? `${a.name} <${a.address}>` : (a.name ?? a.address ?? ""));

/** Prototype `#searchPreview` (`renderPreview`). */
export function ReadingPane({ mailId }: { mailId: number | null }) {
  const { t, i18n } = useTranslation("search");
  const s = useStyles();
  const q = useQuery({ queryKey: ["mailGet", mailId], queryFn: () => api.mailGet(mailId as number), enabled: mailId !== null });

  let content;
  if (mailId === null) {
    content = <EmptyState icon={<MailRegular />} title={t("preview.pickTitle")} body={t("preview.pickBody")} />;
  } else if (q.error) {
    content = (
      <MessageBar intent="error">
        <MessageBarBody>{formatAppError(toAppError(q.error), t)}</MessageBarBody>
      </MessageBar>
    );
  } else if (!q.data) {
    content = (
      <div className={s.center}>
        <Spinner />
      </div>
    );
  } else {
    const m = q.data;
    const lines = (["to", "cc", "bcc"] as const).filter((k) => m[k].length > 0);
    content = (
      <>
        <div className={s.head}>
          <Title3>{m.subject || t("preview.noSubject")}</Title3>
          <div className={s.meta}>
            <Persona className={s.persona} name={m.from.name ?? m.from.address ?? ""} avatar={{ size: 40 }}
              primaryText={{ className: s.ellipsis }} secondaryText={{ className: s.ellipsis, children: m.from.address ?? "" }} />
            <span className={s.spacer} />
            <Caption1 className={s.date} title={t("preview.date")}>{formatDateTime(m.sentAt, i18n.language)}</Caption1>
          </div>
          {lines.map((k) => (
            <div key={k} className={s.line}>
              <Caption1>{t(`preview.${k}`)}</Caption1>
              <span>{m[k].map(fmt).join(", ")}</span>
            </div>
          ))}
          <div className={s.chips}>
            <Caption1>{t("preview.foundIn")}</Caption1>
            {m.occurrences.map((o) => (
              <Badge key={`${o.pstId}-${o.pstItemId}`} appearance="tint" color="informative">
                {`${o.pstDisplayName} · ${o.folderPath}`}
              </Badge>
            ))}
          </div>
          {m.attachments.length > 0 ? (
            <div className={s.atts}>
              <Caption1 className={s.muted}>{t("preview.attachments")}</Caption1>
              <div className={s.chips}>
                {m.attachments.map((a) => (
                  <AttachmentCard key={a.id} attachment={a} />
                ))}
              </div>
            </div>
          ) : null}
        </div>
        <div className={s.body}>
          {m.bodyHtml !== null ? (
            <MailPreview html={m.bodyHtml} title={m.subject || t("preview.noSubject")} />
          ) : (
            <pre className={s.text}>{m.bodyText}</pre>
          )}
        </div>
      </>
    );
  }
  return (
    <div className={s.root} data-testid="reading-pane">
      {content}
    </div>
  );
}
