#!/usr/bin/env python3
"""Rewrites `via` declarations into formats written in place (docs/MIGRATION.md, "From `via` to
formats"):

    -> T via Json            ->  -> json T          (Text: text, Binary: bytes)
    body b: T via Json       ->  json b: T
    body b?: T via Csv       ->  Csv b?: T           (kept as a format path)

A representation becomes a word only when it provably is Concord's marker: `concord::Json` (or
`::concord::Json`), or a bare `Json` imported in the same file with `use concord::...`
(`use concord::{Json, Text as Plain}` maps `Plain` too). Anything else — `codec::Text`, a bare
`Text` not imported from `concord`, a renamed crate's `c4::Json`, a downstream `Form` — is kept as
a format path, which means the same type as before.

It only moves words: no default is introduced, so the result means exactly what the input meant.
Hoisting a repeated format into `request json;` / `response json;` on the client or a scope is a
separate, manual step (it changes which endpoints a later edit affects). Imports that only served
`via` (`use concord::{Json, Text, Binary}`) become unused and are reported by rustc. In Markdown
files only fenced code blocks are rewritten.

Usage: scripts/migrate_via.py FILE...   (rewrites in place, prints the number of edits per file)
       scripts/migrate_via.py --self-test
"""
import re
import sys

# `via` predates the built-in `Form`: a `Form` written then is a downstream type and stays a path.
WORDS = {"Json": "json", "Text": "text", "Binary": "bytes"}
IMPORT = re.compile(r"\buse\s+(?:::)?concord::(?:\{([^}]*)\}|(\w+(?:\s+as\s+\w+)?))\s*;")
REPR = r"((?:::)?[A-Za-z_]\w*(?:::[A-Za-z_]\w*)*)"
OUTPUT = re.compile(r"->(\s*)(.+?)\s+via\s+" + REPR)
BODY = re.compile(r"\bbody\s+(r#\w+|\w+)(\??)\s*:\s*(.+?)\s+via\s+" + REPR)


def imported_markers(text: str) -> dict[str, str]:
    """Bare names that `use concord::...` brings into scope for Concord's markers, with their word."""
    names = {}
    for m in IMPORT.finditer(text):
        for item in (m.group(1) or m.group(2)).split(","):
            parts = item.split()
            if not parts or parts[0] not in WORDS:
                continue
            alias = parts[2] if len(parts) == 3 and parts[1] == "as" else parts[0]
            names[alias] = WORDS[parts[0]]
    return names


def rewrite(text: str, imported: dict[str, str] | None = None) -> tuple[str, int]:
    count = 0
    if imported is None:
        imported = imported_markers(text)

    def word(path: str) -> str:
        segments = path.removeprefix("::").split("::")
        if len(segments) == 2 and segments[0] == "concord" and segments[1] in WORDS:
            return WORDS[segments[1]]
        if len(segments) == 1 and segments[0] in imported:
            return imported[segments[0]]
        return path

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


def self_test() -> None:
    cases = [
        ("use concord::{Json, Text};\nGET a() -> Item via Json;", "-> json Item"),
        ("use concord::{Json, Text};\nPUT b(body n: String via Text);", "text n: String"),
        ("GET c() -> Item via concord::Json;", "-> json Item"),
        ("GET d() -> Raw via ::concord::Binary;", "-> bytes Raw"),
        ("use concord::Text as Plain;\nGET e() -> String via Plain;", "-> text String"),
        # Not provably Concord's: kept as a path, so the meaning does not change (review F3).
        ("PUT f(body b: String via codec::Text);", "codec::Text b: String"),
        ("GET g() -> String via Text;", "-> Text String"),
        ("GET h() -> Item via c4::Json;", "-> c4::Json Item"),
        ("use concord::Json;\nPOST i(body f: Login via Form);", "Form f: Login"),
    ]
    for source, expected in cases:
        out, n = rewrite(source)
        assert n == 1 and expected in out, (source, out)
    print(f"self-test: {len(cases)} cases ok")


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
    if sys.argv[1:] == ["--self-test"]:
        self_test()
    else:
        main(sys.argv[1:])
