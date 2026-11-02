#!/usr/bin/env python3
"""Summarize benches/scale/ab.sh results: mean ± stddev (s) per row and suite, plus each suite's
ratio to the first suite named on the command line (or the alphabetically first). Usage:
ab_summary.py OUT [BASELINE_SUITE_TAG]"""
import glob, json, os, sys

out = sys.argv[1]
results = {}
for path in sorted(glob.glob(os.path.join(out, "*.json"))):
    row, suite = os.path.basename(path)[:-5].rsplit(".", 1)
    r = json.load(open(path))["results"][0]
    results.setdefault(row, {})[suite] = (r["mean"], r["stddev"] or 0.0, len(r["times"]))
suites = sorted({s for v in results.values() for s in v})
base = sys.argv[2] if len(sys.argv) > 2 else suites[0]
print("| row | " + " | ".join(suites) + " | " + " | ".join(f"{s}/{base}" for s in suites if s != base) + " |")
print("|---" * (1 + len(suites) + len(suites) - 1) + "|")
for row in results:
    cells = []
    for s in suites:
        m = results[row].get(s)
        cells.append(f"{m[0]:.3f} ± {m[1]:.3f} (n={m[2]})" if m else "—")
    ratios = []
    for s in suites:
        if s == base: continue
        a, b = results[row].get(s), results[row].get(base)
        ratios.append(f"{a[0] / b[0]:.2f}" if a and b else "—")
    print(f"| {row} | " + " | ".join(cells) + " | " + " | ".join(ratios) + " |")
