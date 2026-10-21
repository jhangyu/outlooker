import assert from "node:assert/strict";
import { attrOf, tid } from "./support";

const IMAGE_PREFIX = process.platform === "win32" ? "http://mailimg.localhost/" : "mailimg://localhost/";

describe("seeded library (zh-TW): Chinese search and inline image", () => {
  it("finds the seeded mail and serves its inline image through mailimg", async () => {
    await $(tid("search-box")).waitForDisplayed({ timeout: 120_000 });
    await $(tid("search-box")).setValue("報價單");
    const row = $(`//*[@data-testid="mail-row"][contains(., "E2E 報價單 inline")]`);
    await row.waitForDisplayed({ timeout: 30_000 });
    await row.click();
    await browser.waitUntil(async () => ((await attrOf(tid("mail-preview"), "srcdoc")) ?? "").includes("<img"), {
      timeout: 30_000,
      timeoutMsg: "preview has no image",
    });
    // The sandbox="" frame has an opaque origin, so WebDriver cannot enter it: read the URL from srcdoc and load
    // it in the main document, which has the same CSP (img-src includes the mailimg scheme).
    const srcdoc = (await attrOf(tid("mail-preview"), "srcdoc")) ?? "";
    const src = /<img[^>]*\ssrc="([^"]+)"/.exec(srcdoc)?.[1] ?? "";
    assert.ok(src.startsWith(IMAGE_PREFIX), `image src ${src}`);
    const width = await browser.execute(async (url: string) => {
      const img = new Image();
      img.src = url;
      try {
        await img.decode();
      } catch {
        return 0;
      }
      return img.naturalWidth;
    }, src);
    assert.ok(width > 0, `naturalWidth ${width}`);
  });
});
