import { Body1Strong, Button, Caption1, Card, Tooltip, makeStyles, tokens } from "@fluentui/react-components";
import { ArrowDownloadRegular } from "@fluentui/react-icons";
import { useTranslation } from "react-i18next";
import type { AttachmentMeta } from "../../api/bindings/AttachmentMeta";
import { ellipsis, formatBytes } from "../common";

const useStyles = makeStyles({
  card: { display: "flex", flexDirection: "row", alignItems: "center", gap: tokens.spacingHorizontalS, width: "260px" },
  tile: {
    width: "36px", height: "36px", display: "grid", placeItems: "center", flexShrink: 0,
    borderRadius: tokens.borderRadiusMedium, backgroundColor: tokens.colorBrandBackground2, color: tokens.colorBrandForeground1,
    fontSize: tokens.fontSizeBase100, fontWeight: tokens.fontWeightSemibold,
  },
  text: { display: "flex", flexDirection: "column", minWidth: 0, flex: 1 },
  name: { ...ellipsis },
});

/** Metadata only; download comes in a later phase (requirements: attachments are re-read from the PST later). */
export function AttachmentCard({ attachment: a }: { attachment: AttachmentMeta }) {
  const { t, i18n } = useTranslation("search");
  const s = useStyles();
  const ext = a.fileName.includes(".") ? (a.fileName.split(".").pop() ?? "").toUpperCase().slice(0, 4) : "";
  return (
    <Card size="small" className={s.card}>
      <span className={s.tile} aria-hidden>
        {ext}
      </span>
      <div className={s.text}>
        <Body1Strong className={s.name} title={a.fileName}>{a.fileName}</Body1Strong>
        <Caption1 className={s.name} title={a.mimeType}>{`${formatBytes(a.sizeBytes, i18n.language)} · ${a.mimeType}`}</Caption1>
      </div>
      {/* Disabled buttons get no pointer events, so the Tooltip wraps a span. */}
      <Tooltip content={t("preview.downloadLater")} relationship="description">
        <span>
          <Button appearance="subtle" disabled icon={<ArrowDownloadRegular />} aria-label={t("preview.download", { name: a.fileName })} />
        </span>
      </Tooltip>
    </Card>
  );
}
