use bytes::Bytes;
use concord_core::advanced::{
    DebugSink, RateLimitContext, RateLimitFuture, RateLimitPermit, RateLimitResponseAction,
    RateLimitResponseContext, RateLimiter, SanitizedHeaders,
};
use concord_core::prelude::{ApiClient, ApiClientError, DebugLevel};
use concord_macros::api;
use concord_test_support::{ScriptedReply, deterministic_mock};
use http::{Method, StatusCode};
use std::future::Future;
use std::pin::Pin;
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

    fn request_error<'a>(
        &'a self,
        ctx: concord_core::advanced::RequestErrorHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        let events = self.events.clone();
        Box::pin(async move {
            events
                .lock()
                .expect("hooks lock")
                .push(format!("hook_request_error:{}", ctx.meta.endpoint));
        })
    }
}

#[tokio::test]
async fn no_content_endpoint_omits_accept_and_returns_unit() -> Result<(), ApiClientError> {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let (server, handle) = deterministic_mock()
        .reply(
            ScriptedReply::status(StatusCode::OK)
                .with_body(Bytes::from_static(b"SECRET_NO_CONTENT_SENTINEL")),
        )
        .build();
    let mut client = ApiClient::<NoContentHelperApiCx>::with_safe_reqwest_builder(
        NoContentHelperApiVars::new(),
        NoContentHelperApiAuthVars::new(),
        |builder| server.configure_application(builder),
    )
    .expect("deterministic no-content client");
    client.set_debug_sink(Arc::new(RecordingDebugSink::new(events.clone())));
    client.set_runtime_hooks(Arc::new(RecordingHooks::new(events.clone())));
    client.configure(|cfg| {
        cfg.rate_limiter(Arc::new(RecordingRateLimiter::new(events.clone())));
        cfg.debug(DebugLevel::V);
    });

    let value: () = client.request(endpoints::Ping::new()).execute().await?;
    assert_eq!(value, ());

    let requests = handle.recorded();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0]
            .headers
            .get(http::header::ACCEPT)
            .and_then(|value| value.to_str().ok()),
        Some("*/*")
    );
    assert_eq!(requests[0].headers.get(http::header::CONTENT_TYPE), None);
    assert_eq!(
        requests[0].body_category,
        concord_core::__development::CapturedBodyCategory::Empty
    );

    let events = events.lock().expect("events lock").clone();
    let rate_limit_idx = events
        .iter()
        .position(|event| event == "rate_limit_acquire")
        .expect("rate limit event");
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
    assert!(rate_limit_idx < post_response_idx);
    assert!(pre_send_idx < post_response_idx);
    assert!(post_response_idx < rate_limit_response_idx);
    handle.finish();
    Ok(())
}

#[tokio::test]
async fn no_content_status_failure_is_body_free() {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let sentinel = "SECRET_NO_CONTENT_SENTINEL";
    let (server, handle) = deterministic_mock()
        .reply(
            ScriptedReply::status(StatusCode::INTERNAL_SERVER_ERROR)
                .with_body(Bytes::copy_from_slice(sentinel.as_bytes())),
        )
        .build();
    let mut client = ApiClient::<NoContentHelperApiCx>::with_safe_reqwest_builder(
        NoContentHelperApiVars::new(),
        NoContentHelperApiAuthVars::new(),
        |builder| server.configure_application(builder),
    )
    .expect("deterministic no-content client");
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
    assert_eq!(handle.recorded_len(), 1);
    handle.finish();
}
