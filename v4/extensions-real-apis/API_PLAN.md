# Extension API plan (coordinator-owned public interface)

Status: working contract for the implementers of the 4.4 extension phase. The coordinator owns
these public spellings; an implementer who finds one unsound changes it **and records why** in
`DECISIONS.md` (same directory) rather than silently diverging. Semantics come from the handoff
contracts (`CSV.md`, `STREAMING.md`, `PAGINATION.md`); this file fixes names, module layout and
ownership so that independent work composes.

## Layout and features

| Package / module | Owner | Feature |
|---|---|---|
| `concord-extras::stream` (byte sources, `OpenError`, `ReadError`, `Upload`, lazy `RecordBody`) | coordinator (done, commit 5d3ebf7) | `stream` |
| `concord-extras::csv` | CSV implementer | `csv` (implies `stream`) |
| `concord-extras::jsonl` | JSONL implementer | `jsonl` (implies `stream`) |
| `crates/concord-pagination` (new crate, no runtime dependency on Concord) | pagination implementer | — |
| `concord` facade: features `csv`, `jsonl`, `stream` forward to extras; modules re-exported as `concord::csv`, `concord::jsonl`, `concord::stream` | coordinator | |
| `examples/testserver` (owned loopback server with observable cleanup) | coordinator | |
| `examples/{github,stripe,shopify,salesforce,kubernetes}-sdk` and `*-app` | SDK implementers | |

Inside `concord-extras`, the module `csv` shadows the crate `csv`: refer to the dependency as
`::csv::...`.

Rust 1.99.1, edition 2024, `unsafe_code = "forbid"`, `missing_docs = "warn"`, clippy `-D warnings`.
No new dependency beyond: `csv 1.4`, `csv-core 0.1.13`, `futures-core 0.3`, `http-body 1`
(already added to the workspace), `futures-util 0.3` (pagination only, `default-features = false`,
features as needed). Ask the coordinator before adding anything else.

## Shared streaming contract (exists)

```rust
use concord_extras::stream::{ByteSource, Chunks, SourceError, OpenError, OpenErrorKind,
                              ReadError, ReadErrorKind, Position, Upload, UploadError, from_iter};
```

Crate-internal helpers every reader/writer must use (see `src/stream/`):
`open_response(response, operation, |head| checks)`, `Driver<S>` (unconsumed chunk, EOF, positions,
fused state, optional total limits, `error(kind, codec)`), `yield_now()` + `SKIP_BUDGET` (yield after
skipping that many ignored bytes in one poll), `RecordBody<S, E: RecordEncoder<T>>` (lazy upload body,
zero polls until the transport asks).

Every reader type has this shape (names per format):

```rust
impl XReader {                       // immutable, validated configuration
    pub fn open<T: DeserializeOwned>(&self, response: reqwest::Response) -> Result<XRecords<T>, OpenError>;
    pub fn open_for<T>(&self, response: reqwest::Response, operation: &'static str) -> Result<XRecords<T>, OpenError>;
    pub fn read<T, S: ByteSource>(&self, source: S) -> XRecords<T, S>;   // no status/head checks
}
impl<T, S: ByteSource> XRecords<T, S> {   // default S = reqwest::Response
    pub async fn next(&mut self) -> Option<Result<T, ReadError>>;      // fused after error / end
    pub fn position(&self) -> Position;
}
```

No spawned tasks, channels or background progress. Dropping the records value drops the source.

## CSV (`concord_extras::csv`)

```rust
pub struct Csv<H = HasHeader, D = Comma, P = StandardCsv>(PhantomData<..>);
pub struct HasHeader; pub struct NoHeader; pub struct Delimiter<const BYTE: u8>;
pub type Comma = Delimiter<b','>; pub type Semicolon = Delimiter<b';'>;
pub type Tab = Delimiter<b'\t'>; pub type Pipe = Delimiter<b'|'>;
pub trait HeaderPolicy: sealed::Sealed { const HAS_HEADER: bool; }
pub trait DelimiterPolicy: sealed::Sealed { const BYTE: u8; }
pub trait CsvProfile: 'static { fn options() -> CsvProfileOptions { CsvProfileOptions::default() } }
pub struct StandardCsv;               // CsvProfile with defaults
#[non_exhaustive] pub struct CsvProfileOptions { .. }  // builder: quote, escape, double_quote,
    // quoting, comment, trim, read_terminator, write_terminator(LineEnding), bom(BomPolicy),
    // read_mapping(ReadMapping), columns (writer schema), flexible/width policy,
    // max_record_bytes, max_header_bytes, max_fields, max_total_bytes, max_records, media(MediaPolicy)
    // — NO header flag, NO delimiter (they come from H and D)
pub struct CsvOptions { .. }          // runtime: CsvOptions::new(HeaderMode, delimiter: u8) + the same builder
pub enum HeaderMode { Present, Absent }
pub enum ReadMapping { Default, Positional, Columns(Vec<String>) /* constructor ReadMapping::columns([..]) */ }
pub struct CsvConfigError;            // invalid dialect, before any polling
impl<H, D, P> Csv<H, D, P> {
    pub fn options() -> CsvOptions;   // explicit conversion of the static policy to runtime options
    pub fn reader() -> Result<CsvReader, CsvConfigError>;
    pub fn writer() -> Result<CsvWriter, CsvConfigError>;
}
impl CsvReader { pub fn new(CsvOptions) -> Result<Self, CsvConfigError>; open/open_for/read as above;
                 pub fn open_raw(..) -> Result<CsvRecords<RawRecord>, OpenError>; /* raw route */ }
impl<T, S> CsvRecords<T, S> { next(), position(), pub fn headers(&self) -> Option<&RawRecord> }
pub struct RawRecord;                 // owned byte fields: len(), get(i) -> Option<&[u8]>, iter(), to_strings()
pub struct RawTable { pub headers: Option<RawRecord>, pub records: Vec<RawRecord> }
pub struct Rows<I>;                   // finite iterator payload: Rows::new(iter).with_columns([..])
pub struct StreamRows<S>;             // one-shot Stream<Item = Result<T, E>> payload: StreamRows::new(s).with_columns([..])
impl CsvWriter { pub fn new(CsvOptions) -> Result<Self, CsvConfigError>;
                 pub fn upload<S, T, E>(&self, rows: StreamRows<S>) -> Upload;     // lazy body + matching Content-Type
                 pub fn to_vec<T: Serialize>(&self, rows: Rows<impl IntoIterator<Item = T>>) -> Result<Vec<u8>, CodecError>; }
// Encode/Decode:
impl<T: DeserializeOwned, H, D, P> Decode<Vec<T>> for Csv<H, D, P>
impl<H, D, P> Decode<RawTable> for Csv<H, D, P>
impl<T: Serialize, H, D, P> Encode<Vec<T>> for Csv<H, D, P>
impl<I: IntoIterator, H, D, P> Encode<Rows<I>> for Csv<H, D, P> where I::Item: Serialize
impl<S, T, E, H, D, P> Encode<StreamRows<S>> for Csv<H, D, P>   // lazy body via RecordBody
```

`Encode::CONTENT_TYPE` = `text/csv; header=present` / `header=absent` from `H` (a const per impl).
Finite and streamed paths share one framing/mapping implementation (csv-core framing with limits,
`::csv::StringRecord/ByteRecord::deserialize` for mapping, a small field-name capture serializer for
writer schema checks). Migrate `examples/extension` to a profile of this codec.

## JSON Lines (`concord_extras::jsonl`)

```rust
pub struct JsonLines;                 // strict: application/jsonl (+ documented accepted aliases), BOM and blank lines refused
pub struct Ndjson;                    // application/x-ndjson; blank lines ignored (explicit leniency)
impl<T: DeserializeOwned> Decode<Vec<T>> for JsonLines / Ndjson
impl<T: Serialize> Encode<Vec<T>> for JsonLines / Ndjson     // compact, LF after every record
impl<S, T, E> Encode<StreamRecords<S>> for JsonLines / Ndjson
pub struct StreamRecords<S>;          // one-shot Stream<Item = Result<T, E>>
pub struct JsonLinesOptions;          // JsonLinesOptions::json_lines() / ndjson(); blank_lines(BlankLines),
                                      // bom policy, max_record_bytes, max_total_bytes, max_records,
                                      // record_errors(RecordErrors::{Stop, Yield}), media(MediaPolicy)
pub struct JsonLinesReader;           // new(JsonLinesOptions); open/open_for/read
pub struct JsonLinesRecords<T, S = reqwest::Response>;
pub struct JsonLinesWriter;           // new(opts); upload(StreamRecords) -> Upload; to_vec(..)
pub struct JsonValuesReader;          // consecutive JSON values (Kubernetes watch): new(); max_value_bytes; media policy
pub struct JsonValues<T, S = reqwest::Response>;
```

`RecordErrors::Yield` yields a `ReadError` of kind `Decode`/`Format` for one fully delimited
line and continues; `Stop` (default) is terminal. JSON values: a top-level array is one value;
`1` + `2 ` is `12`; only true EOF ends a primitive.

## Pagination (`concord-pagination`)

Entry points (semantics: `contracts/PAGINATION.md` P01–P09):

```rust
pub struct Page<T, K, M = ()> { pub items: Vec<T>, pub next: Option<K>, pub meta: M }
pub enum Resume<K> { Start, At(K) }
pub enum Order { Ordered, Completion }       // Ordered is the default
pub enum OnError { Stop, Continue }          // Stop is the default
pub fn sequential<K, T, M, E, F, Fut>(from: Resume<K>, budget: Budget, fetch: F) -> Sequential<..>
    where F: FnMut(Option<K>) -> Fut, Fut: Future<Output = Result<Page<T, K, M>, E>>;
pub fn independent<K, I, R, E, F, Fut>(keys: I, opts: Options, fetch: F) -> Independent<..>
    where I: IntoIterator<Item = K>, F: FnMut(K) -> Fut, Fut: Future<Output = Result<R, E>>;
pub fn first_page_then<..>(first: K, opts, fetch, plan: impl FnOnce(&R) -> Result<I, PlanError>) -> Independent<..>;
pub fn manifest_jobs<..>(opts: JobOptions, discover: FD, job: FJ) -> Jobs<..>
    // FD: FnMut(Option<MK>) -> Future<Output = Result<Manifest<D, MK>, ED>>
    // FJ: FnMut(D) -> Future<Output = Result<R, EJ>>   (the job owns response, parser and sink)
```

Each traversal: `async fn next(&mut self)`, a cheap `checkpoint()`/`progress()` snapshot, an
explicit `finish(self) -> Report`, and (sequential) `items()` with item-level checkpoints. Reports
distinguish exhaustion, configured bound, observed failure, user stop and incomplete discovery,
and keep failed identity separate from restart identity. No spawned tasks, no tokio dependency, no
hidden retry. Debug/Display never print keys, cursors or items.
