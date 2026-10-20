//! Generated rate limits (§12) through downstream-generated v3 clients:
//! the registry, named policies with formals and exact bindings, inline and
//! window-set attachments, deduplication, explicit key labels and schemas,
//! typed encoding IDs with the canonical V3 bytes, origin and
//! endpoint dimensions, endpoint identity, limiter capability, atomic
//! admission and the built-in local limiter.
//!
//! Dimension labels are explicit; bare names refer to their current owner.
//! Inline policies have stable IDs, custom limiters declare capabilities and
//! custom key types declare encoding IDs. Every required value has an
//! explicit semantic role in the current contract.

#![allow(non_snake_case)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use concord::error::{ClientBuildErrorKind, ConfigurationErrorKind, FailureStage};
use concord::http::SafeProxy;
use concord::rate_limit::{
    BoxFuture, CanonicalKeyWriter, Enforcement, LimiterCapability, NoopRateLimiter,
    RateDimensionKind, RateKeyEncoder, RateKeyPart, RateLimitError, RateLimitErrorKind,
    RateLimitPermit, RateLimitPlan, RateLimitRequest, RateLimitResponse, RateLimiter,
    RatePolicySchema, UTF8_ENCODING_ID,
};
use concord::retry::{RetryDecision, RetryFailure, RetryPolicy};
use concord::{Binary, Bytes, DispatchKnowledge, api};
use concord_test_support::server::{ResponseScript, ScriptedServer};

#[cfg(feature = "rate-limit")]
use concord::rate_limit::LocalRateLimiter;

// ---------------------------------------------------------------------------
// Declarations
// ---------------------------------------------------------------------------

api! {
    client RateApi {
        timeout 30s;
        base "http://rate-limit.example.test/";

        values {
            query tenant: String;
        }

        rate_limit {
            id "concord.rate_limits.1";
            key [origin];
            cost 1;
            allow 500 per 10s;
            allow 30000 per 10m;
        }

        scope regions(region: String) {
            path "regions/{region}";
            rate_limit { id "concord.rate_limits.2"; key [origin]; cost 1; allow 20 per 1s; }

            GET read(item: String) -> Bytes via Binary {
                path "items/{item}";
                rate_limit { id "concord.rate_limits.3"; key [origin]; cost 2; allow 100 per 1m; }
            }

            GET public() -> Bytes via Binary;
        }
    }
}

api! {
    client EmptyRateApi {
        timeout 30s;
        base "http://empty-rate.example.test/";

        GET read() -> Bytes via Binary;
    }
}

/// A custom key part with its own stable encoding contract.
pub struct CanonicalOnly {
    text: &'static str,
    calls: Arc<AtomicUsize>,
}

impl RateKeyPart for CanonicalOnly {
    const ENCODING_ID: &'static str = "test.resource/1";

    fn write_canonical(&self, out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        out.write_tagged_bytes("test.resource/v1", self.text.as_bytes())
    }
}

/// A value with no `Display`, `RequestAtom` or `RateKeyPart` implementation.
pub struct NoDisplay;

/// The explicit canonical rate-key adapter for [`NoDisplay`].
pub struct NoDisplayKey;

impl RateKeyEncoder<NoDisplay> for NoDisplayKey {
    const ENCODING_ID: &'static str = "test.no-display/1";

    fn write_canonical(
        _value: &NoDisplay,
        out: &mut CanonicalKeyWriter,
    ) -> Result<(), RateLimitError> {
        out.write_tagged_bytes("test.no-display", b"fixed")
    }
}

api! {
    client MinimalBoundsApi {
        timeout 30s;
        base "http://minimal-rate.example.test/";

        GET read(item: NoDisplay) -> Bytes via Binary {
            rate_limit {
                id "concord.rate_limits.4";
                key [origin, "item" = item via NoDisplayKey];
                cost 1;
                allow 1 per 1s;
            }
        }
    }
}

api! {
    client IdentityRateApi {
        timeout 30s;
        base "http://identity-rate.example.test/";

        rate_limit { id "concord.rate_limits.5"; key [origin]; cost 1; allow 100 per 1h; }

        GET direct() -> Bytes via Binary;

        scope outer(region: String) {
            path "outer/{region}";
            rate_limit { id "concord.rate_limits.6"; key [origin]; cost 1; allow 20 per 1h; }

            GET immediate() -> Bytes via Binary;

            scope inner(kind: String) {
                path "inner/{kind}";
                rate_limit { id "concord.rate_limits.7"; key [origin]; cost 1; allow 10 per 1h; }

                GET nested() -> Bytes via Binary {
                    rate_limit { id "concord.rate_limits.8"; key [origin]; cost 2; allow 30 per 1h; }
                }
            }
        }

        scope sibling(region: String) {
            path "sibling/{region}";
            rate_limit { id "concord.rate_limits.9"; key [origin]; cost 1; allow 7 per 1h; }

            GET sibling_endpoint() -> Bytes via Binary {
                id "stable.identity.sibling";
                rate_limit { id "concord.rate_limits.10"; key [origin]; cost 1; allow 5 per 1h; }
            }
        }

        GET outside() -> Bytes via Binary;
    }
}

api! {
    client ExplicitStableRateApi {
        timeout 30s;
        base "http://explicit-stable-rate.example.test/";

        scope legacy_branch() {
            rate_limit { id "service.shared.quota"; key [origin]; cost 1; allow 2 per 1h; }

            GET first() -> Bytes via Binary;
        }

        scope renamed_branch() {
            // Same identity and window schema at an unrelated endpoint; cost
            // is per attachment and is not part of bucket identity (RATE-01).
            rate_limit { id "service.shared.quota"; key [origin]; cost 2; allow 2 per 1h; }

            GET second() -> Bytes via Binary;
        }
    }
}

api! {
    client NamedRuleRateApi {
        timeout 30s;
        base "http://named-rule-rate.example.test/";

        rate_limits {
            policy account_reads(account) {
                id "concord.rate_limits.13";
                key ["account" = account];
                cost 1;
                allow 2 per 1h;
            }
            /// Declared for a future endpoint; inspection reports it unused.
            policy reserved_writes() {
                id "concord.rate_limits.reserved";
                key [origin];
                allow 1 per 1s;
            }
        }

        scope accounts(account: String) {
            rate_limit account_reads(account = account);

            GET read() -> Bytes via Binary;
        }

        scope profiles(account: String) {
            rate_limit account_reads(account = account);

            GET inspect() -> Bytes via Binary;
        }

        GET lookup(account: String) -> Bytes via Binary {
            rate_limit account_reads(account = account) { cost 2; }
        }
    }
}

// Two generated contracts for the same logical origin (different fixed base
// paths) and one for a different origin, all with the same policy ID and
// label schema. `origin` is the complete logical destination (RATE-03), so
// only the first two can share a bucket, and only through one limiter object.
api! {
    client FirstSharedIdApi {
        timeout 30s;
        base "http://shared-id.example.test/first/";

        GET read() -> Bytes via Binary {
            rate_limit { id "service.cross_contract.quota"; key [origin]; cost 1; allow 1 per 1h; }
        }
    }
}

api! {
    client SecondSharedIdApi {
        timeout 30s;
        base "http://shared-id.example.test/second/";

        GET renamed_read() -> Bytes via Binary {
            rate_limit { id "service.cross_contract.quota"; key [origin]; cost 1; allow 1 per 1h; }
        }
    }
}

api! {
    client OtherOriginSharedIdApi {
        timeout 30s;
        base "http://other-shared-id.example.test/";

        GET read() -> Bytes via Binary {
            rate_limit { id "service.cross_contract.quota"; key [origin]; cost 1; allow 1 per 1h; }
        }
    }
}

api! {
    client ClientBucketRateApi {
        timeout 30s;
        base "http://client-bucket.example.test/";

        rate_limit { id "concord.rate_limits.16"; key [origin]; cost 1; allow 1 per 1h; }

        GET direct() -> Bytes via Binary;

        scope descendant(value: String) {
            path "descendant/{value}";

            GET nested() -> Bytes via Binary;
        }
    }
}

api! {
    client ScopeBucketRateApi {
        timeout 30s;
        base "http://scope-bucket.example.test/";

        rate_limit { id "concord.rate_limits.17"; key [origin]; cost 1; allow 100 per 1h; }

        scope branch(value: String) {
            path "branch/{value}";
            rate_limit { id "concord.rate_limits.18"; key [origin]; cost 1; allow 1 per 1h; }

            GET immediate() -> Bytes via Binary;

            scope nested(value: String) {
                path "nested/{value}";

                GET descendant() -> Bytes via Binary;
            }
        }

        GET outside() -> Bytes via Binary;
    }
}

api! {
    client HostBucketRateApi {
        timeout 30s;
        base "http://host-bucket.example.test/";

        scope region(region: String) {
            subdomain replace [region, "api"];
            rate_limit { id "concord.rate_limits.19"; key [origin]; cost 1; allow 100 per 1h; }

            GET read() -> Bytes via Binary;
        }
    }
}

api! {
    client ScopeValueBucketRateApi {
        timeout 30s;
        base "http://scope-value-bucket.example.test/";

        scope account(account: String) {
            rate_limit {
                id "concord.rate_limits.20";
                key ["account.account" = account];
                cost 1;
                allow 100 per 1h;
            }

            GET read() -> Bytes via Binary;
        }
    }
}

api! {
    client CollisionRateApi {
        timeout 30s;
        base "http://collision-rate.example.test/";

        scope a__b(value: String) {
            path "ab/{value}";
            rate_limit { id "concord.rate_limits.21"; key [origin]; cost 1; allow 1 per 1h; }

            GET first() -> Bytes via Binary;
        }

        scope a(value: String) {
            path "a/{value}";

            scope b(value: String) {
                path "b/{value}";
                rate_limit { id "concord.rate_limits.22"; key [origin]; cost 1; allow 1 per 1h; }

                GET second() -> Bytes via Binary;
            }
        }
    }
}

#[derive(Clone, Copy)]
pub enum RouteHost {
    Platform,
    Regional,
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
            platform: RouteHost = RouteHost::Platform;
            regional: RouteHost = RouteHost::Regional;
        }

        scope platform() {
            subdomain replace [client.platform, "api"];
            rate_limit { id "concord.rate_limits.23"; key [origin]; cost 1; allow 100 per 1h; }

            GET read() -> Bytes via Binary;
        }

        scope regional() {
            subdomain replace [client.regional, "api"];
            rate_limit { id "concord.rate_limits.24"; key [origin]; cost 1; allow 100 per 1h; }

            GET read() -> Bytes via Binary;
        }
    }
}

api! {
    client PartitionedRateApi {
        timeout 30s;
        base "http://partitioned-rate.example.test/";

        values {
            tenant: String;
        }

        scope regional(region: String) {
            subdomain replace [region, "api"];

            GET read(resource: CanonicalOnly) -> Bytes via Binary {
                id "stable.partitioned.read";
                rate_limit {
                    id "concord.rate_limits.25";
                    key [
                        origin,
                        endpoint,
                        "client.tenant" = client.tenant,
                        "regional.region" = regional.region,
                        "endpoint.resource" = resource,
                    ];
                    cost 1;
                    allow 1 per 1h;
                }
            }
        }
    }
}

api! {
    client EndpointBucketRateApi {
        timeout 30s;
        base "http://endpoint-bucket.example.test/";

        rate_limit { id "concord.rate_limits.26"; key [endpoint]; cost 1; allow 100 per 1h; }

        GET first() -> Bytes via Binary {
            id "stable.endpoint.first";
        }

        GET second() -> Bytes via Binary {
            id "stable.endpoint.second";
        }
    }
}

mod nested_rate {
    use concord::{Binary, Bytes, api};

    api! {
        client NestedRateApi {
            timeout 30s;
            base "http://nested-rate.example.test/";

            rate_limit { id "concord.rate_limits.27"; key [origin]; cost 1; allow 1 per 1s; }

            GET read() -> Bytes via Binary;
        }
    }
}

mod repeated_rate {
    use concord::{Binary, Bytes, api};

    api! {
        client FirstRateApi {
            timeout 30s;
            base "http://first-rate.example.test/";

            rate_limit { id "concord.rate_limits.28"; key [origin]; cost 1; allow 1 per 1s; }

            GET read() -> Bytes via Binary;
        }
    }

    api! {
        client SecondRateApi {
            timeout 30s;
            base "http://second-rate.example.test/";

            rate_limit { id "concord.rate_limits.29"; key [origin]; cost 1; allow 1 per 1s; }

            GET read() -> Bytes via Binary;
        }
    }
}

/// The identity-relevant shape of the canonical §22 Catalog: same base,
/// DNS labels, policy IDs, labels, windows, costs and endpoint IDs.
mod catalog {
    use concord::{Binary, Bytes, Text, api};

    api! {
        client CatalogApi {
            base "https://catalog.example.test/root/";
            timeout 20s;

            values {
                #[sensitive] query tenant: String;
            }

            rate_limits {
                policy tenant_reads(tenant) {
                    id "catalog.tenant.read";
                    key [origin, "tenant" = tenant];
                    allow 120 per 1m;
                    allow 5000 per 1h;
                }
            }

            subdomain ["api"];
            path "v2";

            scope regional(region: String) {
                subdomain [region];
                path "regions/{region}";
                rate_limit tenant_reads(tenant = client.tenant);

                GET item_summaries() -> Bytes via Binary {
                    id "catalog.items.list";
                    path "items";
                }

                GET item(id: String) -> Bytes via Binary {
                    id "catalog.item.get";
                    path "items/{id}";
                }

                PUT update_item(id: String, body input: String via Text) -> Bytes via Binary {
                    id "catalog.item.update";
                    path "items/{id}";
                    effect idempotent;
                    rate_limit {
                        id "catalog.item.write";
                        key [origin, endpoint, "client.tenant" = client.tenant];
                        cost 5;
                        allow 30 per 1m;
                    }
                }
            }
        }
    }
}

/// The same Catalog write policy ID and value under the label `tenant`.
mod relabelled_catalog {
    use concord::{Binary, Bytes, Text, api};

    api! {
        client CatalogApi {
            base "https://catalog.example.test/root/";
            timeout 20s;

            values {
                #[sensitive] query tenant: String;
            }

            subdomain ["api"];
            path "v2";

            scope regional(region: String) {
                subdomain [region];
                path "regions/{region}";

                PUT update_item(id: String, body input: String via Text) -> Bytes via Binary {
                    id "catalog.item.update";
                    path "items/{id}";
                    effect idempotent;
                    rate_limit {
                        id "catalog.item.write";
                        key [origin, endpoint, "tenant" = client.tenant];
                        cost 5;
                        allow 30 per 1m;
                    }
                }
            }
        }
    }
}

api! {
    client LabelPreservedApi {
        timeout 30s;
        base "http://label-state.example.test/";

        values {
            tenant: String;
        }

        GET write() -> Bytes via Binary {
            rate_limit { id "labels.write"; key ["client.tenant" = client.tenant]; allow 1 per 1h; }
        }
    }
}

api! {
    client LabelRenamedApi {
        timeout 30s;
        base "http://label-state.example.test/";

        values {
            tenant: String;
        }

        GET write() -> Bytes via Binary {
            rate_limit { id "labels.write"; key ["tenant" = client.tenant]; allow 1 per 1h; }
        }
    }
}

api! {
    client WindowSetRateApi {
        timeout 30s;
        base "http://window-set-rate.example.test/";

        rate_limits {
            windows scarce { allow 1 per 1h; }
            /// Declared but not yet attached; inspection reports it unused.
            windows spare { allow 3 per 1s; }
        }

        GET first() -> Bytes via Binary {
            rate_limit "window_set.first" key [origin] using scarce;
        }

        GET second() -> Bytes via Binary {
            rate_limit "window_set.second" key [origin] using scarce;
        }

        GET first_again() -> Bytes via Binary {
            rate_limit "window_set.first" key [origin] using scarce;
        }
    }
}

api! {
    client DedupRateApi {
        timeout 30s;
        base "http://dedup-rate.example.test/";

        rate_limits {
            policy shared_reads(account) {
                id "dedup.shared";
                key ["account" = account];
                allow 5 per 1h;
            }
        }

        scope accounts(account: String) {
            rate_limit shared_reads(account = account);

            GET read() -> Bytes via Binary {
                // Structurally identical repeated attachment: deduplicated.
                rate_limit shared_reads(account = accounts.account);
            }

            GET profile() -> Bytes via Binary;
        }
    }
}

mod before_rename {
    use concord::{Binary, Bytes, api};

    api! {
        client RenameApi {
            timeout 30s;
            base "http://rename-rate.example.test/";

            scope legacy() {
                GET read() -> Bytes via Binary {
                    id "rename.pinned.read";
                    rate_limit { id "rename.pinned"; key [endpoint]; allow 1 per 1h; }
                }

                GET derived() -> Bytes via Binary;
            }
        }
    }
}

mod after_rename {
    use concord::{Binary, Bytes, api};

    api! {
        client RenameApi {
            timeout 30s;
            base "http://rename-rate.example.test/";

            scope renamed() {
                GET read() -> Bytes via Binary {
                    id "rename.pinned.read";
                    rate_limit { id "rename.pinned"; key [endpoint]; allow 1 per 1h; }
                }

                GET derived() -> Bytes via Binary;
            }
        }
    }
}

/// Canonical lowercase adapter with its own encoding contract.
pub struct LowercaseKey;

impl RateKeyEncoder<String> for LowercaseKey {
    const ENCODING_ID: &'static str = "test.lowercase/1";

    fn write_canonical(value: &String, out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError> {
        out.write_tagged_bytes("utf8", value.to_ascii_lowercase().as_bytes())
    }
}

/// A custom constant-bearing key part.
pub struct TenantKey(&'static str);

impl RateKeyPart for TenantKey {
    const ENCODING_ID: &'static str = "test.tenant-key/1";

    fn write_canonical(&self, out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError> {
        out.write_tagged_bytes("test.tenant", self.0.as_bytes())
    }
}

api! {
    client EncodingRateApi {
        timeout 30s;
        base "http://encoding-rate.example.test/";

        values {
            region: String;
        }

        rate_limits {
            policy typed(region, shard, flag, grade, delta, count, tenant) {
                id "encoding.typed";
                key [
                    "region" = region via LowercaseKey,
                    "shard" = shard,
                    "flag" = flag,
                    "grade" = grade,
                    "delta" = delta,
                    "count" = count,
                    "tenant" = tenant,
                ];
                allow 100 per 1h;
            }
        }

        GET read(shard: u32, flag: bool, grade: char, delta: i64, count: usize, tenant: TenantKey) -> Bytes via Binary {
            rate_limit typed(
                region = client.region,
                shard = shard,
                flag = flag,
                grade = grade,
                delta = delta,
                count = count,
                tenant = tenant,
            );
        }

        GET inline_read(shard: u16) -> Bytes via Binary {
            rate_limit {
                id "encoding.inline";
                key ["shard" = shard, "region" = client.region];
                allow 100 per 1h;
            }
        }
    }
}

api! {
    client SchemaV1Api {
        timeout 30s;
        base "http://schema-rate.example.test/";

        values {
            tenant: String;
        }

        GET read() -> Bytes via Binary {
            rate_limit { id "schema.tenant"; key [origin, "tenant" = client.tenant]; allow 5 per 1h; }
        }
    }
}

api! {
    client SchemaLabelApi {
        timeout 30s;
        base "http://schema-rate.example.test/";

        values {
            tenant: String;
        }

        GET read() -> Bytes via Binary {
            rate_limit { id "schema.tenant"; key [origin, "client.tenant" = client.tenant]; allow 5 per 1h; }
        }
    }
}

api! {
    client SchemaEncodingApi {
        timeout 30s;
        base "http://schema-rate.example.test/";

        values {
            tenant: String;
        }

        GET read() -> Bytes via Binary {
            rate_limit {
                id "schema.tenant";
                key [origin, "tenant" = client.tenant via LowercaseKey];
                allow 5 per 1h;
            }
        }
    }
}

api! {
    client SchemaOrderApi {
        timeout 30s;
        base "http://schema-rate.example.test/";

        values {
            tenant: String;
        }

        GET read() -> Bytes via Binary {
            rate_limit { id "schema.tenant"; key ["tenant" = client.tenant, origin]; allow 5 per 1h; }
        }
    }
}

api! {
    client SchemaWindowApi {
        timeout 30s;
        base "http://schema-rate.example.test/";

        values {
            tenant: String;
        }

        GET read() -> Bytes via Binary {
            rate_limit { id "schema.tenant"; key [origin, "tenant" = client.tenant]; allow 6 per 1h; }
        }
    }
}

api! {
    client AtomicRateApi {
        timeout 30s;
        base "http://atomic-rate.example.test/";

        rate_limits {
            policy scarce() { id "atomic.scarce"; key [origin]; allow 1 per 1h; }
            policy shared() { id "atomic.shared"; key [origin]; allow 2 per 1h; }
        }

        GET both() -> Bytes via Binary {
            rate_limit scarce();
            rate_limit shared();
        }

        GET shared_only() -> Bytes via Binary {
            rate_limit shared();
        }

        GET scarce_only() -> Bytes via Binary {
            rate_limit scarce();
        }
    }
}

api! {
    client CapacityRateApi {
        timeout 30s;
        base "http://capacity-rate.example.test/";

        GET pair(key: String) -> Bytes via Binary {
            rate_limit { id "capacity.left"; key ["key" = key]; allow 5 per 1h; }
            rate_limit { id "capacity.right"; key ["key" = key]; allow 5 per 1h; }
        }

        GET single(key: String) -> Bytes via Binary {
            rate_limit { id "capacity.single"; key ["key" = key]; allow 5 per 1h; }
        }
    }
}

api! {
    client ClockRateApi {
        timeout 30s;
        base "http://clock-rate.example.test/";

        GET refill() -> Bytes via Binary {
            rate_limit { id "clock.refill"; key [origin]; allow 2 per 10s; }
        }

        GET cooled() -> Bytes via Binary {
            rate_limit { id "clock.cooldown"; key [origin]; allow 100 per 1s; }
        }

        GET keyed(key: String) -> Bytes via Binary {
            rate_limit { id "clock.keyed"; key ["key" = key]; allow 1 per 10s; }
        }
    }
}

// ---------------------------------------------------------------------------
// Capturing and scripted limiters
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Eq, PartialEq)]
struct CapturedRule {
    id: String,
    parts: Vec<(String, Vec<u8>)>,
    encodings: Vec<&'static str>,
    cost: u32,
    windows: Vec<(u32, Duration)>,
}

/// The canonical V3 framing:
/// `u8(tag.len) || tag || u64_be(bytes.len) || bytes`.
fn framed(tag: &str, payload: &[u8]) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(1 + tag.len() + 8 + payload.len());
    encoded.push(u8::try_from(tag.len()).expect("short tag"));
    encoded.extend_from_slice(tag.as_bytes());
    encoded.extend_from_slice(&(payload.len() as u64).to_be_bytes());
    encoded.extend_from_slice(payload);
    encoded
}

fn canonical_utf8(value: &str) -> Vec<u8> {
    framed("utf8", value.as_bytes())
}

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
            encodings: rule
                .key()
                .parts()
                .iter()
                .map(|part| part.encoding_id())
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

#[derive(Default)]
struct CapturingLimiter {
    acquisitions: Mutex<Vec<Vec<CapturedRule>>>,
    observations: Mutex<Vec<Vec<CapturedRule>>>,
    logical_origins: Mutex<Vec<Option<String>>>,
    capabilities: Mutex<Vec<LimiterCapability>>,
}

const CAPTURING_CAPABILITY: LimiterCapability =
    LimiterCapability::new(Enforcement::Local, "concord.test.capturing", 1);

impl RateLimiter for CapturingLimiter {
    fn capability(&self) -> LimiterCapability {
        CAPTURING_CAPABILITY
    }

    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        let captured = capture(request.plan);
        let logical_origin = request.logical_origin.map(str::to_owned);
        let capability = request.capability;
        Box::pin(async move {
            self.acquisitions
                .lock()
                .expect("acquisition capture lock")
                .push(captured);
            self.logical_origins
                .lock()
                .expect("logical-origin capture lock")
                .push(logical_origin);
            self.capabilities
                .lock()
                .expect("capability capture lock")
                .push(capability);
            Ok(RateLimitPermit::granted())
        })
    }

    fn observe(&self, response: RateLimitResponse<'_>) {
        let captured = capture(response.request.plan);
        self.observations
            .lock()
            .expect("observation capture lock")
            .push(captured);
    }
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
    fn capability(&self) -> LimiterCapability {
        LimiterCapability::new(Enforcement::Local, "concord.test.gated", 1)
    }

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

#[derive(Default)]
struct SmallKeyLimiter {
    acquisitions: AtomicUsize,
}

impl RateLimiter for SmallKeyLimiter {
    fn capability(&self) -> LimiterCapability {
        LimiterCapability::new(Enforcement::Local, "concord.test.small-key", 1)
    }

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

#[derive(Default)]
struct ZeroKeyLimiter;

impl RateLimiter for ZeroKeyLimiter {
    fn capability(&self) -> LimiterCapability {
        LimiterCapability::new(Enforcement::Local, "concord.test.zero-key", 1)
    }

    fn preparation_key_limit(&self) -> usize {
        0
    }

    fn acquire<'a>(
        &'a self,
        _request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        Box::pin(async { Ok(RateLimitPermit::granted()) })
    }
}

/// A deterministic shared persistent adapter following the RATE-05 contract
/// (the same compare-before-admit rules as the `concord-extras`
/// conformance fixture): inside `acquire` and before any state use, every
/// policy schema of the plan is compared with the registry under one
/// namespace and algorithm version; an incompatible active schema fails
/// closed without admission. Registration and admission are one atomic step.
#[derive(Default)]
struct SharedSchemaStore {
    state: Mutex<SharedSchemaState>,
}

#[derive(Default)]
struct SharedSchemaState {
    registrations: Vec<(String, LimiterCapability, RatePolicySchema)>,
    admissions: u64,
}

impl SharedSchemaStore {
    fn admissions(&self) -> u64 {
        self.state.lock().expect("shared store").admissions
    }

    fn registered(&self, id: &str) -> Option<RatePolicySchema> {
        self.state
            .lock()
            .expect("shared store")
            .registrations
            .iter()
            .find(|(registered, _, _)| registered == id)
            .map(|(_, _, schema)| schema.clone())
    }
}

const SHARED_CAPABILITY: LimiterCapability =
    LimiterCapability::new(Enforcement::SharedPersistent, "concord.test.shared", 1);

struct SharedSchemaLimiter(Arc<SharedSchemaStore>);

impl RateLimiter for SharedSchemaLimiter {
    fn capability(&self) -> LimiterCapability {
        SHARED_CAPABILITY
    }

    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        let result = (|| {
            let mut state = self.0.state.lock().expect("shared store");
            let mut new = Vec::new();
            for policy in request.plan.policies() {
                let id = policy.id().as_str();
                match state
                    .registrations
                    .iter()
                    .find(|(registered, _, _)| registered == id)
                {
                    Some((_, capability, schema)) => {
                        if *capability != request.capability || schema != policy.schema() {
                            return Err(RateLimitError::new(RateLimitErrorKind::SchemaConflict));
                        }
                    }
                    None => new.push((id.to_owned(), request.capability, policy.schema().clone())),
                }
            }
            state.registrations.extend(new);
            state.admissions += 1;
            Ok(RateLimitPermit::granted())
        })();
        Box::pin(async move { result })
    }
}

/// Keeps a paused current-thread runtime busy so that Tokio's test clock
/// never auto-advances while a request waits on a real loopback socket; time
/// then moves only through explicit `tokio::time::advance` calls.
#[cfg(feature = "rate-limit")]
struct ClockGuard(tokio::task::JoinHandle<()>);

#[cfg(feature = "rate-limit")]
impl ClockGuard {
    fn start() -> Self {
        Self(tokio::spawn(async {
            loop {
                tokio::task::yield_now().await;
            }
        }))
    }
}

#[cfg(feature = "rate-limit")]
impl Drop for ClockGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Lets real time pass (for loopback I/O that an early admission would
/// start) without moving the paused Tokio clock.
#[cfg(feature = "rate-limit")]
async fn settle() {
    tokio::task::spawn_blocking(|| std::thread::sleep(Duration::from_millis(150)))
        .await
        .expect("real-time settle");
    for _ in 0..50 {
        tokio::task::yield_now().await;
    }
}

fn proxy(server: &ScriptedServer) -> SafeProxy {
    SafeProxy::parse(server.url("")).expect("loopback proxy")
}

fn builder() -> rate_api::RateApiBuilder {
    RateApi::builder().set_tenant("tenant-west")
}

#[cfg(feature = "rate-limit")]
fn local_limiter() -> Arc<dyn RateLimiter> {
    Arc::new(LocalRateLimiter::new())
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

// ---------------------------------------------------------------------------
// Limiter selection and capability (V3-R12)
// ---------------------------------------------------------------------------

#[test]
fn generated_rate_limited_clients_fail_closed_without_an_explicit_limiter() {
    let error = match builder().build() {
        Ok(_) => panic!("rate-limited client unexpectedly built without a limiter"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), ClientBuildErrorKind::LimiterUnavailable);
}

#[test]
fn a_limiter_admitting_no_key_bytes_is_an_explicit_build_incompatibility() {
    let error = match builder()
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(ZeroKeyLimiter)))
        .build()
    {
        Ok(_) => panic!("a zero-key limiter cannot admit a nonempty generated key"),
        Err(error) => error,
    };
    // Every v3 key has at least one dimension; dimensions are never dropped
    // to fit (§12.3).
    assert_eq!(error.kind(), ClientBuildErrorKind::LimiterUnavailable);
    // A client without policies accepts the same limiter.
    EmptyRateApi::builder()
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(ZeroKeyLimiter)))
        .build()
        .expect("no declared policy needs key bytes");
}

#[tokio::test]
async fn explicit_noop_authorizes_nonempty_generated_plan_and_is_visibly_unenforced() {
    let server = ScriptedServer::start([ResponseScript::ok(b"noop".to_vec())])
        .expect("no-op rate-limit server");
    let proxy = proxy(&server);
    let client = builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(NoopRateLimiter::new())))
        .build()
        .expect("explicit no-op is an intentional selection");
    let plan = client
        .regions("eu")
        .read("item")
        .plan()
        .expect("no-op plan");
    assert_eq!(plan.rate_limit_policy_count(), 3);
    assert_eq!(
        plan.rate_limit().enforcement(),
        Some(Enforcement::Unenforced)
    );
    assert_eq!(
        plan.rate_limit().enforcement_label(),
        "declared policies; enforcement explicitly disabled by application selection"
    );
    assert_eq!(
        plan.rate_limit().limiter(),
        Some(NoopRateLimiter::CAPABILITY)
    );
    assert!(format!("{plan:?}").contains("enforcement explicitly disabled"));
    let body = client
        .regions("eu")
        .read("item")
        .await
        .expect("no-op authorized request");
    assert_eq!(body, Bytes::from_static(b"noop"));
}

#[cfg(feature = "rate-limit")]
#[test]
fn static_contract_is_readable_without_runtime_services_and_plans_disclose_enforcement() {
    // No builder, limiter, secret or network: the static contract and unbound
    // descriptions are complete on their own.
    let contract = RateApi::contract();
    let endpoint_policy = contract
        .policy("concord.rate_limits.3")
        .expect("endpoint policy coverage");
    assert_eq!(endpoint_policy.covered().len(), 1);
    assert_eq!(endpoint_policy.covered()[0].cost(), 2);
    let client_policy = contract
        .policy("concord.rate_limits.1")
        .expect("client policy coverage");
    let mut covered = client_policy
        .covered()
        .iter()
        .map(|endpoint| endpoint.id().to_owned())
        .collect::<Vec<_>>();
    covered.sort();
    assert_eq!(
        covered,
        ["rate_api.regions.public", "rate_api.regions.read"]
    );
    assert_eq!(
        client_policy
            .dimensions()
            .iter()
            .map(|dimension| dimension.label())
            .collect::<Vec<_>>(),
        ["origin"]
    );
    assert_eq!(client_policy.windows().len(), 2);
    assert_eq!(
        RateApi::unbound()
            .regions("eu")
            .read("item")
            .description()
            .endpoint_id()
            .as_str(),
        "rate_api.regions.read"
    );
    let named = NamedRuleRateApi::contract();
    assert!(
        named
            .policy_definitions()
            .iter()
            .any(|definition| definition.id() == "concord.rate_limits.reserved")
    );
    // Unused definitions are disclosed, not turned into required inputs.
    assert!(
        named
            .policy("concord.rate_limits.reserved")
            .expect("unused policy coverage")
            .is_unused()
    );
    let shared = named
        .policy("concord.rate_limits.13")
        .expect("named policy coverage");
    assert!(!shared.is_unused());
    assert_eq!(shared.covered().len(), 3);
    assert_eq!(shared.attachments().len(), 3);
    let windows = WindowSetRateApi::contract();
    let spare = windows
        .window_sets()
        .iter()
        .find(|set| set.name() == "spare")
        .expect("spare window set");
    assert!(spare.used_by().is_empty());

    let local = builder()
        .configure_runtime(|runtime| runtime.rate_limiter(local_limiter()))
        .build()
        .expect("local client");
    let plan = local.regions("eu").read("item").plan().expect("local plan");
    assert_eq!(plan.rate_limit().enforcement(), Some(Enforcement::Local));
    assert_eq!(
        plan.rate_limit().enforcement_label(),
        Enforcement::Local.plan_label()
    );
    let summary = plan.rate_limit();
    assert_eq!(
        summary
            .policies()
            .iter()
            .map(|policy| policy.schema().id().as_str())
            .collect::<Vec<_>>(),
        [
            "concord.rate_limits.1",
            "concord.rate_limits.2",
            "concord.rate_limits.3"
        ]
    );
    assert_eq!(summary.policies()[2].cost().get(), 2);

    let empty = EmptyRateApi::new().expect("no limiter is needed without policies");
    let plan = empty.read().plan().expect("empty plan");
    assert_eq!(plan.rate_limit().enforcement(), None);
    assert_eq!(
        plan.rate_limit().enforcement_label(),
        "no declared rate-limit policies"
    );
}

#[tokio::test]
async fn no_local_limiter_private_fallback_executes_an_empty_generated_plan() {
    let server =
        ScriptedServer::start([ResponseScript::ok(b"empty".to_vec())]).expect("empty-plan server");
    let proxy = proxy(&server);
    let client = EmptyRateApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("an empty plan needs no explicit limiter");
    assert_eq!(
        client.read().await.expect("empty-plan request"),
        Bytes::from_static(b"empty")
    );
}

// ---------------------------------------------------------------------------
// Coverage, keys and identities
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sibling_route_families_resolve_distinct_origins_and_explicit_policy_ids() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"platform".to_vec()),
        ResponseScript::ok(b"regional".to_vec()),
    ])
    .expect("route-family server");
    let proxy = proxy(&server);
    let limiter = Arc::new(CapturingLimiter::default());
    let configured: Arc<dyn RateLimiter> = limiter.clone();
    let client = RouteFamilyApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.rate_limiter(configured))
        .build()
        .expect("route-family client");

    client.platform().read().await.expect("platform request");
    client.regional().read().await.expect("regional request");

    let origins = limiter
        .logical_origins
        .lock()
        .expect("route-family logical origins");
    assert_eq!(
        origins.as_slice(),
        [
            Some("http://platform.api.route-family.example.test:80".to_owned()),
            Some("http://regional.api.route-family.example.test:80".to_owned()),
        ]
    );
    drop(origins);

    let acquisitions = limiter
        .acquisitions
        .lock()
        .expect("route-family acquisitions");
    assert_eq!(acquisitions.len(), 2);
    assert_eq!(acquisitions[0].len(), 1);
    assert_eq!(acquisitions[1].len(), 1);
    assert_eq!(acquisitions[0][0].id, "concord.rate_limits.23");
    assert_eq!(acquisitions[1][0].id, "concord.rate_limits.24");
    assert_eq!(
        acquisitions[0][0].parts,
        [(
            "origin".to_owned(),
            canonical_utf8("http://platform.api.route-family.example.test:80")
        )]
    );
    assert_ne!(acquisitions[0][0].parts, acquisitions[1][0].parts);
}

#[tokio::test]
async fn client_global_rule_shares_one_bucket() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"direct".to_vec()),
        ResponseScript::ok(b"nested".to_vec()),
    ])
    .expect("client-global server");
    let proxy = proxy(&server);
    let limiter = Arc::new(CapturingLimiter::default());
    let configured: Arc<dyn RateLimiter> = limiter.clone();
    let client = ClientBucketRateApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.rate_limiter(configured))
        .build()
        .expect("client-global client");

    client.direct().await.expect("direct request");
    client
        .descendant("scope-a")
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
async fn scope_rule_without_value_dimensions_shares_across_scope_values() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"first".to_vec()),
        ResponseScript::ok(b"second".to_vec()),
        ResponseScript::ok(b"descendant".to_vec()),
    ])
    .expect("scope-global server");
    let proxy = proxy(&server);
    let limiter = Arc::new(CapturingLimiter::default());
    let configured: Arc<dyn RateLimiter> = limiter.clone();
    let client = ScopeBucketRateApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.rate_limiter(configured))
        .build()
        .expect("scope-global client");

    client
        .branch("account-a")
        .immediate()
        .await
        .expect("first scope value");
    client
        .branch("account-b")
        .immediate()
        .await
        .expect("second scope value");
    // A scope attachment covers every descendant endpoint (RATE-01).
    client
        .branch("account-c")
        .nested("inner")
        .descendant()
        .await
        .expect("descendant request");

    let acquisitions = limiter.acquisitions.lock().expect("scope-global captures");
    assert_eq!(acquisitions.len(), 3);
    assert_eq!(acquisitions[0][1].id, "concord.rate_limits.18");
    assert_eq!(acquisitions[0][1].id, acquisitions[1][1].id);
    assert_eq!(acquisitions[0][1].parts, acquisitions[1][1].parts);
    assert_eq!(acquisitions[0][1].parts[0].0, "origin");
    assert_eq!(acquisitions[2][1], acquisitions[0][1]);
}

#[tokio::test]
async fn scope_rule_by_origin_separates_logical_origins() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"eu".to_vec()),
        ResponseScript::ok(b"us".to_vec()),
    ])
    .expect("host-bucket server");
    let proxy = proxy(&server);
    let limiter = Arc::new(CapturingLimiter::default());
    let configured: Arc<dyn RateLimiter> = limiter.clone();
    let client = HostBucketRateApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.rate_limiter(configured))
        .build()
        .expect("host-bucket client");

    client.region("eu").read().await.expect("EU host");
    client.region("us").read().await.expect("US host");

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
    assert_eq!(acquisitions[0][0].encodings, [UTF8_ENCODING_ID]);
}

#[tokio::test]
async fn scope_rule_by_scope_value_separates_values_under_the_preserved_label() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"first".to_vec()),
        ResponseScript::ok(b"second".to_vec()),
    ])
    .expect("scope-value server");
    let proxy = proxy(&server);
    let limiter = Arc::new(CapturingLimiter::default());
    let configured: Arc<dyn RateLimiter> = limiter.clone();
    let client = ScopeValueBucketRateApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.rate_limiter(configured))
        .build()
        .expect("scope-value client");

    client
        .account("account-a")
        .read()
        .await
        .expect("first account");
    client
        .account("account-b")
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
    let proxy = proxy(&server);
    let limiter = Arc::new(CapturingLimiter::default());
    let configured: Arc<dyn RateLimiter> = limiter.clone();
    let client = PartitionedRateApi::builder()
        .set_tenant("tenant-a")
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
            .regional("eu")
            .read(CanonicalOnly {
                text: resource,
                calls: Arc::clone(&canonical_calls),
            })
            .await
            .expect("partitioned request");
        assert_eq!(output, expected);
    }

    // One canonical encoding per preparation.
    assert_eq!(canonical_calls.load(Ordering::SeqCst), 3);
    let acquisitions = limiter.acquisitions.lock().expect("partitioned captures");
    assert_eq!(acquisitions.len(), 3);
    assert!(acquisitions.iter().all(|plan| plan.len() == 1));
    let rule = &acquisitions[0][0];
    assert_eq!(
        rule.parts
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
    assert_eq!(
        rule.parts
            .iter()
            .map(|(_, value)| value.clone())
            .collect::<Vec<_>>(),
        [
            canonical_utf8("http://eu.api.partitioned-rate.example.test:80"),
            canonical_utf8("stable.partitioned.read"),
            canonical_utf8("tenant-a"),
            canonical_utf8("eu"),
            framed("test.resource/v1", b"resource-a"),
        ]
    );
    assert_eq!(
        rule.encodings,
        [
            UTF8_ENCODING_ID,
            UTF8_ENCODING_ID,
            UTF8_ENCODING_ID,
            UTF8_ENCODING_ID,
            "test.resource/1",
        ]
    );
    assert_eq!(acquisitions[0][0].parts, acquisitions[1][0].parts);
    assert_ne!(acquisitions[0][0].parts, acquisitions[2][0].parts);
    assert_eq!(
        acquisitions[2][0].parts.last(),
        Some(&(
            "endpoint.resource".to_owned(),
            framed("test.resource/v1", b"resource-b")
        ))
    );
}

#[cfg(feature = "rate-limit")]
#[tokio::test]
async fn client_clones_share_state() {
    let server = ScriptedServer::start([ResponseScript::ok(b"first".to_vec())])
        .expect("shared-client limiter server");
    let proxy = proxy(&server);
    let client = PartitionedRateApi::builder()
        .set_tenant("tenant-a")
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(|runtime| runtime.rate_limiter(local_limiter()))
        .build()
        .expect("locally limited partitioned client");
    let clone = client.clone();

    client
        .regional("eu")
        .read(CanonicalOnly {
            text: "resource-a",
            calls: Arc::new(AtomicUsize::new(0)),
        })
        .await
        .expect("first request");

    let repeated = tokio::spawn(async move {
        clone
            .regional("eu")
            .read(CanonicalOnly {
                text: "resource-a",
                calls: Arc::new(AtomicUsize::new(0)),
            })
            .await
    });
    tokio::time::sleep(Duration::from_millis(100)).await;

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
    let proxy = proxy(&server);
    let limiter = Arc::new(CapturingLimiter::default());
    let configured: Arc<dyn RateLimiter> = limiter.clone();
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
    // No origin dimension: no logical origin is disclosed.
    assert_eq!(
        limiter.logical_origins.lock().expect("origins").as_slice(),
        [None, None]
    );
}

#[tokio::test]
async fn endpoint_rule_by_argument_separates_values() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"first".to_vec()),
        ResponseScript::ok(b"second".to_vec()),
    ])
    .expect("endpoint-value server");
    let proxy = proxy(&server);
    let limiter = Arc::new(CapturingLimiter::default());
    let configured: Arc<dyn RateLimiter> = limiter.clone();
    let client = PartitionedRateApi::builder()
        .set_tenant("tenant-a")
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.rate_limiter(configured))
        .build()
        .expect("endpoint-value client");

    for resource in ["resource-a", "resource-b"] {
        client
            .regional("eu")
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
    let proxy = proxy(&server);
    let limiter = Arc::new(SmallKeyLimiter::default());
    let configured: Arc<dyn RateLimiter> = limiter.clone();
    let client = PartitionedRateApi::builder()
        .set_tenant("tenant-a")
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.rate_limiter(configured))
        .build()
        .expect("small-key-limit client");
    let error = client
        .regional("eu")
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
    assert_eq!(error.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    assert!(!error.to_string().contains(KEY_SENTINEL));
    assert!(!format!("{error:?}").contains(KEY_SENTINEL));
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 0);
    assert_eq!(server.request_count(), 0);
}

#[tokio::test]
async fn custom_limiter_receives_canonical_ordered_plan_and_owns_observation() {
    let server = ScriptedServer::start([ResponseScript::ok(b"granted".to_vec())])
        .expect("rate-limit server");
    let proxy = proxy(&server);
    let limiter = Arc::new(CapturingLimiter::default());
    let configured: Arc<dyn RateLimiter> = limiter.clone();
    let client = builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.rate_limiter(configured))
        .build()
        .expect("custom-limited generated client");

    let body = client
        .regions("eu")
        .read("item-42")
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
    assert!(
        rules
            .iter()
            .all(|rule| { rule.parts.len() == 1 && rule.parts[0].0 == "origin" })
    );
    assert_eq!(rules[0].cost, 1);
    assert_eq!(
        rules[0].windows,
        [
            (500, Duration::from_secs(10)),
            (30_000, Duration::from_secs(600)),
        ]
    );
    assert_eq!(rules[2].cost, 2);
    drop(acquisitions);
    // Core passes the capability it snapshotted at build.
    assert_eq!(
        limiter
            .capabilities
            .lock()
            .expect("capabilities")
            .as_slice(),
        [CAPTURING_CAPABILITY]
    );

    let observations = limiter.observations.lock().expect("observation capture");
    assert_eq!(observations.len(), 1);
    assert_eq!(observations[0], rules_from_limiter(&limiter));
}

#[tokio::test]
async fn inherited_declarations_keep_explicit_identity_across_descendant_modules() {
    let responses = (0..5).map(|_| ResponseScript::ok(b"identity".to_vec()));
    let server = ScriptedServer::start(responses).expect("identity server");
    let proxy = proxy(&server);
    let limiter = Arc::new(CapturingLimiter::default());
    let configured: Arc<dyn RateLimiter> = limiter.clone();
    let client = IdentityRateApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.rate_limiter(configured))
        .build()
        .expect("identity client");

    client.direct().await.expect("direct request");
    client
        .outer("eu")
        .immediate()
        .await
        .expect("immediate request");
    client
        .outer("eu")
        .inner("nested")
        .nested()
        .await
        .expect("nested request");
    client
        .sibling("us")
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
    assert_eq!(acquisitions[2][2].id, "concord.rate_limits.7");
    assert_eq!(acquisitions[2][2].cost, 1);
    assert_eq!(
        acquisitions[2][2].windows,
        [(10, Duration::from_secs(3600))]
    );
    assert_eq!(acquisitions[2][3].cost, 2);
    assert_eq!(acquisitions[2][3].id, "concord.rate_limits.8");
    assert_eq!(
        acquisitions[2][3].windows,
        [(30, Duration::from_secs(3600))]
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
            let proxy = proxy(&server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime({
            let limiter: Arc<dyn RateLimiter> = limiter.clone();
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
    assert_eq!(acquisitions[0][0].parts, acquisitions[1][0].parts);
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
            let proxy = proxy(&server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime({
            let limiter: Arc<dyn RateLimiter> = limiter.clone();
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()
        .expect("named-rule capture client");

    client
        .accounts("shared-account")
        .read()
        .await
        .expect("scope attachment");
    client
        .profiles("shared-account")
        .inspect()
        .await
        .expect("sibling-scope attachment");
    client
        .lookup("shared-account")
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
            let proxy = proxy(&server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime(|runtime| runtime.rate_limiter(local_limiter()))
        .build()
        .expect("named-rule shared-state client");

    client
        .accounts("shared-account")
        .read()
        .await
        .expect("first unit from shared bucket");
    // `lookup` costs 2 against the same (ID, label, value) bucket, which has
    // one unit left.
    let waiting_client = client.clone();
    let waiting = tokio::spawn(async move { waiting_client.lookup("shared-account").await });
    tokio::time::sleep(Duration::from_millis(100)).await;

    assert!(!waiting.is_finished());
    assert_eq!(server.request_count(), 1);

    client
        .lookup("different-account")
        .await
        .expect("different logical key retains its own capacity");
    assert_eq!(server.request_count(), 2);
    assert!(!waiting.is_finished());

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
            let proxy = proxy(&server);
            move |http| http.proxy(proxy)
        })
        .retry(policy)
        .configure_runtime({
            let limiter: Arc<dyn RateLimiter> = limiter.clone();
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
    // Every visible attempt is a new admission of the same complete plan.
    assert_eq!(acquisitions.len(), 2);
    assert_eq!(acquisitions[0][0].id, "service.shared.quota");
    assert_eq!(acquisitions[0][0], acquisitions[1][0]);
}

// ---------------------------------------------------------------------------
// Sharing and separation (V3-R03)
// ---------------------------------------------------------------------------

#[cfg(feature = "rate-limit")]
#[tokio::test]
async fn shared_limiter_reuses_one_explicit_id_across_generated_contracts() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"first".to_vec()),
        ResponseScript::ok(b"other origin".to_vec()),
    ])
    .expect("cross-contract shared limiter server");
    let limiter: Arc<dyn RateLimiter> = Arc::new(LocalRateLimiter::new());
    let first = FirstSharedIdApi::builder()
        .configure_http({
            let proxy = proxy(&server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime({
            let limiter = Arc::clone(&limiter);
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()
        .expect("first shared-ID client");
    let other = OtherOriginSharedIdApi::builder()
        .configure_http({
            let proxy = proxy(&server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime({
            let limiter = Arc::clone(&limiter);
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()
        .expect("other-origin shared-ID client");
    let second = SecondSharedIdApi::builder()
        .configure_http({
            let proxy = proxy(&server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime(move |runtime| runtime.rate_limiter(limiter))
        .build()
        .expect("second shared-ID client");

    first.read().await.expect("first cross-contract request");
    let waiting = tokio::spawn(async move { second.renamed_read().await });
    tokio::time::sleep(Duration::from_millis(100)).await;

    assert!(
        !waiting.is_finished(),
        "same ID, labels, origin and limiter share"
    );
    assert_eq!(server.request_count(), 1);
    // A different logical origin is a different key value under the same ID.
    other
        .read()
        .timeout(Duration::from_secs(5))
        .expect("finite timeout")
        .await
        .expect("a different origin has its own bucket");
    assert_eq!(server.request_count(), 2);
    assert!(!waiting.is_finished());
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
            let proxy = proxy(&server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime(|runtime| runtime.rate_limiter(local_limiter()))
        .build()
        .expect("first independent client");
    let second = SecondSharedIdApi::builder()
        .configure_http({
            let proxy = proxy(&server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime(|runtime| runtime.rate_limiter(local_limiter()))
        .build()
        .expect("second independent client");

    first.read().await.expect("first independent request");
    second
        .renamed_read()
        .timeout(Duration::from_secs(5))
        .expect("finite timeout")
        .await
        .expect("second independent request must not inherit first-limiter quota debt");
    assert_eq!(server.request_count(), 2);
}

#[cfg(feature = "rate-limit")]
#[tokio::test]
async fn reused_numeric_windows_under_different_ids_keep_separate_state() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"first".to_vec()),
        ResponseScript::ok(b"second".to_vec()),
    ])
    .expect("window-set server");
    let client = WindowSetRateApi::builder()
        .configure_http({
            let proxy = proxy(&server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime(|runtime| runtime.rate_limiter(local_limiter()))
        .build()
        .expect("window-set client");

    client.first().await.expect("first identity");
    // The same `scarce` numbers under a different ID grant no shared state.
    client
        .second()
        .timeout(Duration::from_secs(5))
        .expect("finite timeout")
        .await
        .expect("a different ID has its own bucket");
    // The same ID at another endpoint shares the bucket.
    let waiting_client = client.clone();
    let waiting = tokio::spawn(async move { waiting_client.first_again().await });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!waiting.is_finished());
    assert_eq!(server.request_count(), 2);
    waiting.abort();
    let _ = waiting.await;

    let contract = WindowSetRateApi::contract();
    let first = contract.policy("window_set.first").expect("first coverage");
    let mut covered = first
        .covered()
        .iter()
        .map(|endpoint| endpoint.id().to_owned())
        .collect::<Vec<_>>();
    covered.sort();
    assert_eq!(
        covered,
        [
            "window_set_rate_api.first",
            "window_set_rate_api.first_again"
        ]
    );
    assert_eq!(first.window_set(), Some("scarce"));
}

// ---------------------------------------------------------------------------
// Explicit labels, identity stability and canonical bytes (V3-R05, V3-R13)
// ---------------------------------------------------------------------------

#[cfg(feature = "default-tls")]
fn hex_bytes(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&text[index..index + 2], 16).expect("hex byte"))
        .collect()
}

#[cfg(feature = "default-tls")]
#[tokio::test]
async fn catalog_keys_equal_the_v3_oracle_and_labels_are_state_affecting() {
    const BASELINE: &str = include_str!("../../../../fixtures/contracts/v3_catalog_recording.json");
    let baseline: serde_json::Value = serde_json::from_str(BASELINE).expect("baseline JSON");
    let entries = baseline.as_array().expect("baseline entries");
    assert_eq!(entries.len(), 12);

    for entry in entries {
        let label = entry["label"].as_str().expect("label");
        let mut fields = label.split(':');
        let (tenant, region, endpoint) = (
            fields.next().expect("tenant"),
            fields.next().expect("region"),
            fields.next().expect("endpoint"),
        );
        let limiter = Arc::new(CapturingLimiter::default());
        let client = catalog::CatalogApi::builder()
            .set_tenant(tenant)
            .configure_http(|http| {
                http.proxy(SafeProxy::parse("http://127.0.0.1:1").expect("closed-port proxy"))
            })
            .configure_runtime({
                let limiter: Arc<dyn RateLimiter> = limiter.clone();
                move |runtime| runtime.rate_limiter(limiter)
            })
            .build()
            .expect("catalog client");
        let regional = client.regional(region);
        // Admission precedes native handoff, so the complete key is captured
        // even though the closed proxy port then fails the transport.
        let result = match endpoint {
            "item_summaries" => regional.item_summaries().await,
            "item" => regional.item("item-1").await,
            "update_item" => regional.update_item("item-1", "update".to_owned()).await,
            other => panic!("unexpected baseline endpoint {other}"),
        };
        let error = result.expect_err("closed proxy port");
        assert_ne!(
            error.dispatch_knowledge(),
            DispatchKnowledge::ResponseObserved
        );

        let recorded = &entry["limiter"];
        assert_eq!(
            limiter.logical_origins.lock().expect("origins").as_slice(),
            [Some(
                recorded["logical_origin"]
                    .as_str()
                    .expect("origin")
                    .to_owned()
            )],
            "{label}"
        );
        let acquisitions = limiter.acquisitions.lock().expect("captures");
        assert_eq!(acquisitions.len(), 1, "{label}");
        let expected = recorded["policies"]
            .as_array()
            .expect("policies")
            .iter()
            .map(|policy| {
                let key = policy["key"].as_array().expect("key");
                CapturedRule {
                    id: policy["id"].as_str().expect("id").to_owned(),
                    parts: key
                        .iter()
                        .map(|part| {
                            (
                                part[0].as_str().expect("label").to_owned(),
                                hex_bytes(part[1].as_str().expect("hex")),
                            )
                        })
                        .collect(),
                    encodings: vec![UTF8_ENCODING_ID; key.len()],
                    cost: u32::try_from(policy["cost"].as_u64().expect("cost")).expect("cost"),
                    windows: policy["windows"]
                        .as_array()
                        .expect("windows")
                        .iter()
                        .map(|window| {
                            (
                                u32::try_from(window[0].as_u64().expect("capacity"))
                                    .expect("capacity"),
                                Duration::from_millis(window[1].as_u64().expect("millis")),
                            )
                        })
                        .collect(),
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(acquisitions[0], expected, "{label}");
    }
    // The helper framing matches the captured V3 key bytes.
    assert_eq!(
        canonical_utf8("tenant-a"),
        hex_bytes("0475746638000000000000000874656e616e742d61")
    );

    // The same policy ID and value under the label `tenant` is a different
    // key: labels are state-affecting (RATE-03).
    let preserved = Arc::new(CapturingLimiter::default());
    let client = catalog::CatalogApi::builder()
        .set_tenant("tenant-a")
        .configure_http(|http| {
            http.proxy(SafeProxy::parse("http://127.0.0.1:1").expect("closed-port proxy"))
        })
        .configure_runtime({
            let limiter: Arc<dyn RateLimiter> = preserved.clone();
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()
        .expect("preserved catalog");
    let _ = client
        .regional("eu")
        .update_item("item-1", "update".to_owned())
        .await;
    let relabelled = Arc::new(CapturingLimiter::default());
    let client = relabelled_catalog::CatalogApi::builder()
        .set_tenant("tenant-a")
        .configure_http(|http| {
            http.proxy(SafeProxy::parse("http://127.0.0.1:1").expect("closed-port proxy"))
        })
        .configure_runtime({
            let limiter: Arc<dyn RateLimiter> = relabelled.clone();
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()
        .expect("relabelled catalog");
    let _ = client
        .regional("eu")
        .update_item("item-1", "update".to_owned())
        .await;
    let preserved = rules_from_limiter(&preserved);
    let relabelled = rules_from_limiter(&relabelled);
    let preserved_write = preserved
        .iter()
        .find(|rule| rule.id == "catalog.item.write")
        .expect("preserved write policy");
    let relabelled_write = relabelled
        .iter()
        .find(|rule| rule.id == "catalog.item.write")
        .expect("relabelled write policy");
    assert_eq!(preserved_write.parts[2].0, "client.tenant");
    assert_eq!(relabelled_write.parts[2].0, "tenant");
    assert_eq!(preserved_write.parts[2].1, relabelled_write.parts[2].1);
    assert_ne!(preserved_write.parts, relabelled_write.parts);
    assert_eq!(preserved_write.windows, relabelled_write.windows);
}

#[cfg(feature = "rate-limit")]
#[tokio::test]
async fn label_change_with_the_same_id_and_value_selects_a_different_local_bucket() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"preserved".to_vec()),
        ResponseScript::ok(b"renamed".to_vec()),
    ])
    .expect("label state server");
    let limiter = local_limiter();
    let preserved = LabelPreservedApi::builder()
        .set_tenant("tenant-a")
        .configure_http({
            let proxy = proxy(&server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime({
            let limiter = Arc::clone(&limiter);
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()
        .expect("preserved label client");
    let renamed = LabelRenamedApi::builder()
        .set_tenant("tenant-a")
        .configure_http({
            let proxy = proxy(&server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime(move |runtime| runtime.rate_limiter(limiter))
        .build()
        .expect("renamed label client");

    preserved.write().await.expect("preserved label request");
    renamed
        .write()
        .timeout(Duration::from_secs(5))
        .expect("finite timeout")
        .await
        .expect("the renamed label is a different bucket");
    let waiting = tokio::spawn(async move { preserved.write().await });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !waiting.is_finished(),
        "the preserved label keeps its own debt"
    );
    assert_eq!(server.request_count(), 2);
    waiting.abort();
    let _ = waiting.await;
}

#[tokio::test]
async fn generated_formal_and_bound_keys_use_typed_encoding_ids_and_canonical_bytes() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"typed".to_vec()),
        ResponseScript::ok(b"inline".to_vec()),
    ])
    .expect("encoding server");
    let limiter = Arc::new(CapturingLimiter::default());
    let client = EncodingRateApi::builder()
        .set_region("EU-West")
        .configure_http({
            let proxy = proxy(&server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime({
            let limiter: Arc<dyn RateLimiter> = limiter.clone();
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()
        .expect("encoding client");

    let plan = client
        .read(7_u32, true, 'é', -2_i64, 3_usize, TenantKey("acme"))
        .plan()
        .expect("typed plan");
    let schema = plan.rate_limit().policies()[0].schema();
    assert_eq!(schema.id().as_str(), "encoding.typed");
    assert_eq!(
        schema
            .dimensions()
            .iter()
            .map(|dimension| (dimension.label(), dimension.kind(), dimension.encoding_id()))
            .collect::<Vec<_>>(),
        [
            ("region", RateDimensionKind::Custom, "test.lowercase/1"),
            ("shard", RateDimensionKind::Custom, "concord.u32/1"),
            ("flag", RateDimensionKind::Custom, "concord.bool/1"),
            ("grade", RateDimensionKind::Custom, "concord.char/1"),
            ("delta", RateDimensionKind::Custom, "concord.i64/1"),
            ("count", RateDimensionKind::Custom, "concord.usize/1"),
            ("tenant", RateDimensionKind::Custom, "test.tenant-key/1"),
        ]
    );

    client
        .read(7_u32, true, 'é', -2_i64, 3_usize, TenantKey("acme"))
        .await
        .expect("typed request");
    client.inline_read(9_u16).await.expect("inline request");

    let acquisitions = limiter.acquisitions.lock().expect("encoding captures");
    let typed = &acquisitions[0][0];
    assert_eq!(
        typed.parts,
        [
            ("region".to_owned(), framed("utf8", b"eu-west")),
            ("shard".to_owned(), framed("u32", &7_u32.to_be_bytes())),
            ("flag".to_owned(), framed("bool", &[1])),
            ("grade".to_owned(), framed("char", &0xE9_u32.to_be_bytes())),
            ("delta".to_owned(), framed("i64", &(-2_i64).to_be_bytes())),
            ("count".to_owned(), framed("usize", &3_u64.to_be_bytes())),
            ("tenant".to_owned(), framed("test.tenant", b"acme")),
        ]
    );
    // Encoding IDs are schema metadata carried beside, never inside, the
    // value bytes.
    assert_eq!(
        typed.encodings,
        [
            "test.lowercase/1",
            "concord.u32/1",
            "concord.bool/1",
            "concord.char/1",
            "concord.i64/1",
            "concord.usize/1",
            "test.tenant-key/1",
        ]
    );
    let inline = &acquisitions[1][0];
    assert_eq!(inline.id, "encoding.inline");
    assert_eq!(
        inline.parts,
        [
            ("shard".to_owned(), framed("u16", &9_u16.to_be_bytes())),
            ("region".to_owned(), canonical_utf8("EU-West")),
        ]
    );
    assert_eq!(inline.encodings, ["concord.u16/1", UTF8_ENCODING_ID]);
}

#[tokio::test]
async fn a_value_needs_only_its_rate_key_role_not_display() {
    let server = ScriptedServer::start([ResponseScript::ok(b"display".to_vec())])
        .expect("display-bound server");
    let limiter = Arc::new(CapturingLimiter::default());
    let client = MinimalBoundsApi::builder()
        .configure_http({
            let proxy = proxy(&server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime({
            let limiter: Arc<dyn RateLimiter> = limiter.clone();
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()
        .expect("minimal-bound client");
    // VALUE-06: a required value needs a semantic use.
    // `NoDisplay` has no `Display`, atom or key-part implementation; its only
    // role is a rate dimension through an explicit `RateKeyEncoder` (§12.3).
    assert_eq!(
        client.read(NoDisplay).await.expect("displayless request"),
        Bytes::from_static(b"display")
    );
    let acquisitions = limiter.acquisitions.lock().expect("captures");
    assert_eq!(
        acquisitions[0][0].parts[1],
        ("item".to_owned(), framed("test.no-display", b"fixed"))
    );
    assert_eq!(acquisitions[0][0].encodings[1], "test.no-display/1");
}

#[cfg(feature = "rate-limit")]
#[tokio::test]
async fn pinned_endpoint_ids_preserve_state_across_a_scope_rename() {
    // Navigation-derived IDs change with the rename and say so; pinned IDs
    // do not (IDENTITY-01).
    assert_eq!(
        before_rename::RenameApi::unbound()
            .legacy()
            .derived()
            .description()
            .endpoint_id()
            .as_str(),
        "rename_api.legacy.derived"
    );
    assert_eq!(
        after_rename::RenameApi::unbound()
            .renamed()
            .derived()
            .description()
            .endpoint_id()
            .as_str(),
        "rename_api.renamed.derived"
    );
    assert_eq!(
        before_rename::RenameApi::unbound()
            .legacy()
            .read()
            .description()
            .endpoint_id()
            .as_str(),
        "rename.pinned.read"
    );
    assert_eq!(
        after_rename::RenameApi::unbound()
            .renamed()
            .read()
            .description()
            .endpoint_id()
            .as_str(),
        "rename.pinned.read"
    );

    let server =
        ScriptedServer::start([ResponseScript::ok(b"before".to_vec())]).expect("rename server");
    let limiter = local_limiter();
    let before = before_rename::RenameApi::builder()
        .configure_http({
            let proxy = proxy(&server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime({
            let limiter = Arc::clone(&limiter);
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()
        .expect("before-rename client");
    let after = after_rename::RenameApi::builder()
        .configure_http({
            let proxy = proxy(&server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime(move |runtime| runtime.rate_limiter(limiter))
        .build()
        .expect("after-rename client");

    before.legacy().read().await.expect("before-rename request");
    let waiting = tokio::spawn(async move { after.renamed().read().await });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !waiting.is_finished(),
        "the renamed scope keeps the pinned endpoint bucket"
    );
    assert_eq!(server.request_count(), 1);
    waiting.abort();
    let _ = waiting.await;
}

// ---------------------------------------------------------------------------
// Repeated attachment (V3-R04)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn exact_repeated_named_attachment_is_deduplicated_and_inherited() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"read".to_vec()),
        ResponseScript::ok(b"profile".to_vec()),
    ])
    .expect("dedup server");
    let limiter = Arc::new(CapturingLimiter::default());
    let client = DedupRateApi::builder()
        .configure_http({
            let proxy = proxy(&server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime({
            let limiter: Arc<dyn RateLimiter> = limiter.clone();
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()
        .expect("dedup client");

    let plan = client.accounts("acct").read().plan().expect("dedup plan");
    assert_eq!(plan.rate_limit_policy_count(), 1);
    client.accounts("acct").read().await.expect("read");
    client.accounts("acct").profile().await.expect("profile");
    let acquisitions = limiter.acquisitions.lock().expect("dedup captures");
    assert_eq!(
        acquisitions[0].len(),
        1,
        "one admission of the repeated identity"
    );
    assert_eq!(acquisitions[0], acquisitions[1]);
    assert_eq!(
        acquisitions[0][0].parts,
        [("account".to_owned(), canonical_utf8("acct"))]
    );

    // There is no descendant rate-off: the scope attachment covers both
    // endpoints in the static contract.
    let contract = DedupRateApi::contract();
    let coverage = contract.policy("dedup.shared").expect("coverage");
    let mut covered = coverage
        .covered()
        .iter()
        .map(|endpoint| endpoint.id().to_owned())
        .collect::<Vec<_>>();
    covered.sort();
    assert_eq!(
        covered,
        [
            "dedup_rate_api.accounts.profile",
            "dedup_rate_api.accounts.read"
        ]
    );
}

#[tokio::test]
async fn explicit_policy_identities_repeat_within_one_contract_and_differ_by_declaration() {
    let server = ScriptedServer::start((0..6).map(|_| ResponseScript::ok(b"identity".to_vec())))
        .expect("identity collision server");
    let limiter = Arc::new(CapturingLimiter::default());
    let configured = || -> Arc<dyn RateLimiter> { limiter.clone() };
    let first = IdentityRateApi::builder()
        .configure_http({
            let proxy = proxy(&server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime({
            let limiter = configured();
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()
        .expect("first identity client");
    let clone = first.clone();
    clone.direct().await.expect("clone request");
    let second = IdentityRateApi::builder()
        .configure_http({
            let proxy = proxy(&server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime({
            let limiter = configured();
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()
        .expect("independent identity client");
    second.direct().await.expect("independent request");

    let nested = nested_rate::NestedRateApi::builder()
        .configure_http({
            let proxy = proxy(&server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime({
            let limiter = configured();
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()
        .expect("nested-module client");
    nested.read().await.expect("nested-module request");

    let first_repeated = repeated_rate::FirstRateApi::builder()
        .configure_http({
            let proxy = proxy(&server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime({
            let limiter = configured();
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()
        .expect("first repeated client");
    first_repeated.read().await.expect("first repeated request");

    let second_repeated = repeated_rate::SecondRateApi::builder()
        .configure_http({
            let proxy = proxy(&server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime({
            let limiter = configured();
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
    let limiter = Arc::new(CapturingLimiter::default());
    let client = CollisionRateApi::builder()
        .configure_http({
            let proxy = proxy(&server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime({
            let limiter: Arc<dyn RateLimiter> = limiter.clone();
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()
        .expect("collision-safe generated symbols");

    client
        .a__b("first-scope")
        .first()
        .await
        .expect("first punctuation path");
    client
        .a("outer-scope")
        .b("inner-scope")
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
            let proxy = proxy(&server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime({
            let limiter: Arc<dyn RateLimiter> = limiter.clone();
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()
        .expect("unrelated bucket client");
    client
        .sibling("sibling-value")
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
    assert!(
        nested_rate::NestedRateApi::contract()
            .policy("concord.rate_limits.27")
            .is_some()
    );
    assert!(
        repeated_rate::FirstRateApi::contract()
            .policy("concord.rate_limits.29")
            .is_none()
    );
}

// ---------------------------------------------------------------------------
// Shared schema continuity (V3-R06)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn shared_adapter_rejects_changed_schemas_before_admission() {
    let server = ScriptedServer::start((0..3).map(|_| ResponseScript::ok(b"schema".to_vec())))
        .expect("schema server");
    let store = Arc::new(SharedSchemaStore::default());
    macro_rules! shared_client {
        ($client:ident) => {
            $client::builder()
                .set_tenant("tenant-a")
                .configure_http({
                    let proxy = proxy(&server);
                    move |http| http.proxy(proxy)
                })
                .configure_runtime({
                    let limiter: Arc<dyn RateLimiter> =
                        Arc::new(SharedSchemaLimiter(Arc::clone(&store)));
                    move |runtime| runtime.rate_limiter(limiter)
                })
                .build()
                .expect("shared-schema client")
        };
    }

    let v1 = shared_client!(SchemaV1Api);
    let v1_plan = v1.read().plan().expect("v1 plan");
    assert_eq!(
        v1_plan.rate_limit().enforcement(),
        Some(Enforcement::SharedPersistent)
    );
    v1.read().await.expect("first use registers the schema");
    assert_eq!(store.admissions(), 1);
    let registered = store
        .registered("schema.tenant")
        .expect("registered schema");
    assert_eq!(&registered, v1_plan.rate_limit().policies()[0].schema());

    // A changed label, encoding ID, dimension order or window schema under
    // the same ID is an incompatible contract: fail closed before any state
    // use or dispatch.
    let label = shared_client!(SchemaLabelApi);
    let encoding = shared_client!(SchemaEncodingApi);
    let order = shared_client!(SchemaOrderApi);
    let window = shared_client!(SchemaWindowApi);
    for (name, result) in [
        ("label", label.read().await),
        ("encoding", encoding.read().await),
        ("order", order.read().await),
        ("window", window.read().await),
    ] {
        let error = result.expect_err(name);
        assert_eq!(
            error.rate_limit_kind(),
            Some(RateLimitErrorKind::SchemaConflict),
            "{name}"
        );
        assert_eq!(error.failure_stage(), FailureStage::RateAdmission, "{name}");
        assert_eq!(
            error.dispatch_knowledge(),
            DispatchKnowledge::NotDispatched,
            "{name}"
        );
    }
    assert_eq!(store.admissions(), 1);
    assert_eq!(server.request_count(), 1);
    for (name, plan) in [
        ("label", label.read().plan().expect("plan")),
        ("encoding", encoding.read().plan().expect("plan")),
        ("order", order.read().plan().expect("plan")),
        ("window", window.read().plan().expect("plan")),
    ] {
        assert_ne!(
            &registered,
            plan.rate_limit().policies()[0].schema(),
            "{name}"
        );
    }

    // The unchanged contract keeps using the registered state.
    v1.read().await.expect("compatible schema");
    assert_eq!(store.admissions(), 2);
    assert_eq!(server.request_count(), 2);
}

// ---------------------------------------------------------------------------
// Atomic admission (V3-R07)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn real_application_send_cannot_start_before_custom_limiter_grant() {
    let server = ScriptedServer::start([ResponseScript::ok(b"after grant".to_vec())])
        .expect("gated loopback server");
    let proxy = proxy(&server);
    let limiter = Arc::new(GatedLimiter::new());
    let configured: Arc<dyn RateLimiter> = limiter.clone();
    let client = builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.rate_limiter(configured))
        .build()
        .expect("gated generated client");
    let request = tokio::spawn(async move { client.regions("eu").read("gated-item").await });
    limiter.reached.notified().await;
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(server.request_count(), 0);
    limiter.permits.add_permits(1);
    assert_eq!(
        request.await.expect("request task").expect("gated request"),
        Bytes::from_static(b"after grant")
    );
    server
        .wait_for_requests(1, Duration::from_secs(1))
        .expect("granted request reached loopback");
}

#[cfg(feature = "rate-limit")]
#[tokio::test]
async fn waiting_multi_policy_admission_reserves_no_prefix() {
    let server = ScriptedServer::start((0..3).map(|_| ResponseScript::ok(b"ok".to_vec())))
        .expect("atomic server");
    let client = AtomicRateApi::builder()
        .configure_http({
            let proxy = proxy(&server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime(|runtime| runtime.rate_limiter(local_limiter()))
        .build()
        .expect("atomic client");

    // scarce 1/1 and shared 1/2 are consumed.
    client.both().await.expect("first conjunctive admission");
    // scarce is exhausted, so this plan waits...
    let waiting_client = client.clone();
    let waiting = tokio::spawn(async move { waiting_client.both().await });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!waiting.is_finished());
    // ...without reserving the shared unit it could have taken.
    client
        .shared_only()
        .timeout(Duration::from_secs(5))
        .expect("finite timeout")
        .await
        .expect("the waiting plan debited nothing");
    assert_eq!(server.request_count(), 2);
    let exhausted_client = client.clone();
    let exhausted = tokio::spawn(async move { exhausted_client.shared_only().await });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!exhausted.is_finished(), "shared is now exhausted");
    assert!(!waiting.is_finished());
    assert_eq!(server.request_count(), 2);
    waiting.abort();
    exhausted.abort();
    let _ = waiting.await;
    let _ = exhausted.await;
}

#[cfg(feature = "rate-limit")]
#[tokio::test]
async fn state_capacity_failure_debits_and_inserts_nothing_and_never_evicts_debt() {
    let server = ScriptedServer::start((0..2).map(|_| ResponseScript::ok(b"ok".to_vec())))
        .expect("capacity server");
    let limiter: Arc<dyn RateLimiter> = Arc::new(
        LocalRateLimiter::new()
            .quota_capacity(3)
            .expect("positive quota capacity"),
    );
    let client = CapacityRateApi::builder()
        .configure_http({
            let proxy = proxy(&server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime(move |runtime| runtime.rate_limiter(limiter))
        .build()
        .expect("capacity client");

    client.pair("a").await.expect("two entries fit");
    let error = client
        .pair("b")
        .await
        .expect_err("two more entries exceed the capacity of three");
    assert_eq!(
        error.rate_limit_kind(),
        Some(RateLimitErrorKind::StateCapacity)
    );
    assert_eq!(error.failure_stage(), FailureStage::RateAdmission);
    assert_eq!(error.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    assert_eq!(server.request_count(), 1);
    // Had the failed plan inserted one of its entries, this would not fit.
    client
        .single("c")
        .await
        .expect("exactly one entry still fits");
    assert_eq!(server.request_count(), 2);
    // Every held entry carries debt: nothing is evicted to make room.
    let error = client
        .single("d")
        .await
        .expect_err("no active debt is evicted");
    assert_eq!(
        error.rate_limit_kind(),
        Some(RateLimitErrorKind::StateCapacity)
    );
    assert_eq!(server.request_count(), 2);
}

#[cfg(feature = "rate-limit")]
#[tokio::test]
async fn committed_admission_is_not_refunded_when_the_request_is_cancelled() {
    let server = ScriptedServer::start([ResponseScript::DelayBeforeHeaders {
        delay: Duration::from_millis(800),
        status: 200,
        body: b"late".to_vec(),
    }])
    .expect("slow server");
    let client = AtomicRateApi::builder()
        .configure_http({
            let proxy = proxy(&server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime(|runtime| runtime.rate_limiter(local_limiter()))
        .build()
        .expect("cancellation client");

    let cancelled = tokio::time::timeout(Duration::from_millis(200), client.scarce_only()).await;
    assert!(cancelled.is_err(), "dropped after admission and dispatch");
    server
        .wait_for_requests(1, Duration::from_secs(2))
        .expect("the admitted request was handed off");
    let waiting_client = client.clone();
    let waiting = tokio::spawn(async move { waiting_client.scarce_only().await });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !waiting.is_finished(),
        "the committed unit was not refunded"
    );
    assert_eq!(server.request_count(), 1);
    waiting.abort();
    let _ = waiting.await;
}

// ---------------------------------------------------------------------------
// Built-in local limiter under the injected clock (V3-R08)
// ---------------------------------------------------------------------------

#[cfg(feature = "rate-limit")]
fn clock_client(server: &ScriptedServer, limiter: Arc<dyn RateLimiter>) -> ClockRateApi {
    let proxy = proxy(server);
    ClockRateApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.rate_limiter(limiter))
        .build()
        .expect("clock client")
}

#[cfg(feature = "rate-limit")]
#[tokio::test(start_paused = true)]
async fn local_refill_is_continuous_at_the_window_boundary() {
    let _clock = ClockGuard::start();
    let server = ScriptedServer::start((0..3).map(|_| ResponseScript::ok(b"ok".to_vec())))
        .expect("refill server");
    let client = clock_client(&server, local_limiter());

    // Capacity 2 per 10 s refills one unit every 5 s.
    client.refill().await.expect("first unit");
    client.refill().await.expect("second unit");
    let waiting_client = client.clone();
    let waiting = tokio::spawn(async move { waiting_client.refill().await });
    settle().await;
    assert!(!waiting.is_finished());
    tokio::time::advance(Duration::from_millis(4_999)).await;
    settle().await;
    assert!(
        !waiting.is_finished(),
        "no overshoot before the refill instant"
    );
    assert_eq!(server.request_count(), 2);
    tokio::time::advance(Duration::from_millis(1)).await;
    waiting
        .await
        .expect("waiting task")
        .expect("admitted exactly at the refill instant");
    assert_eq!(server.request_count(), 3);
}

#[cfg(feature = "rate-limit")]
#[tokio::test(start_paused = true)]
async fn server_cooldown_waits_only_its_remaining_time() {
    let _clock = ClockGuard::start();
    let server = ScriptedServer::start([
        ResponseScript::status(429, b"slow down".to_vec()).with_header("retry-after", "7"),
        ResponseScript::ok(b"ok".to_vec()),
    ])
    .expect("cooldown server");
    let client = clock_client(&server, local_limiter());

    let error = client.cooled().await.expect_err("rejected 429");
    assert_eq!(error.http_status().map(|status| status.as_u16()), Some(429));
    tokio::time::advance(Duration::from_secs(3)).await;
    let waiting_client = client.clone();
    let waiting = tokio::spawn(async move { waiting_client.cooled().await });
    settle().await;
    tokio::time::advance(Duration::from_millis(3_999)).await;
    settle().await;
    assert!(!waiting.is_finished(), "four seconds of cooldown remained");
    assert_eq!(server.request_count(), 1);
    tokio::time::advance(Duration::from_millis(1)).await;
    waiting
        .await
        .expect("waiting task")
        .expect("admitted when the cooldown expired");
    assert_eq!(server.request_count(), 2);
}

#[cfg(feature = "rate-limit")]
#[tokio::test(start_paused = true)]
async fn only_fully_replenished_idle_state_is_pruned() {
    let _clock = ClockGuard::start();
    let server = ScriptedServer::start((0..2).map(|_| ResponseScript::ok(b"ok".to_vec())))
        .expect("prune server");
    let limiter: Arc<dyn RateLimiter> = Arc::new(
        LocalRateLimiter::new()
            .quota_capacity(1)
            .expect("positive capacity")
            .idle_ttl(Duration::from_secs(60))
            .expect("positive TTL"),
    );
    let client = clock_client(&server, limiter);

    client.keyed("a").await.expect("the only entry");
    let capacity = |error: concord::ApiError| {
        assert_eq!(
            error.rate_limit_kind(),
            Some(RateLimitErrorKind::StateCapacity)
        );
    };
    // `a` carries debt.
    capacity(client.keyed("b").await.expect_err("active debt is kept"));
    // Fully replenished after 10 s, but idle for less than the TTL.
    tokio::time::advance(Duration::from_secs(30)).await;
    capacity(
        client
            .keyed("b")
            .await
            .expect_err("replenished but not idle long enough"),
    );
    tokio::time::advance(Duration::from_millis(29_999)).await;
    capacity(
        client
            .keyed("b")
            .await
            .expect_err("still inside the idle TTL"),
    );
    tokio::time::advance(Duration::from_millis(1)).await;
    client
        .keyed("b")
        .await
        .expect("replenished and idle for the TTL: pruned");
    assert_eq!(server.request_count(), 2);
}

#[cfg(feature = "default-tls")]
#[test]
fn catalog_plans_disclose_value_free_policy_schemas_without_rendering_bodies() {
    let client = catalog::CatalogApi::builder()
        .set_tenant("tenant-sentinel")
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(NoopRateLimiter::new())))
        .build()
        .expect("catalog client");
    let read = client
        .regional("eu")
        .item("item-1")
        .plan()
        .expect("read plan");
    let write = client
        .regional("eu")
        .update_item("item-1", "body-sentinel".to_owned())
        .plan()
        .expect("write plan");
    assert_eq!(read.rate_limit_policy_count(), 1);
    assert_eq!(write.rate_limit_policy_count(), 2);
    let write_schema = write.rate_limit().policies()[1].schema();
    assert_eq!(write_schema.id().as_str(), "catalog.item.write");
    assert_eq!(
        write_schema
            .dimensions()
            .iter()
            .map(|dimension| (dimension.label(), dimension.kind()))
            .collect::<Vec<_>>(),
        [
            ("origin", RateDimensionKind::Origin),
            ("endpoint", RateDimensionKind::Endpoint),
            ("client.tenant", RateDimensionKind::Custom),
        ]
    );
    assert_eq!(write.rate_limit().policies()[1].cost().get(), 5);
    let rendered = format!("{write:?}{:?}", write.rate_limit());
    assert!(!rendered.contains("body-sentinel"));
    assert!(!rendered.contains("tenant-sentinel"));
}
