/** `[data-testid="<id>"]` (R1-06 / R1-07 Produces lists). */
export const tid = (id: string): string => `[data-testid="${id}"]`;

/** Queues native-picker results for the VITE_E2E=1 bypass of src/api/dialog.ts. */
export async function setPicks(picks: Array<string[] | string | null>): Promise<void> {
  await browser.execute((p: Array<string[] | string | null>) => {
    (window as unknown as { __outlookerE2ePicks: Array<string[] | string | null> }).__outlookerE2ePicks = p;
  }, picks);
}

/** Visible text of the first match, "" when nothing matches (safe inside waitUntil across re-renders). */
export async function textOf(selector: string): Promise<string> {
  const el = await $(selector);
  return (await el.isExisting()) ? el.getText() : "";
}

/** Attribute of the first match, null when nothing matches. */
export async function attrOf(selector: string, name: string): Promise<string | null> {
  const el = await $(selector);
  return (await el.isExisting()) ? el.getAttribute(name) : null;
}

export async function count(selector: string): Promise<number> {
  return (await $$(selector)).length;
}
