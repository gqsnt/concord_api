# Extension implementation decisions

Deviations from, and refinements of, [API_PLAN.md](API_PLAN.md), with the reason for each. Entries
are prefixed by area (`CSV-`, ...). Semantics come from the handoff contracts (`CSV.md`,
`STREAMING.md`).

## CSV (`concord_extras::csv`)

**CSV-01 `CsvWriter::upload` returns `Result<Upload, CodecError>`** (plan: `-> Upload`). A
`StreamRows` payload may carry explicit columns that are invalid (empty, repeated) or conflict with
the configured columns; CSV.md §5 forbids silently choosing one. Both are known before any polling,
so they are refused at preparation instead of failing the body later.

**CSV-02 Public items beyond the plan.** `CsvContext` (the `Decode::Context` must be a public type;
opaque, with `CsvContext::of::<H, D, P>()` for direct `decode_body` calls); `Csv::<H, D, P>::CONTENT_TYPE`;
`CsvReader::{options, check_head, open_raw_for, read_raw}`; `CsvRecords::{operation, into_source,
retained_bytes, peak_retained_bytes, pending_input_bytes}` (the last three on coordinator request,
for the stress evidence); `CsvWriter::{options, content_type}`; `CsvOptions::{with_profile,
header_mode, delimiter, profile}`; the value enums `LineEnding`, `RecordTerminator`, `BomPolicy`,
`Trim`, `QuoteStyle`, `MediaPolicy`; `DEFAULT_MAX_{RECORD_BYTES,HEADER_BYTES,FIELDS}`;
`Encode<RawTable>` (the raw write route, symmetric with `Decode<RawTable>`); `RawRecord::new`,
`RawRecord: Serialize` (a sequence of byte fields, so raw records can be written through
`Rows`/`StreamRows`). All value enums are `#[non_exhaustive]`.

**CSV-03 Builder spellings.** `quoting(bool)` is the reader's quote recognition (csv-core's);
writers quote per `quote_style(QuoteStyle)`. The width policy is `flexible(bool)`.
`max_total_bytes(u64)` and `max_records(u64)` set an optional total (no limit by default).
`escape(Option<u8>)`, `comment(Option<u8>)` follow csv-core. `double_quote(false)` requires an escape
byte (otherwise a quote inside a quoted field has no representation).

**CSV-04 One meaning for the profile's `columns`: the table's columns.** Writers with a header emit
them; named rows must emit exactly these fields in this order (never reordered); positional rows
are labelled by them; a payload's different list is refused. Readers with a header require the wire
header to be exactly these names, in order (a `Format` error while reading, CSV.md §3 "validate
those names without replacing them"); readers without a header use them as the names of the
positions (with `ReadMapping::Default`) and as the table width. `ReadMapping::Columns` with a header
is a `CsvConfigError` (ambiguous, CSV.md §3); with a different profile list, also. This also covers
the extension example's `id,name` rule without a separate validation option.

**CSV-05 Head checks.** The `header=` contradiction is derived from `Content-Type`, so it is in
`check_content_type` (the core contract puts every Content-Type-derived check there and only
those); `IgnoreContentType<Csv<..>>` therefore skips it, and nothing else: dialect validation is in
`context`, table rules while parsing (tested). A repeated `header` parameter and a value other than
`present`/`absent` (case-insensitive) are refused. `charset=us-ascii` is accepted, as by `Json` and
`Form`. `MediaPolicy::{TextCsv, TextCsvOrMissing, Unchecked}` is the reader's (and the finite
decoder's) profile-level relaxation.

**CSV-06 What the limits count.** Readers: the unescaped field bytes of one record (header and data
records have separate limits) and its field count. The owned buffers grow by doubling up to the
limit (`reserve_exact`), and at the limit `csv-core` writes into a one-element probe: anything
written there is the byte or field end past the limit, reported as a `Limit` error. The buffers
therefore never exceed `max(max_header_bytes, max_record_bytes)` bytes plus `max_fields` offsets,
and a record of exactly the limit is accepted (tested at every split). Writers: the encoded record
without its terminator, which is never smaller than its field bytes, so whatever a writer accepts is
within a reader's limit of the same value. One `next()` poll parses at most `SKIP_BUDGET` input
bytes between yields, whether they are skipped (comments, blank lines) or part of a large record.

**CSV-07 BOM handling is ours.** `csv-core` strips a BOM itself, but only when its first call
receives all three bytes (split-dependent). The table detects the BOM across chunk boundaries per
`BomPolicy` (strip one / reject / preserve; a partial BOM is replayed as data) and gives `csv-core`
at most one byte on its first call, which disables its own stripping.

**CSV-08 Workaround for a `csv-core` 0.1.13 DFA divergence.** With a comment byte configured, the
DFA returns a record of one empty field for a final comment line that has no terminator
(`a,b\n# trailer`); the NFA returns nothing (reproduced with a scratch program against csv-core
directly; a genuine final `""` is one empty field on both). The table tracks physical lines when a
comment byte is set and drops only that record. Regression test:
`frame::tests::an_unterminated_final_comment_line_is_not_a_record` (fails without the workaround).
Worth reporting upstream.

**CSV-09 Typed rows are UTF-8 as a whole.** A typed record goes through `StringRecord`, so invalid
UTF-8 anywhere in it is a `Format` error, also in a field the row type ignores; raw records keep
bytes. Named mapping also requires a UTF-8 header without empty or repeated names; positional and
raw reading keep such headers.

**CSV-10 Writer schema capture.** The capture serializer follows `csv`'s header rules (named fields
hold one scalar; scalars outside structs make a positional row; structs inside tuples/sequences
concatenate names) and refuses maps, flattened structs and tuple/struct variants. It applies to
`NoHeader` too (schema consistency), so a named row with a nested container is refused even
though `csv` would flatten it positionally. An inferred header with repeated names (two structs of
the same fields in a tuple) is refused. A row's own `Serialize` failure keeps its message only as
the unredacted source.

**CSV-11 Observed late-failure timing of lazy uploads.** Preparation polls the source zero times
(tested). A row/schema/source failure fails the transfer: with a source that is pending between
rows (paced test), the server holds exactly the rows sent before the failure and never receives the
last chunk. With an always-ready source, hyper keeps polling the ready body into its write queue and
aborts the connection on the body error before flushing it: the server saw the request head and no
body byte (observed on loopback; printed by the test, not asserted, as it is hyper's buffering). The
client's error is a reqwest request/body error with the `UploadError` in its source chain. An empty
header-required source without columns fails at the end of the body (late), not at preparation.

**CSV-12 Diagnostics.** `RawRecord`'s `Debug` shows the field count only; `CsvRecords`'s shows
operation, position and state. All CSV labels are static; positions are `record` (1-based data
record), `field` (1-based), `fields`/`expected` (widths) and `limit`.

**CSV-13 Foundation change.** `stream::Driver::take` is now `#[allow(dead_code)]`: the CSV reader
copies fields into owned records and never takes chunk slices, so with `csv` alone (or `csv` +
the `jsonl` stub) it was an unused-code warning under `-D warnings`. Observation, not changed:
`stream::FromIter` requires `I: Unpin`, so `from_iter(vec_of_results)` needs `T: Unpin`; an
`impl<I> Unpin for FromIter<I>` would lift that (the iterator is never pinned structurally).

**CSV-14 Extension example migration (CSV.md item 17).** `examples/extension` now profiles the codec
(`CatalogExport`: `comment(Some(b'#'))`, `columns(["id", "name"])`) and its `Csv` is a type alias of
`Csv<HasHeader, Comma, CatalogExport>` (previously a unit struct; still named the same way by
`Decoder::new::<Csv>()`). Intentional changes, all tested and listed in its README: quoted fields,
a leading BOM, `\r` terminators and `charset=us-ascii` are now accepted; `header=absent` is refused
before the body; error labels and positions are the codec's (record/field instead of line/byte).
A new `export_rows` shows the incremental reader with the same profile.

**CSV-15 Scope notes.** CSV.md item 14's "two parallel page jobs hold their permits through body
completion" belongs to the pagination crate. Item 16's Salesforce adapter is left to the SDK author;
the profile API (static types and `CsvOptions::new(HeaderMode, delimiter)` with the same setters)
covers its header/delimiter/line-ending choices.
