# H03 measurement: the sealed idiom corpus, the I5 relabel and the costs

Instrument for hypothesis H03 (`v4/desirability/cards/H03.md`, exact inputs). The corpus, its
intended requests and every per-snippet result are sealed outside the repository:
`<WS>/work/sealed/h03-bank/` (corpus) and `<WS>/work/sealed/h03-runs/` (results), with
`<WS>` = `/home/canarit/projects/Concord_Desirability_Loop`. This directory holds only the
procedure and the corpus hash.

| File | What |
|---|---|
| `corpus.py` | the generator: `traits` (rustc trait probes at the baseline), `build` (the mechanical rule → `corpus/snippets.json`, `refs.json`, `generation.json`) |
| `harness.py` | throwaway arm trees (`git archive`, fresh mtimes, provenance marker), the I3 R3 compile sandbox and run sandbox with a local receiver, verdicts, the corpus hash, the chronology log |
| `measure.py` | `validate` (S1 references and value probes at the baseline, twice; intended requests), `freeze` (`BANK.sha256`), `run --arm LABEL [--commit SHA] --rep N` |
| `report.py` | per-arm verdicts, reproducibility, regressions, wrong requests → sealed `RESULTS.md`, `summary.json` |
| `relabel.py` | §2: `diagnostics/caller_bank.py` per arm into a sealed bank directory, `diagnostics/score.py bank`, the relabel runs, the report |
| `costs.sh`, `costs_summary.py`, `expanded.py` | §3: `benches/scale/ab.sh` one row per invocation (load average before each row, suites rotated per row), medians and ratios, expanded sizes |
| `BANK.sha256` | SHA-256 of the canonical listing of `<WS>/work/sealed/h03-bank/{inputs,corpus}/` |

Arms are commits of `/home/canarit/projects/concord_api` (`harness.ARMS`: baseline `a883f7bb`,
A `d793fecc`, B `7f14da4d`, A′ `fc16090d` as `Aprime`, B′ `1329a3e5` as `Bprime`); any further arm runs with
`--arm LABEL --commit SHA` against the same frozen corpus. Cargo: one command at a time,
`CARGO_BUILD_JOBS=6`, `CARGO_TARGET_DIR=<WS>/work/targets/base/h03-<arm>` (one per arm, never
shared). Python: `python3 -I`. No model agents.

## 1. Idiom corpus

Inputs: the SDK declarations at the baseline (parsed by `../decl.py`) and the sealed per-SDK
`inputs/<sdk>.json` (client construction pointed at the receiver, each position's canonical value
and S1 spelling, each call's reply fixture and the expectation written from the declaration and
the provider documentation its `PROVENANCE.md` cites).

The rule (`corpus.py` docstring; every exclusion is recorded in `corpus/generation.json`):
calls in declaration order capped at the first 8 with a caller input; positions = positional
values and bodies of the navigation, value setters, parameter-group setters (called as the
endpoint's own method); spellings S1–S8 by the declared type's trait impls **at the baseline**,
decided by rustc (`corpus.py traits`: one probe per type and trait — `Default`, `FromStr`,
`From<&str>`, `From<inner>` for SDK newtypes, `PartialEq` — compiled in an integration test of the
SDK at the baseline). An S7 spelling identical to the position's S1 spelling is excluded. Each
snippet names its S1 reference: the S1 spelling of the value its spelling produces (canonical, or
the type's default for S2, S7 `T::default()`, S8 `Default::default()`).

Validation (`measure.py validate`, baseline only, S1 references and trusted value probes only):
every canonical S1 compiles, returns `Ok`, sends one request, the same in two runs, compared with
the written expectation (method, target, header values, present/absent headers, body); value
probes (`<T as Default>::default()`, `<T as From<..>>::from(..)`, `"..".parse::<T>()` against the
reference's S1 value, `==`) all hold. An S1 reference of a *default* value that the SDK refuses
(an empty `String` as a path segment: Concord's "path segment is empty" build error) leaves its
default-valued snippets without an intended request; `validate --write` removes them with that
reason (`generation.json` → `removed_after_validation`), writes `corpus/intended.json` (the run-1
request of each S1 reference) and copies the reply fixtures into `corpus/replies/` (every arm gets
the baseline's reply bytes). Then `freeze`; `run` refuses any other corpus.

A run (`measure.py run`): a fresh tree of the arm, every snippet plus the instrument controls in
one integration test per SDK, compiled in the compile sandbox (`compile_rounds`, first error and
its rendered text kept per failing snippet), each compiled snippet run alone in the run sandbox
against a receiver answering its reply. **Pass** = compiles, run completes, result `Ok`, and the
requests equal the intended request (`probes/score.py compare_requests`: method, path and query
pairs in order, header names, header values, body). Provenance: the tree's marker (arm, commit,
archive hash) is compiled into each test and echoed in its record; freshness from cargo's JSON
(no workspace unit reused in the first round, no path package outside the tree). Controls, built
at run time from each SDK's first S1 reference: a changed literal (compiles, `Ok`, different
request) and an appended non-existent method (does not compile); both must fail.

## 2. I5 relabel

`relabel.py bank --arm X`: `caller_bank.py` on a fresh tree with its bank directory redirected to
`<WS>/work/sealed/h03-runs/i5/<arm>/bank/` (the committed `diagnostics/bank` is never written),
`score.py bank`, and for each case whose seeded line compiles, the seeded and the corrected line
run against the receiver (setup pointed at it with `with_base_url`; Kraken with a private policy
and fixed test credentials so account calls are sent; a line binding a GitHub scope handle is
completed with `.labels()`, the endpoint the bank's own GitHub cases call). Resolved iff the
seeded line sends exactly the corrected line's requests. `relabel.py report`: resolved / still
diagnosed / failures, `wrong_setter` expected-type naming, newly named internals versus the
baseline, and the E0283 labels.

## 3. Costs

`costs.sh OUT SUITE_BASELINE SUITE_ARM..` with suites from the baseline's
`benches/scale/gen.py suite OUT --concord <arm tree>/crates/concord` (byte-identical workloads
apart from the Concord path): one explicit warm phase (ab.sh's own `--workspace` dev and release
builds), then `benches/scale/ab.sh` unchanged, one row per invocation, suites rotated per row,
each row started only when the 1-minute load average is below `LOAD_MAX` (default 8; polled, a
row that cannot start before `DEADLINE` is recorded as not measured), the load recorded before and
after each row (`load.tsv`). `costs_summary.py OUT [--rerun OUT10] [--expanded DIR]`: medians,
arm/baseline and arm/arm ratios; rows whose arm/baseline median exceeds 1.05 are re-run with
`RUNS=10` (`ROW_LIST=".." RUNS=10 costs.sh ..`). `expanded.py OUT SUITE..`: bytes of each API
crate's `-Zunpretty=expanded` (pinned 1.99.1 with `RUSTC_BOOTSTRAP=1`, own target directory);
`benches/scale` has no README at the measured commits, so this follows the H08 lane's comparison.

## Limits

- The corpus measures spellings at the positions of 18 calls of six SDKs; String positions keep
  `IntoValue` in every candidate by design, so their non-literal spellings fail on every arm.
- S3 "an integer" for a primitive integer is an unsuffixed literal (`2026.into()`); its S7
  counterpart `u16::from(2026)` does not compile on any arm (the literal falls back to `i32`).
- Snippets compile in one integration test per SDK; a failing snippet is dropped and the rest
  rebuilt (rounds), so one snippet's error cannot hide another's verdict, but rustc's first error
  per snippet is what is recorded.
- The receiver answers every request with one fixture; `Ok` means the SDK accepted that reply.
- Compile-time timing shares the machine with other lanes; the load average before each row is
  recorded, not controlled.
