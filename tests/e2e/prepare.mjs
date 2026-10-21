// E2E app-data dirs and seeded libraries under scripts/tmp/e2e/ (gitignored). The user's real app data dir is
// never used: every run sets OUTLOOKER_APP_DATA_DIR to one of these dirs.
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

export const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
export const E2E_ROOT = path.join(REPO_ROOT, "scripts", "tmp", "e2e");
export const APP_DATA = {
  fresh: path.join(E2E_ROOT, "fresh"),
  seeded: path.join(E2E_ROOT, "seeded"),
  big: path.join(E2E_ROOT, "big"),
};
export const GROUPS = ["smoke", "flow", "seeded", "scroll"];
export const GROUP_APP_DATA = { smoke: APP_DATA.fresh, flow: APP_DATA.fresh, seeded: APP_DATA.seeded, scroll: APP_DATA.big };

const LIBRARIES = {
  [APP_DATA.seeded]: { root: path.join(E2E_ROOT, "seeded-root"), language: "zh-TW", messages: 2000 },
  [APP_DATA.big]: { root: path.join(E2E_ROOT, "big-root"), language: "en", messages: 120000 },
};

function fail(step, detail) {
  console.error(`prepare: ${step} failed: ${detail}`);
  process.exit(1);
}

/** Seeds the library once (re-runs reuse a root that has its marker) and writes the bootstrap file. */
function prepareLibrary(appData) {
  const lib = LIBRARIES[appData];
  if (!fs.existsSync(path.join(lib.root, "outlooker-data.json"))) {
    fs.rmSync(lib.root, { recursive: true, force: true });
    const args = ["run", "--release", "--locked", "-p", "outlooker-loadgen", "--", "seed",
      "--data-root", lib.root, "--messages", String(lib.messages)];
    const r = spawnSync("cargo", args, { cwd: path.join(REPO_ROOT, "src-tauri"), stdio: "inherit", shell: process.platform === "win32" });
    if (r.status !== 0) fail(`seed ${path.basename(lib.root)}`, `exit ${r.status ?? r.signal}`);
  }
  fs.mkdirSync(appData, { recursive: true });
  const bootstrap = { data_dir: lib.root, language: lib.language };
  fs.writeFileSync(path.join(appData, "settings.json"), JSON.stringify(bootstrap, null, 2) + "\n");
}

/** Makes the app-data dir of `group` ready: `fresh/` is emptied on every call (onboarding starts from zero). */
export function prepareGroup(group) {
  const appData = GROUP_APP_DATA[group];
  if (!appData) fail("group", `unknown group ${group}`);
  fs.mkdirSync(E2E_ROOT, { recursive: true });
  if (appData === APP_DATA.fresh) {
    fs.rmSync(appData, { recursive: true, force: true });
    fs.mkdirSync(appData, { recursive: true });
  } else {
    prepareLibrary(appData);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  for (const group of GROUPS) prepareGroup(group);
}
