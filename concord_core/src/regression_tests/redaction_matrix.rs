#![allow(clippy::needless_update)] // Matrix fixtures keep `..Default::default()` for resilience to added fields.

use super::common::{
    CapturedExecution, DeterministicHarness, ItemsEndpoint, MockResponse, PaginationVariant,
    ScriptedReply, TestAuthVars, auth_policy, client, request_plan,
};
use crate::regression_tests::test_api::{
    AuthPlacement, BufferedResponse, EncodedRequest, Format, PreparedBody, RequestEntity,
    RequestPlan, ResponseEntity,
};
use crate::support::{
    RedactionSentinels, assert_error_chain_does_not_contain_any, assert_text_does_not_contain_any,
};
use bytes::Bytes;
use concord_core::advanced::{
    BodyCodec, CodecError, DecodeContext, EncodeContext, EncodedBody, ErrorContext,
    RateLimitBucketUse, RateLimitContext, RateLimitErrorKind, RateLimitFuture, RateLimitKey,
    RateLimitKeyPart, RateLimitKeyValue, RateLimitPermit, RateLimitPlan, RateLimitResponseAction,
    RateLimitResponseContext, RateLimiter, ResponseCodec, TextContentType,
};
use concord_core::error::ErrorCategory;
use concord_core::prelude::{ApiClientError, PaginationTermination, Text};
use http::{HeaderName, HeaderValue, Method, StatusCode};
use std::error::Error;
use std::fmt;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use tokio::sync::Mutex;

const REQUEST_HEADER_SENTINEL: &str = "PR20_REQUEST_HEADER_SENTINEL";
const REQUEST_QUERY_SENTINEL: &str = "PR20_REQUEST_QUERY_SENTINEL";
const REQUEST_BODY_SENTINEL: &str = "PR20_REQUEST_BODY_SENTINEL";
const REQUEST_CODEC_BODY_SENTINEL: &str = "LEAK_SENTINEL_REQUEST_BODY";
const REQUEST_CODEC_SOURCE_SENTINEL: &str = "LEAK_SENTINEL_CODEC_SOURCE";
const RESPONSE_BODY_SENTINEL: &str = "PR20_RESPONSE_BODY_SENTINEL";
const RESPONSE_CODEC_SENTINEL: &str = "PR20_RESPONSE_CODEC_SENTINEL";
const AUTH_SENTINEL: &str = "PR20_AUTH_SENTINEL";
const RATE_LIMIT_SENTINEL: &str = "PR20_RATE_LIMIT_SENTINEL";

#[derive(Clone, Copy)]
struct MatrixSentinelError(&'static str);

impl fmt::Debug for MatrixSentinelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = self.0;
        f.write_str("<redacted>")
    }
}

impl fmt::Display for MatrixSentinelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = self.0;
        f.write_str("<redacted>")
    }
}

impl Error for MatrixSentinelError {}

#[derive(Clone, Copy, Debug, Default)]
struct FailingRequestCodec;

impl BodyCodec for FailingRequestCodec {
    type Value = String;
    type Content = TextContentType;

    fn format() -> Format {
        Format::Text
    }

    fn encode(_value: Self::Value, _ctx: EncodeContext<'_>) -> Result<EncodedBody, CodecError> {
        Err(CodecError::with_source(
            REQUEST_CODEC_BODY_SENTINEL,
            MatrixSentinelError(REQUEST_CODEC_SOURCE_SENTINEL),
        ))
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct FailingResponseCodec;

impl ResponseCodec for FailingResponseCodec {
    type Value = String;
    type Content = TextContentType;

    fn format() -> Format {
        Format::Text
    }

    fn decode(bytes: Bytes, _ctx: DecodeContext<'_>) -> Result<Self::Value, CodecError> {
        let rendered = String::from_utf8_lossy(&bytes);
        if rendered.contains(RESPONSE_BODY_SENTINEL) {
            Err(CodecError::with_source(
                "response decoding failed",
                MatrixSentinelError(RESPONSE_CODEC_SENTINEL),
            ))
        } else {
            Ok(rendered.into_owned())
        }
    }
}

#[derive(Clone)]
struct RecordingRateLimiter {
    events: Arc<Mutex<Vec<String>>>,
    acquire_count: Arc<AtomicUsize>,
    fail_on_acquire: Option<usize>,
}

impl RecordingRateLimiter {
    fn failing_on_acquire(events: Arc<Mutex<Vec<String>>>, fail_on_acquire: usize) -> Self {
        Self {
            events,
            acquire_count: Arc::new(AtomicUsize::new(0)),
            fail_on_acquire: Some(fail_on_acquire),
        }
    }
}

fn rate_limit_key_value_label(ctx: &RateLimitContext<'_>, part: &RateLimitKeyPart) -> String {
    let value = match &part.value {
        RateLimitKeyValue::Static(value) => value.as_ref().to_string(),
        RateLimitKeyValue::Endpoint => ctx.endpoint.to_string(),
        RateLimitKeyValue::Method => ctx.method.as_str().to_string(),
        RateLimitKeyValue::UrlHost => ctx.url_host.unwrap_or("<none>").to_string(),
    };
    format!("{}={value}", part.name)
}

fn rate_limit_bucket_label(ctx: &RateLimitContext<'_>, bucket: &RateLimitBucketUse) -> String {
    let parts = bucket
        .key
        .parts()
        .iter()
        .map(|part| rate_limit_key_value_label(ctx, part))
        .collect::<Vec<_>>()
        .join(",");
    format!("{}:{}:[{parts}]", bucket.id.kind, bucket.id.name)
}

fn rate_limit_plan_label(ctx: &RateLimitContext<'_>) -> String {
    ctx.plan
        .buckets()
        .iter()
        .map(|bucket| rate_limit_bucket_label(ctx, bucket))
        .collect::<Vec<_>>()
        .join("|")
}

impl RateLimiter for RecordingRateLimiter {
    fn acquire<'a>(
        &'a self,
        ctx: RateLimitContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitPermit, ApiClientError>> {
        let events = self.events.clone();
        let acquire_count = self.acquire_count.clone();
        let fail_on_acquire = self.fail_on_acquire;
        Box::pin(async move {
            let acquire = acquire_count.fetch_add(1, AtomicOrdering::SeqCst) + 1;
            let label = rate_limit_plan_label(&ctx);
            events
                .lock()
                .await
                .push(format!("rate_acquire#{acquire}:{label}"));
            if fail_on_acquire == Some(acquire) {
                return Err(ApiClientError::RateLimit {
                    ctx: ErrorContext {
                        endpoint: ctx.endpoint,
                        method: ctx.method.clone(),
                    },
                    source: concord_core::advanced::RateLimitError::new(
                        RateLimitErrorKind::AcquireFailed,
                        "rate-limit acquisition denied",
                    ),
                });
            }
            Ok(RateLimitPermit)
        })
    }

    fn on_response<'a>(
        &'a self,
        _ctx: RateLimitResponseContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitResponseAction, ApiClientError>> {
        Box::pin(async move { Ok(RateLimitResponseAction::Continue) })
    }
}

fn policy_with_request_markers(
    header: Option<&'static str>,
    query_value: Option<&'static str>,
) -> crate::regression_tests::test_api::ResolvedPolicy {
    let mut policy = crate::regression_tests::test_api::ResolvedPolicy::default();
    if let Some(value) = header {
        policy.headers.insert(
            HeaderName::from_static("x-redaction-matrix"),
            HeaderValue::from_static(value),
        );
    }
    if let Some(value) = query_value {
        policy
            .query
            .push(("redaction".to_string(), value.to_string()));
    }
    policy
}

fn plan_with_body(
    name: &'static str,
    method: Method,
    path: &'static str,
    policy: crate::regression_tests::test_api::ResolvedPolicy,
    body: Option<&'static str>,
) -> RequestPlan {
    let mut plan = request_plan(name, method, path, policy, None);
    if let Some(body) = body {
        plan.body = PreparedBody::reusable_bytes(
            Bytes::from_static(body.as_bytes()),
            Some(HeaderValue::from_static("application/json")),
        );
    }
    plan
}

fn assert_source_surfaces_are_redacted(source: &(dyn Error + 'static), sentinels: &[&str]) {
    assert_text_does_not_contain_any(&source.to_string(), sentinels);
    assert_text_does_not_contain_any(&format!("{source:?}"), sentinels);
    assert_text_does_not_contain_any(&format!("{source:#?}"), sentinels);
}

fn assert_header_matches_sentinel(
    request: &CapturedExecution,
    header_name: HeaderName,
    sentinel: &str,
) {
    let value = request
        .headers
        .get(header_name)
        .and_then(|value| value.to_str().ok());
    assert!(value == Some(sentinel), "request header sentinel mismatch");
}

fn assert_body_matches_sentinel(request: &CapturedExecution, sentinel: &str) {
    assert!(request.body.is_bytes(), "request body category mismatch");
    assert!(!format!("{request:?}").contains(sentinel));
}

fn assert_authorization_matches_bearer_sentinel(request: &CapturedExecution, sentinel: &str) {
    assert!(!request.headers.contains_key(http::header::AUTHORIZATION));
    assert!(!format!("{request:?}").contains(sentinel));
}

#[tokio::test]
async fn request_encoding_failure_redacts_request_body_sentinel() {
    let err = EncodedRequest::<FailingRequestCodec>::prepare(
        REQUEST_BODY_SENTINEL.to_string(),
        ErrorContext {
            endpoint: "RequestEncodeMatrix",
            method: Method::POST,
        },
    )
    .expect_err("request encoding should fail");

    assert!(matches!(err, ApiClientError::Codec { .. }));
    assert_eq!(err.category(), ErrorCategory::Decode);
    assert_eq!(err.context().endpoint, "RequestEncodeMatrix");
    assert_eq!(err.context().method, Method::POST);
    let rendered = format!("{err}\n{err:?}\n{err:#?}");
    assert!(rendered.contains("request body encoding failed"));
    assert!(!rendered.contains(REQUEST_CODEC_BODY_SENTINEL));
    assert!(!rendered.contains(REQUEST_CODEC_SOURCE_SENTINEL));
    match &err {
        ApiClientError::Codec { source, .. } => {
            assert_source_surfaces_are_redacted(
                source.as_ref(),
                &[REQUEST_CODEC_BODY_SENTINEL, REQUEST_CODEC_SOURCE_SENTINEL],
            );
        }
        _ => panic!("expected codec error"),
    }
    assert_error_chain_does_not_contain_any(
        &err,
        &[REQUEST_CODEC_BODY_SENTINEL, REQUEST_CODEC_SOURCE_SENTINEL],
    );
}

#[tokio::test]
async fn response_decoding_failure_redacts_response_and_request_sentinels() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let harness = DeterministicHarness::new(
        events,
        vec![MockResponse::text(StatusCode::OK, RESPONSE_BODY_SENTINEL)],
    );
    let sent = harness.clone();
    let client = client(
        TestAuthVars {
            token: Some(AUTH_SENTINEL.to_string()),
            identity: "sentinel",
        },
        harness,
    );
    let mut policy = policy_with_request_markers(Some(REQUEST_HEADER_SENTINEL), None);
    policy.auth = auth_policy(AuthPlacement::Bearer).auth;
    let plan = plan_with_body(
        "ResponseDecodeMatrix",
        Method::GET,
        "/redaction/response-decode",
        policy,
        None,
    );

    let err = BufferedResponse::<FailingResponseCodec>::execute(&client, plan)
        .await
        .expect_err("decode failure should surface");

    assert!(matches!(err, ApiClientError::Decode { .. }));
    assert_eq!(err.category(), ErrorCategory::Decode);
    assert_eq!(err.context().endpoint, "ResponseDecodeMatrix");
    assert_eq!(err.context().method, Method::GET);
    assert_eq!(err.decode_status(), Some(StatusCode::OK));
    assert_eq!(err.decode_content_type(), Some("text/plain"));
    assert_eq!(sent.sent_count().await, 1);
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 1);
    assert_header_matches_sentinel(
        &requests[0],
        HeaderName::from_static("x-redaction-matrix"),
        REQUEST_HEADER_SENTINEL,
    );
    assert_authorization_matches_bearer_sentinel(&requests[0], AUTH_SENTINEL);
    match &err {
        ApiClientError::Decode { source, .. } => {
            assert_source_surfaces_are_redacted(
                source.as_ref(),
                &[
                    REQUEST_HEADER_SENTINEL,
                    RESPONSE_BODY_SENTINEL,
                    RESPONSE_CODEC_SENTINEL,
                ],
            );
        }
        _ => panic!("expected decode error"),
    }
    let rendered = format!("{err}\n{err:?}\n{err:#?}");
    assert!(rendered.contains("response body decode failed"));
    assert!(!rendered.contains(AUTH_SENTINEL));
    assert!(!rendered.contains(REQUEST_HEADER_SENTINEL));
    assert!(!rendered.contains(REQUEST_QUERY_SENTINEL));
    assert!(!rendered.contains(REQUEST_BODY_SENTINEL));
    assert!(!rendered.contains(RESPONSE_BODY_SENTINEL));
    assert!(!rendered.contains(RESPONSE_CODEC_SENTINEL));
    assert_error_chain_does_not_contain_any(
        &err,
        &[
            AUTH_SENTINEL,
            REQUEST_HEADER_SENTINEL,
            REQUEST_QUERY_SENTINEL,
            REQUEST_BODY_SENTINEL,
            RESPONSE_BODY_SENTINEL,
            RESPONSE_CODEC_SENTINEL,
        ],
    );
}

#[tokio::test]
async fn http_status_failure_redacts_request_and_response_sentinels() -> Result<(), ApiClientError>
{
    let sentinels = RedactionSentinels::new(
        REQUEST_HEADER_SENTINEL,
        REQUEST_BODY_SENTINEL,
        RESPONSE_BODY_SENTINEL,
    );
    let events = Arc::new(Mutex::new(Vec::new()));
    let harness = DeterministicHarness::new(
        events,
        vec![
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, RESPONSE_BODY_SENTINEL)
                .expect_body(Bytes::from_static(REQUEST_BODY_SENTINEL.as_bytes()))
                .expect_query_pair("redaction", REQUEST_QUERY_SENTINEL),
        ],
    );
    let sent = harness.clone();
    let client = client(TestAuthVars::default(), harness);
    let policy =
        policy_with_request_markers(Some(REQUEST_HEADER_SENTINEL), Some(REQUEST_QUERY_SENTINEL));
    let plan = plan_with_body(
        "HttpStatusMatrix",
        Method::POST,
        "/redaction/http-status",
        policy,
        Some(REQUEST_BODY_SENTINEL),
    );

    let err = client
        .execute_plan::<Text<String>>(plan)
        .await
        .expect_err("HTTP 500 should surface as a typed status error");

    assert!(matches!(err, ApiClientError::HttpStatus { .. }));
    assert_eq!(err.category(), ErrorCategory::HttpStatus);
    assert_eq!(err.context().endpoint, "HttpStatusMatrix");
    assert_eq!(err.context().method, Method::POST);
    assert_eq!(err.http_status(), Some(StatusCode::INTERNAL_SERVER_ERROR));
    assert_eq!(sent.sent_count().await, 1);
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_header_matches_sentinel(
        request,
        HeaderName::from_static("x-redaction-matrix"),
        REQUEST_HEADER_SENTINEL,
    );
    assert_body_matches_sentinel(request, REQUEST_BODY_SENTINEL);
    assert_error_chain_does_not_contain_any(
        &err,
        &[
            sentinels.auth,
            REQUEST_QUERY_SENTINEL,
            sentinels.body,
            sentinels.response,
        ],
    );
    Ok(())
}

#[tokio::test]
async fn request_execution_failure_redacts_request_material() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let harness = DeterministicHarness::from_replies(
        events,
        [ScriptedReply::disconnect_after_request_body()
            .expect_body(Bytes::from_static(REQUEST_BODY_SENTINEL.as_bytes()))
            .expect_query_pair("redaction", REQUEST_QUERY_SENTINEL)],
    );
    let sent = harness.clone();
    let client = client(TestAuthVars::default(), harness);
    let policy =
        policy_with_request_markers(Some(REQUEST_HEADER_SENTINEL), Some(REQUEST_QUERY_SENTINEL));
    let plan = plan_with_body(
        "TransportMatrix",
        Method::PUT,
        "/redaction/harness",
        policy,
        Some(REQUEST_BODY_SENTINEL),
    );

    let err = client
        .execute_plan::<Text<String>>(plan)
        .await
        .expect_err("harness failure should surface as harness error");

    assert!(matches!(
        err,
        ApiClientError::RequestExecution { .. }
            | ApiClientError::Connect { .. }
            | ApiClientError::Timeout { .. }
    ));
    assert_eq!(err.category(), ErrorCategory::RequestExecution);
    assert_eq!(err.context().endpoint, "TransportMatrix");
    assert_eq!(err.context().method, Method::PUT);
    let first_source = std::error::Error::source(&err).expect("request error source");
    let opaque = first_source
        .downcast_ref::<concord_core::prelude::RequestErrorSource>()
        .unwrap_or_else(|| panic!("request execution source was {first_source:?}"));
    assert!(
        opaque.source().is_some(),
        "sanitized source chain is retained"
    );
    assert_eq!(sent.sent_count().await, 1);
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_header_matches_sentinel(
        request,
        HeaderName::from_static("x-redaction-matrix"),
        REQUEST_HEADER_SENTINEL,
    );
    assert_body_matches_sentinel(request, REQUEST_BODY_SENTINEL);
    assert_error_chain_does_not_contain_any(
        &err,
        &[
            REQUEST_HEADER_SENTINEL,
            REQUEST_QUERY_SENTINEL,
            REQUEST_BODY_SENTINEL,
        ],
    );
}

#[tokio::test]
async fn auth_rejection_redacts_auth_sentinel_and_context() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let harness = DeterministicHarness::new(
        events,
        vec![MockResponse::text(
            StatusCode::UNAUTHORIZED,
            RESPONSE_BODY_SENTINEL,
        )],
    );
    let sent = harness.clone();
    let client = client(
        TestAuthVars {
            token: Some(AUTH_SENTINEL.to_string()),
            identity: "matrix-user",
        },
        harness,
    );
    let plan = request_plan(
        "AuthMatrix",
        Method::GET,
        "/redaction/auth",
        auth_policy(AuthPlacement::Bearer),
        None,
    );

    let err = client
        .execute_plan::<Text<String>>(plan)
        .await
        .expect_err("auth rejection should surface as an auth error");

    assert!(matches!(err, ApiClientError::Auth { .. }));
    assert_eq!(err.category(), ErrorCategory::AuthRejected);
    assert_eq!(err.context().endpoint, "AuthMatrix");
    assert_eq!(err.context().method, Method::GET);
    assert_eq!(sent.sent_count().await, 1);
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 1);
    assert_authorization_matches_bearer_sentinel(&requests[0], AUTH_SENTINEL);
    match &err {
        ApiClientError::Auth { source, .. } => {
            assert_source_surfaces_are_redacted(source, &[AUTH_SENTINEL, RESPONSE_BODY_SENTINEL]);
        }
        _ => panic!("expected auth error"),
    }
    assert_error_chain_does_not_contain_any(&err, &[AUTH_SENTINEL, RESPONSE_BODY_SENTINEL]);
}

#[tokio::test]
async fn terminal_status_redacts_request_and_response_sentinels() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let harness = DeterministicHarness::new(
        events,
        vec![MockResponse::text(
            StatusCode::INTERNAL_SERVER_ERROR,
            RESPONSE_BODY_SENTINEL,
        )],
    );
    let sent = harness.clone();
    let client = client(TestAuthVars::default(), harness);
    let mut policy = crate::regression_tests::test_api::ResolvedPolicy::default();
    policy.headers.insert(
        HeaderName::from_static("x-redaction-matrix"),
        HeaderValue::from_static(REQUEST_HEADER_SENTINEL),
    );
    let plan = plan_with_body("RetryMatrix", Method::GET, "/redaction/retry", policy, None);

    let err = client
        .execute_plan::<Text<String>>(plan)
        .await
        .expect_err("terminal status should surface as a status error");

    assert!(matches!(err, ApiClientError::HttpStatus { .. }));
    assert_eq!(err.category(), ErrorCategory::HttpStatus);
    assert_eq!(err.context().endpoint, "RetryMatrix");
    assert_eq!(err.context().method, Method::GET);
    assert_eq!(err.http_status(), Some(StatusCode::INTERNAL_SERVER_ERROR));
    assert_eq!(sent.sent_count().await, 1);
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 1);
    for request in &requests {
        assert_header_matches_sentinel(
            request,
            HeaderName::from_static("x-redaction-matrix"),
            REQUEST_HEADER_SENTINEL,
        );
    }
    assert_error_chain_does_not_contain_any(
        &err,
        &[REQUEST_HEADER_SENTINEL, RESPONSE_BODY_SENTINEL],
    );
}

#[tokio::test]

async fn rate_limit_acquire_failure_redacts_key_material_and_context() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let limiter = Arc::new(RecordingRateLimiter::failing_on_acquire(events.clone(), 1));
    let harness = DeterministicHarness::from_replies(events.clone(), std::iter::empty());
    let sent = harness.clone();
    let mut client = client(TestAuthVars::default(), harness);
    client.configure(|cfg| {
        cfg.rate_limiter(limiter);
    });
    let mut policy = crate::regression_tests::test_api::ResolvedPolicy::default();
    let mut plan = RateLimitPlan::new();
    plan.push_bucket(
        RateLimitBucketUse::new(
            "matrix",
            "tenant",
            RateLimitKey::new(vec![RateLimitKeyPart::static_value(
                "tenant",
                RATE_LIMIT_SENTINEL,
            )]),
        )
        .with_window(concord_core::advanced::RateLimitWindow::new(
            NonZeroU32::new(5).expect("non-zero"),
            std::time::Duration::from_secs(1),
        )),
    );
    policy.rate_limit = plan;
    let plan = plan_with_body(
        "RateLimitMatrix",
        Method::GET,
        "/redaction/rate-limit",
        policy,
        None,
    );

    let err = client
        .execute_plan::<Text<String>>(plan)
        .await
        .expect_err("rate-limit acquire failure should surface as rate-limit error");

    assert!(matches!(err, ApiClientError::RateLimit { .. }));
    assert_eq!(err.category(), ErrorCategory::RateLimit);
    assert_eq!(err.context().endpoint, "RateLimitMatrix");
    assert_eq!(err.context().method, Method::GET);
    assert_eq!(
        err.rate_limit_error().map(|source| source.kind()),
        Some(RateLimitErrorKind::AcquireFailed)
    );
    assert_eq!(sent.sent_count().await, 0);
    let events = events.lock().await.clone();
    assert!(
        events
            .iter()
            .any(|event| event.contains(RATE_LIMIT_SENTINEL)),
        "rate-limit acquire should observe the sentinel in the planned key"
    );
    if let Some(source) = err.rate_limit_error() {
        assert_source_surfaces_are_redacted(source, &[RATE_LIMIT_SENTINEL]);
    } else {
        panic!("expected a rate-limit source error");
    }
    assert_error_chain_does_not_contain_any(&err, &[RATE_LIMIT_SENTINEL]);
}

#[tokio::test]
async fn pagination_late_page_failure_redacts_request_and_response_sentinels()
-> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let harness = DeterministicHarness::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "one,two"),
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, RESPONSE_BODY_SENTINEL),
        ],
    );
    let sent = harness.clone();
    let client = client(TestAuthVars::default(), harness);
    let endpoint = ItemsEndpoint {
        policy: policy_with_request_markers(Some(REQUEST_HEADER_SENTINEL), None),
        start: 0,
        count: 2,
        pagination: PaginationVariant::OffsetLimit {
            offset: 0,
            limit: 2,
        },
        ..Default::default()
    };

    let err = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(100))
        .collect()
        .await
        .expect_err("later pagination failure should surface as a typed status error");

    assert!(matches!(err, ApiClientError::HttpStatus { .. }));
    assert_eq!(err.category(), ErrorCategory::HttpStatus);
    assert_eq!(err.context().endpoint, "Items");
    assert_eq!(err.context().method, Method::GET);
    assert_eq!(err.http_status(), Some(StatusCode::INTERNAL_SERVER_ERROR));
    assert_eq!(sent.sent_count().await, 2);
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 2);
    for request in &requests {
        assert_header_matches_sentinel(
            request,
            HeaderName::from_static("x-redaction-matrix"),
            REQUEST_HEADER_SENTINEL,
        );
    }
    assert_error_chain_does_not_contain_any(
        &err,
        &[REQUEST_HEADER_SENTINEL, RESPONSE_BODY_SENTINEL],
    );
    Ok(())
}
