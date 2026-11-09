# Direction: errors — the provider's error document as a fact, without per-SDK plumbing (A1 + A2)

Saved by the coordinator from the direction lane's final message (condensed; every number and
finding kept). Lane: Claude subagent (dispatched before the owner's Codex-only instruction).
Branch `direction/errors` (from `ba10189b`; merge of `spike/A-derive-error` + `spike/A-error-document`,
one conflict in Salesforce `examples/salesforce-sdk/src/error.rs` resolved minimally, plus the combining layer), worktree
`/home/canarit/projects/Concord_Next/work/wt/dir-errors`, slot `lane-a`; measure `/home/canarit/projects/Concord_Next/work/wt/dir-errors-measure.md`; logs
`/home/canarit/projects/Concord_Next/work/wt/dir-errors-*.log`.

**Status: works.** Mechanism added on top of A1 + A2: the derive gains the variant attribute
`#[from(concord::Error<D>)]`, which generates the split (the provider's `Failure<D>` builds that
variant's field through `From<Failure<D>>`; every other failure goes through `From<concord::Error>`),
so the per-SDK conversion is gone; `#[source]` on one field per variant (plain, `Box<T>`,
`Option<T>`); `Received::failure_as::<F, D>(limit)` for a response that arrived outside a declared
endpoint. A2's non-ASCII regression fixed (identifier rules through `syn`; `{détail}`, `{コード}`
compile).

## Shape (Anthropic)
```rust
// api!:
error ErrorDocument in Json;
// SDK error type, after (before: 4 variants + hand-written Display, source(), two From impls,
// and A1 alone added an 8-line From<concord::Error<ErrorDocument>>):
#[derive(Debug, concord::derive::Error)]
#[non_exhaustive]
pub enum Error {
    #[display("invalid message request: {0}")] InvalidRequest(#[source] InvalidRequest),
    #[display("{0}")] #[from(concord::Error<ErrorDocument>)] Api(Box<ApiError>),
    #[display("{0}")] Concord(#[source] concord::Error),
    #[display("{0}")] Incomplete(Box<Incomplete>),
}
impl From<Failure<ErrorDocument>> for ApiError { fn from(failure: Failure<ErrorDocument>) -> Self { Self { failure } } }
// send policy, after (before: receive, status branch, document read, wrap):
let prepared = endpoint.prepare()?.error_body_limit(ERROR_BODY_LIMIT);
let (value, meta) = prepared.receive().await?.decode_with_meta().await?;
```
Caller unchanged by this layer (A1's `document_error()` → `failure()` stays).

## What disappears (net hand-written lines, all classifier categories; vs base `ba10189b`)

| SDK | A1 alone | A2 alone | parts | **combined** | glue base → combined |
|---|---|---|---|---|---|
| distribution-registry | −41 | not adoptable (`source()`) | −41 | **−77** | 1,180 → 1,099 (−81) |
| salesforce | +18 | −55 | −37 | **−62** | 941 → 879 (−62) |
| anthropic-messages | −1 | not adoptable (`source()`) | −1 | **−28** | 438 → 407 (−31) |
| clickhouse | +11 | not migrated | +11 | **−21** | 969 → 970 (+1: `exception()` became a `From<Failure>`, counted glue) |
| **4 SDKs** | −13 | −55 | **−68** | **−188** | 3,863 → 3,690 (**−173**) |

Whole merged tree (incl. A1's S3, A2's B2/GitHub/Discord, controls): glue 7,602 → 7,391 (−211;
`glue:error` −198), DSL +6, M4 6.45 → 6.52, negations 0. Without `failure_as`, Salesforce would be
−43 (worse than A2 alone). ClickHouse's parts total is understated (A2 could have removed ≈25 lines
there; that lane did not migrate it). What went: every SDK's `From<concord::Error<D>>` split
(8–11 lines), plus `Display`/`source()`/`From` impls in Registry, Anthropic, ClickHouse; Registry's
manual `ApiError` `Debug` became derived (from A1), `ApiError::new` became `From<Failure>`,
`AuthError` gains `impl Error` (+6).

## Simple case
Nager, PokeAPI, Frankfurter byte-identical; DSL 8/7/11; tests pass.

## Costs
M3 (this layer): `#[from(concord::Error<D>)]`, `#[source]`, `Received::failure_as` (≈58 glue lines per
concept); carried: A1 (statement, `Failure`, `Document`, defaulted `D` on 7 generics, 13 methods) and
A2 (derive, `display`, `from`). Breaking (this layer): Registry `RegistryError::Auth` source chain
gains a level; additive impls (Registry, Anthropic, ClickHouse, Salesforce, `failure_as`);
`#[from(T)]` replaces that variant's `From<field>`; `Display`/`Debug` text of migrated enums
identical. A1's and A2's breaking lists carried unchanged.
M11 (UI cases): provider variant field without `From<Failure<D>>` → E0277 at the field type;
`#[from(concord::Error<Doc>)]` without any `From<concord::Error>` → E0277 at the attribute type (help
exposes the `()` default); `#[from(String)]` → E0599 "no method named `into_provider`" (weakest,
noisy suggestion); `#[source]` on `u32` → E0277 at the type; misplaced/duplicated attributes → all in
one build at each attribute. M7: derive doc 70 → 102 words, `failure_as` 24 (not caller path); caller
path +3 words per adopter `send()` (from A1). M8 not measured.
Not done: `D` still propagates through SDK signatures (≈0 net lines, visible: Salesforce's query trait
has 8); A1's diagnostic limits remain; `#[source]` has no transparent mode.

## What it rules out
A type-erased document in a non-generic `concord::Error` (commits to `Error<D>`); status-routed
provider variants (Datadog: 401/403/400/429/else) keep a hand-written `From` (routing in the attribute
would be behavior, C2); thiserror-style field `#[from]`/`transparent`; the split without
`From<concord::Error>`; one variant serving two documents (a scope override needs one variant per
document).

## Tests
core/macros/concord `--all-features`: 575 passed, 0 failed, 4 ignored (43 suites); SDKs, dependents,
controls: 508 passed, 0 failed (82 suites), canaries 9 binaries / 11 tests; core dependents check
clean; clippy, rustfmt, rustdoc `-D warnings` clean. New: `non_ascii_field_names_format`,
`marked_fields_are_the_source`, `the_derive_splits_the_provider_answer_from_other_failures` (a 409
document reaches `Api`, never printed; a 2xx decode error stays `Concord`),
`a_response_outside_the_declaration_reads_a_named_document`, 7 UI cases. No existing expected value
changed.

## Lane's assessment
The combination removes more than the parts (−188 net, −173 glue on 4 SDKs vs −68), and is the first
state where Registry and Anthropic can adopt the derive at all. One `error D in F;` line plus one
`#[from(concord::Error<D>)]` attribute replace each SDK's document reading, state mapping and split.
Costs: A1's `D` in signatures and its diagnostic limits, 3 small concepts; status-routed SDKs still
hand-write the split.

## Review

### Round 1

Source: [direction review](/home/canarit/projects/Concord_Next/work/reviews/direction-errors-review-1.md).

VERDICT: ACCEPT_WITH_FIXES
CHARTER: C1=y C2=n C3=y C4=n C5=n C6=y C7=y C8=y C9=n

FINDINGS:

- [blocking] Optional sources introduce another hygiene collision — [derive_error.rs:320](/home/canarit/projects/Concord_Next/work/wt/dir-errors/crates/concord-macros/src/derive_error.rs:320) — Generated `.map(|cause| cause as …)` uses the authored field’s span. An ambient `const cause: u8 = 0` makes `cause` a constant pattern, rejecting an otherwise valid `#[source] Option<std::io::Error>`. — Give both closure identifiers mixed-site hygiene and add a compile-pass regression with that ambient constant.

- [non-blocking] Split test’s secret prints through Debug — [error_document.rs:330](/home/canarit/projects/Concord_Next/work/wt/dir-errors/crates/concord/tests/error_document.rs:330) — `Refusal` derives Debug over `title`, which receives `"SECRET-TITLE"`. Consequently, formatting the resulting `SdkError` with Debug exposes it. The split test checks Display but omits Debug, so the record’s “never printed” claim is false. — Withhold the title in the payload’s Debug, assert ordinary and alternate Debug secrecy, and qualify the record’s guarantee.

- [non-blocking] A provider fact remains duplicated — [salesforce-sdk/src/error.rs:159](/home/canarit/projects/Concord_Next/work/wt/dir-errors/examples/salesforce-sdk/src/error.rs:159) — `failure_as::<IgnoreContentType<Json>, ApiErrorBody>` repeats the codec/document pairing declared in `api.rs:45`. Shared reading machinery disappears, but C2’s single-source requirement remains unmet. — Provide typed access to the declared error handling for external responses, or retain this explicitly as a direction limit.

- [non-blocking] C5 diagnostic limits remain — [error_document_policy_mismatch.stderr:1](/home/canarit/projects/Concord_Next/work/wt/dir-errors/crates/concord/tests/ui_json/error_document_policy_mismatch.stderr:1) — Policy errors still span the whole declaration; suggested code leaves `D` undeclared; codec mismatch exposes `__private::ErrorFormat`; `#[from(String)]` produces a generated-method error and irrelevant `into_proxy` suggestion. — Improve authored spans, supply valid generic examples, and diagnose unsupported provider-conversion syntax directly.

- [non-blocking] Generated documentation remains inaccurate and over budget — [emit.rs:1526](/home/canarit/projects/Concord_Next/work/wt/dir-errors/crates/concord-macros/src/emit.rs:1526) — Policy summaries and generated error sections still name `concord::Error` where execution returns `Error<D>`. The inherited three-word increase exceeds M7’s default budget, without document-aware emitter coverage. — Render the resolved error type consistently, shorten the wording, and add focused emitter assertions.

CHECKED:

- Reviewed `ba10189b..12605058`, core/macro implementations, migrations, tests, snapshots, records and prior reviews. Worktree remained clean.
- **M1/M5 reproduced exactly:** glue **3,863 → 3,690 (−173)**; declaration negations **0 → 0**.

  | SDK | Glue change |
  |---|---:|
  | Distribution Registry | −81 |
  | Salesforce | −62 |
  | Anthropic Messages | −31 |
  | ClickHouse | +1 |

- **The numerical combination claim holds for the implemented trees.** Net classified lines, including DSL, change by **−188**, versus **−68** for the parts. The corresponding glue comparison is **−173 versus −81**. This includes new source support and `failure_as`, plus Salesforce’s independently replaceable standard Debug implementation; it does not isolate the split attribute’s contribution.
- **C1/C2:** `error D in F;` describes provider facts. Attributes add structural conversions and formatting, without retry, recovery or status-routing policy; substantive decisions remain Rust. C2 nevertheless fails on the duplicated Salesforce pairing above.
- **C3:** Nager, PokeAPI and Frankfurter are unchanged; declaration lines remain **8/7/11**. Non-adopters need no new concepts.
- **C4/C5:** inheritance is explicit and deterministic; the hygiene and diagnostic findings prevent compliance.
- **C6/C7/C8:** the mechanisms are locally understandable; native client access, `prepare`, parts, raw responses and incremental exits remain available. No retry, global state or hidden execution actor was added.
- **C9:** the compile-fail fixtures are connected to `trybuild::compile_fail` and exact stderr snapshots. Unexpected compilation—or loss of an expected diagnostic—would fail those tests. The split test explicitly requires `Api` for the 409 and `Concord` for the 2xx decode failure: wrong routing fails it. The Debug hole, new hygiene case and previously recorded import/doc coverage gaps remain.
- No fundamental conflict between A1 and A2: A1 reads the declared document; A2 constructs the SDK error. Scope overrides still require separate variants or handwritten conversion when one variant must serve multiple document types.
- The compatibility inventory is adequate **when read together with the carried A1/A2 reports**: generic signatures and policy bounds, consumed-response behavior/Openable restriction, Registry fields and variants, Anthropic accessor/output changes, and Registry’s additional Auth source level.
- **Secret handling:** `Error<D>`, `Failure<D>` and `Document<D>` withhold documents/bytes; `failure_as` uses that same safe wrapper. The derive delegates formatting to SDK payloads and cannot preserve that protection after a conversion extracts unredacted fields. Existing SDK formatting also retains previously identified provider-controlled output paths; passing canaries does not establish blanket body secrecy.
- Saved logs corroborate **575 core tests passed, 4 ignored**, and **508 SDK/dependent/control tests passed**. Changed inherited expectations match the A1 record.

NOT CHECKED:

- No builds, tests, mutation tests, clippy, rustdoc or performance measurements executed, as instructed. The hygiene finding follows from the generated tokens; its reproducer was not compiled.
- M8 remains unmeasured. No exhaustive migration assessment across all 55 SDKs.
- No edits or network operations performed.

### Fixes after round 1

From the [direction source's fix section](/home/canarit/projects/Concord_Next/work/design/directions/errors.md): Codex fix commit `4c43a3f2` gives generated optional-source bindings mixed-site hygiene inside an anonymous const. `ambient_constants_never_shadow_the_generated_bindings` guards the original collision. The split fixture's document type now withholds its title and checks ordinary/alternate Display and Debug; restoring derived Debug fails the new assertion. Routing assertions remain. The derive delegates to SDK payload formatting, so extracted provider fields need their own redaction.

Recorded limits: Salesforce's `failure_as::<IgnoreContentType<Json>, ApiErrorBody>` still repeats the declared codec/document pair; no cheap API reads that declaration for a raw response. A1's diagnostics and generated-doc wording/budget remain. The review below supersedes the earlier hygiene-completeness claim.

### Round 2 (last)

Source: [REJECT](/home/canarit/projects/Concord_Next/work/reviews/direction-errors-review-2.md), reviewed `12605058..4c43a3f2`. Codex `gpt-6.1-sol`; independent session, same provider.

CHARTER: C1=y C2=n C3=y C4=n C5=n C6=y C7=y C8=y C9=n

**REJECT. Two blocking limits remain for P4.** The round-1 hygiene fix introduced both regressions:

- [blocking] Shielding imports capture authored formatter hooks. Valid `#[display(with = error)]` resolves to the imported identity function instead of the author's hook. Scope shielding to generated names and add a compile-pass case for a hook named `error`.
- [blocking] Aliasing every field panics on an unused raw identifier such as `r#type`, through `Ident::new("__field_r#type", …)`. Alias only referenced fields, generate valid names independently of raw spelling, and add compile-pass coverage.
- [non-blocking] Salesforce still restates the declared codec/document pair. Typed access to declared handling is needed before claiming C2 compliance.
- [non-blocking] Declaration-wide spans, undeclared `D` in suggestions, private names and unsupported-conversion errors remain. Improve authored spans and actionable diagnostics with fixtures.
- [non-blocking] Generated docs still name `concord::Error` where execution returns `Error<D>` and retain the wording/budget limit. Resolve the type, shorten wording and add emitter assertions.

The original ambient-constant collision and split-fixture secrecy are fixed. Saved fix logs substantiate **576 passed, 4 ignored, 139 trybuild cases**, **493 SDK/dependent/control tests passed**, and clean clippy. These are the later fix selection's results; earlier totals above describe the original selection. Glue remains **−173 on 4 SDKs**; that result does not resolve the two derive correctness failures. Reviewers did not execute builds, tests, mutations, rustdoc or performance measurements; the new failures follow from static expansion analysis.
