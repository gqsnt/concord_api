#!/usr/bin/env python3
"""Checks that every ```rust block of IMPLEMENTATION_READOUT.md appears verbatim in the file named
in backticks in the paragraph before it. Run from the repository root:

    python3 -I v4/extensions-real-apis/readout_check.py
"""

import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))


def main():
    text = open(os.path.join(HERE, "IMPLEMENTATION_READOUT.md"), encoding="utf-8").read()
    failures = 0
    checked = 0
    source = None
    for part in re.split(r"(```rust\n.*?```)", text, flags=re.S):
        if part.startswith("```rust\n"):
            block = part[len("```rust\n"):-3]
            if source is None:
                print("FAIL  a code block without a named source file")
                failures += 1
                continue
            body = open(os.path.join(ROOT, source), encoding="utf-8").read()
            checked += 1
            if block not in body:
                print(f"FAIL  block not found verbatim in {source}:\n{block}")
                failures += 1
        else:
            named = re.findall(r"`([\w./-]+\.rs)`", part)
            if named:
                source = named[-1]
    print(f"{checked} code blocks checked, {failures} not found")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
