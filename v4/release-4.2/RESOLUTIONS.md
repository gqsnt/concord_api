# 4.2 — resolutions of F01–F04, M01–M04, T01 and the Stage 4 follow-ups

Each finding: where it was, the baseline reproduction on the frozen 4.1 control, the change, the
repaired assertion on the candidate, its maintained regression and how the regression was
qualified (the fix removed alone makes the test fail on its intended assertion; the source is then
restored byte for byte). Raw logs and receipts are in the work directory
`/home/canarit/projects/concord-v42/` (`logs/`, `records/`, `lanes/`); the portable subset is in
[evidence/](evidence/) and [reports/](reports/).

Baseline replay: 18/18 recipes of the review's portable case file matched exit and witnesses on
`replay-v41` (prepared by the handoff helper from local Git `afcec40`, 1,733 source files verified)
— [summary](evidence/replay/baseline-v41-SUMMARY.json). An expected 101 there reproduces a finding;
it is not a pass. Candidate replay: copies of consumers 1–3 (unchanged inputs and locks) against a
`git archive` of `f0693b5`, with a new repaired-state case file
([cases](evidence/replay/candidate-cases-compiler.json)) — 24/24 matched
([summary](evidence/replay/candidate-f0693b5-SUMMARY.json)). The receipts hash the consumer inputs and
the path-dependency crates separately ([baseline F01#0](evidence/replay/baseline-v41-F01-0.receipt.json),
[candidate F01#0](evidence/replay/candidate-f0693b5-F01-0.receipt.json)).

## F01 — hygienic conditional `Clone` bounds — RESOLVED

- Source: `crates/concord-macros/src/emit.rs` `endpoint_clone` deduplicated bounds by
  `to_string()`. Change (`f0693b5`): one span-bearing `for<'__concord> T: Clone` bound per value and
  body occurrence; the higher-ranked strategy is unchanged. Codegen impact (nightly expansion of
  07dcdd0 vs f0693b5): Riot example 19 → 23 bounds, +210 of 419,496 expanded bytes; quickstart,
  synthetic SDK and clone-compat unchanged ([summary](evidence/f01-codegen-summary.txt)).
- Baseline: `same_forward` check exit 101 E0277 `Clone`. Candidate: exit 0, run prints the expected
  URL; `same_clone_call`/`direct_clone_call` exit 101 E0599 (the intended negative); both-Clone
  clone runs.
- Maintained: `checks/clone-hygiene` (helpers a/c `Clone`, b/d not; forward, reverse, both-Clone,
  both-non-Clone, same type, direct paths; required, optional and body values; trybuild clone-call
  negatives naming the non-`Clone` type).
- Qualified: reverting the dedup removal → E0277 at the `forward` declaration
  ([excerpt](evidence/qualification/F01-dedup.excerpt.txt)).
- Analogous text keys inspected, unchanged: cfg-predicate dedup (no hygienic paths), literal header
  statics (string values), native key paths (already one per occurrence), generated-name checks.

## F02 — type macros in output position — RESOLVED

- Source: `crates/concord-macros/src/syntax.rs` `parse_output`. Change: a path followed by `!` is a type macro, never the
  removed in-place format (narrow guard; a type-first parse was rejected because it would swallow
  `-> Csv (A, B)`/`-> Json ()` into rustc E0214 instead of the migration message). A braced type
  macro that takes the endpoint's block gets a direct hint.
- Baseline: `--features macro-output-paren` exit 101 "expected curly braces". Candidate: paren,
  bracket and brace features exit 0; consumer-3 `macro_type` checks.
- Maintained: `crates/concord/tests/output_types.rs` (three delimiters; `crate::`, `self::`,
  `r#mod::` paths; nested in Vec/Option/tuple/generic; alias, unit, array; body and output),
  `checks/clone-hygiene/tests/type_macros.rs` (absolute and relative cross-crate macros decoded over
  localhost), UI `output_type_macro_errors` (removed spellings around a macro keep their messages).
- Qualified: guard reverted → "expected curly braces" ([excerpt](evidence/qualification/F02-guard.excerpt.txt)).

## F03 — type text keeps literal meaning — RESOLVED (widened by review)

- Source: `crates/concord-macros/src/syntax.rs` `type_text` (string replacements over the whole stream). Change (`f0693b5`):
  one token walker (the emitter's `expr_text`, moved and given a type mode) emits every identifier
  and literal as its own token text. Independent review then widened it in four rounds:
  punctuation written apart never joins into another token (`aae6218`); a macro's input is
  rendered token for token and numbers keep the space a following `.` needs (`e5082c4`);
  executable advice names a payload holding a `macro_rules!` fragment `T` instead of spelling it
  (`e5082c4`: no written text is that fragment, which is opaque to another macro's matcher).
- Baseline: advice `"a::b"`; the historical applied advice fails E0277 (4 ≠ 6). Candidate: advice
  keeps `count("a :: b")`; the historical bad applied source still fails; the regenerated advice
  applied compiles with `Payload<6>`. C02-B's rustdoc capture replayed on the candidate: prose keeps
  `"a < b"`, `r#"p :: q"#`, `"x , y"` as the signatures do ([result](evidence/c02b-rustdoc-result.json);
  HTML text extraction, not visual).
- Maintained: renderer unit tests (all literal kinds, separate punctuation, macro input token for
  token, numbers before dots), emitter rustdoc tests, UI `migration_literal_advice` and
  `migration_token_advice`, `crates/concord/tests/applied_advice.rs` (applied advice compiled and
  checked against its snapshot; the selected macro arm checked by body bytes).
- Qualified: old renderer → snapshot shows `"a::b"` and three unit tests fail; each widening reverted
  alone is caught (R1, R4, R5, R6 excerpts in [qualification/](evidence/qualification/)).

### Forwarded fragments (pre-existing since 4.1, found by the F03 reviews)

Concord re-emitted `macro_rules!` fragments as groups rustc reads as ungrouped. Corrections:
`located` keeps groups holding a fragment intact (`aae6218`); value, body and output types and
initializers are taken from the written tokens with fragments regrouped (`e5082c4`), by context
(`0670240`: every expression fragment of several tokens in expression positions; `1dad499`: a
type fragment in an expression's type positions — turbofish, qualified path, after `as` — stays a
type, so no `unused_parens`), then without any guess (`c69b9b4`: every expression fragment in an
expression position is parenthesized, the added parentheses spanned at the call site so rustc does
not report them; `c2cf3a9`: a path stays bare, so it can head a struct literal; `033e08e`: only a
fragment that parses as an expression is parenthesized, so lifetimes, patterns and non-expression
types are left as written; `81b4ce2`: a type with `+` is grouped there too, and a negative literal
fragment is left as written so it can bound a range pattern; `67006bb`: grouped only before a
postfix operator, `(-1isize).unsigned_abs()`). Pre-existing boundary kept:
a forwarded `let` statement inside an initializer block is rejected by the initializer parse (as on
4.1). Effects: `pick!($e * 2)` body/output, `Payload<{ $e * 2 }>` and
`= $e * 2` mean 6 (4.1: 5 or a type mismatch); `$e.meaning()` with `$e = &VALUE` calls the `&Foo`
impl; `[u8; $e.len()]` compiles; a forwarded `dyn A + B` behind `&` is accepted. Maintained:
`crates/concord/tests/forwarded_types.rs` (under `deny(unused_parens)`: shifts, casts, pointer casts,
turbofish, qualified paths, struct-literal paths in endpoint and owner initializers, lifetimes,
trait objects, negative literals as range bounds and before a method call). Qualified: R3, R7–R12,
R14–R16, R18.

## F04 — raw identifiers in executable advice — RESOLVED

- Source: `crates/concord-macros/src/syntax.rs` trailing body-format advice used `plain(&name)`. Change: the identifier as
  written (`plain` removed from `crates/concord-macros/src/syntax.rs`; lookup normalization in `crates/concord-macros/src/model.rs` unchanged).
- Baseline: advice `body type`. Candidate: `body r#type?: String` and `request concord::Text;`; the
  historical bad applied source still fails; the regenerated advice applied passes.
- Maintained: UI `migration_raw_advice`, `applied_advice.rs`. Qualified: `plain` restored → UI shows
  `body type?` ([excerpt](evidence/qualification/F04-plain.excerpt.txt)).

## M01 — benchmark equivalence — RESOLVED

The S3 manual side now prepares `Accept: application/json`. The existing `Case` registry states
each pair's setup, measured stage and comparator; `benches/criterion/src/tests.rs` executes every
registered closure twice and compares 47 promised pairs (method, exact URL, header names/values/
multiplicity/same-name order/sensitivity, body presence and bytes, version, timeout), with negative
controls (changed/missing Accept, repeated value/order/multiplicity, URL/method, body/absence vs
empty, native config, sensitivity) and an S3 pin against a conflicting client default. Round-trip
pairs are labelled partial floors. Historical S3 ratios kept with a linked notice; affected IDs
re-measured ([PERF.md](PERF.md)). RV03's shipped-pair equality on a candidate copy of consumer-4:
all comparator fields equal. Report: [m01-bench](reports/m01-bench-REPORT.md).

## M02 — examples declare their own features — RESOLVED

`examples/renamed` declares `extras`; `cargo test --locked -p concord-example-renamed` alone: 3
passed (before: exit 101 E0432). New gate step `test-examples-isolated` tests every example package
alone (7 packages, 126 tests on the integrated candidate) and fails on an unlisted example; the
no-extras consumer is checked for having no extras feature. Fresh extraction confined to marked
outputs, stale sources detected, a packaged consumer built from a deep non-ASCII path. Report:
[tooling](reports/tooling-REPORT.md).

## M03 — executable workflow guard — RESOLVED

`scripts/check-ci.py --workflow` parses `ci.yml` (documented YAML subset) and counts only an
enabled, blocking `scripts/verify.sh --package --bench` step on Linux and Windows plus the nightly
lane; 17 positive and 49 negative maintained cases (`scripts/tests/test_check_ci.py`); 8 mutants of
the real workflow pass the 4.1 checker and fail the new one. The final executable review found a
gate step disabled by `set -n`/`set -o noexec` still accepted; the gate's step may now only turn on
`-e`, `-u`, `-x` and `-o errexit|nounset|pipefail|xtrace` (`c2cf3a9`, qualified R13), looking
through leading assignments and `builtin`/`command` (`81b4ce2`, qualified R17). Static inspection only: CI logs remain
the execution evidence. Correction note for 4.1: its `ci-inventory` step was a text search (D24,
Z01); the 4.1 CI runs themselves are not invalidated.

## M04 — mutation helper — RETIRED

`tests-gen/m4_mutations.py` had no maintained caller and 4 of 9 mutations target deleted files. It
now exits 2 without applying anything or reporting a verdict; `scripts/tests/test_m4_retired.py`
checks the refusal and that nothing maintained calls it. Historical records unchanged.

## T01 — metadata-stage assertion — RESOLVED

`examples/native/tests/metadata_stage.rs`: a localhost server sends a 2xx head the format refuses
and withholds the body; the call returns `Metadata` within a bound and the response is recovered
unread (its body read after release); controls: valid head+body decodes, valid head + bad body is
`Decode`, Status recoverable. Mutation triple: clean 3 passed → head check removed: intended
"Metadata before body" assertion fails → restored 3 passed. Report: [runtime-tests](reports/runtime-tests-REPORT.md).

## Stage 4 — documentation and linked follow-ups

- D01 documentation: factual corrections and task notes ([docs report](reports/docs-REPORT.md));
  both fresh documentation-first trials found the `&HeaderValue` omission, corrected in `c621b6c`
  ([Opus trial](reports/trial-opus-REPORT.md), [Terra trial](reports/trial-terra-REPORT.md); agent
  simulations, not human research).
- D16-sol (original BLOCKED_ENVIRONMENT, unchanged): follow-up `crates/concord/tests/d16_followup.rs`
  (hand-calculated topology, three compile-negative diagnostics, a deliberately wrong expectation
  shown to fail).
- D22-terra (original INCOMPLETE, unchanged): follow-up completed inside O01 — source-bound restored
  comparisons and scaling observations ([o01 report](reports/o01-header-plan-REPORT.md)).
- I20-A (original INCOMPLETE, unchanged): follow-up `crates/concord/tests/lifecycle.rs`.
- I22-A / P08-A (original BLOCKED_ENVIRONMENT / INCOMPLETE, unchanged): the packaged-consumer check
  now builds from a deep non-ASCII path; native Windows result: see [STATUS.md](STATUS.md).
- H05 (original INCOMPLETE, unchanged): its missing synthetic calibration was supplied by the 4.1
  review's RV04; not repeated.
- Dedicated security coverage remains NOT_ASSESSED; the incidental multipart Content-Type
  SECURITY_FOLLOWUP from the review stays unassigned.
