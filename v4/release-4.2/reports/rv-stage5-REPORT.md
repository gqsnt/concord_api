# rv-stage5 — independent Stage 5 adoption review (Brief C), O01 and O02

> Saved by the coordinator from the reviewer's returned text (the harness refused the subagent's
> own write; it did not route around it). Reviewer: fresh Claude Opus subagent (Agent tool,
> requested `opus`/`xhigh`; effective effort not observable). The reviewer authored none of the
> reviewed code. Logs: `out/`; probe scripts: `probes/`; scratch tree `src/` (disposable).

## Verdicts

| | O01 native header planning | O02 response metadata materialization |
|---|---|---|
| Recommendation | **Concur: DEFER_WITH_EVIDENCE** (not ADOPT; RETAIN_BASELINE also defensible) | **Concur: DEFER_WITH_EVIDENCE** (not ADOPT; RETAIN_BASELINE also defensible) |
| Counterexample | none; probe catches 2 seeded mutants | none; probe catches the author's early-exit mutant |
| Real experiment | yes: independent rebuild reproduces binary sizes; timing rerun reproduces effects | yes: same |
| Trigger | concrete, add a threshold | concrete, add a threshold |

## O01

- Equivalence argument: N ≤ 16 runs the unchanged scan (compile-time branch); N > 16 sorts indices
  by `(name.as_str(), index)`; predecessor = baseline's latest earlier same-name contribution;
  structural check in declaration order with the media rule last (first error and "earlier" label
  unchanged); reverse group walk reproduces `live`, any-marked reproduces `sensitive`. Assumption
  `HeaderName ==` agrees with `as_str()` equality checked directly.
- Core differential (`rv_o01_probe.rs`): 30,456 chains, N ∈ {1,2,8,15,16,17,18,19,24,31,32,33,64,
  65,128,200,512,1024,2048}, colliding/adjacent/reserved/sensitive-look-alike names from every
  constructor, random and late-injected violations; 10,461 accepted, 19,995 errors; transcripts
  byte-identical (SHA-256 `92075277…`, 38 MB). Seeded mutants (first vs last accept contribution;
  reversed tie-break) produce 520 and 96,589 changed lines; the author's oracle fails on both.
- Macro-level differential (`rv_o01_macro_probe.rs`): 18 endpoints on a long client (N 16–18) and
  a short client (const/static aliases, override/append/remove-reintroduce, empty and non-empty
  `each`, `else inherit`, reserved removal, two-error orders, sticky `#[sensitive]`, suppressed
  failing formatter, `validate_header_names()`): baseline = candidate byte-identical
  (`c70e2485…`); long path equals short path after owner normalization.
- Author tests on the candidate: core exit 0; 8 public binaries, 297 tests exit 0; bench
  `EQUIVALENCE: 134 registered cases; 66 groups; 68 comparisons`.
- Stack: on default stacks both pass up to N = 2048; with reduced stacks baseline and candidate fail
  at the same points (512 KiB/384 KiB after N = 1024, 256 KiB after N = 512). Release peak stack not
  measured.
- Measurement validity: allocation counting only in `o01-alloc`; identical allocation tables;
  only `__private.rs` differs; same lock; source switches recompile; tables recomputed from
  `timings.json` match; PLAN.md hashed before timing. Rerun (BCCBCBBC): full-prepare ratios 32
  mixed 0.43, 64 mixed 0.27, 128 mixed 0.165, N = 1 ≈ 1.01. For N ≤ 16 (identical algorithm)
  layout/codegen shifts reach the acceptance bounds (e.g. prepare/16/distinct −8 to −9 %,
  plan/8/mixed +4.8 % in the rerun): the ≤ 5 % bound was observed, not certifiable.
- Side note (baseline, unchanged by O01): a text value reading a native-key value is promoted to
  sensitive when the macro cannot decide `S_XB == AUTHORIZATION` at compile time (over-redaction,
  safe direction).
- Judgement: complete, allocation-free, no behaviour change found; gains at N ≥ 32 real; no caller
  evidence for such chains; ~80 lines and a second path the maintained exhaustive tests never reach.
  Suggested trigger: a real declaration with native chains ≥ 32 where `prepare()` is ≥ 1 % of
  request CPU (then keep `o01_header_groups` plus a differential as maintained tests).

## O02

- Only `charsets()` changes: a value string is built only for `charset`; every reading/advancing/
  `None` branch unchanged (unterminated quote, trailing backslash, empty token, missing `=`,
  separators): whole-input validation preserved; `single()`, policies, `CodecError` and Context
  untouched.
- Direct differential vs a verbatim baseline copy (`media_rv_probe.rs`): exhaustive sequences of up
  to 6 tokens over `; = " \ SP HT charset CharSet x utf-8 @ , ""`, 400k random longer, 1.5M
  structured inputs with 17 malformed-tail kinds: 7,129,043 inputs, 0 differences; the early-exit
  mutant is caught (`"text/plain;charset=charset="`).
- Author's full-outcome corpus re-run: 8 runs (none/json/form/json,form × arm) exit 0, rows
  identical; baseline rows hash-match the author's logs. `media` 3 and 13 passed; `concord`
  `metadata` 10 passed (incl. retention).
- Measurement validity: counting allocator only in `o02_alloc`; same harness/toolchain/lock;
  `--cap-lints allow` both arms (sizes identical without it); PLAN.md before timing. Rerun
  (BCCBCBBC): 12-param head 0.72, 12-param full 16 B 0.72, 10 KiB 0.965, bare 1.00–1.01, charset
  0.97–0.98, Binary control 1.00.
- Judgement: complete, tiny, no behaviour change found; saves ~7–8 ns and one allocation per
  non-charset parameter, i.e. ~0–25 ns per real response. Suggested trigger: `media::single`/
  `charsets` ≥ 1 % of decode CPU in a real profile (then keep the corpus and the direct
  differential as maintained tests).

## Limitations

Finite debug-build corpora, not proofs; no release peak stack, Windows, compile-time, Form/Text
timing, end-to-end network timing or real workload distributions; reruns on a non-idle host
(powersave, no pinning): differences under 5 % are noise-level. No Git changes; `wt-o01` and
`wt-o02` unchanged.
