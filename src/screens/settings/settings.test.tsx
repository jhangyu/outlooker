import { fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api/commands");
vi.mock("../../api/dialog");
vi.mock("../../api/events", async () => (await import("../../app/testing")).eventsMock);

import { api } from "../../api/commands";
import { mockApiDefaults, renderWithProviders } from "../../app/testing";
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
    expect([0, 101, 50.5, Number.NaN].some(isValidQuality)).toBe(false);
    expect(isValidQuality(1) && isValidQuality(100)).toBe(true);
    renderWithProviders(<SettingsScreen />);
    await typeQuality("0");
    expect(await screen.findByText("Enter a whole number from 1 to 100.")).toBeTruthy();
    expect(api.settingsUpdate).not.toHaveBeenCalled();
    await typeQuality("75");
    await waitFor(() => expect(api.settingsUpdate).toHaveBeenCalledWith({ language: null, webpQuality: 75 }));
  });
});
