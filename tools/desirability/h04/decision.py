"""H04 decision rule: the card's WIN / LOSS / INCONCLUSIVE, computed from a paired score table.

    python3 -I tools/desirability/h04/decision.py --selftest

Thresholds, as frozen in `work/state/cards/H04.draft.md` ("Target outcome and decision"), with the
coordinator's fix-round-1 decision on control facts (to be written into the card before any
outcome is read):

- WIN: >= 6 net additional correct answers among the 72 candidate witness answers; neither
  provider has a negative net witness change; controls lose <= 2 of their 72 answers; no critical
  control fact is newly misrepresented; no recurrent new critical-task failure in any SDK; every
  gate passes, the docs-truth gate included. (A one-provider run cannot WIN: halved sample,
  provisional.)
- LOSS: a gate failure (the docs-truth gate included), a repeated substantive regression or a
  newly misrepresented control fact attributable to the candidate, or no useful improvement with
  increased caller obligations.
- INCONCLUSIVE: a smaller gain, provider disagreement, a ceiling, missing oracle/tooling, or a
  blocking regression not (yet) attributed.

Operational definitions (each is reported):

- a *pair* is one (SDK, task, provider, repetition) answered in both arms; a *correction* is
  baseline wrong / candidate right, a *regression* the reverse; net = corrections - regressions
  (= candidate correct - baseline correct over complete pairs);
- *controls lose*: the number of control **regressions** (control pairs right in the baseline and
  wrong in the candidate), never offset by corrections. This is the only rule (fix round 3: the
  former `net` alternative was removed; `decide` takes no rule parameter). The summary's
  `diagnostic_not_the_decision.control_net_loss` (regressions minus corrections, floored at 0) is
  a diagnostic, not the decision, and nothing reads it;
- *recurrent new critical-task failure*: an (SDK, task) with >= `RECURRENT` (2) regressions;
- *recurrent control-fact regression* (card; restored in fix round 2): a control (SDK, task,
  critical fact) that is right in the baseline answer and wrong in the candidate answer of the same
  pair in >= `RECURRENT` (2) pairs, counted per fact whether or not the task already failed for
  another fact. It blocks WIN like a recurrent task failure (INCONCLUSIVE, or LOSS when attributed);
- *newly misrepresented control fact* (coordinator decision, fix round 1): a control (SDK, task,
  critical fact) that passes in **every** baseline-arm answer of its SDK and task (both providers,
  all repetitions) and fails in **at least one** candidate-arm answer. It blocks WIN: the result is
  INCONCLUSIVE, or LOSS when the coordinator attributes it to the candidate docs
  (`attributable="yes"`);
- inputs: `gates` (rustdoc `-D warnings`, doc examples, full gate + CI on the exact candidate,
  independent docs-only review), `docs_truth` ("no critical control fact misrepresented in the
  candidate docs", from the candidate review), `increased_obligations`, `attributable`; each
  `"pass"|"fail"|"unknown"` or `"yes"|"no"|"unknown"`. A failed gate or docs-truth gate is LOSS;
  while either is unknown an otherwise winning table is `WIN_PENDING_GATES`, never WIN.
"""
import argparse
import json
import sys

WIN_NET = 6
CONTROL_LOSS_MAX = 2
RECURRENT = 2
WITNESS_ANSWERS = 72
CONTROL_ANSWERS = 72


def pairs(rows):
    """{(sdk, task, provider, rep): {"baseline": row, "candidate": row}} from score rows
    (`sdk`, `role`, `task`, `provider`, `rep`, `arm`, `pass`, `facts` {fact: bool})."""
    out = {}
    for r in rows:
        out.setdefault((r["sdk"], r["task"], r["provider"], r["rep"]), {})[r["arm"]] = r
    return out


def newly_misrepresented(rows):
    """{"sdk/tN/F": {"baseline_answers", "candidate_failures"}} for control facts that pass in every
    baseline answer of their SDK and task and fail in at least one candidate answer."""
    base, cand = {}, {}
    for r in rows:
        if r["role"] != "control":
            continue
        for f, ok in (r.get("facts") or {}).items():
            k = f"{r['sdk']}/t{r['task']}/{f}"
            (base if r["arm"] == "baseline" else cand).setdefault(k, []).append(bool(ok))
    out = {}
    for k, b in sorted(base.items()):
        c = cand.get(k, [])
        if b and all(b) and c.count(False) >= 1:
            out[k] = {"baseline_answers": len(b), "candidate_failures": c.count(False)}
    return out


def summarize(rows):
    P = pairs(rows)
    role = {r["sdk"]: r["role"] for r in rows}
    providers = sorted({r["provider"] for r in rows})
    s = {"providers": providers, "pairs": 0, "incomplete_pairs": [],
         "witness": {"baseline": 0, "candidate": 0, "corrections": 0, "regressions": 0, "answers": 0},
         "control": {"baseline": 0, "candidate": 0, "corrections": 0, "regressions": 0, "answers": 0},
         "by_provider": {p: {"witness_net": 0, "control_net": 0, "witness_corrections": 0, "witness_regressions": 0}
                         for p in providers},
         "task_regressions": {}, "task_corrections": {}, "control_fact_regressions": {}}
    for key, arms in sorted(P.items()):
        sdk, task, prov, rep = key
        if set(arms) != {"baseline", "candidate"}:
            s["incomplete_pairs"].append(list(key))
            continue
        s["pairs"] += 1
        b, c = bool(arms["baseline"]["pass"]), bool(arms["candidate"]["pass"])
        grp = s["witness"] if role[sdk] == "witness" else s["control"]
        grp["answers"] += 1
        grp["baseline"] += b
        grp["candidate"] += c
        if c and not b:
            grp["corrections"] += 1
            s["task_corrections"][f"{sdk}/t{task}"] = s["task_corrections"].get(f"{sdk}/t{task}", 0) + 1
        if b and not c:
            grp["regressions"] += 1
            s["task_regressions"][f"{sdk}/t{task}"] = s["task_regressions"].get(f"{sdk}/t{task}", 0) + 1
        bp = s["by_provider"][prov]
        net = int(c) - int(b)
        if role[sdk] == "witness":
            bp["witness_net"] += net
            bp["witness_corrections"] += int(c and not b)
            bp["witness_regressions"] += int(b and not c)
        else:
            bp["control_net"] += net
            bf, cf = arms["baseline"].get("facts") or {}, arms["candidate"].get("facts") or {}
            for f, ok in cf.items():
                if not ok and bf.get(f):
                    k = f"{sdk}/t{task}/{f}"
                    s["control_fact_regressions"][k] = s["control_fact_regressions"].get(k, 0) + 1
    for g in ("witness", "control"):
        s[g]["net"] = s[g]["candidate"] - s[g]["baseline"]
    s["control_lost"] = s["control"]["regressions"]  # regression counting: the only rule
    s["control_loss_rule"] = "regressions"
    s["diagnostic_not_the_decision"] = {"control_net_loss": max(0, -s["control"]["net"])}
    s["recurrent_task_regressions"] = {k: v for k, v in s["task_regressions"].items() if v >= RECURRENT}
    s["recurrent_control_fact_regressions"] = {k: v for k, v in s["control_fact_regressions"].items() if v >= RECURRENT}
    s["newly_misrepresented_control_facts"] = newly_misrepresented(rows)
    return s


def decide(rows, gates="unknown", increased_obligations="unknown", attributable="unknown",
           expected_providers=2, docs_truth="unknown"):
    """(verdict, reasons, summary). verdict in WIN, WIN_PENDING_GATES, LOSS, INCONCLUSIVE. Controls
    lose their regressions (no rule parameter: regression counting is the only rule)."""
    s = summarize(rows)
    reasons = []
    if gates == "fail":
        return "LOSS", ["a gate failed"], s
    if docs_truth == "fail":
        return "LOSS", ["docs-truth gate failed: a critical control fact is misrepresented in the candidate docs"], s
    one_provider = len(s["providers"]) < expected_providers
    full = (s["witness"]["answers"] == WITNESS_ANSWERS and s["control"]["answers"] == CONTROL_ANSWERS
            and not s["incomplete_pairs"])
    blocking = {**s["recurrent_task_regressions"], **s["recurrent_control_fact_regressions"],
                **{k: v["candidate_failures"] for k, v in s["newly_misrepresented_control_facts"].items()}}
    if blocking:
        if attributable == "yes":
            return "LOSS", ["a blocking regression attributable to the candidate (recurrent task failure, recurrent "
                            "control-fact regression or newly misrepresented control fact): " + json.dumps(blocking)], s
        if s["recurrent_task_regressions"]:
            reasons.append("recurrent new critical-task failure (investigate; LOSS if attributable): "
                           + json.dumps(s["recurrent_task_regressions"]))
        if s["recurrent_control_fact_regressions"]:
            reasons.append("recurrent control-fact regression (>= 2 pairs; investigate; LOSS if attributable): "
                           + json.dumps(s["recurrent_control_fact_regressions"]))
        if s["newly_misrepresented_control_facts"]:
            reasons.append("newly misrepresented control fact (passes in every baseline answer, fails in a candidate answer; "
                           "investigate; LOSS if attributable): " + json.dumps(s["newly_misrepresented_control_facts"]))
    no_improvement = s["witness"]["net"] <= 0
    if no_improvement and increased_obligations == "yes":
        return "LOSS", ["no useful improvement (witness net %+d) with increased caller obligations" % s["witness"]["net"]], s
    win_conditions = {
        "witness_net>=6": s["witness"]["net"] >= WIN_NET,
        "no_provider_negative": all(v["witness_net"] >= 0 for v in s["by_provider"].values()),
        "control_lost<=2": s["control_lost"] <= CONTROL_LOSS_MAX,
        "no_recurrent_task_regression": not s["recurrent_task_regressions"],
        "no_recurrent_control_fact_regression": not s["recurrent_control_fact_regressions"],
        "no_newly_misrepresented_control_fact": not s["newly_misrepresented_control_facts"],
        "two_providers": not one_provider,
        "complete_sample": full,
    }
    s["win_conditions"] = win_conditions
    if all(win_conditions.values()):
        if gates == "pass" and docs_truth == "pass":
            return "WIN", ["all WIN conditions hold and the gates (docs-truth included) passed"], s
        return "WIN_PENDING_GATES", ["all answer-based WIN conditions hold; gates not yet confirmed "
                                     f"(gates {gates}, docs-truth {docs_truth})"], s
    for k, ok in win_conditions.items():
        if not ok:
            reasons.append(f"WIN condition not met: {k}")
    if one_provider:
        reasons.append("one-provider run: halved sample, provisional, cannot WIN")
    if s["witness"]["baseline"] >= WITNESS_ANSWERS - 2:
        reasons.append("ceiling: baseline witness answers %d/%d" % (s["witness"]["baseline"], s["witness"]["answers"]))
    if 0 < s["witness"]["net"] < WIN_NET:
        reasons.append("smaller gain (witness net %+d)" % s["witness"]["net"])
    nets = [v["witness_net"] for v in s["by_provider"].values()]
    if nets and min(nets) < 0 <= max(nets):
        reasons.append("provider disagreement: " + json.dumps({p: v["witness_net"] for p, v in s["by_provider"].items()}))
    return "INCONCLUSIVE", reasons, s


# ---------------------------------------------------------------- self-test (synthetic tables)

WITNESSES = ("cloudflare-zones", "sentry", "kraken-spot-rest")
CONTROLS = ("nager-holidays", "xero-accounting", "strava")


def synthetic(witness_base=40, witness_gain=None, providers=("claude", "codex"), regress_task=None,
              control_regress=0, control_correct=0, new_fact=(), fact_failing_in_baseline=(), masked_fact_regress=()):
    """A complete paired table: per arm 72 witness + 72 control answers (3 repetitions).

    Witnesses: the first `witness_base` keys pass in both arms (facts F1 = pass, F2 true);
    `witness_gain` {provider: (corrections, regressions)} flips distinct pairs (spread over
    distinct (SDK, task) units unless `regress_task` puts the regressions on one unit first).
    Controls: every answer passes, except Strava repetition 3 in both arms (so Strava's F1 facts
    already fail in some baseline answer); `control_regress` candidate regressions on Strava
    repetitions 1-2 (distinct tasks, F1 fails), `control_correct` corrections on Strava repetition
    3; `new_fact` keys whose candidate answer fails F2, a fact true in every baseline answer;
    `fact_failing_in_baseline` keys whose baseline answer fails F2 too (in the same unit);
    `masked_fact_regress` keys whose task already fails in both arms (F1 false) while F2 is right in
    the baseline answer and wrong in the candidate answer (a fact regression a task count misses)."""
    witness_gain = witness_gain or {p: (0, 0) for p in providers}
    rows = []
    keys_w = [(s, t, p, r) for s in WITNESSES for t in (1, 2, 3, 4) for p in providers for r in (1, 2, 3)]
    keys_c = [(s, t, p, r) for s in CONTROLS for t in (1, 2, 3, 4) for p in providers for r in (1, 2, 3)]
    spread = lambda k: (k[3], k[2], k[0], k[1])  # noqa: E731
    base_w = {k: i < witness_base for i, k in enumerate(keys_w)}
    cand_w = dict(base_w)
    for p, (corr, regr) in witness_gain.items():
        pk = [k for k in keys_w if k[2] == p]
        wrong = sorted([k for k in pk if not base_w[k]], key=spread)
        right = sorted([k for k in pk if base_w[k]], key=spread)
        if regress_task:
            right.sort(key=lambda k: (k[0], k[1]) != regress_task)
        for k in wrong[:corr]:
            cand_w[k] = True
        for k in right[:regr]:
            cand_w[k] = False
    for k in keys_w:
        for arm, v in (("baseline", base_w[k]), ("candidate", cand_w[k])):
            rows.append({"sdk": k[0], "role": "witness", "task": k[1], "provider": k[2], "rep": k[3], "arm": arm,
                         "pass": v, "facts": {"F1": v, "F2": True}})
    facts = {}
    for k in keys_c:
        ok = not (k[0] == "strava" and k[3] == 3)
        facts[(k, "baseline")] = {"F1": ok, "F2": True}
        facts[(k, "candidate")] = {"F1": ok, "F2": True}
    regress_keys = sorted([k for k in keys_c if k[0] == "strava" and k[3] in (1, 2)], key=lambda k: (k[3], k[2], k[1]))
    for k in regress_keys[:control_regress]:
        facts[(k, "candidate")]["F1"] = False
    for k in sorted([k for k in keys_c if k[0] == "strava" and k[3] == 3], key=lambda k: (k[2], k[1]))[:control_correct]:
        facts[(k, "candidate")]["F1"] = True
    for k in new_fact:
        facts[(k, "candidate")]["F2"] = False
    for k in fact_failing_in_baseline:
        facts[(k, "baseline")]["F2"] = False
    for k in masked_fact_regress:
        facts[(k, "baseline")] = {"F1": False, "F2": True}
        facts[(k, "candidate")] = {"F1": False, "F2": False}
    for k in keys_c:
        for arm in ("baseline", "candidate"):
            f = facts[(k, arm)]
            rows.append({"sdk": k[0], "role": "control", "task": k[1], "provider": k[2], "rep": k[3], "arm": arm,
                         "pass": all(f.values()), "facts": f})
    return rows


def selftest():
    checks = []

    def case(name, rows, want, **kw):
        got, reasons, s = decide(rows, **kw)
        checks.append({"case": name, "want": want, "got": got, "ok": got == want, "reasons": reasons,
                       "witness_net": s["witness"]["net"], "control_lost": s["control_lost"]})

    g = lambda a, b: {"claude": a, "codex": b}  # noqa: E731
    ok_gates = {"gates": "pass", "docs_truth": "pass"}
    # witness net around 6
    case("net +5 (3+2 corrections)", synthetic(witness_gain=g((3, 0), (2, 0))), "INCONCLUSIVE", **ok_gates)
    case("net +6 (3+3)", synthetic(witness_gain=g((3, 0), (3, 0))), "WIN", **ok_gates)
    case("net +7 (4+3)", synthetic(witness_gain=g((4, 0), (3, 0))), "WIN", **ok_gates)
    case("net +6, gates unknown", synthetic(witness_gain=g((3, 0), (3, 0))), "WIN_PENDING_GATES", docs_truth="pass")
    case("net +6, gates pass, docs-truth unknown", synthetic(witness_gain=g((3, 0), (3, 0))), "WIN_PENDING_GATES", gates="pass")
    case("net +6, gates fail", synthetic(witness_gain=g((3, 0), (3, 0))), "LOSS", gates="fail", docs_truth="pass")
    case("net +6, docs-truth gate fails", synthetic(witness_gain=g((3, 0), (3, 0))), "LOSS", gates="pass", docs_truth="fail")
    # provider net around 0
    case("provider nets +6/0", synthetic(witness_gain=g((6, 0), (1, 1))), "WIN", **ok_gates)
    case("provider nets +7/-1", synthetic(witness_gain=g((7, 0), (0, 1))), "INCONCLUSIVE", **ok_gates)
    # control losses around 2 (on facts that already fail in a baseline answer)
    case("control regressions 2", synthetic(witness_gain=g((3, 0), (3, 0)), control_regress=2), "WIN", **ok_gates)
    case("control regressions 3", synthetic(witness_gain=g((3, 0), (3, 0)), control_regress=3), "INCONCLUSIVE", **ok_gates)
    # the official verdict counts control regressions, never offset by corrections (fix round 3):
    # 3 regressions + 3 corrections is control net 0 but 3 lost answers
    rows3 = synthetic(witness_gain=g((3, 0), (3, 0)), control_regress=3, control_correct=3)
    case("control regressions 3, 3 corrections (net 0): regressions counted", rows3, "INCONCLUSIVE", **ok_gates)
    _, _, s3 = decide(rows3, **ok_gates)
    checks.append({"case": "official control loss is the regression count (3), not the net loss (0)",
                   "want": "3 lost, net diagnostic 0", "got": f"{s3['control_lost']} lost, net diagnostic "
                   f"{s3['diagnostic_not_the_decision']['control_net_loss']}",
                   "ok": s3["control_lost"] == s3["control"]["regressions"] == 3
                   and s3["diagnostic_not_the_decision"]["control_net_loss"] == 0
                   and not s3["win_conditions"]["control_lost<=2"], "reasons": [],
                   "witness_net": s3["witness"]["net"], "control_lost": s3["control_lost"]})
    try:
        decide(rows3, control_loss_rule="net", **ok_gates)
        refused = False
    except TypeError:
        refused = True
    checks.append({"case": "decide() has no control-loss rule parameter (a net rule cannot be selected)",
                   "want": "TypeError", "got": "TypeError" if refused else "accepted", "ok": refused, "reasons": [],
                   "witness_net": s3["witness"]["net"], "control_lost": s3["control_lost"]})
    # newly misrepresented control fact (coordinator decision, fix round 1)
    one = [("nager-holidays", 1, "claude", 1)]
    case("single control fact regression (fact passes in every baseline answer)",
         synthetic(witness_gain=g((3, 0), (3, 0)), new_fact=one), "INCONCLUSIVE", **ok_gates)
    case("single control fact regression, attributable", synthetic(witness_gain=g((3, 0), (3, 0)), new_fact=one), "LOSS",
         attributable="yes", **ok_gates)
    case("single control fact regression, gates unknown", synthetic(witness_gain=g((3, 0), (3, 0)), new_fact=one), "INCONCLUSIVE")
    case("control fact failing in candidate but also in one baseline answer (not newly misrepresented)",
         synthetic(witness_gain=g((3, 0), (3, 0)), new_fact=one, fact_failing_in_baseline=[("nager-holidays", 1, "codex", 2)]),
         "WIN", **ok_gates)
    # recurrent control-fact regression (restored, fix round 2): the reviewer's counterexample, two
    # regressions of one control fact masked by task failures that already exist, plus a pre-existing
    # baseline failure of that fact (so the single-failure rule does not apply)
    masked = [("nager-holidays", 2, "claude", 1), ("nager-holidays", 2, "codex", 1)]
    pre = [("nager-holidays", 2, "claude", 2)]
    case("2 masked control-fact regressions + a baseline failure of that fact",
         synthetic(witness_gain=g((3, 0), (3, 0)), masked_fact_regress=masked, fact_failing_in_baseline=pre), "INCONCLUSIVE",
         **ok_gates)
    case("2 masked control-fact regressions + a baseline failure of that fact, attributable",
         synthetic(witness_gain=g((3, 0), (3, 0)), masked_fact_regress=masked, fact_failing_in_baseline=pre), "LOSS",
         attributable="yes", **ok_gates)
    case("1 masked control-fact regression + a baseline failure of that fact",
         synthetic(witness_gain=g((3, 0), (3, 0)), masked_fact_regress=masked[:1], fact_failing_in_baseline=pre), "WIN",
         **ok_gates)
    # recurrent new critical-task failure (2 regressions on one witness sdk/task)
    case("1 regression on a task", synthetic(witness_gain=g((4, 1), (3, 0)), regress_task=("sentry", 1)), "WIN", **ok_gates)
    case("2 regressions on one task", synthetic(witness_gain=g((5, 2), (3, 0)), regress_task=("sentry", 1)), "INCONCLUSIVE",
         **ok_gates)
    case("2 regressions on one task, attributable", synthetic(witness_gain=g((5, 2), (3, 0)), regress_task=("sentry", 1)),
         "LOSS", attributable="yes", **ok_gates)
    # no improvement with increased obligations
    case("net 0, obligations up", synthetic(), "LOSS", increased_obligations="yes", **ok_gates)
    case("net 0, obligations unknown", synthetic(), "INCONCLUSIVE", **ok_gates)
    case("net -1, obligations up", synthetic(witness_gain=g((0, 1), (0, 0))), "LOSS", increased_obligations="yes")
    # one provider: cannot WIN
    case("one provider, net +6", synthetic(witness_base=20, witness_gain={"claude": (6, 0)}, providers=("claude",)),
         "INCONCLUSIVE", **ok_gates)
    # incomplete sample
    rows = synthetic(witness_gain=g((3, 0), (3, 0)))
    case("missing one candidate answer", [r for r in rows if not (r["sdk"] == "strava" and r["task"] == 4 and r["rep"] == 3
                                                                  and r["provider"] == "codex" and r["arm"] == "candidate")],
         "INCONCLUSIVE", **ok_gates)
    # ceiling
    case("ceiling (baseline 72/72)", synthetic(witness_base=72), "INCONCLUSIVE", **ok_gates)
    ok = all(c["ok"] for c in checks)
    return ok, checks


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--selftest", action="store_true")
    a = ap.parse_args()
    if a.selftest:
        ok, checks = selftest()
        for c in checks:
            print(("ok  " if c["ok"] else "FAIL") + f" {c['case']}: want {c['want']}, got {c['got']} "
                  f"(witness net {c['witness_net']:+d}, control lost {c['control_lost']})")
        print(f"{sum(c['ok'] for c in checks)}/{len(checks)} decision cases as expected")
        sys.exit(0 if ok else 1)
    ap.print_help()


if __name__ == "__main__":
    main()
