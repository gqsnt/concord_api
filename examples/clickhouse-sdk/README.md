# concord-sdk-clickhouse

The declaration owns the provider error document: `error String in IgnoreContentType<Text>;`. Native prepared/decoded
paths carry that document as `D` in `Prepared<T, D>` and `concord::Error<D>`; SDK status routing
and validation remain in the policy. Concord withholds document contents in formatting.

A bounded SDK for the [ClickHouse HTTP interface](https://clickhouse.com/docs/interfaces/http),
declared with `concord::api!` ([`src/wire.rs`](src/wire.rs)) and wrapped in a small public
client. Not published (`publish = false`); a workspace example of Concord.

## What it covers

| Area | Operations and behaviour |
|---|---|
| Queries | `ClickHouse::select(query, format)`: SQL text with typed placeholders `{name:Type}`, values bound by name and sent as `param_<name>` URL parameters in ClickHouse's escaped text format; `GET /?query=..` (the HTTP interface treats every `GET` as read-only) |
| Output formats | `CSVWithNames` or `JSONEachRow`, chosen at run time (`OutputFormat`), requested with the `X-ClickHouse-Format` header and checked against the `X-ClickHouse-Format` response header and the `Content-Type` before the body is read |
| Rows | read incrementally into any `serde::Deserialize` type (`Rows::next`), no read-ahead, from arbitrary chunk boundaries (quoted CSV fields and UTF-8 characters split across chunks) |
| Inserts | `ClickHouse::insert(table)`: rows from a one-shot `Stream`, serialized while the chunked body is sent, as `JSONEachRow` or `CSVWithNames` (every field quoted); the SDK writes the `INSERT INTO .. FORMAT ..` statement from validated identifiers |
| Authentication | HTTP Basic (`Credentials::basic`) or the documented `X-ClickHouse-User`/`X-ClickHouse-Key` headers (`Credentials::headers`); secrets flagged sensitive and absent from every `Debug` |
| Request context | `query_id`, `quota_key`, default database (`X-ClickHouse-Database`), `wait_end_of_query=1`, `send_progress_in_http_headers=1` |
| Metadata | `ResponseMeta`: `X-ClickHouse-Query-Id`, `-Summary`, `-Progress` (all of them, in order), `-Format`, `-Timezone`, `-Server-Display-Name`, `-Exception-Code`, `-Exception-Tag` — each an `Option`, never promised |
| Diagnostics | non-2xx exceptions with status, code (header or `Code: N` prefix), name (`SYNTAX_ERROR`) and the bounded message (`Error::Server`); in-body exceptions after a 2xx (`Error::LateException`) |
| Cancellation | `ClickHouse::cancel(&query_id)`: `KILL QUERY WHERE query_id = {id:String} ASYNC` with the ID bound; dropping `Rows` stops the client's read only |
| Health | `ClickHouse::ping()`: `GET /ping` |

**Excluded:** general SQL support (the SDK checks placeholders, not SQL), database and table
management, sessions (`session_id`), transactions (an insert is not atomic), compression
(`compress`, `Content-Encoding`), path-routed tables (26.8+), every other format, external data,
`http_write_exception_in_output_format=1` (see Limitations).

## Construction

```rust,no_run
use concord_sdk_clickhouse::{ClickHouse, Credentials, Identifier};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
// Follows no redirect: the credentials reach only this server. Add timeouts here.
let http = ClickHouse::recommended_http_client()?;
let clickhouse = ClickHouse::new(http, Credentials::basic("analyst", "fixture-password")?)
    .with_base_url("https://clickhouse.internal.example:8443")? // default http://localhost:8123
    .with_database(Identifier::new("default")?);
# let _ = clickhouse; Ok(()) }
```

`Credentials::headers(user, password)` sends `X-ClickHouse-User`/`X-ClickHouse-Key` instead.
reqwest drops `Authorization` on a cross-origin redirect but **not** a custom header, so with
header credentials use a client that refuses redirects (`recommended_http_client`) or one whose
policy refuses cross-origin hops. `Credentials::none()` uses the server's `default` user.

## Workflow A: a typed parameterized SELECT in a runtime-selected format

```rust,no_run
use concord_sdk_clickhouse::{ClickHouse, Error, OutputFormat, Outcome, Query, QueryId};

#[derive(Debug, serde::Deserialize)]
struct Match {
    date: String,
    season: u16,
    home_team: String,
    away_team: String,
    home_team_goals: u8,
    away_team_goals: u8,
}

async fn report(clickhouse: &ClickHouse, team: &str, csv: bool) -> Result<Vec<Match>, Error> {
    // The SQL is a literal; the values are bound, never spliced into it.
    let query = Query::new(
        "SELECT date, season, home_team, away_team, home_team_goals, away_team_goals \
         FROM football WHERE season = {season:Int16} AND home_team = {team:String}",
    )
    .bind("season", 2021_i16)?
    .bind("team", team)?;
    let format = if csv { OutputFormat::CsvWithNames } else { OutputFormat::JsonEachRow };

    let mut rows = clickhouse
        .select(query, format)
        .query_id(QueryId::new("report-2021")?)
        .rows::<Match>()
        .await?; // Server (non-2xx), Response (another format), LateException (2xx head)
    println!("query id: {:?}", rows.meta().query_id()); // Option: not promised

    let mut matches = Vec::new();
    while let Some(row) = rows.next().await {
        matches.push(row?); // LateException after some rows, or Read (transport, malformed, decode)
    }
    match rows.outcome() {
        Outcome::Complete => Ok(matches),
        // A clean end without `X-ClickHouse-Exception-Tag`: an unmarked late error may have
        // been read as rows. The caller decides whether that is acceptable.
        Outcome::EndedWithoutMarker => Ok(matches),
        _ => unreachable!("next() returned None after a clean end"),
    }
}
```

## Workflow B: stream an insert, then query it back

```rust,no_run
use std::convert::Infallible;

use concord_sdk_clickhouse::{
    CallContext, ClickHouse, Error, Identifier, OutputFormat, Outcome, Query, QueryId, Table, stream,
};

#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct Match {
    date: String,
    season: u16,
    home_team: String,
    away_team: String,
    home_team_goals: u8,
    away_team_goals: u8,
}

async fn load(clickhouse: &ClickHouse, rows: Vec<Match>) -> Result<Vec<Match>, Error> {
    let table = Identifier::new("football_load")?;
    // The caller's context, the same for both calls; distinct query IDs.
    let context = CallContext::new()
        .database(Identifier::new("fixtures")?)
        .quota_key("loader-7")?;

    let receipt = clickhouse
        .insert(Table::new(table.clone()))
        .query_id(QueryId::new("football-load/7/insert")?)
        .context(&context)
        // Polled only while the body is sent; `csv_with_names` is the CSV variant.
        .json_each_row(stream::from_iter(rows.into_iter().map(Ok::<_, Infallible>)))
        .await?;
    let written = receipt.summary().ok().flatten().and_then(|s| s.written_rows);
    println!("insert {:?}: {written:?} rows", receipt.query_id());

    let query = Query::new("SELECT * FROM {table:Identifier} WHERE season = {season:Int16}")
        .bind("table", &table)?
        .bind("season", 2021_i16)?;
    let back = clickhouse
        .select(query, OutputFormat::JsonEachRow)
        .query_id(QueryId::new("football-load/7/verify")?)
        .context(&context)
        .rows::<Match>()
        .await?
        .collect()
        .await?;
    assert_eq!(back.outcome, Outcome::Complete);
    Ok(back.rows)
}
```

The insert statement is `` INSERT INTO `football_load` FORMAT JSONEachRow `` in the `query`
URL parameter; the body is compact JSON with a line feed after every row
(`Content-Type: application/x-ndjson`, chunked). `csv_with_names` sends `CSVWithNames` with
every field double-quoted and LF record ends (`text/csv; header=present`).

## Errors

`Error` is one enum; `Display` names the operation (`ClickHouseHttp.select`), the status and the
exception code, **never** a server message, a bound value, a URL or a header value (exception
messages can quote the query and its parameters: read them with `message()`).

| Variant | When | Kept |
|---|---|---|
| `Request(RequestError)` | refused before sending: unbound placeholder, unknown or duplicate parameter, invalid identifier, query ID, quota key or credential | — |
| `Concord(concord::Error)` | Concord could not build the request, or the transport failed before a head, while sending an insert body, or while `Insert::send` read the receipt body after the head | Concord's error; `meta()` is `None`, including for that post-head transport failure |
| `Server(ServerException)` | non-2xx: `status()`, `code()`, `name()`, `message()` (≤ 64 KiB) | `ResponseMeta` |
| `LateException(LateException)` | 2xx, then an exception: the tagged block in the body, or `X-ClickHouse-Exception-Code` on the head; `rows_before()`, `source()` | `ResponseMeta` |
| `Response(ResponseError)` | 2xx head refused before the body: another format (`FormatMismatch`), a `Content-Type` the reader refuses | `ResponseMeta` |
| `Read(ReadFailure)` | the body failed after it started: `Transport` (a broken body), `Format`, `Decode`, `Limit`, with the position | `ResponseMeta`, Concord's `ReadError` |

`Error::is_outcome_unknown()` is `true` when the server may have done the work, fully or partly
(a transport failure after sending, a late exception, a broken body).

## Completion and partial outcomes

A 2xx status is **not** a complete result: ClickHouse sends the status and headers first, so an
exception during execution arrives inside the body. The HTTP interface page documents the block
it writes then (with `http_write_exception_in_output_format=0`, the default):

```text
\r\n__exception__\r\n<TAG>\r\n<error message>\n<message_length> <TAG>\r\n__exception__\r\n
```

`<TAG>` is the response's `X-ClickHouse-Exception-Tag`. (The page writes the message line as
ending in `\r\n`; the server source writes `\n` and counts it in `<message_length>`, which the
page's own example — 261 bytes, `262` — confirms. The parser locates the trailer by the tag and
accepts both.) Since 24.11 the server then breaks the HTTP framing (release notes: curl reports
"transfer closed with outstanding read data remaining").

The SDK's detection contract, in the order it is applied:

| Server behaviour | SDK result |
|---|---|
| non-2xx status (exception before the response started) | `Error::Server` |
| 2xx head whose `X-ClickHouse-Format` is not the requested format | `Error::Response` (body unread) |
| 2xx head with `X-ClickHouse-Exception-Code` (headers were still being sent, e.g. progress headers) | `Error::LateException` (`LateSource::Head`, or `Block` if the body holds the block) |
| 2xx, the tagged block appears in the body, split across chunks anywhere, whether HTTP framing ends cleanly or breaks | every row before it, then `Error::LateException` (`LateSource::Block { complete }`) |
| 2xx, the body breaks without a block | `Error::Read` of kind `Transport`; `Outcome::Failed` |
| 2xx, untagged plain-text exception at the end (servers that send no tag; before 24.11 the body even ended cleanly) | detected only when the text does not parse as rows (`Error::Read` `Format`/`Decode`); a text that parses (a CSV row of the right width) is a row, and the read ends with `Outcome::EndedWithoutMarker` |
| 2xx, clean end, nonempty tag announced, no block | `Outcome::Complete` under the tagged-exception contract |
| 2xx, clean end, tag absent or empty | `Outcome::EndedWithoutMarker` |

Rows returned before an error stay returned (`next()`); `collect()` returns all rows or the error.
`Outcome::Complete` is conditional on the server using the tagged-block contract: the tag is not
proof of query success if a different exception representation is enabled or a server behaves
otherwise. In particular, `http_write_exception_in_output_format=1` is not recognized by this SDK.
An empty tag is retained in response metadata but cannot be used to detect a block.

**`wait_end_of_query=1`** (`Select::wait_end_of_query(true)`) makes the server buffer the whole
result (memory up to `http_response_buffer_size`, then a temporary file) and send the head after
the query finished, so most execution errors become `Error::Server`. Trade-offs: no row arrives
before the query ends, and the server holds the whole result. It is not a guarantee — the page
says it "doesn't completely solve the problem" (the result must fit the buffer, and
`send_progress_in_http_headers` can interfere) — so the SDK checks the body the same way with it.

**Inserts** are not transactional. A source or row failure after the body started, a broken
connection or a late exception leaves an unknown number of rows stored
(`is_outcome_unknown()`); nothing is retried or rolled back.

**Cancellation.** Dropping `Rows` closes the connection; the page documents that running requests
do not stop when the connection is lost. `ClickHouse::cancel(&id)` sends `KILL QUERY .. ASYNC`,
which does not wait for the query to stop (a read-only user can stop only its own queries; an ID
that matches nothing is the `NOTHING_TO_KILL` exception by default).

## Limitations

- Placeholders are found by a small scanner that skips string literals, quoted identifiers and
  comments; the server remains the judge of the SQL and of each value against its declared type.
- Bound values: integers, `bool`, finite floats, strings, `Identifier`, `Option` (NULL as `\N`);
  no arrays, tuples or maps.
- Identifiers are restricted to `[A-Za-z_][A-Za-z0-9_]*`.
- `JSONEachRow` writes 64-bit integers as JSON strings by default
  (`output_format_json_quote_64bit_integers`): read them as `String` or with your own
  deserializer. `CSVWithNames` writes NULL as `\N`.
- `http_write_exception_in_output_format=1` (a user profile setting) writes exceptions in the
  output format instead of the tagged block; the SDK does not recognise that form.
- CSV inserts cannot send NULL (no quoted spelling); use `JSONEachRow`.
- The non-2xx exception text is read up to 64 KiB (`ERROR_BODY_LIMIT`); the in-body block up to
  16 KiB (its documented maximum).
- The response `Content-Type` of the two formats (`text/csv; charset=UTF-8; header=present`,
  `application/x-ndjson; charset=UTF-8`) comes from the ClickHouse source, not the docs; Concord's
  readers check it.

## Tests and fixtures

`cargo test -p concord-sdk-clickhouse` runs offline against the loopback server of
`examples/testserver`: `tests/protocol.rs` (wire shape, documented success and failure shapes,
negative cases), `tests/workflows.rs` (workflows A and B, the perturbation, cancellation), and
unit tests (placeholder scanner, escaping, the exception guard at every split, counters).

The fixtures are hand-authored from the official documentation, or synthetic
([`PROVENANCE.md`](PROVENANCE.md)). They show agreement with the documented shapes; they are
**not** vendor conformance, and no real ClickHouse server was contacted.
