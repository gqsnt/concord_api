#!/usr/bin/env python3
"""Run opt-in Criterion benches and save a paired time/allocation table.

The bench package owns its std-only counting global allocator. Fixture setup,
client construction, Criterion and output destruction are excluded; complete
operations (including Script's identical request recording) are counted. Counts
include realloc as an allocation event. The runtime is single-threaded/paused.
"""

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys

REPO_ROOT = Path(__file__).resolve().parents[1]
TARGET = REPO_ROOT / "target"


def table(log):
    counts = dict((name, int(count)) for name, count in re.findall(r"^ALLOC (\S+) median=(\d+)", log, re.MULTILINE))
    rows = []
    for name in counts:
        estimates = TARGET / "criterion" / name.replace("/", "_") / "new/estimates.json"
        if estimates.is_file():
            stats = json.loads(estimates.read_text())
            estimate = stats.get("slope") or stats["mean"]
            rows.append((name, estimate["point_estimate"] / 1000, counts[name]))
    lookup = {name: (time, count) for name, time, count in rows}
    result = ["operation | Concord us / allocations | reqwest us / allocations | added us / allocations",
              "--- | ---: | ---: | ---:"]
    for name, time, count in rows:
        if not name.endswith("/concord"):
            continue
        baseline = lookup.get(name.removesuffix("/concord") + "/reqwest")
        if baseline:
            raw_time, raw_count = baseline
            result.append(f"{name.removesuffix('/concord')} | {time:.3f} / {count} | {raw_time:.3f} / {raw_count} | {time - raw_time:+.3f} / {count - raw_count:+}")
    if "get/build/concord" in lookup and "get/build/reqwest" in lookup:
        time, count = lookup["get/build/concord"]
        raw_time, raw_count = lookup["get/build/reqwest"]
        result.append(f"GET budget (<2 us, <25 allocations added): {'PASS' if time - raw_time < 2 and count - raw_count < 25 else 'FAIL'}")
    return "\n".join(result) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report-only", type=Path, help="read allocations from an existing run log")
    parser.add_argument("--expansion", nargs="?", type=Path,
                        const=Path("tests/tour-gen/src/tasks.rs"),
                        help="measure macro expansion of a Rust file (default: the Tasks client)")
    args, extra = parser.parse_known_args()
    if args.expansion is not None:
        if args.report_only or extra:
            parser.error("--expansion accepts only its optional input file")
        source = (REPO_ROOT / args.expansion).resolve()
        if not source.is_file():
            parser.error(f"no description file: {source}")
        TARGET.mkdir(exist_ok=True)
        output = TARGET / "expansion-measurement.json"
        output.unlink(missing_ok=True)
        env = dict(os.environ, CARGO_NET_OFFLINE="true", CARGO_BUILD_JOBS="4",
                   CARGO_TARGET_DIR=str(TARGET), CONCORD_MEASURE_INPUT=str(source),
                   CONCORD_MEASURE_OUTPUT=str(output))
        command = ["cargo", "test", "--offline", "-p", "concord-macros", "--lib",
                   "write_tests::c1_16_measure_macro_expansion_once", "--",
                   "--ignored", "--exact", "--test-threads=1"]
        code = subprocess.run(command, cwd=REPO_ROOT, env=env).returncode
        if code:
            return code
        print(output.read_text(), end="")
        print("Expansion report: target/expansion-measurement.json")
        return 0
    if args.report_only:
        output = table(args.report_only.read_text())
    else:
        env = os.environ.copy()
        env.update(CARGO_NET_OFFLINE="true", CARGO_BUILD_JOBS="4", CARGO_TARGET_DIR=str(TARGET))
        command = ["cargo", "bench", "--offline", "--manifest-path", "tools/measure/Cargo.toml", *extra]
        TARGET.mkdir(exist_ok=True)
        (TARGET / "measurements.txt").unlink(missing_ok=True)
        with (TARGET / "bench.log").open("w") as log:
            process = subprocess.Popen(command, cwd=REPO_ROOT, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
            for line in process.stdout:
                print(line, end="", flush=True)
                log.write(line)
            code = process.wait()
        if code:
            return code
        output = table((TARGET / "bench.log").read_text())
    (TARGET / "measurements.txt").write_text(output)
    print(output, end="")
    return 0


if __name__ == "__main__":
    sys.exit(main())
