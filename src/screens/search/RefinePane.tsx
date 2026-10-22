import {
  Body1Strong, Caption1Strong, Checkbox, CounterBadge, Tree, TreeItem, TreeItemLayout, makeStyles, mergeClasses, tokens,
} from "@fluentui/react-components";
import { DocumentRegular, FolderRegular, MailInboxRegular } from "@fluentui/react-icons";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import type { FilterOptions } from "../../api/bindings/FilterOptions";
import { buildFolderTree, type FolderTreeNode } from "../../components/dsl";
import type { RefineState } from "./compose";

const useStyles = makeStyles({
  root: {
    display: "flex", flexDirection: "column", minHeight: 0, minWidth: 0,
    backgroundColor: tokens.colorNeutralBackground1, borderRight: `1px solid ${tokens.colorNeutralStroke2}`,
  },
  head: {
    display: "flex", alignItems: "center", minHeight: "44px", boxSizing: "border-box",
    padding: `${tokens.spacingVerticalS} ${tokens.spacingHorizontalM}`, borderBottom: `1px solid ${tokens.colorNeutralStroke2}`,
  },
  scroll: { display: "flex", flexDirection: "column", flex: 1, minHeight: 0, overflowY: "auto", padding: `${tokens.spacingVerticalS} 0` },
  section: {
    padding: `${tokens.spacingVerticalS} ${tokens.spacingHorizontalM} ${tokens.spacingVerticalXS}`, color: tokens.colorNeutralForeground3,
  },
  label: { overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", minWidth: 0 },
  check: { paddingLeft: tokens.spacingHorizontalM },
  picked: { backgroundColor: tokens.colorBrandBackground2, fontWeight: tokens.fontWeightSemibold },
});

function FolderItems({ nodes, value, onPick }: { nodes: FolderTreeNode[]; value: RefineState; onPick(path: string): void }) {
  const s = useStyles();
  return (
    <>
      {nodes.map((n) => (
        <TreeItem key={n.path} itemType={n.children.length > 0 ? "branch" : "leaf"} value={`folder:${n.path}`}>
          <TreeItemLayout iconBefore={<FolderRegular />} onClick={() => onPick(n.path)} main={{ className: s.label, title: n.name }}
            className={mergeClasses(value.folderPath === n.path && s.picked)}
            aside={<CounterBadge count={n.count} overflowCount={9_999_999} size="small" appearance="ghost" />}>
            {n.name}
          </TreeItemLayout>
          {n.children.length > 0 ? (
            <Tree>
              <FolderItems nodes={n.children} value={value} onPick={onPick} />
            </Tree>
          ) : null}
        </TreeItem>
      ))}
    </>
  );
}

/** Prototype `#facets`: All mail, PST files, folder tree, quick filters. */
export function RefinePane({ value, onChange, options }: { value: RefineState; onChange(next: RefineState): void; options: FilterOptions | undefined }) {
  const { t } = useTranslation("search");
  const s = useStyles();
  const folders = useMemo(() => buildFolderTree(options?.folders ?? []), [options]);
  const all = value.pstId === null && value.folderPath === null;
  return (
    <aside className={s.root} aria-label={t("refine")}>
      <div className={s.head}>
        <Body1Strong>{t("refine")}</Body1Strong>
      </div>
      <div className={s.scroll}>
        <Caption1Strong className={s.section}>{t("sources")}</Caption1Strong>
        <Tree aria-label={t("sources")}>
          <TreeItem itemType="leaf" value="all">
            <TreeItemLayout iconBefore={<MailInboxRegular />} className={mergeClasses(all && s.picked)} main={{ className: s.label }}
              onClick={() => onChange({ ...value, pstId: null, folderPath: null })}>
              {t("allMail")}
            </TreeItemLayout>
          </TreeItem>
          {(options?.psts ?? []).map((p) => (
            <TreeItem key={p.id} itemType="leaf" value={`pst:${p.id}`}>
              <TreeItemLayout iconBefore={<DocumentRegular />} className={mergeClasses(value.pstId === p.id && s.picked)}
                main={{ className: s.label, title: p.displayName }}
                onClick={() => onChange({ ...value, pstId: value.pstId === p.id ? null : p.id })}>
                {p.displayName}
              </TreeItemLayout>
            </TreeItem>
          ))}
        </Tree>
        <Caption1Strong className={s.section}>{t("folders")}</Caption1Strong>
        <Tree aria-label={t("folders")}>
          <FolderItems nodes={folders} value={value}
            onPick={(path) => onChange({ ...value, folderPath: value.folderPath === path ? null : path })} />
        </Tree>
        <Caption1Strong className={s.section}>{t("quick")}</Caption1Strong>
        <Checkbox className={s.check} label={t("hasAttachment")} checked={value.hasAttachment} onChange={(_, d) => onChange({ ...value, hasAttachment: d.checked === true })} />
        <Checkbox className={s.check} label={t("last90Days")} checked={value.last90Days} onChange={(_, d) => onChange({ ...value, last90Days: d.checked === true })} />
      </div>
    </aside>
  );
}
