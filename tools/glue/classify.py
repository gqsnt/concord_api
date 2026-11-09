"""Glue classifier: label every non-blank line of hand-written SDK source.

    python3 -I tools/glue/classify.py [--repo .] [--json OUT.json] [--md OUT.md]
        [--sample N --seed S --sample-out FILE] [--sdk NAME ...]

Labels (charter, "Where the promise breaks today: glue"):
  model       provider data types, their serde and their accessors
  behavior    decisions: auth, signing, retry, rate limiting, traversal, validation, provider semantics
  glue:<cat>  code that restates a provider fact or works around the generated surface
              error | result | operation | redaction | forwarding | construction | paging | docs
  other       imports, re-exports, constants, module docs, macros and anything unrecognized
  declaration the `api!` block (the facts; reported apart, never glue)
  test        `#[cfg(test)]` modules inside `src/` (reported apart)

Every rule works on Rust items (struct, enum, impl, fn, const, ...), found by brace matching on
source with strings and comments blanked. A line takes the label of the innermost item that
contains it; comments and attributes take the label of the item they precede. Heuristic by
design: see README.md for the rules and their known errors.
"""
import argparse
import json
import os
import random
import re
import sys
from collections import Counter, defaultdict

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from static_scan import ENDPOINT_RE, REDACTING, api_spans, match_brace, strip_strings_and_comments  # noqa: E402

GLUE = ["error", "result", "operation", "redaction", "forwarding", "construction", "paging", "docs"]
MODEL_FILES = {"model.rs", "models.rs", "types.rs", "scalars.rs", "expandable.rs"}
PAGING_FILES = {"pages.rs", "pagination.rs", "paging.rs", "link.rs", "links.rs", "list.rs"}
BEHAVIOR_FILES = {"auth.rs", "sigv4.rs", "session.rs", "limiter.rs", "rate.rs", "ratelimit.rs", "retry.rs",
                  "polling.rs", "compression.rs", "credentials.rs", "store.rs", "guard.rs", "digest.rs",
                  # wire formats and provider protocols the SDK implements
                  "form.rs", "csv.rs", "xml.rs", "atom.rs", "codecs.rs", "format.rs", "streaming.rs", "stream.rs",
                  "events.rs", "soql.rs", "upload.rs", "download.rs", "bulk.rs", "bulk_query.rs", "bulk_export.rs",
                  "ingest.rs", "watch.rs", "transport.rs", "insert.rs", "rows.rs", "render.rs", "graphql.rs",
                  "batch.rs", "export.rs", "partitions.rs", "pit.rs", "trades.rs", "add_items.rs", "blob.rs",
                  "manifest.rs", "connection.rs"}
BEHAVIOR_NAME = re.compile(r"(Policy|Session|Limiter|Cache|Clock|Guard|Gate|Budget|Strategy|Retry|Settings|Signer|Refresher|Journal|Ledger)$")
PAGING_NAME = re.compile(r"(Cursor|Resume|Continuation|NextPage|PageToken|Paginat\w*|Pages|Pager|LinkPages)$|^(Cursor|Page(Token|Cursor|Links?))")
ERROR_FN = re.compile(r"^(rejection|reject|failure|fault|problem|decode_error|error_from|into_error|provider_error|status_class|classify_status|error_for|map_error)$")
BEHAVIOR_TOKENS = re.compile(r"\b(retry|attempt|attempts|limiter|acquire|admit|token|refresh|sleep|backoff|deadline|sign|signature|nonce|budget|journal|ledger|replay)\b", re.I)
CONTROL = re.compile(r"\b(if|match|for|while|loop|await)\b")
LOGIC = re.compile(r"\b(if|for|while|loop|await)\b|\bmatch\s+(?!self\b)")  # `match self` stays an accessor
META_FIELD = re.compile(r":\s*(Option<\s*)?(\w*::)?(ResponseMeta|ResponseHead\w*|HeaderMap|StatusCode|\w*Meta|\w*Metadata)\b")
PAGING_TOKENS = re.compile(r"\b(next|cursor|after|starting_after|page_token|offset|continuation|resume|Resume)\b")
SERDE_DERIVE = re.compile(r"derive\([^)]*\b(Deserialize|Serialize)\b")
SERDE_IMPL = re.compile(r"\b(Serialize|Deserialize<'de>|Visitor<'de>|DeserializeSeed<'de>)\s+for\b")
ERROR_NAME = re.compile(r"(Error|Fault|Failure|Rejection|Problem)s?$")
RESULT_NAME = re.compile(r"^(Response|ResponseMeta|Meta|Received\w*|With\w*Meta|\w*Envelope|Answer)$")
OP_ARM = re.compile(r"^\s*(?:\|\s*)?\w+Operation::\w+")
OP_TABLE = re.compile(r"^\s*(pub(\([^)]*\))?\s+)?(const|static)\s+\w*(WRITE|READ|IDEMPOTENT|UNCERTAIN|PAGED|SAFE)\w*\s*:")
DOC_FACT = re.compile(r"`(GET|POST|PUT|PATCH|DELETE|HEAD)\s+/|`/(api|v\d)[\w{}/.-]*`|\b(GET|POST|PUT|PATCH|DELETE)\s+`/")
CALLS_SEND = re.compile(r"\.\s*(send|prepare|receive|send_raw|into_parts)\s*\(")
CTOR_RET = re.compile(r"->\s*(Self|Result<\s*Self\b|Result<Self,|Option<Self>)")
OP_FN = re.compile(r"^(is_read|is_write|is_idempotent|idempotent|is_safe|is_mutation|is_mutating|class_of|"
                   r"operation_class|classify_operation|is_paged|is_uncertain_write|writes|reads)$")
OP_LINE = re.compile(r"\.method\(\)\s*(==|!=)\s*(Some\()?\s*\"(GET|POST|PUT|PATCH|DELETE)\"|"
                     r"\.name\(\)\s*(==|!=)\s*\"[\w.]+\"|Method::(GET|HEAD)\s*\|")


def line_starts(text):
    out, k = [0], text.find("\n")
    while k >= 0:
        out.append(k + 1)
        k = text.find("\n", k + 1)
    return out


def lineno(starts, pos):
    lo, hi = 0, len(starts) - 1
    while lo < hi:
        mid = (lo + hi + 1) // 2
        if starts[mid] <= pos:
            lo = mid
        else:
            hi = mid - 1
    return lo


def split_items(clean, a, b):
    """Items in clean[a:b]: (start, end_exclusive, header, body_start, body_end_or_None)."""
    items, i = [], a
    while i < b:
        while i < b and clean[i].isspace():
            i += 1
        if i >= b:
            break
        start = i
        if clean[i] == ";":  # stray terminator
            i += 1
            continue
        depth, j = 0, i
        end, body = None, None
        while j < b:
            c = clean[j]
            if c in "([":
                depth += 1
            elif c in ")]":
                depth -= 1
            elif c == ";" and depth == 0:
                end = j + 1
                break
            elif c == "{" and depth == 0:
                close = match_brace(clean, j)
                body = (j + 1, close)
                end = close + 1
                k = end
                while k < b and clean[k] in " \t":
                    k += 1
                if k < b and clean[k] == ";":
                    end = k + 1
                break
            j += 1
        if end is None:
            end = b
        header = re.sub(r"\s+", " ", clean[i:(body[0] - 1) if body else end]).strip()
        items.append((start, end, header, body[0] if body else None, body[1] if body else None))
        i = end
    return items


def header_kind(header):
    """(kind, name, trait, self_ty, attrs) from an item header with its attributes."""
    attrs = " ".join(re.findall(r"#!?\[[^\]]*\]", header))
    h = re.sub(r"#!?\[[^\]]*\]", " ", header).strip()
    h = re.sub(r"^(pub(\([^)]*\))?\s+)", "", h)
    h = re.sub(r"^((unsafe|async|const|extern(\s+\"\w+\")?|default)\s+)+", "", h)
    m = re.match(r"(struct|enum|union|trait|type|mod|fn|const|static|use|macro_rules!|impl)\b", h)
    if not m:
        if re.search(r"\w+!\s*$", h) or re.search(r"!\s*\(", h) or "!" in h.split(" ")[0]:
            return "macro", h.split("!")[0].split("::")[-1], None, None, attrs
        return "unknown", None, None, None, attrs
    kind = m.group(1)
    if kind == "impl":
        rest = re.sub(r"^impl\s*(<.*?>)?\s*", "", h)
        rest = re.split(r"\bwhere\b", rest)[0]
        if " for " in f" {rest} ":
            trait, ty = rest.split(" for ", 1)
        else:
            trait, ty = None, rest
        ty = re.sub(r"<.*", "", ty.strip()).split("::")[-1].strip("&' ")
        return "impl", None, trait.strip() if trait else None, ty, attrs
    nm = re.match(r"(?:struct|enum|union|trait|type|mod|fn|const|static|macro_rules!)\s*(\w+)", h)
    return kind, nm.group(1) if nm else None, None, None, attrs


class Sdk:
    def __init__(self, path):
        self.path = path
        self.name = os.path.basename(path)
        self.files = sorted(os.path.join(dp, f) for dp, _, fs in os.walk(os.path.join(path, "src"))
                            for f in fs if f.endswith(".rs"))
        self.texts = {fp: open(fp, encoding="utf-8").read() for fp in self.files}
        self.cleans = {fp: strip_strings_and_comments(t) for fp, t in self.texts.items()}
        self.clients, self.endpoints = set(), set()
        for fp, text in self.texts.items():
            for s, e in api_spans(text, self.cleans[fp]):
                bclean = self.cleans[fp][s:e]
                self.clients |= set(re.findall(r"\bclient\s+([A-Z]\w*)", bclean))
                self.endpoints |= {x[1] for x in ENDPOINT_RE.findall(bclean)}
        self.types = {}  # type name -> label decided from its definition
        self.wrappers = set()

    def type_label(self, fp, kind, name, attrs, body_clean, header):
        base = os.path.basename(fp)
        if kind in ("struct", "enum", "union", "type"):
            fields = body_clean or header
            if name in self.wrappers or any(re.search(r"[:(<,]\s*(Arc<|Box<)?(crate::)?(\w+::)?" + c + r"\b(?!\w)", fields)
                                            for c in self.clients):
                return "glue:construction"
            if name and ERROR_NAME.search(name) and not SERDE_DERIVE.search(attrs):
                return "glue:error"
            if name in ("Response", "ResponseMeta") or (name and RESULT_NAME.match(name) and not SERDE_DERIVE.search(attrs)):
                return "glue:result"
            if SERDE_DERIVE.search(attrs):
                return "model"
            if base != "error.rs" and kind == "struct" and META_FIELD.search(fields) and fields.count(":") >= 2:
                return "glue:result"
            if name and BEHAVIOR_NAME.search(name):
                return "behavior"
            if base in BEHAVIOR_FILES and base not in PAGING_FILES:
                return "behavior" if kind != "type" else "other"
            if base in PAGING_FILES or (name and PAGING_NAME.search(name)):
                return "glue:paging"
            if base == "error.rs":
                return "glue:error"
            if base in MODEL_FILES or "/model/" in fp:
                return "model"
            if base in BEHAVIOR_FILES or base == "policy.rs":
                return "behavior"
            return "model" if kind != "type" else "other"
        return None

    def pre_pass(self):
        """Wrapper structs and the label of every named type, before methods are judged."""
        for fp, clean in self.cleans.items():
            for start, end, header, bs, be in self._all_items(fp):
                kind, name, _, _, attrs = header_kind(header)
                if kind in ("struct", "enum", "union", "type") and name:
                    body_clean = clean[bs:be] if bs is not None else ""
                    lab = self.type_label(fp, kind, name, attrs, body_clean, header)
                    if lab == "glue:construction":
                        self.wrappers.add(name)
                    self.types.setdefault(name, lab)

    def _all_items(self, fp, a=None, b=None):
        clean = self.cleans[fp]
        a = 0 if a is None else a
        b = len(clean) if b is None else b
        for it in split_items(clean, a, b):
            yield it
            kind = header_kind(it[2])[0]
            if kind == "mod" and it[3] is not None:
                yield from self._all_items(fp, it[3], it[4])

    def label_fn(self, fp, name, header, body, owner_label, owner_ty):
        base = os.path.basename(fp)
        nb = sum(1 for ln in body.split("\n") if ln.strip())
        calls_ep = any(re.search(r"\.\s*" + re.escape(e) + r"\s*\(", body) for e in self.endpoints)
        if name and OP_FN.match(name):
            return "glue:operation"
        if name and ERROR_FN.match(name):
            return "glue:error"
        clean_body = strip_strings_and_comments(body)
        if owner_label == "policy":  # a method of a SendWith/SendOperation impl
            if BEHAVIOR_TOKENS.search(clean_body):
                return "behavior"
            if re.search(r"Response\s*[<{(]|\bwith_meta\b|\bhead\(\)|decode_with_meta", clean_body):
                return "glue:result"
            if re.search(r"\bstatus\(\)|is_success|StatusCode::|Error::", clean_body):
                return "glue:error"
            return "behavior"
        if owner_label in ("model", None) and nb <= 12 and not LOGIC.search(clean_body) \
                and owner_ty in self.types and self.types.get(owner_ty) == "model":
            return "model"  # an accessor of a provider data type
        if owner_label in ("glue:error", "glue:result", "glue:redaction", "glue:paging"):
            return owner_label
        if owner_ty in self.wrappers and CTOR_RET.search(header) and not calls_ep:
            return "glue:construction"
        if calls_ep and (CALLS_SEND.search(body) or owner_ty in self.wrappers) and nb <= 14 \
                and not re.search(r"\b(for|while|loop)\b", strip_strings_and_comments(body)):
            return "glue:paging" if PAGING_TOKENS.search(body) else "glue:forwarding"
        if re.search(r"\b(Response|ResponseMeta)\s*\{", body) and nb <= 20 and base not in BEHAVIOR_FILES \
                and not re.search(r"\b(if|match)\b|Err\(", clean_body):
            return "glue:result"
        if name and (name.endswith("_from") or re.match(r"(pages|next_page|cursor|resume|starting_at)", name)):
            return "glue:paging"
        if re.search(r"\bResponseMeta\b", header) and re.search(r"head|meta", name or ""):
            return "glue:result"
        if CTOR_RET.search(header) and "Err(" in body and "Result" in header:
            return "behavior"  # a validating constructor
        if owner_label == "model" and base not in BEHAVIOR_FILES and not LOGIC.search(clean_body):
            return "model"
        if owner_label == "model":
            return "behavior"  # logic on a provider data type: refresh, validation, conversion
        if base in BEHAVIOR_FILES or base == "policy.rs":
            return "behavior"
        if base in PAGING_FILES:
            return "glue:paging"
        if base == "error.rs":
            return "glue:error"
        return "behavior"

    def label_impl(self, fp, trait, ty, body):
        t = trait or ""
        tl = self.types.get(ty)
        if re.search(r"\bFrom<\s*(concord::)?Error\s*>|\bstd::error::Error\b|\berror::Error\b|^Error$", t):
            return "glue:error"
        if re.search(r"\bFrom<", t) and tl == "glue:error":
            return "glue:error"
        if re.search(r"\bDebug\b", t):
            if REDACTING.search(body):
                return "glue:redaction"
            return tl if tl in ("glue:error", "glue:result") else "other"
        if re.search(r"\bDisplay\b", t):
            if REDACTING.search(body):
                return "glue:redaction"
            if tl == "glue:error" or (ty and ERROR_NAME.search(ty)):
                return "glue:error"
            return "model" if tl == "model" else "other"
        if SERDE_IMPL.search(t + " for"):
            return "model"
        if re.search(r"\b(SendWith|SendOperation|Sends)\b", t):
            return "policy"  # judged per method; the frame takes the majority
        if re.search(r"\b(Page|Pages|Cursor|Paginated|Continuation|NextPage|LinkPages)\b", t):
            return "glue:paging"
        if re.search(r"\b(Encode|Decode|Incremental|Format|Codec)\b", t):
            return "behavior"
        if trait:
            return tl if tl and tl != "other" else "other"
        return None  # inherent: judged per method

    def classify(self):
        self.pre_pass()
        out = {}
        for fp in self.files:
            text, clean = self.texts[fp], self.cleans[fp]
            starts = line_starts(text)
            lines = text.split("\n")
            labels = ["other"] * len(lines)
            spans = api_spans(text, clean)

            def mark(a, b, lab):
                for ln in range(lineno(starts, a), lineno(starts, max(a, b - 1)) + 1):
                    labels[ln] = lab

            def attached(prev_end, start):
                """Start of the comments directly above an item (not `//!` module docs)."""
                first = lineno(starts, start)
                ln = first - 1
                top = lineno(starts, prev_end) if prev_end > 0 else -1
                keep = first
                while ln > top:
                    st = lines[ln].strip()
                    if st.startswith("//!") or st.startswith("#!["):
                        break
                    if st.startswith("//") or st.startswith("/*") or st.startswith("*"):
                        keep = ln
                    elif st:
                        break
                    ln -= 1
                return starts[keep] if keep < first else start

            def walk(a, b, owner_label=None, owner_ty=None, in_test=False):
                prev_end = a
                for start, end, header, bs, be in split_items(clean, a, b):
                    kind, name, trait, ty, attrs = header_kind(header)
                    start, prev_end = attached(prev_end, start), end
                    raw_header = text[start:(bs - 1) if bs else end]
                    test = in_test or re.search(r"#\[cfg\(test\)\]", raw_header) is not None
                    if test:
                        mark(start, end, "test")
                        continue
                    if any(s <= start <= e for s, e in spans) or (kind == "macro" and name == "api"):
                        mark(start, end, "declaration")
                        continue
                    body = text[bs:be] if bs is not None else ""
                    if kind == "use":
                        mark(start, end, "other")
                    elif kind == "mod":
                        mark(start, end, "other")
                        if bs is not None:
                            walk(bs, be)
                    elif kind in ("type", "const", "static") and owner_label:
                        mark(start, end, "behavior" if owner_label == "policy" else owner_label)
                    elif kind in ("struct", "enum", "union", "type"):
                        mark(start, end, self.types.get(name) or self.type_label(
                            fp, kind, name, attrs, clean[bs:be] if bs else "", header))
                    elif kind == "impl":
                        lab = self.label_impl(fp, trait, ty, body)
                        is_policy = lab == "policy"
                        if is_policy:
                            lab = None
                        mark(start, end, lab or "other")
                        if bs is not None:
                            walk(bs, be, owner_label="policy" if is_policy else (lab if lab else self.types.get(ty)), owner_ty=ty)
                            if lab is None:  # inherent or policy impl: its frame takes the type's or the majority label
                                inner = Counter(labels[lineno(starts, bs) + 1:lineno(starts, be)])
                                majority = inner.most_common(1)[0][0] if inner else "other"
                                frame = majority if is_policy else (self.types.get(ty) or majority)
                                if is_policy:
                                    policy_frames[ty] = frame
                                for ln in list(range(lineno(starts, start), lineno(starts, bs) + 1)) + [lineno(starts, be)]:
                                    if labels[ln] == "other":
                                        labels[ln] = frame
                    elif kind == "fn":
                        mark(start, end, self.label_fn(fp, name, raw_header, body, owner_label, owner_ty))
                    elif kind in ("const", "static"):
                        mark(start, end, "behavior" if owner_label == "policy" else (owner_label if owner_label and owner_label != "other" else "other"))
                    elif kind == "trait":
                        mark(start, end, "behavior")
                    else:
                        mark(start, end, "other")

            policy_frames, unit_types = {}, {}
            for start, end, header, bs, be in self._all_items(fp):
                kind, name, _, _, _ = header_kind(header)
                if kind == "struct" and bs is None and "(" not in header:
                    unit_types[name] = (start, end)
            walk(0, len(clean))
            for ty, frame in policy_frames.items():  # a unit-struct policy type takes its policy's label
                if ty in unit_types:
                    a, b = unit_types[ty]
                    a = attached(0, a) if a > 0 else a
                    for ln in range(lineno(starts, a), lineno(starts, max(a, b - 1)) + 1):
                        if labels[ln] in ("model", "behavior", "other"):
                            labels[ln] = frame
            # line-level overrides: operation tables and arms, docs restating declared facts
            for i, ln in enumerate(lines):
                if labels[i] in ("declaration", "test"):
                    continue
                st = ln.strip()
                if OP_ARM.match(ln) or OP_TABLE.match(ln) or OP_LINE.search(ln):
                    labels[i] = "glue:operation"
                elif (st.startswith("///") or st.startswith("//!")) and DOC_FACT.search(st):
                    labels[i] = "glue:docs"
            # leading module docs and file-level attributes stay "other" (already the default)
            out[fp] = [(i + 1, labels[i], lines[i]) for i in range(len(lines)) if lines[i].strip()]
        return out


def summarize(results, repo):
    per_sdk, totals, by_cat_items = {}, Counter(), defaultdict(list)
    for sdk, files in results.items():
        c = Counter()
        for fp, rows in files.items():
            run = None
            for n, lab, _ in rows:
                c[lab] += 1
                if lab.startswith("glue:"):
                    if run and run[1] == lab and n <= run[3] + 3:
                        run[3] = n
                        run[4] += 1
                    else:
                        if run:
                            by_cat_items[run[1]].append(tuple(run))
                        run = [os.path.relpath(fp, repo), lab, n, n, 1]
                elif run and n > run[3] + 3:
                    by_cat_items[run[1]].append(tuple(run))
                    run = None
            if run:
                by_cat_items[run[1]].append(tuple(run))
        c["glue"] = sum(v for k, v in c.items() if k.startswith("glue:"))
        per_sdk[sdk] = dict(c)
        totals.update(c)
    excerpts = {}
    for cat, runs in by_cat_items.items():
        runs.sort(key=lambda r: -r[4])
        picked, seen = [], set()
        for f, _, a, b, n in runs:
            sdk = f.split("/")[1] if f.startswith("examples/") else f
            if sdk in seen:
                continue
            seen.add(sdk)
            picked.append({"file": f, "lines": f"{a}-{b}", "nonblank": n})
            if len(picked) == 3:
                break
        excerpts[cat] = picked
    return per_sdk, dict(totals), excerpts


def markdown(per_sdk, totals, excerpts, commit):
    cols = ["glue"] + [f"glue:{g}" for g in GLUE] + ["model", "behavior", "other", "declaration", "test"]
    head = "| SDK | " + " | ".join(c.replace("glue:", "") for c in cols) + " |"
    out = [f"Commit `{(commit or '?')[:8]}`. Non-blank lines of `examples/*-sdk/src/**/*.rs`.", "",
           "## Totals", "", "| Label | Lines | Share of SDK source |", "|---|---|---|"]
    src = sum(v for k, v in totals.items() if k != "glue")
    for c in cols:
        out.append(f"| {c} | {totals.get(c, 0):,} | {100 * totals.get(c, 0) / src:.1f}% |")
    out += ["", f"All non-blank lines: {src:,}.", "", "## Per SDK (sorted by glue)", "", head,
            "|---|" + "---|" * len(cols)]
    for sdk, c in sorted(per_sdk.items(), key=lambda kv: -kv[1].get("glue", 0)):
        out.append(f"| {sdk} | " + " | ".join(str(c.get(k, 0)) for k in cols) + " |")
    out += ["", "## Largest excerpts per glue category (three SDKs each)", ""]
    for g in GLUE:
        ex = excerpts.get(f"glue:{g}", [])
        out.append(f"- **{g}**: " + "; ".join(f"`{e['file']}:{e['lines']}` ({e['nonblank']} lines)" for e in ex))
    return "\n".join(out) + "\n"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", default=".")
    ap.add_argument("--json")
    ap.add_argument("--md")
    ap.add_argument("--sdk", nargs="*")
    ap.add_argument("--sample", type=int, default=0)
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--sample-out")
    a = ap.parse_args()
    ex = os.path.join(a.repo, "examples")
    sdks = sorted(d for d in os.listdir(ex) if d.endswith("-sdk") and (not a.sdk or d in a.sdk))
    results = {}
    for d in sdks:
        results[d] = Sdk(os.path.join(ex, d)).classify()
    per_sdk, totals, excerpts = summarize(results, a.repo)
    try:
        import subprocess
        commit = subprocess.run(["git", "-C", a.repo, "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()
    except OSError:
        commit = None
    if a.json:
        json.dump({"commit": commit, "instrument": "glue classifier v1", "totals": totals, "per_sdk": per_sdk,
                   "excerpts": excerpts}, open(a.json, "w"), indent=1, sort_keys=True)
    md = markdown(per_sdk, totals, excerpts, commit)
    if a.md:
        open(a.md, "w").write(md)
    if a.sample:
        rng = random.Random(a.seed)
        pool = [(sdk, os.path.relpath(fp, a.repo), n, lab, txt) for sdk, files in results.items()
                for fp, rows in files.items() for n, lab, txt in rows if lab not in ("declaration", "test")]
        by_sdk = defaultdict(list)
        for p in pool:
            by_sdk[p[0]].append(p)
        chosen_sdks = rng.sample(sorted(by_sdk), min(len(by_sdk), max(15, a.sample // 3)))
        picks = []
        while len(picks) < a.sample:
            sdk = chosen_sdks[len(picks) % len(chosen_sdks)]
            picks.append(rng.choice(by_sdk[sdk]))
        lines = [f"{i + 1}. `{f}:{n}` **{lab}** — `{txt.strip()[:140]}`" for i, (_, f, n, lab, txt) in enumerate(picks)]
        open(a.sample_out or "/dev/stdout", "w").write("\n".join(lines) + "\n")
    if not a.md:
        print(md)


if __name__ == "__main__":
    main()
