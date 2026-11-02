#!/usr/bin/env python3
"""Rewrites `via` declarations into formats written in place (docs/MIGRATION.md, "From `via` to
formats"):

    -> T via Json            ->  -> json T          (Text: text, Binary: bytes, qualified or
    body b: T via Json       ->  json b: T           not: taken to be Concord's markers)
    body b?: T via Csv       ->  Csv b?: T           (any other type is kept as a format path,
                                                     including a downstream `Form`)

It only moves words: no default is introduced, so the result means exactly what the input meant.
Hoisting a repeated format into `request json;` / `response json;` on the client or a scope is a
separate, manual step (it changes which endpoints a later edit affects). Imports that only served
`via` (`use concord::{Json, Text, Binary}`) become unused and are reported by rustc. In Markdown
files only fenced code blocks are rewritten.

Usage: scripts/migrate_via.py FILE...   (rewrites in place, prints the number of edits per file)
"""
import re
import sys

# `via` predates the built-in `Form`: a `Form` written then is a downstream type and stays a path.
WORDS = {"Json": "json", "Text": "text", "Binary": "bytes"}
REPR = r"((?:::)?[A-Za-z_]\w*(?:::[A-Za-z_]\w*)*)"
OUTPUT = re.compile(r"->(\s*)(.+?)\s+via\s+" + REPR)
BODY = re.compile(r"\bbody\s+(r#\w+|\w+)(\??)\s*:\s*(.+?)\s+via\s+" + REPR)


def word(path: str) -> str:
    return WORDS.get(path.split("::")[-1], path)


def rewrite(text: str) -> tuple[str, int]:
    count = 0

    def body(m: re.Match) -> str:
        nonlocal count
        count += 1
        return f"{word(m.group(4))} {m.group(1)}{m.group(2)}: {m.group(3)}"

    def output(m: re.Match) -> str:
        nonlocal count
        count += 1
        return f"->{m.group(1)}{word(m.group(3))} {m.group(2)}"

    text = BODY.sub(body, text)
    text = OUTPUT.sub(output, text)
    return text, count


def rewrite_markdown(text: str) -> tuple[str, int]:
    parts = re.split(r"(```[^\n]*\n.*?```)", text, flags=re.S)
    total = 0
    for i, part in enumerate(parts):
        if part.startswith("```"):
            parts[i], n = rewrite(part)
            total += n
    return "".join(parts), total


def main(paths: list[str]) -> None:
    for path in paths:
        with open(path, encoding="utf-8") as f:
            source = f.read()
        new, n = (rewrite_markdown if path.endswith(".md") else rewrite)(source)
        if n:
            with open(path, "w", encoding="utf-8", newline="\n") as f:
                f.write(new)
        print(f"{n:4d}  {path}")


if __name__ == "__main__":
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    main(sys.argv[1:])
