"""I3 stated-boundary controls using existing bank truth; no model readers or truth generation.

python3 -I tools/desirability/probes/boundary_selftest.py --out DIR --no-rustdoc-build
Add --generated-docs --docs-commit COMMIT to check composition against an exact arm's docs.
Both legacy and stated packs are written under DIR, never over the input bank.
"""
import argparse
import copy
import hashlib
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import common as C  # noqa: E402
import pack as P  # noqa: E402
import score as S  # noqa: E402


ANSWER_FIELDS = ("method", "url", "headers", "body", "content_type", "accept")


def sha(text):
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def scorer_controls(banks, bank_dir):
    truths, items, positives, sources = {}, {}, {}, {}
    for bank in banks:
        path = bank_dir / "truth" / (bank["short"] + ".json")
        truth = C.read_json(path)
        sources[bank["short"]] = {"path": str(path), "sha256": C.sha256_file(path),
                                  "repo_head": truth.get("repo_head")}
        for it in bank["items"]:
            if it["kind"] != "wire" or it["id"] not in truth["items"]:
                continue
            t = truth["items"][it["id"]]
            assert t["boundary"] == it["boundary"], it["id"]
            # Check the stored oracle, including the prepared dump retained for operation items.
            for view in (t, t["prepared"]) if "prepared" in t else (t,):
                for field, header in (("accept", "accept"), ("content_type", "content-type")):
                    first = next((v for k, v in view["headers"] if k == header), None)
                    assert view[field] == first, (it["id"], field, view)
            canaries = bank["picks"][it["pick"]]["canaries"]
            answer = {f: copy.deepcopy(t[f]) for f in ANSWER_FIELDS}
            scores = S.score_wire(t, answer, canaries)
            assert scores["exact"], (it["id"], scores)
            positives[it["id"]] = {"answer": answer, "scores": scores}
            truths[it["id"]], items[it["id"]] = t, canaries

    # Deliberately exercise missing/default Accept, SDK-set Accept, and bodies at both boundaries.
    assert truths["openai/w1"]["accept"] is None
    assert truths["fred/w4"]["accept"] is not None
    assert truths["kubernetes/w1"]["accept"] is not None
    for iid in ("datadog-events/w2", "google-drive/w2"):
        assert truths[iid]["content_type"] and truths[iid]["body"] is not None
    cases = []

    def bad(iid, name, mutate, expected):
        answer = copy.deepcopy(positives[iid]["answer"])
        mutate(answer)
        scores = S.score_wire(truths[iid], answer, items[iid])
        failed = sorted(f for f in S.R1_FIELDS if scores[f] is False)
        assert failed == sorted(expected), (iid, name, failed, expected)
        assert scores["critical"] and not scores["exact"], (iid, name, scores)
        cases.append({"item": iid, "case": name, "answer": answer,
                      "expected_wrong_fields": sorted(expected), "wrong_fields": failed, "scores": scores})

    bad("openai/w1", "transport-default Accept in scalar field",
        lambda a: a.update(accept="*/*"), ["accept"])
    bad("openai/w1", "transport-default Accept in headers only",
        lambda a: a["headers"].append(["accept", "*/*"]), ["header_names"])
    bad("openai/w1", "transport-default Accept in headers and scalar field",
        lambda a: (a["headers"].append(["accept", "*/*"]), a.update(accept="*/*")),
        ["header_names", "accept"])
    for iid in ("fred/w4", "kubernetes/w1"):
        bad(iid, "omitted SDK-set Accept scalar field", lambda a: a.pop("accept"), ["accept"])
    for iid in ("datadog-events/w2", "google-drive/w2"):
        bad(iid, "wrong Content-Type scalar field",
            lambda a: a.update(content_type="application/x-i3-selftest"), ["content_type"])
    bad("google-drive/w2", "omitted SDK-set Content-Type scalar field",
        lambda a: a.pop("content_type"), ["content_type"])
    return {"truth_sources": sources, "good": positives, "bad": cases,
            "all_available_truths_share_header_boundary": True}


def pack_pair(a, banks, out):
    outputs, logs = {}, {}
    for mode in ("legacy", "stated"):
        dest = out / mode / "bank"
        dest.mkdir(parents=True, exist_ok=True)
        for bank in banks:
            name = bank["short"] + ".json"
            shutil.copyfile(Path(a.bank) / name, dest / name)
        shutil.copytree(Path(a.bank) / "truth", dest / "truth", dirs_exist_ok=True)
        args = [sys.executable, "-I", str(Path(C.HERE) / "pack.py"),
                "--bank", str(dest), "--repo", a.repo]
        if a.no_rustdoc_build:
            args.append("--no-rustdoc-build")
        if a.generated_docs:
            args.append("--generated-docs")
        if a.docs_commit:
            args += ["--docs-commit", a.docs_commit]
        if mode == "stated":
            args.append("--stated-boundary")
        run = subprocess.run(args, capture_output=True, text=True)
        log = {"command": args, "exit": run.returncode, "stdout": run.stdout, "stderr": run.stderr}
        C.write_json(str(out / (mode + "-pack.json")), log)
        assert run.returncode == 0, log
        outputs[mode], logs[mode] = dest / "packed", log
    return outputs, logs


def question_sections(text):
    section = text.split("## 4. Questions\n\n", 1)[1].split("\n## 5. Generated endpoint documentation", 1)[0]
    pieces = re.split(r"(?m)^### Q(\d+)\n\n", section)
    return pieces[0].strip(), {q: body.strip() for q, body in zip(pieces[1::2], pieces[2::2])}


def prompt_controls(banks, outputs, out, generated_docs):
    checked, preambles, before_after = {}, {}, []
    for bank in banks:
        short = bank["short"]
        metas = {mode: C.read_json(path / (short + ".meta.json")) for mode, path in outputs.items()}
        assert metas["legacy"]["stated_boundary"] is False
        assert metas["stated"]["stated_boundary"] is True
        for session in ("wire", "rc"):
            texts = {mode: (path / f"{short}.{session}.md").read_text(encoding="utf-8")
                     for mode, path in outputs.items()}
            old, new = texts["legacy"], texts["stated"]
            for mode, text in texts.items():
                assert metas[mode]["sessions"][session]["sha256"] == sha(text)
            if session == "rc":
                assert old == new, short + ": rc changed"
            else:
                old_keys, old_qs = question_sections(old)
                new_keys, new_qs = question_sections(new)
                assert set(old_qs) == set(new_qs)
                assert new_keys == (P.WIRE_KEYS + " Use the observation boundary stated in each question. "
                                    + P.WIRE_HEADER_BOUNDARY
                                    + " Every answer object carries its `q` number and a `confidence`.")
                expected = old.replace(old_keys, new_keys, 1)
                hashes = metas["stated"]["sessions"][session]
                assert hashes["questions_text_sha256"] == sha(P.questions_text(bank, session, True))
                for it in bank["items"]:
                    if it["kind"] != "wire":
                        continue
                    q = str(it["q"])
                    left, right = old_qs[q], new_qs[q]
                    assert P.WIRE_HEADER_BOUNDARY in right, it["id"]
                    _, left_snippet = left.split("\n\n```rust\n", 1)
                    right_ask, right_snippet = right.split("\n\n```rust\n", 1)
                    assert left_snippet == right_snippet, it["id"] + ": snippet changed"
                    assert "request the server receives" not in right_ask
                    assert "reqwest" not in right_ask and "*/*" not in right_ask
                    if it["boundary"] == "operation":
                        assert "`call.send().await` send, including send-policy edits?" in right_ask
                    else:
                        assert "`call.prepare()` build, before any send-policy edits?" in right_ask
                    prior = preambles.setdefault(it["boundary"], right_ask)
                    assert prior == right_ask, it["id"] + ": SDK-specific preamble"
                    assert hashes["question_text_sha256"][q] == sha(right), it["id"]
                    expected = expected.replace(f"### Q{q}\n\n{left}", f"### Q{q}\n\n{right}", 1)
                    if it["id"] in ("openai/w1", "kubernetes/w1"):
                        before_after.append(f"## {it['id']}: before\n\n{left}\n\n## {it['id']}: after\n\n{right}\n")
                assert expected == new, short + ": prompt changed outside wire questions/answer keys"
            if generated_docs:
                appendices = [t.split("\n## 5. Generated endpoint documentation", 1)[1] for t in (old, new)]
                assert appendices[0] == appendices[1], short + ": generated-doc appendix changed"
                for mode, text in texts.items():
                    audit = metas[mode]["generated_docs_exposure"][session]
                    assert audit["prompt_sha256"] == sha(text)
                    base = text.split("\n## 5. Generated endpoint documentation", 1)[0]
                    assert audit["base_prompt_sha256"] == sha(base)
            checked[f"{short}.{session}"] = {"legacy_sha256": sha(old), "stated_sha256": sha(new),
                                             "only_intended_changes": True}
    (out / "before-after-questions.md").write_text("\n".join(before_after), encoding="utf-8")
    return {"sessions": checked, "identical_preamble_per_boundary": preambles,
            "wire_items_checked": sum(it["kind"] == "wire" for b in banks for it in b["items"]),
            "all_question_hashes_verified": True, "generated_docs_composition": generated_docs}


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--bank", default=C.MAIN_BANK)
    ap.add_argument("--repo", default=C.REPO)
    ap.add_argument("--out", required=True)
    ap.add_argument("--no-rustdoc-build", action="store_true")
    ap.add_argument("--generated-docs", action="store_true")
    ap.add_argument("--docs-commit")
    a = ap.parse_args()
    if a.docs_commit and not a.generated_docs:
        ap.error("--docs-commit requires --generated-docs")
    out, bank_dir = Path(a.out).resolve(), Path(a.bank).resolve()
    out.mkdir(parents=True, exist_ok=True)
    banks = [C.read_json(p) for p in sorted(bank_dir.glob("*.json")) if p.name != "index.json"]
    assert banks, "no banks"
    # Preserve input bank and truth files; packing and scoring only read them.
    source_files = list(bank_dir.glob("*.json")) + list((bank_dir / "truth").glob("*.json"))
    before = {str(p): C.sha256_file(p) for p in source_files}
    controls = scorer_controls(banks, bank_dir)
    outputs, logs = pack_pair(a, banks, out)
    prompts = prompt_controls(banks, outputs, out, a.generated_docs)
    assert before == {str(p): C.sha256_file(p) for p in source_files}, "input bank/truth changed"
    result = {"ok": True, "scorer": controls, "prompts": prompts, "pack_commands": logs,
              "input_bank_and_truth_unchanged": True}
    C.write_json(str(out / "selftest.json"), result)
    print(f"GOOD -> correct: {len(controls['good'])} existing wire truths (both boundaries)")
    for case in controls["bad"]:
        print(f"BAD -> wrong: {case['item']}: {case['case']}: {', '.join(case['wrong_fields'])} only")
    print(f"PROMPTS: {len(banks)} SDKs, {prompts['wire_items_checked']} wire items; "
          "boundary wording and exact question hashes verified; only wire questions/answer keys changed; rc unchanged")
    print(f"GENERATED DOCS: {'composition verified; appendices unchanged' if a.generated_docs else 'not requested'}")
    print(f"PASS; details: {out / 'selftest.json'}")


if __name__ == "__main__":
    main()
