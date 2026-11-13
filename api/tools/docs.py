#!/usr/bin/env python3
"""Build the four user crates with strict rustdoc and list name-only docs.

Generated macro/tour documentation remains owned by its writer. This inventory
flags terse source comments for human review; compiler missing_docs and rustdoc
check completeness and link resolution, including feature-gated items.
"""

import argparse
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

API = Path(__file__).resolve().parents[1]
CRATES = ("concord", "concord-core", "concord-extras", "concord-testkit")
STOP = {"a", "an", "the", "of", "for", "this", "its", "is", "it", "to", "get", "return", "returns"}


def name_only():
    for crate in CRATES:
        for path in sorted((API / crate / "src").rglob("*.rs")):
            docs = []
            for line, source in enumerate(path.read_text().splitlines(), 1):
                text = source.strip()
                if text.startswith("///"):
                    docs.append(text[3:].strip())
                elif text.startswith("#[") or not text:
                    continue
                else:
                    match = re.match(r"pub\s+(?:(?:async|const|unsafe)\s+)?(?:fn|struct|enum|trait|type|mod|const)\s+(\w+)", text)
                    if docs and match:
                        words = set(re.findall(r"[a-z]+", " ".join(docs).lower())) - STOP
                        name = set(re.findall(r"[A-Z]?[a-z]+|[A-Z]+(?![a-z])", match[1]))
                        name = {word.lower() for word in name}
                        if words and words <= name:
                            yield f"{path.relative_to(API)}:{line}: {match[1]}: {' '.join(docs)}"
                    docs = []


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--audit-only", action="store_true")
    args = parser.parse_args()
    rows = list(name_only())
    print("\n".join(rows), flush=True)
    print(f"DOCS: {len(rows)} name-only comments", flush=True)
    if args.audit_only:
        return bool(rows)
    env = os.environ.copy()
    target = Path(env.get("CARGO_TARGET_DIR", API / "target"))
    shutil.rmtree(target / "doc/concord", ignore_errors=True)
    env.update(CARGO_NET_OFFLINE="true", CARGO_BUILD_JOBS="4", CARGO_TARGET_DIR=str(target),
               RUSTDOCFLAGS="-D warnings -D missing_docs -D rustdoc::broken_intra_doc_links")
    command = ["cargo", "doc", "--offline", "--no-deps", "--features", "concord/tower,concord/multipart"]
    for crate in CRATES:
        command += ["-p", crate]
    result = subprocess.run(command, cwd=API, env=env)
    if result.returncode:
        return result.returncode
    page = target / "doc/concord/arg/trait.Arg.html"
    if not page.is_file():
        print("DOCS: missing concord::arg::Arg page", file=sys.stderr)
        return 1
    for rendered in (target / "doc/concord").rglob("*.html"):
        if re.search(r'id="impl[^"]*Arg', rendered.read_text()):
            print(f"DOCS: Arg blanket implementations became visible in {rendered.name}", file=sys.stderr)
            return 1
    print("DOCS: facade Arg page exists; blanket implementations are hidden", flush=True)
    return bool(rows)


if __name__ == "__main__":
    sys.exit(main())
