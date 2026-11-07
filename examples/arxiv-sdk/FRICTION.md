# Concord costs and capability choices

No shared crate, check, script or existing example was edited. Suggested changes
below are observations for Concord maintainers; provider models and decisions stay
inside this SDK.

| Cost / evidence | Handwritten work and boundary | Smallest change that could remove the generic cost |
| --- | --- | --- |
| No XML format among the formats listed in `concord-codecs/README.md`. `src/atom.rs` is 163 lines; protocol tests cover prefixes, extension collisions, DTDs and malformed XML. | Namespace-aware Atom projection, required/optional fields and error-entry detection. Atom/arXiv semantics are provider work; XML plumbing is generic. | An optional namespace-aware XML frontend/helper in codecs could supply tree parsing, limits and static parse diagnostics. Keep the Atom model and extension selection here. No core/macro change required. |
| A `SendWith` policy receives an endpoint to prepare, not its typed argument values (`concord-core/src/prepared.rs`). `policy.rs` reads `start` and `max_results` from the built query to enforce a combined window limit. | A small query projection and comparison with echoed response fields. Window limits are provider semantics. The checks run before admission, without rewriting the request. | No shared change is necessary: a future SDK `Window: QueryPairs` input could enforce the combined rule before declaration construction. Typed request-validation hooks could avoid projection, but would broaden Concord for little benefit in this slice. |
| `prepare()` deliberately bypasses a client's send policy (AUTHOR_GUIDE §5.13). The native-prepared-send test still observes the declared byte limit. | Explicit documentation that direct prepared execution bypasses pacing/window and Atom-error interpretation. No native mutation is needed internally. | Generated `prepare()` rustdoc could mention the declared policy name and list that it is not invoked. Behavior should remain an explicit native exit. |
| One specialized policy is needed for 2xx error feeds as well as HTTP failures (`policy.rs`), with safe diagnostics in `error.rs`. The HTTP error-body matrix proves status survives every document failure. | Atom error classification and boxed SDK errors. Provider classification belongs here; bounded document reading and response metadata are shared already. | No shared change required. `SendWith`, `document`, `decode_with_meta`, redacted metadata and declared result templates fit this use directly. |
| Search continuation must join OpenSearch fields to request state (`lib.rs::pages`). Bad offsets, overfull/empty pages and resume tests exercise this adapter. | Provider field extraction and the 30,000 window boundary. Scheduling, prefix retention and checked arithmetic are delegated. | No shared change required: `next_offset` and `Pages` already remove the generic loop and continuation arithmetic. |

Capability decisions after reading every requested companion README:

- **concord-pagination:** adopted `Pages`, mandatory `max_pages`, `Resume`,
  `next_offset`, explicit bounded/provider errors and complete collection with a prefix.
  The SDK chooses the offset fields and validates response/request agreement.
- **concord-ratelimit** with **`tokio`:** adopted one three-second fixed-window key
  and a concurrency lease held through body consumption. `TokioClock` is the production
  clock; virtual clocks only drive local tests. Existing clock injection fits directly.
- **concord-codecs:** adopted public `check_content_type` and `Charset::Utf8` without
  optional serializer features. A custom `Decode<Feed>` is necessary for Atom XML.
  The shared media parser avoids repeating singleton/charset logic.
- **concord-retry:** not adopted. This bounded public-read slice sends once and reports
  errors. There is no provider-specific retry contract requiring an automatic replay;
  adding attempts would add policy and latency. Reqwest native retry is disabled in the
  supplied transport. A future opt-in retry policy must admit every attempt separately.
- **concord-auth:** not adopted because both operations are public with no credentials,
  refresh or signing. Tests assert that the SDK adds no Authorization header.
- **Concord facade/macros:** ordinary `api!`, scope, keyed queries, initialized values,
  `#[sensitive]`, custom response format, inherited `limit`, and the generated `send`
  policy result template supply the public usage. `reqwest-defaults` supplies transport
  TLS support. JSON, streaming, multipart and form formats are unnecessary.

Example assistance: only `examples/testserver/Cargo.toml` (harness dependency setup)
and `examples/testserver/src/lib.rs` (request/reply/body/lifecycle API) were directly
consulted. No other `*-sdk` file was opened. Embedded quickstart and provider-policy
snippets in the required guides were used as documented in [PROVENANCE.md](PROVENANCE.md).
