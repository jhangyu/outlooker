import assert from "node:assert/strict";
import path from "node:path";
import { REPO_ROOT } from "./prepare.mjs";
import { attrOf, count, setPicks, textOf, tid } from "./support";

const PST = path.join(REPO_ROOT, "tests", "fixtures", "external", "outlook.pst");
const OUTLOOK_ROW = `${tid("pst-row")}[data-pst-name="outlook"]`;

describe("flow: onboarding, import, search, preview, Advanced SQL, removal", () => {
  it("onboards into a new library", async () => {
    await $(tid("onboarding-lang-en")).waitForClickable({ timeout: 60_000 });
    await $(tid("onboarding-lang-en")).click();
    await $(tid("onboarding-next")).click();
    await $(tid("onboarding-dir-default")).click();
    await $(tid("onboarding-next")).waitForEnabled();
    await $(tid("onboarding-next")).click();
    await $(tid("onboarding-db-ready")).waitForDisplayed({ timeout: 120_000 });
    await $(tid("onboarding-next")).click();
  });

  it("imports outlook.pst to Completed", async () => {
    await setPicks([[PST]]);
    await $(tid("onboarding-pick-pst")).click();
    await browser.waitUntil(async () => (await textOf(`${OUTLOOK_ROW} ${tid("pst-status")}`)) === "Completed", {
      timeout: 120_000,
      timeoutMsg: "outlook.pst did not reach Completed",
    });
  });

  it("finds the Welcome mail and previews it in the sandboxed frame", async () => {
    await $(tid("nav-search")).click();
    await $(tid("search-box")).setValue("Welcome");
    await $(tid("mail-row")).waitForDisplayed({ timeout: 30_000 });
    await $(tid("mail-row")).click();
    await browser.waitUntil(
      async () => (await textOf(tid("reading-pane"))).includes("Welcome to Microsoft Office Outlook 2003"),
      { timeout: 30_000, timeoutMsg: "reading pane did not show the Welcome subject" },
    );
    assert.equal(await attrOf(tid("mail-preview"), "sandbox"), "");
  });

  it("lists the mail in Advanced SQL mail-list mode", async () => {
    await $(tid("nav-advancedSql")).click();
    await $(tid("sql-new")).click();
    await $(tid("sql-editor")).click();
    await $(tid("sql-editor")).addValue("SELECT id, subject FROM v_mail");
    await $(tid("sql-run")).click();
    await browser.waitUntil(async () => (await attrOf(tid("sql-result"), "data-mode")) === "mailList", {
      timeout: 30_000,
      timeoutMsg: "sql-result never reached data-mode=mailList",
    });
    assert.equal(await count(`${tid("sql-result")} ${tid("mail-row")}`), 1);
  });

  it("removes the PST", async () => {
    await $(tid("nav-import")).click();
    await $(`${OUTLOOK_ROW} ${tid("pst-remove")}`).click();
    await $(tid("remove-confirm")).click();
    await browser.waitUntil(async () => (await count(tid("pst-row"))) === 0, {
      timeout: 60_000,
      timeoutMsg: "pst-row still present after removal",
    });
    await browser.waitUntil(async () => (await textOf("body")).includes("No PST imported yet"), {
      timeout: 10_000,
      timeoutMsg: "import empty state not shown",
    });
  });
});
