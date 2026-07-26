#![allow(non_snake_case)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::{fmt, fmt::Display};

use concord::Bytes;
use concord::api;
use concord::error::{ClientBuildErrorKind, ConfigurationErrorKind};
use concord::http::SafeProxy;
use concord::rate_limit::{
    BoxFuture, NoopRateLimiter, RateLimitError, RateLimitPermit, RateLimitPlan, RateLimitRequest,
    RateLimitResponse, RateLimiter,
};
use concord_test_support::server::{ResponseScript, ScriptedServer};

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
    }

    scope regions(region: String) {
        rate_limit {
            20 / 1s;
        }
        GET read(item: String) -> Bytes {
            rate_limit {
                cost 2;
                100 / 1m;
            }
        }
        GET public() -> Bytes;
    }
}

api! {
    client EmptyRateApi {
        base "http://empty-rate.example.test/";
    }

    GET read() -> Bytes;
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

pub struct NoDisplay;

api! {
    client MinimalBoundsApi {
        base "http://minimal-rate.example.test/";
    }

    GET read(item: crate::NoDisplay) -> Bytes {
        rate_limit { 1 / 1s; }
    }
}

api! {
    client IdentityRateApi {
        base "http://identity-rate.example.test/";
        rate_limit { 100 / 1h; }
    }

    GET direct() -> Bytes;
    scope outer(region: String) {
        rate_limit { 20 / 1h; }
        GET immediate() -> Bytes;
        scope inner(kind: String) {
            rate_limit { 10 / 1h; }
            GET nested() -> Bytes {
                rate_limit { cost 2; 30 / 1h; }
            }
        }
    }
    scope sibling(region: String) {
        rate_limit { 7 / 1h; }
        GET sibling_endpoint() -> Bytes {
            id "stable.identity.sibling";
            rate_limit { 5 / 1h; }
        }
    }
    GET outside() -> Bytes;
}

api! {
    client ClientBucketRateApi {
        base "http://client-bucket.example.test/";
        rate_limit { 1 / 1h; }
    }

    GET direct() -> Bytes;
    scope descendant(value: String) {
        GET nested() -> Bytes;
    }
}

api! {
    client ScopeBucketRateApi {
        base "http://scope-bucket.example.test/";
        rate_limit { 100 / 1h; }
    }

    scope branch(value: String) {
        rate_limit { 1 / 1h; }
        GET immediate() -> Bytes;
        scope nested(value: String) {
            GET descendant() -> Bytes;
        }
    }
    GET outside() -> Bytes;
}

api! {
    client HostBucketRateApi {
        base "http://host-bucket.example.test/";
    }

    scope region(region: String) {
        host [scope.region, "api"];
        rate_limit {
            by [host];
            100 / 1h;
        }
        GET read() -> Bytes;
    }
}

api! {
    client ScopeValueBucketRateApi {
        base "http://scope-value-bucket.example.test/";
    }

    scope account(account: String) {
        rate_limit {
            by [scope.account];
            100 / 1h;
        }
        GET read() -> Bytes;
    }
}

api! {
    client CollisionRateApi {
        base "http://collision-rate.example.test/";
    }

    scope a__b(value: String) {
        rate_limit { 1 / 1h; }
        GET first() -> Bytes;
    }
    scope a(value: String) {
        scope b(value: String) {
            rate_limit { 1 / 1h; }
            GET second() -> Bytes;
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum RouteHost {
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

    scope platform() {
        host [vars.platform, "api"];
        rate_limit { 100 / 1h; }
        GET read() -> Bytes;
    }

    scope regional() {
        host [vars.regional, "api"];
        rate_limit { 100 / 1h; }
        GET read() -> Bytes;
    }
}

api! {
    client PartitionedRateApi {
        base "http://partitioned-rate.example.test/";
        vars {
            tenant: String;
        }
    }

    scope regional(region: String) {
        host [scope.region, "api"];

        GET read(resource: crate::DisplayOnly) -> Bytes {
            id "stable.partitioned.read";
            rate_limit {
                by [host, endpoint, vars.tenant, scope.region, ep.resource];
                1 / 1h;
            }
        }
    }
}

api! {
    client EndpointBucketRateApi {
        base "http://endpoint-bucket.example.test/";
        rate_limit {
            by [endpoint];
            100 / 1h;
        }
    }

    GET first() -> Bytes {
        id "stable.endpoint.first";
    }

    GET second() -> Bytes {
        id "stable.endpoint.second";
    }
}

mod nested_rate {
    use concord::api;

    api! {
        client NestedRateApi {
            base "http://nested-rate.example.test/";
            rate_limit { 1 / 1s; }
        }

        GET read() -> Bytes;
    }
}

mod repeated_rate {
    use concord::api;

    api! {
        client FirstRateApi {
            base "http://first-rate.example.test/";
            rate_limit { 1 / 1s; }
        }

        GET read() -> Bytes;
    }

    api! {
        client SecondRateApi {
            base "http://second-rate.example.test/";
            rate_limit { 1 / 1s; }
        }

        GET read() -> Bytes;
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

#[derive(Default)]
struct SmallKeyLimiter {
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

impl RateLimiter for SmallKeyLimiter {
    fn preparation_key_limit(&self) -> usize {
        4
    }

    fn acquire<'a>(
        &'a self,
        _request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        self.acquisitions.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(RateLimitPermit::granted()) })
    }
}

impl CapturingLimiter {
    fn capture(plan: &RateLimitPlan) -> Vec<CapturedRule> {
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

    fn observe<'a>(
        &'a self,
        response: RateLimitResponse<'a>,
    ) -> BoxFuture<'a, Result<(), RateLimitError>> {
        let captured = Self::capture(response.request.plan);
        Box::pin(async move {
            self.observations
                .lock()
                .expect("observation capture lock")
                .push(captured);
            Ok(())
        })
    }
}

fn builder() -> RateApiBuilder {
    RateApi::builder().tenant("tenant-west".to_owned())
}

#[test]
fn generated_default_limiter_selection_matches_the_resolved_feature_graph() {
    match builder().build() {
        Ok(_client) => {}
        Err(error) => {
            assert_eq!(error.kind(), ClientBuildErrorKind::LimiterUnavailable);
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
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.rate_limiter(configured))
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
            .ends_with("route_family_api.platform.rate_limit")
    );
    assert!(
        acquisitions[1][0]
            .id
            .ends_with("route_family_api.regional.rate_limit")
    );
    assert_ne!(acquisitions[0][0].id, acquisitions[1][0].id);
}

#[tokio::test]
async fn client_global_rule_shares_one_bucket() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"direct".to_vec()),
        ResponseScript::ok(b"nested".to_vec()),
    ])
    .expect("client-global server");
    let proxy = SafeProxy::parse(server.url("")).expect("client-global proxy");
    let limiter = Arc::new(CapturingLimiter::default());
    let configured = Arc::clone(&limiter);
    let client = ClientBucketRateApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.rate_limiter(configured))
        .build()
        .expect("client-global client");

    client.direct().await.expect("direct request");
    client
        .descendant("scope-a".to_owned())
        .nested()
        .await
        .expect("nested request");

    let acquisitions = limiter.acquisitions.lock().expect("client-global captures");
    assert_eq!(acquisitions.len(), 2);
    assert_eq!(acquisitions[0][0].id, acquisitions[1][0].id);
    assert!(acquisitions[0][0].parts.is_empty());
    assert!(acquisitions[1][0].parts.is_empty());
}

#[tokio::test]
async fn scope_rule_without_by_shares_across_scope_values() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"first".to_vec()),
        ResponseScript::ok(b"second".to_vec()),
    ])
    .expect("scope-global server");
    let proxy = SafeProxy::parse(server.url("")).expect("scope-global proxy");
    let limiter = Arc::new(CapturingLimiter::default());
    let configured = Arc::clone(&limiter);
    let client = ScopeBucketRateApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.rate_limiter(configured))
        .build()
        .expect("scope-global client");

    client
        .branch("account-a".to_owned())
        .immediate()
        .await
        .expect("first scope value");
    client
        .branch("account-b".to_owned())
        .immediate()
        .await
        .expect("second scope value");

    let acquisitions = limiter.acquisitions.lock().expect("scope-global captures");
    assert_eq!(acquisitions.len(), 2);
    assert_eq!(acquisitions[0][1].id, acquisitions[1][1].id);
    assert!(acquisitions[0][1].parts.is_empty());
    assert!(acquisitions[1][1].parts.is_empty());
}

#[tokio::test]
async fn scope_rule_by_host_separates_hosts() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"eu".to_vec()),
        ResponseScript::ok(b"us".to_vec()),
    ])
    .expect("host-bucket server");
    let proxy = SafeProxy::parse(server.url("")).expect("host-bucket proxy");
    let limiter = Arc::new(CapturingLimiter::default());
    let configured = Arc::clone(&limiter);
    let client = HostBucketRateApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.rate_limiter(configured))
        .build()
        .expect("host-bucket client");

    client
        .region("eu".to_owned())
        .read()
        .await
        .expect("EU host");
    client
        .region("us".to_owned())
        .read()
        .await
        .expect("US host");

    let acquisitions = limiter.acquisitions.lock().expect("host-bucket captures");
    assert_eq!(acquisitions[0][0].id, acquisitions[1][0].id);
    assert_eq!(
        acquisitions[0][0].parts,
        [(
            "host".to_owned(),
            "eu.api.host-bucket.example.test".to_owned()
        )]
    );
    assert_eq!(
        acquisitions[1][0].parts,
        [(
            "host".to_owned(),
            "us.api.host-bucket.example.test".to_owned()
        )]
    );
}

#[tokio::test]
async fn scope_rule_by_scope_value_separates_values() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"first".to_vec()),
        ResponseScript::ok(b"second".to_vec()),
    ])
    .expect("scope-value server");
    let proxy = SafeProxy::parse(server.url("")).expect("scope-value proxy");
    let limiter = Arc::new(CapturingLimiter::default());
    let configured = Arc::clone(&limiter);
    let client = ScopeValueBucketRateApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.rate_limiter(configured))
        .build()
        .expect("scope-value client");

    client
        .account("account-a".to_owned())
        .read()
        .await
        .expect("first account");
    client
        .account("account-b".to_owned())
        .read()
        .await
        .expect("second account");

    let acquisitions = limiter.acquisitions.lock().expect("scope-value captures");
    assert_eq!(acquisitions[0][0].id, acquisitions[1][0].id);
    assert_eq!(
        acquisitions[0][0].parts,
        [("scope.account".to_owned(), "account-a".to_owned())]
    );
    assert_eq!(
        acquisitions[1][0].parts,
        [("scope.account".to_owned(), "account-b".to_owned())]
    );
}

#[tokio::test]
async fn composite_keys_are_ordered_and_distinct() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"first".to_vec()),
        ResponseScript::ok(b"same".to_vec()),
        ResponseScript::ok(b"different".to_vec()),
    ])
    .expect("partitioned-key server");
    let proxy = SafeProxy::parse(server.url("")).expect("partitioned-key proxy");
    let limiter = Arc::new(CapturingLimiter::default());
    let configured = Arc::clone(&limiter);
    let client = PartitionedRateApi::builder()
        .tenant("tenant-a".to_owned())
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.rate_limiter(configured))
        .build()
        .expect("partitioned-key client");
    let display_calls = Arc::new(AtomicUsize::new(0));

    for (resource, expected) in [
        ("resource-a", Bytes::from_static(b"first")),
        ("resource-a", Bytes::from_static(b"same")),
        ("resource-b", Bytes::from_static(b"different")),
    ] {
        let output = client
            .regional("eu".to_owned())
            .read(DisplayOnly {
                text: resource,
                calls: Arc::clone(&display_calls),
            })
            .await
            .expect("partitioned request");
        assert_eq!(output, expected);
    }

    assert_eq!(display_calls.load(Ordering::SeqCst), 3);
    let acquisitions = limiter.acquisitions.lock().expect("partitioned captures");
    assert_eq!(acquisitions.len(), 3);
    assert!(acquisitions.iter().all(|plan| plan.len() == 1));
    let expected_first = vec![
        (
            "host".to_owned(),
            "eu.api.partitioned-rate.example.test".to_owned(),
        ),
        ("endpoint".to_owned(), "stable.partitioned.read".to_owned()),
        ("vars.tenant".to_owned(), "tenant-a".to_owned()),
        ("scope.region".to_owned(), "eu".to_owned()),
        ("ep.resource".to_owned(), "resource-a".to_owned()),
    ];
    assert_eq!(acquisitions[0][0].parts, expected_first);
    assert_eq!(acquisitions[0][0].parts, acquisitions[1][0].parts);
    assert_ne!(acquisitions[0][0].parts, acquisitions[2][0].parts);
    assert_eq!(
        acquisitions[2][0].parts.last(),
        Some(&("ep.resource".to_owned(), "resource-b".to_owned()))
    );
}

#[cfg(feature = "rate-limit")]
#[tokio::test(start_paused = true)]
async fn client_clones_share_state() {
    let server = ScriptedServer::start([ResponseScript::ok(b"first".to_vec())])
        .expect("shared-client limiter server");
    let proxy = SafeProxy::parse(server.url("")).expect("shared-client limiter proxy");
    let client = PartitionedRateApi::builder()
        .tenant("tenant-a".to_owned())
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("default-limited partitioned client");
    let clone = client.clone();

    client
        .regional("eu".to_owned())
        .read(DisplayOnly {
            text: "resource-a",
            calls: Arc::new(AtomicUsize::new(0)),
        })
        .await
        .expect("first request");

    let repeated = tokio::spawn(async move {
        clone
            .regional("eu".to_owned())
            .read(DisplayOnly {
                text: "resource-a",
                calls: Arc::new(AtomicUsize::new(0)),
            })
            .await
    });
    tokio::task::yield_now().await;

    assert!(
        !repeated.is_finished(),
        "a client clone must retain the built client's limiter object"
    );
    assert_eq!(
        server.request_count(),
        1,
        "the repeated request must wait before transport"
    );
    repeated.abort();
    let _ = repeated.await;
}

#[tokio::test]
async fn client_rule_by_endpoint_separates_endpoints() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"first".to_vec()),
        ResponseScript::ok(b"second".to_vec()),
    ])
    .expect("endpoint-key server");
    let proxy = SafeProxy::parse(server.url("")).expect("endpoint-key proxy");
    let limiter = Arc::new(CapturingLimiter::default());
    let configured = Arc::clone(&limiter);
    let client = EndpointBucketRateApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.rate_limiter(configured))
        .build()
        .expect("endpoint-key client");

    client.first().await.expect("first endpoint");
    client.second().await.expect("second endpoint");

    let acquisitions = limiter.acquisitions.lock().expect("endpoint captures");
    assert_eq!(acquisitions.len(), 2);
    assert_eq!(acquisitions[0][0].id, acquisitions[1][0].id);
    assert_eq!(
        acquisitions[0][0].parts,
        [("endpoint".to_owned(), "stable.endpoint.first".to_owned())]
    );
    assert_eq!(
        acquisitions[1][0].parts,
        [("endpoint".to_owned(), "stable.endpoint.second".to_owned())]
    );
}

#[tokio::test]
async fn endpoint_rule_by_argument_separates_values() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"first".to_vec()),
        ResponseScript::ok(b"second".to_vec()),
    ])
    .expect("endpoint-value server");
    let proxy = SafeProxy::parse(server.url("")).expect("endpoint-value proxy");
    let limiter = Arc::new(CapturingLimiter::default());
    let configured = Arc::clone(&limiter);
    let client = PartitionedRateApi::builder()
        .tenant("tenant-a".to_owned())
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.rate_limiter(configured))
        .build()
        .expect("endpoint-value client");

    for resource in ["resource-a", "resource-b"] {
        client
            .regional("eu".to_owned())
            .read(DisplayOnly {
                text: resource,
                calls: Arc::new(AtomicUsize::new(0)),
            })
            .await
            .expect("endpoint-value request");
    }

    let acquisitions = limiter
        .acquisitions
        .lock()
        .expect("endpoint-value captures");
    assert_eq!(acquisitions[0][0].id, acquisitions[1][0].id);
    assert_ne!(acquisitions[0][0].parts, acquisitions[1][0].parts);
    assert_eq!(
        acquisitions[0][0].parts.last(),
        Some(&("ep.resource".to_owned(), "resource-a".to_owned()))
    );
    assert_eq!(
        acquisitions[1][0].parts.last(),
        Some(&("ep.resource".to_owned(), "resource-b".to_owned()))
    );
}

#[tokio::test]
async fn generated_key_limit_first_excess_is_redacted() {
    const KEY_SENTINEL: &str = "resolved-key-private-7c41";

    let server = ScriptedServer::start([ResponseScript::ok(b"unexpected".to_vec())])
        .expect("key-limit server");
    let proxy = SafeProxy::parse(server.url("")).expect("key-limit proxy");
    let limiter = Arc::new(SmallKeyLimiter::default());
    let configured = Arc::clone(&limiter);
    let client = PartitionedRateApi::builder()
        .tenant("tenant-a".to_owned())
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.rate_limiter(configured))
        .build()
        .expect("small-key-limit client");
    let error = client
        .regional("eu".to_owned())
        .read(DisplayOnly {
            text: KEY_SENTINEL,
            calls: Arc::new(AtomicUsize::new(0)),
        })
        .await
        .expect_err("resolved key above selected limit");

    assert_eq!(
        error.configuration_kind(),
        Some(ConfigurationErrorKind::InvalidRateLimit)
    );
    assert!(!error.to_string().contains(KEY_SENTINEL));
    assert!(!format!("{error:?}").contains(KEY_SENTINEL));
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 0);
    assert_eq!(server.request_count(), 0);
}

#[tokio::test]
async fn custom_limiter_receives_canonical_ordered_plan_and_owns_observation() {
    let server = ScriptedServer::start([ResponseScript::ok(b"granted".to_vec())])
        .expect("rate-limit server");
    let proxy = SafeProxy::parse(server.url("")).expect("local proxy");
    let limiter = Arc::new(CapturingLimiter::default());
    let configured = Arc::clone(&limiter);
    let client = builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.rate_limiter(configured))
        .build()
        .expect("custom-limited generated client");

    let body = client
        .regions("eu".to_owned())
        .read("item-42".to_owned())
        .await
        .expect("granted application request");
    assert_eq!(body, Bytes::from_static(b"granted"));
    let acquisitions = limiter.acquisitions.lock().expect("acquisition capture");
    assert_eq!(acquisitions.len(), 1);
    let rules = &acquisitions[0];
    assert_eq!(rules.len(), 3);
    assert!(rules[0].id.ends_with("rate_api.rate_limit"));
    assert!(rules[1].id.ends_with("rate_api.regions.rate_limit"));
    assert!(rules[2].id.ends_with("rate_api.regions.read.rate_limit"));
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
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.rate_limiter(configured))
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
        .configure_http({
            let proxy = SafeProxy::parse(server.url("")).expect("first identity proxy");
            move |http| http.proxy(proxy)
        })
        .configure_runtime({
            let limiter = Arc::clone(&limiter);
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()
        .expect("first identity client");
    let clone = first.clone();
    clone.direct().await.expect("clone request");
    let second = IdentityRateApi::builder()
        .configure_http({
            let proxy = SafeProxy::parse(server.url("")).expect("second identity proxy");
            move |http| http.proxy(proxy)
        })
        .configure_runtime({
            let limiter = Arc::clone(&limiter);
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()
        .expect("independent identity client");
    second.direct().await.expect("independent request");

    let nested = nested_rate::NestedRateApi::builder()
        .configure_http({
            let proxy = SafeProxy::parse(server.url("")).expect("nested identity proxy");
            move |http| http.proxy(proxy)
        })
        .configure_runtime({
            let limiter = Arc::clone(&limiter);
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()
        .expect("nested-module client");
    nested.read().await.expect("nested-module request");

    let first_repeated = repeated_rate::FirstRateApi::builder()
        .configure_http({
            let proxy = SafeProxy::parse(server.url("")).expect("first repeated proxy");
            move |http| http.proxy(proxy)
        })
        .configure_runtime({
            let limiter = Arc::clone(&limiter);
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()
        .expect("first repeated client");
    first_repeated.read().await.expect("first repeated request");

    let second_repeated = repeated_rate::SecondRateApi::builder()
        .configure_http({
            let proxy = SafeProxy::parse(server.url("")).expect("second repeated proxy");
            move |http| http.proxy(proxy)
        })
        .configure_runtime({
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
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime({
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
        .configure_http({
            let proxy = SafeProxy::parse(server.url("")).expect("unrelated bucket proxy");
            move |http| http.proxy(proxy)
        })
        .configure_runtime({
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
    assert_eq!(acquisitions[0][2].id, "stable.identity.sibling.rate_limit");
    assert_ne!(acquisitions[0][1].id, acquisitions[0][2].id);
    assert_ne!(acquisitions[0][2].id, acquisitions[1][0].id);
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
async fn explicit_noop_authorizes_nonempty_generated_plan() {
    let server = ScriptedServer::start([ResponseScript::ok(b"noop".to_vec())])
        .expect("no-op rate-limit server");
    let proxy = SafeProxy::parse(server.url("")).expect("local proxy");
    let client = builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(NoopRateLimiter::new())))
        .build()
        .expect("explicit no-op is an intentional selection");
    let body = client
        .regions("eu".to_owned())
        .read("item".to_owned())
        .await
        .expect("no-op authorized request");
    assert_eq!(body, Bytes::from_static(b"noop"));
}

#[tokio::test]
async fn no_local_limiter_private_fallback_executes_an_empty_generated_plan() {
    let server =
        ScriptedServer::start([ResponseScript::ok(b"empty".to_vec())]).expect("empty-plan server");
    let proxy = SafeProxy::parse(server.url("")).expect("local proxy");
    let client = EmptyRateApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("an empty plan needs no explicit limiter");
    assert_eq!(
        client.read().await.expect("empty-plan request"),
        Bytes::from_static(b"empty")
    );
}

#[tokio::test]
async fn unreferenced_non_display_values_remain_valid() {
    let server = ScriptedServer::start([ResponseScript::ok(b"display".to_vec())])
        .expect("display-bound server");
    let proxy = SafeProxy::parse(server.url("")).expect("local proxy");
    let client = MinimalBoundsApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(NoopRateLimiter::new())))
        .build()
        .expect("minimal-bound client");
    assert_eq!(
        client.read(NoDisplay).await.expect("displayless request"),
        Bytes::from_static(b"display")
    );
}

#[test]
fn nested_and_repeated_macro_invocations_keep_rate_contracts_isolated() {
    let nested = nested_rate::NestedRateApi::builder()
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(NoopRateLimiter::new())))
        .build()
        .expect("nested rate client");
    let first = repeated_rate::FirstRateApi::builder()
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(NoopRateLimiter::new())))
        .build()
        .expect("first repeated rate client");
    let second = repeated_rate::SecondRateApi::builder()
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(NoopRateLimiter::new())))
        .build()
        .expect("second repeated rate client");
    let _: nested_rate::nested_rate_api::ReadRequest = nested.read();
    let _: repeated_rate::first_rate_api::ReadRequest = first.read();
    let _: repeated_rate::second_rate_api::ReadRequest = second.read();
}

#[tokio::test]
async fn real_application_send_cannot_start_before_custom_limiter_grant() {
    let server = ScriptedServer::start([ResponseScript::ok(b"after grant".to_vec())])
        .expect("gated loopback server");
    let proxy = SafeProxy::parse(server.url("")).expect("local proxy");
    let limiter = Arc::new(GatedLimiter::new());
    let configured: Arc<dyn RateLimiter> = limiter.clone();
    let client = builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.rate_limiter(configured))
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
