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
   `.cargo/` and any cargo target directory (`CACHEDIR.TAG`) into `<run>/measure`: regular files
   and directories only (a symbolic link or special file is left out and makes the session
   invalid), every file with a fresh modification time (the measurement target is shared between
   runs and cargo decides freshness by mtime: an older file would let an earlier run's build be
   reused). Every command on that copy runs inside bubblewrap with the network unshared: the diff
   is `git diff --no-index` between the immutable baseline and the copy under a trusted git
   environment (`GIT_CONFIG_NOSYSTEM=1`, `GIT_CONFIG_GLOBAL=/dev/null`, a scratch `HOME`,
   `-c core.fsmonitor=false -c diff.external= -c core.hooksPath=/dev/null`, `--no-ext-diff
   --no-textconv`); cargo runs with the copy bound **read-only** (only the measurement target
   directory is writable; one `cargo metadata` sees it writable to refresh `Cargo.lock`), the
   owner's `CARGO_HOME` (read-only in the agent's sandbox) and the baseline's toolchain pinned
   through `RUSTUP_TOOLCHAIN`.
6. **Exposure and tampering → invalid.** The transcript, the agent's diff, its changed paths and
   the `.git` files it created or changed (loose objects decompressed) are scanned for paths only
   the hidden bank has (`LEAK`). Tampering signals: a `.git` change beyond normal commits and index
   updates (`GIT_NORMAL`), any `.cargo/` file, a changed toolchain file, a nested `.git`, a
   symbolic link or special file, a refused oracle installation or an oracle changed during the
   measurement, build-time code or a changed measurement harness (`build_time_code`), and a
   harness-forging pattern in the added lines (`harness_scan`; its ambiguous hits make the status
   `needs review` instead). Any hit sets `outcome.status = "invalid"`; nothing is scored (the
   measurements are kept under `unscored`).
7. **Measure** (cargo one command at a time):
   - the diff (files, hunks, stat: diagnostics only);
   - **authoritative semantic edit sites** (`expected_sites`): per named provider fact, where the
     final tree states it -- reference site, declared alternative, elsewhere in the SDK, or not
     recognised; a behavioural fact (every role but `docs`) counts only where code states it
     (comments and rustdoc blanked, `code_text`); an unrecognised behavioural fact is `needs
     review` when the oracle passed (the pattern may miss a behaviour-preserving spelling) and
     `not stated` otherwise;
   - **missed propagation** (`propagation_checks`), next to the counts for the baseline and the
     reference solution;
   - the SDK's existing tests (and the agent's own): `cargo test -p <pkg> --no-fail-fast`;
   - the **hidden oracle**: written to `examples/<sdk>-sdk/tests/oracle_<kind>.rs` after the
     existing tests (`install_oracle`: no link is followed, an existing path is refused), run with
     `cargo test -p <pkg> --test oracle_<kind> -- --format pretty` and listed with `-- --list`;
     it passes only if every oracle test named in the sealed source is reported as executed and
     `ok`, nothing else ran and the summary counts agree (exit status alone is not enough); its
     hash is checked again afterwards;
   - for e4, the **classification**: whether the tree's code (comments and rustdoc blanked)
     contains the reference's classification site; when it does not, the stage that caught the
     omission, attributed only on the task's classification-specific evidence naming the new
     operation, both inside one failure block (`omission_stage`; else `cannot be assessed`), or
     `needs review` when the oracle passed.
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
# The measurement harness the oracles build on: the loopback test server crate (every oracle) and
# the registry crates an oracle uses for its own expectations. The agent's diff must not change the
# crate's files, nor redirect any of these crates to another source (`path`, `git`, ...).
HARNESS_DIRS = ("examples/testserver/",)
HARNESS_CRATES = {"concord-example-testserver", "concord-testserver", "serde_json", "tokio", "base64",
                  "hmac", "sha2", "bytes"}
DEP_TABLES = ("dependencies", "dev-dependencies", "dev_dependencies")
BUILD_DEP_TABLES = ("build-dependencies", "build_dependencies")
SOURCE_KEYS = ("path", "git", "registry", "registry-index", "branch", "tag", "rev")
# Static scan of the agent's added lines for code that could forge the test harness's verdict
# (the oracle's assertions run in the candidate's process; see the README's residual risk).
FORGE = re.compile(r"test result:|\.\.\. (?:ok|FAILED|ignored)\b|running (?:\d+|\{[^}]*\}) tests?\b")
STDIO = re.compile(r"\b(?:e?print(?:ln)?|dbg)!\s*\(|\bio::(?:stdout|stderr)\b|\b(?:stdout|stderr)\(\)|"
                   r"/dev/(?:stdout|stderr|tty|fd/)|/proc/self/fd|\bfrom_raw_fd\b|\bFromRawFd\b|"
                   r"\blibc::(?:write|dup2?|fdopen)\b")
EXIT = re.compile(r"\bprocess::(?:exit|abort)\b|\buse\s+std::process::(?:\*|\{[^}]*\b(?:exit|abort)\b|"
                  r"(?:exit|abort)\b)|\blibc::(?:_exit|exit|abort|kill|raise)\b|\bintrinsics::abort\b")
BEFORE_MAIN = re.compile(r"\blink_section\b|\.init_array|#\[\s*used\s*\]|\bno_mangle\b|\bexport_name\b|"
                         r"\bglobal_asm!|\basm!\s*\(")


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
            "-c", "commit.gpgsign=false", "-c", "core.quotepath=false", "-c", "gc.auto=0"]
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
    # gc.auto=0 in the export's own config: a tree past git's loose-object threshold (≈ 6,700) made the export
    # commit start a detached `gc --auto` that packed objects during the agent's session, which the tamper check
    # then reported as removed `.git` objects (LF holdout, all 40 runs at f178e63d). No other git behaviour changes.
    for cmd in (["init", "-q", "-b", "base"], ["config", "gc.auto", "0"], ["add", "-A"],
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
    directory. Only regular files and directories are copied: symbolic links and special files are
    left out and recorded (each one makes the session invalid), so the measurement copy contains
    no link a later host-side write could follow. Returns what was left out."""
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
                left_out["symlinks"].append(f"{r} -> {os.readlink(e.path)}")
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


def links_and_special(root):
    """Every symbolic link and special file under `root` (lstat only; links are not followed)."""
    found = []
    for dirpath, dirnames, filenames in os.walk(root):
        for name in dirnames + filenames:
            p = os.path.join(dirpath, name)
            mode = os.lstat(p).st_mode
            if stat.S_ISLNK(mode):
                found.append(f"{os.path.relpath(p, root)} -> {os.readlink(p)}")
            elif not (stat.S_ISDIR(mode) or stat.S_ISREG(mode)):
                found.append(f"{os.path.relpath(p, root)} (special, mode {oct(mode)})")
    return sorted(found)


def install_oracle(root, rel, text):
    """Write the oracle at `root/rel` on the host without following anything the agent could have
    placed: every parent is opened relative to the previous one with O_NOFOLLOW|O_DIRECTORY (and
    checked with lstat to be a real directory), the file itself is created with
    O_CREAT|O_EXCL|O_NOFOLLOW, so an existing file or link at the path is an error, never a target.
    Returns (ok, detail)."""
    parts = rel.split("/")
    fd = os.open(root, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    checked = []
    try:
        for name in parts[:-1]:
            try:
                st = os.lstat(name, dir_fd=fd)
            except FileNotFoundError:
                os.mkdir(name, 0o755, dir_fd=fd)
                st = os.lstat(name, dir_fd=fd)
            if not stat.S_ISDIR(st.st_mode):
                return False, f"parent {'/'.join(checked + [name])} is not a real directory (mode {oct(st.st_mode)})"
            nfd = os.open(name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=fd)
            os.close(fd)
            fd = nfd
            checked.append(name)
        try:
            out = os.open(parts[-1], os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o444, dir_fd=fd)
        except FileExistsError:
            try:
                mode = oct(os.lstat(parts[-1], dir_fd=fd).st_mode)
            except OSError:
                mode = "?"
            return False, f"the oracle path {rel} already exists in the measurement copy (mode {mode})"
        with os.fdopen(out, "w", encoding="utf-8") as f:
            f.write(text)
        return True, "created with O_CREAT|O_EXCL|O_NOFOLLOW; parents opened with O_NOFOLLOW|O_DIRECTORY"
    except OSError as e:
        return False, f"oracle installation refused: {e}"
    finally:
        os.close(fd)


def diff_added_lines(patch):
    """{path: [(new line number, text), ..]} of the lines a unified diff adds."""
    out, path, n = {}, None, 0
    for ln in patch.splitlines():
        if ln.startswith("diff --git "):
            path = None
        elif ln.startswith("+++ "):
            p = ln[4:].strip().strip('"')
            path = p[2:] if p.startswith("b/") else None
            if path is not None:
                out.setdefault(path, [])
        elif ln.startswith("@@"):
            m = re.match(r"@@ -\d+(?:,\d+)? \+(\d+)", ln)
            n = int(m.group(1)) if m else 0
        elif path is not None and ln.startswith("+"):
            out[path].append((n, ln[1:]))
            n += 1
        elif path is not None and not ln.startswith("-") and not ln.startswith("\\"):
            n += 1
    return out


def comment_flags(lines):
    """Per line of Rust source: is it a comment or rustdoc line (`//`, `/* .. */`, `#[doc ..]`)?"""
    flags, block = [], False
    for ln in lines:
        s = ln.strip()
        if block:
            flags.append(True)
            block = "*/" not in s
            continue
        if s.startswith("/*"):
            flags.append(True)
            block = "*/" not in s
            continue
        flags.append(s.startswith(("//", "*", "#[doc", "#![doc")))
    return flags


def harness_scan(patch):
    """Static scan of the lines the agent added for code that could forge the test harness's
    verdict from inside the oracle binary (or the existing tests): text shaped like libtest output
    (`test result:`, `... ok`, `running N tests`), writes to stdout/stderr and process exit/abort in
    library code, and code that runs before `main` or replaces a symbol. Returns
    {"invalid": [..], "review": [..]} ("path:line: reason: text")."""
    added = diff_added_lines(patch)
    invalid, review, stdio = [], [], []
    forging = False
    for path, rows in sorted(added.items()):
        is_rs = path.endswith(".rs")
        is_doc = path.lower().endswith((".md", ".txt", ".adoc", ".rst"))
        lib = is_rs and "/src/" in f"/{path}" and not re.search(r"(^|/)src/(main\.rs|bin/)", path)
        test = is_rs and not lib and re.search(r"(^|/)(tests|benches)/", path)
        flags = comment_flags([t for _, t in rows]) if is_rs else [is_doc] * len(rows)
        for (n, text), comment in zip(rows, flags):
            where = f"{path}:{n}"
            snippet = text.strip()[:160]
            if FORGE.search(text):
                if comment or is_doc:
                    review.append(f"{where}: libtest-shaped text in a comment or document: {snippet}")
                else:
                    forging = True
                    invalid.append(f"{where}: libtest-shaped text (harness forging): {snippet}")
            if not is_rs or comment:
                continue
            if BEFORE_MAIN.search(text):
                invalid.append(f"{where}: code that runs before main or replaces a symbol: {snippet}")
            if EXIT.search(text):
                (invalid if lib else review).append(
                    f"{where}: process exit/abort added in {'library' if lib else 'test' if test else 'other'} code: {snippet}")
            if lib and STDIO.search(text):
                stdio.append(f"{where}: library code writes to stdout/stderr: {snippet}")
    if stdio:
        if forging:
            invalid += [s + " (with libtest-shaped text in the same diff)" for s in stdio]
        else:
            review += stdio
    return {"invalid": invalid[:50], "review": review[:50]}


def load_toml(path):
    if not os.path.isfile(path) or os.path.islink(path):
        return None
    try:
        return tomllib.load(open(path, "rb"))
    except (tomllib.TOMLDecodeError, UnicodeDecodeError, OSError):
        return "unparseable"


def dep_tables(doc):
    """(table label, {name: spec}) for every dependency table of a manifest, target-specific ones
    and `[workspace.dependencies]` included."""
    out = []
    for t in DEP_TABLES + BUILD_DEP_TABLES:
        if isinstance(doc.get(t), dict):
            out.append((t, doc[t]))
    for cfg, body in (doc.get("target") or {}).items():
        if isinstance(body, dict):
            for t in DEP_TABLES + BUILD_DEP_TABLES:
                if isinstance(body.get(t), dict):
                    out.append((f"target.{cfg}.{t}", body[t]))
    ws = doc.get("workspace")
    if isinstance(ws, dict) and isinstance(ws.get("dependencies"), dict):
        out.append(("workspace.dependencies", ws["dependencies"]))
    return out


def dep_source(spec):
    if not isinstance(spec, dict):
        return {}
    return {k: spec.get(k) for k in SOURCE_KEYS if k in spec}


def is_proc_macro(doc):
    lib = doc.get("lib") if isinstance(doc, dict) else None
    return isinstance(lib, dict) and bool(lib.get("proc-macro") or lib.get("proc_macro"))


def manifest_changes(old, new):
    """Build-time code and dependency redirection a manifest change introduces."""
    hits = []
    old = old if isinstance(old, dict) else {}
    po, pn = old.get("package") or {}, new.get("package") or {}
    for key in ("build", "links", "metabuild"):
        if pn.get(key) != po.get(key):
            hits.append(f"[package] {key} = {pn.get(key)!r} (was {po.get(key)!r})")
    if is_proc_macro(new) and not is_proc_macro(old):
        hits.append("declares a proc-macro crate ([lib] proc-macro = true)")
    for key in ("patch", "replace"):
        if new.get(key) != old.get(key):
            hits.append(f"[{key}] changed")
    old_tables = dict(dep_tables(old))
    old_sources = {}
    for _, table in old_tables.items():
        for name, spec in table.items():
            old_sources.setdefault(name, dep_source(spec))
    for label, table in dep_tables(new):
        if label.endswith(BUILD_DEP_TABLES):
            if table != old_tables.get(label, {}):
                hits.append(f"[{label}] changed")
            continue
        for name, spec in table.items():
            crate = spec.get("package", name) if isinstance(spec, dict) else name
            source = dep_source(spec)
            # Registry crates come from the read-only owner cache; only a non-registry source
            # (path, git, another registry) that the baseline did not give the same entry redirects one.
            if (name in HARNESS_CRATES or crate in HARNESS_CRATES) and source and source != old_sources.get(name):
                hits.append(f"[{label}] {name}: source {source} (was {old_sources.get(name) or 'the registry'}): "
                            f"a measurement-harness crate redirected")
    return hits


def nearest_manifest(root, rel):
    d = os.path.dirname(rel)
    while True:
        m = os.path.join(root, d, "Cargo.toml") if d else os.path.join(root, "Cargo.toml")
        if os.path.isfile(m) and not os.path.islink(m):
            return os.path.relpath(m, root)
        if not d:
            return None
        d = os.path.dirname(d)


def build_time_code(baseline, copy, files):
    """Changes that let the agent's code run at build time or replace the measurement harness:
    an added or changed `build.rs`; a manifest that adds or changes `[build-dependencies]`, a
    `build`/`links` key, `[patch]`/`[replace]`, or declares a proc-macro crate; a change inside an
    existing proc-macro crate; a change of the harness crate (`HARNESS_DIRS`) or a redirection of a
    harness crate (`HARNESS_CRATES`). No task needs any of them. Returns a list of findings."""
    hits = []
    for rel in sorted(files):
        exists = os.path.lexists(os.path.join(copy, rel))
        if os.path.basename(rel) == "build.rs" and exists:
            hits.append(f"{rel}: build script added or changed")
        if any(rel.startswith(h) for h in HARNESS_DIRS):
            hits.append(f"{rel}: the measurement harness crate was changed")
        if os.path.basename(rel) == "Cargo.toml" and exists:
            new = load_toml(os.path.join(copy, rel))
            if isinstance(new, dict):
                hits += [f"{rel}: {h}" for h in manifest_changes(load_toml(os.path.join(baseline, rel)), new)]
        for root in (baseline, copy):
            m = nearest_manifest(root, rel)
            if m and is_proc_macro(load_toml(os.path.join(root, m))):
                hits.append(f"{rel}: inside the proc-macro crate {os.path.dirname(m)}")
                break
    return sorted(set(hits))


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


def doc_spans(rel, text):
    """Character ranges of `text` that are comments or documentation: for Rust sources the comments
    and doc attributes found by the token-aware `rust_comment_spans`; for prose files (`.md`,
    `.txt`) the whole text. Data files: none."""
    if rel.lower().endswith((".md", ".txt", ".adoc", ".rst")):
        return [(0, len(text))]
    if not rel.endswith(".rs"):
        return []
    return rust_comment_spans(text)


# Where a Rust token that matters for comment detection can start: a comment, an attribute, an
# identifier (string prefixes `b`, `c`, `r`, `br`, `cr` are identifiers to the lexer until a quote
# or `#` follows), a string or a character literal / lifetime.
_RS_START = re.compile(r"""//|/\*|#!?\[|[A-Za-z_][A-Za-z0-9_]*|["']""")
_RS_DOC_ATTR = re.compile(r"#!?\[\s*doc\b")
_RS_RAW = re.compile(r'(?:br|cr|r)(#*)"')
_RS_CHAR = re.compile(r"'(?:\\(?:x[0-9A-Fa-f]{2}|u\{[0-9A-Fa-f_]{1,6}\}|[^\n])|[^\\'\n\r\t])'")


def _rs_quoted_end(text, j):
    """End of a `"`-delimited (non-raw) string whose opening quote is at `j`: backslash escapes
    skip the next character; the literal may span lines; unterminated runs to the end."""
    n, j = len(text), j + 1
    while j < n:
        ch = text[j]
        if ch == "\\":
            j += 2
            continue
        j += 1
        if ch == '"':
            return j
    return n


def _rs_block_end(text, j):
    """End of a block comment opening at `j` (`/*`, `/**`, `/*!`); Rust block comments nest."""
    n, depth, j = len(text), 1, j + 2
    while j < n and depth:
        if text.startswith("/*", j):
            depth, j = depth + 1, j + 2
        elif text.startswith("*/", j):
            depth, j = depth - 1, j + 2
        else:
            j += 1
    return j


def _rs_token_end(text, i):
    """If a string, raw string, character literal or comment starts at `i`, its end; else None.
    `('comment', end)` for comments, `('literal', end)` for literals."""
    if text.startswith("//", i):
        e = text.find("\n", i)
        return "comment", len(text) if e < 0 else e
    if text.startswith("/*", i):
        return "comment", _rs_block_end(text, i)
    m = _RS_RAW.match(text, i)
    if m:
        close = '"' + m.group(1)
        e = text.find(close, m.end())
        return "literal", len(text) if e < 0 else e + len(close)
    if text.startswith(('b"', 'c"'), i):
        return "literal", _rs_quoted_end(text, i + 1)
    if text.startswith('"', i):
        return "literal", _rs_quoted_end(text, i)
    q = i + 1 if text.startswith("b'", i) else i
    if text.startswith("'", q):
        m = _RS_CHAR.match(text, q)
        if m:
            return "literal", m.end()
    return None


def rust_comment_spans(text):
    """Comment and documentation ranges of Rust source, from a scan that follows the lexer: line
    comments (`//`, `///`, `//!`) to the end of the line, block comments (`/* */`, `/** */`,
    `/*! */`) with nesting, and `#[doc ..]` / `#![doc ..]` attributes whole (bracket-balanced). String
    contents are never comments: normal strings with escapes, byte strings `b".."`, C strings
    `c".."`, raw strings `r#".."#`, `br#".."#`, `cr#".."#` (any number of `#`), strings spanning
    lines; a character literal (`'"'`, `'\\''`, `'\\u{..}'`, `b'x'`) is told from a lifetime or
    label (`'a`, `'static`) by its closing quote."""
    spans, i, n = [], 0, len(text)
    while i < n:
        m = _RS_START.search(text, i)
        if not m:
            break
        i, tok = m.start(), m.group()
        if tok.startswith("#"):
            if _RS_DOC_ATTR.match(text, i) and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")):
                spans.append((i, _rs_attr_end(text, text.index("[", i))))
                i = spans[-1][1]
            else:
                i = m.end()
            continue
        if tok[0].isalpha() or tok[0] == "_":
            # A string prefix only when the identifier is exactly one and a quote or `#"` follows.
            if tok in ("r", "br", "cr", "b", "c"):
                found = _rs_token_end(text, i)
                if found:
                    i = found[1]
                    continue
            i = m.end()
            continue
        found = _rs_token_end(text, i)
        if found is None:  # a lifetime or label: the quote alone
            i += 1
            continue
        kind, end = found
        if kind == "comment":
            spans.append((i, end))
        i = end
    return spans


def _rs_attr_end(text, j):
    """End of an attribute whose `[` is at `j`: brackets balanced, literals and comments skipped."""
    n, depth = len(text), 0
    while j < n:
        found = _rs_token_end(text, j)
        if found:
            j = found[1]
            continue
        ch = text[j]
        if ch == "[":
            depth += 1
        elif ch == "]":
            depth -= 1
            if depth == 0:
                return j + 1
        j += 1
    return n


def count_matches(sdk_root, checks):
    """Per check: matches of its regex over its glob, split into code and comment/documentation
    matches (`doc_spans`), with the file list of each."""
    rows = []
    for c in checks:
        n, docs, where, where_doc = 0, 0, [], []
        for rel in tree_files(sdk_root, c["glob"]):
            try:
                text = open(os.path.join(sdk_root, rel), encoding="utf-8").read()
            except (UnicodeDecodeError, OSError):
                continue
            hits = [m.start() for m in re.finditer(c["regex"], text)]
            if not hits:
                continue
            spans = doc_spans(rel, text)
            k_doc = sum(1 for h in hits if any(a <= h < b for a, b in spans))
            n += len(hits)
            docs += k_doc
            where.append(f"{rel}:{len(hits)}")
            if k_doc:
                where_doc.append(f"{rel}:{k_doc}")
        rows.append({"class": c.get("class"), "meaning": c["meaning"], "regex": c["regex"],
                     "glob": c["glob"], "count": n, "code_count": n - docs, "doc_count": docs,
                     "where": where, "where_in_comments_or_docs": where_doc})
    return rows


def code_text(rel, text):
    """`text` with its comments and documentation (`doc_spans`) blanked to spaces, newlines and
    offsets kept: the code a behavioural pattern must match. A comment such as
    `// TODO: admit /0/private/BalanceEx` states nothing, and a pattern spanning several parts
    (`fn list_comments[\\s\\S]*?validate\\(`) cannot complete itself inside a comment."""
    spans = doc_spans(rel, text)
    if not spans:
        return text
    chars = list(text)
    for a, b in spans:
        for i in range(a, min(b, len(chars))):
            if chars[i] != "\n":
                chars[i] = " "
    return "".join(chars)


def code_match(regex, rel, text):
    """Does `regex` match `text` with its comments and documentation blanked (`code_text`)?"""
    return re.search(regex, code_text(rel, text)) is not None


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
    """Classify every named provider fact by where the agent's final tree states it. A behavioural
    fact (every role but `docs`) is stated only by code: its `after` pattern is matched with the
    comments and documentation blanked (`code_match`), so a comment naming the new fact earns no
    credit (it is listed under `new_fact_only_in_comment_or_doc_in`). `docs` facts are read raw."""
    rows = []
    for s in sites:
        if s["role"] in SITE_BOUND_ROLES and not s.get("any_file"):
            files = [f for f in [s["file"]] + s.get("alternatives", []) if os.path.isfile(os.path.join(sdk_root, f))]
        else:
            files = sorted({rel for pat in search_area(s) for rel in tree_files(sdk_root, pat)})
        raw_after = [rel for rel in files if s.get("after") and re.search(s["after"], read(sdk_root, rel))]
        found = [rel for rel in raw_after if s["role"] == "docs" or code_match(s["after"], rel, read(sdk_root, rel))]
        stale = [rel for rel in files if s.get("before") and re.search(s["before"], read(sdk_root, rel))]
        # The old fact only in a comment or rustdoc (a historical note): settled with the oracle
        # verdict in `review_unrecognised` (a review candidate when the oracle passed).
        stale_code = [rel for rel in stale if s["role"] == "docs" or code_match(s["before"], rel, read(sdk_root, rel))]
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
        old_in_code = bool(s.get("before")) and (ref in stale_code or (s.get("any_file") and bool(stale_code)))
        rows.append({"fact": s["fact"], "role": s["role"], "reference_file": ref,
                     "declared_alternatives": alts, "where": where,
                     "new_fact_in": found, "new_fact_in_changed_files": [f for f in found if f in changed],
                     "new_fact_only_in_comment_or_doc_in": [f for f in raw_after if f not in found],
                     "old_fact_in": stale, "old_fact_at_reference_site": old_at_ref,
                     "old_fact_at_reference_site_in_code": old_in_code,
                     # kept for comparison with earlier results: the reference-site-only reading
                     "satisfied_at_reference_site": ref in found and not old_at_ref if not s.get("any_file") else bool(found) and not stale,
                     "stated": where != NOT_STATED and not old_at_ref})
    return rows


def review_unrecognised(rows, oracle_passed):
    """An unrecognised behavioural fact is `needs review` when the oracle passed (behaviour
    preserved, spelling unknown to the pattern), and stays `not stated` when it did not. An old fact
    left at the reference site only inside a comment or rustdoc (a historical note) does not count
    against the fact when the oracle passed: it is a review candidate instead."""
    for r in rows:
        if r["where"] == NOT_STATED and oracle_passed and r["role"] in BEHAVIOURAL_ROLES:
            r["where"] = NEEDS_REVIEW
            r["stated"] = False
            r["needs_review"] = True
        if oracle_passed and r["old_fact_at_reference_site"] and not r["old_fact_at_reference_site_in_code"]:
            r["old_fact_at_reference_site"] = False
            r["old_fact_only_in_comment_or_doc"] = True
            r["review_candidate"] = "the old fact appears at the reference site only in a comment or rustdoc"
            r["stated"] = r["where"] not in (NOT_STATED, NEEDS_REVIEW)
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


def cargo(ctx, args, log, tree_writable=False):
    """One cargo command on the measurement copy, in the sandbox, network unshared, offline, with
    the trusted CARGO_HOME and the baseline's toolchain. The copy is bound **read-only** (build
    scripts, proc macros and tests cannot rewrite the sources, the injected oracle or the harness
    crate); only the measurement target directory is writable. `tree_writable` is used once, for
    `cargo metadata` (which may refresh `Cargo.lock` and runs no build script or macro)."""
    env = {"CARGO_NET_OFFLINE": "true", "CARGO_TARGET_DIR": ctx["measure"],
           "CARGO_HOME": os.path.join(HOME, ".cargo")}
    if ctx.get("toolchain"):
        env["RUSTUP_TOOLCHAIN"] = ctx["toolchain"]
    writable = [ctx["measure"]] + ([ctx["measure_dir"]] if tree_writable else [])
    ro = [] if tree_writable else [ctx["measure_dir"]]
    cmd = sandbox_cmd(writable, ["cargo"] + args, ctx["measure_dir"], env, ro=ro, network=False)
    return sh(cmd, log=log, timeout=3600)


TEST_BLOCK = re.compile(r"^---- (.+) stdout ----\s*$")
ERROR_LINE = re.compile(r"^error(\[E\d+\])?:")
LISTED_FAILURE = re.compile(r"^    (\S.*?)\s*$")
# libtest's pretty run list adds the test mode to a name (`test x - should panic ... FAILED`); the
# failure headers and the final list use the bare name.
TEST_MODE = re.compile(r" - (?:should panic|compile fail|compile)$")


def _error_blocks(lines):
    """rustc diagnostics in cargo's own output: an `error` line up to the blank line that ends it."""
    blocks, cur = [], []
    for ln in lines:
        if ERROR_LINE.match(ln):
            if cur:
                blocks.append("\n".join(cur))
            cur = [ln]
        elif cur and ln.strip():
            cur.append(ln)
        elif cur:
            blocks.append("\n".join(cur))
            cur = []
    if cur:
        blocks.append("\n".join(cur))
    return blocks


def _test_section_blocks(sec):
    """One test target's section of a cargo log (its `Running` / `Doc-tests` line up to the next):
    (failure blocks, framing problem or None, index of the first line after the target's output).

    libtest frames failures as: the run list (`test <name> ... FAILED`), a `failures:` line, one
    `---- <name> stdout ----` header per failed test with output followed by that output, a second
    `failures:` line with the failed names (`    <name>`), the `test result:` summary. Captured
    output may contain any of these lines, so the framing is read from the outside in: the last
    summary line, the last `failures:` line before it (the final list), the first `failures:`
    line; the inventory of failed tests is the final list, which must equal the run list's FAILED
    names and the summary's count. A header is a boundary only if it names a test of that
    inventory; a header-shaped line naming anything else stays inside the block it occurs in and is
    not evidence (it is dropped from the block's text); two headers naming the same test, output
    before the first header, or an inventory that disagrees make the section ambiguous (no blocks)."""
    name = sec[0].strip()
    results = [i for i, ln in enumerate(sec) if RESULT_LINE.search(ln)]
    lists = [i for i, ln in enumerate(sec) if ln.strip() == "failures:"]
    headers = [i for i, ln in enumerate(sec) if TEST_BLOCK.match(ln)]
    if not results:
        # A binary that ended without a summary (abort, exit): cargo's own lines follow; a failure
        # framing without its summary cannot be read.
        if lists or headers:
            return [], f"{name}: failure output without the target's summary line", len(sec)
        return [], None, 1
    r = results[-1]
    nfailed = int(RESULT_LINE.search(sec[r]).group(3))
    lists = [i for i in lists if i < r]
    if not nfailed:
        if lists or any(h < r for h in headers):
            return [], f"{name}: failure framing in a target whose summary reports no failure", r + 1
        return [], None, r + 1
    if len(lists) < 2:
        return [], f"{name}: {nfailed} failed, but the `failures:` framing is incomplete", r + 1
    f1, f2 = lists[0], lists[-1]
    inventory = []
    for ln in sec[f2 + 1:r]:
        if not ln.strip():
            continue
        m = LISTED_FAILURE.match(ln)
        if not m:
            return [], f"{name}: unexpected line in the final failures list: {ln.strip()[:80]}", r + 1
        inventory.append(m.group(1))
    run_list = sorted(TEST_MODE.sub("", t.group(1).strip()) for ln in sec[:f1]
                      if (t := TEST_LINE.match(ln.rstrip())) and t.group(2) == "FAILED")
    if sorted(inventory) != run_list or len(set(inventory)) != len(inventory) or len(inventory) != nfailed:
        return [], (f"{name}: the failed-test inventory disagrees (final list {sorted(inventory)}, run list "
                    f"{run_list}, summary {nfailed} failed)"), r + 1
    region = sec[f1 + 1:f2]
    bounds = [(i, m.group(1)) for i, ln in enumerate(region)
              if (m := TEST_BLOCK.match(ln)) and m.group(1) in inventory]
    named = [b for _, b in bounds]
    duplicated = sorted({b for b in named if named.count(b) > 1})
    if duplicated:
        return [], f"{name}: more than one failure header for {duplicated}", r + 1
    first = bounds[0][0] if bounds else len(region)
    if any(ln.strip() for ln in region[:first]):
        return [], f"{name}: output before the first failure header", r + 1
    blocks = []
    for k, (i, _) in enumerate(bounds):
        end = bounds[k + 1][0] if k + 1 < len(bounds) else len(region)
        body = [ln for ln in region[i + 1:end] if not TEST_BLOCK.match(ln)]
        blocks.append("\n".join([region[i]] + body))
    return blocks, None, r + 1


def failure_blocks(log):
    """The failure reports of a cargo log, one block each, and the framing problems found:
    `(blocks, problems)`. A block is one rustc `error` diagnostic in cargo's own output (outside
    any test target's output) or one failed test's output with its `---- <test> stdout ----` header
    (which names the test), framed against the failed-test inventory (`_test_section_blocks`). A
    target whose framing is ambiguous contributes no block and one problem."""
    lines = log.splitlines()
    starts = [i for i, ln in enumerate(lines) if RUNNING.match(ln)]
    outside = [lines[:starts[0]] if starts else lines]
    blocks, problems = [], []
    for k, s in enumerate(starts):
        e = starts[k + 1] if k + 1 < len(starts) else len(lines)
        found, problem, after = _test_section_blocks(lines[s:e])
        blocks += found
        if problem:
            problems.append(problem)
        outside.append(lines[s + after:e])
    for seg in outside:
        blocks += _error_blocks(seg)
    return blocks, problems


def omission_stage(evidence, stages):
    """The stage that detected an omitted classification, attributed only on evidence that is both
    classification-specific and tied to the new operation: `evidence` is the sealed
    `omission_evidence` (`regex`: the omission's own failure, e.g. Kraken `SigningInput`;
    `subject`: a term naming the new operation, type or path, e.g. `BalanceEx`), `stages` the
    ordered list of (stage, failed, log) of the stages that ran. The detecting stage is the first
    failed stage with one failure block (`failure_blocks`: one rustc diagnostic or one test's
    failure output) that matches **both** patterns; a match anywhere else in the log does not count.
    A failed stage whose failure output is ambiguously framed (a delimiter collision in captured
    output, `_test_section_blocks`) ends the search before any of its blocks is accepted as
    evidence: the row says `cannot be assessed` (captured output could have manufactured a
    matching block, and an earlier stage may have detected the omission unseen). Failures without that
    evidence are listed, not attributed; with none the row says `cannot be assessed`."""
    evidence = evidence or {}
    regex, subject = evidence.get("regex"), evidence.get("subject")
    failed = [s for s, f, _ in stages if f]
    if not regex or not subject:
        return {"stage": "cannot be assessed (no classification-specific evidence naming the new operation "
                         "is declared for this task)",
                "evidence": regex, "subject": subject, "failed_without_evidence": failed}
    unrelated, outside = [], []
    for name, is_failed, log in stages:
        if not is_failed:
            continue
        blocks, problems = failure_blocks(log)
        if problems:
            return {"stage": (f"cannot be assessed: the failure output of {name} is ambiguously framed "
                              f"({'; '.join(problems)[:300]})"),
                    "evidence": regex, "subject": subject, "failed_without_evidence": unrelated + [name],
                    "evidence_outside_a_block_naming_the_operation": outside,
                    "ambiguous_framing": {name: problems}}
        for block in blocks:
            m, n = re.search(regex, block), re.search(subject, block)
            if m and n:
                return {"stage": name, "evidence": regex, "subject": subject, "evidence_text": m.group(0),
                        "subject_text": n.group(0), "evidence_block": block.splitlines()[0][:200],
                        "failed_without_evidence": unrelated,
                        "evidence_outside_a_block_naming_the_operation": outside,
                        "ambiguous_framing": {}}
        if any(re.search(regex, b) for b in blocks):
            # The failure kind is there, but no single failure names the new operation with it
            # (e.g. `SigningInput` from a test of the existing `Balance` operation).
            outside.append(name)
        unrelated.append(name)
    return {"stage": ("cannot be assessed: " + (", ".join(unrelated) + " failed, but not with classification-"
                      "specific evidence naming the new operation in one failure" if unrelated
                      else "no stage failed")),
            "evidence": regex, "subject": subject, "failed_without_evidence": unrelated,
            "evidence_outside_a_block_naming_the_operation": outside}


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
    # Static checks of the diff (no agent code runs): build-time code or a changed measurement
    # harness, and code that could forge the harness's output.
    result["build_time_code"] = build_time_code(ctx["baseline"], mdir, list(stats["files"]))
    result["harness_scan"] = harness_scan(patch)
    sites = check_sites(sdk_root, meta.get("expected_sites", []), changed)
    prop = count_matches(sdk_root, meta.get("propagation_checks", []))

    lock = os.path.join(mdir, "Cargo.lock")
    lock_before = sha256_file(lock) if os.path.isfile(lock) and not os.path.islink(lock) else None
    mcode, mtext = cargo(ctx, ["metadata", "--offline", "--format-version", "1"], None, tree_writable=True)
    with open(os.path.join(out, "cargo-lock.log"), "w", encoding="utf-8") as f:
        f.write(f"$ cargo metadata --offline --format-version 1 (tree writable; exit {mcode})\n"
                + (mtext[-4000:] if mcode else "ok\n"))
    result["lockfile"] = {"metadata_exit": mcode, "cargo_lock_changed":
                          lock_before != (sha256_file(lock) if os.path.isfile(lock) and not os.path.islink(lock) else None)}

    code, existing_text = cargo(ctx, ["test", "-p", meta["package"], "--no-fail-fast"],
                                os.path.join(out, "cargo-existing.log"))
    rows = targets(existing_text)
    built = "could not compile" not in existing_text
    result["existing_tests"] = {"built": built, "passed": code == 0 and bool(rows) and all(r["ok"] for r in rows),
                                "exit": code, "targets": [{k: v for k, v in r.items() if k != "tests"} for r in rows],
                                "tests_passed": sum(r["passed"] for r in rows),
                                "tests_failed": sum(r["failed"] for r in rows)}
    if not result["existing_tests"]["passed"]:
        result["existing_tests"]["first_error"] = first_error(existing_text)

    test_name = f"oracle_{kind}"
    rel = f"{meta['sdk_dir']}/tests/{test_name}.rs"
    before_install = links_and_special(mdir)
    ok, detail = install_oracle(mdir, rel, task["oracle"])
    sealed_sha = hashlib.sha256(task["oracle"].encode("utf-8")).hexdigest()
    result["oracle_installation"] = {"path": rel, "ok": ok, "detail": detail, "sealed_sha256": sealed_sha,
                                     "links_or_special_files_before": before_install}
    text, code, listed = "", -1, None
    if ok:
        code, text = cargo(ctx, ["test", "-p", meta["package"], "--test", test_name, "--", "--format", "pretty"],
                           os.path.join(out, "cargo-oracle.log"))
        if "could not compile" not in text:
            lcode, ltext = cargo(ctx, ["test", "-p", meta["package"], "--test", test_name, "--", "--list"],
                                 os.path.join(out, "cargo-oracle-list.log"))
            listed = parse_listing(ltext) if lcode == 0 else []
    verdict = oracle_verdict(task["oracle_tests"], text, code, listed)
    if not ok:
        verdict["status"] = "not_run (oracle installation refused)"
    result["oracle"] = verdict
    if not verdict["pass"] and ok:
        result["oracle"]["first_error"] = first_error(text)
    if ok:
        try:
            fd = os.open(os.path.join(mdir, rel), os.O_RDONLY | os.O_NOFOLLOW)
            with os.fdopen(fd, "rb") as f:
                after_sha = hashlib.sha256(f.read()).hexdigest()
        except OSError as e:
            after_sha = f"unreadable: {e}"
        result["oracle_installation"]["after_runs_sha256"] = after_sha
        result["oracle_installation"]["unchanged"] = after_sha == sealed_sha
    result["links_or_special_files_after_measurement"] = links_and_special(mdir)
    status = verdict["status"]
    result["edit_sites"] = review_unrecognised(sites, verdict["pass"])

    cls = meta.get("classification")
    if kind == "e4" and cls:
        chk = cls.get("check")
        present, only_comment = None, None
        if chk:
            # Code only: a comment or rustdoc naming the classification is not one.
            chk_text = read(sdk_root, chk["file"])
            present = code_match(chk["regex"], chk["file"], chk_text)
            only_comment = not present and bool(re.search(chk["regex"], chk_text))
        row = {"applies": cls.get("applies"), "reference_site": cls.get("site"),
               "classification_site_present": present,
               "classification_check_only_in_comment_or_doc": only_comment,
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
            existing = result["existing_tests"]
            stages = [("compiler (SDK build)", not existing["built"], existing_text),
                      ("existing tests (or the agent's own tests)", existing["built"] and not existing["passed"],
                       existing_text),
                      ("compiler at the caller (hidden oracle compile)", status == "compile_error", text),
                      ("hidden oracle at run time only", status != "compile_error" and not verdict["pass"], text)]
            found = omission_stage(cls.get("omission_evidence"), stages)
            row["omission_caught_by"] = found["stage"]
            row["omission_evidence"] = found
            row["verdict"] = "classification site absent (fact missing); caught by: " + row["omission_caught_by"]
        result["classification"] = row

    base = count_matches(os.path.join(ctx["baseline"], meta["sdk_dir"]), meta.get("propagation_checks", []))
    ref = reference_counts(ctx, open(task["reference"], encoding="utf-8").read(), meta["sdk_dir"],
                           meta.get("propagation_checks", []))
    for i, row in enumerate(prop):
        row["base_count"] = base[i]["count"]
        row["reference_count"] = ref[i]["count"] if ref else None
        row["reference_code_count"] = ref[i]["code_count"] if ref else None
        excess = row["count"] > (row["reference_count"] or 0)
        code_excess = row["code_count"] > (row["reference_code_count"] or 0)
        if excess and not code_excess and verdict["pass"]:
            # Only comments or documentation carry the extra old-pattern matches (e.g. a historical
            # note naming the previous version) and the oracle passed: not missed propagation.
            row["missed"] = False
            row["review_candidate"] = ("matches beyond the reference only in comments or documentation "
                                       "(oracle passed): read agent.patch")
        else:
            row["missed"] = excess
    result["propagation"] = prop
    result["missed_propagation"] = [r["meaning"] for r in prop if r["missed"]]
    result["propagation_review_candidates"] = [r["meaning"] for r in prop if r.get("review_candidate")]
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
        left = tamper["left_out_of_measurement"]
        signals = ([f".git: {x}" for x in gitc["beyond_normal"]]
                   + [f".cargo: {x}" for x in tamper["cargo_files"]]
                   + [f"nested .git: {x}" for x in tamper["nested_git"]]
                   + [f"toolchain file changed: {x}" for x in tamper["toolchain_files_changed"]]
                   + [f"symbolic link (not copied): {x}" for x in left["symlinks"]]
                   + [f"special file (not copied): {x}" for x in left["special"]])
        result["tampering"] = tamper

        patch = measure(ctx, task, result)
        inst = result["oracle_installation"]
        signals += ([f"oracle installation refused: {inst['detail']}"] if not inst["ok"] else [])
        signals += ([f"the oracle changed during the measurement ({inst.get('after_runs_sha256')})"]
                    if inst["ok"] and not inst.get("unchanged") else [])
        signals += [f"link or special file in the measurement copy after the build: {x}"
                    for x in result["links_or_special_files_after_measurement"]]
        signals += [f"build-time code or measurement-harness change: {x}" for x in result["build_time_code"]]
        signals += [f"harness forging pattern: {x}" for x in result["harness_scan"]["invalid"]]
        tamper["signals"] = signals
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
            "propagation_review_candidates": result["propagation_review_candidates"],
            "old_fact_only_in_comment_or_doc": [s["fact"] for s in sites if s.get("old_fact_only_in_comment_or_doc")],
            "classification": result.get("classification", {}).get("verdict"),
        }
        common = {"observed_model": agent["observed"], "requested": agent["requested"]}
        reasons = []
        if in_transcript or in_tree or gitc["leak_hits"]:
            reasons.append("the transcript, the tree or its .git shows access to a bank file")
        if signals:
            reasons.append("tampering: " + "; ".join(signals[:10]))
        review = result["harness_scan"]["review"]
        if reasons:
            result["unscored"] = scored
            result["outcome"] = {"status": "invalid", "scored": False, "reason": " | ".join(reasons),
                                 "also_needs_review": review, **common}
        elif review:
            # Ambiguous scan hits (stdout/stderr writes or exit in non-library code, libtest-shaped
            # text in a comment or document): scored, but read agent.patch before using the scores.
            result["outcome"] = {"status": "needs review", "scored": True, "review": review, **scored, **common}
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
