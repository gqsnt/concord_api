"""Check error codes and primary source positions against the stable fixtures."""

import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

MACROS = Path(__file__).resolve().parents[1]
ROOT = MACROS.parent
ERROR = re.compile(r"^error(?:\[(E\d+)\])?:", re.MULTILINE)


def expected(fixture):
    snapshot = fixture.with_suffix(".stderr").read_text(encoding="utf-8")
    headers = list(ERROR.finditer(snapshot))
    location = re.compile(r"^\s*--> " + re.escape(fixture.relative_to(MACROS).as_posix())
                          + r":(\d+):(\d+)$", re.MULTILINE)
    positions = set()
    for header, end in zip(headers, [*(h.start() for h in headers[1:]), len(snapshot)]):
        span = location.search(snapshot, header.end(), end)
        if span is None:
            raise ValueError(f"{fixture.name}: missing source location in the snapshot")
        positions.add((header[1] or "", int(span[1]), int(span[2])))
    if not positions:
        raise ValueError(f"{fixture.name}: no errors in the snapshot")
    return positions


def run(directory, fixtures):
    manifest = [
        '[package]\nname = "diagnostic-spans"\nversion = "0.0.0"\nedition = "2021"',
        '[workspace]',
        '[dependencies]',
        f'concord = {{ package = "concord-api", path = {json.dumps(str(ROOT / "concord-api"))}, features = ["multipart"] }}',
        'serde = { version = "1", features = ["derive"] }',
        f'type-macro = {{ path = {json.dumps(str(ROOT / "tests/type-macro"))} }}',
    ]
    for name, fixture in fixtures.items():
        manifest.append(f'[[bin]]\nname = "{name}"\npath = {json.dumps(str(fixture))}')
    (directory / "Cargo.toml").write_text("\n".join(manifest) + "\n", encoding="utf-8")
    shutil.copyfile(ROOT / "Cargo.lock", directory / "Cargo.lock")
    return subprocess.run(
        [os.environ.get("CARGO", "cargo"), "check", "--offline", "--keep-going",
         "--bins", "--message-format=json"],
        cwd=directory, capture_output=True, text=True,
    )


def main(group):
    fixtures = {
        f"fixture_{index}": fixture
        for index, fixture in enumerate(sorted((MACROS / "tests/compiler-errors" / group).glob("*.rs")))
    }
    if not fixtures:
        raise ValueError(f"{group}: no compiler fixtures")
    scratch = ROOT / "target/msrv/diagnostic-spans"
    scratch.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=group + "-", dir=scratch) as directory:
        result = run(Path(directory), fixtures)
    errors = {name: set() for name in fixtures}
    for line in result.stdout.splitlines():
        event = json.loads(line)
        if event["reason"] != "compiler-message":
            continue
        name, diagnostic = event["target"]["name"], event["message"]
        if name not in fixtures or diagnostic["level"] != "error":
            continue
        code = (diagnostic["code"] or {}).get("code", "")
        for span in diagnostic["spans"]:
            if span["is_primary"] and Path(span["file_name"]) == fixtures[name]:
                start = (span["line_start"], span["column_start"])
                end = (span["line_end"], span["column_end"])
                errors[name].add((code if code.startswith("E") else "", start, end))
    failed = result.returncode != 101
    for name, fixture in fixtures.items():
        # One compiler may underline `api::Get` where another underlines just `Get`.
        missing = {
            (code, line, column) for code, line, column in expected(fixture)
            if not any(actual == code and (start == (line, column) or start < (line, column) < end)
                       for actual, start, end in errors[name])
        }
        if missing:
            print(f"{fixture.name}: missing {sorted(missing)}, got {sorted(errors[name])}")
            failed = True
    if failed:
        print(result.stderr, file=sys.stderr)
    return int(failed)


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1]))
