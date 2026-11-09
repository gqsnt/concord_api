"""Summarize an H03 cost run (`costs.sh` output): per row the median of each suite, the ratio of each
arm suite to the baseline suite, the load average recorded before the row, and the rows whose
ratio exceeds 1.05 (to re-run with RUNS=10). Expanded sizes (`--expanded DIR`): bytes of each API
crate's `-Zunpretty=expanded` output per suite, with the ratio to the baseline.

    python3 -I tools/desirability/h03/costs_summary.py OUT [--base suite-baseline] [--rerun OUT10] [--expanded DIR]
"""
import argparse
import glob
import json
import os


def load(out):
    res = {}
    for p in sorted(glob.glob(os.path.join(out, "*.*.json"))):
        row, suite = os.path.basename(p)[:-5].rsplit(".", 1)
        r = json.load(open(p))["results"][0]
        res.setdefault(row, {})[suite] = {"median": r["median"], "mean": r["mean"], "stddev": r.get("stddev") or 0.0,
                                          "n": len(r["times"]), "times": r["times"]}
    load = {}
    lp = os.path.join(out, "load.tsv")
    if os.path.exists(lp):
        lines = open(lp).read().splitlines()
        head = lines[0].split("\t")
        for ln in lines[1:]:
            f = dict(zip(head, ln.split("\t")))
            f.setdefault("load_before", f.get("loadavg", "?"))
            load[f["row"]] = f  # the last entry of a row wins (a retried row)
    return res, load


def table(res, load, base, title):
    suites = sorted({s for v in res.values() for s in v if s != base})
    pairs = [(x, y) for i, x in enumerate(suites) for y in suites[i + 1:]]
    lines = [f"### {title}", "", "| row | load before / after | " + " | ".join([base] + suites) + " | " +
             " | ".join(f"{s}/base" for s in suites) + "".join(f" | {y}/{x}" for x, y in pairs) + " |",
             "|---" * (2 + 2 * len(suites) + 1 + len(pairs)) + "|"]
    flagged = []
    for row, l in load.items():
        if l.get("status", "measured") != "measured":
            lines.append(f"| {row} | {l.get('load_before')} | {l['status']} |")
    for row in res:
        if load.get(row, {}).get("status", "measured") != "measured":
            continue
        b = res[row].get(base)
        cells = [f"{res[row][s]['median']:.3f} (n={res[row][s]['n']})" if s in res[row] else "—" for s in [base] + suites]
        ratios = []
        for s in suites:
            a = res[row].get(s)
            if a and b:
                q = a["median"] / b["median"]
                ratios.append(f"{q:.3f}")
                if q > 1.05:
                    flagged.append(row)
            else:
                ratios.append("—")
        for x, y in pairs:
            a, c = res[row].get(y), res[row].get(x)
            ratios.append(f"{a['median'] / c['median']:.3f}" if a and c else "—")
        l = load.get(row, {})
        lines.append(f"| {row} | {l.get('load_before', '?')} / {l.get('load_after', '?')} | " + " | ".join(cells) + " | " + " | ".join(ratios) + " |")
    return lines, sorted(set(flagged))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("out")
    ap.add_argument("--base", default="suite-baseline")
    ap.add_argument("--rerun")
    ap.add_argument("--expanded")
    a = ap.parse_args()
    res, load_ = load(a.out)
    lines, flagged = table(res, load_, a.base, f"RUNS=3 ({a.out})")
    out = {"rows": res, "load": load_, "flagged_over_1.05": flagged}
    if a.rerun and os.path.isdir(a.rerun):
        r2, l2 = load(a.rerun)
        more, f2 = table(r2, l2, a.base, f"re-run RUNS=10 ({a.rerun})")
        lines += [""] + more
        out.update({"rerun_rows": r2, "rerun_load": l2, "rerun_flagged_over_1.05": f2})
    if a.expanded and os.path.isdir(a.expanded):
        sizes = json.load(open(os.path.join(a.expanded, "sizes.json")))
        suites = sorted({s for s in sizes if s != a.base})
        lines += ["", "### expanded size (bytes of `-Zunpretty=expanded`)", "",
                  "| crate | " + " | ".join([a.base] + suites) + " | " + " | ".join(f"{s}/base" for s in suites) + " |",
                  "|---" * (1 + 2 * len(suites) + 1) + "|"]
        for crate in sorted(sizes[a.base]):
            b = sizes[a.base][crate]["bytes"]
            cells = [str(sizes[s][crate]["bytes"]) for s in [a.base] + suites]
            lines.append(f"| {crate} | " + " | ".join(cells) + " | " +
                         " | ".join(f"{sizes[s][crate]['bytes'] / b:.4f}" for s in suites) + " |")
        out["expanded"] = sizes
    print("\n".join(lines))
    json.dump(out, open(os.path.join(a.out, "summary.json"), "w"), indent=1, sort_keys=True)


if __name__ == "__main__":
    main()
