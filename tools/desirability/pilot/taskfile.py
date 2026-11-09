"""Pilot task files: schema, validation and loading of a task set.

A task set is a directory `<set>/tasks/` (sealed, outside the repository) holding one file per
task, `*.task.json` or `*.task.toml`, plus the files they reference (patches, oracles, reply
fixtures, reader texts) and an optional `set.json`. Every other file is data. The schema is the
README's "Task files"; `validate_task` enforces it strictly (unknown keys are errors, so a typo
cannot silently drop an expectation) and `load_set` refuses a set with any invalid task.
"""
import json
import os
import re
import tomllib

KINDS = ("compile", "request", "stage", "reader")
ROLES = ("witness", "contrary", "control")
STAGES = ("SDK build", "caller compile (existing tests)", "caller compile", "run-time refusal",
          "wrong behaviour caught by the existing tests", "wrong behaviour caught by the oracle only", "never")
ID_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._/-]{0,119}$")

COMMON = {"format": int, "id": str, "kind": str, "class": str, "sdk": str, "role": str, "group": str,
          "note": str, "control": dict}
REQUIRED = ("format", "id", "kind", "class", "sdk")
KIND_FIELDS = {
    "compile": {"code": str, "form": str, "imports": list, "expect": dict},
    "request": {"setup": (str, list), "call": str, "imports": list, "replies": list, "limit_s": int, "expect": dict},
    "stage": {"oracle": str, "oracle_name": str, "package": str, "names": str, "patches": dict, "refusal": dict,
              "expect": dict},
    "reader": {"number": int, "text": str, "text_file": str, "key": dict, "reference": dict, "wrong": dict,
               "equivalent": list, "fixtures": str, "sdk_info": dict, "compile_at": str, "title": str},
}
KIND_REQUIRED = {"compile": ("code", "expect"), "request": ("call", "replies", "expect"),
                 "stage": ("oracle", "patches", "expect"), "reader": ("number", "key", "reference")}
REPLY_KEYS = {"status", "headers", "body", "body_file", "hang_up", "delay_ms"}
REQUEST_KEYS = {"method", "target", "headers", "body"}


def _type_ok(v, t):
    if t is int:
        return isinstance(v, int) and not isinstance(v, bool)
    return isinstance(v, t)


def _rel_file(tasks_dir, rel, what, errs, directory=False):
    if not isinstance(rel, str) or not rel or os.path.isabs(rel) or ".." in rel.replace("\\", "/").split("/"):
        errs.append(f"{what}: {rel!r} must be a relative path inside the task set")
        return None
    p = os.path.join(tasks_dir, rel)
    if os.path.islink(p) or not (os.path.isdir(p) if directory else os.path.isfile(p)):
        errs.append(f"{what}: {rel} is not a regular {'directory' if directory else 'file'} of the task set")
        return None
    return p


def _str_list(v, what, errs):
    if not isinstance(v, list) or not all(isinstance(x, str) for x in v):
        errs.append(f"{what} must be a list of strings")
        return False
    return True


def _check_replies(replies, base, what, errs):
    if not replies:
        errs.append(f"{what}: at least one reply")
    for i, r in enumerate(replies):
        if not isinstance(r, dict) or set(r) - REPLY_KEYS:
            errs.append(f"{what}[{i}]: a reply has only {sorted(REPLY_KEYS)}")
            continue
        if "body_file" in r:
            _rel_file(base, r["body_file"], f"{what}[{i}].body_file", errs)
        if "headers" in r and not all(isinstance(h, list) and len(h) == 2 and all(isinstance(x, str) for x in h)
                                      for h in r["headers"]):
            errs.append(f"{what}[{i}].headers: [[name, value], ..]")


def _check_requests(reqs, what, errs):
    if not isinstance(reqs, list):
        errs.append(f"{what}: a list of requests")
        return
    for i, q in enumerate(reqs):
        if not isinstance(q, dict) or set(q) != REQUEST_KEYS:
            errs.append(f"{what}[{i}]: a request has exactly {sorted(REQUEST_KEYS)} (method, target = path?query, "
                        "headers = [[name, value], ..] without host/content-length/transfer-encoding/connection/"
                        "accept-encoding, body = text)")
            continue
        if not all(isinstance(h, list) and len(h) == 2 and all(isinstance(x, str) for x in h) for h in q["headers"]):
            errs.append(f"{what}[{i}].headers: [[name, value], ..]")


def validate_task(t, tasks_dir, path="?"):
    """A list of problems of one parsed task (empty when valid)."""
    errs = []
    if not isinstance(t, dict):
        return [f"{path}: a task is a JSON/TOML object"]
    kind = t.get("kind")
    for k in REQUIRED:
        if k not in t:
            errs.append(f"missing `{k}`")
    if kind not in KINDS:
        return errs + [f"`kind` must be one of {KINDS}"]
    allowed = {**COMMON, **KIND_FIELDS[kind]}
    for k, v in t.items():
        if k not in allowed:
            errs.append(f"unknown key `{k}` for kind {kind}")
        elif not _type_ok(v, allowed[k]):
            errs.append(f"`{k}` has the wrong type")
    for k in KIND_REQUIRED[kind]:
        if k not in t:
            errs.append(f"missing `{k}` (kind {kind})")
    if errs:
        return errs
    if t["format"] != 1:
        errs.append("`format` must be 1")
    if not ID_RE.match(t["id"]):
        errs.append("`id`: letters, digits, `._/-`, at most 120 characters")
    if "role" in t and t["role"] not in ROLES:
        errs.append(f"`role` must be one of {ROLES}")
    if "control" in t:
        c = t["control"]
        if set(c) - {"expect", "reason"} or c.get("expect") not in ("pass", "fail"):
            errs.append("`control`: {expect: pass|fail, reason?: text the failure reason must contain}")
    base = os.path.dirname(path) if path != "?" else tasks_dir
    e = t.get("expect", {})
    if kind == "compile":
        if t.get("form", "body") not in ("body", "items"):
            errs.append("`form` must be body or items")
        if "imports" in t:
            _str_list(t["imports"], "`imports`", errs)
        if set(e) - {"compiles", "first"} or not isinstance(e.get("compiles"), bool):
            errs.append("`expect`: {compiles: bool, first?: {..}}")
        first = e.get("first")
        if first is not None:
            if e.get("compiles") is not False:
                errs.append("`expect.first` only when compiles is false")
            if not isinstance(first, dict) or set(first) - {"code", "mentions", "absent", "matches", "in_snippet", "scope"}:
                errs.append("`expect.first`: {code?, mentions?, absent?, matches?, in_snippet?, scope?}")
            else:
                if "code" in first and not (isinstance(first["code"], str) or _str_list(first["code"], "`expect.first.code`", [])):
                    errs.append("`expect.first.code`: a code or a list of codes")
                for k in ("mentions", "absent"):
                    if k in first:
                        _str_list(first[k], f"`expect.first.{k}`", errs)
                if "matches" in first:
                    try:
                        re.compile(first["matches"])
                    except (re.error, TypeError):
                        errs.append("`expect.first.matches` is not a regular expression")
                if first.get("scope", "all") not in ("all", "primary"):
                    errs.append("`expect.first.scope`: all | primary")
                if "in_snippet" in first and not isinstance(first["in_snippet"], bool):
                    errs.append("`expect.first.in_snippet` is a boolean")
    elif kind == "request":
        if "imports" in t:
            _str_list(t["imports"], "`imports`", errs)
        if isinstance(t.get("setup"), list):
            _str_list(t["setup"], "`setup`", errs)
        _check_replies(t["replies"], base, "`replies`", errs)
        if set(e) - {"ok", "outcome", "requests", "volatile"} or "requests" not in e:
            errs.append("`expect`: {requests, ok?, outcome?, volatile?}")
        else:
            _check_requests(e["requests"], "`expect.requests`", errs)
        if "ok" in e and not isinstance(e["ok"], bool):
            errs.append("`expect.ok` is a boolean")
        if "outcome" in e and not isinstance(e["outcome"], str):
            errs.append("`expect.outcome` is the exact outcome text")
        v = e.get("volatile", {})
        if not isinstance(v, dict) or set(v) - {"headers"} or not _str_list(v.get("headers", []), "`expect.volatile.headers`", []):
            errs.append("`expect.volatile`: {headers: [lower-case names whose values are not compared]}")
        if not 1 <= t.get("limit_s", 20) <= 50:
            errs.append("`limit_s`: 1..50 (the run sandbox stops at 60 s)")
    elif kind == "stage":
        _rel_file(base, t["oracle"], "`oracle`", errs)
        if "names" in t:
            try:
                re.compile(t["names"])
            except re.error:
                errs.append("`names` is not a regular expression")
        if not re.fullmatch(r"[a-z][a-z0-9_]*", t.get("oracle_name", "pilot_oracle")):
            errs.append("`oracle_name`: a test target name")
        for arm, p in t["patches"].items():
            if not isinstance(p, dict) or set(p) - {"reference", "omission", "controls"} or not {"reference", "omission"} <= set(p):
                errs.append(f"`patches.{arm}`: {{reference, omission, controls?}}")
                continue
            for v in ("reference", "omission"):
                _rel_file(base, p[v], f"`patches.{arm}.{v}`", errs)
            for i, c in enumerate(p.get("controls", [])):
                if not isinstance(c, dict) or set(c) - {"name", "patch", "expect"} or "patch" not in c or "name" not in c:
                    errs.append(f"`patches.{arm}.controls[{i}]`: {{name, patch, expect?: fail|pass}}")
                else:
                    _rel_file(base, c["patch"], f"`patches.{arm}.controls[{i}].patch`", errs)
        for arm, rx in t.get("refusal", {}).items():
            try:
                re.compile(rx)
            except (re.error, TypeError):
                errs.append(f"`refusal.{arm}` is not a regular expression")
        if set(e) - {"stages", "names_endpoint"} or not e.get("stages") or not set(e["stages"]) <= set(STAGES):
            errs.append(f"`expect`: {{stages: subset of {list(STAGES)}, names_endpoint?: bool}}")
        if e.get("names_endpoint") and "names" not in t:
            errs.append("`expect.names_endpoint` needs `names`")
    elif kind == "reader":
        if ("text" in t) == ("text_file" in t):
            errs.append("exactly one of `text` and `text_file`")
        if "text_file" in t:
            _rel_file(base, t["text_file"], "`text_file`", errs)
        if "fixtures" in t:
            _rel_file(base, t["fixtures"], "`fixtures`", errs, directory=True)
        key = t["key"]
        if not isinstance(key.get("checks"), list) or not isinstance(key.get("facts"), list) or not key["facts"]:
            errs.append("`key`: {checks: [..], facts: [{id, checks}, ..], claims?} (the H04 key format)")
        else:
            ids = {c.get("id") for c in key["checks"]}
            for f in key["facts"]:
                if not set(f.get("checks", [])) <= ids or not f.get("checks"):
                    errs.append(f"`key.facts`: {f.get('id')} names an unknown check")
            for c in key["checks"]:
                if c.get("kind") not in ("url", "choice", "int", "ident", "lower", "type", "code"):
                    errs.append(f"`key.checks`: {c.get('id')} has an unknown kind {c.get('kind')!r}")
        if "sdk_info" in t and set(t["sdk_info"]) != {"dir", "pkg", "crate"}:
            errs.append("`sdk_info`: {dir, pkg, crate}")
        if "compile_at" in t and not re.fullmatch(r"arm|[0-9a-f]{7,40}", t["compile_at"]):
            errs.append("`compile_at`: `arm` or a commit")
    return errs


def parse_file(path):
    with open(path, "rb") as f:
        raw = f.read()
    if path.endswith(".task.toml"):
        return tomllib.loads(raw.decode("utf-8"))
    return json.loads(raw.decode("utf-8"))


def task_files(tasks_dir):
    out = []
    for dp, dn, fn in os.walk(tasks_dir):
        dn.sort()
        for f in sorted(fn):
            if f.endswith((".task.json", ".task.toml")):
                out.append(os.path.join(dp, f))
    return out


def load_set(set_dir):
    """{"dir", "tasks_dir", "meta", "tasks": {id: task}, "problems": [..]}; every task carries
    `_path` (its file) and `_dir` (the directory its relative paths resolve against)."""
    tasks_dir = os.path.join(set_dir, "tasks")
    if not os.path.isdir(tasks_dir):
        return {"dir": set_dir, "tasks_dir": tasks_dir, "meta": {}, "tasks": {}, "problems": [f"{tasks_dir} is missing"]}
    meta_p = os.path.join(tasks_dir, "set.json")
    meta = json.load(open(meta_p, encoding="utf-8")) if os.path.isfile(meta_p) else {}
    tasks, problems = {}, []
    if set(meta) - {"format", "name", "description", "reader_template", "commit"}:
        problems.append("set.json: only format, name, description, reader_template, commit")
    if "reader_template" in meta:
        _rel_file(tasks_dir, meta["reader_template"], "set.json reader_template", problems)
    for p in task_files(tasks_dir):
        rel = os.path.relpath(p, tasks_dir)
        try:
            t = parse_file(p)
        except (ValueError, tomllib.TOMLDecodeError, UnicodeDecodeError) as e:
            problems.append(f"{rel}: not parseable: {e}")
            continue
        for e in validate_task(t, tasks_dir, p):
            problems.append(f"{rel}: {e}")
        if isinstance(t, dict) and isinstance(t.get("id"), str):
            if t["id"] in tasks:
                problems.append(f"{rel}: duplicate id {t['id']}")
            t["_path"], t["_dir"] = p, os.path.dirname(p)
            tasks[t["id"]] = t
    if not tasks and not problems:
        problems.append("no task files (*.task.json, *.task.toml)")
    return {"dir": set_dir, "tasks_dir": tasks_dir, "meta": meta, "tasks": tasks, "problems": problems}


def public(t):
    """The fields of a task a public summary may name: id, kind, class, SDK, role, group."""
    return {"id": t["id"], "kind": t["kind"], "class": t["class"], "sdk": t["sdk"], "role": t.get("role", "-"),
            "group": t.get("group", t["sdk"])}
