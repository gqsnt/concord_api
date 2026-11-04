# Extension implementation decisions

Deviations from, and resolutions of open points in, [API_PLAN.md](API_PLAN.md). Entries are
prefixed by area (`JSONL-`, `CSV-`, `PAG-`, ...).

## JSON Lines / consecutive JSON values (`concord_extras::jsonl`)

**JSONL-01 — `MediaPolicy` lives in `jsonl`.** The plan names `media(MediaPolicy)` for both CSV and
JSONL options without giving the type a home. It is defined in `concord_extras::jsonl` so the
parallel CSV work does not conflict in `stream`; the coordinator may hoist it to `stream` if CSV
adopts the same shape. Shape: a `Copy` value with `const` constructors `standard()`,
`standard_or(&'static [&'static str])`, `only(&'static [&'static str])` and `any()`. Entries are
validated as `type/subtype` RFC 9110 tokens (no parameters, no `*`) like
`HeaderValue::from_static`: a compile error in a `const`, a panic otherwise. This keeps
`JsonLinesReader::new` infallible, as planned, with no invalid configuration left to report.

**JSONL-02 — Documented media types.** `JsonLines`: `application/jsonl`, `application/jsonlines`,
`application/x-jsonlines`. `Ndjson`: `application/x-ndjson`, `application/ndjson`. The families do
not accept each other (a provider's actual type is accepted deliberately with `MediaPolicy`).
`JsonValuesReader`: `application/json` and `application/*+json`, as `Json`; parameters such as
Kubernetes' `stream=watch` are ignored, `application/vnd.kubernetes.protobuf;stream=watch` is
refused. All non-`any` policies require exactly one well-formed `Content-Type` (not a media range)
whose charset, if any, is `utf-8`/`us-ascii` declared once; a missing field is refused. Request
`Content-Type` and `Accept` suggestion: `application/jsonl` / `application/x-ndjson`.

**JSONL-03 — Finite `Decode` = preset, exactly.** `Decode<Vec<T>>` for `JsonLines`/`Ndjson` runs the
reader's framing code over the collected body with `JsonLinesOptions::json_lines()`/`ndjson()`,
including the default 16 MiB line limit (`DEFAULT_MAX_RECORD_BYTES`, also the default
`max_value_bytes`). One rule, one implementation; a caller needing another limit on a collected
body reads it with a configured `JsonLinesReader` over `Chunks`.

**JSONL-04 — What `RecordErrors::Yield` recovers.** Recoverable, reported as an `Err` item for one
line delimited on both sides (LF or the true end): a refused blank line, invalid UTF-8, invalid
JSON (including a truncated final line) and a `Decode` mismatch. Always terminal: a line over
`max_record_bytes` (the reader stops before the line's end, so it is not isolated and is not
skipped), a refused byte order mark, transport errors and totals. `JsonValues` has no yield mode:
every error is terminal.

**JSONL-05 — UTF-8 validated per line before serde.** serde_json does not validate the bytes of a
string it skips (`IgnoredAny`, unknown fields), so the profile's UTF-8 rule is enforced with
`std::str::from_utf8` on every line/value, whatever `T` reads. Cost: one extra linear pass per
record.

**JSONL-06 — Limits count content.** `max_record_bytes` counts a line's bytes excluding its LF or
CRLF; the line buffer may hold one extra pending CR (it may start a CRLF), so its length and
allocation stay within `max + 1`. A trailing CR at the true end (no LF) is content.
`max_value_bytes` counts a value from its first to its last byte, whitespace excluded; the buffer
stays within `max`. Both are enforced before the buffer grows; a line/value wholly inside one chunk
is checked the same way (chunk-independent outcome). The optional byte total is the foundation's:
checked per received chunk.

**JSONL-07 — Writer.** Compact serde_json, LF after every record including the last, no BOM; the
same bytes for both families. A record whose JSON contains an LF (possible only through
`serde_json::value::RawValue`) is refused, since it would split the record. `JsonLinesWriter` uses
only the options' family (its `Content-Type`); reader-only options do not apply. Added
`JsonLinesWriter::content_type()`; `to_vec` takes any `IntoIterator<Item = T: Serialize>`.

**JSONL-08 — Consecutive-value boundaries.** A number/literal ends at JSON whitespace, at the start
of `{`, `[` or `"`, or at the true end of input — serde_json `StreamDeserializer`'s documented rule
(`12{"a":1}` is two values, `truefalse` is one malformed token). `,`, `:`, `}` and `]` outside a
container belong to the token and fail when it is parsed (`Format`); so does a byte order mark.
Input ending inside an object, array or string is `Format` ("the input ends inside a JSON value");
a literal cut at the true end is `Format` ("a JSON value is incomplete").

**JSONL-09 — Work per poll.** `next()` yields to the executor once after consuming `SKIP_BUDGET`
(64 KiB) without producing a record — counting every consumed byte, not only skipped ones — so an
always-ready source cannot make one poll unbounded. The value scanner also stops after
`SKIP_BUDGET` whitespace bytes inside one chunk. Scanning one line/value is bounded by
min(chunk, limit).

**JSONL-10 — Resource accessors (coordinator request).** `JsonLinesRecords` and `JsonValues` have
`retained_bytes()` (length of the library-owned buffer holding a line/value that started in an
earlier chunk; `0` between records; excludes the current chunk's unconsumed remainder and returned
records), `peak_retained_bytes()` (its maximum over the reader's life, including a complete line
while parsed from the buffer), `pending_input_bytes()` (the current chunk's unconsumed remainder,
the source's memory, shared not copied) and `into_source()` (documented as partially consumed,
never unread). A buffered line is released right after it is parsed.

**JSONL-11 — Foundation touch points.** `media.rs` only: the dead-code allowance of `Media::params`
now applies whenever `csv` is off (it is read only by CSV), and `check_charset`/`essence` are
compiled for `jsonl` too. With `--all-features` the `params` field is still unread while `csv` is a
stub on this branch (the same warning exists before this change); it disappears once CSV reads it.
No change to `Driver`, `ReadError`, `OpenError` or `RecordBody`.

**JSONL-12 — Test-only additions.** A dev-dependency entry for the existing `serde_json` with its
`raw_value` feature, to test the writer's LF guard (no new crate). The seed directories carry
`.gitattributes` `* -text`: the repository's `* text=auto` would otherwise normalize the CRLF seed.

**JSONL-13 — Observed upload failure timing (hyper 1.x through reqwest 0.13, loopback).** A source
or serialization failure in the body's first poll fails `send` before anything reaches the
receiver (it observed 0 bytes, not even the head: hyper writes the head with the first body
frame). A failure after a frame was handed over leaves the receiver with the complete records of
the earlier frames and no terminating chunk; `send` returns a reqwest error (`is_request`) whose
source chain contains the `UploadError` (label, records serialized). Nothing is replayed.
