"""Check release orchestration without running the expensive child checks."""

from contextlib import redirect_stdout
import io
import os
import subprocess
import unittest
from unittest.mock import patch

import release


class Release(unittest.TestCase):
    def test_success_runs_each_recipe_in_order_and_reports_each_result(self):
        output = io.StringIO()
        with patch.object(release.subprocess, "run", return_value=subprocess.CompletedProcess([], 0)) as run, redirect_stdout(output):
            self.assertEqual(release.main([]), 0)
        self.assertEqual([call.args[0] for call in run.call_args_list], [
            ["just", "gate"], ["just", "docs-audit"], ["just", "doc-examples"],
            ["just", "panics"], ["just", "package"],
            ["just", "features-all"], ["just", "mutants", "--jobs", "1"],
        ])
        self.assertTrue(all(call.kwargs == {"cwd": release.REPO_ROOT, "check": False} for call in run.call_args_list))
        self.assertEqual(output.getvalue().splitlines(), [
            *(f"RELEASE {step}: passed" for step in release.STEPS), "RELEASE: passed",
        ])

    def test_each_failure_stops_the_release_and_names_the_step(self):
        for index, step in enumerate(release.STEPS):
            with self.subTest(step=step):
                results = [subprocess.CompletedProcess([], 0)] * index
                results.append(subprocess.CompletedProcess([], 7))
                output = io.StringIO()
                with patch.object(release.subprocess, "run", side_effect=results) as run, redirect_stdout(output):
                    self.assertEqual(release.main([]), 7)
                self.assertEqual(run.call_count, index + 1)
                self.assertEqual(output.getvalue().splitlines(), [
                    *(f"RELEASE {name}: passed" for name in release.STEPS[:index]),
                    f"RELEASE {step}: FAILED", f"RELEASE: FAILED at {step}",
                ])

    def test_socket_policy_is_inherited_by_every_release_step(self):
        with patch.dict(os.environ, {"CONCORD_SKIP_SOCKETS": "1"}), patch.object(
            release.subprocess, "run", return_value=subprocess.CompletedProcess([], 0)
        ) as run, redirect_stdout(io.StringIO()):
            self.assertEqual(release.main([]), 0)
            self.assertEqual(os.environ["CONCORD_SKIP_SOCKETS"], "1")
        commands = [call.args[0] for call in run.call_args_list]
        self.assertEqual(commands[-1], ["just", "mutants", "--jobs", "1"])
        self.assertTrue(all(call.kwargs == {"cwd": release.REPO_ROOT, "check": False}
                            for call in run.call_args_list))

    def test_child_cannot_start_or_is_interrupted_reports_a_failure(self):
        for error, code in ((OSError("unavailable"), 1), (KeyboardInterrupt(), 130)):
            with self.subTest(error=type(error).__name__):
                output = io.StringIO()
                with patch.object(release.subprocess, "run", side_effect=error) as run, redirect_stdout(output):
                    self.assertEqual(release.main([]), code)
                run.assert_called_once()
                self.assertEqual(output.getvalue().splitlines()[-1], "RELEASE: FAILED at gate")


if __name__ == "__main__":
    unittest.main()
