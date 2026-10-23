// Cargo target runner (src-tauri/.cargo/config.toml). Debug test binaries get a per-test time cap (1 s, 3 s for
// cluster_lifecycle; CLAUDE.md "Test policy"). Binaries that use the shared PostgreSQL cluster get it booted once,
// before they start, by examples/test_pg_server.rs. Other binaries pass through unchanged.
import { spawn, spawnSync } from "node:child_process";
import { once } from "node:events";
import { createInterface } from "node:readline";
import { basename, dirname, join, sep } from "node:path";
import { judge } from "./test-time-judge.mjs";

// Loaded through `node -e`, so argv[1] is the binary.
const [bin, ...args] = process.argv.slice(1);
const name = basename(bin);
const isTest = bin.includes(`${sep}debug${sep}deps${sep}`);
const env = { ...process.env };

// Cold PostgreSQL template build (~1.5 s, only after a db/ or lifecycle/ source change) runs once, uncapped,
// before the capped run. A binary without a `warm_template` test runs 0 tests here.
if (isTest) {
  const warm = spawnSync(bin, ["--ignored", "--exact", "warm_template", "-q"], { encoding: "utf8" });
  if (warm.status !== 0) {
    process.stderr.write(warm.stdout + warm.stderr);
    process.exit(warm.status ?? 1);
  }
}

// The shared cluster: one per binary that needs it (today the merged DB test binary `it` and the import_bench
// example). The server stops the cluster and removes its run dir when its stdin closes, including when this
// runner dies; a SIGKILLed server is swept by the next server start.
let server;
if (/^(it-|import_bench)/.test(name)) {
  const profile = dirname(dirname(bin)); // target/<profile>, from deps/ or examples/
  const release = profile.endsWith(`${sep}release`) ? ["--release"] : [];
  const build = spawnSync(env.CARGO ?? "cargo", ["build", "--workspace", "--example", "test_pg_server", ...release], {
    stdio: ["ignore", "ignore", "inherit"],
  });
  if (build.status !== 0) process.exit(build.status ?? 1);
  const exe = join(profile, "examples", `test_pg_server${process.platform === "win32" ? ".exe" : ""}`);
  server = spawn(exe, [], { stdio: ["pipe", "pipe", "inherit"], detached: true });
  const first = once(createInterface({ input: server.stdout }), "line");
  const [line] = await Promise.race([first, once(server, "exit").then(() => [null])]);
  if (!line) process.exit(1);
  env.OUTLOOKER_TEST_PG = line;
}
const stopServer = async () => {
  if (!server) return;
  server.stdin.end();
  if (server.exitCode === null) await once(server, "exit");
};

if (!isTest) {
  const r = spawnSync(bin, args, { stdio: "inherit", env });
  await stopServer();
  process.exit(r.status ?? 1);
}

// The only exception binary: tests that start their own PostgreSQL (user-approved 3 s base cap).
const capS = Number(process.env.CARGO_TEST_CAP_S ?? (name.startsWith("cluster_lifecycle-") ? 3 : 1));
// ponytail: the cap reads libtest's unstable --report-time lines ("test NAME ... ok <1.234s>"), so it works for
// every crate layout; RUSTC_BOOTSTRAP is set for the test binary only, never rustc. Ceiling: it depends on libtest's
// line format, so it fails closed: tests ran but no time was parsed (format change, `-q`, `--format json`) = exit 1.
const unmeasured = `the ${capS} s per-test cap could not be measured (CLAUDE.md "Test policy"); do not pass -q/--quiet/--format`;
if (/(^| )(-q|--quiet|--format[ =]\S+)( |$)/.test(args.join(" "))) {
  console.error(unmeasured);
  await stopServer();
  process.exit(1);
}
env.RUSTC_BOOTSTRAP = "1";
const timeLine = /^test (.+) \.\.\. .* <([\d.]+)s>$/;
const child = spawn(bin, ["-Z", "unstable-options", "--report-time", ...args], { stdio: ["inherit", "pipe", "inherit"], env });
const over = [];
let timed = 0;
let ran = 0;
createInterface({ input: child.stdout }).on("line", (line) => {
  console.log(line);
  const m = timeLine.exec(line);
  if (m) timed++;
  if (m && Number(m[2]) > capS) over.push(m[1]);
  const r = /^test result: .*?(\d+) passed; (\d+) failed/.exec(line);
  if (r) ran += Number(r[1]) + Number(r[2]);
});
const [code] = await once(child, "close");
// Each test over the base cap runs once more, alone; the shared judgment decides on that time.
let fail = ran > 0 && timed === 0;
if (fail) console.error(unmeasured);
for (const t of over) {
  const solo = spawnSync(bin, ["-Z", "unstable-options", "--report-time", "--exact", t, "--test-threads", "1"], { env, encoding: "utf8" });
  const m = solo.stdout.split("\n").map((l) => /^test (.+) \.\.\. (\S+) <([\d.]+)s>$/.exec(l)).find((x) => x?.[1] === t);
  const v = m?.[2] === "ok" ? judge(t, Number(m[3]), capS)
    : { fail: true, line: `FAIL: ${t}: solo rerun ${m ? m[2] : `produced no timing; ${unmeasured}`}\n${solo.stdout}${solo.stderr}` };
  console.error(v.line);
  fail ||= v.fail;
}
await stopServer();
process.exit(fail ? 1 : (code ?? 1));
