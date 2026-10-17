"""Integrity and semantic-exit controls for the final return envelope."""
from pathlib import Path
import tempfile
import unittest
import zipfile

from qualification_final_return import (
    command_passes, package_manifest, product_path, safe_relative, verify_package, zip_package,
)
from qualification_ci_logs import write_json


class FinalReturnTests(unittest.TestCase):
    def test_expected_nonzero_is_distinct_from_infrastructure_failure(self):
        receipt = {
            "native_exit_code": 1, "launcher_error": None, "timed_out": False,
            "end_snapshot_error": None, "digest_errors": {}, "inputs_changed": False,
            "source_clean_at_start": True, "source_clean_at_end": True,
        }
        self.assertTrue(command_passes(receipt, 1))
        self.assertFalse(command_passes(receipt, 0))
        for key, value in [("timed_out", True), ("inputs_changed", True),
                           ("launcher_error", "could not launch"), ("native_exit_code", None)]:
            self.assertFalse(command_passes({**receipt, key: value}, 1))

    def test_source_paths_cannot_escape_or_alias_the_package(self):
        for name in ["../escape", "/absolute", "a/../b", "a//b", "a/./b", "a\\b", ""]:
            with self.assertRaises(ValueError):
                safe_relative(name)
        self.assertEqual(safe_relative("crates/core/src/lib.rs"), Path("crates/core/src/lib.rs"))

    def test_product_equivalence_scope_contains_build_inputs(self):
        for name in ["Cargo.lock", "Cargo.toml", "rust-toolchain.toml", "justfile",
                     "crates/concord-core/src/http/provider.rs", "crates/concord/build.rs",
                     "crates/concord/Cargo.toml", "tools/perf/src/bin/head_cost.rs"]:
            self.assertTrue(product_path(name), name)
        for name in ["dev_doc/release_gate_status.md", "scripts/qualification_final_return.py",
                     "crates/concord-inspect/tests/generated_contract.rs"]:
            self.assertFalse(product_path(name), name)

    def test_manifest_detects_modified_and_unlisted_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "source").mkdir()
            path = root / "source/input.rs"
            path.write_bytes(b"original")
            manifest = package_manifest(root)
            write_json(root / "manifests/package_sha256.json", manifest)
            verify_package(root, manifest)
            path.write_bytes(b"changed")
            with self.assertRaises(RuntimeError):
                verify_package(root, manifest)
            path.write_bytes(b"original")
            (root / "unlisted.txt").write_bytes(b"extra")
            with self.assertRaises(RuntimeError):
                verify_package(root, manifest)

    def test_zip_rejects_a_stale_embedded_manifest(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            root = base / "return"
            root.mkdir()
            path = root / "input.txt"
            path.write_text("before")
            write_json(root / "manifests/package_sha256.json", package_manifest(root))
            path.write_text("after")
            with self.assertRaises(RuntimeError):
                zip_package(root, base / "return.zip")

    def test_zip_contains_the_manifest_and_exact_file_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            root = base / "return"
            root.mkdir()
            (root / "binary.dat").write_bytes(bytes(range(256)))
            write_json(root / "manifests/package_sha256.json", package_manifest(root))
            destination = base / "return.zip"
            zip_package(root, destination)
            with zipfile.ZipFile(destination) as archive:
                self.assertEqual(archive.read("binary.dat"), bytes(range(256)))
                self.assertEqual(set(archive.namelist()), {"binary.dat", "manifests/package_sha256.json"})


if __name__ == "__main__":
    unittest.main()
