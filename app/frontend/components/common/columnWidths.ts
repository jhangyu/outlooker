import {
  createTableColumn, useTableColumnSizing_unstable, useTableFeatures,
  type TableColumnId, type TableColumnSizingOptions,
} from "@fluentui/react-components";
import { useCallback, useMemo, type KeyboardEvent, type ReactNode } from "react";
import { readNumbers, writeNumber } from "./splits";

export type ColumnDefaults = Record<string, { width: number; minWidth?: number }>;

const PREFIX = "outlooker.columnWidths.";
const DEFAULT_MIN_WIDTH = 48;

/**
 * Persistence + Fluent sizing params. Spread onto <DataGrid>, or feed to useGridColumnSizing.
 * `defaults` must be referentially stable. Only defaultWidth/minWidth are set (no idealWidth), so a new
 * options reference never resets widths of existing columns.
 */
export function useColumnWidths(surfaceKey: string, defaults: ColumnDefaults) {
  const columnSizingOptions = useMemo<TableColumnSizingOptions>(() => {
    const stored = readNumbers(PREFIX + surfaceKey);
    return Object.fromEntries(
      Object.entries(defaults).map(([id, d]) => [id, { defaultWidth: stored[id] ?? d.width, minWidth: d.minWidth ?? DEFAULT_MIN_WIDTH }]),
    );
  }, [surfaceKey, defaults]);

  const onColumnResize = useCallback(
    (_e: unknown, d: { columnId: TableColumnId; width: number }) => {
      writeNumber(PREFIX + surfaceKey, String(d.columnId), d.width);
    },
    [surfaceKey],
  );

  return {
    resizableColumns: true as const,
    resizableColumnsOptions: { autoFitColumns: false as const },
    columnSizingOptions,
    onColumnResize,
  };
}

/** Hand-rolled grid adapter: Fluent's resize engine without DataGrid. */
export function useGridColumnSizing(surfaceKey: string, defaults: ColumnDefaults) {
  const sizing = useColumnWidths(surfaceKey, defaults);
  const columns = useMemo(() => Object.keys(defaults).map((columnId) => createTableColumn<never>({ columnId })), [defaults]);
  const { columnSizing_unstable: cs } = useTableFeatures({ columns, items: [] }, [
    useTableColumnSizing_unstable({
      columnSizingOptions: sizing.columnSizingOptions,
      onColumnResize: sizing.onColumnResize,
      autoFitColumns: false,
    }),
  ]);

  const list = cs.getColumnWidths();
  const widths = Object.fromEntries(list.map((c) => [String(c.columnId), c.width]));
  const total = list.reduce((sum, c) => sum + c.width, 0);

  const headerProps = (id: string): { aside: ReactNode; onKeyDown: (e: KeyboardEvent) => void } => ({
    // Typed as a slot shorthand, but the hook always builds a TableResizeHandle element (or null).
    aside: cs.getTableHeaderCellProps(id).aside as ReactNode,
    onKeyDown: (e) => {
      if (e.key === "Enter" || e.key === " ") cs.enableKeyboardMode(id)(e as never);
    },
  });

  return { widths, total, headerProps };
}
