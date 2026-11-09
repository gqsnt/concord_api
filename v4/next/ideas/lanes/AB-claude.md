# Families A (errors) and B (results): idea lane, provider claude

Repository read: `work/wt/rv-p1` at `ba10189b` (read-only). Line ranges below are raw file lines at
that commit. The friction catalogue `work/inventory/FRICTION.md` does not exist yet. I read the
witnesses' own `FRICTION.md` instead (Slack, Kraken) and the sources of every witness, every
counterexample and every simple control named in the prompt.

## What the code shows (observed, before any design)

- **The same policy is written again and again.** It goes `prepare → receive → if non-2xx
  { document::<F, D>(LIMIT) → SDK container } → decode_with_meta → SDK Response`. It appears
  almost verbatim in Anthropic `lib.rs:155-166`, B2 `wire.rs:83-100`, DigitalOcean
  `lib.rs:114-136`, Frankfurter `lib.rs:79-90`, Rest Countries `lib.rs:144-158`, MediaWiki
  `policy.rs:13-38`, Kraken `policy.rs:148-167` and Cloudflare `policy.rs:123-135`.
  - 42 crates call `Received::document::<F, D>`: 41 SDKs plus quickstart.
  - 11 SDKs read error bodies by hand with `bytes_limited`.
- **31 SDKs keep the same container**: a head plus `document: Result<D, concord::Error>`.
- **6 SDKs reinvent the same body classification**: document / empty / not a document / too
  long / unreadable.
  - Registry `ErrorBody` (`error.rs:321-349`).
  - S3 `ErrorBody` (`error.rs:187-200`).
  - Salesforce `ErrorBodyState` (`error.rs:192-205`).
  - Stripe `DocumentProblem` (`send.rs:219-229`).
  - Frankfurter `ErrorBodyError` (`error.rs:24-63`).
  - B2 `Body` (`error.rs:236-246`).
- **Many send statements are stateless.** There are 41 `send` statements, and at least 27 are
  stateless types whose only job is result shape plus error document. All three simple controls
  already have one:
  - Nager `send KeepHead;`
  - PokeAPI `send KeepHead -> Result<Response<_>, Error>;`
  - Frankfurter `send SendPolicy;`
- **23 `Response<T>`-style structs and 12 `ResponseMeta` types.** About 750 of the 2,061
  `glue:result` lines are typed reads of provider response headers: request ids, rate limits,
  pagination counters, S3 object heads.
- **Errors also arrive inside 2xx bodies**, in 6 SDKs: Slack, Kraken, Cloudflare, MediaWiki,
  QuickBooks, ClickHouse. QuickBooks tunnels its in-2xx `Fault` through
  `CodecError::source_unredacted` and a downcast (`format.rs:60-67`, `policy.rs:174-182`).
- **The specific error variants matter to callers.** Tests across the SDKs match `Error::Provider`
  283 times and `Error::Http` 107 times, so every design must say what happens to those `match`es.
- **Classifier caveat.** `glue:error` (11,581 lines) also counts two kinds of code no A design
  removes:
  - local validation enums, such as Stripe `ValueError` and `FormError`;
  - provider code vocabularies, such as `ErrorCode` enums.

  The estimates below count only lines a design actually deletes. **measured** means counted
  from the files cited. **extrapolated** means scaled to the other SDKs with the same pattern.

Several designs share these core types. Each is introduced once and reused:

- `concord::Failure<D>` and `concord::Document<D>`, in A1.
- `concord::Response<T, R = ()>`, in B1.
- `concord::Envelope` and `concord::Refusal<E>`, in B2.

---

# Family A: errors

## A1. `Failure<D>`: the non-2xx answer as one core type (smaller than you think)

Fact it expresses: a non-2xx answer is the provider's head plus, if the body allows, its error
document. The status is authoritative whatever happened to the body.

Declaration (author writes): unchanged. This design is library-only. The policy and the error
module of a real witness (Anthropic) change.

Before, `examples/anthropic-messages-sdk/src/lib.rs:155-166` and `error.rs:227-323`:
```rust
let received = endpoint.prepare()?.receive().await?;
if !received.status().is_success() {
    let (meta, document) = received
        .document::<Json, error::ErrorDocument>(ERROR_BODY_LIMIT)
        .await;
    return Err(Error::Api(Box::new(ApiError::new(meta, document))));
}
// error.rs:227-323: struct ApiError { meta, document: Result<ErrorDocument, concord::Error> }
// + status() + provider_error() + document_error() + meta() + Display + Debug  (≈95 lines)
```
After:
```rust
let received = endpoint.prepare()?.receive().await?;
if !received.status().is_success() {
    return Err(Error::Api(Box::new(received.failure::<Json, ErrorDocument>(ERROR_BODY_LIMIT).await)));
}
// error.rs: `pub type ApiError = concord::Failure<ErrorDocument>;`
// Kept: request_id() (header, else document) and retry_after(). Both are Anthropic facts.
```
Core addition (about 120 lines in `concord-core`, next to `Received::document`):
```rust
pub struct Failure<D> { /* ResponseMeta, Document<D>, the bounded bytes */ }
impl<D> Failure<D> {
    pub fn status(&self) -> StatusCode;
    pub fn meta(&self) -> &ResponseMeta;
    pub fn operation(&self) -> Option<Operation>;
    pub fn document(&self) -> Option<&D>;
    pub fn body(&self) -> &Document<D>;
    pub fn bytes_unredacted(&self) -> Option<&Bytes>; // kept even when D decoded (Notion, Linear)
}
#[non_exhaustive]
pub enum Document<D> {
    Read(D),
    Empty,                 // HEAD, or zero bytes (S3, Registry, Salesforce distinguish it)
    Unrecognized(Error),   // Metadata (media type) or Decode (shape); kind() says which
    TooLong(u64),          // the limit; nothing more was read
    Broken(Error),         // Transport while reading
}
impl<T> Received<T> { pub async fn failure<F: Decode<D>, D>(self, limit: usize) -> Failure<D>; }
```
- `Display` reads like this: `POST Anthropic.messages.create: HTTP 529 (error document read)`.
- `Debug` prints the status, the operation, the header names and the variant of `body`. A `Read`
  document prints with `D`'s own `Debug`, which the SDK controls (family D).

Call site (caller writes), Frankfurter `README.md:96-105`:
```rust
// before
Err(Error::Provider(error)) => {
    eprintln!("HTTP {}", error.meta.status());
    match &error.document { Ok(p) => eprintln!("{}", p.message), Err(cause) => eprintln!("{cause}") }
}
// after (Frankfurter keeps its enum; the payload is now concord::Failure<Problem>)
Err(Error::Provider(failure)) => {
    eprintln!("HTTP {}", failure.status());
    match failure.document() { Some(p) => eprintln!("{}", p.message), None => eprintln!("{:?}", failure.body()) }
}
```
SDK code that disappears (≈780 raw lines measured in 11 witnesses, ≈1,300 extrapolated over the 31
SDKs with a meta + document container):

| SDK | File and lines | What it is | Raw lines removed |
|---|---|---|---|
| Anthropic | `error.rs:219-323` | — | ≈80 |
| Frankfurter | `error.rs:24-93` | `ErrorBodyError` and `ProviderError` | 70 |
| Kraken | `error.rs:94-115` | `HttpFailure`; `retry_after` kept | ≈12 |
| Cloudflare | `error.rs:70-152` | `ProviderError`; `attempts`, `has_code` and `retry_after` kept | ≈58 |
| B2 | `error.rs:236-339` | the inconsistent-status rule kept | ≈92 |
| QuickBooks | `error.rs:92-140` | — | ≈35 |
| Stripe | `send.rs:214-268` and `send.rs:480-498` | — | ≈65 |
| Registry | `error.rs:251-349` and `error.rs:747-783` | the 401 challenge and `has_code` kept | ≈115 |
| Salesforce | `error.rs:182-307` | — | ≈110 |
| S3 | `error.rs:187-305` | — | ≈100 |
| Slack | `error.rs:151-194` | `ErrorBody` | 44 |

Hardest counterexample: Cloudflare. Its refusal also counts `attempts`. It keeps a 10-line wrapper,
`ProviderError { failure: Failure<ErrorEnvelope>, attempts }`. B2 also needs extra code: a document
whose `status` contradicts the HTTP status (`error.rs:236-276`) becomes a 6-line SDK accessor over
`document()`.

Simple control (Nager): unchanged. Nager assumes no error document. Frankfurter (control): its
declaration is unchanged, and `error.rs:24-93` becomes `pub type ProviderError =
concord::Failure<Problem>;`.

Charter check:
- C1: no new word.
- C2: only in part. The restated container goes, but the document type is still named in Rust
  policy code, not declared.
- C3: the controls do not grow.
- C5: `Decode<D>`'s `on_unimplemented` fires at the `failure::<F, D>` call.
- C6: one sentence explains it.
- C7: head and raw bytes stay reachable.
- C8: one bounded read, no retry.
- C9: needs a canary test that `Failure`'s `Debug` and `Display` never contain a header value, a
  body byte or a document field, plus one wire test per `Document` variant.

New concepts: `concord::Failure<D>`, `concord::Document<D>`, `Received::failure`.

Breaking changes: none in Concord, because the change is additive. An adopting SDK changes the
public type of its provider variant: `error.document`, a `Result` field, becomes `failure.body()`
or `failure.document()`.

Why it might be wrong: it removes duplicated code but does not move the fact. The declaration still
cannot say what an error looks like, so each SDK keeps a policy that names `D`. The five-way
`Document` is also one more taxonomy to learn, compared with `Result<D, Error>` plus `kind()`.

## A2. `error D;`: the provider's error document as a declared fact

Fact it expresses: the non-2xx bodies of this API, or of this scope, are documents of type `D` in
format `F`.

Declaration (author writes), Backblaze B2.

Before, `examples/backblaze-b2-native-sdk/src/wire.rs:20-25` and `wire.rs:76-100`:
```rust
pub client B2Wire {
    base "https://api.backblazeb2.com";
    path "b2api/v4";
    format Json;
    limit crate::JSON_BODY_LIMIT;
    send B2Policy -> Result<(_, ResponseMeta), Error>;
// + B2Policy: `if !received.status().is_success() { return Err(provider_error(received).await) }`
// + provider_error(): document::<Json, WireErrorDocument>(ERROR_BODY_LIMIT) → ProviderError::new
```
After:
```rust
pub client B2Wire {
    base "https://api.backblazeb2.com";
    path "b2api/v4";
    format Json;
    limit crate::JSON_BODY_LIMIT;
    error WireErrorDocument;      // non-2xx bodies: decoded with `Json`, read up to 64 KiB
    send B2Policy -> Result<(_, ResponseMeta), Error>;
// B2Policy::send is one line: Ok(endpoint.prepare()?.receive().await?.decode_with_meta().await?)
```
Other witnesses:
- Stripe and Registry: `error ErrorDocument in IgnoreContentType<Json>;`.
- S3: `error ErrorDocument in S3ErrorXml;`. S3 wraps `xml::parse_error_document` as a 12-line
  `Decode` implementation.
- Notion: `error ProviderError;`.

Rules, one sentence each:
- `error` is an owner statement, and the nearest one wins, as for formats.
- Without `in F`, the document is decoded with the nearest `response`/`format` in effect where the
  `error` statement is written. An endpoint's own `response Binary;` does not change how its
  errors decode.
- A non-2xx body is read up to 64 KiB. 29 of the 41 SDK limits are 64 KiB and 12 are 16 KiB.
- `decode()` and the generated `send()` then return `concord::Error<D>`. Its `provider()` is
  `Some(&Failure<D>)` (A1), and the kind is still `ErrorKind::Status`.
- Without a send policy, the generated `send()` returns `Result<T, concord::Error<D>>`.

Call site (caller writes):
```rust
// before (DigitalOcean, any policy-free or policy SDK): match on the SDK's Provider variant
// after, client without a policy:
match api.droplets().list().send().await {
    Err(e) => if let Some(problem) = e.document() { eprintln!("{} {}", e.status().unwrap(), problem.id) },
    Ok(page) => { /* ... */ }
}
// after, SDK that keeps its enum: unchanged (`Err(Error::Provider(failure))`): its
// From<concord::Error<D>> routes `provider()` into the variant.
```
SDK code that disappears: everything listed for A1, plus the non-2xx branches and readers. That is
≈200 raw lines more measured, ≈1,650 in all extrapolated.

| SDK | File and lines | Raw lines |
|---|---|---|
| Anthropic | `lib.rs:157-163` | 7 |
| Frankfurter | `lib.rs:81-86` | 6 |
| Kraken | `policy.rs:150-155` | 6 |
| Cloudflare | `policy.rs:127-135` | 9 |
| B2 | `wire.rs:86-100` | 15 |
| Rest Countries | `lib.rs:149-156` | 8 |
| DigitalOcean | `lib.rs:124-130` | 7 |
| MediaWiki | `policy.rs:15-20` | 6 |
| Stripe | `send.rs:466-472` and `send.rs:480-498` | 26 |
| Notion | `client.rs:230-245` | 16 |
| Slack | `lib.rs:237-252` | 16 |
| Linear | `lib.rs:228-240` | 13 |
| Snowflake | `client.rs:191-227` | 37 (the handle-mismatch check stays, ≈8) |

Salesforce's asynchronous `SfError::from_concord` and `from_open` (`error.rs:46-70`, 25 lines)
also go: the body is already read.

Hardest counterexample: Discord sends `RateLimitBody` on 429 and `ErrorDocument` otherwise
(`limiter.rs:375-389`). One `error` type per owner covers it in either of two ways:
- `error DiscordError;` with `#[serde(untagged)]`, since the shapes differ (`retry_after`/`global`
  against `code`/`message`);
- a status-aware format, since the format's `context` sees the head.

Per-status syntax is deliberately not added. The real gap is Elasticsearch: on a GET, a 404 is
either a success (`found:false`) or an error document (`document.rs:224-271`). `error` alone
cannot tell them apart; see B4.

Simple control (Nager): unchanged. No `error` is declared, so generated code is identical and a
`Status` error keeps its response unread exactly as today. Frankfurter (control) gains one line,
`error Problem;`, and loses `lib.rs:81-86` and `error.rs:24-93`.

Charter check:
- C1: "errors are `Problem` documents" is a provider fact.
- C2: the fact is declared once, and the decision what to do with it stays in Rust.
- C3: opt-in, so the controls do not change.
- C4: the default format rule and the 64 KiB bound are conventions, visible in one cheatsheet row.
- C5: an error at the statement itself, such as "`Json` cannot decode `Problem`, the error
  document of this owner; write `error Problem in F;`".
- C6: one sentence explains it.
- C7: the body of a declared non-2xx is consumed, but bounded. The head and bytes remain in
  `Failure`; `into_response()` no longer returns it.
- C8: one bounded read, no retry.
- C9: needs wire tests (oversize, empty `HEAD`, wrong media type, broken body), a canary test and a
  compile-fail test for the format mismatch.

New concepts: the statement `error D [in F];`, `concord::Error<D = ()>`, and the accessors
`Error::provider()` and `Error::document()`, plus A1's types.

Breaking changes:
- For clients without `error`: none.
- For an adopting client:
  - `Status` errors no longer keep an unread response (`response()` and `into_response()`).
  - `Prepared`, `Received`, `Endpoint` and `Sends` gain a defaulted `D`, so a generic policy bound
    `E: Endpoint<T>` names `D` for that client.
  - The SDK error variants that callers match stay, if the SDK keeps its enum.

Why it might be wrong: the defaulted `D` spreads through `Prepared`, `Received`, `Endpoint` and
`Sends`. An erased store with `err.document::<D>()` avoids that (A4), but then the type no longer
says what can fail. A fixed 64 KiB bound is a convention. The 12 SDKs at 16 KiB accept it or keep a
policy.

## A3. Errors inside a 2xx: one failure type for both channels

Fact it expresses: a 2xx body can be the provider's refusal. Examples: Slack `ok:false`, Cloudflare
`success:false`, Kraken's non-empty `error`, MediaWiki's `error` member, QuickBooks `Fault`.

The mechanism is the B2 envelope (cross-family). The envelope's `open()` returns
`Refusal::Provider(d)`, and the declaration ties that `d` to the declared `error D`. A 200 refusal
and a 4xx then produce the same `concord::Error<D>`, with a `Failure<D>` keeping the real status.
The kind is new, `ErrorKind::Refused`, and `delivery()` is `Answered`.

Declaration (author writes), Kraken.

Before, `examples/kraken-spot-rest-sdk/src/lib.rs:32-47`:
```rust
pub client Kraken(policy: Policy) {
    base "https://api.kraken.com";
    path "0";
    response Json;
    limit RESPONSE_LIMIT;
    send policy;
    operations KrakenOperation;
    scope public {
        path "public";
        GET pairs(query pair?: String) -> Envelope<Pairs> { path "AssetPairs"; }
        GET ticker(query pair: String) -> Envelope<Tickers> { path "Ticker"; }
```
After:
```rust
pub client Kraken(policy: Policy) {
    base "https://api.kraken.com";
    path "0";
    response Json;
    limit RESPONSE_LIMIT;
    error ErrorDocument;          // `{"error": [..]}`: the body of a non-2xx (A2)
    envelope Envelope<_>;         // a 2xx body; its `error` list is the same ErrorDocument (B2)
    send policy -> Result<Response<_>, Error>;
    operations KrakenOperation;
    scope public {
        path "public";
        GET pairs(query pair?: String) -> Pairs { path "AssetPairs"; }
        GET ticker(query pair: String) -> Tickers { path "Ticker"; }
```
Rust, written once, replaces `policy.rs:156-166` and `error.rs:117-140`:
```rust
impl<T> concord::Envelope for Envelope<T> {
    type Value = T; type Rest = (); type Error = ErrorDocument;
    fn open(self) -> Result<(T, ()), Refusal<ErrorDocument>> {
        if !self.error.is_empty() { return Err(Refusal::Provider(ErrorDocument { error: self.error })); }
        self.result.map(|v| (v, ())).ok_or(Refusal::Malformed(CodecError::new("success without result")))
    }
}
```

Call site (caller writes), from the Kraken README failure table (`README.md:200-202`):
```rust
// before
Err(Error::Http(f)) => ..f.document..,         Err(Error::Api(f)) => ..f.classes()..
// after: one variant; the channel is the status
Err(Error::Provider(f)) if f.status().is_success() => ..f.document().unwrap().classes()..
Err(Error::Provider(f)) => ..f.document()..
```
The SDK can also keep two variants by routing on `err.kind()` (`Refused` against `Status`), so
callers do not break.

SDK code that disappears: ≈180 raw lines in 5 SDKs. Each envelope implementation adds about 12
back, so ≈130 net.

| SDK | Lines removed |
|---|---|
| Slack | `lib.rs:255-272`, `error.rs:274-288` (`into_provider`), `error.rs:138-149` (`EnvelopeProblem`: its four cases become `Refusal::Malformed` labels) |
| Kraken | `policy.rs:157-165`, `error.rs:117-140` (`ApiFailure`; `classify` moves onto `ErrorDocument`) |
| Cloudflare | `policy.rs:178-197` and `policy.rs:217-238` (the `Failure` and `MissingResult` arms) |
| MediaWiki | `policy.rs:22-33` |
| QuickBooks | `format.rs:21-32` (`FaultInSuccess`), `format.rs:60-67`, `policy.rs:170-182` (the downcast) |

Hardest counterexample:
- QuickBooks: the `Fault` replaces the entity, at the top level or inside `QueryResponse`; it does
  not wrap it. QuickBooks declares `envelope Faulted<_>;` with a custom `Deserialize`. Today's
  `Probe` (`format.rs:34-46`) moves into it, so this works but saves little.
- ClickHouse: the 2xx error is signalled by a header (`X-ClickHouse-Exception-Code`) in a
  streaming body (`client.rs:402-444`). This design does not cover it, and it stays SDK behavior.
- Linear: GraphQL errors next to partial data. `open()` is Rust, so Linear keeps `errors` as
  envelope rest when `data` exists and refuses only when `data` is null.

Simple control (Nager): unchanged, because there is no envelope.

Charter check:
- C1: "the body is this envelope and its error member is `D`" is a provider fact.
- C2: the rule "`ok:false` means refused" is Rust (`open`), not declaration control flow.
- C3: opt-in, so the controls do not change.
- C4: the rule is visible at the owner.
- C5: if `Envelope::Error` is not the declared `error` type, the compile error is at the
  `envelope` statement.
- C8: no extra request.
- C9: needs a test that a 200 refusal and a 4xx yield one `Failure<D>` type with the real status.

New concepts: `ErrorKind::Refused` (`ErrorKind` is `#[non_exhaustive]`, so adding it does not
break), and `Refusal<E>`, shared with B2.

Breaking changes: SDKs with separate 2xx and non-2xx variants (Kraken `Api`/`Http`, Slack
`Api`/`Http`/`RateLimited`) either merge them, which breaks callers, or route on `kind()`, which
does not.

Why it might be wrong: merging the two channels can hide a distinction a caller relied on. The
`Refusal::Malformed(CodecError)` path also turns SDK-specific envelope problems into static
labels; Slack's `EnvelopeProblem::MissingChannel` is an example.

## A4. Error documents carried by the format: no new statement

Fact it expresses: the API's wire format includes its error document. A format is already a
declared provider fact.

Declaration (author writes), Anthropic.

Before, `examples/anthropic-messages-sdk/src/lib.rs:85-86`:
```rust
        format Json;
        send SendPolicy -> Result<Response<_>, Error>;
```
After:
```rust
        format Problems<Json, ErrorDocument>;   // JSON both ways; non-2xx bodies are ErrorDocument
        send SendPolicy -> Result<Response<_>, Error>;
```
- `concord::Problems<F, D>` is a core format adapter. It delegates `Encode` and `Decode` to `F`,
  and it overrides a new provided method of `Decode`, so existing formats are unaffected:
  ```rust
  fn failure(head: &ResponseHead<'_>, body: Bytes) -> Option<Box<dyn Any + Send + Sync>> { None }
  ```
- Concord stores the decoded document erased. The caller reads it with
  `err.document::<ErrorDocument>()`.
- An SDK format that already knows its errors overrides `failure` itself, and so does a
  per-provider one. QuickBooks `QboJson` gains a sibling `refuse(&body)` for its in-2xx `Fault`,
  which replaces the downcast (`policy.rs:174-182`).

Call site (caller writes):
```rust
// before: Err(Error::Api(api)) => api.error_type()
// after (SDK enum unchanged; its From reads the erased document once):
Err(Error::Api(api)) => api.error_type()
// after (no SDK enum): err.document::<ErrorDocument>().map(|d| d.error.error_type())
```
SDK code that disappears: the same as A2, ≈1,650 extrapolated, minus ≈10 lines per SDK that
writes its own format instead of using `Problems`. QuickBooks also loses `format.rs:21-32` and
`policy.rs:170-182`, ≈25 lines. Nothing grows in declarations that already have a `format`
statement.

Hardest counterexample: Registry and S3. Their endpoints use several response formats:
- Registry: `Binary` for the manifest (`api.rs:67-71`), `-> stream` for blobs (`api.rs:83`), and
  `Json` for tags.
- S3: `ListXml`, `HeadFormat`, `Download` and `PutFormat`.

Each would need `Problems<Binary, ErrorDocument>` and so on, once per format statement: 4 to 5
wrappers in place of one fact. Otherwise the errors on those endpoints go undocumented.

Simple control (Nager): unchanged (`format Json;`).

Charter check:
- C1: no new word.
- C2: the fact sits in a declared format type.
- C3: the controls do not change.
- C4: weak. With `Problems<..>` the document is visible, but an SDK format that overrides `failure`
  hides it in Rust.
- C5: a wrong `D` shows up only at the `document::<D>()` access, as `None`. That is not where the
  mistake was written.
- C7: unaffected; leaving stays possible.

New concepts: `concord::Problems<F, D>`, `Decode::failure` (a provided method), and
`Error::document::<D>()`.

Breaking changes: none for existing formats, since the method is provided. The same unread-body
change as A2 applies to clients that adopt it.

Why it might be wrong: the type of `send()` no longer says what can fail (the caller promise), and
a wrong `D` fails silently at run time. It also ties the error document to the success format,
which is false for every SDK with several response formats.

## A5. `#[derive(concord::Error)]`: the SDK keeps its enum, loses the boilerplate

Fact it expresses: none about the provider. It removes the restated plumbing around Concord's
error, and so keeps every variant callers `match` on.

Declaration (author writes): unchanged. The SDK error enum changes. Kraken.

Before, `examples/kraken-spot-rest-sdk/src/error.rs:164-219`: an enum with 6 variants, `status()`,
two `From` impls, a 6-arm `Display` and `std::error::Error`. That is 56 lines.

After:
```rust
#[derive(Debug, concord::Error)]
#[non_exhaustive]
pub enum Error {
    #[display("Kraken local refusal: {0:?}")] Local(LocalError),          // From<LocalError>
    #[concord] Concord(concord::Error),                                    // From<concord::Error>, Display = Concord's
    #[provider] Http(Box<concord::Failure<ErrorDocument>>),                // status()/head() read it
    #[display("Kraken successful envelope is missing its result")] MissingResult(Box<ResponseMeta>),
    #[display("Kraken trade-history page refused: {0.problem:?}")] Page(Box<PageFailure>),
}
```
The derive generates:
- `From` for each single-field variant (`#[from(skip)]` opts out);
- `Display`: delegation for `#[concord]` and `#[provider]`, the `#[display]` text otherwise;
- `std::error::Error` with no `source()`, as Concord's policy is;
- accessors that walk the variants: `status()`, `head()`, `operation()` and `delivery()`. Each
  takes it from whichever field is a `concord::Error`, a `Failure` or a `ResponseMeta`.

Call site (caller writes): unchanged. `Err(Error::Http(f))`, `e.status()` and `e.to_string()` all
still compile.

SDK code that disappears: ≈400 raw lines measured in 9 SDKs. Extrapolated, ≈1,500 to 2,000 over the
45 SDKs with an `error.rs`; this is the most uncertain number here.

| SDK | Lines | Raw lines removed |
|---|---|---|
| Kraken | `error.rs:181-219` | ≈30 |
| Frankfurter | `error.rs:113-159` | ≈42 |
| Anthropic | `error.rs:44-100` | ≈35 (`request_id` kept) |
| Cloudflare | `error.rs:238-315` | ≈40 |
| Slack | `error.rs:238-272` | ≈28 |
| Notion | `error.rs:132-218` | ≈50 |
| Stripe | `send.rs:330-453` | ≈70 (the custom `Api` `Display` kept) |
| Registry | `error.rs:54-183` | ≈80 |
| Nager | `error.rs:34-86` | ≈35 (`kind()` kept) |

Hardest counterexample: Stripe's `Display` for `Api` prints the type, the code, the rate-limit
reason and the request id, but only when each is present (`send.rs:398-421`). A format string
cannot express that, so Stripe keeps that one arm by hand (`#[display(with = fmt_api)]`).

Simple control (Nager): the declaration is unchanged. `error.rs` shrinks from 86 to about 50 lines,
and `kind()`, the HTTP 400 rule, stays because it is SDK behavior.

Charter check:
- C1 and C2: not a declaration feature. It removes restatement and states no fact.
- C3: the controls do not change.
- C5: derive errors point at the variant attribute.
- C6: this is the risk. It is `thiserror` plus Concord accessors, and the attribute set must stay
  tiny.
- C9: needs a canary test over a derived enum, so that no variant prints a header or body.

New concepts: the derive `concord::Error` and the attributes `#[concord]`, `#[provider]`,
`#[display]` and `#[from(skip)]`.

Breaking changes: none, since it is opt-in per SDK and keeps the public enum.

Why it might be wrong: this is a framework-shaped convenience, not a fact, and `thiserror` already
covers most of it. A plain recommendation plus three free functions (`concord::status_of(&e)` and
so on) might buy 80% of it for no API surface. It also locks the SDK error design into one pattern.

---

# Family B: results

## B1. `concord::Response<T>` in core (smaller than you think)

Fact it expresses: every answer has a head. The value and the head it came with form one core
type, not 23 copies.

Declaration (author writes): unchanged. The PokeAPI control is the witness.

Before, `examples/pokeapi-sdk/src/lib.rs:81` and `lib.rs:101-108`:
```rust
        send KeepHead -> Result<Response<_>, Error>;
// + pub struct Response<T> { pub value: T, pub meta: ResponseMeta }   (lib.rs:101-108)
```
After:
```rust
        send KeepHead -> Result<concord::Response<_>, Error>;
```
The core type replaces the SDK struct:
```rust
#[non_exhaustive]
pub struct Response<T, R = ()> { pub value: T, pub meta: ResponseMeta, pub rest: R }  // `rest`: B2
impl<T> From<(T, ResponseMeta)> for Response<T> { .. }   // `received.decode_with_meta().await?.into()`
impl<T, R> Response<T, R> { pub fn new(..); pub fn into_value(self) -> T; pub fn map(..) }
// Debug: `meta` and `value: <withheld>`. Concord never prints body content (Kraken, Notion,
// Strava, Mailgun already withhold it by hand).
```

Call site (caller writes): unchanged (`r.value`, `r.meta.status()`). For the SDKs whose field had
another name (Nager `holidays`, Frankfurter `rate`), the field becomes `value` if they adopt.

SDK code that disappears: ≈300 to 350 lines across 23 SDKs. This covers the wrapper structs and the
`Response { value, meta }` assembly lines:

| SDK | Lines |
|---|---|
| Slack | `lib.rs:29-40` |
| Kraken | `policy.rs:14-28` |
| Stripe | `send.rs:135-142` |
| Notion | `client.rs:9-22` |
| Anthropic | `lib.rs:118-127` (`request_id` goes to B3) |
| Frankfurter | `lib.rs:56-63` |
| Nager | `lib.rs:41-48` |
| PokeAPI | `lib.rs:101-108` |
| Linear | `lib.rs:157-160` |
| GitLab | `meta.rs:15-30` |

The same wrapper also goes in DigitalOcean, Dropbox, Airtable, USGS, Mailgun, Intercom, Strava,
Twilio, Google Drive, Adyen, Open-Meteo, Snowflake and Sentry.

Hardest counterexample: PokeAPI. Its meta keeps the final URL (`lib.rs:90-99`, `lib.rs:169-176`),
which `concord::ResponseMeta` deliberately omits because a URL is unredacted. PokeAPI keeps its own
meta and adopts only for the structure (`Response<T, ()>` with a URL elsewhere), or stays as it is.

Simple control (Nager): the declaration is unchanged, and the `HolidayResponse` (8 lines) goes. Its
callers see `.value` in place of `.holidays`, but only if Nager adopts.

Charter check:
- C1: no new word.
- C2: neutral; this is a library type.
- C3: the controls do not change.
- C6: one sentence explains it.
- C7: unaffected; leaving stays possible.
- C9: needs a canary test that `Response`'s `Debug` never prints the value or a header value.

New concepts: `concord::Response<T, R = ()>`.

Breaking changes: none in Concord, because the change is additive. An adopting SDK renames a public
type.

Why it might be wrong: it removes about 15% of `glue:result`. The other 85% is provider header
facts (B3), envelopes (B2) and readers (out of scope). It also invites one more default
(`Debug` withholding the value) that some SDKs, Cloudflare for one, did not choose.

## B2. `envelope E<_>;`: the success envelope as a declared fact, `_` is the value

Fact it expresses: on this API, a 2xx body is `E<value>`. The value lives inside, the envelope may
refuse (A3), and it carries other members the caller needs: warnings, a cursor, diagnostics.

Declaration (author writes), Cloudflare.

Before, `examples/cloudflare-zones-sdk/src/lib.rs:68-72` and `lib.rs:83-105`:
```rust
        base "https://api.cloudflare.com";
        path "client/v4";
        format Json;
        limit SUCCESS_BODY_LIMIT;
        send policy;
        ...
            GET list(/* .. */) -> Paged<Zone>;
        ...
            GET get() -> Single<Zone>;
```
After:
```rust
        base "https://api.cloudflare.com";
        path "client/v4";
        format Json;
        limit SUCCESS_BODY_LIMIT;
        error ErrorEnvelope;                        // A2: non-2xx bodies
        envelope Envelope<_>;                       // `{success, errors, messages, result, result_info}`
        send policy -> Result<Response<_, Rest>, Error>;
        ...
            GET list(/* .. */) -> Vec<Zone>;
        ...
            GET get() -> Zone;
```
The rules:
- The wire type is the template with `_` replaced by the declared output, here
  `Envelope<Vec<Zone>>`. The macro already has this substitution for `send` templates
  (`emit.rs:2090`, `with_output`).
- It is decoded with the nearest response format. `concord::Envelope` (Rust) then opens it:
  ```rust
  pub trait Envelope { type Value; type Rest; type Error;
      fn open(self) -> Result<(Self::Value, Self::Rest), Refusal<Self::Error>>; }
  pub enum Refusal<E> { Provider(E) /* → A3 */, Malformed(CodecError) /* → ErrorKind::Decode */ }
  ```
- `Received::open()` returns `Result<Response<Value, Rest>, Error<D>>`, so a policy calls it in
  one line.
- Without a policy, the generated `send()` returns exactly that.
- `prepare()?.send()` still returns the raw envelope, as Cloudflare documents today
  (`wire.rs:3-8`).
- `-> stream` endpoints and endpoints without `->` are not enveloped.
- `_` in `send P -> Template` is now the public value, which fixes Kraken's F3, where `_` stood
  for the whole envelope. It also makes Kraken's and Cloudflare's `send()` rustdoc name a concrete
  type (seed B4).

Cloudflare's two envelopes, `Single` and `Paged` with two `Deserialize` impls (`wire.rs:89-176`),
become one `Envelope<V>`. It keeps `result` as `RawValue`, parses it lazily in `open()` (success is
still read first), and has one `Rest { errors, messages, result_info }`.

Call site (caller writes), Slack:
```rust
// before (crate::Client wrapper, lib.rs:210-232)
let r = slack.conversation_info("C1", ConversationOptions::default()).await?;
r.value; r.warning; r.response_metadata;
// after (the generated client can be public: its send() is Slack-aware)
let r = api.conversations_info("C1".into()).send().await?;
r.value.channel; r.rest.warning; r.rest.response_metadata; r.meta.status();
```

SDK code that disappears: ≈300 net raw lines in 6 SDKs. It also unlocks construction and
forwarding glue in families C and G: Slack's `Client` wrapper (`lib.rs:118-139`) exists only
because the generated `send()` was not Slack-aware (Slack `FRICTION.md` item 2).

| SDK | Lines removed | What replaces them | Net |
|---|---|---|---|
| Slack | `lib.rs:162-174`, `lib.rs:222-231`, `lib.rs:235-279`; `error.rs:274-288` | a 15-line `open` | ≈80 |
| Cloudflare | `wire.rs:89-176` (→ ≈40), `policy.rs:31-45`, `policy.rs:156-240` (two impls → one, ≈25) | — | ≈120 |
| Kraken | `policy.rs:156-166`; `trades.rs` page projection | — | ≈15 |
| MediaWiki | `policy.rs:21-38`, `models.rs:116-125` | — | ≈20 |
| Linear | `lib.rs:157-160`, `lib.rs:225-244` | — | ≈25 |
| QuickBooks | `format.rs:1-70` | a ≈25-line `Faulted<_>` envelope | ≈45 |

Hardest counterexample: Cloudflare, for two reasons:
- If it kept two envelope shapes, `dns_records` would hold one `Single` endpoint (`create`) and one
  `Paged` endpoint (`list`), and need an endpoint-level `envelope Paged<_>;`, one more line. The
  unified envelope avoids that.
- A refusal whose `result` has the wrong shape must still report Cloudflare's errors
  (`wire.rs:185-198`). This stays in its `Deserialize`/`open`, which is model code.

MediaWiki's continuation and Cloudflare's `result_info` are the "envelope carries paging the caller
needs" counterexample. They live in `rest`, never dropped.

Simple control (Nager): unchanged, because there is no envelope. PokeAPI and Frankfurter are also
unchanged.

Charter check:
- C1: "the body is `E<value>`" is a provider fact.
- C2: the split is Rust (`open`), with no field-path language in the declaration.
- C3: opt-in, so the controls do not change.
- C4: `_` is written at the owner, and each endpoint's `->` is the value its docs describe.
- C5: if `<E<T> as Envelope>::Value` is not `T`, or `Error` is not the declared `error`, the error
  is at the `envelope` statement.
- C6: one sentence explains it: "`_` is the value; the body is `E<value>`, opened by
  `concord::Envelope`".
- C7: leaving stays possible through `prepare()?.send()`, which returns the raw envelope.
- C9: needs a wire test that `prepare()?.send()` still returns the raw envelope, and a
  compile-fail test for the value mismatch.

New concepts: the statement `envelope E<_>;`, `concord::Envelope`, `concord::Refusal`,
`Received::open`, and `Response::rest`.

Breaking changes:
- Endpoint outputs of adopters change from `Envelope<Pairs>` to `Pairs`, and the `Sends<O>` bounds
  change with them (Cloudflare `pagination.rs:81`).
- Native callers of `prepare()?.send()` keep the envelope.

Why it might be wrong: the `Rest` type is one shared shape per envelope. An API whose siblings
differ per endpoint gets `Option` fields, or needs endpoint-level `envelope` lines that grow the
DSL. A `Malformed(CodecError)` also loses the SDK's typed "missing result" variants, which callers
may match today.

## B3. `meta Name(header(..) field?: T, ..);`: declared response header fields

Fact it expresses: this provider answers with these headers, with these meanings and types. Examples
are a request id, rate-limit counters, pagination totals, a content digest, an API version. The
charter lists rate-limit header names as a declaration fact.

Declaration (author writes), Stripe. Today the fact is restated in Rust at
`examples/stripe-sdk/src/send.rs:79-133`: `ResponseMeta` and a 20-line `from_head` reading
`request-id`, `idempotent-replayed`, `stripe-should-retry`, `stripe-rate-limited-reason` and
`stripe-version`.

After, inside `pub client Stripe(..) { .. }`, next to `response Json;`:
```rust
        /// What Stripe says about every answer besides its body.
        meta StripeMeta(
            header("request-id") request_id?: String,
            header("idempotent-replayed") idempotent_replayed?: bool,
            header("stripe-should-retry") should_retry?: bool,
            header("stripe-rate-limited-reason") rate_limited_reason?: RateLimitedReason,
            header("stripe-version") stripe_version?: String,
        );
```
- The same placement syntax as inputs, mirrored. A `params Name(..)` precedent already names a
  group by the author.
- It generates `pub trait StripeMeta`, implemented for `concord::ResponseMeta`, `ResponseHead<'_>`,
  `Failure<D>` and `Error<D>`. So the same accessor works before the body is read, on success and on
  every failure.
- Each field is one accessor that parses lazily and strictly, `fn request_id(&self) ->
  Result<Option<String>, FieldError>`. The rules are those of `concord::field`: absent is `Ok(None)`;
  repeated or invalid is `FieldError`. A field written without `?` is `Result<T, FieldError>`.
- `T: FromStr`. Integers parse strictly, so `+5` is refused.
- `each` reads every field line, which gives Cloudflare's multi-line `Ratelimit`.
- `#[sensitive]` withholds a field from Concord's output (family D/J).
- `HEAD head_object(..) -> ObjectHead;`, where `ObjectHead` is a declared `meta`, would decode from
  the head alone (S3 `HeadFormat`). Listed as an extension.

Call site (caller writes):
```rust
// before (Stripe)
if let Some(id) = &response.meta.request_id { .. }
if error.meta().is_some_and(ResponseMeta::is_rate_limited) { .. }
// after (`use stripe::StripeMeta;`)
if let Ok(Some(id)) = response.meta.request_id() { .. }
if let Some(f) = error.provider() { f.rate_limited_reason()?; }
```

SDK code that disappears: ≈450 to 500 net raw lines over 12 SDKs. It also gives E/J (limiters,
logging) typed rate-limit headers for free.

| SDK | Lines | What stays | Net |
|---|---|---|---|
| Stripe | `send.rs:79-133` | 8 for `FromStr` of `RateLimitedReason` | ≈40 |
| S3 | `error.rs:130-185` | — | ≈50 |
| Slack | `error.rs:9-56` | `RetryAfter` as `FromStr` | ≈30 |
| Registry | `meta.rs:19-102` | the digest and length rules | ≈55 |
| DigitalOcean | `lib.rs:155-184` | — | ≈25 |
| Cloudflare | `error.rs:13-68` | the structured-field parser as `FromStr` | ≈40 |
| GitLab | `meta.rs:32-150` | `Link` parsing | ≈60 |
| GitHub | `meta.rs:62-162` | — | ≈60 |
| Anthropic | `error.rs:102-109`, `lib.rs:129-134` | — | ≈10 |
| QuickBooks | `error.rs:129-136` (`intuit_tid`) | — | ≈6 |
| Dropbox | `error.rs:7-42` | — | ≈25 |
| ClickHouse | `meta.rs:30-126` | — | ≈50 |

Hardest counterexample: GitLab and GitHub `Link`. Relation parsing resolves relative targets
against the response URL (GitLab `meta.rs:74`, `meta.rs:100`: `links: link::parse(.., url)`). A header value
alone does not determine the result, so it is not `FromStr` and stays hand-written. S3 `x-amz-meta-*`
is a family of header names, not one name, and is not declarable either; S3 keeps
`xml::metadata_headers`.

Simple control (Nager): unchanged, because Nager declares no response fields.

Charter check:
- C1: header names and types are provider facts.
- C2: the parse rule is `FromStr` in Rust.
- C3: opt-in, so the controls do not change.
- C4: each field is written where it is true, and the nearest owner applies.
- C5: if `T` is not `FromStr`, the error is at the field.
- C6: one sentence explains it.
- C8: lazy parse, no hidden work.
- C9: needs a canary test that `Debug` of `ResponseMeta` still prints names only, plus strict-parse
  tests (repeated, `+5`, overflow).

New concepts: the owner statement `meta Name(..)`, one generated trait per statement, and output
placements of the `header(..)` and `each` kind.

Breaking changes: none in Concord, because the change is additive. Adopting SDKs replace public meta
struct fields, `meta.request_id`, with methods, `meta.request_id()`.

Why it might be wrong: callers must import the generated trait. The H09c baseline misses were all
import-path errors. `Result<Option<T>, FieldError>` accessors are also heavier to call than the
`Option<String>` fields SDKs expose today, and silent-dropping SDKs (Stripe, S3) did that on
purpose.

## B4. `status 200, 202;`: the statuses an endpoint succeeds with

Fact it expresses: this operation answers with exactly these statuses. Examples are Snowflake 202
(still running) and 200 (done), Registry 201/202/204, S3 `HeadObject` 200/206/304, and Drive 308
(upload incomplete).

Declaration (author writes), Snowflake.

Before, `examples/snowflake-sdk/src/api.rs:45-51`, plus the statuses threaded through the SDK at
`client.rs:169`, `client.rs:228-233` and `client.rs:295`:
```rust
        POST submit(query("requestId") request_id: RequestId, body statement: Statement) -> StatementBody {
            query { "async" = "true"; }
            response StatementJson;
        }
        scope statement(handle: StatementHandle) {
            path "{handle}";
            GET status() -> StatementBody { response StatementJson; }
// SDK: self.execute(prepared, &[200, 202], ..) and `if !statuses.contains(..) { UnexpectedStatus }`
```
After:
```rust
        POST submit(query("requestId") request_id: RequestId, body statement: Statement) -> StatementBody {
            query { "async" = "true"; }
            status 200, 202;
            response StatementJson;
        }
        scope statement(handle: StatementHandle) {
            path "{handle}";
            GET status() -> StatementBody { status 200, 202; response StatementJson; }
```
Registry gains one line on each endpoint whose status the spec fixes:
- `PUT put_manifest(..) { path ..; request Encoded; status 201; }`
- `POST start_upload(..) { ..; status 202; }`

Rules:
- Without the statement, any 2xx is a success, as today (C3).
- With it, only the listed statuses reach the format. The format's `context` already sees the
  status (`api.rs:20-30`).
- Any other status, another 2xx included, is `ErrorKind::Status`. Under A2 its body is read as the
  error document.
- A listed 3xx (304, 308) counts as an answer. reqwest does not follow 304 or 308 without a
  `Location`.

Call site (caller writes): unchanged. Snowflake's `Submission` and Registry's `put_manifest`
callers see the same results. An unexpected 2xx is now Concord's `Status` error, not Registry's
`ProtocolProblem::UnexpectedStatus`.

SDK code that disappears: ≈60 to 90 raw lines in 4 SDKs.

| SDK | Lines removed |
|---|---|
| Registry | `error.rs:785-806` (`expect_status`); 9 call sites (`client.rs:139`, `blob.rs:63,125`, `upload.rs:98,239,285,342`, `manifest.rs:277,368`) and the `drain` calls after them |
| Snowflake | `client.rs:228-233`; the `statuses` parameter and three call-site arrays |
| Google Drive | the 308 handling (`upload.rs:59-71`) |
| S3 | `client.rs:398-401`, if 304 becomes an output rather than an error (S3's choice) |

Hardest counterexample: Elasticsearch GET 404 (`document.rs:224-271`). The same status is a success
(`found:false`) or an error document (a missing index), depending on the body. A status list cannot
decide that, so Elasticsearch keeps its `Get404` decoder. Snowflake keeps `StatementJson`
(`api.rs:13-33`), because 200 and 202 have different bodies; B4 states the fact and does not
replace the dispatch.

Simple control (Nager): unchanged.

Charter check:
- C1: a status code is a provider fact.
- C2: what a 202 means stays in the format and SDK, in Rust.
- C3: absent means any 2xx, as now.
- C4: the statuses are written on the endpoint.
- C5: a non-numeric or out-of-range status is an error at the statement.
- C6: one sentence explains it.
- C9: needs wire tests for an unlisted 2xx and for a listed 304.

New concepts: the statement `status N, ..;` on endpoints and scopes.

Breaking changes: none, because the statement is opt-in.

Why it might be wrong: it is small, and the status-to-body dispatch still needs a custom format
(Snowflake, GitHub's conditional `304`, `meta.rs:195-216`). A typed per-status output, an enum the
SDK derives with `#[status(202)]`, would remove more but adds a sum-type concept to the language.
That is also one line per endpoint, against the "DSL lines per endpoint do not grow" goal; it is
paid only where the provider fixes a status.

## B5. The head always comes back: generated `send()` returns `concord::Response<T>`

Fact it expresses: every HTTP answer has a head, so the type says so. This is not an SDK option:
the generated `send()` returns it.

Declaration (author writes), Nager (the simple control).

Before, `examples/nager-holidays-sdk/src/lib.rs:22-25` plus `lib.rs:41-71` (`HolidayResponse` and
the `KeepHead` policy, 31 lines):
```rust
    pub client NagerHolidays() {
        base "https://nagerholidays.com";
        send KeepHead;
        format Json;
```
After:
```rust
    pub client NagerHolidays() {
        base "https://nagerholidays.com";
        format Json;
```
- Without a policy, the generated `send()` is `prepare()?.receive().await?.decode_response()` and
  returns `Result<concord::Response<T>, concord::Error>`.
- With A2 it returns `Error<D>`; with B2, `Response<T, Rest>`.
- `Prepared::send` (the native path) keeps returning `T`.

Call site (caller writes), Nager `README.md:48-50`:
```rust
// before
let response = api.holidays("AT", 2026).send().await?;
println!("HTTP {}", response.meta.status());
for holiday in &response.holidays { .. }
// after
let response = api.holidays("AT", 2026).send().await?;
println!("HTTP {}", response.meta.status());
for holiday in &response.value { .. }
```
SDK code that disappears: with A2, ≈650 raw lines. About 20 stateless policies whose only job is
head plus document go entirely, each ≈15 to 30 lines with docs:

| SDK | Lines |
|---|---|
| Nager | `lib.rs:41-71` |
| PokeAPI | `lib.rs:188-205`, `lib.rs:273-288` (`finish` is kept only for the URL) |
| Frankfurter | `lib.rs:56-91` |
| Anthropic | `lib.rs:118-167` |
| DigitalOcean | `lib.rs:114-145` |
| B2 | `wire.rs:76-100` |
| Rest Countries | `lib.rs:144-159` |

The pattern repeats in Open Library, NASA, Postmark, Square, ntfy, World Bank, USGS, Intercom,
Dropbox, Mailgun and others. B1's wrapper structs go too. About 20 declarations lose their `send`
line.

Hardest counterexample: the 16 SDKs whose callers use a bare-`T` `send()`, among them GitHub,
Kubernetes, Shopify and Microsoft Graph. Every call site gains `.value`. Generic code over
`Endpoint<T>` sees two meanings of "send": the generated `send()` returns `Response<T>`, while
`Prepared::send` returns `T`.

Simple control (Nager): the declaration loses one line, and 31 lines of SDK code go. The caller
reads `.value`, where it read `.holidays`. The quickstart's first example also gains a `.value`.

Charter check:
- C1: removes a statement, the `send` that only shaped the result.
- C3: the declaration does not grow, but the result shape of the simplest call changes, and a new
  user meets `Response` in their first call. That is the cost.
- C4: the rule is visible, and deterministic.
- C6: one sentence explains it.
- C7: `prepare()?.send()` keeps returning `T`, and leaving stays possible.

New concepts: none beyond B1, though `Response` becomes unavoidable.

Breaking changes: every caller of a policy-free generated `send()` in the 16 SDKs above, the
CHEATSHEET example and the quickstart.

Why it might be wrong: most simple users want the value, not the head. B5 makes them pay `.value`
so that SDK authors stop writing `KeepHead`. Two terminal shapes, the generated `send()` and the
native `Prepared::send`, is the kind of rule that needs a paragraph (C6).

---

## How the designs compose (lane view, not a decision)

- **Library first, no language change: A1 + B1.** Each removes the most-repeated block of its
  family. Together they are about 1,600 lines extrapolated, with zero DSL change and no breakage.
  Everything below builds on their types.
- **The declared facts: A2 + B2.** A3 is the shared mechanism, and together they cover every
  error/envelope witness except ClickHouse's header-signalled in-stream error and Elasticsearch's
  ambiguous 404. They also unlock the private-declaration wrappers in Slack, Notion and Registry,
  which removes glue in families C and G.
- **B3 is independent.** It also gives E (rate-limit headers for limiters) and J (request ids for
  logging) their facts.
- **B4 is small and independent.**
- **Contrasts kept on purpose.**
  - A4 is the no-new-word alternative to A2; it loses type honesty and fits multi-format SDKs badly.
  - A5 helps with zero facts.
  - B5 is the only design that deletes `KeepHead`-style policies, and it does so at the cost of the
    simplest caller.
- **Rejected.**
  - Per-status error statements (`error 429 => X;`): Discord's two shapes are separable by serde,
    and a status-aware format already sees the head.
  - Declared field paths (`unwrap "result"`): needs a JSON-path language, and Cloudflare's
    success-first lazy parse breaks it.
  - Provider error-code vocabularies in the declaration: an OpenAPI dump; they stay model code.
