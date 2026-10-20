#!/usr/bin/env python3
"""Record native qualification commands and collect their source-bound outputs."""

import argparse
import datetime as dt
import hashlib
from html.parser import HTMLParser
import json
import os
from pathlib import Path
import platform
import shutil
import signal
import subprocess
import sys
import time
import threading


def utc():
    return dt.datetime.now(dt.timezone.utc).isoformat()


def digest(path):
    h = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def write_json(path, value):
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    Path(path).write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def git(source, *arguments):
    return subprocess.check_output(["git", "-C", str(source), *arguments]).decode("utf-8")


def snapshot(source):
    source = Path(source)
    names = git(source, "ls-files", "--cached", "--others", "--exclude-standard", "-z").split("\0")
    files = {}
    for name in names:
        if name:
            path = source / name
            files[name] = digest(path) if path.is_file() else None
    return {
        "head": git(source, "rev-parse", "HEAD").strip(),
        "tree": git(source, "rev-parse", "HEAD^{tree}").strip(),
        "status": git(source, "status", "--porcelain=v1", "--untracked-files=all"),
        "input_files_sha256": files,
        "file_scope": "tracked and nonignored untracked source files",
    }


def environment(source):
    names = [
        "CARGO_TARGET_DIR", "CARGO_BUILD_JOBS", "CARGO_INCREMENTAL", "CARGO_NET_OFFLINE",
        "CARGO_PROFILE_DEV_DEBUG", "CARGO_PROFILE_TEST_DEBUG",
        "RUSTFLAGS", "RUSTDOCFLAGS", "RUSTC_BOOTSTRAP", "RUSTUP_TOOLCHAIN",
        "GITHUB_SHA", "GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT", "RUNNER_OS",
        "RUNNER_ARCH", "ImageOS", "ImageVersion",
    ]
    result = {
        "platform": platform.platform(),
        "machine": platform.machine(),
        "python": sys.version,
        "cpu_count": os.cpu_count(),
        "variables": {name: os.environ.get(name) for name in names},
    }
    result["disk_usage"] = dict(zip(["total", "used", "free"], shutil.disk_usage(source)))
    for filename in ["/proc/meminfo", "/sys/fs/cgroup/cpu.max", "/sys/fs/cgroup/memory.max"]:
        path = Path(filename)
        if path.is_file():
            result[filename] = path.read_text()
    cpuinfo = Path("/proc/cpuinfo")
    if cpuinfo.is_file():
        result["cpu_model"] = sorted(set(line.split(":", 1)[1].strip() for line in cpuinfo.read_text().splitlines() if line.startswith("model name")))
    if hasattr(os, "sched_getaffinity"):
        result["cpu_affinity"] = sorted(os.sched_getaffinity(0))
    for name, command in [
        ("rustc", ["rustc", "--version", "--verbose"]),
        ("cargo", ["cargo", "--version", "--verbose"]),
        ("msrv", ["rustup", "run", "1.86.0", "rustc", "--version", "--verbose"]),
        ("just", ["just", "--version"]),
        ("pwsh", ["pwsh", "-NoLogo", "-NoProfile", "-Command", "$PSVersionTable.PSVersion.ToString()"]),
    ]:
        try:
            run = subprocess.run(command, cwd=source, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, check=False)
            result[name] = {"command": command, "exit_code": run.returncode, "output": run.stdout}
        except OSError as exc:
            result[name] = {"command": command, "exit_code": None, "error": repr(exc)}
    return result


def record(source, out, label, command, allow_dirty=False, timeout_seconds=5400):
    source, out = Path(source).resolve(), Path(out).resolve()
    out.mkdir(parents=True, exist_ok=True)
    receipt_path = out / (label + ".json")
    if receipt_path.exists() or (out / (label + ".log")).exists():
        raise RuntimeError("refusing to overwrite an existing command receipt: " + label)
    before = snapshot(source)
    if before["status"] and not allow_dirty:
        raise RuntimeError("qualification source is not clean")
    write_json(out / (label + ".inputs.json"), before)
    started, clock = utc(), time.monotonic()
    native_exit, error = None, None
    timed_out = threading.Event()
    collection_finished = threading.Event()
    timer = None
    process = None

    def terminate_group():
        if process is None:
            return
        try:
            if os.name == "posix":
                os.killpg(process.pid, signal.SIGKILL)
            elif process.poll() is None:
                process.kill()
        except ProcessLookupError:
            pass
    try:
        with (out / (label + ".log")).open("w", encoding="utf-8") as log:
            process = subprocess.Popen(command, cwd=source, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, errors="replace", bufsize=1, start_new_session=(os.name == "posix"))
            def expire():
                # A descendant can retain the output pipe after its leader
                # exits. Bound the whole collection, not only leader lifetime.
                if not collection_finished.is_set():
                    timed_out.set()
                    terminate_group()
            timer = threading.Timer(timeout_seconds, expire)
            timer.daemon = True
            timer.start()
            for line in process.stdout:
                log.write(line)
                log.flush()
                print(line, end="", flush=True)
            native_exit = process.wait()
    except Exception as exc:
        error = repr(exc)
        terminate_group()
        if process is not None:
            try:
                native_exit = process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                error += "; child could not be reaped after termination"
    finally:
        collection_finished.set()
        if timer is not None:
            timer.cancel()
            timer.join(timeout=1)
    finished, elapsed = utc(), time.monotonic() - clock
    after, end_snapshot_error = None, None
    try:
        after = snapshot(source)
        write_json(out / (label + ".end-inputs.json"), after)
    except Exception as exc:
        end_snapshot_error = repr(exc)
    digest_errors = {}

    def receipt_digest(path):
        try:
            return digest(path)
        except Exception as exc:
            digest_errors[path.name] = repr(exc)
            return None

    receipt = {
        "schema": "concord.qualification.command/1",
        "source": str(source),
        "command": command,
        "started_utc": started,
        "finished_utc": finished,
        "duration_seconds": elapsed,
        "native_exit_code": native_exit,
        "launcher_error": error,
        "timed_out": timed_out.is_set(),
        "timeout_seconds": timeout_seconds,
        "head": before["head"],
        "tree": before["tree"],
        "inputs_changed": before != after if after is not None else None,
        "end_snapshot_error": end_snapshot_error,
        "source_clean_at_start": not before["status"],
        "source_clean_at_end": not after["status"] if after is not None else None,
        "log_sha256": receipt_digest(out / (label + ".log")),
        "inputs_sha256": receipt_digest(out / (label + ".inputs.json")),
        "end_inputs_sha256": receipt_digest(out / (label + ".end-inputs.json")),
        "digest_errors": digest_errors,
        "selected_environment": {k: v for k, v in os.environ.items() if k in {
            "CARGO_TARGET_DIR", "CARGO_BUILD_JOBS", "CARGO_INCREMENTAL",
            "CARGO_PROFILE_DEV_DEBUG", "CARGO_PROFILE_TEST_DEBUG",
            "RUSTFLAGS", "RUSTC_BOOTSTRAP", "RUSTUP_TOOLCHAIN",
        }},
    }
    write_json(receipt_path, receipt)
    print("QUALIFICATION_RECEIPT " + json.dumps(receipt, sort_keys=True), flush=True)
    return native_exit == 0 and error is None and not timed_out.is_set() and end_snapshot_error is None and not digest_errors and before == after


class VisibleHtml(HTMLParser):
    """Capture actual rendered-document text without executable/style payloads."""

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.hidden_depth = 0
        self.parts = []

    def handle_starttag(self, tag, attrs):
        if tag in {"script", "style"}:
            self.hidden_depth += 1

    def handle_endtag(self, tag):
        if tag in {"script", "style"} and self.hidden_depth:
            self.hidden_depth -= 1

    def handle_data(self, data):
        if not self.hidden_depth and data.strip():
            self.parts.append(data.strip())

    def text(self):
        return " ".join(self.parts)


def collect(source, out):
    source, out = Path(source).resolve(), Path(out).resolve()
    target = Path(os.environ.get("CARGO_TARGET_DIR", str(source / "target"))).resolve()
    destination = out / "generated"
    destination.mkdir(parents=True, exist_ok=True)
    # Keep actual HTML and package/source archives. Build products and credentials
    # are never searched or copied.
    selections = [
        (target / "doc", "rustdoc", ("*.html", "*.css", "*.js", "*.svg", "*.png", "*.woff2")),
        (target / "doc-audit/doc", "rustdoc-audit", ("*.html", "*.css", "*.js", "*.svg", "*.png", "*.woff2")),
        (target / "doc-audit", "rustdoc-audit-input", ("fixture.rs",)),
        (target / "package", "packages", ("*.crate",)),
        (source / "target" / "source-export", "source-export", ("*.tar", "*.txt")),
    ]
    files = []
    for root, label, patterns in selections:
        if not root.is_dir():
            continue
        paths = set()
        for pattern in patterns:
            paths.update(root.rglob(pattern) if label.startswith("rustdoc") else root.glob(pattern))
        for path in sorted(paths):
            relative = Path(label) / path.relative_to(root)
            copied = destination / relative
            copied.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(path, copied)
            files.append({"path": relative.as_posix(), "bytes": copied.stat().st_size, "sha256": digest(copied)})
    package_runs = Path(os.environ.get("TMPDIR", "/tmp")) / "concord-package-verification"
    if package_runs.is_dir():
        for path in sorted(package_runs.rglob("*")):
            if not path.is_file() or "target" in path.relative_to(package_runs).parts:
                continue
            if path.name not in {"Cargo.toml", "Cargo.toml.orig", "Cargo.lock", "metadata.json"}:
                continue
            relative = Path("extracted-package-inputs") / path.relative_to(package_runs)
            copied = destination / relative
            copied.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(path, copied)
            files.append({"path": relative.as_posix(), "bytes": copied.stat().st_size, "sha256": digest(copied)})
    write_json(out / "generated-files.json", {"captured_utc": utc(), "source": snapshot(source), "files": files})
    selected_pages = [
        "doc/concord_examples/contracts/catalog_v3/struct.CatalogApi.html",
        "doc/concord_examples/contracts/catalog_v3/catalog_api/struct.CatalogApiBuilder.html",
        "doc/concord_examples/contracts/catalog_v3/catalog_api/regional/struct.Regional.html",
        "doc/concord_examples/contracts/catalog_v3/catalog_api/regional/struct.FindItemRequest.html",
        "doc/concord_examples/contracts/catalog_v3/catalog_api/regional/struct.ItemSummariesRequest.html",
        "doc-audit/doc/concord_doc_audit_fixture/struct.LegacyApi.html",
        "doc-audit/doc/concord_doc_audit_fixture/legacy_api/struct.ExportRequest.html",
        "doc-audit/doc/concord_doc_audit_fixture/legacy_api/struct.LegacyApiBuilder.html",
        "doc/concord_core/contracts/trait.IntoValue.html",
        "doc/concord_core/contracts/struct.RequestPlan.html",
        "doc/concord_core/contracts/struct.ApiResponse.html",
        "doc/concord_core/contracts/struct.ByteStream.html",
        "doc/concord_core/records/struct.ItemStream.html",
        "doc/concord_core/pagination/struct.TraversalSummary.html",
    ]
    pages = [{"path": name, "present": (target / name).is_file(),
              "sha256": digest(target / name) if (target / name).is_file() else None}
             for name in selected_pages]
    write_json(out / "selected-rustdoc-pages.json", {"pages": pages, "qualitative_review_performed": False})
    rendered_text = []
    for page in pages:
        if page["present"]:
            parser = VisibleHtml()
            parser.feed((target / page["path"]).read_text(encoding="utf-8"))
            entry = {**page, "visible_text": parser.text()}
            rendered_text.append(entry)
            print("QUALIFICATION_RUSTDOC_TEXT " + json.dumps(entry, ensure_ascii=True), flush=True)
    write_json(out / "selected-rustdoc-text.json", {"pages": rendered_text, "qualitative_review_performed": False})
    print("QUALIFICATION_GENERATED_FILES " + json.dumps({"count": len(files), "bytes": sum(p["bytes"] for p in files)}))
    if not all(page["present"] for page in pages):
        raise RuntimeError("selected Rustdoc output is incomplete; see selected-rustdoc-pages.json")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=["record", "environment", "collect"])
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--label", default="command")
    parser.add_argument("--allow-dirty", action="store_true")
    args, command = parser.parse_known_args()
    if command[:1] == ["--"]:
        command = command[1:]
    if args.mode == "record":
        if not command:
            parser.error("record requires a native command after --")
        return 0 if record(args.source, args.out, args.label, command, args.allow_dirty) else 1
    if command:
        parser.error("unexpected command")
    if args.mode == "environment":
        write_json(args.out / "environment.json", environment(args.source))
    else:
        collect(args.source, args.out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
