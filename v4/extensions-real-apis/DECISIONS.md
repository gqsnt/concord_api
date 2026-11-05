# Extension decisions

Short records of each deviation from, or refinement of, [API_PLAN.md](API_PLAN.md), with its
reason. Implementers append entries under their own prefix (`PAG-`, `CSV-`, `JSONL-`, `SDK-`,
`COORD-`).

## Pagination (`crates/concord-pagination`)

- **PAG-01 — Budgets are mandatory everywhere.** `sequential(from, budget, fetch)` as planned;
  the independent `Options` and the job side of `JobOptions` are built with `Options::new(Budget)`,
  and `JobOptions::new(discovery: Budget, jobs: Options)`. `Budget` wraps `NonZeroUsize`. Reason:
  contract §3 (a page is charged at admission; a huge advertised total or an endless lazy plan must
  not dispatch unboundedly; a repeating manifest chain must end). A budget is a count of logical
  factory calls, not HTTP attempts (documented).

- **PAG-02 — `first_page_then`'s plan returns the page error type, not a crate `PlanError`.**
  Signature: `plan: FnOnce(&R) -> Result<I, E>` with `I: IntoIterator<Item = K>`. Reason: the
  adapter owns envelope validation and its typed protocol error (P05: an inconsistent envelope is a
  typed page failure, not an empty page); a crate `PlanError` would stringify or erase it. A refused
  plan turns the first page into a failure (`Keyed<K, E>`, not emitted as a success), leaves no
  remaining plan, and ends with `Outcome::DiscoveryFailed`, exactly like a failed first page.

- **PAG-03 — `manifest_jobs(from, options, discover, job)` and `Manifest = Page`.** Added a
  `from: Resume<MK>` start so discovery is resumable from `DiscoveryReport::resume`. `Manifest<D,
  MK>` is a type alias of `Page<D, MK>` (descriptors plus the next manifest key), avoiding a second
  page type. Remaining descriptors from a report are rerun with `independent` (documented on
  `JobsReport::into_rerun`) rather than a separate injection API.

- **PAG-04 — Outcome shapes returned by `next`.** `Sequential::next -> Option<Result<Page<T, K, M>,
  E>>`; `Items::next -> Option<Result<T, E>>`; `Independent::next -> Option<Result<Keyed<K, R>,
  Keyed<K, E>>>`; `Jobs::next -> Option<Result<Keyed<D, R>, JobsError<D, EJ, ED>>>` with
  `JobsError::{Job(Keyed<D, EJ>), Discovery(ED)}`. `Keyed { index, key, value }` carries the plan
  (or discovery) position so ordered combination and sparse progress do not depend on key order.
  Errors are moved to the caller by `next`; reports keep the failed *identity* (keys), plus the
  errors that were never emitted (`unemitted_errors`). Nothing is stringified.

- **PAG-05 — One shared `Outcome` enum.** `Exhausted`, `ExhaustedWithFailures`, `Bounded`,
  `Failed`, `FailureLimit`, `Stopped`, `DiscoveryFailed`, `DiscoveryBounded` (`#[non_exhaustive]`);
  each report documents which it can produce. Reason: the contract's required meanings (provider
  exhaustion, configured bound, observed failure, user interruption, incomplete discovery) with
  "a successful subset is never a complete success" made explicit by `ExhaustedWithFailures` and
  `is_success()`.

- **PAG-06 — Failure history is library-owned and bounded.** A traversal retains the key of every
  failed page/job until it ends (successes are not remembered). Bound: one key with `Stop`; with
  `Continue`, `Options::failure_limit(n)` (the failure reaching the limit stops admission with
  `Outcome::FailureLimit`), otherwise the budget. Reason: the contract requires the resume report to
  name failed pages independently of the emission cursor; a caller-owned-only ledger could not.
  Covered by the long alternating success/failure test.

- **PAG-07 — Restart description for independent work.** `Checkpoint { admitted, unfinished }`
  (snapshot) and reports list, in plan order, every admitted key without a successful emitted
  outcome with its state (`Unfinished::{InFlight, Retained, Failed, Cancelled}`), plus the
  unadmitted `rest` of the plan (or `queued` descriptors and `discovery.resume` for jobs).
  `restart()` is the first unfinished key (the ordered restart point); `failed()` lists failures
  separately; `retained` hands back completed-but-unemitted results so finished sink work can be
  reused; `into_rerun()` gives the keys still to run.

- **PAG-08 — Discovery concurrency and backlog.** One manifest call at a time in its own slot beside
  the job window (`window + 1` owned futures at most). A manifest call is admitted only while queued
  descriptors are below `JobOptions::backlog` (default: the job window) and do not already cover the
  remaining job budget; a manifest's descriptors are accepted whole, so the backlog never exceeds
  `backlog - 1 +` one manifest. Reason: smallest overlap that keeps all three quantities bounded and
  separately testable.

- **PAG-09 — Discovery-failure policy.** A manifest failure ends discovery (no guessed next key)
  and is returned as `JobsError::Discovery`. With `OnError::Stop` the jobs in flight are dropped
  (cancelled); with `OnError::Continue` the admitted *and* queued known jobs still run to
  completion. Outcome `DiscoveryFailed` in both cases; the report separates attempted, queued and
  undiscovered work. Tested both ways.

- **PAG-10 — Item checkpoints replay the page.** `ItemCheckpoint { page: Resume<K>, skip }` and
  `resume_items(at, budget, fetch)`. `Items::finish` also returns the unemitted `remainder` and
  `after_remainder` (where to continue if the caller processes the remainder itself). The page
  checkpoint advances only after a page's last item is emitted; an empty page advances it in one
  explicit transition. A stable page is caller-enforced (not detectable in general); a replayed
  page shorter than `skip` is skipped entirely (documented and tested).

- **PAG-11 — Admission happens only while `next` is polled.** A freed slot is refilled on the next
  poll, not before returning a result. Futures are lazy and polled on the caller's task, so early
  admission would gain nothing while the caller processes a result, and a stopped consumer never
  causes admission. Construction calls nothing.

- **PAG-12 — After a halt nothing more is emitted.** Once a stop-mode failure, the failure limit or
  `finish` halts the window, retained results are not emitted (they go to the report's
  `retained`); `next` returns `None`. By construction a contiguous retained prefix is always
  emitted before the scheduler polls for more completions.

- **PAG-13 — Keys are `Clone`; one box per future.** `K`/`D`/`MK: Clone` (copies kept for failed
  and restart identity); items, results and errors are moved, never cloned; no `Send`/`'static`.
  Each admitted future is boxed once (`Pin<Box<Fut>>`) so it can be tagged with its index without
  pin projection under `unsafe_code = "forbid"`; concurrent polling uses `futures-util`'s
  `FuturesUnordered` (feature `alloc` only, `default-features = false`).

- **PAG-14 — Not provided, deliberately.** No `Stream` impls (the plan's `async fn next` only), no
  completion order or continuation after errors for dependent chains (not required; a chain cannot
  continue without a next key), no raw-body lease API (the full-job facade is the contract's
  required form), no byte/record limits (the reader's, at the codec boundary).

- **PAG-15 — No core change.** The traversals need nothing from `concord-core`: SDK adapters use
  generated operations and typed responses through closures.

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

**CSV-16 Escape dialects write what their reader reads (4.4 review, ASTRA-1).** `csv-core`
0.1.13's reader, inside a quoted field, discards the escape byte and copies the next byte whatever
it is (`transition_nfa`: `InQuotedField` + escape → `InEscapedQuote`, which copies one byte back
into `InQuotedField`); outside quotes the escape byte is data (`StartField`/`InField` copy it).
`csv`'s writer escapes only the quote inside a quoted field (`csv_core::quote`), so a literal escape
byte there was dropped on reading and changed the meaning of the byte after it:
`(r"back\slash,comma",)` read back as `backslash,comma` with `double_quote` on and off.
**Chosen: write correctly, not fail closed.** The writer owns its field bytes: `csv-core`'s writer of
the same settings still decides whether a field is quoted (`should_quote`), and inside quotes a
quote is written doubled (or escape + quote with `double_quote(false)`) and an escape byte twice;
unquoted fields are written as they are, which that reader reads literally. Refusing every quoted
field that contains the escape byte was rejected: it would refuse ordinary values (Windows paths)
in an advertised dialect when a correct encoding exists. Wire change: only quoted fields that
contain the escape byte, all of which were previously read back wrong; every other field keeps
`csv`'s bytes (`fields_without_escape_bytes_keep_the_csv_crate_bytes` compares the two writers over
every value type and ten dialects). Evidence (`tests/csv_write_integrity.rs`): the reviewed value
in both modes on the incremental (every split) and finite routes; a corpus of literal, repeated,
leading and trailing escapes and escapes before quotes, delimiters and terminators, for two escape
bytes × both quote modes × three quote styles × CRLF/LF, read at every split point and byte by
byte; every value of up to four bytes over `a \ " , \r \n` (1,555 values) in eight dialects, read
whole, byte by byte and in random partitions; escape bytes in explicit and inferred header names;
raw tables with non-UTF-8 bytes on the finite route. Outside this guarantee, unchanged:
`QuoteStyle::Never` (documented as possibly unparseable) and a reader with `quoting(false)` (CSV-03:
quotes are data to it).

**CSV-17 One serialization per row (4.4 review, SOL-1).** The shape capture (CSV-10) called the
row's `Serialize`, then `csv` called it again to write, so a stateful or conditional implementation
could be checked as `id` and written as `other` under the header `id`. One serializer now does
both: CSV-10's shape rules, the names (captured for the first row, compared afterwards), the field
count, and each scalar's field with `csv`'s value text (`Display` for integers, identical to the
`itoa` text `csv` writes; floats through a small `csv` writer, because `csv` formats them with
`ryu`, which is not on the crate's dependency allowlist in `scripts/check-graph.py`). The header is
inferred from that pass, and the row and an inferred header reach the output only after every
check. Visible to callers: a row's `Serialize` runs once per record (it ran twice); when a row fails
several checks, the error reported can differ (a field or byte limit reached during the pass is
reported before a name change found at its end). Evidence: `each_row_is_serialized_once_*` (finite,
`Encode<Vec<T>>`, upload, `Encode<StreamRows<S>>`: one call per record and `id` over 7, where the
old encoder wrote `MISLABELLED` under `id`).

**CSV-18 Writer limits bound the writer's buffers (4.4 review, ASTRA-2).** The limits were checked
after `csv` had serialized the whole record into an unbounded buffer and it had been copied (about
twice the record's size retained before the refusal). Now a record (header) is encoded into a buffer
that never exceeds `max_record_bytes` (`max_header_bytes`) plus the terminator: each field is
measured before it is copied (a value longer than the remaining room is refused without scanning),
the buffer grows by doubling up to that ceiling (`reserve_exact`), and a refused record never
reaches the output frame; the error keeps `record` and `limit`. The writer also no longer clones
explicit columns to write the header. `max_fields` stops the pass at the first field past it.
Not bounded: what the row's own `Serialize` does before it hands a value over, the finite route's
body, frames already handed to the transport, the captured names (at most `max_fields`), and two
fixed scratch buffers (a 40-byte number buffer, the float writer's 64 bytes). Evidence: unit tests
`write::tests` (buffer capacity ≤ limit + terminator for every record size up to and past the
limit; the frame untouched by a refused record or header) and `fields::tests`; integration tests
(a field after the oversized one is never serialized; a 1,000-field row stops after 4 attempts with
`max_fields(3)`; encoded-size boundaries with quotes and escapes); `tests/csv_write_memory.rs`
(Linux `VmHWM`, reset before each route, one test per binary): refusing a 16 MiB field with a
64-byte limit grew the peak by 12 / 8 / 8 KiB (finite / lazy / header); with the fix reverted, the
same test observed 32,452 / 32,232 / 32,224 KiB (the review: about 32 MiB, finite and lazy). Linux
process high-water observations, not allocation counts.
