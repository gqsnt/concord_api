"""`pilot.py selftest`: the instrument controls (PLAN_AMENDMENT_A1 §6) and the harness's own checks.

Without cargo (always): the control set validates and hashes to its frozen line; malformed task
files are refused with the expected problem; hashing, freezing and the refusal of a changed set;
provenance markers (a target or tree of another commit, a target directory of unknown content);
sealed locations; the run-file binding; the tally (pass rates, regressions, corrections, unpaired,
irreproducible) and the threshold evaluator on synthetic rows; the public summary leaks no task
content; the transcript audit and the candidate-diff check on synthetic transcripts and real
commits.

With cargo (unless `--no-cargo`): every compile, request and stage control of `controls/` runs on
the control commit, twice, through the same code as `run`; each must end as its `control.expect`
says (and fail for the stated reason); both repetitions must agree (verdict and fingerprint); the
reader control's reference, seeded wrong answers, equivalent spellings and a malformed session are
scored through the H04 scorer; reader packing substitutes in one pass, freezes, and the scorer
refuses a session recorded on another prompt and a re-pack after a session.
"""
import json
import os
import shutil
import time

import arms as A
import clearance as CL
import seal
import taskfile as TF
import tally

HERE = os.path.dirname(os.path.abspath(__file__))
CONTROLS = os.path.join(HERE, "controls")


class Checks:
    def __init__(self):
        self.rows = []

    def __call__(self, name, ok, detail=None):
        self.rows.append({"check": name, "ok": bool(ok), "detail": detail})
        print(f"{'ok  ' if ok else 'FAIL'} {name}" + (f"  ({detail})" if detail and not ok else ""), flush=True)


def refused(fn):
    try:
        fn()
    except A.Refused as e:
        return str(e)
    return None


# ---------------------------------------------------------------- without cargo

BAD_TASKS = [
    ({"format": 1, "id": "x", "kind": "compile", "class": "c", "sdk": "s", "code": "", "expect": {"compiles": True}, "extra": 1},
     "unknown key `extra`"),
    ({"format": 1, "id": "x", "kind": "compile", "class": "c", "sdk": "s", "code": ""}, "missing `expect`"),
    ({"format": 1, "id": "x", "kind": "magic", "class": "c", "sdk": "s"}, "`kind` must be one of"),
    ({"format": 1, "id": "x", "kind": "compile", "class": "c", "sdk": "s", "code": "", "expect": {"compiles": True, "first": {"code": "E1"}}},
     "only when compiles is false"),
    ({"format": 1, "id": "x", "kind": "request", "class": "c", "sdk": "s", "call": "x", "expect": {"requests": []},
      "replies": [{"status": 200, "body_file": "../../etc/passwd"}]}, "relative path inside the task set"),
    ({"format": 1, "id": "x", "kind": "request", "class": "c", "sdk": "s", "call": "x", "replies": [{"status": 200}],
      "expect": {"requests": [{"method": "GET", "target": "/"}]}}, "a request has exactly"),
    ({"format": 1, "id": "x", "kind": "stage", "class": "c", "sdk": "s", "oracle": "missing.rs", "patches": {},
      "expect": {"stages": ["sometime"]}}, "is not a regular file"),
    ({"format": 1, "id": "x", "kind": "reader", "class": "c", "sdk": "s", "number": 1, "key": {"checks": [], "facts": []},
      "reference": {}}, "exactly one of `text` and `text_file`"),
    ({"format": 2, "id": "bad id!", "kind": "compile", "class": "c", "sdk": "s", "code": "", "expect": {"compiles": True}},
     "`format` must be 1"),
]


def synthetic_rows():
    """Baseline B and candidate C; deterministic tasks with two repetitions, reader units."""
    rows = []

    def add(task, arm, verdict, rep=1, slot="", fp=None, sdk="s1", cls="k1", kind="compile", role="witness"):
        rows.append({"task": task, "kind": kind, "sdk": sdk, "class": cls, "role": role, "group": sdk, "arm": arm, "slot": slot,
                     "rep": rep, "verdict": verdict, "fingerprint": fp, "reason": "SECRET_SNIPPET reason"})

    for rep in (1, 2):
        add("t-corr", "B", "fail", rep, fp="SECRET_SNIPPET a")
        add("t-corr", "C", "pass", rep)
        add("t-reg", "B", "pass", rep, sdk="s2", cls="k2")
        add("t-reg", "C", "fail", rep, sdk="s2", cls="k2")
        add("t-same", "B", "pass", rep, sdk="s2")
        add("t-same", "C", "pass", rep, sdk="s2")
        add("t-inv", "B", "pass", rep, role="control")
        add("t-inv", "C", "invalid", rep, role="control")
    add("t-flaky", "B", "pass", 1)
    add("t-flaky", "B", "pass", 2)
    add("t-flaky", "C", "pass", 1)
    add("t-flaky", "C", "fail", 2)
    for prov in ("claude", "codex"):
        add("r1", "B", "fail", slot=f"C:{prov}/r1", kind="reader", sdk="s3", cls="t1")
        add("r1", "C", "pass", slot=f"C:{prov}/r1", kind="reader", sdk="s3", cls="t1")
    return rows


def thresholds(extra=()):
    return {"format": 1, "card": "synthetic", "precedence": ["LOSS", "INCONCLUSIVE", "KEEP_CURRENT"], "otherwise": "INCONCLUSIVE",
            "rules": [{"id": "W1", "outcome": "WIN", "type": "require", "metric": "corrections", "filter": {"role": "witness"},
                       "op": ">=", "value": 1},
                      {"id": "W2", "outcome": "WIN", "type": "require", "metric": "fact", "name": "gates", "op": "==", "value": "pass"}]
            + list(extra)}


def py_checks(ck, out):
    s = TF.load_set(CONTROLS)
    ck("controls validate", not s["problems"], s["problems"][:3])
    h = seal.set_hash(CONTROLS)[0]
    ck("controls hash equals the frozen line pilot-controls", seal.ledger().get("pilot-controls") == h,
       f"{h} vs {seal.ledger().get('pilot-controls')}")
    for t, want in BAD_TASKS:
        errs = TF.validate_task(t, s["tasks_dir"], os.path.join(s["tasks_dir"], "x.task.json"))
        ck(f"malformed task refused: {want}", any(want in e for e in errs), errs[:3])
    # hashing, freezing, refusal of a changed set
    scratch_set = os.path.join(out, "scratch-set")
    shutil.copytree(CONTROLS, scratch_set)
    ck("hash is a function of the listing (a copy hashes alike)", seal.set_hash(scratch_set)[0] == h)
    led = os.path.join(out, "LEDGER.sha256")
    h1, new1 = seal.freeze(scratch_set, "scratch", led)
    h2, new2 = seal.freeze(scratch_set, "scratch", led)
    ck("freeze writes once, then is a no-op", new1 and not new2 and h1 == h2 == h)
    ck("an unfrozen name is refused", refused(lambda: seal.check_frozen(scratch_set, "other", led)) is not None)
    p = os.path.join(scratch_set, "tasks", "request", "empty.json")
    with open(p, "a") as f:
        f.write(" ")
    ck("a changed set is refused by run (hash differs from the frozen one)",
       "refusing to run" in (refused(lambda: seal.check_frozen(scratch_set, "scratch", led)) or ""))
    ck("a changed set cannot be refrozen under the same name",
       "needs a new name" in (refused(lambda: seal.freeze(scratch_set, "scratch", led)) or ""))
    # provenance markers
    root = os.path.join(out, "targets")
    sha1, sha2 = "1" * 40, "2" * 40
    d1 = A.target_dir(sha1, "clean", root)
    ck("a target directory is created with its marker", os.path.isfile(os.path.join(d1, A.TARGET_MARKER)))
    ck("a target marked for another commit is refused", refused(lambda: A.check_target(d1, sha2)) is not None)
    ck("a target marked for another use is refused", refused(lambda: A.check_target(d1, sha1, "stage")) is not None)
    junk = os.path.join(root, "pilot", "3" * 12)
    os.makedirs(junk)
    open(os.path.join(junk, "x"), "w").close()
    ck("a target directory with files but no marker is refused", refused(lambda: A.target_dir("3" * 40, "clean", root)) is not None)
    tree = os.path.join(out, "fake-tree")
    os.makedirs(tree)
    with open(os.path.join(tree, A.TREE_MARKER), "w") as f:
        f.write(json.dumps({"commit": sha1}))
    ck("a tree of another commit is refused", refused(lambda: A.check_tree(tree, sha2)) is not None)
    # sealed locations
    ck("a run directory inside the repository is refused", refused(lambda: A.sealed_dir(os.path.join(A.REPO, "x"), "run")) is not None)
    ck("a run directory outside work/sealed is refused",
       refused(lambda: A.sealed_dir(os.path.join(A.WS, "work", "dev", "x"), "run")) is not None)
    ck("a run directory under work/sealed is accepted", refused(lambda: A.sealed_dir(os.path.join(out, "run-x"), "run")) is None)
    # run-file binding
    rfp = os.path.join(out, "run.json")
    body = {"format": 1, "name": "x", "set": scratch_set, "set_name": "scratch", "out": os.path.join(out, "run-x"),
            "arms": [{"name": "b", "commit": sha1}, {"name": "c", "commit": sha2}]}
    with open(rfp, "w") as f:
        json.dump(body, f)
    rf = seal.load_runfile(rfp)
    seal.bind_run(rf)
    with open(rfp, "w") as f:
        json.dump({**body, "reps": 3}, f)
    ck("a run directory refuses another run file", refused(lambda: seal.bind_run(seal.load_runfile(rfp))) is not None)
    with open(rfp, "w") as f:
        json.dump({**body, "arms": []}, f)
    ck("a run file without arms is refused", refused(lambda: seal.load_runfile(rfp)) is not None)
    # tally
    rows = synthetic_rows()
    summ, U = tally.summary(rows, "B")
    c = summ["candidates"]["C"]
    ck("corrections counted per unit (deterministic and reader)",
       sorted(x["unit"] for x in c["corrections"]) == ["r1@C:claude/r1", "r1@C:codex/r1", "t-corr"], c["corrections"])
    ck("regressions: pass at baseline, fail on the arm", [x["unit"] for x in c["regressions"]] == ["t-reg"], c["regressions"])
    ck("invalid units are unpaired, not regressions", c["unpaired"] == ["t-inv"] and c["invalid"] == ["t-inv"], c)
    ck("a verdict that differs between repetitions is irreproducible", c["irreproducible"] == ["t-flaky"], c["irreproducible"])
    ck("pass rates per arm x SDK x class", summ["tables"]["C"]["s2|k2"] == {"pass": 0, "n": 1, "invalid": 0}
       and summ["tables"]["B"]["s1|k1"] == {"pass": 2, "n": 3, "invalid": 0}, summ["tables"]["C"])
    # thresholds
    th = thresholds()
    ck("thresholds file validates", not tally.check_thresholds(th), tally.check_thresholds(th))
    v = lambda t, facts: tally.decide(t, U["C"], U["B"], facts)[0]  # noqa: E731
    ck("WIN when every requirement holds", v(th, {"gates": "pass"}) == "WIN")
    ck("WIN_PENDING while a fact is unknown", v(th, {}) == "WIN_PENDING")
    ck("otherwise when a requirement fails", v(th, {"gates": "fail"}) == "INCONCLUSIVE")
    loss = thresholds([{"id": "L1", "outcome": "LOSS", "type": "trigger", "metric": "regressions", "op": ">", "value": 0}])
    ck("a LOSS trigger fires (one regression)", v(loss, {"gates": "pass"}) == "LOSS")
    allowed = thresholds([{"id": "L1", "outcome": "LOSS", "type": "trigger", "metric": "regressions",
                           "filter": {"task": {"not": ["t-reg"]}}, "op": ">", "value": 0}])
    ck("an expected break listed in the filter is not a kill", v(allowed, {"gates": "pass"}) == "WIN")
    keep = thresholds([{"id": "K1", "outcome": "KEEP_CURRENT", "type": "trigger", "metric": "pass_rate_gain",
                        "filter": {"role": "witness"}, "op": "<", "value": 0.5}])
    ck("KEEP_CURRENT when the gain is below the bound", v(keep, {"gates": "pass"}) == "KEEP_CURRENT")
    one = thresholds([{"id": "I1", "outcome": "INCONCLUSIVE", "type": "trigger", "metric": "corrections", "per": "sdk",
                       "quantifier": ["count", "==", 1], "filter": {"kind": "compile"}, "op": ">", "value": 0}])
    ck("per-group count quantifier (gains in exactly one SDK)", v(one, {"gates": "pass"}) == "INCONCLUSIVE")
    pend = thresholds([{"id": "G", "outcome": "LOSS", "type": "trigger", "metric": "fact", "name": "ci", "op": "==", "value": "fail"}])
    ck("an unknown higher-precedence trigger makes the verdict pending", v(pend, {"gates": "pass"}) == "WIN_PENDING")
    bad = thresholds([{"id": "X", "outcome": "WIN", "type": "trigger", "metric": "nope", "op": "~", "value": 1}])
    ck("a malformed rule is refused", len(tally.check_thresholds(bad)) >= 2, tally.check_thresholds(bad))
    pub = tally.public_summary(summ, {"C": tally.decide(loss, U["C"], U["B"], {"gates": "pass"})}, {"name": "x", "hash": "h"})
    text = json.dumps(pub) + tally.public_md(pub)
    ck("the public summary holds no task content (reasons, fingerprints)", "SECRET_SNIPPET" not in text)
    first_diagnostic_checks(ck)
    # clearance
    transcript_checks(ck, out)
    diff_checks(ck, out)


RENDERED = """error[E0277]: `{integer}` is not accepted for a value of type `String`
  --> examples/x-sdk/tests/__pilot_c0001.rs:5:3
   |
5  |     api.labels().set_per_page(50)
   |                  ------------ ^^ expected `GitHubRepoLabels`
   |
   = help: the trait `IntoValue<u8>` is not implemented for `{integer}`
help: the following other types implement trait `IntoValue<T>`
  --> crates/concord-core/src/value.rs:10:1
   |
10 | impl IntoValue<String> for &str {
"""


def first_diagnostic_checks(ck):
    import kinds as K
    msg = {"rendered": RENDERED, "message": "x", "code": {"code": "E0277"},
           "spans": [{"is_primary": True, "file_name": "examples/x-sdk/tests/__pilot_c0001.rs"}]}
    res = {"compiles": False, "message": msg, "headline": "E0277"}
    rel = "examples/x-sdk/tests/__pilot_c0001.rs"
    v = lambda first: K.compile_verdict({"expect": {"compiles": False, "first": first}}, res, rel)[0]  # noqa: E731
    ck("first diagnostic: an internal name in a help note fails `absent` (whole diagnostic)", v({"absent": ["IntoValue"]}) == "fail")
    ck("first diagnostic: the same passes in scope primary (I5 R5: headline and primary labels)",
       v({"absent": ["IntoValue"], "scope": "primary"}) == "pass")
    ck("first diagnostic: identifier-like names match whole words only",
       v({"absent": ["GitHubRepo"], "scope": "primary"}) == "pass" and v({"absent": ["GitHubRepoLabels"], "scope": "primary"}) == "fail")
    ck("first diagnostic: code, mentions and primary span checks",
       v({"code": "E0277", "mentions": ["GitHubRepoLabels"], "in_snippet": True}) == "pass" and v({"code": ["E0308"]}) == "fail"
       and v({"mentions": ["u8"], "scope": "primary"}) == "fail" and v({"mentions": ["u8"]}) == "pass")
    ck("first diagnostic: lines quoting another file count for `absent`, the snippet's own quoted lines do not",
       v({"absent": ["set_per_page"]}) == "pass" and v({"absent": ["for &str"]}) == "fail")


def transcript_checks(ck, out):
    roles = os.path.join(A.WS, "work", "roles", "templates", "x.md")
    lines = [
        {"type": "user", "message": {"role": "user", "content": "Brief: never read work/sealed or tools/desirability."}},
        {"type": "assistant", "message": {"role": "assistant", "content": [
            {"type": "tool_use", "id": "u1", "name": "Read", "input": {"file_path": roles}},
            {"type": "tool_use", "id": "u2", "name": "Bash", "input": {"command": "grep -rn x . | grep -v '^./tools/desirability'",
                                                                       "description": "search, excluding tools/desirability"}},
            {"type": "tool_use", "id": "u3", "name": "Bash", "input": {"command": "cat /h/work/sealed/h04-bank/tasks/a.json"}},
            {"type": "tool_use", "id": "u4", "name": "Bash", "input": {"command": "ls -R ."}},
            {"type": "tool_use", "id": "u5", "name": "Write", "input": {"file_path": "/h/notes.md", "content": "see work/sealed"}},
            {"type": "tool_use", "id": "u6", "name": "SubagentHandback", "input": {"message": "nothing under work/sealed read"}}]}},
        {"type": "user", "message": {"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": "u1", "content": "Rule: tools/desirability/ is hidden from you."},
            {"type": "tool_result", "tool_use_id": "u2", "content": "src/a.rs:1:x"},
            {"type": "tool_result", "tool_use_id": "u3", "content": "{\"key\": 1} from work/sealed/h04-bank"},
            {"type": "tool_result", "tool_use_id": "u4", "content": "./tools/desirability/h04/scorer.py\n./src/a.rs"}]}},
    ]
    p = os.path.join(out, "synthetic-claude.jsonl")
    with open(p, "w") as f:
        f.write("\n".join(json.dumps(x) for x in lines) + "\n")
    import re
    forbid = re.compile(CL.DEFAULT_FORBID)
    r = CL.audit(p, forbid)
    got = {(h["event"], h["class"]) for h in r["hits"]}
    classes = {h["class"] for h in r["hits"]}
    want = {"brief-text", "exclusion", "report-text", "access", "access-result", "access-evidence", "content-mention"}
    ck("transcript audit: brief text, exclusion, report text, access, its result, evidence, content mention",
       want <= classes and r["status"] == "refused", sorted(classes))
    ck("transcript audit: the brief file's text is brief text, not access",
       not any(h["class"].startswith("access") and "hidden from you" in h["quote"] for h in r["hits"]), sorted(got))
    codex = [{"type": "response_item", "payload": {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "never read work/sealed"}]}},
             {"type": "response_item", "payload": {"type": "function_call", "name": "exec_command", "call_id": "c1",
                                                   "arguments": "{\"cmd\": \"ls tools/desirability/h04\"}"}},
             {"type": "response_item", "payload": {"type": "function_call_output", "call_id": "c1", "output": "scorer.py"}}]
    p2 = os.path.join(out, "synthetic-codex.jsonl")
    with open(p2, "w") as f:
        f.write("\n".join(json.dumps(x) for x in codex) + "\n")
    r2 = CL.audit(p2, forbid)
    ck("transcript audit reads Codex rollouts (brief text; a shell command listing an instrument path is access)",
       {h["class"] for h in r2["hits"]} == {"brief-text", "access"} and r2["status"] == "refused", r2["counts"])
    clean = [lines[0], {"type": "assistant", "message": {"role": "assistant", "content": [
        {"type": "tool_use", "id": "u9", "name": "Bash", "input": {"command": "find . -name '*.rs' -not -path './tools/desirability/*'"}}]}},
        {"type": "user", "message": {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "u9", "content": "./src/a.rs"}]}}]
    p3 = os.path.join(out, "synthetic-clean.jsonl")
    with open(p3, "w") as f:
        f.write("\n".join(json.dumps(x) for x in clean) + "\n")
    ck("transcript audit: a lane that only excludes the forbidden paths is clear", CL.audit(p3, forbid)["status"] == "clear")


def diff_checks(ck, out):
    import re
    forbid = re.compile(CL.DEFAULT_FORBID)
    repo = A.REPO
    tools_commit = A.git(repo, "log", "-1", "--format=%H", "--", "tools/desirability")
    r = CL.diff_check(repo, tools_commit + "^", tools_commit, [], forbid)
    ck("candidate diff: a change under tools/desirability/ is blocking",
       r["status"] == "refused" and any(f["class"] == "instrument-path" for f in r["findings"]), r["status"])
    other = A.git(repo, "log", "-1", "--format=%H", "--", "examples/nager-holidays-sdk/src")
    added = [ln[1:] for ln in A.git(repo, "diff", "-U0", other + "^", other, "--", "examples/nager-holidays-sdk/src").splitlines()
             if ln.startswith("+") and not ln.startswith("+++") and len(CL.norm_line(ln[1:])) >= 30]
    sealed = os.path.join(out, "synthetic-sealed")
    os.makedirs(sealed)
    pick = next((x for x in added if CL.norm_line(x) not in CL.base_lines(repo, other + "^")), None)
    with open(os.path.join(sealed, "oracle.rs"), "w") as f:
        f.write((pick or "no line") + "\n")
    r2 = CL.diff_check(repo, other + "^", other, [sealed], forbid)
    ck("candidate diff: an added line found in a sealed file (and not in the base tree) is reported",
       pick is not None and any(f["class"] == "sealed-line" for f in r2["findings"]), {"commit": other[:12], "line": bool(pick)})
    r3 = CL.diff_check(repo, other + "^", other, [], forbid)
    ck("candidate diff: an SDK-only change is not blocking", r3["status"] != "refused", r3["findings"][:3])


# ---------------------------------------------------------------- with cargo

def cargo_checks(ck, out):
    import kinds as K  # noqa: F401  (sandbox prerequisites are checked on import of the run path)
    import pilot as P
    import readers as RD
    s = TF.load_set(CONTROLS)
    sha = A.full_sha(s["meta"]["commit"])
    rf = {"out": os.path.join(out, "run"), "set": CONTROLS, "set_name": "pilot-controls"}
    reps = {}
    for rep in (1, 2):
        log = []
        t0 = time.monotonic()
        reps[rep] = P.run_arm(rf, s, None, "controls", sha, rep, log)
        seal.write_json(os.path.join(out, "run", "results", f"controls.r{rep}.json"),
                        {"arm": "controls", "commit": sha, "rep": rep, "tasks": reps[rep], "log": log})
        ck(f"controls ran (repetition {rep}, {round(time.monotonic() - t0)} s)", bool(reps[rep]))
    for tid, t in sorted(s["tasks"].items()):
        if t["kind"] == "reader":
            continue
        r = reps[1].get(tid, {"verdict": "missing"})
        want = t["control"]["expect"]
        ok = r["verdict"] == want and (want == "pass" or t["control"].get("reason", "") in (r.get("reason") or ""))
        ck(f"control {tid} must {want}" + (f" ({t['control'].get('reason')})" if want == "fail" else ""), ok,
           f"{r['verdict']}: {(r.get('reason') or '')[:200]}")
    rows = [{**TF.public(s["tasks"][tid]), "task": tid, "arm": "controls", "slot": "", "rep": rep, "verdict": x["verdict"],
             "fingerprint": x.get("fingerprint")} for rep in reps for tid, x in reps[rep].items()]
    _, U = tally.summary(rows, "controls")
    flaky = sorted(k[0] for k, u in U["controls"].items() if u["irreproducible"])
    ck("every control reproduces (verdict and fingerprint equal in both repetitions)", not flaky, flaky)
    # reader controls through the H04 scorer
    t = s["tasks"]["ctl/reader/t1"]
    items = [("ref", t, t["reference"])]
    items += [(f"wrong_{f}", t, {**t["reference"], **p}) for f, p in sorted(t["wrong"].items())]
    items += [(f"equiv{i}", t, {**t["reference"], **p}) for i, p in enumerate(t["equivalent"])]
    items += [("malformed", t, None)]
    res = RD.score_items([t], items, os.path.join(out, "reader"), [])
    ck("reader control: the reference passes", res["ref"]["pass"], res["ref"]["checks"])
    for f in sorted(t["wrong"]):
        w = res[f"wrong_{f}"]
        ck(f"reader control: the seeded wrong answer for {f} fails its fact only",
           not w["pass"] and not w["facts"][f] and all(v for k, v in w["facts"].items() if k != f), w["facts"])
    for i in range(len(t["equivalent"])):
        ck(f"reader control: equivalent spelling {i} passes", res[f"equiv{i}"]["pass"], res[f"equiv{i}"]["checks"])
    ck("reader control: a malformed session scores 0", not res["malformed"]["pass"])
    pack_checks(ck, out, s, sha, RD)


def pack_checks(ck, out, s, sha, RD):
    runs_out = os.path.join(out, "readers-run")
    rf = {"out": runs_out, "arms": [{"name": "base", "commit": sha}, {"name": "cand", "commit": sha}], "baseline": "base"}
    mat = os.path.join(out, "materials", sha[:12], "nager-holidays")
    os.makedirs(mat)
    with open(os.path.join(mat, "DOCS.txt"), "w") as f:
        f.write("Docs mention the literal {TASKS} placeholder and {CRATE}.\n")
    RD.pack(rf, s, os.path.join(out, "materials"), seed=7, providers=["claude", "codex"], reps=1)
    runs = os.path.join(runs_out, "readers", "cand")
    man = json.load(open(os.path.join(runs, "prompts", "manifest.json")))
    prompt = open(os.path.join(runs, man["prompts"]["candidate/nager-holidays"]["path"])).read()
    ck("reader pack: one-pass substitution (placeholders inside materials stay literal)",
       "literal {TASKS} placeholder and {CRATE}" in prompt and prompt.count("## Task 1") == 1
       and "`concord_sdk_nager_holidays` crate" in prompt and "not sent" not in prompt, prompt[:300])
    sched = json.load(open(os.path.join(runs, "schedule.json")))
    t = s["tasks"]["ctl/reader/t1"]
    for x in sched["sessions"]:
        d = os.path.join(runs, "sessions", x["id"])
        seal.write_json(os.path.join(d, "answer.json"), {"answers": [{"task": 1, **t["reference"]}]})
        seal.write_json(os.path.join(d, "meta.json"), {**{k: x[k] for k in ("id", "arm", "commit", "provider", "sdk", "rep")},
                                                       "prompt_sha256": man["prompts"][f"{x['arm']}/{x['sdk']}"]["sha256"]})
    res = RD.score_sessions([t], runs, os.path.join(out, "reader-pack-score"))
    ck("reader score: frozen run with recorded sessions is scored (all references pass)",
       len(res["rows"]) == len(sched["sessions"]) and all(r["verdict"] == "pass" for r in res["rows"]))
    first = sched["sessions"][0]["id"]
    mp = os.path.join(runs, "sessions", first, "meta.json")
    meta = json.load(open(mp))
    seal.write_json(mp, {**meta, "prompt_sha256": "0" * 64})
    ck("reader score: a session recorded on another prompt is refused",
       refused(lambda: RD.score_sessions([t], runs, os.path.join(out, "reader-pack-score2"))) is not None)
    seal.write_json(mp, meta)
    ck("reader pack: packing again after a session is refused",
       refused(lambda: RD.pack(rf, s, os.path.join(out, "materials"), seed=7, providers=["claude", "codex"], reps=1)) is not None)


def main(a):
    stamp = time.strftime("%Y%m%dT%H%M%S")
    out = os.path.join(A.sealed_dir(a.out, "selftest directory"), stamp)
    os.makedirs(out)
    ck = Checks()
    t0 = time.monotonic()
    py_checks(ck, out)
    if not a.no_cargo:
        A.target_root()
        cargo_checks(ck, out)
    for dp, dn, _ in os.walk(out):  # this run's own materialised trees (about 37 MB each); results are kept
        for d in [x for x in dn if x == "trees"]:
            shutil.rmtree(os.path.join(dp, d))
            dn.remove(d)
    bad = [r for r in ck.rows if not r["ok"]]
    seal.write_json(os.path.join(out, "selftest.json"), {"ok": not bad, "checks": ck.rows, "cargo": not a.no_cargo,
                                                         "seconds": round(time.monotonic() - t0, 1)})
    print(json.dumps({"ok": not bad, "checks": len(ck.rows), "failed": [r["check"] for r in bad],
                      "seconds": round(time.monotonic() - t0, 1), "out": out}, indent=1))
    return 0 if not bad else 1
