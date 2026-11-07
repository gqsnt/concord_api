"""I1 static pass: per-SDK desirability metrics from source text (no build).

Ported from the seed scripts (`evidence/tools_seed/{measure,surface}.py` of the mission pack)
into one importable module. Every class is a heuristic over Rust source; its known false
positives are listed in `README.md` next to this file. Run Python with `-I`.

Metric IDs follow the mission's DESIRABILITY contract:
  D1a forwarders, D1b client wrappers, D2a repeated facts, D3 layer code,
  S2 hand-maintained redaction, S3 unmarked credential-like values,
  R4a DSL lines per endpoint, R4b declaration doc lines explaining Concord mechanics.
"""
import os
import re
from collections import defaultdict

METHODS = r"(GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS)"
ENDPOINT_RE = re.compile(r"(?m)^\s*(?:#\[[^\]]*\]\s*)*" + METHODS + r"\s+(\w+)\s*[(<{]")
# Declaration doc lines that explain Concord rather than the provider (seed MECH class).
MECH = re.compile(r"send\(\)|prepare\(|with_base_url|Concord|SendWith|policy|redact|native request|receive\(",
                  re.I)
# Method names too generic to identify a generated endpoint call without a send/prepare nearby.
STD = {"get", "insert", "remove", "clear", "len", "push", "iter", "map", "next", "send", "list",
       "filter", "find", "query", "contains", "parse", "into", "as_ref", "take", "replace", "set",
       "status", "write", "read", "delete", "update", "create", "head", "copy", "search", "version"}
SECRETISH = re.compile(r"(token|secret|password|passwd|api_?key|apikey|credential|signature|auth|bearer|key)\b",
                       re.I)
SECRETISH_ALLOW = {"key_id", "access_key_id", "idempotency_key", "key", "keys", "object_key",
                   "cache_key", "authority", "author", "authors"}
REDACTING = re.compile(r"redact|withh[eo]ld|finish_non_exhaustive|<hidden>|omit|\*\*\*", re.I)


def strip_strings_and_comments(text):
    """Blank string/char literal and comment contents, keeping offsets and newlines."""
    out = list(text)
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        if text.startswith("//", i):
            j = text.find("\n", i)
            j = n if j < 0 else j
            for k in range(i, j):
                out[k] = " "
            i = j
        elif text.startswith("/*", i):
            j = text.find("*/", i + 2)
            j = n if j < 0 else j + 2
            for k in range(i, j):
                if text[k] != "\n":
                    out[k] = " "
            i = j
        elif c == "r" and re.match(r'r#*"', text[i:i + 10]) and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")):
            m = re.match(r'r(#*)"', text[i:])
            hashes = m.group(1)
            end = text.find('"' + hashes, i + len(m.group(0)))
            end = n if end < 0 else end + 1 + len(hashes)
            for k in range(i + len(m.group(0)), max(i + len(m.group(0)), end - 1 - len(hashes))):
                if text[k] != "\n":
                    out[k] = " "
            i = end
        elif c == '"':
            j = i + 1
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            for k in range(i + 1, min(j, n)):
                if text[k] != "\n":
                    out[k] = " "
            i = j + 1
        elif c == "'":
            m = re.match(r"'(\\.|[^\\'])'", text[i:i + 6]) or re.match(r"'\\u\{[0-9a-fA-F]+\}'", text[i:i + 12])
            if m:
                for k in range(i + 1, i + len(m.group(0)) - 1):
                    out[k] = " "
                i += len(m.group(0))
            else:
                i += 1  # a lifetime
        else:
            i += 1
    return "".join(out)


def match_brace(clean, start):
    depth = 0
    for k in range(start, len(clean)):
        if clean[k] == "{":
            depth += 1
        elif clean[k] == "}":
            depth -= 1
            if depth == 0:
                return k
    return len(clean) - 1


def api_spans(text, clean):
    """(open_brace, close_brace) offsets of every `concord::api! { .. }` / `api! { .. }` block."""
    spans = []
    for m in re.finditer(r"(?m)concord::api!\s*\{|^\s*api!\s*\{", clean):
        s = m.end() - 1
        spans.append((s, match_brace(clean, s)))
    return spans


def owner_tree(body, clean):
    """Owners (client/scope/endpoint) of one block with their direct statements."""
    events = []
    for m in re.finditer(r"\bclient\s+([A-Z]\w*)|\bscope\s+(\w+)|(?<![\w.])" + METHODS + r"\s+(\w+)\s*[(<{]", clean):
        kind = "client" if m.group(1) else "scope" if m.group(2) else "endpoint"
        name = m.group(1) or m.group(2) or m.group(4)
        k, depth = m.end() - 1, 0
        while k < len(clean):
            ch = clean[k]
            if ch in "([<":
                depth += 1
            elif ch in ")]>":
                depth -= 1
            elif ch == "{" and depth <= 0:
                break
            elif ch == ";" and depth <= 0:
                k = -1
                break
            k += 1
        if k < 0 or k >= len(clean):
            continue
        events.append((k, match_brace(clean, k), kind, name, m.start()))
    events.sort()
    nodes = []
    for (s, e, kind, name, hdr) in events:
        parent = None
        for idx in range(len(nodes) - 1, -1, -1):
            ps, pe = nodes[idx]["span"]
            if ps < s and e <= pe:
                parent = idx
                break
        flat, depth = [], 0
        for ch_raw, ch in zip(body[s + 1:e], clean[s + 1:e]):
            if ch == "{":
                depth += 1
            if depth == 0:
                flat.append(ch_raw)
            if ch == "}":
                depth -= 1
        flat = "".join(flat)
        pm = re.search(r'(?m)^\s*path\s+"([^"]*)"', flat)
        bm = re.search(r'(?m)^\s*base\s+"([^"]*)"', flat)
        # key sets of the owner's own query/headers blocks (literal keys or header constants)
        keys = set()
        for blk in re.finditer(r"(?s)\b(query|headers)\s*\{(.*?)\}", body[s + 1:e]):
            if blk.start() and depth_of(body[s + 1:e], blk.start()) != 0:
                continue
            for km in re.finditer(r'(?m)^\s*(?:append|override|remove)?\s*("[^"]+"|[A-Z][A-Z0-9_:]*)\s*(?:=|;)', blk.group(2)):
                keys.add(blk.group(1) + ":" + km.group(1).strip('"').lower())
        nodes.append({"kind": kind, "name": name, "span": (s, e), "parent": parent,
                      "path": pm.group(1) if pm else None, "base": bm.group(1) if bm else None,
                      "keys": sorted(keys)})
    return nodes


def depth_of(text, pos):
    d = 0
    for ch in text[:pos]:
        if ch == "{":
            d += 1
        elif ch == "}":
            d -= 1
    return d


def fidelity(nodes):
    """D2a parts: version literals in endpoint paths; >=3 sibling endpoints sharing a literal
    leading segment no scope captures; the same block key on >=3 sibling endpoints."""
    children = defaultdict(list)
    for i, n in enumerate(nodes):
        if n["parent"] is not None:
            children[n["parent"]].append(n)
    shared_heads, shared_keys = [], []
    for i, n in enumerate(nodes):
        if n["kind"] == "endpoint":
            continue
        eps = [c for c in children[i] if c["kind"] == "endpoint"]
        heads = defaultdict(list)
        for c in eps:
            segs = [s for s in (c["path"] or "").split("/") if s]
            if len(segs) >= 2 and not segs[0].startswith("{"):
                heads[segs[0]].append(c["name"])
        for head, names in heads.items():
            if len(names) >= 3:
                shared_heads.append({"owner": n["name"], "prefix": head, "endpoints": len(names)})
        keyed = defaultdict(list)
        for c in eps:
            for k in c["keys"]:
                keyed[k].append(c["name"])
        for k, names in keyed.items():
            if len(names) >= 3:
                shared_keys.append({"owner": n["name"], "key": k, "endpoints": len(names)})
    version_paths = [n["name"] for n in nodes if n["kind"] == "endpoint" and n["path"]
                     and re.search(r"(^|/)v\d+(\.\d+)?(/|$)", n["path"])]
    return version_paths, shared_heads, shared_keys


def fn_bodies(text):
    """(name, is_pub, start, body) for every fn with a body."""
    for m in re.finditer(r"(?m)^(\s*)(pub(?:\([^)]*\))?\s+)?(?:const\s+)?(?:async\s+)?fn\s+(\w+)", text):
        k = text.find("{", m.end())
        semi = text.find(";", m.end())
        if k < 0 or (0 <= semi < k and "where" not in text[m.end():semi]):
            continue
        d = 0
        for j in range(k, len(text)):
            if text[j] == "{":
                d += 1
            elif text[j] == "}":
                d -= 1
                if d == 0:
                    vis = m.group(2) or ""
                    yield m.group(3), bool(vis) and "pub(crate)" not in vis and "pub(super)" not in vis, m.start(), text[k:j + 1]
                    break


def span_lines(text, start, chunk):
    """Line numbers (0-based) of the non-blank lines of `chunk`, found at offset `start`."""
    first = text.count("\n", 0, start)
    return {first + i for i, ln in enumerate(chunk.split("\n")) if ln.strip()}


def nonblank(s):
    return sum(1 for ln in s.split("\n") if ln.strip())


def item_extent(text, start):
    """Text of the item (impl/struct) starting at `start`, through its closing brace."""
    k = text.find("{", start)
    semi = text.find(";", start)
    if k < 0 or (0 <= semi < k):
        return text[start:semi + 1] if semi >= 0 else ""
    clean = strip_strings_and_comments(text[k:])
    return text[start:k + match_brace(clean, 0) + 1]


def analyse_sdk(path):
    files = sorted(os.path.join(dp, f) for dp, _, fs in os.walk(os.path.join(path, "src"))
                   for f in fs if f.endswith(".rs"))
    r = defaultdict(int)
    r.update(version_paths=[], shared_heads=[], shared_keys=[], wrapper_structs=[], forwarders=[],
             unmarked_secretish=[], redacting_impls=[])
    clients, endpoints = set(), set()
    outside_texts = {}
    for fp in files:
        text = open(fp, encoding="utf-8").read()
        clean = strip_strings_and_comments(text)
        r["src_nonblank"] += nonblank(text)
        spans = api_spans(text, clean)
        for (s, e) in spans:
            body, bclean = text[s + 1:e], clean[s + 1:e]
            for ln in body.split("\n"):
                st = ln.strip()
                if not st:
                    continue
                if st.startswith("///") or st.startswith("//!"):
                    r["decl_doc"] += 1
                    if MECH.search(st):
                        r["decl_doc_mech"] += 1
                elif not st.startswith("//"):
                    r["decl_dsl"] += 1
            r["clients"] += len(re.findall(r"\bclient\s+[A-Z]\w*\s*[(<{]", bclean))
            r["scopes"] += len(re.findall(r"\bscope\s+\w+\s*[(<{]", bclean))
            r["endpoints"] += len(ENDPOINT_RE.findall(bclean))
            r["sensitive"] += len(re.findall(r"#\[sensitive\]", body))
            r["send_declared"] += len(re.findall(r"(?m)^\s*send\s+[^;]+;", bclean))
            r["send_template"] += len(re.findall(r"(?m)^\s*send\s+[^;]+->[^;]+;", bclean))
            clients |= set(re.findall(r"\bclient\s+([A-Z]\w*)", bclean))
            endpoints |= {x[1] for x in ENDPOINT_RE.findall(bclean)}
            v, h, k = fidelity(owner_tree(body, bclean))
            r["version_paths"] += v
            r["shared_heads"] += h
            r["shared_keys"] += k
            # S3: declared values with credential-like names lacking #[sensitive]
            # scan the comment/string-blanked text so prose in doc comments is not a "value"
            for m in re.finditer(r"((?:#\[[^\]]*\]\s*)*)((?:query|header|body)(?:\([^)]*\))?\s+)?(\w+)\??\s*:\s*[A-Za-z&]", bclean):
                attrs, name = m.group(1), m.group(3)
                if name in ("path", "base", "send", "limit", "format", "request", "response"):
                    continue
                if SECRETISH.search(name) and "#[sensitive]" not in attrs and not name.endswith("_id") \
                        and name not in SECRETISH_ALLOW:
                    r["unmarked_secretish"].append(name)
        outside = text
        for s, e in reversed(spans):
            outside = outside[:s] + "{" + "\n" * text[s:e].count("\n") + "}" + outside[e + 1:]
        outside_texts[fp] = outside
    d3_union = set()  # (file, line) of every line counted in any D3 component, counted once
    for fp, outside in outside_texts.items():
        oclean = strip_strings_and_comments(outside)
        # D1b: public structs holding a generated client (field type is exactly the client type)
        for m in re.finditer(r"(?s)pub\s+struct\s+(\w+)(?:<[^>]*>)?\s*\{(.*?)\n\}", outside):
            fields = strip_strings_and_comments(m.group(2))
            if any(re.search(r"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?\w+\s*:\s*(?:Arc<|Box<)?" + c + r"\s*[>,]?\s*,?\s*$", fields)
                   for c in clients):
                r["wrapper_structs"].append(m.group(1))
        # D1a: pub fns whose body calls a generated endpoint and prepares/sends/receives
        for name, is_pub, start, body in fn_bodies(outside):
            if not is_pub:
                continue
            called = [e for e in endpoints if re.search(r"\.\s*" + re.escape(e) + r"\s*\(", body)]
            called = [e for e in called if e not in STD or re.search(r"\.(prepare|send|receive)\s*\(", body)]
            if called and re.search(r"prepare\s*\(|\.send\s*\(|receive|execute|run\(", body):
                r["forwarders"].append(name)
                r["d3_forwarder_lines"] += nonblank(body)
                d3_union |= {(fp, ln) for ln in span_lines(outside, outside.index(body, start), body)}
        # S2 + D3 redaction: hand-written Debug/Display impls; withholding ones counted apart
        for m in re.finditer(r"(?m)^\s*impl(?:<[^>]*>)?\s+(?:fmt::|std::fmt::|core::fmt::)?(Debug|Display)\s+for\s+(\w+)", outside):
            ext = item_extent(outside, m.start())
            r["manual_" + m.group(1).lower() + "_impls"] += 1
            if m.group(1) == "Debug":
                r["manual_debug_lines"] += nonblank(ext)
            if REDACTING.search(ext):
                r["redacting_impls"].append(m.group(2))
                r["d3_redaction_lines"] += nonblank(ext)
                d3_union |= {(fp, ln) for ln in span_lines(outside, m.start(), ext)}
        # D3 send policies
        for m in re.finditer(r"(?m)^\s*impl(?:<[^>]*>)?\s+(?:concord::)?SendWith\b", oclean):
            r["send_policy_impls"] += 1
            ext = item_extent(outside, m.start())
            r["d3_policy_lines"] += nonblank(ext)
            d3_union |= {(fp, ln) for ln in span_lines(outside, m.start(), ext)}
        r["from_concord_error"] += len(re.findall(r"impl\s+From<concord::Error>\s+for", outside))
        # D3 value+meta result wrappers: a pub struct with a `meta`/`ResponseMeta` field and a value
        for m in re.finditer(r"(?s)pub\s+struct\s+(\w+)<[^>]*>\s*\{(.*?)\n\}", outside):
            if re.search(r"ResponseMeta", m.group(2)) and re.search(r"\bvalue\s*:|\bdata\s*:", m.group(2)):
                r["result_wrappers"] += 1
                r["d3_result_wrapper_lines"] += nonblank(m.group(0))
                d3_union |= {(fp, ln) for ln in span_lines(outside, m.start(), m.group(0))}
        if os.path.basename(fp) == "error.rs":
            r["d3_error_rs_lines"] += nonblank(outside)
            d3_union |= {(fp, ln) for ln in span_lines(outside, 0, outside)}
    r["S2"] = len(r["redacting_impls"])
    r["D1a"] = len(r["forwarders"])
    r["D1b"] = len(r["wrapper_structs"])
    r["D2a_version_paths"] = len(r["version_paths"])
    r["D2a_shared_heads"] = len(r["shared_heads"])
    r["D2a_shared_keys"] = len(r["shared_keys"])
    # D3: union of the component spans (a line in error.rs that is also a redaction impl counts
    # once). A coarse diagnostic: error.rs includes provider semantics, not only layer code.
    r["D3"] = len(d3_union)
    r["S3_candidates"] = sorted(set(r["unmarked_secretish"]))
    r["R4a_dsl_lines"] = r["decl_dsl"]
    r["R4b_mech_doc_lines"] = r["decl_doc_mech"]
    del r["unmarked_secretish"]
    return dict(r)


TOTALS = ["clients", "scopes", "endpoints", "decl_dsl", "decl_doc", "decl_doc_mech", "sensitive",
          "send_declared", "send_template", "src_nonblank", "D1a", "D1b", "D2a_version_paths",
          "D2a_shared_heads", "D2a_shared_keys", "D3", "d3_error_rs_lines", "d3_policy_lines",
          "d3_forwarder_lines", "d3_redaction_lines", "d3_result_wrapper_lines", "S2",
          "manual_debug_impls", "manual_display_impls", "manual_debug_lines", "send_policy_impls",
          "from_concord_error", "result_wrappers"]


def scan(examples_dir):
    sdks = sorted(d for d in os.listdir(examples_dir) if d.endswith("-sdk"))
    per = {d: analyse_sdk(os.path.join(examples_dir, d)) for d in sdks}
    tot = {k: sum(v.get(k, 0) for v in per.values()) for k in TOTALS}
    tot["sdks"] = len(per)
    tot["sdks_with_D1a"] = sum(1 for v in per.values() if v["D1a"])
    tot["sdks_with_D1b"] = sum(1 for v in per.values() if v["D1b"])
    tot["sdks_with_S2"] = sum(1 for v in per.values() if v["S2"])
    tot["S3_candidates"] = sorted({f"{d}:{n}" for d, v in per.items() for n in v["S3_candidates"]})
    tot["R4a_dsl_lines_per_endpoint"] = round(tot["decl_dsl"] / max(tot["endpoints"], 1), 3)
    return per, tot
