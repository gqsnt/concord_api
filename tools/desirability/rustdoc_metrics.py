"""I1 rustdoc pass: D1c (generated share of the request-making public surface) and R4c (generated
rustdoc words per endpoint), from rustdoc JSON of each SDK.

Produce the JSON with the nightly toolchain (its version is recorded in the scoreboard):

    cargo +nightly rustdoc -p <pkg> --lib -- -Z unstable-options --output-format json

A public item is *generated* when its span lies inside an `api!` block of its file (the macro's
items carry spans in the invocation). Run Python with `-I`.
"""
import json
import os
import re

from static_scan import api_spans, strip_strings_and_comments

WORD = re.compile(r"[A-Za-z0-9_`'.:/{}<>-]+")


def block_line_ranges(src_root):
    """{relative file path: [(first_line, last_line), ...]} of api! blocks (1-based lines)."""
    out = {}
    for dp, _, fs in os.walk(src_root):
        for f in fs:
            if not f.endswith(".rs"):
                continue
            p = os.path.join(dp, f)
            text = open(p, encoding="utf-8").read()
            clean = strip_strings_and_comments(text)
            ranges = [(text.count("\n", 0, s) + 1, text.count("\n", 0, e) + 1) for s, e in api_spans(text, clean)]
            if ranges:
                out[os.path.realpath(p)] = ranges
    return out


def inside(span, ranges, pkg_dir):
    if not span:
        return False
    fn = span.get("filename", "")
    path = os.path.realpath(fn if os.path.isabs(fn) else os.path.join(pkg_dir, fn))
    if path not in ranges:
        # rustdoc filenames are relative to the workspace root
        path = os.path.realpath(os.path.join(pkg_dir, "..", "..", fn))
    line = span.get("begin", [0])[0]
    return any(a <= line <= b for a, b in ranges.get(path, []))


def analyse(rustdoc_json_path, pkg_dir, endpoint_names, forwarder_names, endpoint_count=None):
    """Metrics for one SDK. `endpoint_names` from the declaration parser; `forwarder_names` the
    static pass's D1a functions (hand-written public request-making functions)."""
    doc = json.load(open(rustdoc_json_path))
    ranges = block_line_ranges(os.path.join(pkg_dir, "src"))
    index = doc["index"]
    gen_endpoint_fns = 0
    gen_words = 0
    gen_items = 0
    for item in index.values():
        if item.get("crate_id", 0) != 0:
            continue
        span = item.get("span")
        if not inside(span, ranges, pkg_dir):
            continue
        gen_items += 1
        words = len(WORD.findall(item.get("docs") or ""))
        gen_words += words
        inner = item.get("inner", {})
        if "function" in inner and item.get("name") in endpoint_names \
                and item.get("visibility") == "public":
            gen_endpoint_fns += 1
    # D1c denominator: public generated endpoint entry points + hand-written public forwarders
    denom = gen_endpoint_fns + len(forwarder_names)
    return {
        "generated_public_items": gen_items,
        "generated_endpoint_entry_points": gen_endpoint_fns,
        "D1c_generated_share": round(gen_endpoint_fns / denom, 4) if denom else None,
        "R4c_generated_doc_words": gen_words,
        "R4c_words_per_endpoint": round(gen_words / max(endpoint_count or len(endpoint_names), 1), 1),
        "format_version": doc.get("format_version"),
    }
