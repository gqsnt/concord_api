"""Integrity and semantic-exit controls for the final return envelope."""
from contextlib import redirect_stdout
from copy import deepcopy
import hashlib
from io import StringIO
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch
import zipfile

from qualification_final_return import (
    command_passes, package_manifest, product_path, safe_relative, verify_package, wait_for_ci, write_json,
    zip_package,
)


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


class FinalCiTests(unittest.TestCase):
    """Synthetic API/log fixtures exercise the real collection boundary, without network access."""

    HEAD = "a" * 40
    RUN_ID = 42
    POLICY_JOB = "Fixtures and dependency policy"
    POLICY_STEP = "Dependency and license policy"
    JOB_NAMES = (
        "Quality and policy", "Tests and examples", "Documentation",
        "Stable compiler contracts", "MSRV product libraries",
        "Fixtures and dependency policy", "Platform smoke (ubuntu-latest)",
        "Platform smoke (windows-latest)", "Rust analyzer",
        "Windows implementation qualification",
    )
    PREVIOUS_FAILURE_LOG = (
        "error[rejected]: alloc-no-stdlib BSD-3-Clause\n"
        "error[rejected]: alloc-stdlib BSD-3-Clause\n"
        "error[rejected]: brotli BSD-3-Clause AND MIT\n"
        "advisories ok, bans ok, licenses FAILED, sources ok\n"
    )

    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.args = SimpleNamespace(package=Path(temporary.name) / "return")
        self.stdout = StringIO()
        self.downloaded_paths = []
        self.reset_successful_ci()

    def reset_successful_ci(self):
        self.run = {
            "id": self.RUN_ID, "run_number": 2, "run_attempt": 1,
            "head_sha": self.HEAD, "name": "CI", "event": "push",
            "head_branch": "concord-v3", "status": "completed", "conclusion": "success",
            "repository": {"full_name": "gqsnt/concord_api"},
        }
        # Keep the list response complete even when a later API read changes state.
        self.selected_run = deepcopy(self.run)
        self.jobs = [
            {"id": 100 + index, "name": name, "status": "completed",
             "conclusion": "success", "steps": []}
            for index, name in enumerate(self.JOB_NAMES)
        ]
        self.policy_job()["steps"] = [
            {"name": self.POLICY_STEP, "status": "completed", "conclusion": "success"}
        ]
        self.policy_log = "\x1b[32madvisories ok, bans ok, licenses ok, sources ok\x1b[0m\n"

    def policy_job(self):
        return next(job for job in self.jobs if job["name"] == self.POLICY_JOB)

    def fetch_json(self, path, destination=None):
        if path == f"actions/runs?head_sha={self.HEAD}&per_page=100":
            value = {"workflow_runs": [self.selected_run]}
        elif path == f"actions/runs/{self.RUN_ID}":
            value = self.run
        elif path == f"actions/runs/{self.RUN_ID}/attempts/1/jobs?per_page=100":
            value = {"jobs": self.jobs}
        else:
            self.fail(f"unexpected API request: {path}")
        value = deepcopy(value)
        if destination is not None:
            write_json(destination, value)
        return value

    def fetch_bytes(self, path, destination):
        self.downloaded_paths.append(path)
        destination.parent.mkdir(parents=True, exist_ok=True)
        if path == f"actions/jobs/{self.policy_job()['id']}/logs":
            destination.write_text(self.policy_log, encoding="utf-8")
        elif path == f"actions/runs/{self.RUN_ID}/attempts/1/logs":
            with zipfile.ZipFile(destination, "w") as archive:
                archive.writestr("synthetic-ci-fixture.log", "synthetic collector test fixture\n")
        elif path == f"actions/jobs/{next(job['id'] for job in self.jobs if job['name'] == 'Windows implementation qualification')}/logs":
            destination.write_text("synthetic Windows log fixture\n", encoding="utf-8")
        else:
            self.fail(f"unexpected download request: {path}")
        raw = destination.read_bytes()
        return {"sha256": hashlib.sha256(raw).hexdigest(), "bytes": len(raw), "http_status": 200}

    def collect(self):
        with patch("qualification_final_return.github_json", side_effect=self.fetch_json), \
                patch("qualification_final_return.github_bytes", side_effect=self.fetch_bytes), \
                redirect_stdout(self.stdout):
            return wait_for_ci(self.args, self.HEAD)

    def assert_rejected(self):
        with self.assertRaises(RuntimeError):
            self.collect()
        self.assertFalse((self.args.package / "evidence/final/ci/adjudication.json").exists())
        self.assertNotIn("FINAL_RETURN_CI ", self.stdout.getvalue())

    def test_complete_success_collects_native_responses_and_adjudicates(self):
        result = self.collect()
        self.assertEqual(result["head_sha"], self.HEAD)
        self.assertEqual(result["native_conclusion"], "success")
        self.assertTrue(result["all_ci_jobs_passed"])
        self.assertTrue(result["dependency_policy_passed"])
        self.assertNotIn("license_policy_exception", result)
        self.assertEqual(len(result["jobs"]), 10)
        destination = self.args.package / "evidence/final/ci"
        self.assertEqual(json.loads((destination / "adjudication.json").read_text()), result)
        self.assertEqual((destination / "dependency-policy-native.log").read_text(), self.policy_log)
        self.assertEqual(len(self.downloaded_paths), 3)
        with zipfile.ZipFile(destination / "logs-native.zip") as archive:
            self.assertEqual(archive.read("synthetic-ci-fixture.log"), b"synthetic collector test fixture\n")
        self.assertIn("FINAL_RETURN_CI ", self.stdout.getvalue())

    def test_previous_bsd_failure_cannot_use_the_retired_exception(self):
        # This complete failure was accepted by the previous license-specific bypass.
        self.run["conclusion"] = self.selected_run["conclusion"] = "failure"
        self.policy_job()["conclusion"] = "failure"
        self.policy_job()["steps"][0]["conclusion"] = "failure"
        self.policy_log = self.PREVIOUS_FAILURE_LOG
        self.assert_rejected()

    def test_failed_or_skipped_run_or_job_cannot_be_overridden_by_other_successes(self):
        for target in ("run", "policy_job", "another_job"):
            for conclusion in ("failure", "skipped"):
                with self.subTest(target=target, conclusion=conclusion):
                    self.reset_successful_ci()
                    value = self.run if target == "run" else (
                        self.policy_job() if target == "policy_job" else self.jobs[0]
                    )
                    value["conclusion"] = conclusion
                    self.assert_rejected()

    def test_failed_or_skipped_policy_step_cannot_hide_in_a_successful_job(self):
        for conclusion in ("failure", "skipped"):
            with self.subTest(conclusion=conclusion):
                self.reset_successful_ci()
                self.policy_job()["steps"][0]["conclusion"] = conclusion
                self.assert_rejected()

    def test_policy_step_must_be_present_once_and_complete(self):
        for state in ("missing", "duplicated", "in_progress"):
            with self.subTest(state=state):
                self.reset_successful_ci()
                steps = self.policy_job()["steps"]
                if state == "missing":
                    steps.clear()
                elif state == "duplicated":
                    steps.append(deepcopy(steps[0]))
                else:
                    steps[0].update(status="in_progress", conclusion=None)
                self.assert_rejected()

    def test_successful_api_metadata_does_not_replace_the_native_policy_result(self):
        for text in (self.PREVIOUS_FAILURE_LOG, "cargo-deny output is incomplete\n"):
            with self.subTest(policy_log=text):
                self.policy_log = text
                self.assert_rejected()


if __name__ == "__main__":
    unittest.main()
