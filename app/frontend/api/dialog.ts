import { open } from "@tauri-apps/plugin-dialog";

declare global {
  interface Window {
    __outlookerE2ePicks?: Array<string[] | string | null>;
  }
}

const E2E = import.meta.env.VITE_E2E === "1";

function nextE2ePick(): string[] | string | null {
  return window.__outlookerE2ePicks?.shift() ?? null;
}

/** Native "Open PST" picker (spec Part 03 §2). The core re-validates every path. */
export async function pickPstFiles(): Promise<string[]> {
  if (E2E) {
    const pick = nextE2ePick();
    return Array.isArray(pick) ? pick : pick ? [pick] : [];
  }
  const chosen = await open({ multiple: true, directory: false, filters: [{ name: "Outlook PST", extensions: ["pst"] }] });
  return chosen === null ? [] : Array.isArray(chosen) ? chosen : [chosen];
}

/** Native folder picker for the data location. */
export async function pickFolder(): Promise<string | null> {
  if (E2E) {
    const pick = nextE2ePick();
    return Array.isArray(pick) ? (pick[0] ?? null) : pick;
  }
  const chosen = await open({ multiple: false, directory: true });
  return typeof chosen === "string" ? chosen : null;
}
