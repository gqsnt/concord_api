"""Contract inventory and fail-fast gate regression checks."""

from contextlib import redirect_stdout, redirect_stderr
import io
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import gate
import matrix


class ToolTests(unittest.TestCase):
    def setUp(self):
        # Keep test artifacts with the generated reports.
        target = gate.REPO_ROOT / "target" / "tool-tests"
        target.mkdir(parents=True, exist_ok=True)
        self.temp = tempfile.TemporaryDirectory(dir=target)
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def write(self, relative, text):
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
        return path

    def test_inventory_skips_literals_comments_and_helpers_and_tracks_ignores(self):
        source = '''
// #[test] fn c1_1_comment() {}
/* outer /* nested */ #[test] fn c1_1_block() {} */
const EXAMPLE: &str = r###"#[test] fn c1_1_raw_string() {}"###;
const EXAMPLE2: &str = "#[test] fn c1_1_string() {}";
#[test]
fn ordinary_test() {}
fn c1_1_helper() {}
#[ignore = "not ready"]
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn c4_3_wait() {}
#[test]
#[ignore]
fn c4_3_second() {}
#[test]
#[doc = r"an attribute with ] brackets"]
pub(crate) fn c2_7_present() {}
#[test]
fn c2_7_() {}
#[ignore]
mod unrelated { #[test] fn c2_8_present() {} }
'''
        path = self.write("concord-core/src/lib.rs", source)
        tests = list(matrix.find_tests(path))
        self.assertEqual([(test.number, test.name, test.ignored) for test in tests], [
            ("C4.3", "c4_3_wait", True), ("C4.3", "c4_3_second", True),
            ("C2.7", "c2_7_present", False), ("C2.7", "c2_7_", False),
            ("C2.8", "c2_8_present", False)])
        lines = source.splitlines()
        for test in tests:
            self.assertIn("fn " + test.name, lines[test.line - 1])

    def test_report_covers_every_number_orphans_withdrawn_and_ignored(self):
        path = self.write("docs/reference/contracts.md", '''
| C1.1 | Active; one old clause was withdrawn. | contract |
| C1.2 | **not used** | — |
| C1.3 | A closure `|x| x` and an escaped \\| pipe. | contract |
| C1.4 | Still needs a test. | contract |
''')
        contracts = matrix.read_contracts(path)
        self.assertEqual([contract.withdrawn for contract in contracts], [False, True, False, False])
        self.assertIn("escaped \\| pipe.", contracts[2].text)
        tests = list(matrix.find_tests(self.write("concord-core/tests/rules.rs", '''
#[test] #[ignore] fn c1_1_holds() {}
#[test] fn c1_2_old() {}
#[tokio::test] async fn c99_1_orphan() {}
''')))
        document, summary, gaps = matrix.render(contracts, tests, {"C1.3": "compile-only"}, self.root / "target")
        self.assertEqual(summary, "contracts: 3 that hold, 1 with a test, 2 with none")
        self.assertEqual(gaps, ["C1.4"])
        self.assertIn("(ignored) | 1 |", document)
        self.assertIn("- C99.1:", document)
        self.assertIn("- C1.2:", document)
        self.assertIn("- C1.3 — NO-TEST: compile-only", document)
        self.assertEqual(sum(line.startswith("| [C") for line in document.splitlines()), 4)

    def test_strict_only_fails_unexplained_active_gaps_and_always_writes_report(self):
        self.write("docs/reference/contracts.md", "| C2.7 | Active. | contract |\n| C3.7 | not used | — |\n")
        with patch.object(matrix, "ROOT", self.root), redirect_stdout(io.StringIO()):
            self.assertEqual(matrix.main([]), 0)
            self.assertEqual(matrix.main(["--strict"]), 1)
            self.assertTrue((self.root / "target/contracts-matrix.md").exists())
            self.write("tools/NO-TEST.md", "C2.7:   \n")
            self.assertEqual(matrix.main(["--strict"]), 1)
            self.write("tools/NO-TEST.md", "C2.7: compile error checked outside runtime tests\n")
            self.assertEqual(matrix.main(["--strict"]), 0)

    def test_non_strict_read_error_still_exits_zero(self):
        with patch.object(matrix, "ROOT", self.root), redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
            self.assertEqual(matrix.main([]), 0)
            self.assertEqual(matrix.main(["--strict"]), 1)

    def test_generated_rust_is_excluded(self):
        source = self.write("concord-core/src/lib.rs", "#[test] fn c1_1_real() {}")
        self.write("target/debug/build/output.rs", "#[test] fn c1_2_generated() {}")
        self.assertEqual(list(matrix.rust_files(self.root)), [source])

    def test_new_manifest_feature_is_discovered(self):
        self.write("concord-api/Cargo.toml", '[features]\ndefault = ["json"]\njson = []\nnew_feature = []\n')
        with patch.object(gate, "REPO_ROOT", self.root):
            commands = gate.commands("features")
        self.assertEqual(len(commands), 5)
        self.assertIn(["--features", "new_feature"], [command[-2:] for command in commands])
        self.assertTrue(all("--no-default-features" in command for command in commands))
        self.assertEqual(commands[-1][-1], "--all-features")

    def test_check_and_test_stop_after_first_failing_command(self):
        for recipe in ("check", "test"):
            with self.subTest(recipe=recipe), patch.object(gate, "run", side_effect=subprocess.CalledProcessError(7, "cargo")) as run:
                output = io.StringIO()
                with redirect_stdout(output):
                    self.assertEqual(gate.execute(recipe), 7)
                self.assertEqual(run.call_count, 1)
                self.assertTrue(output.getvalue().splitlines()[-1].startswith(recipe.upper() + ": FAILED"))

    def test_gate_stops_at_failed_step_and_has_exact_last_line(self):
        with patch.object(gate, "execute", side_effect=[0, 0, 7]) as execute, patch("sys.argv", ["gate.py", "gate"]):
            output = io.StringIO()
            with redirect_stdout(output):
                self.assertEqual(gate.main(), 7)
            self.assertEqual([call.args[0] for call in execute.call_args_list], list(gate.STEPS[:3]))
            self.assertEqual(output.getvalue().splitlines()[-1], "GATE: FAILED at check")

    def test_gate_success_order_and_exact_last_line(self):
        with patch.object(gate, "execute", return_value=0) as execute, patch("sys.argv", ["gate.py", "gate"]):
            output = io.StringIO()
            with redirect_stdout(output):
                self.assertEqual(gate.main(), 0)
            self.assertEqual([call.args[0] for call in execute.call_args_list], list(gate.STEPS))
            self.assertEqual(output.getvalue().splitlines()[-1], "GATE: passed")

    def test_msrv_target_is_isolated_and_offline(self):
        env = gate.environment("msrv")
        self.assertEqual(env["CARGO_TARGET_DIR"], str(gate.REPO_ROOT / "target/msrv"))
        self.assertEqual(env["CARGO_NET_OFFLINE"], "true")

    def test_todo_counts_macro_calls_but_not_examples(self):
        self.write("Cargo.toml", '[workspace]\nmembers = ["core"]\n')
        self.write("core/Cargo.toml", '[package]\nname = "core"\n')
        self.write("core/src/lib.rs", '''
// todo!()
const EXAMPLE: &str = r"todo!()";
fn stub() { todo!("not ready"); todo! {} }
''')
        output = io.StringIO()
        with patch.object(gate, "REPO_ROOT", self.root), redirect_stdout(output):
            self.assertEqual(gate.execute("todo"), 0)
        self.assertIn("core: 2 todo!()", output.getvalue())
        self.assertEqual(output.getvalue().splitlines()[-1], "TODO: counted")


if __name__ == "__main__":
    unittest.main()
