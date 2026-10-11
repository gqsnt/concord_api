//! Every public extension role (§17) implemented downstream through the
//! facade only: request/response representations, record decoders, buffered
//! codecs, body factories, credential providers with the bounded provider
//! helper, a pagination controller with public Init and Cursor, limiters and
//! observers; downstream-generated named requests implement the open common
//! controls themselves (V3-L14) and the provider helper enforces its exact
//! origin allowlist (V3-E16).

#![forbid(unsafe_code)]
// The runtime-probe helpers and some imports serve only the TLS-gated
// execution tests below; without `default-tls` they are intentionally unused.
#![cfg_attr(not(feature = "default-tls"), allow(unused_imports, dead_code))]

use std::collections::VecDeque;
use std::future::IntoFuture;
use std::num::NonZeroU32;
#[cfg(feature = "records")]
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

#[cfg(feature = "json")]
use concord::Json;
use concord::auth::{
    AuthError, AuthErrorKind, BoxFuture, CredentialMaterial, CredentialProvider, CredentialRequest,
    ProviderBody, ProviderBodyFactory, ProviderOrigin, ProviderRequest, SecretCredential,
};
use concord::body::{
    BodyDescription, BodyError, BodyFactory, BodyKind, PreparedBody, RequestBodyContext,
    RequestRepresentation, RequestStream,
};
#[cfg(feature = "multipart")]
use concord::body::{MultipartBody, MultipartFactory, MultipartPart};
#[cfg(feature = "records")]
use concord::codec::BytesMut;
use concord::codec::{
    CodecError, CodecErrorKind, DecodeBody, DecodeContext, EncodeBody, EncodeContext,
};
use concord::endpoint::{
    BoundRuntimeRequest, EndpointId, ResponseRequest, RuntimeEndpoint, RuntimeEndpointBuilder,
};
use concord::error::{ErrorCategory, FailureStage};
use concord::http::{HeaderName, HeaderValue, Method, SafeProxy, StatusCode, Url};
use concord::observation::{OperationEvent, OperationObserver};
use concord::pagination::{
    CollectAll, CollectPrefix, HasNextCursor, PageDecisionLimits, PageResponse, PageStep,
    PaginationController, PaginationDescription, PaginationError, PaginationErrorKind, StopReason,
};
use concord::rate_limit::{
    Enforcement, LimiterCapability, RateLimitError, RateLimitErrorKind, RateLimitPermit,
    RateLimitRequest, RateLimitResponse, RateLimiter,
};
#[cfg(feature = "ndjson")]
use concord::records::NdJson;
#[cfg(feature = "csv")]
use concord::records::{Csv, HasHeader, NoHeader, SemicolonDelimiter, TabDelimiter};
#[cfg(feature = "records")]
use concord::records::{ItemStream, RecordDecoder, RecordError};
use concord::response::{
    Buffered, ByteStream, DiscardDriver, ResponseBodyError, ResponseHead, ResponseRepresentation,
    Streaming,
};
use concord::{
    ApiError, Binary, Bytes, CallOptions, ClientBuildError, RequestControls, SecretString, api,
};
use futures_core::Stream;

fn runtime_id(value: &str) -> EndpointId {
    EndpointId::try_from(value).expect("valid runtime endpoint ID")
}

/// The single runtime body-selection call site of this module.
fn runtime_body<B, T, R, O>(
    builder: RuntimeEndpointBuilder<R, O>,
    input: T,
) -> RuntimeEndpointBuilder<R, O>
where
    B: RequestRepresentation<T>,
    T: Send + 'static,
{
    builder.body_repr::<B, T>(input)
}

// ---------------------------------------------------------------------------
// Request representations.
// ---------------------------------------------------------------------------

/// Prepares complete bytes without owning a media type.
pub struct ExtensionRequestSpec;

impl RequestRepresentation<Bytes> for ExtensionRequestSpec {
    const DESCRIPTION: BodyDescription = BodyDescription::buffered(None);

    fn prepare(input: Bytes, _context: RequestBodyContext<'_>) -> Result<PreparedBody, BodyError> {
        Ok(PreparedBody::bytes(input))
    }
}

/// Prepares a one-shot stream whose empty transport chunks core suppresses.
pub struct EmptyChunkRequestSpec;

struct ExtensionEmptyChunkStream {
    chunks: VecDeque<Result<Bytes, std::io::Error>>,
}

impl Stream for ExtensionEmptyChunkStream {
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(self.chunks.pop_front())
    }
}

impl RequestRepresentation<Bytes> for EmptyChunkRequestSpec {
    const DESCRIPTION: BodyDescription = BodyDescription::new(BodyKind::Stream);

    fn prepare(input: Bytes, _context: RequestBodyContext<'_>) -> Result<PreparedBody, BodyError> {
        let exact_length = u64::try_from(input.len()).unwrap_or(u64::MAX);
        let stream = RequestStream::from_stream(ExtensionEmptyChunkStream {
            chunks: [Ok(Bytes::new()), Ok(input), Ok(Bytes::new())]
                .into_iter()
                .collect(),
        })
        .exact_length(exact_length);
        Ok(PreparedBody::stream(stream))
    }
}

// ---------------------------------------------------------------------------
// Response representations and record decoders.
// ---------------------------------------------------------------------------

pub struct ExtensionBufferedSpec;

impl ResponseRepresentation<usize> for ExtensionBufferedSpec {
    type Driver = Buffered;

    fn from_body(body: Bytes, _head: &ResponseHead<'_>) -> Result<usize, ResponseBodyError> {
        Ok(body.len())
    }
}

pub struct ExtensionStreamingSpec;

impl ResponseRepresentation<ByteStream> for ExtensionStreamingSpec {
    type Driver = Streaming;

    fn from_body(
        body: ByteStream,
        _head: &ResponseHead<'_>,
    ) -> Result<ByteStream, ResponseBodyError> {
        Ok(body)
    }
}

pub struct ExtensionDiscardSpec;

impl ResponseRepresentation<()> for ExtensionDiscardSpec {
    type Driver = DiscardDriver;

    fn from_body((): (), _head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        Ok(())
    }
}

#[cfg(feature = "records")]
pub struct ExtensionRecordDecoder {
    state: &'static str,
}

#[cfg(feature = "records")]
impl RecordDecoder for ExtensionRecordDecoder {
    type Record = Bytes;

    fn start(_head: &ResponseHead<'_>) -> Result<Self, RecordError> {
        Ok(Self {
            state: "record-rustdoc-value-sentinel-42ad91",
        })
    }

    fn decode(&mut self, buffer: &mut BytesMut) -> Result<Option<Bytes>, RecordError> {
        assert_eq!(self.state, "record-rustdoc-value-sentinel-42ad91");
        let Some(end) = buffer.iter().position(|byte| *byte == b'|') else {
            return Ok(None);
        };
        let mut record = buffer.split_to(end + 1);
        record.truncate(end);
        Ok(Some(record.freeze()))
    }

    fn decode_eof(&mut self, buffer: &mut BytesMut) -> Result<Option<Bytes>, RecordError> {
        assert_eq!(self.state, "record-rustdoc-value-sentinel-42ad91");
        Ok((!buffer.is_empty()).then(|| buffer.split_to(buffer.len()).freeze()))
    }

    fn max_buffered_bytes() -> NonZeroUsize {
        NonZeroUsize::new(64 * 1024).expect("positive extension staging bound")
    }
}

#[cfg(feature = "records")]
pub struct NoProgressRecordDecoder;

#[cfg(feature = "records")]
impl RecordDecoder for NoProgressRecordDecoder {
    type Record = Bytes;

    fn start(_head: &ResponseHead<'_>) -> Result<Self, RecordError> {
        Ok(Self)
    }

    fn decode(&mut self, _buffer: &mut BytesMut) -> Result<Option<Bytes>, RecordError> {
        Ok(Some(Bytes::from_static(
            b"public-no-progress-record-sentinel-cfe809",
        )))
    }

    fn decode_eof(&mut self, _buffer: &mut BytesMut) -> Result<Option<Bytes>, RecordError> {
        Ok(None)
    }

    fn max_buffered_bytes() -> NonZeroUsize {
        NonZeroUsize::new(64).expect("positive public contract-test bound")
    }
}

#[cfg(feature = "records")]
pub struct ExtensionRecords;

#[cfg(feature = "records")]
impl ResponseRepresentation<ItemStream<Bytes>> for ExtensionRecords {
    type Driver = Streaming;

    fn from_body(
        body: ByteStream,
        head: &ResponseHead<'_>,
    ) -> Result<ItemStream<Bytes>, ResponseBodyError> {
        ItemStream::decode_with::<ExtensionRecordDecoder>(body, head)
    }
}

#[cfg(feature = "records")]
pub struct InvalidRecords;

#[cfg(feature = "records")]
impl ResponseRepresentation<ItemStream<Bytes>> for InvalidRecords {
    type Driver = Streaming;

    fn from_body(
        body: ByteStream,
        head: &ResponseHead<'_>,
    ) -> Result<ItemStream<Bytes>, ResponseBodyError> {
        ItemStream::decode_with::<NoProgressRecordDecoder>(body, head)
    }
}

// ---------------------------------------------------------------------------
// Buffered codec convenience roles and body factories.
// ---------------------------------------------------------------------------

pub struct ExtensionCodec;

impl EncodeBody for ExtensionCodec {
    type Input = String;

    const CONTENT_TYPE: Option<&'static str> = Some("application/x-concord-extension");

    fn encode(input: Self::Input, _context: EncodeContext<'_>) -> Result<Bytes, CodecError> {
        Ok(Bytes::from(input))
    }
}

impl DecodeBody for ExtensionCodec {
    type Output = String;

    const ACCEPT: Option<&'static str> = Some("application/x-concord-extension");

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

/// Reconstructible stream factory. It cannot choose the media type (the
/// prepared body owns it once).
pub struct ExtensionBodyFactory;

impl BodyFactory for ExtensionBodyFactory {
    fn create(&self) -> Result<RequestStream, BodyError> {
        Ok(RequestStream::from_bytes(Bytes::from_static(b"factory")))
    }
}

#[cfg(feature = "multipart")]
pub struct ExtensionMultipartFactory;

#[cfg(feature = "multipart")]
impl MultipartFactory for ExtensionMultipartFactory {
    fn create(&self) -> Result<MultipartBody, BodyError> {
        MultipartBody::new().part("extension", MultipartPart::text("multipart")?)
    }
}

// ---------------------------------------------------------------------------
// Credential material and providers.
// ---------------------------------------------------------------------------

pub struct ExtensionCredential(SecretString);

impl CredentialMaterial for ExtensionCredential {}

impl SecretCredential for ExtensionCredential {
    fn expose_secret(&self) -> &str {
        self.0.expose_secret()
    }
}

impl std::fmt::Debug for ExtensionCredential {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ExtensionCredential(<redacted>)")
    }
}

pub struct ExtensionProviderBody;

impl ProviderBodyFactory for ExtensionProviderBody {
    fn create(&self) -> Result<Bytes, AuthError> {
        Ok(Bytes::from_static(b"bounded-provider-input"))
    }
}

/// Provider using the bounded provider helper against its one declared
/// origin (AUTH-02).
pub struct ExtensionProvider {
    origins: Vec<ProviderOrigin>,
}

impl ExtensionProvider {
    #[must_use]
    pub fn new() -> Self {
        Self {
            origins: vec![
                ProviderOrigin::parse("https://provider.example.test").expect("provider origin"),
            ],
        }
    }
}

impl Default for ExtensionProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl CredentialProvider for ExtensionProvider {
    type Material = ExtensionCredential;

    fn acquire<'a>(
        &'a self,
        context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        Box::pin(async move {
            let _request_body_limit = context.http.request_body_limit();
            let request = ProviderRequest::new(
                Method::POST,
                Url::parse("https://provider.example.test/token")
                    .expect("fixed provider URL is valid"),
            )
            .header(
                HeaderName::from_static("content-type"),
                HeaderValue::from_static("application/octet-stream"),
            )
            .body(ProviderBody::Factory(Arc::new(ExtensionProviderBody)))
            .timeout(Duration::from_secs(2));
            let response = context.http.send(request).await?;
            let value = String::from_utf8(response.into_body().to_vec())
                .map_err(|_| AuthError::new(AuthErrorKind::Acquisition))?;
            Ok(ExtensionCredential(
                SecretString::try_new(value).map_err(AuthError::from)?,
            ))
        })
    }

    fn http_origins(&self) -> &[ProviderOrigin] {
        &self.origins
    }
}

/// Provider returning owned local material without helper HTTP.
pub struct DirectExtensionProvider {
    material: SecretString,
}

impl DirectExtensionProvider {
    #[must_use]
    pub fn new(material: SecretString) -> Self {
        Self { material }
    }
}

static EXTENSION_PROVIDER_ACQUIRES: AtomicUsize = AtomicUsize::new(0);
static EXTENSION_LIMITER_ACQUIRES: AtomicUsize = AtomicUsize::new(0);
static EXTENSION_LIMITER_OBSERVATIONS: AtomicUsize = AtomicUsize::new(0);
static EXTENSION_HOOK_BEFORE: AtomicUsize = AtomicUsize::new(0);
static EXTENSION_HOOK_RESPONSE: AtomicUsize = AtomicUsize::new(0);
static EXTENSION_HOOK_COMPLETION: AtomicUsize = AtomicUsize::new(0);
static EXTENSION_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

impl CredentialProvider for DirectExtensionProvider {
    type Material = ExtensionCredential;

    fn acquire<'a>(
        &'a self,
        context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        EXTENSION_PROVIDER_ACQUIRES.fetch_add(1, Ordering::SeqCst);
        // An undeclared helper grants no destinations at all.
        assert!(context.http.allowed_origins().is_empty());
        Box::pin(async move { Ok(ExtensionCredential(self.material.clone())) })
    }
}

pub fn reset_extension_runtime_probes() {
    EXTENSION_PROVIDER_ACQUIRES.store(0, Ordering::SeqCst);
    EXTENSION_LIMITER_ACQUIRES.store(0, Ordering::SeqCst);
    EXTENSION_LIMITER_OBSERVATIONS.store(0, Ordering::SeqCst);
    EXTENSION_HOOK_BEFORE.store(0, Ordering::SeqCst);
    EXTENSION_HOOK_RESPONSE.store(0, Ordering::SeqCst);
    EXTENSION_HOOK_COMPLETION.store(0, Ordering::SeqCst);
}

#[must_use]
pub fn extension_runtime_probe_counts() -> (usize, usize, usize, usize, usize, usize) {
    (
        EXTENSION_PROVIDER_ACQUIRES.load(Ordering::SeqCst),
        EXTENSION_LIMITER_ACQUIRES.load(Ordering::SeqCst),
        EXTENSION_LIMITER_OBSERVATIONS.load(Ordering::SeqCst),
        EXTENSION_HOOK_BEFORE.load(Ordering::SeqCst),
        EXTENSION_HOOK_RESPONSE.load(Ordering::SeqCst),
        EXTENSION_HOOK_COMPLETION.load(Ordering::SeqCst),
    )
}

// ---------------------------------------------------------------------------
// Pagination: public Init and Cursor, private controller state (§14, §17.2).
// ---------------------------------------------------------------------------

pub struct ExtensionPageCodec;

pub struct ExtensionPage {
    items: Vec<u8>,
    next: Option<String>,
}

impl HasNextCursor<String> for ExtensionPage {
    fn next_cursor(&self) -> Option<&String> {
        self.next.as_ref()
    }
}

impl DecodeBody for ExtensionPageCodec {
    type Output = ExtensionPage;

    const ACCEPT: Option<&'static str> = Some("application/x-concord-page");

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        let text =
            std::str::from_utf8(&body).map_err(|_| CodecError::new(CodecErrorKind::Decode))?;
        let (items, next) = text
            .split_once('|')
            .ok_or_else(|| CodecError::new(CodecErrorKind::Decode))?;
        let items = if items.is_empty() {
            Vec::new()
        } else {
            items
                .split(',')
                .map(|item| {
                    item.parse::<u8>()
                        .map_err(|_| CodecError::new(CodecErrorKind::Decode))
                })
                .collect::<Result<Vec<_>, _>>()?
        };
        Ok(ExtensionPage {
            items,
            next: (next != "-").then(|| next.to_owned()),
        })
    }
}

static EXTENSION_CONSTRUCTION_EVALUATIONS: AtomicUsize = AtomicUsize::new(0);
static EXTENSION_START_CALLS: AtomicUsize = AtomicUsize::new(0);
static EXTENSION_NEXT_CALLS: AtomicUsize = AtomicUsize::new(0);

/// A root value whose clones are counted: pagination initialization reads it
/// through the controller's public `Init`.
pub struct ExtensionConstructionProbe;

impl ExtensionConstructionProbe {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for ExtensionConstructionProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for ExtensionConstructionProbe {
    fn clone(&self) -> Self {
        EXTENSION_CONSTRUCTION_EVALUATIONS.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

pub fn reset_extension_pagination_probes() {
    EXTENSION_CONSTRUCTION_EVALUATIONS.store(0, Ordering::SeqCst);
    EXTENSION_START_CALLS.store(0, Ordering::SeqCst);
    EXTENSION_NEXT_CALLS.store(0, Ordering::SeqCst);
}

#[must_use]
pub fn extension_pagination_probe_counts() -> (usize, usize, usize) {
    (
        EXTENSION_CONSTRUCTION_EVALUATIONS.load(Ordering::SeqCst),
        EXTENSION_START_CALLS.load(Ordering::SeqCst),
        EXTENSION_NEXT_CALLS.load(Ordering::SeqCst),
    )
}

/// Public initialization data of [`ExtensionController`].
pub struct ExtensionInit {
    pub cursor: Option<String>,
    pub construction: ExtensionConstructionProbe,
}

/// Downstream controller with private state.
pub struct ExtensionController {
    cursor: Option<String>,
    _construction: ExtensionConstructionProbe,
}

impl PaginationController<ExtensionPage> for ExtensionController {
    type Item = u8;
    type Init = ExtensionInit;
    type Cursor = Option<String>;

    const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

    fn start(init: Self::Init) -> Result<Self, PaginationError> {
        EXTENSION_START_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(Self {
            cursor: init.cursor,
            _construction: init.construction,
        })
    }

    fn cursor(&self) -> Self::Cursor {
        self.cursor.clone()
    }

    fn advance(
        self,
        page: PageResponse<ExtensionPage>,
        limits: PageDecisionLimits,
    ) -> Result<PageStep<Self, Self::Item>, PaginationError> {
        EXTENSION_NEXT_CALLS.fetch_add(1, Ordering::SeqCst);
        let next = page.page().next_cursor().cloned().map(|cursor| Self {
            cursor: Some(cursor),
            _construction: self._construction,
        });
        let items = page.into_page().items;
        if items.len() > limits.remaining_items() {
            return Err(PaginationError::new(
                PaginationErrorKind::CollectedItemCeilingExceeded,
            ));
        }
        Ok(match next {
            Some(next) => PageStep::Continue { items, next },
            None => PageStep::Done { items },
        })
    }
}

// ---------------------------------------------------------------------------
// Limiter and observer roles.
// ---------------------------------------------------------------------------

pub struct ExtensionLimiter;

impl RateLimiter for ExtensionLimiter {
    fn capability(&self) -> LimiterCapability {
        LimiterCapability::new(Enforcement::Local, "concord.tests.extension", 1)
    }

    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        EXTENSION_LIMITER_ACQUIRES.fetch_add(1, Ordering::SeqCst);
        let _bounded_rule_count = request.plan.policies().len();
        let _attempt = request.attempt;
        Box::pin(async { Ok(RateLimitPermit::granted()) })
    }

    fn observe(&self, response: RateLimitResponse<'_>) {
        EXTENSION_LIMITER_OBSERVATIONS.fetch_add(1, Ordering::SeqCst);
        let _status = response.status;
        let _attempt = response.request.attempt;
    }
}

pub struct ExtensionHooks;

impl OperationObserver for ExtensionHooks {
    fn observe(&self, event: OperationEvent<'_>) {
        match event {
            OperationEvent::AttemptStarted(context) => {
                EXTENSION_HOOK_BEFORE.fetch_add(1, Ordering::SeqCst);
                let _attempt = context.attempt;
            }
            OperationEvent::ResponseHeadObserved(context) => {
                EXTENSION_HOOK_RESPONSE.fetch_add(1, Ordering::SeqCst);
                let _attempt = context.attempt;
            }
            OperationEvent::OperationFinished(context) => {
                EXTENSION_HOOK_COMPLETION.fetch_add(1, Ordering::SeqCst);
                let _attempt_totals = (context.attempts_started, context.policy_retries);
            }
            OperationEvent::PaginationFinished(context) => {
                let _attempt_totals = (context.attempts_started, context.policy_retries);
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Declarations.
// ---------------------------------------------------------------------------

api! {
    client ExtensionApi {
        timeout 30s;
        base "http://extension.example.test/v1/";
        values {
            construction: ExtensionConstructionProbe;
        }
        credentials {
            credential session = custom ExtensionProvider;
        }
        rate_limit { id "concord.contracts.1"; key [origin]; cost 1; allow 10 per 1s; }

        GET protected() -> Bytes via Binary {
            path "protected";
            auth { header "X-Session" = session challenge unauthorized; }
        }

        GET pages(query cursor?: String) -> ExtensionPage via ExtensionPageCodec {
            path "pages";
            paginate via ExtensionController {
                init ExtensionInit { cursor: cursor, construction: client.construction };
                cursor -> cursor;
            }
        }

        POST generic_body(body body: Bytes via ExtensionRequestSpec) -> usize via ExtensionBufferedSpec {
            path "generic";
            status [200..=299, 404];
        }
        POST empty_chunk_body(body body: Bytes via EmptyChunkRequestSpec) -> Bytes via Binary {
            path "empty-chunk";
        }
        GET generic_stream() -> ByteStream via ExtensionStreamingSpec {
            path "stream";
        }
        GET generic_discard() -> () via ExtensionDiscardSpec {
            path "discard";
        }
    }
}

api! {
    client SecureExtensionApi {
        timeout 30s;
        base "https://localhost/";
        credentials {
            credential session = custom DirectExtensionProvider;
        }
        rate_limit { id "concord.contracts.2"; key [origin]; cost 1; allow 10 per 1s; }

        GET protected() -> Bytes via Binary {
            auth { header "X-Session" = session challenge unauthorized; }
        }
    }
}

#[cfg(feature = "records")]
api! {
    client RecordExtensionApi {
        timeout 30s;
        base "http://record-extension.example.test/";

        GET custom() -> ItemStream<Bytes> via ExtensionRecords;
        GET invalid_decoder() -> ItemStream<Bytes> via InvalidRecords;
    }
}

#[cfg(feature = "csv")]
pub type ConfCsv = Csv<HasHeader, SemicolonDelimiter>;

#[cfg(feature = "csv")]
pub type HeaderlessTsv = Csv<NoHeader, TabDelimiter>;

#[cfg(feature = "csv")]
pub type Row = (String, u32);

#[cfg(feature = "csv")]
api! {
    client CsvExtensionApi {
        timeout 30s;
        base "http://csv-extension.example.test/";

        GET rows() -> Vec<Vec<String>> via Csv;
        GET rows_stream() -> ItemStream<Vec<String>> via Csv;
        GET configured_rows() -> Vec<Row> via ConfCsv;
        GET configured_rows_stream() -> ItemStream<Row> via ConfCsv;
        GET headerless_rows() -> Vec<Row> via HeaderlessTsv;
    }
}

#[cfg(feature = "csv")]
#[test]
fn facade_reexports_the_complete_csv_policy_surface() {
    use concord::records::{CommaDelimiter, CsvDelimiter, CsvHeaderMode};

    fn header_mode<H: CsvHeaderMode>() {}
    fn delimiter<D: CsvDelimiter>() {}
    // REP-02: page eligibility is a complete buffered driver plus a typed
    // controller, not a representation-level page marker.
    fn complete_buffered<S>()
    where
        S: ResponseRepresentation<Vec<Row>, Driver = Buffered>,
    {
    }

    header_mode::<HasHeader>();
    header_mode::<NoHeader>();
    delimiter::<CommaDelimiter>();
    delimiter::<SemicolonDelimiter>();
    delimiter::<TabDelimiter>();
    complete_buffered::<ConfCsv>();
    complete_buffered::<HeaderlessTsv>();
}

#[cfg(feature = "ndjson")]
api! {
    client NdJsonExtensionApi {
        timeout 30s;
        base "http://ndjson-extension.example.test/";

        GET values() -> Vec<u32> via NdJson;
        GET values_stream() -> ItemStream<u32> via NdJson;
    }
}

fn extension_client(proxy: Option<SafeProxy>) -> Result<ExtensionApi, ClientBuildError> {
    let proxy = proxy.unwrap_or_else(|| {
        SafeProxy::parse("http://127.0.0.1:43119").expect("fixed credential-free proxy origin")
    });
    ExtensionApi::builder()
        .set_construction(ExtensionConstructionProbe::new())
        .session(ExtensionProvider::new())
        .configure_http(|http| http.proxy(proxy))
        .configure_runtime(|runtime| {
            let observation = Arc::new(ExtensionHooks);
            runtime
                .rate_limiter(Arc::new(ExtensionLimiter))
                .operation_observer(observation.clone())
                .operation_observer(observation.clone())
                .operation_observer(observation)
        })
        .build()
}

/// Compiles every stable extension workflow without private or native
/// integration imports.
pub fn certify_public_extensions() -> Result<(), ClientBuildError> {
    let client = extension_client(None)?;

    let _: extension_api::ProtectedRequest = client.protected();
    let _: extension_api::GenericBodyRequest =
        client.generic_body(Bytes::from_static(b"generic extension input"));
    let _: extension_api::EmptyChunkBodyRequest =
        client.empty_chunk_body(Bytes::from_static(b"empty chunk extension input"));
    let _: extension_api::GenericStreamRequest = client.generic_stream();
    let _: extension_api::GenericDiscardRequest = client.generic_discard();
    let _: CollectPrefix<u8> = client.pages().collect_prefix(2);

    let buffered = runtime_body::<ExtensionCodec, _, _, _>(
        RuntimeEndpoint::builder(runtime_id("extension_api.runtime_buffered"), Method::POST)
            .path("buffered"),
        "bounded-input".to_owned(),
    )
    .response::<ExtensionCodec, String>()
    .build()
    .expect("valid buffered runtime endpoint");
    let _: BoundRuntimeRequest<_, String> = client
        .bind(buffered)
        .expect("runtime endpoint binds to extension client");

    let bytes = runtime_body::<PreparedBody, _, _, _>(
        RuntimeEndpoint::builder(runtime_id("extension_api.runtime_bytes"), Method::POST)
            .path("bytes"),
        PreparedBody::factory(ExtensionBodyFactory)
            .content_type("application/x-concord-extension")
            .expect("one exact media type"),
    )
    .response::<Binary, Bytes>()
    .build()
    .expect("valid bytes runtime endpoint");
    let _: BoundRuntimeRequest<_, Bytes> = client
        .bind(bytes)
        .expect("runtime endpoint binds to extension client");

    let no_content =
        RuntimeEndpoint::builder(runtime_id("extension_api.runtime_none"), Method::DELETE)
            .path("none")
            .response::<concord::Empty, ()>()
            .build()
            .expect("valid no-content runtime endpoint");
    let _: BoundRuntimeRequest<_, ()> = client
        .bind(no_content)
        .expect("runtime endpoint binds to extension client");

    let stream = RuntimeEndpoint::builder(runtime_id("extension_api.runtime_stream"), Method::GET)
        .path("stream")
        .response::<ExtensionStreamingSpec, ByteStream>()
        .build()
        .expect("valid streaming runtime endpoint");
    let _: BoundRuntimeRequest<_, ByteStream> = client
        .bind(stream)
        .expect("runtime endpoint binds to extension client");

    let generic = runtime_body::<ExtensionRequestSpec, _, _, _>(
        RuntimeEndpoint::builder(runtime_id("extension_api.runtime_generic"), Method::POST)
            .path("generic"),
        Bytes::from_static(b"prepared generic input"),
    )
    .response::<ExtensionBufferedSpec, usize>()
    .build()
    .expect("valid generic runtime endpoint");
    let _: BoundRuntimeRequest<_, usize> = client
        .bind(generic)
        .expect("runtime endpoint binds to extension client");

    let page = ExtensionPage {
        items: vec![1, 2],
        next: Some("next".to_owned()),
    };
    let _next_cursor: Option<&String> = page.next_cursor();
    fn assert_controller<C>()
    where
        C: PaginationController<
                ExtensionPage,
                Item = u8,
                Init = ExtensionInit,
                Cursor = Option<String>,
            >,
    {
    }
    assert_controller::<ExtensionController>();

    #[cfg(feature = "json")]
    {
        let json = runtime_body::<Json, _, _, _>(
            RuntimeEndpoint::builder(runtime_id("extension_api.runtime_json"), Method::POST),
            vec![1_u32, 2, 3],
        )
        .response::<Json, Vec<u32>>()
        .build()
        .expect("valid JSON runtime endpoint");
        let _: BoundRuntimeRequest<_, Vec<u32>> = client
            .bind(json)
            .expect("runtime endpoint binds to extension client");
    }

    #[cfg(feature = "multipart")]
    {
        let multipart = runtime_body::<PreparedBody, _, _, _>(
            RuntimeEndpoint::builder(runtime_id("extension_api.runtime_multipart"), Method::POST),
            PreparedBody::multipart_factory(ExtensionMultipartFactory).expect("generated boundary"),
        )
        .response::<concord::Empty, ()>()
        .build()
        .expect("valid multipart runtime endpoint");
        let _: BoundRuntimeRequest<_, ()> = client
            .bind(multipart)
            .expect("runtime endpoint binds to extension client");
    }

    #[cfg(feature = "records")]
    {
        let records = RecordExtensionApi::new().expect("record extension client");
        let _: record_extension_api::CustomRequest = records.custom();
    }

    #[cfg(feature = "csv")]
    {
        let records = CsvExtensionApi::new().expect("CSV extension client");
        let _: csv_extension_api::RowsRequest = records.rows();
        let _: csv_extension_api::RowsStreamRequest = records.rows_stream();
        let _: csv_extension_api::ConfiguredRowsRequest = records.configured_rows();
        let _: csv_extension_api::ConfiguredRowsStreamRequest = records.configured_rows_stream();
        let _: csv_extension_api::HeaderlessRowsRequest = records.headerless_rows();
    }

    #[cfg(feature = "ndjson")]
    {
        let records = NdJsonExtensionApi::new().expect("NDJSON extension client");
        let _: nd_json_extension_api::ValuesRequest = records.values();
        let _: nd_json_extension_api::ValuesStreamRequest = records.values_stream();
    }

    Ok(())
}

#[test]
fn every_public_extension_family_compiles_through_the_facade_only() {
    let _guard = EXTENSION_TEST_LOCK
        .lock()
        .expect("extension test lock poisoned");
    certify_public_extensions().expect("public semantic extension certification");
}

#[tokio::test(flavor = "current_thread")]
#[allow(
    clippy::await_holding_lock,
    reason = "the process-global extension probes require whole-test serialization"
)]
async fn downstream_generated_controller_constructs_binds_and_collects_two_pages() {
    use concord_test_support::server::{ResponseScript, ScriptedServer};

    let _guard = EXTENSION_TEST_LOCK
        .lock()
        .expect("extension test lock poisoned");
    let server = ScriptedServer::start([
        ResponseScript::ok("1,2|continuation")
            .with_header("content-type", "application/x-concord-page"),
        ResponseScript::ok("3|-").with_header("content-type", "application/x-concord-page"),
    ])
    .expect("bounded loopback pagination server");
    let proxy = SafeProxy::parse(server.url("")).expect("credential-free loopback proxy URL");
    let client = extension_client(Some(proxy)).expect("downstream generated client");

    // v2 `.paginate(PageLimit::Prefix(10))` is the explicit prefix terminal
    // with a truthful summary in v3 (§25.2).
    let pagination = client.pages().set_cursor("initial").collect_prefix(10);

    reset_extension_pagination_probes();
    let prefix = pagination
        .await
        .expect("public-only downstream pagination runtime");
    let requests = server.requests();
    let probe_counts = extension_pagination_probe_counts();

    assert_eq!(prefix.items(), [1, 2, 3]);
    assert_eq!(prefix.summary().stop_reason(), StopReason::Exhausted);
    assert_eq!(prefix.summary().pages_delivered(), 2);
    assert_eq!(prefix.summary().items_delivered(), 3);
    assert_eq!(requests.len(), 2);
    assert!(requests[0].target().contains("cursor=initial"));
    assert!(requests[1].target().contains("cursor=continuation"));
    // Init is evaluated once for the traversal (one clone of the root value),
    // the controller starts once and advances once per page.
    assert_eq!(probe_counts, (1, 1, 2));
}

#[tokio::test(flavor = "current_thread")]
#[allow(
    clippy::await_holding_lock,
    reason = "the process-global extension probes require whole-test serialization"
)]
async fn public_request_body_spec_suppresses_empty_chunks_before_transport() {
    use concord_test_support::server::{ResponseScript, ScriptedServer};

    let _guard = EXTENSION_TEST_LOCK
        .lock()
        .expect("extension test lock poisoned");
    let server =
        ScriptedServer::start([ResponseScript::ok("ok")]).expect("bounded loopback body server");
    let proxy = SafeProxy::parse(server.url("")).expect("credential-free loopback proxy URL");
    let client = extension_client(Some(proxy)).expect("downstream generated client");

    let output = client
        .empty_chunk_body(Bytes::from_static(b"public-body"))
        .await
        .expect("managed public request body");
    assert_eq!(output, Bytes::from_static(b"ok"));
    server
        .wait_for_requests(1, Duration::from_secs(10))
        .expect("one bounded body request");
    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].body(), b"public-body");
    // The representation owns no media type, so none is sent.
    assert_eq!(requests[0].header("content-type"), None);
}

#[tokio::test(flavor = "current_thread")]
#[allow(
    clippy::await_holding_lock,
    reason = "the process-global extension probes require whole-test serialization"
)]
async fn endpoint_status_policy_can_accept_a_non_2xx_response() {
    use concord_test_support::server::{ResponseScript, ScriptedServer};

    let _guard = EXTENSION_TEST_LOCK
        .lock()
        .expect("extension test lock poisoned");
    let server = ScriptedServer::start([ResponseScript::status(404, b"accepted-missing".to_vec())])
        .expect("bounded accepted-status server");
    let proxy = SafeProxy::parse(server.url("")).expect("credential-free loopback proxy URL");
    let client = extension_client(Some(proxy)).expect("downstream generated client");

    let response = client
        .generic_body(Bytes::from_static(b"request"))
        .response()
        .await
        .expect("public declared accepted status");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(*response.value(), b"accepted-missing".len());
}

#[cfg(feature = "records")]
#[tokio::test(flavor = "current_thread")]
async fn records_public_decoder_some_from_empty_buffer_fails() {
    use concord::response::StreamErrorKind;
    use concord_test_support::server::{ResponseScript, ScriptedServer};

    let server =
        ScriptedServer::start([ResponseScript::ok("")]).expect("bounded loopback record server");
    let proxy = SafeProxy::parse(server.url("")).expect("credential-free loopback proxy URL");
    let client = RecordExtensionApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("public record-extension client");
    let mut records = client
        .invalid_decoder()
        .await
        .expect("successful streaming response");

    let error = std::future::poll_fn(|context| Pin::new(&mut records).poll_next(context))
        .await
        .expect("one terminal record error")
        .expect_err("empty-buffer record must violate progress");
    // v3 stream consumption failures are correlated `ApiError`s carrying the
    // structural record failure in their stream summary (§15).
    assert_eq!(error.failure_stage(), FailureStage::StreamConsumption);
    assert_eq!(
        error
            .stream_summary()
            .and_then(|summary| summary.error_kind()),
        Some(StreamErrorKind::Internal)
    );
    assert_eq!(error.last_response_status(), Some(StatusCode::OK));
    let rendered = format!("{error} {error:?}");
    assert!(!rendered.contains("public-no-progress-record-sentinel"));
    assert!(
        std::future::poll_fn(|context| Pin::new(&mut records).poll_next(context))
            .await
            .is_none()
    );
}

#[cfg(feature = "default-tls")]
#[tokio::test(flavor = "current_thread")]
#[allow(
    clippy::await_holding_lock,
    reason = "the process-global extension probes require whole-test serialization"
)]
async fn downstream_credential_limiter_and_hooks_execute_through_generated_runtime() {
    use concord_test_support::proxy::ConnectProxy;
    use concord_test_support::tls::TlsServer;

    const CREDENTIAL_SENTINEL: &str = "extension-runtime-credential-sentinel-5ce18a";

    let _guard = EXTENSION_TEST_LOCK
        .lock()
        .expect("extension test lock poisoned");
    let tls = TlsServer::start().expect("trusted application TLS fixture");
    let proxy = ConnectProxy::start(tls.address()).expect("bounded CONNECT proxy");
    let safe_proxy = SafeProxy::parse(proxy.url()).expect("loopback proxy URL");
    let certificate = tls.certificate_pem();

    reset_extension_runtime_probes();
    let client = SecureExtensionApi::builder()
        .session(DirectExtensionProvider::new(
            SecretString::try_new(CREDENTIAL_SENTINEL).expect("bounded extension credential"),
        ))
        .configure_http(move |http| http.proxy(safe_proxy).add_root_certificate_pem(certificate))
        .configure_runtime(|runtime| {
            let observation = Arc::new(ExtensionHooks);
            runtime
                .rate_limiter(Arc::new(ExtensionLimiter))
                .operation_observer(observation.clone())
                .operation_observer(observation.clone())
                .operation_observer(observation)
        })
        .build()
        .expect("public-only extension runtime client");
    let output = client
        .protected()
        .await
        .expect("generated protected extension request");
    assert_eq!(output, Bytes::from_static(b"ok"));

    tls.wait_for_requests(1, Duration::from_secs(10))
        .expect("TLS application request");
    let requests = tls.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].target(), "/");
    assert_eq!(requests[0].header("x-session"), Some(CREDENTIAL_SENTINEL));
    assert_eq!(extension_runtime_probe_counts(), (1, 1, 1, 1, 1, 1));

    drop(client);
    tokio::task::yield_now().await;
    drop(proxy);
}

// ---------------------------------------------------------------------------
// V3-L14: downstream-generated named requests implement the open common
// controls themselves; one generic helper serves several distinct generated
// endpoint types, a response terminal and collection terminals (§23.2).
// ---------------------------------------------------------------------------

/// The §23.2 shared execution helper, verbatim.
pub async fn execute<R, T>(request: R, options: &CallOptions) -> Result<T, ApiError>
where
    R: RequestControls + IntoFuture<Output = Result<T, ApiError>>,
{
    request.with_options(options).await
}

fn assert_common_controls<R, T>()
where
    R: RequestControls + IntoFuture<Output = Result<T, ApiError>>,
{
}

#[tokio::test(flavor = "current_thread")]
#[allow(
    clippy::await_holding_lock,
    reason = "the process-global extension probes require whole-test serialization"
)]
async fn v3_l14_downstream_named_requests_pass_the_generic_execute_helper() {
    use concord_test_support::server::{ResponseScript, ScriptedServer};

    // Compile-time: each downstream-generated named request type itself
    // implements `RequestControls` and `IntoFuture` (no library stand-in).
    assert_common_controls::<extension_api::GenericBodyRequest, usize>();
    assert_common_controls::<extension_api::EmptyChunkBodyRequest, Bytes>();
    assert_common_controls::<extension_api::PagesRequest, ExtensionPage>();
    assert_common_controls::<extension_api::GenericDiscardRequest, ()>();
    assert_common_controls::<ResponseRequest<usize>, concord::ApiResponse<usize>>();
    assert_common_controls::<CollectAll<u8>, Vec<u8>>();

    let _guard = EXTENSION_TEST_LOCK
        .lock()
        .expect("extension test lock poisoned");
    let page = |body: &str| {
        ResponseScript::ok(body.to_owned())
            .with_header("content-type", "application/x-concord-page")
    };
    let server = ScriptedServer::start([
        ResponseScript::ok("generic-helper"),
        ResponseScript::ok("chunk-helper"),
        ResponseScript::status(404, b"response-helper".to_vec()),
        page("4,5|more"),
        page("6|-"),
        page("7|more"),
    ])
    .expect("helper server");
    let proxy = SafeProxy::parse(server.url("")).expect("proxy");
    let client = extension_client(Some(proxy)).expect("downstream generated client");
    let options = CallOptions::new().cap_sends(4);

    // Two distinct generated endpoint request types.
    let generic: usize = execute(client.generic_body(Bytes::from_static(b"a")), &options)
        .await
        .expect("generic body endpoint");
    assert_eq!(generic, "generic-helper".len());
    let chunk: Bytes = execute(client.empty_chunk_body(Bytes::from_static(b"b")), &options)
        .await
        .expect("empty-chunk endpoint");
    assert_eq!(chunk, Bytes::from_static(b"chunk-helper"));

    // A response terminal.
    let response = execute(
        client.generic_body(Bytes::from_static(b"c")).response(),
        &options,
    )
    .await
    .expect("response terminal");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // A collection terminal; the generated setters live on the named request.
    let items: Vec<u8> = execute(
        client
            .pages()
            .set_cursor("start")
            .collect_all(NonZeroU32::new(5).expect("nonzero")),
        &options,
    )
    .await
    .expect("collection terminal");
    assert_eq!(items, [4, 5, 6]);

    // Helper caps propagate into the traversal and are never widened: one
    // send cannot complete a two-page traversal.
    let capped = execute(
        client
            .pages()
            .replace_cursor(None)
            .collect_all(NonZeroU32::new(5).expect("nonzero")),
        &CallOptions::new().cap_sends(1),
    )
    .await
    .expect_err("the send cap applies across pages");
    assert!(
        capped.category() == ErrorCategory::Pagination
            || capped.category() == ErrorCategory::SendCapExhausted,
        "{capped:?}"
    );
    let requests = server.requests();
    assert_eq!(requests.len(), 6);
    assert!(requests[3].target().contains("cursor=start"));
    assert!(requests[4].target().contains("cursor=more"));
    assert!(!requests[5].target().contains("cursor="));
}

// ---------------------------------------------------------------------------
// V3-E16: the provider helper reaches only exact declared origins.
// ---------------------------------------------------------------------------

#[cfg(feature = "default-tls")]
mod provider_http {
    use super::*;
    use concord_test_support::tls::TlsServer;

    /// Provider scripted to probe the helper's authority, recording each
    /// helper outcome by category only.
    pub struct AllowlistProvider {
        pub origins: Vec<ProviderOrigin>,
        pub declared: String,
        pub undeclared: String,
        pub outcomes: Arc<Mutex<Vec<String>>>,
        pub body: &'static [u8],
        pub sends: usize,
    }

    impl CredentialProvider for AllowlistProvider {
        type Material = ExtensionCredential;

        fn acquire<'a>(
            &'a self,
            context: CredentialRequest<'a>,
        ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
            Box::pin(async move {
                let record = |entry: String| self.outcomes.lock().expect("outcomes").push(entry);
                // An undeclared origin is refused before any transport or
                // body-factory work.
                let undeclared = context
                    .http
                    .send(
                        ProviderRequest::new(
                            Method::POST,
                            Url::parse(&self.undeclared).expect("undeclared URL"),
                        )
                        .body(ProviderBody::Bytes(Bytes::from_static(self.body))),
                    )
                    .await;
                record(format!(
                    "undeclared:{:?}",
                    undeclared.err().map(|error| error.kind())
                ));
                let mut token = None;
                for _ in 0..self.sends {
                    let sent = context
                        .http
                        .send(
                            ProviderRequest::new(
                                Method::POST,
                                Url::parse(&self.declared).expect("declared URL"),
                            )
                            .body(ProviderBody::Bytes(Bytes::from_static(self.body))),
                        )
                        .await;
                    match sent {
                        Ok(response) => {
                            record(format!("declared:{}", response.status().as_u16()));
                            if response.status() == StatusCode::OK {
                                token = Some(response.into_body());
                            }
                        }
                        Err(error) => record(format!("declared-error:{:?}", error.kind())),
                    }
                }
                let token = token.ok_or_else(|| AuthError::new(AuthErrorKind::Acquisition))?;
                let text = String::from_utf8(token.to_vec())
                    .map_err(|_| AuthError::new(AuthErrorKind::Acquisition))?;
                Ok(ExtensionCredential(
                    SecretString::try_new(text).map_err(AuthError::from)?,
                ))
            })
        }

        fn http_origins(&self) -> &[ProviderOrigin] {
            &self.origins
        }
    }

    /// Stops every application attempt at admission, after credential
    /// preflight, so provider behavior is observed without app traffic.
    pub struct DenyingLimiter;

    impl RateLimiter for DenyingLimiter {
        fn capability(&self) -> LimiterCapability {
            LimiterCapability::new(Enforcement::Local, "concord.tests.deny", 1)
        }

        fn acquire<'a>(
            &'a self,
            _request: RateLimitRequest<'a>,
        ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
            Box::pin(async { Err(RateLimitError::new(RateLimitErrorKind::Acquire)) })
        }
    }

    api! {
        client ProviderHttpApi {
            timeout 30s;
            base "https://application.invalid/";
            credentials {
                credential session = custom AllowlistProvider;
            }
            rate_limit { id "concord.contracts.provider"; key [origin]; cost 1; allow 10 per 1s; }

            GET protected() -> Bytes via Binary {
                path "protected";
                auth { header "X-Session" = session; }
            }
        }
    }

    pub struct Fixture {
        pub declared: TlsServer,
        pub undeclared: TlsServer,
        pub outcomes: Arc<Mutex<Vec<String>>>,
    }

    pub fn run(
        fixture: &Fixture,
        body: &'static [u8],
        sends: usize,
        configure: impl FnOnce(
            concord::runtime::RuntimeConfigBuilder,
        ) -> concord::runtime::RuntimeConfigBuilder,
    ) -> ProviderHttpApi {
        let origin = ProviderOrigin::parse(fixture.declared.url("localhost", ""))
            .expect("declared loopback origin");
        let certificate = fixture.declared.certificate_pem();
        ProviderHttpApi::builder()
            .session(AllowlistProvider {
                origins: vec![origin],
                declared: fixture.declared.url("localhost", "/token"),
                undeclared: fixture.undeclared.url("localhost", "/stolen"),
                outcomes: Arc::clone(&fixture.outcomes),
                body,
                sends,
            })
            .configure_http(move |http| http.http1_only().add_root_certificate_pem(certificate))
            .configure_runtime(move |runtime| {
                configure(runtime.rate_limiter(Arc::new(DenyingLimiter)))
            })
            .build()
            .expect("provider helper client")
    }

    pub fn fixture() -> Fixture {
        Fixture {
            declared: TlsServer::start().expect("declared provider origin"),
            undeclared: TlsServer::start().expect("undeclared provider origin"),
            outcomes: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[cfg(feature = "default-tls")]
#[tokio::test]
async fn v3_e16_provider_helper_reaches_only_declared_origins_and_never_redirects() {
    let fixture = provider_http::fixture();
    let undeclared_port = fixture.undeclared.address().port();
    fixture
        .declared
        .queue_response_with_headers(
            302,
            &[(
                "location",
                &format!("https://localhost:{undeclared_port}/redirected"),
            )],
            b"moved".to_vec(),
        )
        .expect("redirect response");
    fixture
        .declared
        .queue_response(200, b"helper-token-sentinel".to_vec())
        .expect("token response");
    let client = provider_http::run(&fixture, b"grant=fake", 2, |runtime| runtime);

    let error = client
        .protected()
        .await
        .expect_err("admission stops the application request");
    assert_eq!(error.category(), ErrorCategory::RateLimit);
    assert_eq!(error.failure_stage(), FailureStage::RateAdmission);
    assert_eq!(
        fixture.outcomes.lock().expect("outcomes").as_slice(),
        [
            "undeclared:Some(InvalidConfiguration)".to_owned(),
            "declared:302".to_owned(),
            "declared:200".to_owned(),
        ]
    );
    assert!(
        fixture.undeclared.requests().is_empty(),
        "neither the denied origin nor the redirect target was contacted"
    );
    let requests = fixture.declared.requests();
    assert_eq!(requests.len(), 2);
    for request in &requests {
        assert_eq!(request.method(), "POST");
        assert_eq!(request.target(), "/token");
        assert_eq!(request.body(), b"grant=fake");
        // No application credential placement or cookie identity.
        assert_eq!(request.header("x-session"), None);
        assert_eq!(request.header("authorization"), None);
        assert_eq!(request.header("cookie"), None);
    }
    assert!(!format!("{error} {error:?}").contains("helper-token-sentinel"));
}

#[cfg(feature = "default-tls")]
#[tokio::test]
async fn v3_e16_provider_helper_send_and_body_caps_stop_before_transport() {
    let fixture = provider_http::fixture();
    fixture
        .declared
        .queue_response(200, b"first-token".to_vec())
        .expect("token response");
    let client = provider_http::run(&fixture, b"grant=fake", 2, |runtime| {
        runtime.provider_max_sends(1)
    });
    let error = client.protected().await.expect_err("admission stops");
    assert_eq!(error.category(), ErrorCategory::RateLimit);
    assert_eq!(
        fixture.outcomes.lock().expect("outcomes").as_slice(),
        [
            "undeclared:Some(InvalidConfiguration)".to_owned(),
            "declared:200".to_owned(),
            "declared-error:Acquisition".to_owned(),
        ],
        "the refused undeclared request consumed no send; the cap stops the next"
    );
    assert_eq!(fixture.declared.requests().len(), 1);

    let fixture = provider_http::fixture();
    let client = provider_http::run(&fixture, b"grant=oversized-provider-body", 1, |runtime| {
        runtime.provider_request_limit(8)
    });
    let error = client
        .protected()
        .await
        .expect_err("provider cannot obtain material");
    assert_eq!(error.category(), ErrorCategory::AuthenticationProvider);
    assert_eq!(error.failure_stage(), FailureStage::CredentialPreflight);
    assert_eq!(
        fixture.outcomes.lock().expect("outcomes").as_slice(),
        [
            "undeclared:Some(InvalidConfiguration)".to_owned(),
            "declared-error:RequestTooLarge".to_owned(),
        ]
    );
    assert!(fixture.declared.requests().is_empty());
    assert!(fixture.undeclared.requests().is_empty());
}
