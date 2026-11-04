# Independent compiler correction review

**Verdict: REVISE.** Reviewed `f0693b5752d26e78ef2161dd4dda19f5f7f4f715` against `07dcdd03c61a1822341e92d3820e66117f1aef14`. The F01 clone and F02 output-type controls exercised here pass, as do the particular F03 literal and F04 raw-identifier applied repairs. The shared renderer nonetheless changes the meaning of other repair text, including a working public type-macro case. That violates the requested F03/F04 meaning-preservation contract and is blocking for this integrated delta. The two renderer findings below have separate causes and need separate regressions.

## Blocking correctness findings

### 1. Separate punctuation is joined into a different macro token

**Location:** `crates/concord-macros/src/syntax.rs::token_text`, especially lines 132–135 and 170–182. The spacing rules join a `.` to a neighboring `.` regardless of the first punctuation token's `Spacing::Alone`. The same renderer supplies executable migration advice and expression/type prose.

**Reproducer:** The retained independent fixtures are `probes/consumer-3/src/bin/punct_hint.rs`, `punct_original.rs`, and `punct_applied.rs` under this report's directory. The relevant macro is:

```rust
macro_rules! choose {
    (. .) => { Payload<1> };
    (..) => { Payload<2> };
}
```

For the old spelling `body p: choose!(. .) via Fmt`, `cargo +1.99.1 check --offline --locked --bin punct_hint` exits 101 with the advice `write body p: choose!(..) and request Fmt;` (backticks omitted here). The current syntax retaining the original tokens, `body p: choose!(. .) { request Fmt; }`, compiles and prepares body `1`; applying the advised `body p: choose!(..)` compiles and prepares body `2`. Both run commands exited 0 and asserted the corresponding bytes. Run from `lanes/rv-compiler/probes/consumer-3`, through `timeout 600 /home/canarit/projects/concord-v42/tools/heavy.sh`, with `cargo +1.99.1 run --offline --locked --bin punct_original` and then `--bin punct_applied`.

**Expected / actual and impact:** Advice must preserve the two separate punctuation tokens and the chosen payload type. Instead it silently selects the other macro arm, changing the type and request body after a user follows valid-looking advice. This is an actual applied-code result, not merely a string comparison. Other token-sensitive macro inputs may be affected by the same joining policy.

**Strongest counterargument:** The original spelling is obsolete and this particular token-matching macro is unusual; normal literals, paths and the supplied applied-advice cases pass. The defect arises only when a user applies the hint. Nevertheless the required contract explicitly covers punctuation and rejects repairs that alter payload meaning, and the original current-syntax control demonstrates the intended payload unambiguously.

**Requested regression:** Preserve token jointness when rendering macro arguments, including separating punctuation whose concatenation lexes differently. Keep this exact public compile/run/advice/apply test and add cases for `&&`, comment openers, `<-`, and adjacent punctuation where relevant, checking semantic behavior as well as parsing.

### 2. Invisible macro groups lose expression precedence in advice

**Location:** `crates/concord-macros/src/syntax.rs::token_text`, lines 149–159, where `Delimiter::None` is rendered with empty delimiters. A group created by forwarding a `macro_rules!` expression has semantic grouping even though it has no printed delimiters.

**Reproducer:** `probes/consumer-3/src/bin/group_macro_hint.rs` forwards `$e:expr` from `declare!(1 + 2)` into `pick!($e * 2)`. Its diagnostic says `body p: pick!(1 + 2 * 2)`; parentheses around the forwarded expression have disappeared. The same original type `pick!($e * 2)` in a valid query fixture `invisible_query.rs` accepts `Payload<6>` and prepares `p=6`. The advised expression as standalone Rust evaluates to 5; `group_macro_applied.rs` applies it in a body and asserts prepared bytes `5` (see execution results below). The expression should retain the grouping equivalent to `pick!((1 + 2) * 2)`.

**Expected / actual and impact:** Forwarded expression grouping must survive presentation when needed for precedence. The hint instead recommends a different const-generic type, so applying it can silently change payload semantics. This is distinct from literal opacity and punctuation jointness.

**Strongest counterargument:** The old syntax cannot itself compile. Also, the separate current-syntax *body* control `group_macro_original.rs` fails with E0308 (`BodyFormat<Payload<6>>` versus `BodyFormat<Payload<5>>`) even before applying advice. I reproduced an equivalent body inconsistency on the released replay source as well, so this existing generated-code issue is not claimed as introduced by the reviewed delta. The valid query control independently establishes the intended type's value, and the renderer's flattening and emitted advice are directly observed. Fixing the hint alone must not be presented as fixing the pre-existing body inconsistency.

**Requested regression:** Exercise `None`-delimited groups forwarded through `macro_rules!`, including nested expression groups in type-macro arguments, type fragments and expression documentation. Confirm advice applied as code preserves precedence and runtime/const value. Include the body mismatch as a separately tracked regression or boundary; a simple parse-back string equality check is insufficient.

## Other observations and scope

- `endpoint_clone` now emits one higher-ranked bound per original endpoint field/body occurrence and does not reconstruct type identity from display text. The selected mixed-order, non-Clone and both-Clone public tests passed. A separate enabled-`cfg` override probe prepared successfully with a suppressed inherited contribution; cloning its request holding the non-Clone forwarded payload failed with E0599 naming that payload. The disabled endpoint referenced a missing type without breaking compilation.
- The `owners_cfg_predicate` text key deduplicates cfg alternatives, not type obligations. `Distinct` deduplicates generated documentation strings, not emitted field types or Clone bounds. I found no demonstrated analogous type-identity defect in this narrow inspection; I did not prove hygiene equivalence for all possible cfg syntax.
- The F02 lookahead guard accepts the tested absolute, qualified, raw-identifier, nested and forwarded macro types. A malformed `path ;` following `item!()` produced the actionable `expected string literal` at the path statement; the maintained UI test covers a brace-delimited macro without a following endpoint delimiter and legacy in-place, swapped and `via` diagnostics. This does not establish all Rust type forms.
- No optional polish item is required for this verdict. The stage asks for modest clone-bound code-generation impact to be measured; I did not independently measure it and cannot certify that performance requirement from the inspected code. This is a coverage limit, not a demonstrated regression.

## Executed controls and observations

All Cargo commands used Rust `+1.99.1`, `--offline --locked`, a finite `timeout`, and the two-slot `tools/heavy.sh` wrapper. No unfiltered tests were run in replay consumers. The replay runner and per-case logs are retained at `run_cases.py` and `case-logs/` beside this report; the runner checks expected exit status and required/forbidden witnesses, not merely exit status.

- `timeout 900 tools/heavy.sh python3 lanes/rv-compiler/run_cases.py`: on the original released replay source, selected F01 original failure and four controls matched; selected F02 original paren failure matched; selected F03 and F04 failure/control/applied-old cases matched. On the copied consumers pointed at this candidate, all nine selected F01 cases, five selected F02 cases, three selected F03 cases, and four selected F04 cases matched their expected exits and witnesses. The original unfiltered consumer-2 tests were explicitly skipped; two author-recorded `_applied_v42` binaries were absent from the portable consumer and skipped, not claimed as executed. Candidate's maintained applied-advice tests below cover the corresponding repaired examples.
- `cargo +1.99.1 test --offline --locked -p concord --test applied_advice --test output_types`: 2 applied-advice tests and 1 output-types test passed. The command originally also named other packages, but the named test targets selected only these Concord integration targets.
- `cargo +1.99.1 test --offline --locked -p concord-clone-hygiene-check -p concord-clone-compat -p concord-hygiene-check -p concord-macros`: passed, including 4 clone-hygiene tests with four compile-fail cases, cross-crate localhost output decoding, 4 clone compatibility runtime tests, 12 positive/negative clone doctests, native-header hygiene, and 14 macro unit tests.
- `cargo +1.99.1 test --offline --locked -p concord --test attributes --test ui`: attributes and all selected UI snapshots passed, including legacy format/media diagnostics, the new output-type diagnostics, literal/raw hints and clone rejection. The original invocation named several additional packages but these two named targets executed for Concord.
- `cargo +1.99.1 test --offline --locked -p concord --test formats --test values --test composition`: 29 + 15 + 37 passed, including cfg-disabled formats, clone behavior and suppressed contribution controls.
- `cargo +1.99.1 test --offline --locked -p concord-example-renamed`: failed with the known independent M02 missing `extras` feature (`Binary`/`Text` imports), outside this compiler delta. Repeating with `--features concord4/extras` passed all 3 tests, including renamed-facade native headers.
- Independent public controls in copied `probes/consumer-1`: `cargo ... run --bin suppressed` printed `SUPPRESSED_OK`; `cargo ... check --bin suppressed_clone` exited 101 with the expected E0599 non-Clone bound. An initial attempt to remove a contribution on the same owner was invalid grammar and was corrected to an inherited override; it is not used as evidence of a product defect.
- Independent public controls in `probes/consumer-3`: `cargo ... run --bin forwarded_types` passed for a forwarded `$t:ty`, raw module and raw macro identifiers, brace and nested types. `cargo ... check --bin malformed_endpoint` exited 101 at `path ;` with `expected string literal`. `punct_original`, `punct_applied`, `punct_hint`, `invisible_query`, `group_macro_hint`, and the group applied fixture are described in the findings. The native Rust `macro_rules!` control for `(1 + 2) * 2` compiled with `Payload<6>`. `invisible_original` produced the same 6-versus-5 E0308 on candidate and released baseline.
- `cargo +1.99.1 doc --offline --locked --no-deps --bin doc_literal` generated real rustdoc HTML. Inspection of `struct.Probe.html` and `struct.ProbeEcho.html` found prose retaining `count("a :: b")` and `count(r#"c , d"#)`; rustdoc abbreviates the const expression in the displayed signature, so the applied-advice type and byte assertions are the stronger meaning control. `doc_group` generated HTML showing the forwarded initializer as `1 + 2 * 2`; that specific generated initializer also evaluated to 5, so it is not evidence of a prose/signature mismatch.
- `git diff --check 07dcdd0 f0693b5` passed; the review snapshot was unchanged by these probes.

## Untested and limits

This was a targeted Linux review, not a full workspace gate, cross-platform run, or exhaustive Rust grammar/token corpus. I did not execute every permutation of suppressed contributions, inherited cfg predicates, `None`-group nesting, lifetime jointness, raw literal prefixes/suffixes, or punctuation pairs such as comments and `<-`; nor did I independently benchmark code-generation size. The full original unfiltered consumer-2 test command was intentionally skipped under the review rules. Other candidate-stage work, including M02, is outside this verdict. No actual model or reasoning-effort setting was observable to this session.
