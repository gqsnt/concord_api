use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::{fmt, fmt::Display};

use concord_core::advanced::{
    BoxFuture, ClientBuildErrorKind, NoopRateLimiter, RateLimitError, RateLimitObserver,
    RateLimitPermit, RateLimitRequest, RateLimitResponse, RateLimiter, SafeProxy,
};
use concord_core::prelude::Bytes;
use concord_macros::api;
use concord_test_support::server::{ResponseScript, ScriptedServer};

static OBSERVER_DEFAULTS: AtomicUsize = AtomicUsize::new(0);

pub struct CountingObserver;

impl Default for CountingObserver {
    fn default() -> Self {
        OBSERVER_DEFAULTS.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

impl RateLimitObserver for CountingObserver {
    fn observe(
        &self,
        _response: &concord_core::advanced::RateLimitResponseHead<'_>,
    ) -> Option<concord_core::advanced::Cooldown> {
        None
    }
}

api! {
    client RateApi {
        base "http://rate-limit.example.test/";
        var tenant: String;
        rate_limit {
            500 / 10s;
            30000 / 10m;
        }
        rate_limit_observer crate::CountingObserver;
        scope Regions(region: String) {
            rate_limit {
                20 / 1s;
            }
            GET Read(item: String) -> Bytes {
                rate_limit {
                    cost 2;
                    100 / 1m;
                }
            }
            GET Public() -> Bytes;
        }
    }
}

api! {
    client EmptyRateApi {
        base "http://empty-rate.example.test/";
        GET Read() -> Bytes;
    }
}

pub struct DisplayOnly {
    text: &'static str,
    calls: Arc<AtomicUsize>,
}

impl Display for DisplayOnly {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.calls.fetch_add(1, Ordering::SeqCst);
        formatter.write_str(self.text)
    }
}

api! {
    client MinimalBoundsApi {
        base "http://minimal-rate.example.test/";
        GET Read(item: crate::DisplayOnly) -> Bytes {
            rate_limit { 1 / 1s; }
        }
    }
}

mod nested_rate {
    use concord_macros::api;

    api! {
        client NestedRateApi {
            base "http://nested-rate.example.test/";
            rate_limit { 1 / 1s; }
            GET Read() -> Bytes;
        }
    }
}

mod repeated_rate {
    use concord_macros::api;

    api! {
        client FirstRateApi {
            base "http://first-rate.example.test/";
            rate_limit { 1 / 1s; }
            GET Read() -> Bytes;
        }
    }

    api! {
        client SecondRateApi {
            base "http://second-rate.example.test/";
            rate_limit { 1 / 1s; }
            GET Read() -> Bytes;
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CapturedRule {
    id: String,
    parts: Vec<(String, String)>,
    cost: u32,
    windows: Vec<(u32, std::time::Duration)>,
}

#[derive(Default)]
struct CapturingLimiter {
    acquisitions: Mutex<Vec<Vec<CapturedRule>>>,
    observations: Mutex<Vec<Vec<CapturedRule>>>,
}

struct GatedLimiter {
    reached: tokio::sync::Notify,
    permits: tokio::sync::Semaphore,
    acquisitions: AtomicUsize,
}

impl GatedLimiter {
    fn new() -> Self {
        Self {
            reached: tokio::sync::Notify::new(),
            permits: tokio::sync::Semaphore::new(0),
            acquisitions: AtomicUsize::new(0),
        }
    }
}

impl RateLimiter for GatedLimiter {
    fn acquire<'a>(
        &'a self,
        _request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        self.acquisitions.fetch_add(1, Ordering::SeqCst);
        self.reached.notify_one();
        Box::pin(async move {
            self.permits
                .acquire()
                .await
                .expect("test gate remains open")
                .forget();
            Ok(RateLimitPermit::granted())
        })
    }
}

impl CapturingLimiter {
    fn capture(plan: &concord_core::advanced::RateLimitPlan) -> Vec<CapturedRule> {
        plan.rules()
            .iter()
            .map(|rule| CapturedRule {
                id: rule.id().as_str().to_owned(),
                parts: rule
                    .key()
                    .parts()
                    .iter()
                    .map(|part| (part.name().to_owned(), part.value().to_owned()))
                    .collect(),
                cost: rule.cost().get(),
                windows: rule
                    .windows()
                    .iter()
                    .map(|window| (window.capacity().get(), window.duration()))
                    .collect(),
            })
            .collect()
    }
}

impl RateLimiter for CapturingLimiter {
    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        let captured = Self::capture(request.plan);
        Box::pin(async move {
            self.acquisitions
                .lock()
                .expect("acquisition capture lock")
                .push(captured);
            Ok(RateLimitPermit::granted())
        })
    }

    fn observe<'a>(&'a self, response: RateLimitResponse<'a>) -> BoxFuture<'a, ()> {
        let captured = Self::capture(response.request.plan);
        Box::pin(async move {
            self.observations
                .lock()
                .expect("observation capture lock")
                .push(captured);
        })
    }
}

fn builder() -> RateApiBuilder {
    RateApi::builder().tenant("tenant-west".to_owned())
}

#[test]
fn generated_default_limiter_selection_matches_the_resolved_feature_graph() {
    OBSERVER_DEFAULTS.store(0, Ordering::SeqCst);
    match builder().build() {
        Ok(_client) => {
            assert_eq!(OBSERVER_DEFAULTS.load(Ordering::SeqCst), 1);
        }
        Err(error) => {
            assert_eq!(error.kind(), ClientBuildErrorKind::LimiterUnavailable);
            assert_eq!(OBSERVER_DEFAULTS.load(Ordering::SeqCst), 0);
        }
    }
}

#[tokio::test]
async fn custom_limiter_receives_canonical_ordered_plan_and_owns_observation() {
    OBSERVER_DEFAULTS.store(0, Ordering::SeqCst);
    let server = ScriptedServer::start([ResponseScript::ok(b"granted".to_vec())])
        .expect("rate-limit server");
    let proxy = SafeProxy::parse(server.url("")).expect("local proxy");
    let limiter = Arc::new(CapturingLimiter::default());
    let configured = Arc::clone(&limiter);
    let client = builder()
        .http(move |http| http.proxy(proxy))
        .runtime(move |runtime| runtime.rate_limiter(configured))
        .build()
        .expect("custom-limited generated client");

    let body = client
        .regions("eu".to_owned())
        .read("item-42".to_owned())
        .await
        .expect("granted application request");
    assert_eq!(body, Bytes::from_static(b"granted"));
    assert_eq!(OBSERVER_DEFAULTS.load(Ordering::SeqCst), 0);

    let acquisitions = limiter.acquisitions.lock().expect("acquisition capture");
    assert_eq!(acquisitions.len(), 1);
    let rules = &acquisitions[0];
    assert_eq!(rules.len(), 3);
    assert!(rules[0].id.ends_with("::RateApi::rate_limit"));
    assert!(rules[1].id.ends_with("::RateApi::Regions::rate_limit"));
    assert!(
        rules[2]
            .id
            .ends_with("::RateApi::Regions::Read::rate_limit")
    );
    assert!(rules.windows(2).all(|pair| pair[0].id != pair[1].id));
    assert!(rules.iter().all(|rule| rule.parts.is_empty()));
    assert_eq!(rules[0].cost, 1);
    assert_eq!(
        rules[0].windows,
        [
            (500, std::time::Duration::from_secs(10)),
            (30_000, std::time::Duration::from_secs(600)),
        ]
    );
    assert_eq!(rules[2].cost, 2);
    drop(acquisitions);

    let observations = limiter.observations.lock().expect("observation capture");
    assert_eq!(observations.len(), 1);
    assert_eq!(observations[0], rules_from_limiter(&limiter));
}

fn rules_from_limiter(limiter: &CapturingLimiter) -> Vec<CapturedRule> {
    limiter
        .acquisitions
        .lock()
        .expect("acquisition capture")
        .first()
        .expect("one acquisition")
        .clone()
}

#[tokio::test]
async fn explicit_noop_authorizes_nonempty_generated_plan_without_dsl_observer() {
    OBSERVER_DEFAULTS.store(0, Ordering::SeqCst);
    let server = ScriptedServer::start([ResponseScript::ok(b"noop".to_vec())])
        .expect("no-op rate-limit server");
    let proxy = SafeProxy::parse(server.url("")).expect("local proxy");
    let client = builder()
        .http(move |http| http.proxy(proxy))
        .runtime(|runtime| runtime.rate_limiter(Arc::new(NoopRateLimiter::new())))
        .build()
        .expect("explicit no-op is an intentional selection");
    let body = client
        .regions("eu".to_owned())
        .read("item".to_owned())
        .await
        .expect("no-op authorized request");
    assert_eq!(body, Bytes::from_static(b"noop"));
    assert_eq!(OBSERVER_DEFAULTS.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn no_governor_private_fallback_executes_an_empty_generated_plan() {
    let server =
        ScriptedServer::start([ResponseScript::ok(b"empty".to_vec())]).expect("empty-plan server");
    let proxy = SafeProxy::parse(server.url("")).expect("local proxy");
    let client = EmptyRateApi::builder()
        .http(move |http| http.proxy(proxy))
        .build()
        .expect("an empty plan needs no explicit limiter");
    assert_eq!(
        client.read().await.expect("empty-plan request"),
        Bytes::from_static(b"empty")
    );
}

#[tokio::test]
async fn generated_rate_limits_add_no_display_bound_or_runtime_formatting() {
    let server = ScriptedServer::start([ResponseScript::ok(b"display".to_vec())])
        .expect("display-bound server");
    let proxy = SafeProxy::parse(server.url("")).expect("local proxy");
    let client = MinimalBoundsApi::builder()
        .http(move |http| http.proxy(proxy))
        .runtime(|runtime| runtime.rate_limiter(Arc::new(NoopRateLimiter::new())))
        .build()
        .expect("minimal-bound client");
    let calls = Arc::new(AtomicUsize::new(0));
    let value = DisplayOnly {
        text: "display-only-key",
        calls: Arc::clone(&calls),
    };
    assert_eq!(
        client.read(value).await.expect("display-only request"),
        Bytes::from_static(b"display")
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn nested_and_repeated_macro_invocations_keep_rate_contracts_isolated() {
    let nested = nested_rate::NestedRateApi::builder()
        .runtime(|runtime| runtime.rate_limiter(Arc::new(NoopRateLimiter::new())))
        .build()
        .expect("nested rate client");
    let first = repeated_rate::FirstRateApi::builder()
        .runtime(|runtime| runtime.rate_limiter(Arc::new(NoopRateLimiter::new())))
        .build()
        .expect("first repeated rate client");
    let second = repeated_rate::SecondRateApi::builder()
        .runtime(|runtime| runtime.rate_limiter(Arc::new(NoopRateLimiter::new())))
        .build()
        .expect("second repeated rate client");
    let _: concord_core::Request<_> = nested.read();
    let _: concord_core::Request<_> = first.read();
    let _: concord_core::Request<_> = second.read();
}

#[tokio::test]
async fn real_application_send_cannot_start_before_custom_limiter_grant() {
    let server = ScriptedServer::start([ResponseScript::ok(b"after grant".to_vec())])
        .expect("gated loopback server");
    let proxy = SafeProxy::parse(server.url("")).expect("local proxy");
    let limiter = Arc::new(GatedLimiter::new());
    let configured: Arc<dyn RateLimiter> = limiter.clone();
    let client = builder()
        .http(move |http| http.proxy(proxy))
        .runtime(move |runtime| runtime.rate_limiter(configured))
        .build()
        .expect("gated generated client");
    let request = tokio::spawn(async move {
        client
            .regions("eu".to_owned())
            .read("gated-item".to_owned())
            .await
    });
    limiter.reached.notified().await;
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(server.request_count(), 0);
    limiter.permits.add_permits(1);
    assert_eq!(
        request.await.expect("request task").expect("gated request"),
        Bytes::from_static(b"after grant")
    );
    server
        .wait_for_requests(1, std::time::Duration::from_secs(1))
        .expect("granted request reached loopback");
}
