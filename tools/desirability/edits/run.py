"""I4 runner: one fresh editing agent on one task in an isolated source export, then the hidden oracle.

    python3 -I tools/desirability/edits/run.py --task SDK/KIND --provider codex|claude \
        --commit SHA [--out DIR] [--model M] [--effort E] [--repo PATH] [--bank DIR]
        [--agent-target DIR] [--measure-target DIR] [--timeout SECONDS] [--keep]
    python3 -I tools/desirability/edits/run.py --probe-only --task SDK/KIND --commit SHA
        [--provider codex|claude] [--deep]
    python3 -I tools/desirability/edits/run.py --task SDK/KIND --commit SHA \
        [--replay PATCH] [--replay-shell CMD]          (instrument validation: no agent runs)

The answer-bearing bank (oracles, reference and control patches, the full `meta.json` with the
expected sites) is **not** in the repository: it is read from `--bank` (default
`<WS>/work/sealed/i4-bank`, layout `tasks/<sdk>/<kind>/`), and its canonical listing must hash to
`BANK.sha256` (next to this file) or the run is refused. The repository keeps only the agent-facing
`task.md` and the non-revealing `meta.json` fields (checked against the sealed copy).

Steps (see `README.md` for what each outcome means):

1. **Export.** `git archive SHA`, minus `tools/desirability/`, is unpacked into
   `<WS>/work/i4-runs/<run-id>/baseline` (made read-only; never mounted into the agent's sandbox:
   the immutable baseline the diff is taken against), then copied to `<run>/agent/src` with a fresh
   `git init` + one commit (a trusted git environment: no system or global config, a scratch
   `HOME`, hooks off). No object of the real repository is reachable from it.
2. **Sandbox.** The agent runs inside bubblewrap (`sandbox_cmd`): the root file system read-only;
   the whole home directory replaced by an empty tmpfs (this hides `~/projects` -- the repository,
   every worktree, the loop workspace and the sealed bank -- and also `~/.claude` and `~/.codex`,
   whose transcripts, rollouts and thread databases hold the bank's text); then only
   `<run>/agent` (the export and the per-run provider home) and the agent's target directory are
   bound back writable, `~/.cargo` and `~/.rustup` read-only, the agent binary's release directory
   read-only, and the provider's credential file into the fresh per-run provider home
   (`<run>/agent/codex-home`, `CODEX_HOME`; `<run>/agent/claude-home`, `CLAUDE_CONFIG_DIR`), so
   the session's rollout is written inside the run directory. The baseline, the measurement copy
   and the output directory are not visible. `/tmp`, `/var/tmp` and `/run` are private tmpfs (the
   DNS resolver directory is bound back read-only), a new PID namespace with its own `/proc`, a
   minimal `/dev`, an empty environment plus an explicit list. The network is NOT unshared (the
   session must reach the provider API).
3. **Isolation check before trusting it.** The same sandbox runs a probe first (`PROBE`); the agent
   is launched only when every check passes; the probe output is kept in `result.json`.
4. **Agent.** Fill `prompts/edit_task.md` (the part after its `---` line; the template's
   illustrative `Example:` line is dropped) with the task's sdk, kind, package and `task.md` text:
   - codex: `codex exec -m gpt-6.1-sol -c model_reasoning_effort="xhigh" -c daybreak=false
     -s danger-full-access -C <src> --skip-git-repo-check --json -o <run>/agent/last.md`, prompt
     on stdin. Codex's own Linux sandbox cannot run inside the bubblewrap sandbox on this host
     (its bundled bwrap needs a nested user namespace, which the AppArmor `bwrap-userns-restrict`
     stacking denies), so `-s danger-full-access` is passed and the outer bubblewrap is the
     confinement;
   - claude: `claude -p --output-format json --allowedTools "Bash Edit Write Read Glob Grep"
     --tools Bash,Edit,Write,Read,Glob,Grep --permission-mode acceptEdits --safe-mode
     --strict-mcp-config --no-session-persistence [--model M] [--effort E]`, prompt on stdin.
   Requested model/effort are what this script passed; observed values come from harness metadata
   only: the Codex rollout's `turn_context`, Claude's JSON result (`modelUsage` keys).
5. **Nothing the agent could configure runs on the host.** After the agent, the host only *reads*
   its tree: a manifest of `.git/**` (compared with the one taken before the launch), the
   `.cargo/` directories and toolchain files, then a copy of the tree **without** any `.git`, any
   `.cargo/` and any cargo target directory (`CACHEDIR.TAG`) into `<run>/measure`, every file with
   a fresh modification time (the measurement target is shared between runs and cargo decides
   freshness by mtime: an older file would let an earlier run's build be reused). Every command
   on that copy runs inside bubblewrap with the network unshared and only the copy (and the
   measurement target directory) bound: the diff is `git diff --no-index` between the immutable
   baseline and the copy under a trusted git environment (`GIT_CONFIG_NOSYSTEM=1`,
   `GIT_CONFIG_GLOBAL=/dev/null`, a scratch `HOME`, `-c core.fsmonitor=false -c diff.external=
   -c core.hooksPath=/dev/null`, `--no-ext-diff --no-textconv`); cargo runs with the owner's
   `CARGO_HOME` (read-only in the agent's sandbox) and the baseline's toolchain pinned through
   `RUSTUP_TOOLCHAIN`.
6. **Exposure and tampering → invalid.** The transcript, the agent's diff, its changed paths and
   the `.git` files it created or changed (loose objects decompressed) are scanned for paths only
   the hidden bank has (`LEAK`). Tampering signals: a `.git` change beyond normal commits and index
   updates (`GIT_NORMAL`), any `.cargo/` file, a changed toolchain file, a nested `.git`. Either
   kind of hit sets `outcome.status = "invalid"`; nothing is scored (the measurements are kept
   under `unscored`).
7. **Measure** (cargo one command at a time):
   - the diff (files, hunks, stat: diagnostics only);
   - **authoritative semantic edit sites** (`expected_sites`): per named provider fact, where the
     final tree states it -- reference site, declared alternative, elsewhere in the SDK, or not
     recognised; an unrecognised behavioural fact is `needs review` when the oracle passed (the
     pattern may miss a behaviour-preserving spelling) and `not stated` otherwise;
   - **missed propagation** (`propagation_checks`), next to the counts for the baseline and the
     reference solution;
   - the SDK's existing tests (and the agent's own): `cargo test -p <pkg> --no-fail-fast`;
   - the **hidden oracle**: copied to `examples/<sdk>-sdk/tests/oracle_<kind>.rs`, run with
     `cargo test -p <pkg> --test oracle_<kind> -- --format pretty` and listed with `-- --list`;
     it passes only if every oracle test named in the sealed source is reported as executed and
     `ok`, nothing else ran and the summary counts agree (exit status alone is not enough);
   - for e4, the **classification**: whether the tree contains the reference's classification
     site; when it does not, the stage that caught the omission, or `needs review` when the oracle
     passed.
8. Write `<out>/result.json` (plus `events.jsonl`, `last.md`, `stderr.log`, `agent.patch`,
   `EDIT_REPORT.md`, cargo logs, `isolation.txt`) and remove the run's trees unless `--keep`.

Run Python with `-I`.
"""
import argparse
import datetime
import fnmatch
import glob
import hashlib
import json
import os
import re
import shutil
import stat
import subprocess
import sys
import tomllib
import zlib

HERE = os.path.dirname(os.path.abspath(__file__))
TASKS = os.path.join(HERE, "tasks")  # public part: task.md and the non-revealing meta.json fields
BANK_HASH_FILE = os.path.join(HERE, "BANK.sha256")
DEFAULT_REPO = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
WS = "/home/canarit/projects/Concord_Desirability_Loop"
DEFAULT_BANK = os.path.join(WS, "work", "sealed", "i4-bank")
PROJECTS = os.path.dirname(WS)  # /home/canarit/projects: hidden inside the sandbox
RUNS = os.path.join(WS, "work", "i4-runs")
PROMPT = os.path.join(WS, "prompts", "edit_task.md")
HOME = os.path.expanduser("~")
BWRAP = "/usr/bin/bwrap"
CODEX_MODEL = "gpt-6.1-sol"
CLAUDE_TOOLS = ["Bash", "Edit", "Write", "Read", "Glob", "Grep"]
PUBLIC_META_FIELDS = ("sdk", "kind", "package", "sdk_dir", "title", "public_names")
RESULT_LINE = re.compile(r"test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored")
SUMMARY_LINE = re.compile(r"test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; "
                          r"(\d+) measured; (\d+) filtered out")
TEST_LINE = re.compile(r"^test (.+?) \.\.\. (ok|FAILED|ignored)(?:,.*)?$")
LIST_LINE = re.compile(r"^(.+): test$")
RUNNING = re.compile(r"^\s+Running (\S+)|^\s+Doc-tests (\S+)")
# Paths that only the hidden bank has. (The SDKs have their own `VALIDATION.md` and
# `validation.json` fixtures, and an old release report names a `reference.patch`: those are not
# matched.) In the isolated export none of these files exists; a hit means the session reached
# the bank some other way, and the session is invalid.
LEAK = re.compile(r"desirability/edits|edits/tasks|edits/validation\.json|i4-bank|"
                  r"/e[1-5]/(?:oracle\.rs|reference\.patch|omission\.patch|partial\.patch|meta\.json)|"
                  r"\boracle_e[1-5]\b|\bomission\.patch\b|\bpartial\.patch\b")
# `.git` changes that normal commits, index updates, branch switches and stashes make. Anything
# else the agent created, changed or removed under `.git` is a tampering signal.
GIT_NORMAL = re.compile(r"^(?:index|COMMIT_EDITMSG|ORIG_HEAD|packed-refs|"
                        r"objects/[0-9a-f]{2}/[0-9a-f]{38,62}|"
                        r"refs/heads/.+|refs/stash|logs/HEAD|logs/refs/heads/.+|logs/refs/stash)$")
GIT_HEAD = re.compile(r"^(?:ref: refs/heads/\S+|[0-9a-f]{40,64})\n?$")
TOOLCHAIN_FILES = ("rust-toolchain", "rust-toolchain.toml")
# The no-op cargo runner of the harness controls (validate.py and the run.py replay control).
NOOP_RUNNER_CONFIG = "[target.'cfg(all())']\nrunner = \"/usr/bin/true\"\n"


def now():
    return datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def sh(cmd, cwd=None, log=None, env=None, timeout=3600, stdin=None):
    try:
        r = subprocess.run(cmd, cwd=cwd, input=stdin, stdout=subprocess.PIPE,
                           stderr=subprocess.STDOUT, text=True, timeout=timeout, env=env)
        code, out = r.returncode, r.stdout
    except subprocess.TimeoutExpired as e:
        out = e.stdout.decode(errors="replace") if isinstance(e.stdout, bytes) else (e.stdout or "")
        code, out = -1, out + "\n[timeout]"
    if log:
        with open(log, "w", encoding="utf-8") as f:
            f.write("$ " + " ".join(cmd) + "\n" + out)
    return code, out


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


# ------------------------------------------------------------------ the sealed bank

def bank_hash(bank):
    """SHA-256 of the sealed bank's canonical listing: every regular file under `<bank>/tasks/`,
    one line `<sha256 of file>  <path relative to tasks/>` each, sorted by path (POSIX separators,
    byte order), newline-terminated. Returns (hash, number of files, number of tasks)."""
    root = os.path.join(bank, "tasks")
    names = []
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames.sort()
        for f in filenames:
            p = os.path.join(dirpath, f)
            if os.path.isfile(p) and not os.path.islink(p):
                names.append(os.path.relpath(p, root).replace(os.sep, "/"))
    names.sort(key=lambda n: n.encode())
    listing = "".join(f"{sha256_file(os.path.join(root, n))}  {n}\n" for n in names)
    tasks = {"/".join(n.split("/")[:2]) for n in names if n.count("/") >= 2}
    return hashlib.sha256(listing.encode()).hexdigest(), len(names), len(tasks)


def recorded_bank_hash():
    try:
        return open(BANK_HASH_FILE, encoding="utf-8").read().split()[0]
    except (OSError, IndexError):
        return None


def oracle_test_names(oracle_text):
    """The test functions of an oracle source: `#[test]` / `#[tokio::test(..)]` items."""
    return sorted(re.findall(r"#\[(?:tokio::)?test(?:\([^)]*\))?\]\s*(?:#\[[^\]]*\]\s*)*"
                             r"(?:pub\s+)?(?:async\s+)?fn\s+(\w+)", oracle_text))


def load_task(task, bank):
    """The sealed task (full meta, oracle, reference) and the public text; the public meta.json
    fields must equal the sealed ones."""
    sdk, kind = task.split("/")
    sealed_dir = os.path.join(bank, "tasks", sdk, kind)
    public_dir = os.path.join(TASKS, sdk, kind)
    meta = json.load(open(os.path.join(sealed_dir, "meta.json"), encoding="utf-8"))
    public = json.load(open(os.path.join(public_dir, "meta.json"), encoding="utf-8"))
    drift = {k: (public.get(k), meta.get(k)) for k in PUBLIC_META_FIELDS if public.get(k) != meta.get(k)}
    if drift:
        raise RuntimeError(f"{task}: public meta.json differs from the sealed one: {drift}")
    leaked = sorted(set(public) - set(PUBLIC_META_FIELDS))
    if leaked:
        raise RuntimeError(f"{task}: the public meta.json carries sealed fields {leaked}")
    task_text = open(os.path.join(public_dir, "task.md"), encoding="utf-8").read()
    oracle = open(os.path.join(sealed_dir, "oracle.rs"), encoding="utf-8").read()
    names = oracle_test_names(oracle)
    if sorted(meta.get("oracle_tests", [])) != names:
        raise RuntimeError(f"{task}: sealed meta.json oracle_tests {meta.get('oracle_tests')} != "
                           f"the oracle's test functions {names}")
    return {"meta": meta, "task_text": task_text, "oracle": oracle, "oracle_tests": names,
            "reference": os.path.join(sealed_dir, "reference.patch"), "dir": sealed_dir}


# ------------------------------------------------------------------ test output

def targets(out):
    """Per test target: its name, ok, passed/failed/ignored counts and the tests it reported."""
    rows, current, tests = [], None, {}
    for ln in out.splitlines():
        m = RUNNING.match(ln)
        if m:
            current, tests = m.group(1) or ("doctests " + m.group(2)), {}
            continue
        t = TEST_LINE.match(ln.rstrip())
        if t and current:
            tests[t.group(1)] = t.group(2)
        m = RESULT_LINE.search(ln)
        if m and current:
            rows.append({"target": current, "ok": m.group(1) == "ok", "passed": int(m.group(2)),
                         "failed": int(m.group(3)), "ignored": int(m.group(4)), "tests": tests})
            current, tests = None, {}
    return rows


def section(out, target):
    """The part of a cargo test transcript that belongs to one test target (its `Running` line to
    its result line), or ''."""
    lines, keep, buf = out.splitlines(), False, []
    for ln in lines:
        m = RUNNING.match(ln)
        if m:
            keep = (m.group(1) or "") == target
            if keep:
                buf = [ln]
            continue
        if keep:
            buf.append(ln)
            if RESULT_LINE.search(ln):
                return "\n".join(buf) + "\n"
    return ""


def oracle_verdict(expected, text, code, listed=None):
    """Did the oracle run? Passed only if the exit status is 0, every expected test is reported as
    executed and `ok`, no other test ran, nothing failed or was ignored or filtered out, the one
    summary line counts exactly the expected tests and (when given) the binary's own listing names
    exactly the expected tests. An exit status of 0 without that is `not_executed`."""
    results = {}
    for ln in text.splitlines():
        m = TEST_LINE.match(ln.rstrip())
        if m:
            results[m.group(1)] = m.group(2)
    sums = SUMMARY_LINE.findall(text)
    summary = None
    if len(sums) == 1:
        s = sums[0]
        summary = {"ok": s[0] == "ok", "passed": int(s[1]), "failed": int(s[2]), "ignored": int(s[3]),
                   "measured": int(s[4]), "filtered_out": int(s[5])}
    expected = sorted(expected)
    reported_ok = sorted(n for n, r in results.items() if r == "ok")
    failed = sorted(n for n, r in results.items() if r == "FAILED")
    ignored = sorted(n for n, r in results.items() if r == "ignored")
    missing = [n for n in expected if n not in results]
    unexpected = sorted(n for n in results if n not in expected)
    counts_equal = (summary is not None and summary["ok"] and summary["passed"] == len(expected)
                    and summary["failed"] == 0 and summary["ignored"] == 0
                    and summary["measured"] == 0 and summary["filtered_out"] == 0)
    listed_ok = listed is None or sorted(listed) == expected
    passed = (code == 0 and bool(expected) and reported_ok == expected and not failed and not ignored
              and not unexpected and counts_equal and listed_ok)
    compile_error = "could not compile" in text or (
        code != 0 and not sums and bool(re.search(r"^error(\[E\d+\])?:", text, re.M)))
    if passed:
        status = "passed"
    elif compile_error:
        status = "compile_error"
    elif failed or (summary and summary["failed"]):
        status = "test_failed"
    else:
        status = "not_executed"
    return {"status": status, "pass": passed, "exit": code, "expected": expected,
            "reported_ok": reported_ok, "failed": failed, "ignored": ignored, "missing": missing,
            "unexpected": unexpected, "summary": summary, "counts_equal": counts_equal,
            "listed": sorted(listed) if listed is not None else None, "listed_matches": listed_ok}


def parse_listing(text):
    return sorted(m.group(1) for ln in text.splitlines() if (m := LIST_LINE.match(ln.rstrip())))


def first_error(out):
    lines = out.splitlines()
    for i, ln in enumerate(lines):
        if ln.startswith("error"):
            return "\n".join(lines[i:i + 6])
    for i, ln in enumerate(lines):
        if "panicked at" in ln:
            return "\n".join(lines[i:i + 4])
    return "\n".join(lines[-6:])


# ------------------------------------------------------------------ isolation

def trusted_git(home):
    """argv prefix and environment for git on trees this script created (never on the agent's)."""
    env = {"PATH": "/usr/bin:/bin", "HOME": home, "LANG": "C.UTF-8", "GIT_CONFIG_NOSYSTEM": "1",
           "GIT_CONFIG_GLOBAL": "/dev/null", "GIT_ATTR_NOSYSTEM": "1", "GIT_TERMINAL_PROMPT": "0",
           "GIT_OPTIONAL_LOCKS": "0"}
    argv = ["git", "-c", "core.fsmonitor=false", "-c", "diff.external=", "-c", "core.hooksPath=/dev/null",
            "-c", "user.name=i4-runner", "-c", "user.email=i4-runner@localhost",
            "-c", "commit.gpgsign=false", "-c", "core.quotepath=false"]
    return argv, env


def export_source(repo, sha, baseline, src, git_home):
    """`git archive SHA` minus tools/desirability into the read-only `baseline`; a copy of it in
    `src` with a fresh one-commit history."""
    os.makedirs(baseline)
    archive = subprocess.Popen(["git", "-C", repo, "archive", "--format=tar", sha],
                               stdout=subprocess.PIPE)
    untar = subprocess.run(["tar", "-x", "-C", baseline, "--exclude=tools/desirability"],
                           stdin=archive.stdout, capture_output=True, text=True)
    archive.stdout.close()
    if archive.wait() or untar.returncode:
        raise RuntimeError(f"export failed: {untar.stderr}")
    if os.path.exists(os.path.join(baseline, "tools", "desirability")):
        raise RuntimeError("export still contains tools/desirability")
    shutil.copytree(baseline, src, symlinks=True)
    os.makedirs(git_home, exist_ok=True)
    git, env = trusted_git(git_home)
    env["GIT_CEILING_DIRECTORIES"] = os.path.dirname(src)
    for cmd in (["init", "-q", "-b", "base"], ["add", "-A"],
                ["commit", "-q", "-m", f"I4 base: export of {sha[:12]} without tools/desirability"]):
        code, out = sh(git + ["-C", src] + cmd, env=env)
        if code:
            raise RuntimeError(f"git {cmd[0]} failed in the export: {out}")
    set_writable(baseline, False)


def set_writable(root, writable):
    for dirpath, dirnames, filenames in os.walk(root):
        for name in [dirpath] + [os.path.join(dirpath, f) for f in filenames]:
            if os.path.islink(name):
                continue
            mode = os.lstat(name).st_mode
            mode = (mode | stat.S_IWUSR) if writable else (mode & ~(stat.S_IWUSR | stat.S_IWGRP | stat.S_IWOTH))
            os.chmod(name, mode)


def resolve_binary(name):
    path = shutil.which(name, path=os.path.join(HOME, ".local", "bin") + os.pathsep + os.environ.get("PATH", ""))
    return os.path.realpath(path) if path else None


def provider_setup(provider, agent_root, effort):
    """Binds, environment and argv[0] for the provider inside the sandbox: a fresh per-run provider
    home in the agent's directory with only the credential file bound through (read-write, so a
    token refresh updates the owner's file instead of forking it)."""
    ro, rw, env = [], [], {}
    if provider == "codex":
        exe = resolve_binary("codex")
        # .../releases/<version>/bin/codex: the release directory holds its helpers (bwrap, rg)
        release = os.path.dirname(os.path.dirname(exe))
        ro.append(release)
        home = os.path.join(agent_root, "codex-home")
        os.makedirs(home, exist_ok=True)
        auth = os.path.join(HOME, ".codex", "auth.json")
        placeholder = os.path.join(home, "auth.json")
        open(placeholder, "a").close()
        rw.append((auth, placeholder))
        cfg = [f'model = "{CODEX_MODEL}"', f'model_reasoning_effort = "{effort}"']
        tier = owner_codex_setting("service_tier")
        if tier:
            cfg.append(f"service_tier = {tier}")
        with open(os.path.join(home, "config.toml"), "w") as f:
            f.write("# Written by run.py: a fresh per-run CODEX_HOME (no owner MCP servers or plugins).\n"
                    + "\n".join(cfg) + "\n")
        env["CODEX_HOME"] = home
        return exe, ro, rw, env, {"service_tier": tier}
    exe = resolve_binary("claude")
    ro.append(exe)
    home = os.path.join(agent_root, "claude-home")
    os.makedirs(home, exist_ok=True)
    cred = os.path.join(HOME, ".claude", ".credentials.json")
    if os.path.isfile(cred):
        placeholder = os.path.join(home, ".credentials.json")
        open(placeholder, "a").close()
        rw.append((cred, placeholder))
    env["CLAUDE_CONFIG_DIR"] = home
    return exe, ro, rw, env, {}


def owner_codex_setting(key):
    """A top-level scalar of the owner's Codex config (raw TOML text), or None."""
    try:
        for ln in open(os.path.join(HOME, ".codex", "config.toml"), encoding="utf-8"):
            if ln.startswith("["):
                break
            m = re.match(rf"^{key}\s*=\s*(.+?)\s*$", ln)
            if m:
                return m.group(1)
    except OSError:
        pass
    return None


def sandbox_cmd(writable, argv, cwd, env, ro=(), rw_pairs=(), network=True):
    """bubblewrap around `argv`: the root read-only, the home directory and `~/projects` hidden,
    then only `writable` bound back read-write and `ro` read-only. See the module docstring."""
    cmd = [BWRAP, "--die-with-parent", "--new-session", "--unshare-pid", "--unshare-ipc",
           "--ro-bind", "/", "/",
           # every project, the bank, ~/.claude and ~/.codex (transcripts, rollouts, thread DBs)
           "--tmpfs", HOME,
           # (inside HOME already; kept explicit) the repository, its worktrees, the workspace
           "--tmpfs", PROJECTS,
           "--tmpfs", "/tmp", "--tmpfs", "/var/tmp", "--tmpfs", "/run",
           # /etc/resolv.conf points into /run/systemd/resolve
           "--ro-bind-try", "/run/systemd/resolve", "/run/systemd/resolve",
           "--dev", "/dev", "--proc", "/proc",
           "--ro-bind", os.path.join(HOME, ".cargo"), os.path.join(HOME, ".cargo"),
           "--ro-bind", os.path.join(HOME, ".rustup"), os.path.join(HOME, ".rustup")]
    for p in writable:
        cmd += ["--bind", p, p]
    if not network:
        cmd.append("--unshare-net")
    for p in ro:
        cmd += ["--ro-bind", p, p]
    for src, dst in rw_pairs:
        cmd += ["--bind", src, dst]
    base = {"HOME": HOME, "USER": os.environ.get("USER", "canarit"),
            "LOGNAME": os.environ.get("USER", "canarit"),
            "PATH": f"{HOME}/.cargo/bin:/usr/local/bin:/usr/bin:/bin",
            "LANG": "C.UTF-8", "TMPDIR": "/tmp", "CARGO_HOME": f"{HOME}/.cargo",
            "RUSTUP_HOME": f"{HOME}/.rustup", "CARGO_BUILD_JOBS": "6"}
    base.update(env)
    cmd.append("--clearenv")
    for k, v in sorted(base.items()):
        cmd += ["--setenv", k, v]
    return cmd + ["--chdir", cwd, "--"] + list(argv)


PROBE = r"""
echo "## ls -A {projects}"; ls -A {projects}
echo "## find {projects} -maxdepth 6 -type d (directories leading to the bound paths)"
find {projects} -maxdepth 6 -type d -not -path '{src}/*' -not -path '{target}/*' | sort
echo "## git -C {src} log --oneline"; git -C {src} log --oneline
echo "## commits reachable from any ref: $(git -C {src} rev-list --all | wc -l)"
echo "## alternates: $(cat {src}/.git/objects/info/alternates 2>/dev/null || echo none)"
echo "## ls -A {home}"; ls -A {home}
for p in {repo}/tools/desirability/edits/tasks {here}/tasks {bank} {run}/baseline {run}/measure \
         {out} {home}/.codex/sessions {home}/.codex/thread_history_1.sqlite {home}/.claude/projects \
         {src}/tools/desirability; do
  if [ -e "$p" ]; then echo "VISIBLE $p"; else echo "absent $p"; fi
done
echo "## processes visible: $(ls /proc | grep -cE '^[0-9]+$')"
echo "## pid 1 in this namespace: $(cat /proc/1/comm)"
echo "## writable test: $(touch {home}/.probe_rw 2>&1 && echo 'home tmpfs writable (private)')"
echo "## /tmp entries: $(ls -A /tmp | wc -l)"
"""

DEEP = r"""
echo "## user-owned paths outside .cargo/.rustup, the agent dir, the target dir and bound binaries"
find / \( -path /proc -o -path /sys -o -path /dev -o -path {home}/.cargo -o -path {home}/.rustup \
          -o -path {agent} -o -path {target} {extra_prune} \) -prune -o -user $(id -u) -print 2>/dev/null | head -50
echo "## bank marker search (grep -rIl over every readable file except /proc /sys /dev /usr /snap and the toolchains)"
timeout 600 grep -rIl --exclude-dir=proc --exclude-dir=sys --exclude-dir=dev --exclude-dir=usr \
  --exclude-dir=snap --exclude-dir=.rustup --exclude-dir=registry -e '{marker}' / 2>/dev/null | head -20
echo "## marker search done"
echo "## positive control: the same search for a string of the export ({control})"
timeout 600 grep -rIl --exclude-dir=proc --exclude-dir=sys --exclude-dir=dev --exclude-dir=usr \
  --exclude-dir=snap --exclude-dir=.rustup --exclude-dir=registry -e '{control}' / 2>/dev/null | head -5
echo "## control search done"
"""


def run_probe(ctx, deep=False, repo=DEFAULT_REPO, marker=None):
    src, agent_root, target = ctx["src"], ctx["agent_root"], ctx["target"]
    script = PROBE.format(projects=PROJECTS, src=src, target=target, home=HOME, repo=repo, here=HERE,
                          bank=ctx["bank"], run=ctx["run_dir"], out=ctx["out"])
    if deep:
        extra = "".join(f" -o -path {p}" for p in ctx["ro"])
        script += DEEP.format(home=HOME, agent=agent_root, target=target, extra_prune=extra,
                              marker=marker or "oracle_client_id_header",
                              control="Every Strava rule the send policy keys on")
    code, out = sh(sandbox_cmd([agent_root, target], ["/bin/sh", "-c", script], src, ctx["env"],
                               ctx["ro"], ctx["rw"]), timeout=900)
    checks = {
        "projects_lists_only_workspace": out.split("## ls -A " + PROJECTS + "\n", 1)[-1].split("\n## ", 1)[0].split()
        == [os.path.basename(WS)],
        "one_commit": "## commits reachable from any ref: 1\n" in out,
        "no_alternates": "## alternates: none" in out,
        "bank_baseline_out_and_transcripts_absent": not re.search(r"^VISIBLE ", out, re.M),
        "own_pid_namespace": bool(re.search(r"## processes visible: ([1-9])\n", out)),
    }
    leaked = []
    if deep:
        hits = out.split("## bank marker search", 1)[-1].split("\n", 1)[-1].split("## marker search done")[0]
        leaked = [ln for ln in hits.splitlines() if ln.strip()]
        checks["marker_not_found"] = not leaked
        control = out.split("## positive control", 1)[-1].split("\n", 1)[-1].split("## control search done")[0]
        checks["search_finds_a_control_string"] = os.path.join(src, "examples", "strava-sdk", "src", "auth.rs") in control
    return {"exit": code, "ok": code == 0 and all(checks.values()), "checks": checks,
            "marker_hits": leaked, "output": out}


# ------------------------------------------------------------------ agents

def fill_prompt(meta, task_text):
    text = open(PROMPT, encoding="utf-8").read()
    body = text.split("\n---\n", 1)[1] if "\n---\n" in text else text
    lines, skip = [], False
    for ln in body.splitlines():
        if ln.startswith("Example:"):
            skip = True
            continue
        if skip and ln.strip() and not ln.startswith(("Constraints", "-", "When")):
            continue
        skip = False
        lines.append(ln)
    body = "\n".join(lines).strip() + "\n"
    # "**Task {kind}: {task text}.**" holds a one-line text; the task files are several paragraphs.
    body = body.replace("**Task {kind}: {task text}.**", "**Task {kind}.**\n\n{task text}")
    return (body.replace("{sdk}", meta["sdk"]).replace("{kind}", meta["kind"])
            .replace("{pkg}", meta["package"]).replace("{commit}", meta.get("_commit", ""))
            .replace("{task text}", task_text.strip()))


def run_agent(cmd, prompt, timeout):
    start = now()
    try:
        r = subprocess.run(cmd, input=prompt, capture_output=True, text=True, timeout=timeout)
        stdout, stderr, code = r.stdout, r.stderr, r.returncode
    except subprocess.TimeoutExpired as e:
        stdout = e.stdout.decode(errors="replace") if isinstance(e.stdout, bytes) else (e.stdout or "")
        stderr, code = "timeout", -1
    return stdout, stderr, code, start, now()


def agent_sandbox(ctx, argv):
    env = dict(ctx["env"], CARGO_TARGET_DIR=ctx["target"])
    return sandbox_cmd([ctx["agent_root"], ctx["target"]], argv, ctx["src"], env, ctx["ro"], ctx["rw"])


def run_codex(prompt, ctx, effort, timeout, codex_sandbox):
    last = os.path.join(ctx["agent_root"], "last.md")
    argv = [ctx["exe"], "exec", "-m", CODEX_MODEL, "-c", f'model_reasoning_effort="{effort}"',
            "-c", "daybreak=false", "-s", codex_sandbox]
    if codex_sandbox == "workspace-write":
        argv += ["-c", "sandbox_workspace_write.network_access=true",
                 "-c", f'sandbox_workspace_write.writable_roots=["{ctx["target"]}"]']
    argv += ["-C", ctx["src"], "--skip-git-repo-check", "--json", "-o", last]
    cmd = agent_sandbox(ctx, argv)
    stdout, stderr, code, start, end = run_agent(cmd, prompt, timeout)
    open(os.path.join(ctx["out"], "events.jsonl"), "w").write(stdout)
    open(os.path.join(ctx["out"], "stderr.log"), "w").write(stderr or "")
    tid, usage = None, None
    for line in stdout.splitlines():
        try:
            e = json.loads(line)
        except ValueError:
            continue
        if e.get("type") == "thread.started":
            tid = e.get("thread_id")
        elif e.get("type") == "turn.completed":
            usage = e.get("usage")
    observed = {"model": "not visible", "effort": "not visible", "sandbox": "not visible",
                "rollout": None, "source": "rollout turn_context"}
    transcript = stdout
    if tid:
        hits = glob.glob(os.path.join(ctx["env"]["CODEX_HOME"], "sessions", "*", "*", "*", f"*{tid}*.jsonl"))
        if hits and not os.path.islink(hits[0]):
            kept = os.path.join(ctx["out"], "rollout.jsonl")
            keep_rollout(hits[0], kept)
            observed["rollout"] = kept
            transcript = open(hits[0], encoding="utf-8", errors="replace").read()
            for line in transcript.splitlines():
                try:
                    rec = json.loads(line)
                except ValueError:
                    continue
                if rec.get("type") == "turn_context":
                    p = rec.get("payload", {})
                    sp = p.get("sandbox_policy")
                    cm = (p.get("collaboration_mode") or {}).get("settings") or {}
                    observed.update({"model": p.get("model"),
                                     "effort": p.get("effort") or cm.get("reasoning_effort") or "not visible",
                                     "sandbox": sp.get("type") if isinstance(sp, dict) else sp})
                    break
    text = open(last, errors="replace").read() if os.path.isfile(last) and not os.path.islink(last) else ""
    open(os.path.join(ctx["out"], "last.md"), "w").write(text)
    return {"cmd": cmd, "start": start, "end": end, "exit": code, "final_message": text[-4000:],
            "thread_id": tid, "usage": usage, "observed": observed, "transcript": transcript,
            "requested": {"provider": "openai/codex", "model": CODEX_MODEL, "effort": effort,
                          "sandbox": codex_sandbox, **ctx["provider_extra"],
                          "outer_sandbox": "bubblewrap (run.py sandbox_cmd)"}}


ACCOUNT_FIELDS = ("creator_user_id", "creator_account_id", "account_id", "user_id")


def keep_rollout(src, dst):
    """Copy the rollout with the owner's account identifiers (session_meta) withheld."""
    with open(src, encoding="utf-8", errors="replace") as f, open(dst, "w", encoding="utf-8") as g:
        for line in f:
            try:
                rec = json.loads(line)
            except ValueError:
                g.write(line)
                continue
            payload = rec.get("payload")
            if rec.get("type") == "session_meta" and isinstance(payload, dict):
                for k in ACCOUNT_FIELDS:
                    if k in payload:
                        payload[k] = "<withheld>"
                line = json.dumps(rec) + "\n"
            g.write(line)


def run_claude(prompt, ctx, model, effort, timeout):
    argv = [ctx["exe"], "-p", "--output-format", "json", "--allowedTools", " ".join(CLAUDE_TOOLS),
            "--tools", ",".join(CLAUDE_TOOLS), "--permission-mode", "acceptEdits", "--safe-mode",
            "--strict-mcp-config", "--no-session-persistence"]
    if model:
        argv += ["--model", model]
    if effort:
        argv += ["--effort", effort]
    cmd = agent_sandbox(ctx, argv)
    stdout, stderr, code, start, end = run_agent(cmd, prompt, timeout)
    open(os.path.join(ctx["out"], "result_raw.json"), "w").write(stdout)
    open(os.path.join(ctx["out"], "stderr.log"), "w").write(stderr or "")
    result = {}
    try:
        result = json.loads(stdout)
    except ValueError:
        for line in stdout.splitlines()[::-1]:
            try:
                result = json.loads(line)
                break
            except ValueError:
                continue
    usage = result.get("modelUsage") or {}
    text = result.get("result") or ""
    open(os.path.join(ctx["out"], "last.md"), "w").write(text)
    observed = {"models": sorted(usage.keys()) or "not visible", "effort": "not visible",
                "source": "claude -p JSON result: modelUsage keys"}
    return {"cmd": cmd, "start": start, "end": end, "exit": code, "final_message": text[-4000:],
            "session_id": result.get("session_id"), "num_turns": result.get("num_turns"),
            "is_error": result.get("is_error"), "cost_usd": result.get("total_cost_usd"),
            "usage": result.get("usage"), "observed": observed, "transcript": stdout,
            "requested": {"provider": "anthropic/claude-code", "model": model or "default (not passed)",
                          "effort": effort or "default (not passed)",
                          "outer_sandbox": "bubblewrap (run.py sandbox_cmd)"}}


def run_replay(ctx, patch, shell):
    """Instrument validation: apply PATCH and/or run CMD inside the agent's sandbox (as an agent
    would), instead of an agent."""
    script = []
    if patch:
        kept = os.path.join(ctx["agent_root"], "replay.patch")
        shutil.copy(patch, kept)
        script.append(f"git apply --whitespace=nowarn {kept}")
    if shell:
        script.append(shell)
    code, text = sh(agent_sandbox(ctx, ["/bin/sh", "-ec", "\n".join(script)]), timeout=600)
    open(os.path.join(ctx["out"], "replay.log"), "w").write(text)
    if code:
        raise RuntimeError(f"replay failed ({code}): {text[-2000:]}")
    return {"replay": os.path.abspath(patch) if patch else None, "replay_shell": shell,
            "transcript": "", "observed": "replay: no agent ran", "requested": "replay: no agent ran"}


# ------------------------------------------------------------------ reading the agent's tree

def git_manifest(src):
    """`.git/**` of the agent's tree as {relative path: (kind, digest)}; read only."""
    root = os.path.join(src, ".git")
    if os.path.islink(root) or not os.path.isdir(root):
        return {"": ("not a directory", os.readlink(root) if os.path.islink(root) else
                     ("file" if os.path.exists(root) else "absent"))}
    out = {}
    for dirpath, dirnames, filenames in os.walk(root):
        for name in filenames + [d for d in dirnames if os.path.islink(os.path.join(dirpath, d))]:
            p = os.path.join(dirpath, name)
            rel = os.path.relpath(p, root).replace(os.sep, "/")
            st = os.lstat(p)
            if stat.S_ISLNK(st.st_mode):
                out[rel] = ("link", os.readlink(p))
            elif stat.S_ISREG(st.st_mode):
                try:
                    out[rel] = ("file", sha256_file(p))
                except OSError as e:
                    out[rel] = ("unreadable", str(e))
            else:
                out[rel] = ("other", oct(st.st_mode))
    return out


def git_changes(src, before, after):
    """Classify the agent's `.git` changes: normal (commits, index, branches, stash) or not; scan the
    files it created or changed (loose objects decompressed) for bank paths."""
    root = os.path.join(src, ".git")
    added = sorted(set(after) - set(before))
    removed = sorted(set(before) - set(after))
    modified = sorted(k for k in set(before) & set(after) if before[k] != after[k])
    beyond, hits = [], set()
    for rel in removed:
        beyond.append(f"removed .git/{rel}")
    for rel in added + modified:
        kind = after[rel][0]
        if rel == "":
            beyond.append(f".git replaced ({after[rel]})")
            continue
        p = os.path.join(root, rel)
        if kind != "file":
            beyond.append(f"{'added' if rel in added else 'changed'} .git/{rel} ({kind})")
            continue
        try:
            data = open(p, "rb").read(64 << 20)
        except OSError:
            beyond.append(f"unreadable .git/{rel}")
            continue
        if rel.startswith("objects/") and GIT_NORMAL.match(rel):
            try:
                data = zlib.decompressobj().decompress(data, 256 << 20)
            except zlib.error:
                beyond.append(f"unreadable loose object .git/{rel}")
        hits |= set(LEAK.findall(data.decode("latin-1")))
        if rel == "HEAD":
            if not GIT_HEAD.match(data.decode("latin-1")):
                beyond.append("HEAD no longer names a branch or a commit")
        elif not GIT_NORMAL.match(rel):
            beyond.append(f"{'added' if rel in added else 'changed'} .git/{rel}")
    return {"added": len(added), "modified": len(modified), "removed": len(removed),
            "normal": not beyond, "beyond_normal": beyond[:50], "leak_hits": sorted(hits)}


def is_cargo_target(path):
    return os.path.isfile(os.path.join(path, "CACHEDIR.TAG")) and not os.path.islink(path)


def copy_agent_tree(src, dst, skip_root_files=("EDIT_REPORT.md",)):
    """Copy the agent's tree to `dst` without reading it through anything the agent configures:
    no `.git` (at any depth), no `.cargo` (at any depth, whatever its type), no cargo target
    directory; symbolic links copied as links, special files skipped. Returns what was left out."""
    left_out = {"git": [], "cargo": [], "target_dirs": [], "special": [], "symlinks": []}

    def walk(s, d, rel):
        os.makedirs(d, exist_ok=True)
        with os.scandir(s) as it:
            entries = sorted(it, key=lambda e: e.name)
        for e in entries:
            r = f"{rel}/{e.name}" if rel else e.name
            if not rel and e.name in skip_root_files:
                continue
            if e.name == ".git":
                left_out["git"].append(r)
                continue
            if e.name == ".cargo":
                left_out["cargo"].append(r)
                continue
            st = os.lstat(e.path)
            if stat.S_ISDIR(st.st_mode):
                if is_cargo_target(e.path):
                    left_out["target_dirs"].append(r)
                    continue
                walk(e.path, os.path.join(d, e.name), r)
            elif stat.S_ISREG(st.st_mode):
                try:
                    # Fresh modification times (copyfile, not copy2): the measurement target is
                    # shared between runs and cargo's freshness check is by mtime, so a file older
                    # than an earlier run's build would let that build (another tree's code) be
                    # reused. Every copied file is newer than any earlier artifact.
                    shutil.copyfile(e.path, os.path.join(d, e.name), follow_symlinks=False)
                    shutil.copymode(e.path, os.path.join(d, e.name), follow_symlinks=False)
                except OSError:
                    left_out["special"].append(r + " (unreadable)")
            elif stat.S_ISLNK(st.st_mode):
                os.symlink(os.readlink(e.path), os.path.join(d, e.name))
                left_out["symlinks"].append(r)
            else:
                left_out["special"].append(r)

    walk(src, dst, "")
    return left_out


def cargo_files(src):
    """Every file under any `.cargo` directory of the agent's tree (outside `.git` and target dirs)."""
    found = []
    for dirpath, dirnames, filenames in os.walk(src):
        rel = os.path.relpath(dirpath, src)
        dirnames[:] = [d for d in dirnames if d != ".git" and not is_cargo_target(os.path.join(dirpath, d))]
        parts = [] if rel == "." else rel.split(os.sep)
        if ".cargo" in parts:
            found += [os.path.join(rel, f) for f in filenames]
        for d in dirnames:
            if d == ".cargo" and os.path.islink(os.path.join(dirpath, d)):
                found.append(os.path.join(rel, d) + " (symlink)")
        if ".cargo" in filenames:
            found.append(os.path.join(rel, ".cargo") + " (file)")
    return sorted(found)


def nested_git(src):
    found = []
    for dirpath, dirnames, filenames in os.walk(src):
        rel = os.path.relpath(dirpath, src)
        if rel != "." and (".git" in dirnames or ".git" in filenames):
            found.append(os.path.join(rel, ".git"))
        dirnames[:] = [d for d in dirnames if d != ".git" and not is_cargo_target(os.path.join(dirpath, d))]
    return sorted(found)


def toolchain_changes(src, baseline):
    changed = []
    for name in TOOLCHAIN_FILES:
        a, b = os.path.join(baseline, name), os.path.join(src, name)
        ra = open(a, "rb").read() if os.path.isfile(a) else None
        try:
            rb = open(b, "rb").read() if os.path.isfile(b) and not os.path.islink(b) else (
                b"<symlink>" if os.path.islink(b) else None)
        except OSError:
            rb = b"<unreadable>"
        if ra != rb:
            changed.append(name)
    return changed


def baseline_toolchain(baseline):
    try:
        return tomllib.load(open(os.path.join(baseline, "rust-toolchain.toml"), "rb"))["toolchain"]["channel"]
    except (OSError, KeyError, tomllib.TOMLDecodeError):
        return None


# ------------------------------------------------------------------ measurements

def tree_files(root, pattern):
    hits = []
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if d not in ("target", ".git")]
        for f in filenames:
            p = os.path.join(dirpath, f)
            if os.path.islink(p):
                continue
            rel = os.path.relpath(p, root)
            if fnmatch.fnmatch(rel, pattern) or ("/**/" in pattern and
                                                 fnmatch.fnmatch(rel, pattern.replace("/**/", "/"))):
                hits.append(rel)
    return sorted(hits)


def count_matches(sdk_root, checks):
    rows = []
    for c in checks:
        n, where = 0, []
        for rel in tree_files(sdk_root, c["glob"]):
            try:
                text = open(os.path.join(sdk_root, rel), encoding="utf-8").read()
            except (UnicodeDecodeError, OSError):
                continue
            k = len(re.findall(c["regex"], text))
            if k:
                n += k
                where.append(f"{rel}:{k}")
        rows.append({"class": c.get("class"), "meaning": c["meaning"], "regex": c["regex"],
                     "glob": c["glob"], "count": n, "where": where})
    return rows


def read(sdk_root, rel):
    p = os.path.join(sdk_root, rel)
    if os.path.islink(p) or not os.path.isfile(p):
        return ""
    return open(p, encoding="utf-8", errors="replace").read()


def search_area(site):
    """Where a fact may legitimately be stated, by the site's role: SDK source for code facts,
    tests and fixtures for test expectations, any text file of the package for docs."""
    f = site["file"]
    if site["role"] == "test-expectation":
        return ["tests/**/*.rs", "tests/*.rs", "fixtures/**/*", "fixtures/*"]
    if site["role"] == "docs":
        return ["src/**/*.rs", "src/*.rs", "*.md", "docs/**/*.md"]
    if f.startswith("fixtures/"):
        return ["fixtures/**/*", "fixtures/*"]
    return ["src/**/*.rs", "src/*.rs"]


# Roles whose fact is about one place (a classification table, a second site that must follow):
# the same name elsewhere in the SDK does not satisfy them, so only the reference file and the
# declared alternatives are read.
SITE_BOUND_ROLES = ("classification", "propagation")
# Roles whose fact is behaviour the oracle exercises: when the pattern does not recognise one but
# the oracle passed, the fact may be stated in a spelling the pattern misses (needs review).
BEHAVIOURAL_ROLES = ("authoritative", "classification", "propagation")
NOT_STATED = "not stated"
NEEDS_REVIEW = "needs review (not recognised by the pattern, but the oracle passed: read agent.patch)"


def check_sites(sdk_root, sites, changed):
    """Classify every named provider fact by where the agent's final tree states it."""
    rows = []
    for s in sites:
        if s["role"] in SITE_BOUND_ROLES and not s.get("any_file"):
            files = [f for f in [s["file"]] + s.get("alternatives", []) if os.path.isfile(os.path.join(sdk_root, f))]
        else:
            files = sorted({rel for pat in search_area(s) for rel in tree_files(sdk_root, pat)})
        found = [rel for rel in files if s.get("after") and re.search(s["after"], read(sdk_root, rel))]
        stale = [rel for rel in files if s.get("before") and re.search(s["before"], read(sdk_root, rel))]
        ref = s["file"]
        alts = s.get("alternatives", [])
        if s.get("any_file"):
            where = "reference site" if found else NOT_STATED
        elif ref in found:
            where = "reference site"
        elif any(a in found for a in alts):
            where = "declared alternative site"
        elif found:
            where = "elsewhere (undeclared location: read agent.patch)"
        else:
            where = NOT_STATED
        old_at_ref = bool(s.get("before")) and (ref in stale or (s.get("any_file") and bool(stale)))
        rows.append({"fact": s["fact"], "role": s["role"], "reference_file": ref,
                     "declared_alternatives": alts, "where": where,
                     "new_fact_in": found, "new_fact_in_changed_files": [f for f in found if f in changed],
                     "old_fact_in": stale, "old_fact_at_reference_site": old_at_ref,
                     # kept for comparison with earlier results: the reference-site-only reading
                     "satisfied_at_reference_site": ref in found and not old_at_ref if not s.get("any_file") else bool(found) and not stale,
                     "stated": where != NOT_STATED and not old_at_ref})
    return rows


def review_unrecognised(rows, oracle_passed):
    """An unrecognised behavioural fact is `needs review` when the oracle passed (behaviour
    preserved, spelling unknown to the pattern), and stays `not stated` when it did not."""
    for r in rows:
        if r["where"] == NOT_STATED and oracle_passed and r["role"] in BEHAVIOURAL_ROLES:
            r["where"] = NEEDS_REVIEW
            r["stated"] = False
            r["needs_review"] = True
    return rows


def tree_diff(ctx):
    """`git diff --no-index` between the immutable baseline and the measurement copy, inside
    bubblewrap with a trusted git environment; paths rewritten to `a/<path>` / `b/<path>`."""
    run_dir = ctx["run_dir"]
    git, env = trusted_git(ctx["git_home"])
    env["GIT_CEILING_DIRECTORIES"] = os.path.dirname(run_dir)
    argv = git + ["diff", "--no-index", "--no-renames", "--no-color", "--no-ext-diff",
                  "--no-textconv", "--", "baseline", "measure"]
    cmd = sandbox_cmd([ctx["git_home"]], argv, run_dir, env,
                      ro=[ctx["baseline"], ctx["measure_dir"]], network=False)
    r = subprocess.run(cmd, capture_output=True, text=True, errors="replace", timeout=900)
    if r.returncode not in (0, 1):
        raise RuntimeError(f"git diff --no-index failed ({r.returncode}): {r.stderr[-2000:]}")
    header = re.compile(r'(^|\s|")([ab])/(?:baseline|measure)/')
    lines = []
    for ln in r.stdout.splitlines():
        if ln.startswith(("diff --git ", "--- ", "+++ ")):
            ln = header.sub(r"\1\2/", ln)
        lines.append(ln)
    patch = "\n".join(lines) + ("\n" if lines else "")
    files, cur, hunks, plus, minus = {}, None, 0, 0, 0
    for ln in lines:
        if ln.startswith("diff --git "):
            m = re.match(r'diff --git "?a/(.+?)"? "?b/(.+?)"?$', ln)
            cur = m.group(2) if m else ln
            files.setdefault(cur, 0)
        elif ln.startswith("@@") and cur:
            files[cur] += 1
            hunks += 1
        elif ln.startswith("+") and not ln.startswith("+++ "):
            plus += 1
        elif ln.startswith("-") and not ln.startswith("--- "):
            minus += 1
    stat_line = f"{len(files)} files changed, {plus} insertions(+), {minus} deletions(-)"
    return patch, {"files": files, "hunks": hunks, "stat": [stat_line] if files else []}


def cargo(ctx, args, log):
    """One cargo command on the measurement copy, in the sandbox, network unshared, offline, with
    the trusted CARGO_HOME and the baseline's toolchain."""
    env = {"CARGO_NET_OFFLINE": "true", "CARGO_TARGET_DIR": ctx["measure"],
           "CARGO_HOME": os.path.join(HOME, ".cargo")}
    if ctx.get("toolchain"):
        env["RUSTUP_TOOLCHAIN"] = ctx["toolchain"]
    cmd = sandbox_cmd([ctx["measure_dir"], ctx["measure"]], ["cargo"] + args, ctx["measure_dir"], env,
                      network=False)
    return sh(cmd, log=log, timeout=3600)


def reference_counts(ctx, reference_text, sdk_dir, checks):
    """Propagation counts in the baseline plus `reference.patch` (applied in a copy, in the sandbox)."""
    ref_dir = ctx["reference_dir"]
    shutil.copytree(ctx["baseline"], ref_dir, symlinks=True)
    set_writable(ref_dir, True)
    git, env = trusted_git(ctx["git_home"])
    env["GIT_CEILING_DIRECTORIES"] = os.path.dirname(ref_dir)
    cmd = sandbox_cmd([ref_dir, ctx["git_home"]], git + ["apply", "-"], ref_dir, env, network=False)
    code, out = sh(cmd, stdin=reference_text, timeout=600)
    if code:
        return None
    return count_matches(os.path.join(ref_dir, sdk_dir), checks)


def measure(ctx, task, result):
    meta, kind = task["meta"], task["meta"]["kind"]
    out, mdir = ctx["out"], ctx["measure_dir"]
    sdk_root = os.path.join(mdir, meta["sdk_dir"])
    patch, stats = tree_diff(ctx)
    open(os.path.join(out, "agent.patch"), "w").write(patch)
    changed = [f[len(meta["sdk_dir"]) + 1:] for f in stats["files"] if f.startswith(meta["sdk_dir"] + "/")]
    outside = [f for f in stats["files"] if not f.startswith(meta["sdk_dir"] + "/")]
    build_scripts = [f for f in stats["files"] if f.endswith("build.rs") or f.endswith("Cargo.toml")]
    result["diagnostics"] = {"diff": stats, "files_outside_sdk": outside,
                             "build_or_manifest_changed": build_scripts,
                             "note": "hunks and files are diagnostics only (PLAN_AMENDMENT_A1 §6)"}
    sites = check_sites(sdk_root, meta.get("expected_sites", []), changed)
    prop = count_matches(sdk_root, meta.get("propagation_checks", []))

    code, text = cargo(ctx, ["test", "-p", meta["package"], "--no-fail-fast"],
                       os.path.join(out, "cargo-existing.log"))
    rows = targets(text)
    built = "could not compile" not in text
    result["existing_tests"] = {"built": built, "passed": code == 0 and bool(rows) and all(r["ok"] for r in rows),
                                "exit": code, "targets": [{k: v for k, v in r.items() if k != "tests"} for r in rows],
                                "tests_passed": sum(r["passed"] for r in rows),
                                "tests_failed": sum(r["failed"] for r in rows)}
    if not result["existing_tests"]["passed"]:
        result["existing_tests"]["first_error"] = first_error(text)
    test_name = f"oracle_{kind}"
    with open(os.path.join(sdk_root, "tests", f"{test_name}.rs"), "w", encoding="utf-8") as f:
        f.write(task["oracle"])
    code, text = cargo(ctx, ["test", "-p", meta["package"], "--test", test_name, "--", "--format", "pretty"],
                       os.path.join(out, "cargo-oracle.log"))
    listed = None
    if "could not compile" not in text:
        lcode, ltext = cargo(ctx, ["test", "-p", meta["package"], "--test", test_name, "--", "--list"],
                             os.path.join(out, "cargo-oracle-list.log"))
        listed = parse_listing(ltext) if lcode == 0 else []
    verdict = oracle_verdict(task["oracle_tests"], text, code, listed)
    result["oracle"] = verdict
    if not verdict["pass"]:
        result["oracle"]["first_error"] = first_error(text)
    status = verdict["status"]
    result["edit_sites"] = review_unrecognised(sites, verdict["pass"])

    cls = meta.get("classification")
    if kind == "e4" and cls:
        chk = cls.get("check")
        present = None
        if chk:
            present = bool(re.search(chk["regex"], read(sdk_root, chk["file"])))
        row = {"applies": cls.get("applies"), "reference_site": cls.get("site"),
               "classification_site_present": present,
               "compiler_catches_omission_by_design": cls.get("compiler_catches_omission")}
        if present and status == "passed":
            row["verdict"] = "classified (reference site present, oracle passed)"
        elif present:
            row["verdict"] = ("classification site present but the oracle fails: a wrong or partial "
                              "classification, or another defect (read the oracle failures)")
        elif present is False and status == "passed":
            row["needs_review"] = True
            row["verdict"] = ("needs review: the reference classification site is not recognised, but "
                              "the oracle passed (a behaviour-preserving classification elsewhere or in "
                              "another spelling; read agent.patch)")
        elif present is False:
            if not result["existing_tests"]["built"]:
                row["omission_caught_by"] = "compiler (SDK build)"
            elif not result["existing_tests"]["passed"]:
                row["omission_caught_by"] = "existing tests (or the agent's own tests)"
            elif status == "compile_error":
                row["omission_caught_by"] = "compiler at the caller (hidden oracle compile)"
            else:
                row["omission_caught_by"] = "not by the compiler or existing tests: hidden oracle only"
            row["verdict"] = "classification site absent (fact missing); caught by: " + row["omission_caught_by"]
        result["classification"] = row

    base = count_matches(os.path.join(ctx["baseline"], meta["sdk_dir"]), meta.get("propagation_checks", []))
    ref = reference_counts(ctx, open(task["reference"], encoding="utf-8").read(), meta["sdk_dir"],
                           meta.get("propagation_checks", []))
    for i, row in enumerate(prop):
        row["base_count"] = base[i]["count"]
        row["reference_count"] = ref[i]["count"] if ref else None
        row["missed"] = row["count"] > (row["reference_count"] or 0)
    result["propagation"] = prop
    result["missed_propagation"] = [r["meaning"] for r in prop if r["missed"]]
    return patch


def exposure(transcript, patch, files):
    hits = set(LEAK.findall(transcript or ""))
    tree = set(LEAK.findall(patch or "")) | {f for f in files if LEAK.search(f)}
    return sorted(hits), sorted(tree)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--task", required=True)
    ap.add_argument("--provider", choices=["codex", "claude"], default="codex")
    ap.add_argument("--commit", required=True)
    ap.add_argument("--out", default=None, help="default: <run dir>/out")
    ap.add_argument("--model", default=None, help="claude only (codex is always gpt-6.1-sol)")
    ap.add_argument("--effort", default=None, help="codex default xhigh; claude: not passed unless set")
    ap.add_argument("--codex-sandbox", default="danger-full-access",
                    help="codex -s value inside the bubblewrap sandbox (Codex's own Linux sandbox "
                         "cannot nest inside bubblewrap here; see the module docstring)")
    ap.add_argument("--repo", default=DEFAULT_REPO)
    ap.add_argument("--bank", default=DEFAULT_BANK, help="the sealed bank (layout tasks/<sdk>/<kind>/)")
    ap.add_argument("--agent-target", default=None,
                    help="agent CARGO_TARGET_DIR (default: a fresh per-run dir, removed afterwards)")
    ap.add_argument("--measure-target", default=os.path.join(WS, "work", "targets", "i4-measure"))
    ap.add_argument("--timeout", type=int, default=4 * 3600)
    ap.add_argument("--keep", action="store_true", help="keep the run's trees and the per-run agent target")
    ap.add_argument("--probe-only", action="store_true", help="export, run the isolation probe, stop")
    ap.add_argument("--deep", action="store_true", help="probe: also search the visible disk for a bank marker")
    ap.add_argument("--replay", default=None, metavar="PATCH",
                    help="instrument validation: apply PATCH in the agent's sandbox (no agent runs), "
                         "then measure exactly as after a session")
    ap.add_argument("--replay-shell", default=None, metavar="CMD",
                    help="instrument validation: run CMD with /bin/sh in the agent's sandbox (after "
                         "--replay, if both), then measure")
    ap.add_argument("--label", default=None, help="appended to the run id")
    a = ap.parse_args()

    bank = os.path.abspath(a.bank)
    digest, nfiles, ntasks = bank_hash(bank)
    recorded = recorded_bank_hash()
    if digest != recorded:
        sys.exit(f"the sealed bank {bank} hashes to {digest} ({nfiles} files), but BANK.sha256 records "
                 f"{recorded}: refusing to run against an unrecorded bank")
    task = load_task(a.task, bank)
    meta = task["meta"]
    sdk, kind = a.task.split("/")
    # The task file's own heading names the kind; the prompt already states it.
    task_text = "\n".join(ln for ln in task["task_text"].splitlines() if not ln.startswith("# ")).strip()
    meta["_commit"] = a.commit
    repo = os.path.abspath(a.repo)
    sha = sh(["git", "-C", repo, "rev-parse", "--verify", a.commit + "^{commit}"])[1].strip()
    stamp = datetime.datetime.now().strftime("%Y%m%d%H%M%S")
    replaying = bool(a.replay or a.replay_shell)
    run_id = (f"{sdk}-{kind}-{'replay' if replaying else a.provider}-{stamp}"
              + ("-probe" if a.probe_only else "") + (f"-{a.label}" if a.label else ""))
    run_dir = os.path.join(RUNS, run_id)
    agent_root = os.path.join(run_dir, "agent")
    src = os.path.join(agent_root, "src")
    per_run_target = a.agent_target is None
    target = a.agent_target or os.path.join(WS, "work", "targets", "i4-agent-runs", run_id)
    out = os.path.abspath(a.out or os.path.join(run_dir, "out"))
    if out.startswith(agent_root + os.sep) or out == agent_root:
        sys.exit("--out must not be inside the agent's writable directory")
    for d in (agent_root, target, a.measure_target, out):
        os.makedirs(d, exist_ok=True)
    result = {"task": a.task, "provider": a.provider, "commit": sha, "run_dir": run_dir,
              "package": meta["package"], "sdk_dir": meta["sdk_dir"], "started": now(),
              "agent_target": target, "measure_target": a.measure_target,
              "bank": {"path": bank, "sha256": digest, "files": nfiles, "tasks": ntasks,
                       "matches_BANK.sha256": True}}
    effort = a.effort or ("xhigh" if a.provider == "codex" else None)
    ctx = {"run_dir": run_dir, "agent_root": agent_root, "src": src, "target": target,
           "baseline": os.path.join(run_dir, "baseline"), "measure_dir": os.path.join(run_dir, "measure"),
           "reference_dir": os.path.join(run_dir, "reference"), "git_home": os.path.join(run_dir, "git-home"),
           "measure": a.measure_target, "out": out, "bank": bank}
    try:
        export_source(repo, sha, ctx["baseline"], src, ctx["git_home"])
        ctx["toolchain"] = baseline_toolchain(ctx["baseline"])
        exe, ro, rw, env, extra = provider_setup(a.provider, agent_root, effort)
        ctx.update({"exe": exe, "ro": ro, "rw": rw, "env": env, "provider_extra": extra})
        probe = run_probe(ctx, deep=a.deep, repo=repo)
        open(os.path.join(out, "isolation.txt"), "w").write(probe["output"])
        result["isolation_check"] = {k: v for k, v in probe.items() if k != "output"}
        print(probe["output"])
        print(json.dumps(result["isolation_check"], indent=1))
        if a.probe_only or not probe["ok"]:
            if not probe["ok"]:
                result["outcome"] = {"status": "not run", "reason": "isolation check failed"}
            return
        prompt = fill_prompt(meta, task_text)
        open(os.path.join(out, "prompt.md"), "w").write(prompt)
        git_before = git_manifest(src)
        if replaying:
            agent = run_replay(ctx, a.replay and os.path.abspath(a.replay), a.replay_shell)
        elif a.provider == "codex":
            agent = run_codex(prompt, ctx, effort, a.timeout, a.codex_sandbox)
        else:
            agent = run_claude(prompt, ctx, a.model, effort, a.timeout)
        transcript = agent.pop("transcript", "")
        result["agent"] = agent

        # From here on the agent's tree is only read on the host; nothing it configures runs.
        report = os.path.join(src, "EDIT_REPORT.md")
        result["edit_report_written"] = os.path.isfile(report) and not os.path.islink(report)
        if result["edit_report_written"]:
            shutil.copyfile(report, os.path.join(out, "EDIT_REPORT.md"))
        gitc = git_changes(src, git_before, git_manifest(src))
        tamper = {"git": gitc, "cargo_files": cargo_files(src), "nested_git": nested_git(src),
                  "toolchain_files_changed": toolchain_changes(src, ctx["baseline"])}
        tamper["left_out_of_measurement"] = copy_agent_tree(src, ctx["measure_dir"])
        if os.path.exists(os.path.join(ctx["measure_dir"], ".cargo")):
            raise RuntimeError("the measurement copy still has a .cargo directory")
        signals = ([f".git: {x}" for x in gitc["beyond_normal"]]
                   + [f".cargo: {x}" for x in tamper["cargo_files"]]
                   + [f"nested .git: {x}" for x in tamper["nested_git"]]
                   + [f"toolchain file changed: {x}" for x in tamper["toolchain_files_changed"]]
                   + [f"special file: {x}" for x in tamper["left_out_of_measurement"]["special"]])
        tamper["signals"] = signals
        result["tampering"] = tamper

        patch = measure(ctx, task, result)
        in_transcript, in_tree = exposure(transcript, patch, result["diagnostics"]["diff"]["files"])
        result["exposure"] = {"transcript_hits": in_transcript, "tree_hits": in_tree,
                              "git_hits": gitc["leak_hits"], "pattern": LEAK.pattern}
        sites = result["edit_sites"]
        scored = {
            "oracle_pass": result["oracle"]["pass"],
            "oracle_status": result["oracle"]["status"],
            "oracle_tests_executed_ok": f"{len(result['oracle']['reported_ok'])}/{len(task['oracle_tests'])}",
            "existing_tests_pass": result["existing_tests"]["passed"],
            "facts": {w: sum(1 for s in sites if s["where"] == w) for w in sorted({s["where"] for s in sites})},
            "facts_stated": f"{sum(1 for s in sites if s['stated'])}/{len(sites)}",
            "facts_needing_review": [s["fact"] for s in sites if s.get("needs_review")],
            "facts_not_stated": [s["fact"] for s in sites if s["where"] == NOT_STATED],
            "old_fact_left_at_reference_site": [s["fact"] for s in sites if s["old_fact_at_reference_site"]],
            "missed_propagation": len(result["missed_propagation"]),
            "classification": result.get("classification", {}).get("verdict"),
        }
        common = {"observed_model": agent["observed"], "requested": agent["requested"]}
        reasons = []
        if in_transcript or in_tree or gitc["leak_hits"]:
            reasons.append("the transcript, the tree or its .git shows access to a bank file")
        if signals:
            reasons.append("tampering: " + "; ".join(signals[:10]))
        if reasons:
            result["unscored"] = scored
            result["outcome"] = {"status": "invalid", "scored": False, "reason": " | ".join(reasons),
                                 **common}
        else:
            result["outcome"] = {"status": "valid", "scored": True, **scored, **common}
    finally:
        result["finished"] = now()
        json.dump(result, open(os.path.join(out, "result.json"), "w"), indent=1, sort_keys=True)
        if not a.keep:
            for d in (ctx["baseline"], ctx["reference_dir"]):
                if os.path.isdir(d):
                    set_writable(d, True)
            for d in (agent_root, ctx["measure_dir"], ctx["reference_dir"], ctx["baseline"], ctx["git_home"]):
                shutil.rmtree(d, ignore_errors=True)
            if per_run_target:
                shutil.rmtree(target, ignore_errors=True)
    print(json.dumps(result.get("outcome", {}), indent=1))


if __name__ == "__main__":
    main()
