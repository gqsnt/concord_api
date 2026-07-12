#![allow(unused_imports)]

use concord_core::advanced::{
    DebugSink, DynBody, PostResponseHookContext, PreSendHookContext, RateLimitContext,
    RateLimitFuture, RateLimitPermit, RateLimitResponseAction, RateLimitResponseContext,
    RateLimiter, RequestExecutionContext, RuntimeHooks, SanitizedHeaders, Transport,
    TransportError,
};
use concord_core::prelude::*;
use concord_macros::api;
use http::{HeaderMap, StatusCode};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex as StdMutex};

use self::url_hardening_api::UrlHardeningApi;

api! {
    client UrlHardeningApi {
        base "https://example.com"
        var tenant: String

        policies {
            rate_limit app {
                bucket application by [host] {
                    100 / 1s
                }
            }
        }

        default {
            rate_limit app
        }
    }

    scope regional(tenant_id: String) {
        host [tenant_id, "api"]
        path ["v1"]

        GET Show(id: String, prefix: String)
            as show
            path ["items", id, fmt["p-", prefix]]
            -> Json<String>

        GET FmtOnly(value: String)
            as fmt_only
            path ["fmt", fmt[value]]
            -> Json<String>
    }
}

#[derive(Clone, Default)]
struct RecordingEvents {
    events: Arc<StdMutex<Vec<String>>>,
}

impl RecordingEvents {
    fn push(&self, event: impl Into<String>) {
        self.events
            .lock()
            .expect("recording events lock")
            .push(event.into());
    }

    fn snapshot(&self) -> Vec<String> {
        self.events.lock().expect("recording events lock").clone()
    }
}

struct RecordedRequest {
    meta: concord_core::transport::RequestMeta,
    url: url::Url,
    headers: http::HeaderMap,
    timeout: Option<std::time::Duration>,
}

#[derive(Clone)]
struct RecordingTransport {
    records: RecordingEvents,
    requests: Arc<StdMutex<Vec<RecordedRequest>>>,
}

impl RecordingTransport {
    fn new(records: RecordingEvents) -> Self {
        Self {
            records,
            requests: Arc::new(StdMutex::new(Vec::new())),
        }
    }

    fn requests(&self) -> Vec<RecordedRequest> {
        std::mem::take(&mut *self.requests.lock().expect("transport requests lock"))
    }
}

impl Transport for RecordingTransport {
    fn send(
        &self,
        req: http::Request<DynBody>,
    ) -> Pin<Box<dyn Future<Output = Result<http::Response<DynBody>, TransportError>> + Send>> {
        let records = self.records.clone();
        let requests = self.requests.clone();
        Box::pin(async move {
            let (parts, _body) = req.into_parts();
            let context = parts
                .extensions
                .get::<RequestExecutionContext>()
                .cloned()
                .expect("context");
            let url: url::Url = parts.uri.to_string().parse().expect("URL");
            records.push(format!("transport:{}", url.as_str()));
            requests
                .lock()
                .expect("transport requests lock")
                .push(RecordedRequest {
                    meta: context.meta,
                    url: url.clone(),
                    headers: parts.headers,
                    timeout: context.timeout,
                });
            Ok(http::Response::new(DynBody::from_bytes(
                bytes::Bytes::from_static(b"\"ok\""),
            )))
        })
    }
}

#[derive(Clone)]
struct RecordingRateLimiter {
    records: RecordingEvents,
}

impl RecordingRateLimiter {
    fn new(records: RecordingEvents) -> Self {
        Self { records }
    }
}

impl RateLimiter for RecordingRateLimiter {
    fn acquire<'a>(
        &'a self,
        ctx: RateLimitContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitPermit, ApiClientError>> {
        let records = self.records.clone();
        let url = ctx.url.to_string();
        let host = ctx
            .url_host
            .map(str::to_string)
            .unwrap_or_else(|| "<none>".to_string());
        Box::pin(async move {
            records.push(format!("rate_acquire:{url}:{host}"));
            Ok(RateLimitPermit)
        })
    }

    fn on_response<'a>(
        &'a self,
        ctx: RateLimitResponseContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitResponseAction, ApiClientError>> {
        let records = self.records.clone();
        let url = ctx.meta.url.to_string();
        let host = ctx
            .meta
            .url_host
            .map(str::to_string)
            .unwrap_or_else(|| "<none>".to_string());
        Box::pin(async move {
            records.push(format!("rate_response:{url}:{host}:{}", ctx.status));
            Ok(RateLimitResponseAction::Continue)
        })
    }
}

#[derive(Clone)]
struct RecordingHooks {
    records: RecordingEvents,
}

impl RecordingHooks {
    fn new(records: RecordingEvents) -> Self {
        Self { records }
    }
}

impl RuntimeHooks for RecordingHooks {
    fn pre_send<'a>(
        &'a self,
        ctx: PreSendHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ApiClientError>> + Send + 'a>> {
        let records = self.records.clone();
        let url = ctx.meta.url.to_string();
        Box::pin(async move {
            records.push(format!("hook_pre:{url}"));
            Ok(())
        })
    }

    fn post_response<'a>(
        &'a self,
        ctx: PostResponseHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        let records = self.records.clone();
        let url = ctx.meta.url.to_string();
        Box::pin(async move {
            records.push(format!("hook_post:{url}"));
        })
    }

    fn transport_error<'a>(
        &'a self,
        ctx: concord_core::advanced::TransportErrorHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        let records = self.records.clone();
        let url = ctx.meta.url.to_string();
        Box::pin(async move {
            records.push(format!("hook_transport:{url}"));
        })
    }
}

#[derive(Clone)]
struct RecordingDebugSink {
    records: RecordingEvents,
}

impl RecordingDebugSink {
    fn new(records: RecordingEvents) -> Self {
        Self { records }
    }
}

impl DebugSink for RecordingDebugSink {
    fn request_start(
        &self,
        _dbg: DebugLevel,
        _method: &http::Method,
        url: &str,
        _endpoint: &'static str,
        _page_index: u32,
    ) {
        self.records.push(format!("debug_start:{url}"));
    }

    fn request_headers(&self, _dbg: DebugLevel, _headers: SanitizedHeaders<'_>) {}

    fn response_status(&self, _dbg: DebugLevel, _status: StatusCode, url: &str, _ok: bool) {
        self.records.push(format!("debug_status:{url}"));
    }

    fn response_headers(&self, _dbg: DebugLevel, _headers: SanitizedHeaders<'_>) {}
}

fn configure_client(
    client: UrlHardeningApi<RecordingTransport>,
    rate_limiter: Arc<dyn RateLimiter>,
    hooks: Arc<dyn RuntimeHooks>,
    debug_sink: Arc<dyn DebugSink>,
) -> UrlHardeningApi<RecordingTransport> {
    client.configure(|cfg| {
        cfg.rate_limiter(rate_limiter);
        cfg.runtime_hooks(hooks);
        cfg.debug_sink(debug_sink);
        cfg.debug_level(DebugLevel::V);
    })
}

#[tokio::test]
async fn dynamic_host_accepts_valid_labels_deterministically() -> Result<(), ApiClientError> {
    for (tenant, expected_host) in [
        ("tenant", "tenant.api.example.com"),
        ("tenant-1", "tenant-1.api.example.com"),
    ] {
        let records = RecordingEvents::default();
        let transport = RecordingTransport::new(records.clone());
        let rate_limiter = Arc::new(RecordingRateLimiter::new(records.clone()));
        let hooks = Arc::new(RecordingHooks::new(records.clone()));
        let debug_sink = Arc::new(RecordingDebugSink::new(records.clone()));
        let client = configure_client(
            UrlHardeningApi::new_with_transport("client".to_string(), transport.clone()),
            rate_limiter,
            hooks,
            debug_sink,
        );

        let decoded = client
            .regional(tenant.to_string())
            .show("item".to_string(), "prefix".to_string())
            .await?;

        assert_eq!(decoded, "ok");
        let requests = transport.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].url.host_str(), Some(expected_host));
        assert_eq!(
            requests[0].url.as_str(),
            format!("https://{expected_host}/v1/items/item/p-prefix")
        );

        let events = records.snapshot();
        assert!(events.iter().any(|event| event
            == &format!(
                "rate_acquire:https://{expected_host}/v1/items/item/p-prefix:{expected_host}"
            )));
        assert!(
            events.iter().any(|event| event
                == &format!("hook_pre:https://{expected_host}/v1/items/item/p-prefix"))
        );
        assert!(
            events.iter().any(|event| event
                == &format!("debug_start:https://{expected_host}/v1/items/item/p-prefix"))
        );
        assert!(
            events.iter().any(|event| event
                == &format!("transport:https://{expected_host}/v1/items/item/p-prefix"))
        );
        assert!(
            !events
                .iter()
                .any(|event| event.contains("<unknown>") || event.contains("unknown-host"))
        );
    }

    Ok(())
}

#[tokio::test]
async fn dynamic_host_rejects_dangerous_values() {
    for bad in [
        "evil.com/path",
        "evil.com\\path",
        "user@evil.com",
        "http://evil.com",
        "evil.com?x=1",
        "evil.com#frag",
        " bad.com",
        "bad.com ",
        "bad..com",
        "-bad.com",
        "bad-.com",
        "api.tenant",
        "",
    ] {
        let records = RecordingEvents::default();
        let transport = RecordingTransport::new(records.clone());
        let rate_limiter = Arc::new(RecordingRateLimiter::new(records.clone()));
        let hooks = Arc::new(RecordingHooks::new(records.clone()));
        let debug_sink = Arc::new(RecordingDebugSink::new(records.clone()));
        let client = configure_client(
            UrlHardeningApi::new_with_transport("client".to_string(), transport.clone()),
            rate_limiter,
            hooks,
            debug_sink,
        );

        let err = client
            .regional(bad.to_string())
            .show("item".to_string(), "prefix".to_string())
            .await
            .expect_err("invalid host label should fail before side effects");

        assert_eq!(err.category(), concord_core::error::ErrorCategory::Config);
        assert_eq!(err.context().endpoint, "regional::Show");
        assert_eq!(err.context().method, http::Method::GET);
        assert!(err.to_string().contains("invalid host label"));
        assert!(transport.requests().is_empty());
        assert!(records.snapshot().is_empty());
    }
}

#[tokio::test]
async fn dynamic_path_slash_backslash_rejected_before_side_effects() {
    for bad in ["a/b", "a\\b"] {
        let records = RecordingEvents::default();
        let transport = RecordingTransport::new(records.clone());
        let rate_limiter = Arc::new(RecordingRateLimiter::new(records.clone()));
        let hooks = Arc::new(RecordingHooks::new(records.clone()));
        let debug_sink = Arc::new(RecordingDebugSink::new(records.clone()));
        let client = configure_client(
            UrlHardeningApi::new_with_transport("client".to_string(), transport.clone()),
            rate_limiter,
            hooks,
            debug_sink,
        );

        let err = client
            .regional("tenant".to_string())
            .show(bad.to_string(), "prefix".to_string())
            .await
            .expect_err("invalid path segment should fail before side effects");

        assert_eq!(err.category(), concord_core::error::ErrorCategory::Config);
        assert_eq!(err.context().endpoint, "regional::Show");
        assert_eq!(err.context().method, http::Method::GET);
        assert!(err.to_string().contains("invalid/missing param"));
        assert!(transport.requests().is_empty());
        assert!(records.snapshot().is_empty());
    }
}

#[tokio::test]
async fn dynamic_path_dot_segments_are_safe() {
    for bad in [".", "..", "a/../b"] {
        let records = RecordingEvents::default();
        let transport = RecordingTransport::new(records.clone());
        let rate_limiter = Arc::new(RecordingRateLimiter::new(records.clone()));
        let hooks = Arc::new(RecordingHooks::new(records.clone()));
        let debug_sink = Arc::new(RecordingDebugSink::new(records.clone()));
        let client = configure_client(
            UrlHardeningApi::new_with_transport("client".to_string(), transport.clone()),
            rate_limiter,
            hooks,
            debug_sink,
        );

        let err = client
            .regional("tenant".to_string())
            .show(bad.to_string(), "prefix".to_string())
            .await
            .expect_err("dot segments should be rejected before side effects");

        assert_eq!(err.category(), concord_core::error::ErrorCategory::Config);
        assert_eq!(err.context().endpoint, "regional::Show");
        assert_eq!(err.context().method, http::Method::GET);
        assert!(err.to_string().contains("invalid/missing param"));
        assert!(transport.requests().is_empty());
        assert!(records.snapshot().is_empty());
    }
}

#[tokio::test]
async fn fmt_path_interpolation_follows_dynamic_path_safety() {
    for bad in ["a/b", "a\\b"] {
        let records = RecordingEvents::default();
        let transport = RecordingTransport::new(records.clone());
        let rate_limiter = Arc::new(RecordingRateLimiter::new(records.clone()));
        let hooks = Arc::new(RecordingHooks::new(records.clone()));
        let debug_sink = Arc::new(RecordingDebugSink::new(records.clone()));
        let client = configure_client(
            UrlHardeningApi::new_with_transport("client".to_string(), transport.clone()),
            rate_limiter,
            hooks,
            debug_sink,
        );

        let err = client
            .regional("tenant".to_string())
            .show("item".to_string(), bad.to_string())
            .await
            .expect_err("invalid fmt segment should fail before side effects");

        assert_eq!(err.category(), concord_core::error::ErrorCategory::Config);
        assert_eq!(err.context().endpoint, "regional::Show");
        assert_eq!(err.context().method, http::Method::GET);
        assert!(err.to_string().contains("invalid/missing param"));
        assert!(transport.requests().is_empty());
        assert!(records.snapshot().is_empty());
    }

    for bad in [".", "..", "a/b", "a\\b"] {
        let records = RecordingEvents::default();
        let transport = RecordingTransport::new(records.clone());
        let rate_limiter = Arc::new(RecordingRateLimiter::new(records.clone()));
        let hooks = Arc::new(RecordingHooks::new(records.clone()));
        let debug_sink = Arc::new(RecordingDebugSink::new(records.clone()));
        let client = configure_client(
            UrlHardeningApi::new_with_transport("client".to_string(), transport.clone()),
            rate_limiter,
            hooks,
            debug_sink,
        );

        let err = client
            .regional("tenant".to_string())
            .fmt_only(bad.to_string())
            .await
            .expect_err("invalid fmt segment should fail before side effects");

        assert_eq!(err.category(), concord_core::error::ErrorCategory::Config);
        assert_eq!(err.context().endpoint, "regional::FmtOnly");
        assert_eq!(err.context().method, http::Method::GET);
        assert!(err.to_string().contains("invalid/missing param"));
        assert!(transport.requests().is_empty());
        assert!(records.snapshot().is_empty());
    }

    for (value, expected) in [
        ("a b", "https://tenant.api.example.com/v1/fmt/a%20b"),
        ("\u{00b5}", "https://tenant.api.example.com/v1/fmt/%C2%B5"),
    ] {
        let records = RecordingEvents::default();
        let transport = RecordingTransport::new(records.clone());
        let rate_limiter = Arc::new(RecordingRateLimiter::new(records.clone()));
        let hooks = Arc::new(RecordingHooks::new(records.clone()));
        let debug_sink = Arc::new(RecordingDebugSink::new(records.clone()));
        let client = configure_client(
            UrlHardeningApi::new_with_transport("client".to_string(), transport.clone()),
            rate_limiter,
            hooks,
            debug_sink,
        );

        let decoded = client
            .regional("tenant".to_string())
            .fmt_only(value.to_string())
            .await
            .expect("valid fmt segment should succeed");

        assert_eq!(decoded, "ok");
        assert_eq!(transport.requests()[0].url.as_str(), expected);
        assert!(
            records
                .snapshot()
                .iter()
                .any(|event| event == &format!("transport:{expected}"))
        );
    }
}

#[cfg(feature = "dangerous-raw-response")]
#[tokio::test]
async fn execute_raw_obeys_same_url_host_path_validation() {
    let records = RecordingEvents::default();
    let transport = RecordingTransport::new(records.clone());
    let rate_limiter = Arc::new(RecordingRateLimiter::new(records.clone()));
    let hooks = Arc::new(RecordingHooks::new(records.clone()));
    let debug_sink = Arc::new(RecordingDebugSink::new(records.clone()));
    let client = configure_client(
        UrlHardeningApi::new_with_transport("client".to_string(), transport.clone()),
        rate_limiter,
        hooks,
        debug_sink,
    );

    let err = client
        .regional("tenant".to_string())
        .show("a/b".to_string(), "prefix".to_string())
        .execute_raw_response()
        .await
        .expect_err("execute_raw_response must still reject invalid path segments");

    assert_eq!(err.category(), concord_core::error::ErrorCategory::Config);
    assert!(transport.requests().is_empty());
    assert!(records.snapshot().is_empty());
}

#[tokio::test]
async fn sanitized_url_consistent_for_rate_limit_hooks_debug_transport()
-> Result<(), ApiClientError> {
    let records = RecordingEvents::default();
    let transport = RecordingTransport::new(records.clone());
    let rate_limiter = Arc::new(RecordingRateLimiter::new(records.clone()));
    let hooks = Arc::new(RecordingHooks::new(records.clone()));
    let debug_sink = Arc::new(RecordingDebugSink::new(records.clone()));
    let client = configure_client(
        UrlHardeningApi::new_with_transport("client".to_string(), transport.clone()),
        rate_limiter,
        hooks,
        debug_sink,
    );

    let decoded = client
        .regional("tenant".to_string())
        .show("item".to_string(), "prefix".to_string())
        .await?;

    assert_eq!(decoded, "ok");
    let expected = "https://tenant.api.example.com/v1/items/item/p-prefix";
    let events = records.snapshot();
    assert!(
        events
            .iter()
            .any(|event| event == &format!("rate_acquire:{expected}:tenant.api.example.com"))
    );
    assert!(
        events
            .iter()
            .any(|event| event == &format!("hook_pre:{expected}"))
    );
    assert!(
        events
            .iter()
            .any(|event| event == &format!("debug_start:{expected}"))
    );
    assert!(
        events
            .iter()
            .any(|event| event == &format!("transport:{expected}"))
    );
    assert!(
        !events
            .iter()
            .any(|event| event.contains("<unknown>") || event.contains("unknown-host"))
    );
    Ok(())
}

#[tokio::test]
async fn dynamic_path_values_are_percent_encoded_in_final_url() -> Result<(), ApiClientError> {
    let records = RecordingEvents::default();
    let transport = RecordingTransport::new(records.clone());
    let rate_limiter = Arc::new(RecordingRateLimiter::new(records.clone()));
    let hooks = Arc::new(RecordingHooks::new(records.clone()));
    let debug_sink = Arc::new(RecordingDebugSink::new(records.clone()));
    let client = configure_client(
        UrlHardeningApi::new_with_transport("client".to_string(), transport.clone()),
        rate_limiter,
        hooks,
        debug_sink,
    );

    let decoded = client
        .regional("tenant".to_string())
        .show("item 1".to_string(), "\u{00b5}".to_string())
        .await?;

    assert_eq!(decoded, "ok");
    let expected = "https://tenant.api.example.com/v1/items/item%201/p-%C2%B5";
    assert_eq!(transport.requests()[0].url.as_str(), expected);
    assert!(
        records
            .snapshot()
            .iter()
            .any(|event| event == &format!("transport:{expected}"))
    );
    Ok(())
}
