import { makeStyles, mergeClasses, tokens } from "@fluentui/react-components";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useMemo, useRef } from "react";
import { useTranslation } from "react-i18next";
import type { ColumnMeta } from "../../api/bindings/ColumnMeta";
import { ellipsis, useGridColumnSizing } from "../../components/common";
import { ROW_HEIGHT } from "../../components/mail";

/** Part 03 §6.2: numeric cells are right-aligned and never parsed. */
export const NUMERIC_TYPES = new Set(["int2", "int4", "int8", "float4", "float8", "numeric"]);

const useStyles = makeStyles({
  scroll: { height: "100%", overflow: "auto", position: "relative" },
  head: {
    display: "grid", position: "sticky", top: 0, zIndex: 1, height: `${ROW_HEIGHT}px`, alignItems: "center",
    backgroundColor: tokens.colorNeutralBackground1, borderBottom: `1px solid ${tokens.colorNeutralStroke1}`,
    fontFamily: tokens.fontFamilyMonospace, fontSize: tokens.fontSizeBase200, fontWeight: tokens.fontWeightSemibold,
  },
  row: {
    display: "grid", position: "absolute", top: 0, left: 0, width: "100%", height: `${ROW_HEIGHT}px`, alignItems: "center",
    boxSizing: "border-box", borderBottom: `1px solid ${tokens.colorNeutralStroke3}`, cursor: "pointer", fontSize: tokens.fontSizeBase200,
    ":hover": { backgroundColor: tokens.colorNeutralBackground1Hover },
  },
  selected: { backgroundColor: tokens.colorBrandBackground2, ":hover": { backgroundColor: tokens.colorBrandBackground2 } },
  headCell: { position: "relative", display: "block" },
  cell: { padding: `0 ${tokens.spacingHorizontalS}`, ...ellipsis },
  num: { textAlign: "right", fontVariantNumeric: "tabular-nums" },
  nul: { fontStyle: "italic", color: tokens.colorNeutralForeground4 },
});

export function ResultTable({ columns, rows, selected, onSelect }: {
  columns: ColumnMeta[];
  rows: Array<Array<string | null>>;
  selected: number | null;
  onSelect(index: number): void;
}) {
  const { t } = useTranslation("sql");
  const s = useStyles();
  const ref = useRef<HTMLDivElement>(null);
  const v = useVirtualizer({ count: rows.length, getScrollElement: () => ref.current, estimateSize: () => ROW_HEIGHT, overscan: 10 });
  // Stored widths are keyed by column name (a repeated name gets `#j`) so a name keeps its width across queries.
  const ids = useMemo(() => columns.map((c, j) => (columns.findIndex((x) => x.name === c.name) === j ? c.name : `${c.name}#${j}`)), [columns]);
  const defaults = useMemo(() => Object.fromEntries(ids.map((id) => [id, { width: 160 }])), [ids]);
  const { widths, total, headerProps } = useGridColumnSizing("sql-result", defaults);
  const grid = { gridTemplateColumns: ids.map((id) => `${widths[id]}px`).join(" "), minWidth: `${total}px` };
  return (
    <div ref={ref} className={s.scroll} role="table" aria-rowcount={rows.length + 1}>
      <div role="row" className={s.head} style={grid}>
        {columns.map((c, j) => (
          <span key={ids[j]} role="columnheader" tabIndex={0} title={c.pgType} className={s.headCell}
            onKeyDown={headerProps(ids[j]!).onKeyDown}>
            <span className={mergeClasses(s.cell, NUMERIC_TYPES.has(c.pgType) && s.num)} style={{ display: "block" }}>{c.name}</span>
            {headerProps(ids[j]!).aside}
          </span>
        ))}
      </div>
      <div style={{ height: `${v.getTotalSize()}px`, position: "relative", minWidth: grid.minWidth }}>
        {v.getVirtualItems().map((it) => {
          const row = rows[it.index]!;
          const pick = () => onSelect(it.index);
          return (
            <div key={it.index} role="row" tabIndex={0} aria-selected={selected === it.index} data-testid="sql-table-row"
              className={mergeClasses(s.row, selected === it.index && s.selected)}
              style={{ ...grid, transform: `translateY(${it.start}px)` }}
              onClick={pick}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  pick();
                }
              }}>
              {row.map((cell, j) => (
                <span key={j} role="cell" title={cell ?? undefined}
                  className={mergeClasses(s.cell, NUMERIC_TYPES.has(columns[j]?.pgType ?? "") && s.num, cell === null && s.nul)}>
                  {cell ?? t("result.null")}
                </span>
              ))}
            </div>
          );
        })}
      </div>
    </div>
  );
}
