# Extensions

Concord's extension points are narrow semantic roles. Each receives only the
facts and authority its role needs; none can obtain a native request or
response, forge a binding, read other credentials or bypass the executor
(§17).

## Role contract matrix

| Role | Legitimate inputs and decisions | Forbidden authority |
|---|---|---|
| `RequestAtom` / `AtomEncoder<T>` | one value, a bounded scalar writer | presence, repetition, native URL, body, retry or credential choices |
| `OrderedValues` | a stable borrowed sequence order | implicit protocol emission or unordered-map iteration semantics |
| `RequestRepresentation<I>` | the exact body input, body limit, validated ID and method; one managed `PreparedBody` | reading auth, rate or the native request; a conflicting transport header |
| `ResponseRepresentation<O>` | the accepted head and the core-driver body; a typed output | changing accepted status or contact evidence; a new native driver |
| `PaginationController<P>` | the complete page, declared bounded metadata, the remaining item budget; the next state and items | mutating unmapped values, origin, auth, rate, time or effect; sending |
| `ResumableController<P>` | its own next-request state; deterministic state encoding and validated restoration | credentials, body capture, I/O, or changing endpoint authority |
| `CredentialProvider` | protected source material and reason, bounded `ProviderHttp` to declared origins | the application client, other credential slots, undeclared helper origins |
| `RateLimiter` | the complete protected key plan; typed admission and feedback | bodies, credentials, raw headers; changing the current result in feedback |
| `RetryClassifier` | sanitized semantic failure facts | declaring dispatch certainty or bypassing effect, replay or deadline gates |
| `OperationObserver` | bounded sanitized borrowed facts | mutation, veto, retries, asynchronous work through core |

Every public trait's rustdoc describes ownership, concurrency, cancellation,
input sensitivity, error behavior, resource obligations and negative
capabilities (EXT-01) in one `# Extension contract` section with those seven
labelled bullets; `docs-check` fails when a public `concord-core` or
`concord-extras` trait lacks any of them. Synchronous methods are trusted to
be prompt, bounded and side-effect-free; asynchronous roles are polled under
finite deadlines, and dropping their futures cannot roll back completed
outside effects. Core bounds wire bytes, not a custom decoder's arbitrary heap
use.

## Atoms and adapters

A type owns its canonical protocol spelling with `RequestAtom`; an explicit
`AtomEncoder<T>` adapter chooses another spelling at one use, selected with
`via` (for example `path "reports/{date via IsoDate}"`):

<!-- concord-source: crates/concord-examples/src/docs_spec.rs#atom_encoder_extension -->
```rust
/// A calendar day rendered by the explicit [`IsoDate`] path adapter.
pub struct Day {
    /// Four-digit year.
    pub year: u16,
    /// Month, 1-12.
    pub month: u8,
    /// Day of month, 1-31.
    pub day: u8,
}

/// Explicit atom adapter rendering a [`Day`] as `YYYY-MM-DD`.
pub struct IsoDate;

impl AtomEncoder<Day> for IsoDate {
    fn write(value: &Day, out: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        out.push_str(&format!(
            "{:04}-{:02}-{:02}",
            value.year, value.month, value.day
        ))
    }
}
```

The writer checks every prospective append against the 65,536-byte atom limit
before retaining it. Encoders must be deterministic, prompt and side-effect-free:
plans and executions encode separately. `OrderedValues` supplies by-reference
stable iteration for `each` (slices, arrays, vectors); give an unordered
collection an explicit deterministic adapter.

## Request representations

A full representation has one authority, `prepare`:

<!-- concord-source: crates/concord-examples/src/request_response/specifications.rs#request_body_spec_extension -->
```rust
/// A raw-bytes request representation without a media type.
pub struct ExampleRequestSpec;

impl RequestRepresentation<Bytes> for ExampleRequestSpec {
    fn prepare(input: Bytes, _context: RequestBodyContext<'_>) -> Result<PreparedBody, BodyError> {
        Ok(PreparedBody::bytes(input))
    }
}
```

Return `PreparedBody::bytes`, `empty`, `stream`, `factory` or `multipart`, and
set the exact media type once with `.content_type(..)`. Opt into
`PageRequestRepresentation<I>` only when repeated preparations from the same
captured input produce the same semantic body and media type. See
[Request bodies](request_bodies.md).

## Response representations

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

Choose the core driver (`Buffered`, `Streaming`, `DiscardDriver`), declare
static negotiation with `const ACCEPT`, validate the accepted head in
`validate` (before body acquisition) and convert in `from_body`. Only the
buffered driver supports pagination and head-only absence. See
[Responses](responses.md).

## Buffered codecs

A codec that only encodes or decodes complete buffers implements `EncodeBody`
and/or `DecodeBody`; blanket bridges provide the full representations, calling
`encode` once under the body limit with the single `CONTENT_TYPE`, and
forwarding `ACCEPT`, content-type validation and `decode`:

<!-- concord-source: crates/concord-examples/src/request_response/specifications.rs#codec_extension -->
```rust
pub struct ExampleCodec;

impl EncodeBody for ExampleCodec {
    type Input = String;

    const CONTENT_TYPE: Option<&'static str> = Some("application/vnd.concord.example");

    fn encode(input: Self::Input, _context: EncodeContext<'_>) -> Result<Bytes, CodecError> {
        Ok(Bytes::from(input))
    }
}

impl DecodeBody for ExampleCodec {
    type Output = String;

    const ACCEPT: Option<&'static str> = Some("application/vnd.concord.example");

    fn validate_content_type(content_type: Option<&str>) -> Result<(), CodecError> {
        if content_type == <Self as DecodeBody>::ACCEPT {
            Ok(())
        } else {
            Err(CodecError::new(CodecErrorKind::Decode))
        }
    }

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        String::from_utf8(body.to_vec()).map_err(|_| CodecError::new(CodecErrorKind::Decode))
    }
}
```

`EncodeContext` carries the endpoint, method and body limit; `DecodeContext`
the endpoint, method, status and parsed Content-Type. `CodecError` is
category-only (`Encode`, `Decode`, `Limit`). A codec implements either the
convenience traits or the full traits for one pair, never both.

## Record decoders

A framed record format implements `RecordDecoder`; `decode` consumes complete
records from a bounded staging buffer and `decode_eof` handles a final
unterminated record:

<!-- concord-source: crates/concord-examples/src/request_response/records.rs#record_decoder_extension -->
```rust
/// A `|`-terminated UTF-8 record decoder with a final unterminated record.
pub struct PipeRecords;

/// Representation marker hiding the pipe decoder from endpoint outputs.
pub struct PipeDelimited;

impl RecordDecoder for PipeRecords {
    type Record = String;

    const ACCEPT: Option<&'static str> = Some("application/x-pipe-records");

    fn start(_head: &ResponseHead<'_>) -> Result<Self, RecordError> {
        Ok(Self)
    }

    fn decode(&mut self, buffer: &mut BytesMut) -> Result<Option<String>, RecordError> {
        let Some(end) = buffer.iter().position(|byte| *byte == b'|') else {
            return Ok(None);
        };
        let mut record = buffer.split_to(end + 1);
        record.truncate(end);
        String::from_utf8(record.to_vec())
            .map(Some)
            .map_err(|error| {
                drop(error);
                RecordError::new(RecordErrorKind::Decode)
            })
    }

    fn decode_eof(&mut self, buffer: &mut BytesMut) -> Result<Option<String>, RecordError> {
        if buffer.is_empty() {
            Ok(None)
        } else {
            let record = buffer.split_to(buffer.len());
            String::from_utf8(record.to_vec())
                .map(Some)
                .map_err(|error| {
                    drop(error);
                    RecordError::new(RecordErrorKind::Decode)
                })
        }
    }

    fn max_buffered_bytes() -> NonZeroUsize {
        NonZeroUsize::new(64 * 1024).expect("positive example staging bound")
    }
}
```

Expose it through a buffered representation (`decode_complete_records`) for
`Vec<T>` outputs and a streaming one (`ItemStream::decode_with`) for
`ItemStream<T>` outputs. See [Streams](streams.md).

## Pagination controllers

A controller has a public initialization type (built by the declaration's
`init` clause), private state, and a `Cursor` mapped whole onto an endpoint
slot. The canonical envelope controller (§17.2) decodes nothing itself: `Json`
decodes the envelope and the controller extracts items:

<!-- concord-source: crates/concord-examples/src/pagination/envelope.rs#envelope_controller -->
```rust
use concord::pagination::{
    PageDecisionLimits, PageResponse, PageStep, PaginationController, PaginationDescription,
    PaginationError, PaginationErrorKind,
};
use serde::Deserialize;

/// One provider page: a batch of items and an optional continuation cursor.
#[derive(Deserialize)]
pub struct Envelope<T> {
    /// The page's items.
    pub data: Vec<T>,
    /// The next cursor; absent means the provider proved exhaustion.
    pub next: Option<String>,
}

/// Public initialization data built by `init CursorStart { after: after }`.
pub struct CursorStart {
    /// Cursor of the first request; `None` requests the first page.
    pub after: Option<String>,
}

/// Private controller state. `Cursor = Option<String>` maps onto the whole
/// endpoint `after?: String` slot.
pub struct EnvelopePages {
    after: Option<String>,
}

impl<T: Send + 'static> PaginationController<Envelope<T>> for EnvelopePages {
    type Item = T;
    type Init = CursorStart;
    type Cursor = Option<String>;
    const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

    fn start(init: Self::Init) -> Result<Self, PaginationError> {
        Ok(Self { after: init.after })
    }

    fn cursor(&self) -> Self::Cursor {
        self.after.clone()
    }

    fn advance(
        self,
        response: PageResponse<Envelope<T>>,
        limits: PageDecisionLimits,
    ) -> Result<PageStep<Self, T>, PaginationError> {
        let Envelope { data, next } = response.into_page();
        if data.len() > limits.remaining_items() {
            return Err(PaginationError::new(
                PaginationErrorKind::CollectedItemCeilingExceeded,
            ));
        }
        match next {
            None => Ok(PageStep::Done { items: data }),
            Some(cursor) => {
                // This fixture's domain contract rejects an empty or an
                // immediately repeated provider cursor; core independently
                // catches longer cycles and bounds.
                if cursor.is_empty() || self.after.as_ref() == Some(&cursor) {
                    return Err(PaginationError::new(
                        PaginationErrorKind::InvalidContinuation,
                    ));
                }
                Ok(PageStep::Continue {
                    next: Self {
                        after: Some(cursor),
                    },
                    items: data,
                })
            }
        }
    }
}
```

```text
pub trait PaginationController<P>: Sized + Send + 'static {
    type Item: Send + 'static;
    type Init: Send + 'static;
    type Cursor: Clone + Send + Sync + 'static;
    const DESCRIPTION: PaginationDescription;
    fn start(init: Self::Init) -> Result<Self, PaginationError>;
    fn cursor(&self) -> Self::Cursor;
    fn advance(self, page: PageResponse<P>, limits: PageDecisionLimits)
        -> Result<PageStep<Self, Self::Item>, PaginationError>;
}
```

`advance` returns `PageStep::Done { items }` (provider-contract proof of
exhaustion) or `PageStep::Continue { next, items }`. Core independently
validates the next cursor through the same public shaping rules, rejects
immediate and longer cycles, enforces item and byte bounds, and withholds the
current batch if the continuation is invalid. `PaginationDescription` declares
any header interests (at most 16, 64 KiB of evidence). The private field
`after` can be renamed without touching the declaration:

<!-- concord-pseudocode -->
```rust
GET feed(query after?: String, query count: NonZeroU32 = DEFAULT_PAGE_SIZE)
    -> Envelope<ItemSummary>
{
    path "feed";
    paginate via EnvelopePages {
        init CursorStart { after: after };
        cursor -> after;
    }
}
```

### Durable controller state

Implement `ResumableController<P>` in addition to `PaginationController<P>`
to opt a custom controller into `Pages::checkpoint()` and the generated
`request.resume_pages(&checkpoint, maximum_pages)` method. The declaration's
`paginate via` and `init` syntax stays the same. A controller without this
extra contract remains usable for ordinary traversal; checkpoint creation
returns `CheckpointErrorKind::NotResumable`.

```text
pub trait ResumableController<P>: PaginationController<P> {
    fn save_state(&self) -> Vec<u8>;
    fn restore_state(state: &[u8]) -> Result<Self, PaginationError>;
}
```

`save_state` encodes the complete state needed for the next request, including
any page size or controller policy needed by `advance`. Use a versioned,
deterministic format that can be restored by a recreated client in another
process. Avoid addresses, allocation identities, randomized hashes or
process-local registries. The core token is limited to 4,096 ASCII characters,
including endpoint and controller identification, binding and encoding
overhead; controller state must fit within the remaining space.

`restore_state` must validate the version, complete encoding, numeric ranges
and controller invariants before constructing state. Return a value-free
`PaginationError` for unsupported or malformed state; core exposes restoration
failure as a typed checkpoint error. Both methods are synchronous and must be
prompt, bounded and free of I/O or other external effects. Their authority is
limited to their own controller state. They never acquire credentials,
prepare bodies or send a request.

State is stored as plain data in the token. Include neither credentials nor
body bytes, and avoid sensitive application values. Inputs retained through
controller initialization must be public for checkpointing. Generated requests
reject checkpoint creation and resume with `CheckpointErrorKind::SensitiveCursor`
when a cursor destination or any value referenced by `init` is classified
`#[sensitive]`, including client and scope values. This rejection does not inspect
or sanitize arbitrary custom state. Custom implementations own the contents of
their state encoding and must keep errors free of state values. Core's
`Checkpoint` debug output omits the state. A token checksum detects accidental
corruption and grants no authenticity guarantee.

The first-party offset and page-number controllers preserve both position
and page size. The string and primitive-integer cursor controllers preserve
the whole optional cursor, distinguishing absence from a present empty
string or zero. Header and link controllers preserve their next string
cursor. See [saving and resuming a position](pagination.md#saving-and-resuming-a-position)
for traversal lifetime, request binding and application persistence rules.

## Credential providers

<!-- concord-source: crates/concord-examples/src/authentication/extensions.rs#credential_provider_extension -->
```rust
use std::fmt;
use std::sync::LazyLock;
use std::time::Duration;

use concord::auth::{
    AuthError, AuthErrorKind, BoxFuture, CredentialMaterial, CredentialProvider, CredentialRequest,
    ProviderBody, ProviderOrigin, ProviderRequest, SecretCredential,
};
use concord::codec::{CodecError, CodecErrorKind, DecodeBody, DecodeContext};
use concord::http::{HeaderName, HeaderValue, Method, Url};
use concord::{Bytes, SecretString};

/// Redacted credential material returned by the custom provider example.
pub struct ExampleToken(SecretString);

impl CredentialMaterial for ExampleToken {}

impl SecretCredential for ExampleToken {
    fn expose_secret(&self) -> &str {
        self.0.expose_secret()
    }
}

impl fmt::Debug for ExampleToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ExampleToken(<redacted>)")
    }
}

/// Decoder used by the endpoint-backed credential example.
pub struct ExampleTokenCodec;

impl DecodeBody for ExampleTokenCodec {
    type Output = ExampleToken;

    const ACCEPT: Option<&'static str> = Some("application/vnd.concord.token");

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        let token = String::from_utf8(body.to_vec())
            .map_err(|_| CodecError::new(CodecErrorKind::Decode))?;
        Ok(ExampleToken(
            SecretString::try_from(token).map_err(|_| CodecError::new(CodecErrorKind::Limit))?,
        ))
    }
}

/// The only origin the example provider's helper HTTP may reach.
static IDENTITY_ORIGIN: LazyLock<[ProviderOrigin; 1]> = LazyLock::new(|| {
    [ProviderOrigin::parse("https://identity.example.test")
        .expect("the fixed example identity origin is a valid HTTPS origin")]
});

/// A provider that demonstrates Concord's isolated provider HTTP capability.
pub struct ExampleProvider;

impl CredentialProvider for ExampleProvider {
    type Material = ExampleToken;

    fn acquire<'a>(
        &'a self,
        context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        Box::pin(async move {
            let request = ProviderRequest::new(
                Method::POST,
                Url::parse("https://identity.example.test/token")
                    .expect("fixed provider URL is valid"),
            )
            .header(
                HeaderName::from_static("content-type"),
                HeaderValue::from_static("application/x-www-form-urlencoded"),
            )
            .body(ProviderBody::Bytes(Bytes::from_static(
                b"grant_type=client_credentials",
            )))
            .timeout(Duration::from_secs(2));
            let response = context.http.send(request).await?;
            if !response.status().is_success() {
                return Err(AuthError::new(AuthErrorKind::Acquisition));
            }
            let token = String::from_utf8(response.into_body().to_vec())
                .map_err(|_| AuthError::new(AuthErrorKind::Acquisition))?;
            Ok(ExampleToken(
                SecretString::try_new(token).map_err(AuthError::from)?,
            ))
        })
    }

    fn http_origins(&self) -> &[ProviderOrigin] {
        IDENTITY_ORIGIN.as_slice()
    }
}
```

`acquire` (and optionally `refresh`, which defaults to `acquire`) returns
material implementing `CredentialMaterial` (plus `SecretCredential` for
single-secret placements). `recovery_mode()` selects `None`, `InvalidateOnly`
or `Refresh`. `http_origins()` declares the exact HTTPS origins the isolated
`ProviderHttp` may reach; the empty default grants none, and any other
destination is rejected before transport. See [Authentication](auth.md).

## Rate limiters

A limiter declares its enforcement truthfully and admits complete plans
atomically:

<!-- concord-source: crates/concord-examples/src/rate_limiting/custom_limiter.rs#custom_limiter_extension -->
```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use concord::rate_limit::{
    BoxFuture, Enforcement, LimiterCapability, RateLimitError, RateLimitPermit, RateLimitRequest,
    RateLimiter,
};

/// Deterministically grants attempts while exposing only a bounded counter.
#[derive(Clone, Default)]
pub struct CountingLimiter {
    acquisitions: Arc<AtomicUsize>,
}

impl CountingLimiter {
    /// Creates an empty counter.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of completed acquisition calls.
    #[must_use]
    pub fn acquisitions(&self) -> usize {
        self.acquisitions.load(Ordering::SeqCst)
    }
}

impl RateLimiter for CountingLimiter {
    fn capability(&self) -> LimiterCapability {
        LimiterCapability::new(Enforcement::Unenforced, "concord-examples.counting", 1)
    }

    fn acquire<'a>(
        &'a self,
        _request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        self.acquisitions.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(RateLimitPermit::granted()) })
    }
}
```

<!-- concord-source: crates/concord-examples/src/rate_limiting/custom_limiter.rs#custom_limiter_install -->
```rust
pub fn install(
    runtime: concord::runtime::RuntimeConfigBuilder,
    limiter: Arc<CountingLimiter>,
) -> concord::runtime::RuntimeConfigBuilder {
    runtime.rate_limiter(limiter)
}
```

`capability()` returns a `LimiterCapability` with the enforcement
(`Local`, `Unenforced` or `SharedPersistent`), a bounded namespace and an
algorithm version, snapshotted at build; plans disclose it.
`acquire(RateLimitRequest)` sees the complete effective policies, protected
canonical keys, endpoint/method/page/attempt facts and a schema descriptor per
policy; it must admit the whole conjunctive plan or debit nothing (RATE-04). A
`SharedPersistent` adapter registers and compares schemas inside `acquire` and
fails closed on a mismatch. `observe(RateLimitResponse)` receives the status and
the one parsed server delay and affects only future admission.
`preparation_key_limit()` bounds resolved key bytes. A custom key encoding
implements `RateKeyEncoder<T>` or `RateKeyPart` with a stable `ENCODING_ID`
and writes through `CanonicalKeyWriter`. See [Rate limits](rate_limits.md).

## Retry classifiers and observers

A `RetryClassifier` (or a closure through `classify_fn`) sees a sanitized
`RetryAttempt` and returns `RetryDecision::Retry` or `Stop`; it expresses
appetite only ([Retry](retry.md)). An `OperationObserver` receives sanitized
events after the fact and cannot influence the result
([Planning and observation](planning_and_observation.md#observing-operations)).

## The generic caller trait

`RequestControls` is intentionally **unsealed**: applications may implement it
on their own wrappers so generic helpers work. Implementing it grants no native
request, binding, credential or executor access, and core never treats an
implementation as authenticity evidence (CALL-02). Execution authority comes
from core-owned validated bindings.

## Testing extensions

`concord-test-support` is a development/test crate: deterministic clocks,
scripted providers and limiters, recording observers, body and record fixtures,
and a loopback server, TLS server and CONNECT proxy for exercising downstream
wrappers through public client behavior. It never bypasses shaping, auth, status
or limits to make a test pass. Use owned fake secrets and sentinels only;
recording helpers sanitize by default, and exact private wire assertions
require explicit test-only capture of values the test supplied.
