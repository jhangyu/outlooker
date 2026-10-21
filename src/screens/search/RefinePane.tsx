import {
  Caption1Strong, Checkbox, CounterBadge, Subtitle2, Tree, TreeItem, TreeItemLayout, makeStyles, mergeClasses, tokens,
} from "@fluentui/react-components";
import { DocumentRegular, FolderRegular, MailInboxRegular } from "@fluentui/react-icons";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import type { FilterOptions } from "../../api/bindings/FilterOptions";
import { buildFolderTree, type FolderTreeNode } from "../../components/dsl";
import type { RefineState } from "./compose";

const useStyles = makeStyles({
  root: {
    display: "flex", flexDirection: "column", gap: tokens.spacingVerticalS, minHeight: 0, overflowY: "auto",
    padding: tokens.spacingHorizontalM, backgroundColor: tokens.colorNeutralBackground2, borderRight: `1px solid ${tokens.colorNeutralStroke2}`,
  },
  section: { marginTop: tokens.spacingVerticalM, color: tokens.colorNeutralForeground3 },
  picked: { backgroundColor: tokens.colorBrandBackground2, fontWeight: tokens.fontWeightSemibold },
});

function FolderItems({ nodes, value, onPick }: { nodes: FolderTreeNode[]; value: RefineState; onPick(path: string): void }) {
  const s = useStyles();
  return (
    <>
      {nodes.map((n) => (
        <TreeItem key={n.path} itemType={n.children.length > 0 ? "branch" : "leaf"} value={`folder:${n.path}`}>
          <TreeItemLayout iconBefore={<FolderRegular />} onClick={() => onPick(n.path)}
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
      <Subtitle2>{t("refine")}</Subtitle2>
      <Tree aria-label={t("sources")}>
        <TreeItem itemType="leaf" value="all">
          <TreeItemLayout iconBefore={<MailInboxRegular />} className={mergeClasses(all && s.picked)}
            onClick={() => onChange({ ...value, pstId: null, folderPath: null })}>
            {t("allMail")}
          </TreeItemLayout>
        </TreeItem>
        {(options?.psts ?? []).map((p) => (
          <TreeItem key={p.id} itemType="leaf" value={`pst:${p.id}`}>
            <TreeItemLayout iconBefore={<DocumentRegular />} className={mergeClasses(value.pstId === p.id && s.picked)}
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
      <Checkbox label={t("hasAttachment")} checked={value.hasAttachment} onChange={(_, d) => onChange({ ...value, hasAttachment: d.checked === true })} />
      <Checkbox label={t("last90Days")} checked={value.last90Days} onChange={(_, d) => onChange({ ...value, last90Days: d.checked === true })} />
    </aside>
  );
}
