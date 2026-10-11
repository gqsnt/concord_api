//! Core-integrated rate plans: bound-plan disclosure and execution requests.

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use concord_test_support::executor::DeterministicExecutor;
use concord_test_support::script::ScriptedResponse;
use http::Method;

use super::*;
use crate::client::ClientCore;
use crate::endpoint::EndpointId;
use crate::hooks::BoxFuture;
use crate::http::HttpConfigBuilder;
use crate::request::RuntimeEndpoint;
use crate::request::test_owner::{self, TestOwner};
use crate::runtime::RuntimeConfigBuilder;

/// The `origin` and `tenant` key parts of the captured Catalog baseline.
const FIXTURE_EU_ORIGIN: &str = "0475746638000000000000002768747470733a2f2f65752e6170692e636174616c6f672e6578616d706c652e746573743a343433";
const FIXTURE_TENANT_A: &str = "0475746638000000000000000874656e616e742d61";
const KEY_SENTINEL: &str = "plan-key-value-sentinel-93aa";

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

struct Captured {
    capability: LimiterCapability,
    logical_origin: Option<String>,
    schemas: Vec<RatePolicySchema>,
    parts: Vec<(String, &'static str, String)>,
}

/// A local-enforcement probe whose reported capability changes after the
/// first call, proving that core snapshots it once.
struct CapturingLimiter {
    capability_calls: AtomicU32,
    acquisitions: AtomicUsize,
    captured: Mutex<Vec<Captured>>,
}

impl CapturingLimiter {
    fn new() -> Self {
        Self {
            capability_calls: AtomicU32::new(0),
            acquisitions: AtomicUsize::new(0),
            captured: Mutex::new(Vec::new()),
        }
    }
}

impl RateLimiter for CapturingLimiter {
    fn capability(&self) -> LimiterCapability {
        let call = self.capability_calls.fetch_add(1, Ordering::SeqCst);
        LimiterCapability::new(Enforcement::Local, "test.capturing", call + 1)
    }

    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        self.acquisitions.fetch_add(1, Ordering::SeqCst);
        let captured = Captured {
            capability: request.capability,
            logical_origin: request.logical_origin.map(str::to_owned),
            schemas: request
                .plan
                .policies()
                .iter()
                .map(|policy| policy.schema().clone())
                .collect(),
            parts: request
                .plan
                .policies()
                .iter()
                .flat_map(|policy| policy.key().parts())
                .map(|part| {
                    (
                        part.name().to_owned(),
                        part.encoding_id(),
                        hex(part.value()),
                    )
                })
                .collect(),
        };
        self.captured.lock().expect("captured").push(captured);
        Box::pin(async { Ok(RateLimitPermit::granted()) })
    }
}

fn core(executor: &DeterministicExecutor, runtime: RuntimeConfigBuilder) -> ClientCore {
    ClientCore::build_with_development(
        "https://EU.api.catalog.example.test/root/v2/",
        HttpConfigBuilder::new().http1_only(),
        runtime,
        executor,
    )
    .expect("development core")
}

fn governed_declarations() -> [RateLimitDeclaration; 2] {
    [
        RateLimitDeclaration::new("catalog.tenant.read")
            .window(120, Duration::from_secs(60))
            .window(5_000, Duration::from_secs(3_600))
            .origin()
            .part("tenant", "tenant-a"),
        RateLimitDeclaration::new("catalog.private")
            .cost(2)
            .window(10, Duration::from_secs(1))
            .part("secret.partition", KEY_SENTINEL),
    ]
}

fn governed_endpoint() -> RuntimeEndpoint<crate::codec::Binary, Bytes> {
    let [tenant, private] = governed_declarations();
    RuntimeEndpoint::builder(EndpointId::from_static("catalog.items.list"), Method::GET)
        .path("items")
        .rate_limit(tenant)
        .rate_limit(private)
        .response::<crate::codec::Binary, Bytes>()
        .build()
        .expect("governed runtime endpoint")
}

/// Plans the governed head through the same head composition a generated
/// endpoint uses. The fail-closed "no limiter selected" disclosure cannot be
/// reached through a runtime binding (V3-R11 rejects it at bind), so the
/// summary is exercised on the core plan path directly.
fn plan_for(core: &ClientCore) -> crate::endpoint::RequestPlan {
    let endpoint = governed_endpoint();
    crate::__private::plan_unbound_request(core, endpoint.description(), None, |mut builder| {
        builder.push_static("items")?;
        for declaration in governed_declarations() {
            builder.push_rate_limit(declaration)?;
        }
        builder.finish()
    })
    .expect("bound plan")
}

#[test]
fn bound_plans_disclose_schemas_and_actual_enforcement_without_limiter_work() {
    let executor = DeterministicExecutor::new();
    let capturing = Arc::new(CapturingLimiter::new());
    let selected: Arc<dyn RateLimiter> = capturing.clone();
    let cases = [
        (
            core(
                &executor,
                RuntimeConfigBuilder::new().rate_limiter(Arc::new(NoopRateLimiter::new())),
            ),
            Some(Enforcement::Unenforced),
            "declared policies; enforcement explicitly disabled by application selection",
        ),
        (
            core(
                &executor,
                RuntimeConfigBuilder::new().rate_limiter(selected),
            ),
            Some(Enforcement::Local),
            "declared policies; enforced by the application-selected process-local limiter",
        ),
        (
            core(&executor, RuntimeConfigBuilder::new()),
            None,
            "declared policies; no limiter selected, so admission fails closed",
        ),
    ];
    for (core, enforcement, label) in cases {
        let runtime = test_owner::plan(
            &TestOwner::new(core.clone()).into_projection(),
            &governed_endpoint(),
        );
        if enforcement.is_some() {
            // The runtime binding discloses the same summary.
            let runtime = runtime.expect("runtime plan with a selected limiter");
            assert_eq!(runtime.rate_limit_policy_count(), 2);
            assert_eq!(runtime.rate_limit().enforcement(), enforcement);
            assert_eq!(runtime.rate_limit().enforcement_label(), label);
        } else {
            // V3-R11: without a limiter selection a runtime policy cannot bind.
            assert_eq!(
                runtime
                    .expect_err("runtime policy without limiter selection")
                    .configuration_kind(),
                Some(crate::error::ConfigurationErrorKind::InvalidRateLimit)
            );
        }
        let plan = plan_for(&core);
        let summary = plan.rate_limit();
        assert_eq!(plan.rate_limit_policy_count(), 2);
        assert_eq!(summary.enforcement(), enforcement);
        assert_eq!(summary.enforcement_label(), label);
        let first = summary.policies()[0].schema();
        assert_eq!(first.id().as_str(), "catalog.tenant.read");
        assert_eq!(
            first
                .dimensions()
                .iter()
                .map(|dimension| (dimension.label(), dimension.encoding_id()))
                .collect::<Vec<_>>(),
            [("origin", "concord.utf8/1"), ("tenant", "concord.utf8/1")]
        );
        assert_eq!(summary.policies()[1].cost().get(), 2);
        let rendered = format!("{plan:?} {summary:?}");
        assert!(rendered.contains(label));
        assert!(!rendered.contains(KEY_SENTINEL));
        assert!(!rendered.contains("tenant-a"));
        assert!(!rendered.contains("eu.api.catalog.example.test:443"));
    }
    assert_eq!(
        capturing.acquisitions.load(Ordering::SeqCst),
        0,
        "planning never acquires a limiter or registers a schema"
    );
    assert_eq!(executor.application().request_count().expect("count"), 0);
}

#[tokio::test]
async fn execution_requests_carry_the_build_snapshot_schemas_and_baseline_key_bytes() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"first")))
        .expect("first response");
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"second")))
        .expect("second response");
    let capturing = Arc::new(CapturingLimiter::new());
    let selected: Arc<dyn RateLimiter> = capturing.clone();
    let core = core(
        &executor,
        RuntimeConfigBuilder::new().rate_limiter(selected),
    );
    // A later capability report must not be observed by core.
    assert_eq!(capturing.capability().algorithm_version(), 2);

    for expected in [&b"first"[..], &b"second"[..]] {
        let body = test_owner::root(&core, governed_endpoint())
            .await
            .expect("governed execution");
        assert_eq!(body, Bytes::from(expected.to_vec()));
    }
    let captured = capturing.captured.lock().expect("captured");
    assert_eq!(captured.len(), 2);
    for request in captured.iter() {
        assert_eq!(
            request.capability,
            LimiterCapability::new(Enforcement::Local, "test.capturing", 1),
            "every request carries the capability snapshotted at build"
        );
        assert_eq!(
            request.logical_origin.as_deref(),
            Some("https://eu.api.catalog.example.test:443")
        );
        assert_eq!(request.schemas.len(), 2);
        assert_eq!(
            request.schemas[0].framing_version(),
            RATE_KEY_FRAMING_VERSION
        );
        assert_eq!(
            request.parts[0],
            (
                "origin".to_owned(),
                "concord.utf8/1",
                FIXTURE_EU_ORIGIN.to_owned()
            ),
            "the origin dimension equals the captured v2.1 bytes"
        );
        assert_eq!(
            request.parts[1],
            (
                "tenant".to_owned(),
                "concord.utf8/1",
                FIXTURE_TENANT_A.to_owned()
            )
        );
        assert_eq!(request.parts[2].0, "secret.partition");
    }
    assert_eq!(executor.application().request_count().expect("count"), 2);
}
