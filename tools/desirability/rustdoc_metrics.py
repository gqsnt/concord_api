"""I1 rustdoc pass: D1c (generated share of the request-making public surface) and R4c (generated
rustdoc words per endpoint), from rustdoc JSON of each SDK.

Produce the JSON with the nightly toolchain (its version is recorded in the scoreboard):

    cargo +nightly rustdoc -p <pkg> --lib -- -Z unstable-options --output-format json

A public item is *generated* when its span lies inside an `api!` block of its file (the macro's
items carry spans in the invocation). Only items *reachable* from the crate root count: the JSON
must be built without `--document-private-items` (its `includes_private` is refused), and an item
counts only if a walk from the root module reaches it through module items, `use` re-exports,
the impls of reached types and traits, and the items of reached impls and traits. A `pub` method
of a type that callers cannot name (a `pub(crate)` client, a type in a private module that is not
re-exported) is therefore not part of the public surface. Run Python with `-I`.
"""
import json
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from static_scan import api_spans, strip_strings_and_comments  # noqa: E402

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


def field_ids(kind):
    """Field ids of a struct or variant `kind` (`plain`/`struct` field lists, `tuple` slots)."""
    if not isinstance(kind, dict):
        return []
    out = []
    for k, v in kind.items():
        if k == "tuple" and isinstance(v, list):
            out += [f for f in v if f is not None]
        elif isinstance(v, dict):
            out += v.get("fields", [])
    return out


def reachable_ids(doc):
    """Ids of the local items a walk from the crate root reaches (see the module docs)."""
    index = doc["index"]
    seen = set()
    todo = [doc["root"]]
    while todo:
        i = todo.pop()
        key = str(i)
        if key in seen or key not in index:
            continue
        item = index[key]
        if item.get("crate_id", 0) != 0:
            continue
        seen.add(key)
        inner = item.get("inner") or {}
        for kind, body in inner.items():
            if not isinstance(body, dict):
                continue
            if kind == "module":
                todo += body.get("items", [])
            elif kind == "use":
                if body.get("id") is not None:
                    todo.append(body["id"])
            elif kind in ("struct", "enum", "union"):
                todo += body.get("impls", [])
                todo += body.get("variants", [])
                todo += field_ids(body.get("kind"))
                todo += body.get("fields", [])  # union
            elif kind == "variant":
                todo += field_ids(body.get("kind"))
            elif kind == "trait":
                todo += body.get("items", [])
                todo += body.get("implementations", [])
            elif kind == "impl":
                todo += body.get("items", [])
    return seen


def require_public_view(doc, label="rustdoc JSON"):
    if doc.get("includes_private") is not False:
        raise ValueError(f"{label}: not the public view (includes_private is "
                         f"{doc.get('includes_private')!r}); build it without --document-private-items")


def analyse(rustdoc_json_path, pkg_dir, endpoint_names, forwarder_names, endpoint_count=None):
    """Metrics for one SDK. `endpoint_names` from the declaration parser; `forwarder_names` the
    static pass's D1a functions (hand-written public request-making functions)."""
    doc = json.load(open(rustdoc_json_path))
    require_public_view(doc, rustdoc_json_path)
    reach = reachable_ids(doc)
    ranges = block_line_ranges(os.path.join(pkg_dir, "src"))
    index = doc["index"]
    gen_endpoint_fns = 0
    gen_words = 0
    gen_items = 0
    unreachable_endpoint_fns = 0
    for key, item in index.items():
        if item.get("crate_id", 0) != 0:
            continue
        span = item.get("span")
        if not inside(span, ranges, pkg_dir):
            continue
        if key not in reach:
            inner = item.get("inner", {})
            if "function" in inner and item.get("name") in endpoint_names:
                unreachable_endpoint_fns += 1
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
        "generated_endpoint_fns_unreachable": unreachable_endpoint_fns,
        "D1c_generated_share": round(gen_endpoint_fns / denom, 4) if denom else None,
        "R4c_generated_doc_words": gen_words,
        "R4c_words_per_endpoint": round(gen_words / max(endpoint_count or len(endpoint_names), 1), 1),
        "format_version": doc.get("format_version"),
    }


def selftest():
    """Synthetic controls for the reachability walk (no cargo): a public method on a type that
    no module item or re-export reaches is not counted; a re-exported one is; a JSON built with
    private items is refused."""
    def item(i, name, inner, crate=0):
        return {"id": i, "crate_id": crate, "name": name, "inner": inner, "visibility": "public"}
    index = {
        "0": item(0, "root", {"module": {"items": [1, 5]}}),
        "1": item(1, "Reached", {"struct": {"kind": {"plain": {"fields": [9]}}, "impls": [2]}}),
        "2": item(2, None, {"impl": {"items": [3]}}),
        "3": item(3, "list", {"function": {}}),
        "4": item(4, "Hidden", {"struct": {"kind": "unit", "impls": [6]}}),
        "5": item(5, None, {"use": {"id": 8, "name": "Again", "is_glob": False}}),
        "6": item(6, None, {"impl": {"items": [7]}}),
        "7": item(7, "list", {"function": {}}),
        "8": item(8, "Again", {"enum": {"variants": [10], "impls": []}}),
        "9": item(9, "field", {"struct_field": {}}),
        "10": item(10, "V", {"variant": {"kind": {"tuple": [11, None]}}}),
        "11": item(11, "0", {"struct_field": {}}),
    }
    reach = reachable_ids({"root": 0, "index": index})
    expect = {"0", "1", "2", "3", "5", "8", "9", "10", "11"}
    results = {"reach_matches": reach == expect, "hidden_method_excluded": "7" not in reach}
    for view, should_refuse in ((True, True), (None, True), (False, False)):
        try:
            require_public_view({"includes_private": view, "root": 0, "index": index})
            refused = False
        except ValueError:
            refused = True
        results[f"includes_private={view}: refused={should_refuse}"] = refused == should_refuse
    return results


if __name__ == "__main__":
    r = selftest()
    print(json.dumps(r, sort_keys=True))
    raise SystemExit(0 if all(r.values()) else 1)
