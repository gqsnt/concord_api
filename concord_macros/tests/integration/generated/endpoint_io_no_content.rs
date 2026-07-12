use bytes::Bytes;
use concord_core::advanced::{
    DebugSink, DynBody, RateLimitContext, RateLimitFuture, RateLimitPermit,
    RateLimitResponseAction, RateLimitResponseContext, RateLimiter, SanitizedHeaders, Transport,
    TransportError,
};
use concord_core::prelude::{ApiClient, ApiClientError, DebugLevel};
use concord_macros::api;
use http::{Method, StatusCode};
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

mod no_content_helper_contract {
    #![allow(unused_imports)]
    use super::*;

    api! {
        client NoContentHelperApi {
            base "https://example.com"
        }

        GET Ping
            path ["ping"]
            -> NoContent
    }

    pub(super) use no_content_helper_api::NoContentHelperApiAuthVars;
    pub(super) use no_content_helper_api::NoContentHelperApiCx;
    pub(super) use no_content_helper_api::NoContentHelperApiVars;
    pub(super) use no_content_helper_api::endpoints;
}

use no_content_helper_contract::NoContentHelperApiCx;
use no_content_helper_contract::endpoints;
use no_content_helper_contract::{NoContentHelperApiAuthVars, NoContentHelperApiVars};

#[derive(Clone)]
struct RecordingDebugSink {
    events: Arc<StdMutex<Vec<String>>>,
}

impl RecordingDebugSink {
    fn new(events: Arc<StdMutex<Vec<String>>>) -> Self {
        Self { events }
    }
}

impl DebugSink for RecordingDebugSink {
    fn request_start(
        &self,
        dbg: DebugLevel,
        _method: &Method,
        _url: &str,
        endpoint: &'static str,
        page_index: u32,
    ) {
        self.events
            .lock()
            .expect("debug lock")
            .push(format!("debug_request:{dbg}:{endpoint}:{page_index}"));
    }

    fn request_headers(&self, dbg: DebugLevel, _headers: SanitizedHeaders<'_>) {
        self.events
            .lock()
            .expect("debug lock")
            .push(format!("debug_request_headers:{dbg}"));
    }

    fn response_status(&self, dbg: DebugLevel, status: StatusCode, _url: &str, ok: bool) {
        self.events
            .lock()
            .expect("debug lock")
            .push(format!("debug_response:{dbg}:{status}:{ok}"));
    }

    fn response_headers(&self, dbg: DebugLevel, _headers: SanitizedHeaders<'_>) {
        self.events
            .lock()
            .expect("debug lock")
            .push(format!("debug_response_headers:{dbg}"));
    }
}

#[derive(Clone)]
struct RecordingRateLimiter {
    events: Arc<StdMutex<Vec<String>>>,
}

impl RecordingRateLimiter {
    fn new(events: Arc<StdMutex<Vec<String>>>) -> Self {
        Self { events }
    }
}

impl RateLimiter for RecordingRateLimiter {
    fn acquire<'a>(
        &'a self,
        _ctx: RateLimitContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitPermit, ApiClientError>> {
        let events = self.events.clone();
        Box::pin(async move {
            events
                .lock()
                .expect("rate limit lock")
                .push("rate_limit_acquire".to_string());
            Ok(RateLimitPermit)
        })
    }

    fn on_response<'a>(
        &'a self,
        _ctx: RateLimitResponseContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitResponseAction, ApiClientError>> {
        let events = self.events.clone();
        Box::pin(async move {
            events
                .lock()
                .expect("rate limit lock")
                .push("rate_limit_response".to_string());
            Ok(RateLimitResponseAction::Continue)
        })
    }
}

#[derive(Clone)]
struct RecordingHooks {
    events: Arc<StdMutex<Vec<String>>>,
}

impl RecordingHooks {
    fn new(events: Arc<StdMutex<Vec<String>>>) -> Self {
        Self { events }
    }
}

impl concord_core::advanced::RuntimeHooks for RecordingHooks {
    fn pre_send<'a>(
        &'a self,
        ctx: concord_core::advanced::PreSendHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ApiClientError>> + Send + 'a>> {
        let events = self.events.clone();
        Box::pin(async move {
            events
                .lock()
                .expect("hooks lock")
                .push(format!("hook_pre_send:{}", ctx.meta.endpoint));
            Ok(())
        })
    }

    fn post_response<'a>(
        &'a self,
        ctx: concord_core::advanced::PostResponseHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        let events = self.events.clone();
        Box::pin(async move {
            events
                .lock()
                .expect("hooks lock")
                .push(format!("hook_post_response:{}", ctx.meta.endpoint));
        })
    }

    fn transport_error<'a>(
        &'a self,
        ctx: concord_core::advanced::TransportErrorHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        let events = self.events.clone();
        Box::pin(async move {
            events
                .lock()
                .expect("hooks lock")
                .push(format!("hook_transport_error:{}", ctx.meta.endpoint));
        })
    }
}

#[derive(Clone)]
struct FlagBody {
    chunks: VecDeque<Bytes>,
    polled: Arc<AtomicBool>,
}

impl futures_core::Stream for FlagBody {
    type Item = Result<Bytes, concord_core::advanced::BodyError>;
    fn poll_next(
        mut self: Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.polled.store(true, Ordering::SeqCst);
        std::task::Poll::Ready(self.chunks.pop_front().map(Ok))
    }
}

#[derive(Clone)]
struct NoContentTransport {
    events: Arc<StdMutex<Vec<String>>>,
    requests: Arc<StdMutex<Vec<CapturedRequest>>>,
    responses: Arc<StdMutex<VecDeque<ResponseFixture>>>,
    send_count: Arc<AtomicUsize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CapturedRequest {
    accept: Option<String>,
    content_type: Option<String>,
    body_empty: bool,
}

#[derive(Clone)]
struct ResponseFixture {
    status: StatusCode,
    body: VecDeque<Bytes>,
    poll_flag: Arc<AtomicBool>,
}

impl ResponseFixture {
    fn ok(body: &'static [u8], poll_flag: Arc<AtomicBool>) -> Self {
        Self {
            status: StatusCode::OK,
            body: VecDeque::from(vec![Bytes::copy_from_slice(body)]),
            poll_flag,
        }
    }

    fn status(status: StatusCode, body: &'static [u8], poll_flag: Arc<AtomicBool>) -> Self {
        Self {
            status,
            body: VecDeque::from(vec![Bytes::copy_from_slice(body)]),
            poll_flag,
        }
    }
}

impl NoContentTransport {
    fn new(events: Arc<StdMutex<Vec<String>>>, responses: Vec<ResponseFixture>) -> Self {
        Self {
            events,
            requests: Arc::new(StdMutex::new(Vec::new())),
            responses: Arc::new(StdMutex::new(VecDeque::from(responses))),
            send_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn requests(&self) -> Vec<CapturedRequest> {
        self.requests.lock().expect("requests lock").clone()
    }

    fn send_count(&self) -> usize {
        self.send_count.load(Ordering::SeqCst)
    }

    fn events(&self) -> Vec<String> {
        self.events.lock().expect("events lock").clone()
    }
}

impl Transport for NoContentTransport {
    fn send(
        &self,
        req: http::Request<DynBody>,
    ) -> Pin<Box<dyn Future<Output = Result<http::Response<DynBody>, TransportError>> + Send>> {
        let transport = self.clone();
        Box::pin(async move {
            transport
                .events
                .lock()
                .expect("events lock")
                .push("transport".to_string());
            transport.send_count.fetch_add(1, Ordering::SeqCst);
            let accept = req
                .headers()
                .get(http::header::ACCEPT)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);
            let content_type = req
                .headers()
                .get(http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);
            let body_empty = http_body::Body::size_hint(req.body()).exact() == Some(0);
            transport
                .requests
                .lock()
                .expect("requests lock")
                .push(CapturedRequest {
                    accept,
                    content_type,
                    body_empty,
                });

            let response = transport
                .responses
                .lock()
                .expect("responses lock")
                .pop_front()
                .expect("response fixture");
            let mut result = http::Response::new(DynBody::from_byte_stream(FlagBody {
                chunks: response.body,
                polled: response.poll_flag,
            }));
            *result.status_mut() = response.status;
            Ok(result)
        })
    }
}

#[tokio::test]
async fn no_content_endpoint_omits_accept_and_returns_unit() -> Result<(), ApiClientError> {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let polled = Arc::new(AtomicBool::new(false));
    let transport = NoContentTransport::new(
        events.clone(),
        vec![ResponseFixture::ok(
            b"SECRET_NO_CONTENT_SENTINEL",
            polled.clone(),
        )],
    );
    let mut client = ApiClient::<NoContentHelperApiCx, _>::with_transport(
        NoContentHelperApiVars::new(),
        NoContentHelperApiAuthVars::new(),
        transport.clone(),
    );
    client.set_debug_sink(Arc::new(RecordingDebugSink::new(events.clone())));
    client.set_runtime_hooks(Arc::new(RecordingHooks::new(events.clone())));
    client.configure(|cfg| {
        cfg.rate_limiter(Arc::new(RecordingRateLimiter::new(events.clone())));
        cfg.debug(DebugLevel::V);
    });

    let value: () = client.request(endpoints::Ping::new()).execute().await?;
    assert_eq!(value, ());

    let requests = transport.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].accept, None);
    assert_eq!(requests[0].content_type, None);
    assert!(requests[0].body_empty);
    assert!(!polled.load(Ordering::SeqCst));

    let events = transport.events();
    let rate_limit_idx = events
        .iter()
        .position(|event| event == "rate_limit_acquire")
        .expect("rate limit event");
    let transport_idx = events
        .iter()
        .position(|event| event == "transport")
        .expect("transport event");
    let pre_send_idx = events
        .iter()
        .position(|event| event == "hook_pre_send:Ping")
        .expect("pre-send event");
    let post_response_idx = events
        .iter()
        .position(|event| event == "hook_post_response:Ping")
        .expect("post-response event");
    let rate_limit_response_idx = events
        .iter()
        .position(|event| event == "rate_limit_response")
        .expect("rate-limit response event");
    assert!(rate_limit_idx < transport_idx);
    assert!(pre_send_idx < transport_idx);
    assert!(transport_idx < post_response_idx);
    assert!(post_response_idx < rate_limit_response_idx);
    assert_eq!(transport.send_count(), 1);
    Ok(())
}

#[tokio::test]
async fn no_content_status_failure_is_body_free() {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let polled = Arc::new(AtomicBool::new(false));
    let sentinel = "SECRET_NO_CONTENT_SENTINEL";
    let transport = NoContentTransport::new(
        events.clone(),
        vec![ResponseFixture::status(
            StatusCode::INTERNAL_SERVER_ERROR,
            sentinel.as_bytes(),
            polled.clone(),
        )],
    );
    let mut client = ApiClient::<NoContentHelperApiCx, _>::with_transport(
        NoContentHelperApiVars::new(),
        NoContentHelperApiAuthVars::new(),
        transport.clone(),
    );
    client.set_runtime_hooks(Arc::new(RecordingHooks::new(events.clone())));
    client.configure(|cfg| {
        cfg.rate_limiter(Arc::new(RecordingRateLimiter::new(events.clone())));
    });

    let err = client
        .request(endpoints::Ping::new())
        .execute()
        .await
        .expect_err("status failure should error");
    assert!(matches!(err, ApiClientError::HttpStatus { .. }));
    assert!(!format!("{err:?}").contains(sentinel));
    assert!(!format!("{err}").contains(sentinel));
    assert!(!polled.load(Ordering::SeqCst));
    assert_eq!(transport.send_count(), 1);
}
