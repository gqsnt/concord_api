#!/usr/bin/env python3
"""Independent sensitivity oracle for Concord V4 (SEMANTICS.md section 9).

Generates crates/concord/tests/oracle_sensitivity.rs. Run from anywhere:

    python3 -I tests-gen/oracle_sensitivity.py            # (re)write the Rust file
    python3 -I tests-gen/oracle_sensitivity.py --check    # exit 1 if the file is stale
    python3 -I tests-gen/oracle_sensitivity.py --stats

Domain: one client value `v` (marked `#[sensitive]` or not) reused in a non-empty subset of
placements {host label, path segment, query key Q, header H}, where Q and H range over a plain
name, a safety-list name (spelled so that the "lowercased, `_` -> `-`" normalization matters for
Q) and a near-miss that is not on the list. The placements are written either on the client
(bare `v`) or on the endpoint (`client.v`). A public endpoint value `w` (query `p`) is always
present as a negative control for over-redaction.

Model (from section 9 prose only):
  * a key is sensitive if a contribution reads a marked value or its normalized name is on the
    safety list;
  * a value is withheld if marked or if it feeds a sensitive key on any chain (fixpoint);
  * every key, path segment and host label that reads a withheld value is sensitive;
  * generated Debug withholds withheld values; Prepared Debug shows sensitive keys' values,
    segments and labels as `<redacted>`; sensitive header values carry `is_sensitive()`;
  * sensitive values are always sent on the wire (positive control on `prepared.request()`);
  * Error Display contains only static text (checked on a build error caused by another value).
"""

import itertools
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "..", "crates", "concord", "tests", "oracle_sensitivity.rs")

SAFETY = {
    "authorization", "proxy-authorization", "cookie", "set-cookie", "x-api-key", "api-key",
    "apikey", "x-auth-token", "auth-token", "access-token", "refresh-token", "id-token", "token",
    "client-secret", "secret", "password", "signature", "session",
}
QUERY_NAMES = ["q", "Access_Token", "tokens"]
HEADER_NAMES = ["x-h", "Authorization", "x-token"]
V = "zq7secretv"
W = "pubw9"
BASE_HOST = "sens.example.test"


def normalized(name):
    return name.lower().replace("_", "-")


def cases():
    out = []
    for marked, site in itertools.product((False, True), ("client", "endpoint")):
        for host, path in itertools.product((False, True), repeat=2):
            for q in [None] + QUERY_NAMES:
                for h in [None] + HEADER_NAMES:
                    if not (host or path or q or h):
                        continue
                    out.append(dict(marked=marked, site=site, host=host, path=path, q=q, h=h))
    return out


def withheld(c):
    keys = [n for n in (c["q"], c["h"]) if n]
    return c["marked"] or any(normalized(n) in SAFETY for n in keys)


def render(i, c):
    ref = "v" if c["site"] == "client" else "client.v"
    stmts = []
    if c["host"]:
        stmts.append(f'subdomain "{{{ref}}}";')
    if c["path"]:
        stmts.append(f'path "{{{ref}}}";')
    qblock = [f'"{c["q"]}" = {ref};'] if c["q"] else []
    hblock = [f'"{c["h"]}" = {ref};'] if c["h"] else []
    attr = "#[sensitive] " if c["marked"] else ""
    name = f"S{i:03}"
    lines = [
        "concord::api! {",
        f"    pub client {name}({attr}v: String) {{",
        f'        base "http://{BASE_HOST}";',
    ]
    client_stmts, ep_stmts = [], []
    if c["site"] == "client":
        client_stmts = stmts + ([f"query {{ {' '.join(qblock)} }}"] if qblock else []) + (
            [f"headers {{ {' '.join(hblock)} }}"] if hblock else [])
        ep_q = ['"p" = w;']
        ep_h = []
    else:
        ep_stmts = stmts
        ep_q = qblock + ['"p" = w;']
        ep_h = hblock
    for s in client_stmts:
        lines.append(f"        {s}")
    body = ep_stmts + [f"query {{ {' '.join(ep_q)} }}"] + (
        [f"headers {{ {' '.join(ep_h)} }}"] if ep_h else [])
    lines.append(f"        GET ok(w: String) {{ {' '.join(body)} }}")
    lines.append('        GET fail(bad: String) { headers { "x-bad" = bad; } }')
    if sticky(c):
        # Stickiness: a public override, or removal + public reintroduction, of a key that
        # reads `v` keeps the key's sensitivity.
        for ep, ops in (("over", lambda k: f'override "{k}" = "pubo";'),
                        ("reint", lambda k: f'remove "{k}"; "{k}" = "pubr";')):
            blocks = ([f"query {{ {ops(c['q'])} }}"] if c["q"] else []) + (
                [f"headers {{ {ops(c['h'])} }}"] if c["h"] else [])
            lines.append(f"        GET {ep}() {{ {' '.join(blocks)} }}")
    lines.append("    }")
    lines.append("}")
    return name, "\n".join(lines)


def sticky(c):
    return c["site"] == "client" and bool(c["q"] or c["h"])


def describe(c):
    return (f"marked={c['marked']} site={c['site']} host={c['host']} path={c['path']} "
            f"query={c['q']} header={c['h']}")


def rust_str(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def render_test(i, name, c):
    hidden = withheld(c)
    lines = []
    w = lines.append
    w("#[test]")
    w("#[rustfmt::skip]")
    w(f"fn case_{i:03}() {{")
    w(f"    let case = {rust_str(describe(c))};")
    w(f"    let api = {name}::new(reqwest::Client::new(), {name}Config {{ v: V.into() }});")
    w("    let endpoint = api.clone().ok(W);")
    w("    let endpoint_debug = format!(\"{endpoint:?}\");")
    w(f'    assert!(endpoint_debug.contains(r#"w: "{W}""#), "{{case}}: {{endpoint_debug}}");')
    w("    let p = endpoint.prepare().unwrap();")
    w("    let url = p.request().url().clone();")
    w("    let debug = format!(\"{p:?}\");")
    w("    // Wire-side positive control: the raw value is always sent.")
    if c["host"]:
        w(f'    assert_eq!(url.host_str(), Some("{V}.{BASE_HOST}"), "{{case}}");')
    if c["path"]:
        w(f'    assert_eq!(url.path(), "/{V}", "{{case}}");')
    exp_q = (f"{c['q']}={V}&" if c["q"] else "") + f"p={W}"
    w(f'    assert_eq!(url.query(), Some("{exp_q}"), "{{case}}");')
    if c["h"]:
        hn = c["h"].lower()
        w(f'    let hv = &p.request().headers()["{hn}"];')
        w(f'    assert_eq!(hv, V, "{{case}}");')
        w(f'    assert_eq!(hv.is_sensitive(), {str(hidden).lower()}, "{{case}}");')
    w(f'    assert!(debug.contains("p={W}"), "over-redaction, {{case}}: {{debug}}");')
    w('    let client_debug = format!("{api:?}");')
    w('    let error = api.fail("bad\\u{1}").prepare().unwrap_err();')
    w("    assert_eq!(error.kind(), concord::ErrorKind::Build);")
    w('    let shown = format!("{error} {error:?}");')
    w('    assert!(shown.contains("x-bad"), "{case}: {shown}");')
    if hidden:
        w('    for (what, text) in [("prepared", &debug), ("client", &client_debug), '
          '("endpoint", &endpoint_debug), ("error", &shown)] {')
        w('        assert!(!text.contains(V), "{what} leaks, {case}: {text}");')
        w("    }")
        w('    assert!(client_debug.contains("v: <redacted>"), "{case}: {client_debug}");')
        if c["host"]:
            w(f'    assert!(debug.contains("://<redacted>.{BASE_HOST}"), "{{case}}: {{debug}}");')
        if c["path"]:
            w(f'    assert!(debug.contains("{BASE_HOST}/<redacted>"), "{{case}}: {{debug}}");')
        if c["q"]:
            w(f'    assert!(debug.contains("{c["q"]}=<redacted>"), "{{case}}: {{debug}}");')
        if c["h"]:
            w(f'    assert!(debug.contains("{c["h"].lower()}: <redacted>"), "{{case}}: {{debug}}");')
    else:
        w("    // Negative control: nothing makes `v` sensitive, so it is shown.")
        w(f'    assert!(client_debug.contains(r#"v: "{V}""#), "{{case}}: {{client_debug}}");')
        w('    assert!(!debug.contains("<redacted>"), "{case}: {debug}");')
        if c["host"]:
            w(f'    assert!(debug.contains("://{V}.{BASE_HOST}"), "{{case}}: {{debug}}");')
        if c["path"]:
            w(f'    assert!(debug.contains("{BASE_HOST}/{V}"), "{{case}}: {{debug}}");')
        if c["q"]:
            w(f'    assert!(debug.contains("{c["q"]}={V}"), "{{case}}: {{debug}}");')
        if c["h"]:
            w(f'    assert!(debug.contains("{c["h"].lower()}: {V}"), "{{case}}: {{debug}}");')
        w('    assert!(!shown.contains(V), "error Display/Debug is static text, {case}: {shown}");')
    if sticky(c):
        w("    // Sticky: the public replacement is on the wire; the key stays as sensitive as before.")
        w('    for (p, public) in [(api.clone().over().prepare().unwrap(), "pubo"), '
          '(api.clone().reint().prepare().unwrap(), "pubr")] {')
        w('        let debug = format!("{p:?}");')
        if c["q"]:
            w(f'        assert_eq!(p.request().url().query(), Some(format!("{c["q"]}={{public}}").as_str()), "{{case}}");')
            shown_q = "<redacted>" if hidden else "{public}"
            w(f'        assert!(debug.contains(&format!("{c["q"]}={shown_q}")), "{{case}}: {{debug}}");')
        else:
            w('        assert_eq!(p.request().url().query(), None, "{case}");')
        if c["h"]:
            hn = c["h"].lower()
            w(f'        let hv = &p.request().headers()["{hn}"];')
            w('        assert_eq!(hv, public, "{case}");')
            w(f'        assert_eq!(hv.is_sensitive(), {str(hidden).lower()}, "{{case}}");')
            shown_h = "<redacted>" if hidden else "{public}"
            w(f'        assert!(debug.contains(&format!("{hn}: {shown_h}")), "{{case}}: {{debug}}");')
        if hidden:
            w('        assert!(!debug.contains(public) && !debug.contains(V), "{case}: {debug}");')
        w("    }")
    w("}")
    return "\n".join(lines)


def main():
    args = sys.argv[1:]
    all_cases = cases()
    hidden = sum(1 for c in all_cases if withheld(c))
    if "--stats" in args:
        print(f"cases={len(all_cases)} withheld={hidden} shown={len(all_cases) - hidden} "
              f"sticky={sum(1 for c in all_cases if sticky(c))}")
        return 0
    out = [
        "// @generated by tests-gen/oracle_sensitivity.py -- do not edit by hand.",
        "// Regenerate: python3 -I tests-gen/oracle_sensitivity.py (from the repository root);",
        "// check freshness: python3 -I tests-gen/oracle_sensitivity.py --check",
        "//!",
        "//! Independent sensitivity oracle (SEMANTICS.md section 9): one client value reused across",
        "//! host label, path segment, query and header placements, with and without `#[sensitive]`",
        "//! and under safety-list / near-miss key names. Expected redaction is computed by the",
        "//! generator; every case also checks the raw value on the native request (wire side).",
        f"//! Domain: {len(all_cases)} cases ({hidden} withheld, {len(all_cases) - hidden} shown);",
        f"//! {sum(1 for c in all_cases if sticky(c))} of them also check stickiness under a public",
        "//! `override` and under `remove` + public reintroduction of the same key.",
        "#![allow(missing_docs, clippy::all)]",
        "",
        "use concord::reqwest;",
        "",
        f"const V: &str = {rust_str(V)};",
        f"const W: &str = {rust_str(W)};",
        "",
    ]
    for i, c in enumerate(all_cases):
        name, decl = render(i, c)
        out.append(f"// {describe(c)}")
        out.append(decl)
        out.append(render_test(i, name, c))
        out.append("")
    text = "\n".join(out)
    if "--check" in args:
        with open(OUT) as f:
            if f.read() != text:
                print("stale: rerun python3 -I tests-gen/oracle_sensitivity.py", file=sys.stderr)
                return 1
        return 0
    with open(OUT, "w") as f:
        f.write(text)
    print(f"wrote {os.path.normpath(OUT)}: {len(all_cases)} cases ({hidden} withheld)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
