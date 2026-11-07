Four issues remain: one retained unsafe SDK classification, two macro edge cases, and one incomplete documentation fix. Reviewed the requested commits at `4870f864` by reading code, tests, docs, and cached dependency sources. No builds, tests, changes, or external services.

1. **P1 — The SDK-specific `is_connect()` exceptions still permit false “not sent” conclusions. CONFIRMED by reading.**  
   [Discord limiter.rs:95](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/integration/examples/discord-rest-sdk/src/limiter.rs:95), [Zendesk policy.rs:113](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/integration/examples/zendesk-support-sdk/src/policy.rs:113)

   A create can take effect, return `303`, and encounter a refused connection at the redirect target. Both SDKs then return their ordinary Concord error instead of their uncertain-write variant.

   Discord’s README construction example leaves redirects enabled, yet its error table calls a refused connection “not sent.” Zendesk’s provided `http_client()` disables redirects and avoids this scenario, but a supplied client with redirects enabled exposes it; its README explicitly requires disabling retries without equally stating the redirect restriction.

   This behavior **predates the migrations**: Discord retains it, and reconciliation restores Zendesk’s baseline behavior. It is not a new core `delivery()` defect.

   **Smallest fix:** classify every transport failure returned after preparing a write as uncertain, and update the corresponding claims. Retaining the narrower rule requires a reliable transport restriction.

2. **P2 — `with_output()` still replaces underscores that are parameter names. CONFIRMED by reading.**  
   [emit.rs:1781](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/integration/crates/concord-macros/src/emit.rs:1781)

   The valid template `send P -> fn(_: u8) -> _;`, with endpoint output `Vec<u8>` and policy output `fn(u8) -> Vec<u8>`, rewrites the parameter-name underscore too. It produces the equivalent of `fn(Vec<u8>: u8) -> Vec<u8>`, which is invalid Rust.

   Thus original finding 2 is only partially fixed: lifetimes and braced wildcard patterns are protected, but substitution still does not identify type placeholders structurally.

   **Smallest fix:** replace `syn::Type::Infer` nodes while preserving `BareFnArg::name`, lifetimes, expressions, and unchanged forwarded fragments.

3. **P2 — Skipping every brace group regresses brace-delimited type macros. CONFIRMED by reading.**  
   [emit.rs:1785](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/integration/crates/concord-macros/src/emit.rs:1785)

   Given `macro_rules! result { ($t:ty) => { Result<$t, Error> }; }`, declaring `send P -> result!{_};` now leaves `_` unsubstituted. The macro consequently expands to an inferred type in generated item signatures/bounds instead of the endpoint’s concrete result. The equivalent `result!(_)` substitutes it.

   This delimiter-dependent behavior is **introduced by `95ac36de`**. A brace-delimited type macro is not a const-expression block.

   **Smallest fix:** distinguish actual expression blocks from type-macro inputs, and handle supported macro placeholders consistently across delimiters.

4. **P3 — §5.11 still lists an optional value as a `HeaderMap` input. CONFIRMED by reading.**  
   [AUTHOR_GUIDE.md:1560](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/integration/docs/AUTHOR_GUIDE.md:1560)

   `error.head().map(|head| head.headers())` returns `Option<&HeaderMap>`. Passing that expression to `field::single` or `field::unsigned` still fails to compile.

   **Smallest fix:** show `if let Some(head) = error.head() { field::unsigned(head.headers(), name) }`, or put the field-reader call inside the `map`.

The six original findings resolve as follows:

| Original finding | Assessment |
|---|---|
| 1. Unsafe core `NotSent` | **Fixed.** No new core delivery defect found. |
| 2. Non-type underscores replaced | **Partially fixed.** Lifetime and braced-pattern cases corrected; finding 2 above remains. The brace exclusion also introduces finding 3. |
| 3. Forwarded fragments mishandled | **Targeted fix sound.** `parse_type` is used, and unchanged groups survive substitution. |
| 4. Coercion accepted as equality | **Fixed.** Both inherent `send()` and `Sends` require `SendWith<Out, Output = Declared>`. |
| 5. Query documentation corrupts type text | **Fixed.** `type_text` preserves literals and meaningful token spacing. |
| 6. Inaccurate guide examples | **Partially fixed.** Delivery wording and the uncertainty comment are corrected; the optional-header input remains wrong. |

The following claims are sound by inspection:

- **Core delivery classification:** I found no Concord execution path producing Build/HeaderConflict/Encode after dispatch. Encoding and header validation precede `client.execute`; execution failures become Transport. Status and Metadata retain a response; Decode and Limit arise from response processing; body failures retain the head. Consequently, `Answered` is sound as evidence that Concord received a response head—not evidence that a write succeeded. See [error.rs:347](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/integration/crates/concord-core/src/error.rs:347).
- **Redirect witness:** `w2_delivery.rs` checks both the final `is_connect()` error and the recorded original POST, then requires `Unknown`. That directly addresses the original counterexample.
- **Declared result equality:** the coercible `Box<[T; 1]>` versus `Box<[T]>` case is excluded by the associated-type equality bound. The mismatch and coercion snapshots match the intended diagnostic location.

For **B**, all ten declarations name their existing policy outputs:

| Result shape | SDKs |
|---|---|
| `Result<T, SDK error>` | Discord, NASA, FRED, Open Library, ntfy |
| `Result<Response<T>, SDK error>` | USGS, Dropbox, OpenAI, PokéAPI, GitLab |

The policy bodies and error paths remain intact; I found no lost error case or changed payload type. Their relevant protocol, metadata, streaming, and workflow assertions remain consistent with those outputs.

The header changes have these caller-visible effects:

- **Unchanged:** Discord singleton parsing; NASA/GitLab/GitHub numeric metadata; registry singleton media/digest/location/range handling; S3 singleton handling.
- **Dropbox:** surrounding spaces/tabs become accepted. Signs, repetition, and overflow remain invalid. Added tests pin whitespace and `+1`.
- **ntfy:** signed and repeated `Retry-After` values now yield no hint. Added tests pin this.
- **GitHub download:** length parsing rejects signs/repetition and accepts surrounding whitespace. Invalid length remains an absent hint; the streaming byte limit still applies.
- **Registry:** length parsing rejects a leading `+`; malformed length still produces `MalformedLength`.

For **C**:

- **Postmark:** shared failure closure preserves baseline classification. Post-prepare transport and unusable-success failures on POST remain `UnknownOutcome`. `Retry-After` deliberately becomes stricter: signed/repeated values yield no hint, as documented and tested.
- **Mailgun:** declared result and numeric-header extraction preserve behavior. Failed sends remain `AmbiguousSend`; HTTP-date/repeated `Retry-After` handling still uses its separate parser.
- **Intercom:** declared result and Missing/Invalid/Value distinctions are unchanged.
- **Zendesk:** declared result and idempotency-lookup parsing preserve behavior, including repeated fields becoming `Unrecognized`. The retained connection-rule limitation is finding 1.
- **HubSpot:** `delivery() == NotSent` is currently equivalent to its previous `ErrorKind::Build` check. Transport failures, 408, server errors, and unusable success responses remain `CreateUncertain`.

The changed README/rustdoc result-shape and uncertainty claims match these implementations, subject to the findings above. Compared with `162fc869`, the five wave-2 SDK test changes are **105 added lines, zero deletions**: no existing assertion was weakened. The three rewritten connection-refusal tests were additions from the adoption lane. The repeated-key guard is absent from the final pagination diff.

Areas not covered:

- No compilation, test execution, or rendered-rustdoc verification was performed.
- The forwarding test passes only `$error:ty = Error`; it does not exercise the original arithmetic-precedence or opaque-macro counterexamples.
- Delivery tests still omit direct delivery assertions for encoding failures, header conflicts, and limits, plus nested upload-error provenance and retry histories.
- SDK refusal tests exercise direct connection failures, not a performed write followed by a failing redirect.
- Unrelated uncommitted campaign records were outside this review.