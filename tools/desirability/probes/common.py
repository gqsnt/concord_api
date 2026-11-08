"""Shared pieces of the I3 reader probes: SDK loading, Rust item and type helpers, cargo plumbing.

Run Python with `-I`. Nothing here imports Concord code; it reads SDK source text and the
declaration tree from `../decl.py`.
"""
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import threading

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))  # tools/desirability: decl, static_scan

import decl  # noqa: E402
from static_scan import strip_strings_and_comments, match_brace, api_spans  # noqa: E402

REPO = os.path.realpath(os.path.join(HERE, "..", "..", ".."))
MAIN_BANK = os.path.join(HERE, "bank")
LOOP_ROOT = "/home/canarit/projects/Concord_Desirability_Loop"
# the instrument's own copy (neutral result example); the mission pack's prompts/probe_reader.md is the origin
DEFAULT_PROMPT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "probe_reader.md")
CARGO_TARGET_DIR = os.environ.get("CARGO_TARGET_DIR") or os.path.join(LOOP_ROOT, "work", "targets", "probe")

PRIM_INT = ("u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize")
PRIM_FLOAT = ("f32", "f64")
NONZERO = tuple("NonZero" + t[0].upper() + t[1:] for t in PRIM_INT)


# ---------------------------------------------------------------- SDK model

def sdk_dirs(repo=REPO):
    ex = os.path.join(repo, "examples")
    return sorted(os.path.join(ex, d) for d in os.listdir(ex)
                  if d.endswith("-sdk") and os.path.isfile(os.path.join(ex, d, "Cargo.toml")))


def short_name(sdk_dir):
    return os.path.basename(sdk_dir.rstrip("/"))[:-len("-sdk")]


def sdk_tag(short):
    return "".join(c for c in short.upper() if c.isalnum())


def cargo_toml(sdk_dir):
    return open(os.path.join(sdk_dir, "Cargo.toml"), encoding="utf-8").read()


def pkg_name(sdk_dir):
    m = re.search(r'(?m)^name\s*=\s*"([^"]+)"', cargo_toml(sdk_dir))
    return m.group(1)


def has_dep(sdk_dir, dep):
    text = cargo_toml(sdk_dir)
    deps = text.split("[dependencies]", 1)[1].split("\n[", 1)[0] if "[dependencies]" in text else ""
    return re.search(r"(?m)^" + re.escape(dep) + r"(\.workspace)?\s*=", deps) is not None


def dependencies(sdk_dir):
    """The crate names (`-` as `_`) of a package's `[dependencies]` (inline and `[dependencies.x]`)."""
    out, section = [], None
    for line in cargo_toml(sdk_dir).splitlines():
        line = line.strip()
        m = re.match(r"^\[([^\]]+)\]$", line)
        if m:
            section = m.group(1).strip()
            if section.startswith("dependencies."):
                out.append(section.split(".", 1)[1].strip().strip('"'))
            continue
        km = re.match(r'^"?([A-Za-z0-9_-]+)"?\s*=', line)
        if section == "dependencies" and km:
            out.append(km.group(1))
    return [d.replace("-", "_") for d in out]


def test_regions(clean):
    """(start, end) offsets of `#[cfg(test)] mod x { .. }` bodies (skipped when indexing items)."""
    out = []
    for m in re.finditer(r"#\[cfg\(test\)\]\s*(pub(\([^)]*\))?\s+)?mod\s+\w+\s*\{", clean):
        s = m.end() - 1
        out.append((m.start(), match_brace(clean, s)))
    return out


ITEM_RE = re.compile(r"(?m)^([ \t]*)(?:pub(?:\([^)]*\))?\s+)?(?:(?:const|async|unsafe|extern\s+\"C\")\s+)*"
                     r"(struct|enum|type|trait|const|static|fn|union|impl)\b")


class Sdk:
    """One `examples/<name>-sdk` package: files, declared clients, and an index of Rust items."""

    def __init__(self, sdk_dir, repo=REPO):
        self.dir = os.path.realpath(sdk_dir)
        self.repo = repo
        self.name = os.path.basename(self.dir)
        self.short = short_name(self.dir)
        self.tag = sdk_tag(self.short)
        self.pkg = pkg_name(self.dir)
        self.crate = self.pkg.replace("-", "_")
        self.files = {}
        for dp, dns, fs in os.walk(os.path.join(self.dir, "src")):
            dns.sort()
            for f in sorted(fs):
                if f.endswith(".rs"):
                    p = os.path.join(dp, f)
                    self.files[p] = open(p, encoding="utf-8").read()
        self.clients = []  # (client tree, file path)
        for p, text in self.files.items():
            for c in decl.parse_file(text):
                self.clients.append((c, p))
        self.modules = {os.path.splitext(os.path.basename(p))[0] for p in self.files} - {"lib", "mod", "main"}
        self._index()
        self.index = ModuleIndex(self)
        self._decl_modules = {}
        # crates the SDK uses (R2: `<crate>::..` paths are another crate's items): Concord and the
        # crates it re-exports, the [dependencies] of Cargo.toml, and the first segment of every
        # `use` path that is not the SDK itself or one of its modules
        local = {"crate", "self", "super", self.crate} | self.modules | {p[-1] for p in self.index.mods if p}
        self.crates = ({"concord", "reqwest", "bytes"} | set(dependencies(self.dir))
                       | {full.split("::")[0] for full in self.imports.values()} | self.reexports) - local - {"std", "core", "alloc"}

    def rel(self, path):
        return os.path.relpath(path, self.repo)

    def decl_module(self, rel_file):
        """The module path (a tuple; `()` is the crate root) of the first `api!` block in a source
        file given relative to the repository: the file's module, or the inline `mod` around it.
        R2 types are read in this module's scope."""
        if rel_file not in self._decl_modules:
            p = os.path.realpath(os.path.join(self.repo, rel_file))
            text = self.files.get(p)
            spans = api_spans(text, strip_strings_and_comments(text)) if text else []
            self._decl_modules[rel_file] = self.index.module_at(p, spans[0][0] if spans else 0)
        return self._decl_modules[rel_file]

    # -- items
    def _index(self):
        self.items = {}      # name -> [item dict]
        self.impls = []      # impl item dicts
        self.defaults = set()
        self.aliases = {}    # name -> (params [(name, default)], target text)
        self.imports = {}    # local name -> full path as imported (`use a::b::C as D` → D: a::b::C)
        self.reexports = set()  # crates or modules used under their own name (`pub use concord;`)
        for p, text in self.files.items():
            clean = strip_strings_and_comments(text)
            skip = test_regions(clean)
            for m in ITEM_RE.finditer(clean):
                pos = m.start(2)
                if any(a <= pos <= b for a, b in skip):
                    continue
                kind = m.group(2)
                indent = len(m.group(1).expandtabs())
                start = attr_start(text, m.start())
                end = item_end(clean, pos, kind)
                if end is None:
                    continue
                item = {"kind": kind, "file": p, "start": start, "end": end, "indent": indent,
                        "text": text[start:end].strip("\n"), "head": clean[pos:end]}
                if kind == "impl":
                    item["header"] = re.sub(r"\s+", " ", clean[pos:clean.find("{", pos) if "{" in clean[pos:end] else end]).strip()
                    self.impls.append(item)
                    hm = re.match(r"impl\s*(<.*?>)?\s*(?:(?:\w+::)*Default)\s+for\s+(\w+)", item["header"])
                    if hm:
                        self.defaults.add(hm.group(2))
                    continue
                nm = re.match(r"(?:struct|enum|type|trait|const|static|fn|union)\s+(r#)?(\w+)", clean[pos:])
                if not nm:
                    continue
                item["name"] = nm.group(2)
                derives = re.findall(r"#\[derive\(([^)]*)\)\]", text[start:pos])
                item["derives"] = sorted({d.strip().split("::")[-1] for ds in derives for d in ds.split(",") if d.strip()})
                if "Default" in item["derives"] and kind in ("struct", "enum"):
                    self.defaults.add(item["name"])
                if kind == "type" and indent == 0:
                    am = re.match(r"type\s+(\w+)\s*(<(.*?)>)?\s*=\s*(.*?);\s*$", re.sub(r"\s+", " ", text[pos:end]).strip())
                    if am:
                        params = []
                        for g in split_generics(am.group(3) or ""):
                            gm = re.match(r"(\w+)\s*(?::[^=]*)?(?:=\s*(.*))?$", g.strip())
                            if gm and not g.strip().startswith("'"):
                                params.append((gm.group(1), (gm.group(2) or "").strip() or None))
                        self.aliases[am.group(1)] = (params, am.group(4).strip())
                self.items.setdefault(item["name"], []).append(item)
            # every `use` / `pub use` outside test modules: `use concord::{X, Y as Z}`,
            # `use concord::reqwest::header::HeaderMap`, `pub use concord_pagination as pagination`
            for um in re.finditer(r"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?use\s+(?:::)?([^;]+);", clean):
                if any(a <= um.start() <= b for a, b in skip):
                    continue
                for local, full in use_names(um.group(1)):
                    if local != full:
                        self.imports.setdefault(local, full)
                    else:
                        self.reexports.add(local)

    def find(self, name, kinds=None):
        """The best definition of `name`: top-level first, then the shortest text."""
        cands = [i for i in self.items.get(name, []) if kinds is None or i["kind"] in kinds]
        if not cands:
            return None
        cands.sort(key=lambda i: (i["indent"], len(i["text"])))
        return cands[0]

    def defines_type(self, name):
        return self.find(name, ("struct", "enum", "type", "trait", "union")) is not None

    def has_default(self, type_text):
        t = parse_type(type_text)
        if t[0] == "path":
            last = t[1][-1]
            if last in self.defaults:
                return True
            if last in ("Option", "Vec", "String", "HashMap", "BTreeMap", "HashSet", "BTreeSet",
                        "VecDeque") or last in PRIM_INT + PRIM_FLOAT + ("bool",):
                return True
            if last in self.aliases:
                return self.has_default(self.aliases[last][1])
        if t[0] == "tuple" and not t[1]:
            return True
        return False

    def resolve_alias(self, type_text, depth=0):
        """Expand SDK type aliases at the top of `type_text` (generics substituted)."""
        t = parse_type(type_text)
        if depth > 8 or t[0] != "path" or len(t[1]) != 1 or t[1][0] not in self.aliases:
            return type_text
        params, target = self.aliases[t[1][0]]
        subst = {}
        for i, (pn, dflt) in enumerate(params):
            if i < len(t[2]):
                subst[pn] = t[2][i]
            elif dflt:
                subst[pn] = parse_type(dflt)
        return self.resolve_alias(type_str(substitute(parse_type(target), subst)), depth + 1)

    def sendwith_impls(self):
        """[(generic names, trait arg type, self type text, Output text, in_macro)] of
        `impl<..> SendWith<A> for P { type Output = O; }` in the SDK source."""
        out = []
        for im in self.impls:
            h = im["header"]
            m = re.match(r"impl\s*(<(.*)>)?\s*(?:[\w:]*::)?SendWith\s*<(.*)>\s+for\s+(.+?)(\s+where\b.*)?$", h)
            if not m:
                continue
            gens = [g.strip().split(":")[0].strip() for g in split_generics(m.group(2) or "")
                    if g.strip() and not g.strip().startswith("'")]
            body = im["text"]
            om = re.search(r"type\s+Output\s*=\s*([^;]+);", body)
            in_macro = "$" in h or "$" in (om.group(1) if om else "")
            out.append((gens, m.group(3).strip(), m.group(4).strip(), om.group(1).strip() if om else None,
                        in_macro))
        return out


def use_names(tree, prefix=""):
    """Names introduced by the body of a `use <tree>;`: [(local name, full path)].
    `self` in a group names the group's own path (`use concord::reqwest::{self, Url}` gives
    `reqwest` → `concord::reqwest`); globs introduce nothing."""
    tree = re.sub(r"\s+", " ", tree.strip())
    out = []
    if tree.startswith("{") and tree.endswith("}"):
        for part in split_generics(tree[1:-1]):
            out += use_names(part, prefix)
        return out
    if "::{" in tree:
        head, rest = tree.split("::{", 1)
        for part in split_generics(rest[:rest.rfind("}")]):
            out += use_names(part, prefix + head + "::")
        return out
    m = re.match(r"([\w:$]+?)(?:\s+as\s+(\w+))?$", tree)
    if m:
        full = prefix + m.group(1)
        if full.endswith("::self"):
            full = full[:-len("::self")]
        last = full.split("::")[-1]
        if last != "*" and last != "_" and m.group(2) != "_":
            out.append((m.group(2) or last, full))
    return out


def use_entries(tree, prefix=""):
    """Entries of the body of a `use <tree>;`: [(local name, full path, glob)]. `a::{b, c as d,
    e::*}` gives (b, a::b, False), (d, a::c, False), ("*", a::e, True); `self` in a group names
    the group's own path."""
    tree = re.sub(r"\s*::\s*", "::", re.sub(r"\s+", " ", tree.strip()))
    if tree.startswith("::"):
        tree = tree[2:]
    out = []
    if tree.startswith("{") and tree.endswith("}"):
        for part in split_generics(tree[1:-1]):
            out += use_entries(part, prefix)
        return out
    if "::{" in tree:
        head, rest = tree.split("::{", 1)
        for part in split_generics(rest[:rest.rfind("}")]):
            out += use_entries(part, prefix + head + "::")
        return out
    if tree == "*" or tree.endswith("::*"):
        full = (prefix + tree)[:-3] if tree != "*" else prefix[:-2]
        return [("*", full, True)] if full else []
    m = re.match(r"(?:r#)?([\w:$]+?)(?:\s+as\s+(?:r#)?(\w+))?$", tree)
    if m:
        full = prefix + m.group(1)
        if full.endswith("::self"):
            full = full[:-len("::self")]
        last = full.split("::")[-1]
        if last not in ("*", "_") and m.group(2) != "_":
            out.append((m.group(2) or last, full, False))
    return out


# ---------------------------------------------------------------- module index (R2 owners)

_DECL = re.compile(r"(?<![\w$#])(pub(?:\s*\([^()]*\))?\s+)?(?:unsafe\s+)?(mod|struct|enum|type|trait|union|use)\b"
                   r"\s*(?:r#)?(\w*)")
_CFG_TEST = re.compile(r"#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]")


def _brace_depths(clean):
    out, d = [0] * (len(clean) + 1), 0
    for i, ch in enumerate(clean):
        out[i] = d
        if ch == "{":
            d += 1
        elif ch == "}":
            d -= 1
    out[len(clean)] = d
    return out


class ModuleIndex:
    """The real item paths of one SDK crate, read from its source (README "Scoring", R2).
    `src/lib.rs` is `crate`; `mod x;` declared in it is `src/x.rs` or `src/x/mod.rs` (`crate::x`),
    nested likewise (`mod y;` in `src/x.rs` is `src/x/y.rs` or `src/x/y/mod.rs`; `#[path]` is
    followed); an inline `mod x { .. }` is a module too. Modules under `#[cfg(test)]` are left out.
    Per module: the types it defines at module level (struct, enum, union, trait, type alias, with
    their visibility), its child modules, and its `use` declarations (explicit names, `as`
    renames, globs; `pub` or not). Items generated by macros are not seen."""

    def __init__(self, sdk):
        self.sdk = sdk
        self.mods = {}
        self._clean, self._depth = {}, {}
        lib = os.path.join(sdk.dir, "src", "lib.rs")
        if lib in sdk.files:
            self._load((), lib, 0, None, 0, os.path.dirname(lib))

    def _file(self, path):
        if path not in self._clean:
            clean = strip_strings_and_comments(self.sdk.files[path])
            self._clean[path], self._depth[path] = clean, _brace_depths(clean)
        return self._clean[path], self._depth[path]

    def _load(self, mpath, file, lo, hi, base, child_dir):
        if mpath in self.mods:
            return
        text = self.sdk.files[file]
        clean, depth = self._file(file)
        hi = len(clean) if hi is None else hi
        m = {"file": file, "span": (lo, hi), "children": {}, "defs": {}, "uses": []}
        self.mods[mpath] = m
        for dm in _DECL.finditer(clean, lo, hi):
            p = dm.start()
            if depth[p] != base:  # inside a body: fn, impl, trait, macro call, nested module
                continue
            prev = max(clean.rfind(";", lo, p), clean.rfind("}", lo, p), clean.rfind("{", lo, p))
            if _CFG_TEST.search(clean, prev + 1, p):
                continue
            pub, kind, name = dm.group(1) is not None, dm.group(2), dm.group(3)
            if kind == "use":
                j = dm.end(2)
                while j < hi and not (clean[j] == ";" and depth[j] == base):
                    j += 1
                for local, full, glob in use_entries(clean[dm.end(2):j]):
                    m["uses"].append((local, full, pub, glob))
                continue
            if not name:
                continue
            if kind == "mod":
                k = dm.end()
                while k < hi and clean[k].isspace():
                    k += 1
                if k < hi and clean[k] == "{":
                    m["children"][name] = mpath + (name,)
                    self._load(mpath + (name,), file, k + 1, match_brace(clean, k), base + 1, os.path.join(child_dir, name))
                elif k < hi and clean[k] == ";":
                    pm = re.search(r'#\s*\[\s*path\s*=\s*"([^"]*)"\s*\]', text[prev + 1:p])
                    cands = ([os.path.normpath(os.path.join(os.path.dirname(file), pm.group(1)))] if pm else
                             [os.path.join(child_dir, name + ".rs"), os.path.join(child_dir, name, "mod.rs")])
                    f = next((c for c in cands if c in self.sdk.files), None)
                    if f:
                        stem = os.path.splitext(os.path.basename(f))[0]
                        sub = os.path.dirname(f) if stem == "mod" else os.path.join(os.path.dirname(f), stem)
                        m["children"][name] = mpath + (name,)
                        self._load(mpath + (name,), f, 0, None, 0, sub)
                continue
            if kind == "union" and not re.match(r"\s*[<{]", clean[dm.end():dm.end() + 40]):
                continue  # `union` is a keyword only before a definition
            item = {"kind": kind, "name": name, "pub": pub, "file": file, "pos": p}
            if kind == "type":
                end = item_end(clean, dm.start(2), "type")
                am = re.match(r"type\s+(?:r#)?(\w+)\s*(<(.*?)>)?\s*=\s*(.*?);\s*$",
                              re.sub(r"\s+", " ", clean[dm.start(2):end or hi]).strip())
                if not am:
                    continue  # `type X;` (no target) is not an alias
                params = []
                for g in split_generics(am.group(3) or ""):
                    gm = re.match(r"(\w+)\s*(?::[^=]*)?(?:=\s*(.*))?$", g.strip())
                    if gm and not g.strip().startswith("'"):
                        params.append((gm.group(1), (gm.group(2) or "").strip() or None))
                item["alias"] = (params, am.group(4).strip())
            m["defs"].setdefault(name, item)

    # -- queries
    def module_at(self, file, offset):
        """The innermost module of `file` whose span holds `offset` (`()` when none)."""
        best = ()
        for mp, m in self.mods.items():
            lo, hi = m["span"]
            if m["file"] == file and lo <= offset <= hi and (best == () or len(mp) > len(best)):
                best = mp
        return best

    def defs_named(self, name):
        """[(module path, item)] of every module-level type definition named `name`, the module
        closest to the root first."""
        return sorted(((mp, m["defs"][name]) for mp, m in self.mods.items() if name in m["defs"]),
                      key=lambda x: (len(x[0]), x[0]))

    @staticmethod
    def _visible(mpath, pub, viewer):
        # a private item or import of a module is visible in that module and its descendants
        return pub or tuple(viewer[:len(mpath)]) == mpath

    def lookup(self, mpath, name, viewer, seen=frozenset()):
        """What `name` names in module `mpath` seen from module `viewer`: ("def", module, item)
        for a type defined there, ("mod", path) for a module, ("ext", [segments]) for an item of
        another crate, or None. Explicit `use` names come before glob imports."""
        m, key = self.mods.get(mpath), (mpath, name)
        if m is None or key in seen:
            return None
        seen = seen | {key}
        d = m["defs"].get(name)
        if d and self._visible(mpath, d["pub"], viewer):
            return ("def", mpath, d)
        if name in m["children"]:
            return ("mod", m["children"][name])
        for local, full, pub, glob in m["uses"]:
            if not glob and local == name and self._visible(mpath, pub, viewer):
                return self.resolve_use(mpath, full.split("::"), seen)
        for local, full, pub, glob in m["uses"]:
            if glob and self._visible(mpath, pub, viewer):
                t = self.resolve_use(mpath, full.split("::"), seen)
                if t and t[0] == "mod":
                    r = self.lookup(t[1], name, mpath, seen)  # what the glob's importer sees there
                    if r:
                        return r
        return None

    def resolve_use(self, mpath, segs, seen=frozenset()):
        """A `use` path written in module `mpath` (Rust 2018: `crate::`, `self::`, `super::`, a
        child module, a name imported there, or another crate)."""
        segs = [s for s in segs if s]
        if not segs or mpath not in self.mods:
            return None
        head, rest = segs[0], segs[1:]
        m = self.mods[mpath]
        if head == "crate":
            start = ()
        elif head == "self":
            start = mpath
        elif head == "super":
            start = mpath[:-1] if mpath else None
        elif head in m["children"]:
            start = m["children"][head]
        else:
            imp = next((full for local, full, _, glob in m["uses"]
                        if not glob and local == head and full.split("::") != [head]), None)
            if imp is None or ("use", mpath, head) in seen:
                return ("ext", segs)  # another crate (`concord::Error`, `chrono::NaiveDate`)
            r = self.resolve_use(mpath, imp.split("::"), seen | {("use", mpath, head)})
            if r is None or r[0] == "def":
                return None
            if r[0] == "ext":
                return ("ext", list(r[1]) + rest)
            start = r[1]
        while rest and rest[0] == "super" and head in ("self", "super") and start:
            start, rest = start[:-1], rest[1:]
        if start is None:
            return None
        return self.walk(start, rest, mpath, seen)

    def walk(self, start, rest, viewer, seen=frozenset()):
        """Follow `rest` from module `start`: every segment but the last must name a module (a
        child, or a module or crate imported there), the last a type or module there."""
        cur = start
        for i, seg in enumerate(rest):
            r = self.lookup(cur, seg, viewer, seen)
            if r is None or i == len(rest) - 1:
                return r
            if r[0] == "mod":
                cur = r[1]
            elif r[0] == "ext":
                return ("ext", list(r[1]) + list(rest[i + 1:]))
            else:
                return None  # a type in the middle of a path (`Type::Assoc`) is not resolved
        return ("mod", cur)

    def module_head(self, head, ctx):
        """The module a leading path segment names: a child of the declaration's module `ctx`,
        else a crate-level module (as rustdoc paths spell it), else None."""
        for base in (ctx, ()):
            if base in self.mods and head in self.mods[base]["children"]:
                return self.mods[base]["children"][head]
        return None

    def resolve_path(self, segs, ctx):
        """An answer's qualified SDK path read from the declaration's module `ctx`: `crate::` or
        `<crate name>::` from the root, `self::`/`super::` from `ctx`, a leading module name as
        `module_head` finds it. Every segment but the last must be a real module, and the last
        name must be defined there or imported/re-exported there (private ones only where `ctx`
        can see them). Returns what `lookup` returns, or None."""
        segs = [s for s in segs if s]
        if len(segs) < 2:
            return None
        head, rest = segs[0], segs[1:]
        if head in ("crate", self.sdk.crate):
            start = ()
        elif head == "self":
            start = ctx
        elif head == "super":
            start = ctx[:-1] if ctx else None
        else:
            start = self.module_head(head, ctx)
        while rest and rest[0] == "super" and head in ("self", "super") and start:
            start, rest = start[:-1], rest[1:]
        if start is None or start not in self.mods or not rest:
            return None
        return self.walk(start, rest, ctx)

    @staticmethod
    def ident(r):
        """A comparable identity of a resolution result."""
        if r is None:
            return None
        if r[0] == "def":
            return ("def", r[1], r[2]["name"])
        return (r[0], tuple(r[1]))

    def spelling(self, target, head, ctx):
        """The shortest `<head>::..::Name` path (head `crate` or the crate name) that resolves from
        `ctx` to the same item as `target`, preferring one that also resolves from the crate root
        (public paths, as rustdoc shows them); None when there is none."""
        want, name = self.ident(target), target[2]["name"] if target[0] == "def" else target[1][-1]
        for viewer in ((), ctx):
            for mp in sorted(self.mods, key=lambda p: (len(p), p)):
                if self.ident(self.walk((), list(mp) + [name], viewer)) == want:
                    return [head] + list(mp) + [name]
        return None


def attr_start(text, pos):
    """Move `pos` (start of an item line) back over the attribute and doc-comment lines above it."""
    lines_start = pos
    while True:
        prev_end = text.rfind("\n", 0, lines_start)
        if prev_end < 0:
            return lines_start
        prev_start = text.rfind("\n", 0, prev_end) + 1
        line = text[prev_start:prev_end].strip()
        if line.startswith(("#[", "///")) or (line.endswith(")]") and not line.startswith("//")) \
                or (line.startswith(("#", ")]")) and not line.startswith("#!")):
            lines_start = prev_start
            continue
        return lines_start


def item_end(clean, pos, kind):
    """End offset of the item starting at `pos` (keyword) in comment/string-blanked text."""
    depth = 0
    i = pos
    n = len(clean)
    angle = kind not in ("const", "static")  # `1 << 20` in an initializer is not a generic
    while i < n:
        ch = clean[i]
        if ch in "([" or (angle and ch == "<"):
            depth += 1
        elif ch in ")]" or (angle and ch == ">" and clean[i - 1] not in "-="):
            depth -= 1
        elif ch == "{" and (depth <= 0 or kind in ("fn", "impl", "trait")):
            if kind in ("type", "const", "static"):
                # a braced initializer: skip it
                i = match_brace(clean, i) + 1
                continue
            e = match_brace(clean, i)
            return e + 1
        elif ch == ";" and depth <= 0:
            return i + 1
        i += 1
    return None


def split_generics(text, sep=","):
    parts, depth, cur = [], 0, []
    for i, ch in enumerate(text):
        if ch in "([{<":
            depth += 1
        elif ch in ")]}>" and not (ch == ">" and i > 0 and text[i - 1] in "-="):
            depth -= 1
        if ch == sep and depth == 0:
            parts.append("".join(cur))
            cur = []
        else:
            cur.append(ch)
    if "".join(cur).strip():
        parts.append("".join(cur))
    return [p.strip() for p in parts if p.strip()]


# ---------------------------------------------------------------- types

def parse_type(text):
    """A small Rust type parser: ("path", [segments], [args]) | ("ref", mut, inner) | ("tuple", [..])
    | ("slice", inner) | ("array", inner, len) | ("infer",) | ("raw", text). Heuristic and lenient
    (it is used on SDK declaration text, whose results the compile steps check); R2 answer and
    truth types are parsed by `syn` instead (`type_tree`), which repairs nothing."""
    s = re.sub(r"\s+", " ", (text or "").strip())
    s = s.strip("`").strip()
    if not s:
        return ("raw", "")
    try:
        node, rest = _parse(s, 0)
        if s[rest:].strip():
            return ("raw", s)
        return node
    except (IndexError, ValueError):
        return ("raw", s)


def _skip(s, i):
    while i < len(s) and s[i] == " ":
        i += 1
    return i


def _parse(s, i):
    i = _skip(s, i)
    if s.startswith("&", i):
        i = _skip(s, i + 1)
        lt = re.match(r"'\w+\s*", s[i:])
        if lt:
            i += lt.end()
        mut = False
        if s.startswith("mut ", i):
            mut, i = True, i + 4
        inner, i = _parse(s, i)
        return ("ref", mut, inner), i
    if s.startswith("(", i):
        i += 1
        elems = []
        while True:
            i = _skip(s, i)
            if s[i] == ")":
                i += 1
                break
            e, i = _parse(s, i)
            elems.append(e)
            i = _skip(s, i)
            if s[i] == ",":
                i += 1
        if len(elems) == 1 and not re.search(r",\s*\)$", s[:i]):
            return elems[0], i
        return ("tuple", elems), i
    if s.startswith("[", i):
        inner, i = _parse(s, i + 1)
        i = _skip(s, i)
        if s[i] == ";":
            j = s.index("]", i)
            return ("array", inner, s[i + 1:j].strip()), j + 1
        if s[i] != "]":
            raise ValueError(s)
        return ("slice", inner), i + 1
    if s.startswith("_", i) and not re.match(r"_\w", s[i:]):
        return ("infer",), i + 1
    m = re.match(r"(dyn|impl) ", s[i:])
    if m:
        # opaque: take the rest up to a top-level ',' '>' or ')'
        depth, j = 0, i
        while j < len(s):
            if s[j] in "<([":
                depth += 1
            elif s[j] in ">)]":
                if depth == 0:
                    break
                depth -= 1
            elif s[j] == "," and depth == 0:
                break
            j += 1
        return ("raw", s[i:j].strip()), j
    if s.startswith("<", i):
        # qualified path <A as B>::C
        depth, j = 0, i
        while j < len(s):
            if s[j] == "<":
                depth += 1
            elif s[j] == ">":
                depth -= 1
                if depth == 0:
                    break
            j += 1
        m2 = re.match(r">(::\w+)+", s[j:])
        end = j + (m2.end() if m2 else 1)
        return ("raw", s[i:end]), end
    segs = []
    args = []
    while True:
        m = re.match(r"(::)?\s*(r#)?([A-Za-z_$][\w$]*)", s[i:])
        if not m:
            raise ValueError(s[i:])
        if m.group(1) and not segs:
            segs.append("")
        segs.append(m.group(3))
        i += m.end()
        i = _skip(s, i)
        if s.startswith("::<", i):
            i += 2
        if s.startswith("<", i):
            i += 1
            args = []
            while True:
                i = _skip(s, i)
                if s[i] == ">":
                    i += 1
                    break
                lt = re.match(r"'\w+", s[i:])
                if lt:
                    i += lt.end()
                else:
                    am = re.match(r"(\w+)\s*=\s*", s[i:])
                    if am and not s[i:].startswith(am.group(1) + " =="):
                        # associated binding: keep as raw text
                        e, j = _parse(s, i + am.end())
                        args.append(("raw", am.group(1) + " = " + type_str(e)))
                        i = j
                    else:
                        e, i = _parse(s, i)
                        args.append(e)
                i = _skip(s, i)
                if s[i] == ",":
                    i += 1
            i = _skip(s, i)
        if s.startswith("::", i):
            i += 2
            continue
        break
    return ("path", segs, args), i


def type_str(t):
    k = t[0]
    if k == "path":
        p = "::".join(t[1])
        if t[2]:
            p += "<" + ", ".join(type_str(a) for a in t[2]) + ">"
        return p
    if k == "ref":
        return "&" + ("mut " if t[1] else "") + type_str(t[2])
    if k == "tuple":
        if len(t[1]) == 1:
            return "(" + type_str(t[1][0]) + ",)"
        return "(" + ", ".join(type_str(a) for a in t[1]) + ")"
    if k == "slice":
        return "[" + type_str(t[1]) + "]"
    if k == "array":
        return "[" + type_str(t[1]) + "; " + t[2] + "]"
    if k == "infer":
        return "_"
    return t[1]


def substitute(t, subst):
    """Replace single-segment paths named in `subst` (and `_` under key '_') by trees."""
    k = t[0]
    if k == "path":
        if len(t[1]) == 1 and not t[2] and t[1][0] in subst:
            return subst[t[1][0]]
        return ("path", t[1], [substitute(a, subst) for a in t[2]])
    if k == "ref":
        return ("ref", t[1], substitute(t[2], subst))
    if k == "tuple":
        return ("tuple", [substitute(a, subst) for a in t[1]])
    if k in ("slice",):
        return ("slice", substitute(t[1], subst))
    if k == "array":
        return ("array", substitute(t[1], subst), t[2])
    if k == "infer" and "_" in subst:
        return subst["_"]
    return t


def unify(pattern, target, gens, binding):
    """Match `pattern` (with generic names `gens`) against `target`; fills `binding`."""
    if pattern[0] == "path" and len(pattern[1]) == 1 and not pattern[2] and pattern[1][0] in gens:
        name = pattern[1][0]
        if name in binding:
            return type_str(binding[name]) == type_str(target)
        binding[name] = target
        return True
    if pattern[0] != target[0]:
        return False
    if pattern[0] == "path":
        if pattern[1][-1] != target[1][-1] or len(pattern[2]) != len(target[2]):
            return False
        return all(unify(a, b, gens, binding) for a, b in zip(pattern[2], target[2]))
    if pattern[0] == "ref":
        return pattern[1] == target[1] and unify(pattern[2], target[2], gens, binding)
    if pattern[0] == "tuple":
        return len(pattern[1]) == len(target[1]) and all(unify(a, b, gens, binding) for a, b in zip(pattern[1], target[1]))
    if pattern[0] == "slice":
        return unify(pattern[1], target[1], gens, binding)
    return type_str(pattern) == type_str(target)


# ---------------------------------------------------------------- R2 types: the `syn` parser

FORM_CHECK_DIR = os.path.join(HERE, "form-check")
FORM_CHECK_BIN = os.path.join(CARGO_TARGET_DIR, "release", "concord-probe-form-check")
_FORM_BUILT = []
INVALID = "invalid"  # ("invalid", text, why): a text that is not exactly one Rust type


def form_check_binary():
    """The form-check helper (`form-check/`, a small `syn` binary that is not a workspace member;
    R3 answer contract and R2 types), built once per process with `cargo build --offline
    --release` (a no-op when it is up to date)."""
    if not _FORM_BUILT:
        p = subprocess.run(["cargo", "build", "--offline", "--release", "--quiet", "--manifest-path",
                            os.path.join(FORM_CHECK_DIR, "Cargo.toml")], env=cargo_env(), capture_output=True, text=True)
        if p.returncode or not os.access(FORM_CHECK_BIN, os.X_OK):
            raise SystemExit(f"the form-check helper ({FORM_CHECK_DIR}) did not build:\n{p.stderr[-2000:]}")
        _FORM_BUILT.append(FORM_CHECK_BIN)
    return _FORM_BUILT[0]


class _TypeParser:
    """The form-check helper in `--type` mode, kept running: one hex-encoded type per line in, one
    JSON line out (`{"ok": tree}` or `{"err": why}`). A text the helper dies on is invalid; the
    helper is restarted for the next one."""

    def __init__(self):
        self.proc, self.lock, self.cache = None, threading.Lock(), {}

    def _ask(self, text):
        if self.proc is None or self.proc.poll() is not None:
            self.proc = subprocess.Popen([form_check_binary(), "--type"], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                         stderr=subprocess.DEVNULL, text=True, encoding="utf-8", errors="replace")
        try:
            # hex: newlines in the text cannot break the protocol; a lone surrogate reaches the
            # helper as invalid UTF-8, which it refuses
            self.proc.stdin.write(text.encode("utf-8", "surrogatepass").hex() + "\n")
            self.proc.stdin.flush()
            line = self.proc.stdout.readline()
        except OSError:
            line = ""
        if not line:
            try:
                self.proc.kill()
                self.proc.wait(timeout=10)
            except (OSError, subprocess.TimeoutExpired):
                pass
            self.proc = None
            return {"err": "the type parser failed on this text"}
        return json.loads(line)

    def verdict(self, text):
        with self.lock:
            if text not in self.cache:
                self.cache[text] = self._ask(text)
            return self.cache[text]


_TYPES = _TypeParser()


def _from_json(j):
    k = j["k"]
    if k == "path":  # lifetime arguments are left out (`Cow<'static, str>` is `Cow<str>`)
        return ("path", list(j["segs"]), [_from_json(a) for a in j["args"] if a["k"] != "lifetime"])
    if k == "ref":
        return ("ref", bool(j["mut"]), _from_json(j["elem"]))
    if k == "tuple":
        return ("tuple", [_from_json(e) for e in j["elems"]])
    if k == "slice":
        return ("slice", _from_json(j["elem"]))
    if k == "array":
        return ("array", _from_json(j["elem"]), j["len"])
    if k == "infer":
        return ("infer",)
    return ("raw", j["text"])


def type_tree(text):
    """The tree of one Rust type, with the node kinds of `parse_type`, parsed by `syn` (the
    form-check helper's `--type` mode; README "Scoring", R2): ("path", [segments], [args]) (a
    leading `::` is a first segment ""; generic arguments are the last segment's, lifetimes left
    out), ("ref", mut, inner) (lifetime left out), ("tuple", [..]), ("slice", inner), ("array",
    inner, len text), ("infer",), ("raw", token text) for what is compared as text (`dyn`/`impl`
    bounds, `<A as B>::C`, ..), and ("invalid", text, why) when the text is not exactly one Rust
    type (a missing comma, an extra `>`, a stray token). Nothing is repaired."""
    r = _TYPES.verdict(text)
    if "ok" in r:
        return _from_json(r["ok"])
    return (INVALID, text, r.get("err") or "")


def type_judge(text):
    """None when the text is exactly one Rust type that the R2 rustc identity check can judge;
    otherwise why not (README "Scoring", R2; fix round 6): it does not parse as one type, or it
    holds what the check's function body would infer or run (`_`, `'_`, a reference with an
    elided lifetime, `impl Trait`, a `dyn` lifetime bound other than `'static`, `Self`, a macro,
    an attribute, an expression other than an integer literal), or a qualified path `<T as
    Trait>::Name`, which would have rustc compute the type for the answer. Decided by the
    form-check helper's `--type` mode (`judge`)."""
    r = _TYPES.verdict(text)
    if "ok" not in r:
        return r.get("err") or "does not parse as a Rust type"
    return r.get("judge")


def has_invalid(t):
    """True when the tree (`type_tree` or `canon_ident`) holds an ("invalid", ..) node."""
    k = t[0]
    if k == INVALID:
        return True
    if k in ("path", "id"):
        return any(has_invalid(a) for a in t[2])
    if k == "ref":
        return has_invalid(t[2])
    if k == "tuple":
        return any(has_invalid(a) for a in t[1])
    if k in ("slice", "array"):
        return has_invalid(t[1])
    return False


# Identity tags of the R2 identity diagnostic (README "Scoring", R2; since fix round 6 a
# diagnostic, `R2.diag_identity.*`: rustc decides R2 credit): a type path canonicalizes to the
# definition it reaches, tagged by kind, never to a printable path; an answer cannot write an
# identity.
SDK_DEF = "sdk-def"        # ("sdk-def", <crate>, *module path, Name): a definition in the module index
SDK_ITEM = "sdk-item"      # ("sdk-item", <crate>, Name): an SDK item the index does not place (macro-made)
EXT = "ext-crate"          # ("ext-crate", <crate>, *path): an item of another crate the SDK uses, by path
STD = "std-lib"            # ("std-lib", "collections", Name): HashMap, BTreeMap (STD_PATHS)
PRELUDE = "std-prelude"    # ("std-prelude", Name): Result, Option, Vec, String, Box
PRIM = "primitive"         # ("primitive", name): bool, char, str, the integers and floats
UNRESOLVED = "unresolved"  # ("unresolved", "<path as written>"): equal to no resolved identity
IDENTITY_TAGS = (SDK_DEF, SDK_ITEM, EXT, STD, PRELUDE, PRIM, UNRESOLVED)
PRIMITIVES = frozenset(("bool", "char", "str") + PRIM_INT + PRIM_FLOAT)
PRELUDE_MODULES = ("v1", "rust_2015", "rust_2018", "rust_2021", "rust_2024")


def _std_paths():
    t = {}

    def add(identity, *paths):
        for p in paths:
            t[tuple(p.split("::"))] = identity

    for name, mod in (("Option", "option"), ("Result", "result")):  # defined in core, re-exported by std
        add((PRELUDE, name), f"std::{mod}::{name}", f"core::{mod}::{name}",
            *(f"{c}::prelude::{v}::{name}" for c in ("std", "core") for v in PRELUDE_MODULES))
    for name, mod in (("Vec", "vec"), ("String", "string"), ("Box", "boxed")):  # alloc, re-exported by std
        add((PRELUDE, name), f"std::{mod}::{name}", f"alloc::{mod}::{name}",
            *(f"std::prelude::{v}::{name}" for v in PRELUDE_MODULES))
    add((STD, "collections", "HashMap"), "std::collections::HashMap", "std::collections::hash_map::HashMap")
    add((STD, "collections", "BTreeMap"), "std::collections::BTreeMap", "std::collections::btree_map::BTreeMap",
        "alloc::collections::BTreeMap", "alloc::collections::btree_map::BTreeMap")
    for p in sorted(PRIMITIVES):
        add((PRIM, p), f"std::primitive::{p}", f"core::primitive::{p}")
    return t


# The real standard-library paths the R2 identity diagnostic accepts (README "Scoring", R2;
# review 6, finding 1; rustc decides R2 credit since fix round 6): the std
# types of the truth and their common spellings, with the re-exports (`std` re-exports `core` and
# `alloc`; the prelude modules), each mapped to its identity. A std/core/alloc path not listed is
# unresolved (`alloc::result::Result`, `core::vec::Vec`, `std::results::Result`), so an answer
# cannot invent a standard-library owner. `score.py --selftest` compiles every listed path.
STD_PATHS = _std_paths()


def canon_ident(text, sdk, ctx=(), _depth=0):
    """The R2 identity tree of a type (see README "Scoring", R2), read in the scope of the
    declaration's module `ctx` (`Sdk.decl_module`; `()` is the crate root). Since fix round 6 this
    is a diagnostic (`R2.diag_identity.*`), not credit: rustc compiles the identity check
    (score.py `r2_check_fn`). It drops lifetime arguments and an alias's surplus arguments (review
    8), which rustc refuses. Two types are the same answer only when their trees are equal. A path
    node is `("id", identity, args)`, where `identity` is a tagged tuple (`IDENTITY_TAGS`) and
    `args` the generic arguments' trees; the other nodes are `("ref", mut, inner)`, `("tuple",
    elems)`, `("slice", inner)`, `("array", inner, len)`, `("infer",)`, `("raw", token text)`
    (compared as text: `dyn ..`, `impl ..`, `<A as B>::C`) and `("invalid", text, why)`: the text
    is not exactly one Rust type.

    The text is parsed by `syn` (`type_tree`, the form-check helper), for answers and truth alike;
    a text that does not parse is invalid as a whole (a missing comma, an extra `>`, a stray
    token), never repaired. SDK alias targets are parsed the same way.

    A qualified SDK path (`crate::`, `self::`, `super::`, `<crate name>::`, a leading module name)
    is resolved through the SDK's module index (`ModuleIndex.resolve_path`): every segment must be
    a real module and the last name must be defined or imported/re-exported there; the definition
    it reaches is `("sdk-def", <crate>, *module path, Name)` (SDK type aliases expanded, other
    crates' items by their path: `("ext-crate", "concord", "ResponseMeta")`). A path that does not
    resolve is `("unresolved", <as written>)`, which no resolved identity equals
    (`crate::models::Error` where `models` has no `Error`, `crate::models::models::X`, and the
    internal spellings `sdk::error::Error` or `sdk_def::..`). Bare names are looked up in `ctx`'s
    scope (definitions, `use` imports, globs), then the primitives, the std prelude, then SDK-wide.
    Names the SDK imports from other crates are the imported item (`use concord::Error` →
    `("ext-crate", "concord", "Error")`); `concord::reqwest::..`/`concord::bytes::..` are
    `reqwest::..`/`bytes::..`; a std/core/alloc path is its `STD_PATHS` identity
    (`std::result::Result` = `core::result::Result` = the prelude `Result`), or unresolved when the
    table does not list it (`alloc::result::Result`); a leading `::` is dropped. Any other
    qualified path is another crate's item when its first segment is a crate the SDK uses
    (`Sdk.crates`), kept by path, so a wrong owner (`reqwest::Error`, `concord::Bytes`) does not
    match; with any other first segment (`made_up::Error`, `x::concord::Error`) it is
    unresolved."""
    return _canon(type_tree(text), sdk, _depth, tuple(ctx or ()))


def canon_type(text, sdk, ctx=(), _depth=0):
    """The printable form of `canon_ident` (for the record: truth files, aliases). Identities are
    written `{tag path}`, so the text is never a Rust type an answer could write; comparisons use
    `canon_ident`."""
    return ident_str(canon_ident(text, sdk, ctx, _depth))


def ident_str(n):
    """Printable text of an identity tree (`canon_ident`)."""
    k = n[0]
    if k == "id":
        p = "{" + n[1][0] + " " + "::".join(n[1][1:]) + "}"
        return p + ("<" + ", ".join(ident_str(a) for a in n[2]) + ">" if n[2] else "")
    if k == "ref":
        return "&" + ("mut " if n[1] else "") + ident_str(n[2])
    if k == "tuple":
        return "(" + ident_str(n[1][0]) + ",)" if len(n[1]) == 1 else "(" + ", ".join(ident_str(a) for a in n[1]) + ")"
    if k == "slice":
        return "[" + ident_str(n[1]) + "]"
    if k == "array":
        return "[" + ident_str(n[1]) + "; " + n[2] + "]"
    if k == "infer":
        return "_"
    if k == INVALID:
        return "{invalid " + n[1] + "}"
    return n[1]


PRELUDE_TYPES = {"Result", "Option", "Vec", "String", "Box"}


def _id(ident, cargs):
    return ("id", tuple(ident), tuple(cargs))


def _unresolved(segs, cargs):
    return _id((UNRESOLVED, "::".join(segs)), cargs)


def _canon(t, sdk, depth, ctx=()):
    k = t[0]
    if k == "canon":  # an already canonical subtree (an alias argument)
        return t[1]
    if k == "ref":
        return ("ref", t[1], _canon(t[2], sdk, depth, ctx))
    if k == "tuple":
        return ("tuple", tuple(_canon(a, sdk, depth, ctx) for a in t[1]))
    if k == "slice":
        return ("slice", _canon(t[1], sdk, depth, ctx))
    if k == "array":
        return ("array", _canon(t[1], sdk, depth, ctx), t[2])
    if k != "path":
        return tuple(t)  # ("infer",) or ("raw", text)
    segs = [s for s in t[1] if s]
    if not segs:
        return ("raw", type_str(t))
    return _canon_path(segs, t[2], sdk, depth, ctx)


def _canon_path(segs, args, sdk, depth, ctx=()):
    cargs = [_canon(a, sdk, depth, ctx) for a in args]
    if depth > 8:
        return _unresolved(segs, cargs)
    head = segs[0]
    # No macro-internal spelling (`$crate::__macro::concord_core::X`, `concord_core::X`) is
    # resolved: the probe materials never print one (rustdoc renders `concord::X`), and a
    # `concord_core` segment inside a path (`made_up::concord_core::Error`) is an invented owner.
    if head == "concord":
        if len(segs) >= 3 and segs[1] in ("reqwest", "bytes"):
            return _id((EXT, *segs[1:]), cargs)
        return _id((EXT, *segs), cargs)  # `concord::X` by path: a root export, or a wrong owner
    if head in ("std", "core", "alloc"):  # only real paths (STD_PATHS); any other is an invented owner
        identity = STD_PATHS.get(tuple(segs))
        return _id(identity, cargs) if identity else _unresolved(segs, cargs)
    if len(segs) == 1:
        return _canon_name(segs[0], args, cargs, sdk, depth, ctx)
    # SDK-qualified: `crate::X`, `self::X`, `super::X`, `<crate name>::X`, `<sdk module>::..::X`,
    # resolved only through real modules (README "Scoring", R2)
    if head in ("crate", "self", "super", sdk.crate) or sdk.index.module_head(head, ctx) is not None:
        r = sdk.index.resolve_path(segs, ctx)
        return _canon_resolved(r, cargs, sdk, depth) if r and r[0] != "mod" else _unresolved(segs, cargs)
    if head in sdk.imports:  # a module the SDK imports: `use concord::reqwest; reqwest::Url`
        full = sdk.imports[head].split("::") + segs[1:]
        if full != segs:
            return _canon_path(full, args, sdk, depth + 1, ctx)
    if head in sdk.crates:  # another crate the SDK uses: its item by path
        return _id((EXT, *segs), cargs)
    return _unresolved(segs, cargs)  # any other owner


def _canon_resolved(r, cargs, sdk, depth):
    """The identity tree of a `ModuleIndex` resolution: an SDK type `("sdk-def", <crate>, *module
    path, Name)` (an alias expanded in its own module's scope, with the already canonical
    arguments), an item of another crate by its path."""
    if r[0] == "def":
        mpath, item = r[1], r[2]
        if item.get("alias"):
            return _expand_alias(item["alias"], cargs, sdk, depth, mpath)
        return _id((SDK_DEF, sdk.crate, *mpath, item["name"]), cargs)
    return _canon_path(list(r[1]), [("canon", c) for c in cargs], sdk, depth + 1, ())


def _expand_alias(alias, cargs, sdk, depth, mpath):
    """An SDK `type` alias (params, target text) applied to the already canonical arguments,
    read in its own module `mpath` (generic parameters with defaults filled in); the target and
    defaults are parsed by `syn` like every R2 type."""
    params, target = alias
    subst = {}
    for i, (pn, dflt) in enumerate(params):
        if i < len(cargs):
            subst[pn] = ("canon", cargs[i])
        elif dflt:
            subst[pn] = ("canon", _canon(type_tree(dflt), sdk, depth + 1, mpath))
    return _canon(substitute(type_tree(target), subst), sdk, depth + 1, mpath)


def _canon_name(name, args, cargs, sdk, depth, ctx=()):
    if depth > 8:
        return _unresolved([name], cargs)
    # the declaration module's scope: its definitions, `use` imports and globs
    r = sdk.index.lookup(ctx, name, ctx) if ctx in sdk.index.mods else None
    if r and r[0] in ("def", "ext"):
        return _canon_resolved(r, cargs, sdk, depth)
    if name in PRIMITIVES:
        return _id((PRIM, name), cargs)
    if name in PRELUDE_TYPES:
        return _id((PRELUDE, name), cargs)
    # not in that scope: a name the SDK defines (closest to the root first) or imports anywhere
    defs = sdk.index.defs_named(name)
    if defs:
        return _canon_resolved(("def",) + defs[0], cargs, sdk, depth)
    if name in sdk.imports:
        full = [s for s in sdk.imports[name].split("::") if s]
        if full[0] in ("crate", "self", "super", sdk.crate) or full[0] in sdk.modules:
            # imported from inside the SDK (`use crate::api::DatadogApi`, `use wire::B2Wire`)
            if full[-1] != name:
                return _canon_name(full[-1], args, cargs, sdk, depth + 1, ())
            return _id((SDK_ITEM, sdk.crate, name), cargs)
        if len(full) > 1:
            return _canon_path(full, args, sdk, depth + 1, ())
    if name in sdk.aliases:  # a top-level alias the module index does not place
        return _expand_alias(sdk.aliases[name], cargs, sdk, depth, ())
    if sdk.defines_type(name):
        return _id((SDK_ITEM, sdk.crate, name), cargs)
    return _unresolved([name], cargs)


def accepted_aliases(sdk):
    """The alias equivalences canon_ident applies for this SDK (for the record)."""
    out = []
    for name, (params, target) in sorted(sdk.aliases.items()):
        g = "<" + ", ".join(p + (" = " + d if d else "") for p, d in params) + ">" if params else ""
        out.append(f"type {name}{g} = {target}")
    for local, full in sorted(sdk.imports.items()):
        if not sdk.defines_type(local) and local[:1].isupper() and not local.isupper():
            c = canon_ident(local, sdk)
            if c[0] == "id" and c[1][0] != UNRESOLVED:
                out.append(f"{local} = {ident_str(c)} (imported: {full})")
    return out


# ---------------------------------------------------------------- misc

def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 16), b""):
            h.update(chunk)
    return h.hexdigest()


def write_json(path, obj):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        json.dump(obj, f, indent=1, ensure_ascii=False, sort_keys=False)
        f.write("\n")


def read_json(path):
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def cargo_env():
    env = dict(os.environ)
    env["CARGO_TARGET_DIR"] = CARGO_TARGET_DIR
    env.setdefault("CARGO_BUILD_JOBS", "6")
    env.pop("RUSTFLAGS", None)
    return env


def scratch_copy(repo, name):
    """A throwaway copy of the repository's tracked and untracked (not ignored) files under
    `<repo>/target/tmp/<name>`; recreated on every call."""
    dest = os.path.join(repo, "target", "tmp", name)
    if os.path.isdir(dest):
        shutil.rmtree(dest)
    os.makedirs(dest)
    files = subprocess.run(["git", "-C", repo, "ls-files", "-co", "--exclude-standard", "-z"],
                           check=True, capture_output=True).stdout.decode().split("\0")
    for rel in files:
        if not rel:
            continue
        src = os.path.join(repo, rel)
        if not os.path.isfile(src):
            continue
        dst = os.path.join(dest, rel)
        os.makedirs(os.path.dirname(dst), exist_ok=True)
        shutil.copy2(src, dst)
    return dest


def rust_str(s):
    """A Rust string literal for `s`."""
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n") + '"'


DOC_FENCE = re.compile(r"(?m)^([ \t]*)(///|//!)[ \t]?```.*?\n(?:.*?\n)*?[ \t]*(///|//!)[ \t]?```[^\n]*$")


def strip_doc_examples(text):
    """Replace fenced code blocks inside `///`/`//!` doc comments with one placeholder line, so
    probe materials carry no worked call examples (the cheat sheet's generic example aside)."""
    return DOC_FENCE.sub(lambda m: f"{m.group(1)}{m.group(2)} (code example omitted from the probe materials)", text)


MD_FENCE = re.compile(r"(?ms)^([ \t]*)```.*?^[ \t]*```[^\n]*$")


def strip_md_examples(text):
    """The same for rendered rustdoc text (docs are Markdown)."""
    return MD_FENCE.sub(lambda m: f"{m.group(1)}(code example omitted from the probe materials)", text)


# ---------------------------------------------------------------- call chains (worked answers)

_SEG = re.compile(r"\.\s*(?:r#)?([A-Za-z_]\w*)\s*(?:::\s*<[^<>()]*>\s*)?\(")
_AFTER = re.compile(r"\s*(?:\.\s*await\b|\?)")
TERMINALS = ("send", "prepare")
CALL_OMITTED = "(call example omitted from the probe materials)"


def _close_paren(text, i, limit=600):
    """Index of the `)` matching the `(` at `i` (string literals skipped), or None."""
    depth, j, n = 0, i, min(len(text), i + limit)
    while j < n:
        c = text[j]
        if c == '"':
            j += 1
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
        elif c in "([{":
            depth += 1
        elif c in ")]}":
            depth -= 1
            if depth == 0:
                return j if c == ")" else None
        j += 1
    return None


def find_chains(text):
    """Rust method-call chains in `text`: [(start, end, [method names])] for every maximal run of
    `.name(args)` segments (arguments balanced; whitespace, newlines, `.await` and `?` allowed
    between segments). `api.list().set_limit(20).send().await?` gives [list, set_limit, send]."""
    out, i = [], 0
    while True:
        m = _SEG.search(text, i)
        if not m:
            return out
        start, pos, names = m.start(), m.start(), []
        while True:
            m2 = _SEG.match(text, pos)
            if not m2:
                break
            close = _close_paren(text, m2.end() - 1)
            if close is None:
                break
            names.append(m2.group(1))
            pos = close + 1
            while True:
                m3 = _AFTER.match(text, pos)
                if not m3:
                    break
                pos = m3.end()
            ws = re.match(r"\s*", text[pos:]).end()
            if _SEG.match(text, pos + ws):
                pos += ws
                continue
            break
        if names:
            out.append((start, pos, names))
            i = pos
        else:
            i = m.end()


def endpoint_navs(sdk):
    """[(navigation path (scope names.., endpoint name), setter names on that chain)] for every
    endpoint the SDK declares."""
    out = []
    for client, _ in sdk.clients:
        def setters(owner):
            return {"set_" + p["name"] for p in owner["params"]
                    if "group" not in p and (p.get("optional") or p.get("default") is not None)}
        for ep, anc in decl.walk(client):
            sets = set()
            for o in list(anc) + [ep]:
                sets |= setters(o)
            out.append((tuple(a["name"] for a in anc[1:]) + (ep["name"],), sets))
    return out


def chain_hits(names, navs):
    """The navigation paths a call chain spells out: the full path (two or more segments), or the
    endpoint followed later in the chain by `.send()`, `.prepare()` or one of its setters
    (root endpoints and chains started from a scope value)."""
    hits = []
    for path, setters in navs:
        k = len(path)
        full = k >= 2 and any(tuple(names[i:i + k]) == path for i in range(len(names) - k + 1))
        tail = any(n == path[-1] and any(x in TERMINALS or x in setters for x in names[i + 1:])
                   for i, n in enumerate(names))
        if full or tail:
            hits.append(path)
    return hits


_SPAN = re.compile(r"(`+)((?:(?!\1).)+?)\1", re.S)


def _strip_block(contents, navs):
    """Doc text lines → lines with every code span that holds an endpoint call chain replaced by
    a placeholder, and every line holding such a chain outside a code span replaced whole."""
    joined = "\n".join(contents)
    starts, off = [], 0
    for c in contents:
        starts.append(off)
        off += len(c) + 1
    line_of = lambda pos: max(i for i, s in enumerate(starts) if s <= pos)  # noqa: E731
    spans = [(m.start(), m.end()) for m in _SPAN.finditer(joined)]
    cut_spans, cut_lines = set(), set()
    for s, e, names in find_chains(joined):
        if not chain_hits(names, navs):
            continue
        sp = next(((a, b) for a, b in spans if a <= s < b), None)
        if sp:
            cut_spans.add(sp)
        else:
            cut_lines |= set(range(line_of(s), line_of(max(s, e - 1)) + 1))
    for a, b in cut_spans:  # a span over several lines: drop those lines instead
        la, lb = line_of(a), line_of(b - 1)
        if la != lb:
            cut_lines |= set(range(la, lb + 1))
    out = list(contents)
    for a, b in sorted((sp for sp in cut_spans if line_of(sp[0]) == line_of(sp[1] - 1)), reverse=True):
        li = line_of(a)
        if li in cut_lines:
            continue
        s0 = starts[li]
        out[li] = out[li][:a - s0] + CALL_OMITTED + out[li][b - s0:]
    first = True
    for li in sorted(cut_lines):
        out[li] = CALL_OMITTED if first or li - 1 not in cut_lines else None
        first = False
    return [x for x in out if x is not None]


_COMMENT = re.compile(r"^([ \t]*)(///|//!|//)( ?)(.*)$")


def strip_call_examples(text, navs, markdown=False):
    """Remove worked calls from probe materials: in Rust source, the comment lines (`///`,
    `//!`, `//`); in rendered Markdown docs (`markdown=True`), every line. A code span holding a
    call chain on a generated client, scope or endpoint (see `chain_hits`) is replaced by a
    placeholder; a line holding one outside a code span is replaced whole."""
    if markdown:
        return "\n".join(_strip_block(text.split("\n"), navs))
    lines = text.split("\n")
    out, i = [], 0
    while i < len(lines):
        m = _COMMENT.match(lines[i])
        if not m:
            out.append(lines[i])
            i += 1
            continue
        block = []
        while i < len(lines):
            m2 = _COMMENT.match(lines[i])
            if not m2 or m2.group(2) != m.group(2):
                break
            block.append(m2)
            i += 1
        new = _strip_block([b.group(4) for b in block], navs)
        if new == [b.group(4) for b in block]:
            out += [b.group(0) for b in block]
        else:
            pre = block[0]
            out += [f"{pre.group(1)}{pre.group(2)}{pre.group(3) or ' '}{c}".rstrip() for c in new]
    return "\n".join(out)


def scan_chains(text, navs):
    """[(chain text, [navigation paths])] of the endpoint call chains in `text` (comment prefixes
    at line starts removed first, so a chain split over doc lines is found)."""
    flat = re.sub(r"(?m)^[ \t]*//[/!]?[ \t]?", "", text)
    out = []
    for s, e, names in find_chains(flat):
        hits = chain_hits(names, navs)
        if hits:
            out.append((re.sub(r"\s+", " ", flat[s:e]).strip(), hits))
    return out


def bank_hash(bank_dir):
    """SHA-256 of a bank's canonical listing: the generated question files (`index.json` and
    `<sdk>.json`; not truth/, packed/ or harness/), one line `<sha256 of file>  <name>` each,
    sorted by name, newline-terminated."""
    names = sorted(f for f in os.listdir(bank_dir) if f.endswith(".json") and os.path.isfile(os.path.join(bank_dir, f)))
    listing = "".join(f"{sha256_file(os.path.join(bank_dir, f))}  {f}\n" for f in names)
    return hashlib.sha256(listing.encode()).hexdigest(), len(names)
