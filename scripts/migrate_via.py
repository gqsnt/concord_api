#!/usr/bin/env python3
"""Rewrites `via` declarations into formats written in place (docs/MIGRATION.md, "From `via` to
formats"):

    -> T via Json            ->  -> Json T
    body b: T via concord::Text  ->  concord::Text b: T
    body b?: T via ::c4::Csv     ->  ::c4::Csv b?: T

It only moves the path: the result names exactly the type the input named, because which type a
path means is decided by Rust's name resolution (imports in scope, `cfg`, a local module named
`concord`), which a text rewrite cannot see. Replacing Concord's own `Json`, `Text`, `Binary` by
the words `json`, `text`, `bytes` is a manual, optional step. A bare lowercase name (`via text`)
is left untouched and reported: in format position it would be read as a built-in word, so write
`self::text` (or the word, if it is Concord's) yourself; the compiler's error for the leftover
`via` says the same.

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


REPR = r"((?:::)?(?:r#)?[A-Za-z_]\w*(?:::(?:r#)?[A-Za-z_]\w*)*)"
OUTPUT = re.compile(r"->(\s*)(.+?)\s+via\s+" + REPR)
BODY = re.compile(r"\bbody\s+(r#\w+|\w+)(\??)\s*:\s*(.+?)\s+via\s+" + REPR)


def bare_lowercase(path: str) -> bool:
    """A single lowercase segment: in format position it would be read as a built-in word."""
    return "::" not in path and path.removeprefix("r#")[:1].islower()


def rewrite(text: str) -> tuple[str, int]:
    count = 0

    def body(m: re.Match) -> str:
        nonlocal count
        if bare_lowercase(m.group(4)):
            print(f"kept: `via {m.group(4)}` (write `self::{m.group(4).removeprefix('r#')}` "
                  "or the word yourself)", file=sys.stderr)
            return m.group(0)
        count += 1
        return f"{m.group(4)} {m.group(1)}{m.group(2)}: {m.group(3)}"

    def output(m: re.Match) -> str:
        nonlocal count
        if bare_lowercase(m.group(3)):
            print(f"kept: `via {m.group(3)}` (write `self::{m.group(3).removeprefix('r#')}` "
                  "or the word yourself)", file=sys.stderr)
            return m.group(0)
        count += 1
        return f"->{m.group(1)}{m.group(3)} {m.group(2)}"

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
    # (input, expected substring, number of rewrites)
    cases = [
        ("GET a() -> Item via concord::Json;", "-> concord::Json Item", 1),
        ("GET b() -> Item via Json;", "-> Json Item", 1),
        ("PUT c(body b: String via codec::Text);", "codec::Text b: String", 1),
        # Name resolution decides what a path means; the path is kept (review F3, F3a, F3b).
        ("use crate::codec::Text;\nmod m { use concord::Text; }\nPUT d(body b: String via Text);",
         "Text b: String", 1),
        ("// use concord::Text;\nPUT e(body b: String via Text);", "Text b: String", 1),
        ("mod concord { pub struct Text; }\nPUT f(body b: String via concord::Text);",
         "concord::Text b: String", 1),
        ("GET g() -> Item via c4::Json;", "-> c4::Json Item", 1),
        ("POST h(body f: Login via Form);", "Form f: Login", 1),
        ("PUT i(body b?: String via ::c4::Text);", "::c4::Text b?: String", 1),
        # A bare lowercase name would become a built-in word: left for the author (review F3c).
        ("PUT j(body b: String via text);", "body b: String via text", 0),
        ("GET k() -> Item via r#json;", "-> Item via r#json", 0),
    ]
    for source, expected, count in cases:
        out, n = rewrite(source)
        assert n == count and expected in out, (source, out)
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
