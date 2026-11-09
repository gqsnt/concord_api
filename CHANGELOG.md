# Changelog

## Unreleased — desirability loop (candidate)

Not released: the version is unchanged (4.2.0). Records: `v4/desirability/` (decisions in
`v4/desirability/DECISIONS.md`).

### Added

- **H06c broader `controls` adoption:** audited all 55 example SDKs; nine more adopt `controls`
  (11 with the four H06 pilots). Credentials and their rate/ledger state are replaced together in Discord,
  Sentry and Adyen; DigitalOcean, QuickBooks, Zendesk and USGS withhold destination changes;
  QuickBooks, Salesforce and Datadog withhold unchecked generated construction (Datadog also
  withholds independent key setters). Validated fixture/deployment constructors preserve local
  tests, token rotation retains shared state, QuickBooks `set_connection` and Cloudflare/Zendesk
  `set_policy` remain public. The other 44 SDKs are unchanged. Per-SDK replacement routes are in
  [MIGRATION](docs/MIGRATION.md); the [impact table](v4/desirability/impact/H06c.md) records all decisions.
- **An SDK can withhold a generated control** (H06, candidate B: control visibility; accepted at
  pilot scope by the owner decision of 2026-11-08, at `d39ef1df`). The opt-in
  owner statement `controls { pub(crate) name; remove name; }` on the client or a scope makes a
  generated control crate-visible or leaves it out of the expansion; every control not listed
  stays public, and a declaration without the statement generates exactly what it did. The
  controls are the client's `new` and `with_base_url` and the `set_x` / `maybe_x` of the owner's
  values (a send policy value's `set_policy` included); `new` cannot be removed, and an optional
  value's `set_x` and `maybe_x` take the same access. The four pilot adopters are
  Shopify (`ShopifyAdminShop::set_shop` removed: another shop is another
  `shop(..)` entry; `set_access_token` kept), Zendesk (`Zendesk::set_subdomain` removed: another
  account is `Zendesk::connect`; `set_policy` kept), Salesforce (`SalesforceApi::with_base_url`
  crate-visible and `set_version` removed: `OrgContext::new` is the route to another instance or
  version) and Mastodon (the generated client `Api` is now public, from `Mastodon::api()`, with
  its `new` and `with_base_url` crate-visible, so it stays bound to its instance). The broader
  migration is deferred to H06c because its additional removals fall outside H06's frozen
  expected-breakage list. [Migration notes](docs/MIGRATION.md#to-h06-pilot-sdk-controls) and the
  [55-SDK impact table](v4/desirability/impact/H06.md) record the pilot scope and replacement
  routes. Author guide §2.2.
- **An omitted operation classification fails the SDK build** (H08, candidate B). The opt-in
  client statement `operations Name;` generates `Name`, a `#[non_exhaustive]` enum with one variant
  per endpoint (the endpoint's path below the client in UpperCamelCase, `items.get` → `ItemsGet`;
  `#[cfg]`s carried over), with `ALL`, `operation()` and `from_operation()`. With `send`, every
  generated `send()` passes its endpoint's variant to the policy through the new
  `concord::SendOperation<T, Name>` (the sibling of `SendWith<T>`, which is unchanged), so a policy
  that classifies with a `match` without a wildcard arm stops compiling (E0004, naming the variant)
  when an endpoint is added and not classified. Clients without the statement generate exactly
  what they did. Adopted by the Strava (`Class::of`, the uncertain-outcome wording), Sentry
  (admission buckets, the read/write split and the paging flag; the run-time
  `Configuration("unsupported operation")` refusal is gone) and Kraken (signed or public per
  operation, no longer per URL path, so a base URL with a path prefix is followed instead of every
  call being refused as `SigningInput`) SDKs; their policies implement `SendOperation` instead of
  `SendWith`. Author guide §5.14.
- **Full migration** (H08): every other SDK whose policy or paging classified operations by name,
  path or table adopts it. PayPal (read/write and `PayPal-Request-Id` replay per operation, the
  method rule folded in), Zendesk (read/write and the update's `409` collision; the string
  literal `"Zendesk.ticket.update"` is gone), Datadog (the legacy v1 post told apart from the
  `POST` search), Square (`Error::create_outcome_unknown`; `SquarePolicy` keeps `SendWith` and
  gains a forwarding `SendOperation` impl, since it ignores the variant) and GitHub (no send
  policy: the enum only, and one exhaustive table from operation to listing id from which the
  recognized ids are derived; ids and `LinkPages::new` unchanged). The other SDKs do not adopt
  it: Discord, Cloudflare, Xero, Mailgun, QuickBooks and Spotify decide by HTTP method or URL with
  no per-operation exception; Kubernetes, S3, Adyen, Elasticsearch, Postmark and Europe PMC
  classify through traits that the compiler already checks (Postmark also applies a uniform
  method rule); Microsoft Graph and the distribution registry classify with data the enum cannot
  carry; the rest do not classify operations.

### Documentation

- **Generated documentation states the actual contract first** (H04). The docs `api!` generates
  show, on every request type, the declared base, the declared path and the composed default
  request separately, and that `with_base_url` replaces the whole base including its path; the
  declared `send()` result first (for a policy without a result template, the `SendWith`
  implementation that defines it); `concord::Error` named fully; the `Accept` outcome the
  declaration already decides; the operation name and method. Navigation methods on client and
  scope pages carry a short entry pointing to the request type instead of repeating its summary
  (client and scope pages are about half as long). Policy wording no longer assumes every policy
  signs. No public API, behaviour or version change: only emitted doc attributes changed.

## Unreleased — interface consolidation (candidate)

Not released: the version is unchanged (4.2.0) and no release has been decided. Records:
`v4/sdk20/` and `v4/sdk50/` (decisions in `v4/sdk50/DECISIONS.md`). Source-compatible for callers
of the existing API except where noted.

### Added

- **Owned response metadata.** `concord::ResponseMeta` (status, version, header map, operation;
  `Debug` prints header names, never values), from `Received::meta()` and `Incoming::meta()`:
  the value SDKs kept beside a decoded value or provider error after the body is consumed.
- **One response boundary.** `Prepared::receive()` executes once and returns `Received<T>`: any
  status, body unread, the declared handling and the endpoint identity (`Operation`). Read the head
  (`head()`), then consume once: `decode()` (what `send()` does), `into_incoming()` (an incremental
  reader chosen at run time) or `into_response()` (native). `Decoder::execute`, `Decoder::receive`
  and `Decoder::transport_error` serve natively edited requests without reconstructing execution.
- **Truthful streaming outputs.** `-> stream T` (format `Incremental<T>`) makes `send()` return the
  format's reader without reading the body; `-> stream` returns the unread `Incoming` body.
  `Incremental` is implemented by `Csv<..>`, `JsonLines`, `Ndjson`, the new `jsonl::JsonStream`
  (consecutive JSON values) and `IgnoreContentType<F>`.
- **Readers open declared responses.** `CsvReader::open/open_raw`, `JsonLinesReader::open` and
  `JsonValuesReader::open` accept `Incoming`, `Received<T>` or a native response
  (`stream::Openable`); the endpoint's identity reaches every `ReadError` without `open_for`.
  `completion()` distinguishes a clean end of input from a failure or an early stop.
  `into_stream()` returns an owning standard `Stream` (`stream::RecordStream`).
- **Runtime body media.** `Encode::encode_body` (defaulted) returns an `EncodedBody` whose media
  type is bound to its bytes; contradictions with a declared media type or `content-type` header
  are `Build` errors before I/O. The `Encoded` format sends an `EncodedBody` or `stream::Upload`
  through a declared body.
- **Ordinary pagination.** `concord_pagination::Pages`/`PageItems`/`ItemStream`: nameable,
  owned traversals whose `Ok(None)` means provider exhaustion; caps and failures are typed
  `PagesError`s; `collect_complete` returns `Incomplete` with the collected prefix.
  `JobsReport::into_resume` / `IndependentReport::into_resume` decompose a report losslessly.

- **One SDK adapter for all operations.** Every generated endpoint implements `concord::Endpoint<T>`
  (`prepare()`, `operation()`; the output `T` is a parameter), so an SDK writes one generic adapter (its own error and
  metadata) instead of a wrapper method per operation. `Prepared::operation()`.
- **The SDK's own `send()`.** A client statement `send Policy;` makes every generated `send()`
  return `<Policy as SendWith<T>>::Output`: an SDK's result type and error, metadata, or a signed
  request, from one policy type (`concord::SendWith<T>`), without wrapper methods or imports.
- **Following server links.** `Prepared::follow(url)` replaces the URL of a prepared request with
  one the server returned (`Link: rel="next"`, a `next` field), keeping the endpoint's headers,
  handling and identity; another origin is a `Build` error, and `Debug` withholds the URL.
- **Bounded body reads.** `Received::bytes_limited(n)` reads a body (any status) up to `n` bytes,
  `ErrorKind::Limit` beyond: for provider error documents. `Received::decode_limited(n)` decodes a
  finite output with the same bound.
- **Multipart request bodies** (feature `multipart` of `concord-extras` and of the facade,
  `concord::multipart`; no new dependency, reqwest's own `multipart` feature stays off). A declared
  `body form: MultipartBody` with `request Multipart;` sends `multipart/form-data`
  (`MultipartBody::form_data()`, `.text(name, value)`, `.file(name, file_name, content_type,
  bytes)`) or `multipart/related` (`MultipartBody::related()`, `related_with_type(root)`,
  `.part(Part::new(bytes).header(name, value))`). The boundary, `concord-boundary-<n>`, is the
  first deterministic candidate that occurs in no part; it is the body's runtime media type
  (`EncodedBody` precedence) and the body is sent with `Content-Length`. Control characters in
  names, file names and part header values (tab excepted), `"`/`\` in form-data names and file
  names, and an empty `related` body are encode errors (`Build`, before I/O) that show no part
  bytes. The OpenAI and Google Drive SDK examples moved onto it from hand-written framing.
- **Server-sent events.** Feature `sse` of `concord-extras` (facade: `concord::sse`): `SseReader`
  (`open`, `read`) → `SseEvents` (`next`, `completion`, `position`, `last_event_id`, `retry`,
  `into_stream`, `into_source`), `Event` (`event`, `data`, `id`) and the `Sse` format
  (`-> stream sse::Event` with `response Sse;`). WHATWG framing, no decoding of `data`; stricter
  than a browser: invalid UTF-8, a body ending inside a line or an event, and an event over
  `max_event_bytes` (1 MiB) or past `max_events` are terminal errors. `examples/openai-sdk` reads
  its event stream with it.
- **Readers over a body the SDK checked or wraps; source errors back by value** (`concord-extras`,
  feature `stream` and the readers). Additions only; `open`, `read` and every existing error stay
  as they were.
  - `check_head(&self, &ResponseHead) -> Result<(), CodecError>` on `JsonLinesReader`,
    `JsonValuesReader` and `SseReader`, as `CsvReader` already had: the reader's own media policy,
    exactly what its `open` applies (same label), head only.
  - `read_for(source, operation: &'static str)` on `CsvReader` (and `read_raw_for`),
    `JsonLinesReader`, `JsonValuesReader` and `SseReader`: `read` of any `ByteSource`, without
    status or head checks, naming the operation in every later `ReadError` (as `open_for`).
  - `impl ByteSource for concord_core::Incoming`: a failure is a `SourceError` ("reading the
    response body failed") whose underlying error is Concord's own `Error` (kind `Transport`, the
    operation named, URL removed), where `open(incoming)` keeps reqwest's.
  - `ReadError::into_source_unredacted(self)` and `SourceError::into_source_unredacted(self)`
    (`-> Option<Box<dyn Error + Send + Sync + 'static>>`): the byte source's underlying error by
    value, to `downcast` Concord's error or a provider-typed one without stashing it. A parser's
    error (`Format`/`Decode`) stays in `codec()`, by reference only. `Display`/`Debug` unchanged.

  From wave 4 (`examples/ntfy-sdk`, `examples/openai-sdk`, `examples/clickhouse-sdk` each wrote a
  byte source over `Incoming::chunk` to keep Concord's error, and could not name the operation of
  `read`); tested per reader in `crates/concord-codecs/tests/reader_wrapping.rs` (moved from
  `concord-extras` with the formats).
- **The response head survives a failed body.** Every error raised after a response head arrived
  keeps that head: `Error::head()` (a borrowed `ResponseHead`) and `Error::head_meta()` (an owned
  `ResponseMeta` with the error's operation; clones the header map) return it for a `Status` or
  `Metadata` error and for a `Decode`, `Limit` or `Transport` error raised while the body was read,
  failures of `Incoming::chunk` and `Incoming::drain` included. Concord moves the head out of the
  native response before reading the body instead of cloning it (an `Incoming`, which keeps its
  response, attaches a copy). Both are `None` when no head arrived (`Build`, a transport failure
  before a response). `Error::meta()` keeps its contract — `Some` exactly when `into_response()`
  returns the response (`Status`, `Metadata`) — and `Error::status()` stays the status of a
  `Status` error; the status of any failed call that received a response is
  `error.head().map(|h| h.status())`.
- **A decoded value with its head.** `Received::decode_with_meta()` and
  `Received::decode_limited_with_meta(limit)` return `Result<(T, ResponseMeta), Error>`: `decode()`
  and `decode_limited()` plus the owned head, for an SDK that keeps metadata beside the value. For a
  finite output or no output the head is moved out once the format accepted it; a stream reader or
  an `Incoming` keeps its response, so its head is a copy.
- **Typed documents of any status.** `Received::document::<F, D>(limit)` returns
  `(ResponseMeta, Result<D, Error>)` whatever the status: `F::check_content_type` and `F::context`
  run on the head before the body is read, the body is read up to `limit` bytes, and
  `F::decode_body` decodes it with that context. The declared handling and its 2xx rule are not
  applied. A refused head is `Metadata` (the response kept unread in the error), a longer body
  `Limit`, a failed read `Transport`, a body that does not decode `Decode`; each names the operation
  and carries a copy of the head, and a `Decode` error keeps the bytes (`Error::body_unredacted()`,
  unredacted, never printed by Concord). For provider error documents and for statuses an SDK
  accepts itself (a `202`, a `304`), where `bytes_limited` followed by `decode_body((), ..)` skipped
  the format's head checks and `Context`.
- **Declared body bound: `limit N;`.** A statement on the client, a scope or an endpoint (the
  nearest wins) whose `usize` constant expression bounds the read of a finite typed output
  (`-> T`): a longer body is `ErrorKind::Limit` and is not decoded. The bound is carried by the
  endpoint's `Decoder` (`Decoder::with_limit(n)`, `Decoder::limit()`), so the generated `send()`,
  `Received::decode()`, `Received::decode_with_meta()`, `Decoder::decode` and `prepare()?.send()`
  all honour it; `Received::decode_limited(n)` replaces it for one call. On an endpoint without
  `->` or with `-> stream` it is a compile error; a client or scope bound does not apply to such
  endpoints.
- **Parameter groups: `params Name(..);` and `..Name`.** A client statement declares optional
  (`x?: T`) or initialized (`x: T = v`) values once; an endpoint includes them with `..Name` in its
  values, spliced at that position (wire order as written), with the same `set_x`/`maybe_x`
  setters as if they were written there. Each group also generates a trait `Name` (with the
  client's visibility) of those setters, implemented by every endpoint that includes the group,
  for SDK code written once over them. Compile errors: a required value in a group, `..Name` on a
  scope or the client, a group declared below the client, a group that includes a group, an
  unknown group (with a near-name suggestion), an unused or duplicate group, a group named like a
  generated type, a value on both the endpoint and an included group, a value in two included
  groups.
- **Values that write their own query pairs: `query(..) x?: T`.** An endpoint value whose type
  implements the new `concord::QueryPairs` (`query_pairs(&self, &mut QueryPairsWriter<'_>)`, which
  calls `QueryPairsWriter::pair(key, value)`) chooses its own keys: one of several exclusive
  cursors, or keys named at run time. Its pairs are written where it is declared among the
  endpoint's own parameters (after the keys the chain already has, before keys first introduced by
  the endpoint's `query { }` block); an empty key is a `Build` error. It takes no key and no
  `each`, may not sit on a scope or the client (a parameter group may hold it) and may not be
  `#[sensitive]`; the endpoint's `Debug` withholds it, and `Prepared`'s `Debug` redacts a run-time
  key on the well-known secret list. `QueryPairs` is implemented for `Vec<(K, V)>`, `[(K, V)]`,
  `BTreeMap<K, V>` and references.
- **Generic code sends through the client's policy: `concord::Sends<O>`.** Every endpoint of a
  client with a send policy (`send P;` or `send value;`) implements `Sends<O>` for `O`, the policy's
  output for that endpoint; `Sends::send(self)` is the generated `send()` (its future is `Send`,
  the policy value cloned from the client). A helper bounded on it (every page of a listing) sends
  exactly as callers do without being handed the policy. A client without a policy generates no
  `Sends`; its `send()` is `prepare()?.send()`, reachable through `Endpoint<T>`.
- **Declared send results: `send P -> Template;`.** `send Policy -> Result<_, Error>;` (or
  `send value -> ..;`) makes every generated `send()` return `impl Future<Output = Template> + Send`
  with each type placeholder `_` replaced by the endpoint's output, where plain `send P;` returns
  `<P as SendWith<T>>::Output`; rustdoc shows the concrete type. The template must equal the
  policy's `Output` exactly (an equality bound reported at the template, not a coercion). A
  lifetime `'_`, a `_` inside an expression and anything inside a macro invocation are not
  placeholders (rustc rejects `result!(_)` there). Plain `send P;` is unchanged.
- **What a failure proves about delivery: `Error::delivery()`.** Returns `concord::Delivery`
  (`#[non_exhaustive]`): `NotSent` when Concord refused the request before handing it to the client
  (`ErrorKind::Build`: not built, a header set twice, a body that could not be encoded), `Answered`
  once a response head reached Concord (any status, even if the body then failed, was refused or
  was too long), `Unknown` otherwise — a refused connection included, because the client may have
  followed a redirect from a server that performed the request. Which failures make a write
  ambiguous stays the SDK's rule.
- **Strict header-field readers: `concord::field`.** `field::single(&HeaderMap, name)` returns the
  one field line (`Result<Option<&HeaderValue>, FieldError>`, `Repeated` for several lines) and
  `field::unsigned(&HeaderMap, name)` a non-negative decimal (`Result<Option<u64>, FieldError>`:
  ASCII digits, surrounding spaces and tabs ignored; a sign, fraction or exponent is `Invalid`, so
  `+5` is refused where `str::parse::<u64>` accepts it; `Overflow` beyond `u64::MAX`), on any header
  map: a `ResponseMeta`'s, an error's head, a native response's. `ResponseHead::single` and
  `ResponseHead::unsigned` delegate to them. `concord::FieldError` (`Repeated`, `Invalid`,
  `Overflow`; `#[non_exhaustive]`) never contains the value.

### Changed

- **An empty declared body is sent with `Content-Length: 0`.** reqwest omits the field for an
  empty buffered body, so an empty PUT/POST looked like a request without content (RFC 9110
  §8.6; Google Drive's resumable-upload status probe documents `Content-Length: 0`). Applied when
  Concord executes (`Decoder::execute`, behind `send()`/`receive()`), to the body actually sent,
  so a body replaced natively after `into_parts` keeps its own length. A request without a body
  and streamed bodies are unchanged.
- `Error`'s identity is an `Operation`; `Display` of a labelled native response starts with the
  label. `Error::endpoint()` is unchanged for declared endpoints.
- The reader `open` methods take `impl Openable` instead of `reqwest::Response` (a native
  response still works unchanged).
- `IgnoreContentType<F>` forwards `encode_body`, so a runtime media type of `F` is kept.
- **CSV input that ends inside a quoted field is an error.** A body cut after an opening quote
  (`…,"12.7` at the end of input) was a complete table: `csv-core` ends such a field silently,
  so `Decode<Vec<T>>`/`Decode<RawTable>` returned the record and `CsvReader` ended with
  `completion() == EndOfInput`. Both routes now fail with a terminal `Format` error, "the CSV
  input ends inside a quoted field" (with the `record` number; none for the header), in every
  dialect: doubled quotes, an escape byte, standard or custom terminators; quotes in comment lines
  and with `quoting(false)` open nothing. A closed quoted field without a final terminator is
  still a record. The same tracking corrects a final comment line containing `\r`, which yielded
  a record of one empty field (wave 4, Open-Meteo F4).
- **A comment byte with a custom read terminator is a `CsvConfigError`.** `csv-core` ends a
  comment line only at a line feed, which its DFA does not see when the read terminator is a
  custom byte: everything after the first comment line was silently skipped and the read ended
  cleanly. `comment(Some(_))` with `read_terminator(RecordTerminator::Byte(t))` for any `t` other
  than `\n` is now refused by `CsvReader::new`, `CsvWriter::new` and the static formats (before
  anything is read or sent): "a CSV comment byte needs a read_terminator that ends lines at \n
  (Standard or Byte(b'\n')): a comment line ends only at a line feed". With the standard
  terminators nothing changes; the `comment` option now documents that a lone `\r` does not end
  a comment line (it runs to the next `\n`).
- **A generated name: `operation()`.** Every generated endpoint type has an inherent
  `operation() -> concord::Operation` (its declared identity, without naming the output);
  `Endpoint::operation` forwards to it. An SDK's own inherent `operation` on a generated type now
  conflicts (`E0592`) and is deleted
  ([migration notes](docs/MIGRATION.md#to-the-consolidated-interface)).
- **Generated endpoint documentation.** An endpoint type's rustdoc states the wire shape first —
  `Request: METHOD URL`, labelled as the declared base URL that the client's `with_base_url`
  replaces, then the query keys and headers it may send — and then the body, the output and, with
  a send policy, the policy (its type, or "the client value `name`" for `send value;`) and whether
  `send()` returns the declared result or `SendWith<T>::Output`; `prepare()` notes that it builds
  the request before the policy. It also names the parameter groups the endpoint includes, the
  declared `limit` of its finite body read and, for each `query(..)` value, the type that documents
  its keys.
- **The standard formats live in `concord-codecs`; `concord-extras` is a compatibility facade.**
  The formats, their tests and their features (`json`, `form`, `stream`, `csv`, `jsonl`,
  `multipart`, `sse`; none on by default) moved to the new workspace crate `concord-codecs` (not
  published). The `concord` facade depends on it (its `extras` and format features enable the
  codecs features) instead of `concord-extras`. `concord-extras` re-exports `concord_codecs::*` —
  every item is the same item, not a copy — and forwards the same feature names, so existing paths
  and features keep working; the features this section attributes to `concord-extras` are those
  forwarded features.

### Capability crates

- **`concord_codecs::check_content_type` and `Charset`.**
  `check_content_type(&ResponseHead, accepted: &[&str], Charset) -> Result<(), CodecError>`
  requires exactly one well-formed `Content-Type` matching an accepted `type/subtype` or
  `type/*+suffix` entry (case-insensitive; a response media range is refused; an empty list accepts
  nothing), with the standard formats' parser and static labels: for a custom format's
  `Decode::check_content_type`. `Charset::Utf8` allows no charset or one naming UTF-8 or US-ASCII;
  `Charset::Ignore` checks the parameter syntax only. Not re-exported by `concord`.
- **`concord-pagination`: strict `Link` parsing** (feature `link`; its only dependency is `url`).
  `link::parse(values, &base) -> Result<Vec<Link>, LinkError>` parses every `Link` field value
  (RFC 8288), resolving targets against the response URL; `link::relation(&links, "next")` returns
  `Relation::Absent`, `One(&Link)` or `Several`. The first `rel` wins; a bare `rel`, a trailing `;`,
  an empty unquoted value, non-ASCII or control bytes are errors; `Debug` and `LinkError` show no
  input text. The GitHub, GitLab, Snowflake and distribution-registry SDK examples use it in place
  of their own tokenizers.
- **`concord-pagination`: checked offsets.** `next_offset(offset, limit, len, total)` returns the
  next offset of an offset page, `Ok(None)` at clean exhaustion, or an `OffsetError` (`ZeroLimit`,
  `TooManyItems`, `Overflow`, `BeyondCount`, `DidNotAdvance`) that retains no provider data. With a
  total count offsets are zero-based and a short page may continue; without one a short page ends
  the traversal.
- **`concord-pagination`: provider errors and item events.** `PagesError::provider_error()`
  (`Option<&E>`) and `PagesError::into_provider_error()` (`Result<(E, Resume<K>, Progress), Self>`;
  a cap or an earlier end is returned unchanged) reach the factory's own error without matching
  `PagesError::Provider`. Opt-in `Pages::item_events()` returns `PageItemEvents`, whose `next()`
  yields `PageItemEvent::Item(T)`, `Page(M)` after each fully consumed page that has a successor,
  and `Complete(M)` with the final page's metadata only after a clean end; a cap or a provider
  failure is a `PagesError`.
- **`concord-retry` (pilot).** A new workspace crate (not published), recorded as a pilot.
  `Retry::new(Policy, Replay).run(factory, classify, now, sleep, jitter)` bounds application
  attempts (`Policy::max_attempts`, an optional total `timeout` covering attempts and waits, capped
  exponential backoff) with the caller's `Replay::Permitted`/`Never` separate from the SDK's
  `Decision::Done`/`RetryAfter(Option<Duration>)`/`Stop`; a server delay is a minimum, never
  shortened. `http::retry_after` reads `Retry-After` strictly (decimal seconds or IMF-fixdate; a
  repeated field is an error). Feature `concord`: `Retry::receive(prepared, ReplayMode, ..)` replays
  a `Prepared` through `into_parts()` and `Request::try_clone()` (a body that cannot be cloned is
  refused before I/O with `ReplayMode::Required`, or sent once with `BestEffort`), and
  `Retry::receive_with` — accepted — adds an async, fallible before-attempt hook that returns the
  request with per-attempt state, and `ReceiveRun`/`ReceiveReport` accounting for dispatches, hook
  refusals and attempts without a response head. Feature `tokio`: `Retry::run_tokio`. No runtime or
  HTTP dependency by default.
- **`concord-auth` (pilot).** A new workspace crate (not published), recorded as a pilot with no
  acceptance decision. `basic(user, password)`, `bearer(token)` and `authorization(scheme, token)`
  build sensitive `Authorization` values (`CredentialError` holds no input); `TokenCache<K, E>` is a
  bounded, keyed cache with one shared acquisition per key (`get_or_refresh`), checked lifetimes
  with a refresh margin, and `invalidate(key, &Generation)`, which removes only the generation a
  failed request used, so a late 401 cannot drop a newer token. No runtime or HTTP client
  dependency; `Debug` shows no credential. Stateful refresh-token rotation was not accepted.
- **`concord-ratelimit` (pilot, not accepted).** A new workspace crate (not published), merged as a
  pilot that was not accepted, with no further investment recorded: process-local keyed admission
  (`Limiter<K>`: fixed-window `Budget`s, SDK-interpreted `Observation::Window`/`Cooldown`, a global
  gate, concurrency `Lease`s, `Mode::Wait`/`Refuse`) over an injected `Clock` (`TokioClock` with
  feature `tokio`). Provider headers and bucket rules stay in the SDK.

### Maintenance

- **SDK fixtures keep their bytes on checkout.** The root `.gitattributes` marks
  `examples/*/fixtures/**` and `examples/*/tests/fixtures/**` as `-text`, so CRLF row endings and
  quoted line breaks in wire fixtures are not converted on a Windows checkout; two SDKs without a
  rule of their own had failed their fixture tests on native Windows.

## Unreleased — 4.4 extensions (candidate)

Additive, not released: the version is unchanged (4.2.0) and no release or version has been
decided. Records: `v4/extensions-real-apis/`.

### Added

- **`concord-pagination`**, a new workspace crate (not published): bounded page and job
  traversal over plain request factories — dependent chains with item checkpoints, independent
  page plans, first-page plans and manifest-discovered streamed jobs, with budgets, a bounded
  window, cancellation by drop and reports. It does not depend on Concord, and core Concord still
  performs no pagination ([README](crates/concord-pagination/README.md)).
- **Incremental bodies in `concord-extras`**, re-exported by the `concord` facade under the same
  feature names:
  - `stream` (`concord::stream`): response opening with unread refusals, byte sources, safe read
    errors and lazily produced upload bodies;
  - `csv` (`concord::csv`): `Csv<HasHeader, Delimiter>`, finite and incremental;
  - `jsonl` (`concord::jsonl`): JSON Lines / NDJSON, finite and incremental, and consecutive JSON
    values.
- `ReadError::is_terminal`: whether a read error ended the read. Only a bad line ended by a line
  feed, reported by a JSON Lines reader in `RecordErrors::Yield` mode, is not terminal; the reader
  then goes on with the next record.
- Five provider SDK examples, each with an external caller example and loopback tests:
  `examples/{github,stripe,shopify,salesforce,kubernetes}-sdk` and `-app`.

## Unreleased — V4.3 candidate (documentation and examples)

No public API, behaviour or version change: the four crates are still 4.2.0 and their code is
unchanged. Records: `v4/dx-4.3/`.

### Documentation

- **Request streams are formats.** The format guide (§1) and the author guide (§5.7) said a
  streamed request body is "not a format". An `Encode<T>` takes its payload by value and may return
  a streaming `reqwest::Body`; only a per-body media type (a multipart boundary) needs the native
  route or a `content-type` header the SDK controls, and only an incrementally read *response*
  needs `send_raw`. The guides now say so and link a tested example.
- The caller guide (§7) points at `decoder.finish(Err(e))` for converting a transport failure when
  you execute a request yourself: reqwest's own error prints the full URL.

### Examples and recipes

- `examples/native`: a non-`Clone` one-shot request stream (`upload.rs`), and response metadata
  together with the typed value after a native edit (`metadata.rs`), with what each failure stage
  keeps.
- `examples/sdk`, `examples/app`, `examples/extension`: a two-service SDK; domain adapters that
  return generated operations, independent per-service outcomes, a named workflow and a bounded
  traversal with a continuation; and an independent crate adding an operation the SDK does not
  declare through an SDK-owned context. `examples/fixture` is their loopback test server.
- `docs/RECIPES.md` indexes the new recipes.

## 4.2.0

A focused maintenance release: compiler correctness and exact advice text, fair benchmarks and
trustworthy maintained checks, and corrected caller documentation. The statement DSL, the four
crates, the codec contracts and metadata policies, request types, `IntoValue` borrowing and the
Rust 1.99.1 minimum and pin are unchanged; no edit is needed except where an expression fragment
forwarded by your own `macro_rules!` into a type or initializer now means what Rust says
([migration notes](docs/MIGRATION.md#from-41-to-42)). Not published on crates.io; the four crates
share one version and require each other exactly (`=4.2.0`).

### Fixes

- **Conditional `Clone` keeps every bound.** Types that two crates' macros write with the same
  tokens (`$crate::Payload`) are different types; the bounds of an endpoint request's conditional
  `Clone` were deduplicated by their printed text, so the bound of a non-`Clone` one could be
  dropped and the declaration failed with E0277 without any `.clone()`. Each value and body keeps
  its own bound: such declarations compile, and cloning a request that holds a non-`Clone` value is
  still rejected.
- **Type macros are output types.** `-> payload!()`, `-> m::payload![]` and `-> ::c::payload! {}` are
  ordinary payload types; they were read as the removed in-place format. The removed spellings
  around a type macro still report their replacement, and a braced type macro that takes the
  endpoint's block gets a direct hint.
- **Exact advice and generated documentation.** Types in diagnostics and generated rustdoc are
  rendered token by token: literal contents are kept (`Payload<{ count("a :: b") }>` was suggested
  as `"a::b"`, changing a six-byte length to four), a raw name stays raw (`body r#type?: String`,
  not `body type?`), punctuation written apart stays apart and a macro's input is shown token for
  token, so applied advice selects the same macro arm. A payload holding a `macro_rules!` fragment
  is named `T` instead of spelled.
- **Forwarded `macro_rules!` fragments keep their grouping.** Value, body and output types and
  value initializers that contain an expression fragment (`Payload<{ $e * 2 }>`, `= $e * 2`) mean
  what Rust says (6 for `$e = 1 + 2`, where 4.1 computed 5, or named different types for the body's
  format and the signature); a forwarded `dyn A + B` behind `&` is accepted.

### Maintenance

- **Fair benchmarks.** The S3 construction pair compared unequal work (only Concord prepared
  `Accept: application/json`); the manual side now prepares it too. Every promised equal-work pair
  is registered with its measured stage and compared before timing by an ordinary test (47
  comparisons, with negative controls for a changed or missing `Accept`, repeated values and their
  order, URL, method and body). The 4.1 S3 ratios stay in `benches/criterion/README.md`, marked as
  unequal work; no corrected figure is derived from them.
- **Examples declare their own features.** `examples/renamed` enables `extras`; the gate tests each
  example package alone (`test-examples-isolated`), so workspace feature unification cannot hide a
  missing feature, and checks that the no-extras consumer really has none.
- **Workflow guard inspects structure.** `scripts/check-ci.py --workflow` parses `ci.yml` (a
  documented YAML subset) and counts only an enabled, blocking gate step on Linux and Windows; a
  commented, echoed, disabled or failure-swallowing invocation no longer satisfies it. It is static
  inspection; CI logs remain the evidence that the gate ran.
- **Fresh package extraction.** `scripts/package-check.sh` and `scripts/consumers.sh --packaged`
  recreate only their own marked output, refuse other directories, never build from stale extracted
  sources, and build one packaged consumer from a deep, non-ASCII path.
- **Retired mutation helper.** The historical mutation helper under `tests-gen/` refuses to run:
  it had no maintained caller and could report a build failure as a kill.
- **Metadata stage tested.** `examples/native` asserts that a 2xx head the format refuses is a
  `Metadata` error before its withheld body is read, with the response recoverable; removing the
  head check fails this test. Lifecycle (cancellation, drop, one request per send) and
  multi-owner composition controls were added.
- **Documentation.** Header text accepts horizontal tab (as `http` does); `into_response()` returns
  the response of a `Status` or a `Metadata` error; runtime bases keep percent escapes and whole
  segments may not come out empty, `.` or `..`; `Text`'s `Content-Type` policy; compiler-dependent
  diagnostics and recursion limits; `&HeaderValue` among the `IntoValue` conversions; short task
  notes (Config, format imports, `Accept`, inference, no output vs `-> ()`, scope reuse, copying
  the Riot example) and a compiled example of moving a default into a scope.
- **Performance experiments (nothing adopted).** An allocation-free long-chain header planner and
  a selective response media scanner were measured and independently reviewed; both are deferred
  until workloads show the cases matter. Records: `v4/release-4.2/PERF.md`.


## 4.1.0

The modular V4: four crates, formats as ordinary Rust types, and response metadata checked before
a body is read. Not published on crates.io; depend on a checkout or a git revision. Rust 1.99.1 is
both the minimum and the pinned toolchain. Upgrade steps are in
[`docs/MIGRATION.md`](docs/MIGRATION.md) (its sections on format types and the codec contract).

### Crates

- `concord`: the facade you depend on; `api!`, re-exports, features `json` and
  `reqwest-defaults` (default), `form`, `extras`.
- `concord-core`: the runtime and the open `Encode`/`Decode` contracts.
- `concord-macros`: the declaration macro. It knows no format: it resolves each format as a type
  path, with the same rules as any other path.
- `concord-extras`: the standard formats (`Json`, `Form`, `Text`, `Binary`) and
  `IgnoreContentType<F>`, written against the public traits only.

The four crates share one version and require each other exactly (`=4.1.0`); update them together.

### Changes

- **Formats are Rust paths.** `format Json;`, `response my::Csv;`: no format words, no privileged
  formats. A format of yours is named and resolved exactly like `Json`.
- **Strict `Json`.** A response must declare `application/json` or an `application/*+json` type,
  with no charset other than `utf-8`/`us-ascii`, or the call fails with `ErrorKind::Metadata`.
  For a server that mislabels its responses, `IgnoreContentType<Json>` skips only that check,
  explicitly, per endpoint or scope.
- **`Accept` is a fallback.** A typed endpoint whose format suggests a media type (`Json`, `Form`)
  sends it only when the declared headers leave no `accept` value. An `Accept` you declare always
  wins; a client-wide `Accept` configured on your `reqwest::Client` does not (declare it in
  `headers` instead). When Concord sends none, reqwest's own default goes out. To stop only the
  suggestion, wrap the format so it forwards every decoding step and suggests nothing
  ([format guide, section 8](docs/CUSTOM_FORMATS.md#8-the-accept-suggestion)).
- **Metadata errors keep the response unread.** A refusal by `check_content_type` or `context`
  happens before Concord reads the body; `Error::into_response()` returns the response with its
  body still available.
- **`Client::validate_header_names()`** checks every native header name an endpoint could send,
  without building a request or any value. Its guarantee is limited: a reported endpoint cannot
  build any request, but `prepare()` returns an earlier failure first (body encoding, host, path or
  query formatting), and a passing check says nothing about values.
- **Decompression stays reqwest's.** Enabling reqwest's `gzip` (or other) feature in your
  application works with every format: the format sees the head reqwest exposes and the unpacked
  body. Concord enables no compression feature itself.

### Windows

No library behaviour differs on Windows. The verification scripts did: `scripts/consumers.sh`
passed about 50 KB of `cargo metadata` output as one command-line argument, beyond Windows'
32,767-character limit, so the gate failed there while passing on Linux. It now passes the data as
a file, and every gate step runs `python`, `cargo` and `tar` behind a wrapper that measures each
command line as Windows would quote it (UTF-16 units) and refuses one over the limit on all
platforms. CI now runs the same complete inventory (packages and benchmarks included) on Linux and
Windows; doing so exposed two more Windows-only script defects, also fixed: GNU tar read the
`D:\...` archive path as a remote `host:path`, and a carriage return from Windows Python ended a
path read with `read`.

### Tests and checks added in this release

- `checks/consumers/facade-gzip`: an application with reqwest's gzip support, against compressed
  fixtures, a withheld body and a decompression-disabled control.
- `crates/concord/tests/accept.rs::no_suggestion_keeps_the_metadata_checks`.
- `scripts/argv-guard.sh` and `scripts/argv-length.py` (with a self-test gate step), installed by
  `scripts/verify.sh`; `scripts/check-ci.py` refuses platform-selected gates and `tar -f NAME`.
