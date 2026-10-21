// Builds the E2E binary src-tauri/target/debug/outlooker[.exe]: debug, no bundle, cargo feature e2e
// (embedded WebDriver + OUTLOOKER_APP_DATA_DIR seam), frontend built with VITE_E2E=1 (native picker bypass).
import { spawnSync } from "node:child_process";

const shell = process.platform === "win32";

function run(cmd, args, env = process.env) {
  const r = spawnSync(cmd, args, { stdio: "inherit", env, shell });
  if (r.status !== 0) {
    console.error(`build: ${cmd} ${args.join(" ")} failed (exit ${r.status ?? r.signal})`);
    process.exit(r.status || 1);
  }
}

// tauri-build copies src-tauri/resources/postgresql/ and fails when it is missing (idempotent fetch).
run("node", ["scripts/fetch-postgres.mjs"]);
run(
  "npx",
  ["tauri", "build", "--debug", "--no-bundle", "--features", "e2e", "--config", "tests/e2e/tauri.e2e.conf.json"],
  { ...process.env, VITE_E2E: "1" },
);
