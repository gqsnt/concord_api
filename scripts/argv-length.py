"""The length Windows would give a command line, for scripts/argv-guard.sh.

Reads the program and its arguments from standard input, each ending with a NUL byte (never from
the command line, which is what it measures), joins them with the MSVC quoting rules that Windows
programs and Python's subprocess use (`subprocess.list2cmdline`), and prints the length in UTF-16
code units including the terminating NUL: the unit of CreateProcess' 32,767 limit.

Usage: printf '%s\\0' PROGRAM ARG... | python3 -I scripts/argv-length.py
       python3 -I scripts/argv-length.py --self-test
"""

import subprocess
import sys

LIMIT = 32767


def length(words):
    line = subprocess.list2cmdline(words)
    return len(line.encode("utf-16-le", "surrogatepass")) // 2 + 1


def self_test():
    # (argument, exact length of `tool ARG` + NUL, refused?)
    cases = [
        ("x" * 40000, 4 + 1 + 40000 + 1, True),
        ("x" * 32000, 4 + 1 + 32000 + 1, False),
        ("\\" * 17000, 4 + 1 + 17000 + 1, False),  # backslashes double only before a quote
        ("\U0001f600" * 17000, 4 + 1 + 34000 + 1, True),  # two UTF-16 units each
        ("\U0001f600" * 8000, 4 + 1 + 16000 + 1, False),
        ('"' * 16500, 4 + 1 + 33000 + 1, True),  # each quote is escaped
        ("a b", 4 + 1 + 5 + 1, False),  # quoted: contains a space
    ]
    failed = 0
    for arg, expected, refused in cases:
        got = length(["tool", arg])
        if got != expected or (got > LIMIT) != refused:
            print(f"FAIL  {arg[:8]!r}... ({len(arg)} characters): {got}, expected {expected}")
            failed += 1
    print(f"{len(cases) - failed}/{len(cases)} argv-length cases")
    return 1 if failed else 0


def main():
    if sys.argv[1:] == ["--self-test"]:
        return self_test()
    data = sys.stdin.buffer.read()
    words = [w.decode("utf-8", "surrogateescape") for w in data.split(b"\0")[:-1]]
    print(length(words))
    return 0


if __name__ == "__main__":
    sys.exit(main())
