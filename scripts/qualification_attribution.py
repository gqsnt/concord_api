#!/usr/bin/env python3
"""Targeted source-bound attribution; diagnostic patches never enter the product.

Run after the paired qualification campaign. Runtime ablations restore one old
internal mechanism at a time. Compiler profiles reuse the actual archived scale
inputs. Instrumented timings are kept separate from the uninstrumented campaign.
"""

import argparse
import contextlib
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import statistics
import sys
import time
import tomllib
import zipfile

sys.path.insert(0, str(Path(__file__).resolve().parent / "qualification"))
from evidence import digest, environment, git, record, snapshot, utc, write_json
from performance import archive_inputs, build, locked_packages, parse_runtime, prepare
from qualification_ci_logs import github_bytes, github_json

RUN = 38981477842
CANDIDATE = "579e8eac9a436a57ebc5ea3fcc2e7da3cf330c31"
BEFORE = "ae6006184f3dc903fdfe66c572098ac810e2c6c5"
HISTORICAL = "3765907aa2e424997bfd8f2fcdcc95275e6142f5"


def quiet_record(root, out, label, command, timeout=2400):
    # The recorder itself retains the complete native stream. Print only the
    # receipt here so large rustc profiles remain retrievable through job logs.
    with open(os.devnull, "w") as sink, contextlib.redirect_stdout(sink):
        passed = record(root, out, label, command, True, timeout)
    receipt = json.loads((out / (label + ".json")).read_text())
    print("ATTRIBUTION_RECEIPT " + json.dumps(receipt, sort_keys=True), flush=True)
    if not passed:
        raise RuntimeError("native attribution command failed: " + label)
    return (out / (label + ".log")).read_text()


def replace_once(path, old, new):
    text = path.read_text()
    if text.count(old) != 1:
        raise RuntimeError("diagnostic patch anchor is not unique: " + str(path))
    path.write_text(text.replace(old, new, 1))


def clone_variant(args, roots, label):
    root = args.work / label
    import subprocess
    subprocess.run(["git", "worktree", "add", "--detach", str(root), args.candidate],
                   cwd=args.driver, check=True)
    roots[label] = root
    return root


def instrument_description(path):
    first = "pub(crate) fn build(builder: ContractBuilder) -> Result<ContractDescription, ContractBuildError> {\n"
    replace_once(path, first, first + "    let mut __qualification_times = [std::time::Instant::now(); 9];\n")
    anchors = [
        "    let secret_names = validate_secrets",
        "    // Each owner's selected operations",
        "    let mut endpoints = Vec::new();",
        "    detect_credential_cycles(&credentials",
        "    // Redact local literals under keys",
        "    let dependencies = collect_dependencies",
        "    let root_timeout = match",
    ]
    for index, anchor in enumerate(anchors, 1):
        replace_once(path, anchor,
                     f"    __qualification_times[{index}] = std::time::Instant::now();\n" + anchor)
    replace_once(path, "    Ok(ContractDescription {\n", "    let __qualification_result = Ok(ContractDescription {\n")
    old = "        endpoint_index: endpoint_index.into_boxed_slice(),\n    })\n}"
    new = '''        endpoint_index: endpoint_index.into_boxed_slice(),
    });
    __qualification_times[8] = std::time::Instant::now();
    let __qualification_stages = [
        "tree-values-references", "validation-and-attachments", "head-composition",
        "effective-endpoints", "cycles-coverage-liveness", "redaction-owner-views",
        "dependencies-diagnostics", "result-assembly",
    ];
    for (__qualification_index, __qualification_stage) in __qualification_stages.iter().enumerate() {
        eprintln!(
            "DESCRIPTION_STAGE {} {}",
            __qualification_stage,
            __qualification_times[__qualification_index + 1]
                .duration_since(__qualification_times[__qualification_index]).as_nanos(),
        );
    }
    __qualification_result
}'''
    replace_once(path, old, new)


def runtime_attribution(args, roots):
    wrapper = clone_variant(args, roots, "v3-wrapper-restored")
    path = wrapper / "crates/concord-core/src/response/handler.rs"
    anchor = "pub(crate) async fn acquire_buffered(input: ResponseInput) -> Result<Bytes, ApiError> {\n"
    replace_once(path, anchor, anchor + "    collect_bounded(input).await\n}\n\nasync fn collect_bounded(input: ResponseInput) -> Result<Bytes, ApiError> {\n")

    repeated = clone_variant(args, roots, "v3-composition-restored")
    path = "crates/concord-core/src/inspection/resolve.rs"
    prior_resolve = git(args.driver, "show", args.before + ":" + path)
    old_message = "deprecated value is retained as a no-op migration bridge"
    new_message = "deprecated value is retained as an explicit no-op value"
    if prior_resolve.count(old_message) != 1:
        raise RuntimeError("unexpected historical diagnostic spelling")
    # Restore only the old composition algorithm, preserving the current
    # V3-only public diagnostic wording in this isolated ablation.
    (repeated / path).write_text(prior_resolve.replace(old_message, new_message, 1))

    instrumented = clone_variant(args, roots, "v3-description-stages")
    instrument_description(instrumented / path)

    selected = ["v3-final", "v3-wrapper-restored", "v3-composition-restored",
                "v3-description-stages", "historical"]
    binaries = {}
    for label in selected:
        archive_inputs(roots[label], args.out / "inputs-before" / label)
        # build() emits full Cargo output, which is small for these probes.
        binaries[label] = build(args, roots[label], label, ["future_sizes", "head_first_use"])
    write_json(args.out / "binaries.json", binaries)

    samples = []
    cases = [
        ("future_sizes", None, ["v3-final", "v3-wrapper-restored"]),
        ("head_first_use", "plan-search", ["v3-final", "v3-composition-restored"]),
        ("head_first_use", "sequence", ["v3-final", "v3-composition-restored"]),
        ("head_first_use", "plan-search", ["v3-description-stages"]),
    ]
    for repeat in range(5):
        for index, (binary, category, labels) in enumerate(cases):
            order = labels if repeat % 2 == 0 else list(reversed(labels))
            for label in order:
                entry = binaries[label][binary]
                if digest(entry["path"]) != entry["sha256"]:
                    raise RuntimeError("attribution binary changed")
                os.environ["CARGO_TARGET_DIR"] = str(args.work / "targets" / ("release-" + label))
                name = f"r{repeat + 1}-{index}-{label}"
                output = quiet_record(roots[label], args.out / "runtime", name,
                                      [entry["path"]] + ([category] if category else []), 120)
                parsed = parse_runtime(binary, category, output)
                stages = []
                if label == "v3-description-stages":
                    stages = [{"stage": m[1], "nanoseconds": int(m[2])}
                              for m in re.finditer(r"^DESCRIPTION_STAGE ([a-z-]+) (\d+)$", output, re.M)]
                    if len(stages) != 8 or len({s["stage"] for s in stages}) != 8:
                        raise RuntimeError("missing or repeated description-stage trace")
                samples.append({"repeat": repeat + 1, "case": index, "candidate": label,
                                "probe": binary, "category": category, "receipt": name,
                                "diagnostic_instrumentation": bool(stages),
                                "description_stages": stages, **parsed})
    write_json(args.out / "runtime-attribution.json", {
        "samples": samples,
        "scope": "Ablation process pairs use the final product with exactly one mechanism restored. The stage-instrumented variant adds Instant checkpoints and emits after construction; its total operation timing/allocations are not comparable with the uninstrumented campaign."
    })
    print("ATTRIBUTION_RUNTIME " + json.dumps(samples, sort_keys=True), flush=True)

    # Apply the diagnostic flag to all compiled crates so hidden acquisition
    # coroutine layouts are present, then retain the native full layout dump.
    os.environ["RUSTC_BOOTSTRAP"] = "1"
    # Keep separate rustc processes from interleaving type/field records.
    os.environ["CARGO_BUILD_JOBS"] = "1"
    os.environ["RUSTFLAGS"] = "-Zprint-type-sizes"
    for label in ["historical", "v3-final", "v3-wrapper-restored"]:
        os.environ["CARGO_TARGET_DIR"] = str(args.work / "targets" / ("layout-" + label))
        output = quiet_record(roots[label], args.out / "layouts", label,
                              ["cargo", "build", "--release", "--locked", "-p", "concord-perf",
                               "--bin", "future_sizes"])
        blocks = parse_layouts(output)
        write_json(args.out / "layouts" / (label + "-types.json"), blocks)
        selected_blocks = [block for block in blocks if any(word in block["type"] for word in [
            "ResponseInput", "acquire_buffered", "collect_bounded", "execute_response",
            "execute_with", "execute_managed", "AttemptJournal", "PreparedRequest",
            "OperationContext", "ExecutionContext",
        ])]
        largest = sorted([block for block in blocks if "concord_core" in block["type"]], key=lambda block: block["bytes"], reverse=True)[:12]
        if label != "historical" and not any("acquire_buffered" in block["type"] for block in blocks):
            raise RuntimeError("buffered acquisition layout was not emitted")
        print("ATTRIBUTION_LAYOUTS " + json.dumps({"candidate": label, "types": selected_blocks, "largest_named_concord_core_types": largest}, sort_keys=True), flush=True)
    os.environ.pop("RUSTFLAGS", None)
    os.environ.pop("RUSTC_BOOTSTRAP", None)
    os.environ["CARGO_BUILD_JOBS"] = "2"


def parse_layouts(output):
    blocks = []
    current = None
    for line in output.splitlines():
        match = re.fullmatch(r"print-type-size type: \x60(.+)\x60: (\d+) bytes, alignment: (\d+) bytes", line)
        if match:
            current = {"type": match[1], "bytes": int(match[2]), "alignment": int(match[3]), "fields": []}
            blocks.append(current)
        elif current is not None and line.startswith("print-type-size "):
            current["fields"].append(line[len("print-type-size "):])
        else:
            current = None
    if not blocks:
        raise ValueError("rustc did not emit recognizable type-layout evidence")
    return blocks


def parse_passes(output):
    values = []
    for line in output.splitlines():
        match = re.match(r"time:\s*([0-9.]+);", line)
        if match:
            values.append({"phase": line.rsplit(";", 1)[-1].strip(),
                           "seconds": float(match[1]), "native_line": line})
    if not values or not any("type" in row["phase"] or "borrow" in row["phase"] for row in values):
        raise ValueError("rustc did not emit recognizable front-end timing evidence")
    return values


def fetch_scale_archive(args):
    # This job may start while the existing scale measurement job is finishing.
    deadline = time.monotonic() + 5400
    name = f"v3-scale-{RUN}-1"
    while True:
        response = github_json(f"actions/runs/{RUN}/artifacts")
        found = [a for a in response["artifacts"] if a["name"] == name and not a["expired"]]
        if found:
            if len(found) != 1:
                raise RuntimeError("ambiguous scale artifact")
            artifact = found[0]
            break
        run = github_json(f"actions/runs/{RUN}")
        if run["head_sha"] != args.candidate:
            raise RuntimeError("scale run is not bound to the selected candidate")
        if run["status"] == "completed" or time.monotonic() >= deadline:
            raise RuntimeError("completed scale artifact is unavailable")
        time.sleep(20)
    if artifact["workflow_run"]["head_sha"] != args.candidate:
        raise RuntimeError("scale artifact is not from the selected candidate")
    path = args.out / "scale-campaign.zip"
    github_bytes(f"actions/artifacts/{artifact['id']}/zip", path)
    if "sha256:" + digest(path) != artifact.get("digest"):
        raise RuntimeError("scale archive digest does not match GitHub")
    write_json(args.out / "scale-artifact.json", artifact)
    return path


def bounded_read(archive, name):
    info = archive.getinfo(name)
    if info.file_size > 64 * 1024 * 1024:
        raise RuntimeError("unexpectedly large selected evidence entry")
    return archive.read(info)


def fixture_inputs(fixture):
    return {name: digest(fixture / name) for name in ["Cargo.toml", "Cargo.lock", "src/main.rs"]}


def compiler_attribution(args, roots):
    archive_path = fetch_scale_archive(args)
    os.environ["RUSTC_BOOTSTRAP"] = "1"
    os.environ["CARGO_NET_OFFLINE"] = "true"
    rows = []
    with zipfile.ZipFile(archive_path) as archive:
        complete = json.loads(bounded_read(archive, "completed.json"))
        if complete["candidate"] != args.candidate or not complete["measurement_complete"]:
            raise RuntimeError("scale measurement did not finish")
        for label in ["historical", "v3-before", "v3-final"]:
            root = roots[label]
            prefix = f"scale-inputs/r01-{label}/"
            declared = json.loads(bounded_read(archive, prefix + "files.json"))
            by_name = {item["path"]: item for item in declared}
            originals = {}
            for name in ["Cargo.toml", "Cargo.lock", "point-1000-base.rs",
                         "point-1000-local.rs", "point-1000-parent.rs"]:
                data = bounded_read(archive, prefix + name)
                if hashlib.sha256(data).hexdigest() != by_name[name]["sha256"]:
                    raise RuntimeError("archived fixture entry fails its source manifest")
                originals[name] = data
            manifest = originals["Cargo.toml"].decode()
            dependencies = tomllib.loads(manifest)["dependencies"]
            previous = Path(dependencies["concord"]["path"]).parent.parent
            for name, spec in dependencies.items():
                if isinstance(spec, dict) and "path" in spec:
                    if name not in {"concord", "concord-inspect"} or Path(spec["path"]).parent != previous / "crates":
                        raise RuntimeError("unexpected fixture path dependency")
            fixture = root / "target/attribution-fixture"
            (fixture / "src").mkdir(parents=True, exist_ok=True)
            (fixture / "Cargo.toml").write_text(manifest.replace(str(previous), str(root)))
            (fixture / "Cargo.lock").write_bytes(originals["Cargo.lock"])
            if not locked_packages(fixture).issubset(locked_packages(roots["v3-final"])):
                raise RuntimeError("profile fixture dependencies are not aligned")
            os.environ["CARGO_TARGET_DIR"] = str(args.work / "targets" / ("compiler-" + label))
            for stage in ["base", "local", "parent"]:
                (fixture / "src/main.rs").write_bytes(originals[f"point-1000-{stage}.rs"])
                saved = args.out / "compiler-inputs" / label / stage
                (saved / "src").mkdir(parents=True, exist_ok=True)
                for name in ["Cargo.toml", "Cargo.lock", "src/main.rs"]:
                    shutil.copy2(fixture / name, saved / name)
                initial = fixture_inputs(fixture)
                name = label + "-" + stage
                quiet_record(root, args.out / "compiler", name + "-clean",
                             ["cargo", "clean", "--manifest-path", str(fixture / "Cargo.toml"),
                              "-p", "concord-expansion-scale"])
                output = quiet_record(root, args.out / "compiler", name,
                                      ["cargo", "rustc", "--locked", "--manifest-path",
                                       str(fixture / "Cargo.toml"), "--bin", "concord-expansion-scale",
                                       "--", "-Ztime-passes"])
                final = fixture_inputs(fixture)
                if final != initial:
                    raise RuntimeError("compiled fixture inputs changed during its profile")
                row = {"candidate": label, "stage": stage, "endpoints": 1000,
                       "fixture_inputs_before": initial, "fixture_inputs_after": final,
                       "receipt": name, "passes": parse_passes(output)}
                rows.append(row)
                print("ATTRIBUTION_COMPILER_PASSES " + json.dumps(row, sort_keys=True), flush=True)
    write_json(args.out / "compiler-attribution.json", {
        "rows": rows,
        "scope": "Native rustc -Ztime-passes for the actual archived 1000-endpoint fixture. Debug, unstripped, incremental=0, dependencies aligned, RUSTC_BOOTSTRAP=1. Phase wall times can nest and must not be summed as independent CPU costs. One profile per source/edit; timing trends come from the separate paired campaign."
    })
    os.environ.pop("RUSTC_BOOTSTRAP", None)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=["runtime", "compiler"])
    parser.add_argument("--driver", type=Path, required=True)
    parser.add_argument("--work", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    args.driver, args.work, args.out = args.driver.resolve(), args.work.resolve(), args.out.resolve()
    args.candidate, args.before, args.historical = CANDIDATE, BEFORE, HISTORICAL
    args.work.mkdir(parents=True, exist_ok=False)
    args.out.mkdir(parents=True, exist_ok=False)
    for name in ["RUSTFLAGS", "RUSTDOCFLAGS", "RUSTC_BOOTSTRAP", "CARGO_NET_OFFLINE",
                 "CARGO_PROFILE_DEV_DEBUG", "CARGO_PROFILE_TEST_DEBUG"]:
        os.environ.pop(name, None)
    os.environ["CARGO_BUILD_JOBS"] = "2"
    os.environ["CARGO_INCREMENTAL"] = "0"
    write_json(args.out / "environment-initial.json", environment(args.driver))
    roots = prepare(args)
    write_json(args.out / "environment-measurement.json", environment(args.driver))
    print("ATTRIBUTION_ENVIRONMENT " + json.dumps(environment(args.driver), sort_keys=True), flush=True)
    for label, root in roots.items():
        archive_inputs(root, args.out / "inputs-before" / label)
    try:
        (runtime_attribution if args.mode == "runtime" else compiler_attribution)(args, roots)
    finally:
        for label, root in roots.items():
            archive_inputs(root, args.out / "inputs-after" / label)
    shutil.rmtree(args.out / "binaries", ignore_errors=True)
    write_json(args.out / "completed.json", {
        "finished_utc": utc(), "driver": snapshot(args.driver), "candidate": CANDIDATE,
        "mode": args.mode, "attribution_complete": True,
    })
    return 0


if __name__ == "__main__":
    sys.exit(main())
