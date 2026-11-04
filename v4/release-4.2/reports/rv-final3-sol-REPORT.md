# Independent release re-review — rv-final3-sol

**Verdict: REVISE for re-review head `4619334c54a93b6d420deb480ae0c0111b814da9`.** The exact C1/C2/T1 corrections work, but the widened review finds a newly introduced rustdoc delimiter regression and a remaining M03 prefix bypass. Both require correction. Passing maintained tests and the central gate do not cover them.

The deliverable also explicitly asks for a verdict **for `a6b78a1`**. That comparison source, `a6b78a18c81b7ea1adf0d76da6e3c714f8a0e95d`, remains **REVISE**: I independently reproduced its C1, C2 and T1 failures. Results below distinguish the two commits; no execution on either is relabelled as execution on the other.

I authored none of the reviewed patches and spawned no agents. I read both previous final reports, compiler reports 1–6, the supplied compiler/tooling/release stages, Brief D, the re-review rule and acceptance matrix, and inspected the complete 15-file `a6b78a1..4619334` diff. Execution was on native Linux, `x86_64-unknown-linux-gnu`, Rust/Cargo **1.99.1**, on 2026-11-04 UTC. No actual model/effort setting was exposed to this review. [Source receipt](source-receipt.json) records the compiler, source, lock and final clean state.

## Blocking correctness

### C3 — P2: the negative-literal renderer removes explicit array, block and macro delimiters

**Location:** `crates/concord-macros/src/syntax.rs::token_text::walk`, lines 302–311, particularly `if !types && literal_fragment(&g)` at line 304; `literal_fragment` at line 1289. The helper checks only the group's contents. Unlike the regrouping branch, this renderer branch does **not** require `g.delimiter() == Delimiter::None`. It consequently treats ordinary brackets, braces and macro invocation parentheses containing a negative literal as invisible forwarded fragments.

**Reproducer:** [doc_negative_groups.rs](head/probes/src/doc_negative_groups.rs) has no forwarded fragments, denies warnings, compares runtime results to ordinary Rust, and declares:

```rust
concord::api! { pub client Api { base "http://localhost";
    GET array(query n: usize = [-1isize].len());
    GET branch(query n: isize = if true { -1isize } else { 2isize });
    GET opaque(query n: usize = count!(-1isize));
} }
```

Here `count!` is an ordinary local literal macro returning the singleton array's length. From this lane, `python3 reproduce.py doc_negative_groups --doc` runs the retained fixture and builds real rustdoc through the heavy wrapper, with `+1.99.1 --offline --locked`. Add `--baseline` to execute the identical fixture on `a6b78a1`.

**Expected / actual:** both commits prepare the ordinary Rust results (`n=1`, `n=-1`, `n=1`). However, real generated initializer prose changes as follows:

| Equivalent baseline prose on `a6b78a1` | Actual `4619334` prose |
|---|---|
| `[-1isize].len()` | `(-1isize).len()` |
| `if true { -1isize } else { 2isize }` | `if true -1isize else { 2isize }` |
| `count!(- 1isize)` | `count!- 1isize` |

The first describes a nonexistent method on an integer; the other two are invalid Rust. [Head HTML text](head/negative-rustdoc-text.json) and [baseline HTML text](a6b78a1/negative-rustdoc-text.json) were extracted from actual generated HTML with `HTMLParser`, not inferred from source strings. Runtime and rustdoc command logs are `head/logs/doc-negative-{runtime,rustdoc}.log`, with corresponding baseline logs.

I also extracted these actual descriptions and compiled each as an ordinary Rust initializer with `rustc +1.99.1 --edition=2024 -D warnings`. All three baseline descriptions compile and execute; all three head descriptions fail (rustc exit 1): E0599 for `.len()`, a missing `if` block, and a missing macro delimiter. Retained sources are `head/probes/src/rustdoc_applied_{array,branch,opaque}.rs`; logs are `head/logs/rustdoc-applied-*.log`. [validate.py](validate.py) retains this extraction/application check.

**Introduced or pre-existing:** **introduced by `a6b78a1..4619334`**. Identical ordinary declarations and dependency locks give correct prose on the comparison commit. This also disproves the requested no-fragment unaffected control for generated documentation, although the executed request behavior stays correct.

**Impact:** F03's generated expression descriptions no longer preserve Rust meaning. Users see an impossible or syntactically invalid initializer for a valid API declaration. No request-byte regression is claimed for these fixtures.

**Strongest counterargument:** this is documentation only, while the negative-literal range and method-call runtime repairs are correct. That limits the impact, but faithful generated prose is an explicit mandatory F03 outcome, and this regression affects ordinary declarations without any forwarding.

**Requested regression:** restrict the fragment-specific delimiter rule to genuinely invisible groups; preserve all explicit delimiters, including the group after a macro's `!`. Retain real-rustdoc tests for the three ordinary cases, together with runtime/ordinary-Rust controls. Keep negative fragment method calls, lower/upper range bounds, const/array nesting and opaque macro-input controls passing.

### T2 — P2: prefixed literal `exit`/`exec` still certify a gate that never runs

**Location:** `scripts/check-ci.py::Guard.job`, lines 913–920, particularly the raw first word at line 914. The new `shell_words` normalization (591–601) is used by the two `set` inspectors, while the `CONTROL` check still inspects the unnormalized first word.

**Reproducer:** [workflow-prefix_exit.yml](workflow-prefix_exit.yml) retains the maintained Linux/Windows matrix and nightly lane, with the gate step:

```sh
builtin exit 0
scripts/verify.sh --package --bench
```

From the reviewed root:

```sh
timeout 20 python3 -I scripts/check-ci.py --workflow-file \
  /home/canarit/projects/concord-v42/lanes/rv-final3-sol/workflow-prefix_exit.yml
```

**Expected:** exit 1, no effective full gate, with the same conservative control-command refusal as direct `exit 0`. **Actual on both commits:** exit 0 and `effective full gate on: Linux, Windows`. See [head log](head/logs/workflow-prefix_exit.log) and [baseline log](a6b78a1/logs/workflow-prefix_exit.log).

Independent `bash --noprofile --norc -eo pipefail -c` execution, replacing the gate with `printf 'GATE_EXECUTED\n'`, exits 0 with empty stdout. Direct `exit 0` is correctly rejected by both checkers; safe direct and `builtin set -euo pipefail` controls are accepted and print the witness. `command exit 0`, `REVIEW_OPTION=1 exit 0` and `builtin exec true` also pass both checkers and skip the Bash witness. [tooling.py](tooling.py), [tooling-results.json](tooling-results.json), retained workflows and `shell-*.log` record every command, result and log hash.

**Introduced or pre-existing:** **pre-existing on `a6b78a1`**, not introduced by the new `shell_words` function. It is a remaining M03 correctness gap adjacent to the repaired T1 prefix bypass. T1 itself is resolved for the executed noexec forms.

**Impact:** the static guard certifies a statically skipped gate despite its documented policy rejecting literal control commands. This does not invalidate the actual unmutated workflow or the central gate execution.

**Strongest counterargument:** the checker intentionally supports a bounded shell grammar and cannot evaluate arbitrary programs. These are literal prefixes and literal control builtins in the gate's own step, already recognized separately by this parser. Conservatively rejecting them needs no general shell evaluator.

**Requested regression:** use the supported prefix normalization, or conservatively refuse prefixed control commands, before the `CONTROL` decision. Retain executable negative cases for `builtin`/`command`/assignment-prefixed `exit`, and `builtin exec`, alongside direct controls, safe prefixes, and all repaired noexec/errexit cases.

No additional optional polish finding is needed for this verdict.

## Exact previous findings, independently executed

| Prior finding | `a6b78a1` execution | `4619334` execution |
|---|---|---|
| C1 forwarded `dyn Send + Sync` inside `size_of::<&$t>()` initializer | Exit 101, `ambiguous '+' in a type` | Exit 0, `ordinary=16 concord=n=16` |
| C2 forwarded negative literal as a range-pattern bound | Exit 101, `range pattern bounds cannot have parentheses` | Exit 0, `ordinary=7 concord=n=7` |
| T1 `builtin set -n`, `command set -o noexec`, assignment-prefixed `set -n` | Each checker exits 0 and certifies Linux/Windows; Bash skips the witness | Each exact retained workflow exits 1 with `turns on noexec`; Bash still skips the witness |
| Previous R1 forwarded qualified path heading a struct literal | Pass | Pass; exact retained source prints `FORWARDED_STRUCT_LITERAL_PASSED` |
| Previous R2 direct `set -n` and `set -o noexec` | Both exact retained workflows rejected | Both rejected with noexec witness; combined `set -eun` also rejected |

C1/C2 paired fixtures are unchanged copies of the preceding review, with the ordinary Rust function and runtime equality assertions. Logs are `{head,a6b78a1}/logs/exact-repro_{dyn,range}.log`. C1 and C2 also have passing no-fragment direct controls on both sources. Their preceding dispositions remain as documented: C1 is a regression relative to `9c32294`; C2 already existed there and was a regression relative to 4.1. I did not rerun those two older commits here and do not present their historical classification as new execution.

Exact T1/R2 YAMLs are retained at this lane's top level and executed as `exact-workflow-*.yml.log` under each variant. Widened noexec controls also reject `command -p set -n` and `REVIEW_OPTION=1 command -- builtin set -n` at head; both bypass the comparison checker. Safe shell-option controls pass. The maintained `forwarded_types` test executes both the endpoint and owner-default struct-path controls. The previously optional restored 4.1 changelog heading is preserved by this delta.

## Compiler rounds 1–6: complete counterexample replay

I independently ran the preceding review's **75-step replay on each commit**, with only source/output paths changed. Each finishes `UNEXPECTED: []`; this is **150 executed Cargo steps**, including real rustdoc builds and current-advice application, not the coordinator's execution claim. [Head replay](head/replay.py.log), [baseline replay](a6b78a1/replay.py.log), per-case logs and [witness validation](head/replay-witness-validation.json) retain the results.

| Counterexample | Result on both tested commits |
|---|---|
| Round 1 separate `. .` punctuation | Current hint retains separate tokens; original selects body 1; deliberately old `..` application selects body 2. |
| Round 1 invisible arithmetic precedence / generated body mismatch | Conservative current `T` hint; original query/body controls retain 6; old flattened application retains its negative result 5. |
| Round 2 nested separate `> >` | Hint preserves separate tokens; original body 1 and old joined body 2. |
| Round 2 trailing-dot float `1. . 2` | Hint preserves tokens; original body 1 and old range application body 2. |
| Round 2 opaque `ty`, `expr`, `literal`, `path` | Current hints use `T`; original opaque arms select body 2; formerly flattened applications select body 1. Ident/plain spelling and forwarded-format `F` controls pass. |
| Round 2 initializer prose/runtime mismatch | Runtime `n=6`; actual rustdoc text contains `Initially (1 + 2) * 2`. |
| Round 2 direct forwarded trait-object reference | Original, applied and ordinary Rust controls pass; distinct from the nested initializer C1 above. |
| Round 3 reference-expression dispatch / array length | `rust=borrowed concord=n=borrowed`; reference and array-length type controls pass; actual rustdoc contains `Initially (&VALUE).meaning()`. |
| Round 4 forwarded turbofish type with denied warnings | Exact initializer, reference/generic/array direct-type controls pass. |
| Round 5 shift / nested pointer-cast type | Exact fixtures pass; shift prints `rust=4 concord=n=4`. |
| Round 6 forwarded lifetime | Exact paired and ordinary-only controls pass with warnings denied. |

The replay additionally extracts and applies **21 operator/punctuation and 17 lifetime/literal suggestions** from each commit's diagnostics and checks the resulting payload/body assertions. Nested-forward, ampersand/slash/integer-dot, macro-opacity and deliberate trait-error span controls execute. Old wrong applied spellings are negative comparisons, not claims about current advice.

## Widened fragment comparisons under denied warnings

I independently reran all **74 previous matrix cases**, then added **15 neighboring negative-literal, nested-forward, pattern, struct-literal and trait-object type cases**. Every one of the **89 ordinary macro controls** compiles with `rustc +1.99.1 --edition=2024 -D warnings` and executes on both variants. Paired fixtures deny warnings without exceptions and assert prepared query equality with their ordinary macro function.

I then reran all **89 paired cases per commit with the explicit `RUSTFLAGS='-D warnings'`**, fresh variant-specific target directories and locked/offline Cargo commands. This confirms the same outcomes:

| Comparison source | Original 74 paired cases | Added 15 paired cases | Explicit `-D warnings`, all 89 |
|---|---:|---:|---:|
| `a6b78a1` | 63 pass / 11 compile failures | 11 pass / 4 compile failures | 74 pass / 15 compile failures |
| `4619334` | 71 pass / 3 compile failures | 15 pass | 86 pass / 3 compile failures |

At head the only matrix failures are the three **previously documented forwarded `let`-statement parse boundaries**, not new failures in C1/C2. Comparison failures include the four original C1 variants, four original C2 variants, an additional nested negative range bound, three additional trait-object type positions, and those same three statement cases. The coordinator summary on `67006bb` was inspected as a claim; these numbers come from this review's executions on the two named commits.

| Fragment kind | Cases | Executed legal positions and interactions |
|---|---:|---|
| `expr` | 10 | Arithmetic, borrow/dispatch, shifts/assignment, const arguments, array lengths, nested expressions, negative method receivers and macro inputs. |
| `ty` | 19 | Turbofish, `as`, `*const`/`*mut`, path/reference/generic/array pointees, qualified paths, const/array nesting, trait objects, nested forwarding and macro inputs. |
| `path` | 6 | Struct literals, type/pointer positions, const arguments, array lengths and macro inputs. |
| `ident` | 5 | Struct literals, type positions, const arguments, array lengths and macro inputs. |
| `lifetime` | 6 | Reference/nested generic types, const/array expressions, higher-ranked binder and macro inputs. |
| `literal` | 15 | Negative method receivers, lower/upper ranges, const/array expressions, struct literals, nested forwarding and macro inputs. |
| `pat` | 6 | Match/let and negative range patterns, const arguments, arrays and macro inputs. |
| `block` | 5 | Initializer, const/array expressions, negative method receiver and macro inputs. |
| `stmt` | 7 | Let/item statements in initializer/const/array blocks and macro inputs. Three let cases fail on both commits. |
| `tt` | 5 | Grouped arithmetic/negative receiver, const/array expressions and macro inputs. |
| `vis` | 5 | Empty and `pub(crate)`, local items in initializer/const/array blocks, client visibility and macro inputs. |

All eleven fragment kinds include ordinary-versus-Concord expression-macro and type-macro matcher controls. Positions are selected where the kind is legal Rust, not an assertion that every fragment kind can inhabit every slot. The 15 added definitions are [extra-cases.json](extra-cases.json); original definitions are [matrix-cases.json](head/matrix-cases.json). Sources are retained under each variant's `probes/src/`; [strict_matrix.py](strict_matrix.py), strict summaries/results and [results-summary.json](results-summary.json) retain exact outcomes.

The unchanged `stmt` boundary is `parse_initializer` rejecting `{ $s 6usize }` for `$s:stmt = let _ = 3usize` before regrouping, with `unexpected token, expected ';'`. Both ordinary controls return 6; both Concord variants fail; direct declarations and forwarded const-item statements pass. This remains an explicitly recorded pre-existing parser limit, not a newly introduced defect or a claimed repair. Older 4.1 classification is from the preceding report, not re-executed here.

No-fragment runtime controls (`ordinary_contract` and direct trait-object/range/let declarations) pass on both commits. C3 separately shows that no-fragment **documentation** is affected.

## Maintained F01–F04/M01–M04/T01 and tool checks

Independently executed against **`4619334`**. All Cargo commands used `+1.99.1 --offline --locked`, finite timeouts, review-owned targets and `/home/canarit/projects/concord-v42/tools/heavy.sh`. No unfiltered tests ran in replay consumers.

| Scope | Selection | Observed |
|---|---|---|
| F01/F02 multi-crate/hygiene | `cargo test -p concord-clone-hygiene-check -p concord-clone-compat -p concord-hygiene-check` | Passed, including 4 clone-hygiene tests with 4 compile-fail cases, localhost output decoding, 4 clone-compat runtime tests, native-header hygiene and 12 clone doctests. |
| F02–F04 and runtime neighbors | `cargo test -p concord --test output_types --test applied_advice --test forwarded_types --test lifecycle --test d16_followup --test accept --test bodies --test composition --test values` | 73 tests passed, including D16's 3 trybuild cases and current C1/C2/path/lifetime assertions. |
| UI | `cargo test -p concord --test ui` | 2 harnesses; 83 UI cases pass, without rewritten snapshots. |
| Compiler units / generated prose controls | `cargo test -p concord-macros` | 17 passed, including the new negative-literal doc assertion; it does not cover explicit delimiters in C3. |
| M01 comparator and registered contracts | `cargo test -p concord-bench` | 10 passed, including altered Accept, repeated headers/order, URL/method, body, native configuration and sensitivity negatives. No timing claim. |
| M02 renamed example alone | `cargo test -p concord-example-renamed` | 3 passed. |
| M02 true isolated no-extras consumer | Copied maintained consumer, changed local source paths only; `cargo test --lib --tests` plus feature tree, each locked/offline | 3 passed. `concord-extras` is an unused patch, absent from the actual graph; no JSON/form codec features enabled. Its registry lock closure is a subset of the candidate's. |
| T01 and native positives | `cargo test -p concord-example-native` | 30 passed, including 3 metadata-stage tests and body/error controls. |
| M03/M04 tool tests | `python3 -I -B -m unittest discover -s scripts/tests -p 'test_*.py' -v` | 75 passed, including T1 prefix negatives and mutation-helper retirement/no-caller/refusal controls. T2 is an additional uncovered negative. |
| Fresh output controls | `bash scripts/tests/output-dirs.sh` | 14 controls passed. |

[run_review.py](head/run_review.py) retains the maintained-selection driver. Exact argv, cwd, target, timestamps, expected/actual exits, witnesses and log hashes are in [head/final-receipts.json](head/final-receipts.json) and [baseline receipts](a6b78a1/final-receipts.json). One reviewer-only validation attempt initially expected Cargo's 101 for a direct rustc rejection; rustc correctly returned 1. The runner was corrected and completed. Final receipts deduplicate that exploratory classification; original append-only receipts retain it. No product failure was hidden or reclassified as a pass.

## Source closure, central evidence and limits

The snapshot was clean initially and is clean after removing 258 temporary examples. All **1,847 tracked files** match their initial byte hashes; `git diff --check a6b78a1 4619334` passes. Head and comparison have byte-identical committed workspace locks. Sources were compiled from their named paths with separate target directories, including separate targets for the explicit-warning run. All four production packages resolve the inherited **4.2.0** version. [source-hashes.json](source-hashes.json), variant fixture hashes, final receipt hashes and [source-receipt.json](source-receipt.json) bind these observations.

The completed full Linux gate was **inspected from the central log**, not executed independently as `scripts/verify.sh`. Its source header identifies `4619334`, dirty count 0 and rustc 1.99.1; it records **37 PASS steps and exit 0**, including workspace tests (676 passed), feature lanes, format/clippy, generated/records checks, isolated examples, docs, bench compilation, fresh packages and packaged consumers. [Inspected copy](central-gate-inspected.log), SHA-256 `cc893e96bcaf03babd1f63121f0afec1ff241800d76410a91bca351849ba8dc0`. Fresh package extraction and broader feature/package coverage are relied on only as inspected central evidence.

Native Windows was not executed or independently certified. The current status still says `IN_PROGRESS`; Linux success cannot establish the required native Windows release acceptance. This is an additional release prerequisite, separate from the demonstrated REVISE findings. No new performance qualification, Clone code-size benchmark or O01/O02 adoption measurement was performed; the correction does not adopt those experiments.

This is a finite review, not exhaustive Rust or shell semantics coverage. Untested forms include arbitrary fragment nesting, every pattern/statement and lifetime-binder form, all raw literal suffixes/delimiters and punctuation triples, every trait-object/cast/qualified-type combination, arbitrary downstream macro matchers, hygiene/cfg/suppression combinations, and arbitrary shell prefix/option/control combinations. Real rustdoc was checked for the named expressions, not visually audited in full.

For replay after cleanup: `python3 prepare.py head`, then `python3 head/replay.py`, `python3 head/run_matrix.py`, and `python3 strict_matrix.py head`; finish with `python3 prepare.py head --clean`. Substitute `a6b78a1` for the comparison. `extras.py` runs the exact C1/C2/direct controls and added cases; `tooling.py` reproduces the shell comparisons. These scripts preserve per-command finite timeouts and the heavy wrapper. No credentials or remote mutation were used. All review-owned commands have ended; no required local session remains running.

Required correction scope: C3 and T2, then their ordinary/forwarded neighbors and maintained selections on the resulting identified source. The original C1/C2/T1 repairs are independently verified, not grounds to accept the replacement shared semantic without these remaining corrections.
