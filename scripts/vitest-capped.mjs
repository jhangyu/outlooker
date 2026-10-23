// `npm test`: vitest with the per-test cap of CLAUDE.md "Test policy". The base cap is vite.config.ts
// testTimeout/hookTimeout (1000 ms). A test that failed by TIMEOUT runs once more, alone, with a 6 s timeout, and
// scripts/test-time-judge.mjs decides on that time. Assertion and suite failures are never rerun.
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { judge, TOLERANCE_S } from "./test-time-judge.mjs";

const CAP_S = 1;
const out = join(mkdtempSync(join(tmpdir(), "vitest-capped-")), "result.json");
const vitest = (args) => {
  const r = spawnSync(process.execPath, ["node_modules/vitest/vitest.mjs", "run", "--reporter=default", "--reporter=json", `--outputFile=${out}`, ...args], { stdio: "inherit" });
  return { code: r.status ?? 1, json: JSON.parse(readFileSync(out, "utf8")) };
};
const timedOut = (a) => a.status === "failed" && a.failureMessages.some((m) => /(Test|Hook) timed out in \d+ms/.test(m));

const main = vitest(process.argv.slice(2));
const failed = main.json.testResults.flatMap((f) => f.assertionResults.filter((a) => a.status === "failed").map((a) => ({ file: f.name, a })));
const suiteErrors = main.json.testResults.filter((f) => f.status === "failed" && f.assertionResults.every((a) => a.status !== "failed"));
const slow = failed.filter((x) => timedOut(x.a));
let fail = main.code !== 0 && (slow.length === 0 || slow.length < failed.length || suiteErrors.length > 0);
for (const { file, a } of slow) {
  // -t matches "describe > test"; the rerun's duration includes beforeEach/afterEach hooks.
  const pattern = `^${[...a.ancestorTitles, a.title].join(" > ").replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}$`;
  const solo = vitest([file, "-t", pattern, `--testTimeout=${TOLERANCE_S * 1000}`, `--hookTimeout=${TOLERANCE_S * 1000}`]);
  const r = solo.json.testResults.flatMap((f) => f.assertionResults).find((x) => x.fullName === a.fullName);
  const v = r?.status === "passed" ? judge(a.fullName, Number((r.duration / 1000).toFixed(3)), CAP_S)
    : { fail: true, line: `FAIL: ${a.fullName}: ${r ? `rerun ${r.status} (over ${TOLERANCE_S} s or not a timing failure)` : "rerun produced no timing"}` };
  console.error(v.line);
  fail ||= v.fail;
}
process.exit(fail ? 1 : 0);
