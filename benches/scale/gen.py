#!/usr/bin/env python3
"""Synthetic Concord declarations and consumers for compile-cost and code-size measurement.

Every output is a pure function of the command line: no randomness, no timestamps, fixed
iteration order. Re-running a command rewrites the same files with the same bytes (the only
exception is Cargo.lock, which cargo settles offline from a seeded copy; its hash is recorded).

COMMANDS

  gen.py suite OUT [--concord PATH]
      The measurement workspace (see SUITE). One command; then build members with
      `cargo build --offline --manifest-path OUT/Cargo.toml -p <member>`.

  gen.py api OUT SHAPE [shape options] [--concord PATH] [--name NAME]
      One standalone API crate (its own `[workspace]`) for code-size studies.

  gen.py OUT N CONCORD_PATH
      Legacy form, byte-identical to the original generator: `api OUT rest --endpoints N`.

  --concord defaults to crates/concord of the workspace containing this script. The suite's
  Cargo.lock is seeded from that workspace's Cargo.lock, so dependency versions are the ones
  Concord itself is tested with; generation fails if cargo cannot settle it offline.

SHAPES (each varies one dimension; everything else is fixed)

  rest      --endpoints N   N/10 scopes under the client, 10 endpoints each. Scope: a path value
                            and an optional header. Endpoint: a path value, two optional and one
                            initialized query value. Client: one sensitive header. (PERF.md 2.)
  deep      --depth D --per-level E [--leaf-only]
                            A chain of D nested scopes; each adds a path value and an optional
                            header. E endpoints (path value + optional query) at every level, or
                            only at the deepest level with --leaf-only.
  shared    --endpoints N --contribs C
                            `rest`, plus inherited contributions: C/2 initialized client header
                            values, C/2 initialized client query values, C literal client
                            headers, C literal client query pairs and C/2 templated headers per
                            scope; every endpoint inherits 3C + C/2 of them (C even).
  override  --levels K --endpoints E
                            A chain of K scopes, each with an optional value that overrides the
                            client's `x-o` header; E endpoints at the deepest scope, each
                            overriding it once more. Produces K+1 nested `if let ... else`.
  optionals --values M --endpoints E
                            E endpoints, each with M optional query values (direct placements).
  refs      --values M      One endpoint with M optional values placed through a
                            `query { "k_i" = v_i; }` block, so every placement is a resolved
                            reference (name-resolution stress; output size equals `optionals`).
  fanout    --placements N  One endpoint whose single value feeds N distinct query keys
                            (`query { "k_i" = v; }`): N distinct placement descriptions for one
                            value in the generated documentation.
  typed     --endpoints N   Typed bodies and outputs: N/10 scopes, 10 endpoints each. The client
                            declares `request text; response text;`; per scope six `-> String`
                            and two `body note: String` + `-> String` endpoints inherit them, and
                            two are in-place exceptions (`-> bytes Vec<u8>`, `bytes data:
                            Vec<u8>` without output). Text and bytes only, so the suite needs no
                            `json` feature.

SUITE (OUT/, one cargo workspace; member names are the package names)

  scale50, scale200, scale1000   `rest` API crates (libraries) with 50 / 200 / 1,000 endpoints
  scale_deep                     `deep`: depth 32, 2 endpoints per level (64 endpoints)
  scale_shared                   `shared`: 200 endpoints, C = 16 (56 inherited contributions each)
  scale_typed                    `typed`: 200 endpoints (160 inheriting a format default, 40
                                 in-place exceptions)
  use5, use100, use1000          binaries calling 5 / 100 / 1,000 of scale1000's endpoints
  fan_a, fan_b                   libraries calling 100 endpoints of scale1000 each (50 in common)
  fan_app                        binary depending on fan_a and fan_b (two-crate fanout)
  suite.json                     parameters, endpoint selections, file hashes, Cargo.lock hash

  Endpoint k of scale1000 is `api.s{k // 10}(..).e{k % 10}(..)`; consumer useK calls endpoints
  k = i * 1000 // K for i in 0..K. Each call sets one optional value, builds the request with
  `prepare()` (its URL length goes into a `black_box`) and boxes the `send()` future as
  `dyn Future` without polling it, so both paths are code-generated and nothing does I/O.
  Profiles are cargo's defaults (nothing is set in OUT/Cargo.toml).
"""

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
DEFAULT_CONCORD = os.path.normpath(os.path.join(HERE, "..", "..", "crates", "concord"))
GENERATOR_VERSION = 4

# Declaration/caller syntax, so that the same workload can be generated for an A/B comparison of
# Concord versions:
#   "formats" (current): body/output formats in place or as `request`/`response` defaults;
#   "via"     (refined V4 up to a915c8d, before formats): `body b: T via R`, `-> T via R`;
#   "alpha"   (V4 alpha 8f7adc2): positional root constructor, no `else inherit`, `-> Text`.
# "refined" is accepted as the former name of "via". Only the `typed` shape has bodies and
# outputs; for the others "formats" and "via" produce the same bytes.
SYNTAX = "formats"
SYNTAXES = ["formats", "via", "alpha", "refined"]


def new_scale(path):
    """The root constructor call for the selected syntax (`Scale` reached through `path`; the
    config struct always through `scale1000::`)."""
    if SYNTAX == "alpha":
        return f'{path}Scale::new(concord::reqwest::Client::new(), "key")'
    return f'{path}Scale::new(concord::reqwest::Client::new(), scale1000::ScaleConfig {{ key: "key".into() }})'


def else_inherit():
    return "" if SYNTAX == "alpha" else " else inherit"


def typed_output(fmt, ty):
    """An output of `ty` with format `fmt` (`text`/`bytes`); `fmt` None inherits the default."""
    marker = {"text": "concord::Text", "bytes": "concord::Binary"}
    if SYNTAX == "alpha":
        return f"-> {marker[fmt or 'text']}"
    if SYNTAX in ("via", "refined"):
        return f"-> {ty} via {marker[fmt or 'text']}"
    return f"-> {fmt} {ty}" if fmt else f"-> {ty}"


def typed_body(fmt, name, ty):
    """A body; `fmt` None inherits the `request` default."""
    marker = {"text": "concord::Text", "bytes": "concord::Binary"}
    if SYNTAX == "alpha":
        return f"body {name}: {marker[fmt or 'text']}"
    if SYNTAX in ("via", "refined"):
        return f"body {name}: {ty} via {marker[fmt or 'text']}"
    return f"{fmt} {name}: {ty}" if fmt else f"body {name}: {ty}"


def write(path, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", newline="\n") as f:
        f.write(text)


def concord_dep(concord):
    return f'concord = {{ path = "{concord}", default-features = false }}'


def manifest(name, deps, standalone, bin_crate=False):
    lines = ["[package]", f'name = "{name}"', 'version = "0.0.0"', 'edition = "2024"',
             "publish = false", ""]
    if bin_crate:
        lines += ["[[bin]]", f'name = "{name}"', 'path = "src/main.rs"', ""]
    lines.append("[dependencies]")
    lines += deps
    if standalone:
        lines += ["", "[workspace]"]
    return "\n".join(lines) + "\n"


# ---------------------------------------------------------------- shapes (declarations only)

def shape_rest(n, extra_client=(), extra_client_stmts=(), extra_scope_stmts=()):
    """The original shape; without extras the output is byte-identical to the first gen.py."""
    params = ['#[sensitive] header("x-api-key") key: String'] + list(extra_client)
    lines = ["#![allow(missing_docs)]", "concord::api! {",
             f"    pub client Scale({', '.join(params)}) {{",
             '        base "https://api.example.com/v1";']
    lines += [f"        {st}" for st in extra_client_stmts]
    scopes = max(1, n // 10)
    for s in range(scopes):
        lines.append(f'        scope s{s}(owner: String, header("x-trace") trace?: String) {{')
        lines.append(f'            path "s{s}/{{owner}}";')
        lines += [f"            {st}" for st in extra_scope_stmts]
        for e in range(n // scopes):
            lines.append(f'            GET e{e}(id: u64, query q?: String, query sort?: String, query limit: u32 = 20) {{ path "items/{{id}}"; }}')
        lines.append("        }")
    lines += ["    }", "}"]
    return "\n".join(lines) + "\n"


def shape_shared(n, c):
    if c % 2:
        sys.exit("shared: --contribs must be even")
    half = c // 2
    extra_client = [f'header("x-cv{i}") cv{i}: String = "cv"' for i in range(half)]
    extra_client += [f'query("cq{i}") cq{i}: u32 = {i}' for i in range(half)]
    stmts = []
    if c:
        stmts.append("headers { " + " ".join(f'"x-cl{i}" = "lit{i}";' for i in range(c)) + " }")
        stmts.append("query { " + " ".join(f'"cl{i}" = "lit{i}";' for i in range(c)) + " }")
    scope_stmts = []
    if half:
        scope_stmts.append("headers { " + " ".join(f'"x-s{i}" = "{{owner}}-{i}";'
                                                    for i in range(half)) + " }")
    return shape_rest(n, extra_client, stmts, scope_stmts)


def shape_deep(depth, per_level, leaf_only):
    lines = ["#![allow(missing_docs)]", "concord::api! {",
             '    pub client Deep(#[sensitive] header("x-api-key") key: String) {',
             '        base "https://api.example.com/v1";']
    for d in range(depth):
        ind = "    " * (d + 2)
        lines.append(f'{ind}scope l{d}(v{d}: u32, header("x-l{d}") h{d}?: String) {{')
        lines.append(f'{ind}    path "l{d}/{{v{d}}}";')
        if not leaf_only or d == depth - 1:
            lines += [f'{ind}    GET e{i}(id: u64, query q?: String) {{ path "items/{{id}}"; }}'
                      for i in range(per_level)]
    for d in reversed(range(depth)):
        lines.append("    " * (d + 2) + "}")
    lines += ["    }", "}"]
    return "\n".join(lines) + "\n"


def shape_override(levels, endpoints):
    lines = ["#![allow(missing_docs)]", "concord::api! {",
             '    pub client Over(header("x-o") o: String = "root") {',
             '        base "https://api.example.com/v1";']
    for d in range(levels):
        ind = "    " * (d + 2)
        lines.append(f"{ind}scope k{d}(o{d}?: String) {{")
        lines.append(f'{ind}    headers {{ override "x-o" = o{d}{else_inherit()}; }}')
    ind = "    " * (levels + 2)
    for e in range(endpoints):
        lines.append(f'{ind}GET e{e}(oe?: String) {{ path "e{e}"; headers {{ override "x-o" = oe{else_inherit()}; }} }}')
    for d in reversed(range(levels)):
        lines.append("    " * (d + 2) + "}")
    lines += ["    }", "}"]
    return "\n".join(lines) + "\n"


def shape_optionals(values, endpoints):
    params = ", ".join(f"query p{i}?: String" for i in range(values))
    lines = ["#![allow(missing_docs)]", "concord::api! {",
             "    pub client Opt {", '        base "https://api.example.com/v1";']
    lines += [f'        GET e{e}({params}) {{ path "e{e}"; }}' for e in range(endpoints)]
    lines += ["    }", "}"]
    return "\n".join(lines) + "\n"


def shape_refs(values):
    params = ", ".join(f"v{i}?: String" for i in range(values))
    block = " ".join(f'"k{i}" = v{i};' for i in range(values))
    return "\n".join(["#![allow(missing_docs)]", "concord::api! {",
                      "    pub client Refs {", '        base "https://api.example.com/v1";',
                      f'        GET e({params}) {{ path "e"; query {{ {block} }} }}',
                      "    }", "}"]) + "\n"


def shape_fanout(placements):
    block = " ".join(f'"k{i}" = v;' for i in range(placements))
    return "\n".join(["#![allow(missing_docs)]", "concord::api! {",
                      "    pub client Fan {", '        base "https://api.example.com/v1";',
                      f'        GET e(v: String) {{ path "e"; query {{ {block} }} }}',
                      "    }", "}"]) + "\n"


def shape_typed(n):
    lines = ["#![allow(missing_docs)]", "concord::api! {",
             '    pub client Typed(#[sensitive] header("x-api-key") key: String) {',
             '        base "https://api.example.com/v1";']
    if SYNTAX == "formats":
        lines += ["        request text;", "        response text;"]
    scopes = max(1, n // 10)
    for s in range(scopes):
        lines.append(f'        scope s{s}(owner: String) {{')
        lines.append(f'            path "s{s}/{{owner}}";')
        for e in range(n // scopes):
            kind = e % 10
            if kind < 6:
                sig = f"GET e{e}(id: u64) {typed_output(None, 'String')}"
            elif kind < 8:
                sig = f"PUT e{e}(id: u64, {typed_body(None, 'note', 'String')}) {typed_output(None, 'String')}"
            elif kind == 8:
                sig = f"GET e{e}(id: u64) {typed_output('bytes', 'Vec<u8>')}"
            else:
                sig = f"PUT e{e}(id: u64, {typed_body('bytes', 'data', 'Vec<u8>')})"
            lines.append(f'            {sig} {{ path "items/{{id}}"; }}')
        lines.append("        }")
    lines += ["    }", "}"]
    return "\n".join(lines) + "\n"


def declaration(args):
    return {
        "rest": lambda: shape_rest(args.endpoints),
        "shared": lambda: shape_shared(args.endpoints, args.contribs),
        "deep": lambda: shape_deep(args.depth, args.per_level, args.leaf_only),
        "override": lambda: shape_override(args.levels, args.endpoints),
        "optionals": lambda: shape_optionals(args.values, args.endpoints),
        "refs": lambda: shape_refs(args.values),
        "fanout": lambda: shape_fanout(args.placements),
        "typed": lambda: shape_typed(args.endpoints),
    }[args.shape]()


# ---------------------------------------------------------------- consumers

CONSUMER_PRELUDE = """use std::future::Future;
use std::pin::Pin;

use scale1000::Scale;

/// A boxed `send()` future: forces code generation of the send path without running it.
pub type Sent = Pin<Box<dyn Future<Output = Result<(), concord::Error>>>>;
"""


def selection(k, total=1000):
    return [i * total // k for i in range(k)]


def calls(indices):
    """One block per endpoint; `api`, `n`, `acc` and `sent` are in scope."""
    out = []
    for k in indices:
        s, e = k // 10, k % 10
        setter = ['.set_q("q")', '.set_sort("asc")', ".set_limit(7)"][k % 3]
        out += ["    {",
                f'        let r = api.s{s}("o").e{e}(n as u64){setter};',
                "        acc += r.clone().prepare().map_or(0, |p| p.request().url().as_str().len());",
                "        sent.push(Box::pin(r.send()));",
                "    }"]
    return out


def consumer_bin(indices):
    body = [CONSUMER_PRELUDE, "fn main() {",
            f'    let api = {new_scale("")};',
            "    let n = std::env::args().count();",
            "    let mut acc = 0usize;",
            "    let mut sent: Vec<Sent> = Vec::new();"]
    body += calls(indices)
    body += ["    std::hint::black_box((acc, sent.len()));", "}"]
    return "\n".join(body) + "\n"


def consumer_lib(indices):
    body = ["//! Calls a fixed subset of scale1000's endpoints (two-crate fanout).", "",
            CONSUMER_PRELUDE,
            "/// Builds and boxes one request per selected endpoint.",
            "pub fn exercise(api: &Scale, n: usize) -> (usize, Vec<Sent>) {",
            "    let mut acc = 0usize;",
            "    let mut sent: Vec<Sent> = Vec::new();"]
    body += calls(indices)
    body += ["    (acc, sent)", "}"]
    return "\n".join(body) + "\n"


def fan_app():
    return ("fn main() {\n"
            f"    let api = {new_scale('scale1000::')};\n"
            "    let n = std::env::args().count();\n"
            "    let a = fan_a::exercise(&api, n);\n"
            "    let b = fan_b::exercise(&api, n);\n"
            "    std::hint::black_box((a.0 + b.0, a.1.len() + b.1.len()));\n"
            "}\n")


# ---------------------------------------------------------------- commands

def sha256(path):
    with open(path, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()


def cmd_api(args):
    if args.shape == "rest" and args.name is None:
        args.name = f"scale{args.endpoints}"
    concord = os.path.abspath(args.concord)
    if args.shape == "rest" and args.legacy:
        # Exactly the original file (no [[bin]], same key order and trailing newline).
        write(os.path.join(args.out, "Cargo.toml"), f"""[package]
name = "{args.name}"
version = "0.0.0"
edition = "2024"
publish = false

[dependencies]
{concord_dep(concord)}

[workspace]
""")
    else:
        write(os.path.join(args.out, "Cargo.toml"),
              manifest(args.name or f"scale_{args.shape}", [concord_dep(concord)], True))
    write(os.path.join(args.out, "src", "lib.rs"), declaration(args))


def cmd_suite(args):
    out = os.path.abspath(args.out)
    concord = os.path.abspath(args.concord)
    members = {}

    def api(name, text, params):
        members[name] = {"kind": "api", **params}
        write(os.path.join(out, name, "Cargo.toml"),
              manifest(name, ["concord = { workspace = true }"], False))
        write(os.path.join(out, name, "src", "lib.rs"), text)

    for n in (50, 200, 1000):
        api(f"scale{n}", shape_rest(n), {"shape": "rest", "endpoints": n})
    api("scale_deep", shape_deep(32, 2, False),
        {"shape": "deep", "depth": 32, "per_level": 2, "endpoints": 64})
    api("scale_shared", shape_shared(200, 16),
        {"shape": "shared", "endpoints": 200, "contribs": 16, "inherited_per_endpoint": 56})
    api("scale_typed", shape_typed(200),
        {"shape": "typed", "endpoints": 200, "inheriting": 160, "in_place": 40})

    deps = ["concord = { workspace = true }", 'scale1000 = { path = "../scale1000" }']
    for k in (5, 100, 1000):
        sel = selection(k)
        members[f"use{k}"] = {"kind": "bin", "api": "scale1000", "endpoints": sel}
        write(os.path.join(out, f"use{k}", "Cargo.toml"), manifest(f"use{k}", deps, False, True))
        write(os.path.join(out, f"use{k}", "src", "main.rs"), consumer_bin(sel))
    fan = {"fan_a": list(range(0, 1000, 10)),                                # 100 endpoints
           "fan_b": list(range(0, 500, 10)) + list(range(505, 1000, 10))}   # 50 shared with fan_a
    for name, sel in fan.items():
        members[name] = {"kind": "lib", "api": "scale1000", "endpoints": sel}
        write(os.path.join(out, name, "Cargo.toml"), manifest(name, deps, False))
        write(os.path.join(out, name, "src", "lib.rs"), consumer_lib(sel))
    members["fan_app"] = {"kind": "bin", "depends": ["fan_a", "fan_b"]}
    write(os.path.join(out, "fan_app", "Cargo.toml"),
          manifest("fan_app", deps + ['fan_a = { path = "../fan_a" }',
                                      'fan_b = { path = "../fan_b" }'], False, True))
    write(os.path.join(out, "fan_app", "src", "main.rs"), fan_app())

    write(os.path.join(out, "Cargo.toml"), "\n".join([
        "# Generated by benches/scale/gen.py suite; do not edit.",
        "[workspace]",
        'resolver = "3"',
        "members = [" + ", ".join(f'"{m}"' for m in members) + "]",
        "",
        "[workspace.dependencies]",
        concord_dep(concord),
    ]) + "\n")

    lock = os.path.join(out, "Cargo.lock")
    seed = os.path.normpath(os.path.join(concord, "..", "..", "Cargo.lock"))
    if not args.no_lock:
        shutil.copyfile(seed, lock)
        subprocess.run(["cargo", "metadata", "--offline", "--format-version", "1",
                        "--manifest-path", os.path.join(out, "Cargo.toml")],
                       check=True, stdout=subprocess.DEVNULL)
    files = sorted(os.path.relpath(os.path.join(d, f), out)
                   for d, dirs, fs in os.walk(out)
                   if not os.path.relpath(d, out).startswith("target")
                   for f in fs if f.endswith((".rs", ".toml")))
    record = {
        "generator": "benches/scale/gen.py",
        "generator_version": GENERATOR_VERSION,
        "syntax": SYNTAX,
        "concord": concord,
        "lock_seed": seed,
        "cargo_lock_sha256": sha256(lock) if os.path.exists(lock) else None,
        "members": members,
        "files_sha256": {f: sha256(os.path.join(out, f)) for f in files},
    }
    write(os.path.join(out, "suite.json"), json.dumps(record, indent=1, sort_keys=True) + "\n")
    print(f"suite: {len(members)} crates in {out}; Cargo.lock sha256 "
          f"{record['cargo_lock_sha256'] or '(not generated)'}")


def main(argv):
    legacy = len(argv) == 3 and argv[0] not in ("api", "suite")
    if legacy:  # OUT N CONCORD_PATH
        argv = ["api", argv[0], "rest", "--endpoints", argv[1], "--concord", argv[2]]
    p = argparse.ArgumentParser(prog="gen.py", description=__doc__.split("\n\n")[0],
                                formatter_class=argparse.RawDescriptionHelpFormatter,
                                epilog=__doc__.split("\n\n", 1)[1])
    sub = p.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("suite", help="the measurement workspace")
    s.add_argument("out")
    s.add_argument("--concord", default=DEFAULT_CONCORD)
    s.add_argument("--syntax", choices=SYNTAXES, default="formats")
    s.add_argument("--no-lock", action="store_true", help="do not seed and settle Cargo.lock")
    a = sub.add_parser("api", help="one standalone API crate")
    a.add_argument("out")
    a.add_argument("shape", choices=["rest", "deep", "shared", "override", "optionals", "refs",
                                          "fanout", "typed"])
    a.add_argument("--concord", default=DEFAULT_CONCORD)
    a.add_argument("--syntax", choices=SYNTAXES, default="formats")
    a.add_argument("--name")
    a.add_argument("--endpoints", type=int, default=10)
    a.add_argument("--depth", type=int, default=8)
    a.add_argument("--per-level", type=int, default=2)
    a.add_argument("--leaf-only", action="store_true")
    a.add_argument("--contribs", type=int, default=16)
    a.add_argument("--levels", type=int, default=8)
    a.add_argument("--values", type=int, default=16)
    a.add_argument("--placements", type=int, default=1000)
    args = p.parse_args(argv)
    global SYNTAX
    SYNTAX = args.syntax if not legacy else "formats"
    if args.cmd == "suite":
        cmd_suite(args)
    else:
        args.legacy = legacy or args.shape == "rest"
        cmd_api(args)


if __name__ == "__main__":
    main(sys.argv[1:])
