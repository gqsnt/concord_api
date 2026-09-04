#![forbid(unsafe_code)]

use std::io;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

use concord::auth::{
    AccessToken, Auth, AuthError, BoxFuture, CredentialProvider, CredentialRequest,
};
use concord::body::{
    BodyError, BodyFactory, BodyKind, LengthHint, RequestBody, RequestBodyContext,
    RequestBodyMetadata, RequestContentType, RequestRepresentation, RequestStream,
};
use concord::codec::Text;
#[cfg(feature = "default-tls")]
use concord::endpoint::RequestBodyCategory;
use concord::endpoint::{EndpointId, ResponseAcquisition, RuntimeEndpoint, TimeoutSelection};
use concord::error::ConfigurationErrorKind;
use concord::http::{Method, SafeProxy};
#[cfg(feature = "default-tls")]
use concord::observation::SanitizedHeaderValue;
use concord::observation::{OperationEvent, OperationObserver};
use concord::rate_limit::{
    NoopRateLimiter, RateLimitError, RateLimitPermit, RateLimitRequest, RateLimitResponse,
    RateLimiter,
};
use concord::{Bytes, api};
use concord_test_support::server::{ResponseScript, ScriptedServer};
use tokio::io::{AsyncRead, ReadBuf};

#[cfg(feature = "default-tls")]
const QUERY_SENTINEL: &str = "planned-query-secret-48c7";
#[cfg(feature = "default-tls")]
const HEADER_SENTINEL: &str = "planned-header-secret-95af";
const PROVIDER_SENTINEL: &str = "planned-provider-secret-b40e";
const BODY_SENTINEL: &[u8] = b"planned-body-secret-02d1";

#[derive(Default)]
struct EffectCounts {
    body_metadata: AtomicUsize,
    body_prepare: AtomicUsize,
    body_factory: AtomicUsize,
    stream_polls: AtomicUsize,
    provider: AtomicUsize,
    limiter_acquire: AtomicUsize,
    limiter_observe: AtomicUsize,
    before_send: AtomicUsize,
    response_head: AtomicUsize,
    completion: AtomicUsize,
    heads: Mutex<Vec<(String, String, String)>>,
    lifecycle: Mutex<Vec<(String, &'static str)>>,
}

impl EffectCounts {
    fn assert_no_effects(&self) {
        for count in [
            &self.body_metadata,
            &self.body_prepare,
            &self.body_factory,
            &self.stream_polls,
            &self.provider,
            &self.limiter_acquire,
            &self.limiter_observe,
            &self.before_send,
            &self.response_head,
            &self.completion,
        ] {
            assert_eq!(count.load(Ordering::SeqCst), 0);
        }
        assert!(self.heads.lock().expect("head capture").is_empty());
        assert!(self.lifecycle.lock().expect("lifecycle capture").is_empty());
    }
}

pub struct PlanningBodyInput {
    effects: Arc<EffectCounts>,
}

pub struct PlanningBodySpec;

impl RequestRepresentation<PlanningBodyInput> for PlanningBodySpec {
    fn metadata(
        input: &PlanningBodyInput,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        input.effects.body_metadata.fetch_add(1, Ordering::SeqCst);
        Ok(RequestBodyMetadata {
            length: LengthHint::Unknown,
            content_type: RequestContentType::None,
            kind: BodyKind::Stream,
        })
    }

    fn prepare(
        input: PlanningBodyInput,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBody, BodyError> {
        input.effects.body_prepare.fetch_add(1, Ordering::SeqCst);
        Ok(RequestBody::factory(PlanningBodyFactory {
            effects: input.effects,
        }))
    }
}

struct PlanningBodyFactory {
    effects: Arc<EffectCounts>,
}

impl BodyFactory for PlanningBodyFactory {
    fn create(&self) -> Result<RequestStream, BodyError> {
        self.effects.body_factory.fetch_add(1, Ordering::SeqCst);
        Ok(RequestStream::from_reader(CountingReader {
            effects: Arc::clone(&self.effects),
            bytes: Some(BODY_SENTINEL),
        }))
    }
}

struct CountingReader {
    effects: Arc<EffectCounts>,
    bytes: Option<&'static [u8]>,
}

impl AsyncRead for CountingReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.effects.stream_polls.fetch_add(1, Ordering::SeqCst);
        if let Some(bytes) = self.bytes.take() {
            buffer.put_slice(bytes);
        }
        Poll::Ready(Ok(()))
    }
}

pub(crate) struct CountingProvider {
    effects: Arc<EffectCounts>,
}

impl CredentialProvider for CountingProvider {
    type Material = AccessToken;

    fn acquire<'a>(
        &'a self,
        _context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        self.effects.provider.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {
            Ok(AccessToken::try_new(PROVIDER_SENTINEL).expect("bounded test access token"))
        })
    }
}

struct CountingLimiter {
    effects: Arc<EffectCounts>,
}

impl RateLimiter for CountingLimiter {
    fn acquire<'a>(
        &'a self,
        _request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        self.effects.limiter_acquire.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(RateLimitPermit::granted()) })
    }

    fn observe(&self, _response: RateLimitResponse<'_>) {
        self.effects.limiter_observe.fetch_add(1, Ordering::SeqCst);
    }
}

struct CountingHooks {
    effects: Arc<EffectCounts>,
}

impl OperationObserver for CountingHooks {
    fn observe(&self, event: OperationEvent<'_>) {
        match event {
            OperationEvent::AttemptStarted(context) => {
                self.effects.before_send.fetch_add(1, Ordering::SeqCst);
                self.effects.heads.lock().expect("head capture").push((
                    context.endpoint.to_owned(),
                    context.observed_url.to_owned(),
                    format!("{:?}", context.headers),
                ));
                self.effects
                    .lifecycle
                    .lock()
                    .expect("lifecycle capture")
                    .push((context.endpoint.to_owned(), "attempt_started"));
            }
            OperationEvent::ResponseHeadObserved(context) => {
                self.effects.response_head.fetch_add(1, Ordering::SeqCst);
                self.effects
                    .lifecycle
                    .lock()
                    .expect("lifecycle capture")
                    .push((context.endpoint.to_owned(), "response_observed"));
            }
            OperationEvent::OperationFinished(context) => {
                self.effects.completion.fetch_add(1, Ordering::SeqCst);
                self.effects
                    .lifecycle
                    .lock()
                    .expect("lifecycle capture")
                    .push((context.endpoint.to_owned(), "operation_finished"));
            }
            OperationEvent::PaginationFinished(_) => {}
            _ => {}
        }
    }
}

api! {
    client SecurePlanningApi {
        timeout 30s;
        base "https://secure-planning.example.test/root/";
        credentials {
            credential bearer = custom crate::CountingProvider;
        }


    POST protected(
        resource: String,
        protected_query: String,
        protected_header: String,
        body body: crate::runtime::planning::PlanningBodyInput via crate::PlanningBodySpec,
    ) -> Bytes via Binary {
        id "planning.secure.protected";
        path ["resources", endpoint.resource];
        query { "api_key" = endpoint.protected_query; }
        headers { "X-Api-Key" = endpoint.protected_header; }
        auth { bearer bearer challenge unauthorized; }
        rate_limit { id "concord.planning.1"; key [origin]; cost 1;  allow 1 per 1s; }
    }
}}

api! {
    client PublicPlanningApi {
        timeout 30s;
        base "http://public-planning.example.test/root/";


    POST public_head(
        resource: String,
        query_value: String,
        header_value: String,
        body body: crate::runtime::planning::PlanningBodyInput via crate::PlanningBodySpec,
    ) -> Bytes via Binary {
        id "planning.public.head";
        path ["resources", endpoint.resource];
        query { "q" = endpoint.query_value; }
        headers { "X-Public" = endpoint.header_value; }
        rate_limit { id "concord.planning.2"; key [origin]; cost 1;  allow 1 per 1s; }
    }

    GET lifecycle_probe() -> Bytes via Binary {
        id "planning.generated.lifecycle";
        path ["lifecycle"];
    }

    GET aggregate_head(values: Vec<u16>) -> String via Text {
        id "planning.generated.aggregate_head";
        query { "value" = each endpoint.values; }
    }
}}

api! {
    client StableBeforeApi {
        timeout 30s;
        base "http://stable-before.example.test/";


    GET old_rust_name() -> Bytes via Binary {
        id "stable.contract.identity";
    }
}}

api! {
    client StableAfterApi {
        timeout 30s;
        base "http://stable-after.example.test/";


    GET renamed_rust_name() -> Bytes via Binary {
        id "stable.contract.identity";
    }
}}

fn configured_public_client(
    server: &ScriptedServer,
    effects: &Arc<EffectCounts>,
    timeout: Duration,
) -> PublicPlanningApi {
    let proxy = SafeProxy::parse(server.url("")).expect("loopback proxy");
    let limiter: Arc<dyn RateLimiter> = Arc::new(CountingLimiter {
        effects: Arc::clone(effects),
    });
    let hooks = Arc::new(CountingHooks {
        effects: Arc::clone(effects),
    });
    PublicPlanningApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| {
            runtime
                .runtime_endpoint_timeout(timeout)
                .rate_limiter(limiter)
                .operation_observer(hooks.clone())
                .operation_observer(hooks.clone())
                .operation_observer(hooks)
        })
        .build()
        .expect("public planning client")
}

#[cfg(feature = "default-tls")]
#[test]
fn safe_planning_uses_real_head_shaping_without_any_lifecycle_effect() {
    let effects = Arc::new(EffectCounts::default());
    let limiter: Arc<dyn RateLimiter> = Arc::new(CountingLimiter {
        effects: Arc::clone(&effects),
    });
    let hooks = Arc::new(CountingHooks {
        effects: Arc::clone(&effects),
    });
    let client = SecurePlanningApi::builder()
        .bearer_provider(CountingProvider {
            effects: Arc::clone(&effects),
        })
        .configure_runtime(move |runtime| {
            runtime
                .rate_limiter(limiter)
                .operation_observer(hooks.clone())
                .operation_observer(hooks.clone())
                .operation_observer(hooks)
        })
        .build()
        .expect("secure planning client");
    let endpoint = SecurePlanningApi::unbound().protected(
        "resource".to_owned(),
        QUERY_SENTINEL.to_owned(),
        HEADER_SENTINEL.to_owned(),
        PlanningBodyInput {
            effects: Arc::clone(&effects),
        },
    );

    effects.assert_no_effects();
    let plan = client
        .plan(&endpoint)
        .expect("credential-free protected plan");
    effects.assert_no_effects();

    let description = endpoint.description();
    assert_eq!(description.id(), "planning.secure.protected");
    assert_eq!(description.method(), Method::POST);
    assert_eq!(description.source_path(), "SecurePlanningApi::protected");
    assert_eq!(description.route_shape(), "/resources/{endpoint.resource}");
    assert_eq!(
        description.body_category(),
        RequestBodyCategory::Specification
    );
    assert!(
        description
            .body_specification()
            .is_some_and(|name| name.ends_with("PlanningBodySpec"))
    );
    assert_eq!(
        description.response_acquisition(),
        ResponseAcquisition::Buffered
    );
    assert!(description.capabilities().authentication());
    assert!(description.capabilities().rate_limits());
    assert!(!description.capabilities().pagination());
    assert_eq!(
        description.timeout(),
        TimeoutSelection::Explicit(Duration::from_secs(30))
    );

    assert_eq!(plan.description().id(), description.id());
    assert_eq!(plan.endpoint_id(), "planning.secure.protected");
    assert_eq!(plan.method(), Method::POST);
    assert_eq!(plan.page_index(), 0);
    assert_eq!(plan.rate_limit_policy_count(), 1);
    assert!(!plan.dynamic_body_metadata_available());
    assert_eq!(plan.body_category(), RequestBodyCategory::Specification);
    assert_eq!(plan.response_acquisition(), ResponseAcquisition::Buffered);
    assert!(plan.capabilities().authentication());
    assert!(
        plan.body_specification()
            .is_some_and(|name| name.ends_with("PlanningBodySpec"))
    );
    assert_eq!(
        plan.public_headers()
            .get(&concord::http::HeaderName::from_static("x-api-key")),
        Some(SanitizedHeaderValue::Redacted)
    );

    let safe_output = format!(
        "{plan:?}\n{description:?}\n{}\n{:?}",
        plan.observed_url(),
        plan.public_headers()
    );
    for sentinel in [
        QUERY_SENTINEL,
        HEADER_SENTINEL,
        PROVIDER_SENTINEL,
        std::str::from_utf8(BODY_SENTINEL).expect("body sentinel is UTF-8"),
    ] {
        assert!(!safe_output.contains(sentinel));
    }
}

#[cfg(feature = "default-tls")]
#[test]
fn credential_references_cannot_cross_independent_client_builds() {
    let owner_effects = Arc::new(EffectCounts::default());
    let foreign_effects = Arc::new(EffectCounts::default());
    let owner = SecurePlanningApi::builder()
        .bearer_provider(CountingProvider {
            effects: Arc::clone(&owner_effects),
        })
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(NoopRateLimiter::new())))
        .build()
        .expect("credential owner client");
    let foreign = SecurePlanningApi::builder()
        .bearer_provider(CountingProvider {
            effects: Arc::clone(&foreign_effects),
        })
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(NoopRateLimiter::new())))
        .build()
        .expect("foreign client");
    let endpoint = RuntimeEndpoint::builder(
        EndpointId::new("planning.runtime.foreign_auth").expect("valid endpoint ID"),
        Method::GET,
    )
    .auth(Auth::bearer(owner.credentials().bearer().credential()))
    .build()
    .expect("context-bound runtime endpoint");

    let error = endpoint
        .plan(&foreign)
        .expect_err("foreign client context must be rejected");
    assert_eq!(
        error.configuration_kind(),
        Some(ConfigurationErrorKind::InvalidAuthentication)
    );
    owner_effects.assert_no_effects();
    foreign_effects.assert_no_effects();

    endpoint
        .plan(&owner.clone())
        .expect("a clone shares the credential context");
    owner_effects.assert_no_effects();
    foreign_effects.assert_no_effects();
}

#[tokio::test]
async fn runtime_endpoint_timeout_does_not_change_generated_timeout_or_lazy_execution() {
    let server =
        ScriptedServer::start([ResponseScript::ok(b"planned".to_vec())]).expect("plan server");
    let effects = Arc::new(EffectCounts::default());
    let client = configured_public_client(&server, &effects, Duration::from_secs(17));
    let endpoint = PublicPlanningApi::unbound().public_head(
        "item".to_owned(),
        "summary".to_owned(),
        "visible".to_owned(),
        PlanningBodyInput {
            effects: Arc::clone(&effects),
        },
    );

    let plan = client.plan(&endpoint).expect("generated public plan");
    effects.assert_no_effects();
    assert_eq!(
        plan.timeout_selection(),
        TimeoutSelection::Explicit(Duration::from_secs(30))
    );
    assert_eq!(plan.resolved_timeout(), Some(Duration::from_secs(30)));

    assert_eq!(
        client
            .request(endpoint)
            .await
            .expect("planned endpoint execution"),
        Bytes::from_static(b"planned")
    );
    assert_eq!(effects.body_metadata.load(Ordering::SeqCst), 1);
    assert_eq!(effects.body_prepare.load(Ordering::SeqCst), 1);
    assert_eq!(effects.body_factory.load(Ordering::SeqCst), 1);
    assert!(effects.stream_polls.load(Ordering::SeqCst) >= 1);
    assert_eq!(effects.provider.load(Ordering::SeqCst), 0);
    assert_eq!(effects.limiter_acquire.load(Ordering::SeqCst), 1);
    assert_eq!(effects.limiter_observe.load(Ordering::SeqCst), 1);
    assert_eq!(effects.before_send.load(Ordering::SeqCst), 1);
    assert_eq!(effects.response_head.load(Ordering::SeqCst), 1);
    assert_eq!(effects.completion.load(Ordering::SeqCst), 1);
    assert_eq!(server.request_count(), 1);

    let heads = effects.heads.lock().expect("head capture");
    assert_eq!(heads.len(), 1);
    assert_eq!(heads[0].0, plan.endpoint_id());
    assert_eq!(heads[0].1, plan.observed_url());
    assert_eq!(heads[0].2, format!("{:?}", plan.public_headers()));
}

#[test]
fn runtime_endpoint_plan_remains_zero_effect() {
    let server = ScriptedServer::start([]).expect("unused planning server");
    let effects = Arc::new(EffectCounts::default());
    let client = configured_public_client(&server, &effects, Duration::from_secs(23));

    let inherited = RuntimeEndpoint::builder(
        EndpointId::new("runtime.timeout.inherited").expect("owned ID"),
        Method::GET,
    )
    .path("inherited")
    .accepted_bytes()
    .build()
    .expect("inherited runtime endpoint");
    let explicit = RuntimeEndpoint::builder(
        EndpointId::new("runtime.timeout.explicit").expect("valid endpoint ID"),
        Method::GET,
    )
    .path("explicit")
    .timeout(Duration::from_secs(7))
    .accepted_bytes()
    .build()
    .expect("explicit runtime endpoint");
    let reset = RuntimeEndpoint::builder(
        EndpointId::new("runtime.timeout.reset").expect("valid endpoint ID"),
        Method::GET,
    )
    .path("reset")
    .timeout(Duration::from_secs(9))
    .reset_timeout()
    .accepted_bytes()
    .build()
    .expect("reset runtime endpoint");

    let inherited_plan = inherited.plan(&client).expect("inherited plan");
    let explicit_plan = explicit.plan(&client).expect("explicit plan");
    let reset_plan = reset.plan(&client).expect("reset plan");
    effects.assert_no_effects();

    assert_eq!(
        inherited_plan.timeout_selection(),
        TimeoutSelection::InheritClient
    );
    assert_eq!(
        inherited_plan.resolved_timeout(),
        Some(Duration::from_secs(23))
    );
    assert_eq!(
        explicit_plan.timeout_selection(),
        TimeoutSelection::Explicit(Duration::from_secs(7))
    );
    assert_eq!(
        explicit_plan.resolved_timeout(),
        Some(Duration::from_secs(7))
    );
    assert_eq!(
        reset_plan.timeout_selection(),
        TimeoutSelection::InheritClient
    );
    assert_eq!(reset_plan.resolved_timeout(), Some(Duration::from_secs(23)));

    let description = inherited.description();
    assert_eq!(description.id(), "runtime.timeout.inherited");
    assert_eq!(description.source_path(), "runtime");
    assert_eq!(
        description.route_shape(),
        "<client-base-host>/<client-base>/<runtime-segment>"
    );
    assert_eq!(
        description.response_acquisition(),
        ResponseAcquisition::Buffered
    );
    assert!(!description.capabilities().pagination());
    assert!(!description.capabilities().credential_storage());
}

#[test]
fn runtime_method_limit_is_public_bounded_and_preserved_by_planning() {
    let server = ScriptedServer::start([]).expect("unused method-limit planning server");
    let effects = Arc::new(EffectCounts::default());
    let client = configured_public_client(&server, &effects, Duration::from_secs(23));
    let exact_text = "M".repeat(concord::endpoint::MAX_RUNTIME_ENDPOINT_METHOD_BYTES);
    let exact_method = Method::from_bytes(exact_text.as_bytes()).expect("exact method token");
    let endpoint = RuntimeEndpoint::builder(
        EndpointId::new("runtime.method.exact").expect("valid endpoint ID"),
        exact_method.clone(),
    )
    .accepted_bytes()
    .build()
    .expect("exact runtime method limit");
    let plan = endpoint.plan(&client).expect("exact method plan");
    assert_eq!(plan.method(), &exact_method);
    assert!(format!("{plan:?}").len() < 2_048);
    effects.assert_no_effects();

    let excessive_text = "N".repeat(concord::endpoint::MAX_RUNTIME_ENDPOINT_METHOD_BYTES + 1);
    let excessive_method =
        Method::from_bytes(excessive_text.as_bytes()).expect("excess method token");
    let error = RuntimeEndpoint::builder(
        EndpointId::new("runtime.method.excess").expect("valid endpoint ID"),
        excessive_method,
    )
    .build()
    .err()
    .expect("first excess runtime method");
    assert_eq!(error.kind(), ConfigurationErrorKind::InvalidValue);
    assert!(!error.to_string().contains(&excessive_text));
    assert!(!format!("{error:?}").contains(&excessive_text));
    effects.assert_no_effects();
}

#[test]
fn generated_and_runtime_planning_share_final_head_limits() {
    let server = ScriptedServer::start([]).expect("unused head-limit planning server");
    let effects = Arc::new(EffectCounts::default());
    let client = configured_public_client(&server, &effects, Duration::from_secs(23));
    let dynamic_components = concord::endpoint::MAX_REQUEST_HEAD_COMPONENTS - 1;

    let generated_exact = PublicPlanningApi::unbound().aggregate_head(vec![1; dynamic_components]);
    client
        .plan(&generated_exact)
        .expect("generated head exact boundary including Accept");
    let generated_excess =
        PublicPlanningApi::unbound().aggregate_head(vec![1; dynamic_components + 1]);
    let error = client
        .plan(&generated_excess)
        .expect_err("generated first excess including Accept");
    assert_eq!(
        error.category(),
        concord::error::ErrorCategory::Configuration
    );

    let mut runtime_exact = RuntimeEndpoint::builder(
        EndpointId::new("runtime.head.exact").expect("valid endpoint ID"),
        Method::GET,
    );
    for _ in 0..dynamic_components {
        runtime_exact = runtime_exact.path("x");
    }
    let runtime_exact = runtime_exact
        .response::<Text, String>()
        .build()
        .expect("runtime exact structural boundary");
    runtime_exact
        .plan(&client)
        .expect("runtime head exact boundary including Accept");

    let mut runtime_excess = RuntimeEndpoint::builder(
        EndpointId::new("runtime.head.excess").expect("valid endpoint ID"),
        Method::GET,
    );
    for _ in 0..(dynamic_components + 1) {
        runtime_excess = runtime_excess.path("x");
    }
    let error = runtime_excess
        .response::<Text, String>()
        .build()
        .err()
        .expect("runtime first excess includes the retained method component");
    assert_eq!(error.kind(), ConfigurationErrorKind::InvalidValue);
    effects.assert_no_effects();
}

#[tokio::test]
async fn generated_and_runtime_endpoints_use_the_same_ordered_lifecycle() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"generated".to_vec()),
        ResponseScript::ok(b"runtime".to_vec()),
    ])
    .expect("lifecycle server");
    let effects = Arc::new(EffectCounts::default());
    let client = configured_public_client(&server, &effects, Duration::from_secs(31));

    assert_eq!(
        client.lifecycle_probe().await.expect("generated lifecycle"),
        Bytes::from_static(b"generated")
    );
    let runtime = RuntimeEndpoint::builder(
        EndpointId::new("planning.runtime.lifecycle").expect("valid endpoint ID"),
        Method::GET,
    )
    .path("lifecycle")
    .accepted_bytes()
    .build()
    .expect("runtime lifecycle endpoint");
    assert_eq!(
        runtime
            .try_bind(&client)
            .expect("runtime endpoint binds to client")
            .await
            .expect("runtime lifecycle"),
        Bytes::from_static(b"runtime")
    );

    assert_eq!(
        effects
            .lifecycle
            .lock()
            .expect("lifecycle capture")
            .as_slice(),
        [
            ("planning.generated.lifecycle".to_owned(), "attempt_started"),
            (
                "planning.generated.lifecycle".to_owned(),
                "response_observed"
            ),
            (
                "planning.generated.lifecycle".to_owned(),
                "operation_finished"
            ),
            ("planning.runtime.lifecycle".to_owned(), "attempt_started"),
            ("planning.runtime.lifecycle".to_owned(), "response_observed"),
            (
                "planning.runtime.lifecycle".to_owned(),
                "operation_finished"
            ),
        ]
    );
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].method(), requests[1].method());
    assert_eq!(requests[0].target(), requests[1].target());
}

#[test]
fn explicit_contract_identity_survives_a_rust_endpoint_rename() {
    let before = StableBeforeApi::unbound().old_rust_name();
    let after = StableAfterApi::unbound().renamed_rust_name();
    assert_eq!(before.description().id(), "stable.contract.identity");
    assert_eq!(before.description().id(), after.description().id());
    assert_ne!(
        before.description().source_path(),
        after.description().source_path()
    );
}
