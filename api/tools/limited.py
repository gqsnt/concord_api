#!/usr/bin/env python3
"""Sandbox fallback when build/limited.sh cannot access the systemd user bus.

RLIMIT_AS bounds each process (not aggregate RSS like the preferred systemd
scope). Cargo remains sequential and uses at most four jobs. Artifacts stay in
api/target. Use build/limited.sh when its user bus is available.
"""

import os
from pathlib import Path
import resource
import sys

API = Path(__file__).resolve().parents[1]
ceiling = 8 * 1024**3
resource.setrlimit(resource.RLIMIT_AS, (ceiling, ceiling))
os.environ["CARGO_BUILD_JOBS"] = "4"
os.environ["CARGO_NET_OFFLINE"] = "true"
os.environ.setdefault("CARGO_TARGET_DIR", str(API / "target"))
os.execvp(sys.argv[1], sys.argv[1:])
