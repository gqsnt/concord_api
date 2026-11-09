"""I3 generated-docs controls; no model readers, truth generation, or commits.

python3 -I tools/desirability/probes/generated_docs_selftest.py --out DIR
Builds existing commits a8a548c4^ and a8a548c4, whose only difference is one doc comment.
The tiny Anthropic bank is an exposure fixture, not a new scored bank.
"""
import argparse
import contextlib
import difflib
import io
import json
import os
import re
import sys
from unittest.mock import patch

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import common as C  # noqa: E402
import generated_docs as D  # noqa: E402
import pack as P  # noqa: E402


def unit_controls(out):
    def refuses(fn):
        try:
            fn()
        except SystemExit:
            return True
        return False
    controls = os.path.join(out, "controls")
    os.makedirs(controls, exist_ok=True)
    target = D.A.target_dir("1" * 40, D.USE, controls)
    assert refuses(lambda: D.A.check_target(target, "2" * 40, D.USE))
    unknown = os.path.join(controls, "pilot", "3" * 12 + "-" + D.USE)
    os.makedirs(unknown, exist_ok=True)
    C.write_json(os.path.join(unknown, "unknown.json"), {})
    assert refuses(lambda: D.A.target_dir("3" * 40, D.USE, controls))
    assert refuses(lambda: D.check_json(controls, [{"crate": "missing"}], "i3-tree-wanted"))
    C.write_json(os.path.join(controls, "stale.json"), {"crate_version": "i3-tree-other"})
    assert refuses(lambda: D.check_json(controls, [{"crate": "stale"}], "i3-tree-wanted"))
    # Raw docs must survive, including examples that legacy packing removes. The navigation
    # return ID, not a generated naming convention, selects the request type.
    docs = 'Example: `GET /demo?x=1`.\n\n```rust\napi.demo().prepare()?;\n```\n'
    def item(name, inner, text='Navigation docs.'):
        return {"name": name, "inner": inner, "docs": text, "visibility": "public", "crate_id": 0}
    dj = {"root": 0, "paths": {}, "index": {
        "0": item("fixture", {"module": {"items": [1, 2]}}),
        "1": item("Client", {"struct": {"impls": [3]}}),
        "2": item("UnexpectedRequestName", {"struct": {"impls": []}}, docs),
        "3": item(None, {"impl": {"trait": None, "items": [4]}}),
        "4": item("demo", {"function": {"sig": {"output": {"resolved_path": {"id": 2}}}}}),
    }}
    class SDK:
        crate = "fixture"
        clients = []
        def decl_module(self, path):
            return ()
    pick = {"client": "Client", "client_file": "unused", "scopes": [], "endpoint": "demo", "key": "Client/demo"}
    bank = {"picks": [pick], "items": [{"id": "fixture/w1", "pick": 0, "session": "wire"},
                                         {"id": "fixture/c1", "pick": 0, "session": "rc"}]}
    text, audit = D.appendix(dj, SDK(), bank, "wire")
    assert docs in text
    assert len(audit["blocks"]) == 2
    req = next(b for b in audit["blocks"] if b["kind"] == "request_type")
    assert req["docs_sha256"] == D.sha256(docs)
    assert req["has_example_request_candidate"]
    assert {f["rule"] for f in req["example_request_candidates"]} >= {"http_request", "fenced_example"}
    assert "fixture/c1" in req["example_request_candidates"][0]["bank_items"]
    assert "fn demo" not in text and "fixture/w1" not in text
    dj["index"]["4"]["docs"] = None
    try:
        D.appendix(dj, SDK(), bank, "wire")
    except SystemExit:
        pass
    else:
        raise AssertionError("missing endpoint docs must fail closed")
    return {"raw_docs_and_examples_preserved": True, "examples_flagged_across_bank": True,
            "return_id_resolution": True, "missing_docs_refused": True,
            "wrong_target_provenance_refused": True, "unknown_target_provenance_refused": True,
            "wrong_json_provenance_refused": True, "missing_json_refused": True}


def fixture():
    return {"sdk": "anthropic-messages-sdk", "short": "anthropic-messages",
            "pkg": "concord-sdk-anthropic-messages", "crate": "concord_sdk_anthropic_messages",
            "clients": {"Anthropic": {"ctor": 'Anthropic::new(reqwest::Client::new(), AnthropicConfig { api_key: "fixture".into() })'}},
            "picks": [{"key": "Anthropic/messages/stream", "client": "Anthropic",
                       "client_file": "examples/anthropic-messages-sdk/src/lib.rs", "scopes": ["messages"],
                       "endpoint": "stream", "values": []}],
            "items": [{"id": "anthropic-messages/w1", "pick": 0, "kind": "wire", "session": "wire", "q": 1,
                       "question": "What request does the Messages stream endpoint send?"},
                      {"id": "anthropic-messages/r1", "pick": 0, "kind": "result", "session": "rc", "q": 1,
                       "question": "What does the Messages stream endpoint return?"}]}


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--out", required=True)
    ap.add_argument("--repo", default=C.REPO)
    ap.add_argument("--before", default="a8a548c4^")
    ap.add_argument("--after", default="a8a548c4")
    a = ap.parse_args()
    os.makedirs(a.out, exist_ok=True)
    result = {"unit_controls": unit_controls(a.out), "arms": {}, "sessions": {}}
    file = "examples/anthropic-messages-sdk/src/lib.rs"
    commits = {arm: D.A.full_sha(commit, a.repo) for arm, commit in (("before", a.before), ("after", a.after))}
    changed = D.A.git(a.repo, "diff", "--name-only", commits["before"], commits["after"]).splitlines()
    assert changed == [file], changed
    old, new = [D.A.git(a.repo, "show", f"{commits[arm]}:{file}") for arm in ("before", "after")]
    comment = r"(?m)((?:[ \t]*///[^\n]*\n)+)[ \t]*POST stream\("
    old_comment, new_comment = [re.search(comment, src).group(1) for src in (old, new)]
    assert old_comment != new_comment
    assert old.replace(old_comment, new_comment, 1) == new, "commits must differ only in this doc comment"
    old_doc, new_doc = [re.sub(r"(?m)^[ \t]*/// ?", "", s).rstrip("\n") for s in (old_comment, new_comment)]
    for arm, sha in commits.items():
        bank_dir = os.path.join(a.out, arm)
        os.makedirs(bank_dir, exist_ok=True)
        C.write_json(os.path.join(bank_dir, "anthropic-messages.json"), fixture())
        args = ["pack.py", "--repo", a.repo, "--bank", bank_dir, "--generated-docs", "--docs-commit", sha]
        with patch.object(sys, "argv", args), contextlib.redirect_stdout(io.StringIO()) as stdout:
            P.main()
        result["arms"][arm] = {"commit": sha, "output": stdout.getvalue()}
    for session in ("wire", "rc"):
        prompts = [open(os.path.join(a.out, arm, "packed", f"anthropic-messages.{session}.md"), encoding="utf-8").read()
                   for arm in ("before", "after")]
        before, after = prompts
        expected = before.replace(old_comment.rstrip("\n"), new_comment.rstrip("\n"))
        replacements = {}
        for indent in (12, 4, 0):
            left, right = [P.indent(s, indent) for s in (old_doc, new_doc)]
            replacements[str(indent)] = expected.count(left)
            expected = expected.replace(left, right)
        assert before != after, f"{session}: doc change was not exposed"
        assert expected == after, f"{session}: prompt changed outside the doc comment"
        old_suffix, new_suffix = [s.split(D.APPENDIX, 1)[1] for s in prompts]
        assert old_suffix.count(old_doc) == 2, "both endpoint doc blocks must expose the changed comment"
        assert old_suffix.replace(old_doc, new_doc) == new_suffix, "appendix changed outside raw docs"
        diff = "".join(difflib.unified_diff(before.splitlines(True), after.splitlines(True),
                                             fromfile=f"before/{session}", tofile=f"after/{session}"))
        with open(os.path.join(a.out, f"{session}.diff"), "w", encoding="utf-8") as f:
            f.write(diff)
        result["sessions"][session] = {"differ_exactly_in_doc_comment": True,
                                        "both_appended_blocks_change_only_in_doc_comment": True,
                                        "before_sha256": D.sha256(before), "after_sha256": D.sha256(after),
                                        "doc_replacements_by_indent": replacements}
    # Build A again after B: targets and docs must remain arm-specific.
    args = ["pack.py", "--repo", a.repo, "--bank", os.path.join(a.out, "before"),
            "--generated-docs", "--docs-commit", commits["before"], "--no-rustdoc-build"]
    with patch.object(sys, "argv", args), contextlib.redirect_stdout(io.StringIO()):
        P.main()
    for session in ("wire", "rc"):
        path = os.path.join(a.out, "before", "packed", f"anthropic-messages.{session}.md")
        assert C.sha256_file(path) == result["sessions"][session]["before_sha256"]
    result.update(only_source_difference_is_one_doc_comment=True, a_after_b_identical=True, ok=True)
    C.write_json(os.path.join(a.out, "selftest.json"), result)
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
