import {
  createTableColumn, DataGrid, DataGridBody, DataGridHeader, DataGridHeaderCell, DataGridRow, FluentProvider, webLightTheme,
} from "@fluentui/react-components";
import { act, render, renderHook, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useColumnWidths, type ColumnDefaults } from "./columnWidths";

const DEFAULTS: ColumnDefaults = { a: { width: 100 }, b: { width: 120 } };
const KEY = "outlooker.columnWidths.t";

describe("useColumnWidths", () => {
  it("writes a resized width to localStorage", () => {
    const { result } = renderHook(() => useColumnWidths("t", DEFAULTS));
    act(() => result.current.onColumnResize(undefined, { columnId: "a", width: 300.4 }));
    expect(JSON.parse(localStorage.getItem(KEY)!)).toEqual({ a: 300 });
  });

  it("restores a stored width on remount and keeps unknown ids", () => {
    const first = renderHook(() => useColumnWidths("t", DEFAULTS));
    act(() => first.result.current.onColumnResize(undefined, { columnId: "a", width: 300 }));
    first.unmount();
    const second = renderHook(() => useColumnWidths("t", DEFAULTS));
    expect(second.result.current.columnSizingOptions.a.defaultWidth).toBe(300);
    expect(second.result.current.columnSizingOptions.b.defaultWidth).toBe(120);
    expect(second.result.current.columnSizingOptions.b.minWidth).toBe(48);
    localStorage.setItem(KEY, JSON.stringify({ a: 300, gone: 77 }));
    act(() => second.result.current.onColumnResize(undefined, { columnId: "b", width: 150 }));
    expect(JSON.parse(localStorage.getItem(KEY)!)).toEqual({ a: 300, gone: 77, b: 150 });
  });

  it("falls back to defaults on malformed storage", () => {
    for (const bad of ["{", "[1]", '{"a":"x","b":null}']) {
      localStorage.setItem(KEY, bad);
      const { result } = renderHook(() => useColumnWidths("t", DEFAULTS));
      expect(result.current.columnSizingOptions.a.defaultWidth).toBe(100);
      expect(result.current.columnSizingOptions.b.defaultWidth).toBe(120);
    }
  });

  it("spread onto DataGrid renders the stored width", () => {
    localStorage.setItem("outlooker.columnWidths.t2", JSON.stringify({ a: 240 }));
    const cols = [
      createTableColumn<{ id: string }>({ columnId: "a", renderHeaderCell: () => "HA", renderCell: (x) => x.id }),
      createTableColumn<{ id: string }>({ columnId: "b", renderHeaderCell: () => "HB", renderCell: (x) => x.id }),
    ];
    function Grid() {
      const sizing = useColumnWidths("t2", DEFAULTS);
      return (
        <FluentProvider theme={webLightTheme}>
          <DataGrid {...sizing} items={[]} columns={cols} getRowId={(x) => x.id}>
            <DataGridHeader>
              <DataGridRow>{({ renderHeaderCell }) => <DataGridHeaderCell>{renderHeaderCell()}</DataGridHeaderCell>}</DataGridRow>
            </DataGridHeader>
            <DataGridBody>{() => null}</DataGridBody>
          </DataGrid>
        </FluentProvider>
      );
    }
    render(<Grid />);
    expect(screen.getByText("HA").closest<HTMLElement>("[role=columnheader]")?.style.width).toBe("240px");
    expect(screen.getByText("HB").closest<HTMLElement>("[role=columnheader]")?.style.width).toBe("120px");
  });
});
