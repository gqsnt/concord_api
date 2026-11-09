#!/usr/bin/env python3
"""Build one consumer off/on, guarding the logging-only footprint without feature unification.

Summary, query_pairs and query redaction are deliberately available in both builds. Inspect
unoptimized linked code with debug information disabled so source/debug strings cannot supply
false positives. Cargo's JSON artifacts identify the exact binary and core archive for each
selection; no glob can accidentally inspect an artifact from the other build.
"""

import json
import os
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parent.parent
PACKAGE = "concord-check-tracing-footprint"
ROUTE = b"C9ROUTEBASE/C9ROUTESEGMENT/{id}"
# Both legacy and v0 Rust mangling keep these adjacent components in the archive's symbols.
CALL = re.compile(rb"concord_core.{0,80}3log.{0,20}4Call")


def object_code(archive):
    """Read GNU/BSD ar members, excluding Rust metadata (which also contains rustdoc text)."""
    assert archive.startswith(b"!<arch>\n"), "expected an rlib archive"
    offset, names, objects = 8, b"", []
    while offset < len(archive):
        header = archive[offset:offset + 60]
        assert header[58:60] == b"`\n", "invalid ar member"
        size = int(header[48:58])
        data = archive[offset + 60:offset + 60 + size]
        name = header[:16].strip()
        if name == b"//":
            names = data
        elif name.startswith(b"#1/"):
            length = int(name[3:])
            name, data = data[:length].rstrip(b"\0"), data[length:]
        elif name.startswith(b"/") and name[1:].isdigit():
            name = names[int(name[1:]):].split(b"/\n", 1)[0].split(b"\0", 1)[0]
        name = name.rstrip(b"/")
        if name.endswith((b".o", b".obj")):
            objects.append(data)
        offset += 60 + size + size % 2
    assert objects, "core archive has no object code"
    return b"".join(objects)


def check(enabled):
    env = dict(os.environ, CARGO_PROFILE_DEV_DEBUG="0", CARGO_PROFILE_DEV_OPT_LEVEL="0",
               CARGO_PROFILE_DEV_STRIP="none", CARGO_INCREMENTAL="0")
    command = ["cargo", "build", "--locked", "--offline", "-p", PACKAGE,
               "--message-format=json"]
    if enabled:
        command += ["--features", "tracing"]
    built = subprocess.run(command, cwd=ROOT, env=env, capture_output=True, text=True)
    if built.returncode:
        raise RuntimeError(built.stderr + built.stdout)
    binary = archive = None
    for line in built.stdout.splitlines():
        item = json.loads(line)
        if item.get("reason") != "compiler-artifact":
            continue
        if item["target"]["name"] == PACKAGE:
            binary = Path(item["executable"])
        if item["target"]["name"] == "concord_core":
            archive = next(Path(p) for p in item["filenames"] if p.endswith(".rlib"))
    assert binary and archive, "Cargo did not report the fixture and concord-core artifacts"
    # The executable asserts Route is zero-sized off and a string reference on. It creates an
    # execution future but never polls it, so this guard makes no network requests.
    subprocess.run([str(binary)], check=True)
    linked, core = binary.read_bytes(), object_code(archive.read_bytes())
    assert (ROUTE in linked) == enabled, f"route literal: tracing={enabled}"
    assert (b"concord.call" in core) == enabled, f"event callsites: tracing={enabled}"
    assert bool(CALL.search(core)) == enabled, f"log::Call code: tracing={enabled}"
    print(f"PASS tracing={'on' if enabled else 'off'}: Route representation, route literal, "
          "event callsites, log::Call code")


if __name__ == "__main__":
    check(False)
    check(True)
