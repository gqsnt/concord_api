"""H03 idiom corpus: S1 validation at the baseline, freeze, runs on every arm, instrument controls.

    python3 -I tools/desirability/h03/measure.py validate        # value probes + S1 references at the baseline (twice)
    python3 -I tools/desirability/h03/measure.py freeze          # tools/desirability/h03/BANK.sha256
    python3 -I tools/desirability/h03/measure.py run --arm A --rep 1   # or --arm LABEL --commit SHA (a later arm)

`validate` compiles only S1 references (and trusted value probes) and runs each S1 reference
twice against the local receiver; every canonical S1 must compile, return `Ok`, send the same
request in both runs and match the expectation written from the declaration and the provider
documentation (method, target, the expected header values, present/absent headers, body). An S1
reference of a type's *default* value that the SDK refuses (not `Ok`) cannot give its
default-valued snippets an intended request: those snippets are removed from the corpus with the
reason (recorded in `corpus/generation.json`). The run-1 request of each S1 reference becomes its
intended request (`corpus/intended.json`). `freeze` hashes `inputs/` and `corpus/`; `run` refuses
any other corpus. Each `run` builds a fresh tree of the arm, compiles every snippet (and the
instrument controls) in the compile sandbox and runs each in the run sandbox. A snippet passes iff
it compiles, its run completes with an `Ok` result, and it sends exactly its intended request.
"""
import argparse
import json
import os
import re
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import harness as H  # noqa: E402
import corpus as G  # noqa: E402

CORPUS = G.CORPUS
OUT = os.path.join(H.RUNS, "corpus")


def load(name):
    return json.load(open(os.path.join(CORPUS, name)))


def reply_bytes(sdk, name, tree):
    """The reply body: the frozen copy in `corpus/replies/<sdk>/` (copied from the baseline tree by
    `validate --write`), else the tree's fixture (validation only)."""
    frozen = os.path.join(CORPUS, "replies", sdk, name)
    if os.path.exists(frozen):
        return open(frozen, "rb").read()
    return open(os.path.join(tree, "examples", H.sdk_dir(sdk), "fixtures", name), "rb").read()


def base_path_of(tree, inp):
    client = G.client_of(tree, inp)
    return re.sub(r"^\w+://[^/]*", "", client["stmts"].get("base") or "").rstrip("/")


# ---------------------------------------------------------------- expectation check

def _pairs(q):
    return [x for x in q.split("&") if x]


def check_expect(req, exp):
    """Problems of one recorded request against the written expectation."""
    from score import hexup  # probes/score.py
    probs = []
    if req["method"].upper() != exp["method"]:
        probs.append(f"method {req['method']} != {exp['method']}")
    gp, _, gq = req["target"].partition("?")
    ep, _, eq = exp["target"].partition("?")
    if hexup(gp) != hexup(ep):
        probs.append(f"path {gp} != {ep}")
    if [hexup(x) for x in _pairs(gq)] != [hexup(x) for x in _pairs(eq)]:
        probs.append(f"query {gq!r} != {eq!r}")
    hs = {}
    for k, v in req["headers"]:
        hs.setdefault(k.lower(), []).append(v)
    for k, v in exp.get("headers", {}).items():
        if hs.get(k) != [v]:
            probs.append(f"header {k}: {hs.get(k)} != [{v!r}]")
    for k in exp.get("present", []):
        if k not in hs:
            probs.append(f"header {k} absent")
    for k in exp.get("absent", []):
        if k in hs:
            probs.append(f"header {k} present")
    body = exp.get("body")
    if body is None:
        if req["body"]:
            probs.append(f"unexpected body {req['body'][:120]!r}")
    elif "json" in body:
        try:
            got = json.loads(req["body"])
        except ValueError:
            got = None
        if got != body["json"]:
            probs.append(f"JSON body {req['body'][:200]!r} != {json.dumps(body['json'])}")
    elif "form" in body:
        got = [tuple(x.split("=", 1)) if "=" in x else (x, "") for x in _pairs(req["body"])]
        if got != [tuple(x) for x in body["form"]]:
            probs.append(f"form body {req['body'][:200]!r} != {body['form']}")
    return probs


# ---------------------------------------------------------------- value probes

def value_probes(snippets, refs, inputs, traits):
    """Trusted checks that each variant's value equals its S1 reference's value at the baseline:
    S2 (`<T as Default>::default()` vs the default's S1 spelling), S3 newtype sources
    (`<T as From<src>>::from(..)`), S4 (`"..".parse::<T>().unwrap()`), where T: PartialEq.
    [(sdk, tag, code, label)]"""
    out, seen = [], set()
    for s in snippets:
        if s["class"] not in ("S2", "S3", "S4"):
            continue
        sdk, inp = s["sdk"], inputs[s["sdk"]]
        key = (sdk, s["position"], s["class"], s["variant"])
        if key in seen:
            continue
        seen.add(key)
        pos = {"id": s["position"], "type": s["type"], "param": s["position"].rsplit("/", 1)[1]}
        kind = s["kind"]
        name = G.rust_name(s["type"], kind, inp)
        tr = traits.get(sdk, {}).get(s["type"], {})
        s1, _, lit = G.s1_of(pos, inp, kind)
        if s["class"] == "S2":
            want = G.default_of(pos, inp, kind)[0]
            have = f"<{name} as ::core::default::Default>::default()"
            if want == f"{name}::default()":
                continue  # the default's S1 spelling is the Default impl itself
        elif s["class"] == "S3":
            if s["variant"].startswith("integer"):
                continue  # an integer literal into a primitive integer: identity
            src = s["code"]["call"]  # not used; rebuild the source from the variant
            if s["variant"].startswith("&str"):
                have = f"<{name} as ::core::convert::From<&str>>::from(\"{lit}\")"
            else:
                inner = inp["types"][s["type"]]["newtype_inner"]
                isrc = f"\"{lit}\".to_string()" if G.kind_of(inner) == "string" else f"{lit}{inner}"
                have = f"<{name} as ::core::convert::From<{inner}>>::from({isrc})"
            want = s1
        else:
            if s["variant"] != "parse().unwrap()":
                continue
            have = f"\"{lit}\".parse::<{name}>().unwrap()"
            want = s1
        if not tr.get("PartialEq"):
            out.append((sdk, None, None, f"{key}: no PartialEq, value not probed"))
            continue
        tag = re.sub(r"\W", "_", f"v_{len(out)}")
        code = (f"#[test]\nfn __h03_{tag}() -> ::core::result::Result<(), ::std::boxed::Box<dyn ::std::error::Error>> {{\n"
                f"    let a: {name} = {have};\n    let b: {name} = {want};\n    assert!(a == b);\n    Ok(())\n}}")
        out.append((sdk, tag, code, f"{s['position']} {s['class']} {s['variant']}"))
    return out


# ---------------------------------------------------------------- batches

def add_snippets(batch, snippets, inputs, tree):
    for s in snippets:
        inp = inputs[s["sdk"]]
        reply = s.get("_reply")
        batch.add_run(s["id"], s["sdk"], s["id"].replace("-", "_"), inp["api"], s["code"]["prelude"], s["code"]["call"],
                      reply_bytes(s["sdk"], reply, tree), base_path_of(tree, inp))


def attach_replies(snippets, refs):
    for s in snippets:
        s["_reply"] = refs[s["ref"]]["reply"]


def controls(refs, intended):
    """Instrument controls (PLAN_AMENDMENT_A1 §6), built from each SDK's first S1 reference:
    `ctl-wrong-request`: the reference call with its first string or integer literal changed
    (compiles, sends a different request); `ctl-no-compile`: the reference call with a method
    that does not exist appended (does not compile). Both must fail."""
    out, done = [], set()
    for rid, r in refs.items():
        if r["sdk"] in done or rid not in intended or r["default"]:
            continue
        call = r["code"]["call"]
        m = re.search(r'"([^"\\]+)"', call)
        if m:
            wrong = call[:m.start()] + '"' + m.group(1) + 'zz"' + call[m.end():]
        else:
            m = re.search(r"\b(\d+)(u\d+|i\d+)\b", call)
            if not m:
                continue
            wrong = call[:m.start()] + str(int(m.group(1)) + 1) + m.group(2) + call[m.end():]
        done.add(r["sdk"])
        out.append({"id": f"ctl-wrong-request-{r['sdk']}", "sdk": r["sdk"], "class": "control", "ref": rid,
                    "code": {"prelude": [], "call": wrong}, "_reply": r["reply"], "expect_fail": "different request"})
        out.append({"id": f"ctl-no-compile-{r['sdk']}", "sdk": r["sdk"], "class": "control", "ref": rid,
                    "code": {"prelude": [], "call": call + ".h03_no_such_method()"}, "_reply": r["reply"],
                    "expect_fail": "does not compile"})
    return out


def trim(text, n=60):
    lines = (text or "").splitlines()
    return "\n".join(lines[:n])


# ---------------------------------------------------------------- commands

def cmd_validate(a):
    if H.recorded_hash():
        raise SystemExit("BANK.sha256 exists: the corpus is frozen; validation runs only before the freeze")
    inputs = G.load_inputs()
    traits = load("traits.json")["traits"]
    snippets, refs, gen = load("snippets.json"), load("refs.json"), load("generation.json")
    s1 = [s for s in snippets if s["class"] == "S1"]
    attach_replies(s1, refs)
    tree = os.path.join(H.RUNS, "trees", "baseline-validate")
    prov = H.make_tree("baseline", tree)
    log = [{"provenance": prov}]
    batch = H.Batch(tree, "baseline", H.target_dir("baseline"), inputs, "H03 S1 validation at the baseline")
    add_snippets(batch, s1, inputs, tree)
    probes = value_probes(snippets, refs, inputs, traits)
    for sdk, tag, code, label in probes:
        if tag:
            batch.add_block(sdk, tag, code)
    chronology("validate-compile-start", arm="baseline", commit=prov["commit"], s1_refs=len(s1), non_s1=0,
               value_probes=sum(1 for p in probes if p[1]))
    failed, rendered, exes, fresh = batch.compile(log, sandbox=True)
    run_root = os.path.join(H.target_dir("baseline"), "h03-run")
    r1 = batch.run(exes, failed, rendered, log, run_root)
    r2 = batch.run(exes, failed, rendered, log, run_root)
    # value probes: run each test, exit code 0
    from score import sandbox_run, list_tests  # probes/score.py
    names = {}
    for sdk, m in batch.modules.items():
        exe = exes.get(m.pkg)
        if exe:
            for n in list_tests(exe, os.path.join(run_root, "list")):
                names[(sdk, n.rsplit("::", 1)[-1])] = (exe, n)
    vres = []
    for i, (sdk, tag, code, label) in enumerate(probes):
        if not tag:
            vres.append({"probe": label, "ok": None, "why": "not probed"})
            continue
        if tag in failed:
            vres.append({"probe": label, "ok": False, "why": "does not compile: " + failed[tag]})
            continue
        exe, n = names.get((sdk, f"__h03_{tag}"), (None, None))
        if not exe:
            vres.append({"probe": label, "ok": False, "why": "test not found"})
            continue
        _, code_, err, _, _ = sandbox_run(exe, [n, "--exact"], os.path.join(run_root, f"v{i}"))
        vres.append({"probe": label, "ok": code_ == 0, "why": None if code_ == 0 else err[-400:]})
    by_ref = {s["ref"]: s for s in s1}
    report, intended, failures, refused_defaults = {}, {}, [], []
    for rid, s in by_ref.items():
        a1, a2 = r1.get(s["id"], {}), r2.get(s["id"], {})
        exp = refs[rid]["expect"]
        row = {"snippet": s["id"], "compiles": a1.get("compiles"), "outcome1": a1.get("outcome"), "outcome2": a2.get("outcome"),
               "error": a1.get("error"), "rendered": trim(a1.get("rendered", ""), 30)}
        problems = []
        if not a1.get("compiles"):
            problems.append("does not compile: " + (a1.get("error") or ""))
        elif not a1.get("complete") or not a2.get("complete"):
            problems.append("incomplete run: " + str(a1.get("reason") or a2.get("reason")))
        else:
            if a1.get("provenance_ok") is not True or a2.get("provenance_ok") is not True:
                problems.append("provenance marker mismatch")
            if a1["outcome"] != "@@OK" or a2["outcome"] != "@@OK":
                problems.append(f"not Ok: {a1['outcome']} / {a2['outcome']}")
            if len(a1["requests"]) != 1:
                problems.append(f"{len(a1['requests'])} requests (expected 1)")
            same, why, _ = H.S.compare_requests(a2["requests"], {"recorded": a1["requests"]})
            if not same or a1["requests"] != a2["requests"]:
                problems.append(f"run 2 differs from run 1: {why}")
            if a1["requests"]:
                row["expectation_mismatch"] = check_expect(a1["requests"][0], exp)
            row["requests"] = a1["requests"]
        row["problems"] = problems
        report[rid] = row
        if problems:
            if refs[rid]["default"] and a1.get("compiles") and a1.get("complete") and a1.get("outcome", "").startswith("@@ERR"):
                refused_defaults.append(rid)
            else:
                failures.append(rid)
        else:
            intended[rid] = {"recorded": a1["requests"], "recorded_again": a2["requests"], "volatile": {}}
    vfail = [v for v in vres if v["ok"] is False]
    out = {"provenance": prov, "freshness": fresh, "s1": report, "value_probes": vres, "failures": failures,
           "refused_defaults": refused_defaults,
           "expectation_mismatches": {r: v["expectation_mismatch"] for r, v in report.items() if v.get("expectation_mismatch")}}
    os.makedirs(os.path.join(H.RUNS, "validation"), exist_ok=True)
    stamp = time.strftime("%Y%m%dT%H%M%S")
    json.dump(out, open(os.path.join(H.RUNS, "validation", f"validate-{stamp}.json"), "w"), indent=1, sort_keys=True)
    json.dump(log, open(os.path.join(H.RUNS, "logs", f"validate-{stamp}.log.json"), "w"), indent=1)
    print(json.dumps({"s1_refs": len(report), "pass": len(intended), "failures": failures, "refused_defaults": refused_defaults,
                      "value_probe_failures": [v["probe"] for v in vfail],
                      "expectation_mismatches": len(out["expectation_mismatches"]), "freshness": fresh,
                      "file": f"validate-{stamp}.json"}, indent=1))
    if failures or vfail:
        raise SystemExit("validation failed: fix the corpus inputs (not the rule) and re-run generate + validate")
    if a.write:
        # remove the snippets whose S1 reference (a default value) the SDK refuses, with the reason
        keep, removed = [], []
        for s in snippets:
            if s["ref"] in refused_defaults:
                removed.append({"id": s["id"], "sdk": s["sdk"], "position": s["position"], "class": s["class"],
                                "variant": s["variant"], "ref": s["ref"],
                                "reason": f"S1 reference of the default value is refused at the baseline ({report[s['ref']]['outcome1']})"})
            else:
                keep.append(s)
        gen["removed_after_validation"] = removed
        gen["intended_from"] = {"validation_file": f"validate-{stamp}.json", "provenance": prov}
        counts = {}
        for s in keep:
            c = counts.setdefault(s["sdk"], {})
            c[s["class"]] = c.get(s["class"], 0) + 1
        gen["counts_final"], gen["total_final"] = counts, len(keep)
        json.dump(keep, open(os.path.join(CORPUS, "snippets.json"), "w"), indent=1, sort_keys=True)
        json.dump(gen, open(os.path.join(CORPUS, "generation.json"), "w"), indent=1, sort_keys=True)
        json.dump(intended, open(os.path.join(CORPUS, "intended.json"), "w"), indent=1, sort_keys=True)
        for r in refs.values():
            dst = os.path.join(CORPUS, "replies", r["sdk"], r["reply"])
            os.makedirs(os.path.dirname(dst), exist_ok=True)
            with open(dst, "wb") as fo:
                fo.write(open(os.path.join(tree, "examples", H.sdk_dir(r["sdk"]), "fixtures", r["reply"]), "rb").read())
        print(json.dumps({"written": True, "removed": len(removed), "total_final": len(keep)}))


chronology = H.chronology


def cmd_freeze(a):
    if H.recorded_hash() and not a.force_new:
        raise SystemExit(f"already frozen: {H.recorded_hash()} (a corrected corpus needs --force-new and a full re-run of all arms)")
    h, n = H.bank_hash()
    with open(H.HASH_FILE, "w") as f:
        f.write(f"{h}  H03 sealed idiom corpus: {n} files under <WS>/work/sealed/h03-bank/{{inputs,corpus}}/; SHA-256 of the canonical listing "
                f"(harness.bank_hash: one line `<sha256>  <path>` per file, sorted by path)\n")
    chronology("freeze", bank_hash=h, files=n)
    print(h, n)


def cmd_run(a):
    rec = H.recorded_hash()
    h, n = H.bank_hash()
    if not rec or rec != h:
        raise SystemExit(f"corpus hash {h} does not match BANK.sha256 ({rec}): refusing to run")
    inputs = G.load_inputs()
    snippets, refs, intended = load("snippets.json"), load("refs.json"), load("intended.json")
    attach_replies(snippets, refs)
    ctls = controls(refs, intended)
    if a.arm not in H.ARMS and not a.commit:
        raise SystemExit(f"arm {a.arm} is not one of {sorted(H.ARMS)}: pass --commit")
    tree = os.path.join(H.RUNS, "trees", a.arm)
    prov = H.make_tree(a.arm, tree, a.commit)
    prov2, problems = H.check_tree(tree, a.arm, a.commit)
    if problems:
        raise SystemExit(f"tree provenance: {problems}")
    log = [{"provenance": prov, "bank_hash": h}]
    batch = H.Batch(tree, a.arm, H.target_dir(a.arm), inputs, f"H03 corpus run, arm {a.arm}, rep {a.rep}")
    add_snippets(batch, snippets + ctls, inputs, tree)
    chronology("compile-start", arm=a.arm, rep=a.rep, commit=prov["commit"], snippets=len(snippets),
               non_s1=sum(1 for s in snippets if s["class"] != "S1"), bank_hash=h)
    failed, rendered, exes, fresh = batch.compile(log, sandbox=True)
    chronology("compile-end", arm=a.arm, rep=a.rep, commit=prov["commit"])
    run_root = os.path.join(H.target_dir(a.arm), "h03-run")
    res = batch.run(exes, failed, rendered, log, run_root)
    rows = {}
    for s in snippets + ctls:
        r = res.get(s["id"], {"compiles": None, "complete": False, "reason": "no result"})
        ok, why = H.verdict(r, intended[s["ref"]])
        rows[s["id"]] = {"pass": ok, "reason": why, "compiles": r.get("compiles"), "error": r.get("error"),
                         "rendered": trim(r.get("rendered", ""), 40), "outcome": r.get("outcome"),
                         "requests": r.get("requests"), "provenance_ok": r.get("provenance_ok")}
    ctl_ok = {c["id"]: (rows[c["id"]]["pass"] is False and
                        ((c["expect_fail"] == "does not compile" and rows[c["id"]]["compiles"] is False) or
                         (c["expect_fail"] == "different request" and rows[c["id"]]["compiles"] and rows[c["id"]]["outcome"] == "@@OK"
                          and "different request" in (rows[c["id"]]["reason"] or ""))))
              for c in ctls}
    out = {"arm": a.arm, "rep": a.rep, "provenance": prov, "bank_hash": h, "freshness": fresh, "results": rows,
           "controls": ctl_ok, "time": time.strftime("%Y-%m-%dT%H:%M:%S%z")}
    os.makedirs(OUT, exist_ok=True)
    json.dump(out, open(os.path.join(OUT, f"{a.arm}.r{a.rep}.json"), "w"), indent=1, sort_keys=True)
    json.dump(log, open(os.path.join(H.RUNS, "logs", f"run-{a.arm}.r{a.rep}.log.json"), "w"), indent=1)
    if not a.keep_tree:
        import shutil
        shutil.rmtree(tree, ignore_errors=True)  # keep scratch lean: the tree is re-created per run
    np = sum(1 for k, v in rows.items() if v["pass"] and not k.startswith("ctl-"))
    print(json.dumps({"arm": a.arm, "rep": a.rep, "pass": np, "total": len(snippets), "controls_ok": all(ctl_ok.values()),
                      "freshness": fresh, "provenance_ok_all": all(v["provenance_ok"] is not False for v in rows.values())}))


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    v = sub.add_parser("validate")
    v.add_argument("--write", action="store_true", help="write intended.json and the final snippets (only if validation passes)")
    f = sub.add_parser("freeze")
    f.add_argument("--force-new", action="store_true")
    r = sub.add_parser("run")
    r.add_argument("--arm", required=True, help="baseline, A, B, or a new label with --commit")
    r.add_argument("--commit", help="the arm's commit (for an arm label not in harness.ARMS: a revised candidate)")
    r.add_argument("--rep", required=True, type=int)
    r.add_argument("--keep-tree", action="store_true", help="keep the throwaway tree after the run")
    a = ap.parse_args()
    os.makedirs(os.path.join(H.RUNS, "logs"), exist_ok=True)
    {"validate": cmd_validate, "freeze": cmd_freeze, "run": cmd_run}[a.cmd](a)


if __name__ == "__main__":
    main()
