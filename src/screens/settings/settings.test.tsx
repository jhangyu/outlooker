import { fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api/commands");
vi.mock("../../api/dialog");
vi.mock("../../api/events", async () => (await import("../../app/testing")).eventsMock);

import { api } from "../../api/commands";
import { pickFolder } from "../../api/dialog";
import { makeDbStatus, mockApiDefaults, renderWithProviders } from "../../app/testing";
import { SettingsScreen, isValidQuality } from ".";

async function typeQuality(value: string) {
  const input = await screen.findByRole("spinbutton");
  fireEvent.change(input, { target: { value } });
  fireEvent.blur(input);
  fireEvent.click(screen.getByTestId("settings-webp-apply"));
}

describe("Settings screen", () => {
  beforeEach(() => mockApiDefaults());

  it("WebP quality outside 1-100 is not sent", async () => {
    renderWithProviders(<SettingsScreen />);
    for (const bad of ["0", "101", "50.5"]) {
      await typeQuality(bad);
      expect(await screen.findByText("Enter a whole number from 1 to 100.")).toBeTruthy();
    }
    expect(api.settingsUpdate).not.toHaveBeenCalled();
    await typeQuality("75");
    await waitFor(() => expect(api.settingsUpdate).toHaveBeenCalledWith({ language: null, webpQuality: 75 }));
    expect(isValidQuality(1) && isValidQuality(100) && !isValidQuality(Number.NaN)).toBe(true);
  });

  it("maintenance button runs the task", async () => {
    renderWithProviders(<SettingsScreen />);
    fireEvent.click(await screen.findByTestId("maint-vacuum"));
    await waitFor(() => expect(api.maintenanceRun).toHaveBeenCalledWith("vacuum"));
  });

  it("disables maintenance while a task is running", async () => {
    vi.mocked(api.dbStatus).mockResolvedValue(
      makeDbStatus({ maintenance: { task: "reindex", state: "running", startedAt: "2026-10-01T00:00:00Z", finishedAt: null, error: null } }),
    );
    renderWithProviders(<SettingsScreen />);
    await waitFor(() => expect((screen.getByTestId("maint-analyze") as HTMLButtonElement).disabled).toBe(true));
  });

  it("moves the data folder after confirmation", async () => {
    vi.mocked(pickFolder).mockResolvedValue("/Volumes/Big/Outlooker");
    renderWithProviders(<SettingsScreen />);
    fireEvent.click(await screen.findByTestId("settings-move"));
    fireEvent.click(await screen.findByRole("button", { name: "Move" }));
    await waitFor(() => expect(api.dataDirMove).toHaveBeenCalledWith("/Volumes/Big/Outlooker"));
  });
});
