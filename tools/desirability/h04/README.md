# H04 instrument: docs-pilot task bank (sealed) and scorer

Measures hypothesis H04 ("generated documentation tells the caller the actual contract", a
docs-only pilot; card `work/state/cards/H04.draft.md`, source `First_Experiment.md`, PLAN_AMENDMENT_A1
§2-§3, §6). Fresh model readers get one SDK's materials from one arm's commit and answer four
fixed tasks; every answer is scored automatically against a sealed key; the paired comparison of
the two arms feeds the card's WIN / LOSS / INCONCLUSIVE rule.

The bank (task text, answer keys, critical facts, fixtures, reference and seeded wrong answers,
claim checks, validation record) lives **outside the repository** in
`<WS>/work/sealed/h04-bank/` and must hash to `BANK.sha256` (`scorer.py` and `runner.py` refuse
any other bank). Task text is sealed too: the candidate implementer must not tune to it. This
README names no task and no answer.

## Design

- **SDKs and tasks.** 6 SDKs x 4 tasks = 24: witnesses Cloudflare (`cloudflare-zones`), Sentry,
  Kraken (`kraken-spot-rest`); controls Nager (`nager-holidays`), Xero (`xero-accounting`), Strava.
  Per SDK, the card's four kinds: (1) valid construction and the destination (composed URL, and
  base versus appended path); (2) one ordinary call with stated inputs, and a use of its result;
  (3) the actual public result type, the error owner and retained failure metadata; (4) the
  SDK-specific exception of the card's table with a permitted action (a choice among 4-5 options
  plus a short typed field). Each task lists 2-4 critical facts (66 in all); a task passes only
  when all pass. Control facts are preservation facts and are all stated in B's public materials
  (fix round 1).
- **Keys confirmed at B** (`734991e9`): every expected request, outcome and option claim was
  derived from the SDK source at B and is re-checked by the validation; historical complaints that
  no longer hold at B are recorded in the sealed `VALIDATION.md` and replaced by a current
  documented uncertainty in the same SDK's scope (none manufactured).
- **Materials** (`materials.py`, `materials(commit, sdk, out)`): the same task text; Concord's cheat
  sheet at the bank commit (same for both arms); the SDK's **public types** and its **generated
  rustdoc**, both rendered to text from **that arm's commit** with one extraction: `git archive`
  of the commit, `cargo +nightly doc --no-deps --lib --target x86_64-unknown-linux-gnu` with JSON
  output and **public items only** (fix round 7: in the commit's own cargo target directory
  `$CARGO_TARGET_DIR/h04-target/<sha12>/`, with a `--crate-version` provenance marker naming the
  commit that every JSON must carry, else the build is refused; one shared directory had kept the
  first commit's JSON for the second), rendered from the crate root (re-exports inlined, public
  modules recursed, hidden items skipped, `#[non_exhaustive]`/`#[must_use]`/deprecation shown as
  rustdoc shows them), plus the package README of that commit unless the crate page already holds
  it (the documented README route; deduplicated by normalized containment). One **example-free
  filter** (extraction 5: the coordinator's fix-round-2 decision, fix round 3's prose rule, fix round 4's whitespace and host rules): every
  code block in any form (any fence language, `~~~`, indented) is replaced by "(code example
  omitted from the probe materials)"; every inline code span holding an expression (a call, a
  constructor, a method chain or fragment, a struct literal, a field access, a macro) by "(code
  omitted)"; lines of code left in prose and endpoint call chains (I3's `find_chains` +
  `chain_hits`) are removed too, and so is every expression written in plain prose (field
  accesses `a.b[.c]`, calls with arguments or through a path, chains, fragments, indexes, struct
  literals; URLs, numbers, real hosts, file names, abbreviations, names whose members are
  capitalized and sentence punctuation excepted), by the same rule as a span. Whitespace inside an
  expression does not hide it (fix round 4: `value . count`, tabs, a chain broken over lines,
  `Type :: new ( http )` are read without it), and an operand is never a host (`value.count/2`;
  a host needs a known TLD, a two-label one a strong TLD, and a path after `/`, not a number).
  Contract prose, tables, bare type/item names, literals and wire texts stay: no code reaches
  readers except bare names. The format audit has its own deliberately broad detector (tokens
  holding `.`, `::` or `(`, read with its own whitespace rules), independent of the filter's
  rules: block forms filter byte-identically; conversions from blocks (fence removed, spans,
  table cells), from spans (into prose, blocks, table cells) and from prose (into spans, table
  cells) show no code-like token the original's output does not show, and span/prose/table
  conversions show exactly the same ones; 515 synthetic positives (20 snippets in 12 forms, 25
  one-unit whitespace/operand snippets in 11 forms) leave no marker; switching off any one filter
  rule, the whitespace normalization, or restoring round 3's host-path exception must make the
  audit fail (negative controls), else `prompts` refuses. **Structural gate** (fix rounds 5-6, the coordinator's decisions; bound to the exact
  materials, no normalisation): `prompts` stores each prompt's exact materials (public types and
  rendered docs with the README part) under `RUNS/prompts/inputs/` and builds the prompts from
  them, records the SHA-256 of each SDK's exact candidate materials and an overall digest, and
  writes per SDK a complete unified diff of baseline vs candidate materials
  (`RUNS/prompts/review/<sdk>.diff`: deletions, both ends of moves, whitespace and table changes
  visible; it rebuilds the candidate) with the broad detector's tokens on added lines as an
  annotation; `prompts` and `sessions` refuse unless `--cleared FILE` names those digests for
  every SDK whose candidate materials differ in any byte (written by the coordinator from the
  candidate's independent review of the diffs); any later change invalidates it; byte-identical
  SDKs (B/B) need none. **Prompt integrity** (fix round 7): placeholders are substituted in one
  pass over the template only (inserted docs stay literal); the clearance digest also binds the
  sealed bank's hash, the template, the cheat sheet (always the sealed bank commit's) and every
  final prompt's SHA-256; `sessions` reads each prompt once, verifies that text against the
  sealed sources and the stored inputs (also without a clearance) and hands the same text to the
  workers; the first session freezes the run (`prompts/FROZEN.json`): `prompts` then refuses,
  resuming refuses a changed manifest or prompt and any session recorded on another prompt, and
  the scorer refuses an unfrozen run or a session whose prompt hash differs from the manifest's.
  **Approved inputs** (fix round 8): `bank.json` seals the approved reader template's and cheat
  sheet's SHA-256; `prompts` and `sessions` re-render both arms from their commits and require a
  clearance whenever the template or sheet differ from the approved digests or either arm's
  stored materials differ from that fresh render, so the B/B exemption holds only for approved,
  freshly confirmed inputs; `schedule` refuses on a started run even with `--force`, and
  `FROZEN.json` binds the schedule and its arms (resuming and the scorer refuse a changed
  schedule, changed arms or a session recorded on another arm, commit or slot); a coverage audit lists the reader-visible contract
  statements (URLs, request lines, result types, base/origin statements, retry statements, error
  variants) per SDK and arm and compares the arms. The types rendering has no doc text and must
  be byte-identical across arms (`runner.py prompts` refuses otherwise). Rendered sizes and word
  counts are recorded as diagnostics. Readers see no source, tests, records or other answers.
- **Answers** (`reader.md`): one JSON object per session, `{"answers": [{"task": 1, ..}, ..]}`,
  four answers. Field kinds: URL, option letter, count, name, Rust type, Rust code. Code follows
  the I3 R3 answer contract (one expression; statements only inside a closure body; no `?`; only
  `vec!`/`format!`); the variables a task names (`http`, `api`, `value`, `e`, ..) are provided.
- **Scoring** (`scorer.py`, `harness.py`), no free-text grading:
  - URL: as I3 R1 destination + query (`probes/score.py` `split_url`);
  - option letter, count, name (last path segment, case-insensitive), header name (lower case);
  - type: I3 R2's rustc identity check (`r2_gate` + `r2_check_fn`, invariant marker, positive
    control per truth), compiled as an external caller (`use <crate>::*;`), so aliases,
    re-exports, `crate::`/crate-name and `std`/`core` spellings all pass;
  - code: pasted into a trusted scenario of the sealed key, compiled as an **integration test** of
    the SDK package (a separate crate: only the public API is reachable, so a private generated
    control does not compile) inside the I3 compile sandbox (`CompileSandbox`, driven by
    `truth.compile_rounds` with `--test __h04_probe`), run in the I3 run sandbox (`sandbox_run`)
    against scripted local receivers (`concord-example-testserver`); passes when it compiles and,
    in every run of the check, the outcome value equals the key's and the recorded requests equal
    the key's or a permitted alternative (a default written out) (I3 `compare_requests`: method,
    path and query pairs in order, header names and values, body). Every expression over a result
    or an error runs on the disclosed response and two undisclosed, varying ones (a constant copied
    from the task fails); constructions also run scenarios that exercise every submitted input
    (an expired token refreshed with the submitted client credentials, the default send policy),
    and where a client takes two service origins each origin is its own receiver (which receiver
    got which request is checked, so swapped or reused origins fail).
    Kraken signatures are recomputed in Python from the recorded path and body (HMAC-SHA512),
    never taken from `prepare()`; post-preparation policies (retries, refusals, scope checks) are
    judged by what reaches the receiver.
  - a session whose answer is missing, malformed or failed (tool use twice) scores 0 on all four
    tasks; first answers are kept.
- **Sessions** (`runner.py`): one fresh session per (arm, provider, SDK, repetition): 2 providers x 3
  repetitions x 6 SDKs x 2 arms = 72 sessions, 288 answers. `schedule` writes a seeded, balanced,
  interleaved order (the two arms of a pair adjacent, arm order alternating; 9 baseline-first and
  9 candidate-first pairs per provider; pairs shuffled by the seed). `sessions` runs both providers
  **inside bubblewrap** as `edits/run.py` runs the I4 agents: the home directory (repository,
  worktrees, workspace, sealed bank, runs, `~/.claude`, `~/.codex`) hidden, an empty working
  directory and a fresh provider home with only the credential file bound, both under an opaque
  `$HOME/<random>` mount (no arm, commit, SDK, session or run identifier in any reader-visible
  path), network kept; Claude
  `claude -p` with no tools, no setting sources, strict MCP config, slash commands and skills
  disabled; Codex `codex exec --strict-config` read-only with tool features, skills and the
  multi-agent role disabled. Each attempt's transcript is checked (cwd, tools, MCP servers, skills,
  sandbox policy, no owner instruction file content, no identifying string in the visible
  environment); a tool call or a failed check discards the
  attempt, a second fails the session. Observed model/effort from the event stream or rollout.
  Resumable. `probe` shows the sandbox layout and runs each provider once with a dummy prompt.
- **Decision** (`decision.py`): thresholds exactly as in the card (witness net >= 6 of 72; no
  provider with a negative witness net; controls lose <= 2 of 72, counted as control
  regressions, never offset by corrections: the only rule, no option selects another (fix round
  3); the report's control net loss is labelled a diagnostic, not the decision; no newly misrepresented control
  fact; no recurrent new critical-task failure; one provider cannot WIN). Two control-fact rules
  apply together: a control fact that regresses in >= 2 pairs (counted per fact, even where the
  task already failed for another fact) blocks WIN, and a control critical fact
  that passes in every baseline answer of its SDK and task and fails in any candidate answer is
  newly misrepresented and blocks WIN (INCONCLUSIVE, or LOSS when attributed to the candidate docs:
  the coordinator's fix-round-1 decision); the docs-truth gate (no critical control fact
  misrepresented in the candidate docs, from the candidate review) is an input like the other
  gates. Other operational choices are documented in `decision.py` (pairs, regressions, recurrent
  task failure = 2 regressions in one SDK/task, gates/obligations/attribution as inputs,
  `WIN_PENDING_GATES` while a gate is unknown).
  `scorer.py --runs` reports per task, SDK, provider and arm, critical-fact failures separately,
  paired corrections/regressions, and the verdict.

## Files

| File | What |
|---|---|
| `materials.py` | `materials(commit, sdk, out)`: public types + generated rustdoc as text, deterministic, per commit |
| `reader.md` | the reader prompt (answer format, code rules, placeholders) |
| `runner.py` | `schedule`, `prompts`, `sessions` (I3 runner mechanics, bubblewrap isolation), `probe` |
| `harness.py` | integration-test harness: answer forms, scenario filling, sandboxed compile and run |
| `scorer.py` | field checks, critical facts, runs scoring and report, `--validate` |
| `decision.py` | WIN/LOSS/INCONCLUSIVE and its `--selftest` on synthetic score tables |
| `BANK.sha256` | hash of the sealed bank's canonical listing |

## Commands

```sh
cd <worktree>
export CARGO_TARGET_DIR=/home/canarit/projects/Concord_Desirability_Loop/work/targets/probe CARGO_BUILD_JOBS=4
SEALED=/home/canarit/projects/Concord_Desirability_Loop/work/sealed/h04-bank
RUNS=/home/canarit/projects/Concord_Desirability_Loop/work/sealed/h04-runs/<name>   # prompts hold sealed task text
python3 -I tools/desirability/h04/decision.py --selftest
python3 -I tools/desirability/h04/scorer.py --bank $SEALED --validate --out $SEALED/validation/<name>
python3 -I tools/desirability/h04/runner.py schedule --baseline <B> --candidate <C> --seed <N> --out $RUNS
python3 -I tools/desirability/h04/materials.py --selftest            # synthetic audit, negative controls, gate
python3 -I tools/desirability/h04/materials.py --render-control --commit <B> --out <dir>   # fresh rustdoc per commit (cargo)
python3 -I tools/desirability/h04/runner.py prompts --bank $SEALED --runs $RUNS --materials <materials dir> \
    [--cleared <file>]   # refused while a changed SDK's exact-materials digest is not named in the clearance file
python3 -I tools/desirability/h04/runner.py probe --out <dir>        # isolation probe (dummy prompt), before sessions
python3 -I tools/desirability/h04/runner.py sessions --bank $SEALED --runs $RUNS --materials <materials dir> \
    --claude-model <model> [--parallel 4] [--cleared <file>]   # the same clearance file and materials directory
python3 -I tools/desirability/h04/scorer.py --bank $SEALED --runs $RUNS --out <dir> \
    [--gates pass|fail|unknown] [--docs-truth pass|fail|unknown] \
    [--increased-obligations yes|no|unknown] [--attributable yes|no|unknown]
```

## Instrument validation (at B, recorded in the sealed `VALIDATION.md`)

`scorer.py --validate` (about 25 s with a warm target directory), fix round 8:

- the reference answer of every task passes at B: 24/24;
- one seeded wrong answer per critical fact: 66/66 fail their fact and their task, each with the
  task's other facts still passing;
- constant-answer controls (the disclosed value written as a constant, on every task with an
  expression over a result or an error): 17/17 fail;
- wrong-input construction controls (wrong client secret or ID, swapped or reused service origins,
  non-default send policy): 11/11 fail;
- semantically equivalent spellings, including defaults written out explicitly: 29/29 pass;
- claim checks backing key values and task-4 options (attempt counts, variants, refusals,
  independent signature checks): 32/32 hold; 66 critical facts (the Strava timeout is no longer
  scored: fix round 2; Strava t1's third fact is the documented page-size range: fix round 3);
- malformed sessions (answer `null`, `answers` not a list) score 0 on every task;
- end to end, `--runs` on a synthetic 72-session run (both arms at B; no candidate built): 288
  answers scored, the expected paired net, losses and verdict;
- `decision.py --selftest`: 30/30 synthetic tables and checks at and around each threshold, including
  a single newly misrepresented control fact (not WIN), two control-fact regressions masked by
  existing task failures (not WIN; LOSS when attributed), the docs-truth gate, and the control-loss
  rule (3 regressions + 3 corrections lose 3 answers: not WIN; `decide` takes no rule parameter);
- materials: two independent renders of B byte-identical (texts and meta); the format audit (block,
  span and prose conversions, synthetic positives, negative controls) holds for all six SDKs;
- gate: `runner.py gate-selftest` (synthetic materials: a doc block moved between items, a deleted
  heading, a whitespace-only list indent, a table-boundary change and the round-5 comment and
  blank-line cases each need a clearance and show in a diff that rebuilds the candidate; a
  clearance naming the digests allows; an old clearance after a later deletion or a whitespace
  change, a missing SDK digest or a wrong overall digest block; byte-identical arms need none) and,
  with `--e2e-out`, `prompts`, the `sessions` preflight and the scorer's run check end to end on a
  synthetic candidate (placeholder text in the docs kept literal; a replaced manifest bank commit,
  a forged cheat sheet, a changed template, a changed stored input or prompt file refused; a file
  replaced after validation never reaches a worker; prompts after a session started, a resumed
  session recorded on another prompt, a changed frozen manifest and a scorer given such a session
  refused; fix round 8: B/B prompts with a changed template, B/B prompts regenerated with a changed
  template, both arms' stored materials replaced alike, `schedule --force` after a session, a
  re-seeded schedule, replaced arm commits and a session recorded on another commit refused, in
  `sessions` and in the scorer); the B/B dry run has empty diffs, no deviation and builds;
- rustdoc freshness: `materials.py --render-control` (B's tree and a copy differing in one
  generated doc string, its file time kept at the commit time: rendered A, B, A again, B differs
  wherever A shows the string and A again is byte-identical; the pre-fix pipeline reproduces the
  stale JSON and the provenance check refuses it);
- isolation probe: the sandbox hides the repository, workspace, bank and owner instruction files;
  one dummy-prompt session per provider passes every isolation check (no tools, no MCP servers, no
  skills, read-only Codex sandbox, no multi-agent role, no owner instructions, opaque paths) and
  its whole saved transcript holds no identifying string;
- the bank hash is reproducible and equals `BANK.sha256` (`06be1a2b…`, 137 files, 24 tasks, 66 critical facts).

## Known limits

- Readers are models; repetitions are correlated, not independent participants. Text materials
  measure content and hierarchy, not visual rendering.
- Answers are compiled against B for both arms (a docs-only candidate changes no API; the types
  identity check in `prompts` guards that assumption).
- A code answer is checked on a few scripted replies; another call with the same requests and
  result type would pass. Some task-4 options are false by documentation (provider-side
  uncertainty), not by a run; the runs decide the SDK's observable behaviour.
- No code reaches readers (only bare names, literals and wire texts in prose): documentation that
  explains a contract mainly through examples loses that content in both arms alike. The bare-name
  classifier and the prose rule are conservative (field paths such as `account.id`, navigation
  names such as `Kraken.account.open_orders`, variant patterns and OAuth scope lists written with
  dotted scope names are removed; on B the prose rule removes one wire formula written as nested
  calls).
- Timeouts are not scored (a wall-clock check cannot pin a 30 s value without flaky margins).
- Claude Code's built-in plugins (agents-md, telemetry, privacy, plugin-authoring) and built-in
  subagent types are still listed by its init event; with no tools they cannot be invoked, and the
  working directory and home hold no AGENTS.md or CLAUDE.md.
- The rustdoc JSON depends on the pinned nightly (`rustc 1.101.0-nightly (e9bbadf68 2026-10-12)`,
  format 61); both arms must be rendered with the same toolchain (recorded per materials file).
