# Final executable re-review — rv-final2-sol

**Verdict: REVISE** for `a6b78a18c81b7ea1adf0d76da6e3c714f8a0e95d`.

The exact previous R1/R2 reproducers are repaired and R3's changelog heading is restored. Every substantive counterexample from compiler reviews 1–6 passes its applicable repaired or deliberately negative control. Widening nevertheless found **two compiler correctness gaps and an incomplete M03 noexec repair**. Passing maintained tests and the central gate do not cover these inputs.

This is an independent re-review of `9c32294..a6b78a1`, including its compiler, tooling, tests, changelog and release-record changes, against the supplied handoff, acceptance matrix and re-review rule. I authored none of the reviewed patches and spawned no agents. Actual model and effort were not observable; the lane name is not runtime evidence. Execution was on native Linux `x86_64-unknown-linux-gnu`, Rust/Cargo **1.99.1**, on 2026-11-04 UTC. The reviewed snapshot was clean before and after execution; only temporary review examples were installed and removed.

## Blocking correctness

### C1 — P2: a forwarded trait-object type loses required grouping inside an initializer

**Location:** `crates/concord-macros/src/syntax.rs::expression_fragment` (1284–1288), `regroup_in` (1336–1347), reached through `parse_initializer` (1260–1263). Expression mode now leaves a multi-token group unchanged when its contents do not parse as an expression. A forwarded `dyn Send + Sync` in `size_of::<&$t>()` needs grouping as a **type**, even though the enclosing initializer is an expression. Leaving its invisible delimiter intact does not preserve the ordinary Rust type boundary through this expansion.

Minimal declaration, retained with its ordinary Rust function and runtime equality assertion in [repro_dyn.rs](probes/src/repro_dyn.rs):

```rust
macro_rules! pair {
    ($t:ty) => {
        fn ordinary() -> usize { core::mem::size_of::<&$t>() }
        concord::api! {
            pub client Api {
                base "http://localhost";
                GET x(query n: usize = core::mem::size_of::<&$t>());
            }
        }
    }
}
pair!(dyn Send + Sync);
```

**Reproduction:** from this report's directory, `bash reproduce.sh repro_dyn`. The driver temporarily copies the retained fixture to the snapshot, runs `timeout 300 tools/heavy.sh cargo +1.99.1 run --offline --locked -p concord --example repro_dyn`, and cleans up. The fixture has `#![deny(warnings)]` without lint exceptions.

**Expected:** exit 0 and `n=16` on this host, matching the ordinary macro. **Actual:** exit 101, `ambiguous '+' in a type`, at the forwarded `dyn Send + Sync`. See [head log](logs/head-repro_dyn.log). The separately compiled [ordinary control](probes/src/repro_dyn_plain.rs) passes with `rustc +1.99.1 --edition=2024 -D warnings` and prints `16`; the [direct declaration](probes/src/repro_dyn_direct.rs) passes with `n=16`.

**Introduced or pre-existing:** a regression **relative to the previous frozen `9c32294`**, where this exact fixture compiles and prints `ordinary=16 concord=n=16` ([comparison](logs/9c32294-repro_dyn.log)). The 4.1 `07dcdd0` baseline also failed this form, so it restores an older boundary rather than creating the first occurrence of that boundary. The correction must still preserve behavior that the immediately preceding candidate accepted. All comparison archives retain their committed locks, with registry versions/checksums identical to the candidate.

**Impact:** valid macro-generated public declarations stop compiling, even without denied warnings. The same failure was executed in a braced const argument, an array length, and a qualified associated-value expression: `matrix_ty_dyn_{ref,const,array,qualified}`. The direct forwarded body-type trait-object regression remains passing; this is its nested initializer neighbor.

**Strongest counterargument:** this was unsupported on 4.1, and forwarding `&(dyn Send + Sync)` as a complete parenthesized type or writing the type directly avoids it. That narrows compatibility impact but does not preserve the preceding candidate or the required ordinary-Rust interaction controls.

**Requested regression:** retain the ordinary/Concord/direct comparison under denied warnings, including `&$t`, nested generic arguments, qualified paths, braced const arguments and arrays. Preserve type boundaries that require parentheses while retaining the lifetime, shift, pointer-cast, reference-expression and struct-path fixes. Classifying only the fragment's standalone spelling does not establish its role at the use site.

### C2 — P2: negative literal range bounds acquire syntactically forbidden parentheses

**Location:** the same `expression_fragment` and `regroup_in` branches, particularly 1337–1347 and brace recursion at 1355. The traversal treats the match-arm pattern inside an initializer as expression context. A negative `$n:literal` parses as an expression and receives parentheses, but Rust range-pattern bounds cannot have them.

Retained minimal [repro_range.rs](probes/src/repro_range.rs):

```rust
macro_rules! pair {
    ($n:literal) => {
        fn ordinary() -> usize {
            match 0isize { $n..=1 => 7usize, _ => 8usize }
        }
        concord::api! {
            pub client Api {
                base "http://localhost";
                GET x(query n: usize =
                    match 0isize { $n..=1 => 7usize, _ => 8usize });
            }
        }
    }
}
pair!(-1isize);
```

**Reproduction:** `bash reproduce.sh repro_range`. **Expected:** exit 0, `n=7`, matching ordinary Rust. **Actual:** exit 101, `range pattern bounds cannot have parentheses` ([head log](logs/head-repro_range.log)). The [ordinary-only control](probes/src/repro_range_plain.rs) compiles under `-D warnings` and prints `7`. The no-fragment direct declaration compiles and prepares `n=7`. This is a syntax failure, not a warning that call-site spans can suppress.

**Introduced or pre-existing:** introduced by the integrated compiler corrections relative to **4.1 `07dcdd0`**, whose exact fixture passes ([baseline](logs/07dcdd0-repro_range.log)); already present on **`9c32294`** ([previous candidate](logs/9c32294-repro_range.log)). It is a remaining defect in the widened grouping repair, not newly introduced by the last lifetime change.

**Impact:** a previously valid initializer emitted by a wrapper macro cannot compile. Both lower and upper negative bounds were executed. The same issue also occurs inside a braced const argument and an array-length expression: `matrix_literal_pattern_{range,end}`, `matrix_literal_range_{const,array}`. Ordinary controls all pass with warnings denied.

**Strongest counterargument:** negative literal fragments in match-range bounds are less common than simple arithmetic defaults, and a directly written negative bound works. They are legal Rust, worked on the release baseline and occur inside the initializer grammar being rewritten.

**Requested regression:** keep lower/upper negative literal bounds against ordinary macros, including match patterns nested in const arguments and array lengths, under denied warnings. Preserve pattern roles separately from expression grouping. Retain arithmetic/borrow precedence and macro-input opacity controls when repairing this.

### T1 — P2: literal builtin wrappers and assignment prefixes bypass the noexec guard

**Location:** `scripts/check-ci.py::unsupported_set` (608–615), the equivalent first-word check in `disables_errexit`, and `Guard.job` (898–906). Only a command whose first word is exactly `set` reaches the option check. Bash's literal builtin/command prefixes and a leading assignment still execute the same shell-option change, but are not rejected as unsupported.

Retained workflows preserve the maintained Linux/Windows matrix and nightly lane. Their gate step contains one of:

```sh
builtin set -n
scripts/verify.sh --package --bench
```

```sh
command set -o noexec
scripts/verify.sh --package --bench
```

```sh
REVIEW_OPTION=1 set -n
scripts/verify.sh --package --bench
```

**Reproduction:** from the reviewed root:

```sh
timeout 20 python3 -I scripts/check-ci.py --workflow-file \
  /home/canarit/projects/concord-v42/lanes/rv-final2-sol/builtin-noexec.yml
```

Also execute [command-noexec.yml](command-noexec.yml) and [assignment-noexec.yml](assignment-noexec.yml).

**Expected:** exit 1 with no effective full gate on either platform. **Actual:** all three exit 0 and print `effective full gate on: Linux, Windows`; see [builtin](logs/workflow-builtin-noexec.log), [command](logs/workflow-command-noexec.log), [assignment](logs/workflow-assignment-noexec.log).

I independently executed each prefix with `bash --noprofile --norc -eo pipefail -c`, replacing the gate with `printf GATE_EXECUTED`. All three exit 0 with **empty stdout**. The `set -euo pipefail` control prints the witness. These shell receipts are `logs/shell-{builtin-noexec,command-noexec,assignment-noexec,safe}.log`, with argv and hashes in [final-receipts.json](final-receipts.json).

**Introduced or pre-existing:** all three are also accepted by the exact `9c32294` checker, executed from [previous-check-ci.py](previous-check-ci.py). This is an **incomplete repair of the previous R2/M03 defect**, not a new regression caused by `unsupported_set`. Direct `set -n`, `set -o noexec`, and combined `set -eun` are correctly rejected now.

**Impact:** the checker continues to certify a statically disabled gate. This does not invalidate the actual central execution or the maintained unmutated workflow.

**Strongest counterargument:** the checker deliberately supports a bounded shell grammar and cannot infer arbitrary command behavior. These are literal invocations of the same builtin in the gate's own step. Normalizing these explicit prefixes, or conservatively rejecting them there, requires no general shell evaluator and matches M03's unsupported-construct policy.

**Requested regression:** add executable negatives for all three forms, with the intended rejection witness. Normalize supported builtin/command and assignment prefixes before option inspection, or reject them conservatively. Retain direct noexec/combined-option negatives and safe-option/quoting/multiline positives.

## Previous final-review findings

| Previous item | Executed result on `a6b78a1` |
|---|---|
| R1 forwarded qualified path heading a struct literal | Exact retained fixture passes, printing `FORWARDED_STRUCT_LITERAL_PASSED`; [log](logs/replay-final_forward_path.log). Maintained `forwarded_types` also executes endpoint `h=7` and owner-default `h=8`. |
| R2 `set -n` and `set -o noexec` | Both exact prior YAML fixtures exit 1 with `turns on noexec` and missing effective Linux/Windows gate witnesses. Corresponding Bash executions still skip the following witness, as expected. Combined short `-eun` is also rejected. T1 records the remaining related hole. |
| R3 missing 4.1 changelog heading | Fixed: `CHANGELOG.md:71` restores `## 4.1.0`; the retained `=4.1.0` paragraph is under that heading. The migration reference is now section-specific. |

No additional optional polish finding is required for this verdict.

## Compiler counterexamples from rounds 1–6

[replay.py](replay.py) executes **75 Cargo steps** against this exact source: hint/error controls, runtime/type controls, current automatically applied repairs and two real rustdoc builds. Every expected exit and requested output witness matched; [replay-summary.log](replay-summary.log) ends `UNEXPECTED: []`. Fixtures were retained from `rv-compiler6/probes/src`, including its copies of all earlier substantive counterexamples. Logs are `logs/replay-*.log`; original reports were read for expected semantics rather than treating their old wrong applied spellings as current suggestions.

| Report/counterexample | Re-executed result |
|---|---|
| Round 1 separate `. .` punctuation | Hint preserves `. .`; original selects body 1; retained old wrong `..` application selects body 2. |
| Round 1 invisible arithmetic grouping and generated body mismatch | Current hint uses conservative `T`; original query/body controls select 6. Old flattened application still selects 5, as a negative comparison. |
| Round 2 nested `> >` | Separate tokens retained; original body 1, old joined application body 2. Maintained current applied advice passes. |
| Round 2 trailing-dot float `1. . 2` | Tokens retained; original body 1, old range application body 2. Maintained current applied advice passes. |
| Round 2 opaque `ty`, `expr`, `literal`, `path` | All four original/hint/old-application triples pass their intended contracts: current `T`, opaque body 2, formerly flattened body 1. Forwarded format uses `F`; fragment-preserving format control passes. Ident/plain spelling controls pass. |
| Round 2 initializer prose/runtime mismatch | Runtime query is `n=6`; real rustdoc HTML text contains `Initially (1 + 2) * 2`. |
| Round 2 direct forwarded `dyn A + B` reference boundary | `dyn_original`, applied and ordinary Rust controls pass. C1 is a separate nested initializer position. |
| Round 3 reference expression / method dispatch / array length | `rust=borrowed concord=n=borrowed`; reference length and array const type controls pass. Real rustdoc says `Initially (&VALUE).meaning()`. |
| Round 4 forwarded turbofish type under denied warnings | `round4_type_initializer` compiles and runs; multi-token reference/generic/array direct-type controls pass. |
| Round 5 shift and pointer cast | `rust=4 concord=n=4`; `round5_cast_pointer` passes with warnings denied. |
| Round 6 lifetime | Original ordinary/Concord comparison and ordinary-only fixture compile and run with warnings denied. |

Also replayed the 21 separate-punctuation/operator hints and 17 lifetime/literal hints, extracted **current** suggestions from this candidate's diagnostics, applied all 38 to the runtime fixtures, and asserted their payload/body results. Joined-operator, ampersand, slash, integer-dot, nested-forward, macro-opacity and deliberate trait-error span controls passed. Rustdoc checks parse HTML text, avoiding raw-markup false negatives.

## Widened ordinary-Rust comparisons

[generate_matrix.py](generate_matrix.py), [run_matrix.py](run_matrix.py), [matrix-cases.json](matrix-cases.json) and [matrix-results.json](matrix-results.json) retain **74 individual comparisons**. Every ordinary-only control compiled with `rustc +1.99.1 --edition=2024 -D warnings` and executed successfully. Every paired Concord fixture contains the same ordinary macro function, denies all warnings without exceptions, and compares its prepared query to that function when compilation succeeds.

**Results: 63 Concord comparisons pass; 11 fail compilation.** Four failures are C1 variants, four are C2 variants, and three are the pre-existing statement boundary below. No fixture failure is counted as product evidence if the ordinary control fails.

| Kind | Cases | Executed positions and controls |
|---|---:|---|
| `expr` | 8 | Initializer arithmetic/borrow/dispatch, `<<` and `<<=`, const arguments, array lengths, nested const expressions, downstream macro inputs. |
| `ty` | 16 | Turbofish reference/generic/array types, direct `as`, `*const`/`*mut` casts and array/generic pointees, qualified paths, const/array nesting, trait objects, macro inputs. C1 accounts for 4 failures. |
| `path` | 6 | Qualified struct literal, type/pointer positions, const arguments, array lengths, macro inputs. |
| `ident` | 5 | Struct literal, type position, const arguments, array lengths, macro inputs. |
| `lifetime` | 6 | Reference/nested generic types in initializers, braced const arguments, array lengths, higher-ranked function pointer binder, macro inputs. |
| `literal` | 8 | Negative literal method call, lower/upper match-range bounds, const arguments, array lengths, macro inputs. C2 accounts for 4 failures. |
| `pat` | 5 | Match/let patterns, patterns inside const arguments and array lengths, macro inputs. |
| `block` | 4 | Initializer, braced const, array length, macro inputs. Ordinary Rust requires the supplied operand parentheses in the multiplication cases; both arms use them. |
| `stmt` | 7 | Let statement and const-item statement in initializer/const/array blocks; macro input. Const-item and macro controls pass; 3 let-statement cases hit the pre-existing parser boundary. |
| `tt` | 4 | Grouped arithmetic in initializer, const argument, array length, macro inputs. |
| `vis` | 5 | `pub(crate)` and empty visibility, local items inside initializer/const/array blocks, client visibility and downstream macro inputs. |

All eleven fragment kinds are compared in both expression-macro and type-macro inputs; opaque-versus-literal matching agrees with the ordinary macro. Positions are chosen where each fragment kind is legal Rust, not an assertion that every kind is valid in every type or expression slot.

Declarations with **no forwarded fragments** are unaffected in the executed controls: the previous ordinary URL/body fixture passes, and all three new direct declarations pass with denied warnings on `07dcdd0`, `9c32294` and this head. These direct comparisons cover the trait-object reference, negative range bound and local let statement.

### Additional pre-existing parser boundary: forwarded let statements

`parse_initializer`'s initial `syn::Expr` parse rejects `{ $s 6usize }` when `$s:stmt` is `let _ = 3usize`, with `unexpected token, expected ';'`. Ordinary Rust treats the captured statement as a completed statement and returns 6. [repro_stmt.rs](probes/src/repro_stmt.rs), its [ordinary control](probes/src/repro_stmt_plain.rs), and [head log](logs/head-repro_stmt.log) retain the comparison. `bash reproduce.sh repro_stmt` reproduces it.

The **same parse failure** occurs at both `07dcdd0` and `9c32294`; this is not attributed to the new regrouping delta. The direct declaration prepares `n=6`, and forwarded const-item statement controls pass in all three tested positions at head. Impact is a narrower supported initializer grammar than ordinary Rust forwarding. The strongest counterargument is that this longstanding statement-fragment boundary predates the correction and fails before regrouping; it is not evidence that the new expression classifier first introduced the problem. Track it explicitly and retain the ordinary-versus-Concord let/item regressions if broad statement-fragment support is pursued. It is separate from the three blocking dispositions above, and is not described as repaired.

## Independently executed maintained selections

All Cargo commands used `+1.99.1 --offline --locked`, review-owned target directories, finite timeouts and `/home/canarit/projects/concord-v42/tools/heavy.sh`. No unfiltered test command ran in a replay consumer. Localhost fixtures were used; no credentials, remote runner or publication action was used.

| Scope | Command/selection | Result |
|---|---|---|
| F01/F02 cross-crate and hygiene | `cargo test -p concord-clone-hygiene-check -p concord-clone-compat -p concord-hygiene-check` | Passed: 4 clone-hygiene tests including 4 compile-fail cases, localhost output-type decode, 4 clone-compat runtime tests, native-header hygiene, 12 clone doctests. |
| F02–F04 and neighbors | `cargo test -p concord --test output_types --test applied_advice --test forwarded_types --test lifecycle --test d16_followup --test accept --test bodies --test composition --test values` | 73 harness/runtime tests passed; D16's 3 trybuild cases passed. Current applied advice and owner/endpoint path/lifetime regressions executed. |
| UI | `cargo test -p concord --test ui` | 2 harness tests; all 83 UI cases passed without rewritten snapshots. |
| Compiler units | `cargo test -p concord-macros` | 17 passed. |
| M01 | `cargo test -p concord-bench` | 10 passed, including actual registered-pair contracts and comparator negatives for Accept, repeats/order, URL/method, body, native configuration and sensitivity. |
| M02 | `cargo test -p concord-example-renamed` alone | 3 passed. |
| M02 isolated no-extras | Copied maintained consumer; `cargo test --lib --tests`, normal and facade feature graphs | 3 passed; graph excludes `concord-extras`, JSON and form dependencies/features. Every invocation locked/offline. |
| T01 and native positives | `cargo test -p concord-example-native` | 30 passed, including the 3 metadata-stage tests and body/error controls. |
| M03/M04/tool tests | `python3 -I -B -m unittest discover -s scripts/tests -p 'test_*.py' -v` | 74 passed, including safe/noexec option cases and retired mutation-helper no-caller/refusal/documentation checks. |
| Output directory controls | `bash scripts/tests/output-dirs.sh` | 14 controls passed. |

Exact argv, source cwd, target, expected/actual exit, timestamps and log hashes are in [final-receipts.json](final-receipts.json); [run_review.py](run_review.py) is the maintained-selection driver. Rust/Cargo flags are omitted from the table only for readability. No performance timing or mutation-score claim is made.

Early exploratory matrix versions had invalid ordinary controls (block multiplication, redundant statement semicolons, empty-visibility matcher syntax, and an unused shift result). They were corrected before the final strict run; their initial outcomes are excluded. The final 74 ordinary controls all pass. An initial no-extras lock check was refused because its copied manifest omitted the original local patches; restoring those exact patches resolved it without changing the lock or registry inputs. The append-only exploratory `receipts.jsonl` and early summaries retain those attempts; **final-receipts.json, matrix-results.json and matrix-summary-strict.log are authoritative for the retained final fixtures/logs**.

## Source closure, central gate and acceptance limits

[source-receipt.json](source-receipt.json) binds HEAD, compiler/host, clean final worktree, diff, root lock and central log. [tracked-source.sha256.json](tracked-source.sha256.json) hashes all 1,840 tracked files; [probes.sha256.json](probes.sha256.json) hashes retained fixture sources. All four production packages remain 4.2.0 with coordinated exact internal requirements. The `9c32294` comparison has the identical workspace lock; `07dcdd0` has the same 216 registry package versions/checksums; the isolated consumer's 169 registry packages are a subset of the candidate's graph. Historical and candidate compilations used separate target roots. Runtime/build logs identify the actual source paths.

The completed `scripts/verify.sh --package --bench` was **inspected from the central log**, not executed independently as a full gate. The supplied source-bound log identifies `a6b78a1`, initial dirty count 0, rustc 1.99.1, **37 PASS steps and exit 0**, including workspace tests (676 passed), feature lanes, format/clippy, records/generated checks, isolated examples, docs, bench compilation, fresh package-check and packaged consumers. A byte copy is [central-gate-inspected.log](logs/central-gate-inspected.log), SHA-256 `837618615ba708065eaa26904a772b1b44a7af58de2f23d7d1004ea6519ea7e2`. Fresh package extraction, the other isolated consumer variants and broad gate coverage are relied on **only as inspected central evidence** in this re-review.

I did not execute native Windows or obtain a source-bound native Windows receipt. The snapshot's status explicitly remains `IN_PROGRESS` pending the final records/platform closure. Linux successes do not establish native Windows acceptance; that remains a separate release prerequisite. No full-release ACCEPT is implied by the central Linux gate.

The matrix is finite. Untested forms include arbitrary nesting of all fragments, every pattern/statement variant, arbitrary lifetime binders, all raw literal delimiters/suffixes and punctuation triples, every cast/qualified/trait-object type, all macro matchers, and all hygiene/cfg/suppression combinations. No independent clone-bound size benchmark, visual rustdoc QA, security audit, fresh performance qualification or new package extraction was performed here. The delta does not adopt O01/O02; their recorded deferred decisions were not remeasured.

Required next correction scope: C1, C2 and T1, plus their neighboring interactions and maintained selections on the resulting source. The old counterexamples passing does not certify the replacement shared semantic. The final reviewed worktree is clean, all reviewer commands have ended, and no required local review session is left running.
