import { makeStyles, mergeClasses, tokens } from "@fluentui/react-components";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useRef } from "react";
import { useTranslation } from "react-i18next";
import type { ColumnMeta } from "../../api/bindings/ColumnMeta";
import { ROW_HEIGHT } from "../../components/mail";

/** Part 03 §6.2: numeric cells are right-aligned and never parsed. */
export const NUMERIC_TYPES = new Set(["int2", "int4", "int8", "float4", "float8", "numeric"]);

const useStyles = makeStyles({
  scroll: { height: "100%", overflow: "auto", position: "relative" },
  head: {
    display: "grid", position: "sticky", top: 0, zIndex: 1, height: `${ROW_HEIGHT}px`, alignItems: "center",
    backgroundColor: tokens.colorNeutralBackground2, borderBottom: `1px solid ${tokens.colorNeutralStroke2}`,
    fontSize: tokens.fontSizeBase200, fontWeight: tokens.fontWeightSemibold,
  },
  row: {
    display: "grid", position: "absolute", top: 0, left: 0, width: "100%", height: `${ROW_HEIGHT}px`, alignItems: "center",
    boxSizing: "border-box", borderBottom: `1px solid ${tokens.colorNeutralStroke3}`, cursor: "pointer",
    ":hover": { backgroundColor: tokens.colorNeutralBackground1Hover },
  },
  selected: { backgroundColor: tokens.colorBrandBackground2, ":hover": { backgroundColor: tokens.colorBrandBackground2 } },
  cell: { padding: `0 ${tokens.spacingHorizontalS}`, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" },
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
  const grid = { gridTemplateColumns: `repeat(${columns.length}, minmax(120px, 1fr))`, minWidth: `${columns.length * 120}px` };
  return (
    <div ref={ref} className={s.scroll} role="table" aria-rowcount={rows.length + 1}>
      <div role="row" className={s.head} style={grid}>
        {columns.map((c, j) => (
          <span key={j} role="columnheader" title={c.pgType} className={mergeClasses(s.cell, NUMERIC_TYPES.has(c.pgType) && s.num)}>
            {c.name}
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
