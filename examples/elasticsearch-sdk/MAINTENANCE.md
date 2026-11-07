# SDK48 Elasticsearch maintenance

Date: **2026-11-07**. Live provider calls: **NOT_RUN**. All exercised requests use
`examples/testserver::TestServer::start(|request: &Recorded| -> Reply)` on loopback.
No commit was attempted. No shared crate, check, script or other example was edited.

## Prior exposure and order of work

I did not author this SDK and had no earlier SDK exposure in this session. Before the first
implementation attempt, I read the maintenance capsule; root and SDK Cargo manifests; the SDK
README and PROVENANCE; `src/{lib,model,error,bulk,pit}.rs`; and
`tests/{first_use,protocol,workflows}.rs` plus `tests/common/mod.rs`. This was a source-assisted
maintenance attempt, not a docs-only first-use exercise. A worktree search found no AGENTS.md.

I searched Concord's core, macro and test sources for path encoding, send policies, response
metadata and query-pair extension points. Relevant reads included
`crates/concord-core/src/{response,value}.rs`, the send-parser section of
`crates/concord-macros/src/syntax.rs`, query-pair/send-policy test examples, and
`examples/testserver/src/lib.rs` (reply, request recording, stall and delay APIs).
Some exploratory searches named nonexistent `received.rs`, `send.rs` and `query.rs`; the
actual implementations were located with `rg`. Large combined outputs were truncated;
the needed SDK/reference sections were subsequently read in focused extracts.

I read the five official pages below before implementing. I did **not** read FRICTION.md or
FIRST_USE.md until after the implementation, new regression/consumer tests, and the first
complete successful 40-test package run. Those two deferred notes were then read for comparison;
their referenced other-service examples were not opened. No campaign or parent-directory files
were read. Research stayed on the five authorized documentation pages.

## Documented contract and changes

Official pages read **2026-11-07**:

- [Get a document by its ID](https://www.elastic.co/docs/api/doc/elasticsearch/operation/operation-get):
  GET metadata and the two possible 404 body shapes.
- [Create or update a document in an index](https://www.elastic.co/docs/api/doc/elasticsearch/operation/operation-index):
  replacement source, both conditional parameters, response fields and HTTP 409 on mismatch.
- [Optimistic concurrency control](https://www.elastic.co/docs/reference/elasticsearch/rest-apis/optimistic-concurrency-control):
  obtain the sequence number/primary term pair from GET and supply both to the later write.
- [Bulk index or delete documents](https://www.elastic.co/docs/api/doc/elasticsearch/operation/operation-bulk):
  preserved NDJSON framing and inspection of every item, including errors inside HTTP 200.
- [Paginate search results](https://www.elastic.co/docs/reference/elasticsearch/rest-apis/paginate-search-results):
  preserved PIT lifetime/latest-id and exact last-hit sort values for search_after.

`Elasticsearch::get_document(index, id)` now returns `GetDocument::Found(Document)` or
`GetDocument::NotFound { index, id }`. Only a valid get-result with `found: false` and HTTP 404
means absence. A missing index or unreadable 404 remains a provider error. Found documents
require a usable sequence number and primary term, expose their source deliberately, and hide
it in Debug. Source storage may be disabled, in which case source access returns None.

`Elasticsearch::conditional_index(index, id, version, source)` sends a complete JSON replacement
with both `if_seq_no` and `if_primary_term`. `DocumentVersion` makes the pair one required input;
it is distinct from Elasticsearch's `_version` counter. HTTP 409 is
`Error::VersionConflict(Box<ProviderError>)`, including when the bounded error body is unreadable.
The caller retains status, metadata and typed cause access. No conflict handler rereads or
changes a token automatically.

Both operations are declared under the existing `ElasticsearchApi.index(..)` scope, preserving
client -> scope -> endpoint. Generated endpoint `send()` implements the same typed outcomes.
The convenience methods additionally correlate the returned document id. Bodies and responses
are bounded: source serialization stops at 8 MiB, successful answers at the existing 16 MiB,
error/404 documents at the existing 64 KiB. Single-document JSON permits line breaks.

The SDK performs one send and no automatic retries. A timeout before the head or during a
successful response body is uncertain, as are a reset, malformed success or 5xx. The supplied
reqwest client must have automatic retries and redirects disabled; this SDK cannot inspect or
override its policies. An uncertain outcome requires explicit reconciliation, never blind replay.
Existing bulk and PIT endpoint signatures, request shapes and error classifications are preserved.

## Locations edited

| File | Location and purpose |
|---|---|
| `src/lib.rs` | Crate rustdoc/exports; `ElasticsearchApi.index` GET/PUT declarations; `SendPolicy`, `ResponsePolicy`, `standard_response` and `provider_error`; public `get_document`/`conditional_index` wrappers and transport rustdoc. |
| `src/document.rs` (new) | `DocumentVersion`/`QueryPairs`, bounded `DocumentSource`/`SourceWriter`, `GetDocument`/`Document`/`GetWire`/`Get404`, `IndexResult`/`IndexResponse`, and the two response-policy implementations. |
| `src/model.rs` | `Display for DocumentId`, required when the existing id newtype is used in a generated path segment. Existing id validation/serialization stays intact. |
| `src/error.rs` | `VersionConflict`, status/Display handling; write-wide uncertainty rustdoc; source/version input errors and document id/status protocol errors with exhaustive formatting arms. |
| `tests/documents.rs` (new) | Eight focused fixture tests: generated wire contract, 404 distinction, token/status validation, typed conflict diagnostics, timeouts, other write errors, path/id correlation, bounded input preflight. |
| `tests/maintenance_use.rs` (new) | Public caller reads and replaces, detects its stale token, explicitly rereads/merges while preserving another edit and a changed primary term, then handles normal absence; exact call count forbids hidden replay/reread. |
| `README.md` | Third workflow, generated endpoint usage, complete-replacement/token contract, failure table, transport requirements, bounds, updated exclusions and tests. |
| `PROVENANCE.md` | Dated maintenance reference table, inline fixture derivations, SDK decisions and unchanged dependency/lock accounting. |
| `MAINTENANCE.md` (new) | This record. |

Root Cargo.toml already listed this member and Cargo.lock already contained the package.
Neither needed editing. `src/bulk.rs`, `src/pit.rs`, existing fixtures and every existing test
file, especially `tests/first_use.rs`, are byte-for-byte unchanged from the starting checkout.
No existing assertion was removed or weakened.

## Facts repeated and diagnostics

- The read/replace operations appear in the declaration, convenience methods, public exports,
  README and fixture expectations. Adding only a declaration would not produce the complete
  public workflow or update its documentation.
- `_seq_no`/`_primary_term` are required in `DocumentVersion` and conditionally required in
  `GetWire` (a missing document has neither). They appear in both suites' synthetic response
  fixtures. The query spellings live in one `QueryPairs` implementation, keeping the pair
  mandatory in both generated and convenience calls.
- 404 body discrimination and 409 rejection rules appear in policy code, public error/result
  documentation and independent fixture expectations. Adding the conflict variant also required
  status and Display arms; the compiler checks those matches. No omitted arm was encountered.
- Source bounds/object requirements appear in the constructor, README and preflight regression.
  Redaction is explicit in source/document Debug implementations and tested through generated
  request diagnostics and provider errors.

The initial implementation compiled; the compiler reported an unused test client variable,
which was removed. The first test run had **7 document tests pass, 1 fail**: the contradictory
`404`/`found: true` regression expected `Protocol::DocumentStatus`, but the original untagged
404 enum could not decode its `RawValue` source through Serde's buffered enum representation.
The fix uses a directly decoded GET envelope, discriminates a required error member, and then
constructs the typed result. The existing assertion was kept. All tests then passed. This was
a test-detected response-decoding error, not an omitted edit caught by compilation. No deliberate
omission/mutation experiment was run during maintenance.

## Remaining duplication and responsibilities

Concord's send policy is client-wide: `syntax.rs` explicitly rejects endpoint/scope `send`.
The SDK therefore has four default `ResponsePolicy` implementations and two specialized ones.
The public, doc-hidden trait is needed to route the public generic SendPolicy by output type;
custom users of SendPolicy with new output types now need a ResponsePolicy implementation.
Each of the six existing/new SDK endpoint outputs has one. An endpoint name string is never
used as a dispatch key.

The wrappers repeat index/id conversion and response-id checks. Raw source access/redacted
Debug resemble `pit::Hit`; bounded JSON-object encoding overlaps with bulk's object validation.
The implementations remain separate because bulk also forbids raw line breaks and retains
each operation for item correlation. GET's optional wire token fields and the successful-write
token fields repeat response schema facts. Fixtures deliberately repeat provider facts rather
than deriving expectations from SDK types.

Elasticsearch-specific work belongs here: interpreting `found`, distinguishing an absent
document from an absent index, coupling the concurrency pair, treating 409 as a stale revision,
complete replacement versus partial update, redacting sources/reasons, interpreting bulk item
outcomes and preserving PIT/cursor state. Conflict merging and uncertain-write reconciliation
remain caller decisions. Positive primary terms, input caps and conservative uncertainty are
explicit SDK policies, not claims that the reference pages mandate these exact representations.
Cluster administration, arbitrary index options, routing, bulk concurrency options, scroll,
unbounded buffering and automatic retry remain outside this slice.

Candidate Concord improvements, recorded only (frozen shared baseline respected):

1. In `crates/concord-macros`, allow an endpoint response/send policy override with the same
   typed result template. That would remove the SDK's output-type dispatch trait without
   moving Elasticsearch's 404/409 semantics into Concord.
2. In `crates/concord-core`/codecs, consider a bounded JSON request encoder or reusable bounded
   serialization sink. A response limit does not bound a caller source while encoding it;
   this maintenance needed a local writer. Object-only validation would remain the SDK's rule.
3. In shared usage documentation, show one provider whose 404 is a typed success body and
   whose 409 is a typed rejection, using `Received::document` with an independent error bound.
   Explain that `prepare()?.send()` bypasses the SDK's policy, and show timeout uncertainty
   without retries. Existing delivery/head metadata was sufficient; no core error change is needed.

The deferred FRICTION note independently describes manual provider semantics and redacted data,
and the FIRST_USE note confirms that the original generated/public usage worked. Maintenance
adds the endpoint-specific policy gap and bounded single-source encoding above; it does not
change their historical observations.

## Validation and scope

Commands use `CARGO_BUILD_JOBS=4`, execute sequentially and use the worktree's default target/.

- First build: `cargo test --offline -p concord-sdk-elasticsearch --no-run` (without --locked),
  passed; root membership/lock already present and unchanged.
- `cargo test --offline --locked -p concord-sdk-elasticsearch`: **40 integration tests passed**
  (8 document regression, 1 maintenance consumer, 2 original first-use, 19 original protocol,
  10 original workflow); zero unit/doc tests, zero ignored tests.
- `cargo clippy --offline --locked -p concord-sdk-elasticsearch --all-targets -- -D warnings`:
  passed without warnings.
- Final package test rerun after the consumer/documentation changes: again **40 passed**,
  zero failures or ignored tests.
- `RUSTDOCFLAGS='-D warnings' cargo doc --offline --locked -p concord-sdk-elasticsearch --no-deps`:
  passed; public/generated rustdoc had no warnings.
- `rustfmt --edition 2024 --check` on all six changed/new Rust files and `git diff --check`:
  passed. Final source/diff review found no further changes necessary.
- `git diff --name-only c3fd4f1ee30c1727e26b285270f2cc8d8b694e77 -- crates checks scripts`
  was empty; root Cargo.toml, Cargo.lock and original first-use test diffs were also empty.

No new dependency, feature or lockfile package version was introduced. Final artifacts are left
in the working tree; live provider behavior remains **NOT_RUN**.
