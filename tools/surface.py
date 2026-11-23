#!/usr/bin/env python3
"""Compare the documented Concord API with the release snapshot."""

import argparse
from collections import Counter, defaultdict
from contextlib import contextmanager
from dataclasses import dataclass
import difflib
from hashlib import sha256
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
SNAPSHOT = ROOT / "tools/public-surface.txt"
WORK = ROOT / "target/surface-snapshot"
RUSTDOC_FORMAT = 61
RUSTDOC_TOOLCHAIN = "nightly-2026-10-13"
TARGET = "x86_64-unknown-linux-gnu"
PACKAGES = {
    "concord-api": "concord",
    "concord-testkit": "concord_testkit",
    "concord-core": "concord_core",
    "concord-extras": "concord_extras",
    "concord-macros": "concord_macros",
}
PUBLIC_CRATES = ("concord", "concord_testkit")
MACRO_PACKAGE = "concord-macros"
JSON_FLAGS = ("-Z", "unstable-options", "--output-format", "json",
              "--document-hidden-items", "--document-private-items")
HEADER = f"# Concord public surface v2 ({TARGET})"
DEFAULT_HEADER = "# Public paths with default features"
BLANKET_TRAIT_CRATES = {*PACKAGES.values(), "alloc", "core", "std"}
RUST_INTERNAL_TRAITS = {
    "core::asserting::Printable",
    "core::clone::CloneToUninit",
    "core::io::size_hint::SizeHint",
    "core::marker::Freeze",
    "core::marker::UnsafeUnpin",
    "core::mem::SizedTypeProperties",
    "core::ops::deref::Receiver",
}


def tagged(value):
    (entry,) = value.items()
    return entry


def doc_hidden(item):
    return {"other": "#[doc(hidden)]"} in item["attrs"]


def doc_inline(item):
    return {"other": "#[doc(inline)]"} in item["attrs"]


def doc_only(item):
    # Rustdoc enables cfg(doc). Keep the documentation example out of the library API.
    return any(
        isinstance(attr, dict)
        and attr.get("other", "").startswith('#[attr = CfgTrace([NameValue { name: "doc", value: None,')
        for attr in item["attrs"]
    )


class Crate:
    def __init__(self, data, expected_target=TARGET):
        if data["format_version"] != RUSTDOC_FORMAT or not data["includes_private"]:
            raise ValueError(f"expected private rustdoc JSON format {RUSTDOC_FORMAT}, "
                             f"got format {data['format_version']}; install {RUSTDOC_TOOLCHAIN} "
                             f"and set CONCORD_SURFACE_TOOLCHAIN={RUSTDOC_TOOLCHAIN}")
        if expected_target is not None and data["target"]["triple"] != expected_target:
            raise ValueError(f"expected rustdoc JSON for {expected_target}")
        self.root = data["root"]
        self.items = {int(key): item for key, item in data["index"].items()}
        self.paths = {int(key): (tuple(value["path"]), value["kind"])
                      for key, value in data["paths"].items()}
        self.by_path = {key: item_id for item_id, key in self.paths.items() if item_id in self.items}

    def body(self, item_id):
        return tagged(self.items[item_id]["inner"])


@dataclass(frozen=True)
class Export:
    name: str
    crate: Crate
    item_id: int
    hidden: bool = False
    reexport: bool = False


def has_value_constructor(value):
    return value["kind"] == "unit" or (
        isinstance(value["kind"], dict) and "tuple" in value["kind"]
    )


class Surface:
    def __init__(self, crates):
        self.crates = crates
        self.rows = defaultdict(set)
        self.hidden = set()
        self.reachable = set()

    def resolve(self, crate, item_id):
        if item_id in crate.items and crate.items[item_id]["crate_id"] == 0:
            return crate, item_id
        path = crate.paths[item_id]
        defining = self.crates.get(path[0][0])
        if defining is None:
            return crate, item_id
        return defining, defining.by_path[path]

    def exports(self, crate, module, active=()):
        if (crate, module) in active:
            return
        active = (*active, (crate, module))
        direct = {}
        globs = defaultdict(set)
        for item_id in crate.body(module)[1]["items"]:
            item = crate.items[item_id]
            if item["visibility"] != "public" or doc_only(item):
                continue
            kind, value = crate.body(item_id)
            hidden = doc_hidden(item)
            if kind == "use":
                defining, target = self.resolve(crate, value["id"])
                if value["is_glob"]:
                    if target not in defining.items:
                        raise ValueError(f"cannot expand external glob {value['source']}")
                    for key, export in self.exports(defining, target, active):
                        globs[key].add(Export(export.name, export.crate, export.item_id,
                                              hidden or export.hidden, True))
                    continue
                definition = defining.items.get(target)
                hidden = hidden or (
                    definition is not None and doc_hidden(definition) and not doc_inline(item)
                )
                export = Export(value["name"], defining, target, hidden, True)
            else:
                export = Export(item["name"], crate, item_id, hidden)
            for namespace in self.namespaces(export):
                direct[export.name, namespace] = export

        yield from direct.items()
        for key, candidates in globs.items():
            targets = {(export.crate, export.item_id) for export in candidates}
            if key not in direct and len(targets) == 1:
                yield key, min(candidates, key=lambda export: export.hidden)

    def namespaces(self, export):
        crate, item_id = export.crate, export.item_id
        if item_id in crate.items:
            kind, value = crate.body(item_id)
            if kind == "struct" and has_value_constructor(value):
                return ("type", "value")
        else:
            _, kind = crate.paths[item_id]
        if kind in {"macro", "proc_macro"}:
            return ("macro",)
        if kind in {"function", "constant", "static"}:
            return ("value",)
        return ("type",)

    def walk(self, crate, module, prefix, hidden=False, active=()):
        if (crate, module) in active:
            return
        active = (*active, (crate, module))
        for export in dict.fromkeys(export for _, export in self.exports(crate, module)):
            defining, item_id = export.crate, export.item_id
            item = defining.items.get(item_id)
            if item is not None and doc_only(item):
                continue
            path = f"{prefix}::{export.name}"
            excluded = hidden or export.hidden or export.name == "__private"
            yield path, export, excluded
            if item is not None and "module" in item["inner"]:
                yield from self.walk(defining, item_id, path, excluded, active)

    def collect(self):
        # A private supertrait is inaccessible even through the supporting crates.
        public = {}
        for name, crate in self.crates.items():
            public[name] = list(self.walk(crate, crate.root, name))
            self.reachable.update((export.crate, export.item_id) for _, export, _ in public[name])
        for name in PUBLIC_CRATES:
            for path, export, excluded in public[name]:
                crate, item_id = export.crate, export.item_id
                if item_id not in crate.items:
                    origin, _ = crate.paths[item_id]
                    self.record(path, f"pub use {'::'.join(origin)}", excluded)
                    continue
                origin = ""
                if export.reexport:
                    origin = " [from " + "::".join(crate.paths[item_id][0]) + "]"
                self.item(crate, item_id, path, excluded, origin=origin, export=True)
        return self

    def generate(self, default_paths=()):
        self.collect()
        return render_snapshot(self.rows, self.hidden, default_paths)

    def record(self, path, signature, hidden):
        if hidden:
            self.hidden.add((path, signature))
        else:
            self.rows[path].add(signature)

    def item(self, crate, item_id, path, hidden, *, context="", origin="", export=False):
        item = crate.items[item_id]
        kind, value = crate.body(item_id)
        rust = Rust(crate)
        hidden = hidden or (not export and doc_hidden(item))
        if doc_only(item):
            return
        name = path.rsplit("::", 1)[-1]
        attrs = "#[non_exhaustive] " if "non_exhaustive" in item["attrs"] else ""
        self.record(path, attrs + rust.declaration(name, kind, value) + origin + context, hidden)

        if kind in {"struct", "union", "variant"}:
            shape = value["kind"] if kind != "union" else {"plain": value}
            if isinstance(shape, dict):
                layout, fields = tagged(shape)
                fields = fields if layout == "tuple" else fields["fields"]
                for index, field in enumerate(fields):
                    if field is None:
                        continue
                    member = crate.items[field]
                    if kind == "variant" or member["visibility"] == "public":
                        field_name = str(index) if layout == "tuple" else member["name"]
                        self.item(crate, field, f"{path}::{field_name}", hidden)
        if kind == "enum":
            for variant in value["variants"]:
                name = crate.items[variant]["name"]
                self.item(crate, variant, f"{path}::{name}", hidden)
        if kind == "trait":
            if not hidden:
                seals = self.seals(crate, item_id)
                self.rows[path].add("sealed" if seals else "unsealed")
                for defining, seal in seals:
                    for impl_id in defining.body(seal)[1]["implementations"]:
                        implementation = defining.body(impl_id)[1]
                        self.rows[path].add("seal: " + Rust(defining).implementation(implementation))
            for member in value["items"]:
                name = crate.items[member]["name"]
                self.item(crate, member, f"{path}::{name}", hidden)
        if kind in {"struct", "union", "enum", "trait"}:
            for impl_id in value.get("impls", value.get("implementations", ())):
                self.implementation(crate, impl_id, path, hidden)

    def seals(self, crate, item_id, visited=None):
        if visited is None:
            visited = set()
        if (crate, item_id) in visited:
            return set()
        visited.add((crate, item_id))
        trait = crate.body(item_id)[1]
        bounds = list(trait["bounds"])
        for predicate in trait["generics"]["where_predicates"]:
            bound = predicate.get("bound_predicate")
            if bound and bound["type"] == {"generic": "Self"}:
                bounds.extend(bound["bounds"])
        seals = set()
        for bound in bounds:
            if "trait_bound" not in bound:
                continue
            supertrait = bound["trait_bound"]["trait"]["id"]
            defining, target = self.resolve(crate, supertrait)
            if target not in defining.items or "trait" not in defining.items[target]["inner"]:
                continue
            if (defining, target) not in self.reachable:
                seals.add((defining, target))
            seals.update(self.seals(defining, target, visited))
        return seals

    def implementation(self, crate, item_id, path, hidden):
        item = crate.items[item_id]
        impl = crate.body(item_id)[1]
        rust = Rust(crate)
        trait = impl["trait"]
        if trait is not None:
            trait_name = rust.path(trait, with_args=False)
            if trait_name in RUST_INTERNAL_TRAITS:
                return
            if impl["blanket_impl"] is not None and trait_name.partition("::")[0] not in BLANKET_TRAIT_CRATES:
                return
            defining, target = self.resolve(crate, trait["id"])
            definition = defining.items.get(target)
            hidden = hidden or doc_hidden(item) or (
                definition is not None and doc_hidden(definition)
            ) or "__private" in trait_name.split("::")
            if definition is not None and (defining, target) not in self.reachable:
                return
            if hidden and (impl["is_synthetic"] or impl["blanket_impl"] is not None):
                return
            signature = rust.implementation(impl)
            bindings = []
            for member in impl["items"]:
                kind, value = crate.body(member)
                # Methods are declared on the trait. Associated bindings can
                # differ in every implementation and are part of its signature.
                if kind in {"assoc_type", "assoc_const"}:
                    entry = crate.items[member]
                    declaration = rust.declaration(entry["name"], kind, value)
                    if not hidden and doc_hidden(entry):
                        self.record(f"{path}::{entry['name']}", f"{declaration} [{signature}]", True)
                    else:
                        bindings.append(declaration)
            if bindings:
                signature += " { " + ", ".join(sorted(bindings)) + " }"
            self.record(f"{path}::impl" if hidden else path, signature, hidden)
        else:
            context = f" [{rust.implementation(impl)}]"
            for member in impl["items"]:
                entry = crate.items[member]
                if entry["visibility"] == "public":
                    self.item(crate, member, f"{path}::{entry['name']}", hidden or doc_hidden(item),
                              context=context)


class Rust:
    """Render the signature-bearing parts of rustdoc's versioned schema."""

    def __init__(self, crate):
        self.crate = crate

    def path(self, value, *, with_args=True):
        summary = self.crate.paths.get(value["id"])
        name = "::".join(summary[0]) if summary else value["path"]
        return name + self.arguments(value["args"]) if with_args else name

    def arguments(self, arguments):
        if not arguments:
            return ""
        kind, values = tagged(arguments)
        if kind == "parenthesized":
            return "(" + ", ".join(map(self.type, values["inputs"])) + ")" + self.output(values["output"])
        if kind == "return_type_notation":
            return "(..)"
        if kind != "angle_bracketed":
            raise ValueError(f"unknown generic arguments {kind}")
        rendered = []
        for arg in values["args"]:
            if arg == "infer":
                rendered.append("_")
                continue
            kind, value = tagged(arg)
            if kind == "type":
                rendered.append(self.type(value))
            elif kind == "const":
                rendered.append(value["value"] or value["expr"])
            elif kind == "lifetime":
                rendered.append(value)
            else:
                raise ValueError(f"unknown generic argument {kind}")
        for constraint in values["constraints"]:
            key = constraint["name"] + self.arguments(constraint["args"])
            kind, value = tagged(constraint["binding"])
            if kind == "equality":
                term, inner = tagged(value)
                rendered.append(f"{key} = {self.type(inner) if term == 'type' else inner['expr']}")
            elif kind == "constraint":
                rendered.append(key + self.bounds(value, ": "))
            else:
                raise ValueError(f"unknown associated item binding {kind}")
        return "<" + ", ".join(rendered) + ">" if rendered else ""

    def parameters(self, values):
        rendered = []
        for param in values:
            name = param["name"]
            kind, value = tagged(param["kind"])
            if kind == "type":
                if value["is_synthetic"]:
                    continue  # The input already shows the argument-position impl Trait.
                name += self.bounds(value["bounds"], ": ")
                if value["default"] is not None:
                    name += " = " + self.type(value["default"])
            elif kind == "lifetime":
                if value["outlives"]:
                    name += ": " + " + ".join(value["outlives"])
            elif kind == "const":
                name = f"const {name}: {self.type(value['type'])}"
                if value["default"] is not None:
                    name += " = " + value["default"]
            else:
                raise ValueError(f"unknown generic parameter {kind}")
            rendered.append(name)
        return "<" + ", ".join(rendered) + ">" if rendered else ""

    def binder(self, parameters):
        return "for" + self.parameters(parameters) + " " if parameters else ""

    def bounds(self, bounds, prefix=""):
        rendered = []
        for bound in bounds:
            kind, value = tagged(bound)
            if kind == "trait_bound":
                modifier = {"none": "", "maybe": "?", "maybe_const": "~const "}[value["modifier"]]
                rendered.append(self.binder(value["generic_params"]) + modifier + self.path(value["trait"]))
            elif kind == "outlives":
                rendered.append(value)
            elif kind == "use":
                rendered.append("use<" + ", ".join(next(iter(arg.values())) for arg in value) + ">")
            else:
                raise ValueError(f"unknown generic bound {kind}")
        return prefix + " + ".join(rendered) if rendered else ""

    def generics(self, value):
        rendered = []
        for predicate in value["where_predicates"]:
            kind, pred = tagged(predicate)
            if kind == "bound_predicate":
                if pred["bounds"]:
                    rendered.append(self.binder(pred["generic_params"]) + self.type(pred["type"])
                                    + self.bounds(pred["bounds"], ": "))
            elif kind == "lifetime_predicate":
                rendered.append(pred["lifetime"] + ": " + " + ".join(pred["outlives"]))
            elif kind == "eq_predicate":
                term, rhs = tagged(pred["rhs"])
                rendered.append(self.type(pred["lhs"]) + " = "
                                + (self.type(rhs) if term == "type" else rhs["expr"]))
            else:
                raise ValueError(f"unknown where predicate {kind}")
        return self.parameters(value["params"]), " where " + ", ".join(rendered) if rendered else ""

    def type(self, data):
        if data == "infer":
            return "_"
        kind, value = tagged(data)
        if kind == "primitive" and value == "never":
            return "!"
        if kind in {"generic", "primitive"}:
            return value
        if kind == "resolved_path":
            return self.path(value)
        if kind == "qualified_path":
            owner = self.type(value["self_type"])
            if value["trait"] is not None:
                owner = f"<{owner} as {self.path(value['trait'])}>"
            return f"{owner}::{value['name']}" + self.arguments(value["args"])
        if kind == "tuple":
            return "(" + ", ".join(map(self.type, value)) + ("," if len(value) == 1 else "") + ")"
        if kind == "slice":
            return "[" + self.type(value) + "]"
        if kind == "array":
            return f"[{self.type(value['type'])}; {value['len']}]"
        if kind == "raw_pointer":
            return ("*mut " if value["is_mutable"] else "*const ") + self.type(value["type"])
        if kind == "borrowed_ref":
            lifetime = value["lifetime"] + " " if value["lifetime"] else ""
            return "&" + lifetime + ("mut " if value["is_mutable"] else "") + self.type(value["type"])
        if kind == "dyn_trait":
            parts = [self.binder(trait["generic_params"]) + self.path(trait["trait"])
                     for trait in value["traits"]]
            if value["lifetime"]:
                parts.append(value["lifetime"])
            rendered = "dyn " + " + ".join(parts)
            return f"({rendered})" if len(parts) > 1 else rendered
        if kind == "impl_trait":
            return "impl " + self.bounds(value)
        if kind == "function_pointer":
            return (self.binder(value["generic_params"]) + self.function_prefix(value["header"])
                    + self.function_signature(value["sig"]))
        if kind == "pat":
            return self.type(value["type"]) + " is " + value["__pat_unstable_do_not_use"]
        raise ValueError(f"unknown signature type {kind}")

    def output(self, value):
        return " -> " + self.type(value) if value is not None else ""

    def function_prefix(self, header):
        prefix = "".join(word + " " for word in ("const", "async", "unsafe") if header["is_" + word])
        abi = header["abi"]
        if abi != "Rust":
            if isinstance(abi, dict):
                kind, value = tagged(abi)
                abi = kind + ("-unwind" if value["unwind"] else "")
            prefix += f'extern "{abi}" '
        return prefix + "fn"

    def function_signature(self, value):
        args = [f"{name}: {self.type(arg)}" if name not in {"", "_"} else self.type(arg)
                for name, arg in value["inputs"]]
        if value["is_c_variadic"]:
            args.append("...")
        return "(" + ", ".join(args) + ")" + self.output(value["output"])

    def declaration(self, name, kind, value):
        params, where = self.generics(value["generics"]) if isinstance(value, dict) and "generics" in value else ("", "")
        if kind == "module":
            return f"pub mod {name}"
        if kind == "function":
            return (f"{self.function_prefix(value['header'])} {name}{params}"
                    + self.function_signature(value["sig"]) + where
                    + (" [required]" if not value["has_body"] else ""))
        if kind == "trait":
            prefix = ("unsafe " if value["is_unsafe"] else "") + ("auto " if value["is_auto"] else "")
            dyn = " [dyn compatible]" if value["is_dyn_compatible"] else ""
            return f"{prefix}trait {name}{params}" + self.bounds(value["bounds"], ": ") + where + dyn
        if kind in {"struct", "union", "enum", "variant"}:
            shape = ""
            if kind in {"struct", "variant"}:
                layout = value["kind"]
                if isinstance(layout, dict):
                    tag, data = tagged(layout)
                    fields = data if tag == "tuple" else data["fields"]
                    private = kind == "struct" and any(
                        field is None or self.crate.items[field]["visibility"] != "public"
                        for field in fields
                    )
                    marker = ".." if private else ""
                    shape = f"({marker})" if tag == "tuple" else " {" + marker + "}"
            if kind == "variant" and value["discriminant"] is not None:
                shape += " = " + value["discriminant"]["value"]
            return f"{kind} {name}{params}{shape}{where}"
        if kind == "struct_field":
            return f"{name}: {self.type(value)}"
        if kind in {"type_alias", "assoc_type"}:
            bounds = self.bounds(value.get("bounds", ()), ": ")
            default = " = " + self.type(value["type"]) if value["type"] is not None else " [required]"
            return f"type {name}{params}{bounds}{where}{default}"
        if kind == "trait_alias":
            return f"trait {name}{params} = " + self.bounds(value["params"]) + where
        if kind in {"constant", "assoc_const", "static"}:
            if kind == "static":
                keyword = "static mut" if value["is_mutable"] else "static"
                return f"{keyword} {name}: {self.type(value['type'])}"
            literal = value["const"]["value"] if kind == "constant" else value["value"]
            suffix = f" = {literal}" if literal is not None and literal != "_" else ""
            if kind == "assoc_const" and literal is None:
                suffix = " [required]"
            return f"const {name}: {self.type(value['type'])}{suffix}"
        if kind == "proc_macro":
            return f"proc macro {name}! [{value['kind']}]" + (
                " helpers " + ", ".join(value["helpers"]) if value["helpers"] else ""
            )
        if kind == "macro":
            return " ".join(value.split())
        raise ValueError(f"unsupported public item kind {kind} at {name}")

    def implementation(self, value):
        params, where = self.generics(value["generics"])
        prefix = ("unsafe " if value["is_unsafe"] else "") + "impl" + params + " "
        trait = value["trait"]
        subject = self.type(value["for"])
        if trait is None:
            return prefix + subject + where
        negative = "!" if value["is_negative"] else ""
        annotation = ""
        if value["is_synthetic"]:
            annotation = " [auto]"
        elif value["blanket_impl"] is not None:
            annotation = " [blanket for " + self.type(value["blanket_impl"]) + "]"
        return f"{prefix}{negative}{self.path(trait)} for {subject}{where}{annotation}"


def fingerprint(path, signature):
    return sha256(f"{path}\0{signature}".encode("utf-8")).hexdigest()[:32]


def render_snapshot(rows, hidden, default_paths=()):
    lines = [HEADER, "# All features: public signatures and hidden fingerprints."]
    for root in PUBLIC_CRATES:
        fingerprints = sorted(fingerprint(path, signature) for path, signature in hidden
                              if path.startswith(root + "::"))
        label = "hidden item" if len(fingerprints) == 1 else "hidden items"
        lines.extend(("", f"# {root}: {len(fingerprints)} {label}"))
        lines.extend(f"# hidden {root} {digest}" for digest in fingerprints)
        lines.append("")
        lines.extend(f"{path}\t{signature}" for path in sorted(rows) if path.startswith(root + "::")
                     for signature in sorted(rows[path]))
    lines.extend(("", DEFAULT_HEADER, *sorted(default_paths)))
    return "\n".join(lines) + "\n"


def generate():
    all_crates = build()
    default_crates = build(default=True, macro=all_crates[PACKAGES[MACRO_PACKAGE]])
    return Surface(all_crates).generate(Surface(default_crates).collect().rows)


PATH = r"(?:r#)?\w+(?:::(?:r#)?\w+)+"
INLINE_CODE = re.compile(r"(?<!`)(`+)(?!`)(.+?)(?<!`)\1(?!`)")
INLINE_PATH = re.compile(rf"({PATH})(?:\(\)|!)?")
HEADING = re.compile(r"^ {0,3}(#{1,6})\s+(.*?)\s*#*\s*$")
FENCE = re.compile(r"^ {0,3}(`{3,}|~{3,})(.*)$")
HIDDEN_COUNT = re.compile(r"^# (\w+): (\d+) hidden items?$")


def unreleased_paths(changelog):
    in_section = False
    fence = None
    paths = set()
    for line in changelog.splitlines():
        marker = FENCE.fullmatch(line)
        if fence is not None:
            if marker and marker[1].startswith(fence) and not marker[2].strip():
                fence = None
            continue
        if marker:
            fence = marker[1]
            continue
        heading = HEADING.fullmatch(line)
        if heading and heading[1] == "##":
            if in_section:
                break
            in_section = heading[2] == "Unreleased"
        if in_section:
            for span in INLINE_CODE.finditer(line):
                if match := INLINE_PATH.fullmatch(span[2].strip()):
                    paths.add(match[1])
    return paths


def snapshot_rows(snapshot):
    lines = snapshot.splitlines()
    if not lines or lines[0] != HEADER:
        raise ValueError("unexpected snapshot header; review the format before updating")
    rows = defaultdict(list)
    defaults = False
    for line in lines[1:]:
        if line == DEFAULT_HEADER:
            defaults = True
        elif line and not line.startswith("#"):
            if defaults:
                if not re.fullmatch(PATH, line):
                    raise ValueError(f"invalid default feature path: {line}")
                rows[line].append(None)
                continue
            path, signature = line.split("\t", 1)
            if not re.fullmatch(PATH, path) or not signature:
                raise ValueError(f"invalid public snapshot row: {line}")
            rows[path].append(signature)
    return rows


def hidden_inventory(snapshot):
    fingerprints = {root: Counter() for root in PUBLIC_CRATES}
    counts = {}
    for line in snapshot.splitlines():
        if line.startswith("# hidden "):
            _, _, root, digest = line.split()
            if root not in PUBLIC_CRATES or re.fullmatch(r"[0-9a-f]{32}", digest) is None:
                raise ValueError(f"invalid hidden snapshot row: {line}")
            fingerprints[root][digest] += 1
        elif match := HIDDEN_COUNT.fullmatch(line):
            counts[match[1]] = int(match[2])
    return fingerprints, counts


def hidden_changes(before, after):
    old, old_counts = hidden_inventory(before)
    new, new_counts = hidden_inventory(after)
    added = removed = 0
    for root in PUBLIC_CRATES:
        # Fingerprints detect same-count replacements. Also catch a count edited on its own.
        count_delta = new_counts.get(root, new[root].total()) - old_counts.get(root, old[root].total())
        added += max((new[root] - old[root]).total(), count_delta)
        removed += max((old[root] - new[root]).total(), -count_delta)
    return added, removed


def undocumented(before, after, notes):
    old, new = snapshot_rows(before), snapshot_rows(after)
    documented = unreleased_paths(notes)
    return sorted(path for path in old.keys() | new.keys()
                  if old.get(path) != new.get(path) and path not in documented)


def build(*, default=False, macro=None):
    # Unlike a missing cargo +nightly invocation, rustup which never installs a toolchain.
    toolchain = os.environ.get("CONCORD_SURFACE_TOOLCHAIN", RUSTDOC_TOOLCHAIN)
    missing = f"an installed Rust nightly toolchain is required for rustdoc JSON (selected {toolchain})"
    try:
        nightly = subprocess.run(["rustup", "which", "--toolchain", toolchain, "rustdoc"],
                                 cwd=ROOT, capture_output=True, check=False)
    except FileNotFoundError as error:
        raise ValueError(missing) from error
    if nightly.returncode:
        raise ValueError(missing)
    if not os.access(os.fsdecode(nightly.stdout.strip()), os.X_OK):
        raise ValueError("the installed Rust nightly toolchain needs its rustdoc component")

    target = WORK / ("rustdoc-default" if default else "rustdoc")
    env = os.environ.copy()
    env.pop("CARGO_ENCODED_RUSTDOCFLAGS", None)
    env.update(
        CARGO_TARGET_DIR=str(target),
        CARGO_NET_OFFLINE="true",
        RUSTDOCFLAGS=" ".join(JSON_FLAGS),
    )
    cargo = ["rustup", "run", toolchain, "cargo"]
    options = ["--offline", "--locked", "--target", TARGET]
    libraries = [*cargo, "doc", "--no-deps", "--lib", *options]
    for package in PACKAGES:
        if package != MACRO_PACKAGE:
            libraries.extend(("-p", package))
    if default:
        with default_workspace() as workspace:
            env["RUSTDOCFLAGS"] += " -A rustdoc::broken_intra_doc_links"
            subprocess.run([*libraries, *default_feature_args(workspace)], cwd=workspace, env=env, check=True)
        return load_crates(target, macro=macro)

    subprocess.run([*libraries, "--all-features"], cwd=ROOT, env=env, check=True)
    # With --target, Cargo applies RUSTDOCFLAGS to target crates only.
    # Pass the flags directly to the host proc macro's rustdoc invocation.
    command = [*cargo, "rustdoc", *options, "--all-features", "-p", MACRO_PACKAGE,
               "--lib", "--", *JSON_FLAGS]
    subprocess.run(command, cwd=ROOT, env={**env, "RUSTDOCFLAGS": ""}, check=True)
    return load_crates(target)


def default_feature_args(workspace):
    # Selecting the supporting libraries must not enable their independent
    # defaults. Let Concord's public crates select dependency features as usual.
    args = ["--no-default-features"]
    for package, name in PACKAGES.items():
        if name in PUBLIC_CRATES:
            with (workspace / package / "Cargo.toml").open("rb") as manifest:
                if "default" in tomllib.load(manifest).get("features", {}):
                    args.extend(("--features", f"{package}/default"))
    return args


@contextmanager
def default_workspace():
    # The doc-only sample uses items that need not be in the default API.
    # Leave the source tree intact, and omit the sample only from this build.
    WORK.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="defaults-", dir=WORK) as directory:
        workspace = Path(directory) / "source"
        shutil.copytree(ROOT, workspace, ignore=shutil.ignore_patterns(".git", "target"))
        facade = workspace / "concord-api/src/lib.rs"
        text = facade.read_text(encoding="utf-8")
        sample = "#[cfg(doc)]\npub mod sample;"
        if sample in text:
            facade.write_text(text.replace(sample, "#[cfg(any())]\npub mod sample;", 1), encoding="utf-8")
        yield workspace


def load_crates(target, *, macro=None):
    crates = {}
    for name in PACKAGES.values():
        # Cargo documents the proc macro for the host even with --target.
        host = name == PACKAGES[MACRO_PACKAGE]
        if host and macro is not None:
            crates[name] = macro
            continue
        directory = target if host else target / TARGET
        data = json.loads((directory / "doc" / f"{name}.json").read_text(encoding="utf-8"))
        crates[name] = Crate(data, expected_target=None if host else TARGET)
    return crates


def update_snapshot(text):
    with tempfile.NamedTemporaryFile(dir=WORK,
                                     mode="w", encoding="utf-8", delete=False) as file:
        temporary = Path(file.name)
        try:
            file.write(text)
            file.close()
            temporary.chmod(0o644)
            temporary.replace(SNAPSHOT)
        finally:
            temporary.unlink(missing_ok=True)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--update", action="store_true", help="rewrite tools/public-surface.txt for documented changes")
    mode.add_argument("--release", action="store_true", help="require an exact match to tools/public-surface.txt")
    args = parser.parse_args(argv)
    try:
        current = generate()
        baseline = SNAPSHOT.read_text(encoding="utf-8")
        missing = []
        if current != baseline:
            missing = undocumented(baseline, current, (ROOT / "CHANGELOG.md").read_text(encoding="utf-8"))
        if args.update and not missing:
            update_snapshot(current)
            print("SURFACE: updated tools/public-surface.txt")
            return 0
        if current == baseline:
            print("SURFACE: matches tools/public-surface.txt")
            return 0
        added, removed = hidden_changes(baseline, current)
        print("".join(difflib.unified_diff(baseline.splitlines(keepends=True),
                                         current.splitlines(keepends=True),
                                         fromfile="tools/public-surface.txt",
                                         tofile="current public surface", n=0)), end="", flush=True)
        for path in missing:
            print(f"SURFACE: missing from Unreleased: {path}")
        if added or removed:
            print(f"SURFACE: hidden items changed ({added} added, {removed} removed); "
                  "refresh the snapshot with just surface --update")
        if missing or added or removed:
            print("SURFACE: FAILED (undocumented changes)" if missing else "SURFACE: FAILED (hidden changes)")
            return 1
        if args.release:
            print("SURFACE: FAILED (snapshot is stale); refresh with just surface --update")
            return 1
        if snapshot_rows(baseline) == snapshot_rows(current):
            raise ValueError("snapshot formatting changed; review it and run just surface --update")
        print("SURFACE: changes documented in Unreleased; refresh with just surface --update")
        return 0
    except subprocess.CalledProcessError:
        print("SURFACE: FAILED: rustdoc did not build; run cargo doc to see why", file=sys.stderr)
        return 1
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(f"SURFACE: FAILED ({error})", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
