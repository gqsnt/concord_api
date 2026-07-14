//! Native loopback ownership tests live here.  Each request is built by core as
//! a `reqwest::Request` and executed by the client's managed Reqwest instance.

use super::common::{
    MockResponse, NativeMockHarness, NativeMockReply, NativeReplyGate, ObservationAuthCx,
    ObservationAuthVars, ObservationRuntimeHooks, RecordingRateLimiter, TestAuthVars, TextEndpoint,
    auth_policy, client, configure_runtime, observation_client, request_plan,
};
#[cfg(feature = "dangerous-dev-tools")]
use crate::regression_tests::test_api::{
    AuthPlacement, AuthProvenance, AuthRequirement, AuthUsageId, CredentialRef,
};
#[cfg(feature = "multipart")]
use crate::regression_tests::test_api::{MultipartRequest, RequestEntity};
use crate::regression_tests::test_api::{
    PreparedBody, RawStreamResponse, RegressionEndpoint, RegressionIntoPlan, RegressionPlanContext,
    RequestPlan, ResolvedPolicy, ResponseEntity,
};
use bytes::Bytes;
use concord_core::advanced::{
    AuthError, AuthFuture, BodyError, BodyErrorKind, CredentialContext, CredentialId,
    CredentialProvider, CredentialProviderState, InvalidateReason, OctetStream, RateLimitContext,
    RateLimitFuture, RateLimitPermit, RateLimitResponseAction, RateLimitResponseContext,
    RateLimiter, StreamBody, StreamBodyError, StreamResponse,
};
#[cfg(feature = "multipart")]
use concord_core::advanced::{ErrorContext, MultipartBody};
use concord_core::error::ErrorCategory;
use concord_core::prelude::{
    ApiClient, ApiClientError, ApiKey, ClientContext, RateLimitObservation,
};
use http::{HeaderValue, Method, StatusCode};
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::Mutex;

struct OneShotStreamingChallenge {
    plan: RequestPlan,
}

impl RegressionEndpoint<ObservationAuthCx> for OneShotStreamingChallenge {
    type Response = StreamResponse<OctetStream>;

    fn execute<'a>(
        client: &'a ApiClient<ObservationAuthCx>,
        plan: RequestPlan,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, ApiClientError>> + Send + 'a>,
    > {
        <RawStreamResponse<OctetStream> as ResponseEntity>::execute(client, plan)
    }
}

impl RegressionIntoPlan<ObservationAuthCx> for OneShotStreamingChallenge {
    fn into_plan(
        self,
        _ctx: &RegressionPlanContext<'_, ObservationAuthCx>,
    ) -> Result<RequestPlan, ApiClientError> {
        Ok(self.plan)
    }
}

#[derive(Clone)]
struct ProviderBindingCx;

#[derive(Clone)]
struct ProviderAuthVars {
    state: ProviderAuthState,
}

#[derive(Clone)]
struct ProviderAuthState {
    challenged: Arc<CredentialProviderState<ProviderBindingCx, CountingCredentialProvider>>,
    unrelated: Arc<CredentialProviderState<ProviderBindingCx, CountingCredentialProvider>>,
    binding_resolutions: Arc<AtomicUsize>,
}

impl ClientContext for ProviderBindingCx {
    type Vars = ();
    type AuthVars = ProviderAuthVars;
    type AuthState = ProviderAuthState;

    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTP;
    const DOMAIN: &'static str = "example.com";

    fn init_auth_state(_vars: &Self::Vars, auth: &Self::AuthVars) -> Self::AuthState {
        auth.state.clone()
    }

    fn auth_provider_binding<'a>(
        credential: &CredentialId,
        auth_state: &'a Self::AuthState,
    ) -> Option<concord_core::advanced::AuthProviderBinding<'a, Self>> {
        auth_state
            .binding_resolutions
            .fetch_add(1, Ordering::SeqCst);
        let state = if credential == &CredentialId::new("test", "token") {
            &auth_state.challenged
        } else if credential == &CredentialId::new("test", "unrelated") {
            &auth_state.unrelated
        } else {
            return None;
        };
        Some(state.secret_binding(
            concord_core::advanced::AuthPreparationMode::RequestLocal,
            concord_core::advanced::AuthChallengeMode::Refresh,
        ))
    }
}

#[derive(Clone)]
struct CountingCredentialProvider {
    id: CredentialId,
    acquired: Arc<AtomicUsize>,
    refreshed: Arc<AtomicUsize>,
    invalidated: Arc<AtomicUsize>,
}

impl CredentialProvider<ProviderBindingCx> for CountingCredentialProvider {
    type Credential = ApiKey;

    fn id(&self) -> CredentialId {
        self.id.clone()
    }

    fn acquire<'a>(
        &'a self,
        _ctx: CredentialContext<'a, ProviderBindingCx>,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
        Box::pin(async move {
            let generation = self.acquired.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(ApiKey::new(format!("provider-token-{generation}")))
        })
    }

    fn refresh<'a>(
        &'a self,
        _ctx: CredentialContext<'a, ProviderBindingCx>,
        _current: &'a Self::Credential,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
        Box::pin(async move {
            self.refreshed.fetch_add(1, Ordering::SeqCst);
            Ok(ApiKey::new("unexpected-refresh"))
        })
    }

    fn invalidate<'a>(
        &'a self,
        _ctx: CredentialContext<'a, ProviderBindingCx>,
        _current: Option<&'a Self::Credential>,
        _reason: InvalidateReason,
    ) -> AuthFuture<'a, Result<(), AuthError>> {
        Box::pin(async move {
            self.invalidated.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    }
}

struct LimitedRecordingRateLimiter {
    events: Arc<Mutex<Vec<String>>>,
}

impl RateLimiter for LimitedRecordingRateLimiter {
    fn acquire<'a>(
        &'a self,
        _ctx: RateLimitContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitPermit, ApiClientError>> {
        Box::pin(async move {
            self.events.lock().await.push("rate_acquire".to_string());
            Ok(RateLimitPermit)
        })
    }

    fn on_response<'a>(
        &'a self,
        _ctx: RateLimitResponseContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitResponseAction, ApiClientError>> {
        Box::pin(async move {
            self.events.lock().await.push("rate_response".to_string());
            Ok(RateLimitResponseAction::Limited {
                retry_after: Some(Duration::ZERO),
                target: RateLimitObservation::limited().target,
                cooldown_stored: true,
            })
        })
    }
}

struct ByteChunks(std::collections::VecDeque<Bytes>);

impl ByteChunks {
    fn new(chunks: impl IntoIterator<Item = Bytes>) -> Self {
        Self(chunks.into_iter().collect())
    }
}

struct FirstChunkThenPending {
    first: Option<Bytes>,
    dropped: Arc<AtomicBool>,
    head_gate: Arc<UploadHeadGate>,
    events: Arc<Mutex<Vec<String>>>,
}

struct HeadGatedChunks {
    chunks: std::collections::VecDeque<Bytes>,
    head_gate: Arc<UploadHeadGate>,
}

impl HeadGatedChunks {
    fn new(chunks: impl IntoIterator<Item = Bytes>, head_gate: Arc<UploadHeadGate>) -> Self {
        Self {
            chunks: chunks.into_iter().collect(),
            head_gate,
        }
    }
}

#[derive(Default)]
struct UploadHeadGate {
    released: AtomicBool,
    waker: StdMutex<Option<std::task::Waker>>,
}

impl UploadHeadGate {
    fn release(&self) {
        self.released.store(true, Ordering::Release);
        if let Some(waker) = self.waker.lock().expect("upload gate waker").take() {
            waker.wake();
        }
    }
}

impl futures_core::Stream for FirstChunkThenPending {
    type Item = Result<Bytes, StreamBodyError>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,

        context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        if !self.head_gate.released.load(Ordering::Acquire) {
            *self.head_gate.waker.lock().expect("upload gate waker") =
                Some(context.waker().clone());
            return std::task::Poll::Pending;
        }
        match self.first.take() {
            Some(chunk) => {
                self.events
                    .try_lock()
                    .expect("request phase event lock")
                    .push("request_body_poll".to_string());
                std::task::Poll::Ready(Some(Ok(chunk)))
            }
            None => std::task::Poll::Pending,
        }
    }
}

impl futures_core::Stream for HeadGatedChunks {
    type Item = Result<Bytes, StreamBodyError>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        if !self.head_gate.released.load(Ordering::Acquire) {
            *self.head_gate.waker.lock().expect("upload gate waker") =
                Some(context.waker().clone());
            return std::task::Poll::Pending;
        }
        std::task::Poll::Ready(self.chunks.pop_front().map(Ok))
    }
}

impl Drop for FirstChunkThenPending {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::Release);
    }
}

impl futures_core::Stream for ByteChunks {
    type Item = Result<Bytes, StreamBodyError>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        _context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        std::task::Poll::Ready(self.0.pop_front().map(Ok))
    }
}

fn body_error_kind(error: &ApiClientError) -> Option<BodyErrorKind> {
    if let ApiClientError::RequestBody { kind, .. } = error {
        return Some(*kind);
    }
    let mut source = std::error::Error::source(error);
    while let Some(current) = source {
        if let Some(body) = current.downcast_ref::<BodyError>() {
            return Some(body.kind());
        }
        source = current.source();
    }
    None
}

fn native_stream_plan(
    name: &'static str,
    body: StreamBody,
) -> crate::regression_tests::test_api::RequestPlan {
    let mut plan = request_plan(
        name,
        Method::POST,
        "/native-stream",
        ResolvedPolicy::default(),
        None,
    );
    plan.body = PreparedBody::from_stream_body(
        body,
        Some(HeaderValue::from_static("application/octet-stream")),
    );
    plan
}

#[tokio::test]
async fn native_auth_places_header_and_query_material_on_the_wire() -> Result<(), ApiClientError> {
    const HEADER_SENTINEL: &str = "AUTH_HEADER_WIRE_SENTINEL";
    const QUERY_SENTINEL: &str = "C01_QUERY_WIRE_7QX9_SENTINEL";
    let events = Arc::new(Mutex::new(Vec::new()));
    let harness = NativeMockHarness::from_native_replies(
        events.clone(),
        vec![
            NativeMockReply::ok_text(Bytes::from_static(b"header")),
            NativeMockReply::ok_text(Bytes::from_static(b"query"))
                .expect_query_pair("q", QUERY_SENTINEL),
        ],
    );
    let capture = harness.clone();
    let header_client = observation_client(
        ObservationAuthVars::bearer(HEADER_SENTINEL, "wire", events),
        &harness,
    );

    header_client
        .request(TextEndpoint {
            policy: auth_policy(crate::regression_tests::test_api::AuthPlacement::Bearer),
            ..TextEndpoint::default()
        })
        .response()
        .await?;
    let query_client = observation_client(
        ObservationAuthVars::bearer(
            QUERY_SENTINEL,
            "wire-query",
            Arc::new(Mutex::new(Vec::new())),
        ),
        &harness,
    );
    query_client
        .request(TextEndpoint {
            policy: auth_policy(crate::regression_tests::test_api::AuthPlacement::Query("q")),
            ..TextEndpoint::default()
        })
        .response()
        .await?;

    let requests = capture.requests().await;
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0]
            .headers
            .get(http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok()),
        Some("Bearer AUTH_HEADER_WIRE_SENTINEL")
    );
    #[cfg(feature = "dangerous-dev-tools")]
    assert!(requests[1].url.query().is_none());
    assert!(!format!("{:?}", requests[1]).contains(QUERY_SENTINEL));
    assert!(!requests[1].headers.values().any(|value| {
        value
            .to_str()
            .is_ok_and(|value| value.contains(QUERY_SENTINEL))
    }));
    Ok(())
}

#[tokio::test]
async fn native_auth_challenge_refresh_reconstructs_a_fresh_request() -> Result<(), ApiClientError>
{
    let events = Arc::new(Mutex::new(Vec::new()));
    let harness = NativeMockHarness::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::UNAUTHORIZED, "expired"),
            MockResponse::text(StatusCode::OK, "refreshed"),
        ],
    );
    let capture = harness.clone();
    let client = observation_client(
        ObservationAuthVars::bearer("AUTH_REFRESH_SENTINEL", "refresh", events.clone()),
        &harness,
    );

    let response = client
        .request(TextEndpoint {
            policy: auth_policy(crate::regression_tests::test_api::AuthPlacement::Bearer),
            ..TextEndpoint::default()
        })
        .response()
        .await?;

    assert_eq!(response.value(), "refreshed");
    let requests = capture.requests().await;
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].url, requests[1].url);
    let events = events.lock().await;
    assert!(events.iter().any(|event| event == "auth_retry"));
    Ok(())
}

#[tokio::test]
async fn non_rebuildable_challenged_body_returns_original_status_without_recovery() {
    let events = Arc::new(Mutex::new(Vec::new()));
    const BODY_SENTINEL: &[u8] = b"BUFFERED_CHALLENGE_BODY_MUST_NOT_BE_POLLED";
    let gate = NativeReplyGate::new();
    let harness = NativeMockHarness::from_native_replies(
        events.clone(),
        [NativeMockReply::status(StatusCode::UNAUTHORIZED)
            .with_header(
                http::header::CONTENT_TYPE,
                HeaderValue::from_static("text/plain"),
            )
            .with_response_steps([
                super::common::native_mock::ResponseStep::Gate(gate.clone()),
                super::common::native_mock::ResponseStep::Chunk(Bytes::from_static(BODY_SENTINEL)),
            ])],
    );
    let capture = harness.clone();
    let auth = ObservationAuthVars::bearer("AUTH_ONE_SHOT_SENTINEL", "refresh", events.clone());
    #[cfg(feature = "dangerous-dev-tools")]
    let binding_resolutions = auth.binding_resolutions.clone();
    let mut client = observation_client(auth, &harness);
    client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));
    configure_runtime(
        &mut client,
        Some(Arc::new(LimitedRecordingRateLimiter {
            events: events.clone(),
        })),
    );
    let mut plan = native_stream_plan(
        "OneShotChallenge",
        StreamBody::from_bytes(Bytes::from_static(b"one-shot")),
    );
    plan.endpoint.policy = auth_policy(crate::regression_tests::test_api::AuthPlacement::Bearer);

    let error = tokio::time::timeout(
        Duration::from_millis(500),
        client.execute_plan::<concord_core::prelude::Text<String>>(plan),
    )
    .await
    .expect("terminal status classification must not poll the gated response body")
    .expect_err("one-shot body cannot perform authentication recovery");
    gate.release();

    assert!(
        matches!(error, ApiClientError::HttpStatus { status, .. } if status == StatusCode::UNAUTHORIZED)
    );
    assert!(error.rate_limit_response_action().is_some());
    assert!(!format!("{error:?}\n{error}").contains("BUFFERED_CHALLENGE_BODY_MUST_NOT_BE_POLLED"));
    assert_eq!(capture.sent_count().await, 1);
    let events = events.lock().await;
    for event in [
        "auth_prepare",
        "rate_acquire",
        "pre_send",
        "request_head",
        "request_body_complete",
    ] {
        assert_eq!(
            events
                .iter()
                .filter(|observed| observed.as_str() == event)
                .count(),
            1,
            "unexpected `{event}` count in {events:?}"
        );
    }
    assert!(!events.iter().any(|event| event == "provider_refresh"));
    assert!(!events.iter().any(|event| event == "auth_retry"));
    #[cfg(feature = "dangerous-dev-tools")]
    {
        let position = |needle: &str| {
            events
                .iter()
                .position(|event| event == needle)
                .unwrap_or_else(|| panic!("missing `{needle}` in {events:?}"))
        };
        assert!(
            position("hook_status:401 Unauthorized") < position("rate_response")
                && position("rate_response") < position("auth_classification:401 Unauthorized")
                && position("auth_classification:401 Unauthorized")
                    < position("auth_response_released")
                && position("auth_response_released")
                    < position("auth_invalidation:identity_match=true:applied=true"),
            "{events:?}"
        );
        assert_eq!(binding_resolutions.load(Ordering::SeqCst), 3);
        assert!(
            !events
                .iter()
                .any(|event| event.starts_with("unrelated_auth"))
        );
    }
}

#[tokio::test]
async fn non_rebuildable_challenged_stream_returns_status_after_invalidation() {
    let events = Arc::new(Mutex::new(Vec::new()));
    const BODY_SENTINEL: &[u8] = b"STREAM_CHALLENGE_BODY_MUST_NOT_BE_POLLED";
    let gate = NativeReplyGate::new();
    let harness = NativeMockHarness::from_native_replies(
        events.clone(),
        [NativeMockReply::status(StatusCode::UNAUTHORIZED)
            .with_header(
                http::header::CONTENT_TYPE,
                HeaderValue::from_static("application/octet-stream"),
            )
            .with_response_steps([
                super::common::native_mock::ResponseStep::Gate(gate.clone()),
                super::common::native_mock::ResponseStep::Chunk(Bytes::from_static(BODY_SENTINEL)),
            ])],
    );
    let capture = harness.clone();
    let auth = ObservationAuthVars::bearer("AUTH_STREAM_ONE_SHOT", "refresh", events.clone());
    #[cfg(feature = "dangerous-dev-tools")]
    let binding_resolutions = auth.binding_resolutions.clone();
    let mut client = observation_client(auth, &harness);
    client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));
    configure_runtime(
        &mut client,
        Some(Arc::new(RecordingRateLimiter::new(events.clone()))),
    );
    let mut plan = native_stream_plan(
        "OneShotStreamingChallenge",
        StreamBody::from_bytes(Bytes::from_static(b"one-shot-stream")),
    );
    plan.endpoint.policy = auth_policy(crate::regression_tests::test_api::AuthPlacement::Bearer);
    plan.endpoint.response.accept = Some(HeaderValue::from_static("application/octet-stream"));
    plan.endpoint.response.format = crate::regression_tests::test_api::Format::Binary;

    let error = tokio::time::timeout(
        Duration::from_millis(500),
        client.request(OneShotStreamingChallenge { plan }).execute(),
    )
    .await
    .expect("terminal status classification must not poll the gated response body")
    .expect_err("the original streaming 401 remains a terminal status error");
    gate.release();

    assert!(
        matches!(error, ApiClientError::HttpStatus { status, .. } if status == StatusCode::UNAUTHORIZED)
    );
    assert!(!format!("{error:?}\n{error}").contains("STREAM_CHALLENGE_BODY_MUST_NOT_BE_POLLED"));
    assert_eq!(capture.sent_count().await, 1);
    let events = events.lock().await;
    for event in [
        "auth_prepare",
        "rate_acquire",
        "pre_send",
        "request_head",
        "request_body_complete",
    ] {
        assert_eq!(
            events
                .iter()
                .filter(|observed| observed.as_str() == event)
                .count(),
            1,
            "unexpected `{event}` count in {events:?}"
        );
    }
    assert!(!events.iter().any(|event| event == "provider_refresh"));
    assert!(!events.iter().any(|event| event == "auth_retry"));
    #[cfg(feature = "dangerous-dev-tools")]
    {
        let position = |needle: &str| {
            events
                .iter()
                .position(|event| event == needle)
                .unwrap_or_else(|| panic!("missing `{needle}` in {events:?}"))
        };
        assert!(
            position("hook_status:401 Unauthorized") < position("rate_response")
                && position("rate_response") < position("auth_classification:401 Unauthorized")
                && position("auth_classification:401 Unauthorized")
                    < position("auth_response_released")
                && position("auth_response_released")
                    < position("auth_invalidation:identity_match=true:applied=true"),
            "{events:?}"
        );
        assert_eq!(binding_resolutions.load(Ordering::SeqCst), 3);
        assert!(
            !events
                .iter()
                .any(|event| event.starts_with("unrelated_auth"))
        );
    }
}

#[tokio::test]

async fn streaming_status_non_auth_server_error_remains_http_status() {
    const BODY_SENTINEL: &[u8] = b"STREAM_500_BODY_MUST_NOT_BE_POLLED";
    let events = Arc::new(Mutex::new(Vec::new()));
    let gate = NativeReplyGate::new();
    let harness = NativeMockHarness::from_native_replies(
        events.clone(),
        [NativeMockReply::status(StatusCode::INTERNAL_SERVER_ERROR)
            .with_header(
                http::header::CONTENT_TYPE,
                HeaderValue::from_static("application/octet-stream"),
            )
            .with_response_steps([
                super::common::native_mock::ResponseStep::Gate(gate.clone()),
                super::common::native_mock::ResponseStep::Chunk(Bytes::from_static(BODY_SENTINEL)),
            ])],
    );
    let capture = harness.clone();
    let mut client = observation_client(
        ObservationAuthVars::bearer("unused", "phase", events.clone()),
        &harness,
    );
    client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));
    configure_runtime(
        &mut client,
        Some(Arc::new(LimitedRecordingRateLimiter {
            events: events.clone(),
        })),
    );
    let mut plan = native_stream_plan(
        "StreamingServerError",
        StreamBody::from_bytes(Bytes::from_static(b"one-shot-stream-500")),
    );
    plan.endpoint.response.accept = Some(HeaderValue::from_static("application/octet-stream"));
    plan.endpoint.response.format = crate::regression_tests::test_api::Format::Binary;

    let error = tokio::time::timeout(
        Duration::from_millis(500),
        client.request(OneShotStreamingChallenge { plan }).execute(),
    )
    .await
    .expect("terminal 500 classification must not poll the gated response body")
    .expect_err("ordinary streaming 500 remains a terminal status error");
    gate.release();

    assert!(
        matches!(error, ApiClientError::HttpStatus { status, .. } if status == StatusCode::INTERNAL_SERVER_ERROR)
    );
    assert!(
        error
            .rate_limit_response_action()
            .is_some_and(RateLimitResponseAction::is_limited)
    );
    assert!(!format!("{error:?}\n{error}").contains("STREAM_500_BODY_MUST_NOT_BE_POLLED"));
    assert_eq!(capture.sent_count().await, 1);
    let events = events.lock().await;
    for event in [
        "rate_acquire",
        "pre_send",
        "request_head",
        "request_body_complete",
        "hook_status:500 Internal Server Error",
        "rate_response",
    ] {
        assert_eq!(
            events
                .iter()
                .filter(|observed| observed.as_str() == event)
                .count(),
            1,
            "unexpected `{event}` count in {events:?}"
        );
    }
}

#[tokio::test]
async fn streaming_status_success_returns_stream_response() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut success = MockResponse::text(StatusCode::OK, "stream-success");
    success.headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    let harness = NativeMockHarness::new(events.clone(), vec![success]);
    let capture = harness.clone();
    let mut client = observation_client(
        ObservationAuthVars::bearer("unused", "phase", events.clone()),
        &harness,
    );
    client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));
    configure_runtime(
        &mut client,
        Some(Arc::new(RecordingRateLimiter::new(events.clone()))),
    );
    let mut plan = native_stream_plan(
        "StreamingSuccess",
        StreamBody::from_bytes(Bytes::from_static(b"one-shot-stream-success")),
    );
    plan.endpoint.response.accept = Some(HeaderValue::from_static("application/octet-stream"));
    plan.endpoint.response.format = crate::regression_tests::test_api::Format::Binary;

    let mut response = client
        .request(OneShotStreamingChallenge { plan })
        .execute()
        .await?;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.next_chunk().await?.as_deref(),
        Some(b"stream-success".as_slice())
    );
    assert_eq!(capture.sent_count().await, 1);
    let events = events.lock().await;
    assert_eq!(
        events.iter().filter(|event| *event == "pre_send").count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| *event == "rate_acquire")
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| *event == "hook_status:200 OK")
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| *event == "rate_response")
            .count(),
        1
    );
    Ok(())
}

#[cfg(feature = "dangerous-dev-tools")]
#[tokio::test]
async fn multiple_auth_bindings_observe_only_the_retained_rejection_action() {
    let acquired = Arc::new(AtomicUsize::new(0));
    let invalidated = Arc::new(AtomicUsize::new(0));
    let unrelated_acquired = Arc::new(AtomicUsize::new(0));
    let unrelated_invalidated = Arc::new(AtomicUsize::new(0));
    let state = ProviderAuthState {
        challenged: Arc::new(CredentialProviderState::new(CountingCredentialProvider {
            id: CredentialId::new("test", "token"),
            acquired: acquired.clone(),
            refreshed: Arc::new(AtomicUsize::new(0)),
            invalidated: invalidated.clone(),
        })),
        unrelated: Arc::new(CredentialProviderState::new(CountingCredentialProvider {
            id: CredentialId::new("test", "unrelated"),
            acquired: unrelated_acquired.clone(),
            refreshed: Arc::new(AtomicUsize::new(0)),
            invalidated: unrelated_invalidated.clone(),
        })),
        binding_resolutions: Arc::new(AtomicUsize::new(0)),
    };
    let challenged_events = Arc::new(StdMutex::new(Vec::new()));
    let observed = challenged_events.clone();
    concord_core::__development::observe_credential_provider_state(
        state.challenged.as_ref(),
        Arc::new(move |event| {
            observed
                .lock()
                .expect("challenged lifecycle event lock")
                .push(event);
        }),
    );
    let unrelated_events = Arc::new(StdMutex::new(Vec::new()));
    let observed = unrelated_events.clone();
    concord_core::__development::observe_credential_provider_state(
        state.unrelated.as_ref(),
        Arc::new(move |event| {
            observed
                .lock()
                .expect("unrelated lifecycle event lock")
                .push(event);
        }),
    );

    let wire_events = Arc::new(Mutex::new(Vec::new()));
    let harness = NativeMockHarness::new(
        wire_events,
        vec![
            MockResponse::text(StatusCode::UNAUTHORIZED, "challenge"),
            MockResponse::text(StatusCode::OK, "recovered"),
        ],
    );
    let capture = harness.clone();
    let client = ApiClient::<ProviderBindingCx>::with_safe_reqwest_builder(
        (),
        ProviderAuthVars {
            state: state.clone(),
        },
        |builder| harness.configure_reqwest(builder),
    )
    .expect("multi-binding native mock client");
    let mut policy = auth_policy(AuthPlacement::Bearer);
    policy.auth.requirements.push(AuthRequirement {
        credential: CredentialRef {
            id: CredentialId::new("test", "unrelated"),
        },
        placement: AuthPlacement::Query("unrelated_api_key"),
        usage_id: AuthUsageId::new("test-unrelated"),
        step_id: Some("unrelated"),
        provenance: AuthProvenance::new("test"),
        challenge: Default::default(),
    });

    let response = client
        .execute_plan::<concord_core::prelude::Text<String>>(request_plan(
            "MultiBindingChallenge",
            Method::GET,
            "/multi-binding",
            policy,
            None,
        ))
        .await
        .expect("the retained refresh action recovers once");

    assert_eq!(response.value(), "recovered");
    assert_eq!(capture.sent_count().await, 2);
    assert_eq!(acquired.load(Ordering::SeqCst), 2);
    assert_eq!(invalidated.load(Ordering::SeqCst), 1);
    assert_eq!(unrelated_acquired.load(Ordering::SeqCst), 1);
    assert_eq!(unrelated_invalidated.load(Ordering::SeqCst), 0);
    assert_eq!(state.binding_resolutions.load(Ordering::SeqCst), 7);

    let challenged_events = challenged_events
        .lock()
        .expect("challenged lifecycle event lock");
    assert_eq!(challenged_events.len(), 3, "{challenged_events:?}");
    assert!(matches!(
        challenged_events[0],
        concord_core::__development::CredentialLifecycleEvent::ChallengeClassified {
            status: StatusCode::UNAUTHORIZED
        }
    ));
    assert!(matches!(
        challenged_events[1],
        concord_core::__development::CredentialLifecycleEvent::ResponseReleased
    ));
    match &challenged_events[2] {
        concord_core::__development::CredentialLifecycleEvent::GenerationInvalidated {
            requested,
            current,
            applied,
        } => {
            assert_eq!(requested, current);
            assert!(*applied);
        }
        other => panic!("expected opaque generation invalidation, got {other:?}"),
    }
    assert!(
        unrelated_events
            .lock()
            .expect("unrelated lifecycle event lock")
            .is_empty(),
        "discarded rejection action must not emit lifecycle observations"
    );
}

#[tokio::test]
async fn non_rebuildable_challenged_provider_generation_is_not_reused() {
    let acquired = Arc::new(AtomicUsize::new(0));
    let refreshed = Arc::new(AtomicUsize::new(0));
    let invalidated = Arc::new(AtomicUsize::new(0));
    let unrelated_acquired = Arc::new(AtomicUsize::new(0));
    let unrelated_refreshed = Arc::new(AtomicUsize::new(0));
    let unrelated_invalidated = Arc::new(AtomicUsize::new(0));
    let state = ProviderAuthState {
        challenged: Arc::new(CredentialProviderState::new(CountingCredentialProvider {
            id: CredentialId::new("test", "token"),
            acquired: acquired.clone(),
            refreshed: refreshed.clone(),
            invalidated: invalidated.clone(),
        })),
        unrelated: Arc::new(CredentialProviderState::new(CountingCredentialProvider {
            id: CredentialId::new("test", "unrelated"),
            acquired: unrelated_acquired.clone(),
            refreshed: unrelated_refreshed.clone(),
            invalidated: unrelated_invalidated.clone(),
        })),
        binding_resolutions: Arc::new(AtomicUsize::new(0)),
    };
    state
        .unrelated
        .set_manual(ApiKey::new("unrelated-generation"))
        .await
        .expect("install unrelated generation");
    assert!(state.unrelated.has_value().await);
    #[cfg(feature = "dangerous-dev-tools")]
    let unrelated_before =
        concord_core::__development::credential_generation_snapshot(state.unrelated.as_ref())
            .await
            .expect("unrelated generation is cached");

    let events = Arc::new(Mutex::new(Vec::new()));
    let harness = NativeMockHarness::new(
        events,
        vec![
            MockResponse::text(StatusCode::UNAUTHORIZED, "expired"),
            MockResponse::text(StatusCode::OK, "new-generation"),
        ],
    );
    let capture = harness.clone();
    let client = ApiClient::<ProviderBindingCx>::with_safe_reqwest_builder(
        (),
        ProviderAuthVars {
            state: state.clone(),
        },
        |builder| harness.configure_reqwest(builder),
    )
    .expect("provider-binding native mock client");
    let mut challenged = native_stream_plan(
        "ProviderOneShotChallenge",
        StreamBody::from_bytes(Bytes::from_static(b"one-shot-provider")),
    );
    challenged.endpoint.policy =
        auth_policy(crate::regression_tests::test_api::AuthPlacement::Bearer);

    let error = client
        .execute_plan::<concord_core::prelude::Text<String>>(challenged)
        .await
        .expect_err("the challenged one-shot request remains terminal");
    assert!(
        matches!(error, ApiClientError::HttpStatus { status, .. } if status == StatusCode::UNAUTHORIZED)
    );
    assert_eq!(capture.sent_count().await, 1);
    assert_eq!(acquired.load(Ordering::SeqCst), 1);
    assert_eq!(refreshed.load(Ordering::SeqCst), 0);
    assert_eq!(invalidated.load(Ordering::SeqCst), 0);
    assert!(!state.challenged.has_value().await);
    assert!(state.unrelated.has_value().await);
    #[cfg(feature = "dangerous-dev-tools")]
    assert_eq!(
        concord_core::__development::credential_generation_snapshot(state.unrelated.as_ref())
            .await
            .expect("unrelated generation remains cached"),
        unrelated_before
    );
    assert_eq!(unrelated_acquired.load(Ordering::SeqCst), 0);
    assert_eq!(unrelated_refreshed.load(Ordering::SeqCst), 0);
    assert_eq!(unrelated_invalidated.load(Ordering::SeqCst), 0);

    let response = client
        .execute_plan::<concord_core::prelude::Text<String>>(request_plan(
            "ProviderNextCall",
            Method::GET,
            "/provider-next-call",
            auth_policy(crate::regression_tests::test_api::AuthPlacement::Bearer),
            None,
        ))
        .await
        .expect("a later top-level call reacquires after local invalidation");
    assert_eq!(response.value(), "new-generation");

    assert_eq!(capture.sent_count().await, 2);
    assert_eq!(acquired.load(Ordering::SeqCst), 2);
    assert_eq!(refreshed.load(Ordering::SeqCst), 0);
    assert_eq!(invalidated.load(Ordering::SeqCst), 0);
    let requests = capture.requests().await;
    assert_eq!(
        requests[0]
            .headers
            .get(http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok()),
        Some("Bearer provider-token-1")
    );
    assert_eq!(
        requests[1]
            .headers
            .get(http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok()),
        Some("Bearer provider-token-2")
    );
}

#[cfg(feature = "multipart")]
#[tokio::test]
async fn reusable_multipart_auth_recovery_builds_a_fresh_boundary() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let harness = NativeMockHarness::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::UNAUTHORIZED, "expired"),
            MockResponse::text(StatusCode::OK, "refreshed"),
        ],
    );
    let capture = harness.clone();
    let client = observation_client(
        ObservationAuthVars::bearer("AUTH_MULTIPART_SENTINEL", "refresh", events),
        &harness,
    );
    let mut plan = request_plan(
        "MultipartChallenge",
        Method::POST,
        "/multipart-challenge",
        auth_policy(crate::regression_tests::test_api::AuthPlacement::Bearer),
        None,
    );
    plan.body = MultipartRequest::prepare(
        MultipartBody::new().bytes("payload", Bytes::from_static(b"reusable")),
        ErrorContext {
            endpoint: "MultipartChallenge",
            method: Method::POST,
        },
    )
    .expect("multipart request recipe")
    .body;

    let response = client
        .execute_plan::<concord_core::prelude::Text<String>>(plan)
        .await?;

    assert_eq!(response.value(), "refreshed");
    let requests = capture.requests().await;
    assert_eq!(requests.len(), 2);
    let first = requests[0]
        .headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .expect("first multipart content type");
    let second = requests[1]
        .headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .expect("second multipart content type");
    assert_ne!(
        first, second,
        "authentication recovery must rematerialize multipart"
    );
    Ok(())
}

#[tokio::test]
async fn managed_native_executor_reaches_loopback_and_processes_native_response()
-> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let harness = NativeMockHarness::new(
        events,
        vec![MockResponse::text(StatusCode::OK, "native-response")],
    );
    let capture = harness.clone();
    let client = client(TestAuthVars::default(), harness);

    let response = client.request(TextEndpoint::default()).response().await?;

    assert_eq!(response.value(), "native-response");
    assert_eq!(response.url().as_str(), "http://example.com/text");
    let requests = capture.requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].url.path(), "/text");
    assert!(requests[0].url.query().is_none());
    #[cfg(feature = "dangerous-dev-tools")]
    assert_eq!(requests[0].meta.endpoint.as_deref(), Some("Text"));
    Ok(())
}

#[tokio::test]
async fn native_timeout_is_propagated_to_the_execution_and_cancels_it() {
    let harness = NativeMockHarness::from_native_replies(
        Arc::new(Mutex::new(Vec::new())),
        [NativeMockReply::ok_text(Bytes::from_static(b"late"))
            .with_delay(Duration::from_millis(200))],
    );
    let capture = harness.clone();
    let client = client(TestAuthVars::default(), harness);
    let endpoint = TextEndpoint {
        policy: ResolvedPolicy {
            timeout: Some(Duration::from_millis(20)),
            ..ResolvedPolicy::default()
        },
        ..TextEndpoint::default()
    };

    let error = client
        .request(endpoint)
        .response()
        .await
        .expect_err("delayed response must time out");

    assert_eq!(error.category(), ErrorCategory::Timeout);
    let requests = capture.requests().await;
    assert_eq!(requests.len(), 1);
    #[cfg(feature = "dangerous-dev-tools")]
    assert_eq!(requests[0].timeout, Some(Duration::from_millis(20)));
}

#[tokio::test]
async fn dropping_native_execution_cancels_a_gated_response() {
    let gate = NativeReplyGate::new();
    let harness = NativeMockHarness::from_native_replies(
        Arc::new(Mutex::new(Vec::new())),
        [NativeMockReply::disconnect_after_request().with_gate(gate.clone())],
    );
    let capture = harness.clone();
    let client = client(TestAuthVars::default(), harness);
    let task =
        tokio::spawn(async move { client.request(TextEndpoint::default()).response().await });

    tokio::task::spawn_blocking({
        let gate = gate.clone();
        move || gate.wait_until_entered(Duration::from_secs(1))
    })
    .await
    .expect("gate waiter");
    task.abort();
    assert!(
        task.await
            .expect_err("task must be cancelled")
            .is_cancelled()
    );
    gate.release();
    assert_eq!(capture.sent_count().await, 1);
}

#[tokio::test]
async fn partial_native_upload_cancellation_separates_head_from_incomplete_body_completion() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let head_gate = Arc::new(UploadHeadGate::default());
    let harness = NativeMockHarness::from_native_replies_with_head_action(
        events.clone(),
        [NativeMockReply::disconnect_after_request().expect_request_body_failure()],
        {
            let head_gate = head_gate.clone();
            move || head_gate.release()
        },
    );
    let capture = harness.clone();
    let client = client(TestAuthVars::default(), harness);
    let dropped = Arc::new(AtomicBool::new(false));
    let stream = FirstChunkThenPending {
        first: Some(Bytes::from_static(b"part")),
        dropped: dropped.clone(),
        head_gate,
        events: events.clone(),
    };
    let body =
        StreamBody::from_byte_stream(stream).with_size_hint(http_body::SizeHint::with_exact(8));
    let task = tokio::spawn(async move {
        client
            .execute_plan::<concord_core::prelude::Text<String>>(native_stream_plan(
                "CancelledPartialUpload",
                body,
            ))
            .await
    });

    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if events
                .lock()
                .await
                .iter()
                .any(|event| event == "request_body_poll")
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("request head followed by body polling");
    let phase_snapshot = events.lock().await.clone();
    assert_eq!(
        phase_snapshot,
        ["request_head", "request_body_poll"],
        "the upload body must remain gated until the native request head arrives"
    );

    task.abort();
    assert!(
        task.await
            .expect_err("upload task cancelled")
            .is_cancelled()
    );
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if events
                .lock()
                .await
                .iter()
                .any(|event| event == "request_body_complete")
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("incomplete body completion event");

    assert!(dropped.load(Ordering::Acquire));
    capture.wait_for_sends(1).await;
    let requests = capture.requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].body.as_bytes().map(Bytes::as_ref),
        Some(b"part".as_slice())
    );
}

#[tokio::test]
async fn request_body_limit_known_oversize_is_terminal_and_hook_aligned() {
    const SENTINEL: &[u8] = b"BODY_LIMIT_SENTINEL";
    let events = Arc::new(Mutex::new(Vec::new()));
    let harness = NativeMockHarness::from_native_replies(events.clone(), std::iter::empty());
    let capture = harness.clone();
    let mut client = client(TestAuthVars::default(), harness);
    client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));
    client.configure(|config| {
        config.max_request_body_bytes(4);
    });
    let mut plan = request_plan(
        "KnownOversize",
        Method::POST,
        "/oversize",
        ResolvedPolicy::default(),
        None,
    );
    plan.body = PreparedBody::reusable_bytes(
        Bytes::from_static(SENTINEL),
        Some(HeaderValue::from_static("application/octet-stream")),
    );

    let error = client
        .execute_plan::<concord_core::prelude::Text<String>>(plan)
        .await
        .expect_err("known oversize body must be rejected");

    assert!(matches!(
        &error,
        ApiClientError::RequestBodyLimitExceeded {
            limit: 4,
            actual,
            ..
        } if *actual == SENTINEL.len() as u64
    ));
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    crate::support::assert_error_chain_does_not_contain_any(&error, &["BODY_LIMIT_SENTINEL"]);
    let events = events.lock().await.clone();
    assert!(
        events
            .iter()
            .any(|event| event.starts_with("request_error:RequestBody:"))
    );
    assert!(!events.iter().any(|event| event == "pre_send"));
    assert!(!events.iter().any(|event| event.starts_with("hook_status:")));
    assert_eq!(capture.sent_count().await, 0);
}

#[tokio::test]
async fn exact_length_underflow_and_overflow_are_structural_on_the_wire() {
    for (name, chunks, expected_kind) in [
        (
            "ExactUnderflow",
            vec![Bytes::from_static(b"abc")],
            BodyErrorKind::ExactLengthUnderflow,
        ),
        (
            "ExactOverflow",
            vec![Bytes::from_static(b"abcdef")],
            BodyErrorKind::ExactLengthOverflow,
        ),
    ] {
        let head_gate = Arc::new(UploadHeadGate::default());
        let events = Arc::new(Mutex::new(Vec::new()));
        let harness = NativeMockHarness::from_native_replies_with_head_action(
            events.clone(),
            [NativeMockReply::disconnect_after_request().expect_request_body_failure()],
            {
                let head_gate = head_gate.clone();
                move || head_gate.release()
            },
        );
        let capture = harness.clone();
        let mut client = client(TestAuthVars::default(), harness);
        client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));
        let stream = HeadGatedChunks::new(chunks, head_gate);
        let body =
            StreamBody::from_byte_stream(stream).with_size_hint(http_body::SizeHint::with_exact(5));

        let error = client
            .execute_plan::<concord_core::prelude::Text<String>>(native_stream_plan(name, body))
            .await
            .expect_err("dishonest exact stream must fail");

        assert_eq!(error.category(), ErrorCategory::RequestBody);
        assert_eq!(body_error_kind(&error), Some(expected_kind));
        let opaque = std::error::Error::source(&error)
            .and_then(|source| source.downcast_ref::<concord_core::prelude::RequestErrorSource>())
            .expect("request body error retains its opaque source");
        assert_eq!(opaque.body_kind(), Some(expected_kind));
        assert!(std::error::Error::source(opaque).is_some());
        assert!(
            events
                .lock()
                .await
                .iter()
                .any(|event| event.starts_with("request_error:RequestBody:")),
            "request-error hook must match the exact-length terminal category"
        );
        drop(client);
        capture.wait_for_sends(1).await;
        let requests = capture.requests().await;
        assert_eq!(requests.len(), 1, "exact-length failure wire request count");
        assert!(
            requests[0]
                .body
                .as_bytes()
                .map_or(requests[0].body.is_empty(), |body| body.len() <= 5)
        );
    }
}

#[cfg(feature = "dangerous-dev-tools")]
#[tokio::test]
async fn streaming_limit_stops_excess_before_it_reaches_loopback() {
    let head_gate = Arc::new(UploadHeadGate::default());
    let harness = NativeMockHarness::from_native_replies_with_head_action(
        Arc::new(Mutex::new(Vec::new())),
        [NativeMockReply::disconnect_after_request().expect_request_body_failure()],
        {
            let head_gate = head_gate.clone();
            move || head_gate.release()
        },
    );
    let capture = harness.clone();
    let mut client = client(TestAuthVars::default(), harness);
    client.configure(|config| {
        config.max_request_body_bytes(5);
    });
    let stream = HeadGatedChunks::new(
        [Bytes::from_static(b"abcd"), Bytes::from_static(b"efgh")],
        head_gate,
    );
    let body = StreamBody::from_byte_stream(stream).with_size_hint(http_body::SizeHint::new());

    let error = client
        .execute_plan::<concord_core::prelude::Text<String>>(native_stream_plan(
            "NativeStreamLimit",
            body,
        ))
        .await
        .expect_err("stream must exceed the request limit");

    assert!(matches!(
        error,
        ApiClientError::RequestBodyLimitExceeded {
            limit: 5,
            actual: 8,
            ..
        }
    ));
    drop(client);
    capture.wait_for_sends(1).await;
    let requests = capture.requests().await;
    assert_eq!(
        requests.len(),
        1,
        "request-limit failure wire request count"
    );
    assert!(
        requests[0]
            .body
            .as_bytes()
            .map_or(requests[0].body.is_empty(), |body| body.len() <= 5)
    );
}

#[cfg(feature = "multipart")]
#[tokio::test]
async fn multipart_aggregate_limit_is_enforced_during_native_framing() {
    let head_gate = Arc::new(UploadHeadGate::default());
    let harness = NativeMockHarness::from_native_replies_with_head_action(
        Arc::new(Mutex::new(Vec::new())),
        [NativeMockReply::disconnect_after_request().expect_request_body_failure()],
        {
            let head_gate = head_gate.clone();
            move || head_gate.release()
        },
    );
    let capture = harness.clone();
    let mut client = client(TestAuthVars::default(), harness);
    client.configure(|config| {
        config.max_request_body_bytes(1024);
    });
    let mut plan = request_plan(
        "MultipartAggregateLimit",
        Method::POST,
        "/multipart-limit",
        ResolvedPolicy::default(),
        None,
    );
    plan.body = MultipartRequest::prepare(
        MultipartBody::new().stream(
            "upload",
            StreamBody::from_byte_stream(HeadGatedChunks::new(
                [Bytes::from(vec![b'a'; 700]), Bytes::from(vec![b'b'; 700])],
                head_gate,
            )),
        ),
        ErrorContext {
            endpoint: "MultipartAggregateLimit",
            method: Method::POST,
        },
    )
    .expect("multipart recipe")
    .body;

    let error = client
        .execute_plan::<concord_core::prelude::Text<String>>(plan)
        .await
        .expect_err("multipart framing must count toward the global request limit");

    assert!(matches!(
        error,
        ApiClientError::RequestBodyLimitExceeded {
            limit: 1024,
            actual,
            ..
        } if actual > 1024
    ));
    drop(client);
    capture.wait_for_sends(1).await;
    let requests = capture.requests().await;
    assert_eq!(
        requests.len(),
        1,
        "multipart-limit failure wire request count"
    );
    assert!(
        requests[0]
            .body
            .as_bytes()
            .is_some_and(|body| body.len() <= 1024)
    );
}

#[tokio::test]
async fn advisory_upper_hint_does_not_reject_native_stream_early() -> Result<(), ApiClientError> {
    let harness = NativeMockHarness::new(
        Arc::new(Mutex::new(Vec::new())),
        vec![MockResponse::text(StatusCode::OK, "ok")],
    );
    let capture = harness.clone();
    let mut client = client(TestAuthVars::default(), harness);
    client.configure(|config| {
        config.max_request_body_bytes(4);
    });
    let stream = ByteChunks::new([Bytes::from_static(b"abc")]);
    let mut hint = http_body::SizeHint::new();
    hint.set_upper(100);
    let body = StreamBody::from_byte_stream(stream).with_size_hint(hint);

    let response = client
        .execute_plan::<concord_core::prelude::Text<String>>(native_stream_plan(
            "AdvisoryUpper",
            body,
        ))
        .await?;

    assert_eq!(response.value(), "ok");
    assert_eq!(
        capture.requests().await[0].body.as_bytes(),
        Some(&Bytes::from_static(b"abc"))
    );
    Ok(())
}
