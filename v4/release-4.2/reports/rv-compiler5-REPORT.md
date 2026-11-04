# Compiler delta independent re-review, round 5

**Verdict: REVISE.** Reviewed `9c3229458bafb3f3441d5b76bb176961b2a118f9` against `07dcdd03c61a1822341e92d3820e66117f1aef14`, limited to `crates/concord-macros`, `crates/concord/tests`, and `checks/clone-hygiene`. All maintained F01–F04 tests selected below pass. The round-4 `size_of::<$t>()` failure is fixed at the reviewed head, but the new angle heuristic silently changes a forwarded expression after `<<`. A second forwarded-type cast case still fails under `-D warnings`. Both need correction and interaction tests.

Fixtures, runner, and logs are retained under `probes/` beside this report. They are independent public Concord consumers built as temporary workspace examples. Sources for historical comparisons were made with `git archive` at the stated commits and separate target directories. The snapshot was clean after removing the temporary examples. No actual model or effort setting was observable.

## Blocking findings

### 1. A shift is mistaken for an opening type angle and changes a request value

**Location:** `crates/concord-macros/src/syntax.rs::regroup_in`, especially lines 1320–1329 and 1337–1342. In `1usize << $e.meaning()`, the first `<` follows an operand, but it changes `operand` to false; the second `<` is therefore treated as an opening type bracket. The following forwarded group is then classified by `bare_fragment` as a type, even though it is an expression. Its invisible grouping is lost on the proc-macro round trip.

**Reproducer:** `probes/src/round5_shift.rs` defines a `Copy` type `Foo`, a static `VALUE`, and a `Meaning` trait that returns `1` for `Foo` and `2` for `&Foo`. Both an ordinary macro and a Concord declaration forward `&VALUE` as `$e:expr` into `1usize << $e.meaning()`. Run the fixture as a temporary `concord` example:

```sh
timeout 300 /home/canarit/projects/concord-v42/tools/heavy.sh cargo +1.99.1 run --offline --locked -p concord --example round5_shift
```

The ordinary macro respects fragment grouping and produces `1 << 2 = 4`. On `9c32294` the actual request query is `n=2` and the assertion fails, printing `rust=4 concord=n=2` (exit 101). On an archived `0670240` with an independent target directory, the same source prints `rust=4 concord=n=4` and exits 0. Logs are `probes/logs/round5_shift.log` and `round5_shift_067.log`.

**Expected / impact:** An initializer forwarded through `macro_rules!` must retain ordinary Rust expression semantics. This silently changes prepared request data, not merely formatting. It is newly introduced after `0670240` by the mixed-context traversal added in `1dad499`.

**Strongest counterargument:** The reproducer combines a left shift, a forwarded borrow, and type-dependent method dispatch; the maintained simple reference and turbofish tests pass. It is nevertheless valid public syntax and generates different bytes without a diagnostic. The `0670240` comparison isolates the regression from the preceding fix.

**Requested regression:** Retain the ordinary-versus-Concord runtime assertion. Cover both characters of `<<` and `<<=`, other comparison/shift operators adjacent to forwarded fragments, and type angles in turbofish and qualified paths. Include array lengths and braced const arguments, nested forwards, and opaque macro inputs so the expression fix and the type-context fix are exercised together.

### 2. A forwarded type inside a pointer cast receives lint-inducing parentheses

**Location:** `syntax.rs::regroup_in`, lines 1320–1329 and 1336, with the `Delimiter::None` handling at 1351. The `cast` flag recognizes only the immediately following token after `as`. In `as *const $t`, the `*` consumes that state and the later forwarded multi-token type is treated as an expression, which causes it to be parenthesized.

**Reproducer:** `probes/src/round5_cast_pointer.rs` has `#![deny(warnings)]` and equivalent ordinary and Concord macros using `(std::ptr::null::<u8>() as *const $t).is_null() as usize`, with `$t = core::primitive::u8`. Execute it as a temporary example with the command above, substituting `round5_cast_pointer`. At `9c32294` rustc exits 101: `unnecessary parentheses around type`, pointing to `$t` in the Concord declaration. The plain Rust macro in the same fixture does not trigger the lint. The exact fixture succeeds and runs on isolated `e5082c4`; it fails with the same lint on isolated `0670240`. See `probes/logs/round5_cast_pointer{,_067,_e508}.log`.

**Expected / impact:** A forwarded type in the type operand of a cast must remain warning-free just as in the ordinary macro. This blocks builds that deny warnings. The failure was introduced by the `0670240` regrouping correction relative to `e5082c4`; the current `1dad499` fix resolves a directly forwarded cast type but not this nested pointer form.

**Strongest counterargument:** The generated cast has the intended value when the lint is permitted, and the regrouping change repairs real wrong-value bugs. Nonetheless `-D warnings` is an ordinary supported configuration and warning-free nested type positions are an explicit re-review control.

**Requested regression:** Keep this exact plain-Rust/Concord `-D warnings` comparison and test `*const` and `*mut` casts with multi-token path, reference, generic and array types where valid. Also retain directly forwarded casts, qualified and turbofish types, expression grouping, and macro-input opacity.

## Results for previous reported counterexamples

Both historical `0670240` and final `9c32294` were executed, including all four reports' substantive counterexamples. The cases called `*_applied` below are deliberately retained **old wrong advice**, used as negative comparisons; they are not the current advice. Per-case logs and exit/witness results are in `probes/logs/{067,head}/` and `probes/replay-{067,head}-corrected.log`.

| Prior counterexample | `0670240` | `9c32294` | Executed evidence |
|---|---|---|---|
| Round 1: `. .` joined to `..` | Fixed | Fixed | `punct_hint` preserves `. .`; original selects body 1; old applied `..` selects body 2. Maintained applied-advice tests pass. |
| Round 1: invisible `(1 + 2)` grouping lost, including body mismatch | Fixed conservatively | Fixed conservatively | `group_macro_hint` uses placeholder `T`; original/query/body controls select 6, old flattened applied form selects 5. |
| Round 2: separate nested `> >` joined | Fixed | Fixed | `angles_hint` preserves the tokens; original body 1 and old applied body 2 both assert successfully. |
| Round 2: `1. . 2` rewritten to `1..2` | Fixed | Fixed | `float_hint` preserves the tokens; original body 1, old applied body 2; maintained applied-advice test passes. |
| Round 2: forwarded type, expression, literal or path opacity lost | Fixed conservatively | Fixed conservatively | Four hint/original/old-applied triples use `T`, select body 2 originally and body 1 in old flattened controls. Forwarded format uses `F`; `path_forwarded_current` runs. Ident and plain controls preserve their spelling. |
| Round 2: forwarded initializer prose disagreed with runtime | Fixed | Fixed | `doc_group` requests `n=6`; generated `struct.ProbeX.html`, parsed as HTML text, says `Initially (1 + 2) * 2`. |
| Round 3: forwarded `&VALUE` / `&DATA` misclassified as types | Fixed | Fixed | `fragment_silent` prints `rust=borrowed concord=n=borrowed`; `fragment_ref` and array-length `fragment_const` pass. |
| Round 4: forwarded type in `size_of::<$t>()` under `-D warnings` | Still failing | Fixed | `round4_type_initializer` fails at `0670240` with `unnecessary parentheses around type`, and passes at head. Finding 2 above is a different cast position that remains broken. |

The first replay runner's HTML check looked for contiguous raw HTML and falsely reported no rustdoc hit because rustdoc inserts markup. I corrected it to compare parsed HTML text and reran both full replays; both corrected runners exit 0 with no unexpected cases. The raw logs retain the earlier false alarm.

## Executed controls and source inspection

- All Cargo work used `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, Cargo 1.99.1, `--offline --locked`, finite timeouts, and `/home/canarit/projects/concord-v42/tools/heavy.sh`. The replay runner uses a per-command 180-second timeout as well as the outer 900-second timeout. `scripts/verify.sh` was not run.
- At head, `cargo +1.99.1 test --offline --locked -p concord-clone-hygiene-check -p concord-clone-compat -p concord-hygiene-check -p concord-macros` passed, including public multi-crate same-printed hygienic types, clone successes and compile-fail checks, and macro unit tests. `cargo +1.99.1 test --offline --locked -p concord --test applied_advice --test output_types --test forwarded_types --test ui --test attributes --test values --test formats --test composition` also passed. Full logs are `maintained-packages.log` and `maintained-concord.log`.
- `round4_matrix` passed on both revisions, comparing ordinary macros to generated initializers for unary, borrow, cast, method, range, closure, block, struct literal and nested forward; it also checks a forwarded borrow in an array length and braced const argument against ordinary Rust and concrete prepared types. Finding 1 widens this matrix to shifts. `round4_type_direct` and `round4_type_initializer` cover multi-token reference, generic and array types under denied warnings; finding 2 widens this to a cast's nested pointee type.
- `round4_opacity` checks a forwarded expression in a macro input against ordinary Rust, while `fragment_matrix`, `fragment_nested`, and `ordinary_contract` cover nested forwards, optional/body/output positions, and no-fragment URL/body behavior. The macro-input fixture passed. `span_plain`, `span_forwarded`, and `span_matrix` intentionally fail, with trait errors pointing to the body request format, output response format, and query type declarations, including forwarding invocations.
- Inspection of `endpoint_clone` shows one span-bearing bound for each original value/body occurrence and no type identity reconstructed from displayed text. `parse_output` has the macro-`!` lookahead before its legacy-format path. `git diff --check` for the scoped delta passed. The broader repository diff has unrelated historical trailing whitespace; it is outside this review scope.

## Untested forms and limits

This is a narrow Linux compiler review, not a complete workspace/release or native Windows gate. It is not an exhaustive Rust parser, operator, literal, cfg, hygiene, type nesting, or macro matcher proof. In particular, the new failing `<<` control does not establish behavior of every shift/comparison context; pointer casts do not cover all possible nested cast types. I did not independently benchmark clone-bound code generation. Passing representative previous cases and maintained tests does not offset the two demonstrated failures.
