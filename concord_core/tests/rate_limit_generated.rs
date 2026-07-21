#![cfg(feature = "rate-limit-governor")]

use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use concord_core::advanced::{Cooldown, RateLimitObserver, RateLimitResponseHead, SafeProxy};
use concord_core::prelude::Bytes;
use concord_macros::api;
use concord_test_support::server::{ResponseScript, ScriptedServer};

static OBSERVER_DEFAULTS: AtomicUsize = AtomicUsize::new(0);

pub struct GeneratedObserver;

impl Default for GeneratedObserver {
    fn default() -> Self {
        OBSERVER_DEFAULTS.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

impl RateLimitObserver for GeneratedObserver {
    fn observe(&self, _response: &RateLimitResponseHead<'_>) -> Option<Cooldown> {
        None
    }
}

api! {
    client GovernorApi {
        base "http://generated-governor.example.test/";
        rate_limit { 1 / 1h; }
        rate_limit_observer crate::GeneratedObserver;
        GET Read() -> Bytes;
    }
}

api! {
    client CooldownApi {
        base "http://generated-cooldown.example.test/";
        GET Read() -> Bytes;
    }
}

fn proxy(server: &ScriptedServer) -> SafeProxy {
    SafeProxy::parse(server.url("")).expect("loopback proxy")
}

fn client(server: &ScriptedServer) -> GovernorApi {
    let proxy = proxy(server);
    GovernorApi::builder()
        .http(move |http| http.proxy(proxy))
        .build()
        .expect("generated governor client")
}

#[tokio::test(start_paused = true)]
async fn generated_default_governor_ownership_and_snapshot_contracts_are_exact() {
    OBSERVER_DEFAULTS.store(0, Ordering::SeqCst);
    let first = GovernorApi::builder().build().expect("first build");
    assert_eq!(OBSERVER_DEFAULTS.load(Ordering::SeqCst), 1);
    let cloned = first.clone();
    assert_eq!(OBSERVER_DEFAULTS.load(Ordering::SeqCst), 1);
    let second = GovernorApi::builder().build().expect("second build");
    assert_eq!(OBSERVER_DEFAULTS.load(Ordering::SeqCst), 2);
    drop((first, cloned, second));

    OBSERVER_DEFAULTS.store(0, Ordering::SeqCst);
    let server = ScriptedServer::start([
        ResponseScript::ok(b"first".to_vec()),
        ResponseScript::ok(b"second".to_vec()),
    ])
    .expect("loopback server");
    let first = client(&server);
    let clone = first.clone();
    assert_eq!(
        first.read().await.expect("first request"),
        Bytes::from_static(b"first")
    );
    server
        .wait_for_requests(1, Duration::from_secs(1))
        .expect("first request reached loopback");

    let waiting = tokio::spawn(async move { clone.read().await });
    tokio::task::yield_now().await;
    assert_eq!(server.request_count(), 1);
    tokio::time::advance(Duration::from_secs(3_599)).await;
    tokio::task::yield_now().await;
    assert_eq!(server.request_count(), 1);
    tokio::time::advance(Duration::from_secs(1)).await;
    assert_eq!(
        waiting.await.expect("clone task").expect("second request"),
        Bytes::from_static(b"second")
    );
    server
        .wait_for_requests(2, Duration::from_secs(1))
        .expect("second request reached loopback");
    assert_eq!(OBSERVER_DEFAULTS.load(Ordering::SeqCst), 1);
    drop((first, server));

    OBSERVER_DEFAULTS.store(0, Ordering::SeqCst);
    let server = ScriptedServer::start([
        ResponseScript::ok(b"one".to_vec()),
        ResponseScript::ok(b"two".to_vec()),
    ])
    .expect("loopback server");
    let first = client(&server);
    let second = client(&server);
    assert_eq!(
        first.read().await.expect("first build"),
        Bytes::from_static(b"one")
    );
    assert_eq!(
        second.read().await.expect("second build"),
        Bytes::from_static(b"two")
    );
    server
        .wait_for_requests(2, Duration::from_secs(1))
        .expect("both independent requests reached loopback");
    assert_eq!(OBSERVER_DEFAULTS.load(Ordering::SeqCst), 2);
    drop((first, second, server));

    OBSERVER_DEFAULTS.store(0, Ordering::SeqCst);
    let server =
        ScriptedServer::start([ResponseScript::ok(b"snapshot".to_vec())]).expect("loopback server");
    let client = client(&server);
    let pending = client.read();
    drop(client);
    assert_eq!(
        pending.await.expect("snapshot request"),
        Bytes::from_static(b"snapshot")
    );
    assert_eq!(OBSERVER_DEFAULTS.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn loopback_retry_after_delta_and_date_delay_only_future_requests() {
    let date = httpdate::fmt_http_date(std::time::SystemTime::now() + Duration::from_secs(3_600));
    let server = ScriptedServer::start([
        ResponseScript::status(429, b"delta 429".to_vec()).with_header("Retry-After", "10"),
        ResponseScript::ok(b"after delta".to_vec()),
        ResponseScript::status(429, b"date 429".to_vec()).with_header("Retry-After", date),
        ResponseScript::ok(b"after date".to_vec()),
    ])
    .expect("cooldown loopback server");
    let proxy = proxy(&server);
    let client = CooldownApi::builder()
        .http(move |http| http.proxy(proxy))
        .runtime(|runtime| {
            runtime
                .rate_limit_cooldown_cap(Duration::from_secs(5))
                .status_body_limit(NonZeroUsize::MIN)
        })
        .build()
        .expect("cooldown client");

    let delta = client.read().await.expect_err("delta 429");
    assert_eq!(
        delta.category(),
        concord_core::advanced::ErrorCategory::HttpStatus
    );
    assert_eq!(
        delta.http_status(),
        Some(concord_core::advanced::StatusCode::TOO_MANY_REQUESTS)
    );
    assert_eq!(delta.retry_after(), Some(Duration::from_secs(5)));
    assert_eq!(
        delta
            .http_response()
            .expect("429 response metadata")
            .body()
            .state(),
        concord_core::advanced::StatusBodyState::OmittedKnownTooLarge
    );
    assert_eq!(server.request_count(), 1);
    let after_delta_client = client.clone();
    let after_delta = tokio::spawn(async move { after_delta_client.read().await });
    tokio::time::advance(Duration::from_secs(4)).await;
    tokio::task::yield_now().await;
    assert_eq!(server.request_count(), 1);
    tokio::time::advance(Duration::from_secs(1)).await;
    assert_eq!(
        after_delta.await.expect("delta task").expect("after delta"),
        Bytes::from_static(b"after delta")
    );

    let date = client.read().await.expect_err("date 429");
    assert_eq!(
        date.category(),
        concord_core::advanced::ErrorCategory::HttpStatus
    );
    assert_eq!(date.retry_after(), Some(Duration::from_secs(5)));
    assert_eq!(server.request_count(), 3);
    let after_date = tokio::spawn(async move { client.read().await });
    tokio::time::advance(Duration::from_secs(4)).await;
    tokio::task::yield_now().await;
    assert_eq!(server.request_count(), 3);
    tokio::time::advance(Duration::from_secs(1)).await;
    assert_eq!(
        after_date.await.expect("date task").expect("after date"),
        Bytes::from_static(b"after date")
    );
    server
        .wait_for_requests(4, Duration::from_secs(1))
        .expect("all cooldown requests reached loopback");
}

#[tokio::test(start_paused = true)]
async fn successful_governor_reservation_is_retained_after_transport_failure() {
    OBSERVER_DEFAULTS.store(0, Ordering::SeqCst);
    let server = ScriptedServer::start([
        ResponseScript::CloseWithoutResponse,
        ResponseScript::ok(b"after transport".to_vec()),
    ])
    .expect("transport loopback server");
    let client = client(&server);
    let error = client.read().await.expect_err("transport failure");
    assert_eq!(
        error.category(),
        concord_core::advanced::ErrorCategory::Transport
    );
    server
        .wait_for_requests(1, Duration::from_secs(1))
        .expect("failed attempt reached loopback");

    let waiting = tokio::spawn(async move { client.read().await });
    tokio::task::yield_now().await;
    assert_eq!(server.request_count(), 1);
    tokio::time::advance(Duration::from_secs(3_599)).await;
    tokio::task::yield_now().await;
    assert_eq!(server.request_count(), 1);
    tokio::time::advance(Duration::from_secs(1)).await;
    assert_eq!(
        waiting
            .await
            .expect("waiting task")
            .expect("post-transport request"),
        Bytes::from_static(b"after transport")
    );
    server
        .wait_for_requests(2, Duration::from_secs(1))
        .expect("post-transport attempt reached loopback");
    assert_eq!(OBSERVER_DEFAULTS.load(Ordering::SeqCst), 1);
}
