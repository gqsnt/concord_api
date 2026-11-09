# SDK27 Concord friction

Historical author notes. P4.3 now fixes required handle values and declares checked runtime bases; see the current README and `docs/MIGRATION.md`.

Baseline: `8c27cd1a509c4cac4edde44f60c3d3ca13e2b46f`. Example-assisted author result;
the complete example-file inventory and what each contributed are in
[PROVENANCE.md](PROVENANCE.md#local-reading-and-example-assistance).
No shared crate, check, other example, or script was changed. These are bounded
observations and proposals, not claims of implemented Concord improvements.

| ID | Observable evidence / handwritten cost | Ownership and smallest useful change |
|---|---|---|
| F1: streamed multipart | `concord-codecs` README and multipart rustdoc explicitly require buffered parts. `MailgunMultipart::encode_body` has a shared-codec branch and a native reqwest Form branch; field mapping is repeated. The one-shot test proves preparation polls zero times and `try_clone()` is unavailable. | Provider field names/validation stay in SDK. Stream framing and runtime boundary binding are generic codec mechanics. A streaming multipart body/encoder in `concord-codecs` would remove the reqwest branch and duplicate mapping. Preserve one-shot ownership; do not buffer to manufacture replay. |
| F2: body-plus-media encode ceremony | A runtime-media format must implement the otherwise unusable `Encode::encode` method as a refusal as well as `encode_body`; the protocol test exercises that refusal. | Generic core/codec contract, not Mailgun. A default refusal for the body-only method would remove this implementation obligation without changing the used `encode_body` path. A larger trait redesign is not required by this witness. |
| F3: following links leaves generated policy dispatch | First pages use generated `.send()`. A followed `Prepared` has no policy association, so the factory calls private `SendPolicy::execute`, the same function called by `SendWith::send`. Ordinary `Prepared::send` would lose SDK HTTP-document and ambiguous-send handling. | Generic execution ergonomics. A policy-preserving followed operation, or an explicit prepared-policy dispatch helper, would remove this split. Same-origin protection is already in core; domain path trust stays in SDK. No new endpoint or alternate hierarchy is needed. |
| F4: resume context and cycles | `EventCursor` stores original URL/query/credential; `pages_from` compares them. A per-traversal HashSet stops repeated URLs. The shared crate documents that it cannot detect context mismatch/cycles. Tests exercise recipient/key/domain changes and a repeating URL. | Generic binding/cycle mechanics could be optional `concord-pagination` helpers parameterized by an SDK context and key equality. Mailgun's empty-page termination and link collection rules must remain SDK rules. Existing mandatory page caps already bound storage. |
| F5: transport guarantee outside the declaration | `connect`/`local` construct reqwest with `retry(never)` and redirect `none`; generated `new` still accepts opaque caller transports. Timeout tests assert one fully recorded send and one connection. | Transport is application/SDK context, not a new core default. A documented reusable strict-transport builder in an optional external helper would shorten setup. An arbitrary supplied reqwest client cannot be certified; advanced callers retain that obligation. |
| F6: typed HTTP policy packaging | `SendPolicy` packages `document::<Json, Problem>`, `decode_with_meta`, status classes and redacted output/error types. Core already provides bounded reads and retained heads, so none of those mechanics were reimplemented. | Generic result packaging remains modest repeated code; provider classes and send ambiguity must stay in SDK. The smallest further improvement is a shared response-policy recipe/helper over caller-supplied typed documents and projections, not a Mailgun-specific macro keyword. This witness alone does not justify a new core abstraction. |

The declaration and generated path fit both workflows. Client owns credentials
and regional base; scope owns the domain; endpoints own send/search. The ordinary
JSON event read is the simple control. Buffered multipart uses the shared codec;
only the lazy encoder uses a native codec. No URL/HTTP request is rewritten to
impersonate an undeclared endpoint.

## Shared capability decisions

- `concord-auth::basic` fits directly: correct encoding and sensitive header flags.
  Static API keys need no token cache, acquisition, refresh or 401 replay.
- `concord-pagination::Pages` replaces the generic walk, page budgets, incomplete
  prefixes and failure/resume reports. Its Link-header parser does not apply to
  Mailgun body URLs. The SDK keeps opaque URL validation and empty-page semantics.
- `concord-codecs` JSON and buffered multipart fit. Streaming multipart does not;
  F1 records the narrow native encoder escape. `EncodedBody` keeps runtime media
  and bytes inseparable, even for the lazy path.
- `concord-retry` contributes its strict `Retry-After` parser. Its scheduling
  engine is intentionally not used: this slice promises one application attempt
  for both sends and reads. Wrapping that in a one-attempt Retry adds bookkeeping
  without removing work. No automatic write replay is safe merely because bytes
  can be cloned; provider delivery retries are separate from resubmitting mail.
- `concord-ratelimit` was read and not adopted. The contract gives observations,
  not a configured local admission budget or account-wide coordination policy.
  Rate headers are exposed (invalid/repeated values stay distinct), without
  inventing a quota or interpreting a reset into a sleep. An application can
  choose a shared limiter; no change to Concord is required for this omission.

## Diagnostics and research costs

The initial library build passed. The first integration-test compile reported
E0609 for an authored `progress.requested` assertion (the public field is
`admitted`) and E0382 when formatting an operation while consuming it with
`prepare()`. Reading the signatures and cloning the read-only operation fixed
both. These were author mistakes, not Concord implementation defects; no shared
change is proposed. No `Clone` was added to send bodies.

The HTML documentation reader hid nested event properties; the official `.md`
schema exposed the test-mode flag. A regression assertion now prevents a false
real-delivery conclusion. Retention and timestamp-unit discrepancies are recorded
in PROVENANCE; these are provider documentation costs, not Concord costs.

Directly consulted example files were only `examples/testserver/Cargo.toml` and
`examples/testserver/src/lib.rs`. Required-guide excerpts/descriptions from
Quickstart, Shapes, OpenAI, Google Drive, Frankfurter, Native, Compat, Multipart
and App informed the implementation; the provenance table names every file and
distinguishes direct reading, embedded excerpts, prose-only recipes and non-adoption.
