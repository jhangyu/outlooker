#!/usr/bin/env node
// Downloads, verifies and prunes the bundled PostgreSQL 18.6 (spec Part 01 §3, §6).
// Usage: node scripts/fetch-postgres.mjs [--target <triple>] [--self-test]
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, readdirSync, renameSync, rmSync, writeFileSync, mkdirSync } from "node:fs";
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

async function main() {
  const args = process.argv.slice(2);
  if (args.includes("--self-test")) {
    try {
      verify(Buffer.from("x"), "0".repeat(64));
    } catch {
      console.log("self-test ok: wrong hash rejected");
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
  const tmp = mkdtempSync(join(tmpdir(), "outlooker-pg-"));
  try {
    const archive = join(tmp, asset);
    writeFileSync(archive, buffer);
    execFileSync("tar", ["-xzf", archive, "-C", tmp], { stdio: "inherit" });
    const top = join(tmp, `postgresql-${VERSION}-${triple}`);
    if (!existsSync(join(top, "bin"))) throw new Error(`unexpected archive layout in ${asset}`);
    prune(top);
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
