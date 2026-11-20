#!/usr/bin/env python3
"""Check the offline lowest-version resolution without changing the workspace lockfile."""

import argparse
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import tempfile

REPO_ROOT = Path(__file__).resolve().parents[1]
TARGET = REPO_ROOT / "target" / "minimal-versions"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("overrides", nargs="*", metavar="PACKAGE=VERSION",
                        help="raise a version in the scratch lockfile when a crate is missing from the cache")
    args = parser.parse_args()
    update = ["cargo", "+nightly", "update", "--offline", "-Z", "minimal-versions"]
    commands = [update]
    for override in args.overrides:
        package, _, version = override.partition("=")
        if not package or not version:
            parser.error(f"expected PACKAGE=VERSION, got {override!r}")
        commands.append([*update, "-p", package, "--precise", version])
    commands.append(["cargo", "check", "--offline", "--locked", "--workspace", "--all-features"])

    TARGET.mkdir(parents=True, exist_ok=True)
    files = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=REPO_ROOT,
    )
    env = dict(os.environ, CARGO_NET_OFFLINE="true", CARGO_TARGET_DIR=str(TARGET / "build"))
    with tempfile.TemporaryDirectory(dir=TARGET, prefix="source-") as directory:
        scratch = Path(directory)
        for name in files.split(b"\0")[:-1]:
            relative = Path(os.fsdecode(name))
            source = REPO_ROOT / relative
            if source.is_file():
                destination = scratch / relative
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source, destination)
        for command in commands:
            print("+ " + shlex.join(command), flush=True)
            result = subprocess.run(command, cwd=scratch, env=env)
            if result.returncode:
                print(f"MINIMAL VERSIONS: failed (exit {result.returncode})", flush=True)
                return result.returncode
    print("MINIMAL VERSIONS: passed" + (" with lockfile overrides" if args.overrides else ""), flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
