# Compiler delta re-review, round 6

**Verdict: REVISE** for `c69b9b41ed215c8bf43c687a717f6849bd4f49bf` against `07dcdd03c61a1822341e92d3820e66117f1aef14`, scoped to `crates/concord-macros`, `crates/concord/tests`, and `checks/clone-hygiene`. The maintained F01–F04 selections pass, as do the previous reported head counterexamples. A newly probed forwarded lifetime, however, makes an ordinary valid initializer fail to compile. This is a regression in the integrated compiler delta and prevents acceptance.

## Finding: a forwarded lifetime in an initializer is parenthesized as an expression

**Location:** `crates/concord-macros/src/syntax.rs::parse_initializer` and `regroup_in`, particularly lines 1260–1263 and 1321–1335. Expression mode treats a `Delimiter::None` group containing more than one token as an expression requiring parentheses. A forwarded lifetime such as `'static` also has multiple tokens, but parentheses are invalid in that type position. The parenthesis span suppresses the `unused_parens` lint; it cannot make this syntax valid.

**Reproducer:** Retained in `probes/src/round6_lifetimes.rs`, with a separately compiled ordinary-only control in `round6_lifetimes_plain.rs`. The essential declarations are:

```rust
macro_rules! plain { ($l:lifetime) => {
    fn plain() -> usize { std::mem::size_of::<&$l str>() }
} }
macro_rules! declare { ($l:lifetime) => {
    concord::api! { pub client Api { base "http://x.test";
        GET x(query n: usize = std::mem::size_of::<&$l str>());
    } }
} }
plain!('static);
declare!('static);
```

Copy the retained example to `crates/concord/examples/round6_lifetimes.rs` and run `timeout 300 /home/canarit/projects/concord-v42/tools/heavy.sh cargo +1.99.1 run --offline --locked -p concord --example round6_lifetimes`; remove it after running. On `c69b9b4` rustc exits 101 with `expected type, found lifetime` at `'static` and a second parse error at `str` in the Concord declaration. The ordinary-only control passes, as does the full example, including its prepared-query equality assertion, on archived baseline `07dcdd0`. On isolated archives of `e5082c4` and `0670240`, the same full example fails with the lifetime parse errors. Logs: `probes/logs/round6-{head,plain,07,e508,067}.log`.

**Expected / actual / impact:** A forwarded lifetime used in the type inside an otherwise valid initializer should retain Rust's meaning and compile like the ordinary macro. Instead Concord emits invalid syntax, preventing any use of that API declaration. This fails without relying on denied warnings; the fixture additionally denies warnings to match the requested controls.

**Introduced or pre-existing:** Introduced in the compiler delta relative to `07dcdd0`; already present at `e5082c4` and `0670240`, so not newly introduced by the final `c69b9b4` change. These comparisons do not isolate the first offending commit more narrowly.

**Strongest counterargument:** The demonstrated lifetime appears within a generic type inside an initializer, while the common forwarded expression and complete forwarded-type cases pass. The new grouping fixes real silent value changes, and treating expression fragments as types based on their spelling previously caused a different bug. Nevertheless a lifetime fragment is not a parenthesizable expression, the same public declaration worked on the baseline, and the requested widened review explicitly includes nested type positions and comparisons with ordinary Rust.

**Requested regression:** Retain this exact ordinary-versus-Concord compile/run test and add forwarded lifetimes in reference and generic type positions inside initializers and const/array expressions, including nested forwarding and macro-input opacity. Keep the expression reference, shift, turbofish, cast, `-D warnings`, and runtime grouping controls: a fix must not reintroduce those prior failures. Consider how non-expression fragment kinds are preserved without guessing expression meaning from type-like text.

## Previous counterexamples replayed

The task refers to three previous reports but lists five; I replayed the substantive reported cases from all five at both the explicitly requested `0670240` and the verdict head. In this table “fixed” describes the reported failure, not an assertion that every Rust form is correct. Old `*_applied` fixtures intentionally encode the old incorrect advice and are negative controls, not current suggestions.

| Reported counterexample | `0670240` | `c69b9b4` | Executed witness |
|---|---|---|---|
| Round 1 `. .` joined into `..` | Fixed | Fixed | Hint keeps `. .`; original selects body 1; old applied spelling selects body 2. |
| Round 1 forwarded expression loses precedence in advice / generated body | Fixed conservatively | Fixed conservatively | Hint uses placeholder `T`; original query and body controls retain 6; old flattened applied form selects 5. |
| Round 2 nested `> >` joined | Fixed | Fixed | Hint preserves separate tokens; original body 1 and old-applied body 2. |
| Round 2 `1. . 2` becomes `1..2` | Fixed | Fixed | Hint preserves tokens; original body 1 and old-applied body 2. |
| Round 2 forwarded `ty`, `expr`, `literal`, `path` loses macro matcher opacity | Fixed conservatively | Fixed conservatively | Four current hints use `T`, original selects body 2, old flattened text selects body 1; forwarded format and ordinary/ident controls also replayed. |
| Round 2 initializer documentation differs from request | Fixed | Fixed | Request `n=6`, generated rustdoc text `Initially (1 + 2) * 2`. |
| Round 3 forwarded borrow mistaken for type in initializer/array | Fixed | Fixed | `fragment_silent` prints `rust=borrowed concord=n=borrowed`; `fragment_ref`, `fragment_const` pass. |
| Round 4 `size_of::<$t>()` with warnings denied | Still failing | Fixed | `round4_type_initializer`: `unnecessary parentheses around type` at `0670240`; compiles at head. |
| Round 5 shift `1 << $e.meaning()` | Not applicable as regression at this earlier revision; passes | Fixed | Ordinary and Concord both give 4. The round-5 report identified this failure in the intervening `9c32294`. |
| Round 5 forwarded type nested in pointer cast with warnings denied | Still failing | Fixed | `round5_cast_pointer` gives `unnecessary parentheses around type` at `0670240`; compiles and runs at head. |

The copied round-5 replay runner executed each case independently at both revisions, with per-case exit and output logs in `probes/logs/{067,head}/`; `replay-{067,head}.log` both end with `UNEXPECTED CASES: []`. I additionally checked exact hint, runtime, and span witnesses in those logs; the runner's generic diagnostic checks alone would not prove them. Both round-5 cases were separately executed at each revision. The `0670240` source came from `git archive` and used its own target directory; the head used the detached review snapshot and its own target.

## Other executed controls

- `round4_matrix` compares ordinary `macro_rules!` functions against Concord prepared query values for unary operators, references, casts, method calls, ranges, closures, blocks, struct literals and nested forwards. It also checks a forwarded borrow in array lengths and braced const arguments against ordinary Rust types and prepared types. Passed at both revisions.
- `round4_type_direct` compares multi-token reference, generic and array types in direct type positions under `#![deny(warnings)]`; passed. `round4_type_initializer` and the round-5 pointer fixture establish that head's call-site parentheses do not trigger `unused_parens` in those nested expression contexts. Inspection shows head actually inserts parentheses there; the absence of the lint does not mean no parentheses were inserted. `round4_opacity` checks a forwarded expression remains opaque inside another macro input. `ordinary_contract` tests a declaration without forwarded fragments. All passed at head and `0670240`.
- `span_plain`, `span_forwarded`, and `span_matrix` deliberately fail with trait errors pointing to request/response format and query-type declarations, including macro forwarding invocations; I checked their recorded locations. The lifetime finding above shows an additional parse-error span at its forwarding invocation.
- At head, `timeout 900 tools/heavy.sh cargo +1.99.1 test --offline --locked -p concord-clone-hygiene-check -p concord-clone-compat -p concord-hygiene-check -p concord-macros` passed, including distinct hygienic types, positive/compile-fail clone controls, native-header hygiene, and 17 macro unit tests. The corresponding maintained Concord selection also passed: `cargo +1.99.1 test --offline --locked -p concord --test applied_advice --test output_types --test forwarded_types --test ui --test attributes --test values --test formats --test composition`, under the same timeout and wrapper. It includes actual applied literal/raw/token repairs, output macro types, legacy diagnostics, cfg/composition and runtime forwarding controls.
- Source inspection: `endpoint_clone` adds a span-bearing bound for every original value/body occurrence without deduplicating on display text. `parse_output` and the maintained output-type/UI tests cover the changed type-macro and legacy paths. Scoped `git diff --check` passed. Real rustdoc was generated for the forwarded initializer; the runner inspected its HTML text. Rust is `1.99.1 (58cf8f9c3 2026-10-13)`; Cargo commands used `--locked --offline`, finite timeouts and the heavy wrapper. `scripts/verify.sh` was not run. Temporary examples were removed and `git status --short` was clean.

## Untested forms and limits

This is a narrow Linux compiler review, not an exhaustive Rust grammar, hygiene, cfg, renderer, or release review. The new lifetime control tests `'static` in a reference under a turbofish; other lifetime binders, elided/inferred lifetimes, generic arguments, and nested lifetime forwarding were not exhaustively tested. I did not independently measure clone-bound code generation or execute native Windows checks. The actual model and effort setting were not observable.
