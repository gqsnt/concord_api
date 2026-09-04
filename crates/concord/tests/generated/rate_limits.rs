#![allow(non_snake_case)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::{fmt, fmt::Display};

use concord::Bytes;
use concord::api;
use concord::error::{ClientBuildErrorKind, ConfigurationErrorKind};
use concord::http::SafeProxy;
#[cfg(feature = "rate-limit")]
use concord::rate_limit::LocalRateLimiter;
use concord::rate_limit::{
    BoxFuture, CanonicalKeyWriter, NoopRateLimiter, RateKeyPart, RateLimitError, RateLimitPermit,
    RateLimitPlan, RateLimitRequest, RateLimitResponse, RateLimiter,
};
use concord::retry::{RetryDecision, RetryFailure, RetryPolicy};
use concord_test_support::server::{ResponseScript, ScriptedServer};

api! {
    client RateApi {
        timeout 30s;
        base "http://rate-limit.example.test/";
        values {
            tenant: String;
        }
        rate_limit { id "concord.rate_limits.1"; key [origin]; cost 1;
            allow 500 per 10s;
            allow 30000 per 10m;
        }


    scope regions(region: String) {
        rate_limit { id "concord.rate_limits.2"; key [origin]; cost 1;
            allow 20 per 1s;
        }
        GET read(item: String) -> Bytes via Binary {
            rate_limit { id "concord.rate_limits.3"; key [origin];
                cost 2;
                allow 100 per 1m;
            }
        }
        GET public() -> Bytes via Binary;
    }
}}

api! {
    client EmptyRateApi {
        timeout 30s;
        base "http://empty-rate.example.test/";


    GET read() -> Bytes via Binary;
}}

pub struct CanonicalOnly {
    text: &'static str,
    calls: Arc<AtomicUsize>,
}

impl RateKeyPart for CanonicalOnly {
    fn write_canonical(&self, out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        out.write_tagged_bytes("test.resource/v1", self.text.as_bytes())
    }
}

pub struct NoDisplay;

api! {
    client MinimalBoundsApi {
        timeout 30s;
        base "http://minimal-rate.example.test/";


    GET read(item: crate::NoDisplay) -> Bytes via Binary {
        rate_limit { id "concord.rate_limits.4"; key [origin]; cost 1;  allow 1 per 1s; }
    }
}}

api! {
    client IdentityRateApi {
        timeout 30s;
        base "http://identity-rate.example.test/";
        rate_limit { id "concord.rate_limits.5"; key [origin]; cost 1;  allow 100 per 1h; }


    GET direct() -> Bytes via Binary;
    scope outer(region: String) {
        rate_limit { id "concord.rate_limits.6"; key [origin]; cost 1;  allow 20 per 1h; }
        GET immediate() -> Bytes via Binary;
        scope inner(kind: String) {
            rate_limit { id "concord.rate_limits.7"; key [origin]; cost 1;  allow 10 per 1h; }
            GET nested() -> Bytes via Binary {
                rate_limit { id "concord.rate_limits.8"; key [origin];  cost 2; allow 30 per 1h; }
            }
        }
    }
    scope sibling(region: String) {
        rate_limit { id "concord.rate_limits.9"; key [origin]; cost 1;  allow 7 per 1h; }
        GET sibling_endpoint() -> Bytes via Binary {
            id "stable.identity.sibling";
            rate_limit { id "concord.rate_limits.10"; key [origin]; cost 1;  allow 5 per 1h; }
        }
    }
    GET outside() -> Bytes via Binary;
}}

api! {
    client ExplicitStableRateApi {
        timeout 30s;
        base "http://explicit-stable-rate.example.test/";


    scope legacy_branch() {
        rate_limit { key [origin];
            id "service.shared.quota";
            cost 1;
            allow 2 per 1h;
        }
        GET first() -> Bytes via Binary;
    }

    scope renamed_branch() {
        rate_limit { key [origin];
            cost 2;
            id "service.shared.quota";
            allow 2 per 1h;
        }
        GET second() -> Bytes via Binary;
    }
}}

api! {
    client NamedRuleRateApi {
        timeout 30s;
        base "http://named-rule-rate.example.test/";

        rate_limits {
            policy account_reads(account) { id "concord.rate_limits.13"; cost 1;
                key [account];
                allow 2 per 1h;
            }
        }


    scope accounts(account: String) {
        rate_limit account_reads(account = accounts.account);

        GET read() -> Bytes via Binary;
    }

    scope profiles(account: String) {
        rate_limit account_reads(account = profiles.account);

        GET inspect() -> Bytes via Binary;
    }

    GET lookup(account: String) -> Bytes via Binary {
        rate_limit account_reads(account = endpoint.account) { cost 2; }
    }
}}

api! {
    client FirstSharedIdApi {
        timeout 30s;
        base "http://first-shared-id.example.test/";


    GET read() -> Bytes via Binary {
        rate_limit { key [origin]; cost 1;
            id "service.cross_contract.quota";
            allow 1 per 1h;
        }
    }
}}

api! {
    client SecondSharedIdApi {
        timeout 30s;
        base "http://second-shared-id.example.test/";


    GET renamed_read() -> Bytes via Binary {
        rate_limit { key [origin]; cost 1;
            id "service.cross_contract.quota";
            allow 1 per 1h;
        }
    }
}}

api! {
    client ClientBucketRateApi {
        timeout 30s;
        base "http://client-bucket.example.test/";
        rate_limit { id "concord.rate_limits.16"; key [origin]; cost 1;  allow 1 per 1h; }


    GET direct() -> Bytes via Binary;
    scope descendant(value: String) {
        GET nested() -> Bytes via Binary;
    }
}}

api! {
    client ScopeBucketRateApi {
        timeout 30s;
        base "http://scope-bucket.example.test/";
        rate_limit { id "concord.rate_limits.17"; key [origin]; cost 1;  allow 100 per 1h; }


    scope branch(value: String) {
        rate_limit { id "concord.rate_limits.18"; key [origin]; cost 1;  allow 1 per 1h; }
        GET immediate() -> Bytes via Binary;
        scope nested(value: String) {
            GET descendant() -> Bytes via Binary;
        }
    }
    GET outside() -> Bytes via Binary;
}}

api! {
    client HostBucketRateApi {
        timeout 30s;
        base "http://host-bucket.example.test/";


    scope region(region: String) {
        subdomain replace [region.region, "api"];
        rate_limit { id "concord.rate_limits.19"; cost 1;
            key [origin];
            allow 100 per 1h;
        }
        GET read() -> Bytes via Binary;
    }
}}

api! {
    client ScopeValueBucketRateApi {
        timeout 30s;
        base "http://scope-value-bucket.example.test/";


    scope account(account: String) {
        rate_limit { id "concord.rate_limits.20"; cost 1;
            key [account.account];
            allow 100 per 1h;
        }
        GET read() -> Bytes via Binary;
    }
}}

api! {
    client CollisionRateApi {
        timeout 30s;
        base "http://collision-rate.example.test/";


    scope a__b(value: String) {
        rate_limit { id "concord.rate_limits.21"; key [origin]; cost 1;  allow 1 per 1h; }
        GET first() -> Bytes via Binary;
    }
    scope a(value: String) {
        scope b(value: String) {
            rate_limit { id "concord.rate_limits.22"; key [origin]; cost 1;  allow 1 per 1h; }
            GET second() -> Bytes via Binary;
        }
    }
}}

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

impl concord::RequestAtom for RouteHost {
    fn write_atom(
        &self,
        output: &mut concord::BoundedAtomWriter<'_>,
    ) -> Result<(), concord::AtomError> {
        output.push_str(match self {
            Self::Platform => "platform",
            Self::Regional => "regional",
        })
    }
}

api! {
    client RouteFamilyApi {
        timeout 30s;
        base "http://route-family.example.test/";
        values {
            platform: crate::RouteHost = crate::RouteHost::Platform;
            regional: crate::RouteHost = crate::RouteHost::Regional;
        }


    scope platform() {
        subdomain replace [client.platform, "api"];
        rate_limit { id "concord.rate_limits.23"; key [origin]; cost 1;  allow 100 per 1h; }
        GET read() -> Bytes via Binary;
    }

    scope regional() {
        subdomain replace [client.regional, "api"];
        rate_limit { id "concord.rate_limits.24"; key [origin]; cost 1;  allow 100 per 1h; }
        GET read() -> Bytes via Binary;
    }
}}

api! {
    client PartitionedRateApi {
        timeout 30s;
        base "http://partitioned-rate.example.test/";
        values {
            tenant: String;
        }


    scope regional(region: String) {
        subdomain replace [regional.region, "api"];

        GET read(resource: crate::CanonicalOnly) -> Bytes via Binary {
            id "stable.partitioned.read";
            rate_limit { id "concord.rate_limits.25"; cost 1;
                key [origin, endpoint, client.tenant, regional.region, endpoint.resource];
                allow 1 per 1h;
            }
        }
    }
}}

api! {
    client EndpointBucketRateApi {
        timeout 30s;
        base "http://endpoint-bucket.example.test/";
        rate_limit { id "concord.rate_limits.26"; cost 1;
            key [endpoint];
            allow 100 per 1h;
        }


    GET first() -> Bytes via Binary {
        id "stable.endpoint.first";
    }

    GET second() -> Bytes via Binary {
        id "stable.endpoint.second";
    }
}}

mod nested_rate {
    use concord::api;

    api! {
        client NestedRateApi {
            timeout 30s;
            base "http://nested-rate.example.test/";
            rate_limit { id "concord.rate_limits.27"; key [origin]; cost 1;  allow 1 per 1s; }


        GET read() -> Bytes via Binary;
    }}
}

mod repeated_rate {
    use concord::api;

    api! {
        client FirstRateApi {
            timeout 30s;
            base "http://first-rate.example.test/";
            rate_limit { id "concord.rate_limits.28"; key [origin]; cost 1;  allow 1 per 1s; }


        GET read() -> Bytes via Binary;
    }}

    api! {
        client SecondRateApi {
            timeout 30s;
            base "http://second-rate.example.test/";
            rate_limit { id "concord.rate_limits.29"; key [origin]; cost 1;  allow 1 per 1s; }


        GET read() -> Bytes via Binary;
    }}
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CapturedRule {
    id: String,
    parts: Vec<(String, Vec<u8>)>,
    cost: u32,
    windows: Vec<(u32, std::time::Duration)>,
}

fn canonical_utf8(value: &str) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(1 + 4 + 8 + value.len());
    encoded.push(4);
    encoded.extend_from_slice(b"utf8");
    encoded.extend_from_slice(&(value.len() as u64).to_be_bytes());
    encoded.extend_from_slice(value.as_bytes());
    encoded
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
        plan.policies()
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

    fn observe(&self, response: RateLimitResponse<'_>) {
        let captured = Self::capture(response.request.plan);
        self.observations
            .lock()
            .expect("observation capture lock")
            .push(captured);
    }
}

fn builder() -> RateApiBuilder {
    RateApi::builder().set_tenant("tenant-west".to_owned())
}

#[cfg(feature = "rate-limit")]
fn local_limiter() -> Arc<dyn RateLimiter> {
    Arc::new(LocalRateLimiter::new())
}

#[test]
fn generated_rate_limited_clients_fail_closed_without_an_explicit_limiter() {
    let error = match builder().build() {
        Ok(_) => panic!("rate-limited client unexpectedly built without a limiter"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), ClientBuildErrorKind::LimiterUnavailable);
}

#[tokio::test]
async fn sibling_route_families_resolve_distinct_origins_and_explicit_policy_ids() {
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
    assert_eq!(acquisitions[0][0].id, "concord.rate_limits.23");
    assert_eq!(acquisitions[1][0].id, "concord.rate_limits.24");
    assert_ne!(acquisitions[0][0].id, acquisitions[1][0].id);
    assert_ne!(acquisitions[0][0].parts, acquisitions[1][0].parts);
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
    assert_eq!(acquisitions[0][0].parts, acquisitions[1][0].parts);
    assert_eq!(acquisitions[0][0].parts[0].0, "origin");
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
    assert_eq!(acquisitions[0][1].parts, acquisitions[1][1].parts);
    assert_eq!(acquisitions[0][1].parts[0].0, "origin");
}

#[tokio::test]
async fn scope_rule_by_origin_separates_logical_origins() {
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
            "origin".to_owned(),
            canonical_utf8("http://eu.api.host-bucket.example.test:80")
        )]
    );
    assert_eq!(
        acquisitions[1][0].parts,
        [(
            "origin".to_owned(),
            canonical_utf8("http://us.api.host-bucket.example.test:80")
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
        [("account.account".to_owned(), canonical_utf8("account-a"))]
    );
    assert_eq!(
        acquisitions[1][0].parts,
        [("account.account".to_owned(), canonical_utf8("account-b"))]
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
        .set_tenant("tenant-a".to_owned())
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.rate_limiter(configured))
        .build()
        .expect("partitioned-key client");
    let canonical_calls = Arc::new(AtomicUsize::new(0));

    for (resource, expected) in [
        ("resource-a", Bytes::from_static(b"first")),
        ("resource-a", Bytes::from_static(b"same")),
        ("resource-b", Bytes::from_static(b"different")),
    ] {
        let output = client
            .regional("eu".to_owned())
            .read(CanonicalOnly {
                text: resource,
                calls: Arc::clone(&canonical_calls),
            })
            .await
            .expect("partitioned request");
        assert_eq!(output, expected);
    }

    assert_eq!(canonical_calls.load(Ordering::SeqCst), 3);
    let acquisitions = limiter.acquisitions.lock().expect("partitioned captures");
    assert_eq!(acquisitions.len(), 3);
    assert!(acquisitions.iter().all(|plan| plan.len() == 1));
    assert_eq!(
        acquisitions[0][0]
            .parts
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        [
            "origin",
            "endpoint",
            "client.tenant",
            "regional.region",
            "endpoint.resource",
        ]
    );
    assert_eq!(acquisitions[0][0].parts, acquisitions[1][0].parts);
    assert_ne!(acquisitions[0][0].parts, acquisitions[2][0].parts);
    assert_eq!(
        acquisitions[2][0]
            .parts
            .last()
            .map(|(name, _)| name.as_str()),
        Some("endpoint.resource")
    );
}

#[cfg(feature = "rate-limit")]
#[tokio::test]
async fn client_clones_share_state() {
    let server = ScriptedServer::start([ResponseScript::ok(b"first".to_vec())])
        .expect("shared-client limiter server");
    let proxy = SafeProxy::parse(server.url("")).expect("shared-client limiter proxy");
    let client = PartitionedRateApi::builder()
        .set_tenant("tenant-a".to_owned())
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(|runtime| runtime.rate_limiter(local_limiter()))
        .build()
        .expect("default-limited partitioned client");
    let clone = client.clone();

    client
        .regional("eu".to_owned())
        .read(CanonicalOnly {
            text: "resource-a",
            calls: Arc::new(AtomicUsize::new(0)),
        })
        .await
        .expect("first request");

    let repeated = tokio::spawn(async move {
        clone
            .regional("eu".to_owned())
            .read(CanonicalOnly {
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
        [(
            "endpoint".to_owned(),
            canonical_utf8("stable.endpoint.first")
        )]
    );
    assert_eq!(
        acquisitions[1][0].parts,
        [(
            "endpoint".to_owned(),
            canonical_utf8("stable.endpoint.second")
        )]
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
        .set_tenant("tenant-a".to_owned())
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.rate_limiter(configured))
        .build()
        .expect("endpoint-value client");

    for resource in ["resource-a", "resource-b"] {
        client
            .regional("eu".to_owned())
            .read(CanonicalOnly {
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
        acquisitions[0][0]
            .parts
            .last()
            .map(|(name, _)| name.as_str()),
        Some("endpoint.resource")
    );
    assert_eq!(
        acquisitions[1][0]
            .parts
            .last()
            .map(|(name, _)| name.as_str()),
        Some("endpoint.resource")
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
        .set_tenant("tenant-a".to_owned())
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.rate_limiter(configured))
        .build()
        .expect("small-key-limit client");
    let error = client
        .regional("eu".to_owned())
        .read(CanonicalOnly {
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
    assert_eq!(rules[0].id, "concord.rate_limits.1");
    assert_eq!(rules[1].id, "concord.rate_limits.2");
    assert_eq!(rules[2].id, "concord.rate_limits.3");
    assert!(rules.windows(2).all(|pair| pair[0].id != pair[1].id));
    assert!(
        rules
            .iter()
            .all(|rule| { rule.parts.len() == 1 && rule.parts[0].0 == "origin" })
    );
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
async fn inherited_declarations_keep_explicit_identity_across_descendant_modules() {
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
    assert_eq!(direct_client_id, "concord.rate_limits.5");
    assert_eq!(direct_client_id, &acquisitions[1][0].id);
    assert_eq!(direct_client_id, &acquisitions[2][0].id);
    assert_eq!(direct_client_id, &acquisitions[3][0].id);
    assert_eq!(direct_client_id, &acquisitions[4][0].id);

    let outer_id = &acquisitions[1][1].id;
    assert_eq!(outer_id, "concord.rate_limits.6");
    assert_eq!(outer_id, &acquisitions[2][1].id);
    assert_ne!(outer_id, &acquisitions[3][1].id);
    assert_ne!(outer_id, direct_client_id);
    assert!(!acquisitions[2][2].id.is_empty());
    assert_eq!(acquisitions[2][2].id, "concord.rate_limits.7");
    assert_eq!(acquisitions[2][2].cost, 1);
    assert_eq!(
        acquisitions[2][2].windows,
        [(10, std::time::Duration::from_secs(3600))]
    );
    assert_eq!(acquisitions[2][3].cost, 2);
    assert_eq!(acquisitions[2][3].id, "concord.rate_limits.8");
    assert_eq!(
        acquisitions[2][3].windows,
        [(30, std::time::Duration::from_secs(3600))]
    );
    assert_eq!(acquisitions[3].len(), 3);
    assert_ne!(acquisitions[3][1].id, acquisitions[1][1].id);
    assert_ne!(acquisitions[3][2].id, acquisitions[2][3].id);
    assert!(acquisitions.iter().all(|plan| {
        plan.iter()
            .all(|rule| rule.parts.len() == 1 && rule.parts[0].0 == "origin")
    }));
}

#[tokio::test]
async fn explicit_id_is_exact_and_survives_source_renames_and_cost_changes() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"legacy".to_vec()),
        ResponseScript::ok(b"renamed".to_vec()),
    ])
    .expect("explicit-ID server");
    let limiter = Arc::new(CapturingLimiter::default());
    let client = ExplicitStableRateApi::builder()
        .configure_http({
            let proxy = SafeProxy::parse(server.url("")).expect("explicit-ID proxy");
            move |http| http.proxy(proxy)
        })
        .configure_runtime({
            let limiter = Arc::clone(&limiter);
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()
        .expect("explicit-ID client");

    client
        .legacy_branch()
        .first()
        .await
        .expect("legacy branch request");
    client
        .renamed_branch()
        .second()
        .await
        .expect("renamed branch request");

    let acquisitions = limiter
        .acquisitions
        .lock()
        .expect("explicit-ID acquisitions");
    assert_eq!(acquisitions.len(), 2);
    assert_eq!(acquisitions[0][0].id, "service.shared.quota");
    assert_eq!(acquisitions[1][0].id, "service.shared.quota");
    assert_eq!(acquisitions[0][0].cost, 1);
    assert_eq!(acquisitions[1][0].cost, 2);
    assert_eq!(acquisitions[0][0].windows, acquisitions[1][0].windows);
}

#[tokio::test]
async fn named_rule_bindings_emit_one_stable_logical_schema_across_branches() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"scope".to_vec()),
        ResponseScript::ok(b"sibling".to_vec()),
        ResponseScript::ok(b"endpoint".to_vec()),
    ])
    .expect("named-rule capture server");
    let limiter = Arc::new(CapturingLimiter::default());
    let client = NamedRuleRateApi::builder()
        .configure_http({
            let proxy = SafeProxy::parse(server.url("")).expect("named-rule capture proxy");
            move |http| http.proxy(proxy)
        })
        .configure_runtime({
            let limiter = Arc::clone(&limiter);
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()
        .expect("named-rule capture client");

    client
        .accounts("shared-account".to_owned())
        .read()
        .await
        .expect("scope attachment");
    client
        .profiles("shared-account".to_owned())
        .inspect()
        .await
        .expect("sibling-scope attachment");
    client
        .lookup("shared-account".to_owned())
        .await
        .expect("endpoint attachment");

    let acquisitions = limiter
        .acquisitions
        .lock()
        .expect("named-rule acquisitions");
    assert_eq!(acquisitions.len(), 3);
    assert!(acquisitions.iter().all(|plan| plan.len() == 1));
    assert_eq!(acquisitions[0][0].id, "concord.rate_limits.13");
    assert_eq!(acquisitions[0][0].id, acquisitions[1][0].id);
    assert_eq!(acquisitions[0][0].id, acquisitions[2][0].id);
    assert_eq!(
        acquisitions[0][0].parts,
        [("account".to_owned(), canonical_utf8("shared-account"))]
    );
    assert_eq!(acquisitions[0][0].parts, acquisitions[1][0].parts);
    assert_eq!(acquisitions[0][0].parts, acquisitions[2][0].parts);
    assert_eq!(acquisitions[0][0].cost, 1);
    assert_eq!(acquisitions[1][0].cost, 1);
    assert_eq!(acquisitions[2][0].cost, 2);
    assert_eq!(acquisitions[0][0].windows, acquisitions[1][0].windows);
    assert_eq!(acquisitions[0][0].windows, acquisitions[2][0].windows);
}

#[cfg(feature = "rate-limit")]
#[tokio::test]
async fn named_rule_attachments_share_actual_local_limiter_bucket_state() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"first".to_vec()),
        ResponseScript::ok(b"different".to_vec()),
    ])
    .expect("named-rule shared-state server");
    let client = NamedRuleRateApi::builder()
        .configure_http({
            let proxy = SafeProxy::parse(server.url("")).expect("named-rule shared-state proxy");
            move |http| http.proxy(proxy)
        })
        .configure_runtime(|runtime| runtime.rate_limiter(local_limiter()))
        .build()
        .expect("named-rule shared-state client");

    client
        .accounts("shared-account".to_owned())
        .read()
        .await
        .expect("first unit from shared bucket");
    let waiting_client = client.clone();
    let waiting =
        tokio::spawn(async move { waiting_client.lookup("shared-account".to_owned()).await });
    tokio::task::yield_now().await;

    assert!(!waiting.is_finished());
    assert_eq!(server.request_count(), 1);

    // A ready task prevents Tokio's paused clock from auto-advancing while the
    // independent request waits on the loopback server's real socket.
    let clock_guard = tokio::spawn(async {
        loop {
            tokio::task::yield_now().await;
        }
    });
    client
        .lookup("different-account".to_owned())
        .await
        .expect("different logical key retains its own capacity");
    assert_eq!(server.request_count(), 2);
    assert!(!waiting.is_finished());

    clock_guard.abort();
    let _ = clock_guard.await;

    waiting.abort();
    let _ = waiting.await;
}

#[tokio::test]
async fn policy_retry_reacquires_the_same_explicit_rule_identity() {
    let server = ScriptedServer::start([
        ResponseScript::status(503, b"retry".to_vec()),
        ResponseScript::ok(b"accepted".to_vec()),
    ])
    .expect("explicit-ID retry server");
    let limiter = Arc::new(CapturingLimiter::default());
    let policy = RetryPolicy::builder()
        .max_retries_per_operation(1)
        .without_budget()
        .classify_fn(|attempt| match attempt.failure() {
            RetryFailure::RejectedStatus { status, .. }
                if status == concord::http::StatusCode::SERVICE_UNAVAILABLE =>
            {
                RetryDecision::Retry
            }
            _ => RetryDecision::Stop,
        })
        .build()
        .expect("explicit-ID retry policy");
    let client = ExplicitStableRateApi::builder()
        .configure_http({
            let proxy = SafeProxy::parse(server.url("")).expect("explicit-ID retry proxy");
            move |http| http.proxy(proxy)
        })
        .retry(policy)
        .configure_runtime({
            let limiter = Arc::clone(&limiter);
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()
        .expect("explicit-ID retry client");

    assert_eq!(
        client
            .legacy_branch()
            .first()
            .await
            .expect("retried explicit-ID request"),
        Bytes::from_static(b"accepted")
    );
    let acquisitions = limiter
        .acquisitions
        .lock()
        .expect("explicit-ID retry acquisitions");
    assert_eq!(acquisitions.len(), 2);
    assert_eq!(acquisitions[0][0].id, "service.shared.quota");
    assert_eq!(acquisitions[0][0], acquisitions[1][0]);
}

#[cfg(feature = "rate-limit")]
#[tokio::test]
async fn shared_limiter_reuses_one_explicit_id_across_generated_contracts() {
    let server = ScriptedServer::start([ResponseScript::ok(b"first".to_vec())])
        .expect("cross-contract shared limiter server");
    let limiter: Arc<dyn RateLimiter> = Arc::new(LocalRateLimiter::new());
    let first = FirstSharedIdApi::builder()
        .configure_http({
            let proxy = SafeProxy::parse(server.url("")).expect("first shared-ID proxy");
            move |http| http.proxy(proxy)
        })
        .configure_runtime({
            let limiter = Arc::clone(&limiter);
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()
        .expect("first shared-ID client");
    let second = SecondSharedIdApi::builder()
        .configure_http({
            let proxy = SafeProxy::parse(server.url("")).expect("second shared-ID proxy");
            move |http| http.proxy(proxy)
        })
        .configure_runtime(move |runtime| runtime.rate_limiter(limiter))
        .build()
        .expect("second shared-ID client");

    first.read().await.expect("first cross-contract request");
    let waiting = tokio::spawn(async move { second.renamed_read().await });
    tokio::task::yield_now().await;

    assert!(!waiting.is_finished());
    assert_eq!(server.request_count(), 1);
    waiting.abort();
    let _ = waiting.await;
}

#[cfg(feature = "rate-limit")]
#[tokio::test]
async fn independent_clients_with_the_same_explicit_id_keep_isolated_state() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"first".to_vec()),
        ResponseScript::ok(b"second".to_vec()),
    ])
    .expect("independent explicit-ID server");
    let first = FirstSharedIdApi::builder()
        .configure_http({
            let proxy = SafeProxy::parse(server.url("")).expect("first independent proxy");
            move |http| http.proxy(proxy)
        })
        .configure_runtime(|runtime| runtime.rate_limiter(local_limiter()))
        .build()
        .expect("first independent client");
    let second = SecondSharedIdApi::builder()
        .configure_http({
            let proxy = SafeProxy::parse(server.url("")).expect("second independent proxy");
            move |http| http.proxy(proxy)
        })
        .configure_runtime(|runtime| runtime.rate_limiter(local_limiter()))
        .build()
        .expect("second independent client");

    first.read().await.expect("first independent request");
    second
        .renamed_read()
        .timeout(std::time::Duration::from_secs(5))
        .await
        .expect("second independent request must not inherit first-client quota debt");
    assert_eq!(server.request_count(), 2);
}

#[tokio::test]
async fn explicit_policy_identities_repeat_within_one_contract_and_differ_by_declaration() {
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
    assert_eq!(acquisitions[0][0].id, "concord.rate_limits.5");
    assert_ne!(acquisitions[0][0].id, acquisitions[2][0].id);
    assert_eq!(acquisitions[2][0].id, "concord.rate_limits.27");
    assert_ne!(acquisitions[3][0].id, acquisitions[4][0].id);
    assert_eq!(acquisitions[3][0].id, "concord.rate_limits.28");
    assert_eq!(acquisitions[4][0].id, "concord.rate_limits.29");
    assert!(
        acquisitions
            .iter()
            .all(|plan| { plan[0].parts.len() == 1 && plan[0].parts[0].0 == "origin" })
    );
}

#[tokio::test]
async fn punctuation_distinct_paths_keep_distinct_explicit_policies() {
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
    assert_eq!(acquisitions[0][0].id, "concord.rate_limits.21");
    assert_eq!(acquisitions[1][0].id, "concord.rate_limits.22");
    assert!(
        acquisitions
            .iter()
            .flatten()
            .all(|rule| rule.parts.len() == 1 && rule.parts[0].0 == "origin")
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
    assert_eq!(acquisitions[0][2].id, "concord.rate_limits.10");
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
