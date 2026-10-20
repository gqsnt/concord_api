#![forbid(unsafe_code)]
//! Two-crate rate-key acceptance (V3-R13).
//!
//! `MeteredApi`, its constant-bearing key part (`ShardKey`), its formal
//! `via` adapter (`LowercaseRegion`) and its custom adapter for an opaque
//! type (`AccountKey` for `AccountId`) are declared, and `api!` is expanded,
//! in `concord-downstream-api`. This crate only observes the produced keys
//! through public surfaces: the value-free plan schema and the protected
//! canonical key bytes a selected limiter receives. The expected bytes are
//! the canonical V3 framing, checked three ways: literal hex, the typed
//! public `encode_canonical*` path, and the captured V3 rate-key oracle.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use concord::http::SafeProxy;
use concord::rate_limit::{
    BoxFuture, DEFAULT_RATE_LIMIT_KEY_BYTES, Enforcement, LimiterCapability, RateDimensionKind,
    RateKeyEncoder, RateKeyPart, RateLimitError, RateLimitPermit, RateLimitPlan, RateLimitRequest,
    RateLimiter, UTF8_ENCODING_ID, encode_canonical, encode_canonical_with,
};
use concord_downstream_api::{
    AccountId, AccountKey, LowercaseRegion, MeteredApi, ShardKey, Widget,
};
use concord_test_support::proxy::ConnectProxy;
use concord_test_support::tls::TlsServer;

/// One captured policy of an admitted plan.
#[derive(Clone, Debug, Eq, PartialEq)]
struct CapturedPolicy {
    id: String,
    /// `(label, dimension kind, encoding ID, protected canonical bytes)`.
    parts: Vec<(String, RateDimensionKind, &'static str, Vec<u8>)>,
    cost: u32,
    windows: Vec<(u32, Duration)>,
}

fn capture(plan: &RateLimitPlan) -> Vec<CapturedPolicy> {
    plan.policies()
        .iter()
        .map(|policy| {
            let mut windows = policy
                .windows()
                .iter()
                .map(|window| (window.capacity().get(), window.duration()))
                .collect::<Vec<_>>();
            windows.sort_by_key(|(capacity, duration)| (*duration, *capacity));
            CapturedPolicy {
                id: policy.id().as_str().to_owned(),
                parts: policy
                    .key()
                    .parts()
                    .iter()
                    .map(|part| {
                        (
                            part.name().to_owned(),
                            part.kind(),
                            part.encoding_id(),
                            part.value().to_vec(),
                        )
                    })
                    .collect(),
                cost: policy.cost().get(),
                windows,
            }
        })
        .collect()
}

/// A custom limiter defined in the consuming crate; it records every
/// admission request and grants it.
#[derive(Default)]
struct CapturingLimiter {
    acquisitions: Mutex<Vec<(Option<String>, Vec<CapturedPolicy>)>>,
}

impl RateLimiter for CapturingLimiter {
    fn capability(&self) -> LimiterCapability {
        LimiterCapability::new(Enforcement::Local, "downstream.test.capturing", 1)
    }

    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        let captured = capture(request.plan);
        let origin = request.logical_origin.map(str::to_owned);
        self.acquisitions
            .lock()
            .expect("capture lock")
            .push((origin, captured));
        Box::pin(async { Ok(RateLimitPermit::granted()) })
    }
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

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&text[index..index + 2], 16).expect("hex byte"))
        .collect()
}

fn policy<'a>(policies: &'a [CapturedPolicy], id: &str) -> &'a CapturedPolicy {
    policies
        .iter()
        .find(|policy| policy.id == id)
        .unwrap_or_else(|| panic!("policy {id}"))
}

const REGION: &str = "EU-West";
const SHARD: ShardKey = ShardKey(7);
const ACCOUNT: u64 = 42;
const DAY: u32 = 20_260_926;

/// Literal canonical bytes (hex), computed independently from the V3
/// framing contract.
const ORIGIN_HEX: &str = "0475746638000000000000001568747470733a2f2f6c6f63616c686f73743a343433";
const REGION_HEX: &str = "0475746638000000000000000765752d77657374";
const SHARD_HEX: &str = "13646f776e73747265616d2e73686172642f763100000000000000020007";
const ACCOUNT_HEX: &str =
    "15646f776e73747265616d2e6163636f756e742f76310000000000000008000000000000002a";
const ENDPOINT_HEX: &str = "04757466380000000000000014646f776e73747265616d2e75736167652e676574";
const DAY_HEX: &str = "0375333200000000000000040135283e";

#[test]
fn the_framing_helper_reproduces_bytes_from_the_v3_contract() {
    const BASELINE: &str = include_str!("../../../../fixtures/contracts/v3_catalog_recording.json");
    // The first recorded V3 admission (tenant-a, eu, item_summaries)
    // contains this logical origin and tenant key part.
    let origin = "https://eu.api.catalog.example.test:443";
    let origin_hex = "0475746638000000000000002768747470733a2f2f65752e6170692e636174616c6f672e6578616d706c652e746573743a343433";
    let tenant_hex = "0475746638000000000000000874656e616e742d61";
    assert!(BASELINE.contains(&format!("\"logical_origin\":\"{origin}\"")));
    assert!(BASELINE.contains(&format!("[\"origin\",\"{origin_hex}\"]")));
    assert!(BASELINE.contains(&format!("[\"tenant\",\"{tenant_hex}\"]")));
    assert_eq!(framed("utf8", origin.as_bytes()), hex(origin_hex));
    assert_eq!(framed("utf8", b"tenant-a"), hex(tenant_hex));
}

#[test]
fn downstream_key_types_encode_through_the_typed_public_path() {
    // The constant-bearing custom part, the formal adapter, the custom
    // adapter for an opaque type and a primitive: each typed encoding carries
    // its encoding ID beside (not inside) the canonical bytes.
    let shard = encode_canonical(&SHARD, DEFAULT_RATE_LIMIT_KEY_BYTES).expect("shard");
    assert_eq!(shard.encoding_id(), "downstream.shard/1");
    assert_eq!(shard.encoding_id(), <ShardKey as RateKeyPart>::ENCODING_ID);
    assert_eq!(shard.as_bytes(), hex(SHARD_HEX));
    assert_eq!(
        shard.as_bytes(),
        framed(ShardKey::TAG, &7_u16.to_be_bytes())
    );

    let region = encode_canonical_with::<LowercaseRegion, String>(
        &REGION.to_owned(),
        DEFAULT_RATE_LIMIT_KEY_BYTES,
    )
    .expect("region");
    assert_eq!(
        region.encoding_id(),
        <LowercaseRegion as RateKeyEncoder<String>>::ENCODING_ID
    );
    assert_eq!(region.as_bytes(), hex(REGION_HEX));

    let account = encode_canonical_with::<AccountKey, AccountId>(
        &AccountId::new(ACCOUNT),
        DEFAULT_RATE_LIMIT_KEY_BYTES,
    )
    .expect("account");
    assert_eq!(account.encoding_id(), "downstream.account/1");
    assert_eq!(account.as_bytes(), hex(ACCOUNT_HEX));

    let day = encode_canonical(&DAY, DEFAULT_RATE_LIMIT_KEY_BYTES).expect("day");
    assert_eq!(day.encoding_id(), "concord.u32/1");
    assert_eq!(day.as_bytes(), hex(DAY_HEX));
    assert_eq!(day.as_bytes(), framed("u32", &DAY.to_be_bytes()));
}

#[tokio::test(flavor = "current_thread")]
async fn downstream_generated_keys_match_canonical_bytes_and_expose_encoding_ids() {
    let server = TlsServer::start().expect("TLS loopback");
    let tunnel = ConnectProxy::start(server.address()).expect("CONNECT tunnel");
    let proxy = SafeProxy::parse(tunnel.url()).expect("safe proxy");
    let root = server.certificate_pem();
    let limiter = Arc::new(CapturingLimiter::default());
    let client = MeteredApi::builder()
        .set_region(REGION)
        .configure_http(move |http| {
            http.http1_only()
                .proxy(proxy)
                .add_root_certificate_pem(root)
        })
        .configure_runtime({
            let limiter: Arc<dyn RateLimiter> = limiter.clone();
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()
        .expect("metered client");
    let usage = || client.accounts(AccountId::new(ACCOUNT)).usage(SHARD, DAY);

    // The value-free plan discloses every effective policy schema: labels,
    // dimension kinds and encoding IDs, separately from any key bytes.
    let plan = usage().plan().expect("plan");
    let mut schemas = plan
        .rate_limit()
        .policies()
        .iter()
        .map(|summary| {
            let schema = summary.schema();
            (
                schema.id().as_str().to_owned(),
                schema
                    .dimensions()
                    .iter()
                    .map(|dimension| {
                        (
                            dimension.label().to_owned(),
                            dimension.kind(),
                            dimension.encoding_id(),
                        )
                    })
                    .collect::<Vec<_>>(),
                summary.cost().get(),
            )
        })
        .collect::<Vec<_>>();
    schemas.sort_by(|left, right| left.0.cmp(&right.0));
    let dimension = |label: &str, kind, encoding| (label.to_owned(), kind, encoding);
    assert_eq!(
        schemas,
        [
            (
                "downstream.account".to_owned(),
                vec![dimension(
                    "account",
                    RateDimensionKind::Custom,
                    "downstream.account/1"
                )],
                1,
            ),
            (
                "downstream.metered".to_owned(),
                vec![
                    dimension("origin", RateDimensionKind::Origin, UTF8_ENCODING_ID),
                    dimension(
                        "region",
                        RateDimensionKind::Custom,
                        "downstream.lowercase/1"
                    ),
                    dimension("shard", RateDimensionKind::Custom, "downstream.shard/1"),
                ],
                1,
            ),
            (
                "downstream.usage.day".to_owned(),
                vec![
                    dimension("endpoint", RateDimensionKind::Endpoint, UTF8_ENCODING_ID),
                    dimension("day", RateDimensionKind::Custom, "concord.u32/1"),
                ],
                2,
            ),
        ]
    );
    // Rate values are always protected in automatic observation (the day is
    // also a public path segment, so only the rate-only values are probed).
    let rendered = format!("{plan:?}");
    for protected in ["eu-west", REGION, ShardKey::TAG, AccountKey::TAG] {
        assert!(
            !rendered.contains(protected),
            "plan must not render rate key values ({protected})"
        );
    }
    assert!(
        limiter.acquisitions.lock().expect("captures").is_empty(),
        "planning performs no admission"
    );

    // Execution admits the complete plan once, before the send.
    server
        .queue_response_with_headers(
            200,
            &[("content-type", "application/json")],
            br#"{"id":"u1","name":"Usage"}"#.to_vec(),
        )
        .expect("queue JSON");
    let widget: Widget = usage().await.expect("metered request");
    assert_eq!(widget.id, "u1");
    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].target(), "/metered/usage/20260926");

    let acquisitions = limiter.acquisitions.lock().expect("captures");
    assert_eq!(acquisitions.len(), 1, "one conjunctive admission");
    let (origin, policies) = &acquisitions[0];
    assert_eq!(origin.as_deref(), Some("https://localhost:443"));
    assert_eq!(policies.len(), 3);

    let metered = policy(policies, "downstream.metered");
    assert_eq!(
        metered.parts,
        [
            (
                "origin".to_owned(),
                RateDimensionKind::Origin,
                UTF8_ENCODING_ID,
                hex(ORIGIN_HEX),
            ),
            (
                "region".to_owned(),
                RateDimensionKind::Custom,
                "downstream.lowercase/1",
                hex(REGION_HEX),
            ),
            (
                "shard".to_owned(),
                RateDimensionKind::Custom,
                "downstream.shard/1",
                hex(SHARD_HEX),
            ),
        ]
    );
    assert_eq!(metered.cost, 1);
    assert_eq!(
        metered.windows,
        [
            (100, Duration::from_secs(60)),
            (1_000, Duration::from_secs(3_600))
        ]
    );

    let account = policy(policies, "downstream.account");
    assert_eq!(
        account.parts,
        [(
            "account".to_owned(),
            RateDimensionKind::Custom,
            "downstream.account/1",
            hex(ACCOUNT_HEX),
        )]
    );
    assert_eq!(account.windows, [(10, Duration::from_secs(1))]);

    let day = policy(policies, "downstream.usage.day");
    assert_eq!(
        day.parts,
        [
            (
                "endpoint".to_owned(),
                RateDimensionKind::Endpoint,
                UTF8_ENCODING_ID,
                hex(ENDPOINT_HEX),
            ),
            (
                "day".to_owned(),
                RateDimensionKind::Custom,
                "concord.u32/1",
                hex(DAY_HEX),
            ),
        ]
    );
    assert_eq!(day.cost, 2);
    assert_eq!(day.windows, [(5, Duration::from_secs(60))]);

    // The generated bytes equal the typed public encoding of the same
    // downstream values, and the encoding IDs are schema metadata only: no
    // encoding ID is inserted into the canonical value bytes.
    let typed = [
        (
            &metered.parts[1],
            encode_canonical_with::<LowercaseRegion, String>(
                &REGION.to_owned(),
                DEFAULT_RATE_LIMIT_KEY_BYTES,
            ),
        ),
        (
            &metered.parts[2],
            encode_canonical(&SHARD, DEFAULT_RATE_LIMIT_KEY_BYTES),
        ),
        (
            &account.parts[0],
            encode_canonical_with::<AccountKey, AccountId>(
                &AccountId::new(ACCOUNT),
                DEFAULT_RATE_LIMIT_KEY_BYTES,
            ),
        ),
        (
            &day.parts[1],
            encode_canonical(&DAY, DEFAULT_RATE_LIMIT_KEY_BYTES),
        ),
    ];
    for ((label, _, encoding, bytes), expected) in typed {
        let expected = expected.expect("typed encoding");
        assert_eq!(bytes.as_slice(), expected.as_bytes(), "{label}");
        assert_eq!(*encoding, expected.encoding_id(), "{label}");
        let id = encoding.as_bytes();
        assert!(
            !bytes.windows(id.len()).any(|window| window == id),
            "{label}: the encoding ID is not part of the value bytes"
        );
    }
}
