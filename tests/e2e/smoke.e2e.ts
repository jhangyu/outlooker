import { tid } from "./support";

describe("smoke", () => {
  it("shows the app frame, a blocking or failed screen, or onboarding", async () => {
    const anyFrame = [tid("title-bar"), tid("blocking-screen"), tid("failed-screen"), tid("onboarding-lang-en")].join(", ");
    await $(anyFrame).waitForDisplayed({ timeout: 60_000, timeoutMsg: "no app frame within 60 s" });
  });
});
