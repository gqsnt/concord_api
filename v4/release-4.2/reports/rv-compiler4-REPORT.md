# Compiler delta independent re-review, round 4

**Verdict: REVISE** for the compiler delta at `067024060d0b7b5366e05ef75a56bbb7fdd950f5`, relative to `07dcdd0`, scoped to `crates/concord-macros`, `crates/concord/tests`, and `checks/clone-hygiene`. The previously reported counterexamples are repaired in the controls below. The round-3 correction introduces a different compile failure for a forwarded type nested inside an initializer under `-D warnings`. This is a regression in the correction's context distinction and needs a widened fix and recheck.

## Finding: an initializer's type arguments are treated as expression fragments

**Location:** `crates/concord-macros/src/syntax.rs::parse_initializer`, `regroup_in` (around lines 1259 and 1318). The traversal starts in expression mode and carries that mode into the generic type argument of a turbofish. It consequently wraps every multi-token forwarded fragment there in parentheses, even when that fragment is a Rust type.

**Reproducer:** `probes/src/bin/round4_type_initializer.rs` (retained with logs). It has `#![deny(warnings)]` and compares an ordinary Rust macro with a Concord declaration:

```rust
macro_rules! plain { ($t:ty) => {
    fn ordinary() -> usize { std::mem::size_of::<$t>() }
} }
macro_rules! declare { ($t:ty) => {
    concord::api! { pub client Api { base "http://x.test";
        GET x(query n: usize = std::mem::size_of::<$t>());
    } }
} }
plain!(&'static str);
declare!(&'static str);
```

Copy the fixture into `crates/concord/examples/round4_type_initializer.rs`, then run `timeout 600 /home/canarit/projects/concord-v42/tools/heavy.sh cargo +1.99.1 check --offline --locked -p concord --example round4_type_initializer`. Remove the temporary example afterward. On `0670240` rustc exits 101 with `unnecessary parentheses around type`, pointing at `$t` in the Concord initializer; the equivalent ordinary macro emits no such warning. The emitted spelling is equivalent to `size_of::<(&'static str)>()`. The complete fixture has crate docs, a `main`, and no unrelated warnings; see `probes/logs/round4_type_initializer.log`.

**Expected / actual / impact:** A forwarded type in a type position should remain a type without newly introduced parentheses, including when that type position is nested inside an expression. The declaration compiled with denied warnings on the immediately preceding `e5082c4`; the candidate fails compilation for users who deny warnings. Standalone forwarded reference, vector and array types in body positions do pass the direct `-D warnings` control, so the failure specifically concerns the mixed context. `round4_type_baseline.log` records the successful `e5082c4` check in an archived baseline with its own target directory and lockfile. An initial baseline attempt shared the candidate target directory and produced a misleading failure; it is explicitly discarded, and the isolated rerun is the evidence used here.

**Introduced or pre-existing:** Newly introduced by `0670240` relative to `e5082c4`. The preceding implementation's `bare_fragment` recognizes this fragment as a type; the new outer expression mode no longer does so. The arithmetic/reference grouping defect fixed by the change is real and the required behavior must still be preserved.

**Strongest counterargument:** The request value is otherwise unchanged and the compile failure requires `unused_parens` to be denied. The grouping repair also fixes actual incorrect request values. However, warning-free forwarded types are an explicit re-review control; `-D warnings` is a routine supported Rust build configuration, and the same code compiled on the immediate baseline.

**Requested regression:** Distinguish nested type positions inside expressions, not just the outer initializer/const/array context. Add the exact ordinary-Rust versus Concord `size_of::<$t>()` test under `-D warnings`, with multi-token reference, generic and array types; include type arguments nested inside braced const arguments and arrays. Retain the new reference-expression runtime, const/array grouping, nested forwarding and opaque macro-input tests so that removing type parentheses does not reintroduce the original silent value change.

## Results for every prior reported counterexample

The `*_applied` fixtures that encode the **old wrong advice** are negative controls: their successful compilation and alternative body do not represent the current suggestion.

1. **Round 1, separate `. .` punctuation: fixed.** Current hint prints `choose!(. .)`; the original sends body `1`. The retained old applied `choose!(..)` sends `2`. The maintained applied-advice test applies the current suggestion and checks bytes. `punct_hint`, `punct_original`, `round4_old_punct_applied`.
2. **Round 1, invisible forwarded `(1 + 2)` precedence: fixed conservatively.** Current hint uses placeholder `T` rather than claiming an exact reconstructible type. Original forwarded type/query and body controls send/represent `6`; the old flattened applied spelling still selects `5`. The earlier `invisible_original` and `group_macro_original` body mismatch now also compile and send `6`. The maintained forward and applied tests exercise the corrected behavior. `group_macro_hint`, `group_macro_original`, `round4_old_invisible_{original,query}`, `round4_old_group_macro_{original,applied}`.
3. **Round 2, nested `> >` punctuation: fixed.** Hint preserves `a < b < c > > d`, original selects body `1`, and the old joined applied spelling selects `2`. The maintained current applied-advice test checks the correct result. `angles_{hint,original,applied}`.
4. **Round 2, trailing-dot float followed by dot: fixed.** Hint preserves `1. . 2`, original selects body `1`; the old `1..2` applied fixture selects body `2`. The maintained applied-advice control tests the corrected advice. `float_{hint,original,applied}`.
5. **Round 2, lost forwarded-fragment opacity: fixed conservatively.** For each of `ty`, `expr`, `literal`, and `path`, the current diagnostic names the payload `T` and explains the forwarded fragment; it does not offer the flattened token text as an equivalent replacement. Original opaque fixtures select body `2`, while the old flattened comparisons select body `1`. An ident and an ordinary-source control retain their spelling. A forwarded format path likewise uses `F`, and the fragment-preserving current declaration succeeds. See `opaque_*`, `placeholder_matrix`, `path_hint`, and `path_forwarded_current`.
6. **Round 2, forwarded initializer prose disagreed with request: fixed.** `doc_group` now sends `n=6`; generated rustdoc says `Initially (1 + 2) * 2`. The nested forward runtime control also passes. Real HTML was inspected after `cargo doc`; see `doc_group` and `doc_group_rustdoc` logs.
7. **Round 3, forwarded reference expression misgrouped as a type: fixed.** `fragment_silent` prints `rust=borrowed concord=n=borrowed`; `fragment_ref` passes, as does `fragment_const` for `[u8; $e.len()]`. Generated rustdoc says `Initially (&VALUE).meaning()`. Both the runtime and array fixtures contain equivalent ordinary Rust macro controls.

## Executed controls

All Cargo work used Rust `1.99.1 (58cf8f9c3 2026-10-13)`, Cargo `1.99.1`, `--offline --locked`, a finite `timeout`, and `/home/canarit/projects/concord-v42/tools/heavy.sh`. The candidate commands used the review snapshot's workspace source and lockfile. Probe source, scripts and raw logs are in `probes/`; scripts copy/use examples in the candidate and are retained for inspection. `scripts/verify.sh` was not run.

- Maintained F01–F04 and neighbors passed: `cargo +1.99.1 test --offline --locked -p concord-clone-hygiene-check -p concord-clone-compat -p concord-hygiene-check -p concord-macros`; and `cargo +1.99.1 test --offline --locked -p concord --test applied_advice --test output_types --test forwarded_types --test ui --test attributes --test values --test formats --test composition`. Both exited 0; logs are `maintained-packages.log` and `maintained-concord.log`. This includes same-printed hygienically distinct types, clone positive and compile-fail cases, output macro delimiters and paths, real applied literal/raw advice, diagnostics, cfg and composition. `endpoint_clone` emits a bound for each original value/body type with its span rather than deduplicating type identity by display text; `owners_cfg_predicate`'s textual key deduplicates cfg alternatives.
- `round4_matrix` compares ordinary Rust macros and Concord query values for unary, reference, cast, method call, range, closure, block, struct literal and nested forwarded expressions. It checks `[u8; $e.len()]` and `Marker<{ $e.len() }>` against ordinary Rust types and concrete `Prepared` types. All pass. An initial version contained an invalid ordinary block expression and fixture errors; it was corrected before the passing run and is not used as evidence.
- `round4_type_direct` passes with `#![deny(warnings)]` for forwarded `&'static str`, `Vec<u8>`, and `[u8; 3]` in body type positions with ordinary Rust type aliases. Its initial run failed solely because the fixture omitted docs for its public formatter; the corrected run passes. `round4_opacity` verifies a forwarded expression remains opaque to a macro matcher inside an initializer and matches the ordinary Rust result. The previous `fragment_matrix` checks nested expression/type forwarding, optional values, body and output types; `ordinary_contract` checks the no-fragment URL and bytes.
- Replayed 21 punctuation-pair hints and applied all 21 generated repairs with type/body assertions; joined-token controls passed. Replayed 17 lifetime/literal hints and applied all 17 with body assertions. Ampersand, slash, integer-dot and trait-object controls also passed. `run.log`, `run_more.log` record counts and exits; earlier wrong applied examples are negative comparisons.
- Deliberate `span_plain`, `span_forwarded`, and `span_matrix` failures point at the respective request/response format or query type declaration and include the forwarding invocation. The maintained UI snapshots pass. `git diff --check` passed, and the temporary candidate examples were removed; `git status --short` was clean.

## Untested forms and limits

This is a targeted Linux compiler-delta review, not a full workspace, release, native Windows, or exhaustive Rust grammar/lexer audit. The expression matrix samples the listed forms and positions; it does not cover every nesting, generic argument, lifetime, closure capture, operator, macro matcher, cfg/suppression, hygiene or span combination. I did not independently benchmark clone-bound code generation. Previous report controls are distinguished above from old incorrect applied advice; an old wrong spelling still changing semantics is expected and does not itself indicate a current unsafe suggestion. No actual model or reasoning-effort setting was observable.
