#!/usr/bin/env python3
"""Build a source-bound V3 return from real Git history and native evidence."""
from __future__ import annotations

import argparse
import contextlib
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import sys
import time
import zipfile

sys.path.insert(0, str(Path(__file__).resolve().parent / "qualification"))
from evidence import VisibleHtml, digest, environment, git, record, snapshot, utc, write_json
from qualification_attribution import parse_passes
from qualification_ci_logs import collect_run, github_bytes, github_json, inspect_native_zip, validate_run

BASE = "ae6006184f3dc903fdfe66c572098ac810e2c6c5"
MAIN = "3765907aa2e424997bfd8f2fcdcc95275e6142f5"
MEASURED = "579e8eac9a436a57ebc5ea3fcc2e7da3cf330c31"
TESTED = "2f647826dfefb61a6e5a6770f8af2c523198011e"
QUALIFICATION_RUN = 38981477842
ATTRIBUTION_RUN = 38983860434
CAPTURE_RUN = 38985826352
CAPTURE_SOURCE = "2bba4e3797ece2f8254efd84ef31640605b65ff8"
PREVIOUS_RETURN_SOURCE = "57c9ec273464dfddee1d91052e86f37b939e4c0f"
REPORTS = "dev_doc/reviews/2026_10_17_closure"
HISTORY = [
    "dev_doc/history/CONCORD_V2_1_RELEASE_READINESS_REMEDIATION_PLAN.md",
    "dev_doc/history/concord_v2_1_second_release_readiness_audit.md",
]
ARTIFACTS = [
    (QUALIFICATION_RUN, MEASURED, "v3-linux-qualification-38981477842-1"),
    (QUALIFICATION_RUN, MEASURED, "v3-runtime-38981477842-1"),
    (QUALIFICATION_RUN, MEASURED, "v3-scale-38981477842-1"),
    (ATTRIBUTION_RUN, TESTED, "v3-attribution-runtime-38983860434-1"),
    (ATTRIBUTION_RUN, TESTED, "v3-attribution-compiler-38983860434-1"),
    (ATTRIBUTION_RUN, TESTED, "v3-ci-history-38983860434-1"),
    (CAPTURE_RUN, CAPTURE_SOURCE, "v3-required-body-ui-38985826352-1"),
]
UI_CAPTURE_BLOBS = {
    "crates/concord/tests/ui.rs": "9e34756eb8f17a5c1de52a05d85054d727560b7e",
    "crates/concord/tests/ui/fail/capabilities/required_body_argument_extra.rs": "407e2f0f8f39990440b282d682878750ab9f967c",
    "crates/concord/tests/ui/fail/capabilities/required_body_argument_extra.stderr": "d614bda85528a4c080305752dda31a011053b467",
    "crates/concord/tests/ui/fail/capabilities/required_body_argument_missing.rs": "39b45bfe41c74765d1332837006d7680b052b582",
    "crates/concord/tests/ui/fail/capabilities/required_body_argument_missing.stderr": "ba8f1e107b2a58eff98ce18d28a46ce3a52ce444",
}
CLI_CORRECTION_BLOBS = {
    "tools/xtask/src/contract/mod.rs": "bdcec2106e6c7fb1cc3371f9816ec86cfda59ab7",
    "tools/xtask/tests/contract_cli.rs": "9207bf9ac4c935a2868b4d28b488aaf384db51d9",
}
DOC_ONLY_PATH = "crates/concord-core/src/value.rs"
DOC_OLD = b"///   setter or constructor call, before any request exists."
DOC_NEW = b"///   setter or constructor call, before request execution."
REQUIRED_REPORTS = {
    "CONCORD_V3_FINAL_REPORT.md", "PERFORMANCE_REPORT.md",
    "PERFORMANCE_RUNTIME.csv", "PERFORMANCE_SCALE.csv", "PERFORMANCE_COMPILER.csv",
    "NATIVE_QUALIFICATION_REPORT.md", "V3_ONLY_REPORT.md",
    "REVIEW_DISPOSITION.csv", "RUSTDOC_REVIEW.md",
}
CI_JOBS = {
    "Quality and policy", "Tests and examples", "Documentation",
    "Stable compiler contracts", "MSRV product libraries",
    "Fixtures and dependency policy", "Platform smoke (ubuntu-latest)",
    "Platform smoke (windows-latest)", "Rust analyzer",
    "Windows implementation qualification",
}


def safe_relative(name):
    path = PurePosixPath(name)
    if not name or path.is_absolute() or "\\" in name or any(p in {"", ".", ".."} for p in name.split("/")):
        raise ValueError("noncanonical relative path")
    return Path(*path.parts)


def command_passes(receipt, expected_exit):
    return (receipt["native_exit_code"] == expected_exit
            and not receipt["launcher_error"] and not receipt["timed_out"]
            and not receipt["end_snapshot_error"] and not receipt["digest_errors"]
            and receipt["inputs_changed"] is False
            and receipt["source_clean_at_start"] is True
            and receipt["source_clean_at_end"] is True)


def native(args, label, command, expected_exit=0, required_text=()):
    destination = args.package / "evidence/final/native"
    with open(os.devnull, "w") as sink, contextlib.redirect_stdout(sink):
        record(args.source, destination, label, command, False, 5400)
    receipt = json.loads((destination / (label + ".json")).read_text())
    output = (destination / (label + ".log")).read_text()
    outcome = {
        "label": label, "expected_exit": expected_exit,
        "observed_exit": receipt["native_exit_code"],
        "passed": command_passes(receipt, expected_exit)
                  and all(value in output for value in required_text),
        "required_text": list(required_text), "receipt": receipt,
    }
    args.outcomes.append(outcome)
    print("FINAL_RETURN_COMMAND " + json.dumps(outcome, sort_keys=True), flush=True)
    return output


def tree(source, ref):
    output = subprocess.check_output(["git", "-C", str(source), "ls-tree", "-r", "-z", ref])
    result = {}
    for item in output.split(b"\0"):
        if not item:
            continue
        metadata, raw_name = item.split(b"\t", 1)
        mode, kind, oid = metadata.decode().split()
        name = raw_name.decode("utf-8")
        safe_relative(name)
        if kind != "blob" or mode not in {"100644", "100755"}:
            raise RuntimeError("unsupported source entry: " + name)
        result[name] = {"mode": mode, "git_blob": oid}
    return result


def product_path(name):
    return (name in {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "justfile"}
            or (name.startswith("crates/") and ("/src/" in name or name.endswith(("/Cargo.toml", "/build.rs"))))
            or name.startswith("tools/perf/"))


def input_equivalence(source, baseline, head, selector):
    earlier, later = tree(source, baseline), tree(source, head)
    names = sorted(name for name in earlier.keys() | later.keys() if selector(name))
    different = [name for name in names if earlier.get(name) != later.get(name)]
    return {"baseline": baseline, "candidate": head, "compared_files": len(names),
            "changed_files": different, "identical": not different,
            "comparison": "Git blob identity and mode in the explicitly selected scope; support CLI changes are qualified separately by final native commands and exact CI, not inherited from the library measurements"}


def verify_documentation_edit(source, head):
    def data(ref):
        return subprocess.check_output(["git", "-C", str(source), "cat-file", "blob", ref + ":" + DOC_ONLY_PATH])
    before, after = data(MEASURED), data(head)
    if before.count(DOC_OLD) != 1 or after != before.replace(DOC_OLD, DOC_NEW, 1):
        raise RuntimeError("the declared documentation-only edit has other source changes")
    return {
        "path": DOC_ONLY_PATH, "baseline": MEASURED, "candidate": head,
        "before_sha256": hashlib.sha256(before).hexdigest(),
        "after_sha256": hashlib.sha256(after).hexdigest(),
        "before_git_blob": git(source, "rev-parse", MEASURED + ":" + DOC_ONLY_PATH).strip(),
        "after_git_blob": git(source, "rev-parse", head + ":" + DOC_ONLY_PATH).strip(),
        "exact_single_documentation_comment_replacement": True,
        "old_line": DOC_OLD.decode(), "new_line": DOC_NEW.decode(),
        "qualification": "Runtime/compiler measurements are not repeated for a proved doc-comment-only edit. Final native Rustdoc regenerates and checks the changed public page.",
    }


def collect_corrected_rustdoc(args):
    path = Path(os.environ["CARGO_TARGET_DIR"]) / "doc/concord_core/contracts/trait.IntoValue.html"
    raw = path.read_bytes()
    parser = VisibleHtml()
    parser.feed(raw.decode("utf-8"))
    text = parser.text()
    if "before request execution" not in text or "before any request exists" in text:
        raise RuntimeError("corrected public conversion documentation was not rendered")
    destination = args.package / "evidence/final/rustdoc"
    destination.mkdir(parents=True, exist_ok=True)
    shutil.copy2(path, destination / path.name)
    (destination / "IntoValue.txt").write_text(text, encoding="utf-8")
    result = {
        "head": git(args.source, "rev-parse", "HEAD").strip(),
        "source_path": DOC_ONLY_PATH,
        "generated_path": "doc/concord_core/contracts/trait.IntoValue.html",
        "html_sha256": hashlib.sha256(raw).hexdigest(),
        "text_sha256": digest(destination / "IntoValue.txt"),
        "new_sentence_present": True, "old_sentence_absent": True,
        "native_command_receipt": "evidence/final/native/final-rustdoc.json",
        "review_scope": "The single corrected contract sentence in actual generated HTML; no browser-navigation or human result is inferred.",
    }
    write_json(destination / "correction-receipt.json", result)
    print("FINAL_RETURN_RUSTDOC " + json.dumps(result, sort_keys=True), flush=True)
    return result


def verify_reviewed_edits(source, head):
    entries = tree(source, head)
    result = {}
    for scope, expected in [("native_captured_ui_oracles", UI_CAPTURE_BLOBS),
                            ("contract_cli_correction", CLI_CORRECTION_BLOBS)]:
        observed = {name: entries.get(name, {}).get("git_blob") for name in expected}
        if observed != expected:
            raise RuntimeError("reviewed source edit changed: " + scope)
        result[scope] = {"files": observed, "exact_reviewed_blobs": True}
    result["qualification"] = (
        "The two UI diagnostics were produced natively and then passed without overwrite. "
        "The exact final CI reruns their maintained harness. The changed CLI is tested "
        "by its binary integration tests, explicit semantic controls and exact final CI."
    )
    return result


def collect_recent_ci(args):
    destination = args.package / "evidence/recent-ci"
    destination.mkdir(parents=True, exist_ok=True)
    records = []
    for number, run_id, sha in [
        (30, 38983860517, TESTED),
        (31, 38985826266, CAPTURE_SOURCE),
        (32, 38987720005, PREVIOUS_RETURN_SOURCE),
    ]:
        result = collect_run("CI", number, run_id, sha, destination, time.monotonic() + 120)
        records.append(result)
        if result["collection"] != "complete":
            raise RuntimeError("recent CI attempt is not terminal")
    write_json(destination / "index.json", records)
    return records


def export_source(args, head):
    entries = tree(args.source, head)
    manifest = {}
    for name, metadata in sorted(entries.items()):
        data = subprocess.check_output(["git", "-C", str(args.source), "cat-file", "blob", metadata["git_blob"]])
        path = args.package / "source" / safe_relative(name)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        path.chmod(int(metadata["mode"], 8) & 0o777)
        manifest[name] = {**metadata, "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}
    write_json(args.package / "manifests/source_sha256.json", {
        "commit": head, "tree": git(args.source, "rev-parse", head + "^{tree}").strip(),
        "scope": "Every tracked Git blob, including the two separately identified export-ignore history files. This source-and-native-evidence envelope differs from the maintained distributable source-export. It contains no private specification or private acceptance annex.",
        "files": manifest,
    })
    history = []
    for name in HISTORY:
        original = git(args.source, "rev-parse", BASE + ":" + name).strip()
        current = entries[name]["git_blob"]
        if current != original:
            raise RuntimeError("a preserved history blob changed")
        history.append({"path": name, "baseline_blob": original, **manifest[name]})
    return history


def bounded_member(archive, name, limit=64 * 1024 * 1024):
    info = archive.getinfo(name)
    if info.file_size > limit:
        raise RuntimeError("oversized selected artifact member")
    return archive.read(info)


def collect_previous_return(args):
    destination = args.package / "evidence/previous-return"
    result = collect_run(
        "V3 final source and evidence return", 1, 38987720123,
        PREVIOUS_RETURN_SOURCE, destination, time.monotonic() + 120,
    )
    if (result["collection"] != "complete" or result["attempt"] != 1
            or result["conclusion"] != "failure"):
        raise RuntimeError("the previous final-return attempt changed")
    artifacts = result["native_artifacts"]
    if (len(artifacts) != 1 or artifacts[0]["id"] != 12083827906
            or artifacts[0]["name"] != "v3-incomplete-return-evidence-38987720123-1"
            or artifacts[0]["sha256"] != "7865ec172972a302b333e2c3e8b9b5a7dbb9f66ebce73620f05f48ddb5966ef0"):
        raise RuntimeError("the preserved failed-return artifact changed")
    archive_path = destination / safe_relative(artifacts[0]["file"])
    member = "return/evidence/final/native/python-return-tests.log"
    with zipfile.ZipFile(archive_path) as archive:
        native_log = bounded_member(archive, member)
    log_sha256 = hashlib.sha256(native_log).hexdigest()
    if log_sha256 != "cdc1210a8eede76fcf79b1f199a9a2262678040e3f1a99992dec9c3e9472d0cc":
        raise RuntimeError("the previous failed Python test log changed")
    result["failed_python_log"] = {
        "artifact_id": artifacts[0]["id"], "member": member,
        "sha256": log_sha256, "bytes": len(native_log),
        "native_text": native_log.decode("utf-8"),
    }
    result["retention"] = (
        "The original failed return artifact is retained in full, including its "
        "native failed command receipts. Duplicated upstream evidence is intentional. "
        "This failed attempt is not relabeled as a successful return."
    )
    write_json(destination / "index.json", result)
    print("PREVIOUS_RETURN_PYTHON_TEST_LOG " + json.dumps(result["failed_python_log"], sort_keys=True), flush=True)
    return result


def collect_artifacts(args):
    records = []
    for run_id, sha, name in ARTIFACTS:
        run = github_json(f"actions/runs/{run_id}")
        if run["head_sha"] != sha or run["status"] != "completed" or run["conclusion"] != "success":
            raise RuntimeError("upstream qualification/attribution is not complete")
        values = github_json(f"actions/runs/{run_id}/artifacts")["artifacts"]
        matches = [value for value in values if value["name"] == name and not value["expired"]]
        if len(matches) != 1:
            raise RuntimeError("required source-bound artifact is unavailable or ambiguous")
        artifact = matches[0]
        if artifact["workflow_run"]["head_sha"] != sha:
            raise RuntimeError("artifact source binding changed")
        destination = args.package / "evidence/upstream" / (name + ".zip")
        downloaded = github_bytes(f"actions/artifacts/{artifact['id']}/zip", destination)
        if artifact.get("digest") != "sha256:" + downloaded["sha256"]:
            raise RuntimeError("upstream artifact SHA-256 mismatch")
        inspection = inspect_native_zip(destination)
        if not all(member["crc_verified"] for member in inspection["members"]):
            raise RuntimeError("upstream archive failed native CRC verification")
        records.append({"run": run, "artifact": artifact, "download": downloaded,
                        "path": destination.relative_to(args.package).as_posix()})
        if "attribution-compiler" in name:
            normalize_compiler_archive(args, destination)
    write_json(args.package / "evidence/upstream/index.json", records)
    return records


def normalize_compiler_archive(args, destination):
    with zipfile.ZipFile(destination) as archive:
        original = json.loads(bounded_member(archive, "compiler-attribution.json"))
    normalized = []
    for row in original["rows"]:
        native_lines = "\n".join(item["native_line"] for item in row["passes"])
        corrected = parse_passes(native_lines)
        if len(corrected) != len(row["passes"]) or any(
            old["seconds"] != new["seconds"] or old["native_line"] != new["native_line"]
            for old, new in zip(row["passes"], corrected)
        ):
            raise RuntimeError("normalization changed native timing values or lines")
        normalized.append({**row, "passes": corrected})
    write_json(args.package / "evidence/performance/compiler-attribution-normalized.json", {
        "source_archive_sha256": digest(destination), "rows": normalized,
        "derivation": "The original parser retained RSS text in phase names because this Rustc build uses a tab separator. Only the phase name is reparsed from the preserved native line. No time was changed and no new measurement is claimed.",
    })


def wait_for_ci(args, head):
    deadline = time.monotonic() + 7200
    destination = args.package / "evidence/final/ci"
    destination.mkdir(parents=True, exist_ok=True)
    while True:
        response = github_json(f"actions/runs?head_sha={head}&per_page=100")
        matches = [run for run in response["workflow_runs"]
                   if run["name"] == "CI" and run["event"] == "push"
                   and run["head_branch"] == "concord-v3" and run["head_sha"] == head]
        if len(matches) > 1:
            raise RuntimeError("ambiguous final CI push run")
        if matches and matches[0]["status"] == "completed":
            break
        if time.monotonic() >= deadline:
            raise RuntimeError("final CI did not complete within the collection bound")
        time.sleep(30)
    selected = matches[0]
    run_id, attempt = selected["id"], selected["run_attempt"]
    run = github_json(f"actions/runs/{run_id}", destination / "run-native.json")
    validate_run(run, "CI", selected["run_number"], run_id, head)
    jobs = github_json(f"actions/runs/{run_id}/attempts/{attempt}/jobs?per_page=100",
                       destination / "jobs-native.json")["jobs"]
    if {job["name"] for job in jobs} != CI_JOBS or len(jobs) != len(CI_JOBS):
        raise RuntimeError("final CI job set differs from the maintained contract")
    policy = next(job for job in jobs if job["name"] == "Fixtures and dependency policy")
    if any(job["status"] != "completed" or
           (job["name"] != policy["name"] and job["conclusion"] != "success") for job in jobs):
        raise RuntimeError("a required engineering CI job did not pass")
    github_bytes(f"actions/jobs/{policy['id']}/logs", destination / "dependency-policy-native.log")
    policy_text = (destination / "dependency-policy-native.log").read_text(errors="replace")
    clean = re.sub(r"\x1b\[[0-9;]*m", "", policy_text)
    failed_steps = [step["name"] for step in policy["steps"] if step["conclusion"] == "failure"]
    if policy["conclusion"] == "failure":
        if (failed_steps != ["Dependency and license policy"]
                or "advisories ok, bans ok, licenses FAILED, sources ok" not in clean
                or clean.count("error[rejected]") != 3
                or any(name not in clean for name in ["alloc-no-stdlib", "alloc-stdlib", "brotli", "BSD-3-Clause"])):
            raise RuntimeError("dependency failure is not the separately scoped historical license policy")
    elif policy["conclusion"] != "success":
        raise RuntimeError("unexpected dependency policy conclusion")
    archive = destination / "logs-native.zip"
    downloaded = github_bytes(f"actions/runs/{run_id}/attempts/{attempt}/logs", archive)
    write_json(destination / "archive-inspection.json", inspect_native_zip(archive))
    windows = next(job for job in jobs if job["name"] == "Windows implementation qualification")
    github_bytes(f"actions/jobs/{windows['id']}/logs", destination / "windows-native.log")
    final = github_json(f"actions/runs/{run_id}", destination / "run-after-native.json")
    if (final["head_sha"], final["run_attempt"], final["status"], final["conclusion"]) != (
            head, attempt, run["status"], run["conclusion"]):
        raise RuntimeError("CI attempt changed during collection")
    result = {"run_id": run_id, "run_attempt": attempt, "head_sha": head,
              "native_conclusion": run["conclusion"], "engineering_jobs_passed": True,
              "dependency_policy_conclusion": policy["conclusion"],
              "license_policy_exception": "Owner excluded license work; the original failure remains a failure.",
              "native_archive": downloaded, "jobs": jobs}
    write_json(destination / "adjudication.json", result)
    print("FINAL_RETURN_CI " + json.dumps(result, sort_keys=True), flush=True)
    return result


def contract_controls(args):
    executable = Path(os.environ["CARGO_TARGET_DIR"]) / "debug/concord-xtask"
    prefix = [str(executable), "contract"]
    folder = args.package / "evidence/final/contract"
    folder.mkdir(parents=True, exist_ok=True)
    listed = native(args, "contract-list", prefix + ["list"])
    expected = {"fixture-catalog", "catalog", "riot", "data-dragon"}
    if {line.split()[0] for line in listed.splitlines() if line.strip()} != expected:
        raise RuntimeError("contract registry differs from the canonical V3 targets")
    for target in sorted(expected):
        path = folder / (target + ".json")
        native(args, "contract-export-" + target,
               prefix + ["export", "--target", target, "--output", str(path)])
        first = path.read_bytes()
        document = json.loads(first)
        if document["format"] != "concord.contract/3":
            raise RuntimeError("export used an unsupported contract format")
        native(args, "contract-export-again-" + target,
               prefix + ["export", "--target", target, "--output", str(path)])
        if path.read_bytes() != first:
            raise RuntimeError("repeated contract export is not byte-identical")
        native(args, "contract-check-same-" + target,
               prefix + ["check", "--target", target, "--snapshot", str(path)],
               required_text=("matches the snapshot",))
    catalog_path = folder / "catalog.json"
    catalog = json.loads(catalog_path.read_bytes())
    selected = [
        endpoint for endpoint in catalog["endpoints"]
        if endpoint["id"]["value"] in {"catalog.item.find", "catalog.items.list"}
    ]
    if len(selected) != 2:
        raise RuntimeError("representative Catalog export endpoints changed")
    declaration = args.source / "crates/concord-examples/src/contracts/catalog_v3.rs"
    sample = {
        "head": git(args.source, "rev-parse", "HEAD").strip(),
        "format": catalog["format"], "client": catalog["client"],
        "selected_endpoints": selected,
        "export_sha256": digest(catalog_path),
        "declaration_path": declaration.relative_to(args.source).as_posix(),
        "declaration_sha256": digest(declaration),
        "review_scope": "Actual final native export for cross-reading against declaration, generated caller/endpoint documentation and the retained Rustdoc pages; selection alone is not a reviewer verdict.",
    }
    write_json(folder / "catalog-review-sample.json", sample)
    print("FINAL_RETURN_CATALOG_SAMPLE " + json.dumps(sample, sort_keys=True), flush=True)
    fixture = folder / "fixture-catalog.json"
    changed = folder / "fixture-edited.json"
    text = fixture.read_text()
    edited = text.replace('"text": "3"', '"text": "9"')
    if edited == text:
        raise RuntimeError("expected positive mutation control disappeared")
    changed.write_text(edited)
    native(args, "contract-check-different", prefix + ["check", "--target", "fixture-catalog", "--snapshot", str(changed)],
           1, ("differs from the snapshot", "Provenance"))
    report = native(args, "contract-diff-json", prefix + ["diff", str(changed), str(fixture), "--format", "json"])
    diff = json.loads(report)
    if diff["format"] != "concord.contract-diff/3" or diff["verdict"] != "different" or "Wire" not in report:
        raise RuntimeError("semantic diff positive control failed")
    native(args, "contract-diff-identical", prefix + ["diff", str(fixture), str(fixture), "--format", "text"],
           required_text=("verdict structurally-equal",))
    for version in [1, 2]:
        retired = folder / f"unsupported-format-{version}.json"
        retired.write_text(json.dumps({"format": f"concord.contract/{version}"}))
        native(args, "contract-reject-format-" + str(version),
               prefix + ["check", "--target", "fixture-catalog", "--snapshot", str(retired)], 2)
    unused = folder / "must-not-exist.json"
    native(args, "contract-reject-alias",
           prefix + ["export", "--target", "catalog-v3", "--output", str(unused)],
           2, ("unknown contract target",))
    native(args, "contract-reject-migration", [str(executable), "migrate-v3"], 1, ("usage:",))
    for index, command in enumerate([
        ["export", "--target", "fixture-catalog", "--output", str(unused), "--format", "json"],
        ["check", "--target", "fixture-catalog", "--snapshot", str(fixture), "--format", "text"],
        ["diff", str(fixture), str(fixture), "--output", str(unused)],
        ["diff", str(fixture), str(fixture), "--snapshot", str(fixture)],
    ]):
        native(args, "contract-reject-unused-option-" + str(index), prefix + command, 2)
    if unused.exists():
        raise RuntimeError("a rejected command unexpectedly wrote its output")


def package_manifest(root):
    files = {}
    for path in sorted(root.rglob("*")):
        if path.is_file() and path.relative_to(root).as_posix() != "manifests/package_sha256.json":
            files[path.relative_to(root).as_posix()] = {"bytes": path.stat().st_size, "sha256": digest(path)}
    return {"scope": "All package files except this manifest, avoiding a circular self-hash.", "files": files}


def verify_package(root, manifest):
    observed = package_manifest(root)
    if observed != manifest:
        raise RuntimeError("package contents do not match the complete SHA-256 manifest")


def zip_package(root, destination):
    frozen = json.loads((root / "manifests/package_sha256.json").read_text())
    verify_package(root, frozen)
    expected = dict(frozen["files"])
    expected["manifests/package_sha256.json"] = {
        "bytes": (root / "manifests/package_sha256.json").stat().st_size,
        "sha256": digest(root / "manifests/package_sha256.json"),
    }
    with zipfile.ZipFile(destination, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=6) as archive:
        for path in sorted(root.rglob("*")):
            if path.is_file():
                archive.write(path, path.relative_to(root).as_posix())
    with zipfile.ZipFile(destination) as archive:
        if set(archive.namelist()) != set(expected) or len(archive.namelist()) != len(expected):
            raise RuntimeError("final ZIP member inventory changed")
        for name, metadata in expected.items():
            value = hashlib.sha256()
            size = 0
            with archive.open(name) as stream:
                while chunk := stream.read(1024 * 1024):
                    value.update(chunk)
                    size += len(chunk)
            if size != metadata["bytes"] or value.hexdigest() != metadata["sha256"]:
                raise RuntimeError("final ZIP member failed its SHA-256 manifest")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    args.source, args.out = args.source.resolve(), args.out.resolve()
    args.out.mkdir(parents=True, exist_ok=False)
    args.package = args.out / "return"
    args.package.mkdir()
    args.outcomes = []
    initial = snapshot(args.source)
    head = initial["head"]
    if initial["status"] or head != os.environ["GITHUB_SHA"]:
        raise RuntimeError("final return requires the clean workflow source commit")
    if git(args.source, "rev-parse", "main").strip() != MAIN:
        raise RuntimeError("local main differs from the preserved main")
    write_json(args.package / "evidence/final/environment.json", environment(args.source))
    previous_return = collect_previous_return(args)
    artifacts = collect_artifacts(args)
    equivalence = [
        input_equivalence(args.source, MEASURED, head, lambda p: product_path(p) and p != DOC_ONLY_PATH),
        input_equivalence(args.source, TESTED, head,
                          lambda p: p != DOC_ONLY_PATH and p not in UI_CAPTURE_BLOBS and p not in CLI_CORRECTION_BLOBS
                          and (p.startswith(("crates/", "fixtures/", "tests/", "tools/perf/", "tools/xtask/"))
                               or p in {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "justfile"})),
    ]
    if not all(value["identical"] for value in equivalence):
        raise RuntimeError("final report commit unexpectedly changed qualified product/test inputs")
    write_json(args.package / "evidence/final/source-equivalence.json", equivalence)
    reviewed_edits = verify_reviewed_edits(args.source, head)
    reviewed_edits["documentation_only"] = verify_documentation_edit(args.source, head)
    write_json(args.package / "evidence/final/reviewed-edits.json", reviewed_edits)
    native(args, "python-evidence-tests", [sys.executable, "-m", "unittest", "discover", "-s", "scripts/qualification", "-p", "test_*.py"])
    native(args, "python-attribution-tests", [sys.executable, "-m", "unittest", "discover", "-s", "scripts", "-p", "test_qualification_attribution.py"])
    native(args, "python-return-tests", [sys.executable, "-m", "unittest", "discover", "-s", "scripts", "-p", "test_qualification_final_return.py"])
    native(args, "final-format", ["cargo", "fmt", "--all", "--", "--check"])
    native(args, "final-xtask-tests", ["cargo", "test", "--locked", "-p", "concord-xtask", "--features", "examples"])
    native(args, "final-release-audit", ["just", "release-audit"])
    native(args, "final-source-export", ["just", "source-export"])
    native(args, "final-xtask-build", ["cargo", "build", "--locked", "-p", "concord-xtask", "--features", "examples"])
    native(args, "final-rustdoc", ["env", "RUSTDOCFLAGS=-D warnings", "cargo", "doc", "--locked", "-p", "concord-core", "--all-features", "--no-deps"])
    rustdoc = None
    try:
        rustdoc = collect_corrected_rustdoc(args)
    except Exception as error:
        args.outcomes.append({"label": "generated-rustdoc-semantic-check", "passed": False, "error": type(error).__name__ + ": " + str(error)})
    try:
        contract_controls(args)
    except Exception as error:
        args.outcomes.append({"label": "contract-semantic-controls", "passed": False, "error": type(error).__name__ + ": " + str(error)})
    source_exports = list((args.source / "target/source-export").glob("*.tar"))
    export_folder = args.package / "evidence/final/source-export"
    export_folder.mkdir(parents=True)
    for path in source_exports:
        shutil.copy2(path, export_folder / path.name)
    if not source_exports:
        args.outcomes.append({"label": "maintained-source-export-produced", "passed": False})
    write_json(args.package / "evidence/final/command-adjudication.json", args.outcomes)
    if not all(outcome["passed"] for outcome in args.outcomes):
        raise RuntimeError("one or more final native command/semantic checks failed; native receipts retained")
    ci = wait_for_ci(args, head)
    recent_ci = collect_recent_ci(args)
    history = export_source(args, head)
    if not REQUIRED_REPORTS.issubset({path.name for path in (args.source / REPORTS).iterdir() if path.is_file()}):
        raise RuntimeError("a required public source/qualification report is missing")
    for path in (args.source / REPORTS).iterdir():
        if path.is_file():
            shutil.copy2(path, args.package / path.name)
    integration = args.package / "integration"
    integration.mkdir()
    with (integration / "changes.patch").open("wb") as output:
        subprocess.run(["git", "-C", str(args.source), "format-patch", "--stdout", "--binary", "--full-index", BASE + ".." + head], stdout=output, check=True)
    commits = []
    for commit in git(args.source, "rev-list", "--reverse", BASE + ".." + head).splitlines():
        commits.append({"commit": commit,
                        "parent": git(args.source, "rev-parse", commit + "^").strip(),
                        "tree": git(args.source, "rev-parse", commit + "^{tree}").strip(),
                        "subject": git(args.source, "show", "-s", "--format=%s", commit).strip()})
    current_main = github_json("git/ref/heads/main")["object"]["sha"]
    current_branch = github_json("git/ref/heads/concord-v3")["object"]["sha"]
    final = snapshot(args.source)
    if current_main != MAIN or current_branch != head or final != initial:
        raise RuntimeError("source or branch identities changed during final verification")
    provenance = {
        "schema": "concord.v3.final-return/1", "created_utc": utc(),
        "repository": "gqsnt/concord_api", "branch": "concord-v3", "head": head,
        "parent": git(args.source, "rev-parse", "HEAD^").strip(), "tree": initial["tree"],
        "main": MAIN, "baseline": BASE, "commits": commits, "clean_checkout": True,
        "remote_branch_observed": current_branch, "remote_main_observed": current_main,
        "runtime_measurement_source": MEASURED, "strengthened_oracle_source": TESTED,
        "product_equivalence": equivalence, "reviewed_edits": reviewed_edits,
        "history": history, "ci": ci, "recent_ci": recent_ci, "previous_return": previous_return,
        "corrected_rustdoc": rustdoc,
        "artifacts": [{"id": item["artifact"]["id"], "name": item["artifact"]["name"],
                       "sha256": item["download"]["sha256"], "path": item["path"]} for item in artifacts],
        "product_scope": "V3 only; no migration, older contract format or shared-state import compatibility",
        "machine_qualification": "Native Linux and Windows, pin, genuine MSRV, packaging and downstream",
        "performance": "See the source-bound paired measurements and explicit residual-cost attribution",
        "package_integrity": "RECEIPT-BOUND: verified only by external FINAL_RETURN_RECEIPT.json after ZIP validation",
        "private_acceptance_annex_included": False,
        "delivery_boundary": "The private acceptance annex is delivered separately to its owner. It is deliberately absent from GitHub source, logs and artifacts; this archive does not claim to contain that annex.",
        "publication": "disabled", "version": "3.0.0-beta.1",
        "branch_push": "ordinary concord-v3 source pushes were authorized and performed",
        "main_merge": False, "tag_created": False, "registry_publication": False,
        "local_session_process_exit": "Unknown after executor disconnection; replaced by separately identified native CI results, never relabeled PASS.",
    }
    write_json(args.package / "manifests/provenance.json", provenance)
    report = args.package / "CONCORD_V3_FINAL_REPORT.md"
    with report.open("a", encoding="utf-8") as output:
        output.write("\n\n## Generated final envelope receipt\n\n")
        output.write(f"Verified commit: {head}\n\nTree: {initial['tree']}\n\nParent: {provenance['parent']}\n\n")
        output.write(f"Main preserved: {MAIN}\n\nFinal CI: https://github.com/gqsnt/concord_api/actions/runs/{ci['run_id']}\n\n")
        output.write("All final native command and semantic controls passed their explicitly recorded expected outcomes. Expected CLI exits 1 and 2 remain recorded as nonzero. The final CI license-policy failure is preserved separately. Source, history and clean-state checks passed before assembly. The public source/evidence package becomes verified only when the external FINAL_RETURN_RECEIPT.json records complete=true after the frozen-manifest and ZIP checks. The private acceptance annex is delivered separately and is not part of this GitHub artifact. See manifests/provenance.json and evidence/final/.\n")
    manifest = package_manifest(args.package)
    write_json(args.package / "manifests/package_sha256.json", manifest)
    verify_package(args.package, manifest)
    archive = args.out / "Concord_V3_Final_Return.zip"
    zip_package(args.package, archive)
    shutil.copy2(report, args.out / report.name)
    receipt = {
        "head": head, "tree": initial["tree"], "complete": True,
        "scope": "Public source, review reports and native qualification evidence",
        "private_acceptance_annex_included": False,
        "single_archive_including_private_annex_complete": False,
        "source_manifest_sha256": digest(args.package / "manifests/source_sha256.json"),
        "package_manifest_sha256": digest(args.package / "manifests/package_sha256.json"),
        "zip_sha256": digest(archive), "zip_bytes": archive.stat().st_size,
        "report_sha256": digest(args.out / report.name), "final_ci_run": ci["run_id"],
        "source_files": len(tree(args.source, head)), "package_files": len(manifest["files"]),
        "generated_utc": utc(),
    }
    write_json(args.out / "FINAL_RETURN_RECEIPT.json", receipt)
    print("FINAL_RETURN_COMPLETE " + json.dumps(receipt, sort_keys=True), flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
