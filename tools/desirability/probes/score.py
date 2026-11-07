"""I3 scorer (docs-assisted prediction): R1 wire fields, R2 result/error types, R3 complete calls.

    python3 -I tools/desirability/probes/score.py --bank DIR --runs DIR [--out DIR] [--sdk ..] [--no-r3]
    python3 -I tools/desirability/probes/score.py --bank DIR --selftest [--sdk ..] [--out DIR]

`--runs` is a run.py output directory (`<runs>/<provider>/<sdk>.<session>/answer.json`). Items are
scored against `<bank>/truth/<sdk>.json` (truth.py). Normalization rules are in README.md.

- R1 per field: method; destination (scheme, host, path); query (pairs in order); header names
  (as a set); non-secret header values (exact); body (JSON structurally, else text); Content-Type;
  Accept. Per item: `critical` = method + destination + query all right; `exact` = every field.
- R2: `send_returns` and `error_type` compared as tagged definition identities in the declaration
  module's scope (qualified SDK paths resolved through the SDK's real modules, SDK aliases and
  imports resolved; an unresolved path is its own `unresolved` identity, equal to no resolved
  one; see common.canon_ident); `base_url` in the set of strings that keep the prefix.
- R3: the answer is one expression whose value is the public `send().await` result (it ends with
  `.send().await`; no statement, early exit or other `.await`/`?`), checked with a Rust parser
  (the `form-check/` helper, built on `syn`) behind the regex guard. It passes when, bound to the
  declared send() result type (for `-> stream T`, the reference call's own result type) and
  compiled inside a bubblewrap sandbox, it compiles AND, run in a second sandbox against a local
  receiver answering the item's fixture, it sends the reference call's requests (method, path,
  query pairs in order, header names and stable values, body) and its value is `Ok`. Items whose
  result cannot be checked are reported apart (`diag_pass_result_unchecked`), never in R3.pass.
  Compile-only success and the old method/path/query-only rule are reported separately as
  diagnostics.

`--selftest` scores seeded readers built from the truth (`oracle`, `wrong`, `broken` and the
controls in SELFTEST_EXPECT: wrong and invented error owners, owners built from the SDK's real
modules, internal canonical spellings written directly, documented spellings, wrapped call
spellings, `vec![x; n]` in a struct literal and closures with statements, wrong bodies and
headers, discarded, swallowed and raw results, early returns (also between raw byte strings), a
stream reply of the wrong media type, file-system, network, compile-time file and environment,
and host-socket escapes), prints each expected rate
next to the observed one, and checks the sandbox effects against the same answers outside the
sandbox. Repetitions of an item
(several readers) are not independent participants: pooled rates average each item over readers
first, then over items (n = items). Run Python with `-I`.
"""
import argparse
import json
import os
import re
import resource
import secrets
import shutil
import socket
import stat
import subprocess
import sys
import threading
from collections import defaultdict
from concurrent.futures import ThreadPoolExecutor
from urllib.parse import unquote_plus

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import common as C  # noqa: E402
import generate as G  # noqa: E402
import truth as T  # noqa: E402

R1_FIELDS = ("method", "destination", "query", "header_names", "header_values", "body", "content_type", "accept")
CRITICAL = ("method", "destination", "query")
# an extra filter in front of the sandboxes (README "R3 execution"): answers it matches fail before
# compiling. Compile-time file and environment access (`include!`, `include_str!`, `include_bytes!`,
# `env!`, `option_env!`) and Unix sockets are refused here too.
GUARD = re.compile(r"std::process|\bCommand\b|std::fs|tokio::fs|tokio::process|std::net|TcpStream|std::os\b|"
                   r"\bUnix(?:Stream|Listener|Datagram)\b|\bunsafe\b|"
                   r"include_str|include_bytes|\binclude\s*!|option_env|\benv\s*!|std::env|reqwest::Client|"
                   r"with_base_url|extern\s|macro_rules|#\s*\[|::new\s*\(\s*reqwest")
SCORE_MOD = "__concord_probe_score"


# ---------------------------------------------------------------- R1

def hexup(s):
    return re.sub(r"%[0-9a-fA-F]{2}", lambda m: m.group(0).upper(), s)


def mask(v, canaries):
    if isinstance(v, str):
        return T.mask(v, canaries)
    if isinstance(v, list):
        return [mask(x, canaries) for x in v]
    if isinstance(v, dict):
        return {k: mask(x, canaries) for k, x in v.items()}
    return v


def split_url(u, canaries):
    u = mask((u or "").strip(), canaries)
    u = re.sub(r"%3[cC]secret%3[eE]", "<secret>", u)
    m = re.match(r"^([A-Za-z][A-Za-z0-9+.-]*)://([^/?#]*)([^?#]*)(?:\?([^#]*))?", u)
    if not m:
        return None, None
    scheme, host, path, q = m.groups()
    return (scheme.lower() + "://" + host.lower() + (hexup(path) or "/"),
            [hexup(x) for x in q.split("&") if x] if q else [])


def header_map(h, canaries):
    items = []
    if isinstance(h, dict):
        items = list(h.items())
    elif isinstance(h, list):
        for x in h:
            if isinstance(x, (list, tuple)) and len(x) == 2:
                items.append((x[0], x[1]))
            elif isinstance(x, dict) and "name" in x:
                items.append((x["name"], x.get("value")))
    out = {}
    for k, v in items:
        out.setdefault(str(k).strip().lower(), []).append(mask("" if v is None else str(v).strip(), canaries))
    return {k: ", ".join(v) for k, v in out.items()}


def norm_media(v):
    if v is None:
        return None
    v = str(v).strip()
    return v or None


def score_wire(t, a, canaries):
    a = a or {}
    res = {}
    res["method"] = str(a.get("method") or "").strip().upper() == t["method"].upper()
    td, tq = split_url(t["url"], canaries)
    ad, aq = split_url(a.get("url"), canaries)
    res["destination"] = ad is not None and ad == td
    res["query"] = ad is not None and aq == tq
    th = header_map(t["headers"], canaries)
    ah = header_map(a.get("headers"), canaries)
    res["header_names"] = set(th) == set(ah)
    plain = [k for k in th if k not in set(t.get("secret_headers", []))]
    res["header_values"] = all(ah.get(k) == th[k] for k in plain) if plain else None
    kind = t.get("body_kind")
    ab = a.get("body")
    if kind == "json":
        if isinstance(ab, str):
            try:
                ab = json.loads(ab)
            except ValueError:
                pass
        res["body"] = mask(ab, canaries) == t["body"]
    elif kind == "text":
        res["body"] = isinstance(ab, str) and mask(ab, canaries) == t["body"]
    else:
        res["body"] = ab is None
    res["content_type"] = norm_media(a.get("content_type")) == t.get("content_type")
    res["accept"] = norm_media(a.get("accept")) == t.get("accept")
    res["critical"] = all(res[f] for f in CRITICAL)
    res["exact"] = all(res[f] is not False for f in R1_FIELDS)
    # diagnostic only: query pairs compared after form-decoding
    dec = lambda q: [unquote_plus(x) for x in (q or [])]  # noqa: E731
    res["diag_query_decoded"] = ad is not None and dec(aq) == dec(tq)
    return res


# ---------------------------------------------------------------- R2

def clean_type_answer(s):
    if not isinstance(s, str):
        return ""
    return s.strip().strip("`").strip().rstrip(";").strip()


def norm_base(u):
    u = (u or "").strip().strip("`").strip() if isinstance(u, str) else ""
    m = re.match(r"^([A-Za-z][\w+.-]*)://([^/?#]*)(.*)$", u)
    return (m.group(1).lower() + "://" + m.group(2).lower() + hexup(m.group(3))) if m else u


def score_result(t, a, sdk, ctx=()):
    """R2 for one item; `ctx` is the declaration's module (`Sdk.decl_module`), the scope both
    the truth's and the answer's types are read in."""
    a = a or {}
    res = {}
    r2 = t.get("r2", {})
    if r2.get("resolvable") and r2.get("verified"):
        # both sides canonicalized now, by the same rules (truth files keep theirs for the record),
        # and compared as tagged definition identities (common.canon_ident), never as text
        canon = lambda x: C.canon_ident(x, sdk, ctx=ctx)  # noqa: E731
        res["send_returns"] = canon(clean_type_answer(a.get("send_returns"))) == canon(r2["send_returns"])
        if r2.get("error_type"):
            res["error_type"] = canon(clean_type_answer(a.get("error_type"))) == canon(r2["error_type"])
    bu = t.get("base_url") or {}
    if bu.get("keeps"):
        res["base_url"] = norm_base(a.get("base_url")) in {norm_base(k) for k in bu["keeps"]}
    return res


# ---------------------------------------------------------------- R3

BWRAP = "/usr/bin/bwrap"
RUN_TIMEOUT = 60         # wall-clock seconds per sandboxed answer run
COMPILE_TIMEOUT = 3600   # wall-clock ceiling of one sandboxed cargo build
RUN_PARALLEL = 8
RUN_MAX_FILE = 64 << 20  # RLIMIT_FSIZE of a run: caps its output and any file it writes
HARNESS_SUBDIR = ".probe-score-harness"  # answer modules, inside the throwaway copy
# the R3 answer contract (README "R3 answer contract"): a single expression whose value is the
# public `send().await` result (optionally `?`, the compatibility path), checked with a Rust parser
# (`form-check/`, a small `syn` binary that is not a workspace member; built offline on first use)
FORM_CHECK_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "form-check")
FORM_CHECK_BIN = os.path.join(C.CARGO_TARGET_DIR, "release", "concord-probe-form-check")
_FORM_CACHE = {}  # code -> (lexes, form or None, cut, reason or None)
_FORM_BUILT = []


def form_check_binary():
    """The form-check helper, built once per process with `cargo build --offline --release`
    (a no-op when it is up to date)."""
    if not _FORM_BUILT:
        p = subprocess.run(["cargo", "build", "--offline", "--release", "--quiet", "--manifest-path",
                            os.path.join(FORM_CHECK_DIR, "Cargo.toml")], env=C.cargo_env(), capture_output=True, text=True)
        if p.returncode or not os.access(FORM_CHECK_BIN, os.X_OK):
            raise SystemExit(f"R3 needs the form-check helper ({FORM_CHECK_DIR}); building it failed:\n{p.stderr[-2000:]}")
        _FORM_BUILT.append(FORM_CHECK_BIN)
    return _FORM_BUILT[0]


def _form_run(codes):
    # hex lines: an answer's newlines cannot break the protocol; a lone surrogate (possible in a
    # JSON answer) reaches the helper as invalid UTF-8, which it refuses
    p = subprocess.run([form_check_binary()], input="".join(c.encode("utf-8", "surrogatepass").hex() + "\n" for c in codes),
                       capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=600)
    lines = p.stdout.splitlines()
    if p.returncode or len(lines) != len(codes):
        return None
    out = []
    for ln in lines:
        lexes, form, cut, why = ln.split("\t", 3)
        out.append((lexes == "1", None if form == "refused" else form, int(cut), why or None))
    return out


def form_check(codes):
    """{code: (lexes, form, cut, reason)} from the form-check helper, one process for all the
    codes not seen yet. `lexes`: the code is balanced Rust token trees; `form`: "value", "ok" or
    None (refused, with `reason`); `cut`: the UTF-8 byte length of the code to compile (without
    its trailing `;`). If the helper dies on a batch, each code is checked alone and one it dies
    on is refused."""
    todo = [c for c in dict.fromkeys(codes) if c not in _FORM_CACHE]
    if todo:
        res = _form_run(todo)
        if res is None:
            res = [(_form_run([c]) or [(False, None, 0, "the form checker failed on this answer")])[0] for c in todo]
        _FORM_CACHE.update(zip(todo, res))
    return {c: _FORM_CACHE[c] for c in codes}


def lexes(code):
    """True when `code` is a sequence of balanced Rust token trees (rustc's lexer rules: strings,
    raw and byte strings, chars, lifetimes, nested comments), so pasted into the wrapper it cannot
    close the wrapper's braces."""
    return form_check([code])[code][0]


def call_form(code):
    """(code to compile, form, refusal) of an R3 answer under the answer contract, decided by the
    form-check helper's Rust parser (README "R3 answer contract"): parsed as rustc reads it inside
    the wrapper's `{ <answer> }`, the answer must be exactly one expression statement (one trailing
    `;` is dropped from the code to compile); after one optional outer `( .. )` or `{ .. }`
    holding exactly one expression and one optional `?`, it must be `<receiver>.send().await`
    (form "value") or `<receiver>.send().await?` (form "ok", the compatibility path whose value
    must be the declared result's `Ok` type). The receiver must hold no `return`, `break`,
    `continue`, `?`, loop or block with statements outside closure bodies, and nowhere an
    `.await`, `async`, `unsafe`, `yield`, `try` block, item, attribute or macro other than `vec!`
    and `format!` (whose arguments are checked the same way). form None: refused, with the reason;
    the code is returned unchanged."""
    _, form, cut, why = form_check([code])[code]
    if form is None:
        return code, None, why
    return code.encode("utf-8")[:cut].decode("utf-8"), form, None


def req_key(rec):
    path, _, q = rec["target"].partition("?")
    return (rec["method"].upper(), hexup(path), tuple(hexup(x) for x in q.split("&") if x))


def _json_or_none(text):
    try:
        return json.loads(text)
    except (TypeError, ValueError):
        return None


def _match_volatile(g, w, w2):
    """Structural match where the two reference runs agree; where they differ (a nonce, a
    signature, a timestamp) only the shape is compared."""
    if isinstance(w, dict) and isinstance(w2, dict):
        return isinstance(g, dict) and set(g) == set(w) == set(w2) and all(_match_volatile(g[k], w[k], w2[k]) for k in w)
    if isinstance(w, list) and isinstance(w2, list) and len(w) == len(w2):
        return isinstance(g, list) and len(g) == len(w) and all(_match_volatile(a, b, c) for a, b, c in zip(g, w, w2))
    if w == w2:
        return g == w
    return type(g) is type(w)


def body_matches(g, w, w2, ct, volatile):
    """The R3 body rule: JSON structurally (Content-Type JSON or `+json`), form-urlencoded as
    pairs in order, anything else exactly. A body that differs between the two reference runs is
    compared where they agree."""
    if T.is_json_media(ct):
        jg, jw = _json_or_none(g), _json_or_none(w)
        if jw is not None or w.strip() in ("null",):
            if not volatile:
                return jg == jw
            return _match_volatile(jg, jw, _json_or_none(w2))
    base = (ct or "").split(";")[0].strip().lower()
    if base == "application/x-www-form-urlencoded" and volatile:
        pairs = lambda t: [tuple(x.split("=", 1)) if "=" in x else (x, "") for x in t.split("&") if x]  # noqa: E731
        pg, pw, pw2 = pairs(g), pairs(w), pairs(w2)
        return [k for k, _ in pg] == [k for k, _ in pw] == [k for k, _ in pw2] and \
            all(a[1] == b[1] for a, b, c in zip(pg, pw, pw2) if b[1] == c[1])
    if volatile:
        return len(g) == len(w)
    return g == w


def compare_requests(got, t):
    """(pass, reason, legacy) for the requests an answer made against the reference's: the same
    sequence of method, path and query pairs; the same header names, and the same values for every
    header whose value is stable between two reference runs (secrets compared as `<secret>`); the
    same body. `legacy` is the old method/path/query-only rule (a diagnostic)."""
    want, again, vol = t.get("recorded", []), t.get("recorded_again") or [], t.get("volatile") or {}
    legacy = [req_key(g) for g in got] == [req_key(w) for w in want]
    if len(got) != len(want):
        return False, f"sent {[req_key(g) for g in got]} instead of {[req_key(w) for w in want]}", legacy
    for i, (g, w) in enumerate(zip(got, want)):
        if req_key(g) != req_key(w):
            return False, f"request {i + 1}: {req_key(g)} instead of {req_key(w)}", legacy
        gh, wh = T.header_groups(g), T.header_groups(w)
        if set(gh) != set(wh):
            return False, f"request {i + 1}: header names {sorted(gh)} instead of {sorted(wh)}", legacy
        for k in wh:
            if k not in vol.get("headers", []) and gh[k] != wh[k]:
                return False, f"request {i + 1}: header `{k}` is {gh[k]} instead of {wh[k]}", legacy
        ct = (wh.get("content-type") or [None])[0]
        w2 = again[i]["body"] if i < len(again) else w["body"]
        if not body_matches(g["body"], w["body"], w2, ct, i in vol.get("bodies", [])):
            return False, f"request {i + 1}: body {g['body'][:200]!r} instead of {w['body'][:200]!r}", legacy
    return True, None, legacy


# -- sandbox layouts (bubblewrap)

def _home():
    return os.path.realpath(os.path.expanduser("~"))


def ipc_hides():
    """bwrap arguments that hide host IPC endpoints under /run (and /var/run when it is not a
    symlink to /run) behind a fresh tmpfs. Abstract sockets are isolated by the network
    namespace, SysV IPC by `--unshare-ipc`; other pathname sockets are masked (`socket_masks`)."""
    out = ["--tmpfs", "/run"]
    if os.path.isdir("/var/run") and not os.path.islink("/var/run"):
        out += ["--tmpfs", "/var/run"]
    return out


NS_ARGS = ["--dev", "/dev", "--proc", "/proc", "--unshare-net", "--unshare-pid", "--unshare-ipc",
           "--die-with-parent", "--clearenv"]
SOCKET_FIND = ["find", "/", "(", "-path", "/proc", "-o", "-path", "/sys", "-o", "-path", "/dev", ")", "-prune",
               "-o", "-type", "s", "-exec", "test", "-S", "{}", ";", "-print"]


def scan_sockets(layout):
    """Pathname Unix sockets visible inside a sandbox with `layout` (bwrap mount arguments):
    `find` candidates by directory-entry type, kept only when `test -S` (a stat that sees mounts)
    agrees, so a masked socket no longer counts."""
    cmd = [BWRAP] + layout + NS_ARGS + ["--setenv", "PATH", "/usr/bin:/bin", "--"] + SOCKET_FIND
    r = subprocess.run(cmd, capture_output=True, text=True, timeout=600)
    return sorted(x for x in r.stdout.splitlines() if x.startswith("/"))


def socket_masks(paths):
    """bwrap arguments that cover each socket path with /dev/null (connect: refused)."""
    out = []
    for p in paths:
        out += ["--ro-bind", "/dev/null", p]
    return out


def run_layout(exe=None, scratch=None):
    """The answer run's mounts: the root read-only, /run hidden, the home directory replaced by an
    empty tmpfs with only the test executable and the scratch dir mounted back, a private /tmp."""
    out = ["--ro-bind", "/", "/"] + ipc_hides() + ["--tmpfs", _home()]
    if exe:
        out += ["--ro-bind", exe, exe]
    if scratch:
        out += ["--bind", scratch, scratch]
    return out + ["--tmpfs", "/tmp"]


_RUN_SOCKETS = []


def run_sockets():
    """Pathname sockets visible in the run layout before masking (scanned once per process)."""
    if not _RUN_SOCKETS:
        _RUN_SOCKETS.append(scan_sockets(run_layout()))
    return _RUN_SOCKETS[0]


def sandbox_cmd(exe, args, scratch, env):
    """The answer's test executable under bubblewrap (`run_layout`, the sockets it would still
    show masked), `scratch` the only writable path, no network (a new namespace with only
    loopback, where the local receiver runs), its own PID and IPC namespaces, killed with its
    parent, an empty environment plus `env`, and a wall-clock timeout."""
    cmd = ["timeout", "-k", "5", str(RUN_TIMEOUT), BWRAP] + run_layout(exe, scratch) + socket_masks(run_sockets()) + NS_ARGS
    for k, v in env.items():
        cmd += ["--setenv", k, v]
    return cmd + ["--chdir", scratch, "--", exe] + args


def active_toolchain(cwd):
    r = subprocess.run(["rustup", "show", "active-toolchain"], cwd=cwd, capture_output=True, text=True, env=C.cargo_env())
    name = (r.stdout.split() or [""])[0]
    if r.returncode or not name:
        raise SystemExit(f"R3 compiles answers with rustup's active toolchain; `rustup show active-toolchain` failed: {r.stderr[-300:]}")
    return name


class CompileSandbox:
    """cargo for reader answers inside bubblewrap (a `compile_rounds` runner). The root is
    read-only with /run hidden; the home directory is an empty tmpfs into which only these are
    mounted back: the active rustup toolchain (read-only), `$CARGO_HOME/bin`
    and `$CARGO_HOME/registry` (read-only) inside a writable tmpfs `$CARGO_HOME` (cargo's lock
    files live at its root), the throwaway copy (read-only; the answer modules are inside it) and
    the target directory (writable). No network, own PID and IPC namespaces, a private /tmp, any
    other pathname socket masked, an empty environment except PATH, CARGO_HOME, RUSTUP_HOME,
    RUSTUP_TOOLCHAIN, CARGO_TARGET_DIR and HOME (an empty scratch dir), and `cargo --offline
    --frozen`. Paths are the host's, so cargo's fingerprints match builds made outside."""

    def __init__(self, copy_root, target_dir, parent_env=None):
        home = _home()
        self.home, self.copy_root, self.target_dir = home, copy_root, target_dir
        self.cargo_home = os.path.realpath(os.environ.get("CARGO_HOME") or os.path.join(home, ".cargo"))
        self.rustup_home = os.path.realpath(os.environ.get("RUSTUP_HOME") or os.path.join(home, ".rustup"))
        self.toolchain = active_toolchain(copy_root)
        self.sandbox_home = os.path.join(home, ".concord-probe-sandbox-home")
        self.parent_env = dict(parent_env if parent_env is not None else os.environ)
        self.jobs = os.environ.get("CARGO_BUILD_JOBS") or "6"
        os.makedirs(target_dir, exist_ok=True)
        self.sockets = scan_sockets(self.mounts())  # before masking

    def mounts(self, masked=False):
        tc = os.path.join(self.rustup_home, "toolchains", self.toolchain)
        out = ["--ro-bind", "/", "/"] + ipc_hides() + ["--tmpfs", self.home, "--tmpfs", self.cargo_home]
        for sub in ("bin", "registry"):
            d = os.path.join(self.cargo_home, sub)
            out += ["--ro-bind", d, d]
        out += ["--ro-bind", tc, tc]  # rustup's proxies need only the toolchain (RUSTUP_TOOLCHAIN is set)
        out += ["--ro-bind", self.copy_root, self.copy_root, "--bind", self.target_dir, self.target_dir,
                "--tmpfs", "/tmp", "--dir", self.sandbox_home]
        return out + (socket_masks(self.sockets) if masked else [])

    def env(self):
        return {"PATH": f"{self.cargo_home}/bin:/usr/bin:/bin", "CARGO_HOME": self.cargo_home,
                "RUSTUP_HOME": self.rustup_home, "RUSTUP_TOOLCHAIN": self.toolchain,
                "CARGO_TARGET_DIR": self.target_dir, "HOME": self.sandbox_home}

    def command(self, program):
        cmd = ["timeout", "-k", "10", str(COMPILE_TIMEOUT), BWRAP] + self.mounts(masked=True) + NS_ARGS
        for k, v in self.env().items():
            cmd += ["--setenv", k, v]
        return cmd + ["--chdir", self.copy_root, "--"] + list(program)

    def __call__(self, cwd, args, capture=True, timeout=None):
        assert os.path.realpath(cwd) == os.path.realpath(self.copy_root), cwd
        cargo_args = [args[0], "--offline", "--frozen", "-j", str(self.jobs)] + list(args[1:])
        return subprocess.run(self.command(["cargo"] + cargo_args), env=self.parent_env,
                              capture_output=True, text=True)

    def describe(self):
        return {"sandbox": "bwrap", "toolchain": self.toolchain, "mounts": self.mounts(masked=True),
                "env": self.env(), "sockets_masked": self.sockets, "cargo_flags": ["--offline", "--frozen", "-j", self.jobs]}


def _limits():
    resource.setrlimit(resource.RLIMIT_FSIZE, (RUN_MAX_FILE, RUN_MAX_FILE))
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))


def sandbox_run(exe, args, scratch, sandbox=True):
    """Run one test with a fresh scratch dir and nonce, in the sandbox (`sandbox=False`: directly
    on the host, only for the self-test's outside-the-sandbox demonstrations of its own control
    code). Its output goes to files next to the scratch dir, outside the sandbox's writable path,
    capped by RLIMIT_FSIZE. Returns (records carrying that nonce, exit code, stderr tail, stdout of
    a `--list` run, `@@SANDBOXENV` lines)."""
    if os.path.isdir(scratch):
        shutil.rmtree(scratch)
    os.makedirs(scratch)
    nonce = secrets.token_hex(16)
    nfile = os.path.join(scratch, "nonce")
    with open(nfile, "w") as f:
        f.write(nonce)
    env = {"CONCORD_PROBE_NONCE_FILE": nfile, "RUST_BACKTRACE": "0", "PATH": "/usr/bin:/bin"}
    out_p, err_p = scratch.rstrip("/") + ".stdout", scratch.rstrip("/") + ".stderr"
    with open(out_p, "w") as fo, open(err_p, "w") as fe:
        if sandbox:
            code = subprocess.run(sandbox_cmd(exe, args, scratch, env), stdout=fo, stderr=fe, preexec_fn=_limits).returncode
        else:
            code = subprocess.run(["timeout", "-k", "5", str(RUN_TIMEOUT), exe] + args, stdout=fo, stderr=fe,
                                  preexec_fn=_limits, env=env, cwd=scratch).returncode
    stdout = open(out_p, encoding="utf-8", errors="replace").read()
    stderr = open(err_p, encoding="utf-8", errors="replace").read()
    os.remove(out_p)
    os.remove(err_p)
    recs, marks = [], []
    dec = json.JSONDecoder()
    for line in stdout.splitlines():
        # libtest may print `test <name> ... ` before the test's own output on the same line
        if "@@SANDBOXENV " in line:
            marks.append(line.split("@@SANDBOXENV ", 1)[1])
        pos = line.find("@@PROBE ")
        while pos >= 0:
            try:
                obj, end = dec.raw_decode(line, pos + len("@@PROBE "))
                if obj.get("nonce") == nonce:
                    recs.append(obj)
            except ValueError:
                end = pos + 1
            pos = line.find("@@PROBE ", end)
    shutil.rmtree(scratch, ignore_errors=True)
    return recs, code, (stderr or "")[-1500:], (stdout if args[:1] == ["--list"] else ""), marks


def list_tests(exe, scratch):
    out = sandbox_run(exe, ["--list", "--format", "terse"], scratch)[3]
    return [ln.rsplit(": ", 1)[0] for ln in out.splitlines() if ln.endswith(": test")]


def _answer_modules(copy_root, repo, banks, truths, answers, guard_off=(), form_off=(), unbound=(), fixtures=None):
    """Refusals and generated answer modules for `answers` ([(reader, item id, code)]):
    ({key: refusal or None}, {(pkg, file): Module}, {key: (pkg, runner test name)}). Before
    compiling, an answer is refused when the regex guard matches (unless its reader is in
    `guard_off`), it is not balanced Rust token trees (the form-check helper's lexer), or the
    helper's Rust parser finds it is not one expression whose value is the `send().await` result
    (README "R3 answer contract"; skipped for readers in `form_off`, whose code is compiled as it
    is). Readers in `unbound` are bound without the declared type, and `fixtures` ({key: fixture})
    replaces an item's reply (self-test discrimination and controls only)."""
    out, modules, runs = {}, {}, {}
    harness_dir = os.path.join(copy_root, HARNESS_SUBDIR)
    form_check([c for _, _, c in answers if isinstance(c, str) and c.strip()])  # one helper run
    for reader, iid, code in answers:
        short = iid.split("/")[0]
        b = banks[short]
        item = next(i for i in b["items"] if i["id"] == iid)
        pick = b["picks"][item["pick"]]
        t = truths[short]["items"].get(iid) or {}
        key = (reader, iid)
        if not isinstance(code, str) or not code.strip():
            out[key] = {"compiles": False, "pass": False, "reason": "no code"}
            continue
        g = GUARD.search(code) if reader not in guard_off else None
        if g:
            out[key] = {"compiles": False, "pass": False, "reason": f"refused before compiling (guard: `{g.group(0)}`)"}
            continue
        if not lexes(code):
            out[key] = {"compiles": False, "pass": False,
                        "reason": "not balanced Rust tokens (unbalanced delimiters, or an unterminated literal or comment)"}
            continue
        binding = t.get("binding")
        if binding is None or truths[short].get("truth_format", 0) < T.TRUTH_FORMAT:
            out[key] = {"compiles": False, "pass": False,
                        "reason": f"the truth file predates truth format {T.TRUTH_FORMAT} (single-expression reference, "
                                  "checked stream results); regenerate it with truth.py"}
            continue
        body, form, why = call_form(code)
        if form is None and reader not in form_off:
            out[key] = {"compiles": False, "pass": False, "form": None,
                        "reason": f"refused before compiling (answer contract): {why}"}
            continue
        if form is None:
            body, form = code, "value"
        if reader in unbound:
            binding = {"type": None, "result": None}
        mkey = (b["pkg"], pick["client_file"])
        if mkey not in modules:
            stem = os.path.splitext(os.path.basename(pick["client_file"]))[0]
            path = os.path.join(harness_dir, f"{short}__{stem}.rs")
            modules[mkey] = T.Module(b["pkg"], pick["client_file"], path,
                                     T.header("score.py", f"reader answers for `{short}` (R3)",
                                              os.path.join(repo, "examples", b["sdk"])))
        m = modules[mkey]
        tag = re.sub(r"\W", "_", f"{reader}_{iid}")
        fn = f"__answer_{tag}"
        m.add(f"{reader}|{iid}#ans", T.wrap_call_fn(fn, pick["client"], body, binding, ok_form=(form == "ok"),
                                                     reference=t.get("reference")))
        bare = b["clients"][pick["client"]]["ctor"]
        fx = (fixtures or {}).get(key) or t.get("fixture")
        # fixture files are embedded from the throwaway copy, the only source tree the sandbox sees
        replies = f"&[{T.fixture_expr(fx, m.path, copy_root)}]" if fx else "&[__EMPTY_JSON]"
        m.add(f"{reader}|{iid}#run", T.record_block(f"__run_{tag}", f"{reader}|{iid}", "r3", bare,
                                                    t.get("base_path", ""), T.outcome_of(f"{fn}(api)"),
                                                    replies, 1),
              needs=f"{reader}|{iid}#ans")
        runs[key] = (b["pkg"], f"__run_{tag}")
        out[key] = None
    return out, modules, runs


def _run_tests(exes, runs, failed, run_root, keys=None, sandbox=True):
    """{key: (records, exit code, stderr tail, @@SANDBOXENV lines)} for the runnable answers,
    each its own process (in the sandbox unless `sandbox=False`)."""
    names, results, jobs = {}, {}, []
    for pkg, exe in exes.items():
        for n in list_tests(exe, os.path.join(run_root, "list")):
            names[(pkg, n.rsplit("::", 1)[-1])] = n
    for key, (pkg, fname) in runs.items():
        rid = f"{key[0]}|{key[1]}"
        if (keys is not None and key not in keys) or f"{rid}#ans" in failed or f"{rid}#run" in failed:
            continue
        if pkg not in exes or (pkg, fname) not in names:
            results[key] = ([], None, "test executable or test not found", [])
            continue
        jobs.append((key, exes[pkg], names[(pkg, fname)]))
    with ThreadPoolExecutor(max_workers=RUN_PARALLEL) as pool:
        futs = {pool.submit(sandbox_run, exe, [name, "--exact", "--nocapture", "--test-threads=1"],
                            os.path.join(run_root, str(i)), sandbox): key for i, (key, exe, name) in enumerate(jobs)}
        for fut, key in futs.items():
            recs, code, err, _, marks = fut.result()
            results[key] = (recs, code, err, marks)
    return results, len(names)


def _first_exchange(recs, rid, canaries):
    passes, _ = T.exchanges({rid: recs}, rid, "r3", nonce=recs[0]["nonce"] if recs else "")
    p0 = passes.get(0)
    return p0, [T.req_record(r, canaries) for r in (p0 or {}).get("requests", [])]


def r3_compile_and_check(repo, banks, truths, answers, log, guard_off=(), form_off=(), unbound=(), parent_env=None,
                         fixtures=None):
    """answers: [(reader, item id, code)] → ({(reader, item id): {"compiles", "pass", "reason", ..}},
    context for the self-test's follow-up steps). Answers are compiled with cargo inside bubblewrap
    (`CompileSandbox`); each answer's test then runs alone in the run sandbox (`sandbox_cmd`).
    `parent_env`: the environment bwrap itself starts with (it passes none of it on). `pass` is
    None for items without a request check, and for items whose result is not checked (their
    request-only outcome is `diag_pass_result_unchecked`), so R3.pass always means "the same
    requests and an `Ok` value"."""
    copy_root = os.path.join(repo, "target", "tmp", "probe-score-copy")
    out, modules, runs = _answer_modules(copy_root, repo, banks, truths, answers, guard_off, form_off, unbound, fixtures)
    mods = list(modules.values())
    ctx = {"copy_root": copy_root, "runs": runs, "exes": {}, "failed": {}, "sandbox": None, "modules": modules}
    failed, results = {}, {}
    if mods and not (os.access(BWRAP, os.X_OK) and shutil.which("timeout")):
        raise SystemExit(f"R3 needs {BWRAP} and timeout(1) to compile and run answers in a sandbox; refusing to run them without one")
    if mods:
        C.scratch_copy(repo, "probe-score-copy")
        T.inject(copy_root, mods, SCORE_MOD)
        sandbox = CompileSandbox(copy_root, C.CARGO_TARGET_DIR, parent_env)
        log.append({"compile": sandbox.describe(), "run_sockets_masked": run_sockets()})
        exes = {}
        failed = T.compile_rounds(copy_root, mods, log, artifacts=exes, runner=sandbox)
        T.write_modules(mods)
        results, n_listed = _run_tests(exes, runs, failed, os.path.join(repo, "target", "tmp", "probe-score-run"))
        log.append({"run": "bwrap", "executables": len(exes), "tests_listed": n_listed})
        ctx.update(exes=exes, failed=failed, sandbox=sandbox)
    for key in [k for k, v in out.items() if v is None]:
        reader, iid = key
        short = iid.split("/")[0]
        t = truths[short]["items"].get(iid) or {}
        canaries = banks[short]["picks"][next(i for i in banks[short]["items"] if i["id"] == iid)["pick"]]["canaries"]
        rid = f"{reader}|{iid}"
        if f"{rid}#ans" in failed:
            out[key] = {"compiles": False, "pass": False, "reason": "does not compile: " + failed[f"{rid}#ans"]}
            continue
        res = {"compiles": True}
        if f"{rid}#run" in failed:
            res.update({"pass": False, "reason": "runner does not compile: " + failed[f"{rid}#run"]})
            out[key] = res
            continue
        recs, exit_code, err, marks = results.get(key, ([], None, "not run", []))
        p0, got = _first_exchange(recs, rid, canaries)
        res["recorded"] = [{"method": g["method"], "target": g["target"]} for g in got]
        res["outcome"] = (p0 or {}).get("outcome")
        res["sandbox_exit"] = exit_code
        if marks:
            res["marks"] = marks
        if not t.get("check_available"):
            res.update({"pass": None, "reason": "request check not available: " + t.get("check_unavailable_reason", "?")})
        elif p0 is None or not p0["summary"]:
            res.update({"pass": False, "reason": f"no complete record from the sandboxed run (exit {exit_code}; "
                                                 f"panicked, timed out or exited): {err[-300:]}"})
        else:
            ok, why, legacy = compare_requests(got, t)
            res["diag_legacy_pass"] = legacy
            res["result_checked"] = bool(t.get("result_checked"))
            # the value was bound to the declared send() result type (`binding`); where the result
            # is checked, the call must also have returned `Ok`
            if ok and t.get("result_checked") and p0["outcome"] is not None:
                ok, why = False, f"the call's result is an error, with a reply the reference decodes: {p0['outcome'][:300]}"
            if t.get("result_checked"):
                res["pass"] = ok
            else:  # requests only: reported apart, never in R3.pass
                res["pass"] = None
                res["diag_pass_result_unchecked"] = ok
                why = (why + "; " if why else "") + "result not checked: " + (t.get("result_unchecked_reason") or "?")
            if not ok or not t.get("result_checked"):
                res["reason"] = why
        out[key] = res
    return out, ctx


def r3_outside_runs(ctx, keys, banks, truths, run_root):
    """Self-test only: run the tests of `keys` from the sandboxed build directly on the host (no
    bwrap). {key: {"requests", "outcome", "exit"}}."""
    results, _ = _run_tests(ctx["exes"], ctx["runs"], ctx["failed"], run_root, keys=set(keys), sandbox=False)
    out = {}
    for key, (recs, code, err, _) in results.items():
        short = key[1].split("/")[0]
        canaries = banks[short]["picks"][next(i for i in banks[short]["items"] if i["id"] == key[1])["pick"]]["canaries"]
        p0, got = _first_exchange(recs, f"{key[0]}|{key[1]}", canaries)
        out[key] = {"requests": len(got), "reference_requests": len(truths[short]["items"][key[1]].get("recorded", [])),
                    "outcome": (p0 or {}).get("outcome"), "complete": bool(p0 and p0["summary"]), "exit": code,
                    "stderr": err[-300:] if not (p0 and p0["summary"]) else ""}
    return out


def r3_outside_compile(ctx, repo, banks, truths, answers, log, extra_env, run_keys):
    """Self-test only: compile `answers` with cargo on the host and the inherited environment plus
    `extra_env` (no bwrap), in the same throwaway copy and module files (only their packages are
    rebuilt), then run the tests of `run_keys` in the run sandbox. {key: {"compiles", "reason",
    "marks"}}."""
    readers = {r for r, _, _ in answers}
    out, modules, runs = _answer_modules(ctx["copy_root"], repo, banks, truths, answers, guard_off=readers, form_off=readers)
    exes = {}
    host = lambda cwd, args: T.cargo(cwd, args, extra_env=extra_env)  # noqa: E731
    failed = T.compile_rounds(ctx["copy_root"], list(modules.values()), log, artifacts=exes, runner=host)
    results, _ = _run_tests(exes, runs, failed, os.path.join(repo, "target", "tmp", "probe-score-run"), keys=set(run_keys))
    res = {}
    for key in runs:
        rid = f"{key[0]}|{key[1]}"
        res[key] = {"compiles": f"{rid}#ans" not in failed, "reason": failed.get(f"{rid}#ans"),
                    "marks": results.get(key, ([], None, "", []))[3]}
    for key, v in out.items():
        if v is not None:
            res[key] = {"compiles": False, "reason": v["reason"], "marks": []}
    return res


# ---------------------------------------------------------------- driver

def load(bank_dir, sdks):
    banks, truths = {}, {}
    for f in sorted(os.listdir(bank_dir)):
        if f.endswith(".json") and f != "index.json":
            b = C.read_json(os.path.join(bank_dir, f))
            if sdks and b["short"] not in sdks:
                continue
            tp = os.path.join(bank_dir, "truth", f"{b['short']}.json")
            if os.path.isfile(tp):
                banks[b["short"]] = b
                truths[b["short"]] = C.read_json(tp)
    return banks, truths


def rebuild_call(pick, override=None, add=None):
    """(client setters, call) of a pick rebuilt as generate.make_pick builds them, with some value
    literals replaced (`override`: {value index: literal}) or optional values set (`add`)."""
    override, add, vals = override or {}, add or {}, pick["values"]

    def state(i, v):
        return "set" if i in add else v["state"]

    def lit(i, v):
        return add.get(i, override.get(i, v["literal"]))

    def setters(level):
        return "".join(f".set_{v['name']}({lit(i, v)})" for i, v in enumerate(vals)
                       if v["level"] == level and state(i, v) == "set")

    def args(level):
        return ", ".join(lit(i, v) for i, v in enumerate(vals) if v["level"] == level and state(i, v) == "required")

    call = "api"
    for lvl, sc in enumerate(pick["scopes"], start=1):
        call += f".{G.ident(sc)}({args(lvl)})" + setters(lvl)
    lvl = len(pick["scopes"]) + 1
    call += f".{G.ident(pick['endpoint'])}({args(lvl)})" + setters(lvl)
    return setters(0), call


def control_code(pick, override=None, add=None, prefix="", last=None):
    """A control answer in the R3 contract form (truth.reference_call): one expression, the client
    setters chained on `api`, then the call and `.send().await`; `last(call)` replaces that
    ending, and `prefix` lines (statements: those controls run with the call-form rule off) come
    first."""
    csets, call = rebuild_call(pick, override, add)
    call = "api" + csets + call[len("api"):]
    code = last(call) if last else f"{call}.send().await"
    return f"{prefix}\n{code}" if prefix else code


# the three result-evasion controls (review finding): each sends the reference's requests but its
# value is not the `send().await` result
RESULT_EVASIONS = {
    "r3_named_discard": lambda call: f"let ignored = {call}.send().await;\n()",
    "r3_swallowed_error": lambda call: f"{call}.send().await.ok()",
    "r3_raw_response": lambda call: f"{call}.prepare()?.send_raw().await?",
}
UNBOUND = "__unbound"  # suffix: the same evasion answers bound without the declared type
# review 3, finding 1, the reviewer's answer: an early return leaves the wrapper before the bound
# value; it ends with `.send().await`, so only the single-expression rule refuses it
EARLY_RETURN = lambda call: f"{call}.prepare()?.send_raw().await?; return Ok(()); {call}.send().await"  # noqa: E731
FORM_RULE_OFF = "__form_off"  # suffix: the same answers compiled with the call-form rule off
# review 5, finding 1: the same early exit hidden in a tuple between raw byte strings (`br#"""#`),
# which the fix-round-3 lexer misread; it ends with `.send().await`, so only the parser refuses it
REVIEW5_REPRODUCER = ('(br#"""#, api.holidays("x&y=z",24).prepare()?.send_raw().await?, return Ok(()), br#"""#, '
                      'api).4.holidays("x&y=z",24).send().await')
REVIEW5_ITEM = ("nager-holidays/c1", 'api.holidays("x&y=z", 24).send().await')  # (item, its reference)
BYTE_STRING_EARLY_RETURN = lambda call: (f'(br#"""#, {call}.prepare()?.send_raw().await?, return Ok(()), br#"""#, '  # noqa: E731
                                         f'api).4{call[len("api"):]}.send().await')
# review 5, finding 3: valid single call expressions the fix-round-3 rule refused (a `;` inside
# `vec![x; n]`, statements in a closure body); the parser accepts them
POSITIVE_FORMS = {
    "r3_vec_repeat_in_struct": lambda call: (f"(std::ops::Range {{ start: 0usize, end: vec![0u8; 2].len() }}, api).1"
                                             f"{call[len('api'):]}.send().await"),
    "r3_closure_statements": lambda call: ('(move || -> Option<_> { let n: usize = "2".parse().ok()?; '
                                           'if n == 0 { return None; } Some(api) })().unwrap()'
                                           f"{call[len('api'):]}.send().await"),
}
# review 3, finding 4: equivalent spellings of the contract form
WRAPPED = {"r3_paren_wrapped": lambda code: f"({code})", "r3_block_wrapped": lambda code: f"{{ {code} }}"}
WRONG_MEDIA = "text/plain; charset=utf-8"  # the stream control's reply media type (review 3, finding 2)


def _sample(ftype):
    """A Rust value for a simple field type, different from its Default."""
    f = re.sub(r"\s+", "", ftype)
    ints = "|".join(C.PRIM_INT)
    table = [(r"String", '"wrongbody".to_string()'), (r"Option<String>", 'Some("wrongbody".to_string())'),
             (r"Vec<String>", 'vec!["wrongbody".to_string()]'), (r"Option<Vec<String>>", 'Some(vec!["wrongbody".to_string()])'),
             (rf"(?:{ints})", "7"), (rf"Option<(?:{ints})>", "Some(7)"), (r"bool", "true"), (r"Option<bool>", "Some(true)"),
             (r"(?:std::collections::)?(?:BTreeMap|HashMap)<String,String>", '[("wrong".to_string(), "body".to_string())].into_iter().collect()')]
    return next((v for pat, v in table if re.fullmatch(pat, f)), None)


def wrong_body(sdk, v):
    """A body literal of the same type that serializes differently, or None."""
    k = v.get("kind") or ""
    if k == "string":
        return C.rust_str("wrongbody")
    if k == "json_value":
        return 'serde_json::json!({"name": "wrongbody"})'
    if not k.startswith("default:"):
        return None
    t = C.parse_type(k.split(":", 1)[1])
    name = t[1][-1] if t[0] == "path" else None
    it = sdk.find(name, ("struct",)) if name else None
    lit = v.get("literal") or ""
    if it and lit.endswith("::default()"):
        clean = C.strip_strings_and_comments(it["text"])
        for fm in re.finditer(r"(?m)^\s*pub(?:\(crate\))?\s+(?:r#)?(\w+)\s*:\s*([^\n]+?),?\s*$", clean):
            val = _sample(fm.group(2))
            if val:  # struct update syntax: one expression, as the answer contract requires
                return f"{lit[:-len('::default()')]} {{ {fm.group(1)}: {val}, ..{lit} }}"
    for im in sdk.impls:  # a builder setter `pub fn x(mut self, _: impl Into<String>) -> Self`
        if name and re.match(r"impl\s*(<.*?>)?\s*" + re.escape(name) + r"\b", im["header"]) and " for " not in im["header"]:
            fm = re.search(r"pub\s+fn\s+(\w+)\s*\(\s*mut\s+self\s*,\s*\w+\s*:\s*impl\s+Into\s*<\s*String\s*>\s*\)\s*->\s*Self",
                           C.strip_strings_and_comments(im["text"]))
            if fm:
                return f'{v["literal"]}.{fm.group(1)}("wrongbody")'
    return None


def other_value(v, word):
    """A literal of the same kind as value `v` with another value, or None."""
    k = v.get("kind") or ""
    if k in ("string", "str"):
        return C.rust_str(word)
    if k == "header_value":
        return f"reqwest::header::HeaderValue::from_static({C.rust_str(word)})"
    if k.startswith("int:") and isinstance(v.get("value"), int):
        return str(v["value"] + 1)
    if k == "bool" and isinstance(v.get("value"), bool):
        return "false" if v["value"] else "true"
    if k.startswith(("nonzero:", "nonzerog:")) and isinstance(v.get("value"), int):
        return v["literal"].replace(f"new({v['value']}", f"new({v['value'] + 1}", 1)
    return None


def _requalify(t, sdk, crate_word, ctx):
    """The type tree with SDK-local names written as a real path from the crate root,
    `<crate_word>::..::Name` (the shortest one the module index resolves to the same definition,
    a public one first), and names imported from Concord written `concord::Name` (documented
    alternative spellings of the same owners)."""
    k = t[0]
    if k == "ref":
        return ("ref", t[1], _requalify(t[2], sdk, crate_word, ctx))
    if k == "tuple":
        return ("tuple", [_requalify(a, sdk, crate_word, ctx) for a in t[1]])
    if k != "path":
        return t
    args = [_requalify(a, sdk, crate_word, ctx) for a in t[2]]
    segs = [x for x in t[1] if x]
    r = sdk.index.lookup(ctx, segs[0], ctx) if len(segs) == 1 else None
    if r and r[0] == "def":
        segs = sdk.index.spelling(r, crate_word, ctx) or segs
    elif r and r[0] == "ext" and r[1][0] == "concord" and len(r[1]) == 2:
        segs = list(r[1])
    return ("path", segs, args)


def wrong_owners(sdk, ctx, name):
    """Wrong-owner R2 spellings of `name` built from the SDK's existing modules:
    {"r2_owner_wrong_module": `crate::<m>::<name>` for the first module m (closest to the root)
    that neither defines nor imports `name`, "r2_owner_repeated_module": `crate::<m>::<m>::<name>`
    for the first crate-level module m without a child `m`}; a key is left out when no module fits."""
    mods = sorted((p for p in sdk.index.mods if p), key=lambda p: (len(p), p))
    out = {}
    for p in mods:
        if sdk.index.resolve_path(["crate", *p, name], ctx) is None:
            out["r2_owner_wrong_module"] = "::".join(["crate", *p, name])
            break
    for p in (p for p in mods if len(p) == 1):
        if sdk.index.resolve_path(["crate", p[0], p[0], name], ctx) is None:
            out["r2_owner_repeated_module"] = "::".join(["crate", p[0], p[0], name])
            break
    return out


def _with_error(send_returns, err):
    t = C.parse_type(send_returns)
    if t[0] == "path" and t[1][-1] == "Result" and len(t[2]) == 2:
        return C.type_str(("path", t[1], [t[2][0], C.parse_type(err)]))
    return None


def _spell(n, ident):
    """Type text of an identity tree (common.canon_ident) with each identity written by
    `ident(identity)` (the R2 spoofing controls)."""
    k = n[0]
    if k == "id":
        return ident(n[1]) + ("<" + ", ".join(_spell(a, ident) for a in n[2]) + ">" if n[2] else "")
    if k == "ref":
        return "&" + ("mut " if n[1] else "") + _spell(n[2], ident)
    if k == "tuple":
        inner = [_spell(a, ident) for a in n[1]]
        return "(" + inner[0] + ",)" if len(inner) == 1 else "(" + ", ".join(inner) + ")"
    if k == "slice":
        return "[" + _spell(n[1], ident) + "]"
    if k == "array":
        return "[" + _spell(n[1], ident) + "; " + n[2] + "]"
    return "_" if k == "infer" else n[1]


def pre_fix4_spelling(identity):
    """The canonical text of fix round 3, which was compared as text: an SDK definition was
    `sdk::<module path>::Name` (`sdk::error::Error`), an SDK item outside the module index
    `sdk::Name`; review 5 found an answer writing it directly credited on 52 items."""
    tag, rest = identity[0], list(identity[1:])
    if tag == C.SDK_DEF:
        return "::".join(["sdk"] + rest[1:])  # without the crate name
    if tag == C.SDK_ITEM:
        return "sdk::" + rest[-1]
    if tag == C.STD:
        return "::".join(["std"] + rest)
    return "::".join(rest)  # another crate's path, a prelude or primitive name, unresolved text


def tag_path_spelling(identity):
    """An identity written as a Rust path whose first segment is its tag (`sdk_def::<crate>::..`,
    `std_prelude::Result`): the identity tags are not addressable as paths."""
    return "::".join([identity[0].replace("-", "_")] + list(identity[1:]))


def _has_sdk_identity(n):
    if n[0] == "id":
        return n[1][0] in (C.SDK_DEF, C.SDK_ITEM) or any(_has_sdk_identity(a) for a in n[2])
    if n[0] in ("ref",):
        return _has_sdk_identity(n[2])
    if n[0] == "tuple":
        return any(_has_sdk_identity(a) for a in n[1])
    if n[0] in ("slice", "array"):
        return _has_sdk_identity(n[1])
    return False


def host_address():
    """A non-loopback IPv4 address of this host (the source of its default route), or None."""
    try:
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as u:
            u.connect(("192.0.2.1", 9))  # no packet is sent for a UDP connect
            ip = u.getsockname()[0]
        return None if ip.startswith("127.") else ip
    except OSError:
        return None


class Listener:
    """A TCP listener on a non-loopback host address that counts the connections it accepts."""

    def __init__(self, ip):
        self.sock = socket.socket()
        self.sock.bind((ip, 0))
        self.sock.listen(16)
        self.sock.settimeout(0.2)
        self.addr = self.sock.getsockname()
        self.accepted, self.stop = 0, False
        self.thread = threading.Thread(target=self._loop, daemon=True)
        self.thread.start()

    def _loop(self):
        while not self.stop:
            try:
                c, _ = self.sock.accept()
                self.accepted += 1
                c.close()
            except OSError:
                continue

    def close(self):
        self.stop = True
        self.thread.join()
        self.sock.close()


class UnixListener(Listener):
    """A pathname Unix socket listener (under the host's /run) that counts accepted connections."""

    def __init__(self, path):
        self.path = path
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.bind(path)
        self.sock.listen(16)
        self.sock.settimeout(0.2)
        self.accepted, self.stop = 0, False
        self.thread = threading.Thread(target=self._loop, daemon=True)
        self.thread.start()

    def close(self):
        super().close()
        if os.path.exists(self.path):
            os.remove(self.path)


def _is_socket(path):
    try:
        return stat.S_ISSOCK(os.stat(path).st_mode)
    except OSError:
        return False


def ipc_setup():
    """(listener, socket paths) for the host-socket control: a listener the scorer opens in the
    user's runtime directory under /run, the user's D-Bus session socket there if it exists, and
    the first socket outside /run that the run layout would show before masking."""
    rt = os.environ.get("XDG_RUNTIME_DIR") or f"/run/user/{os.getuid()}"
    listener, targets = None, []
    if os.path.realpath(rt).startswith("/run/") and os.path.isdir(rt) and os.access(rt, os.W_OK):
        listener = UnixListener(os.path.join(rt, f"concord-probe-{secrets.token_hex(6)}.sock"))
        targets.append(listener.path)
    if _is_socket(os.path.join(rt, "bus")):
        targets.append(os.path.join(rt, "bus"))
    other = next((s for s in run_sockets() if not s.startswith(("/run/", "/var/run/"))), None)
    if other:
        targets.append(other)
    return listener, targets


def home_file(escape_dir):
    """A real file in the home directory for the compile-time file control: a shell profile file,
    else one written into the escape directory (inside the repository, also under the home)."""
    for name in (".bash_logout", ".profile", ".bashrc"):
        p = os.path.join(_home(), name)
        if os.path.isfile(p) and not os.path.islink(p):
            return p
    p = os.path.join(escape_dir, "home-file.txt")
    with open(p, "w") as f:
        f.write("a file in the home directory\n")
    return p


SELFTEST_EXPECT = {
    # reader: {metric: expected rate}
    "oracle": {"R1.critical": 1, "R1.exact": 1, "R1.header_values": 1, "R1.body": 1, "R2.send_returns": 1,
               "R2.error_type": 1, "R2.base_url": 1, "R3.pass": 1, "R3.diag_compiles": 1},
    "wrong": {"R1.method": 0, "R1.destination": 0, "R1.query": 0, "R1.critical": 0, "R1.exact": 0, "R1.header_names": 0,
              "R1.body": 0, "R2.send_returns": 0, "R2.error_type": 0, "R2.base_url": 0, "R3.pass": 0},
    "broken": {"R3.pass": 0, "R3.diag_compiles": 0},
    "r2_owner_made_up": {"R2.send_returns": 0, "R2.error_type": 0},
    "r2_owner_reqwest": {"R2.send_returns": 0, "R2.error_type": 0},
    "r2_owner_nested_core": {"R2.send_returns": 0, "R2.error_type": 0},
    "r2_owner_nested_concord": {"R2.send_returns": 0, "R2.error_type": 0},
    "r2_owner_wrong_module": {"R2.send_returns": 0, "R2.error_type": 0},
    "r2_owner_repeated_module": {"R2.send_returns": 0, "R2.error_type": 0},
    "r2_alias_documented": {"R2.send_returns": 1, "R2.error_type": 1},
    "r2_spoof_internal_error": {"R2.send_returns": 0, "R2.error_type": 0},
    "r2_spoof_internal_all": {"R2.send_returns": 0, "R2.error_type": 0},
    "r2_spoof_canonical": {"R2.send_returns": 0, "R2.error_type": 0},
    "r2_spoof_tag_path": {"R2.send_returns": 0, "R2.error_type": 0},
    "r3_ok_form": {"R3.pass": 1, "R3.diag_compiles": 1},
    "r3_paren_wrapped": {"R3.pass": 1, "R3.diag_compiles": 1},
    "r3_block_wrapped": {"R3.pass": 1, "R3.diag_compiles": 1},
    "r3_wrong_body": {"R3.pass": 0, "R3.diag_compiles": 1, "R3.diag_legacy_pass": 1},
    "r3_wrong_header": {"R3.pass": 0, "R3.diag_compiles": 1, "R3.diag_legacy_pass": 1},
    "r3_named_discard": {"R3.pass": 0, "R3.diag_compiles": 0},
    "r3_swallowed_error": {"R3.pass": 0, "R3.diag_compiles": 0},
    "r3_raw_response": {"R3.pass": 0, "R3.diag_compiles": 0},
    "r3_early_return": {"R3.pass": 0, "R3.diag_compiles": 0},
    "r3_review5_reproducer": {"R3.pass": 0, "R3.diag_compiles": 0},
    "r3_byte_string_early_return": {"R3.pass": 0, "R3.diag_compiles": 0},
    "r3_vec_repeat_in_struct": {"R3.pass": 1, "R3.diag_compiles": 1},
    "r3_closure_statements": {"R3.pass": 1, "R3.diag_compiles": 1},
    "r3_stream_wrong_media": {"R3.pass": 0, "R3.diag_compiles": 1, "R3.diag_legacy_pass": 1},
    "r3_sandbox_fs": {"R3.pass": 0, "R3.diag_compiles": 1},
    "r3_sandbox_net": {"R3.pass": 0, "R3.diag_compiles": 1},
    "r3_sandbox_include": {"R3.pass": 0, "R3.diag_compiles": 0},
    "r3_sandbox_env": {"R3.pass": 1, "R3.diag_compiles": 1},
    "r3_sandbox_ipc": {"R3.pass": 0, "R3.diag_compiles": 1},
}
# Discrimination references, not scores (harness variants scoring never uses):
# - the result-evasion answers bound WITHOUT the declared type send the reference's requests and
#   pass, so the type binding is what makes the bound controls fail;
# - the stream wrong-media answers bound without a result check (the round-2 rule for `-> stream`
#   items) pass, so the result check is what makes the control fail;
# - the early-return answers compiled with the call-form rule off compile and pass, so the
#   single-expression rule is what refuses them.
UNBOUND_EXPECT = {r + UNBOUND: {"R3.pass": 1, "R3.diag_compiles": 1} for r in RESULT_EVASIONS}
UNBOUND_EXPECT["r3_stream_wrong_media" + UNBOUND] = {"R3.pass": 1, "R3.diag_compiles": 1}
# - the review-5 answers (raw byte strings around an early return) compiled with the call-form rule
#   off compile and pass, so the parser is what refuses them.
EARLY_EXITS = ("r3_early_return", "r3_review5_reproducer", "r3_byte_string_early_return")
FORM_OFF_EXPECT = {r + FORM_RULE_OFF: {"R3.pass": 1, "R3.diag_compiles": 1} for r in EARLY_EXITS}
REFERENCE_EXPECT = {**UNBOUND_EXPECT, **FORM_OFF_EXPECT}
SANDBOX_CONTROL_ITEMS = 2  # items each file-system, network, include and env control runs on
GUARD_OFF = ("r3_sandbox_include", "r3_sandbox_env", "r3_sandbox_ipc")  # only the sandbox stands in the way
EVASION_READERS = tuple(RESULT_EVASIONS) + tuple(r + UNBOUND for r in RESULT_EVASIONS)
# the sandbox controls put statements before the call; the call-form rule (which refuses them
# too, an effect check) is off for them so that only the sandbox stands in the way
SANDBOX_READERS = ("r3_sandbox_fs", "r3_sandbox_net") + GUARD_OFF
FORM_OFF = EVASION_READERS + tuple(FORM_OFF_EXPECT) + SANDBOX_READERS
R2_OWNERS = (("r2_owner_made_up", "made_up::{}"), ("r2_owner_reqwest", "reqwest::{}"),
             ("r2_owner_nested_core", "made_up::concord_core::{}"), ("r2_owner_nested_concord", "x::concord::{}"))


def selftest_answers(banks, truths, sdks, repo, ctl):
    """({reader: {short: {session: answer object}}}, {control reader: details}) for the seeded
    readers. `oracle`, `wrong` and `broken` answer every item; the control readers only the items
    where their perturbation applies (see README "Self-test"). `ctl`: the TCP listener address
    (`net_addr`), the home file (`home_file`) and the host socket paths (`ipc_targets`)."""
    base = ("oracle", "wrong", "broken")
    readers = {r: {} for r in list(SELFTEST_EXPECT) + list(REFERENCE_EXPECT)}
    net_addr = ctl["net_addr"]
    details = {"r3_sandbox_fs": {"paths": {}}, "r3_sandbox_net": {"address": list(net_addr)},
               "r3_sandbox_include": {"file": ctl.get("home_file"), "items": []}, "r3_sandbox_env": {"items": []},
               "r3_sandbox_ipc": {"targets": {}}, "r3_wrong_body": {}, "r3_wrong_header": {},
               "r2_owner_wrong_module": {}, "r2_owner_repeated_module": {}, "r3_stream_wrong_media": {},
               "r3_early_return": {}, "r3_review5_reproducer": {}, "r3_byte_string_early_return": {},
               "r2_spoof_internal_error": {}, "r2_spoof_tag_path": {}}
    escape_dir = os.path.join(repo, "target", "tmp", "probe-sandbox-escape")
    n_fs = n_net = n_inc = n_env = 0
    ipc_left = list(ctl.get("ipc_targets") or [])

    def put(reader, short, session, ans):
        readers[reader].setdefault(short, {"wire": {"answers": []}, "rc": {"answers": []}})[session]["answers"].append(ans)

    for short, b in banks.items():
        tr = truths[short]["items"]
        sdk = sdks[short]
        for reader in base:
            readers[reader][short] = {"wire": {"answers": []}, "rc": {"answers": []}}
        for it in b["items"]:
            t = tr.get(it["id"])
            if t is None:
                continue
            q = it["q"]
            pick = b["picks"][it["pick"]]
            if it["kind"] == "wire":
                ok = {"q": q, "method": t["method"], "url": t["url"],
                      "headers": {k: v for k, v in t["headers"]}, "body": t["body"],
                      "content_type": t["content_type"], "accept": t["accept"]}
                bad = {"q": q, "method": "DELETE" if t["method"] != "DELETE" else "GET",
                       "url": re.sub(r"^(\w+://[^/?#]+)", r"\1/wrong", t["url"]) + ("&" if "?" in t["url"] else "?") + "wrong=1",
                       "headers": dict([(k, "wrong") for k, _ in t["headers"]] + [("x-wrong", "1")]),
                       "body": "wrong", "content_type": "text/wrong", "accept": "text/wrong"}
                for r, ans in (("oracle", ok), ("wrong", bad), ("broken", bad)):
                    put(r, short, "wire", ans)
            elif it["kind"] == "result":
                r2, bu = t.get("r2", {}), t.get("base_url") or {}
                ok = {"q": q, "send_returns": r2.get("send_returns"), "error_type": r2.get("error_type"),
                      "base_url": (bu.get("keeps") or [None])[0]}
                bad = {"q": q, "send_returns": "Result<WrongType, WrongError>", "error_type": "WrongError",
                       "base_url": "https://wrong.example.test"}
                for r, ans in (("oracle", ok), ("wrong", bad), ("broken", bad)):
                    put(r, short, "rc", ans)
                if r2.get("resolvable") and r2.get("verified") and r2.get("error_type"):
                    ctx = sdk.decl_module(pick["client_file"])
                    last = C.parse_type(r2["error_type"])
                    last = last[1][-1] if last[0] == "path" else "Error"
                    owners = [(reader, owner.format(last)) for reader, owner in R2_OWNERS]
                    for reader, path in sorted(wrong_owners(sdk, ctx, last).items()):
                        owners.append((reader, path))  # built from the SDK's real modules
                        details[reader][it["id"]] = path
                    for reader, owner in owners:
                        sr = _with_error(r2["send_returns"], owner)
                        if sr:  # the right type except the error's owner
                            put(reader, short, "rc", {"q": q, "send_returns": sr, "error_type": owner})
                    alias_sr = C.type_str(_requalify(C.parse_type(r2["send_returns"]), sdk, "crate", ctx))
                    alias_err = C.type_str(_requalify(C.parse_type(r2["error_type"]), sdk, sdk.crate, ctx))
                    put("r2_alias_documented", short, "rc", {"q": q, "send_returns": alias_sr, "error_type": alias_err})
                    # review 5, finding 2: internal canonical spellings written directly
                    sr_id, err_id = C.canon_ident(r2["send_returns"], sdk, ctx), C.canon_ident(r2["error_type"], sdk, ctx)
                    if _has_sdk_identity(err_id):  # the fix-round-3 text of an SDK error was `sdk::..`
                        spoof_err = _spell(err_id, pre_fix4_spelling)
                        sr = _with_error(r2["send_returns"], spoof_err)
                        if sr:  # the reviewer's form: `Result<HolidayResponse, sdk::error::Error>`
                            put("r2_spoof_internal_error", short, "rc", {"q": q, "send_returns": sr, "error_type": spoof_err})
                            details["r2_spoof_internal_error"][it["id"]] = sr
                        put("r2_spoof_internal_all", short, "rc", {"q": q, "send_returns": _spell(sr_id, pre_fix4_spelling),
                                                                   "error_type": spoof_err})
                    put("r2_spoof_canonical", short, "rc", {"q": q, "send_returns": C.ident_str(sr_id),
                                                            "error_type": C.ident_str(err_id)})
                    tag_sr = _spell(sr_id, tag_path_spelling)
                    put("r2_spoof_tag_path", short, "rc", {"q": q, "send_returns": tag_sr,
                                                           "error_type": _spell(err_id, tag_path_spelling)})
                    details["r2_spoof_tag_path"][it["id"]] = tag_sr
            elif it["kind"] == "call":
                put("oracle", short, "rc", {"q": q, "code": t["reference"]})
                put("wrong", short, "rc", {"q": q, "code": "let _ = api.clone();"})
                put("broken", short, "rc", {"q": q, "code": "api.__no_such_endpoint_for_selftest().send().await"})
                if not t.get("check_available"):
                    continue
                checked = t.get("result_checked") and (t.get("binding") or {}).get("result") == "is_ok"
                if checked:
                    for reader, wrap in WRAPPED.items():  # equivalent spellings of the reference
                        put(reader, short, "rc", {"q": q, "code": wrap(t["reference"])})
                    if (pick.get("output") or "").strip().startswith("stream") and t.get("fixture"):
                        # the same reference call, answered with the item's reply under a media
                        # type its stream format refuses: the call errs
                        fx = dict(t["fixture"], content_type=WRONG_MEDIA, name=t["fixture"]["name"] + " as " + WRONG_MEDIA)
                        details["r3_stream_wrong_media"][it["id"]] = fx
                        for reader in ("r3_stream_wrong_media", "r3_stream_wrong_media" + UNBOUND):
                            put(reader, short, "rc", {"q": q, "code": t["reference"]})
                if control_code(pick) != t["reference"]:
                    continue  # rebuilt code differs from the reference: no derived controls here
                vals = pick["values"]
                bodies = [(i, wrong_body(sdk, v)) for i, v in enumerate(vals)
                          if v["placement"] == "body" and v["state"] in ("required", "set")]
                bodies = [(i, x) for i, x in bodies if x]
                if bodies:
                    put("r3_wrong_body", short, "rc", {"q": q, "code": control_code(pick, {bodies[0][0]: bodies[0][1]})})
                    details["r3_wrong_body"][it["id"]] = bodies[0][1]
                hdr = [(i, other_value(v, "wrongheader")) for i, v in enumerate(vals)
                       if v["placement"] == "header" and v["state"] in ("required", "set")]
                hdr = [(i, x) for i, x in hdr if x]
                if hdr:
                    put("r3_wrong_header", short, "rc", {"q": q, "code": control_code(pick, {hdr[0][0]: hdr[0][1]})})
                    details["r3_wrong_header"][it["id"]] = f"`{vals[hdr[0][0]]['name']}` = {hdr[0][1]}"
                else:
                    add = [(i, other_value(dict(v, value=v.get("value") or 1), "addedheader")) for i, v in enumerate(vals)
                           if v["placement"] == "header" and v["state"] == "unset" and v.get("kind")]
                    add = [(i, x) for i, x in add if x]
                    if add:
                        put("r3_wrong_header", short, "rc", {"q": q, "code": control_code(pick, add={add[0][0]: add[0][1]})})
                        details["r3_wrong_header"][it["id"]] = f"`{vals[add[0][0]]['name']}` added = {add[0][1]}"
                if not checked:
                    continue  # the remaining controls need a call whose success is checked
                put("r3_ok_form", short, "rc", {"q": q, "code": control_code(pick, last=lambda c: f"{c}.send().await?")})
                for reader, last in RESULT_EVASIONS.items():
                    put(reader, short, "rc", {"q": q, "code": control_code(pick, last=last)})
                    put(reader + UNBOUND, short, "rc", {"q": q, "code": control_code(pick, last=last)})
                early = control_code(pick, last=EARLY_RETURN)
                put("r3_early_return", short, "rc", {"q": q, "code": early})
                details["r3_early_return"][it["id"]] = early
                byte_early = control_code(pick, last=BYTE_STRING_EARLY_RETURN)
                put("r3_byte_string_early_return", short, "rc", {"q": q, "code": byte_early})
                details["r3_byte_string_early_return"][it["id"]] = byte_early
                if (it["id"], t["reference"]) == REVIEW5_ITEM:  # the reviewer's exact answer
                    for reader in ("r3_review5_reproducer", "r3_review5_reproducer" + FORM_RULE_OFF):
                        put(reader, short, "rc", {"q": q, "code": REVIEW5_REPRODUCER})
                    details["r3_review5_reproducer"][it["id"]] = REVIEW5_REPRODUCER
                if not rebuild_call(pick)[0]:
                    # with the rule off it must compile: the call is written twice, which client
                    # setters (they take the client by value) would not allow
                    put("r3_early_return" + FORM_RULE_OFF, short, "rc", {"q": q, "code": early})
                    put("r3_byte_string_early_return" + FORM_RULE_OFF, short, "rc", {"q": q, "code": byte_early})
                for reader, last in POSITIVE_FORMS.items():
                    put(reader, short, "rc", {"q": q, "code": control_code(pick, last=last)})
                if n_fs < SANDBOX_CONTROL_ITEMS:
                    n_fs += 1
                    path = os.path.join(escape_dir, re.sub(r"\W", "_", it["id"]) + ".txt")
                    details["r3_sandbox_fs"]["paths"][it["id"]] = path
                    # `use std::{fs}` passes the regex filter: only the sandbox stands in the way
                    put("r3_sandbox_fs", short, "rc", {"q": q, "code": control_code(
                        pick, prefix=f"use std::{{fs}};\nfs::write({C.rust_str(path)}, b\"escaped\")?;")})
                if n_net < SANDBOX_CONTROL_ITEMS:
                    n_net += 1
                    ip = ", ".join(net_addr[0].split("."))
                    put("r3_sandbox_net", short, "rc", {"q": q, "code": control_code(
                        pick, prefix=f"use tokio::{{net}};\nlet _conn = net::TcpSocket::new_v4()?"
                                     f".connect(([{ip}], {net_addr[1]}).into()).await?;")})
                if ctl.get("home_file") and n_inc < SANDBOX_CONTROL_ITEMS:
                    n_inc += 1
                    details["r3_sandbox_include"]["items"].append(it["id"])
                    put("r3_sandbox_include", short, "rc", {"q": q, "code": control_code(
                        pick, prefix=f"let __probe_file: &str = include_str!({C.rust_str(ctl['home_file'])});")})
                if n_env < SANDBOX_CONTROL_ITEMS:
                    n_env += 1
                    details["r3_sandbox_env"]["items"].append(it["id"])
                    put("r3_sandbox_env", short, "rc", {"q": q, "code": control_code(
                        pick, prefix='println!("@@SANDBOXENV {:?} {:?}", option_env!("HOME"), option_env!("CODEX_HOME"));')})
                if ipc_left:
                    target = ipc_left.pop(0)
                    details["r3_sandbox_ipc"]["targets"][it["id"]] = target
                    put("r3_sandbox_ipc", short, "rc", {"q": q, "code": control_code(
                        pick, prefix=f"let __probe_ipc = std::os::unix::net::UnixStream::connect({C.rust_str(target)})?;")})
    return readers, details


def load_runs(runs_dir, banks):
    readers, metas = {}, {}
    for provider in sorted(os.listdir(runs_dir)):
        pdir = os.path.join(runs_dir, provider)
        if not os.path.isdir(pdir):
            continue
        for d in sorted(os.listdir(pdir)):
            if "." not in d:
                continue
            short, session = d.rsplit(".", 1)
            if short not in banks:
                continue
            ans_p, meta_p = os.path.join(pdir, d, "answer.json"), os.path.join(pdir, d, "meta.json")
            ans = C.read_json(ans_p) if os.path.isfile(ans_p) else None
            meta = C.read_json(meta_p) if os.path.isfile(meta_p) else {}
            readers.setdefault(provider, {}).setdefault(short, {})[session] = ans
            metas.setdefault(provider, {})[f"{short}.{session}"] = {
                k: meta.get(k) for k in ("requested", "observed", "failed", "answer_error", "attempts")}
    return readers, metas


def rate(vals):
    v = [x for x in vals if x is not None]
    return {"rate": round(sum(1 for x in v if x) / len(v), 4) if v else None, "n": len(v)}


def aggregate(item_rows):
    """item_rows: [{"kind", field: bool|None}] → rates per metric field."""
    agg = {}
    wires = [r for r in item_rows if r["kind"] == "wire"]
    for f in R1_FIELDS + ("critical", "exact", "diag_query_decoded"):
        agg[f"R1.{f}"] = rate([r.get(f) for r in wires])
    res = [r for r in item_rows if r["kind"] == "result"]
    for f in ("send_returns", "error_type", "base_url"):
        agg[f"R2.{f}"] = rate([r.get(f) for r in res])
    calls = [r for r in item_rows if r["kind"] == "call"]
    agg["R3.pass"] = rate([r.get("pass") for r in calls])
    for metric, f in R3_DIAG.items():
        agg[metric] = rate([r.get(f) for r in calls])
    return agg


# R3 diagnostics: compiles only; the old method/path/query-only rule; the item's result was checked;
# the request check alone on items whose result is not checked (never part of R3.pass)
R3_DIAG = {"R3.diag_compiles": "compiles", "R3.diag_legacy_pass": "diag_legacy_pass",
           "R3.diag_result_checked": "result_checked", "R3.diag_pass_result_unchecked": "diag_pass_result_unchecked"}


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--bank", required=True)
    ap.add_argument("--runs")
    ap.add_argument("--out")
    ap.add_argument("--sdk", nargs="*")
    ap.add_argument("--selftest", action="store_true")
    ap.add_argument("--no-r3", action="store_true")
    ap.add_argument("--repo", default=C.REPO)
    a = ap.parse_args()
    bank_dir = os.path.abspath(a.bank)
    repo = os.path.abspath(a.repo)
    banks, truths = load(bank_dir, a.sdk)
    sdk_objs = {s: C.Sdk(os.path.join(repo, "examples", banks[s]["sdk"]), repo) for s in banks}
    listener, ulistener, details, effects, ctl, extra_env = None, None, {}, {}, {}, {}
    if a.selftest:
        # sandbox controls: a host-writable directory outside the sandbox's scratch, and a TCP
        # listener on a non-loopback host address that the host itself can reach
        escape_dir = os.path.join(repo, "target", "tmp", "probe-sandbox-escape")
        os.makedirs(escape_dir, exist_ok=True)
        probe = os.path.join(escape_dir, "host-can-write")
        with open(probe, "w") as f:
            f.write("x")
        effects["fs_dir_host_writable"] = os.path.isfile(probe)
        os.remove(probe)
        ip = host_address()
        if ip:
            listener = Listener(ip)
            with socket.create_connection(listener.addr, timeout=2):
                pass
            deadline = 50
            while listener.accepted < 1 and deadline:
                deadline -= 1
                threading.Event().wait(0.05)
            effects["net_listener"] = f"{listener.addr[0]}:{listener.addr[1]}"
            effects["net_host_reached_listener"] = listener.accepted >= 1
            listener.accepted = 0
            net_addr = listener.addr
        else:
            effects["net_listener"] = None
            effects["net_host_reached_listener"] = None
            net_addr = ("192.0.2.1", 9)  # TEST-NET-1: no listener; reachability on the host not shown
        ctl = {"net_addr": net_addr}
        if not a.no_r3:
            # compile-time and IPC controls: a real file in the home directory, host sockets under
            # /run (and any other the run layout would show), and a host CODEX_HOME for
            # option_env! (a canary value when the host has none, so the control discriminates)
            ctl["home_file"] = home_file(escape_dir)
            ulistener, ctl["ipc_targets"] = ipc_setup()
            ctl["codex_home"] = os.environ.get("CODEX_HOME") or f"/nonexistent/concord-probe-host-codex-home-{secrets.token_hex(4)}"
            extra_env = {} if os.environ.get("CODEX_HOME") else {"CODEX_HOME": ctl["codex_home"]}
        readers, details = selftest_answers(banks, truths, sdk_objs, repo, ctl)
        for paths in details["r3_sandbox_fs"]["paths"].values():
            if os.path.exists(paths):
                os.remove(paths)
        metas = {}
    else:
        readers, metas = load_runs(os.path.abspath(a.runs), banks)
    base_readers = ("oracle", "wrong", "broken")
    rows = {}  # reader -> [row]
    r3_jobs = []
    for reader, per_sdk in readers.items():
        for short, sessions in per_sdk.items():
            b, tr = banks[short], truths[short]["items"]
            for it in b["items"]:
                t = tr.get(it["id"])
                if t is None:
                    continue
                ans_obj = sessions.get(it["session"])
                answers = {a_.get("q"): a_ for a_ in (ans_obj or {}).get("answers", []) if isinstance(a_, dict)}
                ans = answers.get(it["q"])
                if a.selftest and reader not in base_readers and ans is None:
                    continue  # a control reader answers only the items its perturbation applies to
                row = {"reader": reader, "sdk": short, "id": it["id"], "kind": it["kind"],
                       "boundary": it.get("boundary", "prepared"), "answered": ans is not None,
                       "malformed_session": ans_obj is None}
                canaries = b["picks"][it["pick"]]["canaries"]
                if it["kind"] == "wire":
                    row.update(score_wire(t, ans, canaries))
                elif it["kind"] == "result":
                    row.update(score_result(t, ans, sdk_objs[short], sdk_objs[short].decl_module(b["picks"][it["pick"]]["client_file"])))
                    if a.selftest and reader not in base_readers and "base_url" not in ans:
                        row.pop("base_url", None)  # R2 controls answer the types only
                elif it["kind"] == "call":
                    row["code"] = (ans or {}).get("code")
                    r3_jobs.append((reader, it["id"], row["code"]))
                rows.setdefault(reader, []).append(row)
    log = []
    if r3_jobs and not a.no_r3:
        kw = {}
        if a.selftest:
            stream_fx = details.get("r3_stream_wrong_media", {})
            kw = {"guard_off": GUARD_OFF, "form_off": FORM_OFF, "unbound": tuple(UNBOUND_EXPECT),
                  "parent_env": dict(os.environ, **extra_env),
                  "fixtures": {(r, iid): fx for iid, fx in stream_fx.items()
                               for r in ("r3_stream_wrong_media", "r3_stream_wrong_media" + UNBOUND)}}
        r3, ctx = r3_compile_and_check(repo, banks, truths, r3_jobs, log, **kw)
        for reader, rs in rows.items():
            for row in rs:
                if row["kind"] == "call":
                    row.update(r3.get((reader, row["id"]), {}))
        if a.selftest:
            if listener:
                threading.Event().wait(0.3)
                effects["net_sandbox_connections"] = listener.accepted
            fs = details.get("r3_sandbox_fs", {}).get("paths", {})
            effects["fs_files_written"] = sorted(p for p in fs.values() if os.path.exists(p))
            effects["fs_paths_checked"] = sorted(fs.values())
            effects.update(selftest_effects(ctx, r3, r3_jobs, details, ctl, ulistener, banks, truths, repo, log, extra_env))
    if listener:
        listener.close()
    if ulistener:
        ulistener.close()
    report = {"bank": bank_dir, "mode": "selftest" if a.selftest else "runs", "runs": a.runs,
              "measures": "docs-assisted prediction", "readers": {}, "pooled": {}, "r3_compile_log": log,
              "metas": metas}
    for reader, rs in rows.items():
        by_sdk = defaultdict(list)
        for r in rs:
            by_sdk[r["sdk"]].append(r)
        report["readers"][reader] = {"overall": aggregate(rs),
                                     "by_sdk": {s: aggregate(v) for s, v in sorted(by_sdk.items())},
                                     "by_boundary": {bd: aggregate([r for r in rs if r["boundary"] == bd])
                                                     for bd in ("prepared", "operation")},
                                     "items": rs}
    # pooled over readers: each item averaged over readers first (readers are not participants)
    if len(rows) > 1 and not a.selftest:
        per_item = defaultdict(list)
        for rs in rows.values():
            for r in rs:
                per_item[r["id"]].append(r)
        pooled = {}
        for metric in list(aggregate([]).keys()):
            kind = {"R1": "wire", "R2": "result", "R3": "call"}[metric.split(".")[0]]
            f = R3_DIAG.get(metric, metric.split(".", 1)[1])
            vals = []
            for iid, rs in per_item.items():
                v = [r.get(f) for r in rs if r["kind"] == kind and r.get(f) is not None]
                if v:
                    vals.append(sum(1 for x in v if x) / len(v))
            pooled[metric] = {"rate": round(sum(vals) / len(vals), 4) if vals else None, "n_items": len(vals)}
        report["pooled"] = pooled
    if a.selftest:
        report["selftest"] = selftest_verdict(report, effects, details, a.no_r3, bool(a.sdk))
    out = os.path.abspath(a.out or (os.path.join(a.runs, "score") if a.runs else os.path.join(bank_dir, "selftest")))
    C.write_json(os.path.join(out, "score.json"), report)
    md = render_md(report)
    open(os.path.join(out, "score.md"), "w").write(md)
    print(md)


def selftest_effects(ctx, r3, jobs, details, ctl, ulistener, banks, truths, repo, log, extra_env):
    """The sandbox effects of the compile-time, environment and IPC controls, inside and outside
    the sandbox: the same IPC answers run on the host from the sandboxed build, and the same
    include/env answers compiled on the host with the inherited environment (README "Self-test")."""
    eff = {}
    codes = {(r, i): c for r, i, c in jobs}
    sb = ctx.get("sandbox")
    filters = {}
    form_check(list(codes.values()))
    for r in dict.fromkeys(GUARD_OFF + FORM_OFF + EARLY_EXITS + tuple(POSITIVE_FORMS)):
        cs = [c for (rr, _), c in codes.items() if rr == r]
        filters[r] = {"n": len(cs), "guard_refused": sum(1 for c in cs if GUARD.search(c)),
                      "lexes": sum(1 for c in cs if lexes(c)),
                      "form_refused": sum(1 for c in cs if call_form(c)[1] is None)}
    eff["filters_alone"] = filters
    eff["review5_reproducer_refusal"] = call_form(REVIEW5_REPRODUCER)[2]
    # `-> stream` call items: result checked, with a reply of the format's media type
    eff["stream_items"] = {}
    for short, b in banks.items():
        for it in b["items"]:
            t = truths[short]["items"].get(it["id"])
            if it["kind"] == "call" and t and (b["picks"][it["pick"]].get("output") or "").strip().startswith("stream"):
                fx = t.get("fixture") or {}
                eff["stream_items"][it["id"]] = {"result_checked": bool(t.get("result_checked")),
                                                 "binding": t.get("binding"), "fixture": fx.get("name"),
                                                 "content_type": fx.get("content_type"),
                                                 "fixture_tries_failed": t.get("fixture_tries_failed")}
    eff["sockets"] = {"run_unmasked": run_sockets(),
                      "run_masked": scan_sockets(run_layout() + socket_masks(run_sockets())),
                      "compile_unmasked": sb.sockets if sb else None,
                      "compile_masked": scan_sockets(sb.mounts(masked=True)) if sb else None}
    keys = lambda reader: sorted(k for k in codes if k[0] == reader)  # noqa: E731
    inc_keys, env_keys, ipc_keys = keys("r3_sandbox_include"), keys("r3_sandbox_env"), keys("r3_sandbox_ipc")
    eff["include"] = {"file": ctl.get("home_file"), "file_exists_on_host": bool(ctl.get("home_file")) and os.path.isfile(ctl["home_file"]),
                      "inside": {k[1]: {"compiles": r3[k].get("compiles"), "reason": r3[k].get("reason")} for k in inc_keys}}
    eff["env"] = {"sandbox_home": sb.sandbox_home if sb else None, "host_home": os.environ.get("HOME"),
                  "codex_home_host": ctl.get("codex_home"), "codex_home_injected_canary": bool(extra_env),
                  "inside": {k[1]: r3[k].get("marks", []) for k in env_keys}}
    eff["ipc"] = {"targets": details.get("r3_sandbox_ipc", {}).get("targets", {}),
                  "listener": ulistener.path if ulistener else None,
                  "inside": {k[1]: {"compiles": r3[k].get("compiles"), "recorded": len(r3[k].get("recorded") or []),
                                    "outcome": (r3[k].get("outcome") or "")[:300]} for k in ipc_keys}}
    if ulistener:
        threading.Event().wait(0.3)
        eff["ipc"]["listener_inside_connections"], ulistener.accepted = ulistener.accepted, 0
    if ctx.get("exes"):
        run_root = os.path.join(repo, "target", "tmp", "probe-score-run")
        outside = r3_outside_runs(ctx, ipc_keys, banks, truths, run_root)
        eff["ipc"]["outside"] = {k[1]: v for k, v in outside.items()}
        if ulistener:
            threading.Event().wait(0.3)
            eff["ipc"]["listener_outside_connections"] = ulistener.accepted
        oc = r3_outside_compile(ctx, repo, banks, truths, [(r, i, codes[(r, i)]) for r, i in inc_keys + env_keys],
                                log, extra_env, env_keys)
        eff["include"]["outside_compiles"] = {k[1]: oc[k]["compiles"] for k in inc_keys if k in oc}
        eff["include"]["outside_reasons"] = {k[1]: oc[k]["reason"] for k in inc_keys if k in oc and oc[k]["reason"]}
        eff["env"]["outside"] = {k[1]: oc[k]["marks"] for k in env_keys if k in oc}
    return eff


def selftest_verdict(report, effects, details, no_r3, subset):
    """Every expected rate (SELFTEST_EXPECT, and the discrimination references in REFERENCE_EXPECT)
    next to the observed one, and the sandbox and rule effect checks. A control with no
    applicable item fails the verdict on the whole bank and is `n/a` when `--sdk` selects a subset."""
    rows, ok = [], True
    for reader, exp in list(SELFTEST_EXPECT.items()) + list(REFERENCE_EXPECT.items()):
        overall = report["readers"].get(reader, {}).get("overall", {})
        for metric, want in exp.items():
            if no_r3 and metric.startswith("R3"):
                continue
            got = overall.get(metric, {"rate": None, "n": 0})
            if subset and not got["n"]:
                rows.append({"reader": reader, "metric": metric, "expected": want, "observed": None, "n": 0, "ok": None})
                continue
            good = got["rate"] is not None and got["n"] > 0 and abs(got["rate"] - want) < 1e-9
            ok &= good
            rows.append({"reader": reader, "metric": metric, "expected": want, "observed": got["rate"], "n": got["n"], "ok": good})
    checks = []

    def check(name, present, cond):
        checks.append((name, (None if subset else False) if not present else bool(cond)))

    if not no_r3:
        check("the escape directory is writable on the host", True, effects.get("fs_dir_host_writable") is True)
        check("no file written outside the sandbox scratch", bool(effects.get("fs_paths_checked")),
              effects.get("fs_files_written") == [])
        if effects.get("net_listener"):
            check("the host itself reaches the non-loopback listener", True, effects.get("net_host_reached_listener") is True)
            check("no connection from the sandbox reached the listener", True, effects.get("net_sandbox_connections") == 0)
        inc = effects.get("include", {})
        ins, outc = inc.get("inside", {}), inc.get("outside_compiles", {})
        check("the include_str! control's file exists in the host's home directory", True, inc.get("file_exists_on_host"))
        check("include_str! of that file fails to compile in the sandbox (the file is not visible)", bool(ins),
              all(not v["compiles"] and "couldn't read" in (v["reason"] or "") for v in ins.values()))
        check("the same include_str! answers compile outside the sandbox", bool(ins),
              set(outc) == set(ins) and all(outc.values()))
        env = effects.get("env", {})
        want_in = f'Some("{env.get("sandbox_home")}") None'
        want_out = f'Some("{env.get("host_home")}") Some("{env.get("codex_home_host")}")'
        check("compiled in the sandbox, option_env!(\"HOME\") is the sandbox's and option_env!(\"CODEX_HOME\") is None",
              bool(env.get("inside")), all(v == [want_in] for v in env["inside"].values()))
        check("compiled outside the sandbox, the same answers see the host's HOME and CODEX_HOME",
              bool(env.get("inside")), set(env.get("outside", {})) == set(env["inside"])
              and all(v == [want_out] for v in env["outside"].values()))
        ipc = effects.get("ipc", {})
        inside, outside = ipc.get("inside", {}), ipc.get("outside", {})
        check("a UnixStream connect to each host socket fails in the sandbox (no request follows)", bool(inside),
              all(v["compiles"] and v["recorded"] == 0 and v["outcome"] for v in inside.values())
              and ipc.get("listener_inside_connections", 0) == 0)
        check("the same compiled connects succeed outside the sandbox (the call then completes)", bool(inside),
              set(outside) == set(inside) and all(v["complete"] and v["outcome"] is None and v["requests"] == v["reference_requests"]
                                                  for v in outside.values())
              and (ipc.get("listener") is None or ipc.get("listener_outside_connections", 0) >= 1))
        socks = effects.get("sockets", {})
        check("no pathname Unix socket is visible in the run or the compile sandbox (after masking)", True,
              socks.get("run_masked") == [] and socks.get("compile_masked") == [])
        fa = effects.get("filters_alone", {})

        def refused_all(readers, key):
            return (any(fa.get(r, {}).get("n") for r in readers),
                    all(fa.get(r, {}).get(key) == fa.get(r, {}).get("n") for r in readers))

        check("the regex guard alone also refuses every include/env/ipc control answer", *refused_all(GUARD_OFF, "guard_refused"))
        check("the call-form rule alone also refuses every result-evasion answer", *refused_all(EVASION_READERS, "form_refused"))
        check("the call-form rule alone refuses every early-return answer, also the review-5 ones between raw byte strings "
              "(which, with the rule off, compile and pass)", *refused_all(tuple(FORM_OFF_EXPECT), "form_refused"))
        rv = ("r3_review5_reproducer", "r3_byte_string_early_return")
        check("the review-5 answers pass the regex guard and lex as balanced Rust tokens: only the parser refuses them "
              "(the reviewer's answer for its `return`, its inner `?` and its inner `.await`)",
              any(fa.get(r, {}).get("n") for r in rv),
              all(fa.get(r, {}).get("guard_refused") == 0 and fa.get(r, {}).get("lexes") == fa.get(r, {}).get("n")
                  and fa.get(r, {}).get("form_refused") == fa.get(r, {}).get("n") for r in rv)
              and fa.get("r3_review5_reproducer", {}).get("n") == 1
              and all(w in (effects.get("review5_reproducer_refusal") or "")
                      for w in ("`return` outside a closure", "a `?` other than", "an `.await` other than")))
        check("the call-form rule accepts every `vec![x; n]`-in-a-struct-literal and closure-with-statements answer",
              any(fa.get(r, {}).get("n") for r in POSITIVE_FORMS),
              all(fa.get(r, {}).get("form_refused") == 0 for r in POSITIVE_FORMS))
        check("the call-form rule alone also refuses every file-system/network/include/env/ipc control answer (statements)",
              *refused_all(SANDBOX_READERS, "form_refused"))
        si = effects.get("stream_items", {})
        check("every `-> stream` call item has its result checked, with a reply that is not text/plain", bool(si),
              all(v["result_checked"] and not (v["content_type"] or "").startswith("text/plain") for v in si.values()))
    for _, good in checks:
        ok &= good is not False
    return {"all_ok": ok, "expectations": rows, "effects": effects,
            "effect_checks": [{"check": c, "ok": g} for c, g in checks],
            "controls": {k: v for k, v in details.items() if k != "r3_sandbox_net"} | {"r3_sandbox_net": details.get("r3_sandbox_net")}}


def fmt(x):
    return "–" if x["rate"] is None else f"{x['rate']:.2f} ({x.get('n', x.get('n_items'))})"


def render_md(report):
    lines = [f"# I3 scores ({report['mode']}): docs-assisted prediction", ""]
    metrics = list(aggregate([]).keys())
    readers = list(report["readers"])
    lines.append("| metric | " + " | ".join(readers) + (" | pooled (items) |" if report["pooled"] else " |"))
    lines.append("|---" * (len(readers) + 1 + (1 if report["pooled"] else 0)) + "|")
    for m in metrics:
        row = [fmt(report["readers"][r]["overall"][m]) for r in readers]
        if report["pooled"]:
            row.append(fmt(report["pooled"][m]))
        lines.append(f"| {m} | " + " | ".join(row) + " |")
    lines += ["", "Rates are correct/applicable (n). R1.critical = method + destination + query; "
              "R3.pass = compiles and, run in the sandbox, sends the reference's requests (method, path, query, "
              "headers, body) and succeeds where the result is checked; R3.diag_compiles = compiles only; "
              "R3.diag_legacy_pass = the old method/path/query-only rule; R3.diag_result_checked = items whose "
              "reply decodes into the declared output; R3.diag_pass_result_unchecked = the request check alone on items "
              "whose result is not checked (never in R3.pass). Readers ending in `__unbound` (bound without the declared "
              "type or result check) and `__form_off` (compiled with the call-form rule off) are self-test "
              "discrimination references, not scores.", ""]
    st = report.get("selftest")
    if st:
        lines += [f"## Self-test verdict: {'ALL EXPECTATIONS MET' if st['all_ok'] else 'EXPECTATIONS NOT MET'}", "",
                  "| reader | metric | expected | observed (n) | ok |", "|---|---|---|---|---|"]
        for r in st["expectations"]:
            obs = "–" if r["observed"] is None else f"{r['observed']:.2f}"
            mark = "n/a (subset)" if r["ok"] is None else ("yes" if r["ok"] else "NO")
            lines.append(f"| {r['reader']} | {r['metric']} | {r['expected']} | {obs} ({r['n']}) | {mark} |")
        lines += ["", "| sandbox effect check | ok |", "|---|---|"]
        lines += [f"| {c['check']} | {'n/a (subset)' if c['ok'] is None else ('yes' if c['ok'] else 'NO')} |"
                  for c in st["effect_checks"]]
        lines += ["", "Effects: `" + json.dumps(st["effects"]) + "`", ""]
    for r in readers:
        lines.append(f"## {r}: per SDK")
        lines.append("")
        lines.append("| sdk | R1.critical | R1.exact | R2.send_returns | R2.error_type | R2.base_url | R3.pass | R3.diag_compiles |")
        lines.append("|---|---|---|---|---|---|---|---|")
        for s, agg in report["readers"][r]["by_sdk"].items():
            lines.append(f"| {s} | " + " | ".join(fmt(agg[k]) for k in ("R1.critical", "R1.exact", "R2.send_returns",
                                                                        "R2.error_type", "R2.base_url", "R3.pass",
                                                                        "R3.diag_compiles")) + " |")
        lines.append("")
    return "\n".join(lines) + "\n"


if __name__ == "__main__":
    main()
