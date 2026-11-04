# Compiler delta independent re-review

**Verdict: REVISE.** Reviewed `aae6218bad751129ffae2ced6cdd8b2ce714f55c` against `07dcdd03c61a1822341e92d3820e66117f1aef14`, scoped to `crates/concord-macros`, `crates/concord/tests`, and `checks/clone-hygiene`. The two previously reported inputs are repaired in the controls below. The shared renderer still produces suggestions that change payload types and bytes in other cases, and the correction makes one generated initializer description disagree with the actual request. These are blocking under F03/F04 and the re-review rule.

Probe sources and output are in `probes/src/bin/` and `probes/logs/` beside this report. Candidate fixtures ran as temporary examples of the checked-out `concord` package, using the candidate workspace lockfile, not an installed release. Original and applied examples assert the selected type and prepared body bytes.

## Findings

### 1. Nested angle punctuation is joined across separate tokens

**Location:** `crates/concord-macros/src/syntax.rs::token_text`, the `generics > 0` exception in `walk` (around line 158).

**Reproducer:** `angles_hint.rs` declares the obsolete body type `choose!(a < b < c > > d)`, with macro arms `(a < b < c > > d) => Payload<1>` and `(a < b < c >> d) => Payload<2>`. The candidate says `body p: choose!(a<b<c>> d)` and `request Fmt;`. `angles_original.rs` uses current syntax with the original tokens and asserts body bytes `1`. `angles_applied.rs` uses the exact advised type and asserts bytes `2`. Both ran successfully; logs contain `BODY=1` and `BODY=2`, respectively.

**Expected / actual / impact:** The two separately written `>` tokens must not become the joined `>>` token in macro input. The renderer treats the earlier `<` tokens as generic delimiters, even inside arbitrary type-macro token trees, and suppresses the needed space. A valid-looking migration changes the selected macro arm and silently changes the request body. This is a remaining variant of the previous punctuation finding; the old `. .` case itself now passes.

**Strongest counterargument:** Space between genuine nested generic closers is usually cosmetic, and token-matching macros with comparison-like input are less common. Here the input is a valid public type macro, and the successful applied repair demonstrates a real payload change.

**Requested regression:** Retain this exact hint/original/applied type-and-byte test; preserve jointness inside macro token trees without assuming their contents are generics. Include joined and separate `>` in and outside genuine nested generic syntax.

### 2. A float followed by a dot becomes a range

**Location:** `syntax.rs::token_text`, literal/dot spacing in `walk` (around lines 161–164 and 188–191).

**Reproducer:** In `float_hint.rs`, `choose!(1. . 2)` selects the macro arm matching a float literal `1.`, then `.` and `2`; another arm matches `1 .. 2`. The advice is `choose!(1..2)`. `float_original.rs` runs with `Payload<1>` and asserts body `1`; `float_applied.rs` applies the advice, runs with `Payload<2>`, and asserts body `2`.

**Expected / actual / impact:** A trailing-dot float must remain a distinct literal when the next punctuation is `.`. The correction recognizes only an all-digit integer literal before a dot, so the printed spelling re-lexes as an integer plus range. Following the suggestion silently selects another payload.

**Strongest counterargument:** This token sequence is unusual and the ordinary `1 . 2`, `1 .. 2`, `1 ..= 2`, `1.5 . 2`, and `1.5` range controls pass. The language permits it in macro input, and an actual successful applied repair changes the request.

**Requested regression:** Add the exact float/dot applied test, plus a tokenization matrix of trailing-dot and fractional floats before `.`, `..`, and `..=`, checking matched arms and body bytes rather than only reparsing.

### 3. Printing a forwarded fragment as ordinary source removes its opacity to a second macro

**Location:** `syntax.rs::token_text`, `Delimiter::None` branch (around line 199), including the bare-single-token and bare-type decisions.

**Reproducer:** `opaque_type_original.rs` forwards `$x:ty` containing `u8` into `choose!($x)`. `choose` has `(u8) => Payload<1>` followed by `($x:ty) => Payload<2>`. Rust treats the forwarded type as opaque to the literal arm: the original current-syntax endpoint requires `Payload<2>` and sends `2`. `opaque_type_hint.rs` emits advice `choose!(u8)`; `opaque_type_applied.rs` applies that text and successfully sends `1`. Parallel `opaque_expr_*`, `opaque_literal_*`, and `opaque_path_*` controls demonstrate the same 2-to-1 change for `$e:expr`, `$l:literal`, and `$p:path`.

**Expected / actual / impact:** Advice must not silently replace a forwarded fragment with a spelling that a downstream macro interprets differently. Parentheses alone are not a general solution because both fragment opacity and the downstream matcher's grammar matter. This is separate from arithmetic precedence: the prior `(1 + 2) * 2` control now passes.

**Strongest counterargument:** A diagnostic is produced after macro expansion, and source text generally cannot encode the invisible fragment's identity; it may be impossible to construct a universal faithful literal replacement. That is why an apparently executable, meaning-changing repair is unsafe. A more conservative diagnostic can tell the user to retain the existing fragment expression instead of claiming the flattened text is a faithful replacement.

**Requested regression:** Keep public hint/original/applied controls for the four fragment kinds, including an opaque-versus-literal arm and type/byte assertions. Where faithful reconstruction cannot be established, avoid offering flattened executable code as a guaranteed repair.

### 4. Forwarded initializer prose now disagrees with the generated request

**Location:** `syntax.rs::token_text` invisible-group parenthesization, as used by `emit.rs::initially` / `expr_text`; compare `emit.rs::value_inits`.

**Reproducer:** `doc_group.rs` forwards `$e:expr = 1 + 2` into `GET x(query n: usize = $e * 2)`. `cargo ... run --example doc_group` prints `http://127.0.0.1:9/?n=5`. A real `cargo ... doc --no-deps --example doc_group` generates `target/doc/doc_group/struct.ProbeX.html` containing `Initially (1 + 2) * 2`. That expression evaluates to 6, whereas the actual generated request supplies 5.

**Expected / actual / impact:** Generated prose must describe the value the generated code actually uses. The preceding review observed both the old prose `1 + 2 * 2` and actual value 5; the correction changes the prose to 6 while the generated initializer still produces 5. This correction introduces a documentation/behavior contradiction and leaves the underlying forwarded-initializer grouping issue unresolved.

**Strongest counterargument:** The new text better describes the original forwarded Rust expression, and the generated initializer's 5 predates this correction. Correcting the runtime grouping to 6 would make that prose accurate; presenting 6 today while sending 5 remains misleading.

**Requested regression:** Exercise a forwarded and a nested forwarded initializer through generated code and real rustdoc, assert the intended const/runtime value and the documentation together, and retain the resulting behavior as a runtime assertion rather than a prose-only snapshot.

## Executed controls

All Cargo commands used `cargo +1.99.1`, `--locked`, a finite `timeout`, and `/home/canarit/projects/concord-v42/tools/heavy.sh`; the probes additionally used `--offline`. `scripts/verify.sh` was not run. Scripts and detailed outputs are retained under `probes/`. Two initial copied-consumer batches did **not** run: their 4.1 lockfile was incompatible with the 4.2 source and `--locked` rejected them. They are not counted as evidence. The successful batches use the candidate's committed workspace lock instead.

- `test --offline --locked -p concord-clone-hygiene-check -p concord-clone-compat -p concord-macros`: passed, including four clone-hygiene runtime/UI tests, cross-crate type-macro decoding, four clone compatibility tests, twelve clone doctests, and 16 macro unit tests. Inspection of `emit.rs::endpoint_clone` confirms each original field/body occurrence retains its own span-bearing higher-ranked bound; no display-text type deduplication is used.
- `test --offline --locked -p concord --test applied_advice --test output_types --test forwarded_types --test ui`: all passed, including actual applied literal/raw/token advice, direct and nested/qualified type macros, body/output types, legacy diagnostics, and all UI snapshots. The forwarded test checks query, body, optional body, and output with `Payload<6>`.
- `test --offline --locked -p concord --test composition --test attributes`: passed, including cfg, overrides and suppressed contributions. This does not exhaust cfg/hygiene interactions.
- Re-ran the two previous independent cases: `. .` is advised unchanged and its current-syntax original sends `1`; forwarded `1 + 2` is advised as `pick!((1 + 2) * 2)` and its original sends `6`. The maintained applied-advice test executes both suggested replacements. An additional two-layer forwarded-expression hint/original/applied probe also yields `Payload<6>` and body `6`.
- `ops_hints`, `ops_original`, `ops_applied`, and `ops_joined` tested 21 two-character pairs (`::`, `->`, `=>`, comparisons, compound assignments, `&&`, `||`, shifts, `..`, `<-`) in token-sensitive type macros. All 21 separate-token hints were applied automatically and checked against type `Payload<1>` and body `1`; joined controls asserted `Payload<2>` and body `2`. Independent `& &`, `/ /`, and `1 . 2` hints were applied and their byte checks passed. The nested-angle counterexample above is distinct from these shallow inputs.
- `lit_hints`, `lit_original`, and `lit_applied` checked and applied 17 suggestions, asserting the selected type and bytes: `'a`, `'static`, char/escaped char/byte char, normal/raw/byte/raw-byte/C/raw-C string tokens with suffixes, integer before `.`, `..`, `..=`, and `1.5` before those operators. All passed; the separate trailing-dot float finding did not.
- A forwarded `$f:path` (`crate::formats::Fmt`) generated that same path in `request`; the applied endpoint sent `good`. Direct and forwarded deliberate `Encode<Payload<3>>` mismatches both produced E0277 pointing at the `request Fmt` declaration; compare `span_plain.log`, `span_forwarded.log`, and the maintained `representation_unsupported_payload.stderr` and `endpoint_clone_needs_clone_values.stderr` snapshots. This is sampled span evidence, not a guarantee for every nested group.
- Generated actual rustdoc for `doc_group` and compared its initializer prose with the request URL, producing finding 4. `git diff --check 07dcdd0 aae6218` passed.

## Existing boundary and limits

A separate `dyn_original.rs` probe forwards `$t:ty = dyn Send + Sync` into body type `&'static $t`. Current Concord reports `ambiguous + in a type`, while the equivalent ordinary Rust forwarding control `dyn_rust.rs` compiles. The suggested parenthesized `&'static(dyn Send + Sync)` form compiles and its applied endpoint sends `dyn`. I archived the immediately preceding `f0693b5` tree (whose workspace packages are 4.1.0) and ran the same locked example: it also reports ambiguous `+`, so this is a demonstrated pre-existing boundary, not attributed to `aae6218`. It should be tracked separately if such forwarding is intended to be supported; the successful applied case does not prove that the original works.

This targeted Linux review did not run the full workspace gate, Windows, every cfg/clone suppression combination, all multi-character operator triples and punctuation adjacency combinations, or every nested fragment/format-path/type-position permutation. The literal corpus covers the listed spellings, not arbitrary suffixes or all raw-string delimiters. I did not benchmark clone-bound code-generation impact. The preceding baseline archive was used only for the explicit `dyn` comparison; it is not evidence for candidate tests. Model identity and reasoning effort were not observable.
