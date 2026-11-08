"""H03 harness: throwaway arm trees, sandboxed compile and run of generated snippets (I3 R3 reuse).

Every snippet is trusted Rust from the sealed corpus. It is compiled inside a generated
**integration test** of its SDK package, `examples/<sdk>-sdk/tests/__h03_probe.rs`, in a
throwaway tree of the measured arm (`git archive <commit>`, every file given a fresh
modification time, a provenance marker `.h03-provenance` naming the arm and commit and the
SHA-256 of the archive stream). An integration test is a separate crate: it sees the SDK's
public API, as a caller does. The I3 sandboxes and plumbing are reused unchanged:

- compile: `probes/score.py` `CompileSandbox` (bubblewrap, read-only tree, no network, empty
  environment, `cargo --offline --frozen`) driven by `probes/truth.py` `compile_rounds` (errors
  mapped to the generated block by line, failing blocks dropped, rebuilt until clean), with
  `--lib` replaced by `--test __h03_probe`; the first error of each failing block is kept with
  its rendered text (`truth.map_error` is wrapped to record `rendered`);
- run: `probes/score.py` `sandbox_run` (bubblewrap, scratch directory the only writable path,
  loopback-only network, nonce-tagged records), one test per process, against a local receiver
  (`concord-example-testserver`) that answers every request with the snippet's reply;
- requests: `probes/truth.py` `exchanges`/`req_record` and `probes/score.py` `compare_requests`
  (method, path and query pairs in order, header names, header values, body).

Each test prints the tree's provenance marker (compiled in with `include_str!`), which the
harness compares with the tree it built; freshness is read from cargo's JSON
(`compiler-artifact.fresh` of the workspace crates in the first compile round).

Cargo: `CARGO_TARGET_DIR=<WS>/work/targets/base/h03-<arm>` (one per arm, never shared),
`CARGO_BUILD_JOBS` (default 6), one cargo command at a time. Python: `python3 -I`.
"""
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "probes"))
import common as C  # noqa: E402
import score as S  # noqa: E402
import truth as T  # noqa: E402

WS = "/home/canarit/projects/Concord_Desirability_Loop"
REPO = "/home/canarit/projects/concord_api"
BANK = os.path.join(WS, "work", "sealed", "h03-bank")
RUNS = os.path.join(WS, "work", "sealed", "h03-runs")
# arm label -> commit; a later arm can also be measured with `--arm LABEL --commit SHA`
ARMS = {"baseline": "a883f7bb", "A": "d793fecc", "B": "7f14da4d", "Aprime": "fc16090d", "Bprime": "1329a3e5"}
TEST_NAME = "__h03_probe"
MARKER = ".h03-provenance"
RUN_LIMIT_S = 20
SDKS = ["kraken-spot-rest", "cloudflare-zones", "notion", "frankfurter", "nager-holidays", "pokeapi"]


def target_dir(arm):
    return os.path.join(WS, "work", "targets", "base", f"h03-{arm}")


def full_sha(commit):
    return subprocess.run(["git", "-C", REPO, "rev-parse", commit + "^{commit}"], check=True,
                          capture_output=True, text=True).stdout.strip()


def archive_sha(sha):
    p = subprocess.Popen(["git", "-C", REPO, "archive", sha], stdout=subprocess.PIPE)
    h = hashlib.sha256()
    for chunk in iter(lambda: p.stdout.read(1 << 20), b""):
        h.update(chunk)
    if p.wait():
        raise SystemExit(f"git archive {sha} failed")
    return h.hexdigest()


def make_tree(arm, dest, commit=None):
    """A fresh throwaway tree of `arm` at `dest` (emptied first): `git archive`, fresh mtimes,
    provenance marker. Returns the marker dict."""
    sha = full_sha(commit or ARMS[arm])
    if os.path.isdir(dest):
        shutil.rmtree(dest)
    os.makedirs(dest)
    arch = subprocess.Popen(["git", "-C", REPO, "archive", sha], stdout=subprocess.PIPE)
    subprocess.run(["tar", "-x", "-C", dest], stdin=arch.stdout, check=True)
    if arch.wait():
        raise SystemExit(f"git archive {sha} failed")
    now = time.time()
    for dp, dn, fn in os.walk(dest):
        for f in fn + dn:
            try:
                os.utime(os.path.join(dp, f), (now, now), follow_symlinks=False)
            except OSError:
                pass
    prov = {"arm": arm, "commit": sha, "archive_sha256": archive_sha(sha)}
    with open(os.path.join(dest, MARKER), "w") as f:
        f.write(json.dumps(prov, sort_keys=True) + "\n")
    return prov


def check_tree(dest, arm, commit=None):
    """The tree's marker names `arm` and its commit; the archive hash is recomputed."""
    prov = json.load(open(os.path.join(dest, MARKER)))
    sha = full_sha(commit or ARMS[arm])
    problems = []
    if prov.get("arm") != arm:
        problems.append(f"marker arm {prov.get('arm')} != {arm}")
    if prov.get("commit") != sha:
        problems.append(f"marker commit {prov.get('commit')} != {sha}")
    if prov.get("archive_sha256") != archive_sha(sha):
        problems.append("archive hash differs")
    return prov, problems


HEADER = r'''//! Generated by tools/desirability/h03/harness.py: @@WHAT@@. Do not edit; regenerate.
#![allow(warnings, unused, dead_code, missing_docs, unreachable_code, non_snake_case, unused_imports, clippy::all)]
@@IMPORTS@@
use ::@@TESTSERVER@@ as __h03_ts;

/// The tree's provenance marker (arm, commit, archive hash), echoed in every record.
const __H03_PROVENANCE: &str = include_str!(@@PROVPATH@@);

fn __esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// The nonce the harness left for this run (read once, then removed).
fn __take_nonce() -> String {
    match ::std::env::var("CONCORD_PROBE_NONCE_FILE") {
        Ok(p) => {
            let n = ::std::fs::read_to_string(&p).unwrap_or_default();
            let _ = ::std::fs::remove_file(&p);
            n.trim().to_string()
        }
        Err(_) => String::new(),
    }
}

/// `@@OK` for an `Ok` send result, `@@ERR <Display>` for an `Err`.
fn __h03_outcome<T, E: ::core::fmt::Display>(r: &::core::result::Result<T, E>) -> String {
    match r {
        ::core::result::Result::Ok(_) => "@@OK".to_string(),
        ::core::result::Result::Err(e) => format!("@@ERR {}", e),
    }
}

fn __h03_print(id: &str, nonce: &str, reqs: &[__h03_ts::Recorded], outcome: &Option<String>, done: bool) {
    let head = format!("\"id\":{},\"part\":\"h03\",\"cand\":\"\",\"nonce\":{},\"fixture\":\"\",\"pass\":0", __esc(id), __esc(nonce));
    for (n, r) in reqs.iter().enumerate() {
        let mut hs = String::new();
        for (k, v) in &r.headers {
            if !hs.is_empty() {
                hs.push(',');
            }
            hs.push_str(&format!("[{},{}]", __esc(k), __esc(v)));
        }
        println!(
            "@@PROBE {{{},\"n\":{},\"of\":{},\"done\":{},\"method\":{},\"target\":{},\"headers\":[{}],\"body\":{}}}",
            head, n, reqs.len(), done, __esc(&r.method), __esc(&r.target), hs, __esc(&String::from_utf8_lossy(&r.body))
        );
    }
    let outcome = match outcome {
        None => "null".to_string(),
        Some(e) => __esc(e),
    };
    println!("@@PROBE {{{},\"summary\":true,\"of\":{},\"done\":{},\"outcome\":{},\"prov\":{}}}", head, reqs.len(), done, outcome,
             __esc(__H03_PROVENANCE.trim()));
}

/// Runs `run(base)` once against a fresh local receiver that answers every request with
/// `status` and the JSON `body`, and prints what it recorded (I3 record format, nonce-tagged).
fn __h03_record<F, Fut>(id: &str, status: u16, body: &'static [u8], limit_s: u64, run: F)
where
    F: FnOnce(String) -> Fut,
    Fut: ::core::future::Future<Output = Option<String>>,
{
    let nonce = __take_nonce();
    let rt = ::tokio::runtime::Builder::new_current_thread().enable_all().build().expect("runtime");
    rt.block_on(async move {
        let server = __h03_ts::TestServer::start(move |_r: &__h03_ts::Recorded| __h03_ts::Reply::json(status, body)).await;
        let out = ::tokio::time::timeout(::std::time::Duration::from_secs(limit_s), run(server.url())).await;
        let (outcome, done) = match out {
            Ok(o) => (o, true),
            Err(_) => (Some(format!("timed out after {} s", limit_s)), false),
        };
        __h03_print(id, &nonce, &server.requests(), &outcome, done);
    });
}
'''


def sdk_dir(sdk):
    return f"{sdk}-sdk"


def header(tree, sdk, imports, what):
    ts = T.testserver_crate(os.path.join(tree, "examples", sdk_dir(sdk)))
    prov = json.dumps(os.path.join(tree, MARKER))
    return (HEADER.replace("@@WHAT@@", what).replace("@@IMPORTS@@", "\n".join(imports))
            .replace("@@TESTSERVER@@", ts).replace("@@PROVPATH@@", prov))


def snippet_fn(fname, api, prelude, call):
    """The snippet as an async fn returning the outcome string (`?` available)."""
    body = [f"    let api = {api};"]
    body += [f"    {ln}" for ln in prelude]
    body += [f"    let __r = {call}.send().await;", "    Ok(__h03_outcome(&__r))"]
    return (f"async fn {fname}(base: String) -> ::core::result::Result<String, ::std::boxed::Box<dyn ::std::error::Error>> {{\n"
            + "\n".join(body) + "\n}")


def run_fn(tag, rid, fname, reply_bytes, base_path, limit_s=RUN_LIMIT_S, status=200):
    lit = "b\"" + "".join(chr(b) if 32 <= b < 127 and chr(b) not in '"\\' else f"\\x{b:02x}" for b in reply_bytes) + "\""
    return (f"#[test]\nfn __h03_t_{tag}() {{\n"
            f"    __h03_record({C.rust_str(rid)}, {int(status)}, {lit}, {int(limit_s)}, |base: String| async move {{\n"
            f"        Some(match {fname}(format!(\"{{}}{{}}\", base, {C.rust_str(base_path)})).await {{\n"
            f"            ::core::result::Result::Ok(s) => s,\n"
            f"            ::core::result::Result::Err(e) => format!(\"@@SETUP {{}}\", e),\n"
            f"        }})\n    }});\n}}")


class Batch:
    """Generated blocks for one compile of one tree: per SDK one integration-test module."""

    def __init__(self, tree, arm, target, inputs, what):
        self.tree, self.arm, self.target, self.inputs, self.what = tree, arm, target, inputs, what
        self.modules = {}
        self.runs = {}       # key -> (sdk, test fn name, rid, tag)

    def module(self, sdk):
        if sdk not in self.modules:
            info = self.inputs[sdk]
            path = os.path.join(self.tree, "examples", sdk_dir(sdk), "tests", f"{TEST_NAME}.rs")
            self.modules[sdk] = T.Module(info["package"], None, path,
                                         header(self.tree, sdk, info["imports"], f"{self.what} ({sdk})"))
        return self.modules[sdk]

    def add_block(self, sdk, tag, code):
        self.module(sdk).add(tag, code)

    def add_run(self, key, sdk, tag, api, prelude, call, reply_bytes, base_path):
        tag = re.sub(r"\W", "_", tag)
        fname = f"__h03_s_{tag}"
        rid = str(key)
        code = snippet_fn(fname, api, prelude, call) + "\n\n" + run_fn(tag, rid, fname, reply_bytes, base_path)
        self.module(sdk).add(tag, code)
        self.runs[key] = (sdk, f"__h03_t_{tag}", rid, tag)

    def compile(self, log, sandbox=True):
        """Compile every module (sandboxed unless `sandbox=False`, for trusted probes only).
        Returns (failed {tag: first error}, rendered {tag: rendered first error},
        exes {pkg: test executable}, freshness record)."""
        if sandbox and not (os.access(S.BWRAP, os.X_OK) and shutil.which("timeout")):
            raise SystemExit(f"H03 needs {S.BWRAP} and timeout(1): refusing to compile without a sandbox")
        mods = list(self.modules.values())
        exes, fresh, rounds = {}, {}, [0]
        os.makedirs(self.target, exist_ok=True)
        box = S.CompileSandbox(self.tree, self.target) if sandbox else None
        tree_url = "path+file://" + os.path.realpath(self.tree)

        def runner(cwd, args):
            args = list(args)
            i = args.index("--lib")
            args[i:i + 1] = ["--test", TEST_NAME]
            if box:
                r = box(cwd, args)
            else:
                env = C.cargo_env()
                env["CARGO_TARGET_DIR"] = self.target
                r = subprocess.run(["cargo"] + args[:1] + ["--offline", "--frozen"] + args[1:], cwd=cwd, env=env,
                                   capture_output=True, text=True)
            for line in r.stdout.splitlines():
                try:
                    ev = json.loads(line)
                except ValueError:
                    continue
                if ev.get("reason") != "compiler-artifact":
                    continue
                pid = ev.get("package_id", "")
                tgt = ev.get("target") or {}
                if rounds[0] == 0 and pid.startswith("path+file://"):
                    fresh.setdefault(pid, []).append({"target": tgt.get("name"), "kind": tgt.get("kind"), "fresh": ev.get("fresh")})
                if ev.get("executable") and tgt.get("name") == TEST_NAME:
                    for m in mods:
                        if f"#{m.pkg}@" in pid or f"/{m.pkg}#" in pid or pid.startswith(m.pkg + " "):
                            exes[m.pkg] = ev["executable"]
            rounds[0] += 1
            return r

        rendered = {}
        orig_map = T.map_error

        def mapping(msg, spans):
            tag = orig_map(msg, spans)
            if tag and tag not in rendered and msg.get("level", "").startswith("error"):
                rendered[tag] = msg.get("rendered") or msg.get("message", "")
            return tag

        log.append({"compile": box.describe() if box else "host cargo (trusted probes only)",
                    "tree": self.tree, "target": self.target, "blocks": sum(len(m.blocks) for m in mods)})
        t0 = time.monotonic()
        cwd = os.getcwd()
        os.chdir(self.tree)
        T.map_error = mapping
        try:
            failed = T.compile_rounds(self.tree, mods, log, artifacts=None, runner=runner)
        finally:
            T.map_error = orig_map
            os.chdir(cwd)
        log.append({"compile_seconds": round(time.monotonic() - t0, 1), "failed_blocks": len(failed), "rounds": rounds[0]})
        T.write_modules(mods)
        ws_pkgs = {pid: v for pid, v in fresh.items()}
        foreign = sorted(pid for pid in ws_pkgs if not pid.startswith(tree_url))
        stale = sorted(pid for pid, v in ws_pkgs.items() if pid.startswith(tree_url) and any(x["fresh"] for x in v))
        freshness = {"workspace_units_round0": len(ws_pkgs), "foreign_path_packages": foreign,
                     "reused_units_round0": stale}
        return failed, {t: rendered.get(t, "") for t in failed}, exes, freshness

    def run(self, exes, failed, rendered, log, run_root):
        """Run every compiled snippet test in the run sandbox. Returns {key: result}."""
        results, names = {}, {}
        for sdk, m in self.modules.items():
            exe = exes.get(m.pkg)
            if exe:
                for n in S.list_tests(exe, os.path.join(run_root, "list")):
                    names[(sdk, n.rsplit("::", 1)[-1])] = (exe, n)
        prov_text = open(os.path.join(self.tree, MARKER)).read().strip()
        jobs = []
        for key, (sdk, fname, rid, tag) in self.runs.items():
            if tag in failed:
                results[key] = {"compiles": False, "error": failed[tag], "rendered": rendered.get(tag, "")}
                continue
            if (sdk, fname) not in names:
                results[key] = {"compiles": True, "complete": False, "reason": "test executable or test not found",
                                "requests": [], "outcome": None}
                continue
            jobs.append((key, rid) + names[(sdk, fname)])
        from concurrent.futures import ThreadPoolExecutor
        t0 = time.monotonic()
        with ThreadPoolExecutor(max_workers=S.RUN_PARALLEL) as pool:
            futs = {pool.submit(S.sandbox_run, exe, [name, "--exact", "--nocapture", "--test-threads=1"],
                                os.path.join(run_root, str(i))): (key, rid) for i, (key, rid, exe, name) in enumerate(jobs)}
            for fut, (key, rid) in futs.items():
                recs, code, err, _, _ = fut.result()
                nonce = recs[0]["nonce"] if recs else ""
                mine = [r for r in recs if r.get("id") == rid]
                passes, _ = T.exchanges({rid: mine}, rid, "h03", nonce=nonce)
                p0 = passes.get(0)
                if p0 is None or not p0["summary"]:
                    results[key] = {"compiles": True, "complete": False, "requests": [], "outcome": None,
                                    "reason": f"no complete record from the sandboxed run (exit {code}): {err[-300:]}"}
                    continue
                summ = next((r for r in mine if r.get("summary")), {})
                results[key] = {"compiles": True, "complete": True, "outcome": p0["outcome"], "done": p0["done"],
                                "requests": [T.req_record(r, []) for r in p0["requests"]], "exit": code,
                                "provenance_ok": summ.get("prov") == prov_text}
        log.append({"run_seconds": round(time.monotonic() - t0, 1), "runs": len(jobs)})
        return results


def verdict(result, intended):
    """(pass, reason) for one snippet result against its intended request record."""
    if not result.get("compiles"):
        return False, "does not compile: " + result.get("error", "")
    if not result.get("complete"):
        return False, result.get("reason", "no complete record")
    if result.get("provenance_ok") is False:
        return False, "provenance marker mismatch"
    ok_req, why, _ = S.compare_requests(result["requests"], intended)
    if result.get("outcome") != "@@OK":
        return False, f"result not Ok: {result.get('outcome')}" + ("" if ok_req else f"; request differs: {why}")
    if not ok_req:
        return False, f"compiles, Ok, but sends a different request: {why}"
    return True, None


def bank_hash(bank=BANK):
    """SHA-256 of the sealed corpus's canonical listing: every regular file under
    `<bank>/inputs/` and `<bank>/corpus/`, one line `<sha256 of file>  <path relative to bank>`
    each, sorted by path (byte order), newline-terminated. Returns (hash, number of files)."""
    names = []
    for sub in ("inputs", "corpus"):
        root = os.path.join(bank, sub)
        for dp, dn, fn in os.walk(root):
            dn.sort()
            for f in fn:
                p = os.path.join(dp, f)
                if os.path.isfile(p) and not os.path.islink(p):
                    names.append(os.path.relpath(p, bank).replace(os.sep, "/"))
    names.sort(key=lambda n: n.encode())
    listing = "".join(f"{C.sha256_file(os.path.join(bank, n))}  {n}\n" for n in names)
    return hashlib.sha256(listing.encode()).hexdigest(), len(names)


HASH_FILE = os.path.join(HERE, "BANK.sha256")


def recorded_hash():
    try:
        return open(HASH_FILE).read().split()[0]
    except (OSError, IndexError):
        return None


def chronology(event, **kw):
    """Append one timestamped event to `<RUNS>/chronology.jsonl` (freeze, compiles)."""
    rec = dict(kw, event=event, time=time.strftime("%Y-%m-%dT%H:%M:%S%z"), epoch=round(time.time(), 1))
    with open(os.path.join(RUNS, "chronology.jsonl"), "a") as f:
        f.write(json.dumps(rec, sort_keys=True) + "\n")
