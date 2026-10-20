#!/usr/bin/env python3
"""Capture two required-body arity diagnostics in a disposable native worktree.

No expected stderr is supplied by this driver. TRYBUILD=overwrite is used only
for capture; a second invocation must pass with overwrite removed. Product
sources and the active UI harness in the source checkout are never modified.
"""

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

from qualification import evidence

CANDIDATE = "579e8eac9a436a57ebc5ea3fcc2e7da3cf330c31"
TOOLCHAIN = "1.97.1"
NAMES = ("required_body_argument_missing", "required_body_argument_extra")
POSITIVE = "tests/ui/pass/language/current_language.rs"
UI_PREFIX = Path("crates/concord/tests/ui/fail/capabilities")
HARNESS = Path("crates/concord/tests/ui.rs")
CAPTURE_TEST = "required_body_arity_capture"
INSERT_AFTER = '        "tests/ui/fail/capabilities/absent_value_controls.rs",\n'


def native_receipt_ok(receipt):
    return (
        receipt["native_exit_code"] == 0
        and receipt["launcher_error"] is None
        and not receipt["timed_out"]
        and receipt["end_snapshot_error"] is None
        and not receipt["digest_errors"]
    )


def changed_files(before, after):
    left = before["input_files_sha256"]
    right = after["input_files_sha256"]
    return sorted(name for name in left.keys() | right.keys()
                  if left.get(name) != right.get(name))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--repository", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--candidate", default=CANDIDATE)
    args = parser.parse_args()
    if not re.fullmatch(r"[0-9a-fA-F]{40}", args.candidate):
        parser.error("--candidate must be one full commit SHA")

    repository = args.repository.resolve()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    if any(out.iterdir()):
        raise RuntimeError("the output directory must be empty")
    driver = Path(__file__).resolve().parent
    fixtures = driver / "ui_body_arity"
    os.environ["RUSTUP_TOOLCHAIN"] = TOOLCHAIN
    os.environ["CARGO_INCREMENTAL"] = "0"
    os.environ.setdefault("CARGO_BUILD_JOBS", "2")
    os.environ["CARGO_TERM_COLOR"] = "never"
    # A measurement/compile cache is not an evidence artifact.
    os.environ["CARGO_TARGET_DIR"] = str(
        Path(os.environ.get("RUNNER_TEMP", tempfile.gettempdir()))
        / "concord-ui-body-arity-target"
    )
    os.environ.pop("TRYBUILD", None)

    with tempfile.TemporaryDirectory(
        prefix="concord-ui-body-arity-",
        dir=os.environ.get("RUNNER_TEMP"),
    ) as temporary:
        source = Path(temporary) / "source"
        added = False
        try:
            subprocess.run(
                ["git", "-C", str(repository), "worktree", "add", "--detach",
                 str(source), args.candidate],
                check=True,
            )
            added = True
            baseline = evidence.snapshot(source)
            if baseline["head"] != args.candidate.lower() or baseline["status"]:
                raise RuntimeError("candidate checkout must be exact and clean")
            evidence.write_json(out / "baseline.inputs.json", baseline)
            evidence.write_json(out / "environment.json", evidence.environment(source))
            driver_identity = {
                "head": evidence.git(repository, "rev-parse", "HEAD").strip(),
                "tree": evidence.git(repository, "rev-parse", "HEAD^{tree}").strip(),
                "driver_sha256": evidence.digest(Path(__file__)),
                "recorder_sha256": evidence.digest(Path(evidence.__file__)),
                "candidate": baseline["head"],
                "candidate_tree": baseline["tree"],
                "toolchain": TOOLCHAIN,
            }
            evidence.write_json(out / "driver.json", driver_identity)

            original_ui = (source / HARNESS).read_text(encoding="utf-8")
            if original_ui.count(INSERT_AFTER) != 1:
                raise RuntimeError("the maintained UI harness insertion point changed")
            if CAPTURE_TEST in original_ui:
                raise RuntimeError("the capture test must not already exist")
            additions = "".join(
                '        "tests/ui/fail/capabilities/' + name + '.rs",\n'
                for name in NAMES
            )
            final_ui = original_ui.replace(
                INSERT_AFTER, INSERT_AFTER + additions, 1
            )
            integration = out / "integration"
            integration.mkdir()
            (integration / "ui.rs").write_text(final_ui, encoding="utf-8")

            for name in NAMES:
                fixture_source = fixtures / (name + ".rs")
                destination = source / UI_PREFIX / (name + ".rs")
                if destination.exists() or destination.with_suffix(".stderr").exists():
                    raise RuntimeError("capture must start without an existing fixture/snapshot")
                shutil.copy2(fixture_source, destination)
            positive_path = source / "crates/concord" / POSITIVE
            if not positive_path.is_file():
                raise RuntimeError("maintained positive control is absent")

            capture_ui = original_ui + "\n#[test]\nfn " + CAPTURE_TEST + "() {\n"
            capture_ui += "    let cases = trybuild::TestCases::new();\n"
            capture_ui += '    cases.pass("' + POSITIVE + '");\n'
            for name in NAMES:
                capture_ui += (
                    '    cases.compile_fail("tests/ui/fail/capabilities/'
                    + name + '.rs");\n'
                )
            capture_ui += "}\n"
            (source / HARNESS).write_text(capture_ui, encoding="utf-8")
            (out / "capture-ui.rs").write_text(capture_ui, encoding="utf-8")
            shutil.copy2(positive_path, out / "positive-current-language.rs")
            evidence.write_json(
                out / "capture-plan.json",
                {
                    **driver_identity,
                    "positive_control": POSITIVE,
                    "positive_sha256": evidence.digest(positive_path),
                    "intentional_source_edits": [
                        HARNESS.as_posix(),
                        *[(UI_PREFIX / (name + ".rs")).as_posix() for name in NAMES],
                    ],
                    "expected_generated_files": [
                        (UI_PREFIX / (name + ".stderr")).as_posix() for name in NAMES
                    ],
                    "normal_harness_after_integration": (
                        "current_language_surface_and_diagnostics; two explicit "
                        "compile_fail entries in its capabilities list"
                    ),
                    "source_after_manual_fixture_setup": evidence.snapshot(source),
                },
            )
            command = [
                "cargo", "+" + TOOLCHAIN, "test", "--locked", "-p", "concord",
                "--test", "ui", "--all-features", CAPTURE_TEST,
                "--", "--exact", "--nocapture",
            ]
            os.environ["TRYBUILD"] = "overwrite"
            try:
                # False is expected because two stderr files are generated.
                # Preserve that receipt unchanged and validate only the declared
                # mutations below, never treating all dirty runs as acceptable.
                stable_capture = evidence.record(
                    source, out, "ui-body-arity-capture", command,
                    allow_dirty=True, timeout_seconds=1800,
                )
            finally:
                os.environ.pop("TRYBUILD", None)

            receipt = json.loads((out / "ui-body-arity-capture.json").read_text())
            before = json.loads((out / "ui-body-arity-capture.inputs.json").read_text())
            after_path = out / "ui-body-arity-capture.end-inputs.json"
            if not native_receipt_ok(receipt) or not after_path.is_file():
                raise RuntimeError("native capture did not complete; inspect its unchanged receipt")
            after = json.loads(after_path.read_text())
            expected = sorted(
                (UI_PREFIX / (name + ".stderr")).as_posix() for name in NAMES
            )
            changes = changed_files(before, after)
            if (before["head"] != after["head"] or before["tree"] != after["tree"]
                    or changes != expected):
                raise RuntimeError("native capture changed inputs beyond the two expected stderr files")

            snapshots = []
            for name in NAMES:
                path = source / UI_PREFIX / (name + ".stderr")
                content = path.read_text(encoding="utf-8")
                errors = re.findall(r"^error(?:\[[A-Z0-9]+\])?:", content, re.MULTILINE)
                if errors != ["error[E0061]:"]:
                    raise RuntimeError("capture must contain exactly the required E0061 arity error")
                for suffix in (".rs", ".stderr"):
                    shutil.copy2(
                        source / UI_PREFIX / (name + suffix),
                        integration / (name + suffix),
                    )
                snapshot = {
                    "name": name,
                    "final_path": (UI_PREFIX / (name + ".stderr")).as_posix(),
                    "sha256": evidence.digest(path),
                    "bytes": path.stat().st_size,
                    "text": content,
                }
                snapshots.append(snapshot)
                print("QUALIFICATION_UI_ARITY_STDERR " + json.dumps(snapshot), flush=True)

            evidence.write_json(
                out / "capture-adjudication.json",
                {
                    "native_capture_exit_code": receipt["native_exit_code"],
                    "stable_input_recorder_result": stable_capture,
                    "intentional_generated_files_only": changes,
                    "snapshots": snapshots,
                    "overwrite_used_for_capture_only": True,
                    "final_harness_sha256": evidence.digest(integration / "ui.rs"),
                },
            )
            if not evidence.record(
                source, out, "ui-body-arity-verify", command,
                allow_dirty=True, timeout_seconds=1800,
            ):
                raise RuntimeError("verification without TRYBUILD=overwrite failed")
            evidence.write_json(
                out / "result.json",
                {
                    **driver_identity,
                    "capture": "native E0061 snapshots generated",
                    "verification": "same isolated cases passed without overwrite",
                    "active_candidate_modified": False,
                    "final_integration_qualification": "still required after reviewed integration",
                    "integration_files": [
                        {"name": p.name, "sha256": evidence.digest(p)}
                        for p in sorted(integration.iterdir()) if p.is_file()
                    ],
                },
            )
        finally:
            if added:
                subprocess.run(
                    ["git", "-C", str(repository), "worktree", "remove", "--force", str(source)],
                    check=True,
                )
    return 0


if __name__ == "__main__":
    sys.exit(main())
