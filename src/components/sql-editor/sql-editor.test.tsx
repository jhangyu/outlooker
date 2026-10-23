import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { SqlEditor, codePointOffsetToUtf16 } from ".";

const props = { onChange: vi.fn(), onRun: vi.fn(), errorPosition: null, ariaLabel: "SQL editor" };

describe("SQL editor", () => {
  it("codePointOffsetToUtf16 converts a 1-based code point offset", () => {
    const text = "SELECT 😀 x"; // 10 code points, 11 UTF-16 units
    expect(codePointOffsetToUtf16(text, 1)).toBe(0);
    expect(codePointOffsetToUtf16(text, 8)).toBe(7);
    expect(codePointOffsetToUtf16(text, 9)).toBe(9);
    expect(codePointOffsetToUtf16(text, 10)).toBe(10);
    expect(codePointOffsetToUtf16(text, 99)).toBe(11);
  });

  it("renders the document, replaces it from props and runs on Mod-Enter", () => {
    const onRun = vi.fn();
    const r = render(<SqlEditor {...props} value="SELECT 1" onRun={onRun} />);
    const content = screen.getByTestId("sql-editor");
    expect(content.getAttribute("aria-label")).toBe("SQL editor");
    expect(content.textContent).toBe("SELECT 1");
    fireEvent.keyDown(content, { key: "Enter", ctrlKey: true });
    expect(onRun).toHaveBeenCalledTimes(1);
    r.rerender(<SqlEditor {...props} value="SELECT 2" onRun={onRun} />);
    expect(screen.getByTestId("sql-editor").textContent).toBe("SELECT 2");
  });

  it("sqlError position marks the character in the editor", () => {
    const r = render(<SqlEditor {...props} value="SELECT 😀 x" errorPosition={10} />);
    expect(r.container.querySelector(".cm-sql-error")?.textContent).toBe("x");
    r.rerender(<SqlEditor {...props} value="SELECT 😀 x" errorPosition={8} />);
    expect(r.container.querySelector(".cm-sql-error")?.textContent).toBe("😀");
    r.rerender(<SqlEditor {...props} value="SELECT 😀 x" errorPosition={null} />);
    expect(r.container.querySelector(".cm-sql-error")).toBeNull();
  });
});
