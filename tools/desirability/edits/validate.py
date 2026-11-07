"""I4 task validation: each hidden oracle must fail before and pass after its reference patch.

    python3 -I tools/desirability/edits/validate.py --scratch WT [--task SDK/KIND ..] [--all]
        [--bank DIR] [--logs DIR] [--old-oracle REV]
    python3 -I tools/desirability/edits/validate.py --write-hash [--bank DIR]
    python3 -I tools/desirability/edits/validate.py --report-only [--bank DIR]

The bank is sealed outside the repository (`--bank`, default `<WS>/work/sealed/i4-bank`, layout
`tasks/<sdk>/<kind>/`: `meta.json`, `oracle.rs`, `reference.patch`, e4 `omission.patch` /
`partial.patch`, `controls/*.patch`); the repository keeps `tasks/<sdk>/<kind>/task.md`, the
non-revealing `meta.json` fields (checked against the sealed copy) and `BANK.sha256`, the SHA-256
of the bank's canonical listing (`run.bank_hash`; `--write-hash` rewrites it).

`WT` is a throwaway `git worktree add --detach` checkout at the base commit (never the committed
tree). For every task, in `WT`:

1. reset `WT` (`git checkout -- .`, `git clean -fdq examples .cargo`);
2. `git apply --check` the task's `reference.patch` (must apply at the base);
3. copy `oracle.rs` to `examples/<sdk>-sdk/tests/oracle_<kind>.rs` and run
   `cargo test -p <pkg> --test oracle_<kind> -- --format pretty`: it must not pass;
4. apply `reference.patch` and run `cargo test -p <pkg> --no-fail-fast` (the oracle, every existing
   test target and the doctests): every target must pass, and the oracle's own section must report
   every oracle test named in its source as executed and `ok` (`run.oracle_verdict`), with the
   oracle binary's `--list` naming exactly those tests;
5. **harness control**: the same tree plus a `.cargo/config.toml` whose runner is `/usr/bin/true`
   (cargo then exits 0 without running anything): the oracle verdict must not be a pass;
6. reset `WT`.

**Controls** (`meta.json` `controls`, else `omission.patch` alone) are patches at the base. A
known-bad control must be rejected by the oracle; the run records which stage catches it (the SDK
build, the SDK's existing tests, the caller's compile, the oracle at run time, or nothing). A
`valid variant` control (`"expect": "pass"`) is a behaviour-preserving alternative solution that
the oracle and the whole suite must accept. Kinds:

- `total omission` (e4): the reference minus its classification hunks (`omission.patch`);
- `partial omission` (e4): the classification present but wrong or shared (`partial.patch`);
- `weakness` (any kind): an incomplete provider change that an earlier oracle accepted;
- `valid variant`: a correct solution spelled differently from the reference.

`--old-oracle REV` also runs each control against the oracle as committed at `REV` (from the
repository history, `tools/desirability/edits/tasks/<sdk>/<kind>/oracle.rs`).

Results are merged into `<bank>/validation.json` (one entry per task, the latest run wins, with the
bank hash it ran on) and `<bank>/VALIDATION.md` is regenerated from it (plus
`<bank>/VALIDATION_NOTES.md`). Nothing answer-bearing is written into the repository. Cargo runs
one command at a time with the caller's `CARGO_TARGET_DIR` / `CARGO_BUILD_JOBS`. Run Python with
`-I`.
"""
import argparse
import datetime
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.dont_write_bytecode = True
sys.path.insert(0, HERE)
import run as R  # noqa: E402  (the shared bank, parsing and verdict helpers)


def sh(cmd, cwd, log=None, timeout=3600):
    r = subprocess.run(cmd, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
                       timeout=timeout)
    out = r.stdout
    if log:
        with open(log, "w", encoding="utf-8") as f:
            f.write("$ " + " ".join(cmd) + "\n" + out)
    return r.returncode, out


def reset(wt):
    sh(["git", "checkout", "--", "."], wt)
    sh(["git", "clean", "-fdq", "--", "examples", ".cargo"], wt)
    shutil.rmtree(os.path.join(wt, ".cargo"), ignore_errors=True)


def patch_stats(patch):
    files, hunks, cur = {}, 0, None
    for ln in open(patch, encoding="utf-8"):
        if ln.startswith("+++ b/"):
            cur = ln[6:].strip()
            files[cur] = 0
        elif ln.startswith("@@") and cur:
            files[cur] += 1
            hunks += 1
    return {"files": files, "hunks": hunks}


def run_oracle(wt, pkg, test_name, names, log, listing=True):
    """The oracle alone: `--format pretty`, then (if it compiled) `--list`; `run.oracle_verdict`."""
    code, out = sh(["cargo", "test", "-p", pkg, "--test", test_name, "--", "--format", "pretty"], wt, log)
    listed = None
    if listing and "could not compile" not in out:
        lcode, ltext = sh(["cargo", "test", "-p", pkg, "--test", test_name, "--", "--list"], wt,
                          log.replace(".log", ".list.log"))
        listed = R.parse_listing(ltext) if lcode == 0 else []
    v = R.oracle_verdict(names, out, code, listed)
    if not v["pass"]:
        v["first_error"] = R.first_error(out)
    return v


def suite(out, code):
    rows = R.targets(out)
    return rows, {"built": "could not compile" not in out,
                  "passed": code == 0 and bool(rows) and all(r["ok"] for r in rows),
                  "tests_passed": sum(r["passed"] for r in rows),
                  "tests_failed": sum(r["failed"] for r in rows)}


def strip_tests(rows):
    return [{k: v for k, v in r.items() if k != "tests"} for r in rows]


def validate(wt, task, bank, logs, old_oracle_rev=None):
    sdk, kind = task.split("/")
    t = R.load_task(task, bank)
    meta, d, names = t["meta"], t["dir"], t["oracle_tests"]
    pkg, sdk_dir = meta["package"], meta["sdk_dir"]
    patch = t["reference"]
    rec = {"task": task, "package": pkg, "when": R.now(),
           "base": sh(["git", "rev-parse", "HEAD"], wt)[1].strip(),
           "reference": patch_stats(patch), "oracle_tests": names,
           "oracle_sha256": hashlib.sha256(open(os.path.join(d, "oracle.rs"), "rb").read()).hexdigest()}
    os.makedirs(logs, exist_ok=True)
    stem = os.path.join(logs, f"{sdk}.{kind}")
    reset(wt)
    code, out = sh(["git", "apply", "--check", patch], wt)
    rec["patch_applies"] = code == 0
    if code:
        rec["error"] = out.strip()
        rec["verdict"] = "invalid: reference.patch does not apply"
        return rec
    test_name = f"oracle_{kind}"
    dest = os.path.join(wt, sdk_dir, "tests", f"{test_name}.rs")
    oracle = os.path.join(d, "oracle.rs")
    shutil.copy(oracle, dest)
    rec["before"] = run_oracle(wt, pkg, test_name, names, stem + ".before.log", listing=False)
    code, out = sh(["git", "apply", patch], wt)
    if code:
        rec["verdict"] = "invalid: reference.patch failed to apply after copying the oracle"
        rec["error"] = out.strip()
        reset(wt)
        return rec
    code, out = sh(["cargo", "test", "-p", pkg, "--no-fail-fast"], wt, stem + ".after.log")
    rows, summary = suite(out, code)
    target = f"tests/{test_name}.rs"
    part = R.section(out, target)
    part_ok = bool(R.RESULT_LINE.search(part)) and "test result: ok." in part
    lcode, ltext = sh(["cargo", "test", "-p", pkg, "--test", test_name, "--", "--list"], wt,
                      stem + ".after.list.log")
    oracle_v = R.oracle_verdict(names, part, 0 if part_ok else 1, R.parse_listing(ltext) if lcode == 0 else [])
    after = {"status": "passed" if summary["passed"] else "failed", "targets": strip_tests(rows),
             "oracle": [r for r in strip_tests(rows) if r["target"] == target],
             "oracle_verdict": oracle_v, **summary}
    if not summary["passed"]:
        after["first_error"] = R.first_error(out)
    rec["after"] = after
    ok = rec["before"]["status"] != "passed" and summary["passed"] and oracle_v["pass"]
    rec["verdict"] = "kept" if ok else "invalid"
    if ok:
        # Harness control: a no-op cargo runner makes `cargo test` exit 0 without running a test.
        os.makedirs(os.path.join(wt, ".cargo"), exist_ok=True)
        with open(os.path.join(wt, ".cargo", "config.toml"), "w", encoding="utf-8") as f:
            f.write(R.NOOP_RUNNER_CONFIG)
        hv = run_oracle(wt, pkg, test_name, names, stem + ".noop-runner.log")
        rec["harness_control"] = {"what": "reference + .cargo/config.toml runner = /usr/bin/true",
                                  "cargo_exit": hv["exit"], "status": hv["status"],
                                  "caught": not hv["pass"]}
        if hv["pass"]:
            rec["verdict"] = "invalid: the no-op runner control scored as an oracle pass"
    reset(wt)
    controls = meta.get("controls")
    if controls is None and os.path.exists(os.path.join(d, "omission.patch")):
        controls = [{"patch": "omission.patch", "kind": "total omission",
                     "what": "the reference minus its classification hunks"}]
    rec["controls"] = []
    for c in (controls or []) if ok else []:
        cpatch = os.path.join(d, c["patch"])
        name = os.path.splitext(os.path.basename(c["patch"]))[0]
        row = {"file": c["patch"], "kind": c["kind"], "what": c.get("what", ""),
               "expect": c.get("expect", "fail")}
        row.update(control_stage(wt, pkg, cpatch, oracle, dest, test_name, names, f"{stem}.{name}",
                                 row["expect"]))
        if old_oracle_rev:
            row["old_oracle"] = old_oracle_status(wt, pkg, cpatch, task, old_oracle_rev, dest,
                                                  test_name, f"{stem}.{name}")
        rec["controls"].append(row)
        reset(wt)
        if row["stage"] in ("not caught", "invalid patch", "rejected (a valid variant failed)"):
            rec["verdict"] = f"invalid: control {c['patch']}: {row['stage']}"
    # Kept under its old key for the e4 stage table.
    for row in rec["controls"]:
        if row["kind"] == "total omission":
            rec["omission"] = row
    cls = meta.get("classification") or {}
    if "omission" in rec and cls.get("observed_omission_stage"):
        rec["omission_stage_matches_meta"] = cls["observed_omission_stage"].get("stage") == rec["omission"]["stage"]
    return rec


def old_oracle_status(wt, pkg, patch, task, rev, dest, test_name, stem):
    """The oracle as committed at `rev` against the control `patch` (did it catch it?)."""
    sdk, kind = task.split("/")
    code, text = sh(["git", "show", f"{rev}:tools/desirability/edits/tasks/{sdk}/{kind}/oracle.rs"], wt)
    if code:
        return {"status": "no oracle at " + rev}
    reset(wt)
    code, out = sh(["git", "apply", patch], wt)
    if code:
        return {"status": "invalid patch"}
    with open(dest, "w", encoding="utf-8") as f:
        f.write(text)
    v = run_oracle(wt, pkg, test_name, R.oracle_test_names(text), stem + ".old-oracle.log")
    reset(wt)
    return {"rev": rev, "status": v["status"], "caught": not v["pass"],
            **({"first_error": v.get("first_error")} if not v["pass"] else {})}


def control_stage(wt, pkg, patch, oracle, dest, test_name, names, stem, expect):
    """Which stage catches the known-bad edit `patch`, or whether a valid variant is accepted."""
    out = {"patch": patch_stats(patch)}
    code, text = sh(["git", "apply", patch], wt)
    if code:
        return {"stage": "invalid patch", "error": text.strip()}
    code, text = sh(["cargo", "test", "-p", pkg, "--no-fail-fast"], wt, stem + ".control-existing.log")
    _, summary = suite(text, code)
    out["existing"] = summary
    shutil.copy(oracle, dest)
    out["oracle"] = run_oracle(wt, pkg, test_name, names, stem + ".control-oracle.log")
    status = out["oracle"]["status"]
    if expect == "pass":
        out["stage"] = ("accepted (valid variant)" if summary["passed"] and out["oracle"]["pass"]
                        else "rejected (a valid variant failed)")
    elif not summary["built"]:
        out["stage"] = "compiler (SDK build)"
    elif not summary["passed"]:
        out["stage"] = "existing tests"
    elif status == "compile_error":
        out["stage"] = "compiler at the caller (hidden oracle compile)"
    elif not out["oracle"]["pass"]:
        out["stage"] = "hidden oracle at run time only"
    else:
        out["stage"] = "not caught"
    return out


def write_report(results, bank, digest):
    rows = []
    for task in sorted(results):
        r = results[task]
        b = r.get("before", {})
        a = r.get("after", {})
        ov = a.get("oracle_verdict", {})
        h = r.get("harness_control", {})
        rows.append(
            f"| `{task}` | {r['verdict']} | {b.get('status', '-')} | {a.get('status', '-')} | "
            f"{len(ov.get('reported_ok', []))}/{len(r.get('oracle_tests', []))} | "
            f"{a.get('tests_passed', '-')}/{a.get('tests_failed', '-')} | "
            f"{'caught (' + h['status'] + ', cargo exit ' + str(h['cargo_exit']) + ')' if h else '-'} | "
            f"{r['reference']['hunks']} in {len(r['reference']['files'])} files | {r['when']} |")
    bank_hashes = sorted({r.get("bank_sha256", "-") for r in results.values()})
    text = (
        "# I4 task validation (sealed record)\n\n"
        "Generated by `validate.py` from `validation.json`, next to the sealed bank (not in the\n"
        "repository): the tables are observed runs; the notes mark anything inferred.\n"
        f"Bank hash of these runs: {', '.join(f'`{h}`' for h in bank_hashes)} (current bank: `{digest}`).\n\n"
        "Each task ran in a throwaway worktree at the base commit: the hidden oracle copied to\n"
        "`examples/<sdk>-sdk/tests/oracle_<kind>.rs` must **not pass** without the reference patch,\n"
        "and the package's whole suite (`cargo test -p <pkg> --no-fail-fast`: the oracle, every\n"
        "existing test target, doctests) must **pass** with `reference.patch` applied, the oracle's\n"
        "section reporting every oracle test of its source as executed and `ok` (and its `--list`\n"
        "naming exactly those). *Harness control*: the same tree with a `.cargo/config.toml` no-op\n"
        "runner (`/usr/bin/true`); cargo exits 0, and the oracle verdict must not be a pass.\n\n"
        "| Task | Verdict | Oracle before | Suite after | Oracle tests ok | Tests passed/failed after | Harness control | Reference hunks | Run (UTC) |\n"
        "|---|---|---|---|---|---|---|---|---|\n" + "\n".join(rows) + "\n")
    om = [(t, results[t]["omission"]) for t in sorted(results) if "omission" in results[t]]
    if om:
        text += ("\n## e4: which stage catches an omitted classification (known-bad edit)\n\n"
                 "`omission.patch` is the reference patch minus its classification hunks. Observed, per\n"
                 "task: does the SDK still build, do its existing tests still pass, and does the hidden\n"
                 "oracle catch it (at compile or at run time).\n\n"
                 "| Task | Omission hunks | SDK builds | Existing tests pass | Oracle | Detecting stage | Matches meta.json |\n"
                 "|---|---|---|---|---|---|---|\n")
        for t, o in om:
            e = o.get("existing", {})
            text += (f"| `{t}` | {o['patch']['hunks']} | {e.get('built')} | {e.get('passed')} "
                     f"({e.get('tests_passed')}/{e.get('tests_failed')}) | {o.get('oracle', {}).get('status')} | "
                     f"{o['stage']} | {results[t].get('omission_stage_matches_meta', '-')} |\n")
    ctl = [(t, c) for t in sorted(results) for c in results[t].get("controls", [])]
    if ctl:
        text += ("\n## Controls: every known-bad one must fail the oracle, every valid variant pass\n\n"
                 "Each control is a patch at the base (`meta.json` `controls`). *Old oracle* is the oracle\n"
                 "as committed at the named revision (`--old-oracle`), run against the same control.\n\n"
                 "| Task | Control | Kind | Expect | What it does | SDK builds | Existing tests | Oracle | Stage | Old oracle |\n"
                 "|---|---|---|---|---|---|---|---|---|---|\n")
        for t, c in ctl:
            e = c.get("existing", {})
            old = c.get("old_oracle", {})
            text += (f"| `{t}` | `{c['file']}` | {c['kind']} | {c.get('expect', 'fail')} | {c.get('what', '')} | "
                     f"{e.get('built')} | {e.get('passed')} ({e.get('tests_passed')}/{e.get('tests_failed')}) | "
                     f"{c.get('oracle', {}).get('status')} | {c['stage']} | "
                     f"{(old.get('status', 'not run') + (' @' + old['rev'][:8] if old.get('rev') else ''))} |\n")
    notes = os.path.join(bank, "VALIDATION_NOTES.md")
    if os.path.exists(notes):
        text += "\n" + open(notes, encoding="utf-8").read()
    open(os.path.join(bank, "VALIDATION.md"), "w", encoding="utf-8").write(text)


def write_hash(bank):
    digest, nfiles, ntasks = R.bank_hash(bank)
    rel = os.path.relpath(bank, R.WS)
    with open(R.BANK_HASH_FILE, "w", encoding="utf-8") as f:
        f.write(f"{digest}  I4 sealed bank: {ntasks} tasks, {nfiles} files under <WS>/{rel}/tasks/; "
                f"SHA-256 of the canonical listing (run.bank_hash: one line `<sha256>  <path>` per file, "
                f"sorted by path)\n")
    return digest, nfiles, ntasks


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--scratch")
    ap.add_argument("--task", action="append", default=[])
    ap.add_argument("--all", action="store_true")
    ap.add_argument("--bank", default=R.DEFAULT_BANK)
    ap.add_argument("--logs", default=None)
    ap.add_argument("--report-only", action="store_true")
    ap.add_argument("--write-hash", action="store_true", help="rewrite BANK.sha256 from the bank and stop")
    ap.add_argument("--old-oracle", default=None, help="REV: also run each control against the oracle at REV")
    a = ap.parse_args()
    bank = os.path.abspath(a.bank)
    if a.write_hash:
        print(json.dumps(dict(zip(("sha256", "files", "tasks"), write_hash(bank)))))
        return
    results_path = os.path.join(bank, "validation.json")
    results = json.load(open(results_path)) if os.path.exists(results_path) else {}
    digest, nfiles, ntasks = R.bank_hash(bank)
    recorded = R.recorded_bank_hash()
    if not a.report_only:
        if not a.scratch:
            sys.exit("--scratch WT is required")
        tasks = a.task
        root = os.path.join(bank, "tasks")
        if a.all:
            tasks = sorted(f"{s}/{k}" for s in os.listdir(root) for k in os.listdir(os.path.join(root, s)))
            public = sorted(f"{s}/{k}" for s in os.listdir(R.TASKS) for k in os.listdir(os.path.join(R.TASKS, s)))
            if public != tasks:
                sys.exit(f"the repository's tasks/ and the sealed bank differ: "
                         f"{sorted(set(public) ^ set(tasks))}")
        logs = a.logs or os.path.join(os.path.dirname(os.path.abspath(a.scratch)), "scratch-i4-logs")
        for t in tasks:
            rec = validate(os.path.abspath(a.scratch), t, bank, logs, a.old_oracle)
            rec["bank_sha256"] = digest
            rec["bank_sha256_matches_file"] = digest == recorded
            results[t] = rec
            json.dump(results, open(results_path, "w"), indent=1, sort_keys=True)
            print(json.dumps({"task": t, "verdict": rec["verdict"],
                              "before": rec.get("before", {}).get("status"),
                              "after": rec.get("after", {}).get("status"),
                              "oracle_ok": len(rec.get("after", {}).get("oracle_verdict", {}).get("reported_ok", [])),
                              "oracle_tests": len(rec.get("oracle_tests", [])),
                              "harness": rec.get("harness_control", {}).get("status"),
                              "controls": [(c["file"], c["stage"], c.get("old_oracle", {}).get("status"))
                                           for c in rec.get("controls", [])]}), flush=True)
    write_report(results, bank, digest)


if __name__ == "__main__":
    main()
