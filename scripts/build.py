#!/usr/bin/env python3
"""Build the desktop app: python3 scripts/build.py [--debug] [--clean-install]"""
import argparse
import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PG_DIR = ROOT / "src-tauri" / "resources" / "postgresql"
LIBPFF_ORIGIN = ROOT / "src-tauri" / "native" / "libpff" / "ORIGIN"
BUNDLE_DIR = ROOT / "src-tauri" / "target" / "release" / "bundle"


def die(msg):
    sys.exit(f"build.py: {msg}")


def run(args, **kw):
    print("+", " ".join(args), flush=True)
    rc = subprocess.run(args, cwd=ROOT, **kw).returncode
    if rc:
        sys.exit(rc)


def main():
    ap = argparse.ArgumentParser(description="Build the outlooker desktop app (tauri build).")
    ap.add_argument("--debug", action="store_true", help="pass --debug to tauri build")
    ap.add_argument("--clean-install", action="store_true", help="force npm ci")
    opts = ap.parse_args()

    mise = shutil.which("mise")
    if not mise:
        die("`mise` not found on PATH (https://mise.jdx.dev)")
    if not shutil.which("cargo"):
        die("`cargo` not found on PATH (install Rust via rustup)")
    if not LIBPFF_ORIGIN.exists():
        die("src-tauri/native/libpff is not vendored: run scripts/vendor-libpff.sh")

    if not PG_DIR.is_dir() or not any(PG_DIR.iterdir()):
        run([mise, "exec", "--", "node", "scripts/fetch-postgres.mjs"])
    if opts.clean_install or not (ROOT / "node_modules").is_dir():
        run([mise, "exec", "--", "npm", "ci"])

    cmd = [mise, "exec", "--", "npm", "run", "tauri", "build"]
    if opts.debug:
        cmd += ["--", "--debug"]
    run(cmd, env={**os.environ, "CI": "true"})

    # --debug writes to target/debug, so search both
    for base in (BUNDLE_DIR, BUNDLE_DIR.parents[1] / "debug" / "bundle"):
        if base.is_dir():
            for p in sorted(base.glob("*/*")):
                if p.suffix in {".app", ".dmg", ".msi", ".exe"}:
                    print(p)


if __name__ == "__main__":
    main()
