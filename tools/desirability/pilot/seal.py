"""Pilot sealing: the canonical listing hash of a task set, the frozen-hash ledger, the run file
and its binding to a sealed run directory.

- `set_hash(set)`: the I4 runner's `bank_hash` (imported from `edits/run.py`, unchanged): every
  regular file under `<set>/tasks/`, one line `<sha256 of file>  <path relative to tasks/>`,
  sorted by path (POSIX separators, byte order), newline-terminated; SHA-256 of that listing.
- `BANKS.sha256` (next to this file, committed): one line per frozen task set,
  `<sha256>  <set name>  <files> files, <tasks> task files; frozen <UTC time>`. `freeze` appends a
  line and refuses a name that is already frozen with another hash (a corrected set gets a new
  name). `run` refuses a set whose hash is not the frozen one for its name.
- A run file (JSON, sealed) names the set, its frozen name, the run directory, the arms and the
  repetitions; the first `run` copies it to `<out>/RUN.json` and every later command refuses a run
  file that differs from that copy (results of one run never mix two definitions).
"""
import datetime
import hashlib
import importlib.util
import json
import os
import re

import arms as A

HERE = os.path.dirname(os.path.abspath(__file__))
LEDGER = os.path.join(HERE, "BANKS.sha256")
_spec = importlib.util.spec_from_file_location("i4_run", os.path.join(HERE, "..", "edits", "run.py"))
I4 = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(I4)  # the I4 runner (bank_hash, oracle_verdict, failure_blocks, ...), by import


def set_hash(set_dir):
    """(sha256, files, task files) of a task set's canonical listing."""
    h, nfiles, _ = I4.bank_hash(set_dir)
    ntasks = sum(1 for dp, _, fn in os.walk(os.path.join(set_dir, "tasks")) for f in fn
                 if f.endswith((".task.json", ".task.toml")))
    return h, nfiles, ntasks


def ledger(path=LEDGER):
    out = {}
    if os.path.isfile(path):
        for ln in open(path, encoding="utf-8"):
            parts = ln.split()
            if len(parts) >= 2 and re.fullmatch(r"[0-9a-f]{64}", parts[0]):
                out[parts[1]] = parts[0]
    return out


def freeze(set_dir, name, path=LEDGER):
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]*", name):
        raise A.Refused(f"set name {name!r}: letters, digits, `._-`")
    h, nfiles, ntasks = set_hash(set_dir)
    have = ledger(path).get(name)
    if have == h:
        return h, False
    if have:
        raise A.Refused(f"task set {name} is frozen as {have}; this set hashes to {h}: a changed set needs a new name")
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    with open(path, "a", encoding="utf-8") as f:
        f.write(f"{h}  {name}  {nfiles} files, {ntasks} task files; frozen {stamp}\n")
    return h, True


def check_frozen(set_dir, name, path=LEDGER):
    h, nfiles, _ = set_hash(set_dir)
    want = ledger(path).get(name)
    if want is None:
        raise A.Refused(f"task set {name} is not frozen in {os.path.basename(path)}: freeze it before any candidate run")
    if want != h:
        raise A.Refused(f"task set {set_dir} hashes to {h} ({nfiles} files), but {name} is frozen as {want}: refusing to run")
    return h


RUN_KEYS = {"format", "name", "set", "set_name", "out", "arms", "baseline", "reps", "reader"}


def load_runfile(path, sealed=True):
    """The run file, validated; `sealed` refuses a set or run directory outside the sealed area."""
    with open(path, encoding="utf-8") as f:
        text = f.read()
    rf = json.loads(text)
    probs = []
    if not isinstance(rf, dict) or set(rf) - RUN_KEYS or rf.get("format") != 1:
        probs.append(f"a run file is {{{', '.join(sorted(RUN_KEYS))}}} with format 1")
    else:
        for k in ("name", "set", "set_name", "out", "arms"):
            if k not in rf:
                probs.append(f"missing `{k}`")
        arms = rf.get("arms") or []
        names = [a.get("name") for a in arms if isinstance(a, dict)]
        if not arms or len(names) != len(arms) or len(set(names)) != len(names) or \
                not all(isinstance(a, dict) and set(a) == {"name", "commit"} for a in arms):
            probs.append("`arms`: [{name, commit}, ..] with distinct names")
        if rf.get("baseline", names[0] if names else None) not in names:
            probs.append("`baseline` must name one of the arms (default: the first)")
        if not isinstance(rf.get("reps", 2), int) or not 1 <= rf.get("reps", 2) <= 5:
            probs.append("`reps`: 1..5 (default 2)")
    if probs:
        raise A.Refused(f"run file {path}: " + "; ".join(probs))
    rf.setdefault("baseline", rf["arms"][0]["name"])
    rf.setdefault("reps", 2)
    if sealed:
        A.sealed_dir(rf["set"], "task set")
        A.sealed_dir(rf["out"], "run directory")
    rf["_sha256"] = hashlib.sha256(text.encode("utf-8")).hexdigest()
    rf["_text"] = text
    return rf


def bind_run(rf):
    """Copy the run file into its run directory on first use; refuse a different one later."""
    os.makedirs(rf["out"], exist_ok=True)
    p = os.path.join(rf["out"], "RUN.json")
    if os.path.isfile(p):
        with open(p, encoding="utf-8") as f:
            if f.read() != rf["_text"]:
                raise A.Refused(f"{p} holds another run file: one run directory per run definition")
    else:
        with open(p, "w", encoding="utf-8") as f:
            f.write(rf["_text"])


def write_json(path, obj):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        json.dump(obj, f, indent=1, sort_keys=True, ensure_ascii=False)
        f.write("\n")
