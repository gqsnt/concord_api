"""Explicit local-socket test selectors shared by validation tools."""
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

def cases():
    return [tuple(line.split()) for line in (ROOT / "tools/socket-tests.txt").read_text().splitlines()
            if line.strip() and not line.startswith("#")]

def filterset():
    selected = []
    for binary, name in cases():
        package, _, target = binary.partition("::")
        selector = f"package(={package}) and test(={name})"
        if target:
            selector += f" and binary(={target})"
        selected.append(f"({selector})")
    return "not (" + " or ".join(selected) + ")"

def socket_doc_lines():
    source = (ROOT / "concord-testkit/src/lib.rs").read_text().splitlines()
    start, block, result = None, [], []
    for number, line in enumerate(source, 1):
        text = line.removeprefix("//! ")
        if text.startswith("```"):
            if start is None:
                start, block = number, []
            else:
                if any("Server::start()" in item for item in block):
                    result.append(start)
                start = None
        elif start is not None:
            block.append(text)
    return result
