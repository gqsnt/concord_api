"""I5 diagnostics scorer (metric R5).

Scores compiler output for seeded mistakes. A case passes when
  (a) the primary span of its first error lies in user code (the file the user wrote: for the UI
      cases `tests/ui/<case>.rs`, for the caller bank the caller's test file), and
  (b) the first error message (its headline and the label lines under the primary span) names no
      internal path: `__private`, `__macro`, `__concord`, `IntoValue`, `concord_core::` paths, or a
      generated type name the user never wrote (supplied per case as `generated_names`).
(c) "names the expected type" is reported separately for cases that declare an `expected` type.

    python3 -I tools/desirability/diagnostics/score.py ui crates/concord/tests/ui [--json OUT]
    python3 -I tools/desirability/diagnostics/score.py bank DIR [--json OUT]

The `bank` mode reads `<case>.stderr` next to `<case>.json` ({"user_file": .., "expected": ..,
"generated_names": [..]}) produced by `caller_bank.py`. Run Python with `-I`.
"""
import json
import os
import re
import sys

INTERNAL = re.compile(r"__private|__macro|__concord|\bIntoValue\b|concord_core::|concord_macros::")
SPAN = re.compile(r"^\s*-->\s+(\S+?):(\d+):(\d+)")


def first_error(stderr):
    """(headline, primary_file, block_text) of the first `error` in rustc output."""
    lines = stderr.splitlines()
    for i, ln in enumerate(lines):
        if ln.startswith("error"):
            block = [ln]
            primary = None
            for nxt in lines[i + 1:]:
                if nxt.startswith(("error", "warning")) and block:
                    break
                block.append(nxt)
                m = SPAN.match(nxt)
                if m and primary is None:
                    primary = m.group(1)
            # the label lines under the primary span (up to the first `= note`/`help` section)
            head_block = []
            for b in block:
                if b.strip().startswith(("= note", "= help", "note:", "help:")):
                    break
                head_block.append(b)
            return ln, primary, "\n".join(head_block)
    return None, None, ""


def score_case(stderr, user_file, generated_names=(), expected=None):
    headline, primary, block = first_error(stderr)
    if headline is None:
        return {"error": False, "pass": False, "span_in_user_code": False, "internal_names": []}
    in_user = bool(primary) and os.path.normpath(primary).endswith(os.path.normpath(user_file))
    internal = INTERNAL.findall(block)
    internal += [g for g in generated_names if re.search(r"\b" + re.escape(g) + r"\b", block)]
    out = {"error": True, "headline": headline, "primary": primary, "span_in_user_code": in_user,
           "internal_names": sorted(set(internal)), "pass": in_user and not internal}
    if expected:
        out["names_expected_type"] = expected in block
    return out


def generated_names_for(source, stderr):
    """Generated type names a UI case's user never wrote: identifiers in the compiler output that
    start with a client name declared in the case (`client Api` -> `ApiItem`, `ApiOnce`) and do not
    occur in the case's source."""
    clients = set(re.findall(r"\bclient\s+([A-Z]\w*)", source))
    names = set()
    for c in clients:
        for m in re.finditer(r"\b(" + re.escape(c) + r"[A-Z]\w*)\b", stderr):
            if not re.search(r"\b" + re.escape(m.group(1)) + r"\b", source):
                names.add(m.group(1))
    return sorted(names)


def score_ui(ui_dir):
    rows = {}
    for f in sorted(os.listdir(ui_dir)):
        if f.endswith(".stderr"):
            case = f[:-7]
            stderr = open(os.path.join(ui_dir, f), encoding="utf-8").read()
            rs = os.path.join(ui_dir, case + ".rs")
            source = open(rs, encoding="utf-8").read() if os.path.exists(rs) else ""
            rows[case] = score_case(stderr, f"tests/ui/{case}.rs", generated_names_for(source, stderr))
    return rows


def score_bank(bank_dir):
    rows = {}
    for f in sorted(os.listdir(bank_dir)):
        if f.endswith(".json") and os.path.exists(os.path.join(bank_dir, f[:-5] + ".stderr")):
            meta = json.load(open(os.path.join(bank_dir, f)))
            stderr = open(os.path.join(bank_dir, f[:-5] + ".stderr"), encoding="utf-8").read()
            rows[f[:-5]] = score_case(stderr, meta["user_file"], meta.get("generated_names", ()),
                                      meta.get("expected"))
    return rows


def summary(rows):
    # A case without the expected compile error counts as a failure (it stays in the denominator).
    scored = [r for r in rows.values() if r.get("error")]
    passed = sum(1 for r in rows.values() if r.get("pass"))
    exp = [r for r in scored if "names_expected_type" in r]
    return {"cases": len(rows), "with_error": len(scored), "pass": passed,
            "R5": round(passed / len(rows), 4) if rows else None,
            "span_in_user_code": sum(1 for r in scored if r["span_in_user_code"]),
            "internal_free": sum(1 for r in scored if not r["internal_names"]),
            "names_expected_type": f"{sum(1 for r in exp if r['names_expected_type'])}/{len(exp)}"}


def main():
    mode, path = sys.argv[1], sys.argv[2]
    rows = score_ui(path) if mode == "ui" else score_bank(path)
    s = summary(rows)
    if "--json" in sys.argv:
        out = sys.argv[sys.argv.index("--json") + 1]
        json.dump({"summary": s, "cases": rows}, open(out, "w"), indent=1, sort_keys=True)
    print(json.dumps(s))
    for case, r in rows.items():
        if not r.get("pass"):
            print(f"FAIL {case}: primary={r.get('primary')} internal={r.get('internal_names')}"
                  + ("" if r.get("error") else " (no compile error)"))


if __name__ == "__main__":
    main()
