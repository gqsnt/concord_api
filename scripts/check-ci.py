#!/usr/bin/env python3
"""Keeps the toolchain policy and the CI gate selection in one place.

--toolchain  `rust-version` (the minimum) equals the release pinned in rust-toolchain.toml, every
             production package inherits it, and no living script or workflow selects another
             stable release with `cargo +X.Y` (only `+nightly`, for the non-blocking lane).
--workflow   .github/workflows/ci.yml runs the gates through scripts/verify.sh and lists no cargo
             command of its own, so local and CI runs select the same suites; Linux and Windows
             run the same complete inventory (--package --bench); scripts give tar its archive
             on standard input.

Usage: python3 -I scripts/check-ci.py --toolchain | --workflow
"""

import glob
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
# The four production packages; everything else in the workspace is a test, example or benchmark.
PRODUCTION = ("concord", "concord-core", "concord-macros", "concord-extras")


def read(path):
    with open(os.path.join(ROOT, path), encoding="utf-8") as f:
        return f.read()


def toolchain():
    problems = []
    pin = re.search(r'^channel\s*=\s*"([^"]+)"', read("rust-toolchain.toml"), re.M)
    minimum = re.search(r'^rust-version\s*=\s*"([^"]+)"', read("Cargo.toml"), re.M)
    if not pin or not minimum:
        return ["rust-toolchain.toml channel or workspace rust-version not found"]
    pin, minimum = pin.group(1), minimum.group(1)
    if not re.fullmatch(r"\d+\.\d+\.\d+", pin):
        problems.append(f"the pin must be an exact release, not {pin!r}")
    if pin != minimum:
        problems.append(f"rust-version {minimum} differs from the pinned toolchain {pin}")
    meta = json.loads(subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps", "--locked"],
        cwd=ROOT, capture_output=True, text=True, check=True).stdout)
    found = set()
    for package in meta["packages"]:
        if package["name"] in PRODUCTION:
            found.add(package["name"])
            if package.get("rust_version") != minimum:
                problems.append(f"{package['name']}: rust-version {package.get('rust_version')}, expected {minimum}")
    for name in PRODUCTION:
        if name not in found and os.path.isdir(os.path.join(ROOT, "crates", name)):
            problems.append(f"{name}: not a workspace member")
    living = glob.glob(os.path.join(ROOT, "scripts", "*")) + glob.glob(
        os.path.join(ROOT, ".github", "workflows", "*.yml"))
    for path in living:
        with open(path, encoding="utf-8") as f:
            for n, line in enumerate(f, 1):
                for m in re.finditer(r"(?:cargo|rustup run|toolchain install)\s+\+?(\d+\.\d+(?:\.\d+)?)", line):
                    if m.group(1) != pin:
                        problems.append(f"{os.path.relpath(path, ROOT)}:{n}: selects Rust {m.group(1)}")
    print(f"minimum = pin = {pin}; production packages: {', '.join(sorted(found))}")
    return problems


def workflow():
    problems = []
    text = read(".github/workflows/ci.yml")
    for n, line in enumerate(text.splitlines(), 1):
        if re.search(r"\bcargo\s+(?:\+\S+\s+)?(test|clippy|fmt|doc|build|check|bench|tree|package|run)\b", line):
            problems.append(f"ci.yml:{n}: a cargo command outside scripts/verify.sh: {line.strip()}")
    calls = re.findall(r"scripts/verify\.sh([^\n]*)", text)
    if not calls:
        problems.append("ci.yml does not run scripts/verify.sh")
    if not any("--nightly-only" in c for c in calls):
        problems.append("ci.yml has no non-blocking nightly lane (scripts/verify.sh --nightly-only)")
    if "windows-latest" not in text:
        problems.append("ci.yml has no native Windows job")
    # Every platform runs the same complete inventory: no gate is selected by operating system.
    runs = re.findall(r"run:\s*scripts/verify\.sh([^\n]*)", text)
    full = [r for r in runs if "--nightly-only" not in r]
    if not full or any("--package" not in r or "--bench" not in r for r in full):
        problems.append("the verify job must run scripts/verify.sh --package --bench")
    if "RUNNER_OS" in text or "runner.os" in text:
        problems.append("ci.yml selects gates by platform")
    # GNU tar takes an archive name with a colon before its first slash (a Windows `D:\\...` path)
    # for a remote host, so the scripts give tar its archive on standard input (`-f -`).
    for path in sorted(glob.glob(os.path.join(ROOT, "scripts", "*.sh"))):
        with open(path, encoding="utf-8") as f:
            for n, line in enumerate(f, 1):
                code = line.split("#", 1)[0]
                for m in re.finditer(r"\btar\s+-[A-Za-z]*f\s+(\S+)", code):
                    if m.group(1) != "-":
                        problems.append(f"{os.path.relpath(path, ROOT)}:{n}: tar reads an archive by name; use -f - with a redirect")
    print(f"ci.yml refers to scripts/verify.sh {len(calls)} time(s)")
    return problems


def main():
    if sys.argv[1:] == ["--toolchain"]:
        problems = toolchain()
    elif sys.argv[1:] == ["--workflow"]:
        problems = workflow()
    else:
        print(__doc__)
        return 2
    for p in problems:
        print(f"FAIL  {p}")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
