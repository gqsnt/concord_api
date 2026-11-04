"""The retired mutation helper (tests-gen/m4_mutations.py) stays retired.

It must refuse to run, without mutating, testing or writing anything and without printing any
verdict (its old verdict counted a mutant that did not compile as "killed"), and nothing maintained
may call it again. Run by scripts/verify.sh (step `tool-tests`).
"""

import os
import subprocess
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True  # no caches in the repository, also without -B

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
HELPER = os.path.join("tests-gen", "m4_mutations.py")
# Where a maintained caller would live; historical records under v4/ may keep naming it.
LIVING = ("scripts", ".github", "docs", "crates", "examples", "checks", "benches", "tests-gen",
          "README.md", "CHANGELOG.md")


class Retired(unittest.TestCase):
    def test_refuses_to_run(self):
        with tempfile.TemporaryDirectory() as d:
            checkout = os.path.join(d, "mut")
            os.mkdir(checkout)
            for args in ([], [checkout], [checkout, "a1_init_on_prepare"]):
                p = subprocess.run([sys.executable, "-I", os.path.join(ROOT, HELPER), *args],
                                   capture_output=True, text=True, cwd=d)
                out = p.stdout + p.stderr
                self.assertEqual(p.returncode, 2, out)
                self.assertIn("is retired", out)
                self.assertNotIn("killed", out.lower())
                self.assertNotIn("SURVIVED", out)
            self.assertEqual(os.listdir(checkout), [])  # no log, no edit

    def test_documented_as_retired(self):
        with open(os.path.join(ROOT, HELPER), encoding="utf-8") as f:
            head = f.read(400)
        self.assertIn("RETIRED", head)

    def test_no_maintained_caller(self):
        callers = []
        this = os.path.abspath(__file__)
        for top in LIVING:
            path = os.path.join(ROOT, top)
            files = [path] if os.path.isfile(path) else [
                os.path.join(base, name) for base, _, names in os.walk(path) for name in names]
            for file in files:
                parts = os.path.relpath(file, ROOT).split(os.sep)
                if os.path.abspath(file) in (this, os.path.join(ROOT, HELPER)) or \
                        "target" in parts or "__pycache__" in parts:
                    continue
                try:
                    with open(file, encoding="utf-8") as f:
                        if "m4_mutations" in f.read():
                            callers.append(os.path.relpath(file, ROOT))
                except (UnicodeDecodeError, OSError):
                    continue
        self.assertEqual(callers, [], "the retired helper is referred to again")


if __name__ == "__main__":
    unittest.main()
