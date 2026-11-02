#!/usr/bin/env python3
"""Checks the file and test references in campaign records mechanically.

Every backticked `path.ext` or `path.ext::test_name` in the given Markdown files must resolve:
relative to the repository root, to the record's directory, to `crates/concord/tests/` (the test
matrix's convention), or (for a bare file name) to exactly one file of that name in the
repository; a `::name` must be a `fn name` in that file. Prints one
line per unresolved reference and exits 1 if there is any.

Usage: python3 -I scripts/check-records.py [FILES...]   (default: v4/finalization/*.md, v4/TEST_MATRIX.md)

Earlier campaigns' records (`v4/native-repr/`, `v4/refinement/`) describe their own revision and are
checked at it, not against the current tree.
"""

import glob
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EXTS = "rs|md|py|sh|json|txt|toml|stderr|yml|diff|jsonl"
REF = re.compile(r"`([A-Za-z0-9_./{},*<>-]+\.(?:" + EXTS + r"))(?:::([A-Za-z_][A-Za-z0-9_]*))?`")


def tracked():
    out = subprocess.run(["git", "-C", ROOT, "ls-files"], capture_output=True, text=True,
                         check=True).stdout.split("\n")
    return [p for p in out if p]


def resolve(ref, record_dir, files, by_name):
    if any(c in ref for c in "{}*<>"):
        return None  # a pattern, not a path
    for base in (ROOT, record_dir, os.path.join(ROOT, "crates/concord/tests")):
        p = os.path.normpath(os.path.join(base, ref))
        if os.path.isfile(p):
            return p
    # a path given relative to a crate or test directory, or a bare file name
    matches = [f for f in files if f == ref or f.endswith("/" + ref)]
    if len(matches) == 1:
        return os.path.join(ROOT, matches[0])
    if not matches and "/" not in ref:
        named = by_name.get(ref, [])
        if len(named) == 1:
            return os.path.join(ROOT, named[0])
    return "" if not matches else "ambiguous:" + ",".join(matches[:3])


def main(argv):
    records = argv or sorted(glob.glob(os.path.join(ROOT, "v4/finalization/*.md"))) + [
        os.path.join(ROOT, "v4/TEST_MATRIX.md")]
    files = tracked()
    by_name = {}
    for f in files:
        by_name.setdefault(os.path.basename(f), []).append(f)
    bad = 0
    checked = 0
    for record in records:
        record_dir = os.path.dirname(os.path.abspath(record))
        text = open(record, encoding="utf-8").read()
        for line_no, line in enumerate(text.split("\n"), 1):
            for m in REF.finditer(line):
                ref, test = m.group(1), m.group(2)
                path = resolve(ref, record_dir, files, by_name)
                if path is None:
                    continue
                checked += 1
                where = f"{os.path.relpath(record, ROOT)}:{line_no}"
                if not path or path.startswith("ambiguous:"):
                    print(f"{where}: unresolved `{ref}`" + (f" ({path})" if path else ""))
                    bad += 1
                elif test:
                    body = open(path, encoding="utf-8", errors="replace").read()
                    if not re.search(r"\bfn\s+" + re.escape(test) + r"\b", body):
                        print(f"{where}: no `fn {test}` in `{os.path.relpath(path, ROOT)}`")
                        bad += 1
    print(f"{checked} references checked, {bad} unresolved")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
