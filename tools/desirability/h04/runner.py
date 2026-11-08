"""H04 runner: a seeded, balanced, interleaved schedule of reader sessions; the per-arm prompts; the
sessions themselves (I3 `probes/run.py` mechanics).

    # 1. schedule: 2 providers x 6 SDKs x 3 repetitions x 2 arms = 72 sessions
    python3 -I tools/desirability/h04/runner.py schedule --baseline SHA --candidate SHA --seed N --out RUNS
    # 2. prompts: materials of each arm's commit (materials.py), the bank's task text
    python3 -I tools/desirability/h04/runner.py prompts --bank SEALED --runs RUNS --materials DIR [--cleared FILE]
    # 3. sessions, in schedule order, resumable (finished sessions are skipped)
    python3 -I tools/desirability/h04/runner.py sessions --bank SEALED --runs RUNS --materials DIR --claude-model MODEL \
        [--parallel 4] [--cleared FILE]       # (the first session freezes the run: prompts/FROZEN.json)
    # then: python3 -I tools/desirability/h04/scorer.py --bank SEALED --runs RUNS --out DIR

RUNS holds the bank's task text inside the prompts: keep it outside the repository, next to the
sealed bank (for example `work/sealed/h04-runs/<run name>`).

Schedule. One session per (arm, provider, SDK, repetition), four answers per session. The two arms
of one (provider, SDK, repetition) form a pair and are scheduled next to each other; which arm
goes first alternates over the repetitions (B-C-B or C-B-C, chosen so each provider has as many
baseline-first as candidate-first pairs: 9 and 9), and the 36 pairs are shuffled with the seed.
Sessions are numbered in that order (`NN-<arm>-<provider>-<sdk>-r<rep>`). Arms are blind to the
reader: prompts differ only in the documentation text.

Prompts. Per arm and SDK: `reader.md` filled with the crate name, the cheat sheet
(`docs/CHEATSHEET.md` at the **bank's commit**, the same for both arms), the public types and the
generated documentation rendered from **that arm's commit** by `materials.py` (one extraction for
both arms), and the four tasks' text from the sealed bank (its hash must equal `BANK.sha256`).
The public types must be byte-identical across arms (a docs-only candidate), else `prompts`
refuses, and so it does when the example filter is not format-invariant on an arm's text or the
arms were rendered with different toolchains. `prompts/manifest.json` records every prompt's
SHA-256, sizes and the materials' diagnostics (rendered characters and words, per endpoint type,
filter counts, a leak diagnostic); `prompts/coverage.json` compares the reader-visible contract
statements of the two arms per SDK and category.

Structural gate (fix rounds 5-6, the coordinator's decisions; it does not depend on recognising
code and normalises nothing). `prompts` stores, per arm and SDK, the exact materials each prompt
holds (`RUNS/prompts/inputs/<arm>/<sdk>.types.txt` and `.docs.txt`: the public types and the
rendered docs with the README part, as `build_prompt` inserts them; the prompts are built from
these files) and records the SHA-256 of each SDK's exact candidate materials and an overall
digest. The review evidence per SDK is a complete unified diff of baseline vs candidate materials
(`RUNS/prompts/review/<sdk>.diff`: deletions, both ends of a move, whitespace and table changes
visible; it rebuilds the candidate exactly) with the broad detector's tokens on added lines as an
annotation (`<sdk>.highlights.txt`). `prompts` and `sessions` refuse unless `--cleared FILE` names
`overall <digest>` and `<sdk> <digest>` for every SDK whose candidate materials differ from the
baseline's in any byte (the coordinator writes it from the candidate's independent review of the
diffs); any later change, a deletion or whitespace included, invalidates it; an SDK with
byte-identical materials needs none. On refusal `prompts` writes only the manifest, the stored
inputs and the review files and removes any earlier prompt file.

Prompt integrity (fix round 7). `build_prompt` substitutes the five placeholders in one pass over
the template only, so inserted text (a `{TASKS}` written in the docs) stays literal. The overall
clearance digest also binds the sealed bank's hash, the template, the cheat sheet and every final
prompt's SHA-256 (`gate_context`). The cheat sheet always comes from the sealed bank's own commit,
never from the manifest. `sessions` reads every prompt file once, verifies those strings against
the sealed sources (`verify_run_inputs`: bank, commit, template and sheet identity, the stored
inputs' digests, each prompt rebuilt from them, the clearance; also when no clearance is needed)
and hands the same strings to the workers, which never reopen a file. A run is frozen when its
first session starts (`prompts/FROZEN.json`: the manifest's and every prompt's SHA-256): `prompts`
then refuses (a changed prompt needs a new run directory), resuming refuses a manifest or prompt
that is not the frozen one and any session that recorded another prompt's SHA-256, and the scorer
refuses an unfrozen run or a session whose prompt hash differs from the manifest's.
Approved inputs and frozen schedules (fix round 8). The sealed bank's `bank.json` records the
approved reader template's and cheat sheet's SHA-256 (`approved`, covered by the bank hash).
`prompts` and `sessions` re-render both arms from their commits (`sessions --materials DIR`) and
require a clearance whenever the template or the cheat sheet differ from the approved digests or
either arm's stored materials differ from that fresh render (`deviations`), as well as when the
arms differ; the byte-identical exemption holds only without any deviation. `schedule` refuses on
a started run, even with `--force`; `FROZEN.json` also records the schedule's SHA-256 and the arms;
resuming and the scorer refuse a schedule, arms or manifest arms that are not the frozen ones and
a recorded session that the schedule lacks or whose arm, commit, provider, SDK or repetition
differ from it. `gate-selftest` checks the gate on synthetic materials and, with `--e2e-out`, runs
`schedule`, `prompts`, the `sessions` preflight and the scorer's run check end to end on a
synthetic candidate.

Sessions (I3 `probes/run.py` mechanics, `parse_answer` reused; isolation as
`tools/desirability/edits/run.py` isolates the I4 agents; fix round 1):
- both providers run inside bubblewrap: the root read-only, the whole home directory an empty
  tmpfs (the repository, worktrees, workspace, sealed bank, runs, ~/.claude, ~/.codex hidden),
  private /tmp, /var/tmp, /run (DNS resolver bound back), new PID and IPC namespaces, an empty
  environment plus an explicit list, network kept. Bound back: the provider binary (read-only),
  the attempt's directory (a random name under `$CARGO_TARGET_DIR/h04-readers/`) mounted at an
  opaque path `$HOME/<random>` with neutral subdirectories (`w` the empty working directory, `h`
  a fresh provider home, `o` the output file), so no arm, commit, SDK, session or run identifier
  reaches a reader-visible path (fix round 2), and the provider's credential file into that home;
- claude: `claude -p --output-format stream-json --verbose --model M --tools "" --setting-sources ""
  --strict-mcp-config --safe-mode --no-session-persistence --disable-slash-commands
  --system-prompt S`, `CLAUDE_CONFIG_DIR` = the fresh home with `.credentials.json` bound;
  observed model from the assistant events and `modelUsage`; effort not visible;
- codex: `codex exec --strict-config -m gpt-6.1-sol -c model_reasoning_effort="xhigh" -c
  daybreak=false -s read-only -C <empty dir> --skip-git-repo-check --json -o last.txt`,
  `CODEX_HOME` = the fresh home with `auth.json` bound and a minimal `config.toml` (model, effort,
  the owner's service tier, tool features off, `[skills] include_instructions = false`,
  `[skills.bundled] enabled = false`, `[agents] enabled = false`); observed model, effort,
  sandbox, cwd and multi-agent version from the rollout's `turn_context`;
- every attempt is checked: Claude's init event (cwd = the empty directory, no tools, no MCP
  servers, no skills, no slash commands), Codex's rollout (cwd, read-only sandbox, no skills
  instructions, no multi-agent role, no AGENTS.md instructions) and, for both, no trace of the
  owner's instruction files and no identifying string (arm, commit, SDK, session, run, workspace)
  in the reader-visible environment (Claude's init event; Codex's environment context and cwd). A tool call or a failed check discards the attempt (kept as
  `attempt1/`) and runs it once more; a second one records the session as failed (its answer is
  null and scores 0). First answers are kept; there is no recovery round. Requested and observed
  model/effort and the isolation checks go to `meta.json`.
- `probe` runs the same sandbox with a shell (what is visible) and each provider once with a
  dummy prompt (never the bank's text), searches the whole saved transcripts for identifying
  strings, and writes `probe.json`.
Run Python with `-I`.
"""
import argparse
import concurrent.futures as cf
import glob
import hashlib
import json
import os
import random
import re
import secrets
import shutil
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "..", "probes"))
import common as C  # noqa: E402
import run as R  # noqa: E402  (probes/run.py: the probes directory is first on sys.path)
import materials as M  # noqa: E402
import scorer as SC  # noqa: E402

SYSTEM = ("You answer questions about a Rust SDK using only the materials in the user message. "
          "You have no tools. Reply with exactly one JSON object and nothing else.")
PROVIDERS = ("claude", "codex")
REPS = 3
TEMPLATE = os.path.join(HERE, "reader.md")


def make_schedule(baseline, candidate, seed, providers=PROVIDERS, reps=REPS, sdks=None):
    sdks = list(sdks or M.SDKS)
    rng = random.Random(seed)
    pairs = []
    for p in providers:
        firsts = []
        for i, s in enumerate(sdks):
            pattern = ["baseline", "candidate", "baseline"] if i % 2 == 0 else ["candidate", "baseline", "candidate"]
            pattern = (pattern * reps)[:reps]
            firsts += [(p, s, r + 1, pattern[r]) for r in range(reps)]
        pairs += firsts
    rng.shuffle(pairs)
    sessions = []
    for p, s, rep, first in pairs:
        for arm in (first, "candidate" if first == "baseline" else "baseline"):
            idx = len(sessions)
            sessions.append({"id": f"{idx:02d}-{arm}-{p}-{s}-r{rep}", "order": idx, "arm": arm, "provider": p,
                             "sdk": s, "rep": rep, "commit": baseline if arm == "baseline" else candidate})
    return {"format": 1, "seed": seed, "arms": {"baseline": baseline, "candidate": candidate},
            "providers": list(providers), "reps": reps, "sdks": sdks, "sessions": sessions,
            "balance": {p: {"baseline_first": sum(1 for x in pairs if x[0] == p and x[3] == "baseline"),
                            "candidate_first": sum(1 for x in pairs if x[0] == p and x[3] == "candidate")} for p in providers}}


def cmd_schedule(a):
    if run_started(a.out):
        sys.exit("refused: sessions of this run have started; its schedule is frozen (even with --force; use a new run directory)")
    base, cand = M.full_sha(a.baseline), M.full_sha(a.candidate)
    sched = make_schedule(base, cand, a.seed, a.providers, a.reps)
    path = os.path.join(a.out, "schedule.json")
    if os.path.exists(path) and not a.force:
        sys.exit(f"{path} exists (use --force to replace a schedule before any session ran)")
    C.write_json(path, sched)
    print(json.dumps({"sessions": len(sched["sessions"]), "balance": sched["balance"], "path": path}))


def cheatsheet(commit):
    r = subprocess.run(["git", "-C", C.REPO, "show", f"{commit}:docs/CHEATSHEET.md"], capture_output=True, text=True)
    if r.returncode:
        sys.exit(f"cannot read docs/CHEATSHEET.md at {commit}: {r.stderr}")
    return r.stdout


PLACEHOLDER = re.compile(r"\{(CRATE|CHEATSHEET|TYPES|DOCS|TASKS)\}")


def build_prompt(template, crate, sheet, types, docs, tasks_text):
    """The reader prompt: the template's text below its `---` line with its five placeholders
    replaced in one pass over the template only (fix round 7): inserted content (the cheat sheet,
    the types, the docs, the tasks) is kept literally, so a `{TASKS}` or `{CRATE}` written in the
    documentation stays that text."""
    body = template.split("\n---\n", 1)[1].lstrip("\n")
    values = {"CRATE": crate, "CHEATSHEET": sheet.strip(), "TYPES": types.strip(), "DOCS": docs.strip(),
              "TASKS": tasks_text.strip()}
    return PLACEHOLDER.sub(lambda m: values[m.group(1)], body).strip() + "\n"


def tasks_text(bank, sdk):
    return "\n\n".join(bank["tasks"][(sdk, n)]["text"].strip() for n in range(1, SC.TASKS_PER_SDK + 1))


def sha(text):
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


INPUTS = os.path.join("prompts", "inputs")  # the exact materials each prompt holds, per arm and SDK
REVIEW = os.path.join("prompts", "review")  # the review evidence per SDK (diff and highlights)


def input_paths(runs, arm, sdk):
    d = os.path.join(runs, INPUTS, arm)
    return os.path.join(d, f"{sdk}.types.txt"), os.path.join(d, f"{sdk}.docs.txt")


def stored_materials(runs, arm, sdk):
    """(types, docs) exactly as `prompts` inserted them into this arm's prompt."""
    tp, dp = input_paths(runs, arm, sdk)
    with open(tp, encoding="utf-8", newline="") as f:
        types = f.read()
    with open(dp, encoding="utf-8", newline="") as f:
        docs = f.read()
    return types, docs


FROZEN = os.path.join("prompts", "FROZEN.json")  # written when the first session starts: the run's inputs are final


def build_prompts(runs, bank, template, sheet, sdks):
    """{"<arm>/<sdk>": prompt} built from the stored inputs (the hashed materials)."""
    return {f"{arm}/{sdk}": build_prompt(template, M.SDKS[sdk][2], sheet, *stored_materials(runs, arm, sdk), tasks_text(bank, sdk))
            for arm in ("baseline", "candidate") for sdk in sdks}


def gate_context(bank_hash, template, sheet, prompts):
    """What the overall clearance digest binds besides the candidate's materials (fix round 7):
    the sealed bank's hash, the template, the cheat sheet and every final prompt's SHA-256."""
    return {"bank_hash": bank_hash, "template_sha256": sha(template), "cheatsheet_sha256": sha(sheet),
            **{f"prompt:{k}": sha(v) for k, v in prompts.items()}}


def fresh_materials(sched, materials_dir):
    """{(arm, sdk): (types, docs)} as `build_prompt` inserts them, rendered now from each arm's
    commit (materials.py: the commit's own target directory, provenance-checked rustdoc)."""
    out = {}
    for arm, commit in sched["arms"].items():
        for sdk in sched["sdks"]:
            mat = M.materials(commit, sdk, materials_dir)
            out[(arm, sdk)] = (mat["types"].strip(), mat["docs"].strip())
    return out


def deviations(bank, template, sheet, runs, fresh, sdks):
    """Fix round 8: what differs from the sealed approval (bank.json `approved`) and a fresh
    render, each requiring a clearance: the template, the cheat sheet, either arm's stored
    materials."""
    ap = bank["meta"].get("approved") or {}
    out = []
    if sha(template) != ap.get("reader_template_sha256"):
        out.append("the reader template is not the approved one")
    if sha(sheet) != ap.get("cheatsheet_sha256"):
        out.append("the cheat sheet is not the approved one")
    for arm in ("baseline", "candidate"):
        for sdk in sdks:
            try:
                stored = stored_materials(runs, arm, sdk)
            except OSError:
                out.append(f"{arm}/{sdk}: stored materials missing")
                continue
            if (arm, sdk) not in fresh or stored != fresh[(arm, sdk)]:
                out.append(f"{arm}/{sdk}: the stored materials differ from a fresh render of the arm's commit")
    return out


def verify_run_inputs(runs, bank_dir, cleared, manifest, snapshots, template, sched, fresh):
    """Every check between the sealed sources and the prompts the readers get (fix rounds 6-7);
    [] when the prompts may be built or run. Trusted sources: the sealed bank (its hash in
    `BANK.sha256`; the cheat sheet of the bank's own commit, never the manifest's field), the
    template text given, the stored inputs. `snapshots`: the prompt texts read once (the ones the
    workers will send). Checked: the manifest names this bank, its commit, this template and this
    sheet; the stored inputs match the manifest's materials digests; each snapshot is exactly the
    prompt rebuilt from the stored inputs and has the manifest's SHA-256; the overall digest binds
    the candidate's materials, the bank, the template, the sheet and every final prompt
    (`gate_context`); the manifest's arms are the schedule's; when any SDK's candidate materials
    differ from the baseline's, or the template, the cheat sheet or either arm's stored materials
    differ from the sealed approval and the fresh render `fresh` (`deviations`, fix round 8),
    `cleared` names that overall digest and every changed SDK's digest exactly
    (`materials.digest_gate`)."""
    try:
        bank = SC.load_bank(bank_dir)
    except SystemExit as e:
        return [f"the sealed bank: {e}"]
    bank_hash, commit = SC.bank_hash(bank_dir)[0], bank["meta"]["commit"]
    sheet = cheatsheet(commit)
    problems = []
    if manifest.get("arms") != sched["arms"]:
        problems.append("the manifest's arms are not the schedule's")
    if manifest.get("bank_hash") != bank_hash:
        problems.append("the manifest names another bank")
    if manifest.get("bank_commit") != commit:
        problems.append("the manifest's bank commit is not the sealed bank's")
    if manifest.get("template_sha256") != sha(template):
        problems.append("the template differs from the one the prompts were built with")
    if manifest.get("cheatsheet_sha256") != sha(sheet):
        problems.append("the cheat sheet differs from the sealed bank commit's")
    if "materials_digests" not in manifest or "materials_digest_overall" not in manifest:
        return problems + ["the manifest has no exact-materials digests (built before fix round 6): re-run `prompts`"]
    sdks = list(manifest["materials_digests"])
    try:
        base = {sdk: M.reader_materials(*stored_materials(runs, "baseline", sdk)) for sdk in sdks}
        cand = {sdk: M.reader_materials(*stored_materials(runs, "candidate", sdk)) for sdk in sdks}
        rebuilt = build_prompts(runs, bank, template, sheet, sdks)
    except OSError as e:
        return problems + [f"stored prompt inputs missing ({e})"]
    rec = manifest["materials_digests"]
    if any(M.materials_digest(cand[k]) != rec[k]["candidate"] or M.materials_digest(base[k]) != rec[k]["baseline"] for k in rec):
        problems.append("the stored prompt inputs differ from the manifest's digests")
    if set(snapshots) != set(rebuilt) or set(manifest.get("prompts", {})) != set(rebuilt):
        problems.append("the prompt set differs from the stored inputs'")
    for key in sorted(set(snapshots) & set(rebuilt)):
        if snapshots[key] != rebuilt[key]:
            problems.append(f"{key}: the prompt is not the one built from the hashed materials, template, sheet and tasks")
        elif manifest["prompts"].get(key, {}).get("sha256") != sha(snapshots[key]):
            problems.append(f"{key}: the prompt's SHA-256 is not the manifest's")
    context = gate_context(bank_hash, template, sheet, rebuilt)
    if M.gate_digests(base, cand, context)[2] != manifest["materials_digest_overall"]:
        problems.append("the overall digest does not match the materials, bank, template, sheet and prompts")
    try:
        clearance = M.read_digest_clearance(cleared) if cleared else None
    except (OSError, ValueError) as e:
        return problems + [f"unreadable clearance file: {e}"]
    devs = deviations(bank, template, sheet, runs, fresh, sdks)
    manifest.setdefault("_deviations", devs)
    return problems + M.digest_gate(base, cand, clearance, context, reasons=devs)


def run_started(runs):
    """Whether the run's inputs are frozen: a session has started (`FROZEN`) or left a record."""
    return os.path.exists(os.path.join(runs, FROZEN)) or bool(glob.glob(os.path.join(runs, "sessions", "*", "meta.json")))


def cmd_prompts(a):
    if run_started(a.runs):
        sys.exit("refused: sessions of this run have started; its inputs are frozen (a changed prompt needs a new run directory)")
    bank = SC.load_bank(a.bank)
    sched = C.read_json(os.path.join(a.runs, "schedule.json"))
    template = open(TEMPLATE, encoding="utf-8").read()
    sheet = cheatsheet(bank["meta"]["commit"])  # the sealed bank's own commit
    manifest = {"bank_hash": SC.bank_hash(a.bank)[0], "bank_commit": bank["meta"]["commit"],
                "cheatsheet_sha256": hashlib.sha256(sheet.encode()).hexdigest(), "template_sha256": hashlib.sha256(template.encode()).hexdigest(),
                "arms": sched["arms"], "prompts": {}, "materials": {}}
    rendered = {}
    for arm, commit in sched["arms"].items():
        for sdk in sched["sdks"]:
            rendered[(arm, sdk)] = M.materials(commit, sdk, a.materials)
    for sdk in sched["sdks"]:
        tb, tc = rendered[("baseline", sdk)]["meta"]["types_sha256"], rendered[("candidate", sdk)]["meta"]["types_sha256"]
        if tb != tc and not a.allow_types_diff:
            sys.exit(f"{sdk}: the public types differ between the arms ({tb[:12]} vs {tc[:12]}): not a docs-only candidate")
        for arm in ("baseline", "candidate"):
            if not rendered[(arm, sdk)]["meta"]["format_invariant"]:
                sys.exit(f"{arm}/{sdk}: the example filter is not format-invariant on this text: "
                         f"{rendered[(arm, sdk)]['meta']['format_audit_failures'][:5]}")
        vb = rendered[("baseline", sdk)]["meta"]["rustdoc"]["toolchain"]
        vc = rendered[("candidate", sdk)]["meta"]["rustdoc"]["toolchain"]
        if vb != vc or rendered[("baseline", sdk)]["meta"]["extraction"] != rendered[("candidate", sdk)]["meta"]["extraction"]:
            sys.exit(f"{sdk}: the arms were rendered with different toolchains or extractions ({vb} / {vc}): re-render both")
    # structural gate (fix round 6, the coordinator's decision): bound to the exact materials each
    # prompt holds; the review evidence is a complete diff; no prompt is built until a clearance names
    # the exact-materials digest of every SDK whose candidate materials differ in any byte
    ok_g, gchecks = M.gate_selftest()
    if not ok_g:
        sys.exit("the gate self-test failed: " + json.dumps([c for c in gchecks if not c["ok"]])[:2000])
    for (arm, sdk), mat in rendered.items():
        for path, text in zip(input_paths(a.runs, arm, sdk), (mat["types"].strip(), mat["docs"].strip())):
            os.makedirs(os.path.dirname(path), exist_ok=True)
            with open(path, "w", encoding="utf-8", newline="") as f:
                f.write(text)
    base = {sdk: M.reader_materials(*stored_materials(a.runs, "baseline", sdk)) for sdk in sched["sdks"]}
    cand = {sdk: M.reader_materials(*stored_materials(a.runs, "candidate", sdk)) for sdk in sched["sdks"]}
    prompts = build_prompts(a.runs, bank, template, sheet, sched["sdks"])  # the final prompts, from the hashed bytes
    per, changed, overall = M.gate_digests(base, cand, gate_context(SC.bank_hash(a.bank)[0], template, sheet, prompts))
    os.makedirs(os.path.join(a.runs, REVIEW), exist_ok=True)
    manifest["materials_digests"], manifest["review"] = {}, {}
    for sdk in sched["sdks"]:
        diff = M.review_diff(base[sdk], cand[sdk], sdk)
        if M.apply_diff(base[sdk], diff) != cand[sdk]:
            sys.exit(f"{sdk}: the review diff does not rebuild the candidate's materials")
        notes = M.diff_highlights(diff)
        dpath, hpath = os.path.join(a.runs, REVIEW, f"{sdk}.diff"), os.path.join(a.runs, REVIEW, f"{sdk}.highlights.txt")
        for path, text in ((dpath, diff), (hpath, notes)):
            with open(path, "w", encoding="utf-8", newline="") as f:
                f.write(text)
        manifest["materials_digests"][sdk] = {"baseline": M.materials_digest(base[sdk]), "candidate": per[sdk],
                                              "identical": sdk not in changed}
        manifest["review"][sdk] = {"diff": os.path.relpath(dpath, a.runs), "diff_sha256": M.materials_digest(diff),
                                   "diff_lines": diff.count("\n"), "highlights": os.path.relpath(hpath, a.runs)}
    manifest["materials_digest_overall"] = overall
    for key, text in prompts.items():
        manifest["prompts"][key] = {"path": os.path.join("prompts", key + ".md"), "sha256": sha(text), "chars": len(text)}
    fresh = {(arm, sdk): (mat["types"].strip(), mat["docs"].strip()) for (arm, sdk), mat in rendered.items()}
    problems = verify_run_inputs(a.runs, a.bank, a.cleared, manifest, prompts, template, sched, fresh)
    devs = manifest.pop("_deviations", [])
    manifest["clearance"] = {"file": a.cleared, "file_sha256": C.sha256_file(a.cleared) if a.cleared else None,
                             "changed_sdks": changed, "deviations": devs, "gate_selftest_ok": ok_g, "problems": problems}
    if problems:
        for arm in ("baseline", "candidate"):  # no stale prompt survives a refusal
            for sdk in sched["sdks"]:
                stale = os.path.join(a.runs, "prompts", arm, f"{sdk}.md")
                if os.path.exists(stale):
                    os.remove(stale)
        manifest["prompts"] = {}
        C.write_json(os.path.join(a.runs, "prompts", "manifest.json"), manifest)
        sys.exit("refused: " + ("the candidate's materials differ from the baseline's for " + ", ".join(changed) + "; " if changed else "")
                 + ("deviations from the approved inputs: " + "; ".join(devs) + "; " if devs else "")
                 + "review prompts/review/<sdk>.diff and clear with --cleared FILE naming `overall " + overall
                 + "` and each changed SDK's candidate digest in prompts/manifest.json `materials_digests`: " + "; ".join(problems))
    for (arm, sdk), mat in sorted(rendered.items()):
        path = os.path.join(a.runs, manifest["prompts"][f"{arm}/{sdk}"]["path"])
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf-8", newline="") as f:
            f.write(prompts[f"{arm}/{sdk}"])  # the verified text
        manifest["materials"][f"{arm}/{sdk}"] = {k: mat["meta"][k] for k in mat["meta"] if k not in ("words_per_endpoint_type",)}
        manifest["materials"][f"{arm}/{sdk}"]["words_per_endpoint_type"] = mat["meta"]["words_per_endpoint_type"]
        # leak diagnostic (no values written): a reference answer string of >= 12 characters that
        # appears verbatim in this arm's materials
        flat = mat["docs"] + "\n" + mat["types"]
        hits = [f"t{n}.{field}" for n in range(1, SC.TASKS_PER_SDK + 1)
                for field, val in sorted(bank["tasks"][(sdk, n)]["reference"].items())
                if isinstance(val, str) and len(val) >= 12 and val in flat]
        manifest["materials"][f"{arm}/{sdk}"]["reference_strings_in_materials"] = hits
    # coverage audit: the reader-visible contract statements per category, compared across the arms
    cov = {}
    for sdk in sched["sdks"]:
        b, c = rendered[("baseline", sdk)]["meta"]["coverage"], rendered[("candidate", sdk)]["meta"]["coverage"]
        cov[sdk] = {k: {"baseline": len(b[k]), "candidate": len(c[k]),
                        "only_in_candidate": sorted(set(c[k]) - set(b[k])), "only_in_baseline": sorted(set(b[k]) - set(c[k]))}
                    for k in b}
        manifest["materials"][f"baseline/{sdk}"]["coverage_vs_candidate"] = {
            k: {"baseline": v["baseline"], "candidate": v["candidate"], "only_in_candidate": len(v["only_in_candidate"]),
                "only_in_baseline": len(v["only_in_baseline"])} for k, v in cov[sdk].items()}
        for arm in ("baseline", "candidate"):
            manifest["materials"][f"{arm}/{sdk}"].pop("coverage", None)
    C.write_json(os.path.join(a.runs, "prompts", "coverage.json"), cov)
    C.write_json(os.path.join(a.runs, "prompts", "manifest.json"), manifest)
    for k, v in sorted(manifest["materials"].items()):
        print(json.dumps({"prompt": k, "chars": manifest["prompts"][k]["chars"], "docs_words": v["docs_words"],
                          "types_sha256": v["types_sha256"][:12], "mean_words_per_endpoint_type": v["mean_words_per_endpoint_type"],
                          "reference_strings_in_materials": v["reference_strings_in_materials"]}))


# ---------------------------------------------------------------- isolated reader sessions (fix round 1)
#
# Both providers run inside bubblewrap, as tools/desirability/edits/run.py runs the I4 agents: the
# root file system read-only; the whole home directory an empty tmpfs (this hides ~/projects -- the
# repository, every worktree, the loop workspace, the sealed bank and the runs -- and ~/.claude and
# ~/.codex with the owner's global instructions, skills, plugins, transcripts and rollouts); /tmp,
# /var/tmp and /run private tmpfs (the DNS resolver directory bound back read-only); new PID and IPC
# namespaces; an empty environment plus an explicit list; the network kept (the provider API). Bound
# back: the provider binary (read-only), the session's sandbox directory (an empty working directory
# and a fresh per-session provider home) and the provider's credential file into that home
# (read-write, so a token refresh updates the owner's file). Codex gets a minimal config.toml
# (model, effort, the owner's service tier, tool features off); Claude a fresh CLAUDE_CONFIG_DIR.
# Each session's transcript is then checked: working directory, no tools / no MCP servers offered
# (Claude's init event), sandbox policy and working directory (Codex's turn_context), and no trace
# of the owner's instruction files (their first line) in the transcript or rollout. A failed check
# or a tool call discards the attempt; a second one fails the session.

HOME = os.path.realpath(os.path.expanduser("~"))
BWRAP = "/usr/bin/bwrap"
CODEX_FEATURES_OFF = ("shell_tool", "unified_exec", "view_image", "image_generation", "multi_agent", "multi_agent_v2",
                      "plugins", "apps", "browser_use", "browser_use_external", "computer_use", "hooks", "goals",
                      "sleep_tool", "tool_suggest", "skill_search", "in_app_browser", "memories", "remote_plugin",
                      "plugin_discovery_prefetch", "skill_mcp_dependency_install")
# sandbox directories live under the probe target directory, outside the sealed bank and the runs:
# inside the sandbox only the empty path components leading to the bound directory exist
SANDBOX_ROOT = os.path.join(C.CARGO_TARGET_DIR, "h04-readers")
OWNER_INSTRUCTION_FILES = (os.path.join(HOME, ".claude", "CLAUDE.md"), os.path.join(HOME, ".codex", "AGENTS.md"))


def resolve_binary(name):
    path = shutil.which(name, path=os.path.join(HOME, ".local", "bin") + os.pathsep + os.environ.get("PATH", ""))
    return os.path.realpath(path) if path else None


def owner_codex_setting(key):
    try:
        for ln in open(os.path.join(HOME, ".codex", "config.toml"), encoding="utf-8"):
            if ln.startswith("["):
                break
            m = re.match(rf"^{key}\s*=\s*(.+?)\s*$", ln)
            if m:
                return m.group(1)
    except OSError:
        pass
    return None


def owner_markers():
    """The first non-empty line of each owner instruction file: must not occur in a transcript."""
    out = []
    for p in OWNER_INSTRUCTION_FILES:
        try:
            first = next((ln.strip() for ln in open(p, encoding="utf-8") if ln.strip()), None)
        except OSError:
            first = None
        if first and len(first) >= 12:
            out.append(first)
    return out


def new_box():
    """(host directory, in-sandbox mount point) for one attempt. Both names are random: no arm,
    commit, SDK, session or run identifier reaches a reader-visible path (fix round 2)."""
    token = secrets.token_hex(8)
    host = os.path.join(SANDBOX_ROOT, secrets.token_hex(8))
    os.makedirs(host)
    for d in ("w", "h", "o"):
        os.makedirs(os.path.join(host, d))
    return host, os.path.join(HOME, token)


def provider_setup(provider, box, model, effort, inside=None):
    """(argv[0], read-only binds, (src, dst) read-write binds, env, extra request info). `box` is the
    host directory; `inside` its mount point in the sandbox (the paths the provider sees)."""
    inside = inside or box
    home = os.path.join(box, "h")
    os.makedirs(home, exist_ok=True)
    home_in = os.path.join(inside, "h")
    if provider == "codex":
        exe = resolve_binary("codex")
        release = os.path.dirname(os.path.dirname(exe))  # .../releases/<version>/bin/codex
        auth = os.path.join(HOME, ".codex", "auth.json")
        open(os.path.join(home, "auth.json"), "a").close()
        placeholder = os.path.join(home_in, "auth.json")
        tier = owner_codex_setting("service_tier")
        cfg = [f'model = "{model}"', f'model_reasoning_effort = "{effort}"', "daybreak = false"]
        if tier:
            cfg.append(f"service_tier = {tier}")
        cfg += ["", "[features]"] + [f"{f} = false" for f in CODEX_FEATURES_OFF]
        # no skill instructions and no bundled system skills; no multi-agent role (turn_context
        # multi_agent_version "disabled"); keys checked by `codex exec --strict-config`
        cfg += ["", "[skills]", "include_instructions = false", "", "[skills.bundled]", "enabled = false",
                "", "[agents]", "enabled = false"]
        with open(os.path.join(home, "config.toml"), "w") as f:
            f.write("# Written by runner.py: a fresh per-session CODEX_HOME (no owner instructions, skills, plugins or MCP servers).\n"
                    + "\n".join(cfg) + "\n")
        return exe, [release], [(auth, placeholder)], {"CODEX_HOME": home_in}, {"service_tier": tier, "features_off": list(CODEX_FEATURES_OFF)}
    exe = resolve_binary("claude")
    cred = os.path.join(HOME, ".claude", ".credentials.json")
    open(os.path.join(home, ".credentials.json"), "a").close()
    placeholder = os.path.join(home_in, ".credentials.json")
    return exe, [exe], [(cred, placeholder)], {"CLAUDE_CONFIG_DIR": home_in, "DISABLE_AUTOUPDATER": "1"}, {}


def sandbox_cmd(box, cwd, argv, env, ro=(), rw_pairs=(), inside=None):
    cmd = [BWRAP, "--die-with-parent", "--new-session", "--unshare-pid", "--unshare-ipc",
           "--ro-bind", "/", "/",
           "--tmpfs", HOME,
           "--tmpfs", "/tmp", "--tmpfs", "/var/tmp", "--tmpfs", "/run",
           "--ro-bind-try", "/run/systemd/resolve", "/run/systemd/resolve",
           "--dev", "/dev", "--proc", "/proc",
           "--bind", box, inside or box]
    for p in ro:
        cmd += ["--ro-bind", p, p]
    for src, dst in rw_pairs:
        cmd += ["--bind", src, dst]
    base = {"HOME": HOME, "USER": os.environ.get("USER", "canarit"), "LOGNAME": os.environ.get("USER", "canarit"),
            "PATH": "/usr/local/bin:/usr/bin:/bin", "LANG": "C.UTF-8", "TMPDIR": "/tmp"}
    base.update(env)
    cmd.append("--clearenv")
    for k, v in sorted(base.items()):
        cmd += ["--setenv", k, v]
    return cmd + ["--chdir", cwd, "--"] + list(argv)


ACCOUNT_FIELDS = ("creator_user_id", "creator_account_id", "account_id", "user_id")


def keep_rollout(src, dst):
    """Copy a rollout with the owner's account identifiers (session_meta) withheld (as edits/run.py)."""
    with open(src, encoding="utf-8", errors="replace") as f, open(dst, "w", encoding="utf-8") as g:
        for line in f:
            try:
                rec = json.loads(line)
            except ValueError:
                g.write(line)
                continue
            payload = rec.get("payload")
            if rec.get("type") == "session_meta" and isinstance(payload, dict):
                for k in ACCOUNT_FIELDS:
                    if k in payload:
                        payload[k] = "<withheld>"
                line = json.dumps(rec) + "\n"
            g.write(line)


def run_isolated(provider, prompt, box, out, model, effort, system=SYSTEM, timeout=R.TIMEOUT, inside=None, forbidden=()):
    """One attempt in the sandbox (`box` the host directory, `inside` its opaque mount point).
    Returns the I3-shaped result dict plus `isolation` checks. `forbidden`: identifying strings that
    must not occur in what the reader sees of its environment (cwd, environment context)."""
    inside = inside or box
    cwd = os.path.join(inside, "w")
    exe, ro, rw, env, extra = provider_setup(provider, box, model, effort, inside)
    last_in = os.path.join(inside, "o", "last.txt")
    last_host = os.path.join(box, "o", "last.txt")
    if provider == "claude":
        argv = [exe, "-p", "--output-format", "stream-json", "--verbose", "--model", model, "--tools", "",
                "--setting-sources", "", "--strict-mcp-config", "--safe-mode", "--no-session-persistence",
                "--disable-slash-commands", "--system-prompt", system]
        if effort:
            argv += ["--effort", effort]
    else:
        argv = [exe, "exec", "--strict-config", "-m", model, "-c", f'model_reasoning_effort="{effort}"', "-c", "daybreak=false",
                "-s", "read-only", "-C", cwd, "--skip-git-repo-check", "--json", "-o", last_in]
    cmd = sandbox_cmd(box, cwd, argv, env, ro, rw, inside)
    start = R.now()
    try:
        r = subprocess.run(cmd, input=prompt, capture_output=True, text=True, timeout=timeout)
        stdout, stderr, code = r.stdout, r.stderr, r.returncode
    except subprocess.TimeoutExpired as e:
        stdout = (e.stdout or b"").decode() if isinstance(e.stdout, bytes) else (e.stdout or "")
        stderr, code = "timeout", -1
    end = R.now()
    open(os.path.join(out, "events.jsonl"), "w").write(stdout)
    open(os.path.join(out, "stderr.log"), "w").write(stderr or "")
    markers = owner_markers()
    iso = {"sandbox": "bubblewrap (home and repository hidden, network kept)", "cwd_expected": cwd,
           "owner_instruction_markers_found": [], "checks": {}}
    tool_calls, observed, text, extra_res = [], {}, "", {}
    transcript = stdout
    if provider == "claude":
        models, init, result = set(), {}, {}
        for line in stdout.splitlines():
            try:
                e = json.loads(line)
            except ValueError:
                continue
            if e.get("type") == "system" and e.get("subtype") == "init":
                init = e
            elif e.get("type") == "assistant":
                msg = e.get("message", {})
                if msg.get("model"):
                    models.add(msg["model"])
                for c_ in msg.get("content", []):
                    if c_.get("type") in ("tool_use", "server_tool_use", "mcp_tool_use"):
                        tool_calls.append(c_.get("name"))
            elif e.get("type") == "result":
                result = e
        text = result.get("result") or ""
        usage = result.get("modelUsage") or {}
        observed = {"models": sorted(models | set(usage.keys())), "init_model": init.get("model"), "effort": "not visible",
                    "source": "stream-json assistant message.model, result.modelUsage, system/init"}
        iso["init"] = {k: init.get(k) for k in ("cwd", "tools", "mcp_servers", "permissionMode", "model", "claude_code_version",
                                                "apiKeySource", "slash_commands", "agents", "skills", "plugins", "output_style")}
        visible = json.dumps(init)  # what the session reports about its environment
        iso["checks"] = {"cwd": init.get("cwd") == cwd, "no_tools": init.get("tools") == [],
                         "no_mcp_servers": not init.get("mcp_servers"), "no_skills": not init.get("skills"),
                         "no_slash_commands": not init.get("slash_commands")}
        extra_res = {"num_turns": result.get("num_turns"), "cost_usd": result.get("total_cost_usd"), "usage": result.get("usage")}
    else:
        tid, usage = None, None
        for line in stdout.splitlines():
            try:
                e = json.loads(line)
            except ValueError:
                continue
            if e.get("type") == "thread.started":
                tid = e.get("thread_id")
            elif e.get("type", "").startswith("item."):
                it = e.get("item", {})
                if it.get("type") not in R.TEXT_ITEM_TYPES:
                    tool_calls.append(it.get("type"))
            elif e.get("type") == "turn.completed":
                usage = e.get("usage")
        observed = {"model": "not visible", "effort": "not visible", "sandbox": "not visible", "cwd": None,
                    "rollout": None, "source": "rollout turn_context"}
        hits = glob.glob(os.path.join(box, "h", "sessions", "*", "*", "*", f"*{tid}*.jsonl")) if tid else []
        visible = ""
        if hits and not os.path.islink(hits[0]):
            kept = os.path.join(out, "rollout.jsonl")
            keep_rollout(hits[0], kept)
            observed["rollout"] = os.path.relpath(kept, out)
            transcript = open(hits[0], encoding="utf-8", errors="replace").read()
            rollout_tools = set()
            for line in transcript.splitlines():
                try:
                    rec = json.loads(line)
                except ValueError:
                    continue
                pl = rec.get("payload", {}) if isinstance(rec.get("payload"), dict) else {}
                if rec.get("type") == "turn_context" and observed["model"] == "not visible":
                    sp = pl.get("sandbox_policy")
                    cm = (pl.get("collaboration_mode") or {}).get("settings") or {}
                    observed.update({"model": pl.get("model"), "cwd": pl.get("cwd"), "multi_agent_version": pl.get("multi_agent_version"),
                                     "disabled_plugin_ids": pl.get("disabled_plugin_ids"),
                                     "effort": pl.get("effort") or cm.get("reasoning_effort") or "not visible",
                                     "sandbox": sp.get("type") if isinstance(sp, dict) else sp})
                if rec.get("type") == "response_item" and pl.get("type") == "message":
                    txt = " ".join(c_.get("text", "") for c_ in pl.get("content", []) if isinstance(c_, dict))
                    if "<environment_context>" in txt:
                        visible += txt
                if rec.get("type") in ("session_meta", "turn_context"):
                    visible += " " + str(pl.get("cwd"))
                if rec.get("type") == "response_item" and pl.get("type") in ("function_call", "local_shell_call",
                                                                              "custom_tool_call", "web_search_call"):
                    rollout_tools.add(pl.get("type"))
            tool_calls += sorted(rollout_tools)
        iso["checks"] = {"cwd": observed.get("cwd") == cwd, "sandbox_read_only": observed.get("sandbox") == "read-only",
                         "rollout_found": observed["rollout"] is not None,
                         "no_owner_agents_instructions": "AGENTS.md instructions for" not in transcript,
                         "no_skills_instructions": "<skills_instructions>" not in transcript,
                         "no_multi_agent_role": "<multi_agent_role>" not in transcript and observed.get("multi_agent_version") == "disabled"}
        text = open(last_host, errors="replace").read() if os.path.isfile(last_host) and not os.path.islink(last_host) else ""
        extra_res = {"thread_id": tid, "usage": usage}
    iso["owner_instruction_markers_found"] = [m for m in markers if m in transcript or m in stdout]
    iso["checks"]["no_owner_instructions"] = not iso["owner_instruction_markers_found"]
    # blinding: nothing identifying (arm, commit, SDK, session, run, workspace) in the reader-visible
    # environment (Claude's init event; Codex's environment context and cwd)
    iso["visible_environment"] = visible[:2000]
    iso["identifying_strings_found"] = sorted({f for f in forbidden if f and f.lower() in visible.lower()})
    iso["checks"]["opaque_paths"] = not iso["identifying_strings_found"]
    iso["ok"] = all(iso["checks"].values())
    open(os.path.join(out, "last.txt"), "w").write(text)
    requested = ({"provider": "anthropic/claude-code", "model": model, "effort": effort or "default (not passed)"}
                 if provider == "claude" else
                 {"provider": "openai/codex", "model": model, "effort": effort, "sandbox": "read-only", **extra})
    requested["outer_sandbox"] = "bubblewrap (runner.py sandbox_cmd)"
    return {"cmd": cmd, "start": start, "end": end, "exit": code, "text": text, "tool_calls": tool_calls,
            "observed": observed, "requested": requested, "isolation": iso, **extra_res}


def identifying(s, runs, sched=None):
    """Strings that would identify a session's treatment or provenance if a reader saw them."""
    out = {"baseline", "candidate", "h04", "sealed", "desirability", "concord_desirability_loop", s.get("id", ""),
           os.path.basename(runs), s.get("sdk", "")}
    for c in ((sched or {}).get("arms") or {}).values():
        out |= {c[:7], c[:12]}
    if s.get("commit"):
        out |= {s["commit"][:7], s["commit"][:12]}
    return sorted(x for x in out if x)


def one(job):
    s, prompt_rel, prompt, runs, model, effort, forbidden = job  # the validated text, never reopened
    out = os.path.join(runs, "sessions", s["id"])
    if os.path.isdir(out):
        shutil.rmtree(out)
    os.makedirs(out)
    attempts, res = [], None
    for attempt in (1, 2):
        box, inside = new_box()
        res = run_isolated(s["provider"], prompt, box, out, model, effort, inside=inside, forbidden=forbidden)
        cwd = os.path.join(box, "w")
        leftovers = os.listdir(cwd) if os.path.isdir(cwd) else []
        shutil.rmtree(box, ignore_errors=True)
        bad = bool(res["tool_calls"]) or not res["isolation"]["ok"]
        attempts.append({"attempt": attempt, "start": res["start"], "end": res["end"], "exit": res["exit"],
                         "tool_calls": res["tool_calls"], "isolation": res["isolation"], "workdir_leftovers": leftovers})
        if not bad:
            break
        if attempt == 1:
            keep = os.path.join(out, "attempt1")
            os.makedirs(keep)
            for f in ("events.jsonl", "stderr.log", "last.txt", "rollout.jsonl"):
                if os.path.isfile(os.path.join(out, f)):
                    shutil.move(os.path.join(out, f), os.path.join(keep, f))
    failed = bool(res["tool_calls"]) or not res["isolation"]["ok"]
    why = "session used tools twice" if res["tool_calls"] else "isolation check failed twice"
    answer, err = (None, why) if failed else R.parse_answer(res["text"])
    C.write_json(os.path.join(out, "answer.json"), answer)
    meta = {**{k: s[k] for k in ("id", "order", "arm", "provider", "sdk", "rep", "commit")},
            "prompt": prompt_rel, "prompt_sha256": sha(prompt),
            "requested": res["requested"], "observed": res["observed"], "isolation": res["isolation"], "attempts": attempts,
            "failed": failed, "answer_error": err, "answers": len(answer["answers"]) if answer else 0, "exit": res["exit"],
            "usage": res.get("usage"), "cost_usd": res.get("cost_usd"), "num_turns": res.get("num_turns"),
            "thread_id": res.get("thread_id"), "cmd": [c if c != prompt else "<prompt>" for c in res["cmd"]]}
    C.write_json(os.path.join(out, "meta.json"), meta)
    return meta


PROBE_FORBIDDEN = ("baseline", "candidate", "h04", "sealed", "desirability", "concord_desirability_loop", "734991e9",
                   "cloudflare", "sentry", "kraken", "nager", "xero", "strava", "h04-readers", "h04-runs")


def cmd_probe(a):
    """Isolation probe: the reader sandbox with a shell (what is visible) and, unless --no-providers,
    each provider once with a dummy prompt (never the bank's text): every isolation check, and no
    identifying string anywhere in the saved transcript. Writes <out>/probe.json."""
    os.makedirs(a.out, exist_ok=True)
    report = {"time": R.now(), "markers": [m[:40] for m in owner_markers()], "forbidden": list(PROBE_FORBIDDEN)}
    loop = os.path.dirname(os.path.dirname(os.path.dirname(C.REPO)))  # <WS>/work/wt/<name> -> <WS>
    hidden = [C.REPO, os.path.join(loop, "work", "sealed", "h04-bank", "tasks"), os.path.join(loop, "work", "sealed"),
              os.path.join(loop, "work", "wt"), os.path.join(loop, "work", "baseline"), loop, SANDBOX_ROOT,
              os.path.join(HOME, ".claude", "CLAUDE.md"), os.path.join(HOME, ".codex", "AGENTS.md"),
              os.path.join(HOME, ".codex", "skills"), os.path.join(HOME, ".claude", "skills"), os.path.join(HOME, ".codex", "plugins"),
              os.path.join(HOME, ".claude", "plugins"), os.path.join(HOME, ".claude", "projects"), os.path.join(HOME, ".codex", "sessions"),
              os.path.join(HOME, ".codex", "config.toml"), os.path.join(HOME, ".claude.json")]
    for provider in ("claude", "codex"):
        box, inside = new_box()
        exe, ro, rw, env, _ = provider_setup(provider, box, R.CODEX_MODEL if provider == "codex" else "probe", "xhigh", inside)
        chain, d = [], inside
        while d != "/" and d.startswith(HOME):
            d = os.path.dirname(d)
            chain.append(d)
        probe = ("echo cwd=$(pwd); "
                 + "".join(f"echo ls {d}: $(ls -A {d} | tr '\\n' ' '); " for d in chain)
                 + "for p in " + " ".join(hidden) + "; do if [ -e \"$p\" ]; then echo VISIBLE $p; else echo absent $p; fi; done; "
                 "echo tmp_entries=$(ls -A /tmp | wc -l); echo pid1=$(cat /proc/1/comm); "
                 "echo env=$(env | sort | tr '\\n' ' ')")
        r = subprocess.run(sandbox_cmd(box, os.path.join(inside, "w"), ["/bin/sh", "-c", probe], env, ro, rw, inside),
                           capture_output=True, text=True, timeout=120)
        shutil.rmtree(box, ignore_errors=True)
        lines_ = r.stdout.splitlines()
        report[f"shell_{provider}_layout"] = lines_
        # every listed parent holds only path components of the bound paths (the skeleton bwrap
        # creates for the binds: the opaque session directory, the provider binary, the credential file)
        bound = [inside] + list(ro) + [dst for _, dst in rw]
        skeleton_ok = True
        for l in lines_:
            if not l.startswith("ls "):
                continue
            d, _, entries = l[3:].partition(": ")
            allowed = {os.path.relpath(b, d).split(os.sep)[0] for b in bound if b.startswith(d.rstrip("/") + "/")}
            if not set(entries.split()) <= allowed:
                skeleton_ok = False
        visible_text = " ".join(l for l in lines_ if l.startswith(("cwd=", "ls ", "env="))).lower()
        report[f"shell_{provider}_identifying"] = [f for f in PROBE_FORBIDDEN if f in visible_text]
        report[f"shell_{provider}_skeleton_ok"] = skeleton_ok
        report[f"shell_{provider}_layout_ok"] = (r.returncode == 0 and skeleton_ok and not report[f"shell_{provider}_identifying"]
                                                 and not any(l.startswith("VISIBLE") for l in lines_))
    if not a.no_providers:
        dummy = ("This is an isolation probe, not a task. Reply with exactly this JSON object and nothing else: "
                 '{"answers": [{"task": 0, "probe": "ok"}]}')
        for provider in ("claude", "codex"):
            out = os.path.join(a.out, f"provider-{provider}")
            if os.path.isdir(out):
                shutil.rmtree(out)
            os.makedirs(out)
            box, inside = new_box()
            model = a.claude_model if provider == "claude" else R.CODEX_MODEL
            res = run_isolated(provider, dummy, box, out, model, a.codex_effort if provider == "codex" else None, timeout=600,
                               inside=inside, forbidden=PROBE_FORBIDDEN)
            shutil.rmtree(box, ignore_errors=True)
            ans, err = R.parse_answer(res["text"])
            saved = ""
            for f in ("events.jsonl", "rollout.jsonl", "stderr.log", "last.txt"):
                if os.path.isfile(os.path.join(out, f)):
                    saved += open(os.path.join(out, f), encoding="utf-8", errors="replace").read()
            report[f"provider_{provider}"] = {"exit": res["exit"], "tool_calls": res["tool_calls"], "observed": res["observed"],
                                              "requested": res["requested"], "isolation": res["isolation"], "answer": ans,
                                              "answer_error": err, "stderr_tail": open(os.path.join(out, "stderr.log")).read()[-600:],
                                              "transcript_identifying": [f for f in PROBE_FORBIDDEN if f in saved.lower()]}
    report["ok"] = (all(report.get(f"shell_{p}_layout_ok") for p in ("claude", "codex"))
                    and (a.no_providers or all(report[f"provider_{p}"]["isolation"]["ok"] and not report[f"provider_{p}"]["tool_calls"]
                                               and not report[f"provider_{p}"]["transcript_identifying"]
                                               and report[f"provider_{p}"]["answer"] is not None for p in ("claude", "codex"))))
    C.write_json(os.path.join(a.out, "probe.json"), report)
    print(json.dumps({"ok": report["ok"], **{k: v for k, v in report.items() if k.endswith("_ok") or k.endswith("_identifying")},
                      **{f"{p}_isolation": report.get(f"provider_{p}", {}).get("isolation", {}).get("checks") for p in ("claude", "codex")},
                      **{f"{p}_transcript_identifying": report.get(f"provider_{p}", {}).get("transcript_identifying") for p in ("claude", "codex")}},
                     indent=1))


PLACEHOLDER_LINE = "Literal {TASKS}, {CRATE} and {DOCS} stay as written."


def _build_prompt_round6(template, crate, sheet, types, docs, tasks_text):
    """Round 6's sequential substitution, kept only as the negative control of the placeholder case."""
    body = template.split("\n---\n", 1)[1].lstrip("\n")
    for k, v in (("{CHEATSHEET}", sheet.strip()), ("{TYPES}", types.strip()), ("{DOCS}", docs.strip()),
                 ("{TASKS}", tasks_text.strip())):
        body = body.replace(k, v)
    return body.replace("{CRATE}", crate).strip() + "\n"


def gate_e2e(bank, materials_dir, out):
    """`prompts`, the `sessions` preflight and the scorer's run check end to end on a synthetic
    candidate (no model call, no cargo): both arms read B's rendered materials from
    `materials_dir/<sha12>/`; the candidate arm's docs move one line to the end, get
    `PLACEHOLDER_LINE` and `M.GATE_EXTRA` (filtered) appended. Each case records whether it was
    refused; `ok` compares them with the expected outcomes (fix rounds 6-7)."""
    global TEMPLATE
    commit = C.read_json(os.path.join(bank, "tasks", "bank.json"))["commit"]
    src = os.path.join(materials_dir, M.full_sha(commit)[:12])
    extra = M._render_strings(M.GATE_EXTRA)
    state = {"delete_heading": False}

    def edit(docs):
        lines = docs.split("\n")
        k = next(n for n, x in enumerate(lines) if n > 2 and x.strip() and not x.startswith("#"))
        moved = lines.pop(k)
        if state["delete_heading"]:
            h = next(n for n, x in enumerate(lines) if n > 0 and x.startswith("#"))
            lines.pop(h)
        return "\n".join(lines) + "\n" + moved + "\n" + PLACEHOLDER_LINE + "\n\n" + extra + "\n", moved

    def fake_materials(arm_commit, sdk, _out):
        docs = open(os.path.join(src, f"{sdk}.docs.txt"), encoding="utf-8").read()
        types = open(os.path.join(src, f"{sdk}.types.txt"), encoding="utf-8").read()
        meta = C.read_json(os.path.join(src, f"{sdk}.meta.json"))
        if arm_commit == "synthetic-candidate":
            docs = edit(docs)[0]
        return {"docs": docs, "types": types, "meta": meta}

    def sargs(runs, cleared):
        return argparse.Namespace(runs=runs, bank=bank, cleared=cleared, provider=["codex"], rerun=False, max=None,
                                  claude_model=None, claude_effort=None, codex_model=R.CODEX_MODEL, codex_effort="xhigh",
                                  materials=materials_dir)

    def record(s0, prompt_sha):  # a session record as `one` writes it (the fields the checks read)
        return {"id": s0["id"], "prompt_sha256": prompt_sha, **{k: s0[k] for k in ("arm", "commit", "provider", "sdk", "rep")}}

    def forge(runs, template, docs_edit=None):
        """Rewrites a run's prompts and manifest consistently, as someone with write access could:
        optionally both arms' stored docs of strava edited identically, the prompts rebuilt with
        `template`, every manifest digest recomputed."""
        if docs_edit:
            for arm in ("baseline", "candidate"):
                tp, dp = input_paths(runs, arm, "strava")
                write(dp, docs_edit(open(dp, encoding="utf-8").read()))
        mp = os.path.join(runs, "prompts", "manifest.json")
        man = C.read_json(mp)
        sdks = list(man["materials_digests"])
        sheet = cheatsheet(commit)
        prompts = build_prompts(runs, bk, template, sheet, sdks)
        base = {k: M.reader_materials(*stored_materials(runs, "baseline", k)) for k in sdks}
        cand = {k: M.reader_materials(*stored_materials(runs, "candidate", k)) for k in sdks}
        for k, text in prompts.items():
            write(os.path.join(runs, man["prompts"][k]["path"]), text)
            man["prompts"][k]["sha256"] = sha(text)
        man["template_sha256"] = sha(template)
        man["materials_digests"] = {k: {"baseline": M.materials_digest(base[k]), "candidate": M.materials_digest(cand[k]),
                                        "identical": base[k] == cand[k]} for k in sdks}
        man["materials_digest_overall"] = M.gate_digests(base, cand, gate_context(SC.bank_hash(bank)[0], template, sheet, prompts))[2]
        C.write_json(mp, man)

    def preflight(runs, cleared, template=None):
        try:
            last["why"] = None
            return False, sessions_preflight(sargs(runs, cleared), template)
        except SystemExit as e:
            last["why"] = str(e.code)[:300]
            return bool(e.code), str(e.code)

    def prompts_run(name, runs, candidate, cleared):
        os.makedirs(runs, exist_ok=True)
        C.write_json(os.path.join(runs, "schedule.json"), make_schedule("synthetic-baseline", candidate, 20261108))
        ns = argparse.Namespace(bank=bank, runs=runs, materials=materials_dir, allow_types_diff=False, cleared=cleared)
        try:
            last["why"] = None
            cmd_prompts(ns)
            refused = False
        except SystemExit as e:
            refused = bool(e.code)
            last["why"] = str(e.code)[-300:]
        man = C.read_json(os.path.join(runs, "prompts", "manifest.json"))
        files = sorted(glob.glob(os.path.join(runs, "prompts", "*", "*.md")))
        diffs = {k: open(os.path.join(runs, v["diff"]), encoding="utf-8").read() for k, v in man.get("review", {}).items()}
        return {"case": name, "refused": refused, "prompt_files": len(files)}, man, diffs

    def clearance_file(man, path):
        with open(path, "w", encoding="utf-8") as f:
            f.write("# synthetic clearance (gate self-test)\noverall " + man["materials_digest_overall"] + "\n"
                    + "".join(f"{k} {v['candidate']}\n" for k, v in sorted(man["materials_digests"].items()) if not v["identical"]))
        return path

    def case(name, refused, want, **kw):
        results.append({"case": name, "refused": refused, "want_refused": want, **kw,
                        **({"why": last["why"]} if refused and last.get("why") else {})})
        last["why"] = None

    def write(path, text):
        with open(path, "w", encoding="utf-8", newline="") as f:
            f.write(text)

    keep_materials, keep_template = M.materials, TEMPLATE
    M.materials = fake_materials
    results, last = [], {"why": None}
    try:
        runs = os.path.join(out, "runs")
        r, man, diffs = prompts_run("no clearance", runs, "synthetic-candidate", None)
        moved = {sdk: edit(open(os.path.join(src, f"{sdk}.docs.txt"), encoding="utf-8").read())[1] for sdk in diffs}
        case("prompts without a clearance", r["refused"], True, prompt_files=r["prompt_files"],
             move_both_ends=all(("\n-" + moved[k] + "\n") in d and ("\n+" + moved[k] + "\n") in d for k, d in diffs.items()),
             additions_shown=all("+Read value /* */ . /* */ zq_count here." in d and "\n+zq_count\n" in d for d in diffs.values()))
        good = clearance_file(man, os.path.join(out, "cleared.txt"))
        r, man, _ = prompts_run("digests cleared", runs, "synthetic-candidate", good)
        case("prompts with the clearance", r["refused"], False, prompt_files=r["prompt_files"])
        # 1. placeholder text inside the docs is kept literally (round 6's substitution expanded it)
        p = open(os.path.join(runs, man["prompts"]["candidate/strava"]["path"]), encoding="utf-8").read()
        t, d = stored_materials(runs, "candidate", "strava")
        bk = SC.load_bank(bank)
        old = _build_prompt_round6(open(TEMPLATE, encoding="utf-8").read(), M.SDKS["strava"][2], cheatsheet(commit), t, d,
                                   tasks_text(bk, "strava"))
        case("placeholders in the docs stay literal", not (PLACEHOLDER_LINE in p and p.count("## Task 1:") == 1), False,
             round6_expanded=PLACEHOLDER_LINE not in old and old.count("## Task 1:") > 1)
        refused, res = preflight(runs, good)
        snapshots = res[2][1] if not refused else {}
        case("sessions preflight with the clearance", refused, False,
             jobs_carry_verified_texts=(not refused) and all(j[2] == snapshots[f"{j[0]['arm']}/{j[0]['sdk']}"] for j in res[1]))
        case("sessions preflight without the clearance", preflight(runs, None)[0], True)
        # 2. common content: the manifest's bank commit, the sheet, the template
        mpath = os.path.join(runs, "prompts", "manifest.json")
        mtext = open(mpath, encoding="utf-8").read()
        mt = json.loads(mtext)
        write(mpath, json.dumps({**mt, "bank_commit": "0" * 40}, indent=1))
        case("manifest bank commit replaced", preflight(runs, good)[0], True)
        sheet2 = cheatsheet(commit) + "\nInjected sheet line.\n"
        forged = {k: build_prompt(open(TEMPLATE, encoding="utf-8").read(), M.SDKS[k.split("/")[1]][2], sheet2,
                                  *stored_materials(runs, *k.split("/")), tasks_text(bk, k.split("/")[1])) for k in mt["prompts"]}
        kept_files = {k: open(os.path.join(runs, v["path"]), encoding="utf-8").read() for k, v in mt["prompts"].items()}
        for k, v in mt["prompts"].items():
            write(os.path.join(runs, v["path"]), forged[k])
        write(mpath, json.dumps({**mt, "cheatsheet_sha256": sha(sheet2),
                                 "prompts": {k: {**v, "sha256": sha(forged[k])} for k, v in mt["prompts"].items()}}, indent=1))
        case("sheet replaced in the prompts and the manifest", preflight(runs, good)[0], True)
        for k, v in mt["prompts"].items():
            write(os.path.join(runs, v["path"]), kept_files[k])
        write(mpath, mtext)
        template2 = open(TEMPLATE, encoding="utf-8").read() + "\nOne more instruction.\n"
        case("template changed after prompts", preflight(runs, good, template2)[0], True)
        tpath = os.path.join(out, "reader-changed.md")
        write(tpath, template2)
        TEMPLATE = tpath
        r, _, _ = prompts_run("prompts re-run with a changed template, old clearance", runs, "synthetic-candidate", good)
        TEMPLATE = keep_template
        case("prompts re-run with a changed template keeps no clearance", r["refused"], True)
        r, man, _ = prompts_run("rebuilt", runs, "synthetic-candidate", good)
        case("prompts rebuilt with the original template and the clearance", r["refused"], False)
        # stored input and prompt file changes
        tp, dp = input_paths(runs, "candidate", "strava")
        kept = open(dp, encoding="utf-8").read()
        write(dp, kept + " ")
        case("a stored input changed by one byte", preflight(runs, good)[0], True)
        write(dp, kept)
        ppath = os.path.join(runs, man["prompts"]["candidate/strava"]["path"])
        kept_p = open(ppath, encoding="utf-8").read()
        write(ppath, kept_p.replace("\n", "\n\n", 1))
        case("a prompt file changed", preflight(runs, good)[0], True)
        write(ppath, kept_p)
        # 3. a file replaced after validation: the workers keep the validated text
        refused, res = preflight(runs, good)
        write(ppath, kept_p + "\nInjected after validation.\n")
        job = next(j for j in res[1] if j[0]["arm"] == "candidate" and j[0]["sdk"] == "strava")
        case("a prompt file replaced after validation", "Injected after validation." in job[2] or refused, False,
             worker_text_is_validated=job[2] == kept_p)
        write(ppath, kept_p)
        # 4. freezing: prompts after a session, resume with another prompt, a changed frozen manifest
        refused, res = preflight(runs, good)
        freeze(runs, *res[2])
        s0 = res[1][0][0]
        C.write_json(os.path.join(runs, "sessions", s0["id"], "meta.json"), record(s0, sha(res[1][0][2])))
        r, _, _ = prompts_run("prompts after a session started", runs, "synthetic-candidate", good)
        case("prompts re-run after a session started", r["refused"], True)
        case("resume with the frozen prompts", preflight(runs, good)[0], False)
        C.write_json(os.path.join(runs, "sessions", s0["id"], "meta.json"), record(s0, "0" * 64))
        case("resume with a session answered on another prompt", preflight(runs, good)[0], True)
        sched, sessions = SC.load_runs(runs)
        case("scorer given a session answered on another prompt", bool(SC.run_integrity_problems(runs, sessions)), True)
        C.write_json(os.path.join(runs, "sessions", s0["id"], "meta.json"), record(s0, sha(res[1][0][2])))
        cur = open(mpath, encoding="utf-8").read()
        write(mpath, cur.replace('"chars"', '"chars" ', 1))  # the same JSON, other bytes
        case("a frozen run's manifest changed", preflight(runs, good)[0], True)
        write(mpath, cur)
        case("the frozen run restored", preflight(runs, good)[0], False)
        # fix round 8: the schedule of a started run is frozen
        try:
            cmd_schedule(argparse.Namespace(out=runs, baseline=commit, candidate=commit, seed=7, providers=list(PROVIDERS),
                                            reps=REPS, force=True))
            forced = False
        except SystemExit as e:
            forced = bool(e.code)
            last["why"] = str(e.code)[:300]
        case("schedule --force after a session started", forced, True)
        spath = os.path.join(runs, "schedule.json")
        stext = open(spath, encoding="utf-8").read()
        C.write_json(spath, make_schedule("synthetic-baseline", "synthetic-candidate", 1))  # re-seeded
        case("resume with a re-seeded schedule", preflight(runs, good)[0], True)
        case("scorer given a re-seeded schedule", bool(SC.run_integrity_problems(runs, SC.load_runs(runs)[1])), True)
        sch = json.loads(stext)
        write(spath, json.dumps({**sch, "arms": {**sch["arms"], "candidate": "synthetic-baseline"}}, indent=1))
        case("resume with replaced arm commits", preflight(runs, good)[0], True)
        case("scorer given replaced arm commits", bool(SC.run_integrity_problems(runs, SC.load_runs(runs)[1])), True)
        write(spath, stext)
        C.write_json(os.path.join(runs, "sessions", s0["id"], "meta.json"), {**record(s0, sha(res[1][0][2])), "commit": "0" * 40})
        case("resume with a session recorded on another commit", preflight(runs, good)[0], True)
        C.write_json(os.path.join(runs, "sessions", s0["id"], "meta.json"), record(s0, sha(res[1][0][2])))
        case("the frozen run restored again", preflight(runs, good)[0], False)
        # 5. a later deletion with the old clearance (a fresh run directory: the first is frozen)
        state["delete_heading"] = True
        r, _, diffs = prompts_run("later deletion", os.path.join(out, "runs-later"), "synthetic-candidate", good)
        case("a later heading deletion with the old clearance", r["refused"], True,
             deletion_shown=all(any(ln.startswith("-#") for ln in d.splitlines()) for d in diffs.values()))
        r, _, diffs = prompts_run("B/B", os.path.join(out, "runs-BB"), "synthetic-baseline", None)
        case("B/B prompts with no clearance", r["refused"], False, prompt_files=r["prompt_files"],
             empty_diffs=not any(diffs.values()))
        case("B/B sessions preflight with no clearance", preflight(os.path.join(out, "runs-BB"), None)[0], False)
        # fix round 8: the B/B exemption needs the approved template and sheet and a fresh render
        template2 = open(TEMPLATE, encoding="utf-8").read() + "\nOne more instruction.\n"
        tpath = os.path.join(out, "reader-changed.md")
        write(tpath, template2)
        TEMPLATE = tpath
        r, _, _ = prompts_run("B/B with a changed template", os.path.join(out, "runs-BB-template"), "synthetic-baseline", None)
        TEMPLATE = keep_template
        case("B/B prompts with a changed template and no clearance", r["refused"], True)
        forged_t = os.path.join(out, "runs-BB-forged-template")
        shutil.copytree(os.path.join(out, "runs-BB"), forged_t)
        forge(forged_t, template2)
        case("B/B prompts regenerated with a changed template, no clearance", preflight(forged_t, None, template2)[0], True)
        forged_m = os.path.join(out, "runs-BB-forged-materials")
        shutil.copytree(os.path.join(out, "runs-BB"), forged_m)
        forge(forged_m, open(TEMPLATE, encoding="utf-8").read(), lambda d: d + "\nInjected into both arms alike.\n")
        case("B/B with both arms' stored materials replaced alike, no clearance", preflight(forged_m, None)[0], True)
    finally:
        M.materials, TEMPLATE = keep_materials, keep_template
    extras = ("move_both_ends", "additions_shown", "round6_expanded", "jobs_carry_verified_texts", "worker_text_is_validated",
              "deletion_shown", "empty_diffs")
    ok = all(c["refused"] == c["want_refused"] for c in results) and all(c[k] for c in results for k in extras if k in c)
    return ok, results


def cmd_gate_selftest(a):
    ok, checks = M.gate_selftest()
    for c in checks:
        print(("ok  " if c["ok"] else "FAIL") + f" {c['check']}")
    print(f"{sum(c['ok'] for c in checks)}/{len(checks)} gate checks as expected")
    if a.e2e_out:
        ok_e, res = gate_e2e(a.bank, a.materials, a.e2e_out)
        C.write_json(os.path.join(a.e2e_out, "gate-e2e.json"), {"ok": ok_e, "results": res})
        for r in res:
            print(json.dumps(r))
        print("end to end:", "ok" if ok_e else "FAILED")
        ok = ok and ok_e
    sys.exit(0 if ok else 1)


def sessions_preflight(a, template=None):
    """Every check before a model call (fix rounds 6-7); returns (manifest, jobs). The prompt
    files are read once into `snapshots`; the same strings are verified (`verify_run_inputs`) and
    handed to the workers, which never reopen a file. A run whose first session has started is
    frozen (`FROZEN` records the manifest's and every prompt's SHA-256): the manifest must still be
    that one, and every existing session must record its prompt's SHA-256 from it."""
    template = template if template is not None else open(TEMPLATE, encoding="utf-8").read()
    with open(os.path.join(a.runs, "schedule.json"), encoding="utf-8") as f:
        stext = f.read()
    sched = json.loads(stext)
    mpath = os.path.join(a.runs, "prompts", "manifest.json")
    with open(mpath, encoding="utf-8") as f:
        mtext = f.read()
    manifest = json.loads(mtext)
    snapshots = {}
    for key, rec in manifest.get("prompts", {}).items():
        try:
            with open(os.path.join(a.runs, rec["path"]), encoding="utf-8", newline="") as f:
                snapshots[key] = f.read()
        except OSError:
            sys.exit(f"refused: {key}: prompt file missing")
    problems = verify_run_inputs(a.runs, a.bank, a.cleared, manifest, snapshots, template, sched,
                                 fresh_materials(sched, a.materials))
    manifest.pop("_deviations", None)
    fz = os.path.join(a.runs, FROZEN)
    if os.path.exists(fz):
        frozen = C.read_json(fz)
        if frozen.get("manifest_sha256") != sha(mtext) or frozen.get("prompts") != {k: sha(v) for k, v in snapshots.items()}:
            problems.append("the run is frozen and its manifest or prompts are not the frozen ones")
        if frozen.get("schedule_sha256") != sha(stext) or frozen.get("arms") != sched["arms"]:
            problems.append("the run is frozen and its schedule or arms are not the frozen ones")
    problems += session_record_problems(a.runs, sched, manifest)
    if problems:
        sys.exit("refused by the structural gate: " + "; ".join(problems))
    jobs = []
    for s in sched["sessions"]:
        if s["provider"] not in a.provider:
            continue
        if os.path.isfile(os.path.join(a.runs, "sessions", s["id"], "meta.json")) and not a.rerun:
            continue
        if s["provider"] == "claude" and not a.claude_model:
            sys.exit("--claude-model is required for claude sessions (record what you request)")
        model = a.claude_model if s["provider"] == "claude" else a.codex_model
        effort = a.claude_effort if s["provider"] == "claude" else a.codex_effort
        key = f"{s['arm']}/{s['sdk']}"
        jobs.append((s, manifest["prompts"][key]["path"], snapshots[key], a.runs, model, effort, identifying(s, a.runs, sched)))
    if a.max:
        jobs = jobs[:a.max]
    return manifest, jobs, (mtext, snapshots, stext)


def session_record_problems(runs, sched, manifest):
    """Every recorded session must be one of the schedule's, with its arm, commit, provider, SDK
    and repetition, and must record the manifest's prompt SHA-256 (fix rounds 7-8)."""
    by_id, problems = {s["id"]: s for s in sched["sessions"]}, []
    for mp in sorted(glob.glob(os.path.join(runs, "sessions", "*", "meta.json"))):
        sid = os.path.basename(os.path.dirname(mp))
        meta, s = C.read_json(mp), by_id.get(sid)
        if s is None:
            problems.append(f"{sid}: a recorded session that the schedule does not have")
            continue
        if any(meta.get(k) != s[k] for k in ("arm", "commit", "provider", "sdk", "rep")):
            problems.append(f"{sid}: its recorded arm, commit, provider, SDK or repetition is not the schedule's")
        want = (manifest.get("prompts", {}).get(f"{s['arm']}/{s['sdk']}") or {}).get("sha256")
        if meta.get("prompt_sha256") != want:
            problems.append(f"{sid}: answered with another prompt than the manifest's (a changed prompt needs a new run)")
    return problems


def freeze(runs, mtext, snapshots, stext):
    """Freezes the run when its first session starts: the manifest's, every prompt's and the
    schedule's SHA-256 and the arms' commits."""
    path = os.path.join(runs, FROZEN)
    if not os.path.exists(path):
        C.write_json(path, {"manifest_sha256": sha(mtext), "prompts": {k: sha(v) for k, v in snapshots.items()},
                            "schedule_sha256": sha(stext), "arms": json.loads(stext)["arms"],
                            "frozen_at": time.strftime("%Y-%m-%dT%H:%M:%S%z")})


def cmd_sessions(a):
    manifest, jobs, (mtext, snapshots, stext) = sessions_preflight(a)
    if jobs:
        freeze(a.runs, mtext, snapshots, stext)  # the run's inputs are final from the first session on
    with cf.ThreadPoolExecutor(max_workers=min(4, max(1, a.parallel))) as ex:
        for meta in ex.map(one, jobs):
            obs = meta["observed"]
            print(json.dumps({"id": meta["id"], "requested_model": meta["requested"]["model"],
                              "observed": obs.get("models") or obs.get("model"), "effort": obs.get("effort"),
                              "failed": meta["failed"], "answers": meta["answers"], "answer_error": meta["answer_error"],
                              "attempts": len(meta["attempts"])}))


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0], formatter_class=argparse.RawDescriptionHelpFormatter,
                                 epilog=__doc__.split("\n", 2)[2])
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("schedule", help="write RUNS/schedule.json")
    p.add_argument("--baseline", required=True)
    p.add_argument("--candidate", required=True)
    p.add_argument("--seed", type=int, required=True)
    p.add_argument("--out", required=True)
    p.add_argument("--providers", nargs="+", default=list(PROVIDERS), choices=PROVIDERS)
    p.add_argument("--reps", type=int, default=REPS)
    p.add_argument("--force", action="store_true")
    p = sub.add_parser("prompts", help="render materials per arm and write RUNS/prompts/")
    p.add_argument("--bank", required=True)
    p.add_argument("--runs", required=True)
    p.add_argument("--materials", required=True, help="materials.py output directory (per commit)")
    p.add_argument("--allow-types-diff", action="store_true", help="diagnostic only: build prompts although the public types differ")
    p.add_argument("--cleared", help="clearance file from the candidate's independent review: `overall <digest>` and "
                                     "`<sdk> <digest>` lines naming the added-text digests (needed when the candidate adds text)")
    p = sub.add_parser("gate-selftest", help="the structural gate on a synthetic candidate render (no model call)")
    p.add_argument("--e2e-out", help="also run `prompts` end to end on a synthetic candidate (needs --bank, --materials); "
                                     "writes prompts with the bank's task text: keep it next to the sealed bank")
    p.add_argument("--bank")
    p.add_argument("--materials", help="a materials.py output directory holding B's render")
    p = sub.add_parser("sessions", help="run the scheduled reader sessions (resumable)")
    p.add_argument("--bank", required=True)
    p.add_argument("--runs", required=True)
    p.add_argument("--provider", nargs="+", default=list(PROVIDERS), choices=PROVIDERS)
    p.add_argument("--claude-model", help="a model alias or full name (required for claude sessions)")
    p.add_argument("--claude-effort", help="not passed unless given")
    p.add_argument("--codex-model", default=R.CODEX_MODEL)
    p.add_argument("--codex-effort", default="xhigh")
    p.add_argument("--parallel", type=int, default=4)
    p.add_argument("--max", type=int, help="run at most this many pending sessions (in schedule order)")
    p.add_argument("--rerun", action="store_true", help="also re-run finished sessions (first answers are normally kept)")
    p.add_argument("--cleared", help="the clearance file `prompts` was given (required when the candidate adds text)")
    p.add_argument("--materials", required=True, help="the materials.py output directory `prompts` used (each arm is "
                                                      "re-rendered and compared with the stored inputs)")
    p = sub.add_parser("probe", help="isolation probe: sandbox layout, and each provider once with a dummy prompt")
    p.add_argument("--out", required=True)
    p.add_argument("--claude-model", default="claude-opus-5-5")
    p.add_argument("--codex-effort", default="xhigh")
    p.add_argument("--no-providers", action="store_true", help="only the sandbox layout (no model call)")
    a = ap.parse_args()
    for k in ("out", "runs", "bank", "materials", "e2e_out", "cleared"):
        if getattr(a, k, None):
            setattr(a, k, os.path.abspath(getattr(a, k)))
    {"schedule": cmd_schedule, "prompts": cmd_prompts, "sessions": cmd_sessions, "probe": cmd_probe,
     "gate-selftest": cmd_gate_selftest}[a.cmd](a)


if __name__ == "__main__":
    main()
