#!/usr/bin/env python3
"""Summarize Criterion results as a markdown table.

Reads `<root>/**/<current>/{benchmark,estimates}.json` (default root `target/criterion`, current
`new`) and prints, per benchmark: mean, 95% CI of the mean, throughput, and with `--baseline NAME`
the baseline mean (from `<bench>/<NAME>/estimates.json`) and the relative change of the means.
When the last `cargo bench` ran with `--baseline NAME`, Criterion's own change estimate
(`<bench>/change/estimates.json`: mean relative change with its 95% CI) is shown too.

Usage:
  python3 -I benches/criterion/summarize.py [--root DIR] [--current NAME] [--baseline NAME]
                                            [--filter SUBSTRING] [--criterion-change]

Standard library only.
"""

import argparse
import json
import os
import sys


def fmt_time(ns):
    if ns is None:
        return "-"
    for unit, scale in (("s", 1e9), ("ms", 1e6), ("µs", 1e3)):
        if ns >= scale:
            return f"{ns / scale:.3f} {unit}"
    return f"{ns:.1f} ns"


def load(path):
    try:
        with open(path, encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError):
        return None


def throughput(bench, mean_ns):
    t = bench.get("throughput") or {}
    if not mean_ns:
        return "-"
    if "Bytes" in t:
        return f"{t['Bytes'] / (mean_ns / 1e9) / (1 << 20):.1f} MiB/s"
    if "BytesDecimal" in t:
        return f"{t['BytesDecimal'] / (mean_ns / 1e9) / 1e6:.1f} MB/s"
    if "Elements" in t:
        return f"{mean_ns / t['Elements']:.1f} ns/elem"
    return "-"


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--root", default="target/criterion")
    ap.add_argument("--current", default="new", help="result directory to report (default: new)")
    ap.add_argument("--baseline", help="named baseline (from --save-baseline) to compare against")
    ap.add_argument("--filter", default="", help="only IDs containing this substring")
    ap.add_argument(
        "--criterion-change",
        action="store_true",
        help="also show change/estimates.json (valid only if the last run used --baseline NAME)",
    )
    args = ap.parse_args()

    rows = []
    for dirpath, _dirnames, filenames in os.walk(args.root):
        if os.path.basename(dirpath) != args.current or "benchmark.json" not in filenames:
            continue
        bench = load(os.path.join(dirpath, "benchmark.json"))
        est = load(os.path.join(dirpath, "estimates.json"))
        if not bench or not est:
            continue
        full_id = bench.get("full_id") or bench.get("title") or dirpath
        if args.filter not in full_id:
            continue
        mean = est["mean"]
        ci = mean["confidence_interval"]
        bench_dir = os.path.dirname(dirpath)
        row = {
            "id": full_id,
            "mean": mean["point_estimate"],
            "lo": ci["lower_bound"],
            "hi": ci["upper_bound"],
            "tp": throughput(bench, mean["point_estimate"]),
        }
        if args.baseline:
            base = load(os.path.join(bench_dir, args.baseline, "estimates.json"))
            row["base"] = base["mean"]["point_estimate"] if base else None
            if args.criterion_change:
                change = load(os.path.join(bench_dir, "change", "estimates.json"))
                row["change"] = change["mean"] if change else None
        rows.append(row)

    if not rows:
        sys.exit(f"no '{args.current}' results under {args.root}")
    rows.sort(key=lambda r: r["id"])

    head = ["benchmark", "mean", "95% CI (mean)", "throughput"]
    if args.baseline:
        head += [f"{args.baseline} mean", "change (means)"]
        if args.criterion_change:
            head += ["criterion change [95% CI]"]
    print("| " + " | ".join(head) + " |")
    print("|" + "|".join(["---"] + ["---:"] * (len(head) - 1)) + "|")
    for r in rows:
        cells = [
            f"`{r['id']}`",
            fmt_time(r["mean"]),
            f"{fmt_time(r['lo'])} – {fmt_time(r['hi'])}",
            r["tp"],
        ]
        if args.baseline:
            base = r.get("base")
            cells.append(fmt_time(base))
            cells.append(f"{(r['mean'] / base - 1) * 100:+.1f}%" if base else "-")
            if args.criterion_change:
                c = r.get("change")
                if c:
                    cci = c["confidence_interval"]
                    cells.append(
                        f"{c['point_estimate'] * 100:+.1f}% "
                        f"[{cci['lower_bound'] * 100:+.1f}%, {cci['upper_bound'] * 100:+.1f}%]"
                    )
                else:
                    cells.append("-")
        print("| " + " | ".join(cells) + " |")


if __name__ == "__main__":
    main()
