import {
  Accordion, AccordionHeader, AccordionItem, AccordionPanel, Button, Caption1, Subtitle2, makeStyles, mergeClasses, tokens,
} from "@fluentui/react-components";
import { AddRegular, DocumentRegular } from "@fluentui/react-icons";
import { useTranslation } from "react-i18next";
import type { SavedQuery } from "../../api/bindings/SavedQuery";
import { VIEW_COLUMNS } from "../../components/sql-editor";

const useStyles = makeStyles({
  root: { display: "flex", flexDirection: "column", minHeight: 0, borderRight: `1px solid ${tokens.colorNeutralStroke2}`, backgroundColor: tokens.colorNeutralBackground2 },
  head: { display: "flex", alignItems: "center", minHeight: "44px", padding: `0 ${tokens.spacingHorizontalM}`, borderBottom: `1px solid ${tokens.colorNeutralStroke2}` },
  spacer: { flex: 1 },
  list: { flex: 1, overflowY: "auto", minHeight: 0, padding: tokens.spacingHorizontalXS },
  item: { width: "100%", justifyContent: "flex-start", fontWeight: tokens.fontWeightRegular },
  picked: { backgroundColor: tokens.colorBrandBackground2, fontWeight: tokens.fontWeightSemibold },
  empty: { padding: tokens.spacingHorizontalM, color: tokens.colorNeutralForeground3 },
  schema: { maxHeight: "45%", overflowY: "auto", borderTop: `1px solid ${tokens.colorNeutralStroke2}` },
  col: { display: "flex", gap: tokens.spacingHorizontalS, fontFamily: tokens.fontFamilyMonospace, fontSize: tokens.fontSizeBase200, paddingLeft: tokens.spacingHorizontalM },
  type: { color: tokens.colorNeutralForeground3 },
  view: { fontFamily: tokens.fontFamilyMonospace, fontWeight: tokens.fontWeightSemibold },
});

/** Prototype `#savedList` + `#schema` (column 1). */
export function SavedQueries({ items, selectedId, onPick, onNew }: {
  items: SavedQuery[];
  selectedId: number | null;
  onPick(q: SavedQuery): void;
  onNew(): void;
}) {
  const { t } = useTranslation("sql");
  const s = useStyles();
  return (
    <section className={s.root}>
      <div className={s.head}>
        <Subtitle2>{t("saved")}</Subtitle2>
        <span className={s.spacer} />
        <Button appearance="subtle" icon={<AddRegular />} aria-label={t("new")} title={t("new")} data-testid="sql-new" onClick={onNew} />
      </div>
      <div className={s.list}>
        {items.length === 0 ? <Caption1 className={s.empty}>{t("savedEmpty")}</Caption1> : null}
        {items.map((q) => (
          <Button key={q.id} appearance="subtle" icon={<DocumentRegular />} data-testid="saved-query" data-name={q.name}
            aria-current={q.id === selectedId ? "true" : undefined}
            className={mergeClasses(s.item, q.id === selectedId && s.picked)} onClick={() => onPick(q)}>
            {q.name}
          </Button>
        ))}
      </div>
      <Accordion collapsible className={s.schema}>
        <AccordionItem value="schema">
          <AccordionHeader>{t("schema")}</AccordionHeader>
          <AccordionPanel>
            {Object.entries(VIEW_COLUMNS).map(([view, cols]) => (
              <div key={view}>
                <div className={s.view}>{view}</div>
                {cols.map(([name, type]) => (
                  <div key={name} className={s.col}>
                    <span>{name}</span>
                    <span className={s.type}>{type}</span>
                  </div>
                ))}
              </div>
            ))}
          </AccordionPanel>
        </AccordionItem>
      </Accordion>
    </section>
  );
}
