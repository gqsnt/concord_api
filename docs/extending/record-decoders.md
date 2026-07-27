# Record decoders

Implement a record decoder when one accepted response contains a sequence of
typed logical records and CSV or NDJSON does not match its framing contract.
The same decoder can be collected completely or delivered incrementally. It
works over bounded semantic bytes and never sees a native response or assumes
that transport chunks align with records.

Enable the `records` feature and implement the exact Rustdoc item
`concord::records::RecordDecoder`.

## Complete example

This delimiter-based decoder is compiled in `concord-examples`:

```rust,ignore
{{#include ../../crates/concord-examples/src/request_response/records.rs:record_decoder_extension}}
```

The generated contract selects both delivery forms:

```rust,ignore
{{#include ../../crates/concord-examples/src/request_response/records.rs:records_contract}}
```

`BufferedRecords<PipeRecords>` returns `Vec<String>` through the ordinary
buffered lifecycle. `Records<PipeRecords>` returns a
`RecordStream<PipeRecords>`. No blanket `ResponseBodySpec` exists for every
`RecordDecoder`; the wrapper records the intended lifecycle explicitly.

## Validation and start timing

For either delivery form, Concord first resolves an accepted status and
known-length preflight, then calls `RecordDecoder::validate` with a borrowed
`ResponseHead` before polling accepted body bytes. It calls
`RecordDecoder::start` exactly once later, when the mode-specific
`from_body` conversion begins, and gives that request-local decoder to the
shared private record driver.

`accept` supplies a default `Accept` value only when request shaping did not
already set one. `validate` should enforce the documented media contract.
`start` may derive small bounded state from the accepted head, but should not
retain arbitrary headers or URLs. The head contains actual accepted headers
and a credential-sanitized logical URL; it is not automatically free of
sensitive application data.

For `BufferedRecords<D>`, validation precedes aggregate collection. The body
has already been collected under the buffered-response limit when
`from_body` calls `start`; complete all-or-error record driving then happens
before the request completes and under its deadline. Direct
`Csv<T, H, Delimiter>` and `NdJson<T>` response specifications delegate to
this path.

For `Records<D>`, validation precedes creation of the unpolled bounded byte
stream, then `from_body` calls `start` before the streaming output is handed to
the caller. Later driver work occurs while the caller polls the
`RecordStream`, outside the endpoint deadline.

## Decode by logical framing

`decode(&mut self, &mut BytesMut)` may return at most one record. Concord calls
it repeatedly while complete records can be consumed. Decoder staging may
contain part of one record or multiple records regardless of the source chunk
pattern.

Core checks net staging progress across every callback:

- the decoder must never increase staging length;
- returning `Some(record)` requires a strict net decrease;
- returning `None` may consume bytes;
- returning `None` with unchanged length asks the driver for more source
  input.

An implementation must not emit repeated values without consuming staged
bytes. If one framed input expands into several application values, return
that collection as one record or choose another response design.

Use `split_to`, `advance`, or another explicit buffer operation only after a
complete frame has been identified. Do not scan or clone an ever-growing
prefix without a format-specific bound.

## Pending input and staging are distinct

`max_buffered_bytes` returns a positive `NonZeroUsize` and bounds undecoded
logical staging, not the size of one transport or aggregate source chunk.
The private driver retains pending source separately and feeds staging only
until the decoder can make progress. A large source chunk containing many
small records therefore succeeds when every logical record fits.

If one logical record first requires more than the staging ceiling, the
driver fails before retaining the first excess staging byte. This staging
bound is independent of the aggregate buffered-response or stream-response
byte limit.

Choose the staging limit from the largest valid encoded record plus bounded
framing overhead. It does not limit:

- heap allocation inside a decoded record;
- decoder-owned side tables;
- records retained by buffered collection or the caller;
- downstream batches or queues;
- CPU spent searching current staging.

Bound each separately. Decoder callbacks are synchronous and cannot be
interrupted; Concord can detect contract violations after return, not preempt
unbounded work.

## EOF is a separate contract

`decode_eof` decides what remaining staged bytes mean after source EOF. A
format may accept a final unterminated record, require an explicit delimiter,
or reject the remainder as `IncompleteRecord`.

Return at most one final record per call under the same progress rules.
Return `Ok(None)` only when no staged record remains. The `PipeRecords`
example accepts a nonempty final value without `|`; a stricter protocol should
reject it.

## Errors and redaction

Use `RecordError::new(RecordErrorKind::...)` for structural failures. Use
`with_safe_message` only for constant, reviewed text. Parser diagnostics often
contain raw fields, filenames, JSON fragments, or CSV rows; drop them rather
than attaching them as sources.

In buffered delivery, aggregate body failures remain `ResponseBody`; media
failures become `ResponseContract`; framing, staging, and ordinary decoder
failures become `ResponseDecoding`; driver invariant violations become
`Internal`. The operation returns either the complete vector or an error.

After a `RecordStream` has opened, source and decode failures are typed
`RecordError` items. A successfully decoded prefix is not an automatically
successful complete result. Automatic formatting of the driver and stream
does not expose pending, staging, or decoded data.

## Verification

Drive both wrappers through generated or runtime endpoints and deterministic
local HTTP. For each accepted format, test:

- the whole body in one chunk, every byte separately, and boundaries around
  delimiters, newlines, quoting, and escapes;
- a source chunk larger than staging containing many small records;
- identical buffered and streaming values for every chunk pattern;
- empty body, repeated delimiters, CRLF/LF where relevant, and EOF without a
  final delimiter;
- exact staging maximum and the first logical-record excess;
- invalid encoding, malformed frames, and parser sentinels absent from errors;
- `Some` without consumption and decoder growth, both rejected as internal
  invariant failures;
- buffered all-or-error behavior and dropping a returned stream.

Direct callback tests are useful, but do not prove response media validation,
status acceptance, byte limits, lifecycle completion, or pagination.

## Related

- [Record delivery](../features/records.md)
- [Collect or stream CSV and NDJSON records](../recipes/stream-records.md)
- [Request and response body specifications](body-specifications.md)
- [Hooks, trusted work, and bounds](hooks-and-trusted-work.md)
- [Finite behavior](../concepts/finite-behavior.md)
- Rustdoc: `concord::records::RecordDecoder`
