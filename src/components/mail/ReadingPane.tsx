import { MessageBar, MessageBarBody, Spinner, makeStyles, tokens } from "@fluentui/react-components";
import { MailRegular } from "@fluentui/react-icons";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api } from "../../api/commands";
import { EmptyState, formatAppError, toAppError } from "../common";
import { MessageHeader } from "./MessageHeader";
import { MailPreview } from "./MailPreview";

const useStyles = makeStyles({
  root: { display: "flex", flexDirection: "column", height: "100%", minHeight: 0, backgroundColor: tokens.colorNeutralBackground1 },
  body: { flex: 1, minHeight: 0, overflow: "auto" },
  text: {
    margin: 0, padding: `${tokens.spacingVerticalL} ${tokens.spacingHorizontalXXL}`, maxWidth: "900px", lineHeight: "22px",
    whiteSpace: "pre-wrap", fontFamily: tokens.fontFamilyBase,
  },
  center: { display: "grid", placeItems: "center", height: "100%" },
});

/** Prototype `#searchPreview` (`renderPreview`). */
export function ReadingPane({ mailId }: { mailId: number | null }) {
  const { t } = useTranslation("search");
  const s = useStyles();
  const q = useQuery({ queryKey: ["mailGet", mailId], queryFn: () => api.mailGet(mailId as number), enabled: mailId !== null, gcTime: 30_000 });

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
    content = (
      <>
        <MessageHeader m={m} />
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
