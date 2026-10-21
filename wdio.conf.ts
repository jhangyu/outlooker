import path from "node:path";
import { GROUP_APP_DATA, REPO_ROOT } from "./tests/e2e/prepare.mjs";

const group: string = process.env.OUTLOOKER_E2E_GROUP ?? "smoke";
const appData: string | undefined = (GROUP_APP_DATA as Record<string, string>)[group];
if (!appData) throw new Error(`unknown OUTLOOKER_E2E_GROUP ${group}`);
const binary = path.join(REPO_ROOT, "src-tauri", "target", "debug", process.platform === "win32" ? "outlooker.exe" : "outlooker");

export const config = {
  runner: "local",
  specs: [`./tests/e2e/${group}.e2e.ts`],
  maxInstances: 1,
  capabilities: [
    {
      browserName: "tauri",
      "tauri:options": { application: binary },
      "wdio:tauriServiceOptions": { env: { OUTLOOKER_APP_DATA_DIR: appData } },
    },
  ],
  services: [["@wdio/tauri-service", { driverProvider: "embedded", captureBackendLogs: false, captureFrontendLogs: false }]],
  framework: "mocha",
  mochaOpts: { ui: "bdd", timeout: 600_000 },
  reporters: ["spec"],
  logLevel: "warn",
  waitforTimeout: 10_000,
};
