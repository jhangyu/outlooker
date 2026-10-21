#!/bin/sh
# Vendors the prepared libpff sdist (fork jhangyu/libpff, commit bb5bcc0) into src-tauri/native/libpff.
# Usage: scripts/vendor-libpff.sh [<run-id> | <path-to-libpff_python-*.tar.gz>]
# The CI artifact of run 32393676269 expires 2026-11-18T16:44:21Z. After that, re-run the fork's
# build_wheels workflow (workflow_dispatch) at commit bb5bcc0 and pass the new run id.
set -eu
ARG="${1:-32393676269}"
REPO=jhangyu/libpff
COMMIT=bb5bcc0
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="$ROOT/src-tauri/native/libpff"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

case "$ARG" in
  *.tar.gz) TARBALL="$ARG"; RUN_ID="local-tarball" ;;
  *) gh run download "$ARG" -R "$REPO" -n cibw-sdist -D "$TMP/dl"
     TARBALL="$(ls "$TMP"/dl/libpff_python-*.tar.gz)"; RUN_ID="$ARG" ;;
esac

mkdir -p "$TMP/x"
tar -xzf "$TARBALL" -C "$TMP/x"
SRC="$(ls -d "$TMP"/x/libpff_python-*)"
test -f "$SRC/include/libpff.h"
test -f "$SRC/common/types.h"
test -f "$SRC/libpff/libpff_definitions.h"

rm -rf "$DEST"
mkdir -p "$DEST"
cp -R "$SRC"/. "$DEST"/
{
  echo "repo=$REPO"
  echo "commit=$COMMIT"
  echo "run_id=$RUN_ID"
  echo "artifact=cibw-sdist"
  echo "sdist=$(basename "$TARBALL")"
  echo "sha256=$(shasum -a 256 "$TARBALL" | cut -d' ' -f1)"
} > "$DEST/ORIGIN"
echo "vendored $(basename "$TARBALL") into $DEST"
