"""H03 secondary outcome: the I5 caller bank re-run per arm, with the seeded lines relabelled.

    python3 -I tools/desirability/h03/relabel.py bank --arm A [--commit SHA]   # caller bank + score + relabel runs
    python3 -I tools/desirability/h03/relabel.py report [--arms baseline A B]

`bank` builds a fresh throwaway tree of the arm (`<RUNS>/i5-trees/<arm>`, `harness.make_tree`),
runs `diagnostics/caller_bank.py` on it with its bank directory redirected to
`<RUNS>/i5/<arm>/bank/` (the committed `diagnostics/bank` is never written), scores it with
`diagnostics/score.py bank`, and relabels every case whose seeded line now compiles: the case's
seeded line and its control's corrected line are each run against the local receiver (the H03
harness: compile sandbox, run sandbox, receiver answering `200 {}`), with the bank's setup pointed
at the receiver (`with_base_url`) and, for Kraken, a private policy with fixed test credentials so
that account calls are sent. A compiling case is **resolved** iff the seeded line sends exactly
the requests the corrected line sends (`probes/score.py compare_requests`); one that compiles and
sends a different request (or none) is a **failure**; a case that still does not compile is
scored as today (`score.py`: first error's primary span in the caller's file, no internal name).

`report` prints per arm: resolved / still diagnosed (and how many of those pass R5) / failures;
for each `wrong_setter` case whether the first error names the expected type; for each case
whether its first error newly names Concord internals (`score.py`'s INTERNAL pattern or generated
names) compared with the baseline; for each case that still produces E0283, whether the label
names Concord's internal path or a type parameter.
"""
import argparse
import importlib.util
import json
import os
import re
import shutil
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import harness as H  # noqa: E402

DIAG = os.path.join(HERE, "..", "diagnostics")
OUT = os.path.join(H.RUNS, "i5")
TREES = os.path.join(H.RUNS, "i5-trees")
KRAKEN_PRIVATE = ('Policy::private(concord_sdk_kraken_spot_rest::Credentials::new("h03-key", "aDAzLXNlY3JldA==")?, '
                  'concord_sdk_kraken_spot_rest::Nonces::new(9000))')
# (text in the bank's setup, replacement): the client pointed at the receiver
SETUP_RUN = {
    "nager-holidays": [("NagerHolidays::new(concord::reqwest::Client::new());",
                        "NagerHolidays::new(concord::reqwest::Client::new()).with_base_url(&base)?;")],
    "notion": [('NotionClient::new(reqwest::Client::new(), "t");',
                'NotionClient::new(reqwest::Client::new(), "t").with_base_url(&base)?;')],
    "kraken-spot-rest": [("KrakenConfig { policy: Policy::public() });",
                          "KrakenConfig { policy: " + KRAKEN_PRIVATE + " }).with_base_url(&base)?;")],
    "github": [('GitHubConfig { token: "t".to_owned() });', 'GitHubConfig { token: "t".to_owned() }).with_base_url(&base)?;')],
    "cloudflare-zones": [('Cloudflare::connect(http_client()?, "token")?;',
                          'Cloudflare::connect(http_client()?, "token")?.with_base_url(&base)?;')],
}
# A line that binds a scope handle (the bank's positional-scope-value cases) sends nothing by
# itself: both its seeded and its corrected line are completed with the endpoint the bank's own
# cases call on that scope.
SCOPE_ENDPOINT = {"github": "labels()"}
LET = re.compile(r"^let\s+(\w+)\s*=\s*(.+);$", re.S)


def load_module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def base_path(tree, sdk, decl_file, decl_mod):
    text = open(os.path.join(tree, "examples", f"{sdk}-sdk", decl_file), encoding="utf-8").read()
    c = decl_mod.parse_file(text)[0]
    return re.sub(r"^\w+://[^/]*", "", c["stmts"].get("base") or "").rstrip("/")


def cmd_bank(a):
    sys.path.insert(0, os.path.join(HERE, ".."))
    CB = load_module("h03_caller_bank", os.path.join(DIAG, "caller_bank.py"))
    SC = load_module("h03_i5_score", os.path.join(DIAG, "score.py"))
    if a.arm not in H.ARMS and not a.commit:
        raise SystemExit("pass --commit for an arm label not in harness.ARMS")
    tree = os.path.join(TREES, a.arm)
    prov = H.make_tree(a.arm, tree, a.commit)
    _, problems = H.check_tree(tree, a.arm, a.commit)
    if problems:
        raise SystemExit(f"tree provenance: {problems}")
    out = os.path.join(OUT, a.arm)
    bank = os.path.join(out, "bank")
    if os.path.isdir(bank):
        shutil.rmtree(bank)
    os.makedirs(bank)
    committed = os.path.realpath(os.path.join(DIAG, "bank"))
    assert os.path.realpath(bank) != committed
    CB.BANK = bank
    os.environ["CARGO_TARGET_DIR"] = H.target_dir(a.arm)
    os.environ.setdefault("CARGO_BUILD_JOBS", "6")
    argv = sys.argv
    sys.argv = ["caller_bank.py", "--scratch", tree]
    H.chronology("i5-bank-start", arm=a.arm, commit=prov["commit"])
    try:
        CB.main()
    finally:
        sys.argv = argv
    rows = SC.score_bank(bank)
    summ = SC.summary(rows)
    json.dump({"summary": summ, "cases": rows}, open(os.path.join(out, "score.json"), "w"), indent=1, sort_keys=True)
    # relabel the cases whose seeded line compiles
    compiled = []
    for f in sorted(os.listdir(bank)):
        if f.endswith(".json") and f.count(".") == 2 and not f.startswith(("CONTROLS", "DROPPED")):
            meta = json.load(open(os.path.join(bank, f)))
            if meta.get("compiled"):
                compiled.append(meta)
    relabel = {}
    if compiled:
        inputs, batch_keys = {}, {}
        for meta in compiled:
            spec = CB.SDKS[meta["sdk"]]
            inputs[meta["sdk"]] = {"package": spec["package"], "imports": spec["prelude"].splitlines()}
        batch = H.Batch(tree, a.arm, H.target_dir(a.arm), inputs, f"H03 I5 relabel, arm {a.arm}")
        for meta in compiled:
            sdk, spec = meta["sdk"], CB.SDKS[meta["sdk"]]
            setup = spec["setup"]
            for old, new in SETUP_RUN[sdk]:
                if old not in setup:
                    raise SystemExit(f"{sdk}: setup text changed: {old!r} not found")
                setup = setup.replace(old, new)
            bp = base_path(tree, sdk, spec["decl"][0], CB.decl)
            for which, line in (("seeded", meta["seeded"]), ("corrected", meta["corrected"])):
                m = LET.match(line.strip())
                key = f"{meta['sdk']}.{meta['kind']}.{which}"
                if not m or ".send()" in m.group(2) or ".prepare()" in m.group(2):
                    relabel.setdefault(f"{meta['sdk']}.{meta['kind']}", {})["not_runnable"] = f"{which} line is not `let NAME = <handle>;`"
                    continue
                prelude = [ln.strip() for ln in setup.splitlines() if ln.strip()] + [line.strip()]
                call = m.group(1)
                if "scope value" in (meta.get("variant") or "") and sdk in SCOPE_ENDPOINT:
                    call = f"{call}.{SCOPE_ENDPOINT[sdk]}"
                    relabel.setdefault(f"{meta['sdk']}.{meta['kind']}", {})["completed_with"] = SCOPE_ENDPOINT[sdk]
                batch.add_run(key, sdk, re.sub(r"\W", "_", key), "()", prelude, call, b"{}", bp)
                batch_keys[key] = meta
        log = [{"provenance": prov}]
        failed, rendered, exes, fresh = batch.compile(log, sandbox=True)
        res = batch.run(exes, failed, rendered, log, os.path.join(H.target_dir(a.arm), "h03-i5-run"))
        for meta in compiled:
            case = f"{meta['sdk']}.{meta['kind']}"
            s, c = res.get(case + ".seeded"), res.get(case + ".corrected")
            row = relabel.setdefault(case, {})
            row.update({"seeded": s, "corrected": c})
            if "not_runnable" in row:
                row["verdict"] = "failure (not runnable)"
                continue
            if not (c and c.get("complete") and c.get("requests")):
                row["verdict"] = "failure (the corrected line sent nothing)"
            elif not (s and s.get("compiles") and s.get("complete")):
                row["verdict"] = "failure (seeded run did not complete)"
            else:
                ok, why, _ = H.S.compare_requests(s["requests"], {"recorded": c["requests"]})
                row["verdict"] = "resolved" if ok else f"failure (different request: {why})"
        json.dump(log, open(os.path.join(H.RUNS, "logs", f"i5-relabel-{a.arm}.log.json"), "w"), indent=1)
    json.dump({"arm": a.arm, "provenance": prov, "relabel": relabel}, open(os.path.join(out, "relabel.json"), "w"),
              indent=1, sort_keys=True)
    if not a.keep_tree:
        shutil.rmtree(tree, ignore_errors=True)  # keep scratch lean
    print(json.dumps({"arm": a.arm, "score": summ, "compiled_cases": [f"{m['sdk']}.{m['kind']}" for m in compiled],
                      "relabel": {k: v.get("verdict") for k, v in relabel.items()}}, indent=1))


def label_kind(block):
    """For an E0283 first error: does its label name Concord's internal path or a type parameter?"""
    internal = bool(re.search(r"__private|__macro|__concord|\bIntoValue\b|concord_core::|concord_macros::", block))
    param = bool(re.search(r"type parameter `\w+`|for the type parameter|declared on the (method|associated function)", block))
    return {"names_internal_path": internal, "names_type_parameter": param}


def cmd_report(a):
    SC = load_module("h03_i5_score", os.path.join(DIAG, "score.py"))
    data = {}
    for arm in a.arms:
        p = os.path.join(OUT, arm)
        if os.path.exists(os.path.join(p, "score.json")):
            data[arm] = {"score": json.load(open(os.path.join(p, "score.json"))),
                         "relabel": json.load(open(os.path.join(p, "relabel.json")))["relabel"]}
    base = data.get("baseline")
    report = {}
    for arm, d in data.items():
        rows = d["score"]["cases"]
        resolved, diagnosed, failures, diag_pass = [], [], [], []
        per_case = {}
        for case, r in sorted(rows.items()):
            meta = json.load(open(os.path.join(OUT, arm, "bank", case + ".json")))
            stderr = open(os.path.join(OUT, arm, "bank", case + ".stderr"), encoding="utf-8").read()
            head, primary, block = SC.first_error(stderr)
            entry = {"kind": meta["kind"], "compiles": bool(meta.get("compiled")), "headline": head,
                     "internal_names": r.get("internal_names", []), "pass_r5": r.get("pass")}
            if meta.get("compiled"):
                v = d["relabel"].get(case, {}).get("verdict", "failure (no relabel run)")
                entry["relabel"] = v
                (resolved if v == "resolved" else failures).append(case)
            else:
                diagnosed.append(case)
                if r.get("pass"):
                    diag_pass.append(case)
            if meta["kind"] == "wrong_setter":
                entry["names_expected_type"] = r.get("names_expected_type")
                entry["expected"] = meta.get("expected")
            if head and "E0283" in head:
                entry["e0283_label"] = label_kind(block)
                entry["e0283_block"] = block[:600]
            if base and arm != "baseline":
                b = base["score"]["cases"].get(case, {})
                entry["newly_internal"] = sorted(set(r.get("internal_names", [])) - set(b.get("internal_names", [])))
            per_case[case] = entry
        report[arm] = {"cases": len(rows), "resolved": resolved, "still_diagnosed": len(diagnosed),
                       "still_diagnosed_pass_r5": len(diag_pass), "failures": failures, "summary_r5": d["score"]["summary"],
                       "per_case": per_case}
    json.dump(report, open(os.path.join(OUT, "REPORT.json"), "w"), indent=1, sort_keys=True)
    for arm, r in report.items():
        print(f"== {arm}: cases {r['cases']}, resolved {len(r['resolved'])} {r['resolved']}, still diagnosed {r['still_diagnosed']} "
              f"(R5 pass {r['still_diagnosed_pass_r5']}), failures {len(r['failures'])} {r['failures']}")
        for case, e in r["per_case"].items():
            extra = []
            if "names_expected_type" in e:
                extra.append(f"names expected `{e['expected']}`: {e['names_expected_type']}")
            if e.get("newly_internal"):
                extra.append(f"NEWLY INTERNAL {e['newly_internal']}")
            if "e0283_label" in e:
                extra.append(f"E0283 label: {e['e0283_label']}")
            if e.get("relabel"):
                extra.append(f"relabel: {e['relabel']}")
            print(f"   {case:40s} {('compiles' if e['compiles'] else (e['headline'] or '')[:70]):72s} {'; '.join(extra)}")


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    b = sub.add_parser("bank")
    b.add_argument("--arm", required=True)
    b.add_argument("--commit")
    b.add_argument("--keep-tree", action="store_true")
    r = sub.add_parser("report")
    r.add_argument("--arms", nargs="+", default=["baseline", "A", "B", "Aprime", "Bprime"])
    a = ap.parse_args()
    os.makedirs(os.path.join(H.RUNS, "logs"), exist_ok=True)
    {"bank": cmd_bank, "report": cmd_report}[a.cmd](a)


if __name__ == "__main__":
    main()
