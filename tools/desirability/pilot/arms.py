"""Pilot arms: throwaway trees of arm commits, per-arm cargo target directories with a provenance
marker checked before every use, and the sealed-location guards.

- **Tree**: `git archive <commit>` unpacked into a fresh directory (removed first), every file and
  directory given a fresh modification time (cargo decides freshness by mtime; a file restored to
  the archive's older time could let an earlier build be reused), plus `.pilot-provenance`
  (commit, SHA-256 of the archive stream). `check_tree` re-reads the marker before every use.
- **Target**: `$CARGO_TARGET_DIR/pilot/<sha12>[-<use>]/` (one per commit and use, never shared
  between commits: the H04 stale-rustdoc incident came from one shared directory). The marker
  `PILOT_PROVENANCE.json` names the full commit and the use; `target_dir` creates it in an empty
  directory and refuses a directory whose marker names anything else, or that holds files but no
  marker. `check_target` runs before every cargo command.
- **Sealed locations**: task sets and run directories live under `<WS>/work/sealed/` and never
  inside a repository checkout (`sealed_dir`).
"""
import hashlib
import json
import os
import shutil
import subprocess
import time

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.realpath(os.path.join(HERE, "..", "..", ".."))
WS = "/home/canarit/projects/Concord_Desirability_Loop"
SEALED = os.path.join(WS, "work", "sealed")
TREE_MARKER = ".pilot-provenance"
TARGET_MARKER = "PILOT_PROVENANCE.json"


class Refused(SystemExit):
    """A refusal of the harness (wrong location, stale marker, unfrozen set, ...)."""


def git(repo, *args):
    r = subprocess.run(["git", "-C", repo] + list(args), capture_output=True, text=True)
    if r.returncode:
        raise Refused(f"git {' '.join(args)}: {r.stderr.strip()[:300]}")
    return r.stdout.strip()


def full_sha(commit, repo=REPO):
    return git(repo, "rev-parse", "--verify", commit + "^{commit}")


def archive_sha(sha, repo=REPO):
    p = subprocess.Popen(["git", "-C", repo, "archive", sha], stdout=subprocess.PIPE)
    h = hashlib.sha256()
    for chunk in iter(lambda: p.stdout.read(1 << 20), b""):
        h.update(chunk)
    if p.wait():
        raise Refused(f"git archive {sha} failed")
    return h.hexdigest()


def inside(path, root):
    path, root = os.path.realpath(path), os.path.realpath(root)
    return path == root or path.startswith(root.rstrip("/") + "/")


def checkout_root(path):
    """The git checkout holding `path` (or its nearest existing parent), else None."""
    p = os.path.realpath(path)
    while not os.path.isdir(p):
        p = os.path.dirname(p)
    r = subprocess.run(["git", "-C", p, "rev-parse", "--show-toplevel"], capture_output=True, text=True)
    return r.stdout.strip() or None if r.returncode == 0 else None


def sealed_dir(path, what):
    """Refuse a task set or run directory outside `<WS>/work/sealed/` or inside a checkout."""
    if not inside(path, SEALED):
        raise Refused(f"{what} {path} is not under {SEALED}: sealed content and results stay there")
    if inside(path, REPO) or checkout_root(path):
        raise Refused(f"{what} {path} is inside a git checkout: sealed content never enters a repository")
    return os.path.realpath(path)


def materialise(commit, dest, repo=REPO):
    """A fresh tree of `commit` at `dest`; returns its provenance record."""
    sha = full_sha(commit, repo)
    if os.path.lexists(dest):
        shutil.rmtree(dest)
    os.makedirs(dest)
    arch = subprocess.Popen(["git", "-C", repo, "archive", sha], stdout=subprocess.PIPE)
    subprocess.run(["tar", "-x", "-C", dest], stdin=arch.stdout, check=True)
    if arch.wait():
        raise Refused(f"git archive {sha} failed")
    now = time.time()
    for dp, dn, fn in os.walk(dest):
        for f in fn + dn:
            try:
                os.utime(os.path.join(dp, f), (now, now), follow_symlinks=False)
            except OSError:
                pass
    prov = {"commit": sha, "archive_sha256": archive_sha(sha, repo)}
    with open(os.path.join(dest, TREE_MARKER), "w", encoding="utf-8") as f:
        f.write(json.dumps(prov, sort_keys=True) + "\n")
    return prov


def check_tree(dest, sha):
    try:
        prov = json.load(open(os.path.join(dest, TREE_MARKER), encoding="utf-8"))
    except (OSError, ValueError):
        raise Refused(f"tree {dest} has no readable {TREE_MARKER}")
    if prov.get("commit") != sha:
        raise Refused(f"tree {dest} holds {prov.get('commit')}, not {sha}")
    return prov


def target_root():
    root = os.environ.get("CARGO_TARGET_DIR")
    if not root or not os.path.isabs(root):
        raise Refused("set an absolute CARGO_TARGET_DIR (per-arm subdirectories are made under it)")
    return root


def target_dir(sha, use="clean", root=None):
    """The cargo target directory of (commit, use), created with its marker if new."""
    name = sha[:12] + ("" if use == "clean" else f"-{use}")
    d = os.path.join(root or target_root(), "pilot", name)
    marker = os.path.join(d, TARGET_MARKER)
    if not os.path.exists(marker):
        if os.path.isdir(d) and os.listdir(d):
            raise Refused(f"target {d} holds files but no {TARGET_MARKER}: refusing to reuse a directory of unknown provenance")
        os.makedirs(d, exist_ok=True)
        with open(marker, "w", encoding="utf-8") as f:
            f.write(json.dumps({"commit": sha, "use": use}, sort_keys=True) + "\n")
    check_target(d, sha, use)
    return d


def check_target(d, sha, use="clean"):
    try:
        m = json.load(open(os.path.join(d, TARGET_MARKER), encoding="utf-8"))
    except (OSError, ValueError):
        raise Refused(f"target {d} has no readable {TARGET_MARKER}")
    if m != {"commit": sha, "use": use}:
        raise Refused(f"target {d} is marked {m}, not {{'commit': {sha!r}, 'use': {use!r}}}: refusing a stale target")
    return m
