use super::common::{
    ObservationAuthCx, ObservationAuthVars, ObservationRuntimeHooks, RecordingRateLimiter,
    auth_policy, execute_buffered, request_plan,
};
use crate::regression_tests::test_api::{AuthPlacement, PreparedBody, ResolvedPolicy};
use crate::transport::TlsCapability;
use bytes::Bytes;
use http::{Method, StatusCode};
use http_body::{Body, Frame, SizeHint};
use http_body_util::Full;
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll};
use tokio::sync::Mutex;

#[derive(Clone)]
struct SchemeChangingPaginatedEndpoint {
    offset: u64,
    limit: u64,
    body_factories: Arc<AtomicUsize>,
}

impl crate::regression_tests::test_api::RegressionEndpoint<ObservationAuthCx>
    for SchemeChangingPaginatedEndpoint
{
    type Response = Vec<String>;

    fn execute<'a>(
        client: &'a crate::client::ApiClient<ObservationAuthCx>,
        plan: crate::regression_tests::test_api::RequestPlan,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<Self::Response, crate::error::ApiClientError>>
                + Send
                + 'a,
        >,
    > {
        execute_buffered::<_, super::common::CommaSeparatedItems>(client, plan)
    }
}

impl crate::regression_tests::test_api::RegressionReusableEndpoint<ObservationAuthCx>
    for SchemeChangingPaginatedEndpoint
{
    fn plan(
        &self,
        _ctx: &crate::regression_tests::test_api::RegressionPlanContext<'_, ObservationAuthCx>,
    ) -> Result<crate::regression_tests::test_api::RequestPlan, crate::error::ApiClientError> {
        let mut plan = request_plan(
            "TlsPagination",
            Method::POST,
            "/pages",
            auth_policy(AuthPlacement::Bearer),
            Some(crate::regression_tests::test_api::PaginationMarker),
        );
        plan.endpoint.route.scheme = if self.offset == 0 {
            http::uri::Scheme::HTTP
        } else {
            http::uri::Scheme::HTTPS
        };
        plan.endpoint
            .policy
            .query
            .push(("offset".to_string(), self.offset.to_string()));
        plan.endpoint
            .policy
            .query
            .push(("limit".to_string(), self.limit.to_string()));
        let calls = self.body_factories.clone();
        plan.body = PreparedBody::factory(SizeHint::new(), None, move || {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(crate::io::AdvancedRequestBody::new(Full::<Bytes>::new(
                Bytes::new(),
            )))
        });
        Ok(plan)
    }
}

impl crate::regression_tests::test_api::RegressionPaginatedEndpoint<ObservationAuthCx>
    for SchemeChangingPaginatedEndpoint
{
    type Pagination = crate::pagination::OffsetLimitPagination;
}

impl crate::pagination::PaginateBinding<crate::pagination::OffsetLimitPagination>
    for SchemeChangingPaginatedEndpoint
{
    fn load_pagination(&self) -> crate::pagination::OffsetLimitPagination {
        crate::pagination::OffsetLimitPagination {
            offset: self.offset,
            limit: self.limit,
        }
    }

    fn store_pagination(&mut self, pagination: &crate::pagination::OffsetLimitPagination) {
        self.offset = pagination.offset;
        self.limit = pagination.limit;
    }
}

fn plan(
    scheme: http::uri::Scheme,
    policy: ResolvedPolicy,
    body: PreparedBody,
) -> crate::regression_tests::test_api::RequestPlan {
    let mut plan = request_plan("TlsPreflight", Method::POST, "/tls", policy, None);
    plan.endpoint.route.scheme = scheme;
    plan.body = body;
    plan
}

fn client(
    executor: crate::__development::DeterministicNativeExecutor,
    auth: ObservationAuthVars,
) -> crate::client::ApiClient<ObservationAuthCx> {
    crate::client::ApiClient::with_safe_reqwest_builder((), auth, |builder| {
        crate::__development::configure_application_executor(builder, executor)
            .expect("application executor configuration")
    })
    .expect("managed deterministic client")
}

async fn execute_text(
    client: &crate::client::ApiClient<ObservationAuthCx>,
    plan: crate::regression_tests::test_api::RequestPlan,
) -> Result<String, crate::error::ApiClientError> {
    execute_buffered::<_, crate::codec::text::Text<String>>(client, plan).await
}

struct PollCountingBody {
    polls: Arc<AtomicUsize>,
}

impl Body for PollCountingBody {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        Poll::Ready(None)
    }
}

#[tokio::test]
async fn tls_capability_application_https_preflight_has_zero_protected_side_effects() {
    const QUERY_SECRET: &str = "TLS_QUERY_SECRET_SENTINEL";
    const FRAGMENT_SECRET: &str = "TLS_FRAGMENT_SECRET_SENTINEL";
    const PROXY_SECRET: &str = "TLS_PROXY_SECRET_SENTINEL";
    const BODY_SECRET: &str = "TLS_BODY_SECRET_SENTINEL";

    let events = Arc::new(Mutex::new(Vec::new()));
    let auth = ObservationAuthVars::bearer_replacing(
        "initial-secret",
        "replacement-secret",
        "refresh",
        events.clone(),
    );
    let binding_resolutions = auth.binding_resolutions.clone();
    let executor = crate::__development::DeterministicNativeExecutor::application();
    executor.script_response(crate::__development::ScriptedNativeResponse::bytes(
        StatusCode::OK,
        Bytes::from_static(b"unused"),
    ));
    let mut client = client(executor.clone(), auth);
    client.set_application_tls_capability_for_test(TlsCapability::Unavailable);
    client.set_rate_limiter(Arc::new(RecordingRateLimiter::new(events.clone())));
    client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));

    let state = client.auth_state();
    let generation_before =
        crate::__development::credential_generation_snapshot(state.secret.as_ref()).await;
    let factory_calls = Arc::new(AtomicUsize::new(0));
    let calls = factory_calls.clone();
    let body = PreparedBody::factory(SizeHint::new(), None, move || {
        calls.fetch_add(1, Ordering::SeqCst);
        Ok(crate::io::AdvancedRequestBody::new(Full::<Bytes>::new(
            Bytes::from_static(BODY_SECRET.as_bytes()),
        )))
    });
    let mut request = plan(
        http::uri::Scheme::HTTPS,
        auth_policy(AuthPlacement::Bearer),
        body,
    );
    request
        .endpoint
        .policy
        .query
        .push(("auth_query".to_string(), QUERY_SECRET.to_string()));

    let error = execute_text(&client, request)
        .await
        .expect_err("missing TLS capability must fail before side effects");
    assert!(matches!(
        error,
        crate::error::ApiClientError::TlsCapabilityUnavailable { .. }
    ));
    assert_eq!(error.category(), crate::error::ErrorCategory::Config);
    assert_eq!(binding_resolutions.load(Ordering::SeqCst), 0);
    assert_eq!(factory_calls.load(Ordering::SeqCst), 0);
    assert!(events.lock().await.is_empty());
    assert!(executor.captures().is_empty());
    assert_eq!(executor.remaining_scripts(), 1);
    assert_eq!(
        crate::__development::credential_generation_snapshot(state.secret.as_ref()).await,
        generation_before
    );

    let diagnostics = format!("{error}\n{error:?}");
    for sentinel in [
        QUERY_SECRET,
        FRAGMENT_SECRET,
        PROXY_SECRET,
        BODY_SECRET,
        "example.com",
        "https://",
        "reqwest",
        "rustls",
        "native-tls",
    ] {
        assert!(
            !diagnostics.contains(sentinel),
            "leaked {sentinel}: {diagnostics}"
        );
    }
    assert!(std::error::Error::source(&error).is_none());
}

#[tokio::test]
async fn structural_auth_collision_precedes_tls_capability_preflight() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let executor = crate::__development::DeterministicNativeExecutor::application();
    executor.script_response(crate::__development::ScriptedNativeResponse::bytes(
        StatusCode::OK,
        Bytes::new(),
    ));
    let mut client = client(
        executor.clone(),
        ObservationAuthVars::bearer("secret", "plain", events.clone()),
    );
    client.set_application_tls_capability_for_test(TlsCapability::Unavailable);
    let mut request = plan(
        http::uri::Scheme::HTTPS,
        auth_policy(AuthPlacement::Query("token")),
        PreparedBody::empty(),
    );
    request
        .endpoint
        .policy
        .query
        .push(("token".to_string(), "public".to_string()));

    let error = execute_text(&client, request)
        .await
        .expect_err("structural collision remains the earlier preflight");
    assert!(matches!(error, crate::error::ApiClientError::Auth { .. }));
    assert!(events.lock().await.is_empty());
    assert!(executor.captures().is_empty());
    assert_eq!(executor.remaining_scripts(), 1);
}

#[tokio::test]
async fn https_preflight_leaves_one_shot_body_unpolled_and_unconsumed() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let executor = crate::__development::DeterministicNativeExecutor::application();
    executor.script_response(crate::__development::ScriptedNativeResponse::bytes(
        StatusCode::OK,
        Bytes::new(),
    ));
    let mut client = client(
        executor.clone(),
        ObservationAuthVars::bearer("secret", "plain", events),
    );
    client.set_application_tls_capability_for_test(TlsCapability::Unavailable);
    let polls = Arc::new(AtomicUsize::new(0));
    let body = PreparedBody::one_shot(
        crate::io::AdvancedRequestBody::new(PollCountingBody {
            polls: polls.clone(),
        }),
        None,
    );

    let error = execute_text(
        &client,
        plan(http::uri::Scheme::HTTPS, Default::default(), body),
    )
    .await
    .expect_err("HTTPS must fail before consuming the one-shot body");
    assert!(matches!(
        error,
        crate::error::ApiClientError::TlsCapabilityUnavailable { .. }
    ));
    assert_eq!(polls.load(Ordering::SeqCst), 0);
    assert!(executor.captures().is_empty());
    assert_eq!(executor.remaining_scripts(), 1);
}

#[tokio::test]
async fn no_tls_http_preserves_auth_limiter_hooks_body_and_deterministic_execution() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let auth = ObservationAuthVars::bearer("secret", "plain", events.clone());
    let executor = crate::__development::DeterministicNativeExecutor::application();
    executor.script_response(
        crate::__development::ScriptedNativeResponse::bytes(
            StatusCode::OK,
            Bytes::from_static(b"http-ok"),
        )
        .with_header(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("text/plain"),
        ),
    );
    let mut client = client(executor.clone(), auth);
    client.set_application_tls_capability_for_test(TlsCapability::Unavailable);
    client.set_rate_limiter(Arc::new(RecordingRateLimiter::new(events.clone())));
    client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));
    let factory_calls = Arc::new(AtomicUsize::new(0));
    let calls = factory_calls.clone();
    let body = PreparedBody::factory(SizeHint::new(), None, move || {
        calls.fetch_add(1, Ordering::SeqCst);
        Ok(crate::io::AdvancedRequestBody::new(Full::<Bytes>::new(
            Bytes::new(),
        )))
    });

    let value = execute_text(
        &client,
        plan(
            http::uri::Scheme::HTTP,
            auth_policy(AuthPlacement::Bearer),
            body,
        ),
    )
    .await
    .expect("HTTP remains available without TLS");
    assert_eq!(value, "http-ok");
    assert_eq!(factory_calls.load(Ordering::SeqCst), 1);
    let observed = events.lock().await.clone();
    assert!(observed.iter().any(|event| event == "auth_prepare"));
    assert!(observed.iter().any(|event| event == "rate_acquire"));
    assert!(observed.iter().any(|event| event == "pre_send"));
    assert!(
        observed
            .iter()
            .any(|event| event.starts_with("hook_status:"))
    );
    assert_eq!(executor.captures().len(), 1);
    assert_eq!(executor.remaining_scripts(), 0);
}

#[tokio::test]
async fn tls_enabled_https_reaches_the_deterministic_native_boundary() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let executor = crate::__development::DeterministicNativeExecutor::application();
    executor.script_response(
        crate::__development::ScriptedNativeResponse::bytes(
            StatusCode::OK,
            Bytes::from_static(b"https-ok"),
        )
        .with_header(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("text/plain"),
        ),
    );
    let mut client = client(
        executor.clone(),
        ObservationAuthVars::bearer("secret", "plain", events),
    );
    client.set_application_tls_capability_for_test(TlsCapability::Available);

    let value = execute_text(
        &client,
        plan(
            http::uri::Scheme::HTTPS,
            Default::default(),
            PreparedBody::empty(),
        ),
    )
    .await
    .expect("available TLS capability permits deterministic HTTPS");
    assert_eq!(value, "https-ok");
    assert_eq!(executor.captures().len(), 1);
    assert_eq!(executor.remaining_scripts(), 0);
}

#[tokio::test]
async fn dynamic_origin_http_then_https_rechecks_each_visible_execution() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let auth = ObservationAuthVars::bearer("secret", "refresh", events.clone());
    let executor = crate::__development::DeterministicNativeExecutor::application();
    for body in [b"first".as_slice(), b"unused".as_slice()] {
        executor.script_response(
            crate::__development::ScriptedNativeResponse::bytes(
                StatusCode::OK,
                Bytes::copy_from_slice(body),
            )
            .with_header(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_static("text/plain"),
            ),
        );
    }
    let mut client = client(executor.clone(), auth);
    client.set_application_tls_capability_for_test(TlsCapability::Unavailable);
    client.set_rate_limiter(Arc::new(RecordingRateLimiter::new(events.clone())));
    client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));

    let first = execute_text(
        &client,
        plan(
            http::uri::Scheme::HTTP,
            auth_policy(AuthPlacement::Bearer),
            PreparedBody::empty(),
        ),
    )
    .await
    .expect("first HTTP origin succeeds");
    assert_eq!(first, "first");
    let before_later = events.lock().await.clone();

    let error = execute_text(
        &client,
        plan(
            http::uri::Scheme::HTTPS,
            auth_policy(AuthPlacement::Bearer),
            PreparedBody::empty(),
        ),
    )
    .await
    .expect_err("later HTTPS origin is checked independently");
    assert!(matches!(
        error,
        crate::error::ApiClientError::TlsCapabilityUnavailable { .. }
    ));
    assert_eq!(*events.lock().await, before_later);
    assert_eq!(executor.captures().len(), 1);
    assert_eq!(executor.remaining_scripts(), 1);
}

#[tokio::test]
async fn pagination_http_then_https_preflights_the_later_page_before_side_effects() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let auth = ObservationAuthVars::bearer("secret", "plain", events.clone());
    let executor = crate::__development::DeterministicNativeExecutor::application();
    for body in [b"a,b".as_slice(), b"unused".as_slice()] {
        executor.script_response(
            crate::__development::ScriptedNativeResponse::bytes(
                StatusCode::OK,
                Bytes::copy_from_slice(body),
            )
            .with_header(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_static("text/plain"),
            ),
        );
    }
    let mut inner = client(executor.clone(), auth);
    inner.set_application_tls_capability_for_test(TlsCapability::Unavailable);
    inner.set_rate_limiter(Arc::new(RecordingRateLimiter::new(events.clone())));
    inner.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));
    let client = super::common::RegressionClient::from_inner(inner, None);
    let body_factories = Arc::new(AtomicUsize::new(0));
    let endpoint = SchemeChangingPaginatedEndpoint {
        offset: 0,
        limit: 2,
        body_factories: body_factories.clone(),
    };

    let error = client
        .request(endpoint)
        .paginate(crate::pagination::PaginationTermination::take_pages(2))
        .collect()
        .await
        .expect_err("the later HTTPS page must be preflighted independently");
    assert!(matches!(
        error,
        crate::error::ApiClientError::TlsCapabilityUnavailable { .. }
    ));
    assert_eq!(body_factories.load(Ordering::SeqCst), 1);
    assert_eq!(executor.captures().len(), 1);
    assert_eq!(executor.remaining_scripts(), 1);
    let observed = events.lock().await.clone();
    assert_eq!(
        observed
            .iter()
            .filter(|event| *event == "auth_prepare")
            .count(),
        1
    );
    assert_eq!(
        observed
            .iter()
            .filter(|event| *event == "rate_acquire")
            .count(),
        1
    );
    assert_eq!(
        observed.iter().filter(|event| *event == "pre_send").count(),
        1
    );
}
