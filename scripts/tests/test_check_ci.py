"""Executable controls for the workflow guard (`scripts/check-ci.py --workflow`).

The real workflow and the supported forms must pass with the positive witness
"effective full gate on: Linux, Windows"; each negative must exit 1 with its own intended message,
not merely fail. Run by scripts/verify.sh (step `tool-tests`):

    python3 -I -m unittest discover -s scripts/tests -p "test_*.py"
"""

import importlib.util
import os
import subprocess
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True  # no caches in the repository, also without -B

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
CHECK = os.path.join(ROOT, "scripts", "check-ci.py")
WITNESS = "effective full gate on: Linux, Windows"

_spec = importlib.util.spec_from_file_location("check_ci", CHECK)
check_ci = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(check_ci)

BASE = """\
name: ci
on:
  push:
  pull_request:
jobs:
  verify:
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, windows-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - run: rustup show
      - name: gate
        shell: bash
        run: scripts/verify.sh --package --bench
      - name: logs
        if: failure()
        shell: bash
        run: |
          for log in target/verify-*.log; do cat "$log"; done
  nightly:
    runs-on: ubuntu-latest
    continue-on-error: true
    steps:
      - uses: actions/checkout@v4
      - name: nightly
        shell: bash
        run: scripts/verify.sh --nightly-only
"""
GATE = "        run: scripts/verify.sh --package --bench\n"


def variant(old, new, text=BASE):
    """text with old replaced once (an anchor that does not occur once is a broken test)."""
    if text.count(old) != 1:
        raise AssertionError(f"anchor occurs {text.count(old)} times: {old!r}")
    return text.replace(old, new)


def gate(script):
    """BASE with the gate step's `run` replaced by the given YAML lines (indented for the step)."""
    return variant(GATE, script)


def guard(text, newline="\n"):
    with tempfile.TemporaryDirectory() as d:
        path = os.path.join(d, "ci.yml")
        with open(path, "w", encoding="utf-8", newline=newline) as f:
            f.write(text)
        p = subprocess.run([sys.executable, "-I", CHECK, "--workflow-file", path],
                           capture_output=True, text=True)
    return p.returncode, p.stdout + p.stderr


class Passes(unittest.TestCase):
    def assertPasses(self, text, newline="\n"):
        code, out = guard(text, newline)
        self.assertEqual(code, 0, out)
        self.assertIn(WITNESS, out)
        self.assertNotIn("FAIL", out)
        return out

    def test_real_workflow(self):
        p = subprocess.run([sys.executable, "-I", CHECK, "--workflow"], capture_output=True, text=True)
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn("ci.yml: " + WITNESS, p.stdout)
        self.assertIn("nightly lane on Linux", p.stdout)
        self.assertIn("static inspection only", p.stdout)

    def test_real_workflow_with_crlf_line_ends(self):
        with open(os.path.join(ROOT, ".github", "workflows", "ci.yml"), encoding="utf-8") as f:
            self.assertPasses(f.read(), newline="\r\n")

    def test_base(self):
        out = self.assertPasses(BASE)
        self.assertIn("full gate on Linux, Windows", out)

    def test_literal_block(self):
        self.assertPasses(gate("        run: |\n          rustup show\n          scripts/verify.sh --package --bench\n"))

    def test_literal_block_strip_and_comment(self):
        self.assertPasses(gate("        run: |-  # the gate\n          scripts/verify.sh --package --bench\n\n"))

    def test_folded_block(self):
        self.assertPasses(gate("        run: >\n          scripts/verify.sh\n          --package\n          --bench\n"))

    def test_folded_block_keep(self):
        self.assertPasses(gate("        run: >+\n          scripts/verify.sh --package\n          --bench\n\n"))

    def test_quoted_script_path(self):
        self.assertPasses(gate("        run: '\"scripts/verify.sh\" --package --bench'\n"))

    def test_double_quoted_yaml_scalar(self):
        self.assertPasses(gate("        run: \"bash 'scripts/verify.sh' --package --bench\"\n"))

    def test_bash_and_dot_slash(self):
        self.assertPasses(gate("        run: bash ./scripts/verify.sh --bench --package\n"))

    def test_backslash_continuation(self):
        self.assertPasses(gate("        run: |\n          scripts/verify.sh \\\n            --package --bench\n"))

    def test_sequence_at_key_indentation(self):
        text = BASE.replace("      - ", "    - ").replace("        run", "      run") \
            .replace("        shell", "      shell").replace("        name", "      name") \
            .replace("        if", "      if").replace("          for log", "        for log")
        self.assertPasses(text)

    def test_success_and_always_conditions(self):
        text = variant("      - name: gate\n", "      - name: gate\n        if: ${{ success() }}\n")
        self.assertPasses(text)
        self.assertPasses(variant("      - name: gate\n", "      - name: gate\n        if: always()\n"))

    def test_safe_shell_options(self):
        for options in ("-e", "-eu", "-eux", "-o pipefail", "-eo pipefail", "-euo pipefail", "-o errexit"):
            with self.subTest(options=options):
                self.assertPasses(gate(f"        run: |\n          set {options}\n          scripts/verify.sh --package --bench\n"))

    def test_shell_from_defaults(self):
        text = variant("      - name: gate\n        shell: bash\n", "      - name: gate\n")
        text = variant("jobs:\n", "defaults:\n  run:\n    shell: bash\njobs:\n", text)
        self.assertPasses(text)

    def test_cargo_inside_an_echo_string_is_not_a_command(self):
        self.assertPasses(variant("      - run: rustup show\n", "      - run: echo \"cargo test runs in verify.sh\"\n"))

    def test_single_platform_jobs(self):
        text = variant("    strategy:\n      fail-fast: false\n      matrix:\n        os: [ubuntu-latest, windows-latest]\n"
                       "    runs-on: ${{ matrix.os }}\n", "    runs-on: windows-latest\n")
        linux = "  linux:\n    runs-on: ubuntu-latest\n    steps:\n      - run: scripts/verify.sh --package --bench\n"
        self.assertPasses(variant("  nightly:\n", linux + "  nightly:\n", text))


class Fails(unittest.TestCase):
    def assertFails(self, text, *messages, witness=False):
        code, out = guard(text)
        self.assertEqual(code, 1, out)
        for message in messages:
            self.assertIn(message, out)
        if not witness:
            self.assertNotIn(WITNESS, out)
        return out

    NO_GATE = ("no effective full gate on Linux", "no effective full gate on Windows")

    def test_comment_only_yaml(self):
        out = self.assertFails(gate("        # run: scripts/verify.sh --package --bench\n"
                                    "        run: echo \"verification intentionally omitted\"\n"), *self.NO_GATE)
        self.assertIn("a YAML comment mentions verify.sh (inert)", out)

    def test_comment_only_shell(self):
        out = self.assertFails(gate("        run: |\n          # scripts/verify.sh --package --bench\n"
                                    "          echo skipped\n"), *self.NO_GATE)
        self.assertIn("a shell comment mentions verify.sh (inert)", out)

    def test_echo_string(self):
        out = self.assertFails(gate("        run: echo \"bash scripts/verify.sh --package --bench\"\n"), *self.NO_GATE)
        self.assertIn("not a gate invocation", out)

    def test_missing_gate(self):
        self.assertFails(variant("      - name: gate\n        shell: bash\n" + GATE, ""), *self.NO_GATE)

    def test_step_if_false(self):
        self.assertFails(variant("      - name: gate\n", "      - name: gate\n        if: false\n"),
                         *self.NO_GATE, "the step has if: false (statically disabled)")

    def test_step_if_expression_false(self):
        self.assertFails(variant("      - name: gate\n", "      - name: gate\n        if: ${{ false }}\n"),
                         *self.NO_GATE, "(statically disabled)")

    def test_job_if_false(self):
        self.assertFails(variant("    runs-on: ${{ matrix.os }}\n", "    runs-on: ${{ matrix.os }}\n    if: false\n"),
                         *self.NO_GATE, "job verify has if: false (statically disabled)")

    def test_job_if_expression_false(self):
        self.assertFails(variant("    runs-on: ${{ matrix.os }}\n", "    runs-on: ${{ matrix.os }}\n    if: ${{ false }}\n"),
                         *self.NO_GATE, "(statically disabled)")

    def test_needs_a_disabled_job(self):
        text = variant("    runs-on: ${{ matrix.os }}\n", "    runs-on: ${{ matrix.os }}\n    needs: setup\n")
        text = variant("jobs:\n", "jobs:\n  setup:\n    if: false\n    runs-on: ubuntu-latest\n"
                       "    steps:\n      - run: echo setup\n", text)
        self.assertFails(text, *self.NO_GATE, "job setup has if: false (statically disabled)")

    def test_dynamic_condition(self):
        self.assertFails(variant("      - name: gate\n", "      - name: gate\n        if: github.event_name == 'push'\n"),
                         *self.NO_GATE, "(unsupported condition)")

    def test_missing_windows(self):
        out = self.assertFails(variant("[ubuntu-latest, windows-latest]", "[ubuntu-latest]"),
                               "no effective full gate on Windows", witness=True)
        self.assertNotIn("no effective full gate on Linux", out)
        self.assertIn("effective full gate on: Linux", out)

    def test_missing_linux(self):
        out = self.assertFails(variant("[ubuntu-latest, windows-latest]", "[windows-latest]"),
                               "no effective full gate on Linux")
        self.assertNotIn("no effective full gate on Windows", out)

    def test_unknown_runner_label(self):
        self.assertFails(variant("[ubuntu-latest, windows-latest]", "[ubuntu-latest, self-hosted]"),
                         *self.NO_GATE, "unsupported runner label 'self-hosted'")

    def test_matrix_include(self):
        self.assertFails(variant("        os: [ubuntu-latest, windows-latest]\n",
                                 "        os: [ubuntu-latest]\n        include:\n          - os: windows-latest\n"),
                         *self.NO_GATE, "matrix include/exclude is unsupported")

    def test_or_true(self):
        self.assertFails(gate("        run: scripts/verify.sh --package --bench || true\n"),
                         *self.NO_GATE, "shell operator `||` next to the gate")

    def test_semicolon_true(self):
        self.assertFails(gate("        run: scripts/verify.sh --package --bench; true\n"),
                         *self.NO_GATE, "shell operator `;` next to the gate")

    def test_or_on_the_next_line(self):
        self.assertFails(gate("        run: |\n          scripts/verify.sh --package --bench ||\n          true\n"),
                         *self.NO_GATE, "shell operator `||` next to the gate")

    def test_skipped_by_and(self):
        self.assertFails(gate("        run: false && scripts/verify.sh --package --bench\n"),
                         *self.NO_GATE, "shell operator `&&` next to the gate")

    def test_pipe_and_background(self):
        self.assertFails(gate("        run: scripts/verify.sh --package --bench | tee gate.log\n"),
                         *self.NO_GATE, "shell operator `|` next to the gate")
        self.assertFails(gate("        run: scripts/verify.sh --package --bench &\n"),
                         *self.NO_GATE, "shell operator `&` next to the gate")

    def test_redirection(self):
        self.assertFails(gate("        run: scripts/verify.sh --package --bench > gate.log\n"),
                         *self.NO_GATE, "redirection or subshell")

    def test_step_continue_on_error(self):
        self.assertFails(variant("      - name: gate\n", "      - name: gate\n        continue-on-error: true\n"),
                         *self.NO_GATE, "the step has continue-on-error: true, which swallows")

    def test_job_continue_on_error(self):
        self.assertFails(variant("    runs-on: ${{ matrix.os }}\n", "    runs-on: ${{ matrix.os }}\n    continue-on-error: true\n"),
                         *self.NO_GATE, "the job has continue-on-error: true, which swallows")

    def test_set_plus_e(self):
        self.assertFails(gate("        run: |\n          set +e\n          scripts/verify.sh --package --bench\n          echo done\n"),
                         *self.NO_GATE, "`set +e` disables errexit")

    def test_set_noexec(self):
        # Final review R2: `set -n` makes Bash read the gate without running it (exit 0).
        for option in ("-n", "-o noexec", "-en", "-eo noexec"):
            with self.subTest(option=option):
                self.assertFails(gate(f"        run: |\n          set {option}\n          scripts/verify.sh --package --bench\n"),
                                 *self.NO_GATE, "turns on noexec: the step reads its commands without running them")

    def test_set_noexec_behind_prefixes(self):
        # Re-review T1: a leading assignment or `builtin`/`command` still runs `set`.
        for line in ("builtin set -n", "command set -o noexec", "REVIEW_OPTION=1 set -n", "command -p set -n"):
            with self.subTest(line=line):
                self.assertFails(gate(f"        run: |\n          {line}\n          scripts/verify.sh --package --bench\n"),
                                 *self.NO_GATE, "turns on noexec: the step reads its commands without running them")
        self.assertFails(gate("        run: |\n          builtin set +e\n          scripts/verify.sh --package --bench\n"),
                         *self.NO_GATE, "disables errexit")

    def test_control_command_behind_prefixes(self):
        # Re-review T2: a prefixed `exit`/`exec` still runs before the gate.
        for line in ("builtin exit 0", "command exit 0", "REVIEW_OPTION=1 exit 0", "builtin exec true"):
            with self.subTest(line=line):
                word = line.split()[-2] if line.endswith("0") else "exec"
                self.assertFails(gate(f"        run: |\n          {line}\n          scripts/verify.sh --package --bench\n"),
                                 *self.NO_GATE, f"unsupported shell construct `{word}`")

    def test_set_unmodelled_option(self):
        self.assertFails(gate("        run: |\n          set -f\n          scripts/verify.sh --package --bench\n"),
                         *self.NO_GATE, "changes shell option `-f`, which the guard does not model")

    def test_set_plus_o_errexit(self):
        self.assertFails(gate("        run: |\n          set +o errexit\n          scripts/verify.sh --package --bench\n"),
                         *self.NO_GATE, "disables errexit")

    def test_compound_command(self):
        self.assertFails(gate("        run: |\n          if false; then\n            scripts/verify.sh --package --bench\n          fi\n"),
                         *self.NO_GATE, "unsupported shell construct `if`")

    def test_early_exit(self):
        self.assertFails(gate("        run: |\n          exit 0\n          scripts/verify.sh --package --bench\n"),
                         *self.NO_GATE, "unsupported shell construct `exit`")

    def test_here_document(self):
        self.assertFails(gate("        run: |\n          cat <<EOF\n          scripts/verify.sh --package --bench\n          EOF\n"),
                         *self.NO_GATE, "unsupported shell construct (a here-document)")

    def test_ineffective_bash_option(self):
        out = self.assertFails(gate("        run: bash -n scripts/verify.sh --package --bench\n"), *self.NO_GATE)
        self.assertIn("not a gate invocation: bash -n scripts/verify.sh", out)

    def test_incomplete_inventory(self):
        self.assertFails(gate("        run: scripts/verify.sh --package\n"),
                         *self.NO_GATE, "an incomplete inventory")

    def test_unknown_argument(self):
        self.assertFails(gate("        run: scripts/verify.sh --package --bench --quick\n"),
                         *self.NO_GATE, "arguments ['--quick']")

    def test_expansion(self):
        self.assertFails(gate("        run: scripts/verify.sh --package --bench $EXTRA\n"),
                         *self.NO_GATE, "not a supported invocation")

    def test_windows_without_bash(self):
        self.assertFails(variant("      - name: gate\n        shell: bash\n", "      - name: gate\n"),
                         *self.NO_GATE, "must set `shell: bash`")

    def test_other_shell(self):
        self.assertFails(variant("      - name: gate\n        shell: bash\n", "      - name: gate\n        shell: pwsh\n"),
                         *self.NO_GATE, "unsupported shell 'pwsh'")

    def test_bash_env(self):
        self.assertFails(variant("      - name: gate\n", "      - name: gate\n        env:\n          BASH_ENV: x.sh\n"),
                         *self.NO_GATE, "['BASH_ENV'] in the environment")

    def test_reusable_workflow_is_not_a_gate(self):
        text = variant("    strategy:\n", "    uses: ./.github/workflows/gate.yml\n    strategy:\n")
        out = self.assertFails(text, *self.NO_GATE)
        self.assertIn("reusable workflow", out)

    def test_cargo_command_in_a_step(self):
        self.assertFails(variant("      - run: rustup show\n", "      - run: cargo +nightly test --workspace\n"),
                         "a cargo command outside scripts/verify.sh", witness=True)

    def test_platform_selection(self):
        self.assertFails(variant("      - name: gate\n", "      - name: gate\n        if: runner.os == 'Linux'\n"),
                         "selects gates by platform")

    def test_no_nightly_lane(self):
        text = variant("        run: scripts/verify.sh --nightly-only\n", "        run: echo nightly\n")
        self.assertFails(text, "no nightly lane", witness=True)

    def test_trigger_missing(self):
        self.assertFails(variant("on:\n  push:\n  pull_request:\n", "on:\n  workflow_dispatch:\n"),
                         "does not run on push", "does not run on pull_request", witness=True)

    def test_trigger_filter(self):
        self.assertFails(variant("  push:\n", "  push:\n    branches: [main]\n"),
                         "unsupported trigger filter on push", witness=True)

    # Outside the supported YAML subset: rejected with a line, never guessed.

    def test_anchor_and_alias(self):
        self.assertFails(variant("    runs-on: ${{ matrix.os }}\n", "    runs-on: &os ${{ matrix.os }}\n"),
                         "unsupported workflow syntax: anchors, aliases and tags")

    def test_tab_indentation(self):
        self.assertFails(variant("        shell: bash\n        run: scripts/verify.sh --package --bench\n",
                                 "        shell: bash\n\trun: scripts/verify.sh --package --bench\n"),
                         "unsupported workflow syntax: a tab in indentation")

    def test_flow_mapping(self):
        self.assertFails(variant("      - run: rustup show\n", "      - {run: rustup show}\n"),
                         "unsupported workflow syntax: flow mappings")

    def test_duplicate_key(self):
        self.assertFails(gate(GATE + "        run: echo replaced\n"), "duplicate key 'run'")

    def test_multiline_plain_scalar(self):
        self.assertFails(gate("        run: scripts/verify.sh\n          --package --bench\n"),
                         "a scalar continued on another line")

    def test_merge_key(self):
        self.assertFails(variant("    runs-on: ${{ matrix.os }}\n", "    runs-on: ${{ matrix.os }}\n    <<: {}\n"),
                         "merge keys")

    def test_document_marker(self):
        self.assertFails("---\n" + BASE, "directives and document markers")


class Parser(unittest.TestCase):
    def parse(self, text):
        return check_ci.parse_yaml(text)[0]

    def test_scalars(self):
        doc = self.parse("a: true\nb: ~\nc: 12\nd: 'it''s'\ne: \"x\\ty\\u00e9\"\nf: on\ng: a#b # c\nh: {}\n")
        self.assertEqual(doc, {"a": True, "b": None, "c": 12, "d": "it's", "e": "x\ty\u00e9",
                               "f": "on", "g": "a#b", "h": {}})

    def test_flow_sequence(self):
        self.assertEqual(self.parse("os: [ubuntu-latest, 'windows-latest', \"macos-14\"]  # c\n"),
                         {"os": ["ubuntu-latest", "windows-latest", "macos-14"]})

    def test_literal_chomping(self):
        self.assertEqual(self.parse("a: |\n  x\n   y\n\nb: |-\n  x\n\nc: |+\n  x\n\n"),
                         {"a": "x\n y\n", "b": "x", "c": "x\n\n"})

    def test_folding(self):
        self.assertEqual(self.parse("a: >\n  one\n  two\n\n  three\n    kept\n  four\n"),
                         {"a": "one two\nthree\n  kept\nfour\n"})

    def test_nested_sequences_and_mappings(self):
        self.assertEqual(self.parse("s:\n- a: 1\n  b:\n    - x\n    - y\n- plain\nt: 2\n"),
                         {"s": [{"a": 1, "b": ["x", "y"]}, "plain"], "t": 2})


if __name__ == "__main__":
    unittest.main()
