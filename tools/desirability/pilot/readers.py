"""Pilot task kind `reader`: model-reader questions with critical facts (H04 / I3 style).

The coordinator runs the readers; the harness packs the prompts and scores the answers.

- **Task**: the H04 key format unchanged (`key`: checks of kinds url, choice, int, ident, lower,
  type, code; facts naming checks; optional claims), the task text, a reference answer, one
  seeded wrong answer per fact (`wrong`), equivalent spellings, the reply fixtures, the SDK and
  the commit answers compile against (`compile_at`: a commit, or `arm`).
- **Pack** (`pack`): per candidate arm a paired, balanced, interleaved schedule (H04
  `runner.make_schedule`, imported), and one prompt per (arm, group): the set's reader template
  (the part after its `---` line) with `{TASKS}`, `{CRATE}` and one `{NAME}` per materials file
  `<materials>/<sha12>/<group>/<NAME>.txt` substituted in one pass over the template only
  (inserted text stays literal), `prompts/manifest.json` (SHA-256 of every prompt and input) and
  `prompts/FROZEN.json`. Packing again is refused once any session is recorded.
- **Sessions** (written by the coordinator's runner, H04 layout): `sessions/<id>/answer.json`
  (the parsed JSON answer, or null) and `meta.json` (`id, arm, commit, provider, sdk` = group,
  `rep, prompt_sha256`, requested/observed model and effort, `answer_error`).
- **Score** (`score_sessions`): H04 `scorer.run_integrity_problems` (frozen run, sessions on the
  manifest's prompts, schedule and arms unchanged), then every answer through H04 `scorer.plan` /
  `finish` (imported: the same field checks, compiled checks, runs and critical facts) on a
  materialised tree of the compile commit with its own target directory. A missing or malformed
  session scores 0 on every task of its group; first answers are kept.
"""
import hashlib
import json
import os
import re
import shutil
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "h04"))
sys.path.insert(0, os.path.join(HERE, "..", "probes"))
import common as C  # noqa: E402
import harness as H  # noqa: E402
import scorer as SC  # noqa: E402  (h04/scorer.py)
import arms as A  # noqa: E402
import kinds as K  # noqa: E402
import seal  # noqa: E402

PLACEHOLDER = re.compile(r"\{([A-Z][A-Z0-9_]*)\}")


def sha(text):
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def text_of(t):
    if "text" in t:
        return t["text"]
    with open(os.path.join(t["_dir"], t["text_file"]), encoding="utf-8") as f:
        return f.read()


def inline_key(t):
    """The task's key with every reply `body_file` replaced by the file's text (read as the H04
    harness reads it), so tasks of one SDK may keep their fixtures in different directories."""
    root = os.path.join(t["_dir"], t.get("fixtures", "."))
    key = json.loads(json.dumps(t["key"]))

    def fix(replies):
        for r in replies or []:
            if "body_file" in r:
                with open(os.path.join(root, r.pop("body_file")), encoding="utf-8") as f:
                    r["body"] = f.read()

    for c in key.get("checks", []) + key.get("claims", []):
        for run in c.get("runs", []):
            fix(run.get("replies"))
            fix(run.get("oauth_replies"))
    return key


def groups(tasks):
    """{group: [reader tasks in number order]}; (sdk, number) and (group, number) must be unique."""
    out, seen = {}, set()
    for t in tasks:
        g = t.get("group", t["sdk"])
        for k in ((t["sdk"], t["number"]), ("group", g, t["number"])):
            if k in seen:
                raise A.Refused(f"reader task {t['id']}: {k} is not unique")
            seen.add(k)
        out.setdefault(g, []).append(t)
    return {g: sorted(v, key=lambda x: x["number"]) for g, v in out.items()}


def task_answers(answer, numbers):
    """{task number: answer object} of a session's answer (None or malformed: {}); first kept."""
    if not isinstance(answer, dict) or not isinstance(answer.get("answers"), list):
        return {}
    out = {}
    for a in answer["answers"]:
        if isinstance(a, dict):
            n = SC.norm_int(a.get("task"))
            if n in numbers and n not in out:
                out[n] = a
    return out


# ---------------------------------------------------------------- port of the H04 bank

def port_h04(bank, out_set):
    """Convert the sealed H04 bank (which must hash to `h04/BANK.sha256`) into reader task files
    under `<out_set>/tasks/` (sealed): one `<sdk>/t<n>.task.json` per task, its text as
    `<sdk>/t<n>.md`, the SDK's fixtures copied to `<sdk>/fixtures/`. Deterministic."""
    A.sealed_dir(out_set, "task set")
    b = SC.load_bank(bank, check_hash=True)
    root = os.path.join(out_set, "tasks")
    if os.path.exists(root):
        shutil.rmtree(root)
    os.makedirs(root)
    n = 0
    for short, info in sorted(b["sdks"].items()):
        d = os.path.join(root, short)
        shutil.copytree(os.path.join(bank, "tasks", short, "fixtures"), os.path.join(d, "fixtures"))
        for num in range(1, SC.TASKS_PER_SDK + 1):
            src = b["tasks"][(short, num)]
            with open(os.path.join(d, f"t{num}.md"), "w", encoding="utf-8") as f:
                f.write(src["text"])
            task = {"format": 1, "id": f"{short}/t{num}", "kind": "reader", "class": f"t{num}", "sdk": short,
                    "role": info["role"], "group": short, "number": num, "title": src["key"]["title"],
                    "text_file": f"t{num}.md", "key": src["key"], "reference": src["reference"], "wrong": src["wrong"],
                    "equivalent": src["equivalent"], "fixtures": ".",
                    "sdk_info": {k: info[k] for k in ("dir", "pkg", "crate")}, "compile_at": b["meta"]["commit"]}
            seal.write_json(os.path.join(d, f"t{num}.task.json"), task)
            n += 1
    seal.write_json(os.path.join(root, "set.json"), {"format": 1, "name": "h04-port",
                                                     "description": "the sealed H04 bank as pilot reader task files",
                                                     "commit": b["meta"]["commit"]})
    return n


# ---------------------------------------------------------------- scoring

def score_items(tasks, items, workdir, log, tree_sha=None):
    """items: [(item id, task, answer object or None)] -> {item: H04 `finish` result}. Compiled on
    a fresh tree of the tasks' compile commit (`compile_at`) in its own target directory."""
    commits = {t.get("compile_at") for _, t, _ in items}
    if len(commits) != 1 or None in commits or "arm" in commits:
        if not tree_sha:
            raise A.Refused(f"reader items compile against {sorted(map(str, commits))}: give one commit (compile_at) or an arm")
        commits = {tree_sha}
    sha = A.full_sha(commits.pop())
    tree = os.path.join(workdir, "trees", f"reader-{sha[:12]}")
    prov = A.materialise(sha, tree)
    target = A.target_dir(sha, "clean")
    log.append({"reader_tree": tree, "provenance": prov, "target": target})
    sdks, by_task = {}, {}
    for _, t, _ in items:
        info = t.get("sdk_info") or K.sdk_info(tree, t["sdk"])
        sdks[t["sdk"]] = {**info, "short": t["sdk"], "role": t.get("role", "-")}
        by_task[(t["sdk"], t["number"])] = {"key": inline_key(t)}
    bank = {"tasks": by_task, "sdks": sdks, "meta": {"commit": sha}}
    batch = H.Batch(A.REPO, sha, sdks, {k: tree for k in sdks}, tree=tree, target_dir=target,
                    run_root=os.path.join(workdir, "scratch", "reader-run"))
    plans = {}
    for item, t, ans in items:
        key = by_task[(t["sdk"], t["number"])]["key"]
        ans = ans if isinstance(ans, dict) else {}
        plans[item] = (t["sdk"], t["number"], ans, SC.plan(batch, item, t["sdk"], t["number"], key, ans))
    A.check_tree(tree, sha)
    A.check_target(target, sha, "clean")
    results, _ = batch.compile_and_run(log)
    return SC.finish(plans, results, bank)


def load_sessions(runs):
    sched = C.read_json(os.path.join(runs, "schedule.json"))
    out = []
    for s in sched["sessions"]:
        d = os.path.join(runs, "sessions", s["id"])
        ans = C.read_json(os.path.join(d, "answer.json")) if os.path.isfile(os.path.join(d, "answer.json")) else None
        meta = C.read_json(os.path.join(d, "meta.json")) if os.path.isfile(os.path.join(d, "meta.json")) else None
        out.append({**s, "answer": ans, "meta": meta, "ran": meta is not None})
    return sched, out


def score_sessions(tasks, runs, workdir):
    """Rows (one per ran session x task of its group) for the tally, after the run-integrity check."""
    sched, sessions = load_sessions(runs)
    bad = SC.run_integrity_problems(runs, sessions)
    if bad:
        raise A.Refused("reader run refused: " + "; ".join(bad[:10]))
    by_group = groups(tasks)
    items, meta = [], {}
    missing = []
    for s in sessions:
        if not s["ran"]:
            missing.append(s["id"])
            continue
        gt = by_group.get(s["sdk"])
        if not gt:
            raise A.Refused(f"session {s['id']}: no reader tasks in group {s['sdk']}")
        per = task_answers(s["answer"], {t["number"] for t in gt})
        for t in gt:
            item = f"{s['id']}_t{t['number']}"
            items.append((item, t, per.get(t["number"])))
            meta[item] = (s, t)
    log = []
    scored = score_items(tasks, items, workdir, log)
    rows = []
    for item, res in scored.items():
        s, t = meta[item]
        rows.append({"task": t["id"], "kind": "reader", "sdk": t["sdk"], "class": t["class"], "role": t.get("role", "-"),
                     "group": t.get("group", t["sdk"]), "arm": s["arm"], "slot": f"{s['provider']}/r{s['rep']}", "rep": 1,
                     "session": s["id"], "provider": s["provider"], "session_rep": s["rep"], "number": t["number"],
                     "verdict": "pass" if res["pass"] else "fail",
                     "facts": res["facts"], "checks": res["checks"],
                     "observed": (s["meta"] or {}).get("observed"), "requested": (s["meta"] or {}).get("requested")})
    return {"rows": rows, "missing_sessions": missing, "log": log, "arms": sched.get("arms")}


# ---------------------------------------------------------------- packing

def crate_of(t, sha_):
    """The SDK's crate name: the task's `sdk_info`, else the package name at the arm's commit."""
    if t.get("sdk_info"):
        return t["sdk_info"]["crate"]
    toml = A.git(A.REPO, "show", f"{sha_}:examples/{t['sdk']}-sdk/Cargo.toml")
    m = re.search(r'(?m)^name\s*=\s*"([^"]+)"', toml)
    return m.group(1).replace("-", "_") if m else ""


def build_prompt(template, values):
    body = template.split("\n---\n", 1)[1].lstrip("\n") if "\n---\n" in template else template
    names = set(PLACEHOLDER.findall(body))
    missing = sorted(names - set(values))
    if missing:
        raise A.Refused(f"the reader template needs {missing}: no such materials")
    return PLACEHOLDER.sub(lambda m: values[m.group(1)].strip(), body).strip() + "\n"


def pack(rf, set_, materials, seed, providers, reps):
    """Schedule, prompts, manifest and freeze record for every candidate arm (see module doc)."""
    import runner as RUN  # noqa: E402  (h04/runner.py: the paired, balanced schedule)
    tpl_rel = set_["meta"].get("reader_template")
    if not tpl_rel:
        raise A.Refused("set.json names no reader_template")
    with open(os.path.join(set_["tasks_dir"], tpl_rel), encoding="utf-8") as f:
        template = f.read()
    rtasks = [t for t in set_["tasks"].values() if t["kind"] == "reader"]
    gs = groups(rtasks)
    arms = {a["name"]: A.full_sha(a["commit"]) for a in rf["arms"]}
    base = rf["baseline"]
    done = []
    for cand in [a for a in arms if a != base]:
        runs = os.path.join(rf["out"], "readers", cand)
        if any(os.path.isfile(os.path.join(runs, "sessions", d, "meta.json"))
               for d in (os.listdir(os.path.join(runs, "sessions")) if os.path.isdir(os.path.join(runs, "sessions")) else [])):
            raise A.Refused(f"{runs}: sessions are recorded; its prompts are frozen (use a new run directory)")
        sched = RUN.make_schedule(arms[base], arms[cand], seed, providers, reps, sdks=sorted(gs))
        seal.write_json(os.path.join(runs, "schedule.json"), sched)
        prompts, inputs = {}, {}
        for arm_label, sha_ in (("baseline", arms[base]), ("candidate", arms[cand])):
            for g, ts in gs.items():
                d = os.path.join(materials, sha_[:12], g)
                values = {"TASKS": "\n\n".join(text_of(t).strip() for t in ts), "CRATE": crate_of(ts[0], sha_)}
                if os.path.isdir(d):
                    for f in sorted(os.listdir(d)):
                        m = re.fullmatch(r"([A-Z][A-Z0-9_]*)\.txt", f)
                        if m:
                            with open(os.path.join(d, f), encoding="utf-8", newline="") as fh:
                                values[m.group(1)] = fh.read()
                            inputs[f"{arm_label}/{g}/{f}"] = sha(values[m.group(1)])
                text = build_prompt(template, values)
                p = os.path.join(runs, "prompts", arm_label, f"{g}.md")
                os.makedirs(os.path.dirname(p), exist_ok=True)
                with open(p, "w", encoding="utf-8") as fh:
                    fh.write(text)
                prompts[f"{arm_label}/{g}"] = {"sha256": sha(text), "path": os.path.relpath(p, runs)}
        manifest = {"arms": sched["arms"], "prompts": prompts, "inputs": inputs, "template_sha256": sha(template),
                    "set_hash": seal.set_hash(set_["dir"])[0]}
        seal.write_json(os.path.join(runs, "prompts", "manifest.json"), manifest)
        with open(os.path.join(runs, "prompts", "manifest.json"), encoding="utf-8") as f:
            msha = sha(f.read())
        with open(os.path.join(runs, "schedule.json"), encoding="utf-8") as f:
            ssha = sha(f.read())
        seal.write_json(os.path.join(runs, "prompts", "FROZEN.json"),
                        {"manifest_sha256": msha, "prompts": {k: v["sha256"] for k, v in prompts.items()},
                         "schedule_sha256": ssha, "arms": sched["arms"]})
        done.append({"candidate": cand, "runs": runs, "sessions": len(sched["sessions"]), "prompts": len(prompts)})
    return done
