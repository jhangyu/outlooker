#!/usr/bin/env python3
"""Build the desktop app: python3 app/scripts/build.py [--debug] [--clean-install] [--ci] [--out DIR]
[--platform linux|macos|windows] [--host USER@HOST --ssh-key FILE] [--log FILE]

--platform defaults to the host OS and builds natively. linux on another OS builds in a Docker amd64
container, windows on another OS builds on --host over SSH, macos builds only on macOS."""
import argparse
import base64
import hashlib
import os
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import traceback
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REPO = ROOT.parent
HOST_OS = "linux" if sys.platform.startswith("linux") else {"darwin": "macos", "win32": "windows"}.get(sys.platform)
IMAGE = "outlooker-ci-linux"
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


def dockerfile():
    rust = re.search(r'channel\s*=\s*"([^"]+)"', (ROOT / "backend" / "rust-toolchain.toml").read_text()).group(1)
    node = re.search(r'node\s*=\s*"([^"]+)"', (REPO / ".config" / "mise.toml").read_text()).group(1)
    return f"""FROM --platform=linux/amd64 ubuntu:24.04
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \\
      curl ca-certificates git python3 sudo build-essential xz-utils \\
    && rm -rf /var/lib/apt/lists/*
# ubuntu:24.04 ships user "ubuntu" (uid 1000): mimic a runner, non-root with passwordless sudo
RUN echo 'ubuntu ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/ubuntu
USER ubuntu
ENV HOME=/home/ubuntu \\
    CARGO_HOME=/home/ubuntu/.cargo \\
    RUSTUP_HOME=/home/ubuntu/.rustup \\
    PATH=/home/ubuntu/.local/bin:/home/ubuntu/.local/share/mise/shims:/home/ubuntu/.cargo/bin:/usr/local/bin:/usr/bin:/bin
RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --default-toolchain none --profile minimal \\
    && rustup toolchain install {rust} --profile minimal -c rustfmt -c clippy \\
    && rustup default {rust}
RUN curl -fsSL https://mise.run | sh \\
    && mise use -g node@{node}
RUN sudo mkdir -p /work/app/backend/target && sudo chown -R ubuntu:ubuntu /work \\
    && mkdir -p /home/ubuntu/.cargo/registry
WORKDIR /work
"""


# Remote PowerShell (run via -EncodedCommand, so no cmd.exe quoting). The host has user-scope rustup, mise and Python.
WIN_SCRIPT = r"""
[Console]::OutputEncoding=[Text.Encoding]::UTF8
$ErrorActionPreference='Continue'; $ProgressPreference='SilentlyContinue'
$env:PATH="$env:USERPROFILE\.cargo\bin;$env:LOCALAPPDATA\mise\bin;C:\Python314;C:\Python314\Scripts;$env:PATH"
$env:CI='true'; $env:PYTHONUTF8='1'
$r="$env:USERPROFILE\outlooker-ci"
if (Test-Path $r) { Remove-Item -Recurse -Force $r }
New-Item -ItemType Directory -Force "$r\src" | Out-Null
tar -xzf "$r.tgz" -C "$r\src"
if ($LASTEXITCODE) { exit $LASTEXITCODE }
Set-Location "$r\src"
python app\scripts\build.py --ci --platform windows --out "$r\out" EXTRA_ARGS
exit $LASTEXITCODE
"""


class Tee:
    def __init__(self, *streams):
        self.streams = streams

    def write(self, s):
        for t in self.streams:
            t.write(s)
            t.flush()

    def flush(self):
        for t in self.streams:
            t.flush()


def run(args, input=None, **kw):
    """Run args in ROOT, streaming merged output through sys.stdout (so --log sees it); exit on non-zero."""
    print("+", " ".join(map(str, args)), flush=True)
    p = subprocess.Popen(args, cwd=ROOT, stdin=subprocess.PIPE if input else None, stdout=subprocess.PIPE,
                         stderr=subprocess.STDOUT, text=True, encoding="utf-8", errors="replace", **kw)
    if input:
        p.stdin.write(input)
        p.stdin.close()
    for line in p.stdout:
        sys.stdout.write(line)
    rc = p.wait()
    if rc:
        sys.exit(rc)


def archive(dst, fmt="tar"):
    """Write `git archive HEAD` to dst: builds ship the commit, never working-tree edits."""
    subprocess.run(["git", "archive", f"--format={fmt}", "-o", str(dst), "HEAD"], cwd=REPO, check=True)


def extra_args(opts):
    return [a for a, on in (("--debug", opts.debug), ("--clean-install", opts.clean_install)) if on]


def build_linux_docker(opts):
    if not opts.out:
        die("--out is required when building linux in Docker")
    if not shutil.which("docker"):
        die("`docker` not found on PATH")
    out = opts.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    run(["docker", "build", "--platform", "linux/amd64", "-t", IMAGE, "-"], input=dockerfile())
    work = Path(tempfile.mkdtemp(prefix="outlooker-ci-"))
    try:
        archive(work / ".src.tar")
        subprocess.run(["tar", "-xf", ".src.tar"], cwd=work, check=True)
        (work / ".src.tar").unlink()
        run(["docker", "run", "--rm", "--platform", "linux/amd64", "--user", "ubuntu", "-w", "/work",
             "-v", f"{work}:/work", "-v", f"{out}:/out",
             "-v", "outlooker-ci-cargo:/home/ubuntu/.cargo/registry",
             "-v", "outlooker-ci-target:/work/app/backend/target",
             IMAGE, "python3", "app/scripts/build.py", "--ci", "--platform", "linux", "--out", "/out"] + extra_args(opts))
    finally:
        shutil.rmtree(work, ignore_errors=True)


def build_windows_ssh(opts):
    if not (opts.host and opts.ssh_key):
        die("--host and --ssh-key are required when building windows from another OS")
    if not opts.out:
        die("--out is required when building windows over SSH")
    ssh = ["-o", "BatchMode=yes", "-i", str(opts.ssh_key)]
    tmp = Path(tempfile.mkdtemp(prefix="outlooker-ci-"))
    try:
        tgz = tmp / "src.tgz"
        archive(tgz, "tar.gz")
        # relative scp paths land in the remote user's profile dir
        run(["scp"] + ssh + [str(tgz), f"{opts.host}:outlooker-ci.tgz"])
        script = WIN_SCRIPT.replace("EXTRA_ARGS", " ".join(extra_args(opts)))
        enc = base64.b64encode(script.encode("utf-16-le")).decode()
        run(["ssh"] + ssh + [opts.host, "powershell -NoProfile -ExecutionPolicy Bypass -EncodedCommand " + enc])
        run(["scp", "-r"] + ssh + [f"{opts.host}:outlooker-ci/out", str(tmp)])
        opts.out.mkdir(parents=True, exist_ok=True)
        for p in sorted((tmp / "out").iterdir()):
            shutil.copy2(p, opts.out / p.name)
            print(opts.out / p.name)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def is_bundle(p):
    return (p.is_dir() and p.suffix == ".app") or (
        p.is_file() and (p.suffix in {".dmg", ".msi", ".deb", ".rpm", ".AppImage"} or p.name.endswith("-setup.exe")))


def collect_bundles(bases, out):
    """Copy bundles from each `<base>/*/` dir flat into out (a .app dir becomes .app.tar.gz); return the new paths."""
    out.mkdir(parents=True, exist_ok=True)
    copied = []
    for base in bases:
        for p in sorted(base.glob("*/*")) if base.is_dir() else []:
            if not is_bundle(p):
                continue
            if p.is_dir():
                dst = out / (p.name + ".tar.gz")
                with tarfile.open(dst, "w:gz") as t:
                    t.add(p, arcname=p.name)
            else:
                dst = out / p.name
                shutil.copy2(p, dst)
            copied.append(dst)
    return copied


def build(opts):
    if opts.platform == "macos" and HOST_OS != "macos":
        die("macos bundles can only be built on macOS")
    if opts.platform != HOST_OS:
        return build_linux_docker(opts) if opts.platform == "linux" else build_windows_ssh(opts)

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

    # only this build's profile: older bundles of the other profile must not be shipped
    bases = (BUNDLE_DIR.parents[1] / ("debug" if opts.debug else "release") / "bundle",)
    if opts.out:
        copied = collect_bundles(bases, opts.out)
        for p in copied:
            print(p)
        if not copied:
            die("no bundles found under " + str(bases[0]))
        return
    for base in bases:
        if base.is_dir():
            for p in sorted(base.glob("*/*")):
                if is_bundle(p):
                    print(p)


def main():
    # run() streams child output (vite prints non-cp1252 glyphs) through sys.stdout, which Python < 3.15 encodes as cp1252 on Windows
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")
    ap = argparse.ArgumentParser(description="Build the outlooker desktop app (tauri build).")
    ap.add_argument("--debug", action="store_true", help="pass --debug to tauri build")
    ap.add_argument("--clean-install", action="store_true", help="force npm ci")
    ap.add_argument("--ci", action="store_true", help="CI mode: install Linux system packages, always npm ci")
    ap.add_argument("--out", type=Path, help="copy the built bundles flat into this dir; fails if none found")
    ap.add_argument("--platform", choices=["linux", "macos", "windows"], default=HOST_OS,
                    help="target system (default: host OS); a foreign linux builds in Docker, a foreign windows over SSH")
    ap.add_argument("--host", help="USER@HOST of the Windows machine (--platform windows from another OS)")
    ap.add_argument("--ssh-key", type=Path, help="private key for --host")
    ap.add_argument("--log", type=Path, help="tee all output to this file, ending with HEAD, DIFF and RC lines")
    opts = ap.parse_args()
    if not opts.log:
        return build(opts)
    opts.log.parent.mkdir(parents=True, exist_ok=True)
    with open(opts.log, "w", encoding="utf-8") as f:
        sys.stdout = sys.stderr = Tee(sys.__stdout__, f)
        try:
            build(opts)
            rc = 0
        except SystemExit as e:
            rc = e.code if isinstance(e.code, int) else int(e.code is not None)
            if isinstance(e.code, str):
                print(e.code)
        except Exception:
            traceback.print_exc()
            rc = 1
        git = lambda *a: subprocess.run(["git", *a], cwd=REPO, capture_output=True).stdout
        for line in git("status", "--porcelain").decode().splitlines():
            print("DIRTY " + line)
        print("HEAD=" + git("rev-parse", "HEAD").decode().strip())
        print("DIFF=" + hashlib.sha1(git("diff", "HEAD")).hexdigest())
        print(f"RC={rc}")
        sys.stdout = sys.stderr = sys.__stdout__
    sys.exit(rc)


if __name__ == "__main__":
    main()
