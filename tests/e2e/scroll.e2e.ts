import assert from "node:assert/strict";
import { textOf, tid } from "./support";

const MAX_ROUNDS = 600; // 500 pages of 200 reach the cap; more means the list never stops

async function scrollHeight(): Promise<number> {
  return browser.execute((sel: string) => document.querySelector(sel)?.scrollHeight ?? 0, tid("mail-list"));
}

describe("scroll to the 100,000-row cap in the real webview", () => {
  it("loads pages on scroll and shows the cap row", async () => {
    await $(tid("search-box")).waitForDisplayed({ timeout: 120_000 });
    await $(tid("search-box")).setValue("loadgen");
    await browser.waitUntil(async () => (await textOf(tid("result-count"))).includes("10,000+"), {
      timeout: 60_000,
      timeoutMsg: "result-count never showed 10,000+",
    });
    await $(tid("mail-row")).waitForDisplayed({ timeout: 30_000 });
    let rounds = 0;
    while (!(await $(tid("result-cap-row")).isDisplayed())) {
      rounds += 1;
      assert.ok(rounds <= MAX_ROUNDS, `cap row not reached after ${MAX_ROUNDS} scrolls`);
      const before = await scrollHeight();
      await browser.execute((sel: string) => {
        const list = document.querySelector(sel);
        if (list) list.scrollTop = list.scrollHeight;
      }, tid("mail-list"));
      // Each loaded page grows the virtual total size (32 px rows); the cap row ends the loop.
      await browser.waitUntil(
        async () => (await scrollHeight()) > before || (await $(tid("result-cap-row")).isDisplayed()),
        { timeout: 10_000, timeoutMsg: `no new page within 10 s at round ${rounds}` },
      );
    }
    assert.ok((await scrollHeight()) >= 3_200_000, `scrollHeight ${await scrollHeight()}`);
    const rows = await $$(tid("mail-row")).getElements();
    const lastId = await rows[rows.length - 1]!.getAttribute("data-mail-id");
    assert.ok(/^\d+$/.test(lastId ?? ""), `last data-mail-id ${lastId}`);
  });
});
