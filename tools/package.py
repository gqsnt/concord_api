#!/usr/bin/env python3
"""Each published crate as a stranger receives it. Offline; nothing is uploaded.

1. The crates that are never published say `publish = false` in their own manifest.
2. For each of the five crates, `cargo package --list` shows what its package holds; a file
   that no pattern of its `include` takes fails the run.
3. The five `.crate` files are made (`--no-verify`), unpacked into
   `target/package-check/`, and joined in a scratch workspace there whose `[patch]`
   points each sibling at its unpacked package. Each crate is then checked with its default
   features, with none and with all: a feature of a dependency that only the workspace turned on shows
   here and nowhere else.

`--exclude-lockfile`: while the workspace says `publish = false`, cargo does not stand a
package in for crates.io when it resolves the lock file of a sibling, so that lock file
cannot be made offline. A library's lock file is not used by its users; `cargo publish`
adds it once `publish = false` is lifted.

Run `just package` from the repository root. Requires Python 3.11+.
"""

import glob
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile
import tomllib

REPO_ROOT = Path(__file__).resolve().parents[1]
TARGET = REPO_ROOT / "target"
CHECK = TARGET / "package-check"
# In the order that cargo can package them: a crate after the crates it depends on.
PUBLISHED = ("concord-core", "concord-macros", "concord-extras", "concord-api", "concord-testkit")
NEVER = ("tests/tour", "tests/tour-gen", "tests/conformance", "tests/conformance-gen", "tests/real-apis")
# What cargo writes into every package by itself, whatever `include` says.
PACKAGE = ("cargo", "package", "--offline", "--allow-dirty", "--no-verify", "--exclude-lockfile")
GENERATED = {"Cargo.toml", "Cargo.toml.orig", "Cargo.lock", ".cargo_vcs_info.json"}


class Failed(Exception):
    pass


def environment():
    env = os.environ.copy()
    env.update(CARGO_NET_OFFLINE="true", CARGO_BUILD_JOBS="4")
    env["CARGO_TARGET_DIR"] = str(TARGET)
    return env


def run(command, cwd=REPO_ROOT, env=None, capture=False):
    print("+ " + " ".join(command), flush=True)
    result = subprocess.run(command, cwd=cwd, env=env or environment(), text=True,
                            stdout=subprocess.PIPE if capture else None)
    if result.returncode:
        raise Failed(f"{' '.join(command)} (exit {result.returncode})")
    return result.stdout


def manifest(crate):
    with (REPO_ROOT / crate / "Cargo.toml").open("rb") as stream:
        return tomllib.load(stream)


def check_never_published():
    for crate in NEVER:
        if manifest(crate)["package"].get("publish") is not False:
            raise Failed(f"{crate}: its own manifest must say `publish = false`")
    print(f"never published: {', '.join(NEVER)}", flush=True)


def check_include(crate, files):
    patterns = manifest(crate)["package"].get("include")
    if not patterns:
        raise Failed(f"{crate}: no `include` in [package]")
    rules = [re.compile(glob.translate(p.lstrip("/"), recursive=True, include_hidden=True))
             for p in patterns]
    outside = [f for f in files if f not in GENERATED and not any(r.match(f) for r in rules)]
    if outside:
        raise Failed(f"{crate}: files outside `include`: {', '.join(outside)}")


def unpack(crate, version):
    archive = TARGET / "package" / f"{crate}-{version}.crate"
    with tarfile.open(archive, "r:gz") as tar:
        members = [m for m in tar.getmembers() if m.isfile()]
        tar.extractall(CHECK, filter="data")
    size = sum(m.size for m in members)
    return CHECK / f"{crate}-{version}", len(members), size, archive.stat().st_size


def scratch_workspace(folders):
    lines = ["# Made by tools/package.py: the packages, built outside the repository workspace.",
             "[workspace]", 'resolver = "2"', "members = [" + ", ".join(f'"{f.name}"' for f in folders.values()) + "]",
             "", "[patch.crates-io]"]
    lines += [f'{crate} = {{ path = "{folder.name}" }}' for crate, folder in folders.items()]
    (CHECK / "Cargo.toml").write_text("\n".join(lines) + "\n")
    # Start from the versions that the workspace resolved, so that no network is needed.
    shutil.copy2(REPO_ROOT / "Cargo.lock", CHECK / "Cargo.lock")


def main():
    try:
        check_never_published()
        version = tomllib.loads((REPO_ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
        lists = {}
        for crate in PUBLISHED:
            out = run([*PACKAGE, "--list", "-p", crate],
                      capture=True)
            lists[crate] = out.split()
            print(f"{crate}: " + " ".join(lists[crate]), flush=True)
            check_include(crate, lists[crate])
        packages = [arg for crate in PUBLISHED for arg in ("-p", crate)]
        run([*PACKAGE, *packages])
        if CHECK.exists():
            shutil.rmtree(CHECK)
        CHECK.mkdir(parents=True)
        folders, summary = {}, []
        for crate in PUBLISHED:
            folder, count, size, packed = unpack(crate, version)
            unpacked = sorted(str(p.relative_to(folder)) for p in folder.rglob("*") if p.is_file())
            if unpacked != sorted(lists[crate]):
                raise Failed(f"{crate}: the .crate does not hold what `--list` said")
            folders[crate] = folder
            summary.append(f"{crate}: {count} files, {size / 1024:.1f} KiB unpacked, {packed / 1024:.1f} KiB packed")
        scratch_workspace(folders)
        env = environment()
        env["CARGO_TARGET_DIR"] = str(CHECK / "target")
        for crate in PUBLISHED:
            for flags in ([], ["--no-default-features"], ["--all-features"]):
                run(["cargo", "check", "--offline", "-p", crate, *flags], cwd=CHECK, env=env)
    except (Failed, OSError, KeyError, tarfile.TarError) as error:
        print(f"PACKAGE: FAILED ({error})", flush=True)
        return 1
    for line in summary:
        print(line, flush=True)
    print("PACKAGE: passed", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
