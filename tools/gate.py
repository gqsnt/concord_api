#!/usr/bin/env python3
"""Offline workspace checks. Requires Python 3.11+ for tomllib."""

import argparse
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tomllib

from matrix import rust_files, rust_tokens
from sockets import cases as socket_cases, filterset, socket_doc_lines


REPO_ROOT = Path(__file__).resolve().parents[1]
FEATURES = "concord-api/tower,concord-api/multipart"
STEPS = ("props", "fmt", "check", "lint", "test", "doc", "msrv", "features", "todo", "matrix", "links")


def environment(recipe):
    """Keep all cargo artifacts local, including the separate MSRV cache."""
    env = os.environ.copy()
    env["CARGO_NET_OFFLINE"] = "true"
    target = REPO_ROOT / "target"
    env["CARGO_TARGET_DIR"] = str(target / "msrv" if recipe == "msrv" else target)
    env.setdefault("PROPTEST_CASES", "256")
    if recipe == "doc":
        env["RUSTDOCFLAGS"] = "-D warnings"
    return env


def run(command, recipe):
    print("+ " + shlex.join(command), flush=True)
    subprocess.run(command, cwd=REPO_ROOT, env=environment(recipe), check=True)


def commands(recipe):
    """Return commands in execution order; no later command runs after failure."""
    full = ["--features", FEATURES]
    if recipe == "props":
        return [["cargo", "test", "--offline", "-p", "concord-core", "-p", "concord-extras", "--all-features", "--test", "prop_*"]]
    if recipe == "fmt":
        return [["cargo", "fmt", "--all", "--check"]]
    if recipe == "check":
        base = ["cargo", "check", "--offline", "--workspace", "--all-targets"]
        return [base, base + full]
    if recipe == "lint":
        # Apply workspace lints and promote every Clippy warning to an error.
        return [["cargo", "clippy", "--offline", "--workspace", "--all-targets"]
                + full + ["--", "-D", "warnings"]]
    if recipe == "test":
        runtime = ["cargo", "nextest", "run", "--offline", "--workspace"] + full
        docs = ["cargo", "test", "--offline", "--workspace", "--doc"] + full
        if os.environ.get("CONCORD_SKIP_SOCKETS") == "1":
            runtime += ["-E", filterset()]
            lines = socket_doc_lines()
            print(f"SOCKETS: skipping {len(socket_cases())} tests and {len(lines)} doctests", flush=True)
            if lines:
                docs += ["--", "--skip", "concord-testkit/src/lib.rs"]
                safe = ["cargo", "test", "--offline", "-p", "concord-testkit", "--doc", "--"]
                for line in lines:
                    safe += ["--skip", str(line)]
                return [runtime, docs, safe]
        return [runtime, docs]
    if recipe == "doc":
        return [["cargo", "doc", "--offline", "--no-deps"] + full]
    if recipe == "msrv":
        return [["cargo", "+1.96", "check", "--offline", "--workspace"] + full]
    if recipe == "features":
        with (REPO_ROOT / "concord-api" / "Cargo.toml").open("rb") as manifest:
            names = set(tomllib.load(manifest).get("features", {}))
        base = ["cargo", "check", "--offline", "-p", "concord-api", "--no-default-features"]
        # `default` is a named feature too; test its explicit selection alone.
        return ([base] + [base + ["--features", name] for name in sorted(names)]
                + [base + ["--all-features"]])
    if recipe == "links":
        return [[sys.executable, "-B", str(REPO_ROOT / "tools" / "links.py")]]
    if recipe == "matrix":
        return [[sys.executable, "-B", str(REPO_ROOT / "tools" / "matrix.py")]]
    raise ValueError(f"unknown recipe: {recipe}")


def todo_counts():
    """Count todo! macro invocations, excluding comments and literal contents."""
    errors = 0
    for member in tomllib.loads((REPO_ROOT / "Cargo.toml").read_text())["workspace"]["members"]:
        manifest = REPO_ROOT / member / "Cargo.toml"
        count = 0
        try:
            with manifest.open("rb") as stream:
                name = tomllib.load(stream)["package"]["name"]
            for path in rust_files(manifest.parent):
                tokens = [token.text for token in rust_tokens(path.read_text(encoding="utf-8"))]
                count += sum(tokens[i:i + 3] in (["todo", "!", "("],
                                                 ["todo", "!", "{"],
                                                 ["todo", "!", "["])
                             for i in range(len(tokens) - 2))
            print(f"{name}: {count} todo!()", flush=True)
        except (OSError, ValueError, KeyError) as error:
            errors += 1
            print(f"{manifest.parent.name}: count unavailable ({error})", flush=True)
    print("TODO: counted" + (f" ({errors} unreadable crates)" if errors else ""), flush=True)


def execute(recipe):
    try:
        if recipe == "todo":
            todo_counts()
            return 0
        for command in commands(recipe):
            run(command, recipe)
    except subprocess.CalledProcessError as error:
        code = error.returncode
        print(f"{recipe.upper()}: FAILED (exit {code})", flush=True)
        return code if code > 0 else 1
    except (OSError, ValueError) as error:
        print(f"{recipe.upper()}: FAILED ({error})", flush=True)
        return 1
    except KeyboardInterrupt:
        print(f"{recipe.upper()}: FAILED (interrupted)", flush=True)
        return 130
    # matrix.py owns its prescribed terminal summary.
    if recipe != "matrix":
        print(f"{recipe.upper()}: passed", flush=True)
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("recipe", choices=(*STEPS, "gate"))
    recipe = parser.parse_args().recipe
    if recipe != "gate":
        return execute(recipe)
    results = {}
    code = 0
    for step in STEPS:
        code = execute(step)
        results[step] = code
        if code:
            break
    for step in STEPS:
        status = "pass" if results.get(step) == 0 else "fail"
        detail = " (not run)" if step not in results else ""
        print(f"GATE {step}: {status}{detail}", flush=True)
    failed = next((name for name, result in results.items() if result), None)
    print(f"GATE: FAILED at {failed}" if failed else "GATE: passed", flush=True)
    return code


if __name__ == "__main__":
    sys.exit(main())
