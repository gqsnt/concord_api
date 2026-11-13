#!/usr/bin/env python3
"""Offline engine mutation gate; sockets are opt-in. See mutations/README.md."""

import argparse
import concurrent.futures
import fcntl
import json
import os
from pathlib import Path
import queue
import re
import shlex
import shutil
import signal
import subprocess
import sys
import threading
import time


API = Path(__file__).resolve().parents[1]
DATA = API / "tools" / "mutations"
ROOT = API / "target" / "mutants"
DEFAULT_PACKAGES = ("concord-core", "concord-extras", "concord", "tour", "conformance")
RUN_LOCK_FD = None
# These tests open local listeners (including through Server/local_server).
# Keep the list explicit: a failing test must never be silently auto-excluded.
SOCKET_TESTS = (
    "c6_2_a75_real_reqwest_malformed_heads_and_request_body_failures",
    "c7_1_real_stalled_tls_handshake_reports_the_30_s_http_connect_timeout",
    "c4_2_d272_real_get_retries_close_before_head_then_200",
    "c6_1_a12_real_status_reason_phrase_never_enters_a_source",
    "c6_2_a13_real_refused_port_is_connect_with_closed_vocabulary",
    "c6_2_a13_real_reset_after_connecting_is_connect",
    "c5_4_a_body_that_ends_early_on_the_wire",
    "c5_7_a_refusal_whose_body_ends_early_on_the_wire",
    "c6_1_the_source_of_a_failure_of_the_http_library_holds_no_url",
    "c6_2_a_connection_that_was_refused",
    "c7_1_http_follows_no_redirect",
    "c7_1_http_sets_30_seconds_to_connect",
    "c7_1_http_keeps_no_cookies",
    "c7_1_http_does_not_decompress",
    "c7_3_a_client_given_no_transport_sends_with_concords_own",
    "c7_3_the_default_transport_is_made_once_and_shared_by_every_clone",
    "c7_5_a_client_of_the_user_that_follows_redirects_does_it_below_the_middlewares",
    "c7_6_a_head_and_then_silence_on_the_wire",
    "c9_1_a_reqwest_client_is_a_transport",
    "c8_4_a_connection_that_was_refused_ends_the_pages",
)


class RunnerError(Exception):
    """An invalid catalog or baseline/infrastructure failure, not a caught mutant."""


def positive(text):
    value = int(text)
    if value < 1:
        raise argparse.ArgumentTypeError("must be positive")
    return value


def catalog():
    mutations = []
    seen = set()
    fields = {"id", "area", "contract", "description", "file", "find", "replace"}
    for path in sorted(DATA.glob("[MR]*.json")):
        try:
            item = json.loads(path.read_text(encoding="utf-8"))
        except (ValueError, OSError) as error:
            raise RunnerError(f"{path}: {error}") from error
        if not isinstance(item, dict) or not fields <= set(item) or set(item) - fields - {"requires_sockets", "checks"}:
            raise RunnerError(f"{path}: expected fields {', '.join(sorted(fields))}")
        if any(not isinstance(item[key], str) for key in fields):
            raise RunnerError(f"{path}: every field must be a string")
        if "requires_sockets" in item and not isinstance(item["requires_sockets"], bool):
            raise RunnerError(f"{path}: requires_sockets must be a boolean")
        if "checks" in item and (not isinstance(item["checks"], list) or
                any(check not in ("surface", "docs-core", "docs-extras", "docs-pages", "allocations") for check in item["checks"])):
            raise RunnerError(f"{path}: unknown extra check")
        ident = item["id"]
        if not re.fullmatch(r"[MR]\d{2,}", ident) or path.stem != ident or ident in seen:
            raise RunnerError(f"{path}: invalid or duplicate id")
        relative = Path(item["file"])
        if relative.is_absolute() or ".." in relative.parts or "target" in relative.parts:
            raise RunnerError(f"{path}: file must be a source path relative to api/")
        if not item["find"] or item["find"] == item["replace"]:
            raise RunnerError(f"{path}: find must be nonempty and change the source")
        if any(not item[key].strip() for key in fields - {"replace"}):
            raise RunnerError(f"{path}: empty metadata")
        seen.add(ident)
        mutations.append(item)
    if not mutations:
        raise RunnerError("empty mutation catalog")
    return sorted(mutations, key=lambda item: (item["id"][0], int(item["id"][1:])))


def copy_sources(source, destination):
    if destination.exists():
        shutil.rmtree(destination)
    shutil.copytree(source, destination, ignore=shutil.ignore_patterns(
        "target", ".git", "__pycache__", ".pytest_cache"
    ))


def invoke(command, cwd, env, log, timeout):
    """Bound the entire subprocess group, including cargo's compiler children."""
    with log.open("a", encoding="utf-8") as output:
        output.write("+ " + shlex.join(map(str, command)) + "\n")
        output.flush()
        with subprocess.Popen(command, cwd=cwd, env=env, stdout=output,
                              stderr=subprocess.STDOUT, start_new_session=True,
                              pass_fds=(() if RUN_LOCK_FD is None else (RUN_LOCK_FD,))) as process:
            try:
                return process.wait(timeout=timeout), False
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
                output.write(f"\nMUTATION WATCHDOG: exceeded {timeout} seconds\n")
                return process.returncode, True
            except BaseException:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
                raise


class Worker:
    def __init__(self, number, args):
        self.args = args
        self.home = ROOT / f"worker-{number}"
        self.source = self.home / "api"
        # Copying restored files preserves their old mtimes. Cargo may then
        # consider a killed run's mutated artifacts fresh. Never trust that cache.
        if self.home.exists():
            shutil.rmtree(self.home)
        copy_sources(ROOT / "source", self.source)
        copy_sources(ROOT / "docs", self.home / "docs")
        temporary = self.home / "tmp"
        temporary.mkdir(parents=True, exist_ok=True)
        self.env = dict(os.environ, CARGO_NET_OFFLINE="true", CARGO_TERM_COLOR="never",
                        CARGO_TARGET_DIR=str(self.home / "target"), TMPDIR=str(temporary),
                        CARGO_BUILD_JOBS="4")
        # Do not inherit a parent libtest filter or wrapper which could skip tests.
        self.env.pop("RUST_TEST_THREADS", None)
        # Cargo normally supplies these paths when launching tests. We launch
        # the artifact executables ourselves, including proc-macro test binaries.
        rustlib_log = self.home / "rustlib.log"
        code, timeout = invoke([self.env.get("RUSTC", "rustc"), "--print", "target-libdir"],
                               self.source, self.env, rustlib_log, args.build_timeout)
        if code or timeout:
            raise RunnerError(f"cannot locate Rust shared libraries; see {rustlib_log}")
        rustlib = rustlib_log.read_text().splitlines()[-1]
        if not Path(rustlib).is_dir():
            raise RunnerError(f"invalid Rust shared library directory: {rustlib}")
        for variable in ("LD_LIBRARY_PATH", "DYLD_FALLBACK_LIBRARY_PATH"):
            paths = [str(self.home / "target/debug/deps"), str(self.home / "target/debug"), rustlib]
            if self.env.get(variable):
                paths.append(self.env[variable])
            self.env[variable] = os.pathsep.join(paths)
        metadata_log = self.home / "metadata.log"
        metadata_log.write_text("", encoding="utf-8")
        code, timeout = invoke(["cargo", "metadata", "--offline", "--locked", "--no-deps",
                                "--format-version=1"], self.source, self.env, metadata_log,
                               args.build_timeout)
        if code or timeout:
            raise RunnerError(f"cannot inspect Cargo targets; see {metadata_log}")
        metadata = None
        for line in metadata_log.read_text(encoding="utf-8", errors="replace").splitlines():
            try:
                event = json.loads(line)
            except ValueError:
                continue
            if isinstance(event, dict) and "workspace_members" in event and "packages" in event:
                metadata = event
                break
        if metadata is None:
            raise RunnerError(f"no Cargo metadata; see {metadata_log}")
        self.packages = [package for package in metadata["packages"]
                         if package["id"] in metadata["workspace_members"]
                         and (args.packages == ["all"] or package["name"] in args.packages)]
        if not args.with_sockets:
            # Every test in this target creates a Server, including its
            # should_panic fixture validation test (which binds before panicking).
            for package in self.packages:
                if package["name"] == "concord-testkit":
                    args.exclude_test_target = list(dict.fromkeys(
                        [*args.exclude_test_target, "concord-testkit::server"]))
        missing = set(args.packages) - {package["name"] for package in self.packages} - {"all"}
        if missing:
            raise RunnerError("unknown packages: " + ", ".join(sorted(missing)))
        self.package_names = {package["id"]: package["name"] for package in self.packages}
        targets = {f"{package['name']}::{target['name']}" for package in self.packages
                   for target in package["targets"] if "test" in target["kind"]}
        missing = set(args.exclude_test_target) - targets
        if missing:
            raise RunnerError("unknown integration test targets: " + ", ".join(sorted(missing)))

    def build_commands(self):
        base = ["cargo", "test", "--offline", "--locked", "--all-features",
                "--no-run", "--message-format=json"]
        if not self.args.exclude_test_target:
            command = base + ["--lib", "--tests"]
            if self.args.packages == ["all"]:
                command.append("--workspace")
            else:
                for package in self.args.packages:
                    command.extend(["-p", package])
            return [command]
        # Cargo has no negative target selector. Select each package's other
        # targets explicitly, preserving library and integration tests.
        commands = []
        for package in self.packages:
            command = base + ["-p", package["name"]]
            selectors = []
            for target in package["targets"]:
                if set(target["kind"]) & {"lib", "proc-macro"}:
                    selectors.append("--lib")
                elif "bin" in target["kind"]:
                    selectors.extend(["--bin", target["name"]])
                elif "test" in target["kind"]:
                    name = f"{package['name']}::{target['name']}"
                    if name not in self.args.exclude_test_target:
                        selectors.extend(["--test", target["name"]])
            if selectors:
                commands.append(command + selectors)
        return commands

    def suite(self, label, checks=()):
        logs = ROOT / "logs" / label
        logs.mkdir(parents=True, exist_ok=True)
        build_log = logs / "build.log"
        build_log.write_text("", encoding="utf-8")
        for command in self.build_commands():
            code, timeout = invoke(command, self.source, self.env, build_log,
                                   self.args.build_timeout)
            if timeout:
                return "build timeout", 0, 0
            if code:
                return "compile error", 0, 0
        binaries = {}
        for line in build_log.read_text(encoding="utf-8", errors="replace").splitlines():
            try:
                event = json.loads(line)
            except ValueError:
                continue
            if event.get("reason") == "compiler-artifact" and event.get("executable"):
                if event.get("profile", {}).get("test"):
                    package = self.package_names[event["package_id"]]
                    target = event["target"]["name"]
                    executable = event["executable"]
                    binaries[executable] = (package, target, event)
        if not binaries:
            raise RunnerError(f"{label}: cargo produced no test executables; see {build_log}")
        passed = skipped = 0
        for index, (executable, (package, target, event)) in enumerate(sorted(
                binaries.items(), key=lambda entry: entry[1][:2])):
            log = logs / f"{index:02}-{target}.log"
            log.write_text("", encoding="utf-8")
            command = [executable, "--test-threads=1"]
            for name in (() if self.args.with_sockets else SOCKET_TESTS) + tuple(self.args.skip_test):
                command.extend(["--skip", name])
            # Match cargo's per-package working directory (include_str! fixtures,
            # subprocesses and relative test resources depend on it).
            cwd = Path(event["manifest_path"]).parent
            # Macro integration binaries run nested offline cargo/trybuild jobs.
            timeout_budget = max(self.args.timeout, self.args.build_timeout) if package == "concord-macros" else self.args.timeout
            code, timeout = invoke(command, cwd, self.env, log, timeout_budget)
            output = log.read_text(encoding="utf-8", errors="replace")
            prefix = f"{package}::{target}"
            if timeout:
                return f"timeout in {prefix}", passed, skipped
            if code:
                failed = re.search(r"^test (\S+) \.\.\. FAILED", output, re.MULTILINE)
                if failed:
                    return f"{prefix}::{failed[1]}", passed, skipped
                return f"test process exit {code} in {prefix}", passed, skipped
            summary = re.search(r"test result: ok\. (\d+) passed; .*? (\d+) filtered out", output)
            if not summary:
                raise RunnerError(f"{label}: no libtest summary; see {log}")
            passed += int(summary[1])
            skipped += int(summary[2])
        if not passed:
            raise RunnerError(f"{label}: no tests ran")
        for check in checks:
            env = self.env.copy()
            if check == "surface":
                command = [sys.executable, "-B", "-m", "unittest", "discover", "-s", "tools", "-p", "test_e*_contracts.py"]
            elif check == "docs-pages":
                command = [sys.executable, "-B", "tools/docs.py"]
            elif check == "allocations":
                env["M5_GET_MAX_ALLOCATIONS"] = "23"
                command = ["cargo", "bench", "--offline", "--locked", "--manifest-path", "tools/measure/Cargo.toml", "--bench", "macro_get", "--", "--test"]
            else:
                package = {"docs-core": "concord-core", "docs-extras": "concord-extras"}[check]
                command = ["cargo", "test", "--offline", "--locked", "-p", package, "--all-features", "--doc"]
            log = logs / f"check-{check}.log"
            log.write_text("", encoding="utf-8")
            code, timeout = invoke(command, self.source, env, log, self.args.build_timeout)
            if code or timeout:
                return f"{check} regression check" + (" (timeout)" if timeout else ""), passed, skipped
        return None, passed, skipped

    def mutate(self, mutation):
        started = time.monotonic()
        path = self.source / mutation["file"]
        result = dict(id=mutation["id"], area=mutation["area"])
        try:
            original = path.read_bytes()
        except FileNotFoundError:
            result.update(status="stale", reason="file is missing")
        else:
            find = mutation["find"].encode("utf-8")
            count = original.count(find)
            if count != 1:
                result.update(status="stale", reason=f"find matched {count} times")
            else:
                try:
                    path.write_bytes(original.replace(find, mutation["replace"].encode("utf-8"), 1))
                    reason, passed, skipped = self.suite(mutation["id"], mutation.get("checks", ()))
                    result.update(status="caught" if reason else "survived", reason=reason,
                                  passed=passed, skipped=skipped)
                finally:
                    path.write_bytes(original)
        result["seconds"] = round(time.monotonic() - started, 3)
        return result


def line(result):
    if result["status"] == "caught":
        return f"{result['id']} caught by {result['reason']}"
    return f"{result['id']} {result['status'].upper()}"


def run(args, mutations):
    global RUN_LOCK_FD
    ROOT.mkdir(parents=True, exist_ok=True)
    # A second invocation must not overwrite a running worker or its build cache.
    with (ROOT / ".lock").open("w") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as error:
            raise RunnerError("another mutation run holds target/mutants/.lock") from error
        # The child inherits this lock. SIGKILL of the parent cannot allow a new
        # run to delete artifacts while a surviving cargo/test child is using them.
        RUN_LOCK_FD = lock.fileno()
        if (ROOT / "logs").exists():
            shutil.rmtree(ROOT / "logs")
        copy_sources(API, ROOT / "source")
        copy_sources(API.parent / "docs", ROOT / "docs")
        socket_only = [item for item in mutations if item.get("requires_sockets") and not args.with_sockets]
        mutations = [item for item in mutations if item not in socket_only]
        if not mutations:
            raise RunnerError("selected mutations require --with-sockets")
        workers = [Worker(number + 1, args) for number in range(min(args.jobs, len(mutations)))]
        metadata = dict(packages=args.packages, exclude_test_target=args.exclude_test_target,
                        skip_test=args.skip_test,
                        jobs=len(workers), timeout=args.timeout,
                        build_timeout=args.build_timeout, socket_tests=() if args.with_sockets else SOCKET_TESTS,
                        with_sockets=args.with_sockets, socket_only=[item["id"] for item in socket_only],
                        cache_policy="cold worker build per invocation; artifacts reused only within this run",
                        ids=[item["id"] for item in mutations], baselines=[])
        (ROOT / "last-run.json").write_text(json.dumps(metadata, indent=2) + "\n")
        with (ROOT / "last-run.txt").open("w", encoding="utf-8") as transcript:
            output_lock = threading.Lock()

            def emit(message):
                with output_lock:
                    print(message, flush=True)
                    transcript.write(message + "\n")
                    transcript.flush()

            with concurrent.futures.ThreadPoolExecutor(max_workers=len(workers)) as pool:
                baseline_started = time.monotonic()
                checks = sorted({check for mutation in mutations for check in mutation.get("checks", ())})
                baselines = [pool.submit(worker.suite, f"baseline-{i + 1}", checks)
                             for i, worker in enumerate(workers)]
                for i, future in enumerate(baselines):
                    reason, passed, skipped = future.result()
                    metadata["baselines"].append(dict(worker=i + 1, passed=passed, skipped=skipped,
                                                       failure=reason, seconds=round(time.monotonic() - baseline_started, 3)))
                    if reason:
                        raise RunnerError(f"baseline-{i + 1} failed: {reason}; see target/mutants/logs/")
                pending = queue.Queue()
                for mutation in mutations:
                    pending.put(mutation)

                def consume(worker):
                    results = []
                    while True:
                        try:
                            mutation = pending.get_nowait()
                        except queue.Empty:
                            return results
                        result = worker.mutate(mutation)
                        emit(line(result))
                        results.append(result)

                futures = [pool.submit(consume, worker) for worker in workers]
                results = []
                for future in futures:
                    results.extend(future.result())
            results.sort(key=lambda result: (result["id"][0], int(result["id"][1:])))
            counts = {status: sum(item["status"] == status for item in results)
                      for status in ("caught", "survived", "stale")}
            emit(f"mutants: {len(results)}, caught: {counts['caught']}, "
                 f"survived: {counts['survived']}, stale: {counts['stale']}, socket-only: {len(socket_only)}")
            metadata["results"] = results
            (ROOT / "last-run.json").write_text(json.dumps(metadata, indent=2) + "\n")
            return int(bool(counts["survived"] or counts["stale"]))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("ids", nargs="*", help="mutation ids; omit for the entire catalog")
    parser.add_argument("--list", action="store_true", help="list ids, areas and contracts without building")
    parser.add_argument("--jobs", type=positive, default=1, help="scratch workers (must be 1 under the build memory policy)")
    parser.add_argument("--packages", nargs="+", default=list(DEFAULT_PACKAGES),
                        help="Cargo packages or all (default: concord-core concord-extras concord tour conformance)")
    parser.add_argument("--with-sockets", action="store_true", help="include local listener tests and socket-only mutations such as M46")
    parser.add_argument("--exclude-test-target", action="append", default=[], metavar="PACKAGE::TARGET",
                        help="explicitly omit a broken integration target (repeatable; default: none)")
    parser.add_argument("--skip-test", action="append", default=[], metavar="NAME",
                        help="explicit libtest skip filter (repeatable; default: none beyond socket tests)")
    parser.add_argument("--timeout", type=positive, default=10, help="seconds per test binary (default: 10)")
    parser.add_argument("--build-timeout", type=positive, default=180,
                        help="seconds per compilation (default: 180)")
    args = parser.parse_args()
    if args.jobs != 1:
        parser.error("the build memory policy permits one cargo command at a time; use --jobs 1")
    args.packages = [name for part in args.packages for name in part.split(",") if name]
    if not args.packages or ("all" in args.packages and args.packages != ["all"]):
        parser.error("--packages takes 'all' alone, or package names")
    try:
        mutations = catalog()
        unknown = set(args.ids) - {item["id"] for item in mutations}
        if unknown:
            parser.error("unknown mutation ids: " + ", ".join(sorted(unknown)))
        if args.ids:
            mutations = [item for item in mutations if item["id"] in args.ids]
        if args.list:
            for item in mutations:
                suffix = " [requires --with-sockets]" if item.get("requires_sockets") else ""
                print(f"{item['id']} | {item['area']} | {item['contract']} | {item['description']}{suffix}")
            return 0
        return run(args, mutations)
    except (RunnerError, OSError) as error:
        print(f"mutate: {error}", file=sys.stderr)
        return 2
    except KeyboardInterrupt:
        print("mutate: interrupted; working sources were never modified", file=sys.stderr)
        return 130


if __name__ == "__main__":
    sys.exit(main())
