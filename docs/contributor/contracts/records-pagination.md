# Record delivery and pagination contract

Record delivery turns one accepted response body into typed logical records.
Pagination executes a finite sequence of complete page requests and
accumulates each page's iterable output. Buffered record delivery can serve as
a page response; streaming record delivery cannot.

## Purpose

The contract prevents three architectural mistakes:

- treating transport chunks as record boundaries;
- treating a large source chunk as one staged logical record;
- treating pagination as one transport request and bypassing per-page
  authentication, limiter, hook, deadline, and completion policy.

One private record driver owns framing progress for both buffered collection
and `RecordStream`. Pagination wraps the managed executor and invokes it once
per page; it does not share the record driver's loop.

## Public model

The record surface consists of:

- `RecordDecoder`, a request-local incremental framing and decode contract;
- `BufferedRecords<D>`, a buffered `ResponseBodySpec` whose output is
  `Vec<D::Record>`;
- `Records<D>`, a streaming `ResponseBodySpec`;
- `RecordStream<D>`, yielding `Result<D::Record, RecordError>`;
- `RecordBatches<D>`, an optional positive-size stream adapter.

`BufferedRecords<D>` implements `PageResponseSpec`. There is deliberately no
blanket `ResponseBodySpec` for every `RecordDecoder`: the wrapper expresses
the lifecycle choice.

First-party `Csv<T, H, Delimiter>` and `NdJson<T>` remain
`RecordDecoder`s. Their direct `ResponseBodySpec` implementations delegate to
`BufferedRecords<Self>`, return `Vec<T>`, and implement `PageResponseSpec`.
Wrapping either in `Records<D>` selects streaming without a separate framing
engine.

The pagination surface consists of:

- `PaginationController<Page>`, request-local continuation state;
- `PageRequestBodySpec`, trusted opt-in for stable independently preparable
  page bodies;
- `PageResponseSpec`, explicit opt-in for complete iterable page outputs;
- `PageLimit::Take` and `PageLimit::HardCap`;
- generated `.paginate(...)`, returning one collected `Vec<Item>`.

## Shared record driver

The private `RecordDriver<D>` owns:

- one decoder started for the accepted response;
- decoder-visible staging;
- pending source bytes that have not entered staging;
- EOF and terminal state;
- net-progress, staging-limit, and invariant checks.

Pending source and staging are intentionally distinct. The driver feeds only
enough pending input to let the decoder make progress. A transport or
aggregate chunk larger than the staging limit can therefore contain many
small records and succeed. If one logical record cannot progress at the exact
staging ceiling, the first excess staging byte is rejected before retention.

For each callback:

- staging length must not grow;
- `Some(record)` requires a strict net decrease;
- `None` may consume bytes;
- unchanged `None` asks for more pending/source input;
- EOF uses `decode_eof` under the same rules.

Some-without-consumption and decoder growth are invariant failures, not
ordinary format errors. Network chunks never define record boundaries.

## Buffered record flow

After status acceptance:

1. buffered known-length preflight and `BufferedRecords<D>::validate` run
   before body polling;
2. core acquires the complete aggregate-bounded body;
3. `D::start` creates one request-local decoder in `from_body`;
4. complete bounded bytes become pending driver input;
5. the driver collects records in order until clean EOF;
6. the complete vector is returned, or all accumulated records are dropped on
   error.

This work occurs during `Buffered` conversion under the operation deadline.
Completion reports `Success` on a complete vector, not `StreamOpened`.
`.response()` retains accepted metadata. `.response_bytes()` preserves status
acceptance and `Accept`, but bypasses record validation, startup, and decoding.

## Streaming record flow

After status acceptance:

1. streaming known-length preflight and `Records<D>::validate` run before
   source polling;
2. core creates an unpolled bounded `ByteStream`;
3. `D::start` creates one decoder during `from_body`;
4. core opens `RecordStream<D>` over that source plus the same driver;
5. later caller polls alternate driver progress with bounded source polling;
6. source EOF invokes the shared EOF path.

Opening completes the endpoint with `StreamOpened`. Later source or decoder
errors are `RecordError` items and emit no second completion. Dropping the
record stream drops its underlying byte stream.

Batching does not change the primitive contract. It retains at most its
positive batch size, drops a partial batch on underlying failure, and cannot
bound records retained after yield.

## Pagination flow

The aggregate pagination future is inert until first poll. For each page it:

1. validates controller state;
2. clones the original typed endpoint and its trusted stable body input;
3. binds only declared pagination fields;
4. prepares the bounded public head and canonical credential-sanitized
   identity;
5. rejects repeated or over-limit identity;
6. executes the page through the ordinary managed executor;
7. completes the page operation;
8. accumulates decoded iterable items under the item ceiling;
9. applies short-page and requested-limit rules;
10. calls `next` only if another page is permitted.

After the aggregate result is fixed, the pagination completion hook runs
outside page deadlines. Controller callbacks also run synchronously outside
the per-page deadline.

`BufferedRecords<D>`, direct CSV, and direct NDJSON are eligible page outputs.
`Records<D>`, byte streams, discard responses, and response-bytes acquisition
are not. Pagination requires a complete decoded page before state transition.

## Architectural invariants

- Buffered collection and streaming use one record progress engine.
- Record staging has its own bound in addition to the applicable aggregate
  buffered/streamed response bound.
- Buffered collection is complete all-or-error; no partial vector escapes.
- Streaming errors after opening are items; dropping stops source polling.
- Decoder state, pending bytes, and staging bytes are absent from automatic
  formatting.
- Pagination pages execute sequentially through the ordinary executor.
- Cycle identity includes endpoint, method, logical URL, and public headers,
  never request body bytes or private credentials.
- Pagination bindings cannot target the request-body field.
- A policy retry reconstructs a complete attempt from the prepared page body
  rather than re-running page input encoding.
- Every retained identity and item is charged before first excess.
- Page and aggregate operation identities and completion events remain
  correlated.

## Bounds and trusted work

Buffered records obey the configured buffered-response byte limit; streaming
records obey the stream-response byte and empty-chunk limits. Every decoder
adds a positive staging limit. These limits do not measure decoded object
graphs, decoder side tables, callback CPU, consumer queues, or application
retention.

Pagination has independent page, item, and retained-identity byte ceilings.
`PageLimit::Take(n)` succeeds with no more than `n` pages without asking for
another continuation after page `n`. `HardCap(n)` requires natural
termination within the cap.

Controller and decoder callbacks are synchronous trusted work. They must
perform no external I/O and must bound CPU and allocation.

## Errors and redaction

For buffered records:

- aggregate body read and body-limit failures remain `ResponseBody`;
- media validation is `ResponseContract`;
- framing, staging, and ordinary decode errors are `ResponseDecoding`;
- driver invariant failures are `Internal`.

For an opened stream, corresponding later failures are finite
`RecordErrorKind` values. Neither route retains parser source errors or
record bytes.

Pagination structural failures reduce to `Pagination`; an ordinary page
authentication, limiter, hook, timeout, transport, status, or response error
retains its category. Later-page failure never exposes the accumulated prefix.

## Extension boundary

A `RecordDecoder` receives `ResponseHead` and a bounded `BytesMut` staging
buffer, never native transport or credentials. It owns exact framing and EOF
semantics.

A `PaginationController` receives the decoded page and its own continuation
state. It cannot send requests or inspect private request state. Downstream
bodies opt into pagination only through `PageRequestBodySpec`; cloneability
alone is insufficient. Downstream responses opt in through
`PageResponseSpec`; `ResponseBodySpec` alone is insufficient.

## Failure modes and verification

Record proofs cover complete and one-byte input, every framing boundary, a
source chunk larger than staging with many small records, exact staging and
first excess, consuming `None`, invariant failures, multi-record EOF,
incomplete tails, source failure/drop, buffered/streaming parity, and
all-or-error collection.

Pagination proofs cover zero/take/hard-cap behavior, natural and short-page
termination, cycle identity, first-excess page/item/identity bounds, stable
page bodies and retry reuse, buffered-record/CSV/NDJSON page outputs, rejected
streaming page outputs, later-page failure, authentication recovery, limiter
policy, and page-before-aggregate completion.

Primary owners are:

- `crates/concord-core/src/records/driver.rs` for progress, pending/staging,
  EOF, and collection;
- `crates/concord-core/src/records/stream.rs` and `buffered.rs` for delivery
  adapters;
- `crates/concord-core/src/pagination/` for sequencing and limits;
- `crates/concord-extras/src/records/` for CSV and NDJSON;
- façade generated/runtime/public/UI tests for response and pagination
  composition.

## Related

- [Record delivery](../../features/records.md)
- [Pagination](../../features/pagination.md)
- [Bodies, responses, and finite progress](body-response-progress.md)
- [Managed executor](../architecture/managed-executor.md)
