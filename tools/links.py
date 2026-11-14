#!/usr/bin/env python3
"""Check local files and GitHub heading anchors in every tracked Markdown file.

Use --anchors=warn while headings are being edited. Missing files always fail.
Web URLs and GitHub source-line anchors are not checked.
"""

import argparse
import html
from pathlib import Path
import re
import subprocess
import sys
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
LINK = re.compile(r"!?\[([^\]]*)\]\(\s*(?:<([^>]+)>|((?:[^\s()]|\([^()]*\))+))(?:\s+[^)]*)?\)")
REFERENCE = re.compile(r"!?\[([^\]]+)\]\[([^\]]*)\]")
DEFINITION = re.compile(r"^ {0,3}\[([^\]]+)\]:\s*(?:<([^>]+)>|(\S+))")
FENCE = re.compile(r"^ {0,3}(`{3,}|~{3,})")


def prose(path):
    """Yield lines outside fenced code, retaining their original line numbers."""
    fence = None
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        match = FENCE.match(line)
        if fence:
            if match and match[1][0] == fence[0] and len(match[1]) >= len(fence) and not line[match.end():].strip():
                fence = None
            continue
        if match:
            fence = match[1]
            continue
        yield number, line


def slug(text):
    """GitHub lowercases heading text, removes punctuation, and hyphenates spaces."""
    text = re.sub(r"<[^>]*>", "", text)
    text = LINK.sub(lambda match: match[1], text)
    text = html.unescape(text).lower()
    text = re.sub(r"(?<!\w)_+|_+(?!\w)", "", text)
    return re.sub(r"[^\w\- ]", "", text).replace(" ", "-")


def anchors(path):
    found = set()
    previous = None
    for number, line in prose(path):
        heading = re.match(r"^ {0,3}#{1,6}(?:[ \t]+(.*)|$)", line)
        if heading:
            text = re.sub(r"\s+#+\s*$", "", heading[1] or "").strip()
        elif previous and previous[0] == number - 1 and re.fullmatch(r" {0,3}(?:=+|-+)\s*", line):
            text = previous[1].strip()
        else:
            previous = (number, line) if line.strip() else None
            continue
        base = slug(text)
        candidate = base
        suffix = 0
        while candidate in found:
            suffix += 1
            candidate = f"{base}-{suffix}"
        found.add(candidate)
        previous = None
    return found


def links(path):
    lines = list(prose(path))
    definitions = {}
    for _, line in lines:
        if match := DEFINITION.match(line):
            definitions[" ".join(match[1].lower().split())] = match[2] or match[3]
    for number, line in lines:
        if DEFINITION.match(line):
            continue
        line = re.sub(r"(`+).*?\1", "", line)
        for match in LINK.finditer(line):
            yield number, match[2] or match[3]
        line = LINK.sub("", line)
        for match in REFERENCE.finditer(line):
            label = " ".join((match[2] or match[1]).lower().split())
            if label in definitions:
                yield number, definitions[label]
        line = REFERENCE.sub("", line)
        for match in re.finditer(r"\[([^\]]+)\]", line):
            label = " ".join(match[1].lower().split())
            if label in definitions:
                yield number, definitions[label]


def check(path, cache):
    for number, target in links(path):
        url = urlsplit(target)
        if url.scheme or url.netloc:
            continue
        file, anchor = unquote(url.path), unquote(url.fragment)
        resolved = (path.parent / file).resolve() if file else path.resolve()
        location = f"{path.relative_to(ROOT)}:{number}: {target}"
        if not resolved.exists():
            yield "file", f"{location}: no such file"
        elif anchor and resolved.suffix.lower() == ".md":
            if resolved not in cache:
                cache[resolved] = anchors(resolved)
            if anchor not in cache[resolved]:
                yield "anchor", f"{location}: no such heading"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--anchors", choices=("error", "warn"), default="error")
    args = parser.parse_args(argv)
    tracked = subprocess.check_output(["git", "ls-files", "-z"], cwd=ROOT).decode().split("\0")
    pages = [ROOT / name for name in tracked if name.endswith(".md")]
    cache = {}
    broken = [item for page in pages for item in check(page, cache)]
    for kind, message in broken:
        print(f"{kind.upper()}: {message}")
    files = sum(kind == "file" for kind, _ in broken)
    headings = sum(kind == "anchor" for kind, _ in broken)
    count = sum(1 for page in pages for _ in links(page))
    print(f"LINKS: {count} links in {len(pages)} pages, {files} broken files, {headings} broken anchors")
    return int(bool(files or (headings and args.anchors == "error")))


if __name__ == "__main__":
    sys.exit(main())
