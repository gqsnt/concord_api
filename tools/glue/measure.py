"""Spike measurements M1, M2, M4, M5, M6 between two trees (METHOD.md § Measurements).

    python3 -I tools/glue/measure.py BASE_REPO SPIKE_REPO [--sdk NAME ...] [--md OUT.md]

One reason to exist: every spike must be measured the same way; this wraps `classify.py` (glue,
model, behavior lines) and `static_scan.py` (declaration lines per endpoint) and adds the negation
count. M3, M7–M11 are measured by the spike lane (concepts, docs, compile time, call sites, tests).
"""
import argparse
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import classify  # noqa: E402
import static_scan  # noqa: E402

SIMPLE = ["nager-holidays-sdk", "pokeapi-sdk", "frankfurter-sdk"]
# M5: statements that remove, hide or opt out of generated surface (none exist today; the reverted
# `controls { remove ..; }` is the example). Counted inside `api!` blocks only.
NEGATION = re.compile(r"(?m)^\s*(controls\s*\{|remove\s+\w+\s*;|hide\s+\w+|no_\w+\s*;|#\[(skip|hidden|no_generate)\b)")


def per_sdk(repo, sdks):
    ex = os.path.join(repo, "examples")
    names = sorted(d for d in os.listdir(ex) if d.endswith("-sdk") and (not sdks or d in sdks))
    out = {}
    for d in names:
        path = os.path.join(ex, d)
        res = classify.Sdk(path).classify()
        c = {}
        for rows in res.values():
            for _, lab, _ in rows:
                c[lab] = c.get(lab, 0) + 1
        c["glue"] = sum(v for k, v in c.items() if k.startswith("glue:"))
        r = static_scan.analyse_sdk(path)
        c["decl_dsl"], c["endpoints"] = r["decl_dsl"], r["endpoints"]
        neg = 0
        for fp in classify.Sdk(path).files:
            text = open(fp, encoding="utf-8").read()
            clean = static_scan.strip_strings_and_comments(text)
            for s, e in static_scan.api_spans(text, clean):
                neg += len(NEGATION.findall(clean[s:e]))
        c["negations"] = neg
        out[d] = c
    return out


def table(base, spike):
    keys = ["glue", "glue:error", "glue:result", "glue:operation", "glue:redaction", "glue:forwarding",
            "glue:construction", "glue:paging", "glue:docs", "model", "behavior", "other", "decl_dsl",
            "endpoints", "negations"]
    tot = lambda d, k: sum(v.get(k, 0) for v in d.values())  # noqa: E731
    lines = ["| Measure | Base | Spike | Δ |", "|---|---|---|---|"]
    for k in keys:
        b, s = tot(base, k), tot(spike, k)
        lines.append(f"| {k} | {b:,} | {s:,} | {s - b:+,} |")
    be, se = tot(base, "endpoints") or 1, tot(spike, "endpoints") or 1
    lines.append(f"| M4 DSL lines per endpoint | {tot(base, 'decl_dsl') / be:.2f} | {tot(spike, 'decl_dsl') / se:.2f} | |")
    lines += ["", "Per SDK (glue, model+behavior, DSL lines, negations): base → spike", ""]
    for d in sorted(set(base) | set(spike)):
        b, s = base.get(d, {}), spike.get(d, {})
        if b == s:
            continue
        lines.append(f"- `{d}`: glue {b.get('glue', 0)} → {s.get('glue', 0)}; "
                     f"model+behavior {b.get('model', 0) + b.get('behavior', 0)} → {s.get('model', 0) + s.get('behavior', 0)}; "
                     f"DSL {b.get('decl_dsl', 0)} → {s.get('decl_dsl', 0)}; negations {b.get('negations', 0)} → {s.get('negations', 0)}")
    lines += ["", "M6 simple controls (DSL lines; must not grow):", ""]
    for d in SIMPLE:
        lines.append(f"- `{d}`: {base.get(d, {}).get('decl_dsl', '?')} → {spike.get(d, {}).get('decl_dsl', '?')}")
    return "\n".join(lines) + "\n"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("base")
    ap.add_argument("spike")
    ap.add_argument("--sdk", nargs="*")
    ap.add_argument("--md")
    a = ap.parse_args()
    sdks = sorted(set(a.sdk or []) | set(SIMPLE)) if a.sdk else None
    md = table(per_sdk(a.base, sdks), per_sdk(a.spike, sdks))
    if a.md:
        open(a.md, "w").write(md)
    print(md)


if __name__ == "__main__":
    main()
