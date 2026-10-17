#!/usr/bin/env python3
"""Read preserved native layout fields; this performs no new compilation."""
import argparse
import hashlib
import json
from pathlib import Path
import zipfile

from qualification_ci_logs import github_bytes, github_json, write_json

ARTIFACT = 12082592148
SOURCE = "2f647826dfefb61a6e5a6770f8af2c523198011e"
SHA256 = "fd57284f4873ea4c87bb4a1e6e43b7fd2a669aa0fca7843afc605a0587558b02"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    artifact = github_json(f"actions/artifacts/{ARTIFACT}")
    if artifact["workflow_run"]["head_sha"] != SOURCE or artifact["expired"]:
        raise RuntimeError("native layout artifact source changed or expired")
    path = args.out / "native-layouts.zip"
    result = github_bytes(f"actions/artifacts/{ARTIFACT}/zip", path)
    if result["sha256"] != SHA256 or artifact.get("digest") != "sha256:" + SHA256:
        raise RuntimeError("native layout artifact digest changed")
    selected = []
    with zipfile.ZipFile(path) as archive:
        for candidate in ["historical", "v3-final", "v3-wrapper-restored"]:
            name = "layouts/" + candidate + "-types.json"
            member = archive.getinfo(name)
            if member.file_size > 64 * 1024 * 1024:
                raise RuntimeError("oversized native layout JSON")
            raw = archive.read(name)
            values = json.loads(raw)
            blocks = []
            for value in values:
                kind = value["type"]
                execution = (kind.startswith("{async ") and "Bounded" in kind
                             and any(key in kind for key in [
                                 "execute_finalized_operation", "execute_prepared_operation",
                                 "execute_request_as", "execute_request<", "IntoFuture"]))
                named_state = ("<" not in kind and "{" not in kind and
                               any(key in kind for key in [
                                   "Journal", "OperationContext", "PreparedRequest",
                                   "StreamContext", "StreamObservation", "ResponseInput"]))
                if execution or named_state:
                    blocks.append(value)
            if not any("Bounded" in block["type"] for block in blocks):
                raise RuntimeError("bounded execution native layout is missing")
            row = {"candidate": candidate, "source_member": name,
                   "source_member_sha256": hashlib.sha256(raw).hexdigest(),
                   "artifact_sha256": SHA256, "types": blocks}
            selected.append(row)
            print("NATIVE_LAYOUT_EXCERPT " + json.dumps(row, sort_keys=True), flush=True)
    write_json(args.out / "selected-native-layouts.json", {
        "derivation": "Exact field blocks from the preserved native rustc -Zprint-type-sizes artifact; no compilation or measurement was repeated.",
        "artifact": artifact, "rows": selected,
    })
    # The final return already includes the complete original artifact once.
    path.unlink()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
