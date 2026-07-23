#![allow(non_snake_case)]

use std::future::{Future as _, IntoFuture as _};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::{fmt, fmt::Display};

use concord_core::advanced::{
    BoxFuture, ClientBuildErrorKind, GovernorRateLimiter, NoopRateLimiter, RateLimitError,
    RateLimitObserver, RateLimitPermit, RateLimitRequest, RateLimitResponse, RateLimiter,
    SafeProxy,
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
        vars {
            tenant: String;
        }
        rate_limit {
            500 / 10s;
            30000 / 10m;
        }
        rate_limit_observer crate::CountingObserver;
    }

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

api! {
    client EmptyRateApi {
        base "http://empty-rate.example.test/";
    }

    GET Read() -> Bytes;
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
    }

    GET Read(item: crate::DisplayOnly) -> Bytes {
        rate_limit { 1 / 1s; }
    }
}

api! {
    client IdentityRateApi {
        base "http://identity-rate.example.test/";
        rate_limit { 100 / 1h; }
    }

    GET Direct() -> Bytes;
    scope Outer(region: String) {
        rate_limit { 20 / 1h; }
        GET Immediate() -> Bytes;
        scope Inner(kind: String) {
            rate_limit { 10 / 1h; }
            GET Nested() -> Bytes {
                rate_limit { cost 2; 30 / 1h; }
            }
        }
    }
    scope Sibling(region: String) {
        rate_limit { 7 / 1h; }
        GET SiblingEndpoint() -> Bytes {
            rate_limit { 5 / 1h; }
        }
    }
    GET Outside() -> Bytes;
}

api! {
    client ClientBucketRateApi {
        base "http://client-bucket.example.test/";
        rate_limit { 1 / 1h; }
    }

    GET Direct() -> Bytes;
    scope Descendant(value: String) {
        GET Nested() -> Bytes;
    }
}

api! {
    client ScopeBucketRateApi {
        base "http://scope-bucket.example.test/";
        rate_limit { 100 / 1h; }
    }

    scope Branch(value: String) {
        rate_limit { 1 / 1h; }
        GET Immediate() -> Bytes;
        scope Nested(value: String) {
            GET Descendant() -> Bytes;
        }
    }
    GET Outside() -> Bytes;
}

api! {
    client CollisionRateApi {
        base "http://collision-rate.example.test/";
    }

    scope A__B(value: String) {
        rate_limit { 1 / 1h; }
        GET First() -> Bytes;
    }
    scope A(value: String) {
        scope B(value: String) {
            rate_limit { 1 / 1h; }
            GET Second() -> Bytes;
        }
    }
}

#[derive(Clone, Copy)]
enum RouteHost {
    Platform,
    Regional,
}

impl Display for RouteHost {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Platform => "platform",
            Self::Regional => "regional",
        })
    }
}

api! {
    client RouteFamilyApi {
        base "http://route-family.example.test/";
        vars {
            platform: crate::RouteHost = crate::RouteHost::Platform;
            regional: crate::RouteHost = crate::RouteHost::Regional;
        }
    }

    scope Platform() {
        host [vars.platform, "api"];
        rate_limit { 100 / 1h; }
        GET Read() -> Bytes;
    }

    scope Regional() {
        host [vars.regional, "api"];
        rate_limit { 100 / 1h; }
        GET Read() -> Bytes;
    }
}

mod nested_rate {
    use concord_macros::api;

    api! {
        client NestedRateApi {
            base "http://nested-rate.example.test/";
            rate_limit { 1 / 1s; }
        }

        GET Read() -> Bytes;
    }
}

mod repeated_rate {
    use concord_macros::api;

    api! {
        client FirstRateApi {
            base "http://first-rate.example.test/";
            rate_limit { 1 / 1s; }
        }

        GET Read() -> Bytes;
    }

    api! {
        client SecondRateApi {
            base "http://second-rate.example.test/";
            rate_limit { 1 / 1s; }
        }

        GET Read() -> Bytes;
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
    hosts: Mutex<Vec<Option<String>>>,
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
        let host = request.host.map(str::to_owned);
        Box::pin(async move {
            self.acquisitions
                .lock()
                .expect("acquisition capture lock")
                .push(captured);
            self.hosts.lock().expect("host capture lock").push(host);
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
async fn sibling_route_families_resolve_distinct_hosts_and_lexical_rule_ids() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"platform".to_vec()),
        ResponseScript::ok(b"regional".to_vec()),
    ])
    .expect("route-family server");
    let proxy = SafeProxy::parse(server.url("")).expect("route-family proxy");
    let limiter = Arc::new(CapturingLimiter::default());
    let configured = Arc::clone(&limiter);
    let client = RouteFamilyApi::builder()
        .http(move |http| http.proxy(proxy))
        .runtime(move |runtime| runtime.rate_limiter(configured))
        .build()
        .expect("route-family client");

    client.platform().read().await.expect("platform request");
    client.regional().read().await.expect("regional request");

    let hosts = limiter.hosts.lock().expect("route-family hosts");
    assert_eq!(
        hosts.as_slice(),
        [
            Some("platform.api.route-family.example.test".to_owned()),
            Some("regional.api.route-family.example.test".to_owned()),
        ]
    );
    drop(hosts);

    let acquisitions = limiter
        .acquisitions
        .lock()
        .expect("route-family acquisitions");
    assert_eq!(acquisitions.len(), 2);
    assert_eq!(acquisitions[0].len(), 1);
    assert_eq!(acquisitions[1].len(), 1);
    assert!(
        acquisitions[0][0]
            .id
            .ends_with("::RouteFamilyApi::Platform::rate_limit")
    );
    assert!(
        acquisitions[1][0]
            .id
            .ends_with("::RouteFamilyApi::Regional::rate_limit")
    );
    assert_ne!(acquisitions[0][0].id, acquisitions[1][0].id);
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

#[tokio::test]
async fn inherited_lexical_declarations_keep_one_identity_across_descendant_modules() {
    let responses = (0..5).map(|_| ResponseScript::ok(b"identity".to_vec()));
    let server = ScriptedServer::start(responses).expect("identity server");
    let proxy = SafeProxy::parse(server.url("")).expect("identity proxy");
    let limiter = Arc::new(CapturingLimiter::default());
    let configured = Arc::clone(&limiter);
    let client = IdentityRateApi::builder()
        .http(move |http| http.proxy(proxy))
        .runtime(move |runtime| runtime.rate_limiter(configured))
        .build()
        .expect("identity client");

    client.direct().await.expect("direct request");
    client
        .outer("eu".to_owned())
        .immediate()
        .await
        .expect("immediate request");
    client
        .outer("eu".to_owned())
        .inner("nested".to_owned())
        .nested()
        .await
        .expect("nested request");
    client
        .sibling("us".to_owned())
        .sibling_endpoint()
        .await
        .expect("sibling request");
    client.outside().await.expect("outside request");

    let acquisitions = limiter.acquisitions.lock().expect("identity captures");
    assert_eq!(acquisitions.len(), 5);
    let direct_client_id = &acquisitions[0][0].id;
    assert_eq!(direct_client_id, &acquisitions[1][0].id);
    assert_eq!(direct_client_id, &acquisitions[2][0].id);
    assert_eq!(direct_client_id, &acquisitions[3][0].id);
    assert_eq!(direct_client_id, &acquisitions[4][0].id);

    let outer_id = &acquisitions[1][1].id;
    assert_eq!(outer_id, &acquisitions[2][1].id);
    assert_ne!(outer_id, &acquisitions[3][1].id);
    assert_ne!(outer_id, direct_client_id);
    assert!(!acquisitions[2][2].id.is_empty());
    assert_eq!(acquisitions[2][2].cost, 1);
    assert_eq!(
        acquisitions[2][2].windows,
        [(10, std::time::Duration::from_secs(3600))]
    );
    assert_eq!(acquisitions[2][3].cost, 2);
    assert_eq!(
        acquisitions[2][3].windows,
        [(30, std::time::Duration::from_secs(3600))]
    );
    assert_eq!(acquisitions[3].len(), 3);
    assert_ne!(acquisitions[3][1].id, acquisitions[1][1].id);
    assert_ne!(acquisitions[3][2].id, acquisitions[2][3].id);
    assert!(
        acquisitions
            .iter()
            .all(|plan| plan.iter().all(|rule| rule.parts.is_empty()))
    );
}

#[tokio::test]
async fn declaration_identities_are_deterministic_and_collision_resistant() {
    let server = ScriptedServer::start((0..6).map(|_| ResponseScript::ok(b"identity".to_vec())))
        .expect("identity collision server");
    let limiter = Arc::new(CapturingLimiter::default());
    let first = IdentityRateApi::builder()
        .http({
            let proxy = SafeProxy::parse(server.url("")).expect("first identity proxy");
            move |http| http.proxy(proxy)
        })
        .runtime({
            let limiter = Arc::clone(&limiter);
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()
        .expect("first identity client");
    let clone = first.clone();
    clone.direct().await.expect("clone request");
    let second = IdentityRateApi::builder()
        .http({
            let proxy = SafeProxy::parse(server.url("")).expect("second identity proxy");
            move |http| http.proxy(proxy)
        })
        .runtime({
            let limiter = Arc::clone(&limiter);
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()
        .expect("independent identity client");
    second.direct().await.expect("independent request");

    let nested = nested_rate::NestedRateApi::builder()
        .http({
            let proxy = SafeProxy::parse(server.url("")).expect("nested identity proxy");
            move |http| http.proxy(proxy)
        })
        .runtime({
            let limiter = Arc::clone(&limiter);
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()
        .expect("nested-module client");
    nested.read().await.expect("nested-module request");

    let first_repeated = repeated_rate::FirstRateApi::builder()
        .http({
            let proxy = SafeProxy::parse(server.url("")).expect("first repeated proxy");
            move |http| http.proxy(proxy)
        })
        .runtime({
            let limiter = Arc::clone(&limiter);
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()
        .expect("first repeated client");
    first_repeated.read().await.expect("first repeated request");

    let second_repeated = repeated_rate::SecondRateApi::builder()
        .http({
            let proxy = SafeProxy::parse(server.url("")).expect("second repeated proxy");
            move |http| http.proxy(proxy)
        })
        .runtime({
            let limiter = Arc::clone(&limiter);
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()
        .expect("second repeated client");
    second_repeated
        .read()
        .await
        .expect("second repeated request");

    let acquisitions = limiter.acquisitions.lock().expect("collision captures");
    assert_eq!(acquisitions.len(), 5);
    assert_eq!(acquisitions[0][0].id, acquisitions[1][0].id);
    assert_ne!(acquisitions[0][0].id, acquisitions[2][0].id);
    assert_ne!(acquisitions[3][0].id, acquisitions[4][0].id);
    assert!(acquisitions.iter().all(|plan| plan[0].parts.is_empty()));
}

#[tokio::test]
async fn punctuation_distinct_lexical_paths_have_distinct_private_symbols() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"first".to_vec()),
        ResponseScript::ok(b"second".to_vec()),
    ])
    .expect("punctuation collision server");
    let proxy = SafeProxy::parse(server.url("")).expect("punctuation collision proxy");
    let limiter = Arc::new(CapturingLimiter::default());
    let client = CollisionRateApi::builder()
        .http(move |http| http.proxy(proxy))
        .runtime({
            let limiter = Arc::clone(&limiter);
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()
        .expect("collision-safe generated symbols");

    client
        .a__b("first-scope".to_owned())
        .first()
        .await
        .expect("first punctuation path");
    client
        .a("outer-scope".to_owned())
        .b("inner-scope".to_owned())
        .second()
        .await
        .expect("second punctuation path");

    let acquisitions = limiter.acquisitions.lock().expect("punctuation captures");
    assert_eq!(acquisitions.len(), 2);
    assert_eq!(acquisitions[0].len(), 1);
    assert_eq!(acquisitions[1].len(), 1);
    assert_ne!(acquisitions[0][0].id, acquisitions[1][0].id);
    assert!(
        acquisitions
            .iter()
            .flatten()
            .all(|rule| rule.parts.is_empty())
    );
}

#[tokio::test]
async fn sibling_and_endpoint_local_rules_do_not_share_unrelated_buckets() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"sibling".to_vec()),
        ResponseScript::ok(b"outside".to_vec()),
    ])
    .expect("unrelated bucket server");
    let limiter = Arc::new(CapturingLimiter::default());
    let client = IdentityRateApi::builder()
        .http({
            let proxy = SafeProxy::parse(server.url("")).expect("unrelated bucket proxy");
            move |http| http.proxy(proxy)
        })
        .runtime({
            let limiter = Arc::clone(&limiter);
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()
        .expect("unrelated bucket client");
    client
        .sibling("sibling-value".to_owned())
        .sibling_endpoint()
        .await
        .expect("sibling request");
    client.outside().await.expect("outside request");

    let acquisitions = limiter.acquisitions.lock().expect("unrelated captures");
    assert_eq!(acquisitions.len(), 2);
    assert_eq!(acquisitions[0].len(), 3);
    assert_eq!(acquisitions[1].len(), 1);
    assert_ne!(acquisitions[0][1].id, acquisitions[0][2].id);
    assert_ne!(acquisitions[0][2].id, acquisitions[1][0].id);
}

#[tokio::test(start_paused = true)]
async fn client_rule_quota_is_shared_across_direct_and_scoped_endpoints() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"first".to_vec()),
        ResponseScript::ok(b"unrelated".to_vec()),
        ResponseScript::ok(b"after-cancellation".to_vec()),
        ResponseScript::ok(b"unexpected-extra".to_vec()),
    ])
    .expect("client bucket server");
    let proxy = SafeProxy::parse(server.url("")).expect("client bucket proxy");
    let governor = Arc::new(GovernorRateLimiter::new());
    let client = ClientBucketRateApi::builder()
        .http(move |http| http.proxy(proxy))
        .runtime({
            let governor = Arc::clone(&governor);
            move |runtime| runtime.rate_limiter(governor)
        })
        .build()
        .expect("client bucket client");
    client.direct().await.expect("first bucket reservation");
    server
        .wait_for_requests(1, std::time::Duration::from_secs(1))
        .expect("first request reached server");

    let unrelated = EmptyRateApi::builder()
        .http({
            let proxy = SafeProxy::parse(server.url("")).expect("unrelated client proxy");
            move |http| http.proxy(proxy)
        })
        .build()
        .expect("unrelated client");
    unrelated.read().await.expect("unrelated request");
    server
        .wait_for_requests(2, std::time::Duration::from_secs(1))
        .expect("unrelated request reached server");

    let mut pending = Box::pin(
        client
            .descendant("same-scope".to_owned())
            .nested()
            .into_future(),
    );
    let waker = std::task::Waker::noop();
    let mut context = std::task::Context::from_waker(waker);
    assert!(
        matches!(
            pending.as_mut().poll(&mut context),
            std::task::Poll::Pending
        ),
        "shared client quota must return Pending before transport",
    );
    assert_eq!(server.request_count(), 2);
    drop(pending);
    assert_eq!(server.request_count(), 2);

    tokio::time::advance(std::time::Duration::from_secs(3600)).await;
    assert_eq!(server.request_count(), 2);
    client
        .direct()
        .await
        .expect("cancellation did not consume the refilled quota");
    server
        .wait_for_requests(3, std::time::Duration::from_secs(1))
        .expect("post-cancellation request reached server");
}

#[tokio::test(start_paused = true)]
async fn scope_rule_quota_is_shared_across_immediate_and_nested_descendants() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"first".to_vec()),
        ResponseScript::ok(b"outside".to_vec()),
        ResponseScript::ok(b"after-cancellation".to_vec()),
    ])
    .expect("scope bucket server");
    let proxy = SafeProxy::parse(server.url("")).expect("scope bucket proxy");
    let governor = Arc::new(GovernorRateLimiter::new());
    let client = ScopeBucketRateApi::builder()
        .http(move |http| http.proxy(proxy))
        .runtime({
            let governor = Arc::clone(&governor);
            move |runtime| runtime.rate_limiter(governor)
        })
        .build()
        .expect("scope bucket client");
    client
        .branch("same-scope".to_owned())
        .immediate()
        .await
        .expect("first scope reservation");
    server
        .wait_for_requests(1, std::time::Duration::from_secs(1))
        .expect("first scope request reached server");

    client.outside().await.expect("outside-scope request");
    server
        .wait_for_requests(2, std::time::Duration::from_secs(1))
        .expect("outside-scope request reached server");

    let pending_client = client.clone();
    let pending = tokio::spawn(async move {
        pending_client
            .branch("same-scope".to_owned())
            .nested("same-nested".to_owned())
            .descendant()
            .await
    });
    tokio::task::yield_now().await;
    assert!(!pending.is_finished(), "shared scope quota must wait");
    assert_eq!(server.request_count(), 2);
    tokio::time::advance(std::time::Duration::from_secs(3599)).await;
    tokio::task::yield_now().await;
    assert!(
        !pending.is_finished(),
        "shared scope quota remains unavailable"
    );
    pending.abort();
    assert!(pending.await.is_err());

    tokio::time::advance(std::time::Duration::from_secs(1)).await;
    client
        .branch("same-scope".to_owned())
        .nested("after-cancellation".to_owned())
        .descendant()
        .await
        .expect("scope cancellation did not consume the refilled quota");
    server
        .wait_for_requests(3, std::time::Duration::from_secs(1))
        .expect("post-cancellation scope request reached server");
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
