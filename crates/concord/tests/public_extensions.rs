#![forbid(unsafe_code)]

#[cfg(feature = "records")]
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

#[cfg(feature = "json")]
use concord::advanced::Json;
use concord::advanced::{
    Auth, AuthError, AuthErrorKind, BeforeSend, BodyError, BodyFactory, BodyKind, BoxFuture,
    Buffered, ByteStream, CodecError, CompletionEvent, Cooldown, CooldownScope, CredentialMaterial,
    CredentialProvider, CredentialRequest, DecodeBody, DecodeContext, Discard, EncodeBody,
    EncodeContext, HasNextCursor, HeaderName, HeaderValue, HookError, LengthHint, MediaType,
    Method, PaginationCompletionEvent, PaginationController, PaginationError, ProviderBody,
    ProviderBodyFactory, ProviderRequest, RateLimitError, RateLimitObserver, RateLimitPermit,
    RateLimitRequest, RateLimitResponse, RateLimitResponseHead, RateLimiter, RequestBody,
    RequestBodyContext, RequestBodyMetadata, RequestBodySpec, RequestContentType,
    ResponseBodyError, ResponseBodySpec, ResponseHead, ResponseHeadEvent, RuntimeEndpoint,
    RuntimeHooks, SafeProxy, SecretCredential, StatusCode, Streaming, Url,
};
#[cfg(feature = "records")]
use concord::advanced::{BytesMut, RecordDecoder, RecordError};
#[cfg(feature = "multipart")]
use concord::advanced::{MultipartBody, MultipartFactory, MultipartPart};
use concord::prelude::{Bytes, ClientBuildError, Request, SecretString, StreamBody};

pub struct ExtensionRequestSpec;

impl RequestBodySpec for ExtensionRequestSpec {
    type Input = Bytes;

    fn metadata(
        input: &Self::Input,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        Ok(RequestBodyMetadata {
            length: LengthHint::Exact(u64::try_from(input.len()).unwrap_or(u64::MAX)),
            content_type: RequestContentType::None,
            kind: BodyKind::Buffered,
        })
    }

    fn prepare(
        input: Self::Input,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBody, BodyError> {
        Ok(RequestBody::bytes(input))
    }
}

pub struct ExtensionBufferedSpec;

impl ResponseBodySpec for ExtensionBufferedSpec {
    type Output = usize;
    type Mode = Buffered;

    fn from_body(body: Bytes, _head: &ResponseHead<'_>) -> Result<usize, ResponseBodyError> {
        Ok(body.len())
    }
}

pub struct ExtensionStreamingSpec;

impl ResponseBodySpec for ExtensionStreamingSpec {
    type Output = ByteStream;
    type Mode = Streaming;

    fn from_body(
        body: ByteStream,
        _head: &ResponseHead<'_>,
    ) -> Result<ByteStream, ResponseBodyError> {
        Ok(body)
    }
}

pub struct ExtensionDiscardSpec;

impl ResponseBodySpec for ExtensionDiscardSpec {
    type Output = ();
    type Mode = Discard;

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

pub struct ExtensionMedia;

impl MediaType for ExtensionMedia {
    const MEDIA_TYPE: &'static str = "application/x-concord-extension";
}

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
        if content_type == Self::ACCEPT {
            Ok(())
        } else {
            Err(CodecError::new("extension media mismatch"))
        }
    }

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        String::from_utf8(body.to_vec()).map_err(|_| CodecError::new("extension UTF-8"))
    }
}

pub struct ExtensionBodyFactory;

impl BodyFactory for ExtensionBodyFactory {
    fn content_type(&self) -> Option<HeaderValue> {
        Some(HeaderValue::from_static("application/x-concord-extension"))
    }

    fn create(&self) -> Result<StreamBody, BodyError> {
        Ok(StreamBody::from_bytes(Bytes::from_static(b"factory")))
    }
}

#[cfg(feature = "multipart")]
pub struct ExtensionMultipartFactory;

#[cfg(feature = "multipart")]
impl MultipartFactory for ExtensionMultipartFactory {
    fn create(&self) -> Result<MultipartBody, BodyError> {
        MultipartBody::new().part("extension", MultipartPart::text("multipart"))
    }
}

#[derive(Clone)]
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

pub struct ExtensionProvider;

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
            Ok(ExtensionCredential(SecretString::new(value)))
        })
    }
}

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
        let _bounded_provider_capability = context.http;
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

pub struct ExtensionPageCodec;

pub struct ExtensionPage {
    items: Vec<u8>,
    next: Option<String>,
}

impl IntoIterator for ExtensionPage {
    type Item = u8;
    type IntoIter = std::vec::IntoIter<u8>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
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
            std::str::from_utf8(&body).map_err(|_| CodecError::new("extension page UTF-8"))?;
        let (items, next) = text
            .split_once('|')
            .ok_or_else(|| CodecError::new("extension page structure"))?;
        let items = if items.is_empty() {
            Vec::new()
        } else {
            items
                .split(',')
                .map(|item| {
                    item.parse::<u8>()
                        .map_err(|_| CodecError::new("extension page item"))
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
static EXTENSION_VALIDATION_CALLS: AtomicUsize = AtomicUsize::new(0);
static EXTENSION_NEXT_CALLS: AtomicUsize = AtomicUsize::new(0);

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
    EXTENSION_VALIDATION_CALLS.store(0, Ordering::SeqCst);
    EXTENSION_NEXT_CALLS.store(0, Ordering::SeqCst);
}

#[must_use]
pub fn extension_pagination_probe_counts() -> (usize, usize, usize) {
    (
        EXTENSION_CONSTRUCTION_EVALUATIONS.load(Ordering::SeqCst),
        EXTENSION_VALIDATION_CALLS.load(Ordering::SeqCst),
        EXTENSION_NEXT_CALLS.load(Ordering::SeqCst),
    )
}

pub struct ExtensionController {
    pub cursor: Option<String>,
    pub construction: ExtensionConstructionProbe,
}

impl PaginationController<ExtensionPage> for ExtensionController {
    fn validate(&self) -> Result<(), PaginationError> {
        EXTENSION_VALIDATION_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn next(self, page: &ExtensionPage) -> Result<Option<Self>, PaginationError> {
        EXTENSION_NEXT_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(page.next_cursor().cloned().map(|cursor| Self {
            cursor: Some(cursor),
            construction: self.construction,
        }))
    }
}

pub struct ExtensionLimiter;

impl RateLimiter for ExtensionLimiter {
    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        EXTENSION_LIMITER_ACQUIRES.fetch_add(1, Ordering::SeqCst);
        let _bounded_rule_count = request.plan.rules().len();
        Box::pin(async { Ok(RateLimitPermit::granted()) })
    }

    fn observe<'a>(&'a self, response: RateLimitResponse<'a>) -> BoxFuture<'a, ()> {
        EXTENSION_LIMITER_OBSERVATIONS.fetch_add(1, Ordering::SeqCst);
        let _status = response.status;
        Box::pin(async {})
    }
}

#[derive(Default)]
pub struct ExtensionObserver;

impl RateLimitObserver for ExtensionObserver {
    fn observe(&self, response: &RateLimitResponseHead<'_>) -> Option<Cooldown> {
        (response.status == StatusCode::TOO_MANY_REQUESTS).then_some(Cooldown {
            duration: Duration::from_secs(1),
            scope: CooldownScope::CurrentBuckets,
        })
    }
}

pub struct ExtensionHooks;

impl RuntimeHooks for ExtensionHooks {
    fn before_send<'a>(&'a self, _context: BeforeSend<'a>) -> BoxFuture<'a, Result<(), HookError>> {
        EXTENSION_HOOK_BEFORE.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(()) })
    }

    fn on_response_head<'a>(&'a self, _context: ResponseHeadEvent<'a>) -> BoxFuture<'a, ()> {
        EXTENSION_HOOK_RESPONSE.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {})
    }

    fn on_complete<'a>(&'a self, _context: CompletionEvent<'a>) -> BoxFuture<'a, ()> {
        EXTENSION_HOOK_COMPLETION.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {})
    }

    fn on_pagination_complete<'a>(
        &'a self,
        _context: PaginationCompletionEvent<'a>,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async {})
    }
}

concord::api! {
    client ExtensionApi {
        base "http://extension.example.test/v1/";
        vars {
            construction: crate::ExtensionConstructionProbe;
        }
        credentials {
            credential session = custom crate::ExtensionProvider;
        }
        rate_limit { 10 / 1s; }
    }

    GET protected() -> Bytes {
        auth header "X-Session" = session challenge unauthorized;
    }

    GET pages(cursor: Option<String>) -> crate::ExtensionPageCodec {
        query { "cursor" = ep.cursor; }
        paginate crate::ExtensionController {
            cursor: bind ep.cursor,
            construction: vars.construction,
        }
    }

    POST generic_body(#[body] body: crate::ExtensionRequestSpec) -> crate::ExtensionBufferedSpec;
    GET generic_stream() -> crate::ExtensionStreamingSpec;
    GET generic_discard() -> crate::ExtensionDiscardSpec;
}

concord::api! {
    client SecureExtensionApi {
        base "https://localhost/";
        credentials {
            credential session = custom crate::ExtensionProvider;
        }
        rate_limit { 10 / 1s; }
    }

    GET protected() -> Bytes {
        auth header "X-Session" = session challenge unauthorized;
    }
}

#[cfg(feature = "records")]
concord::api! {
    client RecordExtensionApi {
        base "http://record-extension.example.test/";
    }

    GET custom() -> Records<crate::ExtensionRecordDecoder>;
}

#[cfg(feature = "csv")]
concord::api! {
    client CsvExtensionApi {
        base "http://csv-extension.example.test/";
    }

    GET rows() -> Records<Csv<Vec<String>>>;
}

#[cfg(feature = "ndjson")]
concord::api! {
    client NdJsonExtensionApi {
        base "http://ndjson-extension.example.test/";
    }

    GET values() -> Records<NdJson<u32>>;
}

/// Compiles every stable extension workflow without private or native integration imports.
pub fn certify_public_extensions() -> Result<(), ClientBuildError> {
    let client = ExtensionApi::builder()
        .construction(ExtensionConstructionProbe::new())
        .session_provider(ExtensionProvider)
        .configure_http(|http| {
            http.proxy(
                SafeProxy::parse("http://127.0.0.1:43119")
                    .expect("fixed credential-free proxy origin"),
            )
        })
        .configure_runtime(|runtime| {
            runtime
                .rate_limiter(Arc::new(ExtensionLimiter))
                .hooks(Arc::new(ExtensionHooks))
        })
        .build()?;

    let credential = client.credentials().session().credential();
    let _: extension_api::ProtectedRequest = client.protected();
    let _: extension_api::GenericBodyRequest =
        client.generic_body(Bytes::from_static(b"generic extension input"));
    let _: extension_api::GenericStreamRequest = client.generic_stream();
    let _: extension_api::GenericDiscardRequest = client.generic_discard();
    let _: Auth = Auth::header(
        HeaderName::from_static("x-prepared-session"),
        credential.clone(),
    );

    let buffered = RuntimeEndpoint::builder("extension_api.runtime_buffered".into(), Method::POST)
        .path_segment("buffered")
        .body_codec::<ExtensionCodec>("bounded-input".to_owned())
        .auth(Auth::header(
            HeaderName::from_static("x-prepared-session"),
            credential,
        ))
        .response::<ExtensionCodec>()
        .build()
        .expect("valid buffered runtime endpoint");
    let _: Request<_> = client.request(buffered);

    let bytes = RuntimeEndpoint::builder("extension_api.runtime_bytes".into(), Method::POST)
        .path_segment("bytes")
        .body(RequestBody::factory(ExtensionBodyFactory))
        .response_bytes()
        .build()
        .expect("valid bytes runtime endpoint");
    let _: Request<_> = client.request(bytes);

    let no_content = RuntimeEndpoint::builder("extension_api.runtime_none".into(), Method::DELETE)
        .path_segment("none")
        .build()
        .expect("valid no-content runtime endpoint");
    let _: Request<_> = client.request(no_content);

    let stream = RuntimeEndpoint::builder("extension_api.runtime_stream".into(), Method::GET)
        .path_segment("stream")
        .response_stream::<ExtensionMedia>()
        .build()
        .expect("valid streaming runtime endpoint");
    let _: Request<_> = client.request(stream);

    let generic = RuntimeEndpoint::builder("extension_api.runtime_generic".into(), Method::POST)
        .path_segment("generic")
        .body_spec::<ExtensionRequestSpec>(Bytes::from_static(b"prepared generic input"))
        .response::<ExtensionBufferedSpec>()
        .build()
        .expect("valid generic runtime endpoint");
    let _: Request<_> = client.request(generic);

    let page = ExtensionPage {
        items: vec![1, 2],
        next: Some("next".to_owned()),
    };
    let controller = ExtensionController {
        cursor: None,
        construction: ExtensionConstructionProbe::new(),
    };
    let _next_cursor: Option<&String> = page.next_cursor();
    let _next_controller: Option<ExtensionController> = controller
        .next(&page)
        .expect("bounded controller transition");

    #[cfg(feature = "json")]
    {
        let json = RuntimeEndpoint::builder("extension_api.runtime_json".into(), Method::POST)
            .body_codec::<Json<Vec<u32>>>(vec![1, 2, 3])
            .response::<Json<Vec<u32>>>()
            .build()
            .expect("valid JSON runtime endpoint");
        let _: Request<_> = client.request(json);
    }

    #[cfg(feature = "multipart")]
    {
        let multipart =
            RuntimeEndpoint::builder("extension_api.runtime_multipart".into(), Method::POST)
                .body(RequestBody::multipart_factory(ExtensionMultipartFactory))
                .build()
                .expect("valid multipart runtime endpoint");
        let _: Request<_> = client.request(multipart);
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
    }

    #[cfg(feature = "ndjson")]
    {
        let records = NdJsonExtensionApi::new().expect("NDJSON extension client");
        let _: nd_json_extension_api::ValuesRequest = records.values();
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

    let client = ExtensionApi::builder()
        .construction(ExtensionConstructionProbe::new())
        .session_provider(ExtensionProvider)
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(ExtensionLimiter)))
        .build()
        .expect("downstream generated client");
    let pagination = client.pages().cursor("initial".to_owned()).paginate(10_u32);

    reset_extension_pagination_probes();
    let items = pagination
        .await
        .expect("public-only downstream pagination runtime");
    server
        .wait_for_requests(2, Duration::from_secs(10))
        .expect("two bounded pagination requests");
    let requests = server.requests();
    let probe_counts = extension_pagination_probe_counts();

    assert_eq!(items, [1, 2, 3]);
    assert_eq!(requests.len(), 2);
    assert!(requests[0].target().contains("cursor=initial"));
    assert!(requests[1].target().contains("cursor=continuation"));
    assert_eq!(probe_counts, (1, 2, 2));
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
        .session_provider(DirectExtensionProvider::new(SecretString::new(
            CREDENTIAL_SENTINEL,
        )))
        .configure_http(move |http| http.proxy(safe_proxy).add_root_certificate_pem(certificate))
        .configure_runtime(|runtime| {
            runtime
                .rate_limiter(Arc::new(ExtensionLimiter))
                .hooks(Arc::new(ExtensionHooks))
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
