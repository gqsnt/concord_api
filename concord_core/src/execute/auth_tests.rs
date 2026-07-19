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
use crate::codec::{CodecError, DecodeBody, DecodeContext, OctetStream};
use crate::error::{ApiError, AuthErrorKind, ErrorCategory};
use crate::hooks::{
    BeforeSend, BoxFuture, CompletionEvent, CompletionOutcome, DebugLevel,
    PaginationCompletionEvent, ResponseHeadEvent, RuntimeHooks,
};
use crate::http::HttpConfigBuilder;
use crate::http::provider::{ProviderBody, ProviderRequest};
use crate::operation::OperationId;
use crate::pagination::{
    PageLimit, PaginationController, PaginationError, PaginationRequest, paginate_with,
};
#[cfg(feature = "rate-limit-governor")]
use crate::rate_limit::GovernorRateLimiter;
use crate::rate_limit::{
    RateLimitError, RateLimitKeyPart, RateLimitPermit, RateLimitRequest, RateLimitResponse,
    RateLimiter,
};
use crate::request::{
    BodyMetadata, BodySpec, BufferedResponse, BytesResponse, EmptyBody, EndpointSpec, HeadBuilder,
    PreparedHead, Request, RequestMeta, StreamResponseHandler,
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
    rate_key: Option<String>,
}

struct ProtectedStreamEndpoint {
    credential: CredentialRef<AccessToken>,
}

#[derive(Clone)]
struct PaginatedProtectedEndpoint {
    credential: CredentialRef<AccessToken>,
}

struct AuthenticationPageCodec;

impl DecodeBody for AuthenticationPageCodec {
    type Output = Vec<u8>;

    const ACCEPT: Option<&'static str> = None;

    fn validate_content_type(_content_type: Option<&str>) -> Result<(), CodecError> {
        Ok(())
    }

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        Ok(body.to_vec())
    }
}

impl EndpointSpec for PaginatedProtectedEndpoint {
    type Body = EmptyBody;
    type Response = BufferedResponse<AuthenticationPageCodec>;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(ENDPOINT, Method::GET, Some(Duration::from_secs(30)))
    }

    fn prepare_head_and_body_metadata(
        &self,
        mut builder: HeadBuilder,
    ) -> Result<(PreparedHead, BodyMetadata), ApiError> {
        builder.push_static("paginated-protected")?;
        builder.push_auth(
            Auth::bearer(self.credential.clone()).challenge(ChallengePolicy::Unauthorized),
        )?;
        Ok((builder.finish(), EmptyBody::metadata()))
    }

    fn into_body(self) -> Self::Body {
        EmptyBody::new()
    }
}

#[derive(Clone)]
struct OneAuthenticationPage;

impl PaginationController<Vec<u8>> for OneAuthenticationPage {
    fn apply(&self, _request: &mut PaginationRequest<'_>) -> Result<(), PaginationError> {
        Ok(())
    }

    fn next(&self, _page: &Vec<u8>) -> Result<Option<Self>, PaginationError> {
        Ok(None)
    }
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
    expire_after_preflight: bool,
}

impl CredentialMaterial for FinalizationExpiryToken {
    fn expires_at(&self) -> Option<Instant> {
        let check = self.freshness_checks.fetch_add(1, Ordering::SeqCst);
        (self.expire_after_preflight && check > 0).then(|| Instant::now() - Duration::from_secs(1))
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
struct LimiterWaitExpiryToken {
    value: SecretString,
    expired: Arc<AtomicBool>,
}

impl CredentialMaterial for LimiterWaitExpiryToken {
    fn expires_at(&self) -> Option<Instant> {
        self.expired
            .load(Ordering::SeqCst)
            .then(|| Instant::now() - Duration::from_secs(1))
    }
}

impl SecretCredential for LimiterWaitExpiryToken {
    fn expose_secret(&self) -> &str {
        self.value.expose_secret()
    }
}

struct LimiterWaitExpiryProvider {
    initial_expired: Arc<AtomicBool>,
    acquires: Arc<AtomicUsize>,
    refreshes: Arc<AtomicUsize>,
}

impl CredentialProvider for LimiterWaitExpiryProvider {
    type Material = LimiterWaitExpiryToken;

    fn acquire<'a>(
        &'a self,
        _context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        self.acquires.fetch_add(1, Ordering::SeqCst);
        let expired = Arc::clone(&self.initial_expired);
        Box::pin(async move {
            Ok(LimiterWaitExpiryToken {
                value: SecretString::new(INITIAL),
                expired,
            })
        })
    }

    fn refresh<'a>(
        &'a self,
        _current: &'a Self::Material,
        context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        assert_eq!(context.reason, CredentialReason::Expired);
        self.refreshes.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {
            Ok(LimiterWaitExpiryToken {
                value: SecretString::new(REFRESHED),
                expired: Arc::new(AtomicBool::new(false)),
            })
        })
    }
}

struct LimiterWaitExpiryEndpoint {
    credential: CredentialRef<LimiterWaitExpiryToken>,
}

impl EndpointSpec for LimiterWaitExpiryEndpoint {
    type Body = RequestBody;
    type Response = BytesResponse;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(ENDPOINT, Method::POST, Some(Duration::from_secs(30)))
    }

    fn prepare_head_and_body_metadata(
        &self,
        mut builder: HeadBuilder,
    ) -> Result<(PreparedHead, BodyMetadata), ApiError> {
        builder.push_static("expiry-refresh")?;
        builder.push_auth(Auth::bearer(self.credential.clone()))?;
        let body = RequestBody::bytes(Bytes::new());
        Ok((builder.finish(), body.metadata(ENDPOINT, &Method::POST)?))
    }

    fn into_body(self) -> Self::Body {
        RequestBody::bytes(Bytes::new())
    }
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
        builder.push_rate_limit_rule(
            "AuthenticationApi::rate_limit::finalization",
            std::iter::empty::<RateLimitKeyPart>(),
            1,
            [(1, Duration::from_secs(10))],
        )?;
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
        if let Some(rate_key) = &self.rate_key {
            builder.push_rate_limit_rule(
                "AuthenticationApi::rate_limit::tenant",
                [RateLimitKeyPart::new("vars.tenant", rate_key.clone())],
                1,
                [(1, Duration::from_secs(1))],
            )?;
        }
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
    parent_operation_id: Option<OperationId>,
    page_index: u32,
    recovery_index: u8,
    safe_url: String,
    known_body_length: Option<u64>,
}

type AuthResponseRecord = (OperationId, u8, StatusCode, Option<OperationId>, u32);
type AuthCompletionRecord = (
    OperationId,
    bool,
    Option<StatusCode>,
    Option<OperationId>,
    u32,
    CompletionOutcome,
);
type AuthPaginationRecord = (
    OperationId,
    u32,
    usize,
    bool,
    Option<StatusCode>,
    CompletionOutcome,
);

#[derive(Default)]
struct AuthHooks {
    before: Mutex<Vec<HookRecord>>,
    response: Mutex<Vec<AuthResponseRecord>>,
    completion: Mutex<Vec<AuthCompletionRecord>>,
    pagination: Mutex<Vec<AuthPaginationRecord>>,
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
                parent_operation_id: context.parent_operation_id,
                page_index: context.page_index,
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
            context.parent_operation_id,
            context.page_index,
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
            context.parent_operation_id,
            context.page_index,
            context.outcome,
        ));
        Box::pin(async {})
    }

    fn on_pagination_complete<'a>(
        &'a self,
        context: PaginationCompletionEvent<'a>,
    ) -> BoxFuture<'a, ()> {
        self.pagination.lock().expect("pagination records").push((
            context.operation_id,
            context.pages_completed,
            context.items_collected,
            context.recovered_authentication,
            context.final_status,
            context.outcome,
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

struct FailingProvider;

impl CredentialProvider for FailingProvider {
    type Material = AccessToken;

    fn acquire<'a>(
        &'a self,
        _context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        Box::pin(async { Err(AuthError::new(AuthErrorKind::Acquisition)) })
    }
}

type ObservedSelector = (Arc<SlotSelector<AccessToken>>, Arc<Mutex<Vec<bool>>>);

struct AuthRateLimiter {
    acquisitions: Arc<AtomicUsize>,
    observations: Arc<AtomicUsize>,
    gate: Option<Gate>,
    failure: bool,
    observed_selector: Option<ObservedSelector>,
}

impl AuthRateLimiter {
    fn counting(acquisitions: &Arc<AtomicUsize>, observations: &Arc<AtomicUsize>) -> Self {
        Self {
            acquisitions: Arc::clone(acquisitions),
            observations: Arc::clone(observations),
            gate: None,
            failure: false,
            observed_selector: None,
        }
    }

    fn gated(acquisitions: &Arc<AtomicUsize>, gate: Gate) -> Self {
        Self {
            acquisitions: Arc::clone(acquisitions),
            observations: Arc::new(AtomicUsize::new(0)),
            gate: Some(gate),
            failure: false,
            observed_selector: None,
        }
    }

    fn failing(acquisitions: &Arc<AtomicUsize>) -> Self {
        Self {
            acquisitions: Arc::clone(acquisitions),
            observations: Arc::new(AtomicUsize::new(0)),
            gate: None,
            failure: true,
            observed_selector: None,
        }
    }
}

impl RateLimiter for AuthRateLimiter {
    fn acquire<'a>(
        &'a self,
        _request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        self.acquisitions.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            if let Some(gate) = &self.gate {
                gate.wait().await;
            }
            if self.failure {
                Err(RateLimitError::with_safe_message(
                    crate::error::RateLimitErrorKind::Acquire,
                    "safe application limiter rejection",
                ))
            } else {
                Ok(RateLimitPermit::granted())
            }
        })
    }

    fn observe<'a>(&'a self, _response: RateLimitResponse<'a>) -> BoxFuture<'a, ()> {
        self.observations.fetch_add(1, Ordering::SeqCst);
        if let Some((selector, records)) = &self.observed_selector {
            records
                .lock()
                .expect("limiter observation records")
                .push(selector.is_set());
        }
        Box::pin(async {})
    }
}

struct IsolatedHttpProvider;

struct LoopbackHttpProvider {
    url: url::Url,
    calls: Arc<AtomicUsize>,
}

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

impl CredentialProvider for LoopbackHttpProvider {
    type Material = AccessToken;

    fn acquire<'a>(
        &'a self,
        context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            let response = context
                .http
                .send(ProviderRequest::new(Method::POST, self.url.clone()).header(
                    http::HeaderName::from_static("x-provider-probe"),
                    HeaderValue::from_static("isolated-provider-probe"),
                ))
                .await?;
            if response.body() != &Bytes::from_static(b"ok") {
                return Err(AuthError::new(AuthErrorKind::Acquisition));
            }
            Ok(AccessToken::new("real-provider-application-token"))
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
    core_with_runtime(executor, hooks, RuntimeConfigBuilder::new())
}

fn core_with_runtime(
    executor: &DeterministicExecutor,
    hooks: Arc<AuthHooks>,
    runtime: RuntimeConfigBuilder,
) -> (ClientCore, Arc<Mutex<Vec<String>>>) {
    let output = Arc::new(Mutex::new(Vec::new()));
    let core = ClientCore::build_with_development(
        "https://protected.example.test/v1/",
        HttpConfigBuilder::new().http1_only(),
        runtime.hooks(hooks).debug(DebugLevel::Headers),
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
            rate_key: None,
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
            rate_key: None,
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
async fn real_tls_authentication_recovery_performs_two_visible_limiter_acquisitions() {
    let server = TlsServer::start().expect("TLS recovery loopback server");
    server.queue_status(401).expect("challenge status");
    server.queue_status(200).expect("success status");

    let provider_acquires = Arc::new(AtomicUsize::new(0));
    let provider_refreshes = Arc::new(AtomicUsize::new(0));
    let exact_current = Arc::new(AtomicBool::new(false));
    let selector = SlotSelector::provider(
        RotatingProvider {
            acquires: Arc::clone(&provider_acquires),
            refreshes: Arc::clone(&provider_refreshes),
            exact_current_seen: Arc::clone(&exact_current),
        },
        Duration::ZERO,
    );
    let limiter_acquires = Arc::new(AtomicUsize::new(0));
    let limiter_observations = Arc::new(AtomicUsize::new(0));
    let limiter: Arc<dyn RateLimiter> = Arc::new(AuthRateLimiter::counting(
        &limiter_acquires,
        &limiter_observations,
    ));
    let core = ClientCore::build(
        &server.url("localhost", "/v1/"),
        HttpConfigBuilder::new()
            .http1_only()
            .add_root_certificate_pem(server.certificate_pem()),
        RuntimeConfigBuilder::new().rate_limiter(limiter),
    )
    .expect("managed TLS recovery core");

    let output = request(
        core,
        &selector,
        ChallengePolicy::Unauthorized,
        RequestBody::bytes(Bytes::new()),
    )
    .await
    .expect("real loopback recovery");
    assert_eq!(output, Bytes::from_static(b"ok"));
    server
        .wait_for_requests(2, Duration::from_secs(2))
        .expect("both recovery attempts reached TLS loopback");
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0].header("authorization"),
        Some("Bearer initial-auth-token-1148c7")
    );
    assert_eq!(
        requests[1].header("authorization"),
        Some("Bearer refreshed-auth-token-709fe2")
    );
    assert_eq!(provider_acquires.load(Ordering::SeqCst), 1);
    assert_eq!(provider_refreshes.load(Ordering::SeqCst), 1);
    assert!(exact_current.load(Ordering::SeqCst));
    assert_eq!(limiter_acquires.load(Ordering::SeqCst), 2);
    assert_eq!(limiter_observations.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn paginated_tls_recovery_keeps_one_page_identity_index_and_completion() {
    let server = TlsServer::start().expect("TLS pagination recovery server");
    server.queue_status(401).expect("challenge status");
    server.queue_status(200).expect("success status");

    let provider_acquires = Arc::new(AtomicUsize::new(0));
    let provider_refreshes = Arc::new(AtomicUsize::new(0));
    let exact_current = Arc::new(AtomicBool::new(false));
    let selector = SlotSelector::provider(
        RotatingProvider {
            acquires: Arc::clone(&provider_acquires),
            refreshes: Arc::clone(&provider_refreshes),
            exact_current_seen: Arc::clone(&exact_current),
        },
        Duration::ZERO,
    );
    let limiter_acquires = Arc::new(AtomicUsize::new(0));
    let limiter_observations = Arc::new(AtomicUsize::new(0));
    let limiter: Arc<dyn RateLimiter> = Arc::new(AuthRateLimiter::counting(
        &limiter_acquires,
        &limiter_observations,
    ));
    let hooks = Arc::new(AuthHooks::default());
    let core = ClientCore::build(
        &server.url("localhost", "/v1/"),
        HttpConfigBuilder::new()
            .http1_only()
            .add_root_certificate_pem(server.certificate_pem()),
        RuntimeConfigBuilder::new()
            .hooks(hooks.clone())
            .rate_limiter(limiter),
    )
    .expect("managed TLS pagination core");

    let items = paginate_with(
        Request::new(
            core,
            PaginatedProtectedEndpoint {
                credential: CredentialRef::new(Arc::clone(&selector)),
            },
        ),
        PageLimit::Take(1),
        |_endpoint| Ok(OneAuthenticationPage),
        |_endpoint, _controller| Ok(()),
        |_controller| None,
    )
    .await
    .expect("paginated authentication recovery");
    assert_eq!(items, b"ok");
    server
        .wait_for_requests(2, Duration::from_secs(2))
        .expect("both page attempts reached TLS loopback");
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0].header("authorization"),
        Some("Bearer initial-auth-token-1148c7")
    );
    assert_eq!(
        requests[1].header("authorization"),
        Some("Bearer refreshed-auth-token-709fe2")
    );
    assert_eq!(provider_acquires.load(Ordering::SeqCst), 1);
    assert_eq!(provider_refreshes.load(Ordering::SeqCst), 1);
    assert!(exact_current.load(Ordering::SeqCst));
    assert_eq!(limiter_acquires.load(Ordering::SeqCst), 2);
    assert_eq!(limiter_observations.load(Ordering::SeqCst), 2);

    let before = hooks.before.lock().expect("before records").clone();
    assert_eq!(before.len(), 2);
    assert_eq!(before[0].operation_id, before[1].operation_id);
    assert_eq!(before[0].parent_operation_id, before[1].parent_operation_id);
    assert!(before[0].parent_operation_id.is_some());
    assert_eq!([before[0].page_index, before[1].page_index], [0, 0]);
    assert_eq!([before[0].recovery_index, before[1].recovery_index], [0, 1]);

    let responses = hooks.response.lock().expect("response records");
    assert_eq!(responses.len(), 2);
    assert_eq!(responses[0].0, responses[1].0);
    assert_eq!([responses[0].1, responses[1].1], [0, 1]);
    assert_eq!([responses[0].4, responses[1].4], [0, 0]);
    assert_eq!(responses[0].3, responses[1].3);
    drop(responses);

    let completion = hooks.completion.lock().expect("completion records");
    assert_eq!(completion.len(), 1);
    assert_eq!(completion[0].0, before[0].operation_id);
    assert!(completion[0].1);
    assert_eq!(completion[0].2, Some(StatusCode::OK));
    assert_eq!(completion[0].3, before[0].parent_operation_id);
    assert_eq!(completion[0].4, 0);
    drop(completion);

    let pagination = hooks.pagination.lock().expect("pagination records");
    assert_eq!(pagination.len(), 1);
    assert_eq!(Some(pagination[0].0), before[0].parent_operation_id);
    assert_eq!(pagination[0].1, 1);
    assert_eq!(pagination[0].2, 2);
    assert!(pagination[0].3);
    assert_eq!(pagination[0].4, Some(StatusCode::OK));
    assert_eq!(pagination[0].5, CompletionOutcome::Success);
}

#[tokio::test]
async fn paginated_authentication_failure_keeps_its_ordinary_category_and_correlated_completion() {
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(AuthHooks::default());
    let (core, _debug) = core(&executor, Arc::clone(&hooks));
    let selector = SlotSelector::provider(FailingProvider, Duration::ZERO);

    let error = api_error(
        paginate_with(
            Request::new(
                core,
                PaginatedProtectedEndpoint {
                    credential: CredentialRef::new(Arc::clone(&selector)),
                },
            ),
            PageLimit::Take(1),
            |_endpoint| Ok(OneAuthenticationPage),
            |_endpoint, _controller| Ok(()),
            |_controller| None,
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::AuthenticationProvider);
    assert_eq!(error.auth_kind(), Some(AuthErrorKind::Acquisition));
    assert_eq!(error.pagination_kind(), None);
    assert_eq!(executor.application().request_count().expect("count"), 0);

    let before = hooks.before.lock().expect("before records");
    assert_eq!(before.len(), 1);
    assert!(before[0].parent_operation_id.is_some());
    let page_operation_id = before[0].operation_id;
    let aggregate_operation_id = before[0]
        .parent_operation_id
        .expect("pagination parent operation");
    drop(before);

    assert!(hooks.response.lock().expect("response records").is_empty());
    let completion = hooks.completion.lock().expect("completion records");
    assert_eq!(completion.len(), 1);
    assert_eq!(completion[0].0, page_operation_id);
    assert_eq!(completion[0].2, None);
    assert_eq!(completion[0].3, Some(aggregate_operation_id));
    assert_eq!(completion[0].4, 0);
    assert_eq!(completion[0].5, CompletionOutcome::AuthenticationProvider);
    drop(completion);

    let pagination = hooks.pagination.lock().expect("pagination records");
    assert_eq!(pagination.len(), 1);
    assert_eq!(pagination[0].0, aggregate_operation_id);
    assert_eq!(pagination[0].1, 0);
    assert_eq!(pagination[0].2, 0);
    assert_eq!(pagination[0].4, None);
    assert_eq!(pagination[0].5, CompletionOutcome::AuthenticationProvider);
}

#[tokio::test]
async fn real_tls_provider_http_remains_outside_application_limiter_lifecycle() {
    let provider_server = TlsServer::start().expect("TLS provider loopback server");
    let application_server = TlsServer::start().expect("TLS application loopback server");
    let provider_calls = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        LoopbackHttpProvider {
            url: url::Url::parse(&provider_server.url("localhost", "/token"))
                .expect("provider URL"),
            calls: Arc::clone(&provider_calls),
        },
        Duration::ZERO,
    );
    let hooks = Arc::new(AuthHooks::default());
    let limiter_acquires = Arc::new(AtomicUsize::new(0));
    let limiter_observations = Arc::new(AtomicUsize::new(0));
    let limiter: Arc<dyn RateLimiter> = Arc::new(AuthRateLimiter::counting(
        &limiter_acquires,
        &limiter_observations,
    ));
    let core = ClientCore::build(
        &application_server.url("localhost", "/v1/"),
        HttpConfigBuilder::new()
            .http1_only()
            .add_root_certificate_pem(application_server.certificate_pem()),
        RuntimeConfigBuilder::new()
            .hooks(hooks.clone())
            .rate_limiter(limiter),
    )
    .expect("managed application/provider core");

    assert_eq!(
        request(
            core,
            &selector,
            ChallengePolicy::Never,
            RequestBody::bytes(Bytes::new()),
        )
        .await
        .expect("provider-backed real application request"),
        Bytes::from_static(b"ok")
    );
    provider_server
        .wait_for_requests(1, Duration::from_secs(2))
        .expect("provider request reached TLS loopback");
    application_server
        .wait_for_requests(1, Duration::from_secs(2))
        .expect("application request reached TLS loopback");
    assert_eq!(provider_calls.load(Ordering::SeqCst), 1);
    assert_eq!(provider_server.requests().len(), 1);
    assert_eq!(
        provider_server.requests()[0].header("x-provider-probe"),
        Some("isolated-provider-probe")
    );
    assert_eq!(application_server.requests().len(), 1);
    assert_eq!(
        application_server.requests()[0].header("authorization"),
        Some("Bearer real-provider-application-token")
    );
    assert_eq!(hooks.before.lock().expect("before hooks").len(), 1);
    assert_eq!(hooks.response.lock().expect("response hooks").len(), 1);
    assert_eq!(hooks.completion.lock().expect("completion hooks").len(), 1);
    assert_eq!(limiter_acquires.load(Ordering::SeqCst), 1);
    assert_eq!(limiter_observations.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn paginated_provider_http_never_enters_page_lifecycle_or_limiter_work() {
    let provider_server = TlsServer::start().expect("TLS pagination provider server");
    let application_server = TlsServer::start().expect("TLS pagination application server");
    let provider_calls = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        LoopbackHttpProvider {
            url: url::Url::parse(&provider_server.url("localhost", "/token"))
                .expect("provider URL"),
            calls: Arc::clone(&provider_calls),
        },
        Duration::ZERO,
    );
    let hooks = Arc::new(AuthHooks::default());
    let limiter_acquires = Arc::new(AtomicUsize::new(0));
    let limiter_observations = Arc::new(AtomicUsize::new(0));
    let limiter: Arc<dyn RateLimiter> = Arc::new(AuthRateLimiter::counting(
        &limiter_acquires,
        &limiter_observations,
    ));
    let core = ClientCore::build(
        &application_server.url("localhost", "/v1/"),
        HttpConfigBuilder::new()
            .http1_only()
            .add_root_certificate_pem(application_server.certificate_pem()),
        RuntimeConfigBuilder::new()
            .hooks(hooks.clone())
            .rate_limiter(limiter),
    )
    .expect("managed pagination application/provider core");

    let items = paginate_with(
        Request::new(
            core,
            PaginatedProtectedEndpoint {
                credential: CredentialRef::new(Arc::clone(&selector)),
            },
        ),
        PageLimit::Take(1),
        |_endpoint| Ok(OneAuthenticationPage),
        |_endpoint, _controller| Ok(()),
        |_controller| None,
    )
    .await
    .expect("provider-backed pagination");
    assert_eq!(items, b"ok");
    provider_server
        .wait_for_requests(1, Duration::from_secs(2))
        .expect("provider request reached TLS loopback");
    application_server
        .wait_for_requests(1, Duration::from_secs(2))
        .expect("application page reached TLS loopback");

    assert_eq!(provider_calls.load(Ordering::SeqCst), 1);
    assert_eq!(provider_server.requests().len(), 1);
    assert_eq!(application_server.requests().len(), 1);
    assert_eq!(hooks.before.lock().expect("before hooks").len(), 1);
    assert_eq!(hooks.response.lock().expect("response hooks").len(), 1);
    assert_eq!(hooks.completion.lock().expect("completion hooks").len(), 1);
    assert_eq!(hooks.pagination.lock().expect("pagination hooks").len(), 1);
    assert_eq!(limiter_acquires.load(Ordering::SeqCst), 1);
    assert_eq!(limiter_observations.load(Ordering::SeqCst), 1);
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
    let limiter_acquires = Arc::new(AtomicUsize::new(0));
    let limiter_observations = Arc::new(AtomicUsize::new(0));
    let limiter_ready = Arc::new(Mutex::new(Vec::new()));
    let mut limiter = AuthRateLimiter::counting(&limiter_acquires, &limiter_observations);
    limiter.observed_selector = Some((Arc::clone(&selector), Arc::clone(&limiter_ready)));
    let limiter = Arc::new(limiter);
    let (core, debug) = core_with_runtime(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().rate_limiter(limiter),
    );

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
    assert_eq!(limiter_acquires.load(Ordering::SeqCst), 2);
    assert_eq!(limiter_observations.load(Ordering::SeqCst), 2);
    assert_eq!(
        limiter_ready.lock().expect("limiter readiness").as_slice(),
        [true, true],
        "limiter observation precedes challenge mutation on the first response"
    );
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
    let limiter_acquires = Arc::new(AtomicUsize::new(0));
    let limiter_observations = Arc::new(AtomicUsize::new(0));
    let (core, debug) = core_with_runtime(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().rate_limiter(Arc::new(AuthRateLimiter::counting(
            &limiter_acquires,
            &limiter_observations,
        ))),
    );
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
    assert_eq!(limiter_acquires.load(Ordering::SeqCst), 1);
    assert_eq!(limiter_observations.load(Ordering::SeqCst), 1);

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

#[cfg(feature = "rate-limit-governor")]
#[tokio::test]
async fn configured_key_limit_failure_cuts_off_every_protected_side_effect_and_redacts_key() {
    const KEY_SENTINEL: &str = "configured-key-private-73cf9a0d";
    const KEY_LIMIT: usize = 64;
    const KEY_PART_NAME_BYTES: usize = "vars.tenant".len();
    let executor = DeterministicExecutor::new();
    let provider_calls = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        CountingProvider {
            calls: Arc::clone(&provider_calls),
        },
        Duration::ZERO,
    );
    let body_calls = Arc::new(AtomicUsize::new(0));
    let hooks = Arc::new(AuthHooks::default());
    let governor = Arc::new(
        GovernorRateLimiter::new()
            .key_limit(KEY_LIMIT)
            .expect("configured key limit"),
    );
    let limiter: Arc<dyn RateLimiter> = governor.clone();
    let (core, debug) = core_with_runtime(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().rate_limiter(limiter),
    );
    let mut rate_key = KEY_SENTINEL.to_owned();
    rate_key.extend(std::iter::repeat_n(
        'x',
        KEY_LIMIT + 1 - KEY_PART_NAME_BYTES - rate_key.len(),
    ));
    assert_eq!(rate_key.len() + KEY_PART_NAME_BYTES, KEY_LIMIT + 1);
    let error = api_error(
        Request::new(
            core,
            ProtectedEndpoint {
                credential: CredentialRef::new(selector),
                challenge: ChallengePolicy::Never,
                body: RequestBody::factory(ReplayFactory {
                    calls: Arc::clone(&body_calls),
                }),
                query: false,
                rate_key: Some(rate_key),
            },
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(
        error.configuration_kind(),
        Some(crate::error::ConfigurationErrorKind::InvalidRateLimit)
    );
    assert_eq!(body_calls.load(Ordering::SeqCst), 0);
    assert_eq!(provider_calls.load(Ordering::SeqCst), 0);
    assert_eq!(governor.test_acquisition_count(), 0);
    assert_eq!(governor.test_quota_entry_count(), 0);
    assert!(hooks.before.lock().expect("before records").is_empty());
    assert!(hooks.response.lock().expect("response records").is_empty());
    assert_eq!(
        hooks.completion.lock().expect("completion records").len(),
        1
    );
    assert_eq!(executor.application().request_count().expect("count"), 0);
    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(&error);
    while let Some(current) = source {
        assert!(!current.to_string().contains(KEY_SENTINEL));
        assert!(!format!("{current:?}").contains(KEY_SENTINEL));
        source = current.source();
    }
    let debug = debug.lock().expect("debug output");
    assert_eq!(debug.len(), 1);
    assert!(debug[0].starts_with("completion "));
    assert!(!debug[0].contains(KEY_SENTINEL));
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
    let limiter_acquires = Arc::new(AtomicUsize::new(0));
    let limiter = Arc::new(AuthRateLimiter::counting(
        &limiter_acquires,
        &Arc::new(AtomicUsize::new(0)),
    ));
    let (core, debug) = core_with_runtime(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().rate_limiter(limiter),
    );
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
    assert_eq!(limiter_acquires.load(Ordering::SeqCst), 0);
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
    let limiter_acquires = Arc::new(AtomicUsize::new(0));
    let limiter = Arc::new(AuthRateLimiter::counting(
        &limiter_acquires,
        &Arc::new(AtomicUsize::new(0)),
    ));
    let (core, debug) = core_with_runtime(
        &executor,
        hooks,
        RuntimeConfigBuilder::new().rate_limiter(limiter),
    );
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
    assert_eq!(limiter_acquires.load(Ordering::SeqCst), 0);
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
async fn limiter_failure_is_typed_and_prevents_finalization_debug_and_send() {
    let executor = DeterministicExecutor::new();
    let selector = SlotSelector::static_value(AccessToken::new(INITIAL), Duration::ZERO);
    let hooks = Arc::new(AuthHooks::default());
    let limiter_acquires = Arc::new(AtomicUsize::new(0));
    let (core, debug) = core_with_runtime(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new()
            .rate_limiter(Arc::new(AuthRateLimiter::failing(&limiter_acquires))),
    );
    let error = api_error(
        request(
            core,
            &selector,
            ChallengePolicy::Never,
            RequestBody::bytes(Bytes::new()),
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::RateLimit);
    assert_eq!(
        error.rate_limit_kind(),
        Some(crate::error::RateLimitErrorKind::Acquire)
    );
    assert!(
        error
            .to_string()
            .contains("safe application limiter rejection")
    );
    assert_eq!(limiter_acquires.load(Ordering::SeqCst), 1);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    assert!(hooks.response.lock().expect("responses").is_empty());
    assert!(
        debug
            .lock()
            .expect("debug output")
            .iter()
            .all(|line| !line.starts_with("request-ready "))
    );
}

#[tokio::test]
async fn clear_during_limiter_wait_prevents_stale_credential_send() {
    let executor = DeterministicExecutor::new();
    let selector = SlotSelector::static_value(AccessToken::new(INITIAL), Duration::ZERO);
    let gate = Gate::closed();
    let limiter_acquires = Arc::new(AtomicUsize::new(0));
    let (core, _) = core_with_runtime(
        &executor,
        Arc::new(AuthHooks::default()),
        RuntimeConfigBuilder::new().rate_limiter(Arc::new(AuthRateLimiter::gated(
            &limiter_acquires,
            gate.clone(),
        ))),
    );
    let pending_selector = Arc::clone(&selector);
    let task = tokio::spawn(async move {
        request(
            core,
            &pending_selector,
            ChallengePolicy::Never,
            RequestBody::bytes(Bytes::new()),
        )
        .await
    });
    gate.wait_until_reached().await;
    selector.clear().expect("clear while limiter waits");
    gate.open();
    let error = api_error(task.await.expect("request task"));
    assert_eq!(error.category(), ErrorCategory::AuthenticationUnavailable);
    assert_eq!(limiter_acquires.load(Ordering::SeqCst), 1);
    assert_eq!(executor.application().request_count().expect("count"), 0);
}

#[tokio::test]
async fn complete_source_replacement_during_wait_uses_only_the_new_slot() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new().header(
                AUTHORIZATION,
                HeaderValue::from_static("Bearer newer-auth-token-265f0a"),
            ),
            ScriptedResponse::success(Bytes::from_static(b"new slot")),
        )
        .expect("new-slot response");
    let selector = SlotSelector::static_value(AccessToken::new(INITIAL), Duration::ZERO);
    let gate = Gate::closed();
    let limiter_acquires = Arc::new(AtomicUsize::new(0));
    let (core, _) = core_with_runtime(
        &executor,
        Arc::new(AuthHooks::default()),
        RuntimeConfigBuilder::new().rate_limiter(Arc::new(AuthRateLimiter::gated(
            &limiter_acquires,
            gate.clone(),
        ))),
    );
    let pending_selector = Arc::clone(&selector);
    let task = tokio::spawn(async move {
        request(
            core,
            &pending_selector,
            ChallengePolicy::Never,
            RequestBody::bytes(Bytes::new()),
        )
        .await
    });
    gate.wait_until_reached().await;
    selector
        .replace_static(AccessToken::new(NEWER))
        .expect("complete source replacement");
    gate.open();
    assert_eq!(
        task.await.expect("request task").expect("new-slot send"),
        Bytes::from_static(b"new slot")
    );
    assert_eq!(limiter_acquires.load(Ordering::SeqCst), 1);
    assert_eq!(executor.application().request_count().expect("count"), 1);
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
            expire_after_preflight: true,
        },
        Duration::ZERO,
    );
    let gate = Gate::closed();
    let limiter_acquires = Arc::new(AtomicUsize::new(0));
    let (core, debug) = core_with_runtime(
        &executor,
        Arc::new(AuthHooks::default()),
        RuntimeConfigBuilder::new().rate_limiter(Arc::new(AuthRateLimiter::gated(
            &limiter_acquires,
            gate.clone(),
        ))),
    );
    let task = tokio::spawn(async move {
        Request::new(
            core,
            FinalizationExpiryEndpoint {
                credential: CredentialRef::new(selector),
            },
        )
        .await
    });
    gate.wait_until_reached().await;
    gate.open();
    let error = api_error(task.await.expect("request task"));
    assert_eq!(error.category(), ErrorCategory::AuthenticationUnavailable);
    assert!(checks.load(Ordering::SeqCst) >= 2);
    assert_eq!(limiter_acquires.load(Ordering::SeqCst), 1);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    assert!(
        debug
            .lock()
            .expect("debug output")
            .iter()
            .all(|line| !line.starts_with("request-ready "))
    );
}

#[cfg(feature = "rate-limit-governor")]
#[tokio::test(start_paused = true)]
async fn reservation_is_retained_after_credential_finalization_failure() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new().header(
                AUTHORIZATION,
                HeaderValue::from_static("Bearer newer-auth-token-265f0a"),
            ),
            ScriptedResponse::success(Bytes::from_static(b"after finalization failure")),
        )
        .expect("post-failure response");
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));
    let expiring = SlotSelector::static_value(
        FinalizationExpiryToken {
            value: SecretString::new(INITIAL),
            freshness_checks: Arc::new(AtomicUsize::new(0)),
            expire_after_preflight: true,
        },
        Duration::ZERO,
    );
    let error = api_error(
        Request::new(
            core.clone(),
            FinalizationExpiryEndpoint {
                credential: CredentialRef::new(expiring),
            },
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::AuthenticationUnavailable);
    assert_eq!(executor.application().request_count().expect("count"), 0);

    let stable = SlotSelector::static_value(
        FinalizationExpiryToken {
            value: SecretString::new(NEWER),
            freshness_checks: Arc::new(AtomicUsize::new(0)),
            expire_after_preflight: false,
        },
        Duration::ZERO,
    );
    let later = tokio::spawn(async move {
        Request::new(
            core,
            FinalizationExpiryEndpoint {
                credential: CredentialRef::new(stable),
            },
        )
        .await
    });
    tokio::task::yield_now().await;
    assert_eq!(executor.application().request_count().expect("count"), 0);
    tokio::time::advance(Duration::from_secs(9)).await;
    tokio::task::yield_now().await;
    assert_eq!(executor.application().request_count().expect("count"), 0);
    tokio::time::advance(Duration::from_secs(1)).await;
    assert_eq!(
        later.await.expect("later task").expect("later send"),
        Bytes::from_static(b"after finalization failure")
    );
    assert_eq!(executor.application().request_count().expect("count"), 1);
}

#[tokio::test]
async fn expiry_during_limiter_wait_refreshes_at_current_finalization() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new().header(
                AUTHORIZATION,
                HeaderValue::from_static("Bearer refreshed-auth-token-709fe2"),
            ),
            ScriptedResponse::success(Bytes::from_static(b"refreshed after wait")),
        )
        .expect("refreshed response");
    let expired = Arc::new(AtomicBool::new(false));
    let acquires = Arc::new(AtomicUsize::new(0));
    let refreshes = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        LimiterWaitExpiryProvider {
            initial_expired: Arc::clone(&expired),
            acquires: Arc::clone(&acquires),
            refreshes: Arc::clone(&refreshes),
        },
        Duration::ZERO,
    );
    let gate = Gate::closed();
    let limiter_acquires = Arc::new(AtomicUsize::new(0));
    let (core, _) = core_with_runtime(
        &executor,
        Arc::new(AuthHooks::default()),
        RuntimeConfigBuilder::new().rate_limiter(Arc::new(AuthRateLimiter::gated(
            &limiter_acquires,
            gate.clone(),
        ))),
    );
    let task = tokio::spawn(async move {
        Request::new(
            core,
            LimiterWaitExpiryEndpoint {
                credential: CredentialRef::new(selector),
            },
        )
        .await
    });
    gate.wait_until_reached().await;
    expired.store(true, Ordering::SeqCst);
    gate.open();
    assert_eq!(
        task.await.expect("request task").expect("refreshed send"),
        Bytes::from_static(b"refreshed after wait")
    );
    assert_eq!(acquires.load(Ordering::SeqCst), 1);
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
    assert_eq!(limiter_acquires.load(Ordering::SeqCst), 1);
    assert_eq!(executor.application().request_count().expect("count"), 1);
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
