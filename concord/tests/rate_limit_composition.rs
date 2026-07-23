use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(not(feature = "rate-limit-governor"))]
use concord::error::ClientBuildErrorKind;
use concord::http::SafeProxy;
use concord::rate_limit::{
    BoxFuture, Cooldown, RateLimitError, RateLimitObserver, RateLimitPermit, RateLimitRequest,
    RateLimitResponse, RateLimitResponseHead, RateLimiter,
};
use concord::{Bytes, api};
use concord_test_support::server::{ResponseScript, ScriptedServer};

static DEFAULT_PATH_OBSERVERS: AtomicUsize = AtomicUsize::new(0);
static EXPLICIT_PATH_OBSERVERS: AtomicUsize = AtomicUsize::new(0);

struct DefaultPathObserver;

impl Default for DefaultPathObserver {
    fn default() -> Self {
        DEFAULT_PATH_OBSERVERS.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

impl RateLimitObserver for DefaultPathObserver {
    fn observe(&self, _response: &RateLimitResponseHead<'_>) -> Option<Cooldown> {
        None
    }
}

struct ExplicitPathObserver;

impl Default for ExplicitPathObserver {
    fn default() -> Self {
        EXPLICIT_PATH_OBSERVERS.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

impl RateLimitObserver for ExplicitPathObserver {
    fn observe(&self, _response: &RateLimitResponseHead<'_>) -> Option<Cooldown> {
        None
    }
}

api! {
    client DefaultGovernorApi {
        base "http://default-governor.example.test/";
        rate_limit { 10 / 1s; }
        rate_limit_observer crate::DefaultPathObserver;
    }

    GET Read() -> Bytes;
}

api! {
    client ExplicitLimiterApi {
        base "http://explicit-limiter.example.test/";
        rate_limit { 10 / 1s; }
        rate_limit_observer crate::ExplicitPathObserver;
    }

    GET Read() -> Bytes;
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

    fn observe<'a>(&'a self, response: RateLimitResponse<'a>) -> BoxFuture<'a, ()> {
        assert_eq!(response.cooldown_cap, std::time::Duration::from_secs(60));
        self.observations.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {})
    }
}

#[cfg(feature = "rate-limit-governor")]
#[tokio::test]
async fn facade_installs_the_extras_governor_as_generated_default() {
    DEFAULT_PATH_OBSERVERS.store(0, Ordering::SeqCst);
    let server = ScriptedServer::start([ResponseScript::ok(b"default".to_vec())]).expect("server");
    let proxy = SafeProxy::parse(server.url("")).expect("proxy");
    let client = DefaultGovernorApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("façade governor default");

    assert_eq!(DEFAULT_PATH_OBSERVERS.load(Ordering::SeqCst), 1);
    assert_eq!(
        client.read().await.expect("default-governed request"),
        Bytes::from_static(b"default")
    );
    assert_eq!(server.request_count(), 1);
}

#[cfg(not(feature = "rate-limit-governor"))]
#[test]
fn generated_nonempty_plan_fails_closed_without_governor() {
    DEFAULT_PATH_OBSERVERS.store(0, Ordering::SeqCst);
    let error = match DefaultGovernorApi::builder().build() {
        Ok(_) => panic!("missing generated limiter unexpectedly built"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), ClientBuildErrorKind::LimiterUnavailable);
    assert_eq!(DEFAULT_PATH_OBSERVERS.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn explicit_limiter_wins_and_dsl_observer_is_not_constructed() {
    EXPLICIT_PATH_OBSERVERS.store(0, Ordering::SeqCst);
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

    assert_eq!(EXPLICIT_PATH_OBSERVERS.load(Ordering::SeqCst), 0);
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
