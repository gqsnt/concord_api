"""H04 reader materials: an SDK's public types and its generated rustdoc, rendered to text from
one commit with one deterministic extraction (the same for both arms).

    python3 -I tools/desirability/h04/materials.py --commit SHA --out DIR [--sdk SHORT ..]

`materials(commit, sdk, out)` (also the library entry point used by `runner.py`):

1. the commit's tree is extracted with `git archive` into
   `$CARGO_TARGET_DIR/h04-src/<sha12>/` (never the working tree, so an arm is exactly its commit);
2. `cargo +nightly doc --no-deps --lib --target <host>` with rustdoc JSON output
   (`-Z unstable-options --output-format json`, **public items only**: no
   `--document-private-items`, so a caller sees what docs.rs would show) for the six H04 SDKs at
   once, in **that commit's own target directory** `$CARGO_TARGET_DIR/h04-target/<sha12>/` (fix
   round 7: with one shared directory cargo kept the first commit's JSON for a second commit),
   with `--crate-version h04-tree-<sha>` as a provenance marker; every JSON must name the commit
   (`json_provenance_problems`), else the build is refused. The JSON files are copied to
   `<out>/<sha12>/rustdoc/`. `--render-control` checks this on a copy of the commit's tree that
   differs in one generated doc string (`render_control`);
3. each SDK's JSON is rendered twice from the crate root (re-exports inlined as rustdoc inlines
   them, public modules recursed, `#[doc(hidden)]` and `__`-prefixed items skipped):
   - `<sdk>.types.txt`: the public types, signatures only (no doc text): structs with public
     fields, enums with variants, traits, type aliases, functions, constants, inherent methods,
     trait impls (associated types written out). A docs-only candidate must leave it
     byte-identical (`runner.py` refuses an arm whose types differ from the base's);
   - `<sdk>.docs.txt`: the generated documentation as text, in page order: the crate page, the
     package README (`examples/<sdk>/README.md` of that commit) unless the crate page already
     holds it (whitespace-normalized containment; e.g. `#![doc = include_str!("../README.md")]`),
     then every item with its signature and doc text, its public fields and variants with theirs,
     its methods with theirs;
   - one **example-free filter** (`MarkdownFilter`, extraction 5: fix round 2's coordinator
     decision, fix round 3's prose rule, fix round 4's whitespace and host rules), applied to every docs string and the README identically
     in both arms: every code block in any form (any fence language, `~~~`, indented) is replaced
     by "(code example omitted from the probe materials)"; every inline code span holding an
     expression (a call, a constructor, a method chain or fragment, a struct literal, a field
     access, a macro: anything `bare` does not accept) by "(code omitted)"; in the remaining prose,
     call chains on the SDK's generated client, scopes and endpoints (I3's rule: `probes/common.py`
     `find_chains` + `chain_hits`, navigation from that commit's own `api!` declaration) by "(call
     example omitted ..)", any line that is code (`code_line`: what a removed fence leaves) by the
     block placeholder, and every expression written in plain prose (`prose_expressions`: field
     accesses, calls with arguments or through a path, chains, fragments, indexes, struct
     literals; URLs, numbers, real hosts, file names, abbreviations and sentence punctuation
     excepted; whitespace inside an expression, `value . count` or a chain broken over lines,
     read as absent: `ws_normalize`; an operand, `value.count/2`, never a host) by "(code
     omitted)", the same rule as for a span. Contract prose, tables, bare type/item names,
     literals and wire texts stay: no code reaches readers except bare names.
     `format_audit` uses its own detector (`broad_tokens`, deliberately broad, with its own
     whitespace rules, independent of the filter's) and plain marker searches: block forms filter
     byte-identically; conversions from blocks (fence removed, spans, table cells), from spans
     (into prose, blocks, table cells) and from prose (into spans, table cells) show no code-like
     token the original's output does not show, and span <-> prose <-> table conversions show
     exactly the same ones; synthetic positives (20 snippets in twelve forms, and 25 one-unit
     snippets of the reviewer's whitespace and operand cases in eleven forms) leave no marker;
     negative controls (`audit_controls`: each filter rule switched off, the whitespace
     normalization switched off, round 3's host-path exception restored) must make the audit
     fail (`format_invariant`);
   - the **structural gate** (fix rounds 5-6; used by `runner.py prompts` and `sessions`): the
     SHA-256 of each SDK's exact candidate-arm materials as the reader sees them
     (`reader_materials`, `materials_digest`, `gate_digests`), the check that a clearance file
     names them for every SDK whose materials differ from the baseline's in any byte
     (`digest_gate`), the review evidence (`review_diff`: a complete unified diff, no
     normalisation; `apply_diff` rebuilds the candidate from it; `diff_highlights`: the broad
     detector's tokens on added lines) and `gate_selftest`;
   - `<sdk>.meta.json`: sizes and word counts (diagnostics: total, crate page, README, per
     generated endpoint type), the filter's counts (code blocks, spans, code lines, prose
     expressions, call chains removed by place) and removed texts (diagnostics), the format audit
     and its negative controls, a coverage audit (`coverage`: the reader-visible contract statements by category:
     URLs, `Request:` lines, result types, base-URL/origin statements, retry statements, error
     variants), the toolchain, the JSON's and README's SHA-256, the commit.

Run Python with `-I`. Nothing here reads SDK tests or records: rustdoc's JSON, the package README and
the `api!` declaration (for the navigation paths only).
"""
import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "probes"))
import common as C  # noqa: E402  (I3: REPO, CARGO_TARGET_DIR, cargo_env, sha256_file, write_json)
import pack as P  # noqa: E402  (I3: r_type, r_fn, r_gparams, r_where, r_impl_header, clean_path, hidden, indent)

SDKS = {  # short name -> (directory under examples/, package, crate)
    "cloudflare-zones": ("cloudflare-zones-sdk", "concord-sdk-cloudflare-zones", "concord_sdk_cloudflare_zones"),
    "sentry": ("sentry-sdk", "concord-sdk-sentry", "concord_sdk_sentry"),
    "kraken-spot-rest": ("kraken-spot-rest-sdk", "concord-sdk-kraken-spot-rest", "concord_sdk_kraken_spot_rest"),
    "nager-holidays": ("nager-holidays-sdk", "concord-sdk-nager-holidays", "concord_sdk_nager_holidays"),
    "xero-accounting": ("xero-accounting-sdk", "concord-sdk-xero-accounting", "concord_sdk_xero_accounting"),
    "strava": ("strava-sdk", "concord-sdk-strava", "concord_sdk_strava"),
}
HOST = "x86_64-unknown-linux-gnu"
EXTRACTION = 5  # bump when the rendering changes; recorded in every meta file (4: fix round 3, prose expressions; 5: fix round 4, whitespace inside expressions, hosts)
CALL_OMITTED = C.CALL_OMITTED          # "(call example omitted from the probe materials)": a chain in prose
BLOCK_OMITTED = "(code example omitted from the probe materials)"  # a code block, or a code line in prose
SPAN_OMITTED = "(code omitted)"        # an inline code span holding an expression
FENCE = re.compile(r"^([ \t]*)(```+|~~~+)(.*)$")
LIST_ITEM = re.compile(r"^[ \t]{0,3}(?:[-*+]|\d{1,9}[.)])(?:[ \t]|$)")
SPAN = re.compile(r"(`+)([^`\n]+?)\1")
TYPE_KEYWORDS = {"dyn", "impl", "mut", "const", "fn", "for", "unsafe", "extern", "as", "where", "crate", "self", "super",
                 "Self", "async", "move", "ref", "static"}
FILE_EXT = r"(?:md|rs|json|toml|txt|py|lock|yml|yaml|sh|gpx|fit|tcx|csv|html|xml|gz)"
# a prose line (code spans masked) that is code (what a removed fence leaves): a binding, a match arm, `)?;` or
# `);` at the end, a struct literal, a macro invocation, a function signature, a `use` path, a comment, an opening
# brace at the end, a list of code items ending with a comma (arguments, imports), a struct-literal field, an
# attribute, a JSON member. Calls, field accesses and fragments are removed token by token (`prose_expressions`).
CODE_LINE = [re.compile(p) for p in (
    r"\blet\s+(?:mut\s+)?[A-Za-z_]\w*\s*[:=]",
    r"=>",
    r"\)\?\s*;",
    r"\);\s*$",
    r"\b[A-Z]\w*\s*\{\s*[a-z_]\w*\s*:",
    r"\b[a-z_]\w*!\s*[(\[{]",
    r"\bfn\s+[A-Za-z_]\w*\s*[(<]",
    r"^\s*use\s+\w+::",
    r"^\s*//",
    r"[\w)\]>]\s*\{\s*$",
    r'^\s*(?:(?:[^\s,{]+|[\w:]*\{[^{}]*\})\s*,\s*)+$',
    r"^\s*->",
    r"::<",
    r"^\s*[a-z_]\w*:\s+\S+,\s*$",
    r"^\s*#!?\[[^\]]*\]\s*$",
    r'^\s*"[^"\n]*"\s*:\s*\S',
)]


def is_type_or_path(t):
    """A bare Rust path, type or item name: identifiers, `::`, generics (associated bindings inside
    `<..>`), references, lifetimes, tuples/slices/arrays, `fn(..)` types, trait objects; a trailing
    `()` only on a single name (a method referred to by name: `send()`)."""
    toks = re.findall(r"'[A-Za-z_]\w*|[A-Za-z_]\w*|\d+|::|->|=>|\.\.|[^\sA-Za-z0-9_]", t)
    if not toks or any(x in ("=>", "{", "}", "|", "?", '"', "\\", "..", "$", "@") for x in toks):
        return False
    angle = square = 0
    for i, x in enumerate(toks):
        if x == "<":
            angle += 1
        elif x == ">":
            angle -= 1
        elif x == "[":
            square += 1
        elif x == "]":
            square -= 1
        elif x == "=" and angle <= 0:
            return False
        elif x == ";" and square <= 0:
            return False
        elif x == ".":
            return False  # a field access or a method-chain fragment (`.pages()`) is not a bare name
        elif x == "!":
            if i != len(toks) - 1:
                return False
        elif x == "(":
            p = toks[i - 1] if i else None
            if p in ("fn", "Fn", "FnMut", "FnOnce"):
                continue
            if p is None or p in ("<", ",", "&", "(", "[", "->", "mut", "dyn", "impl", ":", "*", "+"):
                continue
            # `name()` (one segment, no arguments) refers to a method; `Path::f()` is a call
            if toks[i + 1:] == [")"] and i == 1:
                continue
            return False
    for a, b in zip(toks, toks[1:]):
        if (re.fullmatch(r"[A-Za-z_]\w*", a) and re.fullmatch(r"[A-Za-z_]\w*", b)
                and a not in TYPE_KEYWORDS and b not in TYPE_KEYWORDS):
            return False
    return True


def bare(span):
    """True when an inline code span holds no expression: a literal, a wire text (HTTP method and
    target, URL, path, media type, header line, query pairs, JSON document), a file name, an
    attribute, a flag, or a bare path/type/name (`is_type_or_path`)."""
    t = span.strip()
    if not t:
        return True
    if re.fullmatch(r"-?\d[\d_]*(?:\.\d+)?(?:[eE][+-]?\d+)?[a-z0-9]*", t) or re.fullmatch(r"\d+(?:[.,]\d+)+", t):
        return True  # a number, a version or an address (`1.99.1`, `999,999,999.99`, `192.0.2.42`)
    if re.fullmatch(r'b?"(?:[^"\\]|\\.)*"|\'(?:[^\'\\]|\\.)\'', t) or t in (".", ".."):
        return True  # a literal, or a dot path segment
    if t.startswith("#"):  # a one-word attribute (`#[non_exhaustive]`); a path or arguments make it code
        return bool(re.fullmatch(r"#!?\[[A-Za-z_]\w*\]", t))
    if t[0] in "{[":
        try:
            json.loads(t)
            return True
        except ValueError:
            pass
    if re.fullmatch(r"(?:GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS)(?:\s+(?:/|\.\.\./|https?://|\{)\S*)?", t):
        return True  # an HTTP method, optionally with its target
    if re.fullmatch(r"[a-z][a-z0-9+.-]*://\S+", t) or re.fullmatch(r"/\S*", t):
        return True
    if re.fullmatch(MEDIA_TYPE + r"(?:\s*;\s*[\w-]+=[\w-]+)*", t):
        return True  # a media type
    # an expression by the prose rule, URLs and media types aside (fix round 4: the same rule as in
    # prose, before the header-line, query, file-name, host and path-template forms below)
    if prose_expressions(re.sub(MEDIA_TYPE, lambda m: "\0" * len(m.group(0)), URL.sub(lambda m: "\0" * len(m.group(0)), t))):
        return False
    if re.fullmatch(r"[A-Za-z][A-Za-z0-9-]*:\s+[^()`]+", t) and "::" not in t:
        return True
    if re.fullmatch(r"[\w.\-\[\]]+=[^\s&]*(?:&[\w.\-\[\]]+=[^\s&]*)*", t):
        return True
    if re.fullmatch(r"[\w.\-/*]+\." + FILE_EXT, t) and benign_dotted(t.rsplit("/", 1)[-1]):
        return True
    if re.fullmatch(r"[\w-]+(?:\.[\w-]+)+", t) and benign_dotted(t):  # a host or an abbreviation
        return True
    if t.endswith((",", ";")) or (t.startswith("->") and t != "->"):  # a list item, a statement, a signature's tail
        return False
    # path templates and placeholders of the wire (`{zone_id}`, `PublicHolidays/{Year}/{CountryCode}`,
    # `Bearer {api_token}`)
    if re.fullmatch(r"\{[\w.]+\}", t) or (("/" in t) and re.fullmatch(r"[\w.{}/\-]+", t)):
        return True
    if "[]" in t and re.fullmatch(r"[A-Za-z_]\w*(?:\[\])?(?:\.[A-Za-z_]\w*(?:\[\])?)+", t):
        return True  # a JSON path of the wire (`errors[].source`): not Rust
    if re.fullmatch(r"[A-Za-z]+ \{[\w.]+\}", t):
        return True
    # plain words with spaces and no operator, call or brace (`EAPI:Invalid nonce`, `else inherit`,
    # `offline_access accounting.invoices`): an expression with spaces needs an operator or a call
    if (" " in t and re.fullmatch(r"[A-Za-z0-9_ :,.*/-]+", t) and not re.search(r"\b(?:as|return|move|let)\b", t)
            and not t.startswith("//") and not prose_expressions(t)):
        return True  # (fix round 3: not when a word is an expression, e.g. a field access, the prose rule)
    if re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9-]*", t) or re.fullmatch(r"--?[A-Za-z][\w-]*", t):
        return True
    return is_type_or_path(t)


TLDS = {"com", "io", "net", "org", "dev", "test", "example", "app", "co", "uk", "de", "fr", "eu", "info", "local", "localhost"}
# a two-label host needs one of these (`sentry.io`); the others, which are also plausible field names
# (`value.info`, `config.local`, `value.test`), count only as the last of three or more labels
STRONG_TLDS = {"com", "io", "net", "org", "example", "localhost"}
ABBREVIATIONS = {"e.g", "i.e", "etc", "vs", "cf", "approx"}
URL = re.compile(r"[a-z][a-z0-9+.-]*://\S+")
MEDIA_TYPE = r"\b(?:application|text|image|audio|video|multipart|message|font|model|\*)/[A-Za-z0-9.+*-]+"
# what follows a dotted token and makes it an operand: `*`, `+`, `%`, `-` before a number, or `/`
# before a number that is not a version path (`/2` is arithmetic, `/2.0/..` and `/v1` are paths)
ARITHMETIC_AFTER = re.compile(r"\s*(?:[*+%^]|-\s*[\d(]|/\s*(?:\d+(?![\d.]*[./\w{])|[(\s]))")


def is_host(tok):
    """A host name: lowercase letter/digit/hyphen labels, no `_`, the last label a known TLD (a
    two-label host needs a `STRONG_TLDS` one: `sentry.io`, `api.xero.com`, `api.example.test`)."""
    labels = tok.lower().split(".")
    if len(labels) < 2 or not all(re.fullmatch(r"[a-z0-9](?:[a-z0-9-]*[a-z0-9])?", lb) for lb in labels):
        return False
    return labels[-1] in STRONG_TLDS or (len(labels) >= 3 and labels[-1] in TLDS)


def benign_dotted(tok, after=""):
    """A dotted token (no call) that is not an expression: a host (`is_host`), a file name
    (`README.md`; not `.lock` on a lowercase stem: `state.lock` is a field), an abbreviation
    (`e.g`), or a name whose segments after the first are all capitalized (`Nager.Date`: Rust
    members are snake_case). None of them when arithmetic follows (`ARITHMETIC_AFTER`: an operand,
    `value.count/2`). A host followed by `/` stays a host only when a path follows (fix round 4: the
    former host-path exception, any `x.y/..`, is gone). Everything else dotted is an access
    (`value.count`, the navigation name `Kraken.account.open_orders`, `Tokens.scope`): removed alike
    in prose and in spans."""
    t = tok.lstrip(".")
    if ARITHMETIC_AFTER.match(after):
        return False
    if t.lower().rstrip(".") in ABBREVIATIONS:
        return True
    if is_host(t):
        return True
    stem, _, last = t.rpartition(".")
    if re.fullmatch(FILE_EXT, last) and (last != "lock" or not stem.islower()):
        return True
    segs = t.split(".")
    return len(segs) > 1 and all(seg[:1].isupper() for seg in segs[1:])


# ---- whitespace inside expressions (fix round 4): `value . count`, `Type :: new ( http )`, a chain
# broken over lines. Both the filter and the audit's detector read the text with that whitespace
# removed (`ws_normalize`), mapped back to the original positions for the replacement.

_GAP = r"[ \t]*\n?[ \t]*"  # spaces and tabs with at most one line break (a blank line ends a chain)
DOT_GAP = re.compile(r"(?P<l>[\w)\]?])(?P<w1>" + _GAP + r")\.(?P<w2>" + _GAP + r")(?=[A-Za-z_])")
PATH_GAP = re.compile(r"(?<=[\w)>\]])(?P<w1>" + _GAP + r")::(?P<w2>" + _GAP + r")(?=[A-Za-z_<])")
INDEX_GAP = re.compile(r"(?<=[\w)\]])(?P<w1>" + _GAP + r")\[(?P<w2>[ \t]*)(?P<c>-?\d+|[a-z_]\w*|\"[^\"\n]*\"|\d*\.\.=?\d*)?"
                       r"(?P<w3>[ \t]*)\](?![(:\[])")  # an index: a number, a variable, a key, a range
TRY_GAP = re.compile(r"(?<=\))(?P<w1>" + _GAP + r")\?")
PAREN_IN = re.compile(r"\((?P<w>" + _GAP + r")(?=[^\s)])|(?<=[^\s(])(?P<v>" + _GAP + r")\)|\((?P<e>" + _GAP + r")\)")
PAREN_BEFORE = re.compile(r"(?P<l>[\w)\]?])(?P<w>" + _GAP + r")\(")
CODE_CHARS = re.compile(r"&(?=[\w(])|=|\|[\w|]|->|\w\([^\s)]|[a-z_]\w*\.[a-z_]")  # (type paths, strings, `send()` in a parenthetical are prose)


def _drop(text, cuts):
    """(text without the index ranges `cuts`, map from new to old index)."""
    keep, out, last = [], [], 0
    for a, b in sorted(cuts):
        if a < last:
            a = last
        if a >= b:
            continue
        out.append(text[last:a])
        keep.extend(range(last, a))
        last = b
    out.append(text[last:])
    keep.extend(range(last, len(text)))
    return "".join(out), keep


def _left_word(text, i):
    """The run of word characters, `.` and `:` ending at index i (inclusive)."""
    j = i
    while j >= 0 and (text[j].isalnum() or text[j] in "_.:"):
        j -= 1
    return text[j + 1:i + 1]


def ws_normalize(text, broad=False):
    """(text, index map): whitespace removed where it sits inside an expression. Around `::`
    always; before `.` (and after it then) when a name follows; after a `.` alone when a lowercase
    name follows that is not called (a sentence may start with `prepare() builds ..`), the left
    word is a name and no abbreviation or number (sentences do not start in lowercase otherwise); around an index `[..]` (not a link or an intra-doc link); between `)` and `?`;
    inside parentheses; before `(` when the left word is a function path (`Type::new`), a member
    or a dotted name that is not `benign_dotted`, or the parentheses are empty, hold code
    (`CODE_CHARS`: an operator, a call with arguments, a field access) or a list
    of single items with a number or string literal (`make (http, 3)`). With `broad` (the
    audit's detector only), before any `(` whose content holds no space except after commas (a
    call `make (x, 3)`; a prose parenthetical with words, `(if any)`, stays apart)."""
    maps = list(range(len(text)))

    def stage(t, finder):
        new, keep = _drop(t, finder(t))
        return new, keep

    def dots(t):
        cuts = []
        for m in DOT_GAP.finditer(t):
            w1, w2 = m.span("w1"), m.span("w2")
            if w1[1] > w1[0] and (w2[1] > w2[0] or not t[w2[1]].isupper()):
                cuts += [w1, w2]  # (not `as .F`: a capitalized fragment stays a fragment, removed as such)
            elif w2[1] > w2[0]:
                left = _left_word(t, m.start("l"))
                right = t[w2[1]:w2[1] + 1]
                after = re.match(r"\w*(.?)", t[w2[1]:]).group(1)
                if (right.islower() or right == "_") and m.group("l") not in ")]?" and after != "(" \
                        and left.lower().strip(".") not in ABBREVIATIONS and not re.fullmatch(r"[\d.]*", left):
                    cuts.append(w2)  # `value. count`; not `..). prepare() builds` (a sentence naming a method)
        cuts += [x for m in PATH_GAP.finditer(t) for x in (m.span("w1"), m.span("w2"))]
        cuts += [x for m in INDEX_GAP.finditer(t) for x in (m.span("w1"), m.span("w2"), m.span("w3"))]
        cuts += [m.span("w1") for m in TRY_GAP.finditer(t)]
        return cuts

    def inner(t):
        return [m.span(g) for m in PAREN_IN.finditer(t) for g in ("w", "v", "e") if m.group(g) is not None]

    def before(t):
        cuts = []
        for m in PAREN_BEFORE.finditer(t):
            w = m.span("w")
            if w[1] == w[0] or m.group("l") in ")]?":  # `send() (once)`: a call followed by a parenthetical
                continue
            close = _close_paren(t, w[1])
            content = t[w[1] + 1:close] if close is not None else ""
            left = _left_word(t, m.start("l"))
            # a function path (`Type::new (http)`), a member (`value.items (..)`), a fragment; not a
            # type or variant path followed by a parenthetical (`Error::Envelope (MissingResult)`)
            # and not a call followed by one (`send() (once)`)
            chain = ("::" in left and left.rsplit("::", 1)[-1][:1].islower()) or left.startswith(".") or \
                ("." in left.strip(".") and not benign_dotted(left))
            items = [x.strip() for x in content.split(",")]
            literal_list = len(items) > 1 and all(x and not re.search(r"\s", x) for x in items) and \
                any(re.fullmatch(r"-?\d[\w.]*|\"[^\"]*\"", x) for x in items)
            if close is not None and (chain or not content.strip() or CODE_CHARS.search(content) or literal_list):
                cuts.append(w)
            elif broad and close is not None and "\0" not in content and \
                    re.fullmatch(r"[^\s,]+(?:,\s*[^\s,]+)*", content):
                cuts.append(w)
        return cuts

    for finder in (dots, dots, inner, before):
        text, keep = stage(text, finder)
        maps = [maps[k] for k in keep]
    return text, maps


def _close_paren(text, i):
    depth, j = 0, i
    while j < len(text):
        c = text[j]
        if c == "\n":
            return None
        if c == "(":
            depth += 1
        elif c == ")":
            depth -= 1
            if depth == 0:
                return j
        j += 1
    return None


STRUCT_BODY = re.compile(r"\s*\{\s*(?:(?:[a-z_]\w*(?:\s*:[^{}\n]*?)?|\.\.)\s*,\s*)*(?:[a-z_]\w*(?:\s*:[^{}\n]*?)?|\.\.)?\s*\}")
WIRE_PLACEHOLDER = re.compile(r"\s*\{[\w.]+\}")  # `Bearer {token}`: a wire template, not a struct literal


def prose_expressions(text):
    """[(start, end)] of the expressions written in plain prose (fix rounds 3-4; spans, URLs and
    placeholders must be masked first), in `text`'s positions. The text is read with the whitespace
    inside expressions removed (`ws_normalize`: `value . count`, a chain broken over lines). A
    token is a postfix chain `[.]name[::name..]` followed by calls `(..)` (balanced on the line; an
    unclosed one runs to the end of the line), `?`, indexes `[..]` and members `.name`. It is an
    expression when it has a member (a field access, unless `benign_dotted`, which also looks at
    what follows: `value.count/2` is an operand), a call with arguments (other than a plural `(s)`
    or an `Fn(..)` type), a call through a path (`Type::f()`), more than one call, an index, or a
    leading dot (a fragment: `.pages()`, `.items`), or when a type path is followed by a struct
    body (`Config { token: t }`, `Error::X { error, .. }`). A single name with an empty call
    (`send()`, a method referred to by name), a plural, a JSON path with empty brackets
    (`errors[].source`) and a path template placeholder (`{issue.issue}`) are kept, as they are in
    an inline span."""
    norm, idx = ws_normalize(text)
    return [(idx[a], idx[b - 1] + 1) for a, b in _scan_expressions(norm) if b > a]


def _scan_expressions(text):
    out, pos = [], 0
    start_re = re.compile(r"(?<![\w.:/@\-])(\.\.|\.)?([A-Za-z_]\w*(?:::[A-Za-z_]\w*)*)")  # (`..Base`: a struct update)
    while True:
        m = start_re.search(text, pos)
        if not m:
            return out
        st, j = m.start(), m.end()
        lead, path = m.group(1) or "", m.group(2)
        calls, members, call_args, unclosed, indexes, empty_ix = 0, 0, [], False, 0, 0
        while j < len(text):
            if text[j] == "(":
                close = _close_paren(text, j)
                if close is None:
                    unclosed = True
                    j = text.find("\n", j) if "\n" in text[j:] else len(text)
                    break
                calls += 1
                call_args.append(text[j + 1:close].strip())
                j = close + 1
                continue
            if text[j] == "?" and calls:
                j += 1
                continue
            mi = re.match(r"\[([^\]\n]*)\]", text[j:])
            if mi:  # an index (`items[0]`); empty brackets are a JSON path of the wire (`errors[].source`)
                indexes += 1
                empty_ix += not mi.group(1).strip()
                j += mi.end()
                continue
            mm = re.match(r"\.([A-Za-z_]\w*)", text[j:])
            if mm:
                members += 1
                j += mm.end()
                continue
            break
        tok = text[st:j]
        plural = calls == 1 and members == 0 and call_args[0] in ("s", "S", "es")
        expr = False
        if unclosed:
            expr = True
        elif st > 0 and text[st - 1] == "{" and text[j:j + 1] == "}" and not calls:
            expr = False  # a path template placeholder
        elif indexes and not calls and indexes == empty_ix:
            expr = False  # a JSON path (`errors[].source`), kept as in a span
        elif lead or indexes:
            expr = True
        elif members and not calls:
            expr = not benign_dotted(tok, text[j:j + 24])
        elif calls == 1 and not members and path.rsplit("::", 1)[-1] in ("Fn", "FnMut", "FnOnce", "fn"):
            expr = False  # a closure or function type (`Fn(&str) -> bool`)
        elif calls and not plural:
            expr = members > 0 or "::" in path or any(call_args) or calls > 1
        elif not calls and not members and path.rsplit("::", 1)[-1][:1].isupper():
            body = STRUCT_BODY.match(text, j)
            if body and not WIRE_PLACEHOLDER.fullmatch(text, j, body.end()):
                expr, j = True, body.end()
        if expr:
            out.append((st, j))
        pos = max(j, m.end())


def code_line(line):
    """A prose line (code spans already masked) that is code."""
    return any(p.search(line) for p in CODE_LINE)


class MarkdownFilter:
    """The one example filter of the materials, the same for both arms and independent of how
    anything is written (fix round 2: example-free, the coordinator's decision; fix round 3: prose
    expressions). Every code block, fenced with any info string (```rust, ```text, ```, ~~~) or
    indented by four spaces, is replaced by "(code example omitted from the probe materials)";
    every inline code span that holds an expression (a call, a constructor, a method chain, a
    struct literal, a field access, a macro: anything `bare` does not accept) by "(code omitted)";
    in the remaining prose, call chains on the SDK's generated client, scopes and endpoints (I3's
    rule: `probes/common.py` `find_chains` + `chain_hits`, navigation from the arm's own `api!`
    declaration) by "(call example omitted ..)", any line that is code (`code_line`: a fence
    removed leaves code in prose) by the block placeholder, and every expression written in plain
    prose (`prose_expressions`) by "(code omitted)". Contract prose, tables and bare names (types,
    paths, items, literals, wire texts) are kept: no code reaches readers except bare names."""

    def __init__(self, navs):
        self.navs = navs
        self.stats = {"code_blocks": 0, "spans_kept": 0, "spans_removed": 0, "prose_expressions": 0, "prose_code_lines": 0,
                      "chains_removed": {"prose": 0}}
        self.removed = []  # chain texts removed from prose (diagnostic)
        self.removed_spans = []  # span texts replaced (diagnostic)
        self.removed_lines = []  # prose lines replaced (diagnostic)
        self.removed_prose = []  # prose expression tokens replaced (diagnostic)
        self.sources = []  # (name, raw docs string) of every string filtered (for the audits)

    def prose(self, lines):
        text = "\n".join(lines)
        # 1. inline code spans: bare names kept, expressions replaced
        def span(m):
            if bare(m.group(2)):
                self.stats["spans_kept"] += 1
                return m.group(0)
            self.stats["spans_removed"] += 1
            self.removed_spans.append(m.group(2))
            return SPAN_OMITTED
        text = SPAN.sub(span, text)
        # 2. endpoint call chains outside spans
        cuts = []
        masked = SPAN.sub(lambda m: "\0" * len(m.group(0)), text)
        for st, en, names in C.find_chains(masked):
            if not C.chain_hits(names, self.navs):
                continue
            m = re.search(r"[A-Za-z_][\w.]*$", masked[:st])
            cuts.append((m.start() if m else st, en))
            self.stats["chains_removed"]["prose"] += 1
            self.removed.append(re.sub(r"\s+", " ", text[st:en]).strip())
        merged = []
        for a, b in sorted(cuts):
            if merged and a < merged[-1][1]:
                merged[-1] = (merged[-1][0], max(b, merged[-1][1]))
            else:
                merged.append((a, b))
        for a, b in reversed(merged):
            text = text[:a] + CALL_OMITTED + text[b:]
        # 3. lines of code left in prose (before step 4, so a signature is still whole)
        out = []
        for ln in text.split("\n"):
            probe = SPAN.sub(lambda m: "x" * len(m.group(0)), ln)
            for ph in (SPAN_OMITTED, CALL_OMITTED, BLOCK_OMITTED):  # placeholders are not code
                probe = probe.replace(ph, "x" * len(ph))
            if code_line(probe):
                self.stats["prose_code_lines"] += 1
                self.removed_lines.append(ln.strip())
                out.append(re.match(r"^[ \t]*", ln).group(0) + BLOCK_OMITTED)
            else:
                out.append(ln)
        text = "\n".join(out)
        # 4. expressions written in plain prose (fix round 3): field accesses, calls, constructors,
        # fragments, struct literals (`prose_expressions`)
        masked = SPAN.sub(lambda m: "\0" * len(m.group(0)), text)
        masked = URL.sub(lambda m: "\0" * len(m.group(0)), masked)
        for ph in (SPAN_OMITTED, CALL_OMITTED, BLOCK_OMITTED):
            masked = masked.replace(ph, "\0" * len(ph))
        for a, b in reversed(prose_expressions(masked)):
            self.stats["prose_expressions"] += 1
            self.removed_prose.append(text[a:b])
            text = text[:a] + SPAN_OMITTED + text[b:]
        out = text.split("\n")
        return out

    def apply(self, md):
        out, prose = [], []

        def flush():
            if prose:
                out.extend(self.prose(prose))
                prose.clear()

        def block(indent_):
            self.stats["code_blocks"] += 1
            if out and out[-1].strip() == BLOCK_OMITTED and out[-1].startswith(indent_):
                return  # adjacent blocks become one placeholder
            out.append(indent_ + BLOCK_OMITTED)

        lines = md.split("\n")
        i, in_list, prev_blank = 0, False, True
        while i < len(lines):
            ln = lines[i]
            m = FENCE.match(ln)
            if m and not (m.group(2)[0] == "`" and "`" in m.group(3)):
                indent_, fence = m.group(1), m.group(2)
                j = i + 1
                close = re.compile(r"^[ \t]*" + re.escape(fence[0]) + "{" + str(len(fence)) + r",}[ \t]*$")
                while j < len(lines) and not close.match(lines[j]):
                    j += 1
                flush()
                block(indent_)
                i, prev_blank = j + 1, False
                continue
            if LIST_ITEM.match(ln):
                in_list = True
            elif ln.strip() and not ln.startswith((" ", "\t")):
                in_list = False
            if prev_blank and not in_list and re.match(r"^(?: {4}|\t)", ln) and ln.strip():
                j = i
                while j < len(lines) and (re.match(r"^(?: {4}|\t)", lines[j]) or not lines[j].strip()):
                    j += 1
                while j > i and not lines[j - 1].strip():
                    j -= 1
                flush()
                block("")
                i, prev_blank = j, False
                continue
            prose.append(ln)
            prev_blank = not ln.strip()
            i += 1
        flush()
        return "\n".join(out)


# ---------------------------------------------------------------- audits (fix rounds 1-3)

def _blocks(md):
    """[(start, end, indent, fence, body lines)] of the top-level fenced blocks."""
    out, lines, i = [], md.split("\n"), 0
    while i < len(lines):
        m = FENCE.match(lines[i])
        if not m or (m.group(2)[0] == "`" and "`" in m.group(3)):
            i += 1
            continue
        fence = m.group(2)
        j = i + 1
        close = re.compile(r"^[ \t]*" + re.escape(fence[0]) + "{" + str(len(fence)) + r",}[ \t]*$")
        while j < len(lines) and not close.match(lines[j]):
            j += 1
        out.append((i, j, m.group(1), fence, [x[len(m.group(1)):] if x.startswith(m.group(1)) else x.lstrip() for x in lines[i + 1:j]]))
        i = j + 1
    return out


def convert(md, how):
    """The same docs string with every fenced block rewritten: `rust`/`text`/`none` relabelled,
    `tilde` (~~~), `indent` (an indented block), `unfence` (the fence removed: code lines become
    prose), `inline` (each code line its own inline span), `joined` (the whole block one inline
    span on one line), `table` (each line an inline span in a table cell)."""
    lines = md.split("\n")
    out, last = [], 0
    for st, en, ind, fence, body in _blocks(md):
        out.extend(lines[last:st])
        last = en + 1
        if how in ("rust", "text", "none"):
            out += [ind + fence + {"rust": "rust", "text": "text", "none": ""}[how]] + [ind + x for x in body] + [ind + fence]
        elif how == "tilde":
            out += [ind + "~~~"] + [ind + x for x in body] + [ind + "~~~"]
        elif how == "indent":
            out += [""] + [(ind + "    " + x) if x.strip() else "" for x in body] + [""]
        elif how == "unfence":
            out += [""] + [ind + x for x in body] + [""]
        elif how == "inline":
            out += [""] + [ind + "`" + x.replace("`", "'") + "`" for x in body if x.strip()] + [""]
        elif how == "joined":
            out += ["", ind + "`" + " ".join(x.strip().replace("`", "'") for x in body if x.strip()) + "`", ""]
        elif how == "table":
            out += ["", "| example |", "|---|"] + ["| `" + x.strip().replace("`", "'").replace("|", "/") + "` |" for x in body if x.strip()] + [""]
    out.extend(lines[last:])
    return "\n".join(out)


CONVERSIONS = ("rust", "text", "none", "tilde", "indent", "unfence", "inline", "joined", "table")
BLOCK_FORMS = ("rust", "text", "none", "tilde", "indent")  # must give byte-identical output


CODE_LIKE = re.compile(r"[\w)\]](?:\.|::|\()")  # the audit's broad detector: `.`, `::` or `(` after a word or `)`
LINK_TARGET = re.compile(r"\]\([^)\s]*\)")
_PAIRS = {")": "(", "]": "[", "}": "{"}


def _audit_tok(t):
    """A token without the punctuation around it (trailing sentence marks, emphasis, quotes,
    unbalanced brackets)."""
    while t:
        if t[-1] in ".,;:!?*\"'":
            t = t[:-1]
        elif t[0] in "*\"'":
            t = t[1:]
        elif t[-1] in _PAIRS and t.count(_PAIRS[t[-1]]) < t.count(t[-1]):
            t = t[:-1]
        elif t[0] in "([{" and t.count(t[0]) > t.count({"(": ")", "[": "]", "{": "}"}[t[0]]):
            t = t[1:]
        elif len(t) > 2 and t[-1] in _PAIRS and t[0] == _PAIRS[t[-1]] and _balanced(t[1:-1]):
            t = t[1:-1]  # a token wrapped whole (`(see x)`, an intra-doc link `[x]`)
        else:
            break
    return t


def _balanced(t):
    depth = 0
    for c in t:
        depth += {"(": 1, "[": 1, "{": 1, ")": -1, "]": -1, "}": -1}.get(c, 0)
        if depth < 0:
            return False
    return depth == 0


_BG = r"[ \t]*\n?[ \t]*"
BROAD_WS = [  # the detector's own whitespace rules (fix round 4), written apart from `ws_normalize`
    (re.compile(_BG + r"::" + _BG), "::"),                                       # around `::`
    (re.compile(r"(?<=[\w)\]?])(?:[ \t]+\n?|\n)[ \t]*\." + _BG + r"(?=[A-Za-z_])"), "."),  # around a dot with space before it
    (re.compile(r"\(" + _BG), "("), (re.compile(_BG + r"\)"), ")"),             # inside parentheses
    (re.compile(r"\[" + _BG), "["), (re.compile(_BG + r"\]"), "]"),             # inside brackets
    (re.compile(r"(?<=[\w)\]])[ \t]+(?=\[(?:-?\d+|[a-z_]\w*|\"[^\"\n]*\"|\d*\.\.=?\d*)\](?![(\[:]))"), ""),  # an index, one line
    (re.compile(r"(?<=\))" + _BG + r"(?=\?)"), ""),                               # `) ?`
    (re.compile(r"(?<=\w)[ \t]+(?=\([^\s(),\0]+(?:,[ \t]*[^\s(),\0]+)*\))"), ""),  # a call on one line: one item or a list
]


def broad_ws(text):
    """`text` with the whitespace inside code-like tokens removed, by the detector's own rules
    (`BROAD_WS`): around `::`; around a dot that has whitespace (or a line break) before it and a
    name after it (a dot with space only after it is sentence punctuation, and joining it would
    make tokens of sentence boundaries); inside parentheses and brackets; before an index; before
    `?` after `)`; on one line, before parentheses that hold one item or a list of single items
    (`make (http, 3)`; a parenthetical with words, or an empty one, stays apart). Broader than the
    filter's `ws_normalize` before `(`, written apart from it. Placeholders are separators that
    join nothing."""
    for rx, rep_ in BROAD_WS:
        text = rx.sub(rep_, text)
    return text


def broad_tokens(text):
    """The audit's own detector (fix rounds 3-4), deliberately broad and independent of the filter's
    rules (`bare`, `prose_expressions`, `code_line`, chains, `ws_normalize`): the set of tokens
    holding `.`, `::` or `(` after a word character or `)`, read after the detector's own
    whitespace normalization (`broad_ws`: `value . count` is `value.count`), backticks dropped (a
    span reads as its text), table bars, slashes and whitespace as separators, link targets and
    placeholders dropped. It flags
    hosts, file names and type paths as well; the audits compare sets of these tokens between
    forms, so what the original's output shows is allowed and anything else is a leak. The same
    detector annotates the added lines of the structural gate's review diff (`diff_highlights`)."""
    for ph in (SPAN_OMITTED, CALL_OMITTED, BLOCK_OMITTED):
        text = text.replace(ph, "\0")  # a separator that joins nothing
    text = broad_ws(LINK_TARGET.sub("]", text).replace("`", ""))  # (a span reads as its text)
    toks = (_audit_tok(t) for t in re.split(r"[\s|/\0]+", text))
    return {t for t in toks if CODE_LIKE.search(t)}


def _prose_lines(md):
    lines, keep = md.split("\n"), set()
    for st, en, *_ in _blocks(md):
        keep |= set(range(st, en + 1))
    return lines, keep


def unquote(md):
    """Inline -> prose: every inline code span unwrapped into plain prose."""
    lines, keep = _prose_lines(md)
    return "\n".join(ln if i in keep else SPAN.sub(lambda m: m.group(2), ln) for i, ln in enumerate(lines))


def _quote_line(ln):
    parts = re.split(r"(`+[^`\n]+?`+)", ln)
    for k, part in enumerate(parts):
        if part.startswith("`"):
            continue
        out, pos = [], 0
        for m in re.finditer(r"\S+", part):
            tok = m.group(0)
            if LINK_TARGET.search(tok) or not CODE_LIKE.search(tok):
                continue
            st, en = m.start(), m.end()
            core = _audit_tok(tok)
            if not CODE_LIKE.search(core):
                continue
            st += tok.index(core)
            en = st + len(core)
            depth = part[st:en].count("(") - part[st:en].count(")")
            while depth > 0 and en < len(part):  # a call written with spaces: up to its closing paren
                depth += {"(": 1, ")": -1}.get(part[en], 0)
                en += 1
            if st < pos:
                continue
            out.append(part[pos:st] + "`" + part[st:en] + "`")
            pos = en
        parts[k] = "".join(out) + part[pos:]
    return "".join(parts)


def quote_prose(md):
    """Prose -> inline: every broadly code-like prose token (`CODE_LIKE`, a call extended to its
    closing parenthesis) wrapped into an inline code span."""
    lines, keep = _prose_lines(md)
    return "\n".join(ln if i in keep else _quote_line(ln) for i, ln in enumerate(lines))


def table_rows(md):
    """Prose/inline -> table cell: every prose line (outside blocks, headings and existing tables)
    moved into a one-column table row, spans and prose as written."""
    lines, keep = _prose_lines(md)
    out = []
    for i, ln in enumerate(lines):
        if i in keep or not ln.strip() or ln.lstrip().startswith(("#", "|")):
            out.append(ln)
        else:
            out += ["", "| text |", "|---|", "| " + ln.strip().replace("|", "/") + " |", ""]
    return "\n".join(out)


def span_blocks(md):
    """Inline -> block: every inline code span moved into its own fenced block."""
    lines, keep = _prose_lines(md)
    out = []
    for i, ln in enumerate(lines):
        if i in keep or not SPAN.search(ln):
            out.append(ln)
            continue
        rest = SPAN.sub(SPAN_OMITTED, ln)  # the span's place keeps a placeholder
        out += [rest, ""] + [x for m in SPAN.finditer(ln) for x in ("```", m.group(2), "```")] + [""]
    return "\n".join(out)


AUDIT_FROM_PROSE = {"unquote": unquote, "quote_prose": quote_prose, "table_rows": table_rows,
                    "unquote_table": lambda md: table_rows(unquote(md)), "span_blocks": span_blocks}
SYMMETRIC = ("unquote", "quote_prose", "table_rows", "unquote_table")  # same tokens kept, in both directions


# synthetic positives: one snippet per kind of evidence, written once and rendered into every form
# (block forms, plain prose lines, a sentence, inline spans per line or joined, a sentence with a
# span, table cells plain or as spans); every identifier below occurs only inside an expression, so
# the audit's detector is a plain string search for them, independent of the filter's rules
SYNTHETIC_NAVS = [(("synth_scope", "synth_endpoint"), {"set_limit"})]
SYNTHETIC_MARKERS = ("synth_count", "synth_info", "synth_pages", "synth_items", "SynthClient", "make_synth", "SynthConfig",
                     "synth_token", "synth_scope", "synth_endpoint", "set_limit", "synth_flag", "SynthError", "synth_run",
                     "zqcount", "zqvalue")  # (no `_` in the last two: the former host-path exception needed none)
SYNTHETIC_SNIPPETS = {
    "field": ["value.synth_count"],
    "nested-field": ["value.synth_info.synth_pages"],
    "method-on-field": ["value.synth_items.len()"],
    "constructor": ["SynthClient::new(http)"],
    "free-call": ["make_synth(http, 3)"],
    "struct-literal": ["SynthConfig { synth_token: t }"],
    "closure-use": ["value.synth_items.iter().filter(|i| i.synth_flag).count()"],
    "binding": ["let n = value.synth_count;"],
    "chain": ["api.synth_scope(\"x\").synth_endpoint().set_limit(3).send()"],
    "construction+chain": ["let api = SynthClient::new(http);", "let page = api.synth_scope(\"x\").synth_endpoint().send().await?;"],
    "multi-line-chain": ["let page = api", "    .synth_scope(\"x\")", "    .synth_endpoint()", "    .send()", "    .await?;"],
    "index": ["value.synth_items[0].synth_flag"],
    "pattern": ["SynthError::Bad { synth_token, .. }"],
    "try-chain": ["make_synth(http)?.synth_scope(\"x\")"],
    "await": ["api.synth_endpoint().send().await?"],
    "fragment": [".synth_endpoint()"],
    "field-fragment": [".synth_count"],
    "field-list": ["value.synth_count or value.synth_info"],
    "comment": ["// read value.synth_count here"],
    "signature": ["async fn synth_run(http: Client) -> Result<SynthConfig, SynthError> {"],
}
# fix round 4: the reviewer's two cases (`value . count`, `value.count/2`) and their variants: spaces,
# tabs and line breaks inside the chain; `/`, `*`, `+` after a member; weak-TLD and host-like
# members. Each snippet is one unit, so it is rendered in the forms that keep it whole
# (`SYNTHETIC_UNIT_FORMS`: no span or table row per line, which would leave a bare name).
SYNTHETIC_UNITS = {
    "spaced-field": ["value . zqcount"],
    "tabbed-field": ["value\t.\tzqcount"],
    "space-before-dot": ["value .zqcount"],
    "space-after-dot": ["value. zqcount"],
    "break-before-dot": ["value", ".zqcount"],
    "break-after-dot": ["value.", "zqcount"],
    "broken-chain": ["value", "  .synth_info", "  .synth_pages"],
    "spaced-method": ["value . synth_items . len ( )"],
    "spaced-path-call": ["SynthClient :: new ( http )"],
    "spaced-free-call": ["make_synth ( http , 3 )"],
    "spaced-endpoint-chain": ["api . synth_scope ( \"x\" ) . synth_endpoint ( ) . send ( ) . await ?"],
    "spaced-index": ["value . synth_items [ 0 ] . synth_flag"],
    "divided": ["value.zqcount/2"],
    "divided-spaced": ["value.zqcount / 2"],
    "divided-spaced-field": ["value . zqcount / 2"],
    "multiplied": ["value.zqcount*2"],
    "multiplied-spaced": ["value.zqcount * 2"],
    "added": ["value.zqcount+1"],
    "added-spaced": ["value.zqcount + 1"],
    "subtracted": ["value.zqcount - 1"],
    "path-after-field": ["value.zqcount/v1"],
    "weak-tld-member": ["zqvalue.info"],
    "weak-tld-divided": ["zqvalue.info/2"],
    "host-like-divided": ["zqvalue.io / 2"],
    "host-like-multiplied": ["zqvalue.com*2"],
}
SYNTHETIC_UNIT_FORMS = ("rust", "text", "none", "tilde", "indent", "prose", "sentence", "inline-sentence", "joined",
                        "span-broken", "table-cell")
SYNTHETIC_FORMS = ("rust", "text", "none", "tilde", "indent", "prose", "sentence", "inline", "inline-sentence", "joined",
                   "table", "table-code")


def synthetic_form(lines, how):
    if how in ("rust", "text", "none"):
        return "\n".join(["```" + {"rust": "rust", "text": "text", "none": ""}[how]] + lines + ["```"])
    if how == "tilde":
        return "\n".join(["~~~"] + lines + ["~~~"])
    if how == "indent":
        return "Intro.\n\n" + "\n".join("    " + x for x in lines) + "\n\nOutro."
    if how == "prose":
        return "\n".join(x.strip() for x in lines)
    if how == "sentence":
        return "Then use " + " ".join(x.strip() for x in lines) + " to read it."
    if how == "inline":
        return "\n".join("`" + x.strip() + "`" for x in lines)
    if how == "inline-sentence":
        return "Then use `" + " ".join(x.strip() for x in lines) + "` to read it."
    if how == "joined":
        return "`" + " ".join(x.strip() for x in lines) + "`"
    if how == "table":
        return "| example |\n|---|\n" + "\n".join("| " + x.strip().replace("|", "/") + " |" for x in lines)
    if how == "table-code":
        return "| example |\n|---|\n" + "\n".join("| `" + x.strip().replace("|", "/") + "` |" for x in lines)
    if how == "span-broken":  # one span whose text keeps the line breaks (CommonMark joins them)
        return "Then use `" + "\n".join(lines) + "` to read it."
    if how == "table-cell":  # the whole snippet in one cell
        return "| example |\n|---|\n| " + " ".join(x.strip() for x in lines).replace("|", "/") + " |"
    raise ValueError(how)


def format_audit(docs_strings, navs):
    """Failures of the format audits (fix rounds 1-4). The positives are detected by `broad_tokens`
    (its own whitespace rules, `broad_ws`) and plain marker searches, never by the filter's rules:
    (1) every block form (relabelled `rust`/`text`/none, `~~~`, indented) of every docs string
        filters to byte-identical text;
    (2) block -> prose/inline/table (`CONVERSIONS`: fence removed, one span per line, one joined
        span, table cells) and inline -> block (`span_blocks`): the filtered output shows no
        code-like token (`broad_tokens`) that the original's filtered output does not show;
    (3) inline <-> prose <-> table cell (`unquote`, `quote_prose`, `table_rows`, `unquote_table`):
        the filtered output shows exactly the original's code-like tokens, none more (a leak) and
        none fewer (the same text kept or removed whatever its form);
    (4) synthetic positives (`SYNTHETIC_SNIPPETS`, written once and rendered in every
        `SYNTHETIC_FORMS` form: blocks, prose lines, a sentence, spans per line, a sentence with a
        span, one joined span, table cells plain or as spans), and `SYNTHETIC_UNITS` (fix round 4:
        the reviewer's `value . count` and `value.count/2` with spaces, tabs and line breaks inside
        the chain and `/`, `*`, `+`, `-` after a member) in every `SYNTHETIC_UNIT_FORMS` form: the
        markers are present before filtering and absent after."""
    bad = []
    all_navs = list(navs) + SYNTHETIC_NAVS
    for k, md in list(docs_strings):
        ref = MarkdownFilter(navs).apply(md)
        allowed = broad_tokens(ref)
        forms = [(how, convert(md, how)) for how in CONVERSIONS] + [(how, f(md)) for how, f in AUDIT_FROM_PROSE.items()]
        for how, src in forms:
            got = MarkdownFilter(navs).apply(src)
            if how in BLOCK_FORMS and re.sub(r"\n{2,}", "\n\n", got).strip() != re.sub(r"\n{2,}", "\n\n", ref).strip():
                bad.append(f"{k}: {how}: output differs")
            seen = broad_tokens(got)
            if seen - allowed:
                bad.append(f"{k}: {how}: code-like tokens the original's output does not show {sorted(seen - allowed)[:3]}")
            if how in SYMMETRIC and allowed - seen:
                bad.append(f"{k}: {how}: code-like tokens of the original's output removed {sorted(allowed - seen)[:3]}")
    cases = [(k, lines, how) for k, lines in SYNTHETIC_SNIPPETS.items() for how in SYNTHETIC_FORMS]
    cases += [(k, lines, how) for k, lines in SYNTHETIC_UNITS.items() for how in SYNTHETIC_UNIT_FORMS]
    for k, lines, how in cases:
        src = synthetic_form(lines, how)
        if not any(mk in src for mk in SYNTHETIC_MARKERS):
            bad.append(f"synthetic {k}: {how}: positive lost before filtering")
        got = MarkdownFilter(all_navs).apply(src)
        left = [mk for mk in SYNTHETIC_MARKERS if mk in got]
        if left:
            bad.append(f"synthetic {k}: {how}: survives filtering {left}")
    return bad


_CONTROLS = {}


def _benign_dotted_round3(tok, after=""):
    """The round-3 exception, kept only as a negative control: any `x.y` without `_` followed by `/`
    was a host path, and every TLD counted for two labels."""
    low = tok.lower().rstrip(".")
    if low in ABBREVIATIONS:
        return True
    last, host = low.rsplit(".", 1)[-1], "_" not in tok
    named = all(seg[:1].isupper() for seg in tok.lstrip(".").split(".")[1:])
    return (last in TLDS and host) or bool(re.fullmatch(FILE_EXT, last)) or (after.startswith("/") and host) or named


def audit_controls():
    """Negative controls of the synthetic audit (fix rounds 3-4): with each filter rule switched off
    in turn (the prose rule, the span rule, the code-line rule, the whitespace normalization, and
    the host rule replaced by round 3's host-path exception), the audit must report failures; a
    count of 0 would mean the audit cannot see that rule's positives. Computed once per process."""
    if not _CONTROLS:
        g = globals()
        for name, off in (("prose_expressions", lambda t: []), ("bare", lambda t: True), ("code_line", lambda ln: False),
                          ("ws_normalize", lambda t, broad=False: (t, list(range(len(t))))),
                          ("benign_dotted", _benign_dotted_round3)):
            keep = g[name]
            g[name] = off
            try:
                _CONTROLS[name + " off"] = len(format_audit([], []))
            finally:
                g[name] = keep
    return dict(_CONTROLS)


# ---------------------------------------------------------------- structural gate (fix rounds 4-6)
#
# The coordinator's decisions (fix rounds 5-6): the gate binds to the exact materials, with no
# normalisation. Per SDK, the clearance digest is the SHA-256 of the complete candidate-arm materials
# exactly as they go into the reader's prompt (`reader_materials`: the public types and the rendered
# docs with the README part, as `runner.build_prompt` inserts them); an overall digest covers every
# SDK. `runner.py prompts` and `sessions` refuse unless a clearance file names exactly those digests
# for every SDK whose candidate materials differ from the baseline arm's in any byte (a deletion, a
# move, whitespace); an SDK whose materials are byte-identical needs none. The review evidence is a
# full unified diff (`review_diff`: no normalisation, deletions and both ends of a move visible;
# `apply_diff` reconstructs the candidate from it), with the broad detector's tokens on added lines
# as a separate annotation (`diff_highlights`).

HIGHLIGHT = ("«", "»")  # « » around a code-like token of the broad detector (an annotation only)
DIFF_CONTEXT = 5


def reader_materials(types, docs):
    """The exact SDK materials of one arm: the two texts `build_prompt` inserts (callers pass them
    as inserted, `types.strip()` and `docs.strip()`; nothing is changed here, so a stored input
    altered in any byte changes the digest), framed with their lengths so that no boundary is
    ambiguous."""
    return f"=== public types ({len(types)} characters)\n{types}\n=== documentation ({len(docs)} characters)\n{docs}\n"


def materials_digest(text):
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def review_diff(baseline, candidate, name):
    """A complete unified diff of two materials texts (`DIFF_CONTEXT` lines of context; difflib
    without its junk heuristic; no normalisation of any kind). Empty when they are identical."""
    import difflib
    a, b = baseline.splitlines(keepends=True), candidate.splitlines(keepends=True)
    if a == b:
        return ""
    out = [f"--- baseline/{name}\n", f"+++ candidate/{name}\n"]
    sm = difflib.SequenceMatcher(None, a, b, autojunk=False)
    for group in sm.get_grouped_opcodes(DIFF_CONTEXT):
        i1, i2, j1, j2 = group[0][1], group[-1][2], group[0][3], group[-1][4]
        out.append(f"@@ -{i1 + 1},{i2 - i1} +{j1 + 1},{j2 - j1} @@\n")
        for op, a1, a2, b1, b2 in group:
            if op == "equal":
                out += [" " + x for x in a[a1:a2]]
                continue
            out += ["-" + x for x in a[a1:a2]]
            out += ["+" + x for x in b[b1:b2]]
    return "".join(x if x.endswith("\n") else x + "\n\\ No newline at end of file\n" for x in out)


def apply_diff(baseline, diff):
    """The candidate text rebuilt from the baseline and `review_diff`'s output (a check that the
    diff is complete: every byte of the candidate is either unchanged context or a `+` line)."""
    if not diff:
        return baseline
    a = baseline.splitlines(keepends=True)
    out, pos, lines, k = [], 0, diff.splitlines(keepends=True), 2
    while k < len(lines):
        m = re.match(r"@@ -(\d+),(\d+) \+(\d+),(\d+) @@", lines[k])
        if not m:
            raise ValueError(f"not a hunk header: {lines[k]!r}")
        start = int(m.group(1)) - 1
        out += a[pos:start]
        pos = start
        k += 1
        while k < len(lines) and not lines[k].startswith("@@"):
            ln = lines[k]
            nxt = lines[k + 1] if k + 1 < len(lines) else ""
            body = ln[1:]
            if nxt.startswith("\\ No newline"):
                body = body[:-1]
                k += 1
            if ln[0] == " ":
                if a[pos] != body:
                    raise ValueError("context does not match the baseline")
                out.append(body)
                pos += 1
            elif ln[0] == "-":
                if a[pos] != body:
                    raise ValueError("a removed line does not match the baseline")
                pos += 1
            elif ln[0] == "+":
                out.append(body)
            k += 1
    out += a[pos:]
    return "".join(out)


def diff_highlights(diff):
    """The annotation of a review diff: for every added line holding code-like tokens of the
    broad detector (`broad_tokens`), its candidate line number and the tokens in « »."""
    notes, cand_line = [], 0
    for ln in diff.splitlines():
        m = re.match(r"@@ -\d+,\d+ \+(\d+),\d+ @@", ln)
        if m:
            cand_line = int(m.group(1)) - 1
            continue
        if ln.startswith(("---", "+++", "\\")):
            continue
        if ln.startswith((" ", "+")):
            cand_line += 1
        if ln.startswith("+"):
            toks = sorted(broad_tokens(ln[1:]))
            if toks:
                notes.append(f"candidate line {cand_line}: " + " ".join(HIGHLIGHT[0] + t + HIGHLIGHT[1] for t in toks))
    return "".join(x + "\n" for x in notes)


def gate_digests(baseline, candidate, context=None):
    """({sdk: SHA-256 of the candidate's exact materials}, [SDKs whose candidate materials differ
    from the baseline's in any byte], overall digest: the SHA-256 of the sorted `<sdk> <digest>`
    lines of every SDK, then the sorted `<name> <value>` lines of `context` (fix round 7: the
    runner binds the bank hash, the template, the cheat sheet and every final prompt's SHA-256))."""
    per = {sdk: materials_digest(t) for sdk, t in candidate.items()}
    changed = sorted(sdk for sdk in candidate if candidate[sdk] != baseline[sdk])
    lines = "".join(f"{k} {per[k]}\n" for k in sorted(per)) + "".join(f"{k} {v}\n" for k, v in sorted((context or {}).items()))
    overall = hashlib.sha256(lines.encode()).hexdigest()
    return per, changed, overall


def read_digest_clearance(path):
    """A clearance file: `overall <digest>` and `<sdk> <digest>` lines (blank lines and `#`
    comments ignored), as {name: digest}."""
    out = {}
    with open(path, encoding="utf-8") as f:
        for ln in f:
            ln = ln.strip()
            if not ln or ln.startswith("#"):
                continue
            parts = ln.split()
            if len(parts) != 2:
                raise ValueError(f"{path}: not `<name> <digest>`: {ln!r}")
            out[parts[0]] = parts[1]
    return out


def digest_gate(baseline, candidate, clearance, context=None, reasons=()):
    """Problems (empty: the prompts may be built or run), given {sdk: materials text} per arm.
    When any SDK's candidate materials differ from the baseline's, or `reasons` name another
    deviation (fix round 8: a template, a cheat sheet or stored materials that differ from the
    approved digests or a fresh render), the clearance must name the overall digest and the
    exact-materials digest of every SDK whose arms differ; a digest it names for any SDK must be
    that SDK's current one. With byte-identical arms and no deviation none is needed."""
    per, changed, overall = gate_digests(baseline, candidate, context)
    if not changed and not reasons:
        return []
    if clearance is None:
        why = (["the candidate's materials differ from the baseline's (" + ", ".join(changed) + ")"] if changed else []) + list(reasons)
        return ["; ".join(why) + " and no clearance file was given (--cleared FILE)"]
    problems = []
    if clearance.get("overall") != overall:
        problems.append("the clearance does not name the overall digest of these materials")
    for sdk in sorted(per):
        if sdk in changed and clearance.get(sdk) != per[sdk]:
            problems.append(f"{sdk}: the clearance does not name the digest of this SDK's candidate materials")
        elif sdk not in changed and sdk in clearance and clearance[sdk] != per[sdk]:
            problems.append(f"{sdk}: the clearance names a digest that is not this SDK's materials")
    problems += [f"{k}: not an SDK of this run" for k in clearance if k != "overall" and k not in per]
    return problems


def _clear(baseline, candidate):
    per, changed, overall = gate_digests(baseline, candidate)
    return {"overall": overall, **{k: per[k] for k in changed}}


GATE_BASE_TYPES = "pub struct Alpha\npub struct Beta\npub fn run()"
GATE_BASE_DOCS = (
    "# Synth SDK\n\nThe client `SynthClient` sends `GET /v1/items`.\n\n## Lists\n\n- first item\n  - nested item\n\n"
    "| key | value |\n|---|---|\n| a | 1 |\n| b | 2 |\n\n"
    "pub struct Alpha\n    Alpha one. Alpha two. Alpha three.\n\npub struct Beta\n\n## Errors\n\nFailures are `Error::Http` values.")


GATE_EXTRA = [  # appended to a candidate's docs by `runner.gate_e2e` (rendered by the filter first)
    "Every failure converts into `concord::Error`.",       # a type path
    "For display, compute zq_total (value) first.",        # an expression the filter keeps
    "Read value . zqcount and value.zqcount/2 here.",      # round 4's cases: removed by the filter
    "Read value /* */ . /* */ zq_count here.",             # round 5: comment-separated tokens
    "value\n\n.\n\nzq_count",                              # round 5: blank lines between tokens
]


def _render_strings(strings):
    f = MarkdownFilter([])
    return "\n\n".join(f.apply(x) for x in strings)


def gate_selftest():
    """(ok, checks) of the exact-materials gate on synthetic materials: a three-sentence doc block
    moved from one item to another (both ends in the diff), a deleted heading, a whitespace-only
    list-indent change, a table-boundary change, the round-5 comment-separated and blank-line
    additions: each needs a clearance and the diff shows it; a clearance naming the digests allows;
    an old clearance after a later deletion blocks; byte-identical arms need none. Every diff
    rebuilds its candidate exactly (`apply_diff`)."""
    checks = []

    def check(name, ok, got=None):
        checks.append({"check": name, "ok": bool(ok), "got": got})

    base = {"synth": reader_materials(GATE_BASE_TYPES, GATE_BASE_DOCS), "other": reader_materials("pub fn x()", "Other.")}
    block = "    Alpha one. Alpha two. Alpha three.\n"
    cases = {
        "move a doc block from Alpha to Beta": GATE_BASE_DOCS.replace(block, "").replace("pub struct Beta\n", "pub struct Beta\n" + block),
        "delete a heading": GATE_BASE_DOCS.replace("## Errors\n\n", ""),
        "whitespace-only list indent": GATE_BASE_DOCS.replace("  - nested item", "    - nested item"),
        "table boundary": GATE_BASE_DOCS.replace("| a | 1 |\n| b | 2 |", "| a | 1 | b | 2 |"),
        "comment-separated tokens": GATE_BASE_DOCS + "\n\nRead value /* */ . /* */ zq_count here.",
        "blank lines between tokens": GATE_BASE_DOCS + "\n\nvalue\n\n.\n\nzq_count",
    }
    for name, docs in cases.items():
        cand = {**base, "synth": reader_materials(GATE_BASE_TYPES, docs)}
        diff = review_diff(base["synth"], cand["synth"], "synth")
        rebuilt = apply_diff(base["synth"], diff) == cand["synth"]
        check(f"{name}: no clearance blocks", digest_gate(base, cand, None) != [], digest_gate(base, cand, None))
        check(f"{name}: a clearance naming the digests allows", digest_gate(base, cand, _clear(base, cand)) == [])
        check(f"{name}: the diff is complete (rebuilds the candidate)", rebuilt, diff)
        if name.startswith("move"):
            check("move: the diff shows both ends", ("-" + block) in diff and ("+" + block) in diff, diff)
        if name.startswith("delete"):
            check("delete: the diff shows the deleted heading", "-## Errors\n" in diff, diff)
        if name.startswith("whitespace"):
            check("indent: the diff shows both indents", "-  - nested item\n" in diff and "+    - nested item\n" in diff, diff)
        if name.startswith("table"):
            check("table: the diff shows the old rows and the new one", "-| a | 1 |\n" in diff and "+| a | 1 | b | 2 |\n" in diff, diff)
        if name.startswith("comment"):
            check("comment-separated: highlighted or shown", "zq_count" in diff, diff)
    first = {**base, "synth": reader_materials(GATE_BASE_TYPES, cases["comment-separated tokens"])}
    old = _clear(base, first)
    later = {**base, "synth": reader_materials(GATE_BASE_TYPES, cases["comment-separated tokens"].replace("## Lists\n\n", ""))}
    check("an old clearance after a later deletion blocks", digest_gate(base, later, old) != [], digest_gate(base, later, old))
    space = {**base, "synth": first["synth"].replace("Read value", "Read  value")}
    check("an old clearance after a whitespace change blocks", digest_gate(base, space, old) != [])
    check("a clearance missing the SDK digest blocks", digest_gate(base, first, {"overall": old["overall"]}) != [])
    check("a clearance with a wrong overall digest blocks", digest_gate(base, first, {**old, "overall": "0" * 64}) != [])
    check("byte-identical arms need no clearance", digest_gate(base, dict(base), None) == []
          and review_diff(base["synth"], base["synth"], "synth") == "")
    typed = {**base, "synth": reader_materials(GATE_BASE_TYPES, GATE_BASE_DOCS + "\n\nEvery failure converts into `concord::Error`.")}
    notes = diff_highlights(review_diff(base["synth"], typed["synth"], "synth"))
    check("highlights annotate an added code-like token", HIGHLIGHT[0] + "concord::Error" + HIGHLIGHT[1] in notes, notes)
    return all(c["ok"] for c in checks), checks


def selftest():
    """The materials self-tests: the synthetic format audit (no failure), its negative controls
    (each > 0) and the structural gate (`gate_selftest`)."""
    audit = format_audit([], [])
    controls = audit_controls()
    ok_b, back = gate_selftest()
    ok = not audit and all(controls.values()) and ok_b
    return ok, {"synthetic_audit_failures": audit, "controls": controls, "gate": back,
                "synthetic_cases": len(SYNTHETIC_SNIPPETS) * len(SYNTHETIC_FORMS) + len(SYNTHETIC_UNITS) * len(SYNTHETIC_UNIT_FORMS)}


CONTRACT_PATTERNS = {  # coverage audit: reader-visible contract statements, by category
    "urls": re.compile(r"https?://[^\s`'\")<>]+"),
    "request_lines": re.compile(r"\bRequest:\s"),
    "result_types": re.compile(r"Result<"),
    "base_url_statements": re.compile(r"with_base_url|base URL|origin", re.I),
    "retry_statements": re.compile(r"\bretr(?:y|ies|ied)\b|\battempts?\b|\bresen[dt]\b|\breplay", re.I),
    "error_variants": re.compile(r"\bError::[A-Z]\w*"),
}


def coverage(docs_text):
    """{category: sorted unique statement lines} of the filtered docs (a line per match)."""
    out = {}
    for k, pat in CONTRACT_PATTERNS.items():
        hits = set()
        for ln in docs_text.split("\n"):
            if pat.search(ln):
                hits.add(re.sub(r"\s+", " ", ln).strip())
        out[k] = sorted(hits)
    return out


def words(text):
    return len(re.findall(r"\S+", text))


# ---------------------------------------------------------------- rustdoc JSON walk

class Doc:
    def __init__(self, dj, crate):
        self.dj, self.idx, self.crate = dj, dj["index"], crate
        self.entries = []      # (sort key, kind, path, item)
        self.reexports = []    # (path text, docs)
        self.modules = []      # (path, docs)
        self.seen = set()

    def get(self, i):
        return self.idx.get(str(i))

    def walk(self, mod_item, prefix):
        for cid in mod_item["inner"]["module"].get("items", []):
            it = self.get(cid)
            if not it or P.hidden(it) or it.get("visibility") not in ("public", None):
                continue
            inner = it["inner"]
            if "use" in inner:
                u = inner["use"]
                tgt = self.get(u["id"]) if u.get("id") is not None else None
                if tgt is not None and tgt.get("crate_id") == 0:
                    if u.get("is_glob"):
                        if "module" in tgt["inner"]:
                            self.walk(tgt, prefix)
                        continue
                    self.add(tgt, prefix + [u["name"]], use_docs=it.get("docs"))
                else:
                    ext = self.dj.get("paths", {}).get(str(u.get("id"))) if u.get("id") is not None else None
                    real = P.clean_path("::".join(ext["path"])) if ext and ext.get("path") else None
                    line = (f"pub use {u['source']}" + ("::*" if u.get("is_glob") else "")
                            + (f" as {u['name']}" if not u.get("is_glob") and u["source"].split("::")[-1] != u["name"] else "") + ";")
                    if real and real.split("::")[0] != u["source"].split("::")[0]:
                        line += f"  // {real}"
                    self.reexports.append((line, it.get("docs")))
                continue
            if "module" in inner:
                self.modules.append(("::".join(prefix + [it["name"]]), it.get("docs")))
                self.walk(it, prefix + [it["name"]])
                continue
            self.add(it, prefix + [it["name"]])

    def add(self, it, path, use_docs=None):
        key = (it["id"], tuple(path))
        if key in self.seen:
            return
        self.seen.add(key)
        kind = next((k for k in ("struct", "enum", "union", "trait", "type_alias", "function", "constant",
                                 "static", "macro", "module") if k in it["inner"]), None)
        if kind == "module":
            self.modules.append(("::".join(path), it.get("docs")))
            self.walk(it, path)
            return
        if kind is None:
            return
        order = {"struct": 0, "enum": 1, "union": 2, "trait": 3, "type_alias": 4, "function": 5,
                 "constant": 6, "static": 7, "macro": 8}[kind]
        self.entries.append(((order, "::".join(path)), kind, "::".join(path), it, use_docs))


def attrs(it, must_use=True):
    """The attributes rustdoc shows in a declaration: `#[non_exhaustive]`, `#[must_use]` (with its
    reason; documentation text, so only in the docs rendering, not in the public types) and a
    deprecation notice; derive/serde/lint/inline attributes are not shown."""
    out = []
    for a in it.get("attrs") or []:
        if a == "non_exhaustive":
            out.append("#[non_exhaustive]")
        elif isinstance(a, dict) and "must_use" in a and must_use:
            r = (a["must_use"] or {}).get("reason")
            out.append(f"#[must_use = {json.dumps(r)}]" if r else "#[must_use]")
    dep = it.get("deprecation")
    if dep:
        out.append("#[deprecated" + (f"(note = {json.dumps(dep.get('note'))})" if dep.get("note") else "") + "]")
    return out


def _docs(it, use_docs=None, filt=None):
    parts = [d.strip() for d in (use_docs, it.get("docs")) if d and d.strip()]
    text = "\n\n".join(parts)
    if filt is not None and text:
        filt.sources.append((it.get("name") or "?", text))
        return filt.apply(text)
    return text


def _variant(doc, v):
    vi = v["inner"]["variant"]
    k = vi.get("kind")
    name = v["name"]
    if k == "plain" or k is None:
        return name
    if isinstance(k, dict) and "tuple" in k:
        tys = []
        for fid in k["tuple"]:
            f = doc.get(fid) if fid is not None else None
            tys.append(P.r_type(f["inner"]["struct_field"]) if f else "_")
        return f"{name}({', '.join(tys)})"
    if isinstance(k, dict) and "struct" in k:
        fs = []
        for fid in k["struct"].get("fields", []):
            f = doc.get(fid)
            if f:
                fs.append(f"{f['name']}: {P.r_type(f['inner']['struct_field'])}")
        return f"{name} {{ {', '.join(fs)} }}"
    return name


def render_item(doc, kind, path, it, use_docs, with_docs):
    """Lines for one item page."""
    inner = it["inner"][kind]
    name = path.split("::")[-1]
    L = []
    head = {"struct": "struct", "enum": "enum", "union": "union", "trait": "trait", "type_alias": "type",
            "constant": "const", "static": "static", "macro": "macro"}.get(kind)

    def docs_of(x, n, extra=None):
        if not with_docs:
            return
        d = _docs(x, extra, doc.filter)
        if d:
            L.append(P.indent(d, n))

    L.extend(attrs(it, with_docs))
    if kind == "function":
        L.append(f"pub {P.r_fn({**it, 'name': name})}")
        docs_of(it, 4, use_docs)
        return L
    if kind == "constant":
        L.append(f"pub const {name}: {P.r_type(inner.get('type'))}" + (f" = {inner['const'].get('expr')}" if isinstance(inner.get("const"), dict) and inner["const"].get("expr") else ""))
        docs_of(it, 4, use_docs)
        return L
    if kind == "static":
        L.append(f"pub static {name}: {P.r_type(inner.get('type'))}")
        docs_of(it, 4, use_docs)
        return L
    if kind == "macro":
        L.append(f"macro {name}!")
        docs_of(it, 4, use_docs)
        return L
    if kind == "type_alias":
        L.append(f"pub type {name}{P.r_gparams(inner.get('generics', {}))} = {P.r_type(inner.get('type'))}")
        docs_of(it, 4, use_docs)
        return L
    L.append(f"pub {head} {path}{P.r_gparams(inner.get('generics', {}))}{P.r_where(inner.get('generics', {}))}")
    docs_of(it, 4, use_docs)
    if kind == "struct":
        sk = inner.get("kind", {})
        if isinstance(sk, dict) and "plain" in sk:
            fields = [doc.get(f) for f in sk["plain"].get("fields", [])]
            pubf = [f for f in fields if f and f.get("visibility") == "public" and not P.hidden(f)]
            if pubf:
                L.append("    fields:")
                for f in pubf:
                    L.append(f"        pub {f['name']}: {P.r_type(f['inner']['struct_field'])}")
                    docs_of(f, 12)
            if sk["plain"].get("has_stripped_fields"):
                L.append("    (some fields are private)")
        elif isinstance(sk, dict) and "tuple" in sk:
            tys = []
            for fid in sk["tuple"]:
                f = doc.get(fid) if fid is not None else None
                tys.append(("pub " + P.r_type(f["inner"]["struct_field"])) if f else "_")
            L.append(f"    tuple: ({', '.join(tys)})")
    if kind == "enum":
        L.append("    variants:")
        for vid in inner.get("variants", []):
            v = doc.get(vid)
            if v and not P.hidden(v):
                L.extend("        " + x for x in attrs(v, with_docs))
                L.append(f"        {_variant(doc, v)}")
                docs_of(v, 12)
        if inner.get("has_stripped_variants"):
            L.append("        (some variants are hidden)")
    if kind == "trait":
        for mid in inner.get("items", []):
            m = doc.get(mid)
            if not m or P.hidden(m):
                continue
            if "function" in m["inner"]:
                L.extend("    " + x for x in attrs(m, with_docs))
                L.append("    " + P.r_fn(m) + ";")
                docs_of(m, 8)
            elif "assoc_type" in m["inner"]:
                L.append(f"    type {m['name']};")
                docs_of(m, 8)
        return L
    impls = [doc.get(i) for i in inner.get("impls", [])]
    impls = [im for im in impls if im]
    also = []
    for imp in impls:
        im = imp["inner"]["impl"]
        if im.get("is_synthetic") or im.get("blanket_impl") is not None:
            continue
        if im.get("trait") is None:
            fns = [doc.get(m) for m in im.get("items", [])]
            fns = [m for m in fns if m and not P.hidden(m) and m.get("visibility") == "public"]
            if not fns:
                continue
            L.append("    " + P.r_impl_header(imp) + " {")
            docs_of(imp, 8)
            for m in fns:
                if "function" in m["inner"]:
                    L.extend("        " + x for x in attrs(m, with_docs))
                    L.append("        pub " + P.r_fn(m) + ";")
                    docs_of(m, 12)
                elif "assoc_const" in m["inner"]:
                    ac = m["inner"]["assoc_const"]
                    L.append(f"        pub const {m['name']}: {P.r_type(ac.get('type'))};")
                    docs_of(m, 12)
            L.append("    }")
            continue
        assoc = []
        for mid in im.get("items", []):
            m = doc.get(mid)
            if m and "assoc_type" in m["inner"]:
                assoc.append(f"type {m['name']} = {P.r_type(m['inner']['assoc_type'].get('type'))};")
        if assoc:
            L.append("    " + P.r_impl_header(imp) + " { " + " ".join(assoc) + " }")
        else:
            also.append(P.clean_path(im["trait"]["path"]) + P.r_args(im["trait"].get("args")))
    if also:
        L.append("    also implements: " + ", ".join(sorted(set(also))))
    return L


def _norm(text):
    return re.sub(r"\s+", " ", text).strip()


def render(dj, crate, navs, readme=None, readme_name="README.md"):
    """(types text, docs text, stats) of one crate's rustdoc JSON, plus the package README when
    the crate page does not already include it (fix round 1)."""
    doc = Doc(dj, crate)
    doc.filter = MarkdownFilter(navs)
    root = doc.get(dj["root"])
    doc.walk(root, [])
    doc.entries.sort(key=lambda e: e[0])
    raw_crate = (root.get("docs") or "").strip()
    crate_docs = _docs(root, None, doc.filter)
    # the package README: the documented first stop; included once. When the crate page already
    # holds it (`#![doc = include_str!("../README.md")]`: its whitespace-normalized text occurs in
    # the crate docs) it is not repeated; otherwise it follows the crate page, filtered alike.
    readme_state = "absent"
    readme_text = ""
    if readme is not None and readme.strip():
        if _norm(readme) in _norm(raw_crate):
            readme_state = "included in the crate page"
        else:
            readme_state = "appended"
            doc.filter.sources.append((readme_name, readme))
            readme_text = doc.filter.apply(readme.strip())
    types, docs, per_item = [], [], {}
    section = None
    titles = {"struct": "Structs", "enum": "Enums", "union": "Unions", "trait": "Traits", "type_alias": "Type aliases",
              "function": "Functions", "constant": "Constants", "static": "Statics", "macro": "Macros"}
    docs.append(f"# Crate {crate}\n")
    if crate_docs:
        docs.append(crate_docs + "\n")
    if readme_text:
        docs.append(f"# Package README ({readme_name})\n")
        docs.append(readme_text + "\n")
    if doc.modules:
        docs.append("# Modules\n")
        for p, d in sorted(doc.modules):
            fd = doc.filter.apply(d.strip()) if d and d.strip() else ""
            if d and d.strip():
                doc.filter.sources.append((p, d))
            docs.append(f"pub mod {p}" + ("\n" + P.indent(fd, 4) if fd else ""))
            types.append(f"pub mod {p}")
        docs.append("")
    for (_, path), kind, path_, it, use_docs in doc.entries:
        if titles[kind] != section:
            section = titles[kind]
            docs.append(f"# {section}\n")
            types.append(f"# {section}")
        t = "\n".join(render_item(doc, kind, path_, it, use_docs, False))
        d = "\n".join(render_item(doc, kind, path_, it, use_docs, True))
        types.append(t)
        docs.append(d + "\n")
        per_item[path_] = words(d)
    if doc.reexports:
        docs.append("# Re-exports\n")
        types.append("# Re-exports")
        for line, d in sorted(doc.reexports, key=lambda x: x[0]):
            types.append(line)
            fd = doc.filter.apply(d.strip()) if d and d.strip() else ""
            docs.append(line + ("\n" + P.indent(fd, 4) if fd else ""))
    types_text = "\n".join(types).strip() + "\n"
    docs_text = re.sub(r"\n{3,}", "\n\n", "\n".join(docs)).strip() + "\n"
    endpoint_words = {}
    for (_, path), kind, path_, it, _ in doc.entries:
        if kind != "struct":
            continue
        for iid in it["inner"]["struct"].get("impls", []):
            imp = doc.get(iid)
            if not imp or imp["inner"]["impl"].get("trait") is not None:
                continue
            if any((doc.get(m) or {}).get("name") == "send" for m in imp["inner"]["impl"].get("items", [])):
                endpoint_words[path_] = per_item.get(path_, 0)
    audit_failures = format_audit(doc.filter.sources, navs)
    controls = audit_controls()
    audit_failures += [f"negative control sees nothing: {k}" for k, v in controls.items() if not v]
    cov = coverage(docs_text)
    stats = {"types_chars": len(types_text), "types_words": words(types_text),
             "docs_chars": len(docs_text), "docs_words": words(docs_text),
             "crate_page_words": words(crate_docs), "readme": readme_state, "readme_words": words(readme_text),
             "items": len(doc.entries), "reexports": len(doc.reexports),
             "filter": {**doc.filter.stats, "chains_removed_total": sum(doc.filter.stats["chains_removed"].values())},
             "chains_removed_text": sorted(set(doc.filter.removed)), "spans_removed_text": sorted(set(doc.filter.removed_spans)),
             "prose_lines_removed_text": sorted(set(doc.filter.removed_lines)),
             "prose_expressions_removed_text": sorted(set(doc.filter.removed_prose)),
             "format_invariant": not audit_failures, "format_audit_failures": audit_failures,
             "format_audit_controls": controls,
             "coverage_counts": {k: len(v) for k, v in cov.items()}, "coverage": cov,
             "words_per_endpoint_type": dict(sorted(endpoint_words.items())),
             "endpoint_types": len(endpoint_words),
             "mean_words_per_endpoint_type": round(sum(endpoint_words.values()) / len(endpoint_words), 1) if endpoint_words else None}
    return types_text, docs_text, stats


# ---------------------------------------------------------------- commit extraction and build

def full_sha(commit, repo=C.REPO):
    r = subprocess.run(["git", "-C", repo, "rev-parse", "--verify", commit + "^{commit}"], capture_output=True, text=True)
    if r.returncode:
        raise SystemExit(f"unknown commit {commit!r}: {r.stderr.strip()}")
    return r.stdout.strip()


def extract(sha, repo=C.REPO):
    """The commit's tree under `$CARGO_TARGET_DIR/h04-src/<sha12>/` (created once; a marker file
    records the full SHA and is checked)."""
    dest = os.path.join(C.CARGO_TARGET_DIR, "h04-src", sha[:12])
    marker = os.path.join(dest, ".h04-commit")
    if os.path.isfile(marker) and open(marker).read().strip() == sha:
        return dest
    if os.path.isdir(dest):
        shutil.rmtree(dest)
    os.makedirs(dest)
    arch = subprocess.Popen(["git", "-C", repo, "archive", sha], stdout=subprocess.PIPE)
    subprocess.run(["tar", "-x", "-C", dest], stdin=arch.stdout, check=True)
    if arch.wait():
        raise SystemExit(f"git archive {sha} failed")
    with open(marker, "w") as f:
        f.write(sha + "\n")
    return dest


RUSTDOC_TARGETS = os.path.join(C.CARGO_TARGET_DIR, "h04-target")  # one cargo target directory per commit (fix round 7)


def provenance_marker(key):
    """The rustdoc `--crate-version` that names the tree a JSON was built from (fix round 7): a
    commit's full SHA (or a control tree's key). Rendering never reads it."""
    return f"h04-tree-{key}"


def json_provenance_problems(rd_dir, key):
    """[] when every SDK's rustdoc JSON under `rd_dir` names `key` (`provenance_marker`), else
    what is missing or stale: a JSON from another tree, or built before fix round 7, is refused."""
    want, bad = provenance_marker(key), []
    for _, _, crate in SDKS.values():
        path = os.path.join(rd_dir, f"{crate}.json")
        if not os.path.isfile(path):
            bad.append(f"{crate}: no JSON")
            continue
        got = C.read_json(path).get("crate_version")
        if got != want:
            bad.append(f"{crate}: built from {got!r}, not {want!r}")
    return bad


def build_rustdoc_tree(src, key, out_dir, target_dir=None, provenance=True):
    """Public rustdoc JSON of the six SDKs of the tree `src` (identified by `key`: a commit's full
    SHA), copied to `<out_dir>/rustdoc/`; returns the build log dict.

    Fix round 7: two commits sharing one cargo target directory gave the first commit's JSON for
    the second (cargo hashes path packages relative to the workspace root, so both extracted trees
    map to the same units, and `git archive` sets file times to the commit time, older than the
    earlier build's fingerprints: the second tree looks fresh). Each key now builds in its own
    target directory (`RUSTDOC_TARGETS/<key[:12]>`), and rustdoc is given `--crate-version
    h04-tree-<key>` (in RUSTDOCFLAGS, so a different key also forces a rebuild): after the build,
    and when reusing a copied build, every JSON must name this key (`json_provenance_problems`),
    else the build is refused. `target_dir`/`provenance=False` exist only for the negative control
    (`render_control`)."""
    rd_dir = os.path.join(out_dir, "rustdoc")
    log_path = os.path.join(rd_dir, "build.json")
    if provenance and os.path.isfile(log_path):
        log = C.read_json(log_path)
        if log.get("commit") == key and log.get("exit") == 0 and not json_provenance_problems(rd_dir, key):
            return log
    target = target_dir or os.path.join(RUSTDOC_TARGETS, key[:12] if re.fullmatch(r"[0-9a-f]{40}", key) else key)
    env = C.cargo_env()
    env["CARGO_TARGET_DIR"] = target
    env["RUSTDOCFLAGS"] = "-Z unstable-options --output-format json" + (
        f" --crate-version {provenance_marker(key)}" if provenance else "")
    args = ["cargo", "+nightly", "doc", "--locked", "--offline", "--no-deps", "--lib", "--target", HOST]
    for _, pkg, _ in SDKS.values():
        args += ["-p", pkg]
    r = subprocess.run(args, cwd=src, env=env, capture_output=True, text=True)
    v = subprocess.run(["rustc", "+nightly", "--version"], capture_output=True, text=True, env=env)
    log = {"commit": key, "cmd": args, "exit": r.returncode, "toolchain": v.stdout.strip(),
           "cargo_target_dir": target, "rustdocflags": env["RUSTDOCFLAGS"],
           "provenance": provenance_marker(key) if provenance else None}
    os.makedirs(rd_dir, exist_ok=True)
    if r.returncode:
        log["stderr_tail"] = r.stderr[-4000:]
        C.write_json(log_path, log)
        raise SystemExit(f"rustdoc JSON build failed at {key}: {r.stderr[-1500:]}")
    for _, _, crate in SDKS.values():
        shutil.copy2(os.path.join(target, HOST, "doc", f"{crate}.json"), os.path.join(rd_dir, f"{crate}.json"))
    if provenance:
        bad = json_provenance_problems(rd_dir, key)
        if bad:
            log["exit"], log["provenance_problems"] = -1, bad
            C.write_json(log_path, log)
            raise SystemExit(f"stale rustdoc JSON for {key}: " + "; ".join(bad))
    C.write_json(log_path, log)
    return log


def build_rustdoc(sha, out_dir, repo=C.REPO):
    """`build_rustdoc_tree` of the commit's extracted tree (`extract`)."""
    return build_rustdoc_tree(extract(sha, repo), sha, out_dir)


def materials(commit, sdk, out, repo=C.REPO):
    """{"types": text, "docs": text, "meta": {...}} for `sdk` at `commit`; files under
    `<out>/<sha12>/`."""
    sha = full_sha(commit, repo)
    return materials_from_tree(extract(sha, repo), sha, sdk, os.path.join(out, sha[:12]))


def materials_from_tree(src, key, sdk, out_dir):
    """`materials` of an extracted tree identified by `key` (a commit's full SHA, or a control
    tree's key)."""
    log = build_rustdoc_tree(src, key, out_dir)
    sdk_dir, _, crate = SDKS[sdk]
    jpath = os.path.join(out_dir, "rustdoc", f"{crate}.json")
    dj = C.read_json(jpath)
    navs = C.endpoint_navs(C.Sdk(os.path.join(src, "examples", sdk_dir), src))
    readme_path = os.path.join(src, "examples", sdk_dir, "README.md")
    readme = open(readme_path, encoding="utf-8").read() if os.path.isfile(readme_path) else None
    types, docs, stats = render(dj, crate, navs, readme, f"examples/{sdk_dir}/README.md")
    meta = {"sdk": sdk, "commit": key, "crate": crate, "extraction": EXTRACTION,
            "rustdoc": {"format_version": dj.get("format_version"), "toolchain": log.get("toolchain"),
                        "json_sha256": C.sha256_file(jpath), "public_items_only": True,
                        "provenance": log.get("provenance")},
            "readme_sha256": C.sha256_file(readme_path) if readme is not None else None,
            "navigation_paths": len(navs),
            "types_sha256": hashlib.sha256(types.encode()).hexdigest(),
            "docs_sha256": hashlib.sha256(docs.encode()).hexdigest(), **stats}
    for name, text in ((f"{sdk}.types.txt", types), (f"{sdk}.docs.txt", docs)):
        with open(os.path.join(out_dir, name), "w", encoding="utf-8") as f:
            f.write(text)
    C.write_json(os.path.join(out_dir, f"{sdk}.meta.json"), meta)
    return {"types": types, "docs": docs, "meta": meta}


CONTROL_FILE = os.path.join("crates", "concord-macros", "src", "emit.rs")
CONTROL_OLD = "(the declared base URL; the client's `with_base_url` replaces it)."
CONTROL_NEW = "(the declared base URL; the client's `with_base_url` replaces it; h04 render control)."


def render_control(commit, out, repo=C.REPO):
    """The fix-round-7 control (cargo builds, one at a time). Tree A is `commit`'s; tree B is a
    copy of it differing only in one generated doc string (`CONTROL_FILE`: `CONTROL_OLD` ->
    `CONTROL_NEW`), its file time kept at the commit time as `git archive` sets it (the condition
    of the bug). Fixed pipeline: render A, then B, then A again: B must differ from A for every SDK
    whose A materials show the string, and A again must equal A byte for byte. Negative control:
    the pre-fix pipeline (one shared target directory, no provenance marker) builds A then B: B's
    JSON is A's (the bug), and the provenance check refuses it."""
    sha = full_sha(commit, repo)
    a_src = extract(sha, repo)
    b_src = os.path.join(C.CARGO_TARGET_DIR, "h04-src", f"ctl-{sha[:12]}")
    if os.path.isdir(b_src):
        shutil.rmtree(b_src)
    shutil.copytree(a_src, b_src, symlinks=True)
    path = os.path.join(b_src, CONTROL_FILE)
    st = os.stat(path)
    text = open(path, encoding="utf-8").read()
    if text.count(CONTROL_OLD) < 1:
        raise SystemExit(f"{CONTROL_FILE} at {sha[:12]} has no {CONTROL_OLD!r}")
    with open(path, "w", encoding="utf-8") as f:
        f.write(text.replace(CONTROL_OLD, CONTROL_NEW))
    os.utime(path, ns=(st.st_atime_ns, st.st_mtime_ns))  # the commit time, as extracted
    os.remove(os.path.join(b_src, ".h04-commit"))
    b_key = "ctl-" + hashlib.sha256(open(path, "rb").read()).hexdigest()[:12]
    res = {"commit": sha, "control_tree_key": b_key, "per_sdk": {}}
    runs = {}
    for name, src, key in (("A", a_src, sha), ("B", b_src, b_key), ("A again", a_src, sha)):
        d = os.path.join(out, name.replace(" ", "-"))
        runs[name] = {sdk: materials_from_tree(src, key, sdk, d) for sdk in sorted(SDKS)}
    ok = True
    for sdk in sorted(SDKS):
        a, b, a2 = runs["A"][sdk], runs["B"][sdk], runs["A again"][sdk]
        shows = "the declared base URL; the client's `with_base_url` replaces it" in a["docs"]
        differs = b["docs"] != a["docs"]
        same_again = (a2["docs"], a2["types"], json.dumps(a2["meta"], sort_keys=True)) == \
            (a["docs"], a["types"], json.dumps(a["meta"], sort_keys=True))
        res["per_sdk"][sdk] = {"a_shows_string": shows, "b_differs": differs,
                               "b_shows_change": "h04 render control" in b["docs"], "a_again_identical": same_again}
        ok = ok and same_again and (differs and "h04 render control" in b["docs"] if shows else True)
    ok = ok and any(v["b_differs"] for v in res["per_sdk"].values())
    shared = os.path.join(RUSTDOC_TARGETS, f"ctl-shared-{sha[:12]}")
    if os.path.isdir(shared):
        shutil.rmtree(shared)
    neg = {}
    for name, src, key in (("A", a_src, sha), ("B", b_src, b_key)):
        d = os.path.join(out, "pre-fix-" + name)
        build_rustdoc_tree(src, key, d, target_dir=shared, provenance=False)
        neg[name] = {c: C.sha256_file(os.path.join(d, "rustdoc", f"{c}.json")) for _, _, c in SDKS.values()}
    stale = neg["A"] == neg["B"]
    refused = bool(json_provenance_problems(os.path.join(out, "pre-fix-B", "rustdoc"), b_key))
    res["negative_control"] = {"pre_fix_b_json_equals_a": stale, "provenance_check_refuses_it": refused}
    res["ok"] = ok and refused
    return res


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--commit")
    ap.add_argument("--out")
    ap.add_argument("--selftest", action="store_true", help="synthetic audit, its negative controls and the structural gate")
    ap.add_argument("--render-control", action="store_true",
                    help="fix-round-7 control: --commit A and a copy differing in one generated doc string (cargo builds)")
    ap.add_argument("--sdk", nargs="*", choices=sorted(SDKS))
    ap.add_argument("--repo", default=C.REPO)
    a = ap.parse_args()
    if a.selftest:
        ok, res = selftest()
        print(json.dumps(res, indent=1))
        print("materials self-test:", "ok" if ok else "FAILED")
        sys.exit(0 if ok else 1)
    if not (a.commit and a.out):
        ap.error("--commit and --out are required (or --selftest)")
    if a.render_control:
        res = render_control(a.commit, os.path.abspath(a.out), a.repo)
        C.write_json(os.path.join(os.path.abspath(a.out), "render-control.json"), res)
        print(json.dumps(res, indent=1))
        sys.exit(0 if res["ok"] else 1)
    for s in a.sdk or sorted(SDKS):
        m = materials(a.commit, s, os.path.abspath(a.out), a.repo)["meta"]
        print(json.dumps({**{k: m[k] for k in ("sdk", "commit", "types_chars", "docs_chars", "docs_words",
                                               "crate_page_words", "readme", "endpoint_types", "mean_words_per_endpoint_type",
                                               "format_invariant", "coverage_counts")},
                          "chains_removed": m["filter"]["chains_removed"]}))


if __name__ == "__main__":
    main()
