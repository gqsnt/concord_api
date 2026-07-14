use super::common::{
    ItemsEndpoint, NativeMockHarness, NativeMockReply, PaginationVariant, TestAuthVars, TestCx,
    TextEndpoint, auth_policy, client,
};
use crate::regression_tests::test_api::{
    AuthPlacement, EndpointMeta, EndpointPlan, PreparedBody, RawStreamResponse, RegressionEndpoint,
    RegressionPlanContext, RegressionReusableEndpoint, RequestOverrides, RequestPlan,
    ResolvedPolicy, ResolvedRoute, ResponseEntity, ResponsePlan,
};
use bytes::Bytes;
use concord_core::advanced::PaginationTermination;
use concord_core::advanced::{
    DebugSink, OctetStream, PostResponseHookContext, PreSendHookContext, RateLimitContext,
    RateLimitFuture, RateLimitPermit, RateLimitResponseAction, RateLimitResponseContext,
    RateLimiter, RequestErrorHookContext, RuntimeHooks, SanitizedHeaders, StreamResponse,
};
use concord_core::prelude::{ApiClient, ApiClientError, DebugLevel};
use http::{HeaderValue, Method, StatusCode};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use tokio::sync::Mutex as AsyncMutex;

const QUERY_NAME: &str = "credential_parameter_without_sensitive_words";
const QUERY_SENTINEL: &str = "C01_LOGICAL_URL_8H2K5M_SENTINEL";
const PUBLIC_NAME: &str = "visible";
const PUBLIC_VALUE: &str = "ordinary-value";

#[derive(Default)]
struct UrlObservations {
    pre_send: Vec<String>,
    post_response: Vec<String>,
    request_error: Vec<String>,
    rate_acquire: Vec<String>,
    rate_response: Vec<String>,
    debug_request: Vec<String>,
    debug_response: Vec<String>,
    diagnostics: Vec<String>,
}

#[derive(Clone)]
struct UrlHooks(Arc<Mutex<UrlObservations>>);

impl RuntimeHooks for UrlHooks {
    fn pre_send<'a>(
        &'a self,
        ctx: PreSendHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ApiClientError>> + Send + 'a>> {
        let observations = self.0.clone();
        let url = ctx.meta.url.to_string();
        let diagnostic = format!("{ctx:?}");
        Box::pin(async move {
            let mut observations = observations.lock().expect("URL observations lock");
            observations.pre_send.push(url);
            observations.diagnostics.push(diagnostic);
            Ok(())
        })
    }

    fn post_response<'a>(
        &'a self,
        ctx: PostResponseHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        let observations = self.0.clone();
        let url = ctx.meta.url.to_string();
        let diagnostic = format!("{ctx:?}");
        Box::pin(async move {
            let mut observations = observations.lock().expect("URL observations lock");
            observations.post_response.push(url);
            observations.diagnostics.push(diagnostic);
        })
    }

    fn request_error<'a>(
        &'a self,
        ctx: RequestErrorHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        let observations = self.0.clone();
        let url = ctx.meta.url.to_string();
        let diagnostic = format!("{ctx:?}");
        Box::pin(async move {
            let mut observations = observations.lock().expect("URL observations lock");
            observations.request_error.push(url);
            observations.diagnostics.push(diagnostic);
        })
    }
}

#[derive(Clone)]
struct UrlRateLimiter(Arc<Mutex<UrlObservations>>);

impl RateLimiter for UrlRateLimiter {
    fn acquire<'a>(
        &'a self,
        ctx: RateLimitContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitPermit, ApiClientError>> {
        let observations = self.0.clone();
        let url = ctx.url.to_string();
        let diagnostic = format!("{ctx:?}");
        Box::pin(async move {
            let mut observations = observations.lock().expect("URL observations lock");
            observations.rate_acquire.push(url);
            observations.diagnostics.push(diagnostic);
            Ok(RateLimitPermit)
        })
    }

    fn on_response<'a>(
        &'a self,
        ctx: RateLimitResponseContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitResponseAction, ApiClientError>> {
        let observations = self.0.clone();
        let url = ctx.meta.url.to_string();
        let diagnostic = format!("{ctx:?}");
        Box::pin(async move {
            let mut observations = observations.lock().expect("URL observations lock");
            observations.rate_response.push(url);
            observations.diagnostics.push(diagnostic);
            Ok(RateLimitResponseAction::Continue)
        })
    }
}

#[derive(Clone)]
struct UrlDebugSink(Arc<Mutex<UrlObservations>>);

impl DebugSink for UrlDebugSink {
    fn request_start(
        &self,
        _debug: DebugLevel,
        _method: &Method,
        url: &str,
        _endpoint: &'static str,
        _page_index: u32,
    ) {
        self.0
            .lock()
            .expect("URL observations lock")
            .debug_request
            .push(url.to_string());
    }

    fn request_headers(&self, _debug: DebugLevel, _headers: SanitizedHeaders<'_>) {}

    fn response_status(&self, _debug: DebugLevel, _status: StatusCode, url: &str, _ok: bool) {
        self.0
            .lock()
            .expect("URL observations lock")
            .debug_response
            .push(url.to_string());
    }

    fn response_headers(&self, _debug: DebugLevel, _headers: SanitizedHeaders<'_>) {}
}

fn query_policy() -> ResolvedPolicy {
    let mut policy = auth_policy(AuthPlacement::Query(QUERY_NAME));
    policy
        .query
        .push((PUBLIC_NAME.to_string(), PUBLIC_VALUE.to_string()));
    policy
}

fn text_endpoint() -> TextEndpoint {
    TextEndpoint {
        policy: query_policy(),
        ..TextEndpoint::default()
    }
}

fn expected_text_reply(status: StatusCode, body: &'static [u8]) -> NativeMockReply {
    NativeMockReply::status(status)
        .with_header(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain"),
        )
        .with_body(Bytes::from_static(body))
        .expect_query_pair(QUERY_NAME, QUERY_SENTINEL)
}

fn configured_client(
    replies: Vec<NativeMockReply>,
) -> (
    super::common::TestClient,
    NativeMockHarness,
    Arc<Mutex<UrlObservations>>,
) {
    configured_client_with_identity(replies, "safe-logical-url")
}

fn configured_client_with_identity(
    replies: Vec<NativeMockReply>,
    identity: &'static str,
) -> (
    super::common::TestClient,
    NativeMockHarness,
    Arc<Mutex<UrlObservations>>,
) {
    let harness =
        NativeMockHarness::from_native_replies(Arc::new(AsyncMutex::new(Vec::new())), replies);
    let capture = harness.clone();
    let observations = Arc::new(Mutex::new(UrlObservations::default()));
    let mut client = client(
        TestAuthVars {
            token: Some(QUERY_SENTINEL.to_string()),
            identity,
        },
        harness,
    );
    client.set_runtime_hooks(Arc::new(UrlHooks(observations.clone())));
    client.set_rate_limiter(Arc::new(UrlRateLimiter(observations.clone())));
    client.set_debug_sink(Arc::new(UrlDebugSink(observations.clone())));
    client.set_debug_level(DebugLevel::VV);
    (client, capture, observations)
}

fn assert_logical_url(url: &url::Url, expected_path: &str) {
    assert_eq!(url.scheme(), "http");
    assert_eq!(url.host_str(), Some("example.com"));
    assert_eq!(url.path(), expected_path);
    assert_eq!(url.fragment(), None);
    let pairs = url
        .query_pairs()
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    assert_eq!(
        pairs,
        vec![(PUBLIC_NAME.to_string(), PUBLIC_VALUE.to_string())]
    );
}

fn assert_logical_url_text(url: &str, expected_path: &str) {
    let url = url::Url::parse(url).expect("logical observation URL");
    assert_logical_url(&url, expected_path);
}

fn assert_observations_safe(observations: &UrlObservations, expected_path: &str) {
    for urls in [
        &observations.pre_send,
        &observations.post_response,
        &observations.request_error,
        &observations.rate_acquire,
        &observations.rate_response,
        &observations.debug_request,
        &observations.debug_response,
    ] {
        for url in urls {
            assert_logical_url_text(url, expected_path);
        }
    }
    for diagnostic in &observations.diagnostics {
        assert!(!diagnostic.contains(QUERY_SENTINEL), "{diagnostic}");
    }
}

#[cfg(feature = "dangerous-dev-tools")]
async fn assert_development_captures_safe(
    capture: &NativeMockHarness,
    expected_count: usize,
    expected_path: &str,
) {
    let requests = capture.requests().await;
    assert_eq!(requests.len(), expected_count);
    for request in requests {
        assert_logical_url(&request.url, expected_path);
        assert!(!format!("{request:?}").contains(QUERY_SENTINEL));
        assert!(!request.headers.values().any(|value| {
            value
                .to_str()
                .is_ok_and(|value| value.contains(QUERY_SENTINEL))
        }));
    }
}

#[tokio::test]
async fn buffered_response_status_hooks_rate_limit_and_debug_use_logical_url()
-> Result<(), ApiClientError> {
    let (client, capture, observations) = configured_client(vec![
        expected_text_reply(StatusCode::OK, b"decoded"),
        expected_text_reply(StatusCode::INTERNAL_SERVER_ERROR, b"terminal"),
    ]);

    let decoded = client.request(text_endpoint()).response().await?;
    assert_eq!(decoded.value(), "decoded");
    assert_logical_url(decoded.url(), "/text");
    assert!(!format!("{decoded:?}").contains(QUERY_SENTINEL));

    let error = client
        .request(text_endpoint())
        .response()
        .await
        .expect_err("500 response must remain terminal");
    assert!(matches!(
        error,
        ApiClientError::HttpStatus {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            ..
        }
    ));
    crate::support::assert_error_chain_does_not_contain_any(&error, &[QUERY_SENTINEL]);

    {
        let observations = observations.lock().expect("URL observations lock");
        assert_eq!(observations.pre_send.len(), 2);
        assert_eq!(observations.post_response.len(), 2);
        assert_eq!(observations.rate_acquire.len(), 2);
        assert_eq!(observations.rate_response.len(), 2);
        assert_eq!(observations.debug_request.len(), 2);
        assert_eq!(observations.debug_response.len(), 2);
        assert_observations_safe(&observations, "/text");
    }

    #[cfg(feature = "dangerous-dev-tools")]
    assert_development_captures_safe(&capture, 2, "/text").await;
    #[cfg(not(feature = "dangerous-dev-tools"))]
    let _ = capture;
    Ok(())
}

#[cfg(feature = "dangerous-raw-response")]
#[tokio::test]
async fn feature_gated_built_response_keeps_logical_url() -> Result<(), ApiClientError> {
    let (client, capture, observations) =
        configured_client(vec![expected_text_reply(StatusCode::OK, b"raw")]);
    let response = client
        .request(text_endpoint())
        .execute_raw_response()
        .await?;
    assert_logical_url(response.url(), "/text");
    assert!(!format!("{response:?}").contains(QUERY_SENTINEL));
    assert_eq!(response.body().as_ref(), b"raw");
    assert_observations_safe(
        &observations.lock().expect("URL observations lock"),
        "/text",
    );
    #[cfg(feature = "dangerous-dev-tools")]
    assert_development_captures_safe(&capture, 1, "/text").await;
    #[cfg(not(feature = "dangerous-dev-tools"))]
    let _ = capture;
    Ok(())
}

#[derive(Clone)]
struct QueryStreamEndpoint;

impl RegressionEndpoint<TestCx> for QueryStreamEndpoint {
    type Response = StreamResponse<OctetStream>;

    fn execute<'a>(
        client: &'a ApiClient<TestCx>,
        plan: RequestPlan,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Response, ApiClientError>> + Send + 'a>> {
        <RawStreamResponse<OctetStream> as ResponseEntity>::execute(client, plan)
    }
}

impl RegressionReusableEndpoint<TestCx> for QueryStreamEndpoint {
    fn plan(
        &self,
        _context: &RegressionPlanContext<'_, TestCx>,
    ) -> Result<RequestPlan, ApiClientError> {
        Ok(RequestPlan {
            endpoint: EndpointPlan {
                meta: EndpointMeta {
                    name: "SafeLogicalStream",
                    method: Method::GET,
                    idempotent: true,
                    facade_path: &[],
                },
                route: ResolvedRoute::new(http::uri::Scheme::HTTP, "example.com", "/stream"),
                policy: query_policy(),
                response: ResponsePlan {
                    accept: Some(HeaderValue::from_static("application/octet-stream")),
                    no_content: false,
                    format: crate::regression_tests::test_api::Format::Binary,
                },
                pagination: None,
            },
            body: PreparedBody::empty(),
            overrides: RequestOverrides::default(),
        })
    }
}

#[tokio::test]
async fn streaming_response_url_and_debug_use_logical_url() -> Result<(), ApiClientError> {
    let reply = NativeMockReply::status(StatusCode::OK)
        .with_header(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/octet-stream"),
        )
        .with_body(Bytes::from_static(b"stream"))
        .expect_query_pair(QUERY_NAME, QUERY_SENTINEL);
    let (client, capture, observations) = configured_client(vec![reply]);
    let mut response = client.request(QueryStreamEndpoint).execute_stream().await?;
    assert_logical_url(response.url(), "/stream");
    assert!(!format!("{response:?}").contains(QUERY_SENTINEL));
    assert_eq!(
        response.next_chunk().await?.as_deref(),
        Some(&b"stream"[..])
    );
    assert_observations_safe(
        &observations.lock().expect("URL observations lock"),
        "/stream",
    );
    #[cfg(feature = "dangerous-dev-tools")]
    assert_development_captures_safe(&capture, 1, "/stream").await;
    #[cfg(not(feature = "dangerous-dev-tools"))]
    let _ = capture;
    Ok(())
}

#[tokio::test]
async fn request_error_hook_and_source_chain_use_logical_url() {
    let reply =
        NativeMockReply::disconnect_after_request().expect_query_pair(QUERY_NAME, QUERY_SENTINEL);
    let (client, capture, observations) = configured_client(vec![reply]);
    let error = client
        .request(text_endpoint())
        .response()
        .await
        .expect_err("disconnect must be terminal");
    crate::support::assert_error_chain_does_not_contain_any(&error, &[QUERY_SENTINEL]);
    {
        let observations = observations.lock().expect("URL observations lock");
        assert_eq!(observations.pre_send.len(), 1);
        assert_eq!(observations.request_error.len(), 1);
        assert_eq!(observations.rate_acquire.len(), 1);
        assert!(observations.post_response.is_empty());
        assert!(observations.rate_response.is_empty());
        assert_observations_safe(&observations, "/text");
    }
    #[cfg(feature = "dangerous-dev-tools")]
    assert_development_captures_safe(&capture, 1, "/text").await;
    #[cfg(not(feature = "dangerous-dev-tools"))]
    let _ = capture;
}

#[tokio::test]
async fn authentication_recovery_rebuilds_native_query_but_second_challenge_is_terminal() {
    let replies = vec![
        expected_text_reply(StatusCode::UNAUTHORIZED, b"first challenge"),
        expected_text_reply(StatusCode::UNAUTHORIZED, b"second challenge"),
    ];
    let (client, capture, observations) = configured_client_with_identity(replies, "refresh");
    let error = client
        .request(text_endpoint())
        .response()
        .await
        .expect_err("the second challenged response must be terminal");
    assert!(matches!(error, ApiClientError::Auth { .. }));
    crate::support::assert_error_chain_does_not_contain_any(&error, &[QUERY_SENTINEL]);
    assert_eq!(
        capture.sent_count().await,
        2,
        "there must be no third execution"
    );

    {
        let observations = observations.lock().expect("URL observations lock");
        assert_eq!(observations.pre_send.len(), 2);
        assert_eq!(observations.post_response.len(), 2);
        assert_eq!(observations.rate_acquire.len(), 2);
        assert_eq!(observations.rate_response.len(), 2);
        assert_observations_safe(&observations, "/text");
    }
    #[cfg(feature = "dangerous-dev-tools")]
    assert_development_captures_safe(&capture, 2, "/text").await;
}

fn assert_pagination_url(url: &url::Url, expected_offset: &str) {
    assert_eq!(url.scheme(), "http");
    assert_eq!(url.host_str(), Some("example.com"));
    assert_eq!(url.path(), "/items");
    let pairs = url
        .query_pairs()
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(
        pairs.get(PUBLIC_NAME).map(String::as_str),
        Some(PUBLIC_VALUE)
    );
    assert_eq!(
        pairs.get("offset").map(String::as_str),
        Some(expected_offset)
    );
    assert_eq!(pairs.get("limit").map(String::as_str), Some("2"));
    assert!(!pairs.contains_key(QUERY_NAME));
    assert_eq!(pairs.len(), 3);
}

#[tokio::test]
async fn pagination_observations_state_and_later_page_error_keep_logical_urls()
-> Result<(), ApiClientError> {
    let replies = vec![
        expected_text_reply(StatusCode::OK, b"a,b"),
        expected_text_reply(StatusCode::INTERNAL_SERVER_ERROR, b"later page"),
    ];
    let (client, capture, observations) = configured_client(replies);
    let endpoint = ItemsEndpoint {
        start: 0,
        count: 2,
        policy: query_policy(),
        pagination: PaginationVariant::OffsetLimit {
            offset: 0,
            limit: 2,
        },
    };
    let error = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(10))
        .collect()
        .await
        .expect_err("later page HTTP failure must remain terminal");
    assert!(matches!(
        error,
        ApiClientError::HttpStatus {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            ..
        }
    ));
    crate::support::assert_error_chain_does_not_contain_any(&error, &[QUERY_SENTINEL]);

    {
        let observations = observations.lock().expect("URL observations lock");
        for urls in [
            &observations.pre_send,
            &observations.post_response,
            &observations.rate_acquire,
            &observations.rate_response,
            &observations.debug_request,
            &observations.debug_response,
        ] {
            assert_eq!(urls.len(), 2);
            assert_pagination_url(&url::Url::parse(&urls[0]).expect("first page URL"), "0");
            assert_pagination_url(&url::Url::parse(&urls[1]).expect("later page URL"), "2");
        }
        assert!(
            observations
                .diagnostics
                .iter()
                .all(|diagnostic| !diagnostic.contains(QUERY_SENTINEL))
        );
    }

    #[cfg(feature = "dangerous-dev-tools")]
    {
        let requests = capture.requests().await;
        assert_eq!(requests.len(), 2);
        assert_pagination_url(&requests[0].url, "0");
        assert_pagination_url(&requests[1].url, "2");
        for request in requests {
            assert!(!format!("{request:?}").contains(QUERY_SENTINEL));
        }
    }
    #[cfg(not(feature = "dangerous-dev-tools"))]
    let _ = capture;
    Ok(())
}
