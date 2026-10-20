#![cfg(feature = "rate-limit")]

use concord::http::SafeProxy;
use concord::rate_limit::LocalRateLimiter;
use concord::{Binary, Bytes, api};
use concord_test_support::server::{ResponseScript, ScriptedServer};
use std::sync::Arc;
use std::time::Duration;

api! {
    client R7Cooldown {
        base "http://cooldown.example.test/";
        GET read() -> Bytes via Binary;
        scope sensitive(#[sensitive] tenant: String) {
            subdomain [tenant];
            GET read() -> Bytes via Binary;
        }
    }
}

struct OpaqueLimiter(LocalRateLimiter);

impl concord::rate_limit::RateLimiter for OpaqueLimiter {
    fn capability(&self) -> concord::rate_limit::LimiterCapability {
        LocalRateLimiter::CAPABILITY
    }
    fn acquire<'a>(
        &'a self,
        request: concord::rate_limit::RateLimitRequest<'a>,
    ) -> concord::rate_limit::BoxFuture<
        'a,
        Result<concord::rate_limit::RateLimitPermit, concord::rate_limit::RateLimitError>,
    > {
        assert!(
            request.logical_origin.is_none(),
            "empty plans do not gain a raw origin dimension"
        );
        let debug = format!(
            "{:?}",
            request
                .cooldown_origin
                .expect("core supplies an opaque origin")
        );
        assert!(!debug.contains("private-tenant-sentinel"));
        self.0.acquire(request)
    }
    fn observe(&self, response: concord::rate_limit::RateLimitResponse<'_>) {
        self.0.observe(response);
    }
}

fn configured(
    server: &ScriptedServer,
    limiter: Arc<dyn concord::rate_limit::RateLimiter>,
) -> R7Cooldown {
    let proxy = SafeProxy::parse(server.url("")).unwrap();
    R7Cooldown::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.rate_limiter(limiter))
        .build()
        .unwrap()
}

#[tokio::test]
async fn r7_sensitive_origin_cooldowns_share_only_the_selected_object_and_matching_origin() {
    let server = ScriptedServer::start([
        ResponseScript::Complete {
            status: 429,
            headers: vec![("Retry-After".into(), "2".into())],
            body: vec![],
        },
        ResponseScript::ok(b"independent origin".to_vec()),
        ResponseScript::ok(b"independent object".to_vec()),
        ResponseScript::ok(b"after expiry".to_vec()),
    ])
    .unwrap();
    let limiter = Arc::new(OpaqueLimiter(LocalRateLimiter::new()));
    let first = configured(&server, limiter.clone());
    let shared = configured(&server, limiter);
    let separate = configured(&server, Arc::new(OpaqueLimiter(LocalRateLimiter::new())));
    let scope = first.sensitive("private-tenant-sentinel");
    assert!(
        !scope
            .read()
            .plan()
            .unwrap()
            .observed_url()
            .contains("private-tenant-sentinel")
    );
    assert_eq!(
        scope
            .read()
            .await
            .unwrap_err()
            .last_response_status()
            .unwrap()
            .as_u16(),
        429
    );
    let error = shared
        .sensitive("private-tenant-sentinel")
        .read()
        .timeout(Duration::from_millis(100))
        .unwrap()
        .await
        .unwrap_err();
    assert_eq!(error.operation_counts().unwrap().application_handoffs(), 0);
    assert!(!format!("{error:?}").contains("private-tenant-sentinel"));
    assert_eq!(
        shared.sensitive("other").read().await.unwrap(),
        Bytes::from_static(b"independent origin")
    );
    assert_eq!(
        separate
            .sensitive("private-tenant-sentinel")
            .read()
            .await
            .unwrap(),
        Bytes::from_static(b"independent object")
    );
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(
        scope.read().await.unwrap(),
        Bytes::from_static(b"after expiry")
    );
    assert_eq!(server.request_count(), 4);
}

#[tokio::test]
async fn r7_zero_and_malformed_delays_do_not_install_an_origin_cooldown() {
    for delay in ["0", "malformed"] {
        let server = ScriptedServer::start([
            ResponseScript::Complete {
                status: 429,
                headers: vec![("Retry-After".into(), delay.into())],
                body: vec![],
            },
            ResponseScript::ok(b"ok".to_vec()),
        ])
        .unwrap();
        let client = configured(&server, Arc::new(OpaqueLimiter(LocalRateLimiter::new())));
        assert!(client.read().await.is_err());
        assert_eq!(client.read().await.unwrap(), Bytes::from_static(b"ok"));
        assert_eq!(server.request_count(), 2);
    }
}

#[tokio::test]
async fn r7_empty_plan_429_blocks_the_next_actual_same_origin_endpoint() {
    let server = ScriptedServer::start([
        ResponseScript::Chunked {
            status: 429,
            headers: vec![("Retry-After".into(), "1".into())],
            chunks: vec![],
        },
        ResponseScript::ok(b"recovered".to_vec()),
    ])
    .unwrap();
    let proxy = SafeProxy::parse(server.url("")).unwrap();
    let limiter = Arc::new(LocalRateLimiter::new());
    let client = R7Cooldown::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.rate_limiter(limiter))
        .build()
        .unwrap();
    assert_eq!(
        client
            .read()
            .await
            .unwrap_err()
            .last_response_status()
            .unwrap()
            .as_u16(),
        429
    );
    let error = client
        .read()
        .timeout(Duration::from_millis(100))
        .unwrap()
        .await
        .expect_err("origin cooldown must block this actual request");
    assert_eq!(error.category(), concord::error::ErrorCategory::Timeout);
    assert_eq!(error.operation_counts().unwrap().application_handoffs(), 0);
    assert_eq!(server.request_count(), 1);
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(
        client.read().await.unwrap(),
        Bytes::from_static(b"recovered")
    );
    assert_eq!(server.request_count(), 2);
}
