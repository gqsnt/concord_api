//! Bound plans (§18.4, EXEC-03): generated and runtime-bound plans use real
//! public-head shaping but never prepare a body, obtain credentials, acquire
//! a limiter, observe or send; execution afterwards follows the same ordered
//! lifecycle, and body preparation precedes credential and admission work
//! (V3-E05).

#![forbid(unsafe_code)]
// The secure-planning harness needs an https client; without `default-tls`
// its helpers and imports are intentionally unused.
#![cfg_attr(not(feature = "default-tls"), allow(unused_imports, dead_code))]

use std::io;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

use concord::auth::{AccessToken, AuthError, BoxFuture, CredentialProvider, CredentialRequest};
use concord::body::{
    BodyDescription, BodyError, BodyErrorKind, BodyFactory, BodyKind, PreparedBody,
    RequestBodyContext, RequestRepresentation, RequestStream,
};
use concord::endpoint::{
    EndpointId, RequestBodyCategory, ResponseAcquisition, RuntimeEndpoint, TimeoutSelection,
};
use concord::error::{ConfigurationErrorKind, DispatchKnowledge, ErrorCategory, FailureStage};
use concord::http::{HttpConfigBuilder, Method, SafeProxy};
use concord::observation::SanitizedHeaderValue;
use concord::observation::{OperationEvent, OperationObserver, RequestTarget};
use concord::rate_limit::{
    Enforcement, LimiterCapability, RateLimitError, RateLimitPermit, RateLimitRequest,
    RateLimitResponse, RateLimiter,
};
use concord::{Binary, Bytes, Text, api};
#[cfg(feature = "default-tls")]
use concord_test_support::proxy::ConnectProxy;
use concord_test_support::server::{ResponseScript, ScriptedServer};
#[cfg(feature = "default-tls")]
use concord_test_support::tls::TlsServer;
use tokio::io::{AsyncRead, ReadBuf};

const QUERY_SENTINEL: &str = "planned-query-secret-48c7";
const HEADER_SENTINEL: &str = "planned-header-secret-95af";
const PROVIDER_SENTINEL: &str = "planned-provider-secret-b40e";
const BODY_SENTINEL: &[u8] = b"planned-body-secret-02d1";
const UNKNOWN_PATH_SENTINEL: &str = "unknown-path-value-7461";
const UNKNOWN_QUERY_SENTINEL: &str = "unknown-query-value-2c95";
const UNKNOWN_HEADER_SENTINEL: &str = "unknown-header-value-8a30";

#[derive(Default)]
struct EffectCounts {
    body_prepare: AtomicUsize,
    body_factory: AtomicUsize,
    stream_polls: AtomicUsize,
    provider: AtomicUsize,
    limiter_acquire: AtomicUsize,
    limiter_observe: AtomicUsize,
    before_send: AtomicUsize,
    response_head: AtomicUsize,
    completion: AtomicUsize,
    heads: Mutex<Vec<(String, RequestTarget)>>,
    limiter_views: Mutex<Vec<String>>,
    lifecycle: Mutex<Vec<(String, &'static str)>>,
}

impl EffectCounts {
    fn assert_no_effects(&self) {
        for count in [
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
        assert!(
            self.limiter_views
                .lock()
                .expect("limiter capture")
                .is_empty()
        );
        assert!(self.lifecycle.lock().expect("lifecycle capture").is_empty());
    }
}

pub struct PlanningBodyInput {
    effects: Arc<EffectCounts>,
    fail: bool,
}

/// Counting request representation producing a reconstructible factory body.
/// Its static description is read by plans without calling `prepare`.
pub struct PlanningBodySpec;

impl RequestRepresentation<PlanningBodyInput> for PlanningBodySpec {
    const DESCRIPTION: BodyDescription =
        BodyDescription::new(BodyKind::Stream).with_media_hint("application/x-planning");

    fn prepare(
        input: PlanningBodyInput,
        _context: RequestBodyContext<'_>,
    ) -> Result<PreparedBody, BodyError> {
        input.effects.body_prepare.fetch_add(1, Ordering::SeqCst);
        if input.fail {
            return Err(BodyError::new(BodyErrorKind::Producer));
        }
        Ok(PreparedBody::factory(PlanningBodyFactory {
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

pub struct CountingProvider {
    effects: Arc<EffectCounts>,
    token: &'static str,
}

impl CredentialProvider for CountingProvider {
    type Material = AccessToken;

    fn acquire<'a>(
        &'a self,
        _context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        self.effects.provider.fetch_add(1, Ordering::SeqCst);
        let token = self.token;
        Box::pin(async move { Ok(AccessToken::try_new(token).expect("bounded test access token")) })
    }
}

struct CountingLimiter {
    effects: Arc<EffectCounts>,
}

impl RateLimiter for CountingLimiter {
    fn capability(&self) -> LimiterCapability {
        LimiterCapability::new(Enforcement::Local, "concord.tests.planning", 1)
    }

    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        self.effects.limiter_acquire.fetch_add(1, Ordering::SeqCst);
        let mut visible = format!(
            "endpoint={} method={} origin={:?} page={} attempt={}/{}/{}",
            request.endpoint,
            request.method,
            request.logical_origin,
            request.page_index,
            request.attempt.index,
            request.attempt.policy_retry_index,
            request.attempt.authentication_recovery
        );
        for policy in request.plan.policies() {
            visible.push_str(policy.id().as_str());
            for part in policy.key().parts() {
                visible.push_str(part.name());
                visible.push_str(&String::from_utf8_lossy(part.value()));
            }
        }
        self.effects
            .limiter_views
            .lock()
            .expect("limiter capture")
            .push(visible);
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
                self.effects
                    .heads
                    .lock()
                    .expect("head capture")
                    .push((context.endpoint.to_owned(), context.target));
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
            _ => {}
        }
    }
}

api! {
    client SecurePlanningApi {
        timeout 30s;
        base "https://localhost/root/";
        credentials {
            credential bearer = custom CountingProvider;
        }

        POST protected(
            resource: String,
            protected_query: String,
            protected_header: String,
            body body: PlanningBodyInput via PlanningBodySpec,
        ) -> Bytes via Binary {
            id "planning.secure.protected";
            path "resources/{resource}";
            query { "api_key" = protected_query; }
            headers { "X-Api-Key" = protected_header; }
            auth { bearer bearer challenge unauthorized; }
            rate_limit { id "concord.planning.1"; key [origin]; cost 1; allow 1 per 1s; }
        }

        /// Runtime-binding owner: protected runtime endpoints bind here and
        /// inherit this owner's credential placement.
        scope secured() {
            auth { bearer bearer challenge unauthorized; }
        }
    }
}

api! {
    client PublicPlanningApi {
        timeout 30s;
        base "http://public-planning.example.test/root/";

        POST public_head(
            resource: String,
            query("q") query_value: String,
            header("X-Public") header_value: String,
            body body: PlanningBodyInput via PlanningBodySpec,
        ) -> Bytes via Binary {
            id "planning.public.head";
            path "resources/{resource}";
            rate_limit { id "concord.planning.2"; key [origin]; cost 1; allow 1 per 1s; }
        }

        GET lifecycle_probe() -> Bytes via Binary {
            id "planning.generated.lifecycle";
            path "lifecycle";
        }

        GET aggregate_head(query("value") each values: Vec<u16>) -> String via Text {
            id "planning.generated.aggregate_head";
        }
    }
}

api! {
    client StableBeforeApi {
        timeout 30s;
        base "http://stable-before.example.test/";

        GET old_rust_name() -> Bytes via Binary {
            id "stable.contract.identity";
        }
    }
}

api! {
    client StableAfterApi {
        timeout 30s;
        base "http://stable-after.example.test/";

        GET renamed_rust_name() -> Bytes via Binary {
            id "stable.contract.identity";
        }
    }
}

fn configured_public_client(
    server: &ScriptedServer,
    effects: &Arc<EffectCounts>,
    runtime_endpoint_timeout: Duration,
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
                .runtime_endpoint_timeout(runtime_endpoint_timeout)
                .rate_limiter(limiter)
                .operation_observer(hooks.clone())
                .operation_observer(hooks.clone())
                .operation_observer(hooks)
        })
        .build()
        .expect("public planning client")
}

fn secure_client(
    effects: &Arc<EffectCounts>,
    token: &'static str,
    http: impl FnOnce(HttpConfigBuilder) -> HttpConfigBuilder,
) -> SecurePlanningApi {
    let limiter: Arc<dyn RateLimiter> = Arc::new(CountingLimiter {
        effects: Arc::clone(effects),
    });
    let hooks = Arc::new(CountingHooks {
        effects: Arc::clone(effects),
    });
    SecurePlanningApi::builder()
        .bearer(CountingProvider {
            effects: Arc::clone(effects),
            token,
        })
        .configure_http(http)
        .configure_runtime(move |runtime| {
            runtime
                .rate_limiter(limiter)
                .operation_observer(hooks.clone())
                .operation_observer(hooks.clone())
                .operation_observer(hooks)
        })
        .build()
        .expect("secure planning client")
}

fn runtime_id(value: &str) -> EndpointId {
    EndpointId::new(value).expect("valid runtime endpoint ID")
}

#[cfg(feature = "default-tls")] // https base: TlsUnavailable without TLS
#[test]
fn safe_planning_uses_real_head_shaping_without_any_lifecycle_effect() {
    let effects = Arc::new(EffectCounts::default());
    let client = secure_client(&effects, PROVIDER_SENTINEL, |http| http);
    let endpoint = SecurePlanningApi::unbound().protected(
        "resource",
        QUERY_SENTINEL,
        HEADER_SENTINEL,
        PlanningBodyInput {
            effects: Arc::clone(&effects),
            fail: false,
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
    // The static description stands in for the deferred exact media/length.
    assert_eq!(plan.body_description().family(), Some(BodyKind::Stream));
    assert_eq!(
        plan.body_description().media_hint(),
        Some("application/x-planning")
    );
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
    let authorization = concord::http::HeaderName::from_static("authorization");
    assert!(plan.public_headers().get(&authorization).is_none());

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
        assert!(!safe_output.contains(sentinel), "{sentinel}");
    }
}

// RUNTIME-02 / §16.2: runtime endpoints can no longer attach a credential
// reference of their own (v2's `auth(Auth::bearer(credentials().x().credential()))`
// is removed). A runtime endpoint inherits the placements of the owner it is
// bound at, so the credential context is always the binding client's own:
// independent builds stay isolated and clones share installed material.
#[cfg(feature = "default-tls")]
#[tokio::test]
async fn runtime_endpoints_use_only_the_binding_clients_credential_context() {
    let server = TlsServer::start().expect("TLS server");
    let tunnel = ConnectProxy::start(server.address()).expect("tunnel");
    let http = || {
        let proxy = SafeProxy::parse(tunnel.url()).expect("proxy");
        let certificate = server.certificate_pem();
        move |http: HttpConfigBuilder| {
            http.http1_only()
                .proxy(proxy)
                .add_root_certificate_pem(certificate)
        }
    };
    let owner_effects = Arc::new(EffectCounts::default());
    let foreign_effects = Arc::new(EffectCounts::default());
    let owner = secure_client(&owner_effects, "owner-token", http());
    let foreign = secure_client(&foreign_effects, "foreign-token", http());
    let endpoint = || {
        RuntimeEndpoint::builder(runtime_id("planning.runtime.inherited_auth"), Method::GET)
            .path("inherited")
            .response::<Binary, Bytes>()
            .build()
            .expect("runtime endpoint")
    };

    // Planning under either owner acquires nothing.
    let owner_plan = owner
        .secured()
        .plan_runtime(&endpoint())
        .expect("owner plan");
    let foreign_plan = foreign
        .secured()
        .plan_runtime(&endpoint())
        .expect("foreign plan");
    assert!(owner_plan.capabilities().authentication());
    assert!(foreign_plan.capabilities().authentication());
    owner_effects.assert_no_effects();
    foreign_effects.assert_no_effects();

    owner
        .secured()
        .bind(endpoint())
        .expect("bind under owner")
        .await
        .expect("owner request");
    foreign
        .secured()
        .bind(endpoint())
        .expect("bind under foreign")
        .await
        .expect("foreign request");
    owner
        .clone()
        .secured()
        .bind(endpoint())
        .expect("bind under a clone")
        .await
        .expect("clone request");

    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].target(), "/root/inherited");
    assert_eq!(
        requests[0].header("authorization"),
        Some("Bearer owner-token")
    );
    assert_eq!(
        requests[1].header("authorization"),
        Some("Bearer foreign-token")
    );
    assert_eq!(
        requests[2].header("authorization"),
        Some("Bearer owner-token")
    );
    assert_eq!(
        owner_effects.provider.load(Ordering::SeqCst),
        1,
        "a clone shares the owner's installed material"
    );
    assert_eq!(foreign_effects.provider.load(Ordering::SeqCst), 1);
}

#[cfg(feature = "default-tls")] // https base: TlsUnavailable without TLS
#[test]
fn v3_e05_invalid_body_fails_before_provider_and_admission() {
    let effects = Arc::new(EffectCounts::default());
    let client = secure_client(&effects, PROVIDER_SENTINEL, |http| http);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let error = runtime
        .block_on(std::future::IntoFuture::into_future(client.protected(
            "resource",
            QUERY_SENTINEL,
            HEADER_SENTINEL,
            PlanningBodyInput {
                effects: Arc::clone(&effects),
                fail: true,
            },
        )))
        .expect_err("body preparation fails");
    assert_eq!(error.category(), ErrorCategory::RequestEncoding);
    assert_eq!(error.failure_stage(), FailureStage::Preparation);
    assert_eq!(error.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    assert_eq!(effects.body_prepare.load(Ordering::SeqCst), 1);
    assert_eq!(effects.body_factory.load(Ordering::SeqCst), 0);
    assert_eq!(
        effects.provider.load(Ordering::SeqCst),
        0,
        "no provider work for an already invalid body"
    );
    assert_eq!(effects.limiter_acquire.load(Ordering::SeqCst), 0);
    assert_eq!(effects.before_send.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn runtime_endpoint_timeout_does_not_change_generated_timeout_or_lazy_execution() {
    let server =
        ScriptedServer::start([ResponseScript::ok(b"planned".to_vec())]).expect("plan server");
    let effects = Arc::new(EffectCounts::default());
    let client = configured_public_client(&server, &effects, Duration::from_secs(17));
    let endpoint = PublicPlanningApi::unbound().public_head(
        UNKNOWN_PATH_SENTINEL,
        UNKNOWN_QUERY_SENTINEL,
        UNKNOWN_HEADER_SENTINEL,
        PlanningBodyInput {
            effects: Arc::clone(&effects),
            fail: false,
        },
    );

    let plan = client.plan(&endpoint).expect("generated public plan");
    effects.assert_no_effects();
    assert_eq!(
        plan.timeout_selection(),
        TimeoutSelection::Explicit(Duration::from_secs(30))
    );
    assert_eq!(plan.resolved_timeout(), Some(Duration::from_secs(30)));
    assert!(plan.observed_url().contains(UNKNOWN_PATH_SENTINEL));
    assert!(plan.observed_url().contains(UNKNOWN_QUERY_SENTINEL));
    assert_eq!(
        plan.public_headers()
            .get(&concord::http::HeaderName::from_static("x-public")),
        Some(SanitizedHeaderValue::Text(UNKNOWN_HEADER_SENTINEL))
    );
    let plan_debug = format!("{plan:?}");
    for sentinel in [
        UNKNOWN_PATH_SENTINEL,
        UNKNOWN_QUERY_SENTINEL,
        UNKNOWN_HEADER_SENTINEL,
    ] {
        assert!(
            !plan_debug.contains(sentinel),
            "RequestPlan Debug exposed explicit caller metadata {sentinel}"
        );
    }

    assert_eq!(
        client
            .request(endpoint)
            .await
            .expect("planned endpoint execution"),
        Bytes::from_static(b"planned")
    );
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
    assert_eq!(server.requests()[0].body(), BODY_SENTINEL);

    let heads = effects.heads.lock().expect("head capture");
    assert_eq!(heads.len(), 1);
    assert_eq!(heads[0].0, plan.endpoint_id());
    let (_, after_scheme) = plan
        .observed_url()
        .split_once("://")
        .expect("planned observed URL scheme");
    let (_, path_and_query) = after_scheme
        .split_once('/')
        .expect("planned observed URL path");
    let (path, query) = path_and_query
        .split_once('?')
        .map_or((path_and_query, None), |(path, query)| (path, Some(query)));
    assert_eq!(
        heads[0].1.path_segment_count(),
        path.split('/')
            .filter(|segment| !segment.is_empty())
            .count()
    );
    assert_eq!(
        heads[0].1.query_occurrence_count(),
        query.map_or(0, |query| {
            query
                .split('&')
                .filter(|occurrence| !occurrence.is_empty())
                .count()
        })
    );
    assert_eq!(
        heads[0].1.public_header_occurrence_count(),
        plan.public_headers().len()
    );
    let automatic_views = format!(
        "{heads:?}\n{:?}",
        effects.limiter_views.lock().expect("limiter capture")
    );
    for sentinel in [
        UNKNOWN_PATH_SENTINEL,
        UNKNOWN_QUERY_SENTINEL,
        UNKNOWN_HEADER_SENTINEL,
    ] {
        assert!(
            !automatic_views.contains(sentinel),
            "automatic authority exposed unknown public value {sentinel}"
        );
    }
}

#[tokio::test]
async fn runtime_endpoint_plan_remains_zero_effect() {
    let server = ScriptedServer::start([]).expect("unused planning server");
    let effects = Arc::new(EffectCounts::default());
    let client = configured_public_client(&server, &effects, Duration::from_secs(23));

    let inherited = RuntimeEndpoint::builder(runtime_id("runtime.timeout.inherited"), Method::GET)
        .path("inherited")
        .response::<Binary, Bytes>()
        .build()
        .expect("inherited runtime endpoint");
    let explicit = RuntimeEndpoint::builder(runtime_id("runtime.timeout.explicit"), Method::GET)
        .path("explicit")
        .timeout(Duration::from_secs(7))
        .response::<Binary, Bytes>()
        .build()
        .expect("explicit runtime endpoint");

    let inherited_plan = client.plan_runtime(&inherited).expect("inherited plan");
    let explicit_plan = client.plan_runtime(&explicit).expect("explicit plan");
    effects.assert_no_effects();

    // §3.3 / §16.1: a runtime endpoint bound at an owner inherits that owner's
    // nearest finite timeout (here the declared 30 s), not a separate
    // client-wide runtime-endpoint default.
    assert_eq!(
        inherited_plan.resolved_timeout(),
        Some(Duration::from_secs(30))
    );
    assert_eq!(
        explicit_plan.timeout_selection(),
        TimeoutSelection::Explicit(Duration::from_secs(7))
    );
    assert_eq!(
        explicit_plan.resolved_timeout(),
        Some(Duration::from_secs(7))
    );

    // Selection and reset are request controls on the bound request (§8.1);
    // binding performs no work.
    let bound = client
        .bind(explicit)
        .expect("binding is inert")
        .timeout(Duration::from_secs(9))
        .expect("selected timeout");
    assert_eq!(
        bound.control_summary().selected_timeout(),
        Some(Duration::from_secs(9))
    );
    let reset = bound.reset_timeout();
    assert!(!reset.control_summary().timeout_was_selected());
    assert_eq!(
        reset.control_summary().selected_timeout(),
        Some(Duration::from_secs(7))
    );
    drop(reset);
    effects.assert_no_effects();

    let description = inherited_plan.description();
    assert_eq!(description.id(), "runtime.timeout.inherited");
    assert_eq!(
        description.response_acquisition(),
        ResponseAcquisition::Buffered
    );
    assert!(!description.capabilities().pagination());
    assert!(!description.capabilities().credential_storage());
    // Only owner-level attachments are inherited: the root declares no auth
    // and no rate policy (the policies of `public_head` are endpoint-local).
    assert!(!description.capabilities().authentication());
    assert!(!description.capabilities().rate_limits());
    assert_eq!(server.request_count(), 0);
}

#[test]
fn runtime_method_limit_is_public_bounded_and_preserved_by_planning() {
    let server = ScriptedServer::start([]).expect("unused method-limit planning server");
    let effects = Arc::new(EffectCounts::default());
    let client = configured_public_client(&server, &effects, Duration::from_secs(23));
    let exact_text = "M".repeat(concord::endpoint::MAX_RUNTIME_ENDPOINT_METHOD_BYTES);
    let exact_method = Method::from_bytes(exact_text.as_bytes()).expect("exact method token");
    let endpoint =
        RuntimeEndpoint::builder(runtime_id("runtime.method.exact"), exact_method.clone())
            .response::<Binary, Bytes>()
            .build()
            .expect("exact runtime method limit");
    let plan = client.plan_runtime(&endpoint).expect("exact method plan");
    assert_eq!(plan.method(), &exact_method);
    assert!(format!("{plan:?}").len() < 2_048);
    effects.assert_no_effects();

    let excessive_text = "N".repeat(concord::endpoint::MAX_RUNTIME_ENDPOINT_METHOD_BYTES + 1);
    let excessive_method =
        Method::from_bytes(excessive_text.as_bytes()).expect("excess method token");
    let error = RuntimeEndpoint::builder(runtime_id("runtime.method.excess"), excessive_method)
        .response::<Binary, Bytes>()
        .build()
        .expect_err("first excess runtime method");
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
    assert_eq!(error.category(), ErrorCategory::Configuration);

    let mut runtime_exact = RuntimeEndpoint::builder(runtime_id("runtime.head.exact"), Method::GET);
    for _ in 0..dynamic_components {
        runtime_exact = runtime_exact.path("x");
    }
    let runtime_exact = runtime_exact
        .response::<Text, String>()
        .build()
        .expect("runtime exact structural boundary");
    client
        .plan_runtime(&runtime_exact)
        .expect("runtime head exact boundary including Accept");

    let mut runtime_excess =
        RuntimeEndpoint::builder(runtime_id("runtime.head.excess"), Method::GET);
    for _ in 0..(dynamic_components + 1) {
        runtime_excess = runtime_excess.path("x");
    }
    let error = runtime_excess
        .response::<Text, String>()
        .build()
        .expect_err("runtime first excess includes the retained method component");
    // The v3 runtime builder charges each recorded path operation to the
    // structural budget and reports the first excess path segment as a route
    // error (V3-S11); the method component is still counted first.
    assert_eq!(error.kind(), ConfigurationErrorKind::InvalidRoute);
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
    let runtime = RuntimeEndpoint::builder(runtime_id("planning.runtime.lifecycle"), Method::GET)
        .path("lifecycle")
        .response::<Binary, Bytes>()
        .build()
        .expect("runtime lifecycle endpoint");
    assert_eq!(
        client
            .bind(runtime)
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
    // The static contract is readable without any runtime service (V3-R12).
    assert!(
        StableAfterApi::contract()
            .endpoint("stable.contract.identity")
            .is_some()
    );
}
