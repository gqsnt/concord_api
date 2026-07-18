#![cfg(feature = "default-tls")]

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bytes::Bytes;
use concord_test_support::executor::DeterministicExecutor;
use concord_test_support::gate::Gate;
use concord_test_support::script::{CapturedHeaderValue, ExpectedRequest, ScriptedResponse};
use concord_test_support::tls::TlsServer;
use http::header::AUTHORIZATION;
use http::{HeaderValue, Method, StatusCode};

use crate::auth::{
    AccessToken, ApiKey, Auth, AuthError, BasicCredential, ChallengePolicy, CredentialMaterial,
    CredentialProvider, CredentialReason, CredentialRef, CredentialRequest, RecoveryMode,
    SecretCredential, SecretString, SlotSelector,
};
use crate::body::{BodyError, BodyFactory, RequestBody, StreamBody};
#[cfg(feature = "multipart")]
use crate::body::{MultipartBody, MultipartPart};
use crate::client::ClientCore;
use crate::codec::OctetStream;
use crate::error::{ApiError, AuthErrorKind, ErrorCategory};
use crate::hooks::{
    BeforeSend, BoxFuture, CompletionEvent, DebugLevel, ResponseHeadEvent, RuntimeHooks,
};
use crate::http::HttpConfigBuilder;
use crate::http::provider::{ProviderBody, ProviderRequest};
use crate::operation::OperationId;
use crate::request::{
    BodyMetadata, BodySpec, BytesResponse, EndpointSpec, HeadBuilder, PreparedHead, Request,
    RequestMeta, StreamResponseHandler,
};
use crate::runtime::RuntimeConfigBuilder;

const ENDPOINT: &str = "AuthenticationApi::protected";
const INITIAL: &str = "initial-auth-token-1148c7";
const REFRESHED: &str = "refreshed-auth-token-709fe2";
const NEWER: &str = "newer-auth-token-265f0a";

struct ProtectedEndpoint {
    credential: CredentialRef<AccessToken>,
    challenge: ChallengePolicy,
    body: RequestBody,
    query: bool,
}

struct ProtectedStreamEndpoint {
    credential: CredentialRef<AccessToken>,
}

struct BasicEndpoint {
    credential: CredentialRef<BasicCredential>,
}

impl EndpointSpec for BasicEndpoint {
    type Body = RequestBody;
    type Response = BytesResponse;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(ENDPOINT, Method::GET, Some(Duration::from_secs(30)))
    }

    fn prepare_head_and_body_metadata(
        &self,
        mut builder: HeadBuilder,
    ) -> Result<(PreparedHead, BodyMetadata), ApiError> {
        builder.push_static("basic")?;
        builder.push_auth(Auth::basic(self.credential.clone()))?;
        let body = RequestBody::bytes(Bytes::new());
        Ok((builder.finish(), body.metadata(ENDPOINT, &Method::GET)?))
    }

    fn into_body(self) -> Self::Body {
        RequestBody::bytes(Bytes::new())
    }
}

struct HeaderEndpoint {
    credential: CredentialRef<ApiKey>,
}

impl EndpointSpec for HeaderEndpoint {
    type Body = RequestBody;
    type Response = BytesResponse;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(ENDPOINT, Method::GET, Some(Duration::from_secs(30)))
    }

    fn prepare_head_and_body_metadata(
        &self,
        mut builder: HeadBuilder,
    ) -> Result<(PreparedHead, BodyMetadata), ApiError> {
        builder.push_static("header")?;
        builder.push_auth(Auth::header(
            http::HeaderName::from_static("x-api-key"),
            self.credential.clone(),
        ))?;
        let body = RequestBody::bytes(Bytes::new());
        Ok((builder.finish(), body.metadata(ENDPOINT, &Method::GET)?))
    }

    fn into_body(self) -> Self::Body {
        RequestBody::bytes(Bytes::new())
    }
}

impl EndpointSpec for ProtectedStreamEndpoint {
    type Body = RequestBody;
    type Response = StreamResponseHandler<OctetStream>;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(ENDPOINT, Method::GET, Some(Duration::from_secs(30)))
    }

    fn prepare_head_and_body_metadata(
        &self,
        mut builder: HeadBuilder,
    ) -> Result<(PreparedHead, BodyMetadata), ApiError> {
        builder.push_static("protected-stream")?;
        builder.push_auth(
            Auth::bearer(self.credential.clone()).challenge(ChallengePolicy::Unauthorized),
        )?;
        let body = RequestBody::bytes(Bytes::new());
        Ok((builder.finish(), body.metadata(ENDPOINT, &Method::GET)?))
    }

    fn into_body(self) -> Self::Body {
        RequestBody::bytes(Bytes::new())
    }
}

struct MultiAuthEndpoint {
    first: CredentialRef<AccessToken>,
    second: CredentialRef<AccessToken>,
    first_challenge: ChallengePolicy,
    body_calls: Arc<AtomicUsize>,
}

impl EndpointSpec for MultiAuthEndpoint {
    type Body = RequestBody;
    type Response = BytesResponse;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(ENDPOINT, Method::POST, Some(Duration::from_secs(30)))
    }

    fn prepare_head_and_body_metadata(
        &self,
        mut builder: HeadBuilder,
    ) -> Result<(PreparedHead, BodyMetadata), ApiError> {
        builder.push_static("protected")?;
        builder.push_auth(Auth::bearer(self.first.clone()).challenge(self.first_challenge))?;
        builder.push_auth(Auth::header(
            http::HeaderName::from_static("x-secondary-token"),
            self.second.clone(),
        ))?;
        let body = RequestBody::factory(ReplayFactory {
            calls: Arc::clone(&self.body_calls),
        });
        Ok((builder.finish(), body.metadata(ENDPOINT, &Method::POST)?))
    }

    fn into_body(self) -> Self::Body {
        RequestBody::factory(ReplayFactory {
            calls: self.body_calls,
        })
    }
}

struct DynamicCollisionEndpoint {
    credential: CredentialRef<AccessToken>,
    body_calls: Arc<AtomicUsize>,
}

impl EndpointSpec for DynamicCollisionEndpoint {
    type Body = RequestBody;
    type Response = BytesResponse;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(ENDPOINT, Method::POST, Some(Duration::from_secs(30)))
    }

    fn prepare_head_and_body_metadata(
        &self,
        mut builder: HeadBuilder,
    ) -> Result<(PreparedHead, BodyMetadata), ApiError> {
        builder.push_auth(Auth::query("access", self.credential.clone()))?;
        builder.push_query_value("access", &"materially-present-public-value");
        let body = RequestBody::factory(ReplayFactory {
            calls: Arc::clone(&self.body_calls),
        });
        Ok((builder.finish(), body.metadata(ENDPOINT, &Method::POST)?))
    }

    fn into_body(self) -> Self::Body {
        RequestBody::factory(ReplayFactory {
            calls: self.body_calls,
        })
    }
}

#[derive(Clone)]
struct FinalizationExpiryToken {
    value: SecretString,
    freshness_checks: Arc<AtomicUsize>,
}

impl CredentialMaterial for FinalizationExpiryToken {
    fn expires_at(&self) -> Option<Instant> {
        (self.freshness_checks.fetch_add(1, Ordering::SeqCst) > 0)
            .then(|| Instant::now() - Duration::from_secs(1))
    }
}

impl SecretCredential for FinalizationExpiryToken {
    fn expose_secret(&self) -> &str {
        self.value.expose_secret()
    }
}

struct FinalizationExpiryEndpoint {
    credential: CredentialRef<FinalizationExpiryToken>,
}

#[derive(Clone)]
struct ExposureToken {
    value: SecretString,
    debug: Arc<Mutex<Vec<String>>>,
    exposed_after_debug: Arc<AtomicBool>,
}

impl CredentialMaterial for ExposureToken {}

impl SecretCredential for ExposureToken {
    fn expose_secret(&self) -> &str {
        let request_debug_exists = self
            .debug
            .lock()
            .expect("debug output")
            .iter()
            .any(|line| line.contains("request"));
        self.exposed_after_debug
            .store(request_debug_exists, Ordering::SeqCst);
        self.value.expose_secret()
    }
}

struct ExposureEndpoint {
    credential: CredentialRef<ExposureToken>,
}

impl EndpointSpec for ExposureEndpoint {
    type Body = RequestBody;
    type Response = BytesResponse;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(ENDPOINT, Method::GET, Some(Duration::from_secs(30)))
    }

    fn prepare_head_and_body_metadata(
        &self,
        mut builder: HeadBuilder,
    ) -> Result<(PreparedHead, BodyMetadata), ApiError> {
        builder.push_static("exposure")?;
        builder.push_auth(Auth::bearer(self.credential.clone()))?;
        let body = RequestBody::bytes(Bytes::new());
        Ok((builder.finish(), body.metadata(ENDPOINT, &Method::GET)?))
    }

    fn into_body(self) -> Self::Body {
        RequestBody::bytes(Bytes::new())
    }
}

impl EndpointSpec for FinalizationExpiryEndpoint {
    type Body = RequestBody;
    type Response = BytesResponse;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(ENDPOINT, Method::POST, Some(Duration::from_secs(30)))
    }

    fn prepare_head_and_body_metadata(
        &self,
        mut builder: HeadBuilder,
    ) -> Result<(PreparedHead, BodyMetadata), ApiError> {
        builder.push_auth(Auth::bearer(self.credential.clone()))?;
        let body = RequestBody::bytes(Bytes::new());
        Ok((builder.finish(), body.metadata(ENDPOINT, &Method::POST)?))
    }

    fn into_body(self) -> Self::Body {
        RequestBody::bytes(Bytes::new())
    }
}

impl EndpointSpec for ProtectedEndpoint {
    type Body = RequestBody;
    type Response = BytesResponse;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(ENDPOINT, Method::POST, Some(Duration::from_secs(30)))
    }

    fn prepare_head_and_body_metadata(
        &self,
        mut builder: HeadBuilder,
    ) -> Result<(PreparedHead, BodyMetadata), ApiError> {
        builder.push_static("protected")?;
        let auth = if self.query {
            Auth::query("access", self.credential.clone())
        } else {
            Auth::bearer(self.credential.clone())
        };
        builder.push_auth(auth.challenge(self.challenge))?;
        Ok((
            builder.finish(),
            self.body.metadata(ENDPOINT, &Method::POST)?,
        ))
    }

    fn into_body(self) -> Self::Body {
        self.body
    }
}

#[derive(Clone)]
struct HookRecord {
    operation_id: OperationId,
    recovery_index: u8,
    safe_url: String,
    known_body_length: Option<u64>,
}

#[derive(Default)]
struct AuthHooks {
    before: Mutex<Vec<HookRecord>>,
    response: Mutex<Vec<(OperationId, u8, StatusCode)>>,
    completion: Mutex<Vec<(OperationId, bool, Option<StatusCode>)>>,
    observed_ready_during_response_hook: AtomicBool,
    observed_selector: Mutex<Option<Arc<SlotSelector<AccessToken>>>>,
    ready_during_response: Mutex<Vec<bool>>,
    replace_on_first_response: AtomicBool,
    veto_recovery: AtomicBool,
}

impl AuthHooks {
    fn observe(&self, selector: Arc<SlotSelector<AccessToken>>) {
        *self.observed_selector.lock().expect("selector lock") = Some(selector);
    }

    fn replace_on_first_response(&self) {
        self.replace_on_first_response.store(true, Ordering::SeqCst);
    }

    fn veto_recovery(&self) {
        self.veto_recovery.store(true, Ordering::SeqCst);
    }
}

impl RuntimeHooks for AuthHooks {
    fn before_send<'a>(
        &'a self,
        context: BeforeSend<'a>,
    ) -> BoxFuture<'a, Result<(), crate::error::HookError>> {
        self.before
            .lock()
            .expect("before records")
            .push(HookRecord {
                operation_id: context.operation_id,
                recovery_index: context.recovery_index,
                safe_url: context.safe_url.to_owned(),
                known_body_length: context.known_body_length,
            });
        let veto = context.recovery_index == 1 && self.veto_recovery.load(Ordering::SeqCst);
        Box::pin(async move {
            if veto {
                Err(crate::error::HookError::new("recovery veto"))
            } else {
                Ok(())
            }
        })
    }

    fn on_response_head<'a>(&'a self, context: ResponseHeadEvent<'a>) -> BoxFuture<'a, ()> {
        self.response.lock().expect("response records").push((
            context.operation_id,
            context.recovery_index,
            context.status,
        ));
        let selector = self
            .observed_selector
            .lock()
            .expect("selector lock")
            .as_ref()
            .cloned();
        let ready = selector.as_ref().is_some_and(|selector| selector.is_set());
        self.ready_during_response
            .lock()
            .expect("response readiness")
            .push(ready);
        if ready {
            self.observed_ready_during_response_hook
                .store(true, Ordering::SeqCst);
        }
        if context.recovery_index == 0
            && self.replace_on_first_response.swap(false, Ordering::SeqCst)
        {
            selector
                .expect("replacement selector")
                .set(AccessToken::new(NEWER))
                .expect("response-hook replacement");
        }
        Box::pin(async {})
    }

    fn on_complete<'a>(&'a self, context: CompletionEvent<'a>) -> BoxFuture<'a, ()> {
        self.completion.lock().expect("completion records").push((
            context.operation_id,
            context.recovered_authentication,
            context.final_status,
        ));
        Box::pin(async {})
    }
}

struct RotatingProvider {
    acquires: Arc<AtomicUsize>,
    refreshes: Arc<AtomicUsize>,
    exact_current_seen: Arc<AtomicBool>,
}

impl CredentialProvider for RotatingProvider {
    type Material = AccessToken;

    fn acquire<'a>(
        &'a self,
        context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        self.acquires.fetch_add(1, Ordering::SeqCst);
        assert_eq!(context.reason, CredentialReason::Missing);
        Box::pin(async { Ok(AccessToken::new(INITIAL)) })
    }

    fn refresh<'a>(
        &'a self,
        current: &'a Self::Material,
        context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        self.refreshes.fetch_add(1, Ordering::SeqCst);
        self.exact_current_seen.store(
            crate::auth::SecretCredential::expose_secret(current) == INITIAL,
            Ordering::SeqCst,
        );
        assert_eq!(context.reason, CredentialReason::Rejected);
        Box::pin(async { Ok(AccessToken::new(REFRESHED)) })
    }

    fn recovery_mode(&self) -> RecoveryMode {
        RecoveryMode::Refresh
    }
}

struct CountingProvider {
    calls: Arc<AtomicUsize>,
}

struct IsolatedHttpProvider;

impl CredentialProvider for IsolatedHttpProvider {
    type Material = AccessToken;

    fn acquire<'a>(
        &'a self,
        context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        Box::pin(async move {
            let response = context
                .http
                .send(
                    ProviderRequest::new(
                        Method::POST,
                        url::Url::parse("https://isolated-provider.example.test/token")
                            .expect("provider URL"),
                    )
                    .header(
                        http::HeaderName::from_static("x-provider-secret"),
                        HeaderValue::from_static("provider-header-5c2ed86e9c70"),
                    )
                    .body(ProviderBody::Bytes(Bytes::from_static(
                        b"provider-request-body-723ad0d87fc1",
                    ))),
                )
                .await?;
            if response.body() != &Bytes::from_static(b"provider-response-body-f7ae16b96f2d") {
                return Err(AuthError::new(AuthErrorKind::Acquisition));
            }
            Ok(AccessToken::new("application-token-e6cc09dfdf24"))
        })
    }
}

impl CredentialProvider for CountingProvider {
    type Material = AccessToken;

    fn acquire<'a>(
        &'a self,
        _context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(AccessToken::new(INITIAL)) })
    }
}

struct ReplayFactory {
    calls: Arc<AtomicUsize>,
}

impl BodyFactory for ReplayFactory {
    fn create(&self) -> Result<StreamBody, BodyError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(StreamBody::from_bytes(Bytes::from_static(b"factory-body")))
    }
}

struct FailingFactory;

impl BodyFactory for FailingFactory {
    fn create(&self) -> Result<StreamBody, BodyError> {
        Err(BodyError::new(crate::body::BodyErrorKind::Factory))
    }
}

struct FailOnSecondFactory {
    calls: Arc<AtomicUsize>,
}

impl BodyFactory for FailOnSecondFactory {
    fn create(&self) -> Result<StreamBody, BodyError> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        if call == 0 {
            Ok(StreamBody::from_bytes(Bytes::from_static(b"initial-body")))
        } else {
            Err(BodyError::new(crate::body::BodyErrorKind::Factory))
        }
    }
}

fn core(
    executor: &DeterministicExecutor,
    hooks: Arc<AuthHooks>,
) -> (ClientCore, Arc<Mutex<Vec<String>>>) {
    let output = Arc::new(Mutex::new(Vec::new()));
    let core = ClientCore::build_with_development(
        "https://protected.example.test/v1/",
        HttpConfigBuilder::new().http1_only(),
        RuntimeConfigBuilder::new()
            .hooks(hooks)
            .debug(DebugLevel::Headers),
        executor,
    )
    .expect("protected development core")
    .with_debug_output(Arc::clone(&output));
    (core, output)
}

fn request(
    core: ClientCore,
    selector: &Arc<SlotSelector<AccessToken>>,
    challenge: ChallengePolicy,
    body: RequestBody,
) -> Request<ProtectedEndpoint> {
    Request::new(
        core,
        ProtectedEndpoint {
            credential: CredentialRef::new(Arc::clone(selector)),
            challenge,
            body,
            query: false,
        },
    )
}

fn query_request(
    core: ClientCore,
    selector: &Arc<SlotSelector<AccessToken>>,
) -> Request<ProtectedEndpoint> {
    Request::new(
        core,
        ProtectedEndpoint {
            credential: CredentialRef::new(Arc::clone(selector)),
            challenge: ChallengePolicy::Never,
            body: RequestBody::bytes(Bytes::new()),
            query: true,
        },
    )
}

fn api_error<T>(result: Result<T, ApiError>) -> ApiError {
    match result {
        Ok(_) => panic!("expected API failure"),
        Err(error) => error,
    }
}

#[tokio::test]
async fn production_managed_reqwest_inserts_auth_over_real_tls_loopback() {
    const NATIVE_ONLY: &str = "real-loopback-auth-91f4867e0bc2";
    let server = TlsServer::start().expect("TLS loopback server");
    let hooks = Arc::new(AuthHooks::default());
    let debug = Arc::new(Mutex::new(Vec::new()));
    let core = ClientCore::build(
        &server.url("localhost", "/v1/"),
        HttpConfigBuilder::new()
            .http1_only()
            .add_root_certificate_pem(server.certificate_pem()),
        RuntimeConfigBuilder::new()
            .hooks(hooks.clone())
            .debug(DebugLevel::Headers),
    )
    .expect("managed TLS core")
    .with_debug_output(Arc::clone(&debug));
    let selector = SlotSelector::static_value(AccessToken::new(NATIVE_ONLY), Duration::ZERO);

    let output = request(
        core,
        &selector,
        ChallengePolicy::Never,
        RequestBody::bytes(Bytes::new()),
    )
    .await
    .expect("real managed authenticated request");
    assert_eq!(output, Bytes::from_static(b"ok"));
    server
        .wait_for_requests(1, Duration::from_secs(2))
        .expect("managed request reached TLS server");
    let requests = server.requests();
    assert_eq!(requests[0].target(), "/v1/protected");
    assert_eq!(
        requests[0].header("authorization"),
        Some("Bearer real-loopback-auth-91f4867e0bc2")
    );

    let diagnostics = format!(
        "{}\n{:?}\n{:?}",
        debug.lock().expect("debug output").join("\n"),
        hooks.before.lock().expect("before records").len(),
        hooks.response.lock().expect("response records").len(),
    );
    assert!(!diagnostics.contains(NATIVE_ONLY));
}

#[tokio::test]
async fn refresh_recovery_reuses_identity_reenters_full_attempt_and_keeps_secrets_native() {
    const CHALLENGE_BODY: &[u8] = &[b'x'; 70_000];
    let executor = DeterministicExecutor::new();
    let challenge_body_polls = Arc::new(AtomicUsize::new(0));
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .method(Method::POST)
                .native_url("https://protected.example.test/v1/protected")
                .header(
                    AUTHORIZATION,
                    HeaderValue::from_static("Bearer initial-auth-token-1148c7"),
                )
                .body(Bytes::from_static(b"buffered")),
            ScriptedResponse::status(StatusCode::UNAUTHORIZED, Bytes::from_static(CHALLENGE_BODY))
                .body_poll_counter(Arc::clone(&challenge_body_polls)),
        )
        .expect("challenge response");
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .method(Method::POST)
                .native_url("https://protected.example.test/v1/protected")
                .header(
                    AUTHORIZATION,
                    HeaderValue::from_static("Bearer refreshed-auth-token-709fe2"),
                )
                .body(Bytes::from_static(b"buffered")),
            ScriptedResponse::success(Bytes::from_static(b"ok")),
        )
        .expect("recovery response");

    let acquires = Arc::new(AtomicUsize::new(0));
    let refreshes = Arc::new(AtomicUsize::new(0));
    let exact = Arc::new(AtomicBool::new(false));
    let selector = SlotSelector::provider(
        RotatingProvider {
            acquires: Arc::clone(&acquires),
            refreshes: Arc::clone(&refreshes),
            exact_current_seen: Arc::clone(&exact),
        },
        Duration::ZERO,
    );
    let hooks = Arc::new(AuthHooks::default());
    hooks.observe(Arc::clone(&selector));
    let (core, debug) = core(&executor, Arc::clone(&hooks));

    let response = request(
        core.clone(),
        &selector,
        ChallengePolicy::Unauthorized,
        RequestBody::bytes(Bytes::from_static(b"buffered")),
    )
    .response()
    .await
    .expect("one bounded recovery");
    assert_eq!(response.value(), &Bytes::from_static(b"ok"));
    assert!(response.metadata().recovered_authentication());
    assert_eq!(acquires.load(Ordering::SeqCst), 1);
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
    assert!(exact.load(Ordering::SeqCst));
    assert!(
        hooks
            .observed_ready_during_response_hook
            .load(Ordering::SeqCst)
    );
    assert_eq!(core.operation_allocations(), 1);
    assert_eq!(executor.application().request_count().expect("count"), 2);
    assert_eq!(executor.provider().request_count().expect("count"), 0);
    assert_eq!(challenge_body_polls.load(Ordering::SeqCst), 0);

    let before = hooks.before.lock().expect("before records").clone();
    assert_eq!(before.len(), 2);
    assert_eq!(before[0].operation_id, before[1].operation_id);
    assert_eq!([before[0].recovery_index, before[1].recovery_index], [0, 1]);
    for record in before {
        assert_eq!(
            record.safe_url,
            "https://protected.example.test/v1/protected"
        );
        assert!(!record.safe_url.contains(INITIAL));
        assert!(!record.safe_url.contains(REFRESHED));
    }
    let responses = hooks.response.lock().expect("response records");
    assert_eq!(responses.len(), 2);
    assert_eq!(responses[0].0, responses[1].0);
    assert_eq!([responses[0].1, responses[1].1], [0, 1]);
    drop(responses);
    let completion = hooks.completion.lock().expect("completion records");
    assert_eq!(completion.len(), 1);
    assert_eq!(completion[0].0, before_operation_id(&hooks));
    assert!(completion[0].1);
    assert_eq!(completion[0].2, Some(StatusCode::OK));

    let rendered = debug.lock().expect("debug output").join("\n");
    assert!(!rendered.contains(INITIAL));
    assert!(!rendered.contains(REFRESHED));
    let captures = executor.application().captures().expect("captures");
    assert!(captures.iter().all(|capture| {
        capture.safe_url() == Some("https://protected.example.test/v1/protected")
            && capture.headers().iter().any(|header| {
                header.name() == AUTHORIZATION && header.value() == &CapturedHeaderValue::Redacted
            })
            && !format!("{capture:?}").contains(INITIAL)
            && !format!("{capture:?}").contains(REFRESHED)
    }));
}

#[tokio::test]
async fn recovered_stream_reports_recovery_in_owned_metadata() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::UNAUTHORIZED,
            Bytes::from_static(b"unread"),
        ))
        .expect("challenge response");
    let mut headers = http::HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"streamed")).headers(headers))
        .expect("stream response");
    let selector = SlotSelector::provider(
        RotatingProvider {
            acquires: Arc::new(AtomicUsize::new(0)),
            refreshes: Arc::new(AtomicUsize::new(0)),
            exact_current_seen: Arc::new(AtomicBool::new(false)),
        },
        Duration::ZERO,
    );
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));
    let response = Request::new(
        core,
        ProtectedStreamEndpoint {
            credential: CredentialRef::new(selector),
        },
    )
    .await
    .expect("recovered stream should open");
    assert!(response.metadata().recovered_authentication());
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(executor.application().request_count().expect("count"), 2);
}

#[tokio::test]
async fn basic_and_custom_header_material_exist_only_at_the_native_boundary() {
    const API_KEY: &str = "header-auth-868e46f4d604";
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .native_url("https://protected.example.test/v1/basic")
                .header(
                    AUTHORIZATION,
                    HeaderValue::from_static("Basic dXNlcjpwYXNzd29yZA=="),
                ),
            ScriptedResponse::success(Bytes::from_static(b"basic")),
        )
        .expect("Basic response");
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .native_url("https://protected.example.test/v1/header")
                .header(
                    http::HeaderName::from_static("x-api-key"),
                    HeaderValue::from_static(API_KEY),
                ),
            ScriptedResponse::success(Bytes::from_static(b"header")),
        )
        .expect("header response");
    let hooks = Arc::new(AuthHooks::default());
    let (core, debug) = core(&executor, hooks);
    let basic =
        SlotSelector::static_value(BasicCredential::new("user", "password"), Duration::ZERO);
    let key = SlotSelector::static_value(ApiKey::new(API_KEY), Duration::ZERO);

    let basic_result = Request::new(
        core.clone(),
        BasicEndpoint {
            credential: CredentialRef::new(basic),
        },
    )
    .await
    .expect("Basic request");
    let header_result = Request::new(
        core,
        HeaderEndpoint {
            credential: CredentialRef::new(key),
        },
    )
    .await
    .expect("header request");
    assert_eq!(basic_result, Bytes::from_static(b"basic"));
    assert_eq!(header_result, Bytes::from_static(b"header"));

    let before = debug.lock().expect("debug output").join("\n");
    assert!(!before.contains("dXNlcjpwYXNzd29yZA=="));
    assert!(!before.contains(API_KEY));
    let captures = executor.application().captures().expect("captures");
    assert_eq!(captures.len(), 2);
    assert!(captures.iter().all(|capture| {
        capture
            .safe_url()
            .is_some_and(|url| !url.contains(API_KEY) && !url.contains("dXNlcjpwYXNzd29yZA=="))
            && capture
                .headers()
                .iter()
                .all(|header| header.value() == &CapturedHeaderValue::Redacted)
    }));
}

#[tokio::test]
async fn provider_http_data_stays_outside_application_hooks_debug_and_channel() {
    const PROVIDER_HEADER: &str = "provider-header-5c2ed86e9c70";
    const PROVIDER_REQUEST: &str = "provider-request-body-723ad0d87fc1";
    const PROVIDER_RESPONSE: &str = "provider-response-body-f7ae16b96f2d";
    const APPLICATION_TOKEN: &str = "application-token-e6cc09dfdf24";
    let executor = DeterministicExecutor::new();
    executor
        .provider()
        .queue_expected_response(
            ExpectedRequest::new()
                .method(Method::POST)
                .native_url("https://isolated-provider.example.test/token")
                .header(
                    http::HeaderName::from_static("x-provider-secret"),
                    HeaderValue::from_static(PROVIDER_HEADER),
                )
                .body(Bytes::from_static(PROVIDER_REQUEST.as_bytes())),
            ScriptedResponse::success(Bytes::from_static(PROVIDER_RESPONSE.as_bytes())),
        )
        .expect("provider response");
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new().header(
                AUTHORIZATION,
                HeaderValue::from_static("Bearer application-token-e6cc09dfdf24"),
            ),
            ScriptedResponse::success(Bytes::from_static(b"application")),
        )
        .expect("application response");
    let selector = SlotSelector::provider(IsolatedHttpProvider, Duration::ZERO);
    let hooks = Arc::new(AuthHooks::default());
    let (core, debug) = core(&executor, Arc::clone(&hooks));
    let result = request(
        core,
        &selector,
        ChallengePolicy::Never,
        RequestBody::bytes(Bytes::new()),
    )
    .await
    .expect("provider-backed application request");
    assert_eq!(result, Bytes::from_static(b"application"));
    assert_eq!(hooks.before.lock().expect("before hooks").len(), 1);
    assert_eq!(hooks.response.lock().expect("response hooks").len(), 1);
    assert_eq!(hooks.completion.lock().expect("completion hooks").len(), 1);
    assert_eq!(executor.provider().request_count().expect("count"), 1);
    assert_eq!(executor.application().request_count().expect("count"), 1);

    let provider_capture = executor.provider().captures().expect("provider captures");
    assert_eq!(provider_capture.len(), 1);
    assert!(provider_capture[0].safe_url().is_none());
    assert!(provider_capture[0].headers().iter().any(|header| {
        header.name() == "x-provider-secret" && header.value() == &CapturedHeaderValue::Redacted
    }));
    let application_capture = executor
        .application()
        .captures()
        .expect("application captures");
    let diagnostics = format!(
        "{}\n{provider_capture:?}\n{application_capture:?}",
        debug.lock().expect("debug output").join("\n")
    );
    for sentinel in [
        PROVIDER_HEADER,
        PROVIDER_REQUEST,
        PROVIDER_RESPONSE,
        APPLICATION_TOKEN,
    ] {
        assert!(!diagnostics.contains(sentinel));
    }
}

#[tokio::test]
async fn response_hook_replacement_is_observed_before_challenge_and_newer_material_is_reused() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new().header(
                AUTHORIZATION,
                HeaderValue::from_static("Bearer initial-auth-token-1148c7"),
            ),
            ScriptedResponse::status(StatusCode::UNAUTHORIZED, Bytes::from_static(b"unread")),
        )
        .expect("challenge response");
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new().header(
                AUTHORIZATION,
                HeaderValue::from_static("Bearer newer-auth-token-265f0a"),
            ),
            ScriptedResponse::success(Bytes::from_static(b"newer")),
        )
        .expect("newer response");
    let acquires = Arc::new(AtomicUsize::new(0));
    let refreshes = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        RotatingProvider {
            acquires: Arc::clone(&acquires),
            refreshes: Arc::clone(&refreshes),
            exact_current_seen: Arc::new(AtomicBool::new(false)),
        },
        Duration::ZERO,
    );
    let hooks = Arc::new(AuthHooks::default());
    hooks.observe(Arc::clone(&selector));
    hooks.replace_on_first_response();
    let (core, _) = core(&executor, Arc::clone(&hooks));

    let result = request(
        core,
        &selector,
        ChallengePolicy::Unauthorized,
        RequestBody::bytes(Bytes::new()),
    )
    .await
    .expect("newer current credential recovery");
    assert_eq!(result, Bytes::from_static(b"newer"));
    assert_eq!(acquires.load(Ordering::SeqCst), 1);
    assert_eq!(refreshes.load(Ordering::SeqCst), 0);
    assert_eq!(
        hooks
            .ready_during_response
            .lock()
            .expect("response readiness")
            .as_slice(),
        &[true, true]
    );
    assert_eq!(executor.application().request_count().expect("count"), 2);
}

fn before_operation_id(hooks: &AuthHooks) -> OperationId {
    hooks.before.lock().expect("before records")[0].operation_id
}

#[tokio::test]
async fn reconstructible_factory_runs_once_per_concord_attempt_and_no_third_attempt_occurs() {
    let executor = DeterministicExecutor::new();
    for token in [INITIAL, REFRESHED] {
        executor
            .application()
            .queue_expected_response(
                ExpectedRequest::new()
                    .header(
                        AUTHORIZATION,
                        HeaderValue::from_str(&format!("Bearer {token}")).expect("header"),
                    )
                    .body(Bytes::from_static(b"factory-body")),
                ScriptedResponse::status(StatusCode::UNAUTHORIZED, Bytes::from_static(b"terminal")),
            )
            .expect("challenge response");
    }
    let acquires = Arc::new(AtomicUsize::new(0));
    let refreshes = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        RotatingProvider {
            acquires,
            refreshes: Arc::clone(&refreshes),
            exact_current_seen: Arc::new(AtomicBool::new(false)),
        },
        Duration::ZERO,
    );
    let hooks = Arc::new(AuthHooks::default());
    let (core, _) = core(&executor, Arc::clone(&hooks));
    let factory_calls = Arc::new(AtomicUsize::new(0));
    let error = api_error(
        request(
            core,
            &selector,
            ChallengePolicy::Unauthorized,
            RequestBody::factory(ReplayFactory {
                calls: Arc::clone(&factory_calls),
            }),
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::AuthenticationRejected);
    assert_eq!(factory_calls.load(Ordering::SeqCst), 2);
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
    assert_eq!(executor.application().request_count().expect("count"), 2);
    assert_eq!(
        hooks
            .before
            .lock()
            .expect("before records")
            .iter()
            .map(|record| record.known_body_length)
            .collect::<Vec<_>>(),
        [None, None]
    );
}

#[tokio::test]
async fn recovery_hook_veto_is_primary_and_invalidates_exact_challenged_material() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::UNAUTHORIZED,
            Bytes::from_static(b"unread"),
        ))
        .expect("challenge response");
    let refreshes = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        RotatingProvider {
            acquires: Arc::new(AtomicUsize::new(0)),
            refreshes: Arc::clone(&refreshes),
            exact_current_seen: Arc::new(AtomicBool::new(false)),
        },
        Duration::ZERO,
    );
    let hooks = Arc::new(AuthHooks::default());
    hooks.veto_recovery();
    let (core, _) = core(&executor, hooks);
    let error = api_error(
        request(
            core.clone(),
            &selector,
            ChallengePolicy::Unauthorized,
            RequestBody::bytes(Bytes::new()),
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::Hook);
    assert_eq!(refreshes.load(Ordering::SeqCst), 0);
    let current = selector
        .preflight(core.provider_http(None))
        .await
        .expect("later operation refreshes rejected material");
    assert_eq!(current.value().expose_secret(), REFRESHED);
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
    assert_eq!(executor.application().request_count().expect("count"), 1);
}

#[tokio::test]
async fn recovery_reconstruction_failure_is_primary_and_invalidates_challenged_material() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new().body(Bytes::from_static(b"initial-body")),
            ScriptedResponse::status(StatusCode::UNAUTHORIZED, Bytes::from_static(b"unread")),
        )
        .expect("challenge response");
    let refreshes = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        RotatingProvider {
            acquires: Arc::new(AtomicUsize::new(0)),
            refreshes: Arc::clone(&refreshes),
            exact_current_seen: Arc::new(AtomicBool::new(false)),
        },
        Duration::ZERO,
    );
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));
    let calls = Arc::new(AtomicUsize::new(0));
    let error = api_error(
        request(
            core.clone(),
            &selector,
            ChallengePolicy::Unauthorized,
            RequestBody::factory(FailOnSecondFactory {
                calls: Arc::clone(&calls),
            }),
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(refreshes.load(Ordering::SeqCst), 0);
    let current = selector
        .preflight(core.provider_http(None))
        .await
        .expect("later operation refreshes rejected material");
    assert_eq!(current.value().expose_secret(), REFRESHED);
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
    assert_eq!(executor.application().request_count().expect("count"), 1);
}

#[tokio::test]
async fn one_shot_body_prevents_recovery_and_provider_refresh() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new().body(Bytes::from_static(b"one-shot")),
            ScriptedResponse::status(StatusCode::UNAUTHORIZED, Bytes::from_static(b"terminal")),
        )
        .expect("challenge response");
    let refreshes = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        RotatingProvider {
            acquires: Arc::new(AtomicUsize::new(0)),
            refreshes: Arc::clone(&refreshes),
            exact_current_seen: Arc::new(AtomicBool::new(false)),
        },
        Duration::ZERO,
    );
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));
    let stream = StreamBody::from_bytes(Bytes::from_static(b"one-shot"));
    let error = api_error(
        request(
            core,
            &selector,
            ChallengePolicy::Unauthorized,
            RequestBody::stream(stream),
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::AuthenticationRejected);
    assert_eq!(refreshes.load(Ordering::SeqCst), 0);
    assert_eq!(executor.application().request_count().expect("count"), 1);
}

#[cfg(feature = "multipart")]
#[tokio::test]
async fn one_shot_multipart_prevents_recovery_and_provider_refresh() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::UNAUTHORIZED,
            Bytes::from_static(b"terminal"),
        ))
        .expect("challenge response");
    let refreshes = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        RotatingProvider {
            acquires: Arc::new(AtomicUsize::new(0)),
            refreshes: Arc::clone(&refreshes),
            exact_current_seen: Arc::new(AtomicBool::new(false)),
        },
        Duration::ZERO,
    );
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));
    let multipart = MultipartBody::new()
        .part(
            "stream",
            MultipartPart::stream(StreamBody::from_bytes(Bytes::from_static(b"one-shot"))),
        )
        .expect("one-shot multipart");
    let error = api_error(
        request(
            core,
            &selector,
            ChallengePolicy::Unauthorized,
            RequestBody::multipart(multipart),
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::AuthenticationRejected);
    assert_eq!(refreshes.load(Ordering::SeqCst), 0);
    assert_eq!(executor.application().request_count().expect("count"), 1);
}

#[tokio::test]
async fn static_and_endpoint_challenges_mutate_only_exact_attributed_source_without_resend() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::UNAUTHORIZED,
            Bytes::from_static(b"static terminal"),
        ))
        .expect("static challenge");
    let static_slot = SlotSelector::static_value(AccessToken::new(INITIAL), Duration::ZERO);
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));
    let first = api_error(
        request(
            core.clone(),
            &static_slot,
            ChallengePolicy::Unauthorized,
            RequestBody::bytes(Bytes::new()),
        )
        .await,
    );
    assert_eq!(first.category(), ErrorCategory::AuthenticationRejected);
    assert_eq!(executor.application().request_count().expect("count"), 1);
    let second = api_error(
        request(
            core.clone(),
            &static_slot,
            ChallengePolicy::Unauthorized,
            RequestBody::bytes(Bytes::new()),
        )
        .await,
    );
    assert_eq!(second.auth_kind(), Some(AuthErrorKind::Rejected));
    assert_eq!(executor.application().request_count().expect("count"), 1);
    static_slot
        .replace_static(AccessToken::new(REFRESHED))
        .expect("complete static replacement");
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"replaced")))
        .expect("replacement response");
    assert_eq!(
        request(
            core.clone(),
            &static_slot,
            ChallengePolicy::Unauthorized,
            RequestBody::bytes(Bytes::new()),
        )
        .await
        .expect("replacement clears rejection"),
        Bytes::from_static(b"replaced")
    );

    let endpoint_slot = SlotSelector::endpoint(Duration::ZERO);
    endpoint_slot
        .set(AccessToken::new(INITIAL))
        .expect("endpoint material");
    executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::UNAUTHORIZED,
            Bytes::from_static(b"endpoint terminal"),
        ))
        .expect("endpoint challenge");
    api_error(
        request(
            core,
            &endpoint_slot,
            ChallengePolicy::Unauthorized,
            RequestBody::bytes(Bytes::new()),
        )
        .await,
    );
    assert!(!endpoint_slot.is_set());
    assert_eq!(executor.application().request_count().expect("count"), 3);
}

#[tokio::test]
async fn forbidden_policy_and_single_attribution_leave_unrelated_credentials_unchanged() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .header(
                    AUTHORIZATION,
                    HeaderValue::from_static("Bearer initial-auth-token-1148c7"),
                )
                .header(
                    http::HeaderName::from_static("x-secondary-token"),
                    HeaderValue::from_static(NEWER),
                ),
            ScriptedResponse::status(StatusCode::FORBIDDEN, Bytes::from_static(b"terminal")),
        )
        .expect("attributed forbidden response");
    let first = SlotSelector::static_value(AccessToken::new(INITIAL), Duration::ZERO);
    let second = SlotSelector::static_value(AccessToken::new(NEWER), Duration::ZERO);
    let hooks = Arc::new(AuthHooks::default());
    hooks.observe(Arc::clone(&first));
    let (core, _) = core(&executor, Arc::clone(&hooks));
    let error = api_error(
        Request::new(
            core.clone(),
            MultiAuthEndpoint {
                first: CredentialRef::new(Arc::clone(&first)),
                second: CredentialRef::new(Arc::clone(&second)),
                first_challenge: ChallengePolicy::UnauthorizedOrForbidden,
                body_calls: Arc::new(AtomicUsize::new(0)),
            },
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::AuthenticationRejected);
    assert!(!first.is_set());
    assert!(second.is_set());
    assert_eq!(
        hooks
            .ready_during_response
            .lock()
            .expect("response readiness")
            .as_slice(),
        &[true]
    );

    executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::FORBIDDEN,
            Bytes::from_static(b"ordinary forbidden"),
        ))
        .expect("ordinary forbidden response");
    let first = SlotSelector::static_value(AccessToken::new(INITIAL), Duration::ZERO);
    let second = SlotSelector::static_value(AccessToken::new(NEWER), Duration::ZERO);
    let error = api_error(
        Request::new(
            core,
            MultiAuthEndpoint {
                first: CredentialRef::new(Arc::clone(&first)),
                second: CredentialRef::new(Arc::clone(&second)),
                first_challenge: ChallengePolicy::Unauthorized,
                body_calls: Arc::new(AtomicUsize::new(0)),
            },
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::HttpStatus);
    assert!(first.is_set());
    assert!(second.is_set());
    assert_eq!(executor.application().request_count().expect("count"), 2);
}

#[tokio::test]
async fn default_never_preserves_static_material_and_query_auth_exists_only_in_native_url() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::UNAUTHORIZED,
            Bytes::from_static(b"ordinary status"),
        ))
        .expect("ordinary status");
    let selector = SlotSelector::static_value(AccessToken::new(INITIAL), Duration::ZERO);
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));
    let error = api_error(
        request(
            core.clone(),
            &selector,
            ChallengePolicy::Never,
            RequestBody::bytes(Bytes::new()),
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::HttpStatus);
    assert!(selector.is_set());

    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .native_url(format!(
                    "https://protected.example.test/v1/protected?access={INITIAL}"
                ))
                .safe_url("https://protected.example.test/v1/protected"),
            ScriptedResponse::success(Bytes::from_static(b"query")),
        )
        .expect("query response");
    assert_eq!(
        query_request(core, &selector)
            .await
            .expect("native query auth"),
        Bytes::from_static(b"query")
    );
    let captures = executor.application().captures().expect("captures");
    let query_capture = captures.last().expect("query capture");
    assert_eq!(
        query_capture.safe_url(),
        Some("https://protected.example.test/v1/protected")
    );
    assert!(!format!("{query_capture:?}").contains(INITIAL));
}

#[tokio::test]
async fn body_failure_prevents_provider_debug_and_native_send() {
    let executor = DeterministicExecutor::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        CountingProvider {
            calls: Arc::clone(&calls),
        },
        Duration::ZERO,
    );
    let hooks = Arc::new(AuthHooks::default());
    let (core, debug) = core(&executor, Arc::clone(&hooks));
    let error = api_error(
        request(
            core,
            &selector,
            ChallengePolicy::Never,
            RequestBody::factory(FailingFactory),
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    let debug = debug.lock().expect("debug output");
    assert_eq!(debug.len(), 1);
    assert!(debug[0].starts_with("completion "));
    assert_eq!(hooks.before.lock().expect("before records").len(), 1);
    assert!(hooks.response.lock().expect("response records").is_empty());
}

#[tokio::test]
async fn complete_plan_preflight_failure_preserves_earlier_acquisition_and_stops_send() {
    let executor = DeterministicExecutor::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let first = SlotSelector::provider(
        CountingProvider {
            calls: Arc::clone(&calls),
        },
        Duration::ZERO,
    );
    let second = SlotSelector::endpoint(Duration::ZERO);
    let hooks = Arc::new(AuthHooks::default());
    let (core, debug) = core(&executor, hooks);
    let body_calls = Arc::new(AtomicUsize::new(0));
    let endpoint = MultiAuthEndpoint {
        first: CredentialRef::new(Arc::clone(&first)),
        second: CredentialRef::new(second),
        first_challenge: ChallengePolicy::Never,
        body_calls: Arc::clone(&body_calls),
    };
    let error = api_error(Request::new(core, endpoint).await);
    assert_eq!(error.category(), ErrorCategory::AuthenticationUnavailable);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(first.is_set());
    assert_eq!(body_calls.load(Ordering::SeqCst), 1);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    assert!(
        debug
            .lock()
            .expect("debug output")
            .iter()
            .all(|line| !line.starts_with("request-ready "))
    );
}

#[tokio::test]
async fn repeated_placements_reuse_one_attempt_local_credential_result() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .header(
                    AUTHORIZATION,
                    HeaderValue::from_static("Bearer initial-auth-token-1148c7"),
                )
                .header(
                    http::HeaderName::from_static("x-secondary-token"),
                    HeaderValue::from_static(INITIAL),
                ),
            ScriptedResponse::success(Bytes::from_static(b"shared")),
        )
        .expect("shared credential response");
    let calls = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        CountingProvider {
            calls: Arc::clone(&calls),
        },
        Duration::ZERO,
    );
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));
    let body_calls = Arc::new(AtomicUsize::new(0));
    let output = Request::new(
        core,
        MultiAuthEndpoint {
            first: CredentialRef::new(Arc::clone(&selector)),
            second: CredentialRef::new(Arc::clone(&selector)),
            first_challenge: ChallengePolicy::Never,
            body_calls: Arc::clone(&body_calls),
        },
    )
    .await
    .expect("shared credential request");
    assert_eq!(output, Bytes::from_static(b"shared"));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(body_calls.load(Ordering::SeqCst), 1);
    assert_eq!(executor.application().request_count().expect("count"), 1);
}

#[tokio::test]
async fn material_public_collision_fails_preparation_before_body_provider_or_send() {
    let executor = DeterministicExecutor::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        CountingProvider {
            calls: Arc::clone(&calls),
        },
        Duration::ZERO,
    );
    let hooks = Arc::new(AuthHooks::default());
    let (core, debug) = core(&executor, Arc::clone(&hooks));
    let body_calls = Arc::new(AtomicUsize::new(0));
    let error = api_error(
        Request::new(
            core,
            DynamicCollisionEndpoint {
                credential: CredentialRef::new(selector),
                body_calls: Arc::clone(&body_calls),
            },
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(
        error.configuration_kind(),
        Some(crate::error::ConfigurationErrorKind::InvalidAuthentication)
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(body_calls.load(Ordering::SeqCst), 0);
    assert!(hooks.before.lock().expect("before records").is_empty());
    assert_eq!(executor.application().request_count().expect("count"), 0);
    assert_eq!(debug.lock().expect("debug output").len(), 1);
}

#[tokio::test]
async fn finalization_expiration_prevents_native_construction_debug_and_send() {
    let executor = DeterministicExecutor::new();
    let checks = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::static_value(
        FinalizationExpiryToken {
            value: SecretString::new(INITIAL),
            freshness_checks: Arc::clone(&checks),
        },
        Duration::ZERO,
    );
    let (core, debug) = core(&executor, Arc::new(AuthHooks::default()));
    let error = api_error(
        Request::new(
            core,
            FinalizationExpiryEndpoint {
                credential: CredentialRef::new(selector),
            },
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::AuthenticationUnavailable);
    assert!(checks.load(Ordering::SeqCst) >= 2);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    assert!(
        debug
            .lock()
            .expect("debug output")
            .iter()
            .all(|line| !line.starts_with("request-ready "))
    );
}

#[tokio::test]
async fn invalid_native_auth_value_fails_after_safe_debug_and_before_execution() {
    const INVALID: &str = "invalid-auth-sentinel-91af\r\nnot-a-header";
    let executor = DeterministicExecutor::new();
    let selector = SlotSelector::static_value(AccessToken::new(INVALID), Duration::ZERO);
    let (core, debug) = core(&executor, Arc::new(AuthHooks::default()));
    let error = api_error(
        request(
            core,
            &selector,
            ChallengePolicy::Never,
            RequestBody::bytes(Bytes::new()),
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(
        error.configuration_kind(),
        Some(crate::error::ConfigurationErrorKind::InvalidAuthentication)
    );
    assert_eq!(executor.application().request_count().expect("count"), 0);
    let rendered = debug.lock().expect("debug output").join("\n");
    assert!(rendered.contains("request"));
    assert!(!rendered.contains("invalid-auth-sentinel-91af"));
}

#[tokio::test]
async fn authenticated_cleartext_fails_in_public_preparation_before_hook_body_or_provider() {
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(AuthHooks::default());
    let core = ClientCore::build_with_development(
        "http://cleartext.example.test/v1/",
        HttpConfigBuilder::new().http1_only(),
        RuntimeConfigBuilder::new().hooks(hooks.clone()),
        &executor,
    )
    .expect("cleartext core remains valid for public endpoints");
    let calls = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        CountingProvider {
            calls: Arc::clone(&calls),
        },
        Duration::ZERO,
    );
    let body_calls = Arc::new(AtomicUsize::new(0));
    let error = api_error(
        request(
            core,
            &selector,
            ChallengePolicy::Never,
            RequestBody::factory(ReplayFactory {
                calls: Arc::clone(&body_calls),
            }),
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(
        error.configuration_kind(),
        Some(crate::error::ConfigurationErrorKind::InvalidAuthentication)
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(body_calls.load(Ordering::SeqCst), 0);
    assert!(hooks.before.lock().expect("before records").is_empty());
    assert_eq!(executor.application().request_count().expect("count"), 0);
}

#[tokio::test]
async fn request_debug_is_emitted_before_native_boundary_and_never_contains_auth() {
    let executor = DeterministicExecutor::new();
    let gate = Gate::closed();
    executor
        .application()
        .pause_before_execution(gate.clone())
        .expect("pause application boundary");
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new().header(
                AUTHORIZATION,
                HeaderValue::from_static("Bearer initial-auth-token-1148c7"),
            ),
            ScriptedResponse::success(Bytes::from_static(b"ok")),
        )
        .expect("response");
    let (core, debug) = core(&executor, Arc::new(AuthHooks::default()));
    let exposed_after_debug = Arc::new(AtomicBool::new(false));
    let selector = SlotSelector::static_value(
        ExposureToken {
            value: SecretString::new(INITIAL),
            debug: Arc::clone(&debug),
            exposed_after_debug: Arc::clone(&exposed_after_debug),
        },
        Duration::ZERO,
    );
    let pending = Request::new(
        core,
        ExposureEndpoint {
            credential: CredentialRef::new(selector),
        },
    );
    let task = tokio::spawn(async move { pending.await });
    gate.wait_until_reached().await;
    assert!(exposed_after_debug.load(Ordering::SeqCst));
    let rendered = debug.lock().expect("debug output").join("\n");
    assert!(rendered.contains("request"));
    assert!(!rendered.contains(INITIAL));
    gate.open();
    assert_eq!(
        task.await.expect("request task").expect("request result"),
        Bytes::from_static(b"ok")
    );
}
