# Concord costs encountered

Baseline crates remain frozen. This is an **example-assisted** SDK. Entries below describe
observable public APIs/source and actual adapter code, with proposed changes only.

| Cost and evidence | Handwritten work / ownership | Smallest shared change that could remove the generic part |
| --- | --- | --- |
| No XML format is listed in concord-codecs README or exports, while the provider defaults to XML. | `src/xml.rs` implements Decode<SearchPage>, DOM node/UTF-8 bounds, required/singleton fields and provider mapping. Generic media grammar already comes from `check_content_type`; no parser was duplicated. Europe PMC wrappers/field rules belong here. | An optional finite XML codec with explicit parser limits would remove common parsing/format glue; the provider envelope and model subset would still belong to the SDK. |
| The paginator's Budget rustdoc explicitly says it does not compare keys. A repeated cursor is not caught by Pages itself. | `src/pagination.rs` retains a bounded BTreeSet of requested keys and checks the returned token before emitting the page; cloned snapshots keep futures owned without extra locks. Provider repeat-as-error policy belongs here; remembering keys is generic. Workflow tests exercise immediate and longer cycles. | An opt-in bounded repeat-key guard on Pages for comparable keys, with an SDK error callback. Keep this opt-in because providers differ on terminal repeated keys. |
| Generated scope values have setters, but the policy's Endpoint interface exposes only prepare/operation, not the typed requested page size. | SendPolicy inspects `Prepared::request().url().query_pairs()` for pageSize, then validates the decoded count. This is native inspection only: no URL editing, raw execute or decoder escape. Request-bound validation is provider policy; recovering a known input from wire text is mechanics. | Read-only generated value accessors, or an SDK context projection on Endpoint, would let this policy receive the validated typed bound. |
| Bounded raw HTTP diagnostics require keeping status alongside bytes; Received::bytes_limited returns only bytes. | `received.meta()` clones a header map before consuming bytes_limited; HttpError retains that head and either bounded bytes or the read failure. Choosing raw diagnostics instead of assuming a provider error schema is SDK policy. | `bytes_limited_with_meta`, parallel to decode_with_meta/document, would move the success head instead of requiring the extra clone. |
| Pages resume keys cannot prove that base/query/page-size/format are the original plan; README explicitly delegates that association. | The search scope fixes each active plan and documentation requires the same scope configuration when resuming. No custom persistence/checkpoint framework was added; history resets on resumption. | Optional application-supplied plan identity attached to shared checkpoints could reject accidental cross-plan reuse. Provider snapshot changes would remain outside Concord. |

The JSON/XML operations intentionally declare the same search route twice with different
format query literals and response types. Concord correctly keeps each decoder fixed; this
is how the SDK prevents default XML from entering JSON decoding. The core lookup's exact
query composition, response identity checks, source/id alphabets, record cap and explicit
failure types are provider/SDK semantics, not requested generic Concord changes.

## Shared capabilities evaluated

All five capability READMEs were read before choosing:

- `concord-pagination`: used Pages, Resume, Page, mandatory max_pages, explicit bounded/error
  endings, prefixes and reports. No homemade traversal engine. Search metadata is hit_count.
- `concord-codecs`: standard Json via the facade, plus its exported media checker/Charset
  directly for XML. Strict checks happen before reading a body. No IgnoreContentType escape.
- `concord-retry`: used the common Retry-After parser with caller-supplied wall time. The
  scheduling engine does not fit the chosen single-attempt slice; no retry budget/policy was
  invented. The provided reqwest client disables its own retries, and wire counts are tested.
- `concord-auth`: not used; the bounded API slice is public and has no credentials or token
  acquisition. Adding credential mechanics would not implement a provider requirement.
- `concord-ratelimit`: not used; the allowed sources do not establish a quota model for this
  slice. Traversals are sequential; 429 is surfaced, with Retry-After available. A process-local
  invented rate window would not establish the provider's quota. No custom limiter was written.

No native execution escape is used. The XML decoder is an ordinary Concord format and all
workflows call generated send(). Prepared::request is inspected only for the request's
pageSize. The protocol test of prepared.send() verifies that the declaration's byte limit
survives that public lower-level path.

## Example consultation

Directly read `examples/testserver/Cargo.toml` (dev dependency identity) and
`examples/testserver/src/lib.rs` (fixture server, captured requests, scripted body faults,
shutdown). No existing SDK source file was directly opened. The required guides indirectly
supplied `examples/quickstart/src/lib.rs` declaration/send excerpts,
`examples/frankfurter-sdk/src/lib.rs` stateless bounded-error policy excerpt and the
`examples/fred-sdk/src/lib.rs` transport recipe. Those contributions are labelled separately
in the complete [PROVENANCE ledger](PROVENANCE.md#workspace-reading-and-example-assistance).
No provider models or fixtures were copied from an existing SDK.
