#!/usr/bin/env python3
"""Independent composition oracle for Concord V4 query/header slots (SEMANTICS.md section 5).

Generates crates/concord/tests/oracle_composition.rs. Run from anywhere:

    python3 -I tests-gen/oracle_composition.py            # (re)write the Rust file
    python3 -I tests-gen/oracle_composition.py --check    # exit 1 if the file is stale
    python3 -I tests-gen/oracle_composition.py --stats    # print domain sizes only

The model below is written from the prose of SEMANTICS.md section 5 only. It does not read or
mirror the macro's resolver. Domain: a chain client -> scope -> endpoint; each owner has 0 or 1
"statement kind" for query key "k" and, in the same shape, for header "x-k". Query key "a" is
introduced by the client (a block statement, after any direct placement) and "z" by the scope
(after its k statement), so the position of k's slot relative to them is observable.

Model rules used (quoted from section 5):
  * contributions are collected root -> scopes -> endpoint; within one owner, direct placements
    come before block statements (source order); `remove` statements act first, on what the owner
    inherits.
  * `"k" = v` / direct placement: requires no live writer; slot becomes live with v.
  * `append`: no requirement; adds after existing occurrences; slot becomes live.
  * `override "k" = v`: requires live; slot holds exactly v.
  * `override "k" = v else inherit`: requires live; present -> exactly v; absent -> unchanged.
  * `remove`: requires live; emptied, no longer live.
  * a slot is positioned at the key's first appearance on the chain (static: the first statement
    naming it, whether or not its source is present at run time); a removed and reintroduced key
    keeps that position.
  * a source is present when every optional value it reads is present; an absent source emits
    nothing.
  * query string = slots in first-appearance order, occurrences within a slot in order.
Liveness ("requires a live writer earlier on the chain") is a declaration-time property: an
optional source counts as a writer whether or not it is present at run time.
"""

import itertools
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "..", "crates", "concord", "tests", "oracle_composition.rs")

OWNERS = ("c", "s", "e")  # client, scope, endpoint

# Statement kinds. Each expands to a list of abstract ops for one family:
#   ("direct", src)                       direct placement (query only; header uses "intro")
#   ("intro", src) ("append", src) ("override", src, else_inherit) ("remove",)
# src is ("lit", text) or ("opt", owner).
KINDS = ["n", "il", "io", "al", "ao", "ol", "oo", "rm", "ri", "ir", "dp", "dr"]
DESCR = {
    "n": "no statement",
    "il": "introduce literal",
    "io": "introduce optional value",
    "al": "append literal",
    "ao": "append optional value",
    "ol": "override literal",
    "oo": "override optional value else inherit",
    "rm": "remove",
    "ri": "remove; introduce literal",
    "ir": "introduce literal; remove (source order; remove acts first)",
    "dp": "direct optional placement (header family: introduce optional value)",
    "dr": "direct optional placement + remove (header family: remove; introduce optional)",
}


def uses_value(kind):
    return kind in ("io", "ao", "oo", "dp", "dr")


def ops(kind, owner, family):
    lit = lambda tag: ("lit", owner + tag)  # noqa: E731
    opt = ("opt", owner)
    direct = "direct" if family == "query" else "intro"
    return {
        "n": [],
        "il": [("intro", lit("i"))],
        "io": [("intro", opt)],
        "al": [("append", lit("a"))],
        "ao": [("append", opt)],
        "ol": [("override", lit("o"), False)],
        "oo": [("override", opt, True)],
        "rm": [("remove",)],
        "ri": [("remove",), ("intro", lit("r"))],
        "ir": [("intro", lit("r")), ("remove",)],
        "dp": [(direct, opt)],
        "dr": [(direct, opt), ("remove",)] if family == "query" else [("remove",), ("intro", opt)],
    }[kind]


def owner_contributions(owner, kind, family):
    """Ordered (key, op) contributions of one owner for one family."""
    key = "k" if family == "query" else "x-k"
    mine = ops(kind, owner, family)
    direct = [(key, op) for op in mine if op[0] == "direct"]
    block = [(key, op) for op in mine if op[0] != "direct"]
    extra_before, extra_after = [], []
    if family == "query" and owner == "c":
        extra_before = [("a", ("intro", ("lit", "1")))]
    if family == "query" and owner == "s":
        extra_after = [("z", ("intro", ("lit", "1")))]
    return direct + extra_before + block + extra_after


def legal(chain, family):
    """Declaration-time check: None if accepted, else a reason string."""
    live = set()
    for owner, kind in zip(OWNERS, chain):
        contribs = owner_contributions(owner, kind, family)
        for key, op in [c for c in contribs if c[1][0] == "remove"]:
            if key not in live:
                return f"{owner}: remove {key} without a live writer"
            live.discard(key)
        for key, op in [c for c in contribs if c[1][0] != "remove"]:
            if op[0] in ("intro", "direct"):
                if key in live:
                    return f"{owner}: introduce {key} while live"
            elif op[0] == "override":
                if key not in live:
                    return f"{owner}: override {key} without a live writer"
            live.add(key)
    return None


def evaluate(chain, family, present):
    """Run-time result: ordered list of (key, value) occurrences."""
    slots = {}  # insertion-ordered: first appearance

    def value(src):
        if src[0] == "lit":
            return src[1]
        return (src[1] + "V") if present[src[1]] else None

    for owner, kind in zip(OWNERS, chain):
        contribs = owner_contributions(owner, kind, family)
        for key, _ in contribs:
            slots.setdefault(key, [])
        for key, op in [c for c in contribs if c[1][0] == "remove"]:
            slots[key] = []
        for key, op in [c for c in contribs if c[1][0] != "remove"]:
            v = value(op[1])
            if op[0] in ("intro", "direct", "append"):
                if v is not None:
                    slots[key].append(v)
            elif op[0] == "override":
                if v is not None:
                    slots[key] = [v]
                elif not op[2]:
                    raise AssertionError("total override with absent source")
    return [(k, v) for k, vs in slots.items() for v in vs]


def presence_space(chain):
    owners = [o for o, k in zip(OWNERS, chain) if uses_value(k)]
    for bits in itertools.product((False, True), repeat=len(owners)):
        present = {o: False for o in OWNERS}
        present.update(dict(zip(owners, bits)))
        yield owners, present


# ---------------------------------------------------------------------------------------------
# Rust rendering


def rust_src(src):
    return f'"{src[1]}"' if src[0] == "lit" else f"{src[1]}v"


def rust_block(owner, kind, family):
    lines = []
    for key, op in owner_contributions(owner, kind, family):
        if op[0] == "direct":
            continue
        if op[0] == "intro":
            lines.append(f'"{key}" = {rust_src(op[1])};')
        elif op[0] == "append":
            lines.append(f'append "{key}" = {rust_src(op[1])};')
        elif op[0] == "override":
            tail = " else inherit" if op[2] else ""
            lines.append(f'override "{key}" = {rust_src(op[1])}{tail};')
        elif op[0] == "remove":
            lines.append(f'remove "{key}";')
    return " ".join(lines)


def rust_params(owner, kind):
    if not uses_value(kind):
        return ""
    placement = 'query("k") ' if kind in ("dp", "dr") else ""
    return f"{placement}{owner}v?: String"


def rust_owner_body(owner, kind):
    parts = []
    q = rust_block(owner, kind, "query")
    h = rust_block(owner, kind, "header")
    if q:
        parts.append(f"query {{ {q} }}")
    if h:
        parts.append(f"headers {{ {h} }}")
    return " ".join(parts)


def expected_query(occ):
    return "&".join(f"{k}={v}" for k, v in occ)


UI_DIR = os.path.join(HERE, "..", "crates", "concord", "tests", "oracle_ui")

# A few chains the model rejects, emitted as trybuild compile-fail cases (expected stderr is
# blessed with TRYBUILD=overwrite and reviewed; the model only asserts that each is rejected).
REJECT_SAMPLES = [
    ("introduce_while_live", ("il", "il", "n")),
    ("direct_while_live", ("il", "n", "dp")),
    ("override_without_writer", ("n", "ol", "n")),
    ("override_else_inherit_without_writer", ("n", "n", "oo")),
    ("remove_without_writer", ("n", "n", "rm")),
    # Source order puts the introduction first, but `remove` acts first within an owner.
    ("remove_acts_first_within_owner", ("ir", "n", "n")),
]


def render_reject(name, chain):
    ck, sk, ek = chain
    cp, sp, ep = rust_params("c", ck), rust_params("s", sk), rust_params("e", ek)
    ebody = rust_owner_body("e", ek)
    reason = legal(chain, "query") or legal(chain, "header")
    return "\n".join([
        "// @generated by tests-gen/oracle_composition.py -- do not edit by hand.",
        f"// Oracle-rejected chain {ck}/{sk}/{ek}: {reason}.",
        "",
        "concord::api! {",
        f"    pub client C{'(' + cp + ')' if cp else ''} {{",
        '        base "http://oracle.invalid";',
        f"        {rust_owner_body('c', ck)}",
        f"        scope s({sp}) {{",
        f"            {rust_owner_body('s', sk)}",
        f"            GET e({ep}){' { ' + ebody + ' }' if ebody else ';'}",
        "        }",
        "    }",
        "}",
        "",
        "fn main() {}",
        "",
    ])


def main():
    args = sys.argv[1:]
    cases = []
    rejected = 0
    for chain in itertools.product(KINDS, repeat=3):
        rq, rh = legal(chain, "query"), legal(chain, "header")
        if rq or rh:
            rejected += 1
            continue
        cases.append(chain)
    checks = sum(1 for c in cases for _ in presence_space(c))
    if "--stats" in args:
        print(f"kinds={len(KINDS)} chains={len(KINDS) ** 3} accepted={len(cases)} "
              f"rejected={rejected} presence-checks={checks}")
        return 0

    out = []
    w = out.append
    w("// @generated by tests-gen/oracle_composition.py -- do not edit by hand.")
    w("// Regenerate: python3 -I tests-gen/oracle_composition.py (from the repository root);")
    w("// check freshness: python3 -I tests-gen/oracle_composition.py --check")
    w("//!")
    w("//! Independent composition oracle (SEMANTICS.md section 5). Each client/scope/endpoint")
    w("//! carries one statement kind for query key `k` and header `x-k`; the expected query string")
    w("//! and `x-k` values were computed by the generator's own model for every presence state.")
    w(f"//! Domain: {len(KINDS)} kinds per owner, {len(KINDS) ** 3} chains, {len(cases)} accepted,")
    w(f"//! {rejected} rejected by the contract (skipped), {checks} presence checks.")
    w("//! Kinds:")
    for k, d in DESCR.items():
        w(f"//! - `{k}`: {d}")
    w("#![allow(missing_docs, non_snake_case, non_camel_case_types, clippy::all)]")
    w("")
    w("use concord::reqwest;")
    w("")
    w("fn check<T>(p: concord::Prepared<T>, query: &str, header: &[&str], case: &str) {")
    w("    let request = p.request();")
    w("    assert_eq!(request.url().query(), Some(query), \"query, case {case}\");")
    w("    let got: Vec<&str> = request")
    w("        .headers()")
    w("        .get_all(\"x-k\")")
    w("        .iter()")
    w("        .map(|v| v.to_str().unwrap())")
    w("        .collect();")
    w("    assert_eq!(got, header, \"header x-k, case {case}\");")
    w("}")
    w("")

    by_client = {}
    for chain in cases:
        by_client.setdefault(chain[0], {}).setdefault(chain[1], []).append(chain[2])

    for ck, scopes in by_client.items():
        w("concord::api! {")
        cp = rust_params("c", ck)
        w(f"    pub client C_{ck}{'(' + cp + ')' if cp else ''} {{")
        w('        base "http://oracle.invalid";')
        w(f"        {rust_owner_body('c', ck)}")
        for sk, eks in scopes.items():
            sp = rust_params("s", sk)
            w(f"        scope s_{sk}({sp}) {{")
            w(f"            {rust_owner_body('s', sk)}")
            for ek in eks:
                ep = rust_params("e", ek)
                body = rust_owner_body("e", ek)
                w(f"            GET e_{ek}({ep}){' { ' + body + ' }' if body else ';'}")
            w("        }")
        w("    }")
        w("}")
        w("")

        w("#[test]")
        w("#[rustfmt::skip]")
        w(f"fn oracle_client_{ck}() {{")
        w("    let http = reqwest::Client::new();")
        for sk, eks in scopes.items():
            for ek in eks:
                chain = (ck, sk, ek)
                for owners, present in presence_space(chain):
                    q = expected_query(evaluate(chain, "query", present))
                    h = [v for _, v in evaluate(chain, "header", present)]
                    case = f"{ck}/{sk}/{ek} " + ",".join(
                        f"{o}={'P' if present[o] else 'A'}" for o in owners)

                    def ctl(o):
                        if o not in owners:
                            return ""
                        return f'.set_{o}v("{o}V")' if present[o] else f".maybe_{o}v(None)"

                    expr = (f"C_{ck}::new(http.clone()){ctl('c')}"
                            f".s_{sk}(){ctl('s')}.e_{ek}(){ctl('e')}")
                    hs = ", ".join(f'"{v}"' for v in h)
                    w(f'    check({expr}.prepare().unwrap(), "{q}", &[{hs}], "{case}");')
        w("}")
        w("")

    text = "\n".join(out)
    ui = {name: render_reject(name, chain) for name, chain in REJECT_SAMPLES}
    for name, chain in REJECT_SAMPLES:
        reason = legal(chain, "query") or legal(chain, "header")
        assert reason, f"{name} {chain} is accepted by the model"
    if "--check" in args:
        stale = False
        for name, src in ui.items():
            with open(os.path.join(UI_DIR, name + ".rs")) as f:
                stale |= f.read() != src
        with open(OUT) as f:
            if stale or f.read() != text:
                print("stale: rerun python3 -I tests-gen/oracle_composition.py", file=sys.stderr)
                return 1
        return 0
    with open(OUT, "w") as f:
        f.write(text)
    os.makedirs(UI_DIR, exist_ok=True)
    for name, src in ui.items():
        with open(os.path.join(UI_DIR, name + ".rs"), "w") as f:
            f.write(src)
    print(f"wrote {os.path.normpath(OUT)}: {len(cases)} cases, {checks} checks, {rejected} rejected")
    return 0


if __name__ == "__main__":
    sys.exit(main())
