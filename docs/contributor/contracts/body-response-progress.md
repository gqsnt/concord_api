# Bodies, responses, and finite progress contract

Concord owns the transition from application values to one managed outgoing
body and from one native response to a bounded semantic result. Request and
response extensions select approved forms; they do not take over transport,
status handling, progress accounting, or the operation lifecycle.

## Purpose

The body contract exists to preserve four properties across buffered,
streaming, factory, multipart, discard, and decoded forms:

- body work remains lazy until policy has approved the attempt;
- every native byte path converges on core-owned size and progress checks;
- accepted response acquisition uses one of three sealed modes;
- an extension cannot obtain a native or unbounded request or response.

The user workflows are in
[Request bodies and buffered responses](../../features/bodies-and-buffered-responses.md)
and [Streaming responses](../../features/streaming.md). This page owns the
maintainer-level interaction between those capabilities.

## Public model

Request-side contracts are exposed through:

- `RequestBodySpec`, with structural `metadata` followed by one synchronous
  `prepare`;
- `RequestBody`, the opaque empty, bytes, stream, factory, or multipart value;
- `BodyFactory`, which creates a fresh complete stream for a replayable
  attempt;
- `StreamBody`, the managed outgoing byte stream;
- `EncodeBody`, the ordinary complete-buffer codec contract.

Response-side contracts are exposed through:

- `ResponseBodySpec`, with status `accepts_status`, head `validate`, and
  post-acquisition `from_body`;
- sealed `Buffered`, `Streaming`, and `Discard` response modes;
- `ByteStream`, the lazy bounded accepted-response stream;
- `DecodeBody`, the ordinary complete-buffer codec contract;
- `PageResponseSpec`, the explicit buffered capability needed by pagination.

`ResponseHead` gives a selected response specification accepted structural
metadata and actual headers, but never a native response or authenticated
request URL. Its contents can still be application-sensitive and are trusted
extension input.

## Internal flow

Before `before_send`, core asks the request specification only for structural
metadata: body kind, content-type ownership, and a trustworthy length hint.
This phase must not encode data, call a factory, poll a stream, open a file, or
perform transport work.

After hook approval, `prepare` creates exactly one `RequestBody`. Core compares
the result with the earlier metadata, derives replayability, and routes every
stream-producing form through the same final managed-stream adapter. Direct
streams, body factories, multipart streams, and multipart factories therefore
share empty-chunk suppression, aggregate byte accounting, exact-length checks,
error reduction, and cancellation behavior.

On response, core first observes limiter policy and hooks, handles an
authentication challenge when applicable, and then calls the declared status
acceptance callback. The callback defaults to 2xx, runs synchronously, and is
followed immediately by a deadline check. A rejected status uses terminal
status capture. An accepted status, including an explicitly accepted non-2xx,
reaches the declared mode:

- `Buffered` checks known length, collects finite bytes, then calls
  `from_body`;
- `Streaming` checks known length, creates `ByteStream`, then calls
  `from_body` without draining it;
- `Discard` drops the accepted native body without polling it, then calls
  `from_body` with `()`.

`validate` runs before the first accepted body poll. Acceptance, validation,
and conversion callbacks are synchronous and are checked against the operation
deadline immediately after return.

## Architectural invariants

- Request construction is inert; metadata is structural; materialization
  follows hook approval.
- One attempt has one final outgoing body owner. All stream-producing forms
  converge before native transport.
- Empty chunks are never observable as successful public stream items.
- Empty-chunk progress and nonempty-byte volume are independent finite
  dimensions.
- The first chunk that would exceed a byte or empty-chunk ceiling is rejected
  without exposing or retaining that excess.
- Authentication challenge handling is core-owned and precedes declared status
  acceptance. Accepted status handling precedes response specification
  validation and conversion.
- Response modes are sealed. Extensions can choose but cannot implement a
  fourth acquisition mode.
- Decompression, when enabled, remains inside response acquisition, so the
  public response bound applies to post-decompression bytes.
- A streaming response completes when a bounded `ByteStream` is opened, not
  when the caller later finishes polling it.

The exact per-poll fairness budget is private scheduling policy. The public
aggregate empty-chunk ceilings and first-excess behavior are compatibility
facts.

## Bounds

Runtime configuration supplies positive request, buffered-response,
stream-response, status-body, and provider-body ceilings. Hard structural
limits also cover empty chunks, multipart metadata, content types, and public
head construction. The [limits reference](../../reference/limits.md) is the
single numeric inventory.

Core bounds retained transport bytes, not arbitrary extension output. A
decoder can allocate a value larger than its input; a body factory can own
unbounded application state; a stream source can remain pending forever when
the operation timeout is disabled. Those behaviors remain trusted extension
responsibilities.

Exact-length handling is ordered with aggregate enforcement. A body cannot
evade the configured byte ceiling by declaring a larger exact length, and
failure must occur without sending the first excess data.

## Errors and redaction

Request specification, producer, exact-length, and limit failures reduce to
the request-body category with a focused `BodyErrorKind`. Accepted-response
collection, stream, contract, and conversion failures reduce to response-body,
response-contract, or response-decoding categories as appropriate.

Native body errors and extension error text do not cross the public boundary.
The public error retains finite structural context and safe focused kinds.
Status-body access is an explicit bounded terminal-error capability; it is not
the accepted-response acquisition path. See
[Errors](../../reference/errors.md) for the canonical reduction table.

`Debug` implementations for managed bodies and streams omit body values,
factory state, native handles, and staged bytes. Extensions still own
confidentiality of data they log or retain.

## Extension boundary

Use `EncodeBody` and `DecodeBody` for ordinary complete codecs. Implement
`RequestBodySpec` only when structural metadata and lazy managed construction
are required. Implement `ResponseBodySpec` when status acceptance, accepted
head validation, or a sealed non-default mode is required.

Callbacks receive finite semantic contexts, not request credentials, limiter
state, native HTTP objects, or transport error chains. Synchronous callbacks
cannot be preempted, and calls for independent requests may overlap. Custom
sources and factories must bound their own queues, retries, background work,
and metric cardinality.

The detailed trait choices and implementation skeletons live in
[Request and response body specifications](../../extending/body-specifications.md)
and [Codecs](../../extending/codecs.md).

## Failure modes

Maintainers should test at least:

- pre-hook metadata rejection and hook veto without body materialization;
- mismatch between structural metadata and the prepared body;
- factory, multipart, or direct stream creation failure;
- too many successful empty chunks, including ready-empty sources;
- aggregate or exact-length excess at the first violating chunk;
- cancellation while preparing, sending, collecting, or awaiting completion;
- known-length preflight failure before polling the response;
- media-type validation or conversion failure after an accepted status;
- accepted non-2xx acquisition using response rather than status-body limits;
- streaming error after the stream has already been returned;
- discard behavior that never polls the accepted body.

A streaming error delivered after opening is a `ByteStream` item. It does not
retroactively alter the already-fixed `StreamOpened` completion outcome or
emit a second completion event.

## Change hazards

A new body form must converge on the existing managed boundary. Do not add a
parallel native conversion path, run factories during metadata, or make
replayability an unchecked declaration. A new response capability must fit a
sealed mode or justify a deliberate core-contract change; merely exposing a
native response would bypass finite acquisition, redaction, and completion.

Changes to empty-chunk handling require aggregate, per-poll, wake, terminal,
and first-excess tests across request streams, buffered responses, byte
streams, status capture, and provider HTTP. Changes to decompression must be
reviewed against post-decompression accounting.

## Verification

The primary owners are:

- `crates/concord-core/src/body/prepared/tests.rs` for final request-body
  convergence and progress;
- `crates/concord-core/src/execute/tests/laziness.rs` for pre-hook laziness,
  factories, multipart, limits, and exact length;
- `crates/concord-core/src/response/handler.rs` and
  `crates/concord-core/src/response/stream.rs` for sealed acquisition and
  streaming progress;
- `crates/concord-core/src/execute/tests/status.rs` and
  `crates/concord-core/src/execute/tests/streaming.rs` for lifecycle
  integration;
- `crates/concord-core/tests/bounds/status_body_contract.rs` for bounded
  terminal status bodies;
- `crates/concord-core/tests/public_api/body_extensions.rs` and
  `crates/concord-core/tests/public_api/stream_response_api.rs` for the stable
  extension surface;
- `crates/concord/tests/extensions/contracts.rs` and the body/stream examples
  for downstream sufficiency.

Run the focused owner first, then the workspace all-feature suite, doctests,
warning-denied Rustdoc, and relevant UI checks. The maintained matrix is in
[Testing strategy](../testing.md).

## Related

- [Finite behavior](../../concepts/finite-behavior.md)
- [Security and redaction](../../concepts/security-and-redaction.md)
- [Records and pagination contract](records-pagination.md)
- [Managed executor](../architecture/managed-executor.md)
