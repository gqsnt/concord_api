#!/usr/bin/env python3
"""Measure cold Concord client crates with warm dependencies, entirely offline."""

import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import time
import tomllib


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "target" / "weight"
SIZES = (10, 100, 500, 1000, 3000)
ACTIONS = ("check", "build", "doc", "expand")
SHAPES = ("get", "json", "status", "pager")
JSON_SLOTS = {1, 4, 7, 11, 14, 17, 21, 24, 27, 28}
STATUS_SLOTS = {5, 8, 15, 18, 25, 29}
CONCORD_PATH = re.compile(r'(\bpath\s*=\s*")([^"\n]*[/\\])?(concord-(?:api|core|extras|macros|testkit))"')


def endpoint_shape(index):
    slot = index % 30
    if slot % 10 == 0:
        return "pager"
    if slot in JSON_SLOTS:
        return "json"
    if slot in STATUS_SLOTS:
        return "status"
    return "get"


def endpoint(index, shape):
    method = "POST" if shape == "json" else "GET"
    result = "Option<Payload>" if shape == "status" else "Payload"
    inputs = [
        "/// The item identifier.", "id: u64,",
        "/// A search term.", "q: String,",
        "/// Maximum number of results.", "limit?: u32,",
    ]
    query = "q; limit;"
    extra = []
    if shape == "json":
        inputs += ["/// The item to store.", "payload: Payload,"]
        extra = ["body json(payload);"]
    if shape == "status":
        extra = ["status { 404 => None }"]
    if shape == "pager":
        inputs += ["/// Cursor returned by the previous page.", "after?: String,"]
        query += " after;"
        extra = ["paginate move |call, page| page.value().next.as_ref().map(|next| call.after(next));"]
    return [
        f"/// Run operation {index}.",
        f"{method} endpoint_{index:04}(",
        *("    " + line for line in inputs),
        f") -> {result} {{",
        f'    path "operations/{index}/{{id}}";',
        f"    query {{ {query} }}",
        *("    " + line for line in extra),
        "}",
    ]


def description(count, shapes=None):
    """Make one public client with three nested scopes and a stable endpoint mix."""
    lines = [
        "use concord::{json, Json};",
        "#[derive(Clone, serde::Deserialize, serde::Serialize)]",
        "pub struct Payload { pub value: String, pub next: Option<String> }",
        "#[derive(serde::Deserialize)]",
        "pub struct Problem { pub message: String }",
        "concord::api! {",
        "pub client Catalog {",
        '    base "https://catalog.example.test";',
        "    read Json;",
        "    problem Problem;",
    ]
    # Keep the topology comparable as the client grows: ten endpoints per leaf,
    # five leaves per middle scope and three middle scopes per outer scope.
    for outer in range(0, count, 150):
        lines += [f"    scope region_{outer // 150:03} {{", '        path "regions";']
        for middle in range(outer, min(outer + 150, count), 50):
            lines += [f"        scope group_{middle // 50:03} {{", '            path "groups";']
            for leaf in range(middle, min(middle + 50, count), 10):
                lines += [f"            scope section_{leaf // 10:03} {{", '                path "sections";']
                for index in range(leaf, min(leaf + 10, count)):
                    shape = endpoint_shape(index) if shapes is None else shapes[index]
                    lines += ["                " + line for line in endpoint(index, shape)]
                lines += ["            }"]
            lines += ["        }"]
        lines += ["    }"]
    return "\n".join([*lines, "}", "}", ""])


def make_crate(path, checkout, source):
    name = path.name.replace("-", "_")
    path.mkdir(parents=True)
    (path / "src").mkdir()
    (path / "Cargo.toml").write_text(
        f'[package]\nname = "{name}"\nversion = "0.0.0"\nedition = "2024"\npublish = false\n'
        f'\n[workspace]\n\n[dependencies]\nconcord = {{ package = "concord-api", '
        f'path = {json.dumps(str(checkout / "concord-api"), ensure_ascii=False)} }}\n'
        'serde = { version = "1", features = ["derive"] }\n'
    )
    (path / "src/lib.rs").write_text(source)
    shutil.copyfile(ROOT / "Cargo.lock", path / "Cargo.lock")
    return path


def point_at_checkout(manifest, checkout):
    return CONCORD_PATH.sub(
        lambda match: match[1] + json.dumps(str(checkout / match[3]), ensure_ascii=False)[1:],
        manifest,
    )


def copy_client(source, destination, checkout):
    if not (source / "Cargo.toml").is_file():
        raise ValueError(f"client has no Cargo.toml: {source}")
    shutil.copytree(source, destination, ignore=shutil.ignore_patterns(".git", ".cargo", "target"))
    for manifest in destination.rglob("Cargo.toml"):
        original = manifest.read_text()
        changed = point_at_checkout(original, checkout)
        if changed != original:
            manifest.write_text(changed)
    return destination


def make_warm_client(client, destination):
    """Keep the resolved graph and features but leave the client libraries empty."""
    shutil.copytree(client, destination)
    packages = []
    for manifest in destination.rglob("Cargo.toml"):
        metadata = tomllib.loads(manifest.read_text())
        lib = metadata.get("lib", {})
        source = (manifest.parent / lib.get("path", "src/lib.rs")).resolve()
        if not source.is_relative_to(destination):
            raise ValueError(f"client library source is outside the copied workspace: {source}")
        if source.is_file():
            source.write_text("")
            packages.append(metadata["package"]["name"])
    return destination, sorted(set(packages))


def parse_time(report):
    wall = re.search(r"wall clock\) time \(h:mm:ss or m:ss\):\s*([\d:.]+)", report)
    rss = re.search(r"Maximum resident set size \(kbytes\):\s*(\d+)", report)
    seconds = None
    if wall:
        seconds = 0.0
        for part in wall[1].split(":"):
            seconds = 60 * seconds + float(part)
    return {"seconds": seconds, "rss_kib": int(rss[1]) if rss else None}


def memory_scope():
    """Read the effective cgroup v2 ceiling when the current scope exposes it."""
    cgroup = Path("/proc/self/cgroup")
    if not cgroup.is_file():
        return None, None
    for line in cgroup.read_text().splitlines():
        if line.startswith("0::"):
            group = Path("/sys/fs/cgroup") / line[3:].lstrip("/")
            limits = []
            for directory in (group, *group.parents):
                limit = directory / "memory.max"
                if limit.is_file():
                    value = limit.read_text().strip()
                    if value.isdigit():
                        limits.append(int(value))
            return group / "memory.events", min(limits, default=None)
    return None, None


def oom_kills(events):
    if events is not None and events.is_file():
        counts = dict(line.split() for line in events.read_text().splitlines())
        return int(counts.get("oom_kill", 0))
    return 0


class Run:
    def __init__(self, path, limiter, stable, nightly):
        self.path = path
        self.limiter = limiter
        self.stable = stable
        self.nightly = nightly
        self.events, ceiling = memory_scope()
        self.summary = shutil.which("summarize")
        self.failed = False
        versions = {}
        for toolchain in dict.fromkeys((stable, nightly)):
            result = subprocess.run(["rustup", "run", toolchain, "rustc", "-Vv"],
                                    capture_output=True, text=True)
            versions[toolchain] = result.stdout.strip() if not result.returncode else result.stderr.strip()
        path.mkdir(parents=True)
        (path / "environment.json").write_text(json.dumps({
            "started_utc": datetime.now(timezone.utc).isoformat(),
            "versions": versions, "jobs": 4, "limit_mem": "14G",
            "inherited_memory_max_bytes": ceiling, "measureme_summary": self.summary,
        }, indent=2) + "\n")

    def cargo(self, crate, target, label, args, *, nightly=False, expanded=False, warm=None):
        logs = self.path / "logs" / label
        logs.mkdir(parents=True)
        toolchain = self.nightly if nightly else self.stable
        jobs = [] if args[0] == "clean" else ["-j", "4"]
        command = ["rustup", "run", toolchain, "cargo", args[0], "--offline", *jobs, *args[1:]]
        env = dict(os.environ, CARGO_TARGET_DIR=str(target), CARGO_BUILD_JOBS="4",
                   CARGO_NET_OFFLINE="true", LIMIT_MEM="14G", LC_ALL="C")
        for attempt in (1, 2):
            print(f"{label} (attempt {attempt})", flush=True)
            timing = logs / f"{attempt}.time"
            outside = [str(self.limiter), "/usr/bin/time", "-v", "-o", str(timing), *command]
            before = oom_kills(self.events)
            with (logs / f"{attempt}.log").open("w") as log:
                log.write(f"$ {shlex.join(outside)}\n")
                log.flush()
                if expanded:
                    expansion = logs / f"{attempt}.rs"
                    with expansion.open("w") as source:
                        result = subprocess.run(outside, cwd=crate, env=env, stdout=source, stderr=log)
                else:
                    result = subprocess.run(outside, cwd=crate, env=env, stdout=log, stderr=log)
            killed = (result.returncode in (137, 143, -9, -15) or
                      (result.returncode != 0 and oom_kills(self.events) > before))
            row = {"case": label, "attempt": attempt, "toolchain": toolchain,
                   "exit": result.returncode, "memory_stop": killed,
                   **parse_time(timing.read_text() if timing.exists() else "")}
            if warm is not None:
                row["dependency_seed_succeeded"] = warm
            if expanded and result.returncode == 0:
                with expansion.open("rb") as source:
                    row["lines"] = sum(1 for _ in source)
                row["bytes"] = expansion.stat().st_size
            if args[0] == "doc" and (target / "doc").is_dir():
                # Keep previous rustdoc search indexes out of later measurements.
                (target / "doc").rename(logs / f"{attempt}-html")
            with (self.path / "results.jsonl").open("a") as output:
                output.write(json.dumps(row) + "\n")
            print(json.dumps(row), flush=True)
            if not killed or attempt == 2:
                if result.returncode and not killed:
                    self.failed = True
                return result.returncode == 0
            time.sleep(60)
        return False

    def synthetic(self, checkout, sizes, actions, profile, shapes):
        revision = subprocess.check_output(["git", "-C", str(checkout), "rev-parse", "--short=8", "HEAD"],
                                           text=True).strip()
        base = self.path / revision
        seed = make_crate(base / "seed", checkout, "")
        target = base / "cargo"
        options = {
            "check": ["check", "--lib"],
            "build": ["build", "--lib"],
            "doc": ["doc", "--lib", "--no-deps"],
            "expand": ["rustc", "--lib", "--", "-Zunpretty=expanded"],
        }
        warm_cache = {}
        warm_actions = list(actions)
        if (shapes or profile) and "expand" not in warm_actions:
            warm_actions.append("expand")
        for action in warm_actions:
            nightly = action == "expand"
            warm = ["build", "--lib"] if nightly else options[action]
            warm_cache[action] = self.cargo(seed, target, f"{revision}/warm/{action}", warm, nightly=nightly)
        cases = [(f"n{size:04}", description(size), actions) for size in sizes]
        if shapes:
            cases.append(("shapes", description(len(SHAPES), SHAPES), ("expand",)))
        for name, source, case_actions in cases:
            crate = make_crate(base / name, checkout, source)
            for action in case_actions:
                args = options[action]
                if name == "n1000" and action == "build":
                    args = [*args, "--timings"]
                self.cargo(crate, target, f"{revision}/{name}/{action}", args,
                           nightly=action == "expand", expanded=action == "expand",
                           warm=warm_cache[action])
        if profile:
            crate = make_crate(base / "passes", checkout, description(1000))
            label = f"{revision}/n1000/passes"
            flags = ["-Ztime-passes"]
            if self.summary:
                profile_dir = base / "self-profile"
                profile_dir.mkdir()
                flags += [f"-Zself-profile={profile_dir}"]
            if self.cargo(crate, target, label, ["rustc", "--lib", "--", *flags],
                          nightly=True, warm=warm_cache["expand"]):
                if self.summary:
                    profiles = list(profile_dir.glob("*.mm_profdata"))
                    with (base / "self-profile-summary.txt").open("w") as output:
                        for data in profiles:
                            subprocess.run([self.summary, "summarize", str(data)],
                                           stdout=output, stderr=subprocess.STDOUT, check=True)

    def real(self, clients, checkout):
        for source in clients:
            base = self.path / "clients" / source.name
            client = copy_client(source, base / "client", checkout)
            seed, packages = make_warm_client(client, base / "seed")
            if not packages:
                raise ValueError(f"no libraries in client workspace: {source}")
            target = base / "cargo"
            label = f"clients/{source.name}"
            warm = {}
            for action in ("check", "build"):
                warm[action] = self.cargo(seed, target, f"{label}/warm/{action}", [action, "--lib"])
            # Cargo can reuse package fingerprints across relocated workspaces.
            # Drop the empty libraries' artifacts and keep the dependencies.
            clean = ["clean"]
            for package in packages:
                clean += ["--package", package]
            if not self.cargo(client, target, f"{label}/reset", clean):
                continue
            for action in ("check", "build"):
                self.cargo(client, target, f"{label}/{action}", [action, "--lib"], warm=warm[action])


def main():
    parser = argparse.ArgumentParser(
        description=__doc__,
        epilog="Each run keeps its generated crates, Cargo cache, raw logs and results.jsonl. "
        "Expansion uses nightly rustc -Zunpretty=expanded. Measured commands use the build "
        "limiter with LIMIT_MEM=14G and four jobs. A memory stop is recorded and retried once. "
        "Other command failures produce a nonzero exit status.",
    )
    parser.add_argument("--limiter", type=Path, default=os.environ.get("CONCORD_WEIGHT_LIMITER"),
                        help="path to the build limiter (or set CONCORD_WEIGHT_LIMITER)")
    parser.add_argument("--run", default="run", help="new directory name under target/weight")
    parser.add_argument("--checkout", action="append", type=Path, help="Concord checkout; can repeat")
    parser.add_argument("--sizes", nargs="+", type=int, default=SIZES)
    parser.add_argument("--actions", nargs="+", choices=ACTIONS, default=ACTIONS)
    parser.add_argument("--profile", action="store_true", help="profile a debug build with 1,000 endpoints")
    parser.add_argument("--shapes", action="store_true", help="expand one endpoint of each shape")
    parser.add_argument("--client", action="append", type=Path, default=[], help="copy and measure a client")
    parser.add_argument("--only-clients", action="store_true", help="skip the synthetic crates")
    parser.add_argument("--stable", default="stable", help="installed baseline Rust toolchain")
    parser.add_argument("--nightly", default="nightly", help="installed nightly Rust toolchain")
    args = parser.parse_args()
    if args.limiter is None or not args.limiter.is_file() or not os.access(args.limiter, os.X_OK):
        parser.error("provide an executable --limiter or CONCORD_WEIGHT_LIMITER")
    if not re.fullmatch(r"[\w-]+", args.run) or (OUTPUT / args.run).exists():
        parser.error("--run must name a new directory using letters, digits, _ or -")
    if any(size <= 0 for size in args.sizes) or len(set(args.sizes)) != len(args.sizes):
        parser.error("--sizes must be distinct positive endpoint counts")
    if len(set(args.actions)) != len(args.actions):
        parser.error("--actions must be distinct")
    if args.only_clients and not args.client:
        parser.error("--only-clients needs at least one --client")
    checkouts = [path.resolve() for path in (args.checkout or [ROOT])]
    for checkout in checkouts:
        if not (checkout / "concord-api" / "Cargo.toml").is_file():
            parser.error(f"not a Concord checkout: {checkout}")
    run = Run(OUTPUT / args.run, args.limiter.resolve(), args.stable, args.nightly)
    if not args.only_clients:
        for checkout in checkouts:
            run.synthetic(checkout, args.sizes, args.actions, args.profile, args.shapes)
    run.real([source.resolve() for source in args.client], checkouts[0])
    return int(run.failed)


if __name__ == "__main__":
    raise SystemExit(main())
