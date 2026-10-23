import { fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api/commands");
vi.mock("../../api/dialog");
vi.mock("../../api/events", async () => (await import("../../app/testing")).eventsMock);

import { api } from "../../api/commands";
import { mockApiDefaults, renderWithProviders } from "../../app/testing";
import { OnboardingScreen } from ".";

describe("Onboarding", () => {
  beforeEach(() => mockApiDefaults());

  it("onboarding initializes with the chosen dir and language", async () => {
    renderWithProviders(<OnboardingScreen />, { status: { phase: "needsSetup", dataDir: "/data" } });
    fireEvent.click(await screen.findByTestId("onboarding-lang-en"));
    await waitFor(() => expect(api.settingsUpdate).toHaveBeenCalledWith({ language: "en", webpQuality: null }));
    fireEvent.click(screen.getByTestId("onboarding-next"));
    fireEvent.click(await screen.findByTestId("onboarding-dir-default"));
    fireEvent.click(screen.getByTestId("onboarding-next"));
    await waitFor(() => expect(api.appInitialize).toHaveBeenCalledWith({ dataDir: "/data", language: "en" }));
    expect(await screen.findByText("Create database cluster")).toBeTruthy();
  });

  it("shows the init error and stays on the location step", async () => {
    vi.mocked(api.appInitialize).mockRejectedValue({ kind: "invalidInput", field: "dataDir", reason: "notEmpty" });
    renderWithProviders(<OnboardingScreen />, { status: { phase: "needsSetup", dataDir: "/data" } });
    fireEvent.click(await screen.findByTestId("onboarding-next"));
    fireEvent.click(screen.getByTestId("onboarding-next"));
    expect(await screen.findByText("The value of dataDir is not valid.")).toBeTruthy();
    expect(screen.getByTestId("onboarding-dir-default")).toBeTruthy();
  });
});
