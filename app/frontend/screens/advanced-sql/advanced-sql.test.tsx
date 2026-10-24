import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api/commands");
vi.mock("../../api/events", async () => (await import("../../app/testing")).eventsMock);
// The CodeMirror wrapper has its own test; here a textarea stands in so the screen logic runs without layout.
vi.mock("../../components/sql-editor", async () => {
  const { createElement } = await import("react");
  return {
    VIEW_COLUMNS: { v_mail: [["id", "int8"]] },
    SqlEditor: (p: { value: string; onChange(v: string): void; ariaLabel: string }) =>
      createElement("textarea", { "data-testid": "sql-editor", "aria-label": p.ariaLabel, value: p.value, onChange: (e: { target: { value: string } }) => p.onChange(e.target.value) }),
  };
});

import { api } from "../../api/commands";
import type { SqlResult } from "../../api/bindings/SqlResult";
import { mockApiDefaults, renderWithProviders } from "../../app/testing";
import { makeMailRow, stubViewport } from "../../components/mail/fixtures";
import { AdvancedSqlScreen } from ".";

const READY = { status: { counts: { messages: 10, psts: 1 } } };
const TABLE: SqlResult = {
  mode: "table",
  columns: [{ name: "kind", pgType: "text" }, { name: "n", pgType: "int8" }, { name: "note", pgType: "text" }],
  rows: [["to", "12", null], ["cc", "3", "x"]],
  truncated: false,
  elapsedMs: 4,
};

function runSql(sql: string) {
  fireEvent.change(screen.getByTestId("sql-editor"), { target: { value: sql } });
  fireEvent.click(screen.getByTestId("sql-run"));
}

describe("Advanced SQL screen", () => {
  let restore: () => void;
  beforeEach(() => {
    mockApiDefaults();
    restore = stubViewport(800);
  });
  afterEach(() => restore());

  it("sqlExecute is called with maxRows: 10000", async () => {
    renderWithProviders(<AdvancedSqlScreen />, READY);
    runSql("SELECT 1");
    await waitFor(() =>
      expect(api.sqlExecute).toHaveBeenCalledWith({ executionId: expect.stringMatching(/^[0-9a-f-]{36}$/), sql: "SELECT 1", maxRows: 10000 }),
    );
  });

  it("Advanced SQL shows MailList for a result with an id column and a table otherwise", async () => {
    vi.mocked(api.sqlExecute).mockResolvedValueOnce({
      mode: "mailList", rows: [makeMailRow(7, { subject: "from sql" })], missingIds: 1, truncated: false, elapsedMs: 2,
    });
    renderWithProviders(<AdvancedSqlScreen />, READY);
    runSql("SELECT id, subject FROM v_mail");
    await waitFor(() => expect(screen.getByTestId("sql-result").getAttribute("data-mode")).toBe("mailList"));
    expect(screen.getAllByTestId("mail-row")).toHaveLength(1);
    await waitFor(() => expect(api.mailGet).toHaveBeenCalledWith(7));
    vi.mocked(api.sqlExecute).mockResolvedValueOnce(TABLE);
    runSql("SELECT kind, n, note FROM t");
    await waitFor(() => expect(screen.getByTestId("sql-result").getAttribute("data-mode")).toBe("table"));
    expect(screen.getAllByTestId("sql-table-row")).toHaveLength(2);
    expect(screen.queryByTestId("mail-row")).toBeNull();
  });

  it("Run is disabled while executing", async () => {
    let finish!: (r: SqlResult) => void;
    vi.mocked(api.sqlExecute).mockReturnValue(new Promise<SqlResult>((r) => (finish = r)));
    renderWithProviders(<AdvancedSqlScreen />, READY);
    runSql("SELECT pg_sleep(10)");
    await waitFor(() => expect((screen.getByTestId("sql-run") as HTMLButtonElement).disabled).toBe(true));
    const sent = vi.mocked(api.sqlExecute).mock.calls[0]![0];
    fireEvent.click(screen.getByTestId("sql-cancel"));
    await waitFor(() => expect(api.sqlCancel).toHaveBeenCalledWith(sent.executionId));
    finish({ mode: "command", tag: "SELECT", elapsedMs: 1 });
    await waitFor(() => expect((screen.getByTestId("sql-run") as HTMLButtonElement).disabled).toBe(false));
    expect((screen.getByTestId("sql-cancel") as HTMLButtonElement).disabled).toBe(true);
  });

  it("shows sqlError text with its position", async () => {
    vi.mocked(api.sqlExecute).mockRejectedValue({ kind: "sqlError", sqlstate: "42601", message: 'syntax error at or near "SELEC"', position: 1 });
    renderWithProviders(<AdvancedSqlScreen />, READY);
    runSql("SELEC 1");
    expect(await screen.findByText('syntax error at or near "SELEC" at character 1')).toBeTruthy();
  });

  it("saves a new query by name and reports a duplicate name", async () => {
    vi.mocked(api.savedQueryCreate).mockRejectedValueOnce({ kind: "invalidInput", field: "name", reason: "duplicate" });
    renderWithProviders(<AdvancedSqlScreen />, READY);
    fireEvent.change(screen.getByTestId("sql-editor"), { target: { value: "SELECT 1" } });
    fireEvent.click(screen.getByTestId("sql-save"));
    const dialog = await screen.findByRole("dialog");
    fireEvent.change(within(dialog).getByRole("textbox"), { target: { value: "ones" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    expect(await within(dialog).findByText("A saved query with this name already exists.")).toBeTruthy();
    fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(api.savedQueryCreate).toHaveBeenLastCalledWith({ name: "ones", sql: "SELECT 1" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });
});
