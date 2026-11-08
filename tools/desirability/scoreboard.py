"""I1 scoreboard: per-SDK and total static metrics of a commit, plus a Markdown diff.

    python3 -I tools/desirability/scoreboard.py [--repo .] [--rustdoc-dir DIR] [--nightly VERSION]
        [--out v4/desirability/scoreboard/<sha>.json] [--against OLD.json ...]

Static metrics come from `static_scan.py`; D1c and R4c from rustdoc JSON files named
`<crate_name>.json` in `--rustdoc-dir` (see `rustdoc_metrics.py`). `--against` prints a Markdown
table of totals: each given scoreboard against this one.
"""
import argparse
import json
import os
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import decl  # noqa: E402
import rustdoc_metrics  # noqa: E402
import static_scan  # noqa: E402

# Direction of each headline metric (DESIRABILITY.md): -1 lower is better, +1 higher is better.
HEADLINE = {
    "D1a": -1, "D1b": -1, "D1c_generated_share": +1, "D2a_version_paths": -1,
    "D2a_shared_heads": -1, "D2a_shared_keys": -1, "D3": -1, "S2": -1,
    "R4a_dsl_lines_per_endpoint": -1, "decl_doc_mech": -1, "R4c_words_per_endpoint": -1,
}


def endpoint_names(pkg_dir):
    names = set()
    for dp, _, fs in os.walk(os.path.join(pkg_dir, "src")):
        for f in fs:
            if f.endswith(".rs"):
                for c in decl.parse_file(open(os.path.join(dp, f), encoding="utf-8").read()):
                    names |= {ep["name"] for ep, _ in decl.walk(c)}
    return names


def crate_name(pkg_dir):
    for line in open(os.path.join(pkg_dir, "Cargo.toml"), encoding="utf-8"):
        if line.startswith("name"):
            return line.split("=", 1)[1].strip().strip('"').replace("-", "_")
    return None


def build(repo, rustdoc_dir=None, nightly=None):
    examples = os.path.join(repo, "examples")
    per, tot = static_scan.scan(examples)
    if rustdoc_dir:
        shares, words, eps = [], 0, 0
        gen_eps = fwd = unreachable = 0
        for sdk, row in per.items():
            pkg = os.path.join(examples, sdk)
            path = os.path.join(rustdoc_dir, f"{crate_name(pkg)}.json")
            if not os.path.exists(path):
                row["rustdoc"] = None
                continue
            names = endpoint_names(pkg)
            r = rustdoc_metrics.analyse(path, pkg, names, row["forwarders"], row["endpoints"])
            row["rustdoc"] = r
            gen_eps += r["generated_endpoint_entry_points"]
            unreachable += r["generated_endpoint_fns_unreachable"]
            fwd += len(row["forwarders"])
            words += r["R4c_generated_doc_words"]
            eps += row["endpoints"]
        tot["D1c_generated_share"] = round(gen_eps / (gen_eps + fwd), 4) if gen_eps + fwd else None
        tot["R4c_words_per_endpoint"] = round(words / eps, 1) if eps else None
        tot["generated_endpoint_fns_unreachable"] = unreachable
        tot["rustdoc_nightly"] = nightly
    try:
        sha = subprocess.run(["git", "-C", repo, "rev-parse", "HEAD"], capture_output=True,
                             text=True, check=True).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        sha = None
    return {"commit": sha, "instrument": "I1 scoreboard v1", "totals": tot, "per_sdk": per}


def diff_table(old_docs, new):
    keys = [k for k in HEADLINE if k in new["totals"]]
    head = "| Metric | dir | " + " | ".join(f"`{(o.get('commit') or '?')[:8]}`" for o in old_docs) + \
           f" | `{(new.get('commit') or '?')[:8]}` |"
    lines = [head, "|---|---|" + "---|" * (len(old_docs) + 1)]
    for k in keys:
        vals = [o["totals"].get(k) for o in old_docs]
        lines.append(f"| {k} | {'↓' if HEADLINE[k] < 0 else '↑'} | " +
                     " | ".join(str(v) for v in vals) + f" | {new['totals'].get(k)} |")
    return "\n".join(lines)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", default=".")
    ap.add_argument("--rustdoc-dir")
    ap.add_argument("--nightly")
    ap.add_argument("--out")
    ap.add_argument("--against", nargs="*", default=[])
    a = ap.parse_args()
    doc = build(a.repo, a.rustdoc_dir, a.nightly)
    if a.out:
        os.makedirs(os.path.dirname(a.out) or ".", exist_ok=True)
        json.dump(doc, open(a.out, "w"), indent=1, sort_keys=True)
    olds = [json.load(open(p)) for p in a.against]
    print(diff_table(olds, doc))


if __name__ == "__main__":
    main()
