#!/usr/bin/env python3
"""Run portable-case recipes sequentially and write one receipt per step.

Usage: replay.py ROOT CASES_JSON OUT_DIR [--only F01,F02] [--timeout 600] [--expect FILE]

ROOT holds consumer-N/ and source/ (the layout the recipes' cwd values use). Each step's receipt
records the command, cwd, expected/actual exit, every required witness found or missing, the lock
and a source-closure digest (consumer inputs + the path-dependency crates under ROOT/source/crates).
--expect replaces each step's expected_exit/required_output (repaired-state expectations), keyed by
"<case>#<index>"; the historical cases.json is never modified.
"""
from __future__ import annotations

import argparse
import datetime
import hashlib
import json
import subprocess
import sys
from pathlib import Path

SKIP = {"target", ".git"}


def files_under(root: Path):
    for p in sorted(root.rglob("*")):
        if p.is_file() and not (SKIP & set(p.relative_to(root).parts)):
            yield p


def tree_digest(root: Path) -> tuple[str, int]:
    h = hashlib.sha256()
    n = 0
    for p in files_under(root):
        h.update(p.relative_to(root).as_posix().encode() + b"\0")
        h.update(hashlib.sha256(p.read_bytes()).hexdigest().encode() + b"\n")
        n += 1
    return h.hexdigest(), n


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("root", type=Path)
    ap.add_argument("cases", type=Path)
    ap.add_argument("out", type=Path)
    ap.add_argument("--only", default="")
    ap.add_argument("--timeout", type=int, default=600)
    ap.add_argument("--expect", type=Path)
    ap.add_argument("--heavy", default="/home/canarit/projects/concord-v42/tools/heavy.sh")
    a = ap.parse_args()
    cases = json.loads(a.cases.read_text())
    expect = json.loads(a.expect.read_text()) if a.expect else {}
    only = set(filter(None, a.only.split(",")))
    a.out.mkdir(parents=True, exist_ok=True)
    rustc = subprocess.run(["rustc", "+1.99.1", "-vV"], capture_output=True, text=True).stdout
    crates_digest, crates_n = tree_digest(a.root / "source" / "crates")
    summary = []
    for case, steps in cases.items():
        if only and case not in only:
            continue
        for i, step in enumerate(steps):
            key = f"{case}#{i}"
            want = expect.get(key, {})
            exp_exit = want.get("expected_exit", step["expected_exit"])
            witnesses = want.get("required_output", step["required_output"])
            forbidden = want.get("forbidden_output", step.get("forbidden_output", []))
            cwd = a.root / step["cwd"]
            started = datetime.datetime.now(datetime.timezone.utc).isoformat()
            try:
                r = subprocess.run([a.heavy, "timeout", str(a.timeout)] + step["command"],
                                   cwd=cwd, capture_output=True, text=True)
                code, output = r.returncode, r.stdout + r.stderr
            except OSError as e:
                code, output = 127, str(e)
            log = a.out / f"{key.replace('#', '-')}.log"
            log.write_text(output)
            found = {w: (w in output) for w in witnesses}
            absent = {w: (w not in output) for w in forbidden}
            consumer_digest, consumer_n = tree_digest(cwd)
            lock = cwd / "Cargo.lock"
            receipt = {
                "step": key, "command": step["command"], "cwd": str(cwd), "started_at": started,
                "expected_exit": exp_exit, "actual_exit": code, "witnesses": found,
                "forbidden_absent": absent,
                "match": code == exp_exit and all(found.values()) and all(absent.values()),
                "original_receipt": step.get("original_receipt"),
                "lock_sha256": hashlib.sha256(lock.read_bytes()).hexdigest() if lock.exists() else None,
                "consumer_inputs_digest": consumer_digest, "consumer_files": consumer_n,
                "path_dependency_crates_digest": crates_digest, "path_dependency_files": crates_n,
                "rustc": rustc.strip().splitlines(), "log": str(log),
                "log_sha256": hashlib.sha256(output.encode()).hexdigest(),
            }
            (a.out / f"{key.replace('#', '-')}.json").write_text(json.dumps(receipt, indent=2) + "\n")
            summary.append({k: receipt[k] for k in ("step", "expected_exit", "actual_exit", "witnesses", "forbidden_absent", "match")})
            print(json.dumps(summary[-1]), flush=True)
    (a.out / "SUMMARY.json").write_text(json.dumps(summary, indent=2) + "\n")
    return 0 if all(s["match"] for s in summary) else 1


if __name__ == "__main__":
    sys.exit(main())
