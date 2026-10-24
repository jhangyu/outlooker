import { Button, Caption1, Text, Tooltip, makeStyles, tokens } from "@fluentui/react-components";
import { ArrowDownloadRegular, AttachRegular } from "@fluentui/react-icons";
import { useTranslation } from "react-i18next";
import type { AttachmentMeta } from "../../api/bindings/AttachmentMeta";
import { ellipsis, formatBytes } from "../common";

const useStyles = makeStyles({
  row: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS, minHeight: "32px" },
  text: { display: "flex", flexDirection: "column", minWidth: 0, flex: 1 },
  name: { ...ellipsis },
});

/** Compact attachment row for the header popover. Metadata only; download comes in a later phase. */
export function AttachmentRow({ attachment: a }: { attachment: AttachmentMeta }) {
  const { t, i18n } = useTranslation("search");
  const s = useStyles();
  return (
    <div className={s.row}>
      <AttachRegular aria-hidden />
      <div className={s.text}>
        <Text className={s.name} title={a.fileName}>{a.fileName}</Text>
        <Caption1 className={s.name} title={a.mimeType}>{`${formatBytes(a.sizeBytes, i18n.language)} · ${a.mimeType}`}</Caption1>
      </div>
      {/* Disabled buttons get no pointer events, so the Tooltip wraps a span. */}
      <Tooltip content={t("preview.downloadLater")} relationship="description">
        <span>
          <Button appearance="subtle" size="small" disabled icon={<ArrowDownloadRegular />} aria-label={t("preview.download", { name: a.fileName })} />
        </span>
      </Tooltip>
    </div>
  );
}
