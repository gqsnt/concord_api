use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(not(feature = "rate-limit"))]
use concord::error::ClientBuildErrorKind;
use concord::http::SafeProxy;
use concord::rate_limit::{
    BoxFuture, RateLimitError, RateLimitPermit, RateLimitRequest, RateLimitResponse, RateLimiter,
};
use concord::{Bytes, api};
use concord_test_support::server::{ResponseScript, ScriptedServer};

api! {
    client DefaultLocalLimiterApi {
        base "http://default-local-limiter.example.test/";
        rate_limit { 10 / 1s; }
    }

    GET read() -> Bytes;
}

api! {
    client ExplicitLimiterApi {
        base "http://explicit-limiter.example.test/";
        rate_limit { 10 / 1s; }
    }

    GET read() -> Bytes;
}

struct CapturingLimiter {
    acquisitions: Arc<AtomicUsize>,
    observations: Arc<AtomicUsize>,
}

impl RateLimiter for CapturingLimiter {
    fn acquire<'a>(
        &'a self,
        _request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        self.acquisitions.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(RateLimitPermit::granted()) })
    }

    fn observe<'a>(
        &'a self,
        response: RateLimitResponse<'a>,
    ) -> BoxFuture<'a, Result<(), RateLimitError>> {
        assert_eq!(response.cooldown_cap, std::time::Duration::from_secs(60));
        self.observations.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(()) })
    }
}

#[cfg(feature = "rate-limit")]
#[tokio::test]
async fn facade_installs_the_extras_local_limiter_as_generated_default() {
    let server = ScriptedServer::start([ResponseScript::ok(b"default".to_vec())]).expect("server");
    let proxy = SafeProxy::parse(server.url("")).expect("proxy");
    let client = DefaultLocalLimiterApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("façade local limiter default");

    assert_eq!(
        client.read().await.expect("default-governed request"),
        Bytes::from_static(b"default")
    );
    assert_eq!(server.request_count(), 1);
}

#[cfg(not(feature = "rate-limit"))]
#[test]
fn generated_nonempty_plan_fails_closed_without_local_limiter() {
    let error = match DefaultLocalLimiterApi::builder().build() {
        Ok(_) => panic!("missing generated limiter unexpectedly built"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), ClientBuildErrorKind::LimiterUnavailable);
}

#[tokio::test]
async fn explicit_limiter_wins_over_replaced_runtime_configuration() {
    let replaced_acquisitions = Arc::new(AtomicUsize::new(0));
    let replaced_observations = Arc::new(AtomicUsize::new(0));
    let replaced_limiter: Arc<dyn RateLimiter> = Arc::new(CapturingLimiter {
        acquisitions: Arc::clone(&replaced_acquisitions),
        observations: Arc::clone(&replaced_observations),
    });
    let acquisitions = Arc::new(AtomicUsize::new(0));
    let observations = Arc::new(AtomicUsize::new(0));
    let limiter: Arc<dyn RateLimiter> = Arc::new(CapturingLimiter {
        acquisitions: Arc::clone(&acquisitions),
        observations: Arc::clone(&observations),
    });
    let server = ScriptedServer::start([ResponseScript::ok(b"explicit".to_vec())]).expect("server");
    let proxy = SafeProxy::parse(server.url("")).expect("proxy");
    let client = ExplicitLimiterApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.rate_limiter(replaced_limiter))
        .configure_runtime(move |runtime| runtime.rate_limiter(limiter))
        .build()
        .expect("explicit limiter client");

    assert_eq!(replaced_acquisitions.load(Ordering::SeqCst), 0);
    assert_eq!(replaced_observations.load(Ordering::SeqCst), 0);
    assert_eq!(
        client.read().await.expect("explicit-limited request"),
        Bytes::from_static(b"explicit")
    );
    assert_eq!(acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(observations.load(Ordering::SeqCst), 1);
    assert_eq!(server.request_count(), 1);
}
