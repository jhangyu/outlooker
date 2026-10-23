#!/usr/bin/env python3
"""Vendor the prepared libpff sdist (fork jhangyu/libpff, commit bb5bcc0) into src-tauri/native/libpff.

SOURCE is a GitHub Actions run id of the fork's `build_wheels` workflow (artifact `cibw-sdist`,
downloaded with `gh`) or a path to a local `libpff_python-*.tar.gz`.

CI artifacts expire. To get a new run id, re-run the fork's `build_wheels` workflow
(workflow_dispatch) at commit bb5bcc0 and pass the new run id.
"""
import argparse
import hashlib
import shutil
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path

REPO = "jhangyu/libpff"
COMMIT = "bb5bcc0"
ROOT = Path(__file__).resolve().parent.parent
REQUIRED = ("include/libpff.h", "common/types.h", "libpff/libpff_definitions.h")


def main() -> None:
    head, _, tail = (__doc__ or "").partition("\n\n")
    ap = argparse.ArgumentParser(description=head, epilog=tail,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("source", metavar="SOURCE", help="run id or path to libpff_python-*.tar.gz")
    ap.add_argument("--dest", type=Path, default=ROOT / "src-tauri/native/libpff", help="vendor directory (default: %(default)s)")
    opts = ap.parse_args()

    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        if opts.source.endswith(".tar.gz"):
            tarball, run_id = Path(opts.source), "local-tarball"
        else:
            subprocess.run(["gh", "run", "download", opts.source, "-R", REPO, "-n", "cibw-sdist", "-D", str(tmp / "dl")], check=True)
            tarball, run_id = next((tmp / "dl").glob("libpff_python-*.tar.gz")), opts.source

        (tmp / "x").mkdir()
        with tarfile.open(tarball, "r:gz") as tf:
            tf.extractall(tmp / "x", filter="data")
        src = next((tmp / "x").glob("libpff_python-*"))
        for rel in REQUIRED:
            if not (src / rel).is_file():
                sys.exit(f"missing {rel} in {tarball}")

        shutil.rmtree(opts.dest, ignore_errors=True)
        shutil.copytree(src, opts.dest, symlinks=True)
        sha256 = hashlib.sha256(tarball.read_bytes()).hexdigest()
        (opts.dest / "ORIGIN").write_text(
            f"repo={REPO}\ncommit={COMMIT}\nrun_id={run_id}\nartifact=cibw-sdist\nsdist={tarball.name}\nsha256={sha256}\n"
        )
        print(f"vendored {tarball.name} into {opts.dest}")


if __name__ == "__main__":
    main()
