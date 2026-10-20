"""Read-only GitHub Actions evidence downloads; never log redirect URLs or tokens."""
from __future__ import annotations

import hashlib
import http.client
import json
import os
from pathlib import Path
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request

API_BASE = "https://api.github.com/repos/gqsnt/concord_api/"


def _origin(url: str) -> tuple[str, str | None, int | None]:
    value = urllib.parse.urlsplit(url)
    return (value.scheme.lower(), value.hostname, value.port or 443)


class _SafeRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        target = urllib.parse.urlsplit(newurl)
        if target.scheme.lower() != "https" or not target.hostname or target.username or target.password:
            raise urllib.error.URLError("unsafe redirect rejected")
        redirected = super().redirect_request(req, fp, code, msg, headers, newurl)
        if redirected is not None:
            names = {"cookie", "proxy-authorization"}
            if _origin(req.full_url) != _origin(newurl):
                names.add("authorization")
            for name, _ in list(redirected.header_items()):
                if name.lower() in names:
                    redirected.remove_header(name)
        return redirected


_OPENER = urllib.request.build_opener(_SafeRedirect())


def _request(path: str):
    parsed = urllib.parse.urlsplit(path)
    if (parsed.scheme or parsed.netloc or parsed.fragment or path.startswith("/")
            or ".." in parsed.path.split("/")):
        raise RuntimeError("expected a relative repository API path")
    token = os.environ.get("GH_TOKEN") or os.environ.get("GITHUB_TOKEN")
    if not token:
        raise RuntimeError("GH_TOKEN or GITHUB_TOKEN is required")
    request = urllib.request.Request(
        API_BASE + path,
        headers={
            "Authorization": "Bearer " + token,
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
            "User-Agent": "concord-v3-evidence-collector/1",
        },
        method="GET",
    )
    return _OPENER.open(request, timeout=60)


def github_bytes(path: str, destination, *, max_bytes: int = 1024**3) -> dict:
    """Save native response bytes; return digest metadata without signed URLs."""
    destination = Path(destination)
    destination.parent.mkdir(parents=True, exist_ok=True)
    for attempt in range(3):
        temporary = None
        try:
            digest = hashlib.sha256()
            size = 0
            with _request(path) as response:
                status = response.status
                expected_size = response.headers.get("Content-Length")
                if status != 200:
                    raise RuntimeError("unexpected HTTP success status")
                with tempfile.NamedTemporaryFile(
                    dir=destination.parent, prefix=destination.name + ".", suffix=".part", delete=False
                ) as output:
                    temporary = Path(output.name)
                    while chunk := response.read(1024 * 1024):
                        size += len(chunk)
                        if size > max_bytes:
                            raise RuntimeError("response exceeds configured byte limit")
                        digest.update(chunk)
                        output.write(chunk)
                    if expected_size is not None and size != int(expected_size):
                        raise http.client.IncompleteRead(b"", max(0, int(expected_size) - size))
                    output.flush()
                    os.fsync(output.fileno())
            os.replace(temporary, destination)
            return {"sha256": digest.hexdigest(), "bytes": size, "http_status": status}
        except Exception as error:
            code = error.code if isinstance(error, urllib.error.HTTPError) else None
            retry = (code in {429, 500, 502, 503, 504} or
                     (code is None and isinstance(error, (
                         urllib.error.URLError, TimeoutError, ConnectionError, http.client.IncompleteRead
                     ))))
            label = f"HTTP {code}" if code is not None else type(error).__name__
            if not retry or attempt == 2:
                raise RuntimeError(f"GitHub GET failed ({label})") from None
            time.sleep(2 ** attempt)
        finally:
            if temporary is not None:
                temporary.unlink(missing_ok=True)
    raise RuntimeError("GitHub GET retry bound reached")


def github_json(path: str, destination=None):
    """Return decoded JSON; an explicit destination preserves the native API bytes."""
    if destination is None:
        with tempfile.TemporaryDirectory(prefix="concord-github-read-") as temporary:
            return github_json(path, Path(temporary) / "response.json")
    github_bytes(path, destination, max_bytes=32 * 1024 * 1024)
    try:
        return json.loads(Path(destination).read_bytes())
    except (ValueError, UnicodeError):
        raise RuntimeError("GitHub response is not valid JSON") from None

# Native CI collection. Derived excerpts never determine job success.
import argparse
from collections import Counter, deque
from datetime import datetime, timezone
import io
import re
import sys
import zipfile

RUNS = (
    ("CI", 25, 38974543150, "6085722f7a88e9a31eda060cad10b22dc38240c7"),
    ("CI", 26, 38976006579, "91c6cb911aecb4278e1fbd8de1e6053591a3bbff"),
    ("CI", 27, 38977448836, "4265c06884d88009fb72071ef6fce8482b43dcd2"),
    ("CI", 28, 38978327078, "5b3772236d7c1b71f86ebeba35f7d33a30cf3b64"),
    ("CI", 29, 38981477798, "579e8eac9a436a57ebc5ea3fcc2e7da3cf330c31"),
    ("V3 qualification evidence", 1, 38981477842, "579e8eac9a436a57ebc5ea3fcc2e7da3cf330c31"),
)
ANSI = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")
URL = re.compile(r"https?://[^\s<>\"']+")
CATEGORIES = {
    "command_candidate": re.compile(r"\b(?:cargo(?:\s|\+)|just\s|rust-analyzer\s|rustc\s|pwsh\s)"),
    "test_result": re.compile(r"test result:|Summary.*tests? run:|all doctests ran"),
    "completion_marker": re.compile(
        r"source-export (?:exact HEAD|manifest|extracted|excludes)|"
        r"MSRV semantic UI checks passed|packaged downstream verification passed|"
        r"repository policy checks passed|analysis-stats|diagnostics \."
    ),
    "error_shaped_line": re.compile(r"##\[error\]|(?:^|\s)error(?:\[|:)|panicked at|\bFAILED\b"),
}


def utc():
    return datetime.now(timezone.utc).isoformat()


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def redact(line):
    line = ANSI.sub("", line.rstrip("\r\n"))
    for name in ("GH_TOKEN", "GITHUB_TOKEN"):
        token = os.environ.get(name)
        if token:
            line = line.replace(token, "<TOKEN OMITTED>")
    line = URL.sub("<URL OMITTED>", line)
    line = re.sub(r"\b(?:github_pat_[A-Za-z0-9_]+|gh[pousr]_[A-Za-z0-9_]+)\b", "<TOKEN OMITTED>", line)
    return line[:4000] + (" <LINE TRUNCATED>" if len(line) > 4000 else "")


def excerpt(stream):
    first, tail, errors, last_lines = [], deque(maxlen=160), deque(maxlen=200), deque(maxlen=12)
    counts = Counter()
    line_number = 0
    for line_number, native_line in enumerate(stream, 1):
        cleaned = ANSI.sub("", native_line)
        row = {"line": line_number, "text": redact(native_line)}
        last_lines.append(row)
        categories = [name for name, pattern in CATEGORIES.items() if pattern.search(cleaned)]
        if not categories:
            continue
        counts.update(categories)
        row = {**row, "categories": categories}
        if len(first) < 40:
            first.append(row)
        else:
            tail.append(row)
        if "error_shaped_line" in categories:
            errors.append(row)
    selected = {row["line"]: row for row in [*first, *tail, *errors]}
    return {
        "line_count": line_number,
        "matching_counts": dict(counts),
        "sample_policy": "first 40 matches, last 160 matches, last 200 error-shaped matches; 4000 characters per line",
        "samples": [selected[key] for key in sorted(selected)],
        "last_lines": list(last_lines),
        "warning": "Derived text candidates only. API job/step conclusions determine status; negative tests may emit error-shaped text.",
    }


def inspect_native_zip(path):
    members = []
    with zipfile.ZipFile(path) as archive:
        entries = archive.infolist()
        if len(entries) > 10000 or sum(item.file_size for item in entries) > 4 * 1024**3:
            raise RuntimeError("native log archive exceeds inspection bounds")
        for item in entries:
            if item.is_dir():
                continue
            # No member is extracted to disk. Reading its bytes verifies ZIP CRC.
            with archive.open(item) as source:
                with io.TextIOWrapper(source, encoding="utf-8", errors="replace") as text:
                    derived = excerpt(text)
            members.append({
                "member": item.filename, "uncompressed_bytes": item.file_size,
                "compressed_bytes": item.compress_size, "crc32": f"{item.CRC:08x}",
                "crc_verified": True, "excerpt": derived,
            })
    return {"archive": path.name, "members": members}


def validate_run(value, workflow, number, run_id, sha):
    if (
        value.get("id") != run_id or value.get("run_number") != number
        or value.get("head_sha") != sha or value.get("name") != workflow
        or value.get("event") != "push" or value.get("head_branch") != "concord-v3"
        or value.get("repository", {}).get("full_name", "").lower() != "gqsnt/concord_api"
    ):
        raise RuntimeError(f"run identity mismatch for {run_id}")


def publish_linux_review_text(artifact, archive_path):
    # Copies of actual output are exposed separately because the complete
    # implementation stdout can exceed connector response limits.
    if not artifact["name"].startswith("v3-linux-qualification-"):
        return
    with zipfile.ZipFile(archive_path) as archive:
        for name in ["selected-rustdoc-text.json", "selected-rustdoc-pages.json",
                     "linux-implementation.json", "linux-analyzer.json", "release-audit.json"]:
            if name not in archive.namelist():
                continue
            if archive.getinfo(name).file_size > 16 * 1024 * 1024:
                raise RuntimeError("unexpectedly large selected Linux review receipt")
            payload = json.loads(archive.read(name))
            if name == "selected-rustdoc-text.json":
                for page in payload["pages"]:
                    print("CI_EVIDENCE_RUSTDOC_TEXT " + json.dumps(page, ensure_ascii=True), flush=True)
            elif name == "selected-rustdoc-pages.json":
                print("CI_EVIDENCE_RUSTDOC_PAGES " + json.dumps(payload, sort_keys=True), flush=True)
            else:
                print("CI_EVIDENCE_COMMAND_RECEIPT " + json.dumps(payload, sort_keys=True), flush=True)


def collect_run(workflow, number, run_id, sha, root, deadline):
    directory = root / f"run-{number}-{run_id}"
    directory.mkdir(parents=True, exist_ok=True)
    api_path = f"actions/runs/{run_id}"
    poll = 0
    while True:
        before = github_json(api_path, directory / f"run-before-{poll:03}.json")
        validate_run(before, workflow, number, run_id, sha)
        if before["status"] == "completed":
            break
        if time.monotonic() >= deadline:
            return {"workflow": workflow, "run_number": number, "run_id": run_id, "head_sha": sha,
                    "collection": "pending", "status": before["status"]}
        time.sleep(min(30, max(0, deadline - time.monotonic())))
        poll += 1

    attempt = before["run_attempt"]
    result = {
        "workflow": workflow, "run_number": number, "run_id": run_id, "head_sha": sha, "attempt": attempt,
        "status": before["status"], "conclusion": before["conclusion"],
        "observed_at_utc": utc(), "collection": "complete", "jobs": [],
        "native_run_receipt": f"{directory.name}/run-before-{poll:03}.json",
    }
    jobs = []
    for page in range(1, 101):
        filename = f"jobs-page-{page:03}.json"
        response = github_json(
            f"{api_path}/attempts/{attempt}/jobs?per_page=100&page={page}",
            directory / filename,
        )
        batch = response["jobs"]
        for index, job in enumerate(batch):
            if job.get("run_id") != run_id or job.get("head_sha") != sha:
                raise RuntimeError(f"job identity mismatch in run {run_id}")
            jobs.append(job)
            result["jobs"].append({
                key: job.get(key) for key in
                ("id", "name", "status", "conclusion", "started_at", "completed_at", "steps")
            } | {"native_receipt": f"{directory.name}/{filename}", "json_pointer": f"/jobs/{index}"})
        if len(jobs) >= response["total_count"]:
            break
        if not batch:
            raise RuntimeError("incomplete jobs pagination")
    else:
        raise RuntimeError("jobs pagination bound exceeded")

    archive = directory / f"run-{run_id}-attempt-{attempt}-native.zip"
    result["native_archive"] = github_bytes(f"{api_path}/attempts/{attempt}/logs", archive)
    result["native_archive"]["file"] = f"{directory.name}/{archive.name}"
    write_json(directory / "native-archive-excerpts.json", inspect_native_zip(archive))

    for job in jobs:
        if job["name"] != "Windows implementation qualification":
            continue
        log = directory / f"job-{job['id']}-native.log"
        metadata = github_bytes(f"actions/jobs/{job['id']}/logs", log)
        result.setdefault("native_windows_logs", []).append(
            {"job_id": job["id"], "file": f"{directory.name}/{log.name}", **metadata}
        )
        with log.open("r", encoding="utf-8", errors="replace") as stream:
            derived = excerpt(stream)
        write_json(directory / f"job-{job['id']}-excerpts.json", derived)
        if number in {28, 29}:
            print("CI_EVIDENCE_WINDOWS_EXCERPT " + json.dumps({"run_id": run_id, "job_id": job["id"], **derived}, sort_keys=True), flush=True)

    result["native_artifacts"] = []
    artifact_count = 0
    for page in range(1, 101):
        filename = f"artifacts-page-{page:03}.json"
        response = github_json(
            f"{api_path}/artifacts?per_page=100&page={page}", directory / filename
        )
        batch = response["artifacts"]
        for index, artifact in enumerate(batch):
            binding = artifact.get("workflow_run", {})
            if binding.get("id") != run_id or binding.get("head_sha") != sha or artifact.get("expired"):
                raise RuntimeError(f"artifact identity or availability mismatch in run {run_id}")
            destination = directory / f"artifact-{artifact['id']}-native.zip"
            metadata = github_bytes(f"actions/artifacts/{artifact['id']}/zip", destination)
            expected = artifact.get("digest")
            if expected is None or expected != "sha256:" + metadata["sha256"]:
                raise RuntimeError(f"artifact digest mismatch or absent for {artifact['id']}")
            result["native_artifacts"].append({
                "id": artifact["id"], "name": artifact["name"],
                "file": f"{directory.name}/{destination.name}", **metadata,
                "provider_digest_verified": True,
                "native_receipt": f"{directory.name}/{filename}",
                "json_pointer": f"/artifacts/{index}",
            })
            publish_linux_review_text(artifact, destination)
        artifact_count += len(batch)
        if artifact_count >= response["total_count"]:
            break
        if not batch:
            raise RuntimeError("incomplete artifacts pagination")
    else:
        raise RuntimeError("artifacts pagination bound exceeded")

    after = github_json(api_path, directory / "run-after.json")
    validate_run(after, workflow, number, run_id, sha)
    for key in ("run_attempt", "status", "conclusion"):
        if after.get(key) != before.get(key):
            raise RuntimeError(f"run state changed during collection for {run_id}")
    result["identity_and_state_rechecked"] = True
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", "--output", dest="output", type=Path, required=True)
    parser.add_argument("--wait-seconds", type=int, default=5400)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    deadline = time.monotonic() + max(0, args.wait_seconds)
    manifest = {
        "schema": "concord-ci-native-evidence/1", "started_at_utc": utc(),
        "repository": "gqsnt/concord_api", "native_files_are_unmodified": True,
        "derived_excerpts_are_redacted_and_bounded": True, "runs": [],
    }
    for workflow, number, run_id, sha in RUNS:
        try:
            result = collect_run(workflow, number, run_id, sha, args.output, deadline)
        except Exception as error:
            detail = redact(str(error)) if isinstance(error, RuntimeError) else type(error).__name__
            result = {"workflow": workflow, "run_number": number, "run_id": run_id, "head_sha": sha,
                      "collection": "error", "error": detail}
        manifest["runs"].append(result)
        print("CI_EVIDENCE_RUN " + json.dumps(result, sort_keys=True), flush=True)
    manifest["completed_at_utc"] = utc()
    manifest["files"] = []
    for path in sorted(args.output.rglob("*")):
        if path.is_file() and path.name != "manifest.json":
            digest = hashlib.sha256()
            with path.open("rb") as source:
                while chunk := source.read(1024 * 1024):
                    digest.update(chunk)
            manifest["files"].append({
                "path": path.relative_to(args.output).as_posix(),
                "bytes": path.stat().st_size, "sha256": digest.hexdigest(),
            })
    write_json(args.output / "manifest.json", manifest)
    return 0 if all(row["collection"] == "complete" for row in manifest["runs"]) else 1


if __name__ == "__main__":
    sys.exit(main())
