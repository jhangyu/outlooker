import { fireEvent, render, screen } from "@testing-library/react";
import { Activity, useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api/commands");
vi.mock("../../api/events", async () => (await import("../../app/testing")).eventsMock);

import { api } from "../../api/commands";
import { mockApiDefaults, renderWithProviders } from "../../app/testing";
import { MailList, MailPreview, ReadingPane, buildEntries } from ".";
import { makeMailDetail, makeMailRow, stubViewport } from "./fixtures";

const ROWS = [
  makeMailRow(1, { sentAt: "2026-10-03T08:00:00Z" }),
  makeMailRow(2, { sentAt: "2026-10-02T08:00:00Z" }),
  makeMailRow(3, { sentAt: "2026-09-15T08:00:00Z" }),
  makeMailRow(4, { sentAt: null }),
];
const listProps = { selectedId: null, onSelect: vi.fn(), hasMore: false, capped: false, loadingMore: false, onEndReached: vi.fn(), ariaLabel: "results" };

describe("mail components", () => {
  let restore: () => void;
  beforeEach(() => {
    mockApiDefaults();
    restore = stubViewport(800);
  });
  afterEach(() => restore());

  it('preview iframe has sandbox="" and no allow- token', () => {
    render(<MailPreview html="<p>Hi</p>" title="Hello" />);
    const frame = screen.getByTestId("mail-preview");
    expect(frame.tagName).toBe("IFRAME");
    expect(frame.getAttribute("sandbox")).toBe("");
    expect(frame.outerHTML).not.toMatch(/allow-/);
    expect(frame.getAttribute("referrerpolicy")).toBe("no-referrer");
    expect(frame.getAttribute("srcdoc")).toBe("<p>Hi</p>");
  });

  it("preview replaces srcdoc on the same iframe", () => {
    const r = render(<MailPreview html="<p>one</p>" title="a" />);
    const first = screen.getByTestId("mail-preview");
    r.rerender(<MailPreview html="<p>two</p>" title="b" />);
    const second = screen.getByTestId("mail-preview");
    expect(second).toBe(first);
    expect(second.getAttribute("srcdoc")).toBe("<p>two</p>");
  });

  it("reading pane shows the header, attachments and the plain-text fallback", async () => {
    vi.mocked(api.mailGet).mockResolvedValue(makeMailDetail({ bodyHtml: null, bodyText: "Plain body" }));
    renderWithProviders(<ReadingPane mailId={5} />);
    expect(await screen.findByText("Plain body")).toBeTruthy();
    expect(api.mailGet).toHaveBeenCalledWith(5);
    expect(screen.queryByTestId("mail-preview")).toBeNull();
    expect(screen.getByText("Quote Q1")).toBeTruthy();
    expect(screen.getByText("Bob <bob@x.example>")).toBeTruthy();
    expect(screen.getByText("archive · Inbox/Projects")).toBeTruthy();
    expect((screen.getByRole("button", { name: "Download quote.pdf" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("reading pane renders HTML through MailPreview and shows the pick state without a mail", async () => {
    vi.mocked(api.mailGet).mockResolvedValue(makeMailDetail());
    const r = renderWithProviders(<ReadingPane mailId={5} />);
    expect((await screen.findByTestId("mail-preview")).getAttribute("srcdoc")).toBe("<p>Hello</p>");
    r.unmount();
    renderWithProviders(<ReadingPane mailId={null} />);
    expect(screen.getByText("Select a message")).toBeTruthy();
  });

  it("buildEntries inserts month headers and an undated group", () => {
    expect(buildEntries(ROWS, "en", "No date").map((e) => e.kind)).toEqual(["month", "mail", "mail", "month", "mail", "month", "mail"]);
  });

  it("ArrowDown on the list selects the next mail", () => {
    const onSelect = vi.fn();
    renderWithProviders(<MailList {...listProps} rows={ROWS} selectedId={1} onSelect={onSelect} />);
    expect(screen.getAllByTestId("mail-row")).toHaveLength(4);
    expect(screen.getByText("No date")).toBeTruthy();
    fireEvent.keyDown(screen.getByRole("listbox"), { key: "ArrowDown" });
    expect(onSelect).toHaveBeenCalledWith(2);
  });

  it("requests the next page only when the end is within 50 rows", () => {
    const far = vi.fn();
    const r = renderWithProviders(
      <MailList {...listProps} rows={Array.from({ length: 200 }, (_, i) => makeMailRow(i + 1))} hasMore monthHeaders={false} onEndReached={far} />,
    );
    expect(far).not.toHaveBeenCalled();
    expect(screen.getByTestId("list-loader-row")).toBeTruthy();
    r.unmount();
    const near = vi.fn();
    renderWithProviders(
      <MailList {...listProps} rows={Array.from({ length: 60 }, (_, i) => makeMailRow(i + 1))} hasMore monthHeaders={false} onEndReached={near} />,
    );
    expect(near).toHaveBeenCalled();
  });

  it("restores scrollTop after the list is hidden and shown again", () => {
    const rows = Array.from({ length: 200 }, (_, i) => makeMailRow(i + 1));
    function Host() {
      const [hidden, setHidden] = useState(false);
      return (
        <>
          <button data-testid="toggle" onClick={() => setHidden((h) => !h)} />
          <Activity mode={hidden ? "hidden" : "visible"}>
            <MailList {...listProps} rows={rows} monthHeaders={false} />
          </Activity>
        </>
      );
    }
    renderWithProviders(<Host />);
    const list = screen.getByTestId("mail-list");
    list.scrollTop = 640;
    fireEvent.scroll(list);
    fireEvent.click(screen.getByTestId("toggle"));
    list.scrollTop = 0; // the webview drops scrollTop of a display:none subtree
    fireEvent.click(screen.getByTestId("toggle"));
    expect(screen.getByTestId("mail-list").scrollTop).toBe(640);
  });
});
