"""I3 question generator: per SDK, up to 6 stratified endpoints and three items per endpoint.

    python3 -I tools/desirability/probes/generate.py --seed 1 --out tools/desirability/probes/bank
    python3 -I tools/desirability/probes/generate.py --seed 9001 --role validation --holdout-of 1 --out DIR
    python3 -I tools/desirability/probes/generate.py --seed 4242 --role final --holdout-of 1 9001 --out DIR

Writes `<out>/index.json` (eligibility, strata coverage, counts) and `<out>/<sdk>.json` (picks and
items). Deterministic given the seed and the repository's declarations. A held-out bank
(`--holdout-of S1 S2 ..`, the banks generated before it, in order) first takes endpoints none of
those banks picked ("fresh"); when an SDK has none left it reuses an endpoint with a different
instantiation (values, optional values set or left unset), flagged `reused_endpoint`. Every item is
labelled with its observation boundary: `prepared` (no send policy: the request `.prepare()`
builds is what `send()` sends) or `operation` (a client send policy: the question is about what the
public `send()` puts on the wire). The eligibility and value rules are in README.md.
"""
import argparse
import json
import os
import random
import re
import sys
from collections import Counter

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import common as C  # noqa: E402

PICKS_PER_SDK = 6
STRATA = ("scope_depth2", "query", "header", "body", "override_remove", "optional_set",
          "optional_unset", "initialized", "base_path", "scope_path", "format_exception")
WORDS = ("alpha", "bravo", "cedar", "delta", "ember", "fjord", "grove", "harbor", "indigo",
         "juniper", "kelp", "lumen", "maple", "nectar", "onyx", "pixel", "quartz", "raven",
         "sable", "tundra")
PCT_VALUES = ("a b/c", "x&y=z", "50% off")
STAGING = "https://staging.example.test"
RUST_KEYWORDS = {"as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn",
                 "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
                 "return", "self", "static", "struct", "super", "trait", "true", "type", "unsafe",
                 "use", "where", "while", "async", "await", "dyn", "abstract", "become", "box", "do",
                 "final", "macro", "override", "priv", "typeof", "unsized", "virtual", "yield", "try",
                 "gen"}
# v2: call questions state the R3 answer contract (`CALL_CONTRACT`); picks are chosen as in v1.
# pack.py words the call questions of v1 banks the same way, from their stored `intent`.
GENERATOR_VERSION = 2
OPTIONS = {"sdk_constructors": False}  # --sdk-constructors (opt-in; see README "Eligibility")


def ident(name):
    return "r#" + name if name in RUST_KEYWORDS else name


# ---------------------------------------------------------------- value kinds

def value_kind(sdk, type_text, each=False):
    """The literal kind of a declared value type, or None when it is not readable from a literal."""
    t = C.parse_type(sdk.resolve_alias(type_text))
    s = C.type_str(t)
    if t[0] == "ref" and C.type_str(t[2]) == "str":
        return "str"
    if t[0] != "path":
        return None
    segs = [x for x in t[1] if x]
    last = segs[-1]
    if len(segs) > 1 and segs[0] not in ("std", "core", "alloc"):
        return None
    if last == "String" and not t[2]:
        return "string"
    if last in C.PRIM_INT and len(segs) == 1:
        return "int:" + last
    if last in C.PRIM_FLOAT and len(segs) == 1:
        return "float:" + last
    if last == "bool" and len(segs) == 1:
        return "bool"
    if last in C.NONZERO and not t[2]:
        return "nonzero:" + last
    if last == "NonZero" and len(t[2]) == 1 and C.type_str(t[2][0]) in C.PRIM_INT:
        return "nonzerog:" + C.type_str(t[2][0])
    if each and last == "Vec" and len(t[2]) == 1 and value_kind(sdk, C.type_str(t[2][0])) == "string":
        return "vec_string"
    del s
    return None


def body_kind(sdk, type_text):
    """`string`, `json_value` or `default:<Type>` for an eligible body type, else None."""
    resolved = sdk.resolve_alias(type_text)
    t = C.parse_type(resolved)
    if value_kind(sdk, resolved) == "string":
        return "string"
    if t[0] == "path" and t[1][-1] == "Value" and (len(t[1]) == 1 or t[1][-2] == "serde_json"):
        if C.has_dep(sdk.dir, "serde_json"):
            return "json_value"
        return None
    if t[0] == "path" and t[1][-1] in sdk.defaults and sdk.find(t[1][-1], ("struct", "enum")):
        return "default:" + C.type_str(C.parse_type(type_text))
    return None


def literal(kind, rng, pct=False, config=False):
    """Rust source for a value of `kind`. Arguments take `impl IntoValue<T>` (exactly T, or &str
    for String); config fields take exactly T (`.into()` for a string literal)."""
    if kind in ("string", "str"):
        v = rng.choice(PCT_VALUES) if pct else rng.choice(WORDS)
        lit = C.rust_str(v)
        return (lit + ".into()" if config and kind == "string" else lit), v
    if kind.startswith("int:"):
        n = rng.randint(2, 9) if kind.endswith(("u8", "i8")) else rng.randint(2, 30)
        return str(n), n
    if kind.startswith("float:"):
        f = rng.choice((0.5, 1.5, 2.25, 3.75))
        return repr(f), f
    if kind == "bool":
        b = rng.random() < 0.5
        return ("true" if b else "false"), b
    if kind.startswith("nonzero:"):
        n = rng.randint(2, 9)
        return f"std::num::{kind.split(':')[1]}::new({n}).unwrap()", n
    if kind.startswith("nonzerog:"):
        n = rng.randint(2, 9)
        return f"std::num::NonZero::new({n}{kind.split(':')[1]}).unwrap()", n
    if kind == "vec_string":
        vs = rng.sample(WORDS, 2)
        return "vec![" + ", ".join(C.rust_str(v) + ".to_string()" for v in vs) + "]", vs
    raise ValueError(kind)


def body_literal(kind, rng):
    if kind == "string":
        v = rng.choice(WORDS)
        return C.rust_str(v), v
    if kind == "json_value":
        v = rng.choice(WORDS)
        return 'serde_json::json!({"name": ' + C.rust_str(v) + "})", {"name": v}
    ty = kind.split(":", 1)[1]
    t = C.parse_type(ty)
    if t[0] == "path" and t[2]:
        return f"<{ty}>::default()", None
    return f"{ty}::default()", None


# ---------------------------------------------------------------- clients

def sdk_constructor(sdk, type_text):
    """A public way to build a value of an SDK type from no input: an associated const of the type
    (`ApiVersion::PINNED`), else a public associated fn with no arguments returning it
    (`Policy::public()`). None when the type has neither."""
    t = C.parse_type(sdk.resolve_alias(type_text))
    if t[0] != "path" or t[2] or not sdk.find(t[1][-1], ("struct", "enum")):
        return None
    name = t[1][-1]
    impls = [im for im in sdk.impls
             if re.match(r"impl\s+" + re.escape(name) + r"\b", im["header"]) and " for " not in im["header"]]
    for im in impls:
        m = re.search(r"(?m)^\s*pub\s+const\s+([A-Z][A-Z0-9_]*)\s*:\s*(?:Self|" + re.escape(name) + r")\s*=",
                      C.strip_strings_and_comments(im["text"]))
        if m:
            return f"{name}::{m.group(1)}"
    for im in impls:
        m = re.search(r"(?m)^\s*pub\s+(?:const\s+)?fn\s+(\w+)\s*\(\s*\)\s*->\s*(?:Self|" + re.escape(name) + r")\s*\{",
                      C.strip_strings_and_comments(im["text"]))
        if m:
            return f"{name}::{m.group(1)}()"
    return None


def client_ctor(sdk, client, canary_counter):
    """(ctor source, canaries, config fields, reason, notes). `reason` is None when constructible."""
    name = client["name"]
    fields, canaries, notes = [], [], []
    rng = random.Random(f"ctor:{sdk.short}:{name}")
    for p in client["params"]:
        if "group" in p or p.get("optional") or p.get("default") is not None:
            continue
        kind = value_kind(sdk, p["type"])
        resolved = C.type_str(C.parse_type(sdk.resolve_alias(p["type"])))
        is_hv = resolved.split("::")[-1] == "HeaderValue"
        if p.get("sensitive") and (kind == "string" or is_hv):
            canary_counter[0] += 1
            cv = f"CANARY{sdk.tag}{canary_counter[0]}Z"
            canaries.append(cv)
            expr = (C.rust_str(cv) + ".into()") if kind == "string" else f"reqwest::header::HeaderValue::from_static({C.rust_str(cv)})"
        elif is_hv:
            expr = f"reqwest::header::HeaderValue::from_static({C.rust_str(rng.choice(WORDS))})"
        elif kind and kind != "vec_string":
            expr = literal(kind, rng, config=True)[0]
        elif sdk.has_default(p["type"]):
            expr = "Default::default()"
            if p.get("sensitive"):
                notes.append(f"sensitive `{p['name']}: {p['type']}` built with Default::default() (no canary)")
        elif OPTIONS["sdk_constructors"] and sdk_constructor(sdk, p["type"]):
            expr = sdk_constructor(sdk, p["type"])
            notes.append(f"`{p['name']}: {p['type']}` built with the SDK's public constructor `{expr}`")
        else:
            return None, [], [], f"required client value of type `{p['type']}` has no literal form and no Default", notes
        fields.append((ident(p["name"]), expr))
    if fields:
        body = "".join(f"\n    {f}: {e}," for f, e in fields)
        ctor = f"{name}::new(reqwest::Client::new(), {name}Config {{{body}\n}})"
    else:
        ctor = f"{name}::new(reqwest::Client::new())"
    return ctor, canaries, [f for f, _ in fields], None, notes


# ---------------------------------------------------------------- endpoints

def slots_of(client, ancestors, ep):
    """Every value on the chain except the client's required ones (those are config fields):
    [{level, owner, owner_kind, param..}]; level 0 = client, 1.. = scopes, last = endpoint."""
    owners = list(ancestors[1:]) + [ep]
    out = []
    for p in client["params"]:
        if "group" in p:
            continue
        if p.get("optional") or p.get("default") is not None:
            out.append(dict(p, level=0, owner=client["name"], owner_kind="client"))
    groups = client["stmts"].get("groups", {})
    for lvl, o in enumerate(owners, start=1):
        kind = "endpoint" if o is ep else "scope"
        for p in o["params"]:
            if "group" in p:
                for gp in groups.get(p["group"], []):
                    out.append(dict(gp, level=lvl, owner=p["group"], owner_kind="group"))
                continue
            out.append(dict(p, level=lvl, owner=o["name"], owner_kind=kind))
    return out


def make_pick(sdk, client, cfile, ancestors, ep, ctor, base_canaries, seed, canary_base):
    """A candidate pick for one endpoint, or (None, reason)."""
    scopes = list(ancestors[1:])
    key = "/".join([client["name"]] + [s["name"] for s in scopes] + [ep["name"]])
    rng = random.Random(f"{seed}:{sdk.short}:{key}")
    slots = slots_of(client, ancestors, ep)
    canaries = list(base_canaries)
    ncan = [canary_base]

    def canary():
        ncan[0] += 1
        v = f"CANARY{sdk.tag}{ncan[0]}Z"
        canaries.append(v)
        return v

    # eligibility of required values
    for s in slots:
        s["required"] = not s.get("optional") and s.get("default") is None
        if s["placement"] == "body":
            s["kind"] = body_kind(sdk, s["type"])
        else:
            s["kind"] = value_kind(sdk, s["type"], s.get("each"))
            if s["owner_kind"] == "client" and s["kind"] is None and \
                    C.type_str(C.parse_type(sdk.resolve_alias(s["type"]))).split("::")[-1] == "HeaderValue":
                s["kind"] = "header_value"
        if s["required"] and s["kind"] is None:
            what = "body" if s["placement"] == "body" else "value"
            return None, f"required {s['owner_kind']} {what} of type `{s['type']}` not readable"
    # optional values: choose which to set
    readable_opt = [s for s in slots if s.get("optional") and s["kind"] is not None
                    and not (s["placement"] == "query" and s.get("key") == "..")]
    absent_unreadable = any(s.get("optional") and s.get("default") is None and s["kind"] is None for s in slots)
    rng.shuffle(readable_opt)
    n = len(readable_opt)
    if n >= 2:
        k = min(rng.randint(1, n - 1), 3)
    elif n == 1:
        k = 1 if absent_unreadable or rng.random() < 0.5 else 0
    else:
        k = 0
    to_set = {id(s) for s in readable_opt[:k]}
    # percent-encoding: at most one sent string value
    sent_strings = [s for s in slots if s["kind"] in ("string", "str") and s["placement"] in ("path", "query", "header")
                    and (s["required"] or id(s) in to_set) and not s.get("sensitive")]
    pct_slot = id(rng.choice(sent_strings)) if sent_strings and rng.random() < 0.4 else None
    for s in slots:
        if s["required"] or id(s) in to_set:
            s["state"] = "required" if s["required"] else "set"
            if s.get("sensitive") and s["kind"] in ("string", "str", "header_value"):
                cv = canary()
                s["literal"] = C.rust_str(cv) if s["kind"] != "header_value" else \
                    f"reqwest::header::HeaderValue::from_static({C.rust_str(cv)})"
                s["value"] = "<secret>"
            elif s["kind"] == "header_value":
                w = rng.choice(WORDS)
                s["literal"], s["value"] = f"reqwest::header::HeaderValue::from_static({C.rust_str(w)})", w
            elif s["placement"] == "body":
                s["literal"], s["value"] = body_literal(s["kind"], rng)
            else:
                s["literal"], s["value"] = literal(s["kind"], rng, pct=(id(s) == pct_slot))
        elif s.get("default") is not None:
            s["state"] = "initialized"
        else:
            s["state"] = "unset"
    # rust source
    def setters(level):
        return "".join(f".set_{s['name']}({s['literal']})" for s in slots if s["level"] == level and s["state"] == "set")

    def args(level):
        return ", ".join(s["literal"] for s in slots if s["level"] == level and s["state"] == "required")

    client_expr = ctor + setters(0)
    call = "api"
    for lvl, sc in enumerate(scopes, start=1):
        call += f".{ident(sc['name'])}({args(lvl)})" + setters(lvl)
    lvl = len(scopes) + 1
    call += f".{ident(ep['name'])}({args(lvl)})" + setters(lvl)
    pick = {
        "key": key, "client": client["name"], "client_file": sdk.rel(cfile),
        "scopes": [s["name"] for s in scopes], "method": ep["method"], "endpoint": ep["name"],
        "output": ep.get("output"), "docs": ep.get("docs", []),
        "values": [{k: s.get(k) for k in ("level", "owner", "owner_kind", "name", "type", "placement",
                                          "key", "each", "sensitive", "optional", "default", "kind",
                                          "state", "literal", "value")} for s in slots],
        "canaries": canaries, "pct": pct_slot is not None,
        "rust": {"client": client_expr, "call": call},
    }
    pick["strata"] = strata_of(client, scopes, ep, slots)
    return pick, None


def strata_of(client, scopes, ep, slots):
    st = set()
    if len(scopes) >= 2:
        st.add("scope_depth2")
    sent = [s for s in slots if s["state"] in ("required", "set", "initialized")]
    blocks_q = client["stmts"]["query"] + [e for o in scopes + [ep] for e in o["stmts"]["query"]]
    if any(s["placement"] == "query" for s in sent) or any(not e.startswith("remove") for e in blocks_q):
        st.add("query")
    lower_headers = [e for o in scopes + [ep] for e in o["stmts"]["headers"]]
    if any(s["placement"] == "header" and (s["level"] > 0 or s["state"] == "set") for s in sent) or lower_headers:
        st.add("header")
    if any(s["placement"] == "body" for s in sent):
        st.add("body")
    blocks = [e for o in scopes + [ep] for e in o["stmts"]["query"] + o["stmts"]["headers"]]
    if any(re.match(r"(override|remove)\b", e) for e in blocks):
        st.add("override_remove")
    if any(s["state"] == "set" for s in slots):
        st.add("optional_set")
    if any(s["state"] == "unset" for s in slots):
        st.add("optional_unset")
    if any(s["state"] == "initialized" for s in slots):
        st.add("initialized")
    base = client["stmts"]["base"] or ""
    if re.sub(r"^\w+://[^/]*", "", base).strip("/") or client["stmts"]["path"]:
        st.add("base_path")
    if any(o["stmts"]["path"] for o in scopes):
        st.add("scope_path")
    if ep["stmts"]["request"] or ep["stmts"]["response"]:
        st.add("format_exception")
    return sorted(st)


def select(cands, rng, k=PICKS_PER_SDK):
    """Greedy stratified choice: each step takes the candidate covering the most strata not yet
    covered (ties: seeded order); once nothing new is covered, prefer a new (client, first scope)."""
    order = list(cands)
    rng.shuffle(order)
    chosen, covered, areas = [], set(), set()
    while order and len(chosen) < k:
        best = max(order, key=lambda c: (len(set(c["strata"]) - covered),
                                         (c["client"], (c["scopes"] or [""])[0]) not in areas,
                                         -order.index(c)))
        order.remove(best)
        chosen.append(best)
        covered |= set(best["strata"])
        areas.add((best["client"], (best["scopes"] or [""])[0]))
    return chosen


# ---------------------------------------------------------------- items

def location(pick, multi_client):
    parts = [f"`{pick['method']} {pick['endpoint']}`"]
    if pick["scopes"]:
        parts.append("in " + " › ".join(f"`scope {s}`" for s in pick["scopes"]))
    else:
        parts.append("directly on the client")
    if multi_client:
        parts.append(f"of client `{pick['client']}`")
    return " ".join(parts)


def summary(pick):
    text = " ".join(d.strip() for d in pick["docs"]).strip()
    if text:
        text = re.split(r"(?<=[.!?])\s", text)[0].rstrip(".")
        text = re.sub(r"\[`?([^\]`]+)`?\]\([^)]*\)", r"`\1`", text)
        text = re.sub(r"\[(`[^\]]+`)\]", r"\1", text)
        if len(text) > 140:
            text = text[:137].rstrip() + "..."
        return text
    return pick["endpoint"].replace("_", " ").capitalize()


def intent(pick, multi_client):
    vals = []
    for v in pick["values"]:
        if v["state"] in ("required", "set"):
            where = "" if v["owner_kind"] in ("endpoint", "group") else f" ({v['owner_kind']} `{v['owner']}`)"
            vals.append(f"`{v['name']}`{where} = `{v['literal']}`")
    s = f"{summary(pick)}: call {location(pick, multi_client)}"
    if vals:
        s += " with " + ", ".join(vals)
    return s + ". Leave every other optional value as it is"


def base_url_question(sdk, client, pick):
    base = client["stmts"]["base"] or ""
    cpath = client["stmts"]["path"] or ""
    bpath = re.sub(r"^\w+://[^/]*", "", base).strip("/")
    if "{" in base + cpath or (not bpath and not cpath):
        return None
    chain_has_subdomain = client["stmts"]["subdomain"] or any(
        o["stmts"]["subdomain"] for o in owners_on_chain(client, pick))
    if chain_has_subdomain:
        return None
    keep = "/" + "/".join(x for x in (bpath, cpath.strip("/")) if x)
    return {"staging": STAGING, "keep": keep,
            "candidates": [STAGING, STAGING + "/", STAGING + keep, STAGING + keep + "/"]}


def owners_on_chain(client, pick):
    out, cur = [], client
    for s in pick["scopes"]:
        cur = next(c for c in cur["children"] if c["kind"] == "scope" and c["name"] == s)
        out.append(cur)
    return out


def wire_question(pick):
    if pick["boundary"] == "operation":
        ask = ("What request does `call.send().await` put on the wire (the request the server receives)? "
               "Give `method`, `url`, `headers` (only the ones the SDK and Concord set, not the HTTP client's "
               "own; secret values as `<secret>`), `body`, `content_type` and `accept`.")
    else:
        ask = ("What request does `call.prepare()` build? Give `method`, `url`, `headers` (only the "
               "ones Concord sets; secret values as `<secret>`), `body`, `content_type` and `accept`.")
    return ask + "\n\n```rust\nlet api = " + pick["rust"]["client"] + ";\nlet call = " + pick["rust"]["call"] + ";\n```"


def instance(p):
    return p["rust"]["client"] + "\n" + p["rust"]["call"]


def bank_picks(sdk, seed, prior):
    """(chosen picks, eligible candidates) for one SDK: `prior` are the seeds of the banks
    generated before this one (each itself held out from the ones before it)."""
    used_keys, used_inst = set(), set()
    for i, ps in enumerate(prior):
        chosen, _ = bank_picks(sdk, ps, prior[:i]) if i else bank_picks(sdk, ps, [])
        used_keys |= {p["key"] for p in chosen}
        used_inst |= {instance(p) for p in chosen}
    cands, _, _ = sdk_candidates(sdk, seed)
    rng = random.Random(f"select:{seed}:{sdk.short}")
    fresh = [c for c in cands if c["key"] not in used_keys]
    chosen = select(fresh, rng)
    if prior and len(chosen) < PICKS_PER_SDK:
        reuse = [dict(c, reused_endpoint=True) for c in cands
                 if c["key"] in used_keys and instance(c) not in used_inst]
        chosen += select(reuse, random.Random(f"reuse:{seed}:{sdk.short}"), PICKS_PER_SDK - len(chosen))
    return chosen, cands


def result_question(pick, multi_client, bq):
    q = (f"For the endpoint {location(pick, multi_client)}: (a) what type does `.send().await` return "
         "(`send_returns`)? (b) what is the error type in that result (`error_type`)?")
    if bq:
        q += (f" (c) The same API also runs at `{bq['staging']}`. Which exact string do you pass to "
              f"`api.with_base_url(..)` so that requests keep the `{bq['keep']}` prefix, i.e. go to "
              f"`{bq['staging']}{bq['keep']}/...` (`base_url`)?")
    return q


# The R3 answer contract (score.py `call_form`, truth.py `wrap_call_fn`): the answer is one
# expression whose value is the public `send().await` result, bound to the declared result type and
# checked for success. Fix round 3 made it a single expression (no `use` lines or `let` statements;
# the committed banks store no contract text, pack.py words their call questions with this one).
CALL_CONTRACT = ("`code` is one Rust expression evaluating to the `send().await` result itself: it ends "
                 "with `.send().await`, with no `?` on that outermost send and no trailing `;`. Chain client "
                 "setters on `api` (`api.set_x(..).endpoint(..).send().await`) and write values inline: no "
                 "`;`, no `let` or `use` statements, no other `.await` or `?`, and no `return`, `break`, "
                 "`continue`, loops, `async` blocks or `unsafe`. Use only `vec!` and `format!` macros. "
                 "It is evaluated as "
                 "`let result: <the send() result type> = {{ code }};` inside an `async fn` with "
                 "`api: {client}`.")


def call_question_text(client, intent_text):
    """The call question for one item; pack.py words every bank's call questions with it, so the
    contract shown matches the scorer even for banks generated before the contract changed."""
    return (f"Write the call for this, sent with `.send().await`: {intent_text}. "
            + CALL_CONTRACT.format(client=client))


def call_question(pick, multi_client):
    return call_question_text(pick["client"], intent(pick, multi_client))


# ---------------------------------------------------------------- driver

def sdk_candidates(sdk, seed):
    """(candidates, client notes, reasons Counter, eligible-endpoint count)."""
    cands, notes, reasons = [], {}, Counter()
    counter = [0]
    canary_base = 100
    for client, cfile in sdk.clients:
        ctor, canaries, fields, reason, cnotes = client_ctor(sdk, client, counter)
        eps = list(C.decl.walk(client))
        if reason:
            notes[client["name"]] = {"constructible": False, "reason": reason, "endpoints": len(eps)}
            reasons["client not constructible: " + reason] += len(eps)
            continue
        notes[client["name"]] = {"constructible": True, "file": sdk.rel(cfile), "ctor": ctor,
                                 "canaries": canaries, "config_fields": fields, "endpoints": len(eps),
                                 "notes": cnotes}
        for ep, anc in eps:
            pick, why = make_pick(sdk, client, cfile, anc, ep, ctor, canaries, seed, canary_base)
            canary_base += 10
            if pick is None:
                reasons[why] += 1
                continue
            cands.append(pick)
    return cands, notes, reasons


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--seed", type=int, required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--role", default="main", choices=("main", "validation", "final"))
    ap.add_argument("--holdout-of", type=int, nargs="*", default=[])
    ap.add_argument("--repo", default=C.REPO)
    ap.add_argument("--quiet", action="store_true")
    ap.add_argument("--hash-file", help="write the bank's canonical SHA-256 (common.bank_hash) to this file")
    ap.add_argument("--sdk-constructors", action="store_true",
                    help="also build a required client value of an SDK type with the type's public const or "
                         "no-argument constructor (`Policy::public()`, `ApiVersion::PINNED`); off for the committed banks")
    a = ap.parse_args()
    OPTIONS["sdk_constructors"] = a.sdk_constructors
    out = os.path.abspath(a.out)
    os.makedirs(out, exist_ok=True)
    index = {"generator_version": GENERATOR_VERSION, "seed": a.seed, "role": a.role,
             **({"options": dict(OPTIONS)} if a.sdk_constructors else {}),
             "holdout_of": a.holdout_of, "picks_per_sdk": PICKS_PER_SDK,
             "measures": "docs-assisted prediction (see README)",
             "sdks": {}, "ineligible": {}, "totals": {}}
    strata_cov = Counter()
    items_per = Counter()
    eligible_sdks = 0
    for d in C.sdk_dirs(a.repo):
        sdk = C.Sdk(d, a.repo)
        cands, notes, reasons = sdk_candidates(sdk, a.seed)
        chosen, _ = bank_picks(sdk, a.seed, a.holdout_of)
        pool = [c for c in cands if not c.get("reused_endpoint")]
        row = {"pkg": sdk.pkg, "endpoints": sum(n["endpoints"] for n in notes.values()),
               "eligible_endpoints": len(cands), "picks": len(chosen),
               "reused_endpoint_picks": sum(1 for p in chosen if p.get("reused_endpoint")), "clients": {k: {kk: vv for kk, vv in v.items() if kk in ("constructible", "reason", "endpoints")}
                                                 for k, v in notes.items()},
               "ineligible_endpoint_reasons": dict(reasons.most_common())}
        index["sdks"][sdk.short] = row
        if not chosen:
            why = "no eligible endpoint" if not cands else "every eligible endpoint and instantiation is in an earlier bank"
            index["ineligible"][sdk.short] = {"why": why, "reasons": dict(reasons.most_common())}
            continue
        eligible_sdks += 1
        multi = len({p["client"] for p in cands}) > 1 or len(sdk.clients) > 1
        items, picks = [], []
        base_asked = set()
        wq = rq = 0
        for i, p in enumerate(chosen):
            p = dict(p)
            p["index"] = i
            p["boundary"] = "operation" if next(c for c, _ in sdk.clients if c["name"] == p["client"])["stmts"]["send"] else "prepared"
            picks.append(p)
            for s in p["strata"]:
                strata_cov[s] += 1
            client = next(c for c, _ in sdk.clients if c["name"] == p["client"])
            bq = None
            if p["client"] not in base_asked:
                bq = base_url_question(sdk, client, p)
                if bq:
                    base_asked.add(p["client"])
            wq += 1
            items.append({"id": f"{sdk.short}/w{i + 1}", "kind": "wire", "session": "wire", "q": wq,
                          "pick": i, "boundary": p["boundary"], "question": wire_question(p)})
            rq += 1
            items.append({"id": f"{sdk.short}/r{i + 1}", "kind": "result", "session": "rc", "q": rq,
                          "pick": i, "boundary": p["boundary"], "base_url": bq, "question": result_question(p, multi, bq)})
            rq += 1
            items.append({"id": f"{sdk.short}/c{i + 1}", "kind": "call", "session": "rc", "q": rq,
                          "pick": i, "boundary": p["boundary"], "intent": intent(p, multi),
                          "question": call_question(p, multi)})
            items_per["boundary_" + p["boundary"]] += 1
            items_per["reused_endpoint_picks"] += bool(p.get("reused_endpoint"))
            items_per["wire"] += 1
            items_per["result"] += 1
            items_per["call"] += 1
            if bq:
                items_per["result_with_base_url"] += 1
        used_clients = sorted({p["client"] for p in picks})
        bank = {"sdk": sdk.name, "short": sdk.short, "pkg": sdk.pkg, "crate": sdk.crate,
                "seed": a.seed, "role": a.role, "holdout_of": a.holdout_of, "generator_version": GENERATOR_VERSION,
                **({"options": dict(OPTIONS)} if a.sdk_constructors else {}),
                "clients": {k: notes[k] for k in used_clients},
                "picks": picks, "items": items}
        C.write_json(os.path.join(out, f"{sdk.short}.json"), bank)
    index["totals"] = {"sdks": len(index["sdks"]), "eligible_sdks": eligible_sdks,
                       "items": dict(items_per), "strata_coverage": {s: strata_cov.get(s, 0) for s in STRATA},
                       "picks": sum(r["picks"] for r in index["sdks"].values()),
                       "eligible_endpoints": sum(r["eligible_endpoints"] for r in index["sdks"].values()),
                       "endpoints": sum(r["endpoints"] for r in index["sdks"].values())}
    C.write_json(os.path.join(out, "index.json"), index)
    if a.hash_file:
        h, n = C.bank_hash(out)
        with open(a.hash_file, "w", encoding="utf-8") as f:
            f.write(f"{h}  {a.role} bank: seed {a.seed}, held out from seeds {a.holdout_of}; generator v{GENERATOR_VERSION}; "
                    f"SHA-256 of the canonical listing of its {n} question files (common.bank_hash)\n")
    if a.quiet:
        t = index["totals"]
        print(json.dumps({"role": a.role, "seed": a.seed, "eligible_sdks": t["eligible_sdks"], "sdks": t["sdks"],
                          "picks": t["picks"], "items": t["items"]}))
    else:
        t = index["totals"]
        print(f"eligible SDKs: {t['eligible_sdks']} / {t['sdks']}")
        print(f"endpoints: {t['eligible_endpoints']} eligible of {t['endpoints']}; picks: {t['picks']}")
        print("items per type: " + ", ".join(f"{k} {v}" for k, v in t["items"].items()))
        print("strata coverage (picks): " + ", ".join(f"{k} {v}" for k, v in t["strata_coverage"].items()))
        if index["ineligible"]:
            print("ineligible SDKs:")
            for k, v in index["ineligible"].items():
                rs = "; ".join(f"{r} ×{n}" for r, n in list(v["reasons"].items())[:3])
                print(f"  {k}: {v['why']} ({rs})")


if __name__ == "__main__":
    main()
