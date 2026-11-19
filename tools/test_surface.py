"""Regression tests for the rustdoc surface and its changelog check."""

from contextlib import redirect_stderr, redirect_stdout
from copy import deepcopy
import io
import json
from pathlib import Path
import subprocess
import unittest
from unittest.mock import Mock, patch

import surface


GENERICS = {"params": [], "where_predicates": []}
HIDDEN = {"other": "#[doc(hidden)]"}


def item(name, kind, body, *, attrs=(), visibility="public"):
    return {"name": name, "inner": {kind: body}, "attrs": list(attrs),
            "visibility": visibility, "crate_id": 0}


def rustdoc(name, items, paths, root_items):
    return surface.Crate({
        "format_version": surface.RUSTDOC_FORMAT,
        "includes_private": True,
        "target": {"triple": surface.TARGET},
        "root": 0,
        "index": {0: item(name, "module", {"items": root_items}), **items},
        "paths": {number: {"path": path.split("::"), "kind": kind}
                  for number, (path, kind) in paths.items()},
    })


def method(output="bool", attrs=()):
    return item("ready", "function", {
        "header": {"is_const": False, "is_async": False, "is_unsafe": False, "abi": "Rust"},
        "sig": {"inputs": [["self", {"borrowed_ref": {
            "type": {"generic": "Self"}, "lifetime": None, "is_mutable": False,
        }}]], "output": {"primitive": output}, "is_c_variadic": False},
        "generics": GENERICS,
        "has_body": True,
    }, attrs=attrs)


def impl(trait=None, members=(), negative=False, generics=GENERICS):
    return item(None, "impl", {
        "trait": trait, "for": {"resolved_path": {"id": 1, "path": "Request", "args": None}},
        "items": list(members), "generics": generics, "is_negative": negative,
        "is_synthetic": trait is not None, "is_unsafe": False, "blanket_impl": None,
    })


def public_fixture():
    req = item("Request", "struct", {"kind": "unit", "impls": [2, 4, 5, 6], "generics": GENERICS})
    items = {1: req, 2: impl(), 3: method()}
    paths = {1: ("concord_core::requests::Request", "struct")}
    for number, name in enumerate(("Send", "Sync", "Unpin"), start=4):
        trait_id = number + 10
        paths[trait_id] = (f"core::marker::{name}", "trait")
        items[number] = impl({"id": trait_id, "path": name, "args": None}, negative=name == "Unpin")
    # The facade only has an external path for this re-export. Its definition and methods
    # must come from the supporting crate, even though the defining module is private.
    facade = rustdoc("concord", {1: item(None, "use", {
        "name": "Request", "id": 99, "source": "concord_core::Request", "is_glob": False,
    })}, {99: paths[1]}, [1])
    return {
        "concord": facade,
        "concord_core": rustdoc("concord_core", items, paths, []),
        "concord_testkit": rustdoc("concord_testkit", {}, {}, []),
    }


def snapshot(crates):
    return surface.Surface(crates).generate()


class PublicSurface(unittest.TestCase):
    def test_added_method_and_changed_signature_require_the_exported_path(self):
        crates = public_fixture()
        owner = crates["concord_core"]
        original = snapshot(crates)
        owner.items[2]["inner"]["impl"]["items"] = [3]
        added = snapshot(crates)
        owner.items[3] = method("u32")
        changed = snapshot(crates)

        path = "concord::Request::ready"
        for before, after in ((original, added), (added, changed)):
            with self.subTest(before=before == original):
                self.assertEqual(surface.undocumented(before, after, "## Unreleased\n"), [path])
                self.assertEqual(surface.undocumented(
                    before, after, f"## Unreleased\n- Changed `{path}()`.\n"), [])
        self.assertIn("fn ready(self: &Self) -> u32", changed)

    def test_auto_trait_polarity_and_bounds_are_public(self):
        crates = public_fixture()
        original = snapshot(crates)
        self.assertIn("impl !core::marker::Unpin", original)
        self.assertIn("impl core::marker::Send", original)
        self.assertIn("impl core::marker::Sync", original)
        send = crates["concord_core"].items[4]["inner"]["impl"]
        send["is_negative"] = True
        send["generics"] = {"params": [], "where_predicates": [{"bound_predicate": {
            "type": {"generic": "T"}, "bounds": [{"outlives": "'static"}], "generic_params": [],
        }}]}
        changed = snapshot(crates)
        self.assertIn("impl !core::marker::Send", changed)
        self.assertIn("where T: 'static [auto]", changed)
        self.assertEqual(surface.undocumented(original, changed, ""), ["concord::Request"])

    def test_private_supertrait_and_its_generic_seal_are_recorded(self):
        args = {"angle_bracketed": {"args": [{"type": {"generic": "C"}}], "constraints": []}}
        trait = {"generics": GENERICS, "is_auto": False, "is_unsafe": False, "is_dyn_compatible": True,
                 "items": [], "implementations": [], "bounds": []}
        crates = public_fixture()
        crates["concord"] = rustdoc("concord", {
            1: item("IntoNext", "trait", trait),
            2: item("Sealed", "trait", deepcopy(trait)),
        }, {1: ("concord::IntoNext", "trait"), 2: ("concord::sealed::Sealed", "trait")}, [1])
        open_trait = snapshot(crates)
        self.assertIn("concord::IntoNext\tunsealed", open_trait)
        bound = {"path": "Sealed", "id": 2, "args": None}
        trait["bounds"].append({"trait_bound": {"trait": bound, "generic_params": [], "modifier": "none"}})
        old_seal = snapshot(crates)
        self.assertIn("concord::IntoNext\tsealed", old_seal)
        bound["args"] = args
        new_seal = snapshot(crates)
        self.assertIn("Sealed<C>", new_seal)
        self.assertEqual(surface.undocumented(old_seal, new_seal, ""), ["concord::IntoNext"])

    def test_removing_non_exhaustive_changes_the_declaration(self):
        crates = public_fixture()
        struct = crates["concord_core"].items[1]
        struct["attrs"] = ["non_exhaustive"]
        before = snapshot(crates)
        self.assertIn("#[non_exhaustive] struct Request", before)
        struct["attrs"] = []
        self.assertEqual(surface.undocumented(before, snapshot(crates), ""), ["concord::Request"])

    def test_hidden_paths_are_counted_and_require_a_snapshot_refresh(self):
        crates = public_fixture()
        before = snapshot(crates)
        crate = crates["concord"]
        crate.items[10] = item("__private", "module", {"items": [11]})
        crate.items[11] = method()
        crate.items[12] = method(attrs=[HIDDEN])
        crate.items[0]["inner"]["module"]["items"].extend([10, 12])
        after = snapshot(crates)
        self.assertIn("# concord: 3 hidden items", after)
        self.assertNotIn("__private", after)
        self.assertNotIn("::ready", after)

        baseline = Mock()
        baseline.read_text.return_value = before

        def refresh(text):
            baseline.read_text.return_value = text

        def run(args, status, message):
            output = io.StringIO()
            with redirect_stdout(output):
                self.assertEqual(surface.main(args), status)
            self.assertIn(message, output.getvalue())
            self.assertNotIn("missing from Unreleased", output.getvalue())
            return output.getvalue()

        empty_notes = "## Unreleased\n"
        with (patch.object(surface, "build", return_value=crates),
              patch.object(surface, "SNAPSHOT", baseline),
              patch.object(surface, "update_snapshot", side_effect=refresh),
              patch.object(Path, "read_text", return_value=empty_notes) as changelog):
            output = run([], 1, "3 added, 0 removed")
            self.assertIn("refresh the snapshot with just surface --update", output)
            self.assertIn("+# concord: 3 hidden items", output)
            changelog.return_value += "- `concord::__private`, `concord::__private::ready`, `concord::ready`\n"
            run([], 1, "3 added, 0 removed")
            changelog.return_value = empty_notes
            run(["--update"], 0, "SURFACE: updated")
            run([], 0, "SURFACE: matches")

            crate.items[12]["name"] = "renamed"
            run([], 1, "1 added, 1 removed")
            run(["--update"], 0, "SURFACE: updated")
            run([], 0, "SURFACE: matches")

            baseline.read_text.return_value = snapshot(crates).replace(
                "# concord: 3 hidden items", "# concord: 4 hidden items"
            )
            run([], 1, "0 added, 1 removed")

    def test_inline_reexport_exposes_a_hidden_definition(self):
        crates = public_fixture()
        crates["concord_core"].items[1]["attrs"] = [HIDDEN]
        export = crates["concord"].items[1]
        self.assertNotIn("concord::Request\t", snapshot(crates))
        export["attrs"] = [{"other": "#[doc(inline)]"}]
        self.assertIn("concord::Request\tstruct Request", snapshot(crates))

    def test_changelog_requires_each_exact_path_in_unreleased(self):
        before = surface.render_snapshot({}, set())
        rows = {"concord::a": {"fn a()"}, "concord::b": {"fn b()"}}
        after = surface.render_snapshot(rows, set())
        notes = """## Unreleased
### Added
- `concord::a_extra`, `concord::a::child`, `concord::b`
- ``some text with `concord::a` in it``
```text
## Unreleased
`concord::a`
```
## 1.0
- `concord::a`
"""
        self.assertEqual(surface.undocumented(before, after, notes), ["concord::a"])
        self.assertEqual(surface.undocumented(
            before, after, notes.replace("### Added", "### Added\n- `concord::a`")), [])

    def test_nightly_missing_fails_before_cargo(self):
        errors = io.StringIO()
        with patch.object(surface.subprocess, "run", return_value=subprocess.CompletedProcess([], 1)) as run:
            with patch.dict(surface.os.environ, {"CONCORD_SURFACE_TOOLCHAIN": "nightly"}), redirect_stderr(errors):
                self.assertEqual(surface.main([]), 1)
        run.assert_called_once()
        self.assertEqual(run.call_args.args[0], ["rustup", "which", "--toolchain", "nightly", "rustdoc"])
        self.assertIn("an installed Rust nightly toolchain is required", errors.getvalue())

    def test_proc_macro_json_is_read_from_the_host_output(self):
        target = Path("target/surface-snapshot/rustdoc")
        macro_json = target / "doc/concord_macros.json"
        library_json = target / surface.TARGET / "doc"
        read_paths = []

        def read(path, *, encoding):
            read_paths.append(path)
            return json.dumps({
                "format_version": surface.RUSTDOC_FORMAT, "includes_private": True,
                "target": {"triple": "aarch64-apple-darwin" if path == macro_json else surface.TARGET},
                "root": 0, "index": {}, "paths": {},
            })

        with patch.object(Path, "read_text", read):
            self.assertEqual(set(surface.load_crates(target)), set(surface.PACKAGES.values()))
        self.assertCountEqual(read_paths, [macro_json, *(
            library_json / f"{name}.json" for name in surface.PACKAGES.values() if name != "concord_macros"
        )])

    def test_host_proc_macro_receives_json_flags_directly(self):
        result = subprocess.CompletedProcess([], 0, stdout=b"/toolchain/bin/rustdoc\n")
        with (patch.object(surface.subprocess, "run", return_value=result) as run,
              patch.object(surface.os, "access", return_value=True),
              patch.object(surface, "load_crates", return_value={})):
            surface.build()
        _, target, host = run.call_args_list
        self.assertIn("--offline", target.args[0])
        self.assertNotIn("concord-macros", target.args[0])
        self.assertIn("--offline", host.args[0])
        self.assertIn("concord-macros", host.args[0])
        self.assertEqual(host.args[0][host.args[0].index("--") + 1:], list(surface.JSON_FLAGS))
