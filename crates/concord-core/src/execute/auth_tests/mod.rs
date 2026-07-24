#![cfg(feature = "default-tls")]

use std::num::NonZeroU32;
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
    CredentialProvider, CredentialReason, CredentialRef, CredentialRequest,
    MAX_CREDENTIAL_SECRET_BYTES, RecoveryMode, SecretCredential, SecretString, SlotSelector,
};
use crate::body::{
    BodyError, BodyFactory, EmptyBody, RequestBody, RequestBodyContext, RequestBodyMetadata,
    RequestBodySpec, Stream as StreamSpec, StreamBody,
};
#[cfg(feature = "multipart")]
use crate::body::{MultipartBody, MultipartPart};
use crate::client::ClientCore;
use crate::codec::{CodecError, DecodeBody, DecodeContext, EncodeBody, EncodeContext, OctetStream};
use crate::endpoint::EndpointId;
use crate::error::{ApiError, AuthErrorKind, ErrorCategory};
use crate::hooks::{
    BeforeSend, BoxFuture, CompletionEvent, CompletionOutcome, DebugLevel,
    PaginationCompletionEvent, ResponseHeadEvent, RuntimeHooks,
};
use crate::http::HttpConfigBuilder;
use crate::http::provider::{ProviderBody, ProviderRequest};
use crate::operation::OperationId;
use crate::pagination::{
    PageLimit, PaginatedEndpoint, PaginatedRequest, PaginationController, PaginationError,
};
use crate::rate_limit::{
    RateLimitError, RateLimitId, RateLimitKey, RateLimitPermit, RateLimitRequest,
    RateLimitResponse, RateLimitRule, RateLimitWindow, RateLimiter,
};
use crate::request::{
    EndpointSpec, HeadBuilder, PreparedHead, Request, RequestMeta, RuntimeEndpoint,
};
use crate::runtime::RuntimeConfigBuilder;

const ENDPOINT: &str = "authentication_api.protected";
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
    page: u8,
}

struct AuthenticationPageCodec;

struct PreparedRecoveryCodec;

struct PreparedRecoveryInput {
    calls: Arc<AtomicUsize>,
}

#[derive(Clone)]
struct OrderedPreparedCredential {
    id: &'static str,
    value: SecretString,
    calls: Arc<Mutex<Vec<&'static str>>>,
}

impl CredentialMaterial for OrderedPreparedCredential {}

impl SecretCredential for OrderedPreparedCredential {
    fn expose_secret(&self) -> &str {
        self.calls
            .lock()
            .expect("prepared authentication order")
            .push(self.id);
        self.value.expose_secret()
    }
}

impl EncodeBody for PreparedRecoveryCodec {
    type Input = PreparedRecoveryInput;

    const CONTENT_TYPE: Option<&'static str> = None;

    fn encode(input: Self::Input, _context: EncodeContext<'_>) -> Result<Bytes, CodecError> {
        input.calls.fetch_add(1, Ordering::SeqCst);
        Ok(Bytes::from_static(b"prepared-codec"))
    }
}

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
    type Response = AuthenticationPageCodec;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(ENDPOINT, Method::GET, Some(Duration::from_secs(30)))
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_static("paginated-protected")?;
        builder.push_value(&self.page)?;
        builder.push_rate_limit_rule(
            "AuthenticationApi::rate_limit::pagination",
            std::iter::empty::<(&str, &str)>(),
            1,
            [(1, Duration::from_secs(1))],
        )?;
        builder.push_auth(
            Auth::bearer(self.credential.clone()).challenge(ChallengePolicy::Unauthorized),
        )?;
        Ok(builder.finish())
    }

    fn body_metadata(
        &self,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        EmptyBody::metadata(&(), context)
    }

    fn into_body(self) {}
}

#[derive(Clone)]
struct TwoAuthenticationPages {
    page: u8,
}

impl PaginationController<Vec<u8>> for TwoAuthenticationPages {
    fn next(self, _page: &Vec<u8>) -> Result<Option<Self>, PaginationError> {
        if self.page == 0 {
            Ok(Some(Self { page: 1 }))
        } else {
            Ok(None)
        }
    }
}

impl PaginatedEndpoint for PaginatedProtectedEndpoint {
    type Item = u8;
    type Controller = TwoAuthenticationPages;

    fn initial_controller(&self) -> Result<Self::Controller, PaginationError> {
        Ok(TwoAuthenticationPages { page: 0 })
    }

    fn bind_controller(
        endpoint: &mut Self,
        controller: &Self::Controller,
    ) -> Result<(), PaginationError> {
        endpoint.page = controller.page;
        Ok(())
    }
}

struct BasicEndpoint {
    credential: CredentialRef<BasicCredential>,
}

impl EndpointSpec for BasicEndpoint {
    type Body = RequestBody;
    type Response = Bytes;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(ENDPOINT, Method::GET, Some(Duration::from_secs(30)))
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_static("basic")?;
        builder.push_auth(Auth::basic(self.credential.clone()))?;
        Ok(builder.finish())
    }

    fn body_metadata(
        &self,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        let body = RequestBody::bytes(Bytes::new());
        RequestBody::metadata(&body, context)
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
    type Response = Bytes;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(ENDPOINT, Method::GET, Some(Duration::from_secs(30)))
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_static("header")?;
        builder.push_auth(
            Auth::header(
                http::HeaderName::from_static("x-api-key"),
                self.credential.clone(),
            )
            .expect("valid header placement"),
        )?;
        Ok(builder.finish())
    }

    fn body_metadata(
        &self,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        RequestBody::metadata(&RequestBody::bytes(Bytes::new()), context)
    }

    fn into_body(self) -> Self::Body {
        RequestBody::bytes(Bytes::new())
    }
}

impl EndpointSpec for ProtectedStreamEndpoint {
    type Body = RequestBody;
    type Response = StreamSpec<OctetStream>;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(ENDPOINT, Method::GET, Some(Duration::from_secs(30)))
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_static("protected-stream")?;
        builder.push_auth(
            Auth::bearer(self.credential.clone()).challenge(ChallengePolicy::Unauthorized),
        )?;
        Ok(builder.finish())
    }

    fn body_metadata(
        &self,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        RequestBody::metadata(&RequestBody::bytes(Bytes::new()), context)
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
    type Response = Bytes;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(ENDPOINT, Method::POST, Some(Duration::from_secs(30)))
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_static("protected")?;
        builder.push_auth(Auth::bearer(self.first.clone()).challenge(self.first_challenge))?;
        builder.push_auth(
            Auth::header(
                http::HeaderName::from_static("x-secondary-token"),
                self.second.clone(),
            )
            .expect("valid header placement"),
        )?;
        Ok(builder.finish())
    }

    fn body_metadata(
        &self,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        RequestBody::metadata(
            &RequestBody::factory(ReplayFactory {
                calls: Arc::clone(&self.body_calls),
            }),
            context,
        )
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
    type Response = Bytes;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(ENDPOINT, Method::POST, Some(Duration::from_secs(30)))
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_auth(
            Auth::query("access", self.credential.clone()).expect("valid query placement"),
        )?;
        builder.push_query_value("access", &"materially-present-public-value")?;
        Ok(builder.finish())
    }

    fn body_metadata(
        &self,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        RequestBody::metadata(
            &RequestBody::factory(ReplayFactory {
                calls: Arc::clone(&self.body_calls),
            }),
            context,
        )
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
    type Response = Bytes;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(ENDPOINT, Method::POST, Some(Duration::from_secs(30)))
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_static("expiry-refresh")?;
        builder.push_auth(Auth::bearer(self.credential.clone()))?;
        Ok(builder.finish())
    }

    fn body_metadata(
        &self,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        RequestBody::metadata(&RequestBody::bytes(Bytes::new()), context)
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
    type Response = Bytes;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(ENDPOINT, Method::GET, Some(Duration::from_secs(30)))
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_static("exposure")?;
        builder.push_auth(Auth::bearer(self.credential.clone()))?;
        Ok(builder.finish())
    }

    fn body_metadata(
        &self,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        RequestBody::metadata(&RequestBody::bytes(Bytes::new()), context)
    }

    fn into_body(self) -> Self::Body {
        RequestBody::bytes(Bytes::new())
    }
}

impl EndpointSpec for FinalizationExpiryEndpoint {
    type Body = RequestBody;
    type Response = Bytes;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(ENDPOINT, Method::POST, Some(Duration::from_secs(30)))
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_rate_limit_rule(
            "AuthenticationApi::rate_limit::finalization",
            std::iter::empty::<(&str, &str)>(),
            1,
            [(1, Duration::from_secs(10))],
        )?;
        builder.push_auth(Auth::bearer(self.credential.clone()))?;
        Ok(builder.finish())
    }

    fn body_metadata(
        &self,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        RequestBody::metadata(&RequestBody::bytes(Bytes::new()), context)
    }

    fn into_body(self) -> Self::Body {
        RequestBody::bytes(Bytes::new())
    }
}

impl EndpointSpec for ProtectedEndpoint {
    type Body = RequestBody;
    type Response = Bytes;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(ENDPOINT, Method::POST, Some(Duration::from_secs(30)))
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_static("protected")?;
        let auth = if self.query {
            Auth::query("access", self.credential.clone()).expect("valid query placement")
        } else {
            Auth::bearer(self.credential.clone())
        };
        builder.push_auth(auth.challenge(self.challenge))?;
        if let Some(rate_key) = &self.rate_key {
            builder.push_rate_limit_rule(
                "AuthenticationApi::rate_limit::tenant",
                [("vars.tenant", rate_key.as_str())],
                1,
                [(1, Duration::from_secs(1))],
            )?;
        }
        Ok(builder.finish())
    }

    fn body_metadata(
        &self,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        RequestBody::metadata(&self.body, context)
    }

    fn into_body(self) -> Self::Body {
        self.body
    }
}

impl crate::request::RawResponseEndpoint for ProtectedEndpoint {}

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

struct OversizedMaterialProvider;

impl CredentialProvider for OversizedMaterialProvider {
    type Material = AccessToken;

    fn acquire<'a>(
        &'a self,
        _context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        Box::pin(async {
            let mut value = "x".repeat(MAX_CREDENTIAL_SECRET_BYTES);
            value.push_str("provider-material-sentinel-82dc");
            Ok(AccessToken::new(value))
        })
    }
}

type ObservedSelector = (Arc<SlotSelector<AccessToken>>, Arc<Mutex<Vec<bool>>>);

struct AuthRateLimiter {
    acquisitions: Arc<AtomicUsize>,
    observations: Arc<AtomicUsize>,
    gate: Option<Gate>,
    failure: bool,
    observed_selector: Option<ObservedSelector>,
    plans: Option<Arc<Mutex<Vec<CapturedAuthPlan>>>>,
    key_limit: usize,
}

type CapturedAuthRule = (String, Vec<(String, String)>, u32, Vec<(u32, Duration)>);

#[derive(Clone, Debug, Eq, PartialEq)]
struct CapturedAuthPlan {
    page_index: u32,
    rules: Vec<CapturedAuthRule>,
}

fn capture_auth_plan(request: &RateLimitRequest<'_>) -> CapturedAuthPlan {
    CapturedAuthPlan {
        page_index: request.page_index,
        rules: request
            .plan
            .rules()
            .iter()
            .map(|rule| {
                (
                    rule.id().as_str().to_owned(),
                    rule.key()
                        .parts()
                        .iter()
                        .map(|part| (part.name().to_owned(), part.value().to_owned()))
                        .collect(),
                    rule.cost().get(),
                    rule.windows()
                        .iter()
                        .map(|window| (window.capacity().get(), window.duration()))
                        .collect(),
                )
            })
            .collect(),
    }
}

impl AuthRateLimiter {
    fn counting(acquisitions: &Arc<AtomicUsize>, observations: &Arc<AtomicUsize>) -> Self {
        Self {
            acquisitions: Arc::clone(acquisitions),
            observations: Arc::clone(observations),
            gate: None,
            failure: false,
            observed_selector: None,
            plans: None,
            key_limit: crate::rate_limit::DEFAULT_RATE_LIMIT_KEY_BYTES,
        }
    }

    fn capturing(
        acquisitions: &Arc<AtomicUsize>,
        observations: &Arc<AtomicUsize>,
        plans: &Arc<Mutex<Vec<CapturedAuthPlan>>>,
    ) -> Self {
        Self {
            acquisitions: Arc::clone(acquisitions),
            observations: Arc::clone(observations),
            gate: None,
            failure: false,
            observed_selector: None,
            plans: Some(Arc::clone(plans)),
            key_limit: crate::rate_limit::DEFAULT_RATE_LIMIT_KEY_BYTES,
        }
    }

    fn gated(acquisitions: &Arc<AtomicUsize>, gate: Gate) -> Self {
        Self {
            acquisitions: Arc::clone(acquisitions),
            observations: Arc::new(AtomicUsize::new(0)),
            gate: Some(gate),
            failure: false,
            observed_selector: None,
            plans: None,
            key_limit: crate::rate_limit::DEFAULT_RATE_LIMIT_KEY_BYTES,
        }
    }

    fn failing(acquisitions: &Arc<AtomicUsize>) -> Self {
        Self {
            acquisitions: Arc::clone(acquisitions),
            observations: Arc::new(AtomicUsize::new(0)),
            gate: None,
            failure: true,
            observed_selector: None,
            plans: None,
            key_limit: crate::rate_limit::DEFAULT_RATE_LIMIT_KEY_BYTES,
        }
    }
}

impl RateLimiter for AuthRateLimiter {
    fn preparation_key_limit(&self) -> usize {
        self.key_limit
    }

    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        self.acquisitions.fetch_add(1, Ordering::SeqCst);
        if let Some(plans) = &self.plans {
            plans
                .lock()
                .expect("authentication plan capture")
                .push(capture_auth_plan(&request));
        }
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

struct RetainedReservationLimiter {
    acquisitions: AtomicUsize,
    later_gate: Gate,
}

impl RateLimiter for RetainedReservationLimiter {
    fn acquire<'a>(
        &'a self,
        _request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        let acquisition = self.acquisitions.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            if acquisition > 0 {
                self.later_gate.wait().await;
            }
            Ok(RateLimitPermit::granted())
        })
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

fn prepared_rate_rule() -> RateLimitRule {
    RateLimitRule::new(
        RateLimitId::new("Prepared::rate_limit::recovery").expect("prepared rate ID"),
        RateLimitKey::global(),
        NonZeroU32::new(1).expect("prepared rate cost"),
        [RateLimitWindow::new(
            NonZeroU32::new(2).expect("prepared rate capacity"),
            Duration::from_secs(1),
        )
        .expect("prepared rate window")],
    )
    .expect("prepared rate rule")
}

fn api_error<T>(result: Result<T, ApiError>) -> ApiError {
    match result {
        Ok(_) => panic!("expected API failure"),
        Err(error) => error,
    }
}

mod materialization;
mod preflight;
mod provider;
mod recovery;
