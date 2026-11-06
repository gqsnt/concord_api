#!/usr/bin/env python3
"""Checks the production dependency graph of the five crates.

Direct edges come from the manifests (`cargo metadata`, normal dependencies only: dev-dependencies
and the workspace build do not count). Resolved-graph exclusions come from `cargo tree -e normal`
on one package at a time, so feature unification across the workspace cannot hide an edge.

Usage: python3 -I scripts/check-graph.py
"""

import json
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Allowed direct normal dependencies of each production package.
ALLOWED = {
    "concord-core": {"bytes", "percent-encoding", "reqwest", "url"},
    "concord-extras": {"concord-core", "csv", "csv-core", "futures-core", "http-body", "mime", "serde",
                       "serde_json", "serde_urlencoded"},
    # Pagination knows no Concord crate and no runtime: request factories are plain closures.
    "concord-pagination": {"futures-util", "url"},
    "concord-macros": {"http", "mime", "proc-macro2", "quote", "syn"},
    "concord": {"concord-core", "concord-extras", "concord-macros"},
}

# Packages that must not appear in the resolved normal graph of `cargo tree -p PKG ARGS`.
EXCLUDED = [
    ("concord-core", [], {"concord-extras", "concord-macros", "concord", "serde_json", "serde_urlencoded"}),
    ("concord-extras", ["--all-features"], {"concord-macros", "concord", "concord-pagination"}),
    # The CSV parser and serde only with their features (http-body and futures-core already come
    # with reqwest, so the `stream` feature adds no package to the graph).
    ("concord-extras", [], {"csv", "csv-core", "serde", "serde_json"}),
    ("concord-extras", ["--no-default-features", "--features", "jsonl"], {"csv", "csv-core", "serde_urlencoded"}),
    ("concord-extras", ["--no-default-features", "--features", "csv"], {"serde_json", "serde_urlencoded"}),
    # The multipart format adds no package: no serializer, no CSV parser.
    ("concord-extras", ["--no-default-features", "--features", "multipart"], {"csv", "csv-core", "serde",
                                                                              "serde_json", "serde_urlencoded"}),
    # The SSE reader frames events and decodes no payload: no serializer at all.
    ("concord-extras", ["--no-default-features", "--features", "sse"], {"csv", "csv-core", "serde", "serde_json",
                                                                       "serde_urlencoded"}),
    ("concord-pagination", [], {"concord", "concord-core", "concord-extras", "concord-macros", "reqwest",
                                "tokio", "serde", "http", "url"}),
    ("concord-pagination", ["--features", "link"],
     {"concord", "concord-core", "concord-extras", "concord-macros", "reqwest", "http", "tokio", "serde"}),
    ("concord-macros", [], {"concord-core", "concord-extras", "concord", "reqwest", "serde"}),
    ("concord", ["--no-default-features"], {"concord-extras", "serde_json", "serde_urlencoded"}),
    ("concord", ["--no-default-features", "--features", "extras"], {"serde_json", "serde_urlencoded", "csv",
                                                                    "csv-core"}),
    ("concord", [], {"csv", "csv-core", "concord-pagination"}),
]


def main():
    problems = []
    meta = json.loads(subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps", "--locked"],
        cwd=ROOT, capture_output=True, text=True, check=True).stdout)
    packages = {p["name"]: p for p in meta["packages"]}
    for name, allowed in ALLOWED.items():
        normal = {d["name"] for d in packages[name]["dependencies"] if d["kind"] is None}
        extra = normal - allowed
        if extra:
            problems.append(f"{name}: unexpected direct dependencies {sorted(extra)}")
        print(f"{name}: {', '.join(sorted(normal))}")
    for name, args, excluded in EXCLUDED:
        tree = subprocess.run(
            ["cargo", "tree", "--locked", "-e", "normal", "--prefix", "none", "-p", name, *args],
            cwd=ROOT, capture_output=True, text=True, check=True).stdout
        found = {line.split()[0] for line in tree.splitlines() if line.strip()}
        bad = sorted(found & excluded)
        label = " ".join([name, *args])
        if bad:
            problems.append(f"{label}: resolved graph contains {bad}")
        else:
            print(f"{label}: none of {sorted(excluded)}")
    for p in problems:
        print(f"FAIL  {p}")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
