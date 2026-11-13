#!/usr/bin/env python3
"""Check that every link between the pages resolves.

Reads README.md and every Markdown file under docs/. A relative link must name a
file or a folder that exists; a link to a heading of a Markdown page (`page.md#a-heading`)
must name a heading of that page, by GitHub's rule for anchors. Links to the web,
and line anchors (`#L12`), are not checked.
"""

import argparse
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[2]
LINK = re.compile(r"(?<!\!)\[[^\]]*\]\(([^)\s]+)\)")
FENCE = re.compile(r"^\s*```")


def anchors(path, cache={}):
    if path not in cache:
        found = set()
        counts = {}
        in_fence = False
        for line in path.read_text(encoding="utf-8").splitlines():
            if FENCE.match(line):
                in_fence = not in_fence
                continue
            if in_fence or not line.startswith("#"):
                continue
            text = line.lstrip("#").strip().lower()
            text = re.sub(r"[^\w\- ]", "", text).replace(" ", "-")
            n = counts.get(text, 0)
            counts[text] = n + 1
            found.add(text if n == 0 else f"{text}-{n}")
        cache[path] = found
    return cache[path]


def links(path):
    in_fence = False
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if FENCE.match(line):
            in_fence = not in_fence
            continue
        if in_fence:
            continue
        line = re.sub(r"`[^`]*`", "", line)
        for match in LINK.finditer(line):
            yield number, match.group(1)


def check(path):
    for number, target in links(path):
        if re.match(r"^[a-z]+:", target):
            continue
        file, _, anchor = target.partition("#")
        resolved = (path.parent / file).resolve() if file else path
        if not resolved.exists():
            yield f"{path.relative_to(ROOT)}:{number}: {target}: no such file"
            continue
        if anchor and not anchor.startswith("L") and resolved.suffix == ".md":
            if anchor.lower() not in anchors(resolved):
                yield f"{path.relative_to(ROOT)}:{number}: {target}: no such heading"


def main():
    argparse.ArgumentParser(description=__doc__).parse_args()
    pages = [ROOT / "README.md", *sorted((ROOT / "docs").rglob("*.md"))]
    count = sum(1 for page in pages for _ in links(page))
    broken = [line for page in pages for line in check(page)]
    print("\n".join(broken)) if broken else None
    print(f"LINKS: {count} links in {len(pages)} pages, {len(broken)} broken")
    return 1 if broken else 0


if __name__ == "__main__":
    sys.exit(main())
