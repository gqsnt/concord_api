#![forbid(unsafe_code)]

use std::io;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

use concord::auth::{AccessToken, AuthError, BoxFuture, CredentialProvider, CredentialRequest};
use concord::body::{
    BodyError, BodyFactory, BodyKind, LengthHint, RequestBody, RequestBodyContext,
    RequestBodyMetadata, RequestBodySpec, RequestContentType, StreamBody,
};
#[cfg(feature = "default-tls")]
use concord::endpoint::RequestBodyCategory;
use concord::endpoint::{EndpointId, ResponseAcquisition, RuntimeEndpoint, TimeoutSelection};
use concord::error::HookError;
#[cfg(feature = "default-tls")]
use concord::hooks::SanitizedHeaderValue;
use concord::hooks::{BeforeSend, CompletionEvent, ResponseHeadEvent, RuntimeHooks};
use concord::http::{Method, SafeProxy};
use concord::rate_limit::{
    RateLimitError, RateLimitPermit, RateLimitRequest, RateLimitResponse, RateLimiter,
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

impl RequestBodySpec for PlanningBodySpec {
    type Input = PlanningBodyInput;

    fn metadata(
        input: &Self::Input,
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
        input: Self::Input,
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
    fn create(&self) -> Result<StreamBody, BodyError> {
        self.effects.body_factory.fetch_add(1, Ordering::SeqCst);
        Ok(StreamBody::from_reader(CountingReader {
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
        Box::pin(async { Ok(AccessToken::new(PROVIDER_SENTINEL)) })
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

    fn observe<'a>(&'a self, _response: RateLimitResponse<'a>) -> BoxFuture<'a, ()> {
        self.effects.limiter_observe.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {})
    }
}

struct CountingHooks {
    effects: Arc<EffectCounts>,
}

impl RuntimeHooks for CountingHooks {
    fn before_send<'a>(&'a self, context: BeforeSend<'a>) -> BoxFuture<'a, Result<(), HookError>> {
        self.effects.before_send.fetch_add(1, Ordering::SeqCst);
        self.effects.heads.lock().expect("head capture").push((
            context.endpoint.to_owned(),
            context.safe_url.to_owned(),
            format!("{:?}", context.headers),
        ));
        self.effects
            .lifecycle
            .lock()
            .expect("lifecycle capture")
            .push((context.endpoint.to_owned(), "before_send"));
        Box::pin(async { Ok(()) })
    }

    fn on_response_head<'a>(&'a self, context: ResponseHeadEvent<'a>) -> BoxFuture<'a, ()> {
        self.effects.response_head.fetch_add(1, Ordering::SeqCst);
        self.effects
            .lifecycle
            .lock()
            .expect("lifecycle capture")
            .push((context.endpoint.to_owned(), "response_head"));
        Box::pin(async {})
    }

    fn on_complete<'a>(&'a self, context: CompletionEvent<'a>) -> BoxFuture<'a, ()> {
        self.effects.completion.fetch_add(1, Ordering::SeqCst);
        self.effects
            .lifecycle
            .lock()
            .expect("lifecycle capture")
            .push((context.endpoint.to_owned(), "completion"));
        Box::pin(async {})
    }
}

api! {
    client SecurePlanningApi {
        base "https://secure-planning.example.test/root/";
        credentials {
            credential bearer = custom crate::CountingProvider;
        }
    }

    POST protected(
        resource: String,
        protected_query: String,
        protected_header: String,
        #[body] body: crate::PlanningBodySpec,
    ) -> Bytes {
        id "planning.secure.protected";
        path ["resources", ep.resource];
        query { "api_key" = ep.protected_query; }
        headers { "X-Api-Key" = ep.protected_header; }
        auth bearer bearer challenge unauthorized;
        rate_limit { 1 / 1s; }
    }
}

api! {
    client PublicPlanningApi {
        base "http://public-planning.example.test/root/";
    }

    POST public_head(
        resource: String,
        query_value: String,
        header_value: String,
        #[body] body: crate::PlanningBodySpec,
    ) -> Bytes {
        id "planning.public.head";
        path ["resources", ep.resource];
        query { "q" = ep.query_value; }
        headers { "X-Public" = ep.header_value; }
        rate_limit { 1 / 1s; }
    }

    GET lifecycle_probe() -> Bytes {
        id "planning.generated.lifecycle";
        path ["lifecycle"];
    }
}

api! {
    client StableBeforeApi {
        base "http://stable-before.example.test/";
    }

    GET old_rust_name() -> Bytes {
        id "stable.contract.identity";
    }
}

api! {
    client StableAfterApi {
        base "http://stable-after.example.test/";
    }

    GET renamed_rust_name() -> Bytes {
        id "stable.contract.identity";
    }
}

fn configured_public_client(
    server: &ScriptedServer,
    effects: &Arc<EffectCounts>,
    timeout: Duration,
) -> PublicPlanningApi {
    let proxy = SafeProxy::parse(server.url("")).expect("loopback proxy");
    let limiter: Arc<dyn RateLimiter> = Arc::new(CountingLimiter {
        effects: Arc::clone(effects),
    });
    let hooks: Arc<dyn RuntimeHooks> = Arc::new(CountingHooks {
        effects: Arc::clone(effects),
    });
    PublicPlanningApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| {
            runtime
                .runtime_endpoint_timeout(timeout)
                .rate_limiter(limiter)
                .hooks(hooks)
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
    let hooks: Arc<dyn RuntimeHooks> = Arc::new(CountingHooks {
        effects: Arc::clone(&effects),
    });
    let client = SecurePlanningApi::builder()
        .bearer_provider(CountingProvider {
            effects: Arc::clone(&effects),
        })
        .configure_runtime(move |runtime| runtime.rate_limiter(limiter).hooks(hooks))
        .build()
        .expect("secure planning client");
    let endpoint = secure_planning_api::Protected::new(
        "resource".to_owned(),
        QUERY_SENTINEL.to_owned(),
        HEADER_SENTINEL.to_owned(),
        PlanningBodyInput {
            effects: Arc::clone(&effects),
        },
    );

    effects.assert_no_effects();
    let plan = client.plan(&endpoint).expect("safe protected plan");
    effects.assert_no_effects();

    let description = endpoint.description();
    assert_eq!(description.id(), "planning.secure.protected");
    assert_eq!(description.method(), Method::POST);
    assert_eq!(description.source_path(), "SecurePlanningApi::protected");
    assert_eq!(
        description.route_shape(),
        "/<fixed base>/resources/{ep.VALUE}"
    );
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
    assert!(description.capabilities().raw_response());
    assert_eq!(description.timeout(), TimeoutSelection::Disabled);

    assert_eq!(plan.description().id(), description.id());
    assert_eq!(plan.endpoint_id(), "planning.secure.protected");
    assert_eq!(plan.method(), Method::POST);
    assert_eq!(plan.page_index(), 0);
    assert_eq!(plan.rate_limit_rule_count(), 1);
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
        plan.safe_url(),
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

#[tokio::test]
async fn runtime_endpoint_timeout_does_not_change_generated_timeout_or_lazy_execution() {
    let server =
        ScriptedServer::start([ResponseScript::ok(b"planned".to_vec())]).expect("plan server");
    let effects = Arc::new(EffectCounts::default());
    let client = configured_public_client(&server, &effects, Duration::from_secs(17));
    let endpoint = public_planning_api::PublicHead::new(
        "item".to_owned(),
        "summary".to_owned(),
        "visible".to_owned(),
        PlanningBodyInput {
            effects: Arc::clone(&effects),
        },
    );

    let plan = client.plan(&endpoint).expect("generated public plan");
    effects.assert_no_effects();
    assert_eq!(plan.timeout_selection(), TimeoutSelection::Disabled);
    assert_eq!(plan.resolved_timeout(), None);

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
    assert_eq!(heads[0].1, plan.safe_url());
    assert_eq!(heads[0].2, format!("{:?}", plan.public_headers()));
}

#[test]
fn runtime_endpoint_plan_remains_zero_effect() {
    let server = ScriptedServer::start([]).expect("unused planning server");
    let effects = Arc::new(EffectCounts::default());
    let client = configured_public_client(&server, &effects, Duration::from_secs(23));

    let inherited = RuntimeEndpoint::builder(
        EndpointId::new("runtime.timeout.inherited".to_owned()).expect("owned ID"),
        Method::GET,
    )
    .path_segment("inherited")
    .response_bytes()
    .build()
    .expect("inherited runtime endpoint");
    let explicit = RuntimeEndpoint::builder(
        EndpointId::new("runtime.timeout.explicit").expect("valid endpoint ID"),
        Method::GET,
    )
    .path_segment("explicit")
    .timeout(Duration::from_secs(7))
    .response_bytes()
    .build()
    .expect("explicit runtime endpoint");
    let disabled = RuntimeEndpoint::builder(
        EndpointId::new("runtime.timeout.disabled").expect("valid endpoint ID"),
        Method::GET,
    )
    .path_segment("disabled")
    .disable_timeout()
    .response_bytes()
    .build()
    .expect("disabled runtime endpoint");

    let inherited_plan = client.plan(&inherited).expect("inherited plan");
    let explicit_plan = client.plan(&explicit).expect("explicit plan");
    let disabled_plan = client.plan(&disabled).expect("disabled plan");
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
        disabled_plan.timeout_selection(),
        TimeoutSelection::Disabled
    );
    assert_eq!(disabled_plan.resolved_timeout(), None);

    let description = inherited.description();
    assert_eq!(description.id(), "runtime.timeout.inherited");
    assert_eq!(description.source_path(), "runtime");
    assert_eq!(
        description.route_shape(),
        "/<client-base>/<runtime-segment>"
    );
    assert_eq!(
        description.response_acquisition(),
        ResponseAcquisition::Buffered
    );
    assert!(!description.capabilities().pagination());
    assert!(!description.capabilities().credential_storage());
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
    .path_segment("lifecycle")
    .response_bytes()
    .build()
    .expect("runtime lifecycle endpoint");
    assert_eq!(
        client.request(runtime).await.expect("runtime lifecycle"),
        Bytes::from_static(b"runtime")
    );

    assert_eq!(
        effects
            .lifecycle
            .lock()
            .expect("lifecycle capture")
            .as_slice(),
        [
            ("planning.generated.lifecycle".to_owned(), "before_send"),
            ("planning.generated.lifecycle".to_owned(), "response_head"),
            ("planning.generated.lifecycle".to_owned(), "completion"),
            ("planning.runtime.lifecycle".to_owned(), "before_send"),
            ("planning.runtime.lifecycle".to_owned(), "response_head"),
            ("planning.runtime.lifecycle".to_owned(), "completion"),
        ]
    );
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].method(), requests[1].method());
    assert_eq!(requests[0].target(), requests[1].target());
}

#[test]
fn explicit_contract_identity_survives_a_rust_endpoint_rename() {
    let before = stable_before_api::OldRustName::new();
    let after = stable_after_api::RenamedRustName::new();
    assert_eq!(before.description().id(), "stable.contract.identity");
    assert_eq!(before.description().id(), after.description().id());
    assert_ne!(
        before.description().source_path(),
        after.description().source_path()
    );
}
