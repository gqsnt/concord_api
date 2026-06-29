# CSV Records Runtime Contract

## Purpose

CSV is implemented as a `Records<T, F>` format. It is not a new endpoint I/O family and it does not change the current `RecordBody<T>` / `RecordStream<T>` runtime model.

This document records the contract Concord must preserve for CSV runtime support.

## Family Model

CSV is modeled as a record format:

```rust
Records<User, Csv<CsvCommaDelim>>
Records<User, Csv<CsvSemicolonDelim>>
Records<User, Csv<CsvTabDelim>>
```

It is not a top-level endpoint I/O family such as `Csv<T>`.

The existing `Records<T, F>` model remains unchanged.

`CsvCommaDelim`, `CsvSemicolonDelim`, and `CsvTabDelim` are config types used inside `Csv<Cfg>`, not standalone runtime values.

## Runtime Values

Runtime values remain format-free:

```rust
RecordBody<T>
RecordStream<T>
```

The format marker stays in the endpoint type:

```rust
Csv<Cfg>
CsvCommaDelim
CsvSemicolonDelim
CsvTabDelim
```

CSV must not introduce format-specific runtime value types.

## Config Model

CSV uses a config trait like this:

```rust
pub trait CsvConfig {
    const DELIMITER: u8;
    const HAS_HEADERS: bool;
}
```

CSV provides these built-in configs:

```rust
CsvCommaDelim
CsvSemicolonDelim
CsvTabDelim
```

Built-in configs use `HAS_HEADERS = true`. Headerless CSV remains possible through custom `CsvConfig` implementations.

The delimiter must be selected by the config type, not by runtime value.

## Header Behavior

Current encode and decode behavior:

- If `HAS_HEADERS = true`, request encoding writes one header row before the first record.
- If `HAS_HEADERS = true`, response decoding consumes the first row as headers.
- If `HAS_HEADERS = false`, request encoding writes no header row.
- If `HAS_HEADERS = false`, response decoding treats the first row as data.
- Header mismatches must become sanitized codec or record-format errors, not panics.

CSV header semantics are implemented through serde-compatible CSV behavior in the runtime, not through hand-rolled header parsing.

## Empty Rows

Current behavior:

- Empty rows are ignored when the parser classifies them as empty records.
- Rows with the wrong number of fields are errors.
- Whitespace-only unquoted fields should follow the CSV crate's default behavior; do not add an extra trimming layer in the first implementation.

## Quoting and Escaping

CSV quoting and escaping should follow RFC4180-compatible behavior through the `csv` crate in the runtime PR.

Required behavior:

- quoted fields are supported;
- escaped quotes are supported;
- delimiters inside quoted fields are supported;
- CRLF inside quoted fields is supported when the parser can do so safely;
- no hand-rolled CSV parser.

## Line Endings and Final Row

Planned behavior:

- `\n` and `\r\n` are accepted for responses;
- request encoding may use the CSV writer default line terminator unless explicitly configured later;
- a final row without a trailing newline is accepted;
- chunk boundaries must not affect parsing correctness.

## Content Type

CSV uses the ordinary record-format content marker path:

```rust
impl ContentType for Csv<Cfg> {
    const CONTENT_TYPE: &'static str = "text/csv";
}
```

The delimiter must not be encoded as a `Content-Type` parameter initially.
No `header=present` or similar parameter should be emitted in v1.
Future parameters require a separate compatibility review.

## Error Hygiene

CSV errors must be body-free and auth-safe.

They must not include:

- raw row content;
- raw body bytes;
- credentials;
- auth headers;
- payload fragments.

They may include:

- endpoint name;
- method;
- row index if available;
- field-count mismatch category;
- sanitized parser category.

## Streaming and Limits

CSV responses are record streams.

The parser must work across transport chunks and obey the existing record stream/body byte and item limits. CSV must not buffer unbounded responses.

CSV request bodies are stream-like and non-replayable in the same sense as other `Records<T, F>` bodies.

## Retry and Replay

Retry remains a transport/status layer.

Decode failures are not retried.
CSV request bodies must not be automatically replayed after auth refresh or ordinary retry unless a future replayable-body contract is introduced.

## Public Boundary

This is a design contract only.

CSV runtime support is implemented through the existing record runtime and the `Csv<Cfg>` marker/config types. Concord should continue to avoid a separate CSV endpoint family or CSV-specific runtime values.
