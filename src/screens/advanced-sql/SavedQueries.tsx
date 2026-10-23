import {
  Accordion, AccordionHeader, AccordionItem, AccordionPanel, Body1Strong, Button, Caption1, makeStyles, mergeClasses, tokens,
} from "@fluentui/react-components";
import { AddRegular, DocumentRegular } from "@fluentui/react-icons";
import { useRef } from "react";
import { useTranslation } from "react-i18next";
import type { SavedQuery } from "../../api/bindings/SavedQuery";
import { Splitter, ellipsis, useSplit, type SplitDefaults } from "../../components/common";
import { VIEW_COLUMNS } from "../../components/sql-editor";

// Sized pane = the schema (below the border); default is content height capped at 42 % (see `schema` style).
const SPLITS: SplitDefaults = { schema: { min: 60, max: 600 } };

const useStyles = makeStyles({
  root: { display: "flex", flexDirection: "column", minHeight: 0, minWidth: 0, overflow: "hidden", borderRight: `1px solid ${tokens.colorNeutralStroke2}` },
  head: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS, minHeight: "44px", padding: `0 ${tokens.spacingHorizontalM}`, borderBottom: `1px solid ${tokens.colorNeutralStroke2}` },
  title: { flex: 1, ...ellipsis },
  list: { flex: 1, overflowY: "auto", minHeight: 0 },
  item: {
    width: "100%", minWidth: 0, justifyContent: "flex-start", fontWeight: tokens.fontWeightRegular, borderRadius: 0,
    padding: `${tokens.spacingVerticalS} ${tokens.spacingHorizontalM}`, borderBottom: `1px solid ${tokens.colorNeutralStroke3}`,
  },
  itemText: { ...ellipsis },
  picked: { backgroundColor: tokens.colorBrandBackground2, fontWeight: tokens.fontWeightSemibold, boxShadow: `inset 3px 0 0 ${tokens.colorCompoundBrandStroke}` },
  empty: { padding: tokens.spacingHorizontalM, color: tokens.colorNeutralForeground3 },
  schema: { maxHeight: "42%", overflowY: "auto", borderTop: `1px solid ${tokens.colorNeutralStroke2}` },
  // Zero-height flex item sitting exactly on the list | schema border; the splitter hangs off it.
  seam: { position: "relative", height: 0, flexShrink: 0 },
  col: { display: "flex", whiteSpace: "nowrap", gap: tokens.spacingHorizontalS, fontFamily: tokens.fontFamilyMonospace, fontSize: tokens.fontSizeBase200, paddingLeft: tokens.spacingHorizontalM },
  name: { ...ellipsis },
  type: { flexShrink: 0, color: tokens.colorNeutralForeground3 },
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
  const split = useSplit("advanced-sql-saved", SPLITS);
  const seam = useRef<HTMLDivElement>(null);
  const schemaHeight = split.sizes.schema;
  return (
    <section className={s.root}>
      <div className={s.head}>
        <Body1Strong className={s.title}>{t("saved")}</Body1Strong>
        <Button appearance="subtle" icon={<AddRegular />} aria-label={t("new")} title={t("new")} data-testid="sql-new" onClick={onNew} />
      </div>
      <div className={s.list}>
        {items.length === 0 ? <Caption1 className={s.empty}>{t("savedEmpty")}</Caption1> : null}
        {items.map((q) => (
          <Button key={q.id} appearance="subtle" icon={<DocumentRegular />} data-testid="saved-query" data-name={q.name}
            aria-current={q.id === selectedId ? "true" : undefined} title={q.name}
            className={mergeClasses(s.item, q.id === selectedId && s.picked)} onClick={() => onPick(q)}>
            <span className={s.itemText}>{q.name}</span>
          </Button>
        ))}
      </div>
      <div ref={seam} className={s.seam}>
        <Splitter
          orientation="horizontal" reverse {...split.props("schema")}
          measure={() => (seam.current?.nextElementSibling as HTMLElement | null)?.offsetHeight ?? 0}
          style={{ top: -3, left: 0, right: 0 }}
        />
      </div>
      <Accordion collapsible className={s.schema} style={schemaHeight === undefined ? undefined : { height: schemaHeight, maxHeight: "70%" }}>
        <AccordionItem value="schema">
          <AccordionHeader>{t("schema")}</AccordionHeader>
          <AccordionPanel>
            {Object.entries(VIEW_COLUMNS).map(([view, cols]) => (
              <div key={view}>
                <div className={s.view}>{view}</div>
                {cols.map(([name, type]) => (
                  <div key={name} className={s.col}>
                    <span className={s.name}>{name}</span>
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
