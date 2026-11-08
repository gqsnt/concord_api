# Pilot harness: one instrument for every later hypothesis (PLAN_AMENDMENT_A3 §2)

The H04 bank, sandbox, clearance and scoring and the I3 R3 compile/run sandbox, generalised into
one harness. A later hypothesis adds **task files only** (sealed, outside the repository), a run
file, and its card's threshold JSON; it changes no code here. The harness never decides: the card
is the authority, the threshold evaluator is decision support.

## Layout

| File | What |
|---|---|
| `pilot.py` | the CLI: `validate`, `hash`, `freeze`, `run`, `reader-pack`, `reader-score`, `score`, `port-h04`, `regress-h04`, `selftest` |
| `taskfile.py` | task-file schema and strict validation; loading a task set |
| `arms.py` | throwaway arm trees (`git archive`), per-arm cargo target directories with provenance markers, sealed-location guards |
| `seal.py` | task-set hash (I4 `bank_hash`, imported), the frozen-hash ledger, run file and its binding to a run directory |
| `kinds.py` | kinds `compile`, `request`, `stage`: execution on one arm and verdicts |
| `readers.py` | kind `reader`: H04 bank port, prompt packing, scoring through the H04 scorer |
| `tally.py` | pass rates, regressions, reproducibility, the generic threshold evaluator, the public summary |
| `clearance.py` | candidate clearance: lane-transcript audit and candidate-diff check, with a report |
| `selftest.py` | instrument controls and the harness's own checks (`pilot.py selftest`) |
| `controls/` | the public control task set (seeded must-pass and must-fail tasks at `58322527`) |
| `examples/h04.thresholds.json` | the H04 card's task-level thresholds as a threshold JSON |
| `BANKS.sha256` | the ledger of frozen task-set hashes (one line per set) |

Reused by import, unchanged: `probes/score.py` (`CompileSandbox`, `sandbox_run`,
`compare_requests`, answer form, R2 type gate), `probes/truth.py` (`compile_rounds`, `Module`,
`exchanges`, `req_record`), `h04/harness.py` (`Batch`, scripted receivers), `h04/scorer.py`
(`plan`, `finish`, field checks, `run_integrity_problems`), `h04/runner.py` (`make_schedule`),
`h04/decision.py` (regression only), `edits/run.py` (`bank_hash`, `oracle_verdict`,
`failure_blocks`, `targets`). One change outside this directory: `h04/harness.py` `Batch` takes
optional `tree`, `target_dir`, `run_root` (an already materialised tree, its own target directory,
the run scratch); without them its behaviour is unchanged (`h04/scorer.py --validate` passes as
before: 24/24, 94/94, 29/29, 32/32, 30/30, end to end ok). The H08 stage pipeline
(`git show 607fb0ee:tools/desirability/h08/omission.py`, not on this branch) is ported into
`kinds.py` and generalised.

## Workflow

```sh
cd <worktree>
export CARGO_TARGET_DIR=/home/canarit/projects/Concord_Desirability_Loop/work/targets/probe CARGO_BUILD_JOBS=4
WS=/home/canarit/projects/Concord_Desirability_Loop
SET=$WS/work/sealed/<hyp>-tasks          # holds tasks/ (sealed)
P="python3 -I tools/desirability/pilot/pilot.py"
$P validate --set $SET                    # schema; counts by kind and class
$P freeze --set $SET --name <hyp>         # appends the hash to BANKS.sha256 (commit it) before any candidate run
$P run --runfile $WS/work/sealed/<hyp>-runs/<run>.json --rep 1   # every arm; then --rep 2
$P reader-pack --runfile RUN --materials DIR [--seed N --providers claude,codex --reps 3]   # reader tasks only
#   ... the coordinator runs the reader sessions into <out>/readers/<candidate>/sessions/ ...
$P reader-score --runfile RUN
$P score --runfile RUN --thresholds <card>.thresholds.json --fact gates=pass [--fact name=value ..]
python3 -I tools/desirability/pilot/clearance.py --out DIR --transcript LANE.jsonl \
    --repo <repo> --base <baseline> --candidate <candidate>
$P selftest                               # controls + harness checks (about 90 s warm; --no-cargo: 1 s)
```

Sealed locations are enforced: a task set and a run directory must be under `<WS>/work/sealed/`
and outside any git checkout (`arms.sealed_dir`); `run` refuses a set whose hash is not the frozen
one for its name, results are written only to the run directory, and `score` refuses results
produced with another set hash, commit or run file.

## Task files

One file per task, `*.task.json` or `*.task.toml`, anywhere under `<set>/tasks/`; every other file
there is data referenced by relative path (no `..`, no absolute path, no symbolic link). Optional
`<set>/tasks/set.json`: `{format: 1, name, description, commit, reader_template}`. Unknown keys
are errors. Ids are opaque (letters, digits, `._/-`): they appear in the public summary, so an id
must not carry task content.

Common fields:

| field | | meaning |
|---|---|---|
| `format` | required | `1` |
| `id` | required | unique in the set |
| `kind` | required | `compile`, `request`, `stage`, `reader` |
| `class` | required | the card's class (S3, K1, construction, ...): pass rates and thresholds group by it |
| `sdk` | required | SDK short name: `examples/<sdk>-sdk` |
| `role` | optional | `witness`, `contrary`, `control` |
| `group` | optional | reader session group (default: `sdk`) |
| `control` | optional | instrument controls only: `{expect: pass\|fail, reason?: text the failure reason contains}` |
| `note` | optional | free text (sealed; never in the public summary) |

**`compile`**: a caller snippet must or must not compile on an arm.
`code` (Rust), `form` (`body`, default: wrapped as `async fn __pilot_snippet() { <code> }`;
`items`: placed as items), `imports` (use lines; default `use ::<crate>::*;`), `expect`:
`{compiles: bool, first?: {code?: "E0308" | [..], mentions?: [..], absent?: [..], matches?: regex,
in_snippet?: bool, scope?: all | primary}}`. Each snippet is its own integration-test target of
the SDK package (an external caller: public API only), built with `cargo build --keep-going` in the
compile sandbox, one package per cargo command; its first diagnostic is the first `error` message
of that target in cargo's JSON order. `mentions` must occur and `absent` must not occur in the
first diagnostic; scope `all` (default) reads the whole rendered diagnostic, quoted source lines
excluded for `mentions` and the snippet's own quoted lines excluded for `absent` (quoted SDK or
Concord source counts: it is shown to the caller); scope `primary` reads exactly I5 R5's block (the
headline and the lines up to the first `= note`/`= help`/`note:`/`help:`). Names that start and end
with a word character match as whole words (`GitHubRepo` does not match `GitHubRepoLabels`), others
as substrings (`concord_core::`). `in_snippet`: every primary span is in the snippet. A snippet not
built because its package's library failed is `invalid`, not `fail`.

**`request`**: the snippet compiles, runs against the local receiver, returns `Ok`, and sends
exactly the intended request. `setup` (statements or a list of lines; `base: String`, the
receiver's origin, is in scope; `?` is allowed, a setup error is `@@SETUP`), `call` (an expression
whose value is a `Result<T, E: Display>`, e.g. `api.x(..).send().await`), `imports` (use lines
inside the snippet function), `replies` (the H04 reply script: `{status, headers: [[k, v]], body |
body_file, hang_up?, delay_ms?}`; request n gets reply min(n, last)), `limit_s` (default 20),
`expect`: `{requests: [{method, target, headers, body}], ok?: true, outcome?: "@@OK <Debug>",
volatile?: {headers: [names]}}`. `headers` lists every request header except the connection-level
ones the client adds (host, content-length, transfer-encoding, connection, accept-encoding).
Compiled with the H04 `Batch` (one integration-test module per SDK in the compile sandbox, failing
blocks dropped and rebuilt) and run one test per process in the I3 run sandbox; requests compared
with `compare_requests` (method, path and query pairs in order, header names, header values except
`volatile` ones, body: JSON structurally, form pairs in order, else exactly). `outcome` (exact) makes
the harness print `@@OK {:?}` of the value.

**`stage`** (H08): a reference patch passes an oracle; at which stage is the omission detected?
`oracle` (a Rust integration test file), `oracle_name` (default `pilot_oracle`), `package`
(default from Cargo.toml), `names` (regex naming the endpoint), `patches`: `{<arm name> | "*":
{reference, omission, controls?: [{name, patch, expect?: fail | pass}]}}`, `refusal`: `{<arm> |
"*": regex}` (the SDK's own run-time refusal), `expect`: `{stages: [..], names_endpoint?: bool}`.
Per variant a fresh tree (`git archive`, fresh modification times), `git apply --check` then
`git apply`, and inside the compile sandbox: SDK build (`cargo build -p <pkg> --lib`, JSON), the
SDK's existing tests, the oracle compile and run (I4 `oracle_verdict`: every oracle test executed
and `ok`, counts and listing equal). Stages: `SDK build`, `caller compile (existing tests)`,
`caller compile`, `run-time refusal`, `wrong behaviour caught by the existing tests`, `wrong
behaviour caught by the oracle only`, `never`. The SDK must be recompiled in each variant
(`fresh: false`), else the run is invalid. The task is valid on an arm only if the reference passes
(builds, existing tests and oracle pass) and every control behaves as declared; it passes when the
omission's stage is in `expect.stages` (and its first diagnostic names the endpoint, outside quoted
source lines and libtest headers, when required).

**`reader`** (H04 / I3): `number` (task number in its group; answers are keyed by it), `text` or
`text_file`, `key` (the H04 key format unchanged: checks of kinds `url`, `choice`, `int`, `ident`,
`lower`, `type`, `code`; `facts` naming checks; optional `claims`), `reference`, `wrong` (one
seeded wrong answer per fact; `F2+constant`-style extra controls), `equivalent`, `fixtures` (the
directory `body_file`s resolve against; default the task file's), `sdk_info` (`{dir, pkg, crate}`),
`compile_at` (the commit answers compile against: H04 compiled both arms' answers at the bank
commit). A task passes when every fact passes; a fact when all its checks pass.

The **edit** kind stays with I4 (`edits/run.py`): an editing agent in an isolated export, the
expected semantic edit sites, missed propagation and the hidden oracle are I4's (it shares
`bank_hash` and `oracle_verdict` with this harness). A hypothesis whose outcome is "which stage
detects an omitted edit" uses `stage` tasks with coordinator-written patches; one that needs agent
edits uses I4.

## Run file

```json
{"format": 1, "name": "h03b-pilot-1", "set": "<WS>/work/sealed/h03b-tasks", "set_name": "h03b",
 "out": "<WS>/work/sealed/h03b-runs/pilot-1",
 "arms": [{"name": "baseline", "commit": "a883f7bb"}, {"name": "A2", "commit": "<sha>"}],
 "baseline": "baseline", "reps": 2}
```

`baseline` defaults to the first arm, `reps` to 2. The first command copies the file to
`<out>/RUN.json`; later commands refuse a different run file. Results: `<out>/results/<arm>.r<rep>.json`
(never overwritten without `--force`), readers under `<out>/readers/<candidate>/`, scores under
`<out>/score/` (`score.json`, sealed detail; `summary.public.json` and `SUMMARY.public.md`: counts,
task ids and classes, rule states only). Trees under `<out>/trees/` are scratch: re-made by each
command and removed at its end unless `--keep-trees`.

## Arms and provenance

Each arm commit is materialised with `git archive` into `<out>/trees/<arm>-<sha12>[-stage]`, every
file given a fresh modification time, plus `.pilot-provenance` (commit, SHA-256 of the archive
stream). The cargo target directory is `$CARGO_TARGET_DIR/pilot/<sha12>` (clean tree: compile,
request, reader) or `<sha12>-stage` (patched trees), never shared between commits; its marker
`PILOT_PROVENANCE.json` (`{commit, use}`) is created only in an empty directory and checked, with
the tree's marker, before every cargo command; a directory with files but no marker, or a marker
naming another commit or use, is refused. All cargo commands run in the I3 compile sandbox (read-only
tree, writable target, no network, `--offline --frozen`, the tree's own toolchain), one at a time.

## Sealing and hashing

The set hash is the I4 runner's `bank_hash` over `<set>/tasks/`: one line `<sha256>  <path>` per
regular file, sorted by path, SHA-256 of the listing (`hash`). `freeze` appends
`<hash>  <name>  <files> files, <tasks> task files; frozen <UTC>` to `BANKS.sha256` (committed with
the measurement records; the content stays sealed) and refuses a name already frozen with another
hash: a corrected set needs a new name. `run`, `reader-pack`, `reader-score` and `score` refuse a
set whose current hash is not its frozen line; `run` also re-hashes after the run and discards
results if the set changed meanwhile.

## Scoring and decision support (`score`)

Rows are task x arm x slot x repetition with a verdict (`pass`, `fail`, `invalid`) and a
fingerprint (normalised evidence: first diagnostic; requests and outcome; stage, names verdict and
first diagnostic). A **unit** is (task, slot): slot empty for compile/request/stage; for reader
answers `<candidate>:<provider>/r<rep>` (each session is a unit, paired with the baseline session of
the same provider and repetition; repetitions are samples, not reproductions).

- pass rates per arm x SDK x class, per arm x kind, per arm x role, overall; invalid units apart;
- regressions (pass at the baseline, fail on the arm) and corrections, by unit; a unit invalid on
  either side is unpaired;
- reproducibility: each deterministic unit must give the same verdict and fingerprint in every
  repetition (`run --rep 1`, `--rep 2`); a difference is `irreproducible`;
- thresholds (`--thresholds FILE`, per card):

```json
{"format": 1, "card": "cards/H03b.md", "precedence": ["LOSS", "INCONCLUSIVE", "KEEP_CURRENT"],
 "otherwise": "INCONCLUSIVE",
 "rules": [
  {"id": "W1", "outcome": "WIN", "type": "require", "metric": "pass_rate_gain", "filter": {"role": "witness"}, "op": ">=", "value": 0.20},
  {"id": "W2", "outcome": "WIN", "type": "require", "metric": "corrections", "filter": {"role": "witness"}, "per": "sdk", "quantifier": "all", "op": ">=", "value": 1},
  {"id": "L1", "outcome": "LOSS", "type": "trigger", "metric": "regressions", "filter": {"class": {"not": ["K1", "K2", "K3", "K4"]}}, "op": ">", "value": 0},
  {"id": "K1", "outcome": "KEEP_CURRENT", "type": "trigger", "metric": "pass_rate_gain", "filter": {"role": "witness"}, "op": "<", "value": 0.20},
  {"id": "I1", "outcome": "INCONCLUSIVE", "type": "trigger", "metric": "corrections", "filter": {"role": "witness"}, "per": "sdk", "quantifier": ["count", "==", 1], "op": ">", "value": 0},
  {"id": "G1", "outcome": "LOSS", "type": "trigger", "metric": "fact", "name": "gates", "op": "==", "value": "fail"}]}
```

  Metrics over the units matching `filter` (keys `task`, `kind`, `sdk`, `class`, `role`, `group`,
  `provider`; a value, a list, or `{"not": ..}`): `pass`, `total`, `pass_rate`, `baseline_pass_rate`,
  `pass_rate_gain` (arm minus baseline), `corrections`, `regressions`, `net`, `irreproducible`,
  `invalid`, and `fact` (an external input given with `--fact name=value` or `--facts FILE`:
  gates, CI, reviews, migration listings). `per` evaluates the metric per value of a field and
  `quantifier` (`all`, `any`, `none`, `["count", op, n]`) combines them. Verdict per candidate arm:
  the first outcome in `precedence` with a `trigger` rule that holds; else WIN if every `require`
  rule holds, else `otherwise`. A rule over a fact not given is unknown: an unknown requirement gives
  `WIN_PENDING`, an unknown trigger of higher precedence than the verdict appends `_PENDING`. Expected
  breakages named in a card's kill condition (A3 §8) go into the trigger's filter, so an expected
  break is not a kill and an unexpected one is.

## Clearance (`clearance.py`)

The candidate checks used so far, generalised (see the module docstring for the classes):
**transcript audit** of Claude Code JSONL transcripts and Codex rollouts; every forbidden-path hit
(default `work/sealed`, `work/baseline`, `tools/desirability`, `sealed/`, bank directory names;
`--forbid` adds) is classified by where it occurs: `access` / `access-result` (blocking: a file
tool's path, a search path, a shell word that is not an exclusion, and its result),
`access-evidence` / `content-mention` (review: a result showing a path below a forbidden directory
not explained by brief text; text written to a file or passed to another tool), and the benign
`exclusion` (`-not -path`, `-prune`, `grep -v`, `':!..'`, `!` globs, `test ! -e`), `brief-text`
(user/coordinator messages and results of reading a brief file under `work/roles/` or
`--brief-file`), `brief-echo`, `prose-mention`, `report-text` (the lane's own messages,
SubagentHandback/SendMessage, a command's description). **Candidate-diff check**: any changed path
under `tools/desirability/` is blocking; added lines naming a forbidden path, and added lines (at
least 24 characters) found in a sealed file but nowhere in the base tree, are review findings.
Status `refused` / `review` / `clear`, report `clearance.json` + `CLEARANCE.md`. On the four
candidate-lane transcripts of this session (snapshots 2026-11-08 ~18:15; two lanes were still
running): H08 B clear; H03 A review (its own reported grep whose filter failed and listed
instrument file names); H04 review (listings showing instrument paths); H06 review (a card read
with a shell command that names `tools/desirability/pilot/`). The old `audit_transcript.py`
reports raw hit counts only (10-41 hits per lane, most of them brief text or reports).

## Instrument controls and selftest (A1 §6)

`controls/` (frozen as `pilot-controls`; nager-holidays at `58322527`): compile must-pass (a
correct call; a swapped argument whose first diagnostic is E0277, names `String`, primary span in
the snippet) and must-fail (expected to compile but does not; expected an error but compiles;
wrong expected code; an internal name, `IntoValue`, present); request must-pass (the intended
request; an exact `@@OK 0` outcome) and must-fail (compiles but sends a wrong request; does not
compile; a 400 reply, not `Ok`); stage must-pass (reference passes, a wrong-path control fails the
oracle, the omission is detected at SDK build and the diagnostic names `HolidayResponse`/`meta`) and
must-fail (an omission nothing detects: `never`); a reader task (reference passes, each seeded
wrong answer fails its own fact only, equivalent spellings pass, a malformed session scores 0).
`selftest` runs them twice through the code `run` uses and requires every expectation and equal
verdicts and fingerprints in both repetitions, plus 54 cargo-free checks (schema refusals, hashing
and refreeze refusal, markers, sealed locations, run-file binding, tally and threshold cases,
public-summary leak check, first-diagnostic scopes, transcript and diff audits, reader packing,
integrity refusals).

Results (2026-11-08, final code, 18:12 CET): `selftest` 82/82 (54 harness checks, 28 control
checks; both repetitions identical; about 50 s warm). `selftest --no-cargo`: 54/54 in about 1 s.

## Mapping of the existing instruments

| instrument | pilot form |
|---|---|
| H04 docs pilot | `reader` tasks: `port-h04` converts the sealed bank (it must hash to `h04/BANK.sha256`) into 24 reader task files (frozen `h04-port`); prompts via `reader-pack` with the arm materials rendered by `h04/materials.py` as `<sha12>/<sdk>/{TYPES,DOCS,CHEATSHEET}.txt`; the H04 clearance gate on materials diffs stays H04-specific |
| H04 decision | task-level thresholds in `examples/h04.thresholds.json`; the two control-fact rules are fact-level and stay in `h04/decision.py` (a `fact`) |
| I3 R3 (complete ordinary use) | `request` (same sandboxes, receiver, comparison); R1 (prepared request) as a reader `url` check, R2 (result type) as a reader `type` check |
| H08 omission harness | `stage` (same stages, oracle verdict, first diagnostic, names rule, reproducibility over two repetitions) |
| I5 caller diagnostics | `compile`, `form: items`, `expect.compiles: false`, `first: {in_snippet: true, absent: [internal names + generated names], scope: primary}`; "names the expected type" as a second task with `mentions` |
| I4 edits | stays in `edits/run.py` (boundary above) |
| H03 idiom corpus (`work/wt/h03m/tools/desirability/h03/`, frozen) | one `request` task per snippet: `class` = S1..S8, `setup` = `let base = format!("{}{}", base, "<declared base path>");` + `let api = <inputs[sdk].api>;` + the snippet's `prelude`, `call` = `<code.call>.send().await`, `imports` = `inputs[sdk].imports`, `replies` = `[{status: 200, headers: [["content-type", "application/json"]], body_file: corpus/replies/<sdk>/<reply>}]`, `expect.requests` = `intended[ref].recorded`; its two controls are the request must-fail controls here; its "newly rejected spelling" (H03b kill condition) is a regression filtered by class K1-K4 in the thresholds. Not ported: A3 lets the H03 lane finish on its own tooling; the converter is about 60 lines when needed |

## Regression check (H04 pilot-3)

`regress-h04 --set <WS>/work/sealed/pilot-h04-port --set-name h04-port --runs <WS>/work/sealed/h04-runs/pilot-3
--out <WS>/work/sealed/pilot-regress/h04-pilot-3 --gates pass --docs-truth pass --thresholds examples/h04.thresholds.json`:
the 72 saved sessions' 288 answers re-scored through `readers.score_sessions` (fresh tree of
`734991e9`, own target directory) give the **same per-task verdicts as `h04/scorer.py` for 288/288
answers**, the same fact verdicts and the same check verdicts (0 differences; 278 passes on both
sides); `h04/decision.py` on the pilot-scored rows gives WIN, as recorded; the generic evaluator with
the H04 thresholds gives WIN (witness net +6, +3 per provider, 0 control regressions, no SDK/task
with 2 regressions, complete samples).

## Known limits

- Reader runs (sessions) are the coordinator's: the harness packs, freezes and scores; the H04
  isolation runner (`h04/runner.py sessions`) is H04-specific. Session records must follow the H04
  layout (`sessions/<id>/answer.json`, `meta.json` with `prompt_sha256`).
- `reader-pack` takes materials as files; rendering them (rustdoc, filters, the H04 materials
  clearance gate on candidate diffs) stays with `h04/materials.py` and `h04/runner.py`.
- The threshold evaluator works on task units; fact-level rules (H04's control-fact rules) and
  judgement inputs (gates, reviews, attribution) enter as `fact`s.
- Request comparison: `volatile` covers header values only; bodies are compared exactly (JSON
  structurally); independent signature recomputation (H04's Kraken check) is not generalised.
- Stage tasks run cargo inside the compile sandbox with the tree read-only: an SDK test that writes
  into its source tree fails there (none does at `58322527`).
- Compile tasks are built one package per cargo command; a snippet target is a test crate, so
  `#![...]` crate attributes in `items` snippets apply to the whole snippet crate only.
- Public summaries name task ids and classes; ids must be chosen opaque by task authors.
- The transcript audit is lexical: an unusual exclusion form is reported as access and needs a
  reviewer's reading; a forbidden path built at run time (string concatenation) is not seen.

### Known limits recorded by the instrument review (`pilot-harness-review-1-sol`, ACCEPT; A3: non-blocking)
- Reader scores are loaded without checking their set hash, run hash or arm commits (deterministic
  results are bound); a stale reader score could enter a later tally. Check the binding by hand
  until fixed.
- A scheduled reader session without metadata contributes no rows instead of a zero score; a card
  that relies on reader rates must require completeness (all scheduled sessions present).
- `compile_at: arm` for reader items is documented but refused by `score_items`; arm-specific
  reader compilation is not available (H04's fixed compilation commit is unaffected).
- Provenance is checked before entering `h04/harness.py` `Batch`, not before each cargo command
  inside it (the "before every cargo command" statement above is stronger than the code).
- A `request` task may exempt any named header from comparison, including `Accept` or
  authentication headers; an exemption must be justified in the task's `note` and backed by an
  independent check where the exempted value decides correctness.
- `clearance.py` does not enforce that its report (which can quote transcript excerpts) is written
  under `work/sealed`; write it there.
- `clearance.py` classifies some exclusion code (e.g. `assert not name.startswith('tools/desirability/')`
  inside a script) and a lane's own self-report as `access` (blocking); read every `access` hit
  before acting on it (first seen on the H06 A Codex rollout: 5 false positives).
