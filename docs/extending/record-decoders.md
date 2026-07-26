# Record decoders

Implement a record decoder when one successful response should yield typed
records incrementally and CSV or NDJSON does not match the framing contract.
The decoder works over Concord's bounded semantic byte stream; it never sees a
network response or assumes that transport chunks align with records.

Enable the `records` feature and implement the exact Rustdoc item
`concord::records::RecordDecoder`.

## Complete example

This delimiter-based decoder is compiled in `concord-examples`:

```rust,ignore
{{#include ../../crates/concord-examples/src/request_response/records.rs:record_decoder_extension}}
```

The generated endpoint selects it through `Records<D>`:

```rust,ignore
{{#include ../../crates/concord-examples/src/request_response/records.rs:records_contract}}
```

`PipeRecords` is deliberately small, but it demonstrates the required
staging, progress, EOF, and category-only error behavior.

## Opening the stream

Concord handles successful status and streaming known-length preflight before
the decoder runs. It then:

1. calls `RecordDecoder::validate` with a borrowed `ResponseHead`;
2. calls `RecordDecoder::start` once to create request-local decoder state;
3. returns a `RecordStream<D>` to the caller;
4. feeds nonempty bounded body bytes into the staging buffer as that stream is
   polled.

`accept` provides a default `Accept` value only when request shaping did not
already set one. `validate` should check the documented media contract before
body polling. `start` may derive small bounded state from the response head,
but should not retain arbitrary headers or URLs.

The head contains the actual successful headers and a credential-sanitized
logical URL. It is not automatically free of sensitive application data.

## Decode by logical framing

`decode(&mut self, &mut BytesMut)` may return at most one record. It is called
repeatedly while complete records can be consumed before Concord polls another
body chunk. The staging buffer may contain part of a record, one record, or
several records regardless of the transport chunk pattern.

Core checks net buffer progress across every callback:

- the decoder must never increase the staging buffer length;
- returning `Some(record)` requires a strict net decrease;
- returning `None` may consume bytes;
- returning `None` with unchanged length tells Concord that more body data is
  needed.

An implementation must not emit repeated values without consuming staged
bytes. If one framed input expands into several application values, return
that collection as one record or choose another response design.

Use `split_to`, `advance`, or another explicit buffer operation only after a
complete frame has been identified. Do not scan or clone an ever-growing
prefix without a format-specific bound.

## EOF is a separate contract

`decode_eof` decides what remaining bytes mean after the body ends. A format
may accept a final unterminated record, require an explicit delimiter, or
reject the remainder as `IncompleteRecord`.

Return at most one final record per call under the same progress rules. Return
`Ok(None)` only when no staged record remains. The `PipeRecords` example
accepts a nonempty final value without `|`; a stricter protocol should reject
it.

## Independent staging limit

`max_buffered_bytes` returns a positive `NonZeroUsize`. Concord enforces that
ceiling on undecoded staged bytes independently of the response-wide byte
stream limit.

Choose it from the largest valid encoded record plus bounded framing overhead.
It does not limit:

- heap allocation inside a decoded record;
- decoder-owned side tables;
- records retained by the caller;
- batches or downstream queues;
- CPU spent searching the current buffer.

Bound each of those separately. Decoder callbacks are synchronous and cannot
be interrupted; Concord can detect contract violations after they return, not
preempt unbounded work.

## Redacted errors

Use `RecordError::new(RecordErrorKind::...)` for structural failures. Use
`with_safe_message` only for constant, reviewed text. Parser diagnostics often
contain raw fields, filenames, JSON fragments, or CSV rows; drop them rather
than attaching them as sources.

The exact error Rustdoc items are `concord::records::RecordError` and
`concord::records::RecordErrorKind`. Errors encountered after opening arrive
as stream items. A successfully decoded prefix is not an automatically
successful partial result.

## Verification

Drive the real generated endpoint through deterministic local HTTP. For each
accepted format, test:

- the whole body in one chunk and every byte in its own chunk;
- delimiters and quoted or escaped delimiters across chunk boundaries;
- empty body, repeated delimiters, CRLF/LF where relevant, and EOF without a
  final delimiter;
- exact staging maximum and first excess;
- invalid encoding, malformed frames, and parser sentinels absent from errors;
- a decoder that returns `Some` without progress and one that grows the buffer,
  both rejected by core;
- dropping the returned stream before completion.

The canonical record example's local tests already fragment CSV and NDJSON
across arbitrary boundaries. Add equivalent cases for a new decoder instead
of testing its callback in isolation only.

## Related

- [Record streams](../features/records.md)
- [Stream CSV or NDJSON records](../recipes/stream-records.md)
- [Request and response body specifications](body-specifications.md)
- [Hooks, trusted work, and bounds](hooks-and-trusted-work.md)
- [Finite behavior](../concepts/finite-behavior.md)
- Rustdoc: `concord::records::RecordDecoder`
