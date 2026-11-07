# Friction and capability decisions

This is example-assisted; the complete direct example-file ledger is below. Costs are
observable implementation work, separated from provider rules. No shared code was changed.

## F1 — envelope validation/projection around generated decoding

Evidence: `src/policy.rs` must use `prepare()?.receive()` and decode an `Envelope<T>` before
returning `QueryResponse<T>`. `api_errors_warnings_and_missing_query_are_never_success` proves
HTTP 200 alone cannot mean success. `http_status_stays_authoritative_for_every_error_body_failure`
checks why non-2xx needs its separate bounded document result.

Handwritten: one generic `SendWith<Envelope<T>>` policy, provider envelope/error types, warning
refusal and content projection. The provider owns code/info, warnings and revision semantics;
generic mechanics are receiving/decoding and moving metadata. Existing `SendWith`,
`decode_with_meta` and `document` fit well. No core change is warranted to interpret MediaWiki.
The smallest optional generic improvement would be a post-decode mapping hook that retains
metadata, reducing policy dispatch boilerplate while leaving the provider mapping here.

## F2 — complete continuation as a domain value

Evidence: `src/models.rs` implements `QueryPairs` and a deserializer; the category regression
shows dropping only generic `continue` repeats ID 1. The null regression initially failed
because Serde treated null as `None`. Numeric search offsets and an extra future field pass.

Handwritten: an immutable whole-map value, string/integer spelling, generic-field/fixed-key
validation, and absence-versus-null decoding. These are provider rules and conservative SDK
contract checks. Concord already emits runtime keys through ordinary `query(..)`; no production
native escape is required. No Concord change should encode these provider rules. A reusable
strict present-object Serde adapter could remove the tiny presence helper, outside Concord core.

## F3 — projected send result has an associated signature

Evidence: the declaration says `send QueryPolicy;`, so generated rustdoc names
`<QueryPolicy as SendWith<Envelope<SearchResults>>>::Output` instead of the spelled-out
`Result<QueryResponse<SearchResults>, Error>`. The documented `send ... -> Result<_, Error>`
template substitutes the whole declared envelope for `_`; it cannot bind its inner `T`.

Handwritten: public result types and concrete calling examples document the resulting shape;
there is no wrapper per endpoint. This is generic generated-usage friction, not provider
semantics. The smallest improvement is better generated rustdoc showing the concrete associated
output when the compiler can resolve it, or an explicit documented policy-payload projection
binding in result templates. Neither is implemented in this bounded SDK.

## F4 — runtime-pair diagnostic sensitivity is limited

Evidence: `docs/AUTHOR_GUIDE.md` §4.1a disallows `#[sensitive] query(..)`; the `QueryPairs`
rustdoc says prepared URLs show dynamic pairs except well-known secret names. The custom
`Continuation` Debug/Display cannot hide a prepared request's URL representation.

Handwritten: value-free token formatting, secret-safe error types and explicit public-data
diagnostic documentation. These particular continuation tokens contain public query state,
so the slice does not require additional secrecy. The generic limitation remains observable.
The smallest core improvement would allow whole-value sensitivity for runtime-pair placements
and retain those pairs' redaction ranges in `Prepared` metadata.

## Shared capability decisions

All five READMEs were read before deciding:

| Capability | Decision |
|---|---|
| `concord-pagination` | Used: `Pages`, required `max_pages`, `Resume::At`, complete collection/incomplete prefixes and terminal errors. The SDK only clones a generated category request and projects members plus the whole continuation. An empty intermediate page and exact final-page cap both pass. |
| `concord-codecs` | Used via Concord's `json` feature: strict JSON head validation, decoding and static safe diagnostics. No custom codec is needed. Declarative `limit` also applies to prepared sends. |
| `concord-auth` | Not used: no credentials, acquisition, cache or refresh exists in this public-query slice. The caller's identifying User-Agent is a normal required header, defensively redacted. Adding auth machinery would invent a workflow. |
| `concord-retry` | Not used: the bounded slice promises one attempt, returns 429/5xx and preserves Retry-After headers. There is no requested retry contract or need to invent counts/backoff. Supplied transport owns native retry settings. |
| `concord-ratelimit` | Not used: no fixed provider quota/window is established by the permitted sources; serial category pagination and caller-controlled admission suffice. A local limiter would introduce an arbitrary quota claim. |

## Examples consulted

| Pre-existing file directly read | What was taken |
|---|---|
| `examples/testserver/src/lib.rs` | Required fixture API: recorded request shapes, query pairs, replies, interrupted body and owned shutdown. |
| `examples/testserver/Cargo.toml` | Correct dev dependency package name and runtime features. |

No pre-existing `examples/*-sdk` source was opened. Excerpts embedded in the five required
guides informed ordinary declarations/send policy/finite bounds/pagination. This is labelled
example-assisted rather than independent of examples. No other example file was consulted.

## Native escape and proposed shared edits

Production code makes no native request edits. The test-only negative control uses
`prepare().into_parts()` and `url_mut().query_pairs_mut()` to intentionally send an incomplete
token. That mutation exercises the failure contract; it is not an SDK workaround.

F3 and F4 are the concrete shared-surface improvements worth recording. F1's optional hook and
F2's presence adapter are conveniences; the existing mechanisms handle this slice. Proposed
changes remain documentation only, as required by the frozen baseline.
