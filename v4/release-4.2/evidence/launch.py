#!/usr/bin/env python3
"""Launch one bounded side session (Codex Sol/Terra or Claude Opus) and record what it observably ran.

Usage: launch.py NAME FAMILY EFFORT WORKDIR PROMPT_FILE [--timeout SECONDS] [--extra-dir DIR]...
FAMILY is sol, terra or opus. Output goes to /home/canarit/projects/concord-v42/lanes/NAME/.
Mirrors the 4.1 review dispatcher's invocation; never adds a bypass/yolo flag.
"""
from __future__ import annotations

import argparse
import datetime
import hashlib
import json
import os
import re
import signal
import subprocess
from pathlib import Path

CAMPAIGN = Path("/home/canarit/projects/concord-v42")


def now() -> str:
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("name")
    p.add_argument("family", choices=["sol", "terra", "opus"])
    p.add_argument("effort")
    p.add_argument("workdir", type=Path)
    p.add_argument("prompt", type=Path)
    p.add_argument("--timeout", type=int, default=4 * 3600)
    p.add_argument("--extra-dir", action="append", default=[])
    a = p.parse_args()
    out = CAMPAIGN / "lanes" / a.name
    out.mkdir(parents=True, exist_ok=False)
    prompt = a.prompt.read_text()
    (out / "prompt.md").write_text(prompt)
    dirs = [str(a.workdir), str(CAMPAIGN)] + a.extra_dir
    if a.family == "opus":
        argv = ["claude", "--safe-mode", "--print", "--model", "opus", "--effort", a.effort,
                "--output-format", "stream-json", "--verbose", "--permission-mode", "dontAsk",
                "--allowedTools", "Read,Write,Edit,Bash,Glob,Grep",
                "--tools", "Read,Write,Edit,Bash,Glob,Grep", "--strict-mcp-config",
                "--no-session-persistence"]
        for d in dirs:
            argv += ["--add-dir", d]
    else:
        argv = ["codex", "exec", "--ignore-user-config", "--skip-git-repo-check",
                "--disable", "multi_agent", "-m", "gpt-6.1-" + a.family,
                "-c", f'model_reasoning_effort="{a.effort}"',
                "-c", "sandbox_workspace_write.network_access=true",
                "-s", "workspace-write", "-C", str(a.workdir), "--color", "never",
                "-o", str(out / "final.txt")]
        for d in dirs[1:] + [str(Path.home() / ".cargo")]:
            argv += ["--add-dir", d]
        argv.append("-")
    info = {"name": a.name, "family": a.family, "requested_effort": a.effort,
            "requested_model": "opus" if a.family == "opus" else "gpt-6.1-" + a.family,
            "workdir": str(a.workdir), "argv": argv, "started_at": now(),
            "prompt_sha256": hashlib.sha256(prompt.encode()).hexdigest()}
    (out / "launch.json").write_text(json.dumps(info, indent=2) + "\n")
    env = dict(os.environ, CARGO_TERM_COLOR="never", PYTHONDONTWRITEBYTECODE="1")
    with (out / "stdout.log").open("w") as so, (out / "stderr.log").open("w") as se:
        proc = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=so, stderr=se,
                                cwd=a.workdir, start_new_session=True, text=True, env=env)
        info["pid"] = proc.pid
        try:
            proc.communicate(prompt, timeout=a.timeout)
            code = proc.returncode
        except subprocess.TimeoutExpired:
            os.killpg(proc.pid, signal.SIGTERM)
            try:
                proc.wait(timeout=20)
            except subprocess.TimeoutExpired:
                os.killpg(proc.pid, signal.SIGKILL)
                proc.wait()
            code = 124
    stdout = (out / "stdout.log").read_text(errors="replace")
    stderr = (out / "stderr.log").read_text(errors="replace")
    model = effort = session = None
    if a.family == "opus":
        for line in stdout.splitlines():
            try:
                obj = json.loads(line)
            except ValueError:
                continue
            if obj.get("type") == "system" and obj.get("subtype") == "init":
                model, session, effort = obj.get("model"), obj.get("session_id"), obj.get("effort")
            if obj.get("type") == "result":
                (out / "final.txt").write_text(str(obj.get("result", "")))
                usage = list(obj.get("modelUsage", {}))
                info["model_usage"] = usage
    else:
        for key, pat in (("model", r"^model: (.+)$"), ("effort", r"^reasoning effort: (.+)$"),
                         ("session", r"^session id: (.+)$")):
            m = re.search(pat, stderr, re.M)
            if key == "model":
                model = m.group(1) if m else None
            elif key == "effort":
                effort = m.group(1) if m else None
            else:
                session = m.group(1) if m else None
    info.update(finished_at=now(), exit=code, observed_model=model, observed_effort=effort,
                session=session)
    (out / "launch.json").write_text(json.dumps(info, indent=2) + "\n")
    print(json.dumps({k: info[k] for k in ("name", "exit", "observed_model", "observed_effort")}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
