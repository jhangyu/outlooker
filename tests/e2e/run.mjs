// Runs the E2E groups one after another, one app process and one app-data dir per `wdio run`
// (the tauri-service launcher starts one app per capability at once). `--spec tests/e2e/<group>.e2e.ts` runs one group.
import { spawnSync } from "node:child_process";
import path from "node:path";
import { GROUPS, prepareGroup } from "./prepare.mjs";

const at = process.argv.indexOf("--spec");
const only = at >= 0 ? path.basename(process.argv[at + 1] ?? "").replace(/\.e2e\.ts$/, "") : null;
if (only !== null && !GROUPS.includes(only)) {
  console.error(`run: unknown spec ${process.argv[at + 1]} (groups: ${GROUPS.join(", ")})`);
  process.exit(1);
}

for (const group of only ? [only] : GROUPS) {
  prepareGroup(group);
  console.log(`run: group ${group}`);
  const r = spawnSync("npx", ["wdio", "run", "wdio.conf.ts", "--spec", `tests/e2e/${group}.e2e.ts`], {
    stdio: "inherit",
    env: { ...process.env, OUTLOOKER_E2E_GROUP: group },
    shell: process.platform === "win32",
  });
  if (r.status !== 0) {
    console.error(`run: group ${group} failed (exit ${r.status ?? r.signal})`);
    process.exit(r.status || 1);
  }
}
