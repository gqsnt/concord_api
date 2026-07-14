use super::common::{
    ObservationRateLimiter, ObservationRuntimeHooks, TestAuthVars, TestCx, auth_policy,
    request_plan,
};
use crate::__development::{
    CapturedBodyCategory, DeterministicBodyGate, DeterministicFakeCredential,
    DeterministicNativeExecutor, ScriptedNativeResponse, SyntheticExecutionFailure,
    UnsafeCredentialPlacementExpectations, install_application_executor,
};
use crate::advanced::{
    AuthRequirement, CredentialId, OctetStream, PreSendHookContext, RequestErrorHookContext,
    RuntimeHooks,
};
use crate::error::ErrorCategory;
use crate::prelude::{ApiClient, ApiClientError, Text};
use crate::regression_tests::test_api::{
    AuthPlacement, AuthProvenance, AuthUsageId, CredentialRef, PreparedBody,
};
use bytes::Bytes;
use http::{HeaderName, HeaderValue, Method, StatusCode};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const FAKE_SECRET: &str = "deterministic-fake-token-f05";

fn text_response(body: &'static [u8]) -> ScriptedNativeResponse {
    ScriptedNativeResponse::bytes(StatusCode::OK, Bytes::from_static(body)).with_header(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain"),
    )
}

fn client_with(executor: &DeterministicNativeExecutor, auth: TestAuthVars) -> ApiClient<TestCx> {
    let mut client = ApiClient::<TestCx>::new((), auth);
    install_application_executor(&mut client, executor.clone()).expect("application executor");
    client
}

#[derive(Default)]
struct AuthenticationOrderingHooks {
    pre_send_called: AtomicBool,
    pre_send_had_authorization: AtomicBool,
}

impl RuntimeHooks for AuthenticationOrderingHooks {
    fn pre_send<'a>(
        &'a self,
        ctx: PreSendHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ApiClientError>> + Send + 'a>> {
        self.pre_send_called.store(true, Ordering::SeqCst);
        self.pre_send_had_authorization.store(
            ctx.headers.contains_key(http::header::AUTHORIZATION),
            Ordering::SeqCst,
        );
        Box::pin(async { Ok(()) })
    }
}

#[tokio::test]
async fn deterministic_executor_capture_receives_request_after_authentication_ordering() {
    let executor = DeterministicNativeExecutor::application();
    let expectations = UnsafeCredentialPlacementExpectations::new()
        .expect_header(
            http::header::AUTHORIZATION,
            DeterministicFakeCredential::new(format!("Bearer {FAKE_SECRET}")),
        )
        .expect_query_pair("api_key", DeterministicFakeCredential::new(FAKE_SECRET))
        .expect_body_category(CapturedBodyCategory::Empty);
    executor.script_response(
        text_response(b"authenticated").with_unsafe_credential_placement_expectations(expectations),
    );

    let mut client = client_with(
        &executor,
        TestAuthVars {
            token: Some(FAKE_SECRET.to_string()),
            identity: "deterministic",
        },
    );
    let hooks = Arc::new(AuthenticationOrderingHooks::default());
    client.set_runtime_hooks(hooks.clone());

    let mut policy = auth_policy(AuthPlacement::Bearer);
    policy.auth.requirements.push(AuthRequirement {
        credential: CredentialRef {
            id: CredentialId::new("test", "token"),
        },
        placement: AuthPlacement::Query("api_key"),
        usage_id: AuthUsageId::new("test-token-query"),
        step_id: Some("query"),
        provenance: AuthProvenance::new("test"),
        challenge: Default::default(),
    });
    policy
        .query
        .push(("view".to_string(), "public".to_string()));
    policy
        .query
        .push(("route".to_string(), "logical".to_string()));
    policy.headers.insert(
        HeaderName::from_static("x-public-metadata"),
        HeaderValue::from_static("visible"),
    );

    let response = client
        .execute_plan::<Text<String>>(request_plan(
            "DeterministicAuthenticationOrdering",
            Method::GET,
            "/native",
            policy,
            None,
        ))
        .await
        .expect("synthetic authenticated response");
    assert_eq!(response.value(), "authenticated");
    assert!(hooks.pre_send_called.load(Ordering::SeqCst));
    assert!(!hooks.pre_send_had_authorization.load(Ordering::SeqCst));

    let captures = executor.captures();
    assert_eq!(captures.len(), 1);
    let capture = &captures[0];
    assert_eq!(capture.endpoint(), "DeterministicAuthenticationOrdering");
    assert_eq!(capture.method(), Method::GET);
    assert_eq!(capture.page_index(), 0);
    assert!(capture.idempotent());
    assert_eq!(capture.body_category(), CapturedBodyCategory::Empty);
    assert_eq!(capture.known_body_length(), None);
    assert_eq!(
        capture
            .logical_target()
            .query_pairs()
            .find(|(name, _)| name == "route")
            .map(|(_, value)| value.into_owned()),
        Some("logical".to_string())
    );
    assert_eq!(
        capture
            .logical_target()
            .query_pairs()
            .find(|(name, _)| name == "view")
            .map(|(_, value)| value.into_owned()),
        Some("public".to_string())
    );
    assert!(
        capture
            .logical_target()
            .query_pairs()
            .all(|(name, value)| name != "api_key" && value != FAKE_SECRET)
    );
    assert_eq!(
        capture
            .public_headers()
            .get("x-public-metadata")
            .and_then(|value| value.to_str().ok()),
        Some("visible")
    );
    assert!(
        !capture
            .public_headers()
            .contains_key(http::header::AUTHORIZATION)
    );
    assert!(
        capture
            .protected_header_names()
            .contains(&http::header::AUTHORIZATION)
    );
    let rendered = format!("{capture:?}");
    assert!(!rendered.contains(FAKE_SECRET), "{rendered}");
}

#[tokio::test]
async fn synthetic_native_buffered_response_runs_hooks_rate_limit_and_codec_decode() {
    let executor = DeterministicNativeExecutor::application();
    executor.script_response(
        ScriptedNativeResponse::chunks(
            StatusCode::OK,
            [
                Bytes::from_static(b"bounded "),
                Bytes::from_static(b"decode"),
            ],
        )
        .with_header(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain"),
        )
        .with_header(
            HeaderName::from_static("x-rate-observation"),
            HeaderValue::from_static("seen"),
        ),
    );
    let events = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let mut client = client_with(&executor, TestAuthVars::default());
    client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));
    client.set_rate_limiter(Arc::new(ObservationRateLimiter::new(events.clone())));

    let response = client
        .execute_plan::<Text<String>>(request_plan(
            "DeterministicBuffered",
            Method::GET,
            "/buffered",
            Default::default(),
            None,
        ))
        .await
        .expect("buffered response decodes");
    assert_eq!(response.value(), "bounded decode");

    let events = events.lock().await.clone();
    assert_eq!(events.first().map(String::as_str), Some("rate_acquire"));
    assert_eq!(events.get(1).map(String::as_str), Some("pre_send"));
    let hook_status = events
        .iter()
        .position(|event| event == "hook_status:200 OK")
        .expect("response hook status");
    let rate_status = events
        .iter()
        .position(|event| event == "rate_status:200 OK")
        .expect("rate response status");
    assert!(hook_status < rate_status);
    assert!(
        events.iter().any(|event| {
            event.contains("hook_headers") && event.contains("x-rate-observation")
        })
    );
    assert!(
        events.iter().any(|event| {
            event.contains("rate_headers") && event.contains("x-rate-observation")
        })
    );
}

#[tokio::test]
async fn deterministic_capture_reports_body_shape_and_length_without_body_bytes() {
    const BODY_SENTINEL: &[u8] = b"CAPTURE_BODY_BYTES_MUST_NOT_APPEAR";
    let executor = DeterministicNativeExecutor::application();
    executor.script_response(text_response(b"ok"));
    let client = client_with(&executor, TestAuthVars::default());
    let mut plan = request_plan(
        "DeterministicBodyCapture",
        Method::POST,
        "/body-capture",
        Default::default(),
        None,
    );
    plan.body = PreparedBody::reusable_bytes(
        Bytes::from_static(BODY_SENTINEL),
        Some(HeaderValue::from_static("application/octet-stream")),
    );

    let response = client
        .execute_plan::<Text<String>>(plan)
        .await
        .expect("buffered body request");
    assert_eq!(response.value(), "ok");
    let captures = executor.captures();
    let capture = captures.first().expect("capture");
    assert_eq!(capture.body_category(), CapturedBodyCategory::Buffered);
    assert_eq!(
        capture.known_body_length(),
        Some(BODY_SENTINEL.len() as u64)
    );
    let rendered = format!("{capture:?}");
    assert!(
        !rendered.contains(std::str::from_utf8(BODY_SENTINEL).expect("ASCII sentinel")),
        "{rendered}"
    );
}

#[tokio::test]
async fn synthetic_native_raw_buffered_response_uses_shared_bounded_collection() {
    let executor = DeterministicNativeExecutor::application();
    executor.script_response(
        ScriptedNativeResponse::chunks(
            StatusCode::ACCEPTED,
            [Bytes::from_static(b"raw-"), Bytes::from_static(b"buffered")],
        )
        .with_header(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/octet-stream"),
        ),
    );
    let client = client_with(&executor, TestAuthVars::default());
    let response = client
        .execute_plan_raw(request_plan(
            "DeterministicRawBuffered",
            Method::GET,
            "/raw-buffered",
            Default::default(),
            None,
        ))
        .await
        .expect("raw buffered response");
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(response.body(), &Bytes::from_static(b"raw-buffered"));
    assert_eq!(response.url().path(), "/raw-buffered");
}

#[tokio::test]
async fn synthetic_native_status_is_classified_by_the_production_pipeline() {
    let executor = DeterministicNativeExecutor::application();
    executor.script_response(
        ScriptedNativeResponse::bytes(StatusCode::IM_A_TEAPOT, Bytes::from_static(b"terminal"))
            .with_header(
                http::header::CONTENT_TYPE,
                HeaderValue::from_static("text/plain"),
            ),
    );
    let client = client_with(&executor, TestAuthVars::default());
    let error = client
        .execute_plan::<Text<String>>(request_plan(
            "DeterministicStatus",
            Method::GET,
            "/status",
            Default::default(),
            None,
        ))
        .await
        .expect_err("status classification is terminal");
    assert_eq!(error.category(), ErrorCategory::HttpStatus);
    assert_eq!(error.http_status(), Some(StatusCode::IM_A_TEAPOT));
    assert_eq!(executor.captures().len(), 1);
    assert_eq!(executor.remaining_scripts(), 0);
}

#[tokio::test]
async fn synthetic_streaming_response_uses_real_bounded_response_stream() {
    let executor = DeterministicNativeExecutor::application();
    let gate = DeterministicBodyGate::new();
    let mut trailers = http::HeaderMap::new();
    trailers.insert(
        HeaderName::from_static("x-trailer"),
        HeaderValue::from_static("present"),
    );
    executor.script_response(
        ScriptedNativeResponse::chunks(
            StatusCode::OK,
            [Bytes::from_static(b"ab"), Bytes::from_static(b"cd")],
        )
        .with_header(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/octet-stream"),
        )
        .with_trailers(trailers)
        .with_gate(gate.clone()),
    );
    let mut client = client_with(&executor, TestAuthVars::default());
    client.configure(|config| {
        config.max_stream_response_body_bytes(3);
    });
    let mut response = client
        .execute_stream_response::<OctetStream>(request_plan(
            "DeterministicStreamLimit",
            Method::GET,
            "/stream",
            Default::default(),
            None,
        ))
        .await
        .expect("stream response head");

    let first =
        tokio::time::timeout(std::time::Duration::from_millis(20), response.next_chunk()).await;
    assert!(first.is_err(), "gated native body must remain pending");
    gate.release();
    assert_eq!(
        response.next_chunk().await.expect("first bounded chunk"),
        Some(Bytes::from_static(b"ab"))
    );
    let error = response
        .next_chunk()
        .await
        .expect_err("second chunk exceeds shared stream limit");
    assert!(matches!(
        error,
        ApiClientError::ResponseBodyLimitExceeded { limit: 3, .. }
    ));
    assert_eq!(response.next_chunk().await.expect("terminal stream"), None);
}

#[tokio::test]
async fn synthetic_partial_body_failure_flows_through_buffered_body_mapping() {
    let executor = DeterministicNativeExecutor::application();
    executor.script_response(
        ScriptedNativeResponse::chunks(StatusCode::OK, [Bytes::from_static(b"partial")])
            .with_header(
                http::header::CONTENT_TYPE,
                HeaderValue::from_static("text/plain"),
            )
            .with_body_failure(),
    );
    let client = client_with(&executor, TestAuthVars::default());
    let error = client
        .execute_plan::<Text<String>>(request_plan(
            "DeterministicPartialBodyFailure",
            Method::GET,
            "/partial",
            Default::default(),
            None,
        ))
        .await
        .expect_err("partial body failure");
    assert!(matches!(error, ApiClientError::ResponseBody { .. }));
    assert_eq!(error.category(), ErrorCategory::Decode);
}

#[derive(Default)]
struct CategoryHooks {
    observed: Mutex<Vec<ErrorCategory>>,
}

impl RuntimeHooks for CategoryHooks {
    fn request_error<'a>(
        &'a self,
        ctx: RequestErrorHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        self.observed
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(ctx.category);
        Box::pin(async {})
    }
}

#[tokio::test]
async fn synthetic_execution_failure_matches_request_error_hook_and_terminal_error() {
    for (failure, expected) in [
        (SyntheticExecutionFailure::Timeout, ErrorCategory::Timeout),
        (SyntheticExecutionFailure::Connect, ErrorCategory::Connect),
        (
            SyntheticExecutionFailure::Request,
            ErrorCategory::RequestExecution,
        ),
        (SyntheticExecutionFailure::Body, ErrorCategory::RequestBody),
    ] {
        let executor = DeterministicNativeExecutor::application();
        executor.script_failure(failure);
        let mut client = client_with(&executor, TestAuthVars::default());
        let hooks = Arc::new(CategoryHooks::default());
        client.set_runtime_hooks(hooks.clone());
        let error = client
            .execute_plan::<Text<String>>(request_plan(
                "DeterministicExecutionFailure",
                Method::GET,
                "/failure",
                Default::default(),
                None,
            ))
            .await
            .expect_err("synthetic execution failure");
        assert_eq!(error.category(), expected);
        assert_eq!(
            hooks
                .observed
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .as_slice(),
            &[expected]
        );
        let diagnostic = format!("{error:?}\n{error}");
        assert!(!diagnostic.contains("http://"), "{diagnostic}");
        assert!(!diagnostic.contains(FAKE_SECRET), "{diagnostic}");
    }
}

#[test]
fn production_constructor_has_no_executor_selector_and_channel_mismatch_is_rejected() {
    let mut client = ApiClient::<TestCx>::new((), TestAuthVars::default());
    let provider = DeterministicNativeExecutor::provider();
    assert!(install_application_executor(&mut client, provider).is_err());

    let source = include_str!("../client/api.rs");
    assert!(!source.contains("pub fn with_executor"));
    assert!(!source.contains("pub fn new_with_executor"));
    assert!(!source.contains("ApiClient<Cx,"));
}
