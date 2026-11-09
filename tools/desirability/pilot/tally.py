"""Pilot scoring and decision support: pass rates, regressions, reproducibility, a generic
threshold evaluator, and the public summary.

Rows (one per task x arm x slot x repetition): `task, kind, sdk, class, role, group, arm, slot,
rep, verdict` (pass | fail | invalid) and an optional `fingerprint` (the normalised evidence).
A **unit** is (task, slot): slot "" for the deterministic kinds (compile, request, stage), and
"<provider>/r<rep>" for reader answers (each session is its own unit; repetitions are samples,
not reproductions). A unit's verdict on an arm is its lowest repetition's.

- pass rates per arm x SDK x class (and per arm, kind, role), invalid units counted apart;
- regressions (pass at the baseline, fail on the arm) and corrections, per candidate arm, by unit;
  a unit invalid on either side is unpaired and listed;
- reproducibility: a deterministic unit run more than once must give the same verdict and the
  same fingerprint in every repetition; otherwise it is irreproducible;
- thresholds: a per-card JSON (the card stays the authority; README "Thresholds") evaluated per
  candidate arm: rules of type `trigger` (the outcome fires when the rule holds; checked in the
  card's precedence order) and `require` (all must hold for WIN); a rule over an external fact
  that is not given is unknown, and an unknown rule that could change the verdict makes it
  `<verdict>_PENDING`.
"""
import json
import operator

OPS = {"<": operator.lt, "<=": operator.le, "==": operator.eq, "!=": operator.ne, ">=": operator.ge, ">": operator.gt}
FIELDS = ("task", "kind", "sdk", "class", "role", "group", "provider")
METRICS = ("pass", "total", "pass_rate", "baseline_pass_rate", "pass_rate_gain", "corrections", "regressions", "net",
           "irreproducible", "invalid", "fact")
OUTCOMES = ("WIN", "LOSS", "INCONCLUSIVE", "KEEP_CURRENT")


def provider_of(row):
    return row.get("provider") or (row["slot"].split("/")[0] if row.get("slot") else "-")


def units(rows, arm):
    """{(task, slot): {"verdict", "row", "reps": {rep: (verdict, fingerprint)}}} on one arm."""
    out = {}
    for r in rows:
        if r["arm"] != arm:
            continue
        u = out.setdefault((r["task"], r.get("slot", "")), {"reps": {}, "row": r})
        u["reps"][r.get("rep", 1)] = (r["verdict"], json.dumps(r.get("fingerprint"), sort_keys=True))
        if r.get("rep", 1) <= min(u["reps"]):
            u["row"] = r
    for u in out.values():
        u["verdict"] = u["reps"][min(u["reps"])][0]
        u["irreproducible"] = len(set(u["reps"].values())) > 1
    return out


def match(row, filt):
    for k, want in (filt or {}).items():
        v = provider_of(row) if k == "provider" else row.get(k, "-")
        if isinstance(want, dict):
            neg = want.get("not")
            if v in (neg if isinstance(neg, list) else [neg]):
                return False
        elif v not in (want if isinstance(want, list) else [want]):
            return False
    return True


def rate(p, n):
    return round(p / n, 4) if n else None


def metric(name, arm_u, base_u, filt):
    a = {k: u for k, u in arm_u.items() if match(u["row"], filt)}
    b = {k: u for k, u in base_u.items() if match(u["row"], filt)}
    judged = [u for u in a.values() if u["verdict"] != "invalid"]
    bjudged = [u for u in b.values() if u["verdict"] != "invalid"]
    paired = [k for k in a if k in b and a[k]["verdict"] != "invalid" and b[k]["verdict"] != "invalid"]
    corr = sum(1 for k in paired if b[k]["verdict"] == "fail" and a[k]["verdict"] == "pass")
    regr = sum(1 for k in paired if b[k]["verdict"] == "pass" and a[k]["verdict"] == "fail")
    p, n = sum(u["verdict"] == "pass" for u in judged), len(judged)
    bp, bn = sum(u["verdict"] == "pass" for u in bjudged), len(bjudged)
    vals = {"pass": p, "total": n, "pass_rate": rate(p, n), "baseline_pass_rate": rate(bp, bn),
            "pass_rate_gain": None if not n or not bn else round(p / n - bp / bn, 4),
            "corrections": corr, "regressions": regr, "net": corr - regr,
            "irreproducible": sum(u["irreproducible"] for u in a.values()),
            "invalid": sum(u["verdict"] == "invalid" for u in a.values())}
    return vals[name]


def summary(rows, baseline):
    arms = sorted({r["arm"] for r in rows}, key=lambda x: (x != baseline, x))
    U = {a: units(rows, a) for a in arms}
    out = {"arms": arms, "baseline": baseline, "tables": {}, "candidates": {}}
    for a in arms:
        t = {}
        for (task, slot), u in U[a].items():
            r = u["row"]
            for key in (f"{r['sdk']}|{r['class']}", f"*|{r['kind']}", f"role|{r.get('role', '-')}", "*|*"):
                c = t.setdefault(key, {"pass": 0, "n": 0, "invalid": 0})
                if u["verdict"] == "invalid":
                    c["invalid"] += 1
                else:
                    c["n"] += 1
                    c["pass"] += u["verdict"] == "pass"
        out["tables"][a] = dict(sorted(t.items()))
        out["candidates"].setdefault(a, {})["irreproducible"] = sorted(f"{k[0]}{'@' + k[1] if k[1] else ''}"
                                                                      for k, u in U[a].items() if u["irreproducible"])
        out["candidates"][a]["invalid"] = sorted(f"{k[0]}{'@' + k[1] if k[1] else ''}"
                                                 for k, u in U[a].items() if u["verdict"] == "invalid")
    for a in arms:
        if a == baseline:
            continue
        reg, cor, unpaired = [], [], []
        for k, u in U[a].items():
            b = U[baseline].get(k)
            name = f"{k[0]}{'@' + k[1] if k[1] else ''}"
            if b is None or "invalid" in (b["verdict"], u["verdict"]):
                unpaired.append(name)
            elif b["verdict"] == "pass" and u["verdict"] == "fail":
                reg.append({"unit": name, "class": u["row"]["class"], "sdk": u["row"]["sdk"]})
            elif b["verdict"] == "fail" and u["verdict"] == "pass":
                cor.append({"unit": name, "class": u["row"]["class"], "sdk": u["row"]["sdk"]})
        out["candidates"][a].update({"regressions": sorted(reg, key=lambda x: x["unit"]),
                                     "corrections": sorted(cor, key=lambda x: x["unit"]), "unpaired": sorted(unpaired)})
    return out, U


# ---------------------------------------------------------------- thresholds

def check_thresholds(th):
    probs = []
    if not isinstance(th, dict) or set(th) - {"format", "card", "precedence", "otherwise", "rules", "note"} or th.get("format") != 1:
        return ["thresholds: {format: 1, card, precedence?, otherwise?, rules, note?}"]
    prec = th.get("precedence", ["LOSS", "INCONCLUSIVE", "KEEP_CURRENT"])
    if not set(prec) <= set(OUTCOMES) - {"WIN"}:
        probs.append("precedence: outcomes other than WIN")
    if th.get("otherwise", "INCONCLUSIVE") not in OUTCOMES:
        probs.append("otherwise: an outcome")
    ids = set()
    for r in th.get("rules", []):
        rid = r.get("id")
        if rid in ids or not isinstance(rid, str):
            probs.append(f"rule {rid!r}: a unique id")
        ids.add(rid)
        if set(r) - {"id", "outcome", "type", "metric", "filter", "per", "quantifier", "op", "value", "name", "note"}:
            probs.append(f"rule {rid}: unknown keys {sorted(set(r) - {'id', 'outcome', 'type', 'metric', 'filter', 'per', 'quantifier', 'op', 'value', 'name', 'note'})}")
        if r.get("type") not in ("trigger", "require") or r.get("outcome") not in OUTCOMES:
            probs.append(f"rule {rid}: type trigger|require and an outcome")
        if r.get("type") == "require" and r.get("outcome") != "WIN":
            probs.append(f"rule {rid}: `require` rules are WIN conditions")
        if r.get("type") == "trigger" and r.get("outcome") not in prec:
            probs.append(f"rule {rid}: its outcome is not in the precedence list")
        if r.get("metric") not in METRICS or r.get("op") not in OPS or "value" not in r:
            probs.append(f"rule {rid}: metric in {METRICS}, op in {sorted(OPS)}, value")
        if r.get("metric") == "fact" and (not r.get("name") or "per" in r or "filter" in r):
            probs.append(f"rule {rid}: a fact rule has a name and no filter/per")
        if "per" in r and r["per"] not in FIELDS:
            probs.append(f"rule {rid}: per one of {FIELDS}")
        q = r.get("quantifier", "all")
        if "per" in r and not (q in ("all", "any", "none") or (isinstance(q, list) and len(q) == 3 and q[0] == "count" and q[1] in OPS)):
            probs.append(f"rule {rid}: quantifier all|any|none|[\"count\", op, n]")
        if "filter" in r and (not isinstance(r["filter"], dict) or set(r["filter"]) - set(FIELDS)):
            probs.append(f"rule {rid}: filter keys {FIELDS}")
    return probs


def eval_rule(r, arm_u, base_u, facts):
    """(state True/False/None, value(s))."""
    op = OPS[r["op"]]
    if r["metric"] == "fact":
        v = facts.get(r["name"])
        return (None, None) if v is None else (op(v, r["value"]), v)
    if "per" not in r:
        v = metric(r["metric"], arm_u, base_u, r.get("filter"))
        return (None if v is None else op(v, r["value"])), v
    key = r["per"]
    groups = sorted({provider_of(u["row"]) if key == "provider" else u["row"].get(key, "-")
                     for u in list(arm_u.values()) + list(base_u.values()) if match(u["row"], r.get("filter"))})
    vals = {g: metric(r["metric"], arm_u, base_u, {**(r.get("filter") or {}), key: g}) for g in groups}
    hits = [g for g, v in vals.items() if v is not None and op(v, r["value"])]
    q = r.get("quantifier", "all")
    if q == "all":
        state = bool(groups) and len(hits) == len(groups)
    elif q == "any":
        state = bool(hits)
    elif q == "none":
        state = not hits
    else:
        state = OPS[q[1]](len(hits), q[2])
    return state, vals


def decide(th, arm_u, base_u, facts):
    """(verdict, [rule states]) for one candidate arm."""
    states = []
    for r in th["rules"]:
        s, v = eval_rule(r, arm_u, base_u, facts)
        states.append({"id": r["id"], "outcome": r["outcome"], "type": r["type"], "holds": s, "value": v})
    prec = th.get("precedence", ["LOSS", "INCONCLUSIVE", "KEEP_CURRENT"])
    unknown_above = []
    for o in prec:
        trig = [s for s in states if s["type"] == "trigger" and s["outcome"] == o]
        if any(s["holds"] is True for s in trig):
            return (o + ("_PENDING" if unknown_above else "")), states
        unknown_above += [s["id"] for s in trig if s["holds"] is None]
    req = [s for s in states if s["type"] == "require"]
    if any(s["holds"] is False for s in req):
        v = th.get("otherwise", "INCONCLUSIVE")
    elif any(s["holds"] is None for s in req):
        return "WIN_PENDING", states
    else:
        v = "WIN"
    return (v + ("_PENDING" if unknown_above else "")), states


# ---------------------------------------------------------------- reports

def public_summary(summ, verdicts, set_meta):
    """Counts, task ids and classes only: no task text, code, answers, diagnostics or reasons."""
    return {"set": set_meta, "baseline": summ["baseline"], "arms": summ["arms"],
            "pass_rates": summ["tables"],
            "candidates": {a: {"regressions": [{"unit": x["unit"], "class": x["class"]} for x in c.get("regressions", [])],
                               "corrections": [{"unit": x["unit"], "class": x["class"]} for x in c.get("corrections", [])],
                               "unpaired": c.get("unpaired", []), "irreproducible": c["irreproducible"],
                               "invalid": c["invalid"]} for a, c in summ["candidates"].items()},
            "thresholds": {a: {"verdict": v, "rules": [{"id": s["id"], "outcome": s["outcome"], "type": s["type"],
                                                         "holds": s["holds"], "value": s["value"]} for s in st]}
                           for a, (v, st) in verdicts.items()}}


def public_md(pub):
    L = [f"# Pilot summary: {pub['set'].get('name', '?')}", "",
         f"Task set hash `{pub['set'].get('hash', '?')}`; baseline arm `{pub['baseline']}`. Tasks are named by id and class only.", ""]
    for a in pub["arms"]:
        L += [f"## Arm `{a}`", "", "| SDK or group | class | pass/n | invalid |", "|---|---|---|---|"]
        for k, c in pub["pass_rates"][a].items():
            s, cl = k.split("|", 1)
            L.append(f"| {s} | {cl} | {c['pass']}/{c['n']} | {c['invalid']} |")
        c = pub["candidates"].get(a, {})
        L += ["", f"Irreproducible: {', '.join(c.get('irreproducible', [])) or 'none'}",
              f"Invalid: {', '.join(c.get('invalid', [])) or 'none'}"]
        if a != pub["baseline"]:
            L += [f"Regressions: {', '.join(x['unit'] + ' (' + x['class'] + ')' for x in c.get('regressions', [])) or 'none'}",
                  f"Corrections: {', '.join(x['unit'] + ' (' + x['class'] + ')' for x in c.get('corrections', [])) or 'none'}",
                  f"Unpaired: {', '.join(c.get('unpaired', [])) or 'none'}"]
            th = pub["thresholds"].get(a)
            if th:
                L += ["", f"**Threshold evaluation (the card decides): {th['verdict']}**", "",
                      "| rule | outcome | type | holds | value |", "|---|---|---|---|---|"]
                L += [f"| {s['id']} | {s['outcome']} | {s['type']} | {s['holds']} | {json.dumps(s['value'])} |" for s in th["rules"]]
        L.append("")
    return "\n".join(L) + "\n"
