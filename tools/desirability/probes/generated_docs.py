"""Opt-in I3 endpoint doc exposure. Raw rustdoc docs only; never reads truth or runs readers.

Build an exact git archive, using the pilot's per-arm target marker plus rustdoc's
crate-version marker (the H04 stale-JSON safeguard). Keep this separate from legacy packing.
"""
import hashlib
import os
import re
import subprocess
import sys

import common as C

sys.path.insert(0, os.path.join(os.path.dirname(C.HERE), "pilot"))
import arms as A  # noqa: E402

USE = "i3-generated-docs"
APPENDIX = "\n## 5. Generated endpoint documentation\n\n"


def sha256(text):
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def check_json(rd_dir, banks, marker):
    for bank in banks:
        path = os.path.join(rd_dir, bank["crate"] + ".json")
        if not os.path.isfile(path) or C.read_json(path).get("crate_version") != marker:
            raise SystemExit(f"missing or stale generated-docs JSON: {path}; expected {marker}")


def prepare(repo, commit, banks, no_build=False):
    """Return (exact arm source, JSON directory, build record); fail closed on stale JSON."""
    if not os.path.isabs(C.CARGO_TARGET_DIR):
        raise SystemExit("--generated-docs requires an absolute CARGO_TARGET_DIR")
    sha = A.full_sha(commit or "HEAD", repo)
    source = os.path.join(C.CARGO_TARGET_DIR, "i3-generated-docs-src", sha)
    provenance = A.materialise(sha, source, repo)
    target = A.target_dir(sha, USE, C.CARGO_TARGET_DIR)
    marker = f"i3-tree-{sha}"
    env = C.cargo_env()
    env["CARGO_TARGET_DIR"] = target
    env["RUSTDOCFLAGS"] = ("-Z unstable-options --output-format json --document-private-items "
                           f"--crate-version {marker}")
    host_info = subprocess.run(["rustc", "+nightly", "-vV"], env=env, capture_output=True,
                               text=True, check=True).stdout
    host = re.search(r"(?m)^host: (.+)$", host_info).group(1)
    args = ["cargo", "+nightly", "doc", "--locked", "--offline", "--no-deps", "--lib", "--target", host]
    for pkg in sorted({b["pkg"] for b in banks}):
        args += ["-p", pkg]
    A.check_tree(source, sha)
    A.check_target(target, sha, USE)
    log = {"commit": sha, "tree": source, "tree_provenance": provenance,
           "cargo_target_dir": target, "target_provenance": A.check_target(target, sha, USE),
           "cmd": args, "rustdocflags": env["RUSTDOCFLAGS"], "provenance": marker,
           "rustdoc_toolchain": host_info.splitlines()[0], "build_skipped": no_build}
    if not no_build:
        run = subprocess.run(args, cwd=source, env=env, capture_output=True, text=True)
        log.update(rustdoc_build_exit=run.returncode, rustdoc_build_stderr=run.stderr)
        if run.returncode:
            C.write_json(os.path.join(target, "i3-build.json"), log)
            raise SystemExit(f"generated-docs rustdoc failed: {run.stderr[-3000:]}")
    rd_dir = os.path.join(target, host, "doc")
    check_json(rd_dir, banks, marker)
    C.write_json(os.path.join(target, "i3-build.json"), log)
    return source, rd_dir, log


class EndpointDocs:
    """Resolve client -> scope methods -> endpoint by actual rustdoc return-type IDs.

    No guessed generated type names, signatures, implementation text or bank values are rendered.
    Crate-visible generated clients remain supported, as in the legacy I3 materials; visibility
    is recorded in the audit. Missing/ambiguous/hidden items are refused, never silently omitted.
    """
    def __init__(self, dj, sdk):
        self.idx = {str(k): v for k, v in dj["index"].items()}
        self.sdk = sdk
        self.paths = {str(k): v["path"] for k, v in dj["paths"].items() if v.get("crate_id") == 0}
        # Private module paths (e.g. Kubernetes' api) can be absent from `paths`.
        def walk(iid, parent):
            item = self.idx[str(iid)]
            path = parent + [item["name"]] if item.get("name") else parent
            self.paths.setdefault(str(iid), path)
            for child in item.get("inner", {}).get("module", {}).get("items", []):
                walk(child, path)
        walk(dj["root"], [])

    def one(self, candidates, label):
        candidates = list(candidates)
        if len(candidates) != 1:
            raise SystemExit(f"generated-docs: {label}: expected one item, found {len(candidates)}")
        iid = str(candidates[0])
        item = self.idx[iid]
        if "hidden" in str(item.get("attrs", [])) or not item.get("docs"):
            raise SystemExit(f"generated-docs: {label}: hidden item or missing docs")
        return iid

    def method(self, owner, name):
        item = self.idx[owner]
        kind = item["inner"].get("struct")
        if kind is None:
            raise SystemExit(f"generated-docs: navigation owner {owner} is not a struct")
        methods = []
        for imp_id in kind.get("impls", []):
            imp = self.idx[str(imp_id)]["inner"]["impl"]
            if imp.get("trait") is not None:
                continue
            for mid in imp.get("items", []):
                m = self.idx[str(mid)]
                if m.get("name") == name and "function" in m["inner"] and m.get("visibility") == "public":
                    methods.append(str(mid))
        return self.one(methods, f"{self.idx[owner]['name']}::{name}")

    def returns(self, method):
        output = self.idx[method]["inner"]["function"]["sig"].get("output") or {}
        iid = str(output.get("resolved_path", {}).get("id"))
        if iid not in self.idx:
            raise SystemExit(f"generated-docs: navigation method {method} has no local return type")
        return iid

    def endpoint(self, pick):
        module = list(self.sdk.decl_module(pick["client_file"]))
        want = [self.sdk.crate] + module + [pick["client"]]
        owner = self.one((iid for iid, path in self.paths.items()
                          if path == want and "struct" in self.idx.get(iid, {}).get("inner", {})),
                         "::".join(want))
        for name in pick["scopes"]:
            owner = self.returns(self.method(owner, name))
        method = self.method(owner, pick["endpoint"])
        request = self.returns(method)
        self.one([request], f"request type for {pick['key']}")
        owner_path = self.paths.get(owner)
        request_path = self.paths.get(request)
        if not owner_path or not request_path:
            raise SystemExit(f"generated-docs: no rustdoc path for {pick['key']}")
        return [("request_type", "::".join(request_path), request),
                ("navigation_method", "::".join(owner_path + [pick["endpoint"]]), method)]


def example_candidates(docs, bank, related, navs):
    """Conservative, truth-free exposure flags, with verbatim evidence for manual review.

    A literal HTTP target (even a partial request), URL, code fence, or endpoint call chain
    is a candidate. Overflags are intentional: a declared default request counts too. Templates
    are reported as candidates as well; the audit does not claim a candidate is a scored answer.
    Scan against the entire SDK bank, including items outside the current session.
    """
    found = []
    rules = [("http_request", r"\b(?:GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS|CONNECT|TRACE)\s+(?:https?://|/)[^\s`\"<>]+"),
             ("url", r"https?://[^\s`\"<>]+"),
             ("fenced_example", r"(?m)^[ \t]*(?:`{3,}|~{3,}).*$")]
    for rule, pattern in rules:
        for m in re.finditer(pattern, docs):
            found.append({"rule": rule, "line": docs.count("\n", 0, m.start()) + 1,
                          "excerpt": m.group(), "bank_items": sorted(related)})
    for text, paths in C.scan_chains(docs, navs):
        matches = [it["id"] for it in bank["items"]
                   if tuple(bank["picks"][it["pick"]]["scopes"]) +
                   (bank["picks"][it["pick"]]["endpoint"],) in paths]
        if matches:
            found.append({"rule": "endpoint_call_chain", "excerpt": text, "bank_items": sorted(matches)})
    return found


def appendix(dj, sdk, bank, session):
    reader = EndpointDocs(dj, sdk)
    blocks = {}
    for it in bank["items"]:
        if it["session"] != session:
            continue
        pick = bank["picks"][it["pick"]]
        for kind, path, iid in reader.endpoint(pick):
            key = (kind, path)
            if key not in blocks:
                blocks[key] = {"kind": kind, "path": path, "rustdoc_id": iid, "items": []}
            blocks[key]["items"].append(it["id"])
    text, audit = "", []
    navs = C.endpoint_navs(sdk)
    for (kind, path), block in sorted(blocks.items()):
        item = reader.idx[block["rustdoc_id"]]
        docs = item["docs"]  # raw, unfiltered rustdoc Markdown, including authored examples
        label = "Request type" if kind == "request_type" else "Navigation method"
        rendered = f"### {label}: `{path}`\n\n" + docs + "\n\n"
        related = [it["id"] for it in bank["items"]
                   if any(it["pick"] == shown["pick"] for shown in bank["items"]
                          if shown["id"] in block["items"])]
        flags = example_candidates(docs, bank, related, navs)
        audit.append({**block, "visibility": item.get("visibility"), "docs_sha256": sha256(docs),
                      "docs_utf8_bytes": len(docs.encode("utf-8")), "rendered_sha256": sha256(rendered),
                      "example_request_candidates": flags, "has_example_request_candidate": bool(flags)})
        text += rendered
    suffix = APPENDIX + text if blocks else ""
    return suffix, {"format": "i3-generated-docs-exposure-v1", "session": session,
                    "appendix_sha256": sha256(suffix), "blocks": audit,
                    "example_scan": "conservative: HTTP targets, URLs, fences, endpoint chains; no truth used"}
