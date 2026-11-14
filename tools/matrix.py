#!/usr/bin/env python3
"""Build target/contracts-matrix.md from the contracts and contract-prefixed Rust tests.

Only an active contract without any numbered test or a nonempty NO-TEST.md
reason fails --strict. Ignored tests are counted as tests, and marked clearly.
This is a static inventory: cfg/feature conditions are not evaluated and a test
name records an author's claim, not proof that the test passes.
"""

import argparse
from dataclasses import dataclass
import os
from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]
CONTRACT_ROW = re.compile(r"^\|\s*(C(\d+)\.(\d+))\s*\|\s*(.*)")
TEST_NAME = re.compile(r"c(\d+)_(\d+)_\w*")
EXEMPTION = re.compile(r"^\s*(C\d+\.\d+):\s*(\S.*?)\s*$")
SKIP_DIRS = {"target", ".git", ".venv", "__pycache__"}


@dataclass(frozen=True)
class Contract:
    number: str
    text: str
    withdrawn: bool
    line: int


@dataclass(frozen=True)
class Token:
    text: str
    line: int


@dataclass(frozen=True)
class Test:
    number: str
    name: str
    path: Path
    line: int
    ignored: bool


def first_cell(row):
    """Read a Markdown cell without splitting an inline-code or escaped pipe."""
    code_ticks = 0
    i = 0
    while i < len(row):
        if row[i] == "\\":
            i += 2
            continue
        if row[i] == "`":
            end = i + 1
            while end < len(row) and row[end] == "`":
                end += 1
            ticks = end - i
            if not code_ticks:
                code_ticks = ticks
            elif code_ticks == ticks:
                code_ticks = 0
            i = end
            continue
        if row[i] == "|" and not code_ticks:
            return row[:i].strip()
        i += 1
    return row.strip()


def read_contracts(path):
    contracts = []
    seen = set()
    for line, text in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        match = CONTRACT_ROW.match(text)
        if not match:
            continue
        number = match[1]
        if number in seen:
            raise ValueError(f"duplicate contract {number} at {path}:{line}")
        seen.add(number)
        cell = first_cell(match[4])
        # A mention of a withdrawn clause does not withdraw the whole row.
        plain = cell.lstrip("*_` ")
        withdrawn = bool(re.match(r"(?:withdrawn|not used)\b", plain, flags=re.IGNORECASE))
        contracts.append(Contract(number, cell, withdrawn, line))
    if not contracts:
        raise ValueError(f"no contract rows found in {path}")
    return contracts


def rust_files(root):
    """Never inventory generated dependencies or cached build outputs."""
    for directory, dirs, files in os.walk(root):
        dirs[:] = sorted(d for d in dirs if d not in SKIP_DIRS)
        for name in sorted(files):
            if name.endswith(".rs"):
                yield Path(directory) / name


def rust_tokens(source):
    """Tokenize enough Rust to find attributes, skipping comments and literals.

    Literal placeholders prevent text from joining across an intervening string.
    Nested block comments, raw strings, byte/C strings and lifetimes are handled.
    """
    i = 0
    line = 1
    while i < len(source):
        start = i
        token_line = line
        if source[i].isspace():
            i += 1
        elif source.startswith("//", i):
            end = source.find("\n", i)
            i = len(source) if end < 0 else end
        elif source.startswith("/*", i):
            depth = 1
            i += 2
            while i < len(source) and depth:
                if source.startswith("/*", i):
                    depth += 1
                    i += 2
                elif source.startswith("*/", i):
                    depth -= 1
                    i += 2
                else:
                    i += 1
        else:
            raw = re.match(r'(?:br|cr|r)(#*)"', source[i:]) if source[i] in "bcr" else None
            quoted = re.match(r'(?:b|c)?"', source[i:]) if source[i] in 'bc"' else None
            char = re.match(r"(?:b)?'(?:\\(?:u\{[0-9a-fA-F_]+\}|x[0-9a-fA-F]{2}|.)|[^'\\\n])'",
                            source[i:]) if source[i] in "b'" else None
            if raw:
                delimiter = '"' + raw[1]
                end = source.find(delimiter, i + len(raw[0]))
                i = len(source) if end < 0 else end + len(delimiter)
                yield Token("<literal>", token_line)
            elif quoted:
                i += len(quoted[0])
                while i < len(source):
                    if source[i] == "\\":
                        i += 2
                    elif source[i] == '"':
                        i += 1
                        break
                    else:
                        i += 1
                yield Token("<literal>", token_line)
            elif char:
                i += len(char[0])
                yield Token("<literal>", token_line)
            elif source[i].isalpha() or source[i] == "_":
                i += 1
                while i < len(source) and (source[i].isalnum() or source[i] == "_"):
                    i += 1
                yield Token(source[start:i], token_line)
            else:
                i += 1
                yield Token(source[start:i], token_line)
        line += source[start:i].count("\n")


def find_tests(path):
    tokens = list(rust_tokens(path.read_text(encoding="utf-8")))
    attrs = []
    i = 0
    while i < len(tokens):
        text = tokens[i].text
        if text == "#" and i + 1 < len(tokens) and tokens[i + 1].text == "[":
            end = i + 2
            depth = 1
            while end < len(tokens) and depth:
                if tokens[end].text == "[":
                    depth += 1
                elif tokens[end].text == "]":
                    depth -= 1
                end += 1
            attrs.append([token.text for token in tokens[i + 2:end - 1]])
            i = end
            continue
        if text == "fn" and i + 1 < len(tokens):
            name = tokens[i + 1]
            match = TEST_NAME.fullmatch(name.text)
            is_test = any(attr == ["test"] or attr[:5] == ["tokio", ":", ":", "test", "("]
                          or attr == ["tokio", ":", ":", "test"] for attr in attrs)
            if match and is_test:
                ignored = any(attr == ["ignore"] or attr[:2] == ["ignore", "="] for attr in attrs)
                yield Test(f"C{int(match[1])}.{int(match[2])}", name.text, path, name.line, ignored)
            attrs = []
        elif text in {";", "{", "}", "!"}:
            # Attributes on a module, macro or non-function must not leak.
            attrs = []
        i += 1


def read_exemptions(path):
    if not path.exists():
        return {}
    return {match[1]: match[2] for line in path.read_text(encoding="utf-8").splitlines()
            if (match := EXEMPTION.fullmatch(line))}


def excerpt(text):
    text = re.sub(r"\[([^]]+)\]\([^)]+\)", r"\1", text)
    text = text.replace("`", "").replace("**", "")
    words = text.split()
    return " ".join(words[:12]) + (" …" if len(words) > 12 else "")


def escape_cell(text):
    return text.replace("|", "\\|").replace("\n", " ")


def test_link(test, build):
    relative = Path(os.path.relpath(test.path, build)).as_posix()
    return f"[`{test.name}`]({relative}#L{test.line})" + (" (ignored)" if test.ignored else "")


def render(contracts, tests, exemptions, build):
    by_number = {}
    for test in tests:
        by_number.setdefault(test.number, []).append(test)
    known = {contract.number for contract in contracts}
    rows = ["# Contract test matrix", "",
            "Generated by `python3 -B tools/matrix.py`; do not edit by hand.", "",
            "Static inventory of `#[test]` and `#[tokio::test]` functions in the repository. "
            "Feature/cfg conditions are not evaluated. Ignored tests count as tests; "
            "their presence does not mean a contract has passed.", "",
            "| Contract | First words | State | Tests | Ignored |",
            "|---|---|---|---|---|"]
    for contract in contracts:
        numbered = by_number.get(contract.number, [])
        link = f"[{contract.number}](../docs/reference/contracts.md#L{contract.line})"
        links = "<br>".join(test_link(test, build) for test in numbered) or "—"
        state = "not used" if contract.withdrawn else "holds"
        rows.append(f"| {link} | {escape_cell(excerpt(contract.text))} | {state} | "
                    f"{links} | {sum(test.ignored for test in numbered)} |")

    missing = [contract for contract in contracts if contract.number not in by_number]
    rows += ["", "## Contracts with no test", ""]
    for contract in missing:
        reason = exemptions.get(contract.number)
        suffix = " (not used)" if contract.withdrawn else ""
        if reason:
            suffix += f" — NO-TEST: {reason}"
        rows.append(f"- {contract.number}{suffix}")
    if not missing:
        rows.append("None.")

    orphans = [test for test in tests if test.number not in known]
    rows += ["", "## Tests whose number is no contract", ""]
    rows += [f"- {test.number}: {test_link(test, build)}" for test in orphans] or ["None."]
    withdrawn = [contract for contract in contracts if contract.withdrawn and contract.number in by_number]
    rows += ["", "## Unused contracts that still have a test", ""]
    rows += [f"- {contract.number}: " + ", ".join(test_link(test, build) for test in by_number[contract.number])
             for contract in withdrawn] or ["None."]
    active = [contract for contract in contracts if not contract.withdrawn]
    covered = sum(contract.number in by_number for contract in active)
    summary = f"contracts: {len(active)} that hold, {covered} with a test, {len(active) - covered} with none"
    rows += ["", summary, ""]
    gaps = [contract.number for contract in active
            if contract.number not in by_number and contract.number not in exemptions]
    return "\n".join(rows), summary, gaps


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--strict", action="store_true", help="fail for unexplained active coverage gaps")
    args = parser.parse_args(argv)
    try:
        contracts = read_contracts(ROOT / "docs" / "reference" / "contracts.md")
        tests = [test for path in rust_files(ROOT) for test in find_tests(path)]
        build = ROOT / "target"
        exemptions = read_exemptions(ROOT / "tools" / "NO-TEST.md")
        document, summary, gaps = render(contracts, tests, exemptions, build)
        build.mkdir(parents=True, exist_ok=True)
        (build / "contracts-matrix.md").write_text(document, encoding="utf-8")
        if args.strict and gaps:
            print(f"strict: {len(gaps)} active contracts have neither a test nor a NO-TEST reason")
        print(summary, flush=True)
        return int(args.strict and bool(gaps))
    except (OSError, ValueError) as error:
        print(f"matrix: could not generate report: {error}", file=sys.stderr)
        print("contracts: unavailable that hold, unavailable with a test, unavailable with none", flush=True)
        return int(args.strict)


if __name__ == "__main__":
    sys.exit(main())
