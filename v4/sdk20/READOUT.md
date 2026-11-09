# Developer readout: writing and using an SDK on Concord

For a Rust developer deciding how to write or call a provider SDK on Concord. Each Rust block is
a verbatim excerpt, checked by `examples/quickstart/tests/doc_excerpts.rs`, of code the gate
compiles and runs: SDK internals (`src/`) for how an SDK builds on Concord, and the independent
first-use consumers' tests (`tests/first_use.rs`, written from the public docs without the
author) for what a caller writes. Code anchor `668e8e1b`; Microsoft Graph and the Distribution
Registry, merged later (`48ba16f8`, `3479a367`), and the wave 5 records (W5-1..8) are read at
`26c2c652` and described, not quoted. Live provider tier: NOT_RUN (local fixtures only).

## 1. The ordinary path: a declaration, a send policy, a caller

A client naming a send policy makes every generated `send()` return the policy's output, while
`prepare()` stays the declared request with Concord's `Error`. Nager.Holidays (Excerpt of
`examples/nager-holidays-sdk/src/lib.rs`):
```rust,ignore
pub client NagerHolidays() {
    base "https://nagerholidays.com";
    send KeepHead;
    format Json;
```

with one endpoint (Excerpt of `examples/nager-holidays-sdk/src/lib.rs`):
```rust,ignore
    GET holidays(country_code: String, year: u16) -> Vec<Holiday> {
        path "api/v4/Holidays/{country_code}/{year}";
    }
}
```

and the policy, written once (`send Type;` uses `Type::default()`; `examples/nager-holidays-sdk/src/lib.rs`
at `90b69011` — current status 2026-11-07: the SDK50 campaign shortened it to
`received.decode_with_meta()`, see `v4/sdk50/INDEX.md`):
```text
#[derive(Default)]
pub struct KeepHead;

impl SendWith<Vec<Holiday>> for KeepHead {
    type Output = Result<HolidayResponse, Error>;

    async fn send<E: Endpoint<Vec<Holiday>> + Send>(&self, endpoint: E) -> Self::Output {
        let received = endpoint.prepare()?.receive().await?;
        let meta = received.meta();
        match received.decode().await {
            Ok(holidays) => Ok(HolidayResponse { holidays, meta }),
            Err(error) => Err(Error::with_meta(error, meta)),
        }
    }
}
```

`receive()` executes once and returns the response for any status, body unread; `meta()` copies
the head first, so a later failure keeps it; `decode()` is the declared handling (`send()` is
`receive()` then `decode()`). The consumer's call (Excerpt of
`examples/nager-holidays-sdk/tests/first_use.rs`):
```rust,ignore
let response = client(&server).holidays("AT", 2026).send().await.unwrap();
assert_eq!(response.meta.status(), StatusCode::OK);
assert_eq!(response.meta.headers()["x-example"], "first");
```

and a 400 (Excerpt of `examples/nager-holidays-sdk/tests/first_use.rs`):
```rust,ignore
let error = client(&server).holidays("XX", 0).send().await.unwrap_err();
assert_eq!(error.kind(), ErrorKind::InvalidRequest);
assert_eq!(error.status(), Some(StatusCode::BAD_REQUEST));
assert_eq!(error.meta().unwrap().status(), StatusCode::BAD_REQUEST);
let response = error.into_concord().into_response().unwrap();
assert_eq!(response.bytes().await.unwrap().as_ref(), REJECTION);
```

Its other tests: truncated JSON is `Concord(Decode)` with status 200 kept; a body cut mid-transfer
a Concord error with status 200; a `text/plain` 200 `Concord(Metadata)`, never an empty success; a
hang-up before the head has no status and no meta. No partial vector, no retry. Ten of the 25 SDKs
declare a policy; Twilio and Slack measured one and reverted it (an empty-SID refusal stays a
domain method; Slack's `prepare()?.send()` would return an `ok: false` envelope as success).

## 2. Response metadata and typed provider errors

`concord::ResponseMeta` (status, version, headers, operation; `Debug` prints header names, never a
value) comes from `Received::meta()` or `Incoming::meta()`. `concord::Error::meta()` returns it
for `Status` and `Metadata` errors only, the kinds that keep the response; Nager's
`From<concord::Error>` uses it, so `?` keeps a head when one arrived. Reading a provider's error
document is the SDK's job, deliberately and bounded. Frankfurter's policy
(`examples/frankfurter-sdk/src/lib.rs` at `90b69011` — current status 2026-11-07: replaced by
`Received::document` and `decode_with_meta`, see `v4/sdk50/INDEX.md`):
```text
let received = endpoint.prepare()?.receive().await?;
let meta = received.meta();
if !received.status().is_success() {
    // Status is authoritative even when the document is absent, invalid or inconsistent.
    let document = match <Json as Decode<Problem>>::check_content_type(&received.head()) {
        Err(error) => Err(ErrorBodyError::Metadata(error)),
        Ok(()) => match received.bytes_limited(ERROR_BODY_LIMIT).await {
            Err(error) => Err(ErrorBodyError::Read(error)),
            Ok(bytes) => <Json as Decode<Problem>>::decode_body((), bytes)
                .map_err(ErrorBodyError::Decode),
        },
    };
    return Err(Error::Provider(Box::new(ProviderError { meta, document })));
}
```

`bytes_limited(n)` reads whatever the status, with no head check: `Limit` past `n` (rest unread),
`Transport` if reading fails. A 2xx decode failure keeps `meta` too. The caller (Excerpt of
`examples/frankfurter-sdk/tests/first_use.rs`):
```rust,ignore
let busy = provider_error(api.latest_pair("EUR", "CHF").send().await);
assert_eq!(busy.meta.status(), reqwest::StatusCode::SERVICE_UNAVAILABLE);
assert_eq!(
    busy.meta.headers().get("retry-after").map(|v| v.as_bytes()),
    Some(&b"7"[..])
);
assert!(busy.document.is_ok());
```

The consumer also saw `ErrorBodyError::Metadata` for an HTML 502, `Decode` for a truncated 422 and
`Read` for an oversized or cut document, each with the status; `Display`/`Debug` omit the
provider's message. `decode_limited(n)` bounds a success body (eight SDKs). Error schemas,
`Retry-After` and retries are the SDK's or the application's; Concord copies no header, never
retries.

**Current status (2026-11-06):** Ten SDKs use `Received::decode_limited`: Slack, Linear, Dropbox,
Google Drive, Airtable, Snowflake, World Bank, Notion, Microsoft Graph and Distribution Registry
(see their current `examples/*-sdk/src` implementations).

## 3. Pagination with lossless partial outcomes

`concord_pagination::Pages<T, K, M, E>` starts at `Resume::Start` or `Resume::At(key)` and calls
the SDK's page factory. Airtable's, under a cap its callers must pass (Excerpt of
`examples/airtable-sdk/src/lib.rs`):
```rust,ignore
Pages::new(from, move |offset: Option<String>| {
    let client = client.clone();
    let base = base.clone();
    let table = table.clone();
    let options = options.clone();
    async move {
        let response = client
            .list_page(&base, &table, &options, offset.as_deref())
            .await?;
        Ok(Page {
            items: response.value.records,
            next: response.value.offset,
            meta: response.meta,
        })
    }
})
.max_pages(max_pages)
```

`next_page()` is `Ok(None)` only after the provider's last page (an empty page with a cursor is not
the end). A cap reached while the provider has more is `PagesError::Bounded { resume, .. }`, on the
last page plain exhaustion; a failed page is `PagesError::Provider { error, failed, .. }` (`failed`
is the page to retry); every later call is `PagesError::Ended`, never `Ok(None)`. No hidden cap,
no retry. The consumer (Excerpt of `examples/airtable-sdk/tests/first_use.rs`):
```rust,ignore
let mut pages = client.list_pages(BASE, "Tasks", ListOptions::default(), Resume::Start, cap(1));
let mut records = pages.next_page().await.unwrap().unwrap().items;
let error = pages.next_page().await.unwrap_err();
assert!(matches!(error, PagesError::Bounded { .. }));
assert_eq!(error.resume(), Some(&Resume::At(OFFSET.into())));
assert_eq!(error.progress().admitted, 1);
```

Passing `error.resume()` as `from` fetched the rest in one request. `collect_complete()` never
returns a prefix as success: on a 503 the consumer got `Incomplete { collected, error }` with two
records, `failed == Resume::At(offset)` and the 503 head and document in the SDK error.
`PageItems` (`pages.items()`) keeps these rules per item (item `checkpoint()`, `into_stream()`, no
read-ahead) but drops page metadata. `PagesError` prints no provider error or key. A cursor is
valid only with its listing's options, kept in the factory (Airtable, Dropbox; W4-6).

## 4. Incremental outputs

`-> stream T` makes the declared handling return an open reader (`Records<F, T>`, needing
`F: Incremental<T>`) once the head is accepted, body unread; `-> stream` returns the unread
`Incoming`; a policy wraps either. ntfy (Excerpt of `examples/ntfy-sdk/src/lib.rs`):
```rust,ignore
GET subscribe_sse(query since?: Since) -> stream Event {
    path "sse";
    response SseFormat;
}
```

`SseFormat`, ntfy's own `Incremental<Event>`, delegates its head check to Concord's `Sse` and reads
with `SseReader` at a 16 MiB event bound; `send()` returns `Result<Events, NtfyError>`. The SSE
reader completes only at an event boundary: input ending inside a line or after undispatched
fields is a terminal `Format` error, a broken body a terminal URL-free `Transport` error naming the
operation. After one event the consumer hit `Format` (EOF before the blank line; a malformed
frame), `Decode` (missing fields) or `Transport` (cut connection) (Excerpt of
`examples/ntfy-sdk/tests/first_use.rs`):
```rust,ignore
assert_eq!(failure.kind(), kind);
assert_eq!(failure.position().records, 1);
assert!(failure.position().bytes >= message_frame().len() as u64);
assert!(failure.operation().is_some());
assert_eq!(cursor.last_message().unwrap().as_str(), "message001");
assert_eq!(events.completion(), Completion::Failed);
assert!(events.next().await.is_none());
assert!(events.next().await.is_none());
```

`completion()` is `Reading`, `EndOfInput` (the only proof every record was read) or `Failed`;
after a terminal error a reader yields `None`, so the caller keeps the error. The JSON Lines, JSON
values and CSV readers behave alike (CSV ending inside a quoted field is `Format`, D-W45-R4) and
are cancel-safe when their byte source is, as `Incoming` is. Provider completion stays SDK code:
OpenAI declares `-> stream` and reads the `Incoming` with a configured reader whose `check_head`
refuses a wrong media type before any body read (Excerpt of `examples/openai-sdk/src/lib.rs`):
```rust,ignore
let reader = SseReader::new(SseOptions::new().max_event_bytes(MAX_EVENT_BYTES));
// Concord's `text/event-stream` head check (one Content-Type, charset absent or UTF-8).
reader
    .check_head(&incoming.head())
    .map_err(|_| Error::Stream("expected text/event-stream".into()))?;
// The `Incoming` is the byte source: a broken body keeps Concord's own transport error.
Ok(EventStream {
    events: Some(reader.read(incoming)),
    meta,
})
```

An end before `response.completed` is `Error::Stream` (the consumer got the partial delta, then
that error, the head on `stream.meta`). ClickHouse's clean end is `Complete` only if the head
announced `X-ClickHouse-Exception-Tag` and no exception block came, else `EndedWithoutMarker`.
Kubernetes' six watches are `-> stream WatchEvent<T>`; reconnecting and relisting stay the caller's.

## 5. Request bodies

Multipart is `MultipartBody` with `request Multipart;`, runtime media `EncodedBody` with
`request Encoded;`. Google Drive (Excerpt of `examples/google-drive-sdk/src/api.rs`):
```rust,ignore
POST multipart(body payload: MultipartBody, query fields: String) -> File {
    query { "uploadType" = "multipart"; }
    request Multipart;
}
POST initiate(
    body metadata: NewFile, query fields: String,
    header("x-upload-content-type") media_type: String,
    header("x-upload-content-length") length: u64,
) { query { "uploadType" = "resumable"; } }
PUT transfer(
    body payload: EncodedBody,
    header(CONTENT_RANGE) range: String,
) -> File { request Encoded; }
```

Drive builds `MultipartBody::related().part(Part::new(json).header(..)).part(..)`, OpenAI
`MultipartBody::form_data().text("purpose", "batch").file(..)`. The boundary is
`concord-boundary-<n>`, the smallest `n` found in no part; invalid names or headers and an empty
`related` body are `Build` errors before I/O; `related()` omits RFC 2387's `type` on purpose
(Google omits it), `related_with_type(root)` sends it (AST-W34-1); the body goes with
`Content-Length`. A runtime media type contradicting a declared one is a `Build` error. A declared
empty buffered body is sent with `Content-Length: 0` (Drive's resumable probe), decided in
`Decoder::execute` on the body actually sent. A lazily produced upload is an `Upload`, a body
carrying its writer's `Content-Type`. ClickHouse declares one (Excerpt of
`examples/clickhouse-sdk/src/wire.rs`):
```rust,ignore
POST insert(
    query("query") statement: String,
    query("query_id") query_id?: QueryId,
    body rows: Upload,
) -> stream {
    request Encoded;
}
```

and fills it with `JsonLinesWriter::new(JsonLinesOptions::ndjson()).upload(StreamRecords::new(rows))`,
which polls the caller's rows only while the body is sent. A source failing mid-body fails the
transfer; sent bytes are not taken back and nothing is retried. The consumer's source yields one
row, then an error (Excerpt of `examples/clickhouse-sdk/tests/first_use.rs`):
```rust,ignore
assert_eq!(polls.load(Ordering::SeqCst), 0);
let error = upload.await.unwrap_err();
assert!(matches!(&error, Error::Concord(_)));
assert!(error.is_outcome_unknown());
assert_eq!(polls.load(Ordering::SeqCst), 2);
assert!(server.requests().is_empty());
```

No receipt, and `is_outcome_unknown()`: the server may have applied part of it; dedup or replay is
the application's. Resumable protocols stay SDK code (Drive's `308` state machine and `probe()`,
W3-14; the registry's chunked `PATCH`).

## 6. Stateful policies: signing with client state

`send value;` names a required client value as the policy: not placed on the wire, cloned per
`send()` (S3 keeps an `Arc` inside), withheld from `Debug` at every placement (SOL-W23-1); an
optional or defaulted value is refused at compile time. Amazon S3 (Excerpt of
`examples/amazon-s3-sdk/src/api.rs`):
```rust,ignore
pub client S3Api(
    origin: concord::BaseUrl,
    /// The SigV4 context and send policy of every operation.
    mut signer: Signer,
) {
    base origin;
    send signer;
```

`Signer::sign` takes the prepared request apart (`into_parts`), canonicalizes the target and signs
it (SigV4: the client's credentials, region, clock); `SignedRequest::send` executes it with the
endpoint's decoder (`Decoder::execute`) and maps the response to `S3Error` (Excerpt of
`examples/amazon-s3-sdk/src/client.rs`):
```rust,ignore
macro_rules! signed_send {
    ($($output:ty),* $(,)?) => {$(
        impl SendWith<$output> for Signer {
            type Output = Result<$output, S3Error>;

            async fn send<E: Endpoint<$output> + Send>(&self, endpoint: E) -> Self::Output {
                self.sign(endpoint.prepare(), |_| Ok(()))?.send().await
            }
        }
    )*};
}
```

Callers chain the generated setters and call `send()` (`get_object(key)`, `set_if_match`,
`set_range`, `send()` in `tests/first_use.rs`, whose fake verifies every signature). The
generated `prepare()` stays unsigned; `S3Client::prepare(op)` signs without sending (W23-9).
User metadata cannot be declared, so `PutObject::metadata(m)` returns a type sent with
`.send(&s3)` (W23-6/7). The move removed 36 forwarding definitions (245 lines);
`checks/phase-a/tests/send_policy.rs` signs two concurrent clients with different credentials,
each request with its own client's signature.

## 7. Following server links

`Prepared::follow(url)` keeps method, headers (credentials included), body, declared handling and
identity and replaces only the URL, verbatim; another origin (scheme, host, port) is a `Build`
error before I/O; `Debug` withholds a followed URL. Narrower rules are the SDK's. PokéAPI checks
path, user info, fragment and query first (Excerpt of `examples/pokeapi-sdk/src/lib.rs`):
```rust,ignore
pub async fn follow(&self, next: &str) -> Result<Response<PokemonPage>, Error> {
    let list = self.list().prepare()?;
    checked_url(list.request().url(), next, false)?;
    finish(list.follow(next)?.receive().await?).await
}
```

The consumer's traversal (Excerpt of `examples/pokeapi-sdk/tests/first_use.rs`):
```rust,ignore
let result = match next.as_deref() {
    Some(url) => api.follow(url).await,
    None => api.list().set_limit(2).set_offset(0).send().await,
};
```

A foreign origin, another resource, a fragment, user info, a relative URL or a non-URL is
`Error::InvalidUrl`, nothing sent. Following into another operation needs a placeholder prepare
(`pokemon("placeholder")`, W1-17). Also following: GitLab (`Link`), Drive (session URL),
Snowflake (status, partitions), Graph (`nextLink`/`deltaLink`, checked against the listing), the
registry (`Link`, upload `Location`). Redirects are the application client's: reqwest drops
`Authorization`, `Cookie`, `Cookie2`, `Proxy-Authorization`, `WWW-Authenticate` across origins
but forwards GitLab's `PRIVATE-TOKEN`, so GitHub and GitLab build a redirect policy (W2-4).

## 8. Advanced escapes

`Prepared::into_parts()` gives client, native request and decoder; `Decoder::execute` sends an
edited request into the same `Received<T>` boundary as `receive()`. ClickHouse appends per-query
`param_<name>` pairs, outside Concord's redaction (W4-7). As written at SDK20 (`5c3ef5f6`,
`examples/clickhouse-sdk/src/client.rs`; *note 2026-11-07: since SDK50 between waves 1–2 the
pairs are a `query(..)` value of the select/command endpoints and this helper is gone*):
```text
let (client, mut request, decoder) = endpoint.prepare()?.into_parts();
if !params.is_empty() {
    let mut pairs = request.url_mut().query_pairs_mut();
    for (name, value) in params {
        pairs.append_pair(name, value);
    }
}
Ok(decoder.execute(&client, request).await?)
```

S3 signing and GitHub's `send_edited` take the same route. Readers over a wrapped body:
`check_head` is the check `open` would make, `read_for` reads any `ByteSource` naming the
operation, and `Incoming` is a `ByteSource` whose chunk failure is Concord's `Transport` error.
ClickHouse wraps it in a late-exception guard (Excerpt of `examples/clickhouse-sdk/src/client.rs`):
```rust,ignore
if let Err(e) = json.check_head(&incoming.head()) {
    let problem = ResponseProblem::ContentType { label: e.label() };
    return Err(refused(operation, problem, meta));
}
let body = Guarded::new(incoming, meta.exception_tag());
Reader::Json(Box::new(json.read_for(body, operation)))
```

`into_source_unredacted()` returns a source's error by value (OpenAI returns Concord's transport
error from its event stream, ClickHouse from a broken insert receipt). A response no declaration
produced gets an identity from `Operation::label`; `transport_error` makes a native failure
Concord's URL-free `Transport` error (Excerpt of `examples/amazon-s3-sdk/src/object.rs`):
```rust,ignore
let converted = match self.operation {
    Some(op) => op.transport_error(error),
    None => Operation::label("S3 object body").transport_error(error),
};
```

Shopify opens its signed bulk download, a native request without API headers, with
`reader.open_for(response, BULK_RESULT_OPERATION)`: a non-2xx is one `OpenError`, response unread.
`into_response()` leaves the boundary: native `Debug` and `url()` show the full URL.

## 9. Tradeoffs and known limits

| Limit | Met by | Disposition |
|---|---|---|
| A path value is one segment (`a/b` → `a%2Fb`) | S3 keys, rewritten after `prepare()` (W1-5); registry names, one client per repository with the name in a validated base URL (W5-1) | DEFER, product decision: syntax, `.`/`..`/empty-segment and encoding rules |
| `:` is percent-encoded in a segment | registry digests `sha256:…`, last segment rewritten then `follow` (W5-2) | DEFER: the encode set is every SDK's wire bytes |
| A policy cannot replay a request (endpoint consumed, not `Clone`) | registry `401` Bearer challenge: no policy, a request factory per call (W5-3) | DEFER: one consumer; candidate `Prepared::try_clone` |
| `PageItems` drops page metadata | Slack (W2-11), Graph delta checkpoints (W5-4) | DEFER, product decision: both use `Pages` |
| `PagesError` lacks a provider-error accessor; large enough for clippy `result_large_err` | Dropbox (W3-16), Graph consumer boxes it and matches per call site (W5-5) | DEFER: additive accessor next iteration |
| Dynamic query keys; repeated parameter blocks | ClickHouse `param_<name>` (W4-7, section 8); Kubernetes selectors in 19 places (D-A5) | DEFER with F7 parameter groups |
| Policy-only per-request input; a decorated request cannot re-enter the policy | S3 user metadata (W23-6/7) | DEFER: one consumer |

Other costs: a policy missing an output is E0277 at `api!` and at `send` (W23-2); a finite format
used for `-> stream T` gives six identical E0277s (D-A3); an `Incremental` wrapper with non-default
limits calls its configured reader's `check_head` and returns `read_for(body, operation)` from
`open` (resolved by D-W45-R5; AUTHOR_GUIDE §5.9, CUSTOM_FORMATS §9b), keeping the typed
`-> stream T`; provider error
schemas stay SDK code (W4-5, W5-7). Rejected: unsealing `ToHeaderValue` for validated credentials
(D-W2-HDR) and the readers' `ReadError`/`RecordStream` constructors (W3-4).

## 10. The 25 SDKs × Concord features

Derived by `grep -rnE` over each SDK's `src/` (not tests or docs), dropping lines whose first text
is `//`, then reading every hit: 23 SDKs at `668e8e1b`, Graph and the registry at `26c2c652` (via
`git archive`). Same-named SDK items were dropped after reading (`into_parts` of Kubernetes' list,
Stripe's pages, Shopify's connection, S3's SigV4 helper; `attempt.follow()` in GitHub's and
GitLab's redirect policies). Patterns:

```text
policy     ^\s*send [A-Za-z_][A-Za-z0-9_:]*;
Pages      \b(Pages|PageItems|ItemStream)\b
streams    ->\s*stream\b ; \b(SseReader|SseEvents|JsonLinesReader|JsonValuesReader|CsvReader|RecordStream|Records|Incoming|Sse|JsonStream)\b
bodies     -w (MultipartBody|EncodedBody|Upload)
meta       concord::ResponseMeta|use concord::\{[^}]*\bResponseMeta\b ; struct ResponseMeta\b
follow     \.follow\([^)]
native     \binto_parts\(|\bDecoder::|\.into_response\(|\btransport_error\(|\bOperation::label\(|open_for\(
```

| SDK | Send policy | Pages | Streams / readers | Multipart / runtime body | ResponseMeta | `follow` | Native escape |
|---|---|---|---|---|---|---|---|
| SDK01 Nager.Holidays | `send KeepHead;` | — | — | — | Concord's | — | — |
| SDK02 PokéAPI | `send KeepHead;` | — | — | — | own (+ final URL) | yes | — |
| SDK03 Twilio | — (reverted) | — | — | — | Concord's | — | — |
| SDK04 Amazon S3 | `send signer;` | yes | `-> stream Bytes` | — | own | — | `into_parts` + `Decoder::execute`; `Operation::label` |
| SDK05 Frankfurter | `send SendPolicy;` | — | — | — | Concord's | — | — |
| SDK06 GitLab | `send SendPolicy;` | yes | — | — | own | yes | — |
| SDK07 Slack | — (reverted) | yes | — | — | own | — | — |
| SDK08 OpenAI | `send SendPolicy;` | — | `-> stream` + `SseReader` | form-data | Concord's | — | — |
| SDK09 ntfy | `send NtfyPolicy;` | — | `-> stream Event` (SSE, NDJSON) | — | — | — | — |
| SDK10 Linear | — | yes | — | — | Concord's | — | — |
| SDK11 Dropbox | `send DropboxPolicy;` | yes | — | — | wraps Concord's | — | — |
| SDK12 Google Drive | — | yes | — | related; `EncodedBody` | Concord's | yes | — |
| SDK13 Open-Meteo | — | — | `-> stream` (JSON or CSV per call) | — | Concord's | — | — |
| SDK14 Airtable | `send Policy;` | yes | — | — | Concord's | — | — |
| SDK15 ClickHouse | — | — | `-> stream` + JSON Lines / CSV `read_for` | `Upload` (writers) | wraps Concord's | — | `into_parts` + `Decoder::execute` |
| SDK16 Snowflake | — | — (`independent`) | — | — | Concord's | yes | — |
| SDK17 World Bank | `send SendPolicy;` | yes | — | — | Concord's | — | — |
| SDK18 Notion | — | yes | — | — | Concord's | — | — |
| SDK19 Microsoft Graph | — | yes | — | — | Concord's | yes | — |
| SDK20 Distribution Registry | — (W5-3) | yes | `-> stream` (`Incoming::chunk`) | `EncodedBody` | wraps Concord's | yes | — |
| GitHub (Phase A) | — | yes | — | — | own | — | `into_parts` + `Decoder::execute`; `Decoder::incoming` |
| Stripe (Phase A) | — | yes | — | — | own | — | — |
| Shopify (Phase A) | — | — | `JsonLinesReader` on a native download | — | — | — | native request + `open_for` |
| Kubernetes (Phase A) | — | — | `-> stream WatchEvent<T>` (`JsonStream`) | — | — | — | error document via `into_response` |
| Salesforce (Phase A) | — | yes | `-> stream` + `CsvReader` | — | — | — | error document via `into_response` |

**Current status (2026-11-06):** Open-Meteo's `-> stream` permits response selection at run time;
[`OpenMeteo::forecast`](../../examples/open-meteo-sdk/src/lib.rs) buffers the bounded body before
returning `ForecastResponse`, rather than returning an incremental reader.

ResponseMeta: "Concord's" uses `concord::ResponseMeta`, "wraps" is a newtype adding provider
accessors, "own" an SDK struct older than `concord::ResponseMeta` and of another shape. 19 of the
25 use no native escape; the six that do are signing (S3), per-query keys (ClickHouse), request
edits and a cross-origin download (GitHub), a signed download (Shopify) and error documents read
from a status error's native response (Kubernetes, Salesforce).
