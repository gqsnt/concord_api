use super::common::{
    CapturedTransportRequestSnapshot, GateableBodyTransport, GateableHooks, GateableTransport,
    ItemsEndpoint, MockOutcome, MockResponse, MockTransport, ObservationRuntimeHooks,
    PaginationVariant, PhaseGate, SafeRecordingDebugSink, TestAuthVars, TestCx, TextEndpoint,
    assert_events_do_not_contain, auth_policy, buffered_endpoint_execute, client,
    rate_limit_policy, request_plan, retry_policy,
};
use crate::support::{RedactionSentinels, assert_error_chain_does_not_contain_any};
use bytes::Bytes;
use concord_core::advanced::{
    AuthApplicationRequest, AuthAppliedCredential, AuthError, AuthErrorKind, AuthHttpRequest,
    AuthInternalPolicy, AuthMode, AuthPlacement, AuthProvenance, AuthRequirement,
    AuthRequirementId, AuthUsageId, CredentialId, PostResponseHookContext, PreparedAuthCredential,
    PreparedInternalAuth, RuntimeHooks, Transport, TransportErrorKind, apply_secret_credential,
};
use concord_core::prelude::{ApiClient, ApiClientError, PaginationTermination};
use http::{HeaderMap, Method, StatusCode};
use std::error::Error;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use tokio::sync::Mutex;
use tokio::sync::Notify;

const REDACTION_SENTINELS_PR79: RedactionSentinels = RedactionSentinels::new(
    "RAW_AUTH_SENTINEL_PR79",
    "RESPONSE_BODY_SENTINEL_PR79",
    "RESPONSE_OBSERVER_SENTINEL_PR79",
);

fn body_sentinels() -> [&'static str; 2] {
    REDACTION_SENTINELS_PR79.auth_body()
}

const CANCEL_SENTINELS: RedactionSentinels = RedactionSentinels::new(
    "CANCEL_AUTH_SENTINEL",
    "CANCEL_BODY_SENTINEL",
    "CANCEL_RESPONSE_SENTINEL",
);

const INTERNAL_AUTH_SENTINELS: RedactionSentinels = RedactionSentinels::new(
    "INTERNAL_AUTH_SENTINEL",
    "INTERNAL_AUTH_BODY_SENTINEL",
    "INTERNAL_AUTH_RESPONSE_SENTINEL",
);

trait HeaderLookup {
    fn headers(&self) -> &HeaderMap;
}

impl HeaderLookup for CapturedTransportRequestSnapshot {
    fn headers(&self) -> &HeaderMap {
        &self.headers
    }
}

impl HeaderLookup for super::common::CapturedTransportRequest {
    fn headers(&self) -> &HeaderMap {
        &self.headers
    }
}

fn assert_bearer_auth_header_contains<R: HeaderLookup>(request: &R, sentinel: &str) {
    let header = request
        .headers()
        .get(http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_else(|| panic!("authorization header missing or invalid"));
    assert!(
        header.contains(sentinel),
        "authorization header did not contain the expected sentinel"
    );
}

#[derive(Clone)]
struct PostResponseGateHooks {
    gate: PhaseGate,
}

impl PostResponseGateHooks {
    fn new(gate: PhaseGate) -> Self {
        Self { gate }
    }
}

impl RuntimeHooks for PostResponseGateHooks {
    fn post_response<'a>(
        &'a self,
        _ctx: PostResponseHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        let gate = self.gate.clone();
        Box::pin(async move {
            gate.enter("hook_post_response").await;
        })
    }
}

fn transport_client<T: Transport + Clone>(transport: T) -> ApiClient<TestCx, T> {
    ApiClient::with_transport((), TestAuthVars::default(), transport)
}

fn transport_client_with_auth<T: Transport + Clone>(
    auth: TestAuthVars,
    transport: T,
) -> ApiClient<TestCx, T> {
    ApiClient::with_transport((), auth, transport)
}

#[derive(Clone)]
struct InternalAuthVars {
    entered: Arc<Notify>,
    pending: Arc<Notify>,
    block_once: Arc<AtomicBool>,
    internal_secret: &'static str,
    external_secret: &'static str,
    recurse: bool,
}

#[derive(Clone)]
struct InternalAuthCx;

impl concord_core::prelude::ClientContext for InternalAuthCx {
    type Vars = ();
    type AuthVars = InternalAuthVars;
    type AuthState = ();
    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
    const DOMAIN: &'static str = "example.com";

    fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {}

    fn apply_internal_auth<'a>(
        requirement: &'a AuthRequirementId,
        request: &'a mut AuthApplicationRequest<'_>,
        _vars: &'a Self::Vars,
        auth: &'a Self::AuthVars,
        _auth_state: &'a Self::AuthState,
        executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
    ) -> concord_core::advanced::AuthFuture<'a, Result<PreparedInternalAuth, AuthError>> {
        Box::pin(async move {
            assert_eq!(requirement.name(), "internal");
            if auth.recurse {
                let err = executor
                    .send(AuthHttpRequest {
                        method: Method::GET,
                        url: "https://auth.example.com/internal"
                            .parse()
                            .expect("auth url"),
                        headers: HeaderMap::new(),
                        body: concord_core::advanced::TransportRequestBody::Empty,
                        mode: AuthMode::UseAuth(AuthRequirementId::new("test", "internal")),
                        policy: AuthInternalPolicy::default(),
                    })
                    .await
                    .expect_err("recursive internal auth should fail");
                assert_eq!(err.kind, AuthErrorKind::RecursionDetected);
                assert_error_chain_does_not_contain_any(&err, &INTERNAL_AUTH_SENTINELS.all());
                return Err(err);
            }

            if auth.block_once.swap(false, AtomicOrdering::SeqCst) {
                auth.entered.notify_waiters();
                auth.pending.notified().await;
            }
            let requirement = AuthRequirement {
                credential: concord_core::advanced::CredentialRef {
                    id: CredentialId::new("test", "internal"),
                },
                placement: AuthPlacement::Header("X-Internal-Custom"),
                usage_id: AuthUsageId::new("internal-use"),
                step_id: Some("internal"),
                provenance: AuthProvenance::new("internal"),
                challenge: Default::default(),
            };
            let material = concord_core::prelude::ApiKey::new(auth.internal_secret.to_string());
            let application = apply_secret_credential(request, &requirement, &material)?;
            Ok(PreparedInternalAuth::from_application(application))
        })
    }

    fn prepare_auth_requirement<'a>(
        requirement: &'a AuthRequirement,
        request: &'a mut AuthApplicationRequest<'_>,
        _vars: &'a Self::Vars,
        auth: &'a Self::AuthVars,
        _auth_state: &'a Self::AuthState,
        executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
        _meta: &'a concord_core::advanced::RequestMeta,
    ) -> concord_core::advanced::AuthFuture<'a, Result<PreparedAuthCredential, AuthError>> {
        Box::pin(async move {
            let auth_resp = executor
                .send(AuthHttpRequest {
                    method: Method::GET,
                    url: "https://auth.example.com/internal"
                        .parse()
                        .expect("auth url"),
                    headers: HeaderMap::new(),
                    body: concord_core::advanced::TransportRequestBody::Empty,
                    mode: AuthMode::UseAuth(AuthRequirementId::new("test", "internal")),
                    policy: AuthInternalPolicy::default(),
                })
                .await?;
            assert_eq!(auth_resp.status, StatusCode::OK);

            let material = concord_core::prelude::ApiKey::new(auth.external_secret.to_string());
            let application = apply_secret_credential(request, requirement, &material)?;
            let applied = AuthAppliedCredential {
                credential_id: requirement.credential.id.clone(),
                usage_id: requirement.usage_id.clone(),
                step_id: requirement.step_id,
                generation: Some(1),
                provenance: requirement.provenance.clone(),
            };
            Ok(PreparedAuthCredential::new(applied, application))
        })
    }
}

struct InternalAuthEndpoint;

impl concord_core::prelude::Endpoint<InternalAuthCx> for InternalAuthEndpoint {
    type Response = String;

    buffered_endpoint_execute!(InternalAuthCx, concord_core::prelude::Text<String>);
}

impl concord_core::prelude::ReusableEndpoint<InternalAuthCx> for InternalAuthEndpoint {
    fn plan(
        &self,
        _ctx: &concord_core::internal::ClientPlanContext<'_, InternalAuthCx>,
    ) -> Result<concord_core::internal::RequestPlan, ApiClientError> {
        Ok(request_plan(
            "InternalAuth",
            Method::GET,
            "/protected",
            auth_policy(AuthPlacement::Bearer),
            None,
        ))
    }
}

#[tokio::test]
async fn cancel_during_rate_limit_acquire_does_not_send_transport() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let gate = PhaseGate::new();
    let rate_probe = super::common::DropProbe::new("rate_acquire", events.clone());
    let rate_limiter = Arc::new(
        super::common::CountingRateLimiter::new(events.clone())
            .with_gate(gate.clone())
            .with_drop_probe(rate_probe.clone()),
    );
    let transport = GateableTransport::new(
        gate.clone(),
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, "ok-1"),
            MockResponse::text(StatusCode::OK, "ok-2"),
        ],
    );
    let mut client = transport_client(transport.clone());
    client.configure(|cfg| {
        cfg.rate_limiter(rate_limiter.clone());
    });

    let endpoint = TextEndpoint {
        policy: rate_limit_policy(),
        ..Default::default()
    };
    gate.block("rate_acquire").await;
    let task = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .request(endpoint)
                .execute_decoded_with::<concord_core::prelude::Text<String>>()
                .await
        }
    });

    gate.wait_for("rate_acquire", 1).await;
    task.abort();
    let join = task.await;
    assert!(join.is_err());
    assert_eq!(rate_limiter.acquire_started.load(AtomicOrdering::SeqCst), 1);
    rate_probe.wait_for(1).await;
    assert_eq!(rate_probe.count(), 1);
    gate.release_one("rate_acquire").await;
    let second = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .request(TextEndpoint {
                    policy: rate_limit_policy(),
                    ..Default::default()
                })
                .execute_decoded_with::<concord_core::prelude::Text<String>>()
                .await
        }
    });
    gate.wait_for("rate_acquire", 2).await;
    gate.release_one("rate_acquire").await;

    let second = second
        .await
        .expect("second task should join")
        .expect("second request should complete");
    assert_eq!(second.value(), "ok-1");
    assert_eq!(rate_limiter.acquire_started.load(AtomicOrdering::SeqCst), 2);
    assert_eq!(transport.sent_count().await, 1);
    assert_events_do_not_contain(&events, &body_sentinels()).await;
}

#[tokio::test]
async fn cancel_during_pre_send_hook_does_not_send_transport() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let gate = PhaseGate::new();
    let hook_probe = super::common::DropProbe::new("hook_pre_send", events.clone());
    let hooks = Arc::new(
        GateableHooks::new(gate.clone(), events.clone()).with_drop_probe(hook_probe.clone()),
    );
    let transport = GateableTransport::new(
        gate.clone(),
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, "ok-1"),
            MockResponse::text(StatusCode::OK, "ok-2"),
        ],
    );
    let mut client = transport_client(transport.clone());
    client.set_runtime_hooks(hooks.clone());
    let endpoint = TextEndpoint {
        policy: rate_limit_policy(),
        ..Default::default()
    };

    gate.block("hook_pre_send").await;
    let task = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .request(endpoint)
                .execute_decoded_with::<concord_core::prelude::Text<String>>()
                .await
        }
    });

    gate.wait_for("hook_pre_send", 1).await;
    task.abort();
    let join = task.await;
    assert!(join.is_err());
    hook_probe.wait_for(1).await;
    assert_eq!(hook_probe.count(), 1);
    gate.release_one("hook_pre_send").await;
    let second = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .request(TextEndpoint {
                    policy: rate_limit_policy(),
                    ..Default::default()
                })
                .execute_decoded_with::<concord_core::prelude::Text<String>>()
                .await
        }
    });
    gate.wait_for("hook_pre_send", 2).await;
    gate.release_one("hook_pre_send").await;
    let second = second
        .await
        .expect("second task should join")
        .expect("second request should complete");
    assert_eq!(second.value(), "ok-1");
    assert_eq!(transport.sent_count().await, 1);
    assert_events_do_not_contain(&events, &body_sentinels()).await;
}

#[tokio::test]
async fn cancel_while_transport_is_pending_preserves_request_context_and_redacts_auth_sentinel() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let gate = PhaseGate::new();
    let transport_probe = super::common::DropProbe::new("transport_send", events.clone());
    let transport = GateableTransport::new(
        gate.clone(),
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "ok")],
    )
    .with_drop_probe(transport_probe.clone());
    let client = transport_client_with_auth(
        TestAuthVars {
            token: Some(CANCEL_SENTINELS.auth.to_string()),
            identity: "transport-cancel",
        },
        transport.clone(),
    );
    let endpoint = TextEndpoint {
        policy: auth_policy(AuthPlacement::Bearer),
        ..Default::default()
    };

    gate.block("transport_send").await;
    let task = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .request(endpoint)
                .execute_decoded_with::<concord_core::prelude::Text<String>>()
                .await
        }
    });

    gate.wait_for("transport_send", 1).await;
    task.abort();
    let join = task.await;
    assert!(join.is_err());
    let join_err = join.expect_err("task should report cancellation");
    assert!(join_err.is_cancelled());
    transport_probe.wait_for(1).await;
    gate.release_one("transport_send").await;

    let requests = transport.requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].meta.endpoint, "Text");
    assert_eq!(requests[0].meta.method, Method::GET);
    assert_eq!(requests[0].meta.attempt, 0);
    assert_eq!(requests[0].meta.page_index, 0);
    assert_eq!(requests[0].url.path(), "/text");
    assert_bearer_auth_header_contains(&requests[0], CANCEL_SENTINELS.auth);
    assert_error_chain_does_not_contain_any(&join_err, &[CANCEL_SENTINELS.auth]);
}

#[tokio::test]
async fn cancel_during_internal_auth_does_not_poison_recursion_stack() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, "internal-ok"),
            MockResponse::text(StatusCode::OK, "protected-ok"),
        ],
    );
    let client = ApiClient::<InternalAuthCx, _>::with_transport(
        (),
        InternalAuthVars {
            entered: Arc::new(Notify::new()),
            pending: Arc::new(Notify::new()),
            block_once: Arc::new(AtomicBool::new(true)),
            internal_secret: INTERNAL_AUTH_SENTINELS.auth,
            external_secret: INTERNAL_AUTH_SENTINELS.response,
            recurse: false,
        },
        transport.clone(),
    );

    let mut first = Box::pin(
        client
            .request(InternalAuthEndpoint)
            .execute_decoded_with::<concord_core::prelude::Text<String>>(),
    );
    let mut entered = Box::pin(client.auth_vars().entered.notified());

    tokio::select! {
        biased;
        _ = entered.as_mut() => {}
        result = first.as_mut() => panic!("internal auth request completed unexpectedly: {result:?}"),
    }
    drop(first);

    assert_eq!(transport.sent_count().await, 0);

    let value = client
        .request(InternalAuthEndpoint)
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await
        .expect("second request should complete after cancellation")
        .into_value();
    assert_eq!(value, "protected-ok");
    assert_eq!(transport.sent_count().await, 2);
    assert_events_do_not_contain(&events, &INTERNAL_AUTH_SENTINELS.all()).await;
}

#[tokio::test]
async fn real_internal_auth_recursion_is_still_rejected() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "unused")],
    );
    let client = ApiClient::<InternalAuthCx, _>::with_transport(
        (),
        InternalAuthVars {
            entered: Arc::new(Notify::new()),
            pending: Arc::new(Notify::new()),
            block_once: Arc::new(AtomicBool::new(false)),
            internal_secret: INTERNAL_AUTH_SENTINELS.auth,
            external_secret: INTERNAL_AUTH_SENTINELS.response,
            recurse: true,
        },
        transport.clone(),
    );

    let err = client
        .request(InternalAuthEndpoint)
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await
        .expect_err("recursive internal auth should fail");

    match &err {
        ApiClientError::Auth { source, .. } => {
            assert_eq!(source.kind, AuthErrorKind::RecursionDetected);
        }
        other => panic!("expected auth recursion error, got {other:?}"),
    }
    assert_error_chain_does_not_contain_any(&err, &INTERNAL_AUTH_SENTINELS.all());
    assert_eq!(transport.sent_count().await, 0);
}

#[tokio::test]
async fn cancel_while_response_body_is_pending_drops_body_stream_and_redacts_sentinels() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let gate = PhaseGate::new();
    let body_probe = super::common::DropProbe::new("body_chunk", events.clone());
    let transport = GateableBodyTransport::new(
        gate.clone(),
        events.clone(),
        vec![Bytes::from_static(CANCEL_SENTINELS.body.as_bytes())],
    )
    .with_drop_probe(body_probe.clone());
    let client = transport_client_with_auth(
        TestAuthVars {
            token: Some(CANCEL_SENTINELS.auth.to_string()),
            identity: "body-cancel",
        },
        transport.clone(),
    );
    let endpoint = TextEndpoint {
        policy: auth_policy(AuthPlacement::Bearer),
        ..Default::default()
    };

    gate.block("body_chunk").await;
    let task = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .request(endpoint)
                .execute_decoded_with::<concord_core::prelude::Text<String>>()
                .await
        }
    });

    gate.wait_for("body_chunk", 1).await;
    task.abort();
    let join = task.await;
    assert!(join.is_err());
    let join_err = join.expect_err("task should report cancellation");
    assert!(join_err.is_cancelled());
    body_probe.wait_for(1).await;
    gate.release_one("body_chunk").await;

    let requests = transport.requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].meta.endpoint, "Text");
    assert_eq!(requests[0].meta.method, Method::GET);
    assert_eq!(requests[0].meta.attempt, 0);
    assert_eq!(requests[0].meta.page_index, 0);
    assert_eq!(requests[0].url.path(), "/text");
    assert_bearer_auth_header_contains(&requests[0], CANCEL_SENTINELS.auth);
    assert_eq!(transport.read_count(), 0);
    assert_error_chain_does_not_contain_any(
        &join_err,
        &[CANCEL_SENTINELS.auth, CANCEL_SENTINELS.body],
    );
}

#[tokio::test]
async fn cancel_before_retry_progression_stops_the_second_attempt() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let gate = PhaseGate::new();
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, CANCEL_SENTINELS.response),
            MockResponse::text(StatusCode::OK, "should-not-send"),
        ],
    );
    let sent_transport = transport.clone();
    let mut client = transport_client_with_auth(
        TestAuthVars {
            token: Some(CANCEL_SENTINELS.auth.to_string()),
            identity: "retry-cancel",
        },
        transport,
    );
    client.set_runtime_hooks(Arc::new(PostResponseGateHooks::new(gate.clone())));

    let mut policy = rate_limit_policy();
    policy.auth = auth_policy(AuthPlacement::Bearer).auth;
    policy.retry = retry_policy(2).retry;

    gate.block("hook_post_response").await;
    let task = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .request(TextEndpoint {
                    policy,
                    ..Default::default()
                })
                .execute_decoded_with::<concord_core::prelude::Text<String>>()
                .await
        }
    });

    gate.wait_for("hook_post_response", 1).await;
    task.abort();
    let join = task.await;
    assert!(join.is_err());
    let join_err = join.expect_err("task should report cancellation");
    assert!(join_err.is_cancelled());
    gate.release_one("hook_post_response").await;

    let requests = sent_transport.requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].meta.endpoint, "Text");
    assert_eq!(requests[0].meta.method, Method::GET);
    assert_eq!(requests[0].meta.attempt, 0);
    assert_eq!(requests[0].meta.page_index, 0);
    assert_eq!(requests[0].url.path(), "/text");
    assert_bearer_auth_header_contains(&requests[0], CANCEL_SENTINELS.auth);
    assert_error_chain_does_not_contain_any(
        &join_err,
        &[CANCEL_SENTINELS.auth, CANCEL_SENTINELS.response],
    );
}

#[tokio::test]
async fn cancel_during_pagination_stops_later_page_requests() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let gate = PhaseGate::new();
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, CANCEL_SENTINELS.response),
            MockResponse::text(StatusCode::OK, "c,d"),
        ],
    );
    let sent_transport = transport.clone();
    let mut client = transport_client_with_auth(
        TestAuthVars {
            token: Some(CANCEL_SENTINELS.auth.to_string()),
            identity: "pagination-cancel",
        },
        transport,
    );
    client.set_runtime_hooks(Arc::new(PostResponseGateHooks::new(gate.clone())));

    let mut policy = rate_limit_policy();
    policy.auth = auth_policy(AuthPlacement::Bearer).auth;

    gate.block("hook_post_response").await;
    let task = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .request(ItemsEndpoint {
                    policy,
                    start: 0,
                    count: 2,
                    pagination: PaginationVariant::OffsetLimit {
                        offset: 0,
                        limit: 2,
                    },
                    ..Default::default()
                })
                .paginate(PaginationTermination::hard_page_cap(100))
                .collect()
                .await
        }
    });

    gate.wait_for("hook_post_response", 1).await;
    task.abort();
    let join = task.await;
    assert!(join.is_err());
    let join_err = join.expect_err("task should report cancellation");
    assert!(join_err.is_cancelled());
    gate.release_one("hook_post_response").await;

    let requests = sent_transport.requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].meta.endpoint, "Items");
    assert_eq!(requests[0].meta.method, Method::GET);
    assert_eq!(requests[0].meta.page_index, 0);
    assert_eq!(requests[0].meta.attempt, 0);
    assert_eq!(requests[0].url.path(), "/items");
    assert_bearer_auth_header_contains(&requests[0], CANCEL_SENTINELS.auth);
    assert_error_chain_does_not_contain_any(
        &join_err,
        &[CANCEL_SENTINELS.auth, CANCEL_SENTINELS.response],
    );
}

#[tokio::test]
async fn transport_timeout_error_is_typed_and_safe() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let rate_limiter = Arc::new(super::common::CountingRateLimiter::new(events.clone()));
    let transport = super::common::MockTransport::with_outcomes(
        events.clone(),
        vec![MockOutcome::TransportError(TransportErrorKind::Timeout)],
    );
    let raw_auth = TestAuthVars {
        token: Some(REDACTION_SENTINELS_PR79.auth.to_string()),
        identity: "transport-timeout",
    };
    let mut client = transport_client_with_auth(raw_auth, transport.clone());
    client.configure(|cfg| {
        cfg.rate_limiter(rate_limiter.clone());
    });
    client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));
    client.set_debug_sink(Arc::new(SafeRecordingDebugSink::new(events.clone())));

    let endpoint = TextEndpoint {
        policy: {
            let mut policy = rate_limit_policy();
            policy.auth = auth_policy(AuthPlacement::Bearer).auth;
            policy
        },
        ..Default::default()
    };

    let err = client
        .request(endpoint)
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await
        .expect_err("transport timeout should surface as a transport error");

    assert!(matches!(err, ApiClientError::Transport { .. }));
    assert!(matches!(
        err.source().and_then(|source| source.downcast_ref::<concord_core::transport::TransportError>()),
        Some(source) if source.kind() == TransportErrorKind::Timeout
    ));
    assert_eq!(transport.sent_count().await, 1);
    assert_eq!(
        rate_limiter.response_observed.load(AtomicOrdering::SeqCst),
        0
    );
    assert_error_chain_does_not_contain_any(&err, &REDACTION_SENTINELS_PR79.auth_body());
    assert_events_do_not_contain(&events, &body_sentinels()).await;
}

#[tokio::test]
async fn execute_raw_cancellation_matches_raw_contract() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let gate = PhaseGate::new();
    let transport_probe = super::common::DropProbe::new("transport_send", events.clone());
    let transport = GateableTransport::new(
        gate.clone(),
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, "raw-1"),
            MockResponse::text(StatusCode::OK, "raw-2"),
        ],
    )
    .with_drop_probe(transport_probe.clone());
    let mut client = transport_client(transport.clone());
    client.configure(|cfg| {
        cfg.rate_limiter(Arc::new(super::common::CountingRateLimiter::new(
            events.clone(),
        )));
    });

    let endpoint = TextEndpoint {
        policy: rate_limit_policy(),
        ..Default::default()
    };
    gate.block("transport_send").await;
    let task = tokio::spawn({
        let client = client.clone();
        async move { client.request(endpoint).execute_raw().await }
    });

    gate.wait_for("transport_send", 1).await;
    task.abort();
    let join = task.await;
    assert!(join.is_err());
    transport_probe.wait_for(1).await;
    assert_eq!(transport_probe.count(), 1);
    gate.release_one("transport_send").await;
    let raw = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .request(TextEndpoint {
                    policy: rate_limit_policy(),
                    ..Default::default()
                })
                .execute_raw()
                .await
        }
    });
    gate.wait_for("transport_send", 2).await;
    gate.release_one("transport_send").await;
    let raw = raw
        .await
        .expect("later raw task should join")
        .expect("later raw request should complete");
    assert_eq!(raw.status, StatusCode::OK);
    assert_eq!(transport.sent_count().await, 2);
    assert_events_do_not_contain(&events, &body_sentinels()).await;
}

#[tokio::test]
async fn execute_raw_cancellation_during_rate_limit_acquire_does_not_send_transport() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let gate = PhaseGate::new();
    let rate_probe = super::common::DropProbe::new("rate_acquire", events.clone());
    let rate_limiter = Arc::new(
        super::common::CountingRateLimiter::new(events.clone())
            .with_gate(gate.clone())
            .with_drop_probe(rate_probe.clone()),
    );
    let transport = GateableTransport::new(
        gate.clone(),
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, "raw-1"),
            MockResponse::text(StatusCode::OK, "raw-2"),
        ],
    );
    let mut client = transport_client(transport.clone());
    client.configure(|cfg| {
        cfg.rate_limiter(rate_limiter.clone());
    });

    let endpoint = TextEndpoint {
        policy: rate_limit_policy(),
        ..Default::default()
    };
    gate.block("rate_acquire").await;
    let task = tokio::spawn({
        let client = client.clone();
        async move { client.request(endpoint).execute_raw().await }
    });

    gate.wait_for("rate_acquire", 1).await;
    task.abort();
    let join = task.await;
    assert!(join.is_err());
    rate_probe.wait_for(1).await;
    assert_eq!(rate_probe.count(), 1);
    assert_eq!(rate_limiter.acquire_started.load(AtomicOrdering::SeqCst), 1);
    assert_eq!(transport.sent_count().await, 0);
    gate.release_one("rate_acquire").await;

    let raw = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .request(TextEndpoint {
                    policy: rate_limit_policy(),
                    ..Default::default()
                })
                .execute_raw()
                .await
        }
    });
    gate.wait_for("rate_acquire", 2).await;
    gate.release_one("rate_acquire").await;
    let raw = raw
        .await
        .expect("later raw task should join")
        .expect("later raw request should complete");
    assert_eq!(raw.status, StatusCode::OK);
    assert_eq!(rate_limiter.acquire_started.load(AtomicOrdering::SeqCst), 2);
    assert_eq!(transport.sent_count().await, 1);
}

#[tokio::test]
async fn cancellation_observer_surfaces_are_body_auth_free() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let gate = PhaseGate::new();
    let raw_auth = TestAuthVars {
        token: Some(REDACTION_SENTINELS_PR79.auth.to_string()),
        identity: "observer",
    };
    let rate_limiter = Arc::new(
        super::common::CountingRateLimiter::new(events.clone())
            .with_gate(gate.clone())
            .with_drop_probe(super::common::DropProbe::new("rate", events.clone())),
    );
    let transport = GateableTransport::new(
        gate.clone(),
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, REDACTION_SENTINELS_PR79.body),
            MockResponse::text(StatusCode::OK, "ok-2"),
        ],
    )
    .with_drop_probe(super::common::DropProbe::new("transport", events.clone()));
    let hooks = Arc::new(
        GateableHooks::new(gate.clone(), events.clone())
            .with_drop_probe(super::common::DropProbe::new("hook", events.clone())),
    );
    let mut client = transport_client_with_auth(raw_auth, transport.clone());
    client.configure(|cfg| {
        cfg.rate_limiter(rate_limiter.clone());
    });
    client.set_runtime_hooks(hooks.clone());

    let mut policy = rate_limit_policy();
    policy.auth = auth_policy(AuthPlacement::Bearer).auth;
    gate.block("rate_acquire").await;
    let task = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .request(TextEndpoint {
                    policy,
                    ..Default::default()
                })
                .execute_decoded_with::<concord_core::prelude::Text<String>>()
                .await
        }
    });
    gate.wait_for("rate_acquire", 1).await;
    task.abort();
    let join = task.await;
    assert!(join.is_err());
    gate.release_one("rate_acquire").await;
    assert_events_do_not_contain(&events, &body_sentinels()).await;
}

#[tokio::test]
async fn transport_timeout_metadata_reaches_transport_and_is_request_scoped()
-> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = super::common::MockTransport::with_outcomes(
        events.clone(),
        vec![
            MockOutcome::Response(MockResponse::text(StatusCode::OK, "one")),
            MockOutcome::Response(MockResponse::text(StatusCode::OK, "two")),
        ],
    );
    let client = client(TestAuthVars::default(), transport.clone());
    let endpoint = TextEndpoint {
        policy: {
            let mut policy = rate_limit_policy();
            policy.timeout = Some(std::time::Duration::from_secs(5));
            policy
        },
        ..Default::default()
    };
    client
        .request(endpoint.clone())
        .timeout(std::time::Duration::from_secs(2))
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await?;
    client
        .request(endpoint)
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await?;
    let requests = transport.requests().await;
    assert_eq!(requests[0].timeout, Some(std::time::Duration::from_secs(2)));
    assert_eq!(requests[1].timeout, Some(std::time::Duration::from_secs(5)));
    Ok(())
}
