#!/usr/bin/env python3
"""Re-run tests excluding only observed sandbox socket denials.

First run cargo test --offline --workspace --features concord/tower,concord/multipart
--no-fail-fast, saving api/target/m5-tests.log. This tool records exact excluded
names and re-runs all other tests, with nextest's per-test process isolation.
It does not alter tests, their expectations, or the ordinary gate.
"""

import os
from pathlib import Path
import re
import subprocess
import sys

API = Path(__file__).resolve().parents[1]
TARGET = API / "target"


def main():
    blocks = re.split(r"^---- (.*?) stdout ----\n", (TARGET / "m5-tests.log").read_text(), flags=re.MULTILINE)
    sockets = []
    for at in range(1, len(blocks), 2):
        name, body = blocks[at:at + 2]
        if "Operation not permitted" in body or "PermissionDenied" in body:
            sockets.append(name)
    if not sockets:
        raise SystemExit("No observed socket denials; use the ordinary test recipe.")
    sockets = sorted(set(sockets))
    (TARGET / "m5-socket-skips.txt").write_text("\n".join(sockets) + "\n")
    print(f"Excluding {len(sockets)} observed socket-denied test names; see target/m5-socket-skips.txt", flush=True)
    runtime = [name for name in sockets if "(line " not in name]
    docs = [name for name in sockets if "(line " in name]
    expression = "not (" + " or ".join(f"test(={name})" for name in runtime) + ")"
    env = os.environ.copy()
    env.update(CARGO_NET_OFFLINE="true", CARGO_BUILD_JOBS="4", CARGO_TARGET_DIR=str(TARGET))
    command = ["cargo", "nextest", "run", "--offline", "--workspace", "--features", "concord/tower,concord/multipart", "--no-fail-fast", "-E", expression]
    code = subprocess.run(command, cwd=API, env=env).returncode
    doctests = ["cargo", "test", "--offline", "--workspace", "--doc", "--features", "concord/tower,concord/multipart", "--"]
    # Cargo/rustdoc split test arguments on spaces. A full display name such as
    # "... (line 10)" would accidentally filter the other doctest in that file.
    # Exclude the file from the workspace run and run its safe example separately.
    for name in docs:
        doctests += ["--skip", name.split(" - (line ", 1)[0]]
    code = subprocess.run(doctests, cwd=API, env=env).returncode or code
    for name in docs:
        path, line = name.split(" - (line ", 1)
        crate = path.split("/", 1)[0]
        command = ["cargo", "test", "--offline", "-p", crate, "--doc", "--", "--skip", line]
        code = subprocess.run(command, cwd=API, env=env).returncode or code
    return code


if __name__ == "__main__":
    sys.exit(main())
