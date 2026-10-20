# Responses, statuses and expected absence

The declared output type is exactly what ordinary `.await` returns. A response
representation turns an accepted response into that output through a
core-owned driver; statuses, absence and rejected-status bodies have their own
explicit rules (§9.4-§9.6, §10).

## Terminals

<!-- concord-source: crates/concord-examples/src/request_response/acquisition.rs#response_acquisition_contract -->
<!-- concord-dsl -->
```rust
api! {
    client ResponseApi {
        base "http://responses.example.test/";
        timeout 30s;

        GET message() -> String via Text {
            expose accepted_bytes;
        }

        GET optional_message() -> Option<String> via OptionalText {
            status [200..=299, 404];
        }
    }
}
```

`.await` returns the decoded value:

<!-- concord-source: crates/concord-examples/src/request_response/acquisition.rs#decoded_response -->
```rust
/// Returns only the decoded application value.
pub async fn decoded(client: &ResponseApi) -> Result<String, ApiError> {
    client.message().await
}
```

`.response()` keeps the same normal output semantics and adds the accepted
response facts:

<!-- concord-source: crates/concord-examples/src/request_response/acquisition.rs#metadata_response -->
```rust
/// Returns the decoded value together with accepted response metadata.
pub async fn with_metadata(client: &ResponseApi) -> Result<ApiResponse<String>, ApiError> {
    client.message().response().await
}
```

`ApiResponse<O>` exposes `status()`, `version()`, `headers()`, `url()`,
`metadata()`, `body_disposition()`, `value()` and `into_value()`. The URL is credential-sanitized but
may contain ordinary public data, and the actual headers may be sensitive:
these accessors are explicit application-data access, not safe automatic
logging. `Debug` omits raw values. `metadata()` is an `ExecutionMeta` snapshot
(endpoint ID, method, page index, attempts started, policy retries, whether
authentication was recovered, and provider-acquisition facts when present).
`body_disposition()` reports this response's complete, open, or unread body outcome.

`.accepted_bytes()` exists only with `expose accepted_bytes;`:

<!-- concord-source: crates/concord-examples/src/request_response/acquisition.rs#accepted_bytes -->
```rust
/// Preserves declared acceptance and `Accept` while returning bounded bytes.
pub async fn as_bytes(client: &ResponseApi) -> Result<ApiResponse<Bytes>, ApiError> {
    client.message().accepted_bytes().await
}
```

It keeps the origin, auth, rate, effect, status and deadline rules and the
`Accept` negotiation, but bypasses representation-specific validation and
conversion and returns the complete bounded body. It is valid with buffered or
discard normal pairs, cannot bypass a challenge and is never added by a generic
caller trait.

## Accepted statuses

The default accepted set is 200-299 (`status success`). `status 201;`,
`status [200, 204];` or `status [200..=201, 204];` replace it. Codes are
validated (100-599, increasing, disjoint), no final accepted status is below
200, and accepted statuses may not overlap an effective auth challenge. A status
outside the set is a rejected status (below).

## ETag revalidation

Declare `revalidate etag;` once in an endpoint body to opt in. It is not a
client or scope default, and `etag` is the only mode. The endpoint must use
`GET`/`HEAD` with their default safe effect, or explicitly declare `effect safe;`.
Its response representation must be buffered, and it cannot paginate or use a
streaming or discard driver.

A complete 200 response carrying a strong or weak `ETag` populates the client's
bounded LRU store with the tag, raw body and response metadata needed to decode
it again. `Cache-Control: no-store` prevents storage. A 200 without an ETag or
with `no-store` removes any earlier entry for that request. A body larger than
the configured byte bound is delivered without being retained.

Every later matching call performs a managed network operation, sending
`If-None-Match` with the exact stored tag. A 304 reuses the retained response
bytes through the endpoint's normal buffered validation and conversion
contract, with status 200, even when 304 is outside the accepted status set.
A 304 without a matching retained response is a contract failure, including
when storage is disabled or the entry was evicted while the call was in flight.

The key includes the endpoint, method, final URL including query, and all
request headers after authentication, with hashed authentication material.
Different credentials, header values and query values cannot share entries.
Raw credentials and sensitive request values are not retained in the key;
cache diagnostics never expose bodies, tags or key material. The store is
shared by a built client's endpoints and clones, and evicts least recently
used entries until both its entry and body-byte limits hold. Every call
revalidates; freshness lifetimes do not skip network operations.

Configure it on the client builder with
`configure_runtime(|runtime| runtime.revalidation_cache(entries, bytes))`.
The defaults are 256 entries and 8 MiB; zero in either limit disables storage.
See [Runtime configuration](runtime.md#runtimeconfigbuilder).

## Response representations

```text
pub trait ResponseRepresentation<Output>: Send + Sync + 'static
where Output: Send + 'static {
    type Driver: ResponseDriver;
    const ACCEPT: Option<&'static str> = None;
    fn validate(head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> { Ok(()) }
    fn from_body(body: <Self::Driver as ResponseDriver>::Body,
                 head: &ResponseHead<'_>) -> Result<Output, ResponseBodyError>;
}
```

`ResponseDriver` is sealed in core. Its modes are:

| Driver | `Body` | Meaning |
|---|---|---|
| `Buffered` | `Bytes` | complete bounded acquisition (16 MiB default) before conversion |
| `Streaming` | `ByteStream` | a managed stream with its own finite lease ([Streams](streams.md)) |
| `DiscardDriver` | `()` | drops the accepted body **without polling it**; this is not draining |

`ACCEPT` is static negotiation metadata: inspection reads it without running
user code, and it is validated once as an HTTP `Accept` value before any
request exists: for a generated endpoint an invalid constant (for example one
containing a line break) is a compile error of the `api!` expansion; a runtime
endpoint rejects it when constructed. `validate` runs on the
accepted head before body acquisition; `from_body` runs after the driver
returns the body. `ResponseHead` carries the accepted status, method, endpoint
ID, HTTP version, credential-sanitized URL, actual headers, known content length
and safe metadata, never a native response or client.

<!-- concord-source: crates/concord-examples/src/request_response/specifications.rs#response_body_spec_extension -->
```rust
/// A buffered representation returning the accepted body length.
pub struct ExampleBufferedSpec;

impl ResponseRepresentation<usize> for ExampleBufferedSpec {
    type Driver = Buffered;

    fn from_body(body: Bytes, _head: &ResponseHead<'_>) -> Result<usize, ResponseBodyError> {
        Ok(body.len())
    }
}

/// A streaming representation returning the managed byte stream itself.
pub struct ExampleStreamingSpec;

impl ResponseRepresentation<ByteStream> for ExampleStreamingSpec {
    type Driver = Streaming;

    fn from_body(
        body: ByteStream,
        _head: &ResponseHead<'_>,
    ) -> Result<ByteStream, ResponseBodyError> {
        Ok(body)
    }
}

/// A discarding representation: the accepted body is dropped unread.
pub struct ExampleDiscardSpec;

impl ResponseRepresentation<()> for ExampleDiscardSpec {
    type Driver = DiscardDriver;

    fn from_body((): (), _head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        Ok(())
    }
}
```

A simple buffered decoder implements `DecodeBody` (`ACCEPT`,
`validate_content_type`, `decode`) and gets the full buffered representation
through a blanket bridge; see [Request bodies](request_bodies.md#custom-request-representations)
for the paired `ExampleCodec`.

## First-party response representations

| Representation | Outputs | Behavior |
|---|---|---|
| `Json` (feature `json`) | `T: DeserializeOwned + Send` via `Buffered` | negotiates `application/json`; requires a valid `application/json` or `application/*+json` Content-Type, otherwise `InvalidMediaType` before decoding; invalid JSON is a conversion error |
| `Text` | `String` via `Buffered` | invalid UTF-8 is a conversion error |
| `Binary` | `Bytes`, `Vec<u8>` via `Buffered`; `ByteStream` via `Streaming` | no JSON/text conversion; an absent Content-Type is accepted, while a present type must be `application/octet-stream` |
| `Empty` | `()` via `Buffered` | validates an empty accepted body; a nonempty body is an error, not silently discarded |
| `Discard` | `()` via `DiscardDriver` | reports a discarded, unread body, not EOF |
| `NdJson` (feature `ndjson`) | `Vec<T>`, `ItemStream<T>` | framed records; empty lines ignored; a final nonempty line without LF is parsed once (request records: see [Request bodies](request_bodies.md)) |
| `Csv` (feature `csv`) | `Vec<T>`, `ItemStream<T>` | configured dialect and header mode; no unbounded buffering to find a bad record |

No representation is guessed from the output type, and a missing feature is a
compile error, never a fallback to another codec. Pagination needs the
`Buffered` driver; nothing requires `Output: IntoIterator`.

## Expected absence

Three patterns express "missing resource", with deliberately different costs:

| Pattern | Declared as | 404 body | Accepted bytes |
|---|---|---|---|
| Head-only absence | `-> Option<T> { absent on 404; }` | dropped unread | not available |
| Buffered `NotFound<R>` | `-> Option<T> via NotFound<Json> { status [200..=299, 404]; }` | completely acquired under limits | available |
| Custom buffered representation | an `Option<T>` representation plus `status [.., 404]` | acquired and inspected by your code | available |

**Head-only absence** (OUTCOME-01) is resolved after challenge processing and
rate feedback, before accepted-head validation and body polling. The native
body is dropped without draining and the outcome records
`BodyDisposition::DiscardedUnread`; a huge, malformed or stalled irrelevant 404
body cannot turn it into a failure. It requires an `Option<T>` result (an alias
works) and a buffered present representation, and cannot combine with
pagination, a streaming/discard driver, `store_as_*` or
`expose accepted_bytes`. `.response()` returns `ApiResponse<Option<T>>` with
the actual 404 status; it never fakes a 200.

**`NotFound<R>`** (`concord::response::NotFound`) is an ordinary buffered
representation: the accepted status set must include 404; on 404 it bypasses
the wrapped media check but still acquires the complete body under the limit,
so an oversized or truncated 404 body fails like any buffered response; other
statuses delegate to `R` and wrap `Some`.

A **custom buffered representation** maps an accepted 404 itself:

<!-- concord-source: crates/concord-examples/src/request_response/acquisition.rs#optional_response_spec -->
```rust
/// A buffered response that maps an accepted `404` to `None`.
pub struct OptionalText;

impl ResponseRepresentation<Option<String>> for OptionalText {
    type Driver = Buffered;

    const ACCEPT: Option<&'static str> = Some("text/plain");

    fn from_body(
        body: Bytes,
        head: &ResponseHead<'_>,
    ) -> Result<Option<String>, ResponseBodyError> {
        if head.status == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        String::from_utf8(body.to_vec())
            .map(Some)
            .map_err(|_| ResponseBodyError::new(ResponseBodyErrorKind::Decode))
    }
}
```

<!-- concord-source: crates/concord-examples/src/request_response/acquisition.rs#accepted_not_found -->
```rust
/// Treats a declared `404` as an accepted optional result.
pub async fn optional(client: &ResponseApi) -> Result<Option<String>, ApiError> {
    client.optional_message().await
}
```

A plain `-> Option<T>` without `absent on` simply asks the representation to
decode `Option<T>`; there is no status shortcut. More complex expected outcomes
use an application enum and a custom representation; there is no status-match
DSL. An expected domain error from a non-idempotent operation can still have
had an effect.

## Rejected statuses and status bodies

A final status outside the accepted and absence sets is an `ApiError` of the
`HttpStatus` category with stage `RejectedResponse`, the status and safe
response facts, and a **bounded, complete-only status body** (OUTCOME-03).
`error.http_response()` returns the `HttpResponseFailure`; its `body()` is a
`StatusBody` in one of these states:

| `StatusBodyState` | Meaning |
|---|---|
| `Disabled` | capture disabled (`disable_status_body_capture()`) |
| `Empty` | the complete body had no bytes |
| `Complete` | the complete body is retained (64 KiB default limit) |
| `OmittedKnownTooLarge` | a known length exceeded the limit before polling |
| `OmittedLimitExceeded` | unknown-length capture crossed the limit; the prefix was discarded |
| `UnavailableReadFailure` | reading failed after the head |
| `UnavailableDeadline` | the deadline expired during capture |

`bytes()` and `into_bytes()` return data only for `Complete`; a truncated prefix
is never presented as the body. `len()` is known for `Empty` and `Complete`.
`Debug` shows the state and length only. A failed capture never replaces the
`HttpStatus` classification. To decode a provider error document explicitly:

```text
if let Some(failure) = error.http_response() {
    let problem: Result<Problem, _> = failure.decode_status_body::<Json, Problem>();
}
```

`decode_status_body::<R, T>()` (and `decode_body::<C>()` for a `DecodeBody`
codec) works only on retained complete bytes, runs `R::validate` first (so
`Json` still requires JSON media), performs no network work, returns a separate
bounded `StatusBodyDecodeError` and never changes the original status, contact
or retry decisions.

## Errors and failure stages

| Failure | Stage | Dispatch knowledge |
|---|---|---|
| Accepted-head validation or media type | `ResponseConversion` | `ResponseObserved` |
| Truncated or oversized accepted body | `ResponseAcquisition` | `ResponseObserved` |
| Decode/conversion failure | `ResponseConversion` | `ResponseObserved` |
| Rejected final status | `RejectedResponse` | `ResponseObserved` |

Accepted-head validation and accepted-body conversion failures are terminal:
they are never retried, even when a classifier would retry, because the
response was observed and the operation may have had an effect. Error displays
are value-free; decoder messages and raw native causes are not chained.
