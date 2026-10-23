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
};
export const TARGET_DIR = path.resolve(REPO_ROOT, "src-tauri", process.env.CARGO_TARGET_DIR ?? "target");
export const GROUPS = ["flow", "seeded"];
export const GROUP_APP_DATA = { flow: APP_DATA.fresh, seeded: APP_DATA.seeded };

const LIBRARIES = {
  [APP_DATA.seeded]: { root: path.join(E2E_ROOT, "seeded-root"), language: "zh-TW", messages: 2000 },
};

/** Data root of the group's app (its PostgreSQL cluster is `<root>/pgdata`). */
export function groupDataRoot(group) {
  const appData = GROUP_APP_DATA[group];
  return appData === APP_DATA.fresh ? path.join(appData, "data") : LIBRARIES[appData].root;
}

function fail(step, detail) {
  console.error(`prepare: ${step} failed: ${detail}`);
  process.exit(1);
}

/** Seeds the library once (re-runs reuse a root that has its marker) and writes the bootstrap file. */
function prepareLibrary(appData) {
  const lib = LIBRARIES[appData];
  if (!fs.existsSync(path.join(lib.root, "outlooker-data.json"))) {
    fs.rmSync(lib.root, { recursive: true, force: true });
    const cwd = path.join(REPO_ROOT, "src-tauri");
    const shell = process.platform === "win32";
    const build = spawnSync("cargo", ["build", "--release", "--locked", "--workspace", "--bin", "outlooker-loadgen"], { cwd, stdio: "inherit", shell });
    if (build.status !== 0) fail("build outlooker-loadgen", `exit ${build.status ?? build.signal}`);
    const loadgen = path.join(TARGET_DIR, "release", shell ? "outlooker-loadgen.exe" : "outlooker-loadgen");
    const r = spawnSync(loadgen, ["seed", "--data-root", lib.root, "--messages", String(lib.messages)], { stdio: "inherit" });
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
