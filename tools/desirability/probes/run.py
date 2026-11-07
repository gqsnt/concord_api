"""I3 readers: one fresh headless session per SDK bank session (wire / rc), no tools, no repository.

    python3 -I tools/desirability/probes/run.py --bank DIR --provider claude|codex --out DIR
        [--sdk SHORT ..] [--session wire rc] [--model M] [--effort E] [--parallel 4]

Reads `<bank>/packed/<sdk>.<session>.md` (pack.py) and writes, per session,
`<out>/<provider>/<sdk>.<session>/`: `events.jsonl` (the CLI's event stream), `stderr.log`,
`last.txt` (the final message), `answer.json` (the parsed answer or null) and `meta.json`
(requested and observed model/effort, timing, tool calls, attempts). Each session runs in a new
empty directory under `<repo>/target/tmp/probe-run/`.

- claude: `claude -p --output-format stream-json --verbose --model M --tools "" --setting-sources ""
  --strict-mcp-config --safe-mode --no-session-persistence --system-prompt S`, prompt on stdin.
  `stream-json` (a superset of `json`: its last event is the `json` result) is used because the
  transcript is needed to check for tool calls. Observed model: `message.model` of the assistant
  events and the `modelUsage` keys of the result event.
- codex: `codex exec -m gpt-6.1-sol -c model_reasoning_effort="xhigh" -c daybreak=false -s read-only
  -C <empty dir> --skip-git-repo-check --json -o last.txt`, prompt on stdin. Observed model and
  effort: the rollout's `turn_context` (`~/.codex/sessions/**/<thread id>.jsonl`).

A session whose transcript shows a tool call is discarded (kept as `attempt1/`) and run again once;
a second tool call records it as failed. Run Python with `-I`.
"""
import argparse
import concurrent.futures as cf
import glob
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import common as C  # noqa: E402

SYSTEM = ("You answer questions about an HTTP client declaration using only the materials in the user "
          "message. You have no tools. Reply with exactly one JSON object and nothing else.")
CODEX_MODEL = "gpt-6.1-sol"
TIMEOUT = 45 * 60
TEXT_ITEM_TYPES = {"agent_message", "reasoning", "error"}


def now():
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())


def parse_answer(text):
    """(object, error): the answer JSON object from a final message."""
    if not text or not text.strip():
        return None, "empty answer"
    t = text.strip()
    fence = re.search(r"```(?:json)?\s*\n(.*?)\n```", t, re.S)
    cands = [t] + ([fence.group(1)] if fence else [])
    if "{" in t:
        cands.append(t[t.index("{"):t.rindex("}") + 1])
    for c in cands:
        try:
            obj = json.loads(c)
        except ValueError:
            continue
        if isinstance(obj, dict) and isinstance(obj.get("answers"), list):
            return obj, None
    return None, "malformed answer (no JSON object with an `answers` list)"


def run_claude(prompt, workdir, out, model, effort):
    cmd = ["claude", "-p", "--output-format", "stream-json", "--verbose", "--model", model,
           "--tools", "", "--setting-sources", "", "--strict-mcp-config", "--safe-mode",
           "--no-session-persistence", "--system-prompt", SYSTEM]
    if effort:
        cmd += ["--effort", effort]
    start = now()
    try:
        r = subprocess.run(cmd, input=prompt, cwd=workdir, capture_output=True, text=True, timeout=TIMEOUT)
        stdout, stderr, code = r.stdout, r.stderr, r.returncode
    except subprocess.TimeoutExpired as e:
        stdout, stderr, code = (e.stdout or b"").decode() if isinstance(e.stdout, bytes) else (e.stdout or ""), "timeout", -1
    end = now()
    open(os.path.join(out, "events.jsonl"), "w").write(stdout)
    open(os.path.join(out, "stderr.log"), "w").write(stderr or "")
    models, tools_calls, init, result = set(), [], {}, {}
    for line in stdout.splitlines():
        try:
            e = json.loads(line)
        except ValueError:
            continue
        if e.get("type") == "system" and e.get("subtype") == "init":
            init = {k: e.get(k) for k in ("model", "tools", "mcp_servers", "permissionMode", "cwd",
                                          "claude_code_version", "apiKeySource", "output_style")}
        elif e.get("type") == "assistant":
            msg = e.get("message", {})
            if msg.get("model"):
                models.add(msg["model"])
            for c in msg.get("content", []):
                if c.get("type") in ("tool_use", "server_tool_use", "mcp_tool_use"):
                    tools_calls.append(c.get("name"))
        elif e.get("type") == "result":
            result = e
    text = result.get("result") or ""
    open(os.path.join(out, "last.txt"), "w").write(text)
    usage = result.get("modelUsage") or {}
    observed = {"models": sorted(models | set(usage.keys())), "init_model": init.get("model"),
                "effort": "not visible", "tools_offered": init.get("tools"),
                "mcp_servers": init.get("mcp_servers"), "source": "stream-json assistant message.model, result.modelUsage, system/init"}
    return {"cmd": cmd, "start": start, "end": end, "exit": code, "text": text, "tool_calls": tools_calls,
            "observed": observed, "num_turns": result.get("num_turns"), "is_error": result.get("is_error"),
            "cost_usd": result.get("total_cost_usd"), "usage": result.get("usage"),
            "requested": {"provider": "anthropic/claude-code", "model": model, "effort": effort or "default (not passed)"}}


def run_codex(prompt, workdir, out, model, effort):
    last = os.path.join(out, "last.txt")
    cmd = ["codex", "exec", "-m", model, "-c", f'model_reasoning_effort="{effort}"', "-c", "daybreak=false",
           "-s", "read-only", "-C", workdir, "--skip-git-repo-check", "--json", "-o", last]
    start = now()
    try:
        r = subprocess.run(cmd, input=prompt, cwd=workdir, capture_output=True, text=True, timeout=TIMEOUT)
        stdout, stderr, code = r.stdout, r.stderr, r.returncode
    except subprocess.TimeoutExpired as e:
        stdout, stderr, code = (e.stdout or b"").decode() if isinstance(e.stdout, bytes) else (e.stdout or ""), "timeout", -1
    end = now()
    open(os.path.join(out, "events.jsonl"), "w").write(stdout)
    open(os.path.join(out, "stderr.log"), "w").write(stderr or "")
    tid, tool_calls, usage = None, [], None
    for line in stdout.splitlines():
        try:
            e = json.loads(line)
        except ValueError:
            continue
        if e.get("type") == "thread.started":
            tid = e.get("thread_id")
        elif e.get("type", "").startswith("item."):
            it = e.get("item", {})
            if it.get("type") not in TEXT_ITEM_TYPES:
                tool_calls.append(it.get("type"))
        elif e.get("type") == "turn.completed":
            usage = e.get("usage")
    observed = {"model": "not visible", "effort": "not visible", "sandbox": "not visible", "rollout": None,
                "source": "rollout turn_context"}
    if tid:
        hits = glob.glob(os.path.expanduser(f"~/.codex/sessions/*/*/*/*{tid}*.jsonl"))
        if hits:
            observed["rollout"] = hits[0]
            rollout_tools = set()
            for line in open(hits[0], encoding="utf-8"):
                try:
                    rec = json.loads(line)
                except ValueError:
                    continue
                if rec.get("type") == "turn_context" and observed["model"] == "not visible":
                    p = rec.get("payload", {})
                    sp = p.get("sandbox_policy")
                    cm = (p.get("collaboration_mode") or {}).get("settings") or {}
                    observed.update({"model": p.get("model"),
                                     "effort": p.get("effort") or cm.get("reasoning_effort") or "not visible",
                                     "sandbox": sp.get("type") if isinstance(sp, dict) else sp})
                pl = rec.get("payload", {})
                if rec.get("type") == "response_item" and pl.get("type") in ("function_call", "local_shell_call",
                                                                             "custom_tool_call", "web_search_call"):
                    rollout_tools.add(pl.get("type"))
                if rec.get("type") == "event_msg" and isinstance(pl.get("item"), dict) and \
                        pl["item"].get("type") in ("CommandExecution", "FileChange", "McpToolCall", "WebSearch"):
                    rollout_tools.add(pl["item"]["type"])
            tool_calls += sorted(rollout_tools)
    text = open(last).read() if os.path.isfile(last) else ""
    return {"cmd": cmd, "start": start, "end": end, "exit": code, "text": text, "tool_calls": tool_calls,
            "observed": observed, "thread_id": tid, "usage": usage,
            "requested": {"provider": "openai/codex", "model": model, "effort": effort, "sandbox": "read-only"}}


def one(job):
    provider, short, session, prompt_path, out_root, model, effort, repo = job
    prompt = open(prompt_path, encoding="utf-8").read()
    out = os.path.join(out_root, provider, f"{short}.{session}")
    if os.path.isdir(out):
        shutil.rmtree(out)
    os.makedirs(out)
    attempts = []
    res = None
    for attempt in (1, 2):
        workdir = os.path.join(repo, "target", "tmp", "probe-run", provider, f"{short}.{session}.{attempt}.{os.getpid()}")
        if os.path.isdir(workdir):
            shutil.rmtree(workdir)
        os.makedirs(workdir)
        fn = run_claude if provider == "claude" else run_codex
        res = fn(prompt, workdir, out, model, effort)
        leftovers = os.listdir(workdir)
        shutil.rmtree(workdir, ignore_errors=True)
        attempts.append({"attempt": attempt, "start": res["start"], "end": res["end"], "exit": res["exit"],
                         "tool_calls": res["tool_calls"], "workdir_leftovers": leftovers})
        if not res["tool_calls"]:
            break
        if attempt == 1:
            keep = os.path.join(out, "attempt1")
            os.makedirs(keep)
            for f in ("events.jsonl", "stderr.log", "last.txt"):
                if os.path.isfile(os.path.join(out, f)):
                    shutil.move(os.path.join(out, f), os.path.join(keep, f))
    failed = bool(res["tool_calls"])
    answer, err = (None, "session used tools twice") if failed else parse_answer(res["text"])
    C.write_json(os.path.join(out, "answer.json"), answer)
    meta = {"sdk": short, "session": session, "provider": provider,
            "prompt": os.path.relpath(prompt_path, repo) if prompt_path.startswith(repo) else prompt_path,
            "prompt_sha256": hashlib.sha256(prompt.encode()).hexdigest(), "prompt_chars": len(prompt),
            "requested": res["requested"], "observed": res["observed"], "attempts": attempts,
            "failed": failed, "answer_error": err, "answers": len(answer["answers"]) if answer else 0,
            "exit": res["exit"], "usage": res.get("usage"), "cost_usd": res.get("cost_usd"),
            "num_turns": res.get("num_turns"), "thread_id": res.get("thread_id"),
            "cmd": [c if c != prompt else "<prompt>" for c in res["cmd"]]}
    C.write_json(os.path.join(out, "meta.json"), meta)
    return meta


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--bank", required=True)
    ap.add_argument("--provider", required=True, choices=("claude", "codex"))
    ap.add_argument("--out", required=True)
    ap.add_argument("--sdk", nargs="*")
    ap.add_argument("--session", nargs="*", default=["wire", "rc"])
    ap.add_argument("--model", help="claude: a model alias or full name (required); codex: default gpt-6.1-sol")
    ap.add_argument("--effort", help="codex: default xhigh; claude: not passed unless given")
    ap.add_argument("--parallel", type=int, default=4)
    ap.add_argument("--repo", default=C.REPO)
    a = ap.parse_args()
    bank_dir = os.path.abspath(a.bank)
    packed = os.path.join(bank_dir, "packed")
    if not os.path.isdir(packed):
        sys.exit(f"{packed} is missing: run pack.py --bank {a.bank} first")
    if a.provider == "claude" and not a.model:
        sys.exit("--model is required for claude (record what you request)")
    model = a.model or CODEX_MODEL
    effort = a.effort or ("xhigh" if a.provider == "codex" else None)
    shorts = sorted(f.split(".")[0] for f in os.listdir(packed) if f.endswith(".meta.json"))
    if a.sdk:
        shorts = [s for s in shorts if s in a.sdk]
    jobs = []
    for s in shorts:
        for session in a.session:
            p = os.path.join(packed, f"{s}.{session}.md")
            if os.path.isfile(p):
                jobs.append((a.provider, s, session, p, os.path.abspath(a.out), model, effort, os.path.abspath(a.repo)))
    with cf.ThreadPoolExecutor(max_workers=min(4, max(1, a.parallel))) as ex:
        for meta in ex.map(one, jobs):
            obs = meta["observed"]
            print(json.dumps({"sdk": meta["sdk"], "session": meta["session"], "provider": meta["provider"],
                              "requested_model": meta["requested"]["model"],
                              "observed": obs.get("models") or obs.get("model"), "effort": obs.get("effort"),
                              "failed": meta["failed"], "answers": meta["answers"],
                              "answer_error": meta["answer_error"], "attempts": len(meta["attempts"])}))


if __name__ == "__main__":
    main()
