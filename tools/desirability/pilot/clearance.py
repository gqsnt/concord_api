"""Candidate clearance: the lane-transcript audit and the candidate-diff check, with a report.

    python3 -I tools/desirability/pilot/clearance.py --out DIR \
        [--transcript FILE ..] [--brief-file FILE ..] [--forbid REGEX ..] \
        [--repo PATH --base SHA --candidate SHA] [--sealed DIR ..]

Generalises `<WS>/work/tools/audit_transcript.py` (the check used for the H03/H04/H08 candidates).

**Transcript audit** (Claude Code JSONL transcripts and Codex rollouts). Every occurrence of a
forbidden path (default `work/sealed`, `work/baseline`, `tools/desirability`, `sealed/` and the
bank directory names; `--forbid` adds patterns) is classified by where it occurs:

| class | meaning | effect |
|---|---|---|
| `access` | a tool input that reads, lists, searches or enters a forbidden path (a file tool's path, a search path or pattern, a shell command's word that is not an exclusion, a path-like field of another tool) | blocking |
| `access-result` | the result of such a tool call | blocking |
| `access-evidence` | a tool result that shows a path below a forbidden directory, not explained by brief text (e.g. a search whose exclusion filter did not apply) | review |
| `prose-mention` | a tool result that names a forbidden directory without any path below it (records and reports read in the lane) | none |
| `content-mention` | text written into a file (Write/Edit content) or passed to another tool (e.g. a sub-agent prompt) naming a forbidden path | review |
| `exclusion` | a shell or search exclusion (`-not -path`, `! -path`, `-prune`, `grep -v`, `--exclude`, `':!..'` pathspecs, `!` globs, `test ! -e`) | none |
| `brief-text` | the brief: a user or coordinator message, or a result of reading a brief file (under `work/roles/`, or `--brief-file`) | none |
| `brief-echo` | a tool result whose line holding the hit also occurs in brief text | none |
| `report-text` | the lane's own messages and reports (assistant text, SubagentHandback, SendMessage, plan updates, a shell command's description) | none |

Status: `refused` with any blocking hit, `review` with any review hit, else `clear`.

**Candidate-diff check** (`--base`, `--candidate`): every changed path under `tools/desirability/`
is blocking; added lines naming a forbidden path, and added lines (at least 24 characters after
whitespace normalisation) that occur in a sealed file (`--sealed`, default every bank and task
set under `<WS>/work/sealed/`) but nowhere in the base commit's tree, are review findings.

The report (`clearance.json`, `CLEARANCE.md` in `--out`) quotes at most 200 characters around each
hit. Run Python with `-I`.
"""
import argparse
import io
import json
import os
import re
import subprocess
import sys
import tarfile

HERE = os.path.dirname(os.path.abspath(__file__))
WS = "/home/canarit/projects/Concord_Desirability_Loop"
DEFAULT_FORBID = r"work/sealed|work/baseline|tools/desirability|\b(?:h\d+[a-z]?-(?:i4-)?|i4-)bank(?:-[A-Z])?(?![\w.-])|\bsealed/"
REPORT_TOOLS = re.compile(r"^(SubagentHandback|SendMessage|TodoWrite|mcp__ccd_turn__.*|update_plan|wrap_up)$")
FILE_TOOLS = ("Read", "Edit", "Write", "NotebookEdit", "MultiEdit")
SEARCH_TOOLS = ("Glob", "Grep", "LS")
# an exclusion operator immediately before the hit, the hit inside the operator's own argument word
EXCLUSION = re.compile(r"(?:-not\s+-(?:i?path|i?name|i?wholename)\s+|!\s+-(?:i?path|i?name)\s+|--exclude(?:-dir)?(?:=|\s+)|"
                       r"grep\s+(?:-[a-zA-Z]*v[a-zA-Z]*\s+|--invert-match\s+)(?:-[a-zA-Z]+\s+)*|:!|:\(exclude\)|"
                       r"(?:-g|--glob|--iglob)(?:=|\s+)[\"']?!|(?:\btest|\[)\s+!\s+-[edfsL]\s+)[^\s|;&]{0,200}$")
PRUNED = re.compile(r"^[^\s|;&]*\s+-prune\b")  # find .. -path <hit> -prune
SEVERITY = {"access": "blocking", "access-result": "blocking", "access-evidence": "review", "content-mention": "review",
            "exclusion": None, "prose-mention": None, "brief-text": None, "brief-echo": None, "report-text": None}


def as_text(v):
    if isinstance(v, str):
        return v
    if isinstance(v, list):
        return "\n".join(as_text(x.get("text", x.get("content", "")) if isinstance(x, dict) else x) for x in v)
    return json.dumps(v, ensure_ascii=False)


def events(path):
    """[(role, kind, name, text, call_id, input dict)] in transcript order, for both formats:
    role user|assistant|tool; kind message|tool_use|tool_result."""
    out = []
    for ln in open(path, encoding="utf-8", errors="replace"):
        try:
            o = json.loads(ln)
        except ValueError:
            continue
        if not isinstance(o, dict):
            continue
        p = o.get("payload") if isinstance(o.get("payload"), dict) else None
        if o.get("type") == "response_item" and p:  # Codex rollout
            t = p.get("type")
            if t == "message":
                out.append(("user" if p.get("role") in ("user", "developer") else "assistant", "message", None,
                            as_text(p.get("content")), None, None))
            elif t in ("function_call", "custom_tool_call", "local_shell_call"):
                raw = p.get("arguments") or p.get("input") or p.get("action") or ""
                inp = raw if isinstance(raw, dict) else {"command": raw if isinstance(raw, str) else json.dumps(raw)}
                out.append(("assistant", "tool_use", "Bash" if t != "function_call" or p.get("name") in ("shell", "exec_command") else p.get("name"),
                            as_text(raw), p.get("call_id"), inp))
            elif t in ("function_call_output", "custom_tool_call_output", "local_shell_call_output"):
                out.append(("tool", "tool_result", None, as_text(p.get("output")), p.get("call_id"), None))
            continue
        msg = o.get("message")
        if not isinstance(msg, dict):
            continue
        role, content = msg.get("role") or o.get("type"), msg.get("content")
        if isinstance(content, str):
            out.append((role, "message", None, content, None, None))
            continue
        for c in content or []:
            if not isinstance(c, dict):
                continue
            if c.get("type") == "text":
                out.append((role, "message", None, c.get("text", ""), None, None))
            elif c.get("type") == "tool_use":
                inp = c.get("input") if isinstance(c.get("input"), dict) else {"value": c.get("input")}
                out.append((role, "tool_use", c.get("name"), json.dumps(inp, ensure_ascii=False), c.get("id"), inp))
            elif c.get("type") == "tool_result":
                out.append(("tool", "tool_result", None, as_text(c.get("content")), c.get("tool_use_id"), None))
    return out


def use_hits(name, inp, forbid):
    """[(class, field text, match)] of the forbidden-path hits in one tool input, field by field:
    a shell command's words (exclusion forms recognised), a file tool's path, a search tool's
    path or pattern, the prose fields (a command's description, a report) as report text."""
    out = []
    shell = name in ("Bash", "exec", "shell", "exec_command", "local_shell")
    for field, v in (inp or {}).items():
        text = v if isinstance(v, str) else json.dumps(v, ensure_ascii=False)
        for m in forbid.finditer(text):
            if name and REPORT_TOOLS.match(name):
                cls = "report-text"
            elif shell:
                if field in ("command", "cmd", "value", "input"):
                    excluded = EXCLUSION.search(text[max(0, m.start() - 300):m.start()]) or PRUNED.match(text[m.end():m.end() + 300])
                    cls = "exclusion" if excluded else "access"
                else:
                    cls = "report-text"
            elif name in FILE_TOOLS:
                cls = "access" if field in ("file_path", "notebook_path", "path") else "content-mention"
            elif name in SEARCH_TOOLS:
                cls = ("exclusion" if field in ("pattern", "glob") and text[:m.start()].rstrip().endswith("!")
                       else "access" if field in ("path", "pattern", "glob") else "report-text")
            elif re.search(r"path|file|dir|cwd|url", field, re.I):
                cls = "access"
            else:
                cls = "content-mention"
            out.append((cls, text, m))
    return out


def norm_line(s):
    return re.sub(r"\s+", " ", s).strip()


def audit(path, forbid, brief_files=()):
    evs = events(path)
    brief_paths = [os.path.realpath(b) for b in brief_files]
    briefs, uses = [], {}
    for role, kind, name, text, cid, inp in evs:
        if kind == "message" and role == "user":
            briefs.append(text)
    hits = []

    def add(cls, i, name, text, m):
        hits.append({"event": i, "class": cls, "severity": SEVERITY[cls], "tool": name,
                     "quote": text[max(0, m.start() - 100):m.end() + 100].replace("\n", " ")})

    for i, (role, kind, name, text, cid, inp) in enumerate(evs):
        if kind == "tool_use":
            fpath = str((inp or {}).get("file_path") or "")
            is_brief = name == "Read" and (("/work/roles/" in fpath) or os.path.realpath(fpath) in brief_paths)
            classes = set()
            for cls, ftext, m in use_hits(name, inp, forbid):
                classes.add(cls)
                add(cls, i, name, ftext, m)
            uses[cid] = {"name": name, "brief": is_brief, "access": "access" in classes}
        elif kind == "tool_result":
            u = uses.get(cid, {})
            if u.get("brief"):
                briefs.append(text)
            for m in forbid.finditer(text):
                if u.get("brief"):
                    cls = "brief-text"
                elif u.get("access"):
                    cls = "access-result"
                else:
                    a, b = text.rfind("\n", 0, m.start()) + 1, text.find("\n", m.end())
                    line = norm_line(text[a:b if b >= 0 else len(text)])
                    below = re.match(r"/?[\w.-]", text[m.end():m.end() + 2]) and not text[m.end():m.end() + 1].isspace()
                    cls = ("brief-echo" if line and any(line in norm_line(bt) for bt in briefs)
                           else "access-evidence" if below else "prose-mention")
                add(cls, i, u.get("name"), text, m)
        else:
            for m in forbid.finditer(text):
                add("brief-text" if role == "user" else "report-text", i, None, text, m)
    counts = {}
    for h in hits:
        counts[h["class"]] = counts.get(h["class"], 0) + 1
    tool_uses = sum(1 for e in evs if e[1] == "tool_use")
    return {"transcript": path, "events": len(evs), "tool_uses": tool_uses, "counts": counts, "hits": hits,
            "status": status([h["severity"] for h in hits])}


def status(sevs):
    return "refused" if "blocking" in sevs else "review" if "review" in sevs else "clear"


# ---------------------------------------------------------------- candidate diff

def git(repo, *args, binary=False):
    r = subprocess.run(["git", "-C", repo] + list(args), capture_output=True)
    if r.returncode:
        raise SystemExit(f"git {' '.join(args)}: {r.stderr.decode(errors='replace')[:300]}")
    return r.stdout if binary else r.stdout.decode("utf-8", errors="replace")


def default_sealed():
    root = os.path.join(WS, "work", "sealed")
    out = []
    for d in sorted(os.listdir(root)) if os.path.isdir(root) else []:
        p = os.path.join(root, d)
        if os.path.isdir(p) and ("bank" in d or os.path.isdir(os.path.join(p, "tasks"))):
            out.append(p)
    return out


def text_lines(data):
    try:
        s = data.decode("utf-8")
    except UnicodeDecodeError:
        return []
    return [norm_line(x) for x in s.splitlines()]


def base_lines(repo, base):
    lines = set()
    with tarfile.open(fileobj=io.BytesIO(git(repo, "archive", base, binary=True))) as tf:
        for m in tf:
            if m.isfile() and m.size <= 2 << 20:
                lines.update(text_lines(tf.extractfile(m).read()))
    return lines


def sealed_index(dirs, exclude, min_len=24):
    idx = {}
    for d in dirs:
        for dp, dn, fn in os.walk(d):
            dn[:] = [x for x in dn if x not in ("target", "trees", "scratch", ".git")]
            for f in fn:
                p = os.path.join(dp, f)
                try:
                    if os.path.islink(p) or os.path.getsize(p) > 2 << 20:
                        continue
                    data = open(p, "rb").read()
                except OSError:
                    continue
                for ln in text_lines(data):
                    ln = ln.lstrip("+- ").strip() if ln.startswith(("+", "-")) else ln
                    if len(ln) >= min_len and ln not in exclude:
                        idx.setdefault(ln, p)
    return idx


def diff_check(repo, base, cand, sealed_dirs, forbid):
    names = git(repo, "diff", "--name-status", "--no-renames", base, cand).splitlines()
    findings = []
    for ln in names:
        st, _, path = ln.partition("\t")
        if path.startswith("tools/desirability/"):
            findings.append({"severity": "blocking", "class": "instrument-path", "path": path, "status": st})
    patch = git(repo, "diff", "-U0", "--no-color", "--no-ext-diff", base, cand)
    added, cur = [], None
    for ln in patch.splitlines():
        if ln.startswith("+++ "):
            cur = ln[6:] if ln.startswith("+++ b/") else ln[4:]
        elif ln.startswith("+") and not ln.startswith("+++"):
            added.append((cur, ln[1:]))
    for path, ln in added:
        m = forbid.search(ln)
        if m:
            findings.append({"severity": "review", "class": "forbidden-path-mention", "path": path, "quote": ln.strip()[:200]})
    idx = sealed_index(sealed_dirs, base_lines(repo, base)) if sealed_dirs else {}
    for path, ln in added:
        n = norm_line(ln)
        if n in idx:
            findings.append({"severity": "review", "class": "sealed-line", "path": path,
                             "sealed_file": os.path.relpath(idx[n], WS), "chars": len(n)})
    return {"base": base, "candidate": cand, "changed_paths": len(names), "added_lines": len(added),
            "sealed_dirs": [os.path.relpath(d, WS) for d in sealed_dirs], "sealed_lines_indexed": len(idx),
            "findings": findings, "status": status([f["severity"] for f in findings])}


def render_md(rep):
    L = ["# Candidate clearance", "", f"**Status: {rep['status']}**", ""]
    for t in rep.get("transcripts", []):
        L += [f"## Transcript `{t['transcript']}`", "",
              f"{t['events']} events, {t['tool_uses']} tool uses; status **{t['status']}**; hits by class: {json.dumps(t['counts'])}", ""]
        shown = [h for h in t["hits"] if h["severity"]]
        if shown:
            L += ["| event | class | severity | tool | quote |", "|---|---|---|---|---|"]
            L += [f"| {h['event']} | {h['class']} | {h['severity']} | {h['tool'] or ''} | {h['quote'][:200].replace('|', '/')} |"
                  for h in shown[:200]]
        L.append("")
    d = rep.get("diff")
    if d:
        L += [f"## Candidate diff `{d['base'][:12]}..{d['candidate'][:12]}`", "",
              f"{d['changed_paths']} changed paths, {d['added_lines']} added lines, {d['sealed_lines_indexed']} sealed lines "
              f"indexed from {', '.join(d['sealed_dirs']) or 'none'}; status **{d['status']}**", ""]
        if d["findings"]:
            L += ["| severity | class | path | detail |", "|---|---|---|---|"]
            L += [f"| {f['severity']} | {f['class']} | {f['path']} | {f.get('quote') or f.get('sealed_file') or f.get('status', '')} |"
                  for f in d["findings"][:300]]
        L.append("")
    return "\n".join(L) + "\n"


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--out", required=True)
    ap.add_argument("--transcript", action="append", default=[])
    ap.add_argument("--brief-file", action="append", default=[])
    ap.add_argument("--forbid", action="append", default=[])
    ap.add_argument("--repo")
    ap.add_argument("--base")
    ap.add_argument("--candidate")
    ap.add_argument("--sealed", action="append")
    a = ap.parse_args(argv)
    forbid = re.compile("|".join([DEFAULT_FORBID] + a.forbid))
    rep = {"forbid": forbid.pattern, "transcripts": [audit(t, forbid, a.brief_file) for t in a.transcript]}
    if a.base or a.candidate:
        if not (a.repo and a.base and a.candidate):
            ap.error("--repo, --base and --candidate go together")
        rep["diff"] = diff_check(a.repo, a.base, a.candidate, a.sealed if a.sealed is not None else default_sealed(), forbid)
    sevs = [t["status"] for t in rep["transcripts"]] + ([rep["diff"]["status"]] if "diff" in rep else [])
    rep["status"] = "refused" if "refused" in sevs else "review" if "review" in sevs else "clear"
    os.makedirs(a.out, exist_ok=True)
    with open(os.path.join(a.out, "clearance.json"), "w", encoding="utf-8") as f:
        json.dump(rep, f, indent=1, ensure_ascii=False)
    with open(os.path.join(a.out, "CLEARANCE.md"), "w", encoding="utf-8") as f:
        f.write(render_md(rep))
    print(json.dumps({"status": rep["status"],
                      "transcripts": [{"file": os.path.basename(t["transcript"]), "status": t["status"], "counts": t["counts"]}
                                      for t in rep["transcripts"]],
                      "diff": {k: rep["diff"][k] for k in ("status", "changed_paths", "added_lines")} | {
                          "findings": len(rep["diff"]["findings"])} if "diff" in rep else None}, indent=1))
    return rep


if __name__ == "__main__":
    sys.exit(0 if main()["status"] != "refused" else 1)
