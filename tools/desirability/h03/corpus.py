"""H03 idiom corpus generator (the mechanical rule of the H03 measurement brief).

    python3 -I tools/desirability/h03/corpus.py traits   # trait probes at the baseline -> corpus/traits.json
    python3 -I tools/desirability/h03/corpus.py build    # snippets -> corpus/snippets.json, generation.json

Inputs: the SDK declarations of the baseline tree (`examples/<sdk>-sdk`, parsed by
`../decl.py`) and the sealed per-SDK inputs `<BANK>/inputs/<sdk>.json` (client construction,
the canonical value and S1 spelling of each position, each call's reply and the expectation
written from the declaration and the provider documentation it cites). Outputs go only to the
sealed bank `<BANK>/corpus/` (`BANK` = `<WS>/work/sealed/h03-bank`).

Rule (applied mechanically, every exclusion recorded with its reason):
- Calls: per SDK every endpoint in declaration order, capped at the first 8 that take at least
  one caller input (a positional value, a body, or a `set_x` on a value, a scope's included).
- Input positions: every positional value and body of the call's navigation (scopes, then the
  endpoint, declaration order) and every value setter (`set_x`; a parameter-group value's setter
  where the endpoint splices the group, called as the endpoint's own method).
- Spellings, by the declared type's trait impls at the baseline (`traits`: rustc probes compiled
  in an integration test of the SDK at the baseline commit; never what compiles on an arm):
  S1 explicit value (the reference); S2 `Default::default()` (T: Default); S3 `<src>.into()`
  for each natural source with T: From<src> (`&str` literal; an integer literal for a primitive
  integer; the inner type of an SDK newtype, as its S1 value); S4 `"..".parse().unwrap()` and
  `"..".parse()?` (T: FromStr); S5 `".."` and `&String` (T = String; `&HeaderValue` for
  HeaderValue); S6 an integer/float literal (T a primitive number); S7 the explicit forms
  `T::default()`, `"..".parse::<T>()?`, `T::from(src)` (one per S2 / S4 / S3 source); S8 a `let`
  with no annotation whose type comes only from this use (`Default::default()` / `src.into()` /
  `"..".parse().unwrap()`, one per S2 / S3 source / S4). An S7 spelling identical to the
  position's S1 spelling is excluded (S1 covers it).
- One snippet changes one position; every other input uses its S1 spelling; a setter appears
  only in the snippets of its own position. Each snippet names its S1 reference: the S1
  spelling of the value the snippet's spelling produces (the canonical value, or the type's
  default value for S2 / S7 `T::default()` / S8 `Default::default()`).
"""
import argparse
import json
import os
import re
import sys
import urllib.parse

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, ".."))
sys.path.insert(0, HERE)
import decl  # noqa: E402
import harness as H  # noqa: E402

CAP = 8
INTS = ("u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize")
FLOATS = ("f32", "f64")
TRAITS = ("Default", "FromStr", "From<&str>", "From<inner>", "PartialEq")
CORPUS = os.path.join(H.BANK, "corpus")
BASELINE_TREE = os.path.join(H.RUNS, "trees", "baseline-generate")


def load_inputs():
    return {sdk: json.load(open(os.path.join(H.BANK, "inputs", f"{sdk}.json"))) for sdk in H.SDKS}


def kind_of(ty):
    last = re.sub(r"\s+", "", ty).split("::")[-1]
    if last == "String":
        return "string"
    if last == "HeaderValue":
        return "headervalue"
    if last in INTS:
        return "int"
    if last in FLOATS:
        return "float"
    if last == "bool":
        return "bool"
    return "other"


def rust_name(ty, kind, inp):
    if kind == "string":
        return "String"
    if kind in ("int", "float", "bool"):
        return re.sub(r"\s+", "", ty).split("::")[-1]
    return inp["types"][ty]["name"]


def client_of(tree, inp):
    text = open(os.path.join(tree, "examples", H.sdk_dir(inp["sdk"]), inp["decl_file"]), encoding="utf-8").read()
    clients = decl.parse_file(text)
    if len(clients) != 1:
        raise SystemExit(f"{inp['sdk']}: expected one client in {inp['decl_file']}, found {len(clients)}")
    return clients[0]


def walk_calls(client):
    """(call id, [owners: scopes..., endpoint]) in declaration order."""
    out = []

    def visit(owner, chain):
        for ch in owner.get("children", []):
            if ch["kind"] == "endpoint":
                out.append((".".join(o["name"] for o in chain + [ch]), chain + [ch]))
            else:
                visit(ch, chain + [ch])
    visit(client, [])
    return out


def owner_inputs(owner, groups):
    """(positional params, setter params) of one owner; a group splice contributes setters."""
    pos, setters = [], []
    for p in owner.get("params", []):
        if "group" in p:
            for gp in groups.get(p["group"], []):
                setters.append(dict(gp, via_group=p["group"]))
        elif "unparsed" in p:
            raise SystemExit(f"unparsed parameter {p['unparsed']!r} in {owner['name']}")
        elif p["optional"] or p["default"] is not None:
            setters.append(p)
        else:
            pos.append(p)
    return pos, setters


def positions_of(call_id, chain, groups):
    out = []
    for depth, owner in enumerate(chain):
        pos, setters = owner_inputs(owner, groups)
        for p in pos:
            out.append({"id": f"{call_id}/{p['name']}", "param": p["name"], "type": p["type"], "owner": depth,
                        "role": "body" if p["placement"] == "body" else "positional", "placement": p["placement"]})
        for p in setters:
            out.append({"id": f"{call_id}/{p['name']}", "param": p["name"], "type": p["type"], "owner": depth,
                        "role": "group-setter" if p.get("via_group") else "setter", "placement": p["placement"],
                        "group": p.get("via_group")})
    return out


def render_call(chain, groups, exprs, setter):
    """`api.<scope>(..)...<endpoint>(..)` with `exprs` {param: expr}; `.set_<setter>(..)` after the
    owner that declares it."""
    s = "api"
    for owner in chain:
        pos, setters = owner_inputs(owner, groups)
        s += f".{owner['name']}(" + ", ".join(exprs[p["name"]] for p in pos) + ")"
        for p in setters:
            if p["name"] == setter:
                s += f".set_{p['name']}({exprs[p['name']]})"
    return s


# ---------------------------------------------------------------- values and spellings

def s1_of(pos, inp, kind):
    """(S1 spelling of the canonical value, its wire text, the &str literal of the value or None)."""
    v = inp["values"].get(pos["id"])
    if v is None:
        raise SystemExit(f"{inp['sdk']}: no input value for position {pos['id']}")
    lit = v.get("lit")
    ty = re.sub(r"\s+", "", pos["type"]).split("::")[-1]
    if v.get("s1"):
        return v["s1"], v.get("wire", lit), lit
    if kind == "string":
        return f"\"{lit}\".to_string()", lit, lit
    if kind == "int":
        return f"{lit}{ty}", lit, lit
    if kind == "bool":
        return lit, lit, lit
    t = inp["types"][pos["type"]]
    if not t.get("s1"):
        raise SystemExit(f"{inp['sdk']}: position {pos['id']} needs an explicit S1 spelling")
    return t["s1"].replace("{lit}", lit or ""), v.get("wire", lit), lit


def default_of(pos, inp, kind):
    """(S1 spelling of the type's default value, its wire text)."""
    ty = re.sub(r"\s+", "", pos["type"]).split("::")[-1]
    if kind == "string":
        return "String::new()", ""
    if kind == "int":
        return f"0{ty}", "0"
    if kind == "bool":
        return "false", "false"
    d = inp["defaults"].get(pos["type"])
    if not d:
        raise SystemExit(f"{inp['sdk']}: {pos['type']} implements Default but has no default S1 spelling in the inputs")
    return d["s1"], d.get("wire")


def spellings(pos, inp, kind, traits, notes):
    """Every (class, variant, expr, prelude, value) applicable to `pos`, S1 excluded."""
    tr = traits.get(pos["type"], {})
    s1, _, lit = s1_of(pos, inp, kind)
    name = rust_name(pos["type"], kind, inp)
    out = []
    sources = []  # (label, source expr)
    if tr.get("From<&str>") and lit is not None:
        sources.append(("&str", f"\"{lit}\""))
    elif tr.get("From<&str>"):
        notes.append(f"{pos['id']}: From<&str> holds but the position has no string literal (no S3 &str)")
    if kind == "int":
        sources.append(("integer", lit))
    inner = inp["types"].get(pos["type"], {}).get("newtype_inner") if kind == "other" else None
    if inner and tr.get("From<inner>"):
        ik = kind_of(inner)
        src = f"\"{lit}\".to_string()" if ik == "string" else f"{lit}{inner}"
        sources.append((f"inner {inner}", src))
    if tr.get("Default"):
        out.append(("S2", "Default::default()", "Default::default()", [], "default"))
    for label, src in sources:
        out.append(("S3", f"{label} .into()", f"{src}.into()", [], "canonical"))
    if tr.get("FromStr") and lit is not None:
        out.append(("S4", "parse().unwrap()", f"\"{lit}\".parse().unwrap()", [], "canonical"))
        out.append(("S4", "parse()?", f"\"{lit}\".parse()?", [], "canonical"))
    elif tr.get("FromStr"):
        notes.append(f"{pos['id']}: FromStr holds but the position has no string form (no S4)")
    if kind == "string":
        out.append(("S5", "&str literal", f"\"{lit}\"", [], "canonical"))
        out.append(("S5", "&String", "&owned", [f"let owned = \"{lit}\".to_string();"], "canonical"))
    if kind == "headervalue":
        out.append(("S5", "&HeaderValue", "&header", [f"let header = HeaderValue::from_static(\"{lit}\");"], "canonical"))
    if kind in ("int", "float"):
        out.append(("S6", "literal", lit, [], "canonical"))
    if tr.get("Default"):
        out.append(("S7", "T::default()", f"{name}::default()", [], "default"))
    if tr.get("FromStr") and lit is not None:
        out.append(("S7", "parse::<T>()?", f"\"{lit}\".parse::<{name}>()?", [], "canonical"))
    for label, src in sources:
        out.append(("S7", f"T::from({label})", f"{name}::from({src})", [], "canonical"))
    if tr.get("Default"):
        out.append(("S8", "let = Default::default()", "v", ["let v = Default::default();"], "default"))
    for label, src in sources:
        out.append(("S8", f"let = {label} .into()", "v", [f"let v = {src}.into();"], "canonical"))
    if tr.get("FromStr") and lit is not None:
        out.append(("S8", "let = parse().unwrap()", "v", [f"let v = \"{lit}\".parse().unwrap();"], "canonical"))
    kept = []
    for cls, variant, expr, prelude, value in out:
        if not prelude and expr == s1 and value == "canonical":
            notes.append(f"{pos['id']}: {cls} {variant} excluded: identical to the S1 spelling `{name}` reference (S1 covers it)")
            continue
        if not prelude and value == "default" and expr == default_of(pos, inp, kind)[0]:
            notes.append(f"{pos['id']}: {cls} {variant} excluded: identical to the S1 spelling of the default value (S1 covers it)")
            continue
        kept.append((cls, variant, expr, prelude, value))
    return kept


def expected_request(call_in, pos_by_param, wire, setter):
    """The request an S1 reference should send, from the per-call expectation (written from the
    declaration and the provider documentation): method, target, header subset, body."""
    e = call_in["expect"]
    path = e["path"]
    for name, w in wire.items():
        path = path.replace("{" + name + "}", urllib.parse.quote(str(w), safe="-._~"))
    query = []
    for key, param in e.get("query", []):
        p = pos_by_param.get(param)
        if p is None:
            continue
        if p["role"] in ("positional", "body") or param == setter:
            query.append(f"{urllib.parse.quote(key, safe='-._~')}={urllib.parse.quote(str(wire[param]), safe='-._~')}")
    target = path + ("?" + "&".join(query) if query else "")
    return {"method": e["method"], "target": target, "headers": e.get("headers", {}),
            "present": e.get("present", []), "absent": e.get("absent", []), "body": e.get("body"), "doc": e.get("doc")}


def build(tree, inputs, traits):
    snippets, refs, record = [], {}, {"cap": CAP, "sdks": {}}
    for sdk in H.SDKS:
        inp = inputs[sdk]
        client = client_of(tree, inp)
        groups = {g: ps for g, ps in client.get("stmts", {}).get("groups", {}).items()}
        calls = walk_calls(client)
        rec = {"role": inp["role"], "endpoints_in_declaration": [c for c, _ in calls], "calls": [], "skipped_no_input": [],
               "beyond_cap": [], "positions": [], "notes": [], "exclusions": []}
        chosen = []
        for cid, chain in calls:
            ps = positions_of(cid, chain, groups)
            if not ps:
                rec["skipped_no_input"].append(cid)
            elif len(chosen) < CAP:
                chosen.append((cid, chain, ps))
            else:
                rec["beyond_cap"].append(cid)
        tr = traits.get(sdk, {})
        for cid, chain, ps in chosen:
            call_in = inp["calls"][cid]
            rec["calls"].append(cid)
            pos_by_param = {p["param"]: p for p in ps}
            kinds = {p["param"]: kind_of(p["type"]) for p in ps}
            canon = {p["param"]: s1_of(p, inp, kinds[p["param"]]) for p in ps}

            def ref_for(setter, default_param):
                exprs = {k: v[0] for k, v in canon.items()}
                wire = {k: v[1] for k, v in canon.items()}
                label = f"{cid}|base" if setter is None else f"{cid}|set:{setter}"
                if default_param is not None:
                    p = pos_by_param[default_param]
                    exprs[default_param], wire[default_param] = default_of(p, inp, kinds[default_param])
                    label += f"|default:{default_param}"
                if label not in refs:
                    reply = call_in.get("replies", {}).get(label.split("|default:")[0], call_in["reply"])
                    refs[label] = {"id": label, "sdk": sdk, "call": cid, "setter": setter, "default": default_param,
                                   "code": {"prelude": [], "call": render_call(chain, groups, exprs, setter)},
                                   "reply": reply, "expect": expected_request(call_in, pos_by_param, wire, setter)}
                return label

            for p in ps:
                kind = kinds[p["param"]]
                if kind == "other" and p["type"] not in inp["types"]:
                    raise SystemExit(f"{sdk}: no type facts for {p['type']} ({p['id']})")
                setter = p["param"] if p["role"] in ("setter", "group-setter") else None
                notes = []
                vs = spellings(p, inp, kind, tr, notes)
                rec["exclusions"] += [n for n in notes if "excluded" in n]
                rec["notes"] += [n for n in notes if "excluded" not in n]
                base_ref = ref_for(setter, None)
                classes = ["S1"] + sorted({v[0] for v in vs})
                rec["positions"].append({"id": p["id"], "type": p["type"], "kind": kind, "role": p["role"],
                                         "group": p.get("group"), "traits": tr.get(p["type"], {}),
                                         "classes": classes, "variants": len(vs)})
                for cls, variant, expr, prelude, value in vs:
                    same_value = value == "canonical" or default_of(p, inp, kind)[0] == canon[p["param"]][0]
                    ref = base_ref if same_value else ref_for(setter, p["param"])
                    exprs = {k: v[0] for k, v in canon.items()}
                    exprs[p["param"]] = expr
                    snippets.append({"sdk": sdk, "role": inp["role"], "call": cid, "position": p["id"], "type": p["type"],
                                     "kind": kind, "class": cls, "variant": variant, "ref": ref,
                                     "code": {"prelude": prelude, "call": render_call(chain, groups, exprs, setter)}})
        record["sdks"][sdk] = rec
    # S1 snippets: one per reference (deduplicated by code)
    s1 = []
    for rid, r in refs.items():
        s1.append({"sdk": r["sdk"], "role": inputs[r["sdk"]]["role"], "call": r["call"],
                   "position": (f"{r['call']}/{r['setter']}" if r["setter"] else None), "default": r["default"],
                   "type": None, "kind": None, "class": "S1", "variant": "default value" if r["default"] else "canonical",
                   "ref": rid, "code": r["code"]})
    allsn = s1 + snippets
    for i, s in enumerate(allsn):
        s["id"] = f"h03-{i:04d}"
    counts = {}
    for s in allsn:
        c = counts.setdefault(s["sdk"], {})
        c[s["class"]] = c.get(s["class"], 0) + 1
    record["counts"] = counts
    record["total"] = len(allsn)
    return allsn, refs, record


# ---------------------------------------------------------------- trait probes

def type_names(tree, inputs):
    """{sdk: {declared type text: (caller name, kind, newtype inner)}} over the positions of the
    chosen calls."""
    out = {}
    for sdk in H.SDKS:
        inp = inputs[sdk]
        client = client_of(tree, inp)
        groups = client.get("stmts", {}).get("groups", {})
        n = 0
        for cid, chain in walk_calls(client):
            ps = positions_of(cid, chain, groups)
            if not ps:
                continue
            n += 1
            if n > CAP:
                break
            for p in ps:
                k = kind_of(p["type"])
                inner = inp["types"].get(p["type"], {}).get("newtype_inner") if k == "other" else None
                out.setdefault(sdk, {})[p["type"]] = (rust_name(p["type"], k, inp), k, inner)
    return out


def trait_probe_blocks(names):
    blocks = []
    for ty, (name, kind, inner) in sorted(names.items()):
        for tr in TRAITS:
            if tr == "From<inner>" and not inner:
                continue
            bound = {"Default": "::core::default::Default", "FromStr": "::core::str::FromStr",
                     "From<&str>": "::core::convert::From<&'static str>",
                     "From<inner>": f"::core::convert::From<{inner}>", "PartialEq": "::core::cmp::PartialEq"}[tr]
            tag = re.sub(r"\W", "_", f"p_{name}_{tr}")
            blocks.append((ty, tr, tag, f"fn __h03_{tag}() {{\n    fn need<X: ?Sized + {bound}>() {{}}\n    need::<{name}>();\n}}"))
    return blocks


def cmd_traits(a):
    inputs = load_inputs()
    log = []
    prov = H.make_tree("baseline", BASELINE_TREE)
    names = type_names(BASELINE_TREE, inputs)
    batch = H.Batch(BASELINE_TREE, "baseline", H.target_dir("baseline"), inputs, "H03 trait probes at the baseline")
    index = {}
    for sdk in H.SDKS:
        for ty, tr, tag, code in trait_probe_blocks(names.get(sdk, {})):
            batch.add_block(sdk, f"{sdk}:{tag}", code)
            index[f"{sdk}:{tag}"] = (sdk, ty, tr)
    H.chronology("trait-probes-compile-start", arm="baseline", commit=prov["commit"], non_s1=0, snippets=0)
    failed, rendered, exes, fresh = batch.compile(log, sandbox=True)
    out = {}
    for t, (sdk, ty, tr) in index.items():
        out.setdefault(sdk, {}).setdefault(ty, {})[tr] = t not in failed
    for sdk, tys in names.items():
        for ty, (name, kind, inner) in tys.items():
            out.setdefault(sdk, {}).setdefault(ty, {})["From<inner>"] = out[sdk][ty].get("From<inner>") if inner else None
    os.makedirs(CORPUS, exist_ok=True)
    unattributed = {t: m for t, m in failed.items() if m.startswith("unattributed")}
    if unattributed:
        raise SystemExit(f"trait probes: unattributed compile errors: {list(unattributed.items())[:3]}")
    json.dump({"provenance": prov, "traits": out, "probe_errors": {t: failed[t] for t in failed}},
              open(os.path.join(CORPUS, "traits.json"), "w"), indent=1, sort_keys=True)
    json.dump(log, open(os.path.join(H.RUNS, "logs", "traits.log.json"), "w"), indent=1)
    print(json.dumps({"traits": {s: {t: [k for k, v in d.items() if v] for t, d in tys.items()} for s, tys in out.items()}}, indent=1))


def cmd_build(a):
    inputs = load_inputs()
    tj = json.load(open(os.path.join(CORPUS, "traits.json")))
    tree = BASELINE_TREE
    if not os.path.isdir(tree):
        H.make_tree("baseline", tree)
    prov, problems = H.check_tree(tree, "baseline")
    if problems:
        raise SystemExit(f"baseline generation tree: {problems}")
    snippets, refs, record = build(tree, inputs, tj["traits"])
    record["provenance"] = prov
    json.dump(snippets, open(os.path.join(CORPUS, "snippets.json"), "w"), indent=1, sort_keys=True)
    json.dump(refs, open(os.path.join(CORPUS, "refs.json"), "w"), indent=1, sort_keys=True)
    json.dump(record, open(os.path.join(CORPUS, "generation.json"), "w"), indent=1, sort_keys=True)
    print(json.dumps({"total": record["total"], "counts": record["counts"],
                      "calls": {s: r["calls"] for s, r in record["sdks"].items()}}, indent=1))


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("traits")
    sub.add_parser("build")
    a = ap.parse_args()
    os.makedirs(os.path.join(H.RUNS, "logs"), exist_ok=True)
    {"traits": cmd_traits, "build": cmd_build}[a.cmd](a)


if __name__ == "__main__":
    main()
