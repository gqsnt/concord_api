#!/usr/bin/env python3
"""Run the release checks in order, stopping at the first failure."""

import argparse
from pathlib import Path
import subprocess

API = Path(__file__).resolve().parents[1]
STEPS = ("gate", "panics", "package", "features-all", "mutants")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--with-sockets", action="store_true", help="include the socket mutant")
    args = parser.parse_args(argv)
    for step in STEPS:
        command = ["just", step]
        if step == "mutants":
            command += ["--jobs", "1"]
            if args.with_sockets:
                command.append("--with-sockets")
        detail = ""
        try:
            code = subprocess.run(command, cwd=API, check=False).returncode
        except OSError as error:
            detail = f" ({error})"
            code = 1
        except KeyboardInterrupt:
            code = 130
        print(f"RELEASE {step}: {'FAILED' if code else 'passed'}{detail}", flush=True)
        if code:
            print(f"RELEASE: FAILED at {step}", flush=True)
            return code if code > 0 else 1
    print("RELEASE: passed", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
