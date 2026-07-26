# Records and pagination contract

Record streaming and pagination both turn one declared response shape into
multiple application items, but they have different ownership. Records decode
one successful streaming body incrementally. Pagination executes a finite
sequence of complete buffered page requests. They share response contracts and
finite behavior without sharing an execution loop.

## Purpose

This distinction prevents two easy architectural mistakes:

- treating transport chunks as records, which would make framing depend on
  network behavior;
- treating pagination as one transport request, which would bypass per-page
  authentication, limiter, hook, deadline, and completion policy.

`RecordStream` is therefore layered over `ByteStream`, while pagination wraps
the managed executor and invokes it once per page.

## Public model

The record surface consists of:

- `RecordDecoder`, a request-local incremental framing and decode contract;
- `Records<D>`, a streaming `ResponseBodySpec` selecting decoder `D`;
- `RecordStream<D>`, which yields one `Result<Record, RecordError>` at a time;
- `RecordBatches<D>`, an optional positive-size consumer adapter.

The pagination surface consists of:

- `PaginationController<Page>`, request-local continuation state;
- `PageResponseSpec`, explicit opt-in for buffered iterable responses;
- `PageLimit::Take` and `PageLimit::HardCap`, two different termination
  policies;
- the generated `.paginate(...)` operation, which resolves to a collected
  `Vec<Item>`.

The generated pagination binding remains private plumbing. Application and
extension code use the stable controller and request surface, not the hidden
endpoint integration trait.

## Internal flow

### Record stream

After successful status handling and streaming known-length preflight, core
calls decoder `validate`, creates one decoder with `start`, and opens a
`RecordStream`. Polling then alternates between decoder progress and bounded
`ByteStream` input:

1. call `decode` on currently staged bytes;
2. yield one record when strict net buffer consumption produced it;
3. poll the byte stream only when the decoder needs more data;
4. append a nonempty chunk only if it fits the independent staging bound;
5. after body EOF, call `decode_eof` until the final framing decision;
6. report incomplete trailing data or finish.

Network chunks are never record boundaries. The decoder owns framing, and one
logical output must correspond to staged-byte consumption.

### Pagination

The aggregate pagination future is inert until first poll. It then creates one
parent operation identity and one initial controller. For each page,
sequentially:

1. validate current controller state;
2. bind it into a fresh typed endpoint snapshot;
3. prepare a canonical credential-sanitized request identity;
4. reject a repeated identity or an identity that exceeds retained-byte
   policy;
5. execute the page through the ordinary managed executor with a child
   operation identity;
6. complete the page operation;
7. collect its decoded items under the configured item ceiling;
8. apply short-page and requested-limit rules;
9. call `next` only when another page may be requested.

After success or failure is fixed, pagination invokes the aggregate completion
hook outside page deadlines. Controller callbacks are also outside the
per-page deadline and are trusted synchronous application work.

## Architectural invariants

- A decoder cannot increase the staging buffer.
- Returning `Some(record)` requires a strict net decrease in staged bytes.
- Returning `None` may consume bytes; no progress causes core to poll the body
  or resolve EOF rather than spin on the decoder.
- Record staging has its own bound in addition to the cumulative byte-stream
  bound.
- An append that would first exceed staging capacity retains none of the
  excess chunk and yields no oversized record.
- Record errors after opening are stream items; dropping the record stream
  drops its underlying byte stream.
- Pagination pages execute sequentially and use the ordinary executor.
- Cycle identity is derived from endpoint, method, logical URL, and public
  headers, never private credential placement.
- Every complete retained identity is charged to the configured identity-byte
  budget; the page limit also bounds identity count.
- Pagination returns the complete collected vector on success or an error. It
  does not return a partial accumulated result.
- Page and aggregate operation identities, completion events, and recovery
  metadata remain correlated.

Batching does not change the primitive record contract. The adapter holds at
most its positive batch size, drops a partial batch on underlying failure, and
cannot bound memory retained by the caller after a batch is yielded.

## Bounds

`ByteStream` applies the response-wide nonempty-byte and empty-chunk bounds.
Each `RecordDecoder` additionally declares a positive staging limit.
Pagination has independent page, collected-item, and retained-identity-byte
ceilings. The exact defaults, hard limits, and configuration methods belong to
the [limits reference](../../reference/limits.md).

Controller and decoder state are downstream-owned. Concord does not measure
their heap allocation, callback duration, metrics, caches, or spawned work.
`PaginationController` callbacks must be non-blocking and perform no external
I/O because they run synchronously outside the page deadline.

`PageLimit::Take(n)` means “collect no more than `n` pages and succeed after
the last allowed page without asking for another continuation.”
`PageLimit::HardCap(n)` means “allow natural termination through page `n`, but
fail if continuation still exists at the cap.” Conflating these policies
changes user-visible success behavior.

## Errors and redaction

Record framing, decode, incomplete-tail, body, staging-limit, and internal
progress violations become finite `RecordErrorKind` values. A body failure
after stream opening is deliberately reduced to the record contract rather
than leaking the underlying `ApiError`.

Pagination preparation, controller, cycle, page-limit, item-limit, and
identity-limit failures reduce to `ErrorCategory::Pagination` with focused
`PaginationErrorKind` where available. A page's ordinary authentication,
limiter, hook, timeout, transport, status, or decode failure retains its normal
top-level category while aggregate completion stays correlated.

Identity diagnostics and ordinary error formatting must not reveal private
credential material or retained controller state. Public URL and header values
can still contain identifiers, so extensions and hooks retain their own
logging obligations. The canonical reduction behavior is in
[Errors](../../reference/errors.md).

## Extension boundary

A `RecordDecoder` receives `ResponseHead` and a bounded `BytesMut` staging
buffer. It receives no native response, transport stream, credentials, or
hooks. It must implement exact EOF semantics and obey the net-progress
contract on every return path.

A `PaginationController` receives the decoded page and owns only its
request-local continuation value. It cannot send a page, override policy, or
inspect private request state. Calls from distinct pagination operations may
overlap even though one controller value only moves sequentially.

Downstream buffered response specifications opt into pagination explicitly
with `PageResponseSpec`; implementing `ResponseBodySpec` alone is not enough.
Bytes, discard outputs, byte streams, and record streams do not accidentally
become paginatable.

See [Record decoders](../../extending/record-decoders.md) and
[Pagination controllers](../../extending/pagination.md) for implementation
skeletons.

## Failure modes

Record tests must cover:

- a record split across arbitrary network chunks;
- several records in one chunk;
- consumption followed by `None`;
- `Some` without net consumption and any buffer growth;
- first-excess staging behavior;
- clean EOF, final record at EOF, and incomplete trailing framing;
- upstream body failure and decoder failure after stream opening;
- batching success, partial-batch failure, and drop behavior.

Pagination tests must cover:

- zero/take/hard-cap termination differences;
- short-page termination and controller natural termination;
- repeated canonical identity, including credential changes that must not hide
  a public cycle;
- first-excess page, item, and identity ceilings;
- page failure without partial output;
- authentication recovery and rate-limit policy on later pages;
- direct page completion before aggregate completion;
- cancellation before another page begins.

## Change hazards

Changing record progress checks can introduce a busy loop, retain an excess
chunk, or convert framing behavior into transport-dependent behavior. Review
decoder changes with manually polled ready-empty sources and exact staged-byte
assertions.

Changing pagination binding or identity construction can silently weaken cycle
detection. Review every public shaping component that affects the next URL or
header, but never add credentials to identity. New per-page features must use
the managed executor and preserve child/parent event correlation.

Collection semantics are intentionally all-or-error. Returning partial items
would require a new public contract rather than an implementation-only change.

## Verification

Primary owners are:

- `crates/concord-core/src/records/stream.rs` for progress, staging, EOF,
  batching, and drop semantics;
- `crates/concord-core/src/pagination/tests.rs` for controller binding and
  structural preparation;
- `crates/concord-core/src/pagination/runtime_tests/` for sequencing, limits,
  identity, and completion;
- `crates/concord-core/tests/public_api/pagination_api.rs` and record public
  API tests for stable reachability;
- `crates/concord-extras/src/records/` for CSV and NDJSON downstream decoder
  behavior;
- `crates/concord/tests/generated/pagination.rs` and pagination UI cases for
  generated surface and diagnostics;
- `crates/concord-examples/src/request_response/`,
  `crates/concord-examples/src/pagination/`, and the pagination recipe for
  compiled end-to-end compositions.

After focused owners, run all-feature core, extras, façade, examples, workspace
doctests, and the maintained UI suites from
[Testing strategy](../testing.md).

## Related

- [Record streams](../../features/records.md)
- [Pagination](../../features/pagination.md)
- [Bodies, responses, and finite progress](body-response-progress.md)
- [Managed executor](../architecture/managed-executor.md)
