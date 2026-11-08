"""H04 scorer: per-task critical facts for the docs pilot, the paired comparison and the card's
decision; also the instrument validation of the sealed bank.

    python3 -I tools/desirability/h04/scorer.py --bank SEALED --runs RUNS --out DIR
        [--gates pass|fail|unknown] [--increased-obligations yes|no|unknown]
        [--attributable yes|no|unknown]
    python3 -I tools/desirability/h04/scorer.py --bank SEALED --validate --out DIR

The bank (`<SEALED>/tasks/`, never in the repository) holds per SDK four tasks, each with
`task.md` (the text readers see), `key.json` (checks and critical facts), `reference.json` (a
passing answer), `wrong.json` (one seeded wrong answer per critical fact) and the reply fixtures.
The bank must hash (`bank_hash`) to `tools/desirability/h04/BANK.sha256`, or nothing is scored.

Checks (every answer field is scored automatically; nothing is graded as free text):

- `url`: compared like I3 R1 destination + query (`probes/score.py` `split_url`: scheme and host
  lower case, percent-encoding hex upper case, empty path `/`, query pairs in order, not decoded);
- `choice`: one option letter (`B`, `(b)`, `Option B.` are read as `B`);
- `int`: an integer (a JSON number or a decimal string);
- `ident`: a Rust name, compared on its last path segment without a trailing `(..)`/`{..}`,
  case-insensitively (`Error::CreateUncertain`, `CreateUncertain(_)`);
- `lower`: a header name, compared in lower case;
- `type`: I3 R2's rustc identity check (`r2_gate`, then `r2_check_fn` behind the invariant
  marker), compiled in an integration test with `use <sdk crate>::*;` (any spelling rustc
  resolves to the same type passes: re-exports, aliases, `crate::`, the crate name, `std`/`core`);
- `code`: each slot's answer passes the answer form (`harness.check_form`), is pasted into the
  check's trusted scenario, compiled in the integration test (public API only) inside the I3
  compile sandbox and run in the I3 run sandbox against scripted local receivers. A check has one
  or more **runs** (fix round 1): the response the task discloses, and for every expression over
  a result or an error two undisclosed, varying responses (so a constant copied from the task text
  fails), and for constructions further scenarios that exercise the submitted inputs (an expired
  token refreshed with the submitted client credentials, the default send policy) with, where a
  client takes two service origins, one receiver per origin: each run states which receiver gets
  which request (`oauth_requests`). It passes when every run compiles and its outcome equals the
  expected one (`@@OK <Debug>`) and the recorded requests equal the expected ones, or one of the
  permitted alternatives (`requests_any`: a default written out) (method, path and query pairs in
  order, header names and values, body: `compare_requests`) and, where the key says so, every
  request carrying `api-sign` has a Kraken signature recomputed independently here from the
  recorded path and body with the known secret (HMAC-SHA512 over path || SHA256(nonce || body);
  never `prepare()`).

A task passes only when every critical fact passes; a fact passes when all its checks pass. A
session whose answer is missing or malformed scores 0 on all four tasks. `--validate` also scores,
per task, one seeded wrong answer per critical fact and the targeted controls (`F2+constant`,
`F1+secret`, ..: each must fail its fact), the equivalent spellings (each must pass), the claims,
malformed sessions and a synthetic end-to-end run. Run Python with `-I`.
"""
import argparse
import base64
import glob
import hashlib
import hmac
import json
import os
import re
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "..", "probes"))
import common as C  # noqa: E402
import score as S  # noqa: E402  (probes/score.py: the probes directory is first on sys.path)
import harness as H  # noqa: E402
import decision as D  # noqa: E402

BANK_HASH_FILE = os.path.join(HERE, "BANK.sha256")
TASKS_PER_SDK = 4


# ---------------------------------------------------------------- bank

def bank_hash(bank):
    """SHA-256 of the sealed bank's canonical listing: every regular file under `<bank>/tasks/`, one
    line `<sha256 of file>  <path relative to tasks/>` each, sorted by path (POSIX separators, byte
    order), newline-terminated (as `tools/desirability/edits/run.py` `bank_hash`). Returns (hash,
    number of files, number of tasks)."""
    root = os.path.join(bank, "tasks")
    names = []
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames.sort()
        for f in filenames:
            p = os.path.join(dirpath, f)
            if os.path.isfile(p) and not os.path.islink(p):
                names.append(os.path.relpath(p, root).replace(os.sep, "/"))
    names.sort(key=lambda n: n.encode())
    listing = "".join(f"{C.sha256_file(os.path.join(root, n))}  {n}\n" for n in names)
    tasks = {"/".join(n.split("/")[:2]) for n in names if re.match(r"^[^/]+/t\d+/", n)}
    return hashlib.sha256(listing.encode()).hexdigest(), len(names), len(tasks)


def recorded_bank_hash():
    try:
        return open(BANK_HASH_FILE, encoding="utf-8").read().split()[0]
    except (OSError, IndexError):
        return None


def load_bank(bank, check_hash=True):
    if check_hash:
        digest, nfiles, _ = bank_hash(bank)
        rec = recorded_bank_hash()
        if digest != rec:
            raise SystemExit(f"the sealed bank {bank} hashes to {digest} ({nfiles} files), but BANK.sha256 records {rec}: "
                             "refusing to score against an unrecorded bank")
    root = os.path.join(bank, "tasks")
    meta = C.read_json(os.path.join(root, "bank.json"))
    tasks = {}
    for sdk in meta["sdks"]:
        for n in range(1, TASKS_PER_SDK + 1):
            d = os.path.join(root, sdk["short"], f"t{n}")
            tasks[(sdk["short"], n)] = {"key": C.read_json(os.path.join(d, "key.json")),
                                        "reference": C.read_json(os.path.join(d, "reference.json")),
                                        "wrong": C.read_json(os.path.join(d, "wrong.json")),
                                        "equivalent": C.read_json(os.path.join(d, "equivalent.json")),
                                        "text": open(os.path.join(d, "task.md"), encoding="utf-8").read()}
    return {"dir": bank, "meta": meta, "tasks": tasks,
            "sdks": {s["short"]: s for s in meta["sdks"]},
            "fixtures": {s["short"]: os.path.join(root, s["short"]) for s in meta["sdks"]}}


# ---------------------------------------------------------------- field checks in Python

def norm_url(u):
    if not isinstance(u, str):
        return None
    dest, q = S.split_url(u.strip().strip("`").strip(), [])
    return None if dest is None else (dest, q)


def norm_choice(v):
    if not isinstance(v, str):
        return None
    m = re.fullmatch(r"\W*(?:option\s+)?([A-Za-z])\W*", v.strip(), re.I)
    return m.group(1).upper() if m else None


def norm_int(v):
    if isinstance(v, bool):
        return None
    if isinstance(v, int):
        return v
    if isinstance(v, str) and re.fullmatch(r"\s*\d+\s*", v):
        return int(v)
    return None


def norm_ident(v):
    if not isinstance(v, str):
        return None
    s = v.strip().strip("`").strip()
    s = re.sub(r"\s*[({].*$", "", s, flags=re.S)
    s = s.split("::")[-1].strip()
    return s.lower() if re.fullmatch(r"[A-Za-z_]\w*", s) else None


def py_check(c, ans):
    """(ok, reason) for the checks decided without compiling."""
    v = ans.get(c["field"]) if isinstance(ans, dict) else None
    k = c["kind"]
    if k == "url":
        got, want = norm_url(v), norm_url(c["expect"])
        return got == want, None if got == want else f"url {v!r} is not {c['expect']!r}"
    if k == "choice":
        got = norm_choice(v)
        if got is not None and got not in c.get("options", "ABCDEF"):
            return False, f"choice {v!r} is not an offered option"
        return got == c["expect"], None if got == c["expect"] else f"choice {v!r}"
    if k == "int":
        got = norm_int(v)
        return got == c["expect"], None if got == c["expect"] else f"{c['field']} {v!r}"
    if k == "ident":
        got = norm_ident(v)
        ok = got is not None and got in {e.lower() for e in c["expect"]}
        return ok, None if ok else f"{c['field']} {v!r}"
    if k == "lower":
        got = v.strip().strip("`").strip().rstrip(":").lower() if isinstance(v, str) else None
        return got == c["expect"], None if got == c["expect"] else f"{c['field']} {v!r}"
    raise ValueError(k)


# ---------------------------------------------------------------- compiled checks

def kraken_signature_ok(rec, secret_b64):
    """Recompute API-Sign from the recorded request: Base64(HMAC-SHA512(secret, path ||
    SHA256(nonce || body))), nonce = the body's leading `nonce=` pair."""
    hs = {k.lower(): v for k, v in rec["headers"]}
    if "api-sign" not in hs:
        return True, None
    body = rec.get("body") or ""
    m = re.match(r"nonce=(\d+)(?:&|$)", body)
    if not m:
        return False, "signed request without a leading nonce"
    path = rec["target"].split("?", 1)[0]
    inner = hashlib.sha256(m.group(1).encode() + body.encode()).digest()
    want = base64.b64encode(hmac.new(base64.b64decode(secret_b64), path.encode() + inner, hashlib.sha512).digest()).decode()
    return (hs["api-sign"] == want), (None if hs["api-sign"] == want else "API-Sign does not verify")


def code_verdict(run, res):
    """(ok, reason) of one run of a code check (or claim) against its expectation."""
    exp = run.get("expect", {})
    if res is None:
        return False, "not run"
    if exp.get("compiles") is False:
        ok = res.get("compiles") is False
        return ok, None if ok else "compiled, expected a compile error"
    if not res.get("compiles"):
        return False, res.get("reason") or "does not compile"
    if not res.get("complete"):
        return False, res.get("reason") or "incomplete run"
    out = res.get("outcome")
    if "outcome" in exp and out != exp["outcome"]:
        return False, f"outcome {out!r} instead of {exp['outcome']!r}"
    if "outcome_prefix" in exp and not (out or "").startswith(exp["outcome_prefix"]):
        return False, f"outcome {out!r}"
    got = res.get("requests") or []
    if "request_count" in exp and len(got) != exp["request_count"]:
        return False, f"{len(got)} requests instead of {exp['request_count']}"
    alternatives = [exp["requests"]] if "requests" in exp else []
    alternatives += exp.get("requests_any", [])
    if alternatives:
        why = None
        for want in alternatives:  # any permitted form (an omitted default, or the default written out)
            ok, why1, _ = S.compare_requests(got, {"recorded": want, "recorded_again": want, "volatile": {}})
            if ok:
                break
            why = why or why1
        else:
            return False, why
    if "oauth_requests" in exp:  # a run with a separate OAuth receiver: which receiver got which request
        go = res.get("oauth_requests")
        if go is None:
            return False, "no record from the OAuth receiver"
        want = exp["oauth_requests"]
        if len(go) != len(want):
            return False, f"OAuth receiver: {len(go)} requests instead of {len(want)} ({[(r['method'], r['target']) for r in go]})"
        if want:
            ok, why, _ = S.compare_requests(go, {"recorded": want, "recorded_again": want, "volatile": {}})
            if not ok:
                return False, "OAuth receiver: " + why
    if exp.get("kraken_secret_b64"):
        for r in got:
            ok, why = kraken_signature_ok(r, exp["kraken_secret_b64"])
            if not ok:
                return False, why
    return True, None


def plan(batch, item, sdk, n, key, ans, include_claims=False):
    """Add an answer's compiled checks to `batch`; return {check id: (ok, reason) or "pending"}."""
    out = {}
    for c in key["checks"] + (key.get("claims", []) if include_claims else []):
        cid = c["id"] if c["kind"] != "claim" else "claim:" + c["id"]
        k = c["kind"]
        if k in ("url", "choice", "int", "ident", "lower"):
            out[cid] = py_check(c, ans)
        elif k == "type":
            text, why = S.r2_gate(ans.get(c["field"]) if isinstance(ans, dict) else None)
            if text is None:
                out[cid] = (False, why)
                continue
            batch.add_type((item, cid), sdk, f"{item}_{n}_{cid}", c["expect"], text)
            out[cid] = "pending"
        elif k in ("code", "claim"):
            slots, refusal = {}, None
            for slot, spec in (c.get("slots") or {}).items():
                code, why = H.check_form(ans.get(spec["field"]) if isinstance(ans, dict) else None, spec["form"])
                if code is None:
                    refusal = f"{spec['field']}: {why}"
                    break
                slots[slot] = code
            if refusal:
                out[cid] = (False, refusal)
                continue
            for i, run in enumerate(c["runs"]):
                batch.add_run((item, cid, i), sdk, f"{item}_{n}_{cid}_{i}", run["replies"],
                              H.fill(run.get("scenario", c.get("scenario")), slots), run.get("limit_s", H.RUN_LIMIT_S),
                              run.get("oauth_replies"))
            out[cid] = "pending"
        else:
            raise ValueError(k)
    return out


def finish(plans, results, bank):
    """{item: {"checks", "facts", "pass"}} once the batch ran."""
    out = {}
    for item, (sdk, n, ans, checks) in plans.items():
        key = bank["tasks"][(sdk, n)]["key"]
        allc = {c["id"]: c for c in key["checks"]}
        allc.update({"claim:" + c["id"]: c for c in key.get("claims", [])})
        final = {}
        for cid, v in checks.items():
            if v != "pending":
                final[cid] = {"ok": bool(v[0]), "reason": v[1]}
                continue
            c = allc[cid]
            res = results.get((item, cid))
            if c["kind"] == "type":
                if res is None or res.get("compiles") is None:
                    final[cid] = {"ok": False, "reason": (res or {}).get("reason", "not judged"), "unjudged": True}
                else:
                    final[cid] = {"ok": bool(res["compiles"]), "reason": None if res["compiles"] else res.get("reason")}
            else:
                # every run of the check must pass: the disclosed response and the undisclosed variants
                runs = []
                for i, run in enumerate(c["runs"]):
                    r_ = results.get((item, cid, i))
                    ok, why = code_verdict(run, r_)
                    runs.append({"run": run.get("name", str(i)), "ok": ok, "reason": why, "outcome": (r_ or {}).get("outcome"),
                                 "requests": [(r["method"], r["target"]) for r in (r_ or {}).get("requests") or []]})
                bad = next((r for r in runs if not r["ok"]), None)
                final[cid] = {"ok": bad is None, "reason": None if bad is None else f"run {bad['run']}: {bad['reason']}",
                              "runs": runs}
        facts = {f["id"]: all(final.get(cid, {}).get("ok") for cid in f["checks"]) for f in key["facts"]}
        out[item] = {"sdk": sdk, "task": n, "checks": final, "facts": facts, "pass": all(facts.values())}
    return out


def score_answers(bank, items, log, include_claims=False, commit=None):
    """items: [(item id, sdk, task, answer dict or None)] → finish() result."""
    batch = H.Batch(C.REPO, commit or bank["meta"]["commit"], bank["sdks"], bank["fixtures"])
    plans = {}
    for item, sdk, n, ans in items:
        key = bank["tasks"][(sdk, n)]["key"]
        ans = ans if isinstance(ans, dict) else {}
        plans[item] = (sdk, n, ans, plan(batch, item, sdk, n, key, ans, include_claims))
    results, _ = batch.compile_and_run(log)
    return finish(plans, results, bank)


# ---------------------------------------------------------------- sessions

def task_answers(answer):
    """{task number: answer object} from a session's answer (None or malformed: {})."""
    if not isinstance(answer, dict) or not isinstance(answer.get("answers"), list):
        return {}
    out = {}
    for a in answer["answers"]:
        if isinstance(a, dict):
            n = norm_int(a.get("task"))
            if n in range(1, TASKS_PER_SDK + 1) and n not in out:  # first answer kept
                out[n] = a
    return out


def load_runs(runs):
    sched = C.read_json(os.path.join(runs, "schedule.json"))
    sessions = []
    for s in sched["sessions"]:
        d = os.path.join(runs, "sessions", s["id"])
        ans = C.read_json(os.path.join(d, "answer.json")) if os.path.isfile(os.path.join(d, "answer.json")) else None
        meta = C.read_json(os.path.join(d, "meta.json")) if os.path.isfile(os.path.join(d, "meta.json")) else None
        sessions.append({**s, "answer": ans, "meta": meta, "ran": meta is not None})
    return sched, sessions


def run_integrity_problems(runs, sessions):
    """Fix rounds 7-8: the run must be frozen (`prompts/FROZEN.json`) with this manifest, this
    schedule and these arms; the manifest's arms are the schedule's; every recorded session is one
    of the schedule's, with its arm, commit, provider, SDK and repetition, and records the SHA-256
    of its prompt as the manifest has it. [] when the run may be scored."""
    mpath, fpath = os.path.join(runs, "prompts", "manifest.json"), os.path.join(runs, "prompts", "FROZEN.json")
    if not os.path.isfile(mpath):
        return ["the run has no prompts/manifest.json"]
    with open(mpath, encoding="utf-8") as f:
        mtext = f.read()
    with open(os.path.join(runs, "schedule.json"), encoding="utf-8") as f:
        stext = f.read()
    manifest, sched = json.loads(mtext), json.loads(stext)
    digest = lambda t: hashlib.sha256(t.encode("utf-8")).hexdigest()  # noqa: E731
    problems = []
    if manifest.get("arms") != sched["arms"]:
        problems.append("the manifest's arms are not the schedule's")
    recorded = sorted(os.path.basename(os.path.dirname(p)) for p in glob.glob(os.path.join(runs, "sessions", "*", "meta.json")))
    if recorded:
        if not os.path.isfile(fpath):
            problems.append("sessions ran but the run is not frozen (prompts/FROZEN.json)")
        else:
            fz = C.read_json(fpath)
            if fz.get("manifest_sha256") != digest(mtext):
                problems.append("the manifest is not the one the run was frozen with")
            if fz.get("schedule_sha256") != digest(stext) or fz.get("arms") != sched["arms"]:
                problems.append("the schedule or its arms are not the ones the run was frozen with")
    by_id = {s["id"]: s for s in sched["sessions"]}
    for sid in recorded:
        s = by_id.get(sid)
        if s is None:
            problems.append(f"{sid}: a recorded session that the schedule does not have")
            continue
        meta = C.read_json(os.path.join(runs, "sessions", sid, "meta.json"))
        if any(meta.get(k) != s[k] for k in ("arm", "commit", "provider", "sdk", "rep")):
            problems.append(f"{sid}: its recorded arm, commit, provider, SDK or repetition is not the schedule's")
        want = (manifest.get("prompts", {}).get(f"{s['arm']}/{s['sdk']}") or {}).get("sha256")
        if not want or meta.get("prompt_sha256") != want:
            problems.append(f"{sid}: its prompt's SHA-256 is not the manifest's")
    return problems


def rate(xs):
    xs = list(xs)
    return {"pass": sum(xs), "n": len(xs), "rate": round(sum(xs) / len(xs), 3) if xs else None}


def report(rows, bank):
    by = lambda keyf: {k: rate(r["pass"] for r in rows if keyf(r) == k) for k in sorted({keyf(r) for r in rows}, key=str)}  # noqa: E731
    out = {"per_task_provider_arm": {}, "per_sdk_arm": by(lambda r: f"{r['sdk']}|{r['arm']}"),
           "per_provider_arm": by(lambda r: f"{r['provider']}|{r['arm']}"), "per_arm": by(lambda r: r["arm"]),
           "per_role_arm": by(lambda r: f"{r['role']}|{r['arm']}"), "critical_fact_failures": {}}
    for r in rows:
        k = f"{r['sdk']}/t{r['task']}|{r['provider']}|{r['arm']}"
        out["per_task_provider_arm"].setdefault(k, []).append(r["pass"])
        for f, ok in r["facts"].items():
            if not ok:
                fk = f"{r['sdk']}/t{r['task']}/{f}|{r['arm']}|{r['provider']}"
                out["critical_fact_failures"][fk] = out["critical_fact_failures"].get(fk, 0) + 1
    out["per_task_provider_arm"] = {k: rate(v) for k, v in sorted(out["per_task_provider_arm"].items())}
    return out


def render_md(rep, verdict, reasons, summ, sessions_info, bank):
    L = ["# H04 docs pilot: score", "", f"Bank commit (calls compiled against): `{bank['meta']['commit']}`", "",
         f"**Decision: {verdict}**", ""]
    L += [f"- {r}" for r in reasons] + [""]
    L += ["## Paired comparison", "", "| group | baseline | candidate | corrections | regressions | net | answers |", "|---|---|---|---|---|---|---|"]
    for g in ("witness", "control"):
        s = summ[g]
        L.append(f"| {g} | {s['baseline']} | {s['candidate']} | {s['corrections']} | {s['regressions']} | {s['net']:+d} | {s['answers']} |")
    L += ["", "| provider | witness net | witness corrections | witness regressions | control net |", "|---|---|---|---|---|"]
    for p, v in summ["by_provider"].items():
        L.append(f"| {p} | {v['witness_net']:+d} | {v['witness_corrections']} | {v['witness_regressions']} | {v['control_net']:+d} |")
    L += ["", f"Control answers lost (control regressions, the decision's only rule): {summ['control_lost']}",
          f"Control net loss: {summ['diagnostic_not_the_decision']['control_net_loss']} (diagnostic, not the decision)",
          f"Recurrent task regressions: {summ['recurrent_task_regressions'] or 'none'}",
          f"Recurrent control-fact regressions (>= 2 pairs): {summ['recurrent_control_fact_regressions'] or 'none'}",
          f"Newly misrepresented control facts (pass in every baseline answer, fail in a candidate answer): {summ['newly_misrepresented_control_facts'] or 'none'}",
          f"Incomplete pairs: {len(summ['incomplete_pairs'])}", ""]
    L += ["## Per task, provider and arm", "", "| task | provider | arm | pass/n |", "|---|---|---|---|"]
    for k, v in rep["per_task_provider_arm"].items():
        t, p, a = k.split("|")
        L.append(f"| {t} ({bank['meta']['titles'].get(t, '')}) | {p} | {a} | {v['pass']}/{v['n']} |")
    L += ["", "## Critical-fact failures (reported separately)", "", "| task/fact | arm | provider | failures |", "|---|---|---|---|"]
    for k, v in sorted(rep["critical_fact_failures"].items()):
        tf, a, p = k.split("|")
        L.append(f"| {tf} | {a} | {p} | {v} |")
    L += ["", "## Sessions", "", f"{sessions_info}", ""]
    return "\n".join(L) + "\n"


def score_runs(a):
    bank = load_bank(a.bank, check_hash=not getattr(a, "unrecorded", False))
    sched, sessions = load_runs(a.runs)
    bad = run_integrity_problems(a.runs, sessions)
    if bad:
        sys.exit("refused: " + "; ".join(bad[:10]) + (f" (+{len(bad) - 10} more)" if len(bad) > 10 else ""))
    items, rows_meta = [], {}
    missing = []
    for s in sessions:
        if not s["ran"]:
            missing.append(s["id"])
            continue
        per = task_answers(s["answer"])
        for n in range(1, TASKS_PER_SDK + 1):
            item = f"{s['id']}_t{n}"
            items.append((item, s["sdk"], n, per.get(n)))
            rows_meta[item] = s
    log = []
    t0 = time.monotonic()
    scored = score_answers(bank, items, log)
    rows = []
    for item, res in scored.items():
        s = rows_meta[item]
        rows.append({"session": s["id"], "sdk": s["sdk"], "role": bank["sdks"][s["sdk"]]["role"], "task": res["task"],
                     "provider": s["provider"], "rep": s["rep"], "arm": s["arm"], "pass": res["pass"], "facts": res["facts"],
                     "checks": res["checks"],
                     "observed_model": (s["meta"] or {}).get("observed"), "requested": (s["meta"] or {}).get("requested"),
                     "answer_error": (s["meta"] or {}).get("answer_error")})
    verdict, reasons, summ = D.decide(rows, gates=a.gates, increased_obligations=a.increased_obligations,
                                      attributable=a.attributable, docs_truth=a.docs_truth)
    if missing:
        reasons.append(f"{len(missing)} scheduled sessions have not run")
    rep = report(rows, bank)
    os.makedirs(a.out, exist_ok=True)
    C.write_json(os.path.join(a.out, "score.json"), {"verdict": verdict, "reasons": reasons, "summary": summ, "report": rep,
                                                     "rows": rows, "missing_sessions": missing, "log": log,
                                                     "seconds": round(time.monotonic() - t0, 1),
                                                     "schedule": {k: v for k, v in sched.items() if k != "sessions"}})
    info = f"{len(sessions) - len(missing)} of {len(sessions)} scheduled sessions ran; models observed: " + json.dumps(
        sorted({json.dumps(r['observed_model'], sort_keys=True) for r in rows}))
    with open(os.path.join(a.out, "score.md"), "w", encoding="utf-8") as f:
        f.write(render_md(rep, verdict, reasons, summ, info, bank))
    print(json.dumps({"verdict": verdict, "witness_net": summ["witness"]["net"], "control_lost": summ["control_lost"],
                      "reasons": reasons}, indent=1))


# ---------------------------------------------------------------- instrument validation

def validate(a):
    """References pass at the bank commit; each seeded wrong answer fails its fact (and the
    task); every claim holds; a malformed session scores 0; the decision self-test passes; the
    bank hash is reproducible."""
    t_start = time.monotonic()
    digest1 = bank_hash(a.bank)
    digest2 = bank_hash(a.bank)
    bank = load_bank(a.bank, check_hash=not a.unrecorded)
    items = []
    for (sdk, n), t in sorted(bank["tasks"].items()):
        items.append((f"ref_{sdk}_{n}", sdk, n, t["reference"]))
        for f, partial in sorted(t["wrong"].items()):
            items.append((f"wrong_{f}_{sdk}_{n}", sdk, n, {**t["reference"], **partial}))
        for i, partial in enumerate(t["equivalent"]):
            items.append((f"equiv{i}_{sdk}_{n}", sdk, n, {**t["reference"], **partial}))
    # a malformed session: no answer at all, and an answer whose tasks are unreadable
    for sdk in bank["sdks"]:
        for n in range(1, TASKS_PER_SDK + 1):
            items.append((f"malformed_none_{sdk}_{n}", sdk, n, task_answers(None).get(n)))
            items.append((f"malformed_shape_{sdk}_{n}", sdk, n, task_answers({"answers": "not a list"}).get(n)))
    log = []
    ref_items = {i for i, *_ in items if i.startswith("ref_")}
    batch_items = [(i, s, n, ans) for i, s, n, ans in items]
    scored = score_answers(bank, batch_items, log, include_claims=False)
    # claims: compiled once each, with the reference answer (claims carry no slots)
    claim_items = [(f"claim_{sdk}_{n}", sdk, n, {}) for (sdk, n) in sorted(bank["tasks"])]
    batch = H.Batch(C.REPO, bank["meta"]["commit"], bank["sdks"], bank["fixtures"])
    cplans = {}
    for item, sdk, n, ans in claim_items:
        key = dict(bank["tasks"][(sdk, n)]["key"])
        key = {**key, "checks": []}
        cplans[item] = (sdk, n, ans, plan(batch, item, sdk, n, key, ans, include_claims=True))
    cres, _ = batch.compile_and_run(log)
    claims = finish(cplans, cres, bank)
    rows, problems = [], []
    for (sdk, n), t in sorted(bank["tasks"].items()):
        ref = scored[f"ref_{sdk}_{n}"]
        row = {"task": f"{sdk}/t{n}", "title": t["key"]["title"], "reference_pass": ref["pass"],
               "reference_failures": {c: v["reason"] for c, v in ref["checks"].items() if not v["ok"]}, "wrong": {}, "claims": {}}
        if not ref["pass"]:
            problems.append(f"{sdk}/t{n}: the reference answer fails: {row['reference_failures']}")
        for f in sorted(t["wrong"]):
            # `F3` is the seeded wrong answer for fact F3; `F3+constant`, `F1+timeout`, .. are further
            # targeted negative controls of the same fact (constant answers, wrong construction inputs)
            fact = f.split("+")[0]
            w = scored[f"wrong_{f}_{sdk}_{n}"]
            row["wrong"][f] = {"fact": fact, "task_pass": w["pass"], "fact_pass": w["facts"].get(fact),
                               "other_facts_pass": all(v for k, v in w["facts"].items() if k != fact),
                               "reason": next((v["reason"] for c, v in w["checks"].items() if not v["ok"]), None)}
            if w["pass"] or w["facts"].get(fact) or fact not in w["facts"]:
                problems.append(f"{sdk}/t{n}: the seeded wrong answer {f} is not scored wrong on its fact")
        row["equivalent"] = []
        for i in range(len(t["equivalent"])):
            e = scored[f"equiv{i}_{sdk}_{n}"]
            row["equivalent"].append({"pass": e["pass"], "fields": sorted(t["equivalent"][i]),
                                      "failures": {c: v["reason"] for c, v in e["checks"].items() if not v["ok"]}})
            if not e["pass"]:
                problems.append(f"{sdk}/t{n}: an equivalent spelling ({sorted(t['equivalent'][i])}) is scored wrong: {row['equivalent'][-1]['failures']}")
        missing_facts = [f["id"] for f in t["key"]["facts"] if f["id"] not in {k.split("+")[0] for k in t["wrong"]}]
        if missing_facts:
            problems.append(f"{sdk}/t{n}: no seeded wrong answer for {missing_facts}")
        for cid, v in claims[f"claim_{sdk}_{n}"]["checks"].items():
            row["claims"][cid] = {"ok": v["ok"], "reason": v.get("reason"), "outcome": v.get("outcome")}
            if not v["ok"]:
                problems.append(f"{sdk}/t{n}: claim {cid} does not hold: {v.get('reason')}")
        for kind in ("none", "shape"):
            m = scored[f"malformed_{kind}_{sdk}_{n}"]
            row[f"malformed_{kind}_pass"] = m["pass"]
            if m["pass"]:
                problems.append(f"{sdk}/t{n}: a malformed session ({kind}) scores a pass")
        rows.append(row)
    e2e = synthetic_runs(a, bank)
    if not e2e["ok"]:
        problems.append(f"end-to-end synthetic runs: {e2e['problems']}")
    dok, dchecks = D.selftest()
    if not dok:
        problems.append("decision self-test failed")
    hash_ok = digest1 == digest2 and (a.unrecorded or digest1[0] == recorded_bank_hash())
    if not hash_ok:
        problems.append(f"bank hash not reproducible or not recorded: {digest1[0]} / {digest2[0]} / {recorded_bank_hash()}")
    result = {"bank": a.bank, "commit": bank["meta"]["commit"], "bank_hash": digest1[0], "files": digest1[1],
              "tasks": digest1[2], "hash_recomputed_equal": digest1 == digest2, "recorded_hash": recorded_bank_hash(),
              "rows": rows, "decision_selftest": dchecks, "problems": problems, "ok": not problems,
              "seconds": round(time.monotonic() - t_start, 1), "log": log,
              "end_to_end": e2e,
              "malformed_sessions_score_zero": all(not r["malformed_none_pass"] and not r["malformed_shape_pass"] for r in rows)}
    os.makedirs(a.out, exist_ok=True)
    C.write_json(os.path.join(a.out, "validation.json"), result)
    print(json.dumps({"ok": result["ok"], "bank_hash": result["bank_hash"], "files": result["files"], "tasks": result["tasks"],
                      "references_pass": sum(r["reference_pass"] for r in rows), "tasks_total": len(rows),
                      "wrong_answers_failing": sum(1 for r in rows for w in r["wrong"].values() if not w["task_pass"] and not w["fact_pass"]),
                      "wrong_answers_total": sum(len(r["wrong"]) for r in rows),
                      "equivalent_spellings_passing": sum(1 for r in rows for e in r["equivalent"] if e["pass"]),
                      "equivalent_spellings_total": sum(len(r["equivalent"]) for r in rows),
                      "claims_holding": sum(1 for r in rows for c in r["claims"].values() if c["ok"]),
                      "claims_total": sum(len(r["claims"]) for r in rows),
                      "decision_cases": f"{sum(c['ok'] for c in dchecks)}/{len(dchecks)}",
                      "end_to_end": {k: e2e[k] for k in ("ok", "verdict", "witness_net", "control_lost", "answers_scored",
                                                         "mismatched_session_refused", "unfrozen_run_refused",
                                                         "reseeded_schedule_refused", "replaced_arms_refused")},
                      "problems": problems}, indent=1))
    return 0 if result["ok"] else 1


def synthetic_runs(a, bank):
    """End-to-end check of the runs path (`score_runs`) on a synthetic 72-session run whose two
    arms both point at the bank commit (no candidate is built here): baseline sessions of the
    witnesses answer task 1 with the seeded wrong answer for its last fact on repetitions 1 and 2
    (12 baseline failures), every other answer is the reference, and one candidate control session
    is malformed (no answer). Expected: witness net +12 (+6 per provider), the malformed session's
    four answers 0, 4 control answers lost, verdict INCONCLUSIVE (control losses > 2)."""
    import runner as RUN  # noqa: E402  (h04/runner.py, lazily: it imports this module)
    root = os.path.join(a.out, "synthetic-runs")
    if os.path.isdir(root):
        import shutil
        shutil.rmtree(root)
    sched = RUN.make_schedule(bank["meta"]["commit"], bank["meta"]["commit"], seed=1)
    C.write_json(os.path.join(root, "schedule.json"), sched)
    malformed = next(s for s in sched["sessions"] if s["arm"] == "candidate" and s["sdk"] == "strava" and s["rep"] == 3
                     and s["provider"] == "codex")
    # a synthetic manifest and freeze record: each session records its (synthetic) prompt's SHA-256
    prompt_sha = {f"{arm}/{sdk}": hashlib.sha256(f"synthetic prompt {arm}/{sdk}".encode()).hexdigest()
                  for arm in ("baseline", "candidate") for sdk in sched["sdks"]}
    C.write_json(os.path.join(root, "prompts", "manifest.json"), {"arms": sched["arms"],
                                                                  "prompts": {k: {"sha256": v} for k, v in prompt_sha.items()}})
    with open(os.path.join(root, "prompts", "manifest.json"), encoding="utf-8") as f:
        msha = hashlib.sha256(f.read().encode("utf-8")).hexdigest()
    with open(os.path.join(root, "schedule.json"), encoding="utf-8") as f:
        ssha = hashlib.sha256(f.read().encode("utf-8")).hexdigest()
    C.write_json(os.path.join(root, "prompts", "FROZEN.json"), {"manifest_sha256": msha, "prompts": prompt_sha,
                                                                "schedule_sha256": ssha, "arms": sched["arms"]})
    for s in sched["sessions"]:
        answers = []
        for n in range(1, TASKS_PER_SDK + 1):
            t = bank["tasks"][(s["sdk"], n)]
            ans = dict(t["reference"])
            if (s["arm"] == "baseline" and bank["sdks"][s["sdk"]]["role"] == "witness" and n == 1 and s["rep"] in (1, 2)):
                last = sorted(t["wrong"])[-1]
                ans.update(t["wrong"][last])
            answers.append({"task": n, **ans, "confidence": "high"})
        d = os.path.join(root, "sessions", s["id"])
        C.write_json(os.path.join(d, "answer.json"), None if s is malformed else {"answers": answers})
        C.write_json(os.path.join(d, "meta.json"), {"id": s["id"], "observed": {"model": "synthetic"}, "requested": {"model": "synthetic"},
                                                    "answer_error": "malformed answer" if s is malformed else None,
                                                    "prompt_sha256": prompt_sha[f"{s['arm']}/{s['sdk']}"],
                                                    **{k: s[k] for k in ("arm", "commit", "provider", "sdk", "rep")}})
    ns = argparse.Namespace(bank=a.bank, runs=root, out=os.path.join(root, "score"), gates="unknown",
                            increased_obligations="unknown", attributable="unknown",
                            docs_truth="unknown", unrecorded=a.unrecorded)
    score_runs(ns)
    res = C.read_json(os.path.join(root, "score", "score.json"))
    summ = res["summary"]
    mal = [r for r in res["rows"] if r["session"] == malformed["id"]]
    problems = []
    if summ["witness"]["net"] != 12 or any(v["witness_net"] != 6 for v in summ["by_provider"].values()):
        problems.append(f"witness net {summ['witness']['net']} / {summ['by_provider']}")
    if len(mal) != 4 or any(r["pass"] for r in mal):
        problems.append("the malformed session does not score 0 on its four tasks")
    if summ["control_lost"] != 4 or res["verdict"] != "INCONCLUSIVE":
        problems.append(f"control lost {summ['control_lost']}, verdict {res['verdict']}")
    if len(res["rows"]) != 288:
        problems.append(f"{len(res['rows'])} answers scored instead of 288")
    # fix round 7: a session answered with another prompt is refused, and so is an unfrozen run
    other = next(s for s in sched["sessions"] if s["arm"] == "baseline")
    mp = os.path.join(root, "sessions", other["id"], "meta.json")
    keep = C.read_json(mp)
    C.write_json(mp, {**keep, "prompt_sha256": "0" * 64})
    mismatch_refused = bool(run_integrity_problems(root, load_runs(root)[1]))
    try:
        score_runs(argparse.Namespace(**{**vars(ns), "out": os.path.join(root, "score-mismatch")}))
        mismatch_scored = True
    except SystemExit:
        mismatch_scored = False
    C.write_json(mp, keep)
    fz = os.path.join(root, "prompts", "FROZEN.json")
    os.rename(fz, fz + ".off")
    unfrozen_refused = bool(run_integrity_problems(root, load_runs(root)[1]))
    os.rename(fz + ".off", fz)
    # fix round 8: a re-seeded schedule (a session disappears) and replaced arm commits are refused
    spath = os.path.join(root, "schedule.json")
    with open(spath, encoding="utf-8") as f:
        stext = f.read()
    C.write_json(spath, RUN.make_schedule(bank["meta"]["commit"], bank["meta"]["commit"], seed=2))
    reseeded_refused = bool(run_integrity_problems(root, load_runs(root)[1]))
    C.write_json(spath, {**json.loads(stext), "arms": {"baseline": bank["meta"]["commit"], "candidate": "0" * 40}})
    arms_refused = bool(run_integrity_problems(root, load_runs(root)[1]))
    with open(spath, "w", encoding="utf-8") as f:
        f.write(stext)
    restored_ok = not run_integrity_problems(root, load_runs(root)[1])
    if not (mismatch_refused and not mismatch_scored and unfrozen_refused and reseeded_refused and arms_refused and restored_ok):
        problems.append(f"run integrity: mismatch refused {mismatch_refused}, scored {mismatch_scored}, unfrozen refused "
                        f"{unfrozen_refused}, re-seeded schedule refused {reseeded_refused}, replaced arms refused {arms_refused}, "
                        f"restored ok {restored_ok}")
    return {"ok": not problems, "problems": problems, "verdict": res["verdict"], "witness_net": summ["witness"]["net"],
            "control_lost": summ["control_lost"], "answers_scored": len(res["rows"]), "score_md": os.path.join(root, "score", "score.md"),
            "mismatched_session_refused": mismatch_refused and not mismatch_scored, "unfrozen_run_refused": unfrozen_refused,
            "reseeded_schedule_refused": reseeded_refused, "replaced_arms_refused": arms_refused}


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--bank", required=True, help="the sealed bank directory (holding tasks/)")
    ap.add_argument("--runs", help="a runner.py output directory (schedule.json, sessions/)")
    ap.add_argument("--out", required=True)
    ap.add_argument("--validate", action="store_true")
    ap.add_argument("--unrecorded", action="store_true", help="validate a bank whose hash is not yet in BANK.sha256 (authoring only)")
    ap.add_argument("--gates", default="unknown", choices=("pass", "fail", "unknown"))
    ap.add_argument("--increased-obligations", default="unknown", choices=("yes", "no", "unknown"))
    ap.add_argument("--attributable", default="unknown", choices=("yes", "no", "unknown"),
                    help="a blocking regression (recurrent task failure, newly misrepresented control fact) is attributable to the candidate docs")
    ap.add_argument("--docs-truth", default="unknown", choices=("pass", "fail", "unknown"),
                    help="gate from the candidate review: no critical control fact misrepresented in the candidate docs")
    a = ap.parse_args()
    a.bank, a.out = os.path.abspath(a.bank), os.path.abspath(a.out)
    if a.validate:
        sys.exit(validate(a))
    if not a.runs:
        ap.error("--runs or --validate is required")
    if a.unrecorded:
        ap.error("--unrecorded is for authoring validation only")
    a.runs = os.path.abspath(a.runs)
    score_runs(a)


if __name__ == "__main__":
    main()
