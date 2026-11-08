# Review h03-candidate-review-1-sol (CANDIDATE review, H03, on d793fecc (A) and 7f14da4d (B))

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed from rollout turn_context),
2026-11-08T16:03:24Z–16:10:48Z, thread 01a1bbe6-7451-7e50-b34c-4d6123d98347.
Read both diffs in full (`git show` per path batch; inventories cross-checked with `git diff-tree`).
**Verdict: REJECT** (4 blocking, 2 non-blocking). Confirmed: every emission site follows the stated
exact/converting rule (apart from raw type identifiers); request rendering unchanged for retained
inputs; the four alias-site edits are required by A's signatures and keep their assertions; UI
snapshots keep the expected types; test totals recomputed (A 3,957, B 3,958 passed, 32 ignored);
public signatures spot-checked (Frankfurter, Kraken, Nager, Sentry); in the 55 SDKs' 63 `api!`
invocations no renamed String/HeaderValue alias, `Cow`, direct `Option<String>` or associated
String/HeaderValue input exists.

| # | Finding | Severity | Triage |
|---|---|---|---|
| 1 | B's new test `named_generics.rs` uses `concord::Text` (needs `extras`): E0425 in gate steps `test-no-default` and `test-reqwest-defaults-only` | blocking | valid (gate on 7f14da4d fails); fixable (`required-features` or test-local format) |
| 2 | A and B fire the card's kill condition "any newly rejected caller spelling that compiles today": borrowed `&str`/`&String`/`&HeaderValue` into aliases of String/HeaderValue whose final segment differs (observed: 4 repository sites needed migration), generic code bounded by `IntoValue<T>` for a non-String `T` forwarding into an exact input (e.g. `V: IntoValue<u16>` into Nager's year), type macros, function values instantiated through such adapters | blocking | valid — the kill condition is frozen in the card; no exact-type candidate can avoid class 2 (sealed trait, generic code cannot prove `V = T`). Consequence: H03 REJECT (LOSS by kill condition), to the decision review |
| 3 | B breaks external hand-written implementations of generated group traits (`impl IntoValue<String>` argument vs the generated named generic: E0643); undocumented | blocking | valid (implementing side); fixable by keeping argument-position `impl Trait` on group traits and their impls |
| 4 | Raw type identifiers `r#String`/`r#HeaderValue` take the exact branch (string comparison without `unraw`) | blocking | valid; fixable (normalize with `unraw()`, test both raw spellings) |
| 5 | `self_` yields `Value` not the documented `SelfValue` (`Self` fails `parse_str::<Ident>` before the suffix rule); name not asserted | non-blocking | valid; fix with the suffix rule + assertion |
| 6 | `value.rs` trait docs restrict `T` to String/HeaderValue though `IntoValue<T> for T` holds for every `T` | non-blocking | valid; reword |

Other observations recorded: A's CALLER_GUIDE `.parse()?` labelled E0283 (actually E0277; B fixes);
B's public-diff report says zero doc changes despite `IntoValue` doc edits; B's n=10 edit-API rerun
has a 1.08 mean ratio (medians ≈1.015) — the cost bound is to be judged on the independent
re-measurement (H03 measurement lane §3); temporary witnesses compared prepared requests, not sent
ones; the explicit Notion body-default witness fails (`QueryBody` has no `Default`).
Concept budget: the lexical String/HeaderValue exception is itself a spelling-dependent rule; B adds
public generic naming/order rules and turbofish.
