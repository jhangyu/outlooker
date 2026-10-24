import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api/commands");
vi.mock("../../api/events", async () => (await import("../../app/testing")).eventsMock);

import { api } from "../../api/commands";
import type { ImportProgress } from "../../api/bindings/ImportProgress";
import { makePst, mockApiDefaults, renderWithProviders } from "../../app/testing";
import { ImportScreen } from ".";
import { applyProgress, applyStatusChanged } from "./importCache";

const progress = (p: Partial<ImportProgress> = {}): ImportProgress => ({
  pstId: 2, phase: "importing", itemsTotal: 200, itemsDone: 50, stored: 40, duplicates: 5, errors: 5,
  skippedNonMail: 0, imagesConverted: 3, bytesPerSec: 1_048_576, etaSeconds: 120, ...p,
});

describe("Import screen", () => {
  beforeEach(() => mockApiDefaults());

  it("importCache applies status changes, removal and progress without mutation", () => {
    const list = [makePst({ id: 1 }), makePst({ id: 2, status: "running" })];
    const frozen = JSON.stringify(list);
    expect(applyStatusChanged(list, { pstId: 2, status: "completed", removed: false, source: null })[1]!.status).toBe("completed");
    expect(applyStatusChanged(list, { pstId: 1, status: "completed", removed: true, source: null }).map((p) => p.id)).toEqual([2]);
    expect(applyStatusChanged(list, { pstId: 3, status: "queued", removed: false, source: makePst({ id: 3, status: "queued" }) }).length).toBe(3);
    expect(applyProgress(list, progress())[1]!.progress?.itemsDone).toBe(50);
    expect(applyProgress(list, progress({ pstId: 99 }))).toBe(list);
    expect(JSON.stringify(list)).toBe(frozen);
  });

  it("removal dialog shows messageCount and calls pstRemove only after confirm", async () => {
    vi.mocked(api.pstList).mockResolvedValue([makePst({ id: 7, displayName: "archive-2019", messageCount: 1234 })]);
    renderWithProviders(<ImportScreen />);
    fireEvent.click(await screen.findByTestId("pst-remove"));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(/1,234/)).toBeTruthy();
    expect(api.pstRemove).not.toHaveBeenCalled();
    fireEvent.click(within(dialog).getByTestId("remove-confirm"));
    await waitFor(() => expect(api.pstRemove).toHaveBeenCalledWith(7));
  });

  it("Save sends the edited schedule to syncScheduleSet", async () => {
    renderWithProviders(<ImportScreen />);
    fireEvent.click(await screen.findByRole("radio", { name: /Daily/ }));
    fireEvent.click(await screen.findByTestId("sched-save"));
    await waitFor(() => expect(api.syncScheduleSet).toHaveBeenCalledWith({ mode: "daily", time: "08:30", weekdays: [1, 2, 3, 4, 5] }));
  });
});
