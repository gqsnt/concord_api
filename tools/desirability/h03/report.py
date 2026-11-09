"""H03 corpus scorer: per-arm verdicts, reproducibility, regressions, wrong requests.

    python3 -I tools/desirability/h03/report.py [--reps 1 2]

Reads the frozen corpus (`<BANK>/corpus/`) and the run files `<RUNS>/corpus/<arm>.r<rep>.json`
written by `measure.py run`. Writes the sealed `<RUNS>/corpus/RESULTS.md` (per-snippet detail)
and `<RUNS>/corpus/summary.json`; prints only counts, class names and snippet ids.

- Verdict of a snippet on an arm: its rep-1 run (compiles in the R3 compile sandbox, completes in
  the run sandbox with an `Ok` result, sends exactly its intended request); every verdict must
  reproduce in rep 2 (same pass flag, same normalised first diagnostic, same requests).
- Witness success rate: passing / total over the witness SDKs' snippets (Kraken, Cloudflare,
  Notion, Frankfurter), per arm, and its gain over the baseline in points.
- Newly passing spellings: per witness, the snippets failing at the baseline and passing on the
  arm, by class and spelling form.
- Regression: passes at the baseline, fails on the arm (any SDK), with its first diagnostic.
- Wrong request: compiles and runs but sends a different request (always a failure).
"""
import argparse
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import harness as H  # noqa: E402

CORPUS = os.path.join(H.BANK, "corpus")
OUT = os.path.join(H.RUNS, "corpus")
ARMS = ["baseline", "A", "B", "Aprime", "Bprime"]  # known labels first; any other `<label>.r<n>.json` is appended
CLASSES = ["S1", "S2", "S3", "S4", "S5", "S6", "S7", "S8"]


def norm_diag(text):
    """A first diagnostic without tree paths, line/column numbers or addresses."""
    t = re.sub(r"/[^\s:]*?/h03-runs/trees/[^/\s]+/", "<tree>/", text or "")
    t = re.sub(r":\d+:\d+", ":L:C", t)
    t = re.sub(r"0x[0-9a-f]+", "0x?", t)
    return t.strip()


def headline(text):
    return (text or "").splitlines()[0][:160] if text else ""


def rate(p, n):
    return f"{p}/{n}" + (f" ({100.0 * p / n:.1f}%)" if n else "")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--reps", nargs="+", type=int, default=[1, 2])
    a = ap.parse_args()
    snippets = json.load(open(os.path.join(CORPUS, "snippets.json")))
    gen = json.load(open(os.path.join(CORPUS, "generation.json")))
    sid = {s["id"]: s for s in snippets}
    runs = {}
    for f in sorted(os.listdir(OUT)):
        m = re.match(r"(\w+)\.r\d+\.json$", f)
        if m and m.group(1) not in ARMS:
            ARMS.append(m.group(1))
    for arm in ARMS:
        for rep in a.reps:
            p = os.path.join(OUT, f"{arm}.r{rep}.json")
            if os.path.exists(p):
                runs[(arm, rep)] = json.load(open(p))
    bank = H.recorded_hash()
    problems = []
    for k, r in runs.items():
        if r["bank_hash"] != bank:
            problems.append(f"{k}: run of another corpus ({r['bank_hash']})")
    first = {arm: runs.get((arm, a.reps[0])) for arm in ARMS}
    verdict = {arm: {i: first[arm]["results"][i] for i in sid} for arm in ARMS if first[arm]}
    # reproducibility
    repro = {}
    for arm in ARMS:
        if (arm, a.reps[0]) not in runs or len(a.reps) < 2 or (arm, a.reps[1]) not in runs:
            continue
        r1, r2 = runs[(arm, a.reps[0])]["results"], runs[(arm, a.reps[1])]["results"]
        diff = []
        for i in list(sid) + [k for k in r1 if k.startswith("ctl-")]:
            x, y = r1.get(i, {}), r2.get(i, {})
            same = (x.get("pass") == y.get("pass") and norm_diag(x.get("error")) == norm_diag(y.get("error"))
                    and x.get("requests") == y.get("requests") and x.get("outcome") == y.get("outcome"))
            if not same:
                diff.append(i)
        repro[arm] = {"compared": len(sid), "differ": diff}
    roles = {s["sdk"]: s["role"] for s in snippets}
    sdks = list(dict.fromkeys(s["sdk"] for s in snippets))
    summary = {"bank_hash": bank, "problems": problems, "reproducibility": repro, "arms": {}, "controls": {},
               "provenance": {f"{k[0]}.r{k[1]}": {"commit": v["provenance"]["commit"], "freshness": v["freshness"],
                                                   "provenance_ok_all": all(x.get("provenance_ok") is not False for x in v["results"].values())}
                              for k, v in runs.items()}}
    for k, v in runs.items():
        summary["controls"][f"{k[0]}.r{k[1]}"] = v["controls"]
    md = ["# H03 idiom corpus results (sealed)", "", f"Corpus hash `{bank}`; runs: {sorted(f'{k[0]}.r{k[1]}' for k in runs)}", ""]
    for arm in ARMS:
        if arm not in verdict:
            continue
        v = verdict[arm]
        per_sdk, per_cls = {}, {}
        for i, s in sid.items():
            ps = per_sdk.setdefault(s["sdk"], [0, 0])
            ps[1] += 1
            pc = per_cls.setdefault(s["sdk"], {}).setdefault(s["class"], [0, 0])
            pc[1] += 1
            if v[i]["pass"]:
                ps[0] += 1
                pc[0] += 1
        wit = [i for i, s in sid.items() if roles[s["sdk"]] == "witness"]
        wp = sum(1 for i in wit if v[i]["pass"])
        summary["arms"][arm] = {"per_sdk": per_sdk, "per_class": per_cls, "witness": [wp, len(wit)],
                                "total": [sum(1 for i in sid if v[i]["pass"]), len(sid)]}
    base = verdict.get("baseline")
    for arm in [x for x in ARMS if x != "baseline"]:
        if arm not in verdict or not base:
            continue
        v = verdict[arm]
        regress = [i for i in sid if base[i]["pass"] and not v[i]["pass"]]
        newly = [i for i in sid if not base[i]["pass"] and v[i]["pass"]]
        wrong = [i for i in sid if v[i]["compiles"] and v[i].get("outcome") is not None and not v[i]["pass"]
                 and "different request" in (v[i]["reason"] or "")]
        notok = [i for i in sid if v[i]["compiles"] and not v[i]["pass"] and i not in wrong]
        by_w = {}
        for i in newly:
            s = sid[i]
            if roles[s["sdk"]] != "witness":
                continue
            key = f"{s['class']} {s['variant']}"
            by_w.setdefault(s["sdk"], {}).setdefault(key, 0)
            by_w[s["sdk"]][key] += 1
        b_w = summary["arms"]["baseline"]["witness"]
        a_w = summary["arms"][arm]["witness"]
        summary["arms"][arm].update({
            "witness_gain_points": round(100.0 * a_w[0] / a_w[1] - 100.0 * b_w[0] / b_w[1], 1),
            "regressions": [{"id": i, "sdk": sid[i]["sdk"], "class": sid[i]["class"], "variant": sid[i]["variant"],
                             "first_diagnostic": headline(v[i]["error"]) if not v[i]["compiles"] else v[i]["reason"][:200]}
                            for i in regress],
            "newly_passing": {"count": len(newly), "by_sdk": {s: sum(1 for i in newly if sid[i]["sdk"] == s) for s in sdks},
                              "witness_spellings": by_w},
            "wrong_request": [{"id": i, "sdk": sid[i]["sdk"], "class": sid[i]["class"], "variant": sid[i]["variant"],
                               "reason": v[i]["reason"][:300]} for i in wrong],
            "compiles_not_passing_other": [{"id": i, "sdk": sid[i]["sdk"], "class": sid[i]["class"], "reason": (v[i]["reason"] or "")[:200]}
                                           for i in notok],
            "witness_sdks_gaining": sorted(by_w),
        })
    for arm in ARMS:
        if arm not in verdict:
            continue
        v = verdict[arm]
        md += [f"## Arm {arm}", "", "| SDK | role | pass/total | " + " | ".join(CLASSES) + " |",
               "|---|---|---|" + "---|" * len(CLASSES)]
        for sdk in sdks:
            ps = summary["arms"][arm]["per_sdk"][sdk]
            pc = summary["arms"][arm]["per_class"][sdk]
            md.append(f"| {sdk} | {roles[sdk]} | {ps[0]}/{ps[1]} | " +
                      " | ".join(f"{pc[c][0]}/{pc[c][1]}" if c in pc else "—" for c in CLASSES) + " |")
        md += ["", "| snippet | SDK | position | class | spelling | pass | reason |", "|---|---|---|---|---|---|---|"]
        for i, s in sid.items():
            r = v[i]
            code = "; ".join(s["code"]["prelude"] + [s["code"]["call"]]).replace("|", "\\|")
            md.append(f"| {i} | {s['sdk']} | {s['position'] or s['call']} | {s['class']} | {s['variant']} | "
                      f"{'PASS' if r['pass'] else 'fail'} | {(r['reason'] or '').replace('|', '/')[:160]} |")
            md.append(f"|  |  |  |  | `{code[:300]}` |  |  |")
        md.append("")
    json.dump(summary, open(os.path.join(OUT, "summary.json"), "w"), indent=1, sort_keys=True)
    open(os.path.join(OUT, "RESULTS.md"), "w").write("\n".join(md) + "\n")
    # public output: counts, classes, ids only
    pub = {"bank_hash": bank, "problems": problems,
           "reproducibility": {k: {"compared": v["compared"], "differ": v["differ"]} for k, v in repro.items()},
           "controls": summary["controls"], "provenance": summary["provenance"]}
    for arm, d in summary["arms"].items():
        pub[arm] = {k: d[k] for k in d if k not in ("compiles_not_passing_other",)}
    print(json.dumps(pub, indent=1, sort_keys=True))


if __name__ == "__main__":
    main()
