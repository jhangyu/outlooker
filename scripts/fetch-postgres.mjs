#!/usr/bin/env node
// Downloads, verifies and prunes the bundled PostgreSQL 18.6 (spec Part 01 §3, §6).
// Usage: node scripts/fetch-postgres.mjs [--target <triple>] [--self-test]
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { chmodSync, existsSync, mkdtempSync, readFileSync, readdirSync, renameSync, rmSync, statSync, writeFileSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const VERSION = "18.6.1";
const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const SHA_FILE = join(ROOT, "src-tauri", "native", "postgresql.sha256");
const DEST = join(ROOT, "src-tauri", "resources", "postgresql");
const PRUNE_DIRS = ["include", join("lib", "bitcode"), join("lib", "pgxs"), join("lib", "Python")];
const TRIPLES = {
  "darwin-arm64": "aarch64-apple-darwin",
  "darwin-x64": "x86_64-apple-darwin",
  "win32-x64": "x86_64-pc-windows-msvc",
  "win32-arm64": "aarch64-pc-windows-msvc",
  "linux-x64": "x86_64-unknown-linux-gnu",
  "linux-arm64": "aarch64-unknown-linux-gnu",
};

export function verify(buffer, expectedHex) {
  const actual = createHash("sha256").update(buffer).digest("hex");
  if (actual !== expectedHex) throw new Error(`sha256 mismatch: expected ${expectedHex}, got ${actual}`);
}

function pinnedHash(triple) {
  for (const line of readFileSync(SHA_FILE, "utf8").split("\n")) {
    const [hash, name] = line.trim().split(/\s+/);
    if (name === triple) return hash;
  }
  throw new Error(`no pinned hash for ${triple} in ${SHA_FILE}`);
}

function prune(dir) {
  for (const sub of PRUNE_DIRS) rmSync(join(dir, sub), { recursive: true, force: true });
  const lib = join(dir, "lib");
  for (const name of readdirSync(lib)) {
    if (name.startsWith("libLLVM") || name.startsWith("llvmjit") || name.endsWith(".a")) {
      rmSync(join(lib, name), { recursive: true, force: true });
    }
  }
}

// Archive files can be read-only; tauri-build copies resources keeping the mode, so a second build
// could not overwrite its own copy. Make the whole tree owner-writable.
export function makeWritable(dir) {
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, e.name);
    if (e.isSymbolicLink()) continue;
    chmodSync(p, statSync(p).mode | 0o200);
    if (e.isDirectory()) makeWritable(p);
  }
}

async function main() {
  const args = process.argv.slice(2);
  if (args.includes("--self-test")) {
    try {
      verify(Buffer.from("x"), "0".repeat(64));
    } catch {
      const t = mkdtempSync(join(tmpdir(), "fetch-pg-selftest-"));
      try {
        mkdirSync(join(t, "d"));
        writeFileSync(join(t, "d", "f"), "x");
        chmodSync(join(t, "d", "f"), 0o444);
        makeWritable(t);
        if (!(statSync(join(t, "d", "f")).mode & 0o200)) throw new Error("self-test failed: read-only file not made writable");
      } finally {
        rmSync(t, { recursive: true, force: true });
      }
      console.log("self-test ok: wrong hash rejected, read-only files made writable");
      return;
    }
    throw new Error("self-test failed: wrong hash accepted");
  }
  const ti = args.indexOf("--target");
  const triple = ti >= 0 ? args[ti + 1] : TRIPLES[`${process.platform}-${process.arch}`];
  if (!triple) throw new Error(`unsupported host ${process.platform}-${process.arch}`);
  const expected = pinnedHash(triple);
  const stamp = join(DEST, ".sha256");
  if (existsSync(stamp) && readFileSync(stamp, "utf8").trim() === expected) {
    makeWritable(DEST); // repairs trees fetched before the read-only fix
    console.log(`PostgreSQL ${VERSION} (${triple}) already present`);
    return;
  }
  const asset = `postgresql-${VERSION}-${triple}.tar.gz`;
  const url = `https://github.com/theseus-rs/postgresql-binaries/releases/download/${VERSION}/${asset}`;
  console.log(`downloading ${url}`);
  const res = await fetch(url);
  if (!res.ok) throw new Error(`download failed: HTTP ${res.status}`);
  const buffer = Buffer.from(await res.arrayBuffer());
  verify(buffer, expected);
  // Work dir inside the repo (gitignored) so the final rename never crosses drives/devices.
  const work = join(ROOT, "scripts", "tmp");
  mkdirSync(work, { recursive: true });
  const tmp = mkdtempSync(join(work, "fetch-pg-"));
  try {
    const archive = join(tmp, asset);
    writeFileSync(archive, buffer);
    execFileSync("tar", ["-xzf", archive, "-C", tmp], { stdio: "inherit" });
    const top = join(tmp, `postgresql-${VERSION}-${triple}`);
    if (!existsSync(join(top, "bin"))) throw new Error(`unexpected archive layout in ${asset}`);
    prune(top);
    makeWritable(top);
    rmSync(DEST, { recursive: true, force: true });
    mkdirSync(dirname(DEST), { recursive: true });
    renameSync(top, DEST);
    writeFileSync(stamp, `${expected}\n`);
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
  console.log(`PostgreSQL ${VERSION} (${triple}) ready in ${DEST}`);
}

main().catch((e) => {
  console.error(e.message);
  process.exit(e.message.startsWith("sha256 mismatch") ? 2 : 1);
});
