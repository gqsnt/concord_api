"""Discriminating checks for evidence integrity and complete probe parsing."""

import contextlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from evidence import VisibleHtml, record
from performance import CATEGORIES, WARM_NAMES, parse_runtime, parse_scale


class ProbeParsing(unittest.TestCase):
    def test_rustdoc_text_capture_keeps_code_and_omits_scripts(self):
        parser = VisibleHtml()
        parser.feed("<h1>IntoValue&lt;T&gt;</h1><script>ignored()</script><style>hidden{}</style><code>caller_value</code>")
        self.assertEqual(parser.text(), "IntoValue<T> caller_value")

    def warm(self):
        heads = "\n".join("planned " + name + ": https://example.test/" + name + " (1 public header(s))" for name in ["show", "search", "protected", "limited"])
        rows = "\n".join("| " + name + " | 1.00 | 24.0 | -2 | 100 | 90 |" for name in WARM_NAMES)
        return heads + "\n" + rows

    def test_complete_warm_and_signed_net_allocations(self):
        result = parse_runtime("head_cost", None, self.warm())
        self.assertEqual(len(result["values"]), 8)
        self.assertEqual(result["values"][WARM_NAMES[0]]["net_allocations_per_batch"], -2)

    def test_truncated_duplicate_and_nonfinite_warm_are_rejected(self):
        text = self.warm()
        for invalid in [text.replace("planned show:", "missing show:", 1),
                        "\n".join(text.splitlines()[:-1]),
                        text + "\n" + text.splitlines()[-1],
                        text.replace("| 1.00 |", "| nan |", 1),
                        text + "\nplanned extra: https://example.test/ (1 public header(s))",
                        text.replace("https://example.test/show (1 public header(s))", "malformed", 1)]:
            with self.subTest(invalid=invalid[-80:]), self.assertRaises(ValueError):
                parse_runtime("head_cost", None, invalid)

    def test_fresh_process_and_sequence_have_distinct_contracts(self):
        rows = ["first-use " + name + ": ns=100 allocations=2 bytes=48 peak_live_bytes=48 net_allocations=-1" for name in CATEGORIES]
        self.assertEqual(len(parse_runtime("head_first_use", "sequence", "\n".join(rows))["values"]), 9)
        self.assertEqual(len(parse_runtime("head_first_use", "client-build", rows[0])["values"]), 1)
        for category, output in [("execution", rows[0]), ("sequence", "\n".join(reversed(rows))), ("client-build", "\n".join(rows))]:
            with self.subTest(category=category), self.assertRaises(ValueError):
                parse_runtime("head_first_use", category, output)

    def test_future_layout_does_not_invent_timing(self):
        output = "boxed operation future (IntoFuture, ExecApi::bounded): 8000 B\nmanaged execution, client thread: 63.00 allocations/op, 28000.00 bytes/op\n"
        result = parse_runtime("future_sizes", None, output)
        self.assertEqual(result["values"]["future-layout"], {"bytes": 8000})
        with self.assertRaises(ValueError):
            parse_runtime("future_sizes", None, output.splitlines()[0])

    def test_scale_requires_all_points_and_matching_method(self):
        header = "expansion: rustc -Zunpretty=expanded (RUSTC_BOOTSTRAP=1); fixture CARGO_INCREMENTAL=0\n"
        lines = ["| " + " | ".join(map(str, [size] + [1] * 13)) + " |" for size in [1, 10, 100, 500, 1000]]
        self.assertEqual(len(parse_scale(header + "\n".join(lines), [1, 10, 100, 500, 1000])), 5)
        for output in [header + "\n".join(lines[:-1]), header + "\n".join(reversed(lines)), "\n".join(lines)]:
            with self.assertRaises(ValueError):
                parse_scale(output, [1, 10, 100, 500, 1000])


class NativeEvidence(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name) / "source"
        self.out = Path(self.temp.name) / "receipts"
        self.root.mkdir()
        subprocess.run(["git", "init", "-q", str(self.root)], check=True)
        (self.root / "input.txt").write_text("original\n")
        subprocess.run(["git", "-C", str(self.root), "add", "input.txt"], check=True)
        subprocess.run(["git", "-C", str(self.root), "-c", "user.name=Evidence Test", "-c", "user.email=evidence-test@example.invalid", "commit", "-qm", "test input"], check=True)

    def tearDown(self):
        self.temp.cleanup()

    def run_record(self, label, code, timeout=10, allow_dirty=False):
        with contextlib.redirect_stdout(io.StringIO()):
            result = record(self.root, self.out, label, [sys.executable, "-c", code], allow_dirty=allow_dirty, timeout_seconds=timeout)
        return result, json.loads((self.out / (label + ".json")).read_text())

    def test_nonzero_exit_is_preserved(self):
        result, receipt = self.run_record("nonzero", "raise SystemExit(7)")
        self.assertFalse(result)
        self.assertEqual(receipt["native_exit_code"], 7)
        self.assertFalse(receipt["inputs_changed"])

    def test_final_snapshot_failure_retains_the_known_native_exit(self):
        from evidence import snapshot
        before = snapshot(self.root)
        with patch("evidence.snapshot", side_effect=[before, OSError("final snapshot unavailable")]):
            result, receipt = self.run_record("snapshot-failure", "raise SystemExit(7)")
        self.assertFalse(result)
        self.assertEqual(receipt["native_exit_code"], 7)
        self.assertIsNone(receipt["inputs_changed"])
        self.assertIn("final snapshot unavailable", receipt["end_snapshot_error"])

    def test_success_cannot_hide_input_mutation(self):
        result, receipt = self.run_record("mutation", "from pathlib import Path; Path('input.txt').write_text('changed')")
        self.assertFalse(result)
        self.assertEqual(receipt["native_exit_code"], 0)
        self.assertTrue(receipt["inputs_changed"])

    def test_untracked_probe_content_is_part_of_the_input_identity(self):
        from evidence import snapshot
        probe = self.root / "probe.rs"
        probe.write_text("first\n")
        before = snapshot(self.root)
        probe.write_text("second\n")
        after = snapshot(self.root)
        self.assertEqual(before["status"], after["status"])
        self.assertNotEqual(before["input_files_sha256"], after["input_files_sha256"])
        result, receipt = self.run_record("untracked-mutation", "from pathlib import Path; Path('probe.rs').write_text('third')", allow_dirty=True)
        self.assertFalse(result)
        self.assertTrue(receipt["inputs_changed"])

    def test_timeout_is_not_success(self):
        result, receipt = self.run_record("timeout", "import time; time.sleep(30)", 0.2)
        self.assertFalse(result)
        self.assertTrue(receipt["timed_out"])
        self.assertNotEqual(receipt["native_exit_code"], 0)

    @unittest.skipUnless(os.name == "posix", "Linux process-group collection")
    def test_timeout_covers_descendant_output_after_leader_exit(self):
        parent = "import subprocess, sys; subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(30)'])"
        result, receipt = self.run_record("descendant", parent, 1)
        self.assertFalse(result)
        self.assertTrue(receipt["timed_out"])
        self.assertLess(receipt["duration_seconds"], 10)

    def test_valid_receipt_cannot_be_overwritten(self):
        result, receipt = self.run_record("success", "print('finished')")
        self.assertTrue(result)
        self.assertEqual(receipt["native_exit_code"], 0)
        with self.assertRaises(RuntimeError):
            self.run_record("success", "raise SystemExit(1)")


if __name__ == "__main__":
    unittest.main()
