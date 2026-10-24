#!/usr/bin/env python3
"""Build the desktop app: python3 app/scripts/build.py [--debug] [--clean-install] [--ci] [--out DIR]"""
import argparse
import os
import shutil
import subprocess
import sys
import tarfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PG_DIR = ROOT / "backend" / "resources" / "postgresql"
LIBPFF_ORIGIN = ROOT / "backend" / "native" / "libpff" / "ORIGIN"
BUNDLE_DIR = ROOT / "backend" / "target" / "release" / "bundle"
# Tauri v2 Ubuntu prerequisites (v2.tauri.app/start/prerequisites) + patchelf/xdg-utils for AppImage + pkg-config for libpff's build.rs
APT_PACKAGES = [
    "libwebkit2gtk-4.1-dev", "build-essential", "curl", "wget", "file", "libxdo-dev", "libssl-dev",
    "libayatana-appindicator3-dev", "librsvg2-dev", "patchelf", "xdg-utils", "pkg-config",
]


def die(msg):
    sys.exit(f"build.py: {msg}")


def run(args, **kw):
    print("+", " ".join(args), flush=True)
    rc = subprocess.run(args, cwd=ROOT, **kw).returncode
    if rc:
        sys.exit(rc)


def collect_bundles(bases, out):
    """Copy bundles from each `<base>/*/` dir flat into out (a .app dir becomes .app.tar.gz); return the new paths."""
    out.mkdir(parents=True, exist_ok=True)
    copied = []
    for base in bases:
        for p in sorted(base.glob("*/*")) if base.is_dir() else []:
            if p.is_dir() and p.suffix == ".app":
                dst = out / (p.name + ".tar.gz")
                with tarfile.open(dst, "w:gz") as t:
                    t.add(p, arcname=p.name)
            elif p.is_file() and (p.suffix in {".dmg", ".msi", ".deb", ".rpm", ".AppImage"} or p.name.endswith("-setup.exe")):
                dst = out / p.name
                shutil.copy2(p, dst)
            else:
                continue
            copied.append(dst)
    return copied


def main():
    ap = argparse.ArgumentParser(description="Build the outlooker desktop app (tauri build).")
    ap.add_argument("--debug", action="store_true", help="pass --debug to tauri build")
    ap.add_argument("--clean-install", action="store_true", help="force npm ci")
    ap.add_argument("--ci", action="store_true", help="CI mode: install Linux system packages, always npm ci")
    ap.add_argument("--out", type=Path, help="copy the built bundles flat into this dir; fails if none found")
    opts = ap.parse_args()

    if opts.ci and sys.platform.startswith("linux"):
        sudo = [] if os.geteuid() == 0 else ["sudo"]
        run(sudo + ["apt-get", "update"])
        run(sudo + ["apt-get", "install", "-y"] + APT_PACKAGES, env={**os.environ, "DEBIAN_FRONTEND": "noninteractive"})

    mise = shutil.which("mise")
    if not mise:
        die("`mise` not found on PATH (https://mise.jdx.dev)")
    if not shutil.which("cargo"):
        die("`cargo` not found on PATH (install Rust via rustup)")
    if not LIBPFF_ORIGIN.exists():
        die("app/backend/native/libpff is not vendored: run app/scripts/vendor-libpff.py")

    if not PG_DIR.is_dir() or not any(PG_DIR.iterdir()):
        run([mise, "exec", "--", "node", "scripts/fetch-postgres.mjs"])
    if opts.ci or opts.clean_install or not (ROOT / "node_modules").is_dir():
        run([mise, "exec", "--", "npm", "ci"])

    cmd = [mise, "exec", "--", "npm", "run", "tauri", "build"]
    if opts.debug:
        cmd += ["--", "--debug"]
    # APPIMAGE_EXTRACT_AND_RUN: linuxdeploy runs without FUSE (containers, CI)
    run(cmd, env={**os.environ, "CI": "true", "APPIMAGE_EXTRACT_AND_RUN": "1"})

    # --debug writes to target/debug, so search both
    bases = (BUNDLE_DIR, BUNDLE_DIR.parents[1] / "debug" / "bundle")
    if opts.out:
        copied = collect_bundles(bases, opts.out)
        for p in copied:
            print(p)
        if not copied:
            die("no bundles found under " + " or ".join(map(str, bases)))
        return
    for base in bases:
        if base.is_dir():
            for p in sorted(base.glob("*/*")):
                if p.suffix in {".app", ".dmg", ".msi", ".exe"}:
                    print(p)


if __name__ == "__main__":
    main()
