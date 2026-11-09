"""Parse `concord::api! { .. }` declarations into a tree (I3 question generation, I1 support).

A pragmatic parser for the DSL as written in the 55 SDKs (grammar: docs/CHEATSHEET.md). It reads
owners (client, scope, endpoint), their parameter lists and their own statements, the `///` docs
and `#[..]` attributes written before each owner (`docs`, `attrs`), a client's visibility, and
its parameter groups (`stmts["groups"]`: name → parameters). It does not
evaluate Rust expressions or types: those stay as source text. Run Python with `-I`.
"""
import re

from static_scan import strip_strings_and_comments, match_brace, api_spans

METHODS = ("GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS")
PLACEMENT_RE = re.compile(r"^(query|header|body)\b\s*(\((.*?)\))?\s*(each\s+)?")


def split_top(text, sep=","):
    """Split on `sep` at bracket depth 0 (strings already blanked by the caller's clean text)."""
    parts, depth, cur = [], 0, []
    for ch in text:
        if ch in "([{<":
            depth += 1
        elif ch in ")]}>":
            depth -= 1
        if ch == sep and depth == 0:
            parts.append("".join(cur))
            cur = []
        else:
            cur.append(ch)
    if "".join(cur).strip():
        parts.append("".join(cur))
    return parts


def split_raw(raw, clean):
    """Split a parameter list at top-level commas found in the comment/string-blanked text,
    returning the raw segments with doc and line comments removed."""
    out, depth, start = [], 0, 0
    for i, ch in enumerate(clean):
        if ch in "([{<":
            depth += 1
        elif ch in ")]}>":
            depth -= 1
        elif ch == "," and depth == 0:
            out.append(raw[start:i])
            start = i + 1
    out.append(raw[start:])
    segs = []
    for seg in out:
        lines = [ln for ln in seg.split("\n") if not ln.strip().startswith("//")]
        seg = " ".join(ln.strip() for ln in lines).strip()
        if seg:
            segs.append(seg)
    return segs


def parse_param(raw):
    """One parameter: `#[sensitive] header(KEY) name?: Type = expr`, or a group splice `..Name`."""
    s = raw.strip()
    if s.startswith(".."):
        return {"group": s[2:].strip()}
    attrs = re.findall(r"#\[([^\]]*)\]", s)
    s = re.sub(r"#\[[^\]]*\]\s*", "", s).strip()
    placement, key, each = "path", None, False
    m = PLACEMENT_RE.match(s)
    if m:
        placement = m.group(1)
        key = m.group(3)
        each = bool(m.group(4))
        s = s[m.end():].strip()
    else:
        m2 = re.match(r"^each\s+", s)
        if m2:
            each = True
            s = s[m2.end():]
    nm = re.match(r"^(r#)?(\w+)\s*(\?)?\s*:\s*", s)
    if not nm:
        return {"unparsed": raw.strip()}
    rest = s[nm.end():]
    default = None
    # split `Type = expr` at the first top-level '='
    depth = 0
    for i, ch in enumerate(rest):
        if ch in "([{<":
            depth += 1
        elif ch in ")]}>":
            depth -= 1
        elif ch == "=" and depth == 0 and rest[i + 1:i + 2] != ">" and rest[i - 1:i] not in ("=", "!", "<", ">"):
            default = rest[i + 1:].strip()
            rest = rest[:i]
            break
    return {"name": nm.group(2), "optional": bool(nm.group(3)), "type": rest.strip(),
            "default": default, "placement": placement, "key": key, "each": each,
            "sensitive": any(a.strip() == "sensitive" for a in attrs), "attrs": attrs}


def own_statements(body, clean):
    """Statements of an owner body outside its child owners' bodies, as raw text."""
    flat, depth = [], 0
    for ch_raw, ch in zip(body, clean):
        if ch == "{":
            depth += 1
        if depth == 0 or (depth == 1 and ch == "{"):
            pass
        flat.append(ch_raw if depth == 0 else ("\n" if ch_raw == "\n" else " "))
        if ch == "}":
            depth -= 1
    return "".join(flat)


def parse_owner_body(body, clean, owner):
    """Fill owner['stmts'], owner['children'] from the text between its braces."""
    i = 0
    stmts = {"base": None, "path": None, "subdomain": None, "query": [], "headers": [],
             "format": None, "request": None, "response": None, "send": None, "limit": None,
             "params_groups": []}
    children = []
    docs, attrs = [], []  # `///` lines and `#[..]` attributes waiting for the next child owner
    n = len(clean)
    while i < n:
        # skip whitespace (in the raw text: comments are blank in `clean`)
        while i < n and body[i].isspace():
            i += 1
        if i >= n:
            break
        # doc comments and attributes are blanked/raw; capture docs from raw text
        if body.startswith("///", i) or body.startswith("//", i):
            j = body.find("\n", i)
            j = n if j < 0 else j
            if body.startswith("///", i) and not body.startswith("////", i):
                line = body[i + 3:j]
                docs.append(line[1:] if line.startswith(" ") else line)
            i = j + 1
            continue
        if clean.startswith("#[", i):
            j = paren_end_sq(clean, i + 1)
            attrs.append(re.sub(r"\s+", " ", body[i + 2:j]).strip())
            i = j + 1
            continue
        rest = clean[i:]
        m = re.match(r"(?:pub(?:\([^)]*\))?\s+)?(client|scope)\s+(r#)?(\w+)\s*", rest)
        me = re.match(r"(" + "|".join(METHODS) + r")\s+(r#)?(\w+)\s*", rest)
        if m or me:
            mm = m or me
            j = i + mm.end()
            params = []
            if j < n and clean[j] == "(":
                k = paren_end(clean, j)
                params = [parse_param(p) for p in split_raw(body[j + 1:k], clean[j + 1:k])]
                j = k + 1
            out = None
            # the header ends at the first `{` (a body) or `;` (an endpoint without one) outside
            # brackets; `->` before it introduces the output type
            k, depth = j, 0
            while k < n:
                ch = clean[k]
                if ch in "([<":
                    depth += 1
                elif ch in ")]>" and not (ch == ">" and clean[k - 1] == "-"):
                    depth -= 1
                elif ch in "{;" and depth <= 0:
                    break
                k += 1
            if k >= n:
                break
            head = clean[j:k]
            arrow = re.search(r"->", head)
            if arrow:
                out = body[j + arrow.end():k].strip()
            if clean[k] == ";":
                ob = cb = None
                i = k + 1
            else:
                ob = k
                cb = ob + match_brace(clean[ob:], 0)
                i = cb + 1
            if m:
                child = {"kind": m.group(1), "name": m.group(3), "params": params}
                vis = re.match(r"pub(\([^)]*\))?", rest)
                child["visibility"] = (vis.group(0) if vis else "")
            else:
                child = {"kind": "endpoint", "method": me.group(1), "name": me.group(3),
                         "params": params, "output": out}
            child["docs"], child["attrs"] = docs, attrs
            docs, attrs = [], []
            if ob is None:
                parse_owner_body("", "", child)
            else:
                parse_owner_body(body[ob + 1:cb], clean[ob + 1:cb], child)
            children.append(child)
            continue
        # a block statement: query { } / headers { }
        mb = re.match(r"(query|headers)\s*\{", rest)
        if mb:
            docs, attrs = [], []
            ob = i + mb.end() - 1
            cb = ob + match_brace(clean[ob:], 0)
            entries = [e.strip() for e in body[ob + 1:cb].split(";") if e.strip()]
            stmts[mb.group(1)] += [re.sub(r"\s+", " ", e) for e in entries]
            i = cb + 1
            continue
        # a simple statement up to ';'
        semi = clean.find(";", i)
        if semi < 0:
            break
        st = body[i:semi].strip()
        docs, attrs = [], []
        kw = st.split(None, 1)[0] if st else ""
        val = st[len(kw):].strip()
        if kw in ("base", "path", "subdomain"):
            stmts[kw] = val.strip('"')
        elif kw in ("format", "request", "response", "send", "limit"):
            stmts[kw] = val
        elif kw == "params":
            stmts["params_groups"].append(val)
            stmts.setdefault("groups", {}).update([parse_group(val)])
        i = semi + 1
    owner["stmts"] = stmts
    owner["children"] = children


def parse_group(val):
    """`Name(x?: T, ..)` of a `params Name(..);` statement → (name, [parameter, ..])."""
    m = re.match(r"\s*(\w+)\s*\(", val)
    if not m:
        return (val.strip(), [])
    raw = val[m.end():val.rstrip().rfind(")")]
    return (m.group(1), [parse_param(p) for p in split_raw(raw, strip_strings_and_comments(raw))])


def paren_end_sq(clean, j):
    """Offset of the `]` closing the `[` at `clean[j]`."""
    d = 0
    for k in range(j, len(clean)):
        if clean[k] == "[":
            d += 1
        elif clean[k] == "]":
            d -= 1
            if d == 0:
                return k
    return len(clean) - 1


def paren_end(clean, j):
    d = 0
    for k in range(j, len(clean)):
        if clean[k] == "(":
            d += 1
        elif clean[k] == ")":
            d -= 1
            if d == 0:
                return k
    return len(clean) - 1


def parse_file(text):
    """All clients declared in one Rust source file."""
    clean = strip_strings_and_comments(text)
    clients = []
    for (s, e) in api_spans(text, clean):
        root = {"kind": "root", "params": []}
        parse_owner_body(text[s + 1:e], clean[s + 1:e], root)
        clients += [c for c in root["children"] if c["kind"] == "client"]
    return clients


def walk(owner, ancestors=()):
    """Yield (endpoint, ancestors tuple) for every endpoint below `owner`."""
    for c in owner.get("children", []):
        if c["kind"] == "endpoint":
            yield c, ancestors + (owner,)
        else:
            yield from walk(c, ancestors + (owner,))
