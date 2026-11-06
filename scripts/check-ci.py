#!/usr/bin/env python3
r"""Keeps the toolchain policy and the CI gate selection in one place.

--toolchain           `rust-version` (the minimum) equals the release pinned in rust-toolchain.toml,
                      every production package inherits it, and no living script or workflow
                      selects another stable release with `cargo +X.Y` (only `+nightly`, for the
                      non-blocking lane).
--workflow            the workflow guard below on .github/workflows/ci.yml; and the scripts give tar
                      its archive on standard input.
--workflow-file PATH  the workflow guard alone, on another file (scripts/tests uses it).

Usage: python3 -I scripts/check-ci.py --toolchain | --workflow | --workflow-file PATH

Workflow guard
--------------
It parses the workflow (the YAML subset below), then inspects its executable structure: the
triggers, every job's platforms (`runs-on`, a literal matrix) and conditions, and the `run` steps.
It passes only when, on Linux and on Windows, an enabled blocking job has an enabled step whose
script runs `scripts/verify.sh --package --bench` as a plain command whose failure fails the step;
when an enabled job runs `scripts/verify.sh --nightly-only` (the non-blocking lane); when every
other verify.sh invocation selects the complete inventory; when no step runs cargo itself; and
when nothing selects gates by platform (`runner.os`, `RUNNER_OS`). Comments, `echo` strings and
other inert mentions of verify.sh are reported as notes, never counted as invocations.

Static inspection only: a pass shows that the file, as written, asks CI to run the gate. It does
not show that CI ran, which runner image or shell was used, or what the gate printed; the job logs
are the execution evidence. It does not evaluate expressions beyond the literals listed below, the
environment that earlier steps can change (GITHUB_ENV, GITHUB_PATH), or the scripts it calls.

Supported YAML subset (anything else is rejected with its line, never guessed):
  * block mappings and block sequences indented with spaces (a tab in indentation is rejected); a
    sequence may sit at its key's indentation (`steps:` then `- run: ...`);
  * keys: plain or quoted, unique within their mapping (a duplicate is rejected);
  * scalars on one line: plain, 'single-quoted', "double-quoted" (escapes \\ \" \/ \n \t \r \0 and
    space, \xHH, \uHHHH, \UHHHHHHHH); plain `true`/`false`, `null`/`~`/empty and integers are typed
    (YAML 1.2: `on`, `yes`, `no` stay strings), everything else is a string;
  * block scalars `|` (literal) and `>` (folded) with an optional `-` or `+` chomping indicator;
    a line holding only spaces counts as empty;
  * flow sequences of scalars on one line (`[ubuntu-latest, windows-latest]`) and the empty `{}`;
  * `#` comments (a whole line, or after whitespace); CR LF line ends.
  Rejected: anchors (&), aliases (*), tags (!), merge keys (<<), complex keys (?), directives and
  document markers (%, ---, ...), flow mappings other than `{}`, nested flow collections, comments
  inside flow sequences, scalars continued on another line (plain, quoted or flow), `: ` inside a
  plain scalar, and explicit block indentation indicators (`|2`).

Supported structure (inspected for the gate; reported, never assumed):
  * `on`: the workflow must run on `push` and `pull_request`, each without filters;
  * a gate job uses only `name, runs-on, strategy, steps, if, continue-on-error, timeout-minutes,
    env, needs, defaults, permissions`; `runs-on` is a literal label or `${{ matrix.KEY }}` naming
    a literal list (no `include`/`exclude`); labels `ubuntu-*`, `windows-*`, `macos-*` name Linux,
    Windows, macOS, anything else is unsupported; every job it `needs` must be enabled too;
  * `if` (job or step): absent, `true`, `success()` or `always()` runs; `false`, `failure()` or
    `cancelled()` (with or without `${{ }}`) does not; anything else is unsupported for a gate;
  * `continue-on-error`: absent or false; true (also `${{ true }}`) swallows the exit status of a
    gate (the nightly lane's job may set it); an expression is unsupported for a gate;
  * a gate step is a `run` step using only `name, id, run, shell, if, continue-on-error,
    timeout-minutes, env`, with `shell: bash` (the step's or `defaults.run.shell`; required when the
    job includes Windows, whose default shell is PowerShell; any other shell is unsupported), no
    `defaults.run.working-directory`, and no BASH_ENV/ENV/SHELLOPTS/BASHOPTS in its environment;
  * a job-level reusable workflow (`uses:`) is reported and not inspected: it never counts as a gate.

Supported run-script subset (only to find the invocation; this is not a shell evaluator):
  * the script is split into words, operators and newlines: '...' and "..." quoting, backslash
    escapes, backslash-newline continuation, `#` comments at the start of a word;
  * a gate invocation is a command whose words are `scripts/verify.sh` or `./scripts/verify.sh`,
    optionally after `bash`, followed only by `--package`, `--bench`, `--nightly` or
    `--nightly-only` (quoting the path is fine; an expansion is not);
  * it must stand alone between newlines: `||`, `&&`, `;`, `|` or `&` next to it, a redirection or
    a subshell is rejected (they can skip it or swallow its status: `|| true`, `; true`);
  * the step's script may contain no `set +e` (or `+o errexit`), no compound command (if, for,
    while, case, {, (, function, !, time, ...), no exit/return/exec/source/eval/trap/alias/shopt and
    no here-document: such a script is rejected rather than evaluated.
"""

import glob
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
# The production packages; everything else in the workspace is a test, example or benchmark.
PRODUCTION = ("concord-core", "concord-codecs", "concord-extras", "concord-macros", "concord", "concord-pagination")


def read(path):
    with open(os.path.join(ROOT, path), encoding="utf-8") as f:
        return f.read()


def living_files(pattern):
    """Files under scripts/ (recursively, without Python caches) whose name matches the pattern."""
    return sorted(p for p in glob.glob(os.path.join(ROOT, "scripts", "**", pattern), recursive=True)
                  if os.path.isfile(p) and "__pycache__" not in p.split(os.sep))


def toolchain():
    problems = []
    pin = re.search(r'^channel\s*=\s*"([^"]+)"', read("rust-toolchain.toml"), re.M)
    minimum = re.search(r'^rust-version\s*=\s*"([^"]+)"', read("Cargo.toml"), re.M)
    if not pin or not minimum:
        return ["rust-toolchain.toml channel or workspace rust-version not found"]
    pin, minimum = pin.group(1), minimum.group(1)
    if not re.fullmatch(r"\d+\.\d+\.\d+", pin):
        problems.append(f"the pin must be an exact release, not {pin!r}")
    if pin != minimum:
        problems.append(f"rust-version {minimum} differs from the pinned toolchain {pin}")
    meta = json.loads(subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps", "--locked"],
        cwd=ROOT, capture_output=True, text=True, check=True).stdout)
    found = set()
    for package in meta["packages"]:
        if package["name"] in PRODUCTION:
            found.add(package["name"])
            if package.get("rust_version") != minimum:
                problems.append(f"{package['name']}: rust-version {package.get('rust_version')}, expected {minimum}")
    for name in PRODUCTION:
        if name not in found and os.path.isdir(os.path.join(ROOT, "crates", name)):
            problems.append(f"{name}: not a workspace member")
    living = living_files("*") + glob.glob(os.path.join(ROOT, ".github", "workflows", "*.yml"))
    for path in living:
        with open(path, encoding="utf-8") as f:
            for n, line in enumerate(f, 1):
                for m in re.finditer(r"(?:cargo|rustup run|toolchain install)\s+\+?(\d+\.\d+(?:\.\d+)?)", line):
                    if m.group(1) != pin:
                        problems.append(f"{os.path.relpath(path, ROOT)}:{n}: selects Rust {m.group(1)}")
    print(f"minimum = pin = {pin}; production packages: {', '.join(sorted(found))}")
    return problems


# ---------------------------------------------------------------------------------------------
# The supported YAML subset (see the module documentation).


class Unsupported(Exception):
    """A construct outside the supported grammar, at a 1-based line (0: not tied to a line)."""

    def __init__(self, line, message):
        super().__init__(message)
        self.line = line
        self.message = message


class _Yaml:
    def __init__(self, text):
        text = text.lstrip("\ufeff")
        if text.endswith("\n"):
            text = text[:-1]  # the end of the last line, not an empty line after it
        self.lines = [ln[:-1] if ln.endswith("\r") else ln for ln in text.split("\n")]
        self.i = 0
        self.comments = []  # (line, text) of whole-line comments, reported as notes

    def peek(self):
        """(index, indentation, content) of the next line that is not blank or a comment."""
        while self.i < len(self.lines):
            line = self.lines[self.i]
            stripped = line.strip(" \t")
            if stripped == "":
                self.i += 1
                continue
            if stripped.startswith("#"):
                self.comments.append((self.i + 1, stripped))
                self.i += 1
                continue
            n = len(line) - len(line.lstrip(" "))
            if line[n] == "\t":
                raise Unsupported(self.i + 1, "a tab in indentation")
            return self.i, n, line[n:]
        return None

    def document(self):
        first = self.peek()
        if first is None:
            raise Unsupported(0, "an empty workflow")
        node = self.block(first[1])
        rest = self.peek()
        if rest is not None:
            raise Unsupported(rest[0] + 1, "content outside the top-level node")
        return node

    def block(self, n):
        k, _, content = self.peek()
        if content.startswith(("%", "---", "...")):
            raise Unsupported(k + 1, "directives and document markers")
        if content == "?" or content.startswith("? "):
            raise Unsupported(k + 1, "complex keys (?)")
        if _is_entry(content):
            return self.sequence(n)
        return self.mapping(n)

    def mapping(self, n):
        result = {}
        while True:
            nxt = self.peek()
            if nxt is None or nxt[1] < n:
                return result
            k, indentation, content = nxt
            if indentation > n:
                raise Unsupported(k + 1, "unexpected indentation")
            if _is_entry(content):
                raise Unsupported(k + 1, "a sequence entry where a mapping key is expected")
            split = _split_key(content, k + 1)
            if split is None:
                raise Unsupported(k + 1, "expected `key: value` (a scalar continued on another line?)")
            key, rest = split
            if key in result:
                raise Unsupported(k + 1, f"duplicate key {key!r}")
            self.i = k + 1
            result[key] = self.value(rest, n, k + 1, compact=True)

    def sequence(self, n):
        result = []
        while True:
            nxt = self.peek()
            if nxt is None or nxt[1] < n:
                return result
            k, indentation, content = nxt
            if indentation > n:
                raise Unsupported(k + 1, "unexpected indentation")
            if not _is_entry(content):
                return result  # the next key after a sequence at its key's indentation
            after = content[1:]
            text = after.lstrip(" ")
            if _is_entry(text):
                raise Unsupported(k + 1, "nested sequences on one line (`- - x`)")
            if text and text[0] not in "[{|>&*!#" and _split_key(text, k + 1) is not None:
                # `- key: value`: a mapping whose first key stands at its own column.
                column = n + 1 + (len(after) - len(text))
                self.lines[k] = " " * column + text
                self.i = k
                result.append(self.mapping(column))
                continue
            self.i = k + 1
            result.append(self.value(text, n, k + 1, compact=False))

    def value(self, rest, n, line, compact):
        """The value after `key:` or `-` at indentation n; a nested block comes from later lines."""
        if rest == "" or rest.startswith("#"):
            nxt = self.peek()
            if nxt is not None and nxt[1] > n:
                return self.block(nxt[1])
            if compact and nxt is not None and nxt[1] == n and _is_entry(nxt[2]):
                return self.sequence(n)
            return None
        c = rest[0]
        if c in "&*!":
            raise Unsupported(line, "anchors, aliases and tags")
        if c in "|>":
            return self.block_scalar(rest, n, line)
        if c in "@`%,]}?":
            raise Unsupported(line, f"a plain scalar starting with {c!r}")
        if c == "[":
            result = _flow_sequence(rest, line)
        elif c == "{":
            if _strip_comment(rest).strip() != "{}":
                raise Unsupported(line, "flow mappings")
            result = {}
        elif c in "\"'":
            result, tail = _quoted(rest, line)
            if _strip_comment(tail).strip():
                raise Unsupported(line, "text after a quoted scalar")
        else:
            text = _strip_comment(rest).rstrip()
            if ": " in text or ":\t" in text or text.endswith(":"):
                raise Unsupported(line, "`: ` inside a plain scalar (quote it)")
            result = _plain(text)
        nxt = self.peek()
        if nxt is not None and nxt[1] > n:
            raise Unsupported(nxt[0] + 1, "a scalar continued on another line")
        return result

    def block_scalar(self, header, n, line):
        m = re.fullmatch(r"([|>])([+-]?)(?:[ \t]+#.*)?[ \t]*", header)
        if not m:
            raise Unsupported(line, "this block scalar header (indentation indicators are unsupported)")
        style, chomp = m.group(1), m.group(2)
        lines, content = [], None
        while self.i < len(self.lines):
            raw = self.lines[self.i]
            if raw.strip(" ") == "":
                lines.append("")
                self.i += 1
                continue
            indentation = len(raw) - len(raw.lstrip(" "))
            if content is None:
                if indentation <= n:
                    break
                if raw[indentation] == "\t":
                    raise Unsupported(self.i + 1, "a tab in indentation")
                content = indentation
            if indentation < content:
                break
            lines.append(raw[content:])
            self.i += 1
        trailing = 0
        while lines and lines[-1] == "":
            lines.pop()
            trailing += 1
        text = "\n".join(lines) if style == "|" else _fold(lines)
        if chomp == "-":
            return text
        if chomp == "+":
            return text + "\n" * (trailing + (1 if lines else 0))
        return text + "\n" if lines else ""


def _is_entry(content):
    return content == "-" or content.startswith(("- ", "-\t"))


def _strip_comment(text):
    for k, c in enumerate(text):
        if c == "#" and (k == 0 or text[k - 1] in " \t"):
            return text[:k]
    return text


def _split_key(content, line):
    """(key, rest) of `key: rest`, or None when the line is not a mapping entry."""
    if content.startswith("<<"):
        raise Unsupported(line, "merge keys (<<)")
    if content[0] in "\"'":
        key, tail = _quoted(content, line)
        if tail.startswith(":") and (len(tail) == 1 or tail[1] in " \t"):
            return key, tail[1:].lstrip(" \t")
        return None
    if content[0] in "[]{}&*!|>%@`#,?":
        return None
    for k, c in enumerate(content):
        if c == "#" and k > 0 and content[k - 1] in " \t":
            return None
        if c == ":" and (k + 1 == len(content) or content[k + 1] in " \t"):
            key = content[:k].rstrip()
            return (key, content[k + 1:].lstrip(" \t")) if key else None
    return None


ESCAPES = {"\\": "\\", '"': '"', "/": "/", "n": "\n", "t": "\t", "r": "\r", "0": "\0", " ": " "}


def _quoted(text, line):
    """The quoted scalar at the start of text and the text after it (one line only)."""
    quote, out, k = text[0], [], 1
    while k < len(text):
        c = text[k]
        if quote == "'":
            if c == "'":
                if text[k + 1:k + 2] == "'":
                    out.append("'")
                    k += 2
                    continue
                return "".join(out), text[k + 1:]
            out.append(c)
            k += 1
            continue
        if c == '"':
            return "".join(out), text[k + 1:]
        if c == "\\":
            e = text[k + 1:k + 2]
            if e in ESCAPES:
                out.append(ESCAPES[e])
                k += 2
                continue
            width = {"x": 2, "u": 4, "U": 8}.get(e, 0)
            digits = text[k + 2:k + 2 + width]
            if width and re.fullmatch("[0-9A-Fa-f]{%d}" % width, digits):
                out.append(chr(int(digits, 16)))
                k += 2 + width
                continue
            raise Unsupported(line, f"the escape \\{e} in a double-quoted scalar")
        out.append(c)
        k += 1
    raise Unsupported(line, "a quoted scalar continued on another line")


def _plain(text):
    if text in ("", "~", "null", "Null", "NULL"):
        return None
    if text in ("true", "True", "TRUE"):
        return True
    if text in ("false", "False", "FALSE"):
        return False
    if re.fullmatch(r"[-+]?[0-9]+", text):
        return int(text)
    return text


def _flow_sequence(text, line):
    items, k, current, quoted = [], 1, "", False
    while k < len(text):
        c = text[k]
        if c in "[{":
            raise Unsupported(line, "nested flow collections")
        if c == "#" and (current == "" or current[-1] in " \t"):
            raise Unsupported(line, "a comment inside a flow sequence")
        if c in ",]":
            stripped = current.strip()
            if stripped and not quoted:
                items.append(_plain(stripped))
            elif not quoted and c == ",":
                raise Unsupported(line, "an empty flow sequence item")
            current, quoted = "", False
            k += 1
            if c == "]":
                if _strip_comment(text[k:]).strip():
                    raise Unsupported(line, "text after a flow sequence")
                return items
            continue
        if c in "\"'" and not quoted and not current.strip():
            value, tail = _quoted(text[k:], line)
            items.append(value)
            quoted, current = True, ""
            k = len(text) - len(tail)
            continue
        if quoted and c not in " \t":
            raise Unsupported(line, "text after a quoted flow sequence item")
        current += c
        k += 1
    raise Unsupported(line, "a flow sequence continued on another line")


def _fold(lines):
    """YAML folding of a `>` block scalar's lines (trailing empty lines already removed)."""
    out, empties, previous = [], 0, None
    for line in lines:
        if line == "":
            empties += 1
            continue
        kind = "more" if line[:1] in (" ", "\t") else "normal"
        if previous is None:
            out.append("\n" * empties)
        elif previous == "normal" and kind == "normal":
            out.append(" " if empties == 0 else "\n" * empties)
        else:
            out.append("\n" * (empties + 1))
        out.append(line)
        empties, previous = 0, kind
    return "".join(out)


def parse_yaml(text):
    """(document, whole-line comments) of a workflow in the supported subset; or Unsupported."""
    parser = _Yaml(text)
    return parser.document(), parser.comments


# ---------------------------------------------------------------------------------------------
# The supported run-script subset (see the module documentation).

NEWLINE = "\n"
OPERATORS = ("&&", "||", ";;", "|&", ">>", "<<", "<&", ">&", "&>", ";", "&", "|", "<", ">", "(", ")")
SEPARATORS = {";", "&", "&&", "||", "|", "|&", ";;", NEWLINE}
COMPOUND = {"if", "then", "elif", "else", "fi", "for", "while", "until", "do", "done", "case",
            "esac", "select", "function", "{", "}", "[[", "]]", "!", "time", "coproc"}
CONTROL = {"exit", "return", "exec", "source", ".", "eval", "trap", "alias", "shopt"}
GATE_PATHS = ("scripts/verify.sh", "./scripts/verify.sh")
GATE_FLAGS = ("--package", "--bench", "--nightly", "--nightly-only")
CARGO_GATES = {"test", "clippy", "fmt", "doc", "build", "check", "bench", "tree", "package", "run"}


class Word:
    def __init__(self, text, raw, expands):
        self.text, self.raw, self.expands = text, raw, expands


class Command:
    """A simple command: its words, any redirection/subshell operators, and its separators."""

    def __init__(self, before):
        self.before, self.after, self.words, self.ops = before, None, [], []

    def show(self):
        return " ".join(w.raw for w in self.words) or " ".join(self.ops)


def _operator(script, k):
    for op in OPERATORS:
        if script.startswith(op, k):
            return op
    return None


def tokenize(script):
    """(words, operators and newlines, comments) of a run script; or Unsupported."""
    tokens, comments, k, n = [], [], 0, len(script)
    while k < n:
        c = script[k]
        if c == "\n":
            tokens.append(NEWLINE)
            k += 1
            continue
        if c in " \t\r":
            k += 1
            continue
        if c == "\\" and script[k + 1:k + 2] == "\n":
            k += 2
            continue
        if c == "#":
            end = script.find("\n", k)
            end = n if end < 0 else end
            comments.append(script[k:end])
            k = end
            continue
        op = _operator(script, k)
        if op:
            if op == "<<":
                raise Unsupported(0, "a here-document")
            tokens.append(op)
            k += len(op)
            continue
        start, text, expands = k, [], False
        while k < n:
            c = script[k]
            if c in " \t\r\n" or _operator(script, k):
                break
            if c == "\\":
                if script[k + 1:k + 2] != "\n":
                    text.append(script[k + 1:k + 2])
                k += 2
                continue
            if c == "'":
                end = script.find("'", k + 1)
                if end < 0:
                    raise Unsupported(0, "an unterminated quote")
                text.append(script[k + 1:end])
                k = end + 1
                continue
            if c == '"':
                k += 1
                while True:
                    if k >= n:
                        raise Unsupported(0, "an unterminated quote")
                    c = script[k]
                    if c == '"':
                        k += 1
                        break
                    if c == "\\" and script[k + 1:k + 2] in ("$", "`", '"', "\\", "\n"):
                        if script[k + 1] != "\n":
                            text.append(script[k + 1])
                        k += 2
                        continue
                    if c in "$`":
                        expands = True
                    text.append(c)
                    k += 1
                continue
            if c in "$`":
                expands = True
            text.append(c)
            k += 1
        tokens.append(Word("".join(text), script[start:k], expands))
    return tokens, comments


def commands(tokens):
    """The simple commands, each with the separator before and after it."""
    out, current, before = [], None, "start"
    for t in tokens:
        if isinstance(t, str) and t in SEPARATORS:
            if current is not None:
                current.after = t
                out.append(current)
                current = None
                before = t
            elif not (t == NEWLINE and before not in ("start", NEWLINE)):
                before = t  # a newline right after `||`, `&&`, `|`... continues that list
            continue
        if current is None:
            current = Command(before)
        if isinstance(t, Word):
            current.words.append(t)
        else:
            current.ops.append(t)
    if current is not None:
        current.after = "end"
        out.append(current)
    return out


def gate_invocation(command):
    """The verify.sh arguments when the command is a supported invocation, else None."""
    words = [w.text for w in command.words]
    if words[:1] == ["bash"]:
        words = words[1:]
    if not words or words[0] not in GATE_PATHS:
        return None
    return words[1:]


def shell_words(command):
    """The command's words after the prefixes that still run it: leading assignments
    (`NAME=value set -n`) and `builtin`/`command` (`builtin set -n`, `command set -o noexec`)."""
    words = [w.text for w in command.words]
    while words and re.match(r"[A-Za-z_][A-Za-z0-9_]*=", words[0]):
        words = words[1:]
    while words and words[0] in ("builtin", "command"):
        words = words[1:]
        while words and words[0].startswith("-"):
            words = words[1:]
    return words


def disables_errexit(command):
    words = shell_words(command)
    if words[:1] != ["set"]:
        return False
    for k, w in enumerate(words[1:], 1):
        if w == "+o" and words[k + 1:k + 2] == ["errexit"]:
            return True
        if w.startswith("+") and w != "+o" and "e" in w[1:]:
            return True
    return False


# Shell options a gate's step may turn on: they make a failure stop the step, never skip a command.
SAFE_SHORT_OPTIONS = set("eux")
SAFE_LONG_OPTIONS = {"errexit", "nounset", "pipefail", "xtrace"}


def unsupported_set(command):
    """Why a `set` in the gate's step is refused, or None. Only `-e`, `-u`, `-x` and `-o` with
    errexit, nounset, pipefail or xtrace are accepted (`set -eo pipefail`); anything else changes
    behaviour the guard does not model: `-n`/`-o noexec` reads the commands without running them
    (the gate would never run), `+...` turns checks off (errexit: `disables_errexit`). Prefixes
    that still run `set` are looked through (`shell_words`)."""
    words = shell_words(command)
    if words[:1] != ["set"]:
        return None
    k = 1
    while k < len(words):
        w = words[k]
        if w == "-o" or (w.startswith("-") and w.endswith("o") and len(w) > 2):
            name = words[k + 1] if k + 1 < len(words) else ""
            if set(w[1:-1]) <= SAFE_SHORT_OPTIONS and name in SAFE_LONG_OPTIONS:
                k += 2
                continue
            option = f"{w} {name}".strip()
        elif w.startswith("-") and len(w) > 1 and set(w[1:]) <= SAFE_SHORT_OPTIONS:
            k += 1
            continue
        else:
            option = w
        if "noexec" in option or (option.startswith("-") and not option.startswith("-o") and "n" in option):
            return f"`{command.show()}` turns on noexec: the step reads its commands without running them"
        return f"`{command.show()}` changes shell option `{option}`, which the guard does not model"
    return None


# ---------------------------------------------------------------------------------------------
# The workflow structure.

LINUX, WINDOWS, MACOS = "Linux", "Windows", "macOS"
EXPRESSION = re.compile(r"\$\{\{\s*(.*?)\s*\}\}", re.S)
TOP_KEYS = {"name", "run-name", "on", "env", "defaults", "jobs", "permissions", "concurrency"}
JOB_KEYS = {"name", "runs-on", "strategy", "steps", "if", "continue-on-error", "timeout-minutes",
            "env", "needs", "defaults", "permissions"}
STEP_KEYS = {"name", "id", "run", "shell", "if", "continue-on-error", "timeout-minutes", "env"}
SHELL_ENV = {"BASH_ENV", "ENV", "SHELLOPTS", "BASHOPTS"}


def _show(value):
    return str(value).lower() if isinstance(value, bool) else str(value)


def _platform(label):
    for prefix, name in (("ubuntu-", LINUX), ("windows-", WINDOWS), ("macos-", MACOS)):
        if label.startswith(prefix):
            return name
    return None


def _unwrap(value):
    if isinstance(value, str):
        m = EXPRESSION.fullmatch(value.strip())
        return m.group(1).strip() if m else value.strip()
    return value


def condition(value):
    """'runs', 'disabled' or 'unsupported' for an `if:` value (None: absent)."""
    if value is None:
        return "runs"
    v = _unwrap(value)
    if v is True or (isinstance(v, str) and v.lower() in ("true", "success()", "always()")):
        return "runs"
    if v is False or (isinstance(v, str) and v.lower() in ("false", "failure()", "cancelled()")):
        return "disabled"
    return "unsupported"


def flag(value):
    """True, False or None (unsupported) for a `continue-on-error` value (None: absent)."""
    if value is None:
        return False
    v = _unwrap(value)
    if isinstance(v, bool):
        return v
    if isinstance(v, str) and v.lower() in ("true", "false"):
        return v.lower() == "true"
    return None


def _strings(node):
    if isinstance(node, dict):
        for k, v in node.items():
            yield k
            yield from _strings(v)
    elif isinstance(node, list):
        for v in node:
            yield from _strings(v)
    elif isinstance(node, str):
        yield node


def _mapping(node, *keys):
    for key in keys:
        node = node.get(key) if isinstance(node, dict) else None
    return node if isinstance(node, dict) else {}


class Guard:
    def __init__(self, name):
        self.name = name
        self.problems, self.notes = [], []

    def fail(self, message):
        self.problems.append(f"{self.name}: {message}")

    def note(self, message):
        self.notes.append(f"{self.name}: {message}")

    def check(self, text):
        try:
            doc, comments = parse_yaml(text)
        except Unsupported as e:
            where = f":{e.line}" if e.line else ""
            self.problems.append(f"{self.name}{where}: unsupported workflow syntax: {e.message}")
            return
        for line, comment in comments:
            if "verify.sh" in comment:
                self.note(f"line {line}: a YAML comment mentions verify.sh (inert)")
        if not isinstance(doc, dict):
            self.fail("the workflow is not a mapping")
            return
        for key in doc:
            if key not in TOP_KEYS:
                self.fail(f"unsupported top-level key {key!r}")
        if any("runner.os" in s or "RUNNER_OS" in s for s in _strings(doc)):
            self.fail("selects gates by platform (runner.os / RUNNER_OS)")
        self.triggers(doc.get("on"))
        jobs = doc.get("jobs")
        if not isinstance(jobs, dict) or not jobs:
            self.fail("no jobs")
            return
        full, nightly = set(), False
        for job_id, job in jobs.items():
            platforms, lane = self.job(doc, jobs, job_id, job)
            full |= platforms
            nightly = nightly or lane
        for platform in (LINUX, WINDOWS):
            if platform not in full:
                self.fail(f"no effective full gate on {platform} "
                          "(an enabled blocking step running scripts/verify.sh --package --bench)")
        if not nightly:
            self.fail("no nightly lane (scripts/verify.sh --nightly-only in an enabled job)")
        if full:
            self.note(f"effective full gate on: {', '.join(sorted(full))}")

    def triggers(self, on):
        if isinstance(on, str):
            on = {on: None}
        elif isinstance(on, list):
            on = {e: None for e in on if isinstance(e, str)}
        if not isinstance(on, dict):
            self.fail("no `on` triggers")
            return
        for event in ("push", "pull_request"):
            if event not in on:
                self.fail(f"the workflow does not run on {event}")
            elif on[event] not in (None, {}):
                self.fail(f"unsupported trigger filter on {event} (the guard requires unfiltered triggers)")

    def disabled(self, jobs, job_id, seen=()):
        """Why the job, or a job it needs, may not run; None when all statically run."""
        job = jobs.get(job_id)
        if not isinstance(job, dict):
            return f"it needs an unknown job {job_id!r}"
        state = condition(job.get("if"))
        if state != "runs":
            kind = "statically disabled" if state == "disabled" else "unsupported condition"
            return f"job {job_id} has if: {_show(job.get('if'))} ({kind})"
        needs = job.get("needs") or []
        for other in [needs] if isinstance(needs, str) else needs:
            if other in seen + (job_id,):
                return f"needs cycle through {other!r}"
            why = self.disabled(jobs, other, seen + (job_id,))
            if why:
                return why
        return None

    def platforms(self, job):
        """(platforms, problem or None) of a job's runs-on."""
        runs_on = job.get("runs-on")
        if not isinstance(runs_on, str):
            return set(), f"unsupported runs-on {runs_on!r}"
        m = re.fullmatch(r"\$\{\{\s*matrix\.([A-Za-z0-9_-]+)\s*\}\}", runs_on.strip())
        if m:
            matrix = _mapping(job, "strategy", "matrix")
            if not matrix:
                return set(), f"runs-on {runs_on} without a literal matrix"
            if "include" in matrix or "exclude" in matrix:
                return set(), "matrix include/exclude is unsupported"
            labels = matrix.get(m.group(1))
            if not isinstance(labels, list) or not labels or not all(isinstance(x, str) for x in labels):
                return set(), f"matrix.{m.group(1)} is not a literal list of runner labels"
        elif "${{" in runs_on:
            return set(), f"unsupported runs-on expression {runs_on}"
        else:
            labels = [runs_on]
        found = set()
        for label in labels:
            platform = _platform(label)
            if platform is None:
                return set(), f"unsupported runner label {label!r}"
            found.add(platform)
        return found, None

    def job(self, doc, jobs, job_id, job):
        """(platforms with an effective full gate, whether it runs the nightly lane)."""
        if not isinstance(job, dict):
            self.fail(f"job {job_id} is not a mapping")
            return set(), False
        if "uses" in job:
            self.note(f"job {job_id}: reusable workflow {job['uses']!r} (not inspected, never a gate)")
            return set(), False
        steps = job.get("steps")
        if not isinstance(steps, list):
            self.fail(f"job {job_id} has no steps")
            return set(), False
        found = []
        for index, step in enumerate(steps, 1):
            if not isinstance(step, dict):
                self.fail(f"job {job_id} step {index} is not a mapping")
                continue
            if "run" not in step:
                continue
            script = step["run"]
            if not isinstance(script, str):
                self.fail(f"job {job_id} step {index}: `run` is not a string")
                continue
            try:
                tokens, comments = tokenize(script)
            except Unsupported as e:
                if re.search(r"\bcargo\s", script):
                    self.fail(f"job {job_id} step {index}: unsupported shell construct ({e.message}) in a step that mentions cargo")
                if "verify.sh" in script:
                    self.fail(f"job {job_id} step {index}: unsupported shell construct ({e.message}) in a step that mentions verify.sh")
                continue
            for comment in comments:
                if "verify.sh" in comment:
                    self.note(f"job {job_id} step {index}: a shell comment mentions verify.sh (inert)")
            for command in commands(tokens):
                self.cargo(job_id, index, command)
                args = gate_invocation(command) if command.words else None
                if args is not None:
                    found.append((index, step, command, args, tokens))
                elif any("verify.sh" in w.text for w in command.words):
                    self.note(f"job {job_id} step {index}: not a gate invocation: {command.show()}")
        if not found:
            return set(), False

        job_problems = []
        unknown = sorted(set(job) - JOB_KEYS)
        if unknown:
            job_problems.append(f"unsupported job keys {unknown}")
        why = self.disabled(jobs, job_id)
        if why:
            job_problems.append(why)
        platforms, problem = self.platforms(job)
        if problem:
            job_problems.append(problem)
        continues = flag(job.get("continue-on-error"))
        if continues is None:
            job_problems.append(f"unsupported continue-on-error {job.get('continue-on-error')!r}")
        env = set()
        default_shell = None
        for scope in (doc, job):
            if isinstance(scope.get("env"), dict):
                env |= set(scope["env"])
            run = _mapping(scope, "defaults", "run")
            if "working-directory" in run:
                job_problems.append("defaults.run.working-directory is unsupported for a gate")
            if "shell" in run:
                default_shell = run["shell"]

        effective, lane = set(), False
        for index, step, command, args, tokens in found:
            problems = list(job_problems)
            nightly_only = "--nightly-only" in args
            bad = [a for a in args if a not in GATE_FLAGS]
            if bad:
                problems.append(f"not a supported invocation: arguments {bad} (verify.sh refuses unknown ones)")
            if any(w.expands for w in command.words):
                problems.append("not a supported invocation: it contains an expansion")
            if not nightly_only and not ("--package" in args and "--bench" in args):
                problems.append("an incomplete inventory: every platform runs scripts/verify.sh --package --bench")
            if command.ops:
                problems.append(f"redirection or subshell {command.ops} at the gate is unsupported")
            for side in (command.before, command.after):
                if side not in ("start", "end", NEWLINE):
                    problems.append(f"shell operator `{side}` next to the gate: it can skip the gate or swallow its exit status")
            for other in commands(tokens):
                # After prefixes that still run it (`builtin exit 0`, `NAME=1 exec true`).
                plain = shell_words(other)
                first = plain[0] if plain else other.words[0].text if other.words else other.ops[0]
                if first in COMPOUND or first in CONTROL or "(" in other.ops:
                    problems.append(f"unsupported shell construct `{first}` in the gate's step (not evaluated)")
                if disables_errexit(other):
                    problems.append(f"`{other.show()}` disables errexit: the gate's exit status can be swallowed")
                elif unsupported_set(other):
                    problems.append(unsupported_set(other))
            state = condition(step.get("if"))
            if state == "disabled":
                problems.append(f"the step has if: {_show(step.get('if'))} (statically disabled)")
            elif state == "unsupported":
                problems.append(f"the step has if: {_show(step.get('if'))} (unsupported condition)")
            swallows = flag(step.get("continue-on-error"))
            if swallows is None:
                problems.append(f"unsupported continue-on-error {step.get('continue-on-error')!r}")
            elif swallows:
                problems.append("the step has continue-on-error: true, which swallows the gate's exit status")
            if not nightly_only and continues:
                problems.append("the job has continue-on-error: true, which swallows the gate's exit status")
            unknown = sorted(set(step) - STEP_KEYS)
            if unknown:
                problems.append(f"unsupported step keys {unknown}")
            names = env | (set(step["env"]) if isinstance(step.get("env"), dict) else set())
            if names & SHELL_ENV:
                problems.append(f"{sorted(names & SHELL_ENV)} in the environment can change what bash runs")
            shell = step.get("shell", default_shell)
            if shell is None:
                if WINDOWS in platforms:
                    problems.append("a Windows gate step must set `shell: bash` (the Windows default shell is PowerShell)")
            elif shell != "bash":
                problems.append(f"unsupported shell {shell!r} for the gate (only `bash`)")
            where = f"job {job_id} step {index} ({command.show()})"
            if problems:
                for p in problems:
                    self.fail(f"{where}: {p}")
                continue
            kind = "nightly lane" if nightly_only else "full gate"
            self.note(f"{where}: {kind} on {', '.join(sorted(platforms))}")
            if nightly_only:
                lane = True
            else:
                effective |= platforms
        return effective, lane

    def cargo(self, job_id, index, command):
        """No step runs cargo itself: the gates come from scripts/verify.sh."""
        words = [w.text for w in command.words]
        for k, word in enumerate(words):
            if word.replace("\\", "/").rsplit("/", 1)[-1] in ("cargo", "cargo.exe"):
                rest = words[k + 1:]
                if rest[:1] and rest[0].startswith("+"):
                    rest = rest[1:]
                if rest[:1] and rest[0] in CARGO_GATES:
                    self.fail(f"job {job_id} step {index}: a cargo command outside scripts/verify.sh: {command.show()}")
                    return


def check_workflow_file(path, name):
    with open(path, encoding="utf-8") as f:
        text = f.read()
    guard = Guard(name)
    guard.check(text)
    for note in guard.notes:
        print(f"note  {note}")
    return guard.problems


def workflow():
    problems = check_workflow_file(os.path.join(ROOT, ".github", "workflows", "ci.yml"), "ci.yml")
    # GNU tar takes an archive name with a colon before its first slash (a Windows `D:\\...` path)
    # for a remote host, so the scripts give tar its archive on standard input (`-f -`).
    for path in living_files("*.sh"):
        with open(path, encoding="utf-8") as f:
            for n, line in enumerate(f, 1):
                code = line.split("#", 1)[0]
                for m in re.finditer(r"\btar\s+-[A-Za-z]*f\s+(\S+)", code):
                    if m.group(1) != "-":
                        problems.append(f"{os.path.relpath(path, ROOT)}:{n}: tar reads an archive by name; use -f - with a redirect")
    print("static inspection only: the CI job logs, not this check, show that the gate ran")
    return problems


def main():
    sys.stdout.reconfigure(errors="backslashreplace")  # a Windows console code page
    if sys.argv[1:] == ["--toolchain"]:
        problems = toolchain()
    elif sys.argv[1:] == ["--workflow"]:
        problems = workflow()
    elif len(sys.argv) == 3 and sys.argv[1] == "--workflow-file":
        problems = check_workflow_file(sys.argv[2], os.path.basename(sys.argv[2]))
    else:
        print(__doc__)
        return 2
    for p in problems:
        print(f"FAIL  {p}")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
