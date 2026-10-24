#!/usr/bin/env python3
"""CI guard: fail when the CI path invokes any test.

Scanned: .github/workflows/*.yml|yaml, app/scripts/build.py, and (one
indirection) every `npm run <script>` found there resolved in
app/package.json "scripts" (recursively, cycle-safe), plus
tauri.conf.json build.beforeBuildCommand.

Rule: every line is scanned, comments included (a commented-out test command
still counts). Quotes, commas and brackets are blanked first so argv lists
like ["cargo", "test"] match. Patterns are whole-word, so "latest",
"attestation" and "contest" never match. Hits print as file:line and exit 1.

This file is not in the scanned set and must stay out of it.
Usage: ci_guard.py [--root <repo root>]
"""
import argparse
import json
import re
import sys
from pathlib import Path

# npm/pnpm/yarn options (--prefix app, -w app, --silent) may precede the subcommand
OPTS = r"(?:-{1,2}[\w-]+(?:[ =][^\s-]\S*)?\s+)*"
BAD = re.compile(
    r"(?<![\w.-])(?:"
    r"cargo\s+(?:\+\S+\s+)?" + OPTS + r"(?:test|t|nextest|bench)"
    r"|(?:npm|pnpm|yarn)\s+" + OPTS + r"(?:run(?:-script)?\s+" + OPTS + r")?(?:(?:test|e2e)\S*|(?:tst|t|it|cit|install-test)(?:[:-]\S*)?)"
    r"|(?:npx\s+)?(?:vitest|pytest|wdio|nextest)"
    r"|(?:--features|-F)[ =]\S*e2e"
    r"|perf\.rs|outlooker-loadgen"
    r"|node\s+\S*(?:e2e|vitest-capped|cargo-test-runner)\S*"
    r")(?![\w-])"
)
NPM_RUN = re.compile(r"(?<![\w.-])npm\s+" + OPTS + r"run(?:-script)?\s+" + OPTS + r"([\w:.-]+)")


def clean(line):
    return re.sub(r"[\"',\[\]]+", " ", line)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=str(Path(__file__).resolve().parents[2]))
    root = Path(ap.parse_args().root)
    scripts = {}
    pkg = root / "app/package.json"
    if pkg.is_file():
        scripts = json.loads(pkg.read_text()).get("scripts", {})
    hits = []

    def scan(label, lineno, text, seen=()):
        text = clean(text)
        if BAD.search(text):
            hits.append(f"{label}:{lineno}: {text.strip()}")
        for name in NPM_RUN.findall(text):
            if name in scripts and name not in seen:
                scan(f"{label}:{lineno} -> package.json[{name}]", 0,
                     scripts[name], seen + (name,))

    files = sorted((root / ".github/workflows").glob("*.y*ml"))
    files.append(root / "app/scripts/build.py")
    for f in files:
        if not f.is_file():
            continue
        for i, line in enumerate(f.read_text().splitlines(), 1):
            scan(str(f.relative_to(root)), i, line)

    tauri = root / "app/backend/tauri.conf.json"
    if tauri.is_file():
        cmd = json.loads(tauri.read_text()).get("build", {}).get("beforeBuildCommand")
        if isinstance(cmd, dict):
            cmd = cmd.get("script")
        if cmd:
            n = next((i for i, l in enumerate(tauri.read_text().splitlines(), 1)
                      if "beforeBuildCommand" in l), 0)
            scan(str(tauri.relative_to(root)), n, cmd)

    for h in hits:
        print(h)
    return 1 if hits else 0


if __name__ == "__main__":
    sys.exit(main())
