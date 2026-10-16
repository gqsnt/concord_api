#!/usr/bin/env python3
"""Reproduce the V3 correction measurements on one host with isolated inputs.

The optional historical checkout is a measurement input only. Its old language
is never accepted by the current product, fixtures, or command-line tools.
"""

import argparse
import json
import math
import os
from pathlib import Path
import re
import shutil
import statistics
import subprocess
import sys
import tomllib
from urllib.parse import urlsplit

from evidence import digest, environment, git, record, snapshot, utc, write_json

BINS = ["head_cost", "head_first_use", "future_sizes"]
CATEGORIES = [
    "client-build", "direct-construction", "unbound-construction", "binding",
    "plan-show", "plan-search", "plan-protected", "plan-limited", "execution",
]
WARM_NAMES = [
    "direct request construction (scope + search)",
    "unbound endpoint construction (scope + show)",
    "request binding (client.request)",
    "bound-head plan: scope + endpoint path (client.plan)",
    "bound-head plan: query program (search)",
    "bound-head plan: bearer placement (protected)",
    "bound-head plan: rate policy (limited)",
    "managed execution, client thread (loopback, keep-alive)",
]
SCALE_NAMES = [
    "endpoints", "lines", "characters", "fragments", "declarations", "methods",
    "expand_ms", "build_ms", "edit_ms", "binary_bytes", "description_bytes",
    "parent_edit_ms", "cold_ms", "warm_ms",
]


def locked_packages(root):
    packages = tomllib.loads((root / "Cargo.lock").read_text())["package"]
    return {(p["name"], p["version"], p.get("source", ""), p.get("checksum", "")) for p in packages if p.get("source")}


def parse_runtime(binary, category, output):
    values = {}
    heads = {}
    if binary == "head_cost":
        for line in [line for line in output.splitlines() if line.startswith("planned ")]:
            match = re.fullmatch(r"planned (show|search|protected|limited): (\S+) \((\d+) public header\(s\)\)", line)
            if match is None:
                raise ValueError("malformed or unexpected public plan")
            label, url, header_count = match.groups()
            parsed_url = urlsplit(url)
            if parsed_url.scheme != "https" or not parsed_url.hostname or parsed_url.username or parsed_url.password:
                raise ValueError("unexpected public-plan URL")
            if label in heads:
                raise ValueError("duplicate public plan")
            heads[label] = url + " (" + header_count + " public header(s))"
        if list(heads) != ["show", "search", "protected", "limited"]:
            raise ValueError("missing public-plan equivalence controls")
        for line in output.splitlines():
            cells = [c.strip() for c in line.strip().strip("|").split("|")]
            if len(cells) != 6 or cells[0] not in WARM_NAMES:
                continue
            if cells[0] in values:
                raise ValueError("duplicate warm operation")
            values[cells[0]] = dict(zip(
                ["allocations_per_op", "bytes_per_op", "net_allocations_per_batch", "median_ns_per_op", "min_ns_per_op"],
                map(float, cells[1:]),
            ))
        if list(values) != WARM_NAMES:
            raise ValueError("missing warm operation")
    elif binary == "future_sizes":
        sizes = re.findall(r"^boxed operation future \(IntoFuture, ExecApi::bounded\): (\d+) B$", output, re.M)
        execution = re.findall(r"^managed execution, client thread: ([\d.]+) allocations/op, ([\d.]+) bytes/op$", output, re.M)
        if len(sizes) != 1 or len(execution) != 1:
            raise ValueError("missing future layout or execution metrics")
        values["future-layout"] = {"bytes": int(sizes[0])}
        values["execution-allocations"] = dict(zip(["allocations_per_op", "bytes_per_op"], map(float, execution[0])))
    elif binary == "head_first_use":
        for match in re.finditer(r"^first-use ([a-z-]+): ns=(\d+) allocations=(\d+) bytes=(\d+) peak_live_bytes=(\d+) net_allocations=(-?\d+)$", output, re.M):
            name = match[1]
            if name in values:
                raise ValueError("duplicate first-use category")
            values[name] = dict(zip(
                ["ns", "allocations", "bytes", "peak_live_bytes", "net_allocations"],
                map(int, match.groups()[1:]),
            ))
        expected = CATEGORIES if category == "sequence" else [category]
        if list(values) != expected:
            raise ValueError("missing or extra first-use category")
    else:
        raise ValueError("unknown probe")
    if any(not math.isfinite(number) for metrics in values.values() for number in metrics.values()):
        raise ValueError("non-finite measurement")
    return {"values": values, "public_plans": heads}


def parse_scale(output, expected):
    rows = []
    for line in output.splitlines():
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if len(cells) == len(SCALE_NAMES) and all(re.fullmatch(r"\d+", c) for c in cells):
            rows.append(dict(zip(SCALE_NAMES, map(int, cells))))
    if [row["endpoints"] for row in rows] != expected:
        raise ValueError("missing, extra, or reordered scale points")
    if "RUSTC_BOOTSTRAP=1" not in output or "fixture CARGO_INCREMENTAL=0" not in output:
        raise ValueError("unexpected scale expansion or incremental method")
    return rows


def require(command, cwd):
    subprocess.run(command, cwd=cwd, check=True)


def archive_inputs(root, out):
    out.mkdir(parents=True, exist_ok=True)
    write_json(out / "source.json", snapshot(root))
    (out / "patch.diff").write_text(git(root, "diff", "--binary", "HEAD"), encoding="utf-8")
    for name in ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "tools/perf/Cargo.toml",
                 "tools/perf/src/head_cost/declaration.rs"] + ["tools/perf/src/bin/" + b + ".rs" for b in BINS + ["expansion_scale"]]:
        path = root / name
        if path.is_file():
            copy = out / name
            copy.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(path, copy)


def prepare(args):
    roots = {}
    for label, revision in [("v3-before", args.before), ("v3-final", args.candidate)] + (
        [("historical", args.historical)] if args.historical else []
    ):
        root = args.work / label
        require(["git", "worktree", "add", "--detach", str(root), revision], args.driver)
        roots[label] = root
        actual = git(root, "rev-parse", "HEAD").strip()
        if actual != revision:
            raise RuntimeError("resolved source differs from the requested full commit")
    final = roots["v3-final"]
    # The same runtime operations run in all inputs. Retain each checkout's own
    # manifest so no current package requirement is transplanted into history.
    for label, root in roots.items():
        if label != "v3-final":
            for binary in BINS:
                destination = root / "tools/perf/src/bin" / (binary + ".rs")
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(final / "tools/perf/src/bin" / (binary + ".rs"), destination)
            declaration = final / "tools/perf/src/head_cost/declaration.rs"
            if label == "historical":
                declaration = roots["v3-before"] / "tools/perf/src/head_cost/declaration_v2_1.rs.txt"
            destination = root / "tools/perf/src/head_cost/declaration.rs"
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(declaration, destination)
        for package, version in [("rustls", "0.23.46"), ("rustls-webpki", "0.103.15")]:
            current = {(p[0], p[1]) for p in locked_packages(root)}
            if (package, version) not in current:
                if not record(root, args.out / "setup", label + "-align-" + package,
                              ["cargo", "update", "-p", package, "--precise", version], True):
                    # A lock update is an intentional input mutation, recorded
                    # before build. Its native exit must succeed.
                    receipt = json.loads((args.out / "setup" / (label + "-align-" + package + ".json")).read_text())
                    if receipt["native_exit_code"] != 0:
                        raise RuntimeError("dependency alignment failed")
        if not record(root, args.out / "setup", label + "-fetch", ["cargo", "fetch", "--locked"], True, 900):
            raise RuntimeError("locked dependency fetching failed")
    external = {label: sorted(locked_packages(root)) for label, root in roots.items()}
    write_json(args.out / "external-dependencies.json", external)
    if any(packages != external["v3-final"] for packages in external.values()):
        raise RuntimeError("external dependency versions or checksums are not identical")
    os.environ["CARGO_NET_OFFLINE"] = "true"
    return roots


def build(args, root, label, binaries):
    if not record(root, args.out / "feature-graphs", label,
                  ["cargo", "tree", "--locked", "-p", "concord-perf", "-e", "features"], True):
        raise RuntimeError("could not capture the actual probe feature graph")
    target = args.work / "targets" / ("release-" + label)
    os.environ["CARGO_TARGET_DIR"] = str(target)
    command = ["cargo", "build", "--release", "--locked", "-p", "concord-perf"]
    for binary in binaries:
        command += ["--bin", binary]
    if not record(root, args.out / "builds", label, command, True, 2400):
        raise RuntimeError("release probe build failed or changed its inputs")
    frozen = args.out / "binaries" / label
    frozen.mkdir(parents=True, exist_ok=True)
    result = {}
    for binary in binaries:
        source = target / "release" / binary
        copy = frozen / binary
        shutil.copy2(source, copy)
        result[binary] = {"path": str(copy), "bytes": copy.stat().st_size, "sha256": digest(copy)}
    return result


def runtime(args, roots):
    binaries = {label: build(args, root, label, BINS) for label, root in roots.items()}
    write_json(args.out / "binaries.json", binaries)
    before = {label: snapshot(root) for label, root in roots.items()}
    probes = [("head_cost", None), ("future_sizes", None)] + [
        ("head_first_use", category) for category in CATEGORIES + ["sequence"]
    ]
    samples = []
    reference_heads = None
    labels = list(roots)
    for repeat in range(5):
        for probe_index, (binary, category) in enumerate(probes):
            offset = probe_index % len(labels)
            base_order = labels[offset:] + labels[:offset]
            if len(labels) == 3:
                positions = [(0, 1, 2), (1, 2, 0), (2, 0, 1), (2, 1, 0), (0, 2, 1)][repeat]
                order = [base_order[index] for index in positions]
            else:
                order = base_order if repeat % 2 == 0 else list(reversed(base_order))
            for label in order:
                entry = binaries[label][binary]
                if digest(entry["path"]) != entry["sha256"]:
                    raise RuntimeError("a measured binary changed")
                name = f"r{repeat + 1:02}-{binary}-{category or 'all'}-{label}"
                command = [entry["path"]] + ([category] if category else [])
                os.environ["CARGO_TARGET_DIR"] = str(args.work / "targets" / ("release-" + label))
                if not record(roots[label], args.out / "runs", name, command, True, 120):
                    raise RuntimeError("native runtime probe failed or changed its inputs")
                parsed = parse_runtime(binary, category, (args.out / "runs" / (name + ".log")).read_text())
                if parsed["public_plans"]:
                    if reference_heads is None:
                        reference_heads = parsed["public_plans"]
                    if parsed["public_plans"] != reference_heads:
                        raise RuntimeError("the comparators do not plan the same public requests")
                samples.append({"repeat": repeat + 1, "candidate": label, "probe": binary, "category": category, "receipt": name, **parsed})
    after = {label: snapshot(root) for label, root in roots.items()}
    if before != after:
        raise RuntimeError("measurement source changed")
    summary = {}
    for label in labels:
        grouped = {}
        for sample in [s for s in samples if s["candidate"] == label]:
            for operation, metrics in sample["values"].items():
                for metric, value in metrics.items():
                    key = "/".join([sample["probe"], sample["category"] or "all", operation, metric])
                    grouped.setdefault(key, []).append(value)
        if any(len(values) != 5 for values in grouped.values()):
            raise RuntimeError("a metric lacks five independent process samples")
        summary[label] = {key: {"samples": values, "median": statistics.median(values), "min": min(values), "max": max(values)} for key, values in grouped.items()}
    result = {"schema": "concord.performance.runtime/1", "native_processes": len(samples),
              "measurement_inputs_stable": True, "public_plans": reference_heads,
              "samples": samples, "summary": summary,
              "scope": "Current-thread operation allocations and native loopback timing. Excludes server-thread allocations, RSS, TLS-network latency and remote-provider performance. No O07 acceptance is implied."}
    write_json(args.out / "runtime-results.json", result)
    print("PERFORMANCE_RUNTIME_SUMMARY " + json.dumps(summary, sort_keys=True), flush=True)


def capture_scale_harness(root):
    path = root / "tools/perf/src/bin/expansion_scale.rs"
    source = path.read_text()
    points = [
        ("let expand_ms = started.elapsed().as_millis();", "base"),
        ("let edit_ms = started.elapsed().as_millis();", "local"),
        ("let parent_edit_ms = started.elapsed().as_millis();", "parent"),
    ]
    for anchor, label in points:
        if source.count(anchor) != 1:
            raise RuntimeError("scale capture anchor is not unique")
        extra = '\n        fs::copy(fixture.join("src/main.rs"), scratch.join(format!("point-{count}-' + label + '.rs"))).map_err(|error| error.to_string())?;'
        if label == "base":
            extra += '\n        fs::write(scratch.join(format!("point-{count}-expanded.rs")), &expansion).map_err(|error| error.to_string())?;'
        source = source.replace(anchor, anchor + extra, 1)
    path.write_text(source)


def scale(args, roots):
    original_harness = (roots["v3-before"] / "tools/perf/src/bin/expansion_scale.rs").read_bytes()
    final_harness = (roots["v3-final"] / "tools/perf/src/bin/expansion_scale.rs").read_bytes()
    (roots["v3-before"] / "tools/perf/src/bin/expansion_scale.rs").write_bytes(final_harness)
    if "historical" in roots:
        (roots["historical"] / "tools/perf/src/bin/expansion_scale.rs").write_bytes(original_harness)
    for root in roots.values():
        capture_scale_harness(root)
    binaries = {label: build(args, root, label, ["expansion_scale"]) for label, root in roots.items()}
    write_json(args.out / "binaries.json", binaries)
    rows = []
    runs = [(label, [1, 10, 100, 500, 1000], 1) for label in roots]
    # Two more alternating pairs at the largest point distinguish a persistent
    # compile-cost difference from a single noisy runner sample.
    runs += [("v3-final", [1000], 2), ("v3-before", [1000], 2),
             ("v3-before", [1000], 3), ("v3-final", [1000], 3)]
    for label, counts, repeat in runs:
        root = roots[label]
        entry = binaries[label]["expansion_scale"]
        if digest(entry["path"]) != entry["sha256"]:
            raise RuntimeError("scale executable changed")
        os.environ["CARGO_TARGET_DIR"] = str(args.work / "targets" / ("scale-" + label))
        command = [entry["path"]]
        if label == "historical":
            command += ["--baseline-v2-1", str(roots[label])]
        command += list(map(str, counts))
        name = f"r{repeat:02}-{label}"
        if not record(root, args.out / "runs", name, command, True, 3600):
            raise RuntimeError("native scale probe failed or changed tracked inputs")
        output = (args.out / "runs" / (name + ".log")).read_text()
        parsed = parse_scale(output, counts)
        scratch = root / "target/perf/expansion-scale"
        saved = args.out / "scale-inputs" / name
        saved.mkdir(parents=True, exist_ok=True)
        fixture = scratch / ("fixture-v2_1" if label == "historical" else "fixture-v3")
        if not locked_packages(fixture).issubset(locked_packages(roots["v3-final"])):
            raise RuntimeError("scale fixture resolved dependencies outside the aligned inputs")
        for item in [fixture / "Cargo.toml", fixture / "Cargo.lock", fixture / "src/main.rs"] + [
            scratch / f"point-{count}-{kind}.rs" for count in counts for kind in ["base", "local", "parent", "expanded"]
        ]:
            copy = saved / (item.name if item.parent != fixture / "src" else "final-main.rs")
            shutil.copy2(item, copy)
        files = [{"path": p.name, "bytes": p.stat().st_size, "sha256": digest(p)} for p in sorted(saved.iterdir())]
        write_json(saved / "files.json", files)
        rows.append({"candidate": label, "repeat": repeat, "metrics": parsed, "receipt": name})
        print("PERFORMANCE_SCALE_ROWS " + json.dumps(rows[-1], sort_keys=True), flush=True)
    write_json(args.out / "scale-results.json", {
        "schema": "concord.performance.scale/1", "rows": rows,
        "scope": "Debug builds, dependencies prebuilt; cold means only the fixture package cleaned. Source/expansion capture runs outside measured intervals in all candidates. Description bytes are serialized metadata, not retained heap. No O07 acceptance is implied.",
    })


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=["runtime", "scale"])
    parser.add_argument("--driver", type=Path, required=True)
    parser.add_argument("--work", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--candidate", required=True)
    parser.add_argument("--before", required=True)
    parser.add_argument("--historical")
    args = parser.parse_args()
    for revision in [args.candidate, args.before] + ([args.historical] if args.historical else []):
        if not re.fullmatch(r"[0-9a-f]{40}", revision):
            parser.error("source inputs must be complete commit hashes")
    args.driver, args.work, args.out = args.driver.resolve(), args.work.resolve(), args.out.resolve()
    args.work.mkdir(parents=True, exist_ok=False)
    args.out.mkdir(parents=True, exist_ok=False)
    os.environ["CARGO_INCREMENTAL"] = "0"
    os.environ["CARGO_BUILD_JOBS"] = "2"
    for name in ["CARGO_PROFILE_DEV_DEBUG", "CARGO_PROFILE_TEST_DEBUG", "RUSTFLAGS", "RUSTDOCFLAGS", "RUSTC_BOOTSTRAP"]:
        os.environ.pop(name, None)
    write_json(args.out / "environment.json", environment(args.driver))
    roots = prepare(args)
    for label, root in roots.items():
        archive_inputs(root, args.out / "inputs-before" / label)
    try:
        (runtime if args.mode == "runtime" else scale)(args, roots)
    finally:
        for label, root in roots.items():
            archive_inputs(root, args.out / "inputs-after" / label)
    # Raw binaries are hashed and their source/build recipes are retained.
    # Excluding the executable bytes keeps the evidence artifact manageable.
    shutil.rmtree(args.out / "binaries")
    write_json(args.out / "completed.json", {"finished_utc": utc(), "mode": args.mode, "driver": snapshot(args.driver), "candidate": args.candidate, "measurement_complete": True})
    return 0


if __name__ == "__main__":
    sys.exit(main())
