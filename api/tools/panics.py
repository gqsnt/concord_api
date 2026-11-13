#!/usr/bin/env python3
"""Conservative non-test Rust inventory, independent of feature selection.

Lists panic operations, indexing/slicing, arithmetic, numeric `as` casts and
every allow/expect attribute. Operators are deliberately over-reported (float
and overloaded arithmetic too): review is safer than guessing inferred types.
Each finding requires a PANIC-SAFETY comment immediately above or on its line.
This lexical gate complements, rather than replaces, Clippy/type checking.
"""

import argparse
from collections import Counter
from dataclasses import dataclass
import json
import os
from pathlib import Path
import re
import subprocess
import sys

from matrix import rust_tokens

API = Path(__file__).resolve().parents[1]
CRATES = ("concord-core", "concord-extras", "concord")
NUMBER = re.compile(r"(?:[iu](?:8|16|32|64|128|size)|f(?:32|64))$")
REASON = re.compile(r"//\s*PANIC-SAFETY:\s*(\S.*)")
# matrix.rust_tokens splits joint punctuation into individual characters.
JOINT_TOKENS = {
    "::", "->", "=>", "==", "!=", "<=", ">=", "&&", "||", "<<", ">>",
    "+=", "-=", "*=", "/=", "%=", "^=", "&=", "|=", "<<=", ">>=", "..", "...", "..=",
}


@dataclass(frozen=True)
class Finding:
    line: int
    kind: str
    text: str
    reason: str


def closing(tokens, start):
    pairs = {"[": "]", "(": ")", "{": "}"}
    stack = []
    for at in range(start, len(tokens)):
        text = tokens[at].text
        if text in pairs:
            stack.append(pairs[text])
        elif stack and text == stack[-1]:
            stack.pop()
            if not stack:
                return at + 1
    raise ValueError(f"unclosed delimiter at line {tokens[start].line}")


def production_tokens(source):
    """Remove complete cfg(test) items, including their attributes and bodies."""
    tokens = list(rust_tokens(source))
    result = []
    i = 0
    while i < len(tokens):
        if [t.text for t in tokens[i:i + 7]] == ["#", "[", "cfg", "(", "test", ")", "]"]:
            i += 7
            while i < len(tokens):
                if tokens[i].text == "#" and tokens[i + 1].text == "[":
                    i = closing(tokens, i + 1)
                elif tokens[i].text in ("{", ";"):
                    i = closing(tokens, i) if tokens[i].text == "{" else i + 1
                    break
                else:
                    i += 1
        else:
            result.append(tokens[i])
            i += 1
    return result


def reason_at(lines, line):
    # A reason must touch the operation, not just occur somewhere in its function.
    for at in (line - 1, line - 2):
        if at >= 0 and (match := REASON.search(lines[at])):
            return match[1].strip()
    return ""


def macro_syntax(tokens):
    """Exclude matchers and structural tokens, never whole transcribers.

    Repetition contents still receive the ordinary audit. In particular,
    `values[$($index)*]` remains indexing; `helper!(@tag [$($tokens)*])`
    introduces a token group after a selector, not an indexed value.
    """
    ignored = set()
    for i, token in enumerate(tokens):
        if token.text == "macro_rules" and i + 3 < len(tokens) and tokens[i + 1].text == "!":
            body = i + 3
            if tokens[body].text in ("{", "(", "["):
                end = closing(tokens, body)
                arm = body + 1
                while arm < end - 1:
                    if tokens[arm].text in (";", ","):
                        arm += 1
                        continue
                    if tokens[arm].text not in ("(", "[", "{"):
                        break
                    after = closing(tokens, arm)
                    if [t.text for t in tokens[after:after + 2]] != ["=", ">"]:
                        break
                    ignored.update(range(arm, after))
                    # Move to the next arm, but leave this entire body audited.
                    arm = closing(tokens, after + 2)
        if token.text == "$" and i + 1 < len(tokens) and tokens[i + 1].text == "(":
            after = closing(tokens, i + 1)
            # A repetition has an optional single-token separator, including
            # joint punctuation such as `::` which our lexer splits apart.
            for width in (3, 2, 1, 0):
                op = after + width
                if op >= len(tokens) or tokens[op].text not in ("*", "+", "?"):
                    continue
                separator = "".join(t.text for t in tokens[after:op])
                if width == 0 or separator in JOINT_TOKENS or (
                    width == 1 and separator not in ("*", "+", "?", "$", "(", ")", "[", "]", "{", "}")
                ):
                    ignored.update(range(after, op + 1))
                    break
        if token.text == "[" and i >= 2 and tokens[i - 2].text == "@" and re.fullmatch(r"[a-zA-Z_]\w*", tokens[i - 1].text):
            ignored.add(i)
    return ignored


def findings(source):
    lines = source.splitlines()
    tokens = production_tokens(source)
    syntax = macro_syntax(tokens)
    seen = set()
    attributes = set()
    for i, token in enumerate(tokens):
        if i in syntax:
            continue
        text = token.text
        previous = tokens[i - 1].text if i else ""
        following = tokens[i + 1].text if i + 1 < len(tokens) else ""
        kind = None
        if text == "#" and following == "[":
            end = closing(tokens, i + 1)
            attributes.update(range(i, end))
            if tokens[i + 2].text in ("allow", "expect"):
                kind = "lint-exception"
        elif i in attributes:
            continue
        elif text in ("unwrap", "expect") and previous == "." and following == "(":
            kind = text
        elif text in ("panic", "unreachable", "assert") and following == "!":
            kind = text + "!"
        elif previous == "." and following in ("(", ":") and text in (
            "split_at", "split_at_mut", "split_off", "drain", "remove", "insert",
            "chunks", "chunks_mut", "chunks_exact", "windows", "as_chunks",
            "sum", "product", "pow", "next_power_of_two", "div_ceil",
        ):
            kind = "range-or-arithmetic-method"
        elif text == "as" and NUMBER.fullmatch(following):
            kind = "numeric-cast"
        elif text == "[" and (previous in (")", "]", "}", "<literal>", "?") or
                              (re.fullmatch(r"[a-zA-Z_]\w*", previous or "") and
                               previous not in ("return", "let", "in", "const", "static", "pub", "type", "mut", "impl", "for") and
                               not (i > 1 and tokens[i - 2].text == "'"))):
            kind = "index-or-slice"
        elif text in ("+", "-", "*", "/", "%"):
            if text == "-" and following == ">":
                continue
            if text == "+" and following in ("Send", "Sync", "Sized", "Unpin", "Clone", "'", "?"):
                continue
            if text == "+" and (previous == ":" or following in ("Default", "Copy", "TryFrom", "TryInto", "fmt", "serde", ":")):
                continue  # trait bounds
            if text == "*" and not (previous in (")", "]", "}", "<literal>") or
                                    previous.isdigit() or
                                    (re.fullmatch(r"[a-zA-Z_]\w*", previous or "") and
                                     previous not in ("return", "if", "else", "in", "let", "mut"))):
                continue  # dereference/pointer, not multiplication
            if text == "*" and previous == ":":
                continue  # glob import
            if text == "/" and previous == "<literal>" and following == "<literal>":
                continue
            kind = "arithmetic"
        elif text in ("<", ">") and following == text:
            # Shifts cannot be reliably separated from nested generic arguments
            # without a parser. Record numeric-looking shifts conservatively.
            tail = tokens[i + 2].text if i + 2 < len(tokens) else ""
            if lines[token.line - 1].lstrip().startswith("impl"):
                continue
            if previous != ">" and not re.fullmatch(r"[A-Z]\w*", previous) and tail not in ("where", "for", "impl") and (tail.isdigit() or previous.isdigit() or re.fullmatch(r"[a-z_]\w*", tail)):
                kind = "shift"
        if kind and (token.line, kind) not in seen:
            seen.add((token.line, kind))
            yield Finding(token.line, kind, lines[token.line - 1].strip(), reason_at(lines, token.line))


def inventory():
    for crate in CRATES:
        for path in sorted((API / crate / "src").rglob("*.rs")):
            # These files are included only by cfg(test) modules in lib.rs.
            if path.name == "engine_after_caller.rs":
                continue
            for finding in findings(path.read_text()):
                yield path.relative_to(API), finding


def compiler_check():
    """Type-aware cross-check of arithmetic and narrowing, including expansions."""
    command = ["cargo", "clippy", "--offline", "--lib", "--all-features", "--message-format=json"]
    for crate in CRATES:
        command += ["-p", crate]
    command += ["--", "-W", "clippy::arithmetic_side_effects", "-W", "clippy::cast_possible_truncation",
                "-W", "clippy::cast_possible_wrap", "-W", "clippy::cast_sign_loss",
                "-W", "clippy::indexing_slicing"]
    env = os.environ.copy()
    env.update(CARGO_NET_OFFLINE="true", CARGO_BUILD_JOBS="4", CARGO_TARGET_DIR=str(API / "target"))
    result = subprocess.run(command, cwd=API, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    (API / "target/m5-panics-clippy.jsonl").write_text(result.stdout)
    missing = set()
    for line in result.stdout.splitlines():
        message = json.loads(line)
        if message.get("reason") != "compiler-message":
            continue
        diagnostic = message["message"]
        code = (diagnostic.get("code") or {}).get("code", "")
        if not code.startswith("clippy::"):
            continue
        for span in diagnostic["spans"]:
            if not span["is_primary"]:
                continue
            path = API / span["file_name"]
            if path.is_file() and path.relative_to(API).parts[0] in CRATES:
                if not reason_at(path.read_text().splitlines(), span["line_start"]):
                    missing.add((str(path.relative_to(API)), span["line_start"], code))
    for path, line, code in sorted(missing):
        print(f"{path}:{line}: {code}: compiler cross-check requires a local PANIC-SAFETY reason")
    if result.returncode:
        print(result.stderr)
    print(f"PANICS compiler cross-check: {len(missing)} unreviewed diagnostics; cargo exit {result.returncode}")
    return bool(missing) or bool(result.returncode)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--list", action="store_true", help="print all findings, even reviewed ones")
    parser.add_argument("--compiler", action="store_true", help="also cross-check type-aware Clippy diagnostics")
    args = parser.parse_args()
    rows = list(inventory())
    counts = Counter(f.kind for _, f in rows)
    missing = [(p, f) for p, f in rows if not f.reason]
    for path, finding in rows if args.list else missing:
        print(f"{path}:{finding.line}: {finding.kind}: {finding.text}\n  reason: {finding.reason or 'MISSING'}")
    print(f"PANICS: {len(rows)} sites; {len(missing)} without reason; " + ", ".join(f"{k}={v}" for k, v in sorted(counts.items())))
    return (compiler_check() if args.compiler else False) or bool(missing)


if __name__ == "__main__":
    sys.exit(main())
