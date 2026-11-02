#!/usr/bin/env python3
"""Rewrites `via` declarations into formats written in place (docs/MIGRATION.md, "From `via` to
formats"):

    -> T via Json            ->  -> json T          (Text: text, Binary: bytes)
    body b: T via Json       ->  json b: T
    body b?: T via Csv       ->  Csv b?: T           (kept as a format path)

A representation becomes a word only when its path names Concord's marker by itself:
`concord::Json` or `::concord::Json` (also `Text`, `Binary`). Every other path — a bare `Json`,
`codec::Text`, a renamed crate's `c4::Json`, a downstream `Form` — is kept as a format path and
therefore names the same type as before: which type a bare name means depends on Rust's name
resolution (imports in scope, `cfg`), which a text rewrite cannot see. A kept bare `Json` that is
`concord::Json` can be replaced by the word `json` by hand.

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

REPR = r"((?:::)?[A-Za-z_]\w*(?:::[A-Za-z_]\w*)*)"
OUTPUT = re.compile(r"->(\s*)(.+?)\s+via\s+" + REPR)
BODY = re.compile(r"\bbody\s+(r#\w+|\w+)(\??)\s*:\s*(.+?)\s+via\s+" + REPR)


def rewrite(text: str) -> tuple[str, int]:
    count = 0

    def word(path: str) -> str:
        segments = path.removeprefix("::").split("::")
        if len(segments) == 2 and segments[0] == "concord" and segments[1] in WORDS:
            return WORDS[segments[1]]
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
        ("GET a() -> Item via concord::Json;", "-> json Item"),
        ("PUT b(body n: String via concord::Text);", "text n: String"),
        ("GET c() -> Raw via ::concord::Binary;", "-> bytes Raw"),
        # A bare name or another path is kept: it names the same type as before (review F3, F3a).
        ("use concord::{Json, Text};\nGET d() -> Item via Json;", "-> Json Item"),
        ("PUT e(body b: String via codec::Text);", "codec::Text b: String"),
        ("use crate::codec::Text;\nmod m { use concord::Text; }\nPUT f(body b: String via Text);",
         "Text b: String"),
        ("// use concord::Text;\nPUT g(body b: String via Text);", "Text b: String"),
        ("GET h() -> Item via c4::Json;", "-> c4::Json Item"),
        ("POST i(body f: Login via Form);", "Form f: Login"),
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
