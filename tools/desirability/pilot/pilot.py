"""The reusable pilot harness (PLAN_AMENDMENT_A3 §2): later hypotheses add sealed task files only.

    python3 -I tools/desirability/pilot/pilot.py validate --set SET
    python3 -I tools/desirability/pilot/pilot.py hash --set SET
    python3 -I tools/desirability/pilot/pilot.py freeze --set SET --name NAME
    python3 -I tools/desirability/pilot/pilot.py run --runfile RUN.json --rep N [--arm NAME ..] [--force]
    python3 -I tools/desirability/pilot/pilot.py reader-pack --runfile RUN.json --materials DIR [--seed N]
        [--providers claude,codex] [--reps 3]
    python3 -I tools/desirability/pilot/pilot.py reader-score --runfile RUN.json
    python3 -I tools/desirability/pilot/pilot.py score --runfile RUN.json [--thresholds CARD.json]
        [--fact NAME=VALUE ..] [--partial]
    python3 -I tools/desirability/pilot/pilot.py port-h04 --bank H04BANK --out SET
    python3 -I tools/desirability/pilot/pilot.py regress-h04 --set SET --set-name NAME --runs H04RUN --out DIR
    python3 -I tools/desirability/pilot/pilot.py selftest [--out DIR] [--no-cargo]

README.md in this directory is the manual (layout, task-file schema, run file, thresholds, the
mapping of the existing instruments, controls, limits). Cargo runs one command at a time inside
the I3 compile sandbox with `CARGO_TARGET_DIR` (absolute; per-arm subdirectories) and
`CARGO_BUILD_JOBS`. Run Python with `-I`.
"""
import argparse
import datetime
import json
import os
import shutil
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.dont_write_bytecode = True
import arms as A  # noqa: E402
import taskfile as TF  # noqa: E402
import seal  # noqa: E402
import tally  # noqa: E402


def now():
    return datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def drop_trees(out, keep):
    """Remove the materialised arm trees under `out` (scratch, re-made by every command)."""
    t = os.path.join(out, "trees")
    if not keep and os.path.isdir(t):
        shutil.rmtree(t)


def load_valid_set(set_dir):
    s = TF.load_set(set_dir)
    if s["problems"]:
        raise A.Refused("invalid task set:\n  " + "\n  ".join(s["problems"][:40]))
    return s


def cmd_validate(a):
    s = TF.load_set(a.set)
    counts = {}
    for t in s["tasks"].values():
        k = f"{t.get('kind')}|{t.get('class')}"
        counts[k] = counts.get(k, 0) + 1
    print(json.dumps({"ok": not s["problems"], "tasks": len(s["tasks"]), "by_kind_class": dict(sorted(counts.items())),
                      "problems": s["problems"]}, indent=1))
    return 0 if not s["problems"] else 1


def cmd_hash(a):
    h, nfiles, ntasks = seal.set_hash(a.set)
    print(json.dumps({"set_hash": h, "files": nfiles, "task_files": ntasks, "frozen_as": [n for n, v in seal.ledger().items() if v == h]}))


def cmd_freeze(a):
    A.sealed_dir(a.set, "task set")
    load_valid_set(a.set)
    h, new = seal.freeze(a.set, a.name)
    print(json.dumps({"name": a.name, "set_hash": h, "newly_frozen": new, "ledger": seal.LEDGER}))


def run_arm(rf, set_, set_h, arm, sha, rep, log):
    import kinds as K  # cargo-backed kinds (lazy: validate/hash/freeze need no sandbox)
    tasks = [t for t in set_["tasks"].values() if t["kind"] != "reader"]
    ctx = K.ArmCtx(arm, sha, rf["out"], set_["tasks_dir"], log)
    res = {}
    comp = [t for t in tasks if t["kind"] == "compile"]
    reqs = [t for t in tasks if t["kind"] == "request"]
    if comp or reqs:
        ctx.prepare_clean()
        if comp:
            res.update(K.run_compile(comp, ctx))
        if reqs:
            res.update(K.run_requests(reqs, ctx))
    for t in sorted((t for t in tasks if t["kind"] == "stage"), key=lambda x: x["id"]):
        res[t["id"]] = K.run_stage(t, ctx)
    return res


def cmd_run(a):
    rf = seal.load_runfile(a.runfile)
    set_ = load_valid_set(rf["set"])
    set_h = seal.check_frozen(rf["set"], rf["set_name"])
    seal.bind_run(rf)
    if not 1 <= a.rep <= rf["reps"]:
        raise A.Refused(f"--rep must be 1..{rf['reps']}")
    for arm in rf["arms"]:
        if a.arm and arm["name"] not in a.arm:
            continue
        sha = A.full_sha(arm["commit"])
        path = os.path.join(rf["out"], "results", f"{arm['name']}.r{a.rep}.json")
        if os.path.exists(path) and not a.force:
            raise A.Refused(f"{path} exists (results are not overwritten; --force replaces them)")
        log, started = [], now()
        res = run_arm(rf, set_, set_h, arm["name"], sha, a.rep, log)
        after = seal.set_hash(rf["set"])[0]
        if after != set_h:
            raise A.Refused(f"the task set changed during the run ({set_h} -> {after}): results discarded")
        seal.write_json(path, {"arm": arm["name"], "commit": sha, "rep": a.rep, "set_name": rf["set_name"], "set_hash": set_h,
                               "run_sha256": rf["_sha256"], "started": started, "finished": now(), "tasks": res, "log": log})
        counts = {}
        for r in res.values():
            counts[r["verdict"]] = counts.get(r["verdict"], 0) + 1
        print(json.dumps({"arm": arm["name"], "rep": a.rep, "verdicts": counts, "results": path}), flush=True)
    drop_trees(rf["out"], a.keep_trees)


def result_rows(rf, set_, set_h, partial):
    rows, missing = [], []
    for arm in rf["arms"]:
        sha = A.full_sha(arm["commit"])
        for rep in range(1, rf["reps"] + 1):
            p = os.path.join(rf["out"], "results", f"{arm['name']}.r{rep}.json")
            if not os.path.isfile(p):
                missing.append(f"{arm['name']}.r{rep}")
                continue
            r = json.load(open(p, encoding="utf-8"))
            if r["set_hash"] != set_h or r["commit"] != sha or r["run_sha256"] != rf["_sha256"]:
                raise A.Refused(f"{p} was produced with another task set, commit or run file")
            for tid, x in r["tasks"].items():
                t = set_["tasks"][tid]
                rows.append({**TF.public(t), "task": tid, "arm": arm["name"], "slot": "", "rep": rep, "verdict": x["verdict"],
                             "fingerprint": x.get("fingerprint"), "reason": x.get("reason")})
    base = rf["baseline"]
    for arm in rf["arms"]:
        if arm["name"] == base or not any(t["kind"] == "reader" for t in set_["tasks"].values()):
            continue
        p = os.path.join(rf["out"], "readers", arm["name"], "score.json")
        if not os.path.isfile(p):
            missing.append(f"readers/{arm['name']}")
            continue
        rows += json.load(open(p, encoding="utf-8"))["rows"]
    if missing and not partial:
        raise A.Refused(f"missing results: {missing} (run them, or --partial to score what exists)")
    return rows, missing


def cmd_reader_pack(a):
    import readers as RD
    rf = seal.load_runfile(a.runfile)
    set_ = load_valid_set(rf["set"])
    seal.check_frozen(rf["set"], rf["set_name"])
    seal.bind_run(rf)
    print(json.dumps(RD.pack(rf, set_, os.path.abspath(a.materials), a.seed, a.providers.split(","), a.reps), indent=1))


def cmd_reader_score(a):
    import readers as RD
    if a.runfile:
        rf = seal.load_runfile(a.runfile)
        set_ = load_valid_set(rf["set"])
        set_h = seal.check_frozen(rf["set"], rf["set_name"])
        seal.bind_run(rf)
        rtasks = [t for t in set_["tasks"].values() if t["kind"] == "reader"]
        for arm in rf["arms"]:
            if arm["name"] == rf["baseline"]:
                continue
            runs = os.path.join(rf["out"], "readers", arm["name"])
            res = RD.score_sessions(rtasks, runs, rf["out"])
            names = {"baseline": rf["baseline"], "candidate": arm["name"]}
            for r in res["rows"]:
                r["arm"], r["slot"] = names[r["arm"]], f"{arm['name']}:{r['slot']}"
            seal.write_json(os.path.join(runs, "score.json"), {"set_hash": set_h, **res})
            print(json.dumps({"candidate": arm["name"], "answers": len(res["rows"]), "missing_sessions": len(res["missing_sessions"])}))
        drop_trees(rf["out"], a.keep_trees)
        return
    A.sealed_dir(a.out, "output directory")
    set_ = load_valid_set(a.set)
    set_h = seal.check_frozen(a.set, a.set_name)
    rtasks = [t for t in set_["tasks"].values() if t["kind"] == "reader"]
    res = RD.score_sessions(rtasks, a.sessions, a.out)
    seal.write_json(os.path.join(a.out, "reader-score.json"), {"set_hash": set_h, **res})
    drop_trees(a.out, a.keep_trees)
    print(json.dumps({"answers": len(res["rows"]), "missing_sessions": len(res["missing_sessions"]),
                      "out": os.path.join(a.out, "reader-score.json")}))
    return res


def parse_facts(items, path):
    facts = json.load(open(path, encoding="utf-8")) if path else {}
    for it in items or []:
        k, _, v = it.partition("=")
        facts[k] = v
    return facts


def cmd_score(a):
    rf = seal.load_runfile(a.runfile)
    set_ = load_valid_set(rf["set"])
    set_h = seal.check_frozen(rf["set"], rf["set_name"])
    seal.bind_run(rf)
    rows, missing = result_rows(rf, set_, set_h, a.partial)
    summ, U = tally.summary(rows, rf["baseline"])
    verdicts = {}
    if a.thresholds:
        th = json.load(open(a.thresholds, encoding="utf-8"))
        probs = tally.check_thresholds(th)
        if probs:
            raise A.Refused("thresholds: " + "; ".join(probs))
        facts = parse_facts(a.fact, a.facts)
        for arm in summ["arms"]:
            if arm != rf["baseline"]:
                verdicts[arm] = tally.decide(th, U[arm], U[rf["baseline"]], facts)
    d = os.path.join(rf["out"], "score")
    seal.write_json(os.path.join(d, "score.json"), {"set_name": rf["set_name"], "set_hash": set_h, "missing": missing,
                                                     "summary": summ, "verdicts": verdicts, "rows": rows,
                                                     "thresholds_file": a.thresholds})
    meta = {"name": rf["set_name"], "hash": set_h, "run": rf["name"]}
    pub = tally.public_summary(summ, verdicts, meta)
    seal.write_json(os.path.join(d, "summary.public.json"), pub)
    with open(os.path.join(d, "SUMMARY.public.md"), "w", encoding="utf-8") as f:
        f.write(tally.public_md(pub))
    print(json.dumps({"verdicts": {k: v[0] for k, v in verdicts.items()}, "missing": missing,
                      "public": os.path.join(d, "SUMMARY.public.md")}, indent=1))


def cmd_port_h04(a):
    import readers as RD
    n = RD.port_h04(os.path.abspath(a.bank), os.path.abspath(a.out))
    h = seal.set_hash(a.out)
    print(json.dumps({"tasks": n, "set_hash": h[0], "files": h[1]}))


def cmd_regress_h04(a):
    """Re-score H04 saved reader answers through the pilot and compare per-task verdicts with
    the H04 scorer's own score (`<runs>/score-final/score.json`)."""
    a.sessions = a.runs
    res = cmd_reader_score(argparse.Namespace(runfile=None, set=a.set, set_name=a.set_name, sessions=a.runs, out=a.out,
                                             keep_trees=a.keep_trees))
    ref = json.load(open(os.path.join(a.runs, a.reference), encoding="utf-8"))
    want = {(r["session"], f"{r['sdk']}/t{r['task']}"): r for r in ref["rows"]}
    got = {(r["session"], r["task"]): r for r in res["rows"]}
    diffs = []
    for k in sorted(set(want) | set(got)):
        w, g = want.get(k), got.get(k)
        if w is None or g is None:
            diffs.append({"item": k, "problem": "missing in " + ("H04" if w is None else "pilot")})
            continue
        if (g["verdict"] == "pass") != w["pass"] or g["facts"] != w["facts"]:
            diffs.append({"item": k, "h04": {"pass": w["pass"], "facts": w["facts"]},
                          "pilot": {"pass": g["verdict"] == "pass", "facts": g["facts"]}})
        else:
            cw = {c: v["ok"] for c, v in w["checks"].items()}
            cg = {c: v["ok"] for c, v in g["checks"].items()}
            if cw != cg:
                diffs.append({"item": k, "checks_h04": cw, "checks_pilot": cg})
    out = {"compared": len(want), "pilot_items": len(got), "same_task_verdicts": sum(
        1 for k in want if k in got and (got[k]["verdict"] == "pass") == want[k]["pass"]),
        "differences": diffs, "h04_verdict": ref.get("verdict"),
        "pass_h04": sum(r["pass"] for r in ref["rows"]), "pass_pilot": sum(r["verdict"] == "pass" for r in res["rows"])}
    # the H04 decision recomputed from the pilot-scored rows (h04/decision.py, imported), and the
    # generic threshold evaluator on the same rows with the card's task-level thresholds
    import decision as D  # noqa: E402  (h04/decision.py; h04 is on sys.path through readers)
    h04rows = [{"session": r["session"], "sdk": r["sdk"], "role": r["role"], "task": r["number"], "provider": r["provider"],
                "rep": r["session_rep"], "arm": r["arm"], "pass": r["verdict"] == "pass", "facts": r["facts"]} for r in res["rows"]]
    verdict, reasons, summ = D.decide(h04rows, gates=a.gates, docs_truth=a.docs_truth)
    out["h04_decision_on_pilot_rows"] = {"verdict": verdict, "reasons": reasons, "gates": a.gates, "docs_truth": a.docs_truth}
    if a.thresholds:
        th = json.load(open(a.thresholds, encoding="utf-8"))
        probs = tally.check_thresholds(th)
        if probs:
            raise A.Refused("thresholds: " + "; ".join(probs))
        facts = {"gates": a.gates, "docs_truth": a.docs_truth,
                 "fact_level_rules": "fail" if summ["recurrent_control_fact_regressions"] or summ["newly_misrepresented_control_facts"] else "pass"}
        facts = {k: v for k, v in facts.items() if v != "unknown"}
        v, states = tally.decide(th, tally.units(res["rows"], "candidate"), tally.units(res["rows"], "baseline"), facts)
        out["generic_thresholds"] = {"file": a.thresholds, "facts": facts, "verdict": v,
                                     "rules": [{k: s_[k] for k in ("id", "holds", "value")} for s_ in states]}
    seal.write_json(os.path.join(a.out, "regression.json"), out)
    print(json.dumps({k: v for k, v in out.items() if k != "differences"} | {"differences": len(diffs)}, indent=1))
    return 0 if not diffs and (not a.thresholds or out["generic_thresholds"]["verdict"].split("_")[0] == verdict.split("_")[0]) else 1


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    for name in ("validate", "hash"):
        p = sub.add_parser(name)
        p.add_argument("--set", required=True)
    p = sub.add_parser("freeze")
    p.add_argument("--set", required=True)
    p.add_argument("--name", required=True)
    p = sub.add_parser("run")
    p.add_argument("--keep-trees", action="store_true", help="keep the materialised arm trees (scratch)")
    p.add_argument("--runfile", required=True)
    p.add_argument("--rep", type=int, required=True)
    p.add_argument("--arm", action="append")
    p.add_argument("--force", action="store_true")
    p = sub.add_parser("reader-pack")
    p.add_argument("--runfile", required=True)
    p.add_argument("--materials", required=True)
    p.add_argument("--seed", type=int, default=1)
    p.add_argument("--providers", default="claude,codex")
    p.add_argument("--reps", type=int, default=3)
    p = sub.add_parser("reader-score")
    p.add_argument("--keep-trees", action="store_true", help="keep the materialised arm trees (scratch)")
    p.add_argument("--runfile")
    p.add_argument("--set")
    p.add_argument("--set-name")
    p.add_argument("--sessions")
    p.add_argument("--out")
    p = sub.add_parser("score")
    p.add_argument("--runfile", required=True)
    p.add_argument("--thresholds")
    p.add_argument("--fact", action="append")
    p.add_argument("--facts")
    p.add_argument("--partial", action="store_true")
    p = sub.add_parser("port-h04")
    p.add_argument("--bank", required=True)
    p.add_argument("--out", required=True)
    p = sub.add_parser("regress-h04")
    p.add_argument("--keep-trees", action="store_true", help="keep the materialised arm trees (scratch)")
    p.add_argument("--set", required=True)
    p.add_argument("--set-name", required=True)
    p.add_argument("--runs", required=True)
    p.add_argument("--reference", default=os.path.join("score-final", "score.json"))
    p.add_argument("--out", required=True)
    p.add_argument("--gates", default="unknown", choices=("pass", "fail", "unknown"))
    p.add_argument("--docs-truth", default="unknown", choices=("pass", "fail", "unknown"))
    p.add_argument("--thresholds")
    p = sub.add_parser("selftest")
    p.add_argument("--out", default=os.path.join(A.SEALED, "pilot-selftest"))
    p.add_argument("--no-cargo", action="store_true")
    a = ap.parse_args(argv)
    for k in ("set", "out", "runs", "sessions", "bank", "materials"):
        if getattr(a, k, None):
            setattr(a, k, os.path.abspath(getattr(a, k)))
    if a.cmd == "reader-score" and not a.runfile and not (a.set and a.set_name and a.sessions and a.out):
        ap.error("reader-score needs --runfile, or --set, --set-name, --sessions and --out")
    if a.cmd == "selftest":
        import selftest
        return selftest.main(a)
    fn = {"validate": cmd_validate, "hash": cmd_hash, "freeze": cmd_freeze, "run": cmd_run, "reader-pack": cmd_reader_pack,
          "reader-score": cmd_reader_score, "score": cmd_score, "port-h04": cmd_port_h04, "regress-h04": cmd_regress_h04}[a.cmd]
    r = fn(a)
    return r if isinstance(r, int) else 0


if __name__ == "__main__":
    sys.exit(main())
