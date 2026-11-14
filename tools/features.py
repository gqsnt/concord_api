#!/usr/bin/env python3
"""Check every facade feature subset and isolated packages on stable and 1.96.

Uses cargo package --list as the source inventory, expands workspace inheritance,
and copies local dependencies into target/feature-isolation. Each check runs
from the package's own directory with --manifest-path, outside the repository workspace.
No dependency/default-feature settings are otherwise changed. No network or /tmp.
"""

import argparse
import itertools
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tomllib

REPO_ROOT = Path(__file__).resolve().parents[1]
TARGET = REPO_ROOT / "target"
ROOT = TARGET / "feature-isolation"


def value(item):
    if isinstance(item, str):
        return json.dumps(item)
    if isinstance(item, bool):
        return str(item).lower()
    if isinstance(item, list):
        return "[" + ", ".join(map(value, item)) + "]"
    if isinstance(item, dict):
        return "{ " + ", ".join(f"{k} = {value(v)}" for k, v in item.items()) + " }"
    return str(item)


def detached_manifest(source, workspace):
    section = ""
    lines = []
    for line in source.splitlines():
        if match := re.fullmatch(r"\[([^]]+)\]", line):
            section = match[1]
        if section == "lints" and line.strip() == "workspace = true":
            for kind, lints in workspace.get("lints", {}).items():
                lines.append(f"[lints.{kind}]")
                lines.extend(f"{name} = {value(setting)}" for name, setting in lints.items())
            continue
        if match := re.match(r"^(\w[\w-]*)\.workspace\s*=\s*true\s*$", line):
            name = match[1]
            group = "dependencies" if section.endswith("dependencies") else section
            line = f"{name} = {value(workspace[group][name])}"
        elif section.endswith("dependencies") and re.match(r"^[\w-]+\s*=\s*\{", line):
            dependency = tomllib.loads(line)
            name, settings = next(iter(dependency.items()))
            if settings.pop("workspace", False):
                inherited = workspace["dependencies"][name]
                inherited = {"version": inherited} if isinstance(inherited, str) else dict(inherited)
                if "features" in settings:
                    settings["features"] = inherited.get("features", []) + settings["features"]
                inherited.update(settings)
                line = f"{name} = {value(inherited)}"
        lines.append(line)
    return "\n".join(lines) + "\n\n[workspace]\nresolver = \"2\"\n"


def run(command, cwd, env, log):
    label = f"{cwd.name}: " + " ".join(command)
    print("+ " + label, flush=True)
    result = subprocess.run(command, cwd=cwd, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    log.write(f"\n+ {label}\n{result.stdout}")
    log.flush()
    if result.returncode:
        print(result.stdout, flush=True)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--toolchain", action="append", help="default: stable and 1.96")
    parser.add_argument("--plan", action="store_true", help="print matrix without building")
    args = parser.parse_args()
    workspace = tomllib.loads((REPO_ROOT / "Cargo.toml").read_text())["workspace"]
    members = workspace["members"]
    # `default` is itself a selectable named feature. Enumerate it too, even
    # though its combinations resolve to optional-feature sets already covered.
    features = sorted(tomllib.loads((REPO_ROOT / "concord-api/Cargo.toml").read_text())["features"])
    subsets = [subset for n in range(len(features) + 1) for subset in itertools.combinations(features, n)]
    print(f"FEATURES: {', '.join(features)}; {len(subsets)} named subsets plus all-features; {len(members)} isolated crates", flush=True)
    if args.plan:
        for subset in subsets:
            print(",".join(subset) or "(none)")
        return 0
    env = os.environ.copy()
    env.update(CARGO_NET_OFFLINE="true", CARGO_BUILD_JOBS="4")
    TARGET.mkdir(exist_ok=True)
    ROOT.mkdir(exist_ok=True)
    failed = []
    checks = 0
    with (TARGET / "features.log").open("w") as log:
        # Complete all package inventories before replacing any isolated copies.
        inventories = {}
        for member in members:
            result = run(["cargo", "package", "--offline", "--allow-dirty", "--list"], REPO_ROOT / member, env, log)
            if result.returncode:
                failed.append(f"package --list {member}")
                continue
            inventories[member] = result.stdout.splitlines()
        if len(inventories) != len(members):
            return 1
        for member, inventory in inventories.items():
            destination = ROOT / member
            if destination.exists():
                shutil.rmtree(destination)
            destination.mkdir(parents=True)
            for name in inventory:
                source = REPO_ROOT / member / name
                if source.is_file() and name not in ("Cargo.toml", "Cargo.toml.orig", "Cargo.lock"):
                    copied = destination / name
                    copied.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(source, copied)
            manifest = detached_manifest((REPO_ROOT / member / "Cargo.toml").read_text(), workspace)
            (destination / "Cargo.toml").write_text(manifest)
        for toolchain in args.toolchain or ["stable", "1.96"]:
            # Reuse build artifacts across feature subsets, never across toolchains.
            env["CARGO_TARGET_DIR"] = str(TARGET / ("msrv" if toolchain == "1.96" else "debug-isolated"))
            jobs = [(member, ["--no-default-features"]) for member in members]
            jobs += [(member, []) for member in members]
            jobs += [("concord-api", ["--no-default-features"] + (["--features", ",".join(subset)] if subset else [])) for subset in subsets]
            jobs += [("concord-api", ["--all-features"])]
            for member, flags in jobs:
                cwd = ROOT / member
                command = ["cargo", "+" + toolchain, "check", "--offline", "--manifest-path", str(cwd / "Cargo.toml"), *flags]
                result = run(command, cwd, env, log)
                checks += 1
                if result.returncode:
                    failed.append(f"{toolchain} {member} {' '.join(flags)}")
        summary = {"features": features, "subsets": len(subsets), "checks": checks, "failed": failed, "crates": members}
        (TARGET / "features.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(f"FEATURES: {checks} checks; {len(failed)} failed", flush=True)
    return bool(failed)


if __name__ == "__main__":
    sys.exit(main())
