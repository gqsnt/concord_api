#![cfg(feature = "multipart")]
//! Runtime endpoints through generated v3 owners (§16, RUNTIME-01/02).
//!
//! Obligations: V3-R09 (root and scope binding carry path/query/header/auth/
//! rates/time with symbolic suppression; the bound plan agrees with the
//! actual send, including over TLS), V3-R10 (runtime public replacement
//! parity with generated programs for absent/empty/each, static name bounds,
//! forbidden origin/private-source/rate removal rejected), V3-R11
//! (accepted/challenge overlap, generated-ID collision and a missing limiter
//! cannot bypass bind).

use std::future::poll_fn;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use concord::api;
use concord::auth::AuthTarget;
use concord::body::{
    BodyError, BodyFactory, Multipart, MultipartBody, MultipartFactory, MultipartPart,
    PreparedBody, RequestStream,
};
use concord::codec::{
    CodecError, CodecErrorKind, DecodeBody, DecodeContext, EncodeBody, EncodeContext,
};
use concord::endpoint::{
    EndpointId, MAX_RUNTIME_KEY_BYTES, RuntimeAuthDisposition, RuntimeEndpoint,
    RuntimeEndpointBuilder,
};
use concord::error::{ConfigurationErrorKind, ErrorCategory};
use concord::http::{Method, SafeProxy, StatusCode};
use concord::observation::{OperationEvent, OperationObserver};
use concord::rate_limit::{
    BoxFuture, Enforcement, LimiterCapability, LocalRateLimiter, NoopRateLimiter,
    RateLimitDeclaration, RateLimitError, RateLimitPermit, RateLimitRequest, RateLimiter,
};
use concord::response::ByteStream;
use concord::{ApiResponse, Binary, Bytes, Discard, Empty, SecretString, Text};
use concord_test_support::proxy::ConnectProxy;
use concord_test_support::server::{ResponseScript, ScriptedServer};
use concord_test_support::tls::TlsServer;
use futures_core::Stream;

fn runtime_id(value: &str) -> EndpointId {
    EndpointId::new(value).expect("valid runtime endpoint ID")
}

fn secret(value: &str) -> SecretString {
    SecretString::try_new(value).expect("bounded test secret")
}

api! {
    client PreparedAlphaApi {
        base "http://alpha.prepared.example.test/root/";
        timeout 30s;

        GET explicit() -> Bytes via Binary { path "explicit"; }
    }
}

api! {
    client PreparedBetaApi {
        base "http://beta.prepared.example.test/other/";
        timeout 30s;

        GET explicit() -> Bytes via Binary { path "explicit"; }
    }
}

api! {
    client PreparedAuthApi {
        base "http://auth.prepared.example.test/";
        timeout 30s;
        credentials {
            secret token;
            credential api_key = api_key(secret.token);
        }
        auth { header "X-Api-Key" = api_key; }

        GET explicit() -> () via Empty;

        scope custom() {
            auth replace { authorization "Token" = api_key; }
            GET declared() -> () via Empty;
        }
    }
}

api! {
    client RuntimeScopeApi {
        base "http://scope-runtime.example.test/root/";
        timeout 30s;
        values { tenant: String; }
        rate_limits {
            policy tenant_reads(tenant) {
                id "runtime.scope.tenant";
                key [endpoint, "tenant" = tenant];
                cost 1;
                allow 10 per 1m;
            }
        }
        path "v2";
        query { "tenant" = tenant; }

        scope regional(region: String) {
            timeout 5s;
            subdomain ["api"];
            path "regions/{region}";
            query { append "view" = "scope"; }
            headers { "X-Scope" = region; }
            rate_limit tenant_reads(tenant = client.tenant);

            GET declared() -> () via Empty { id "runtime.scope.declared"; }
        }
    }
}

api! {
    client RuntimeAuthScopeApi {
        base "https://runtime-auth-scope.example.test/";
        timeout 30s;
        credentials {
            secret token;
            credential api_key = api_key(secret.token);
            credential access = bearer(secret.token);
        }

        scope protected() {
            auth {
                header "X-Runtime-Scope-Key" = api_key challenge never;
                query "token" = api_key;
            }
            GET declared() -> () via Empty;
        }

        scope challenged() {
            auth { bearer access challenge unauthorized; }
            GET declared() -> () via Empty;
        }
    }
}

api! {
    client RuntimeTlsApi {
        base "https://localhost/api/";
        timeout 30s;
        credentials {
            secret token;
            credential api_key = api_key(secret.token);
        }
        rate_limits {
            policy reads() {
                id "runtime.tls.reads";
                key [origin];
                allow 100 per 1m;
            }
        }

        scope protected(region: String) {
            timeout 7s;
            path "regions/{region}";
            query { "view" = "full"; }
            headers { "X-Region" = region; }
            auth { header "X-Runtime-Key" = api_key; }
            rate_limit reads();

            GET declared() -> () via Empty;
        }
    }
}

api! {
    client ParityApi {
        base "http://parity.example.test/";
        timeout 30s;
        query { "a" = "1"; "locale" = "fr"; "z" = "9"; }

        GET when_present_locale(locale?: String) -> () via Empty {
            id "parity.when_present";
            path "items";
            query { when_present "locale" = locale; }
        }

        GET replace_locale(locale?: String) -> () via Empty {
            id "parity.replace";
            path "items";
            query { replace "locale" = locale; }
        }

        GET append_locale(locale?: String) -> () via Empty {
            id "parity.append";
            path "items";
            query { append "locale" = locale; }
        }

        GET replace_each(locales: Vec<String>) -> () via Empty {
            id "parity.replace_each";
            path "items";
            query { replace "locale" = each locales; }
        }

        GET when_present_each(locales?: Vec<String>) -> () via Empty {
            id "parity.when_present_each";
            path "items";
            query { when_present "locale" = each locales; }
        }

        GET append_each(locales: Vec<String>) -> () via Empty {
            id "parity.append_each";
            path "items";
            query { append "locale" = each locales; }
        }

        GET removed() -> () via Empty {
            id "parity.remove";
            path "items";
            query { remove "locale"; }
        }
    }
}

struct EchoCodec;

struct CountingEncodeCodec;

impl EncodeBody for CountingEncodeCodec {
    type Input = Arc<AtomicUsize>;

    const CONTENT_TYPE: Option<&'static str> = None;

    fn encode(input: Self::Input, _context: EncodeContext<'_>) -> Result<Bytes, CodecError> {
        input.fetch_add(1, Ordering::SeqCst);
        Ok(Bytes::from_static(b"counted-codec"))
    }
}

impl EncodeBody for EchoCodec {
    type Input = String;

    const CONTENT_TYPE: Option<&'static str> = Some("application/x-concord-prepared");

    fn encode(input: Self::Input, _context: EncodeContext<'_>) -> Result<Bytes, CodecError> {
        Ok(Bytes::from(input))
    }
}

impl DecodeBody for EchoCodec {
    type Output = String;

    const ACCEPT: Option<&'static str> = Some("application/x-concord-prepared");

    fn validate_content_type(content_type: Option<&str>) -> Result<(), CodecError> {
        if content_type == Self::ACCEPT {
            Ok(())
        } else {
            Err(CodecError::new(CodecErrorKind::Decode))
        }
    }

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        String::from_utf8(body.to_vec()).map_err(|_| CodecError::new(CodecErrorKind::Decode))
    }
}

struct FreshFactory {
    calls: Arc<AtomicUsize>,
}

impl BodyFactory for FreshFactory {
    fn create(&self) -> Result<RequestStream, BodyError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(RequestStream::from_bytes(Bytes::from_static(
            b"factory-body",
        )))
    }
}

struct FreshMultipartFactory {
    calls: Arc<AtomicUsize>,
}

impl MultipartFactory for FreshMultipartFactory {
    fn create(&self) -> Result<MultipartBody, BodyError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        MultipartBody::new().part("factory", MultipartPart::text("multipart-factory")?)
    }
}

#[derive(Default)]
struct CapturingLimiter {
    plans: Mutex<Vec<CapturedPlan>>,
}

type CapturedKey = Vec<(String, Vec<u8>)>;

#[derive(Debug, PartialEq, Eq)]
struct CapturedRule {
    id: String,
    parts: CapturedKey,
    cost: u32,
    windows: Vec<(u32, Duration)>,
}

type CapturedPlan = Vec<CapturedRule>;

fn canonical_utf8(value: &str) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(1 + 4 + 8 + value.len());
    encoded.push(4);
    encoded.extend_from_slice(b"utf8");
    encoded.extend_from_slice(&(value.len() as u64).to_be_bytes());
    encoded.extend_from_slice(value.as_bytes());
    encoded
}

impl RateLimiter for CapturingLimiter {
    fn capability(&self) -> LimiterCapability {
        LimiterCapability::new(Enforcement::Local, "test.runtime.capturing", 1)
    }

    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        let plan = request
            .plan
            .policies()
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
            .collect();
        Box::pin(async move {
            self.plans.lock().expect("plan capture lock").push(plan);
            Ok(RateLimitPermit::granted())
        })
    }
}

struct RejectingHooks;

impl OperationObserver for RejectingHooks {
    fn observe(&self, event: OperationEvent<'_>) {
        if matches!(event, OperationEvent::AttemptStarted(_)) {
            panic!("test observer panic");
        }
    }
}

struct CountingHooks {
    calls: Arc<AtomicUsize>,
}

impl OperationObserver for CountingHooks {
    fn observe(&self, event: OperationEvent<'_>) {
        if matches!(event, OperationEvent::AttemptStarted(_)) {
            self.calls.fetch_add(1, Ordering::SeqCst);
        }
    }
}

fn proxy(server: &ScriptedServer) -> SafeProxy {
    SafeProxy::parse(server.url("")).expect("loopback proxy")
}

fn alpha(server: &ScriptedServer) -> PreparedAlphaApi {
    PreparedAlphaApi::builder()
        .configure_http({
            let proxy = proxy(server);
            move |http| http.proxy(proxy)
        })
        .build()
        .expect("prepared alpha client")
}

fn offline_alpha() -> PreparedAlphaApi {
    PreparedAlphaApi::new().expect("offline alpha client")
}

fn auth_scope_client() -> RuntimeAuthScopeApi {
    RuntimeAuthScopeApi::builder()
        .token(secret("scope-secret-5d1c"))
        .build()
        .expect("runtime scope auth client")
}

fn window_rule(id: &str) -> RateLimitDeclaration {
    RateLimitDeclaration::new(id).window(10, Duration::from_secs(1))
}

fn build_kind(builder: RuntimeEndpointBuilder) -> Option<ConfigurationErrorKind> {
    builder
        .response::<Empty, ()>()
        .build()
        .err()
        .map(|error| error.kind())
}

fn empty(builder: RuntimeEndpointBuilder) -> RuntimeEndpoint<Empty, ()> {
    builder
        .response::<Empty, ()>()
        .build()
        .expect("structurally valid runtime endpoint")
}

// ---------------------------------------------------------------------------
// Construction surface and ordinary execution (RUNTIME-01).
// ---------------------------------------------------------------------------

#[cfg(feature = "json")]
mod canonical {
    use super::*;

    #[derive(serde::Deserialize)]
    pub struct Item {
        pub _id: Option<String>,
    }

    /// The exact §16.1 construction compiles through the facade.
    #[test]
    fn section_16_1_construction_compiles() -> Result<(), concord::error::ConfigurationError> {
        let endpoint =
            RuntimeEndpoint::builder(EndpointId::new("catalog.dynamic.lookup")?, Method::GET)
                .path("items")
                .path("42")
                .query("view", "summary")
                .response::<concord::Json, Item>()
                .build()?;
        assert_eq!(endpoint.description().id(), "catalog.dynamic.lookup");
        Ok(())
    }
}

/// v2 switched response selections repeatedly (`accepted_bytes()`,
/// `response_empty()`, `response_discard()`); v3 selects the output pair
/// exactly once from the unselected factory builder (RUNTIME-01) and exposes
/// accepted bytes explicitly on a buffered/discard descriptor.
#[test]
fn exact_runtime_endpoint_types_are_available_from_endpoint_domain() {
    let unselected: RuntimeEndpointBuilder =
        RuntimeEndpoint::builder(runtime_id("runtime.default"), Method::GET);
    let buffered: RuntimeEndpointBuilder<EchoCodec, String> =
        unselected.response::<EchoCodec, String>();
    let _: RuntimeEndpoint<EchoCodec, String> = buffered.build().expect("buffered endpoint");
    let _: RuntimeEndpoint<Binary, Bytes> =
        RuntimeEndpoint::builder(runtime_id("runtime.bytes"), Method::GET)
            .response::<Binary, Bytes>()
            .expose_accepted_bytes()
            .build()
            .expect("exposed bytes endpoint");
    let _: RuntimeEndpoint<Binary, ByteStream> =
        RuntimeEndpoint::builder(runtime_id("runtime.stream"), Method::GET)
            .response::<Binary, ByteStream>()
            .build()
            .expect("stream endpoint");
    let _: RuntimeEndpoint<Empty, ()> = empty(RuntimeEndpoint::builder(
        runtime_id("runtime.empty"),
        Method::GET,
    ));
    let _: RuntimeEndpoint<Discard, ()> =
        RuntimeEndpoint::builder(runtime_id("runtime.discard"), Method::DELETE)
            .response::<Discard, ()>()
            .expose_accepted_bytes()
            .build()
            .expect("discard endpoint");
}

#[tokio::test]
async fn prepared_modes_use_the_ordinary_generated_client_runtime() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"decoded".to_vec())
            .with_header("content-type", "application/x-concord-prepared"),
        ResponseScript::ok(b"bytes".to_vec()),
        ResponseScript::ok(b"ignored".to_vec()),
        ResponseScript::ok(b"streamed".to_vec())
            .with_header("content-type", "application/octet-stream"),
    ])
    .expect("prepared mode server");
    let client = alpha(&server);

    let buffered = RuntimeEndpoint::builder(runtime_id("runtime.buffered"), Method::POST)
        .path("items")
        .path("slash value")
        .path("dot-dot")
        .path("tail?query#fragment%value")
        .query("first", 1_u8)
        .query_optional("omitted", Option::<u8>::None)
        .query_each("repeat", vec![2_u8, 3])
        .header("x-prepared-endpoint", "new")
        .body_repr::<EchoCodec, _>("encoded".to_owned())
        .response::<EchoCodec, String>()
        .build()
        .expect("buffered runtime endpoint");
    assert_eq!(
        client
            .bind(buffered)
            .expect("runtime endpoint binds to client")
            .await
            .expect("buffered response"),
        "decoded"
    );

    let bytes = RuntimeEndpoint::builder(runtime_id("runtime.bytes"), Method::GET)
        .path("bytes")
        .response::<Binary, Bytes>()
        .build()
        .expect("bytes runtime endpoint");
    assert_eq!(
        client
            .bind(bytes)
            .expect("runtime endpoint binds to client")
            .await
            .expect("bytes response"),
        b"bytes"[..]
    );

    let no_content = RuntimeEndpoint::builder(runtime_id("runtime.none"), Method::DELETE)
        .path("none")
        .response::<Discard, ()>()
        .build()
        .expect("no-content runtime endpoint");
    client
        .bind(no_content)
        .expect("runtime endpoint binds to client")
        .await
        .expect("no-content response");

    let stream = RuntimeEndpoint::builder(runtime_id("runtime.stream"), Method::GET)
        .path("stream")
        .response::<Binary, ByteStream>()
        .build()
        .expect("stream runtime endpoint");
    let mut response: ByteStream = client
        .bind(stream)
        .expect("runtime endpoint binds to client")
        .await
        .expect("stream response");
    let first = poll_fn(|context| Pin::new(&mut response).poll_next(context))
        .await
        .expect("stream chunk")
        .expect("valid stream chunk");
    assert_eq!(first, b"streamed"[..]);
    assert!(
        poll_fn(|context| Pin::new(&mut response).poll_next(context))
            .await
            .is_none()
    );

    server
        .wait_for_requests(4, Duration::from_secs(2))
        .expect("four prepared sends");
    let requests = server.requests();
    assert_eq!(requests[0].method(), "POST");
    assert!(requests[0].target().contains(
        "/root/items/slash%20value/dot-dot/tail%3Fquery%23fragment%25value?first=1&repeat=2&repeat=3"
    ));
    assert_eq!(requests[0].header("x-prepared-endpoint"), Some("new"));
    assert_eq!(
        requests[0].header("content-type"),
        Some("application/x-concord-prepared")
    );
    assert_eq!(requests[0].body(), b"encoded");
    assert_eq!(requests[1].method(), "GET");
    assert_eq!(requests[2].method(), "DELETE");
    assert_eq!(requests[3].method(), "GET");
}

#[tokio::test]
async fn runtime_endpoint_binary_output_is_exact_and_bounded() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"runtime-binary".to_vec())
            .with_header("content-type", "application/octet-stream"),
        ResponseScript::ok(b"accepted-bytes".to_vec()).with_header("content-type", "text/plain"),
    ])
    .expect("runtime binary server");
    let client = alpha(&server);
    let endpoint = RuntimeEndpoint::builder(runtime_id("runtime.binary"), Method::GET)
        .path("binary")
        .response::<Binary, Bytes>()
        .build()
        .expect("runtime binary endpoint");

    let response: ApiResponse<Bytes> = client
        .bind(endpoint)
        .expect("runtime endpoint binds to client")
        .response()
        .await
        .expect("runtime endpoint binary response");
    assert_eq!(response.value(), b"runtime-binary".as_slice());
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.metadata().endpoint(), "runtime.binary");
    assert_eq!(
        server.requests()[0].header("accept"),
        Some("application/octet-stream")
    );

    // The alternate accepted-bytes terminal is explicit on the descriptor and
    // bypasses only representation validation (§10.4).
    let exposed = RuntimeEndpoint::builder(runtime_id("runtime.accepted_bytes"), Method::GET)
        .path("text")
        .response::<Text, String>()
        .expose_accepted_bytes()
        .build()
        .expect("exposed runtime endpoint");
    let bytes = client
        .bind(exposed)
        .expect("runtime endpoint binds to client")
        .accepted_bytes()
        .expect("exposed accepted bytes")
        .await
        .expect("accepted bytes response");
    assert_eq!(bytes.value().as_ref(), b"accepted-bytes");
    let unexposed = RuntimeEndpoint::builder(runtime_id("runtime.unexposed"), Method::GET)
        .response::<Text, String>()
        .build()
        .expect("unexposed runtime endpoint");
    let error = client
        .bind(unexposed)
        .expect("runtime endpoint binds to client")
        .accepted_bytes()
        .expect_err("not exposed by the descriptor");
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(server.request_count(), 2);
}

#[tokio::test]
async fn runtime_sensitive_values_reach_the_request_but_not_the_plan() {
    const PATH_SENTINEL: &str = "runtime-path-sensitive-81a7";
    const QUERY_SENTINEL: &str = "runtime-query-sensitive-92b8";
    const OPTIONAL_SENTINEL: &str = "runtime-optional-sensitive-a3c9";
    const EACH_SENTINEL: &str = "runtime-each-sensitive-b4da";
    const HEADER_SENTINEL: &str = "runtime-header-sensitive-c5eb";
    const HEADER_EACH_SENTINEL: &str = "runtime-header-each-sensitive-d6fc";

    let server = ScriptedServer::start([ResponseScript::ok(Vec::new())])
        .expect("runtime sensitivity server");
    let client = alpha(&server);
    let endpoint = || {
        empty(
            RuntimeEndpoint::builder(runtime_id("runtime.sensitive"), Method::GET)
                .path_sensitive(PATH_SENTINEL)
                // Neutral key names: redaction here comes from the explicit
                // `_sensitive` classification, not mandatory name matching.
                .query_sensitive("classified", QUERY_SENTINEL)
                .query_optional_sensitive("optional", Some(OPTIONAL_SENTINEL))
                .query_each_sensitive("each", vec![EACH_SENTINEL, EACH_SENTINEL])
                .header_sensitive("x-runtime-sensitive", HEADER_SENTINEL)
                .header_each_sensitive(
                    "x-runtime-sensitive-each",
                    vec![HEADER_EACH_SENTINEL, HEADER_EACH_SENTINEL],
                ),
        )
    };

    let plan = client
        .plan_runtime(&endpoint())
        .expect("runtime sensitive plan");
    let bound = client
        .bind(endpoint())
        .expect("runtime sensitive endpoint binds");
    let evidence = format!(
        "{plan:?}\n{}\n{:?}\n{bound:?}\n{:?}",
        plan.observed_url(),
        plan.public_headers(),
        bound.description()
    );
    for sentinel in [
        PATH_SENTINEL,
        QUERY_SENTINEL,
        OPTIONAL_SENTINEL,
        EACH_SENTINEL,
        HEADER_SENTINEL,
        HEADER_EACH_SENTINEL,
    ] {
        assert!(!evidence.contains(sentinel), "{sentinel}");
    }
    bound.await.expect("runtime sensitive endpoint executes");

    let request = &server.requests()[0];
    for sentinel in [
        PATH_SENTINEL,
        QUERY_SENTINEL,
        OPTIONAL_SENTINEL,
        EACH_SENTINEL,
    ] {
        assert!(request.target().contains(sentinel));
    }
    assert_eq!(request.header("x-runtime-sensitive"), Some(HEADER_SENTINEL));
    assert_eq!(
        request.header("x-runtime-sensitive-each"),
        Some(HEADER_EACH_SENTINEL)
    );
}

// ---------------------------------------------------------------------------
// V3-R09: owner projection and plan/send agreement.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn runtime_endpoint_binding_to_scope_uses_effective_owner_patches_and_timeout() {
    let server = ScriptedServer::start([
        ResponseScript::ok(Vec::new()),
        ResponseScript::ok(Vec::new()),
    ])
    .expect("runtime scope server");
    let proxy = proxy(&server);
    let limiter = Arc::new(CapturingLimiter::default());
    let configured_limiter = Arc::clone(&limiter);
    let client = RuntimeScopeApi::builder()
        .set_tenant("tenant-a")
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.rate_limiter(configured_limiter))
        .build()
        .expect("runtime scope client");
    let scope = client.regional("west");

    let inherited = || {
        empty(
            RuntimeEndpoint::builder(runtime_id("runtime.scope.inherited"), Method::GET)
                .path("items"),
        )
    };
    let plan = scope
        .plan_runtime(&inherited())
        .expect("scope-owned runtime plan");
    assert_eq!(plan.resolved_timeout(), Some(Duration::from_secs(5)));
    assert_eq!(plan.rate_limit_policy_count(), 1);
    assert!(plan.capabilities().rate_limits());
    assert_eq!(
        plan.observed_url(),
        "http://api.scope-runtime.example.test/root/v2/regions/west/items?tenant=tenant-a&view=scope"
    );
    let bound = scope
        .bind(inherited())
        .expect("runtime endpoint binds to scope");
    assert_eq!(
        bound.plan().expect("bound plan").observed_url(),
        plan.observed_url()
    );
    let facts = bound
        .description()
        .runtime_binding()
        .expect("runtime binding facts");
    assert_eq!(facts.owner(), Some("RuntimeScopeApi::regional"));
    assert_eq!(facts.effective_rate_policies(), 1);
    bound.await.expect("scope-owned runtime request");

    // Replacement is explicit (PATCH-02): a plain `query("tenant", ..)` on an
    // inherited key is a bind-time conflict, `query_replace` is the override.
    let conflict = scope
        .bind(empty(
            RuntimeEndpoint::builder(runtime_id("runtime.scope.conflict"), Method::GET)
                .query("tenant", "runtime"),
        ))
        .expect_err("introducing an inherited key conflicts");
    assert_eq!(
        conflict.configuration_kind(),
        Some(ConfigurationErrorKind::InvalidValue)
    );
    let replaced = empty(
        RuntimeEndpoint::builder(runtime_id("runtime.scope.replaced"), Method::GET)
            .path_replace()
            .path("replacement")
            .query_replace("tenant", "runtime")
            .header_off()
            .timeout(Duration::from_secs(9)),
    );
    let replaced_plan = scope.plan_runtime(&replaced).expect("replaced plan");
    assert_eq!(
        replaced_plan.resolved_timeout(),
        Some(Duration::from_secs(9))
    );
    scope
        .bind(replaced)
        .expect("patched runtime endpoint binds to scope")
        .await
        .expect("patched scope-owned runtime request");

    let requests = server.requests();
    // The bound plan agrees with the actual send (absolute-form proxy target).
    assert_eq!(requests[0].target(), plan.observed_url());
    assert_eq!(requests[0].header("x-scope"), Some("west"));
    assert_eq!(requests[1].target(), replaced_plan.observed_url());
    assert_eq!(
        requests[1].target(),
        "http://api.scope-runtime.example.test/root/replacement?view=scope&tenant=runtime"
    );
    assert_eq!(requests[1].header("x-scope"), None);
    let plans = limiter.plans.lock().expect("captured runtime scope plans");
    assert_eq!(plans.len(), 2);
    assert_eq!(plans[0][0].id, "runtime.scope.tenant");
    assert_eq!(
        plans[0][0].parts,
        [
            (
                "endpoint".to_owned(),
                canonical_utf8("runtime.scope.inherited"),
            ),
            ("tenant".to_owned(), canonical_utf8("tenant-a")),
        ]
    );
    // Every inherited rate constraint remains after whole-family fences.
    assert_eq!(plans[1][0].id, "runtime.scope.tenant");
}

#[tokio::test(flavor = "current_thread")]
async fn bound_plan_agrees_with_the_actual_tls_send_including_private_auth() {
    const TOKEN: &str = "runtime-tls-credential-3f9a";
    let server = TlsServer::start().expect("tls server");
    let tunnel = ConnectProxy::start(server.address()).expect("tunnel");
    let proxy = SafeProxy::parse(tunnel.url()).expect("proxy");
    let certificate = server.certificate_pem();
    let client = RuntimeTlsApi::builder()
        .token(secret(TOKEN))
        .configure_http(move |http| {
            http.http1_only()
                .proxy(proxy)
                .add_root_certificate_pem(certificate)
        })
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(LocalRateLimiter::new())))
        .build()
        .expect("tls client");
    server.queue_response(200, Vec::new()).expect("queue");
    server.queue_response(200, Vec::new()).expect("queue");
    let scope = client.protected("eu");

    let endpoint = || {
        empty(
            RuntimeEndpoint::builder(runtime_id("runtime.tls.items"), Method::GET)
                .path("items")
                .path("7")
                .query("page", 2_u32)
                .header("x-runtime", "yes"),
        )
    };
    let plan = scope.plan_runtime(&endpoint()).expect("tls plan");
    assert_eq!(
        plan.observed_url(),
        "https://localhost/api/regions/eu/items/7?view=full&page=2"
    );
    assert_eq!(plan.resolved_timeout(), Some(Duration::from_secs(7)));
    assert_eq!(plan.rate_limit_policy_count(), 1);
    assert!(plan.capabilities().authentication());
    let x_region = concord::http::HeaderName::from_static("x-region");
    let x_runtime = concord::http::HeaderName::from_static("x-runtime");
    let x_key = concord::http::HeaderName::from_static("x-runtime-key");
    assert!(plan.public_headers().get(&x_region).is_some());
    assert!(plan.public_headers().get(&x_runtime).is_some());
    assert!(plan.public_headers().get(&x_key).is_none());
    assert!(!format!("{plan:?}").contains(TOKEN));
    scope
        .bind(endpoint())
        .expect("tls binding")
        .await
        .expect("tls runtime request");

    // Removing the inherited placement by its safe identity: the plan says
    // so prominently and the credential is not sent (RUNTIME-02).
    let removed = empty(
        RuntimeEndpoint::builder(runtime_id("runtime.tls.removed"), Method::GET)
            .path("public")
            .auth_remove(AuthTarget::header("X-Runtime-Key").expect("target")),
    );
    let removed_plan = scope.plan_runtime(&removed).expect("removed plan");
    assert!(!removed_plan.capabilities().authentication());
    assert!(
        format!("{removed_plan:?}").starts_with(
            "RequestPlan { runtime_authentication: INHERITED AUTHENTICATION PARTIALLY REMOVED"
        ),
        "{removed_plan:?}"
    );
    scope
        .bind(removed)
        .expect("removed binding")
        .await
        .expect("removed runtime request");

    server
        .wait_for_requests(2, Duration::from_secs(5))
        .expect("two tls requests");
    let requests = server.requests();
    assert_eq!(
        format!("https://localhost{}", requests[0].target()),
        plan.observed_url()
    );
    assert_eq!(requests[0].header("x-region"), Some("eu"));
    assert_eq!(requests[0].header("x-runtime"), Some("yes"));
    assert_eq!(requests[0].header("x-runtime-key"), Some(TOKEN));
    assert_eq!(
        format!("https://localhost{}", requests[1].target()),
        removed_plan.observed_url()
    );
    assert_eq!(requests[1].header("x-runtime-key"), None);
    assert_eq!(requests[1].header("x-region"), Some("eu"));
}

api! {
    /// A generated ROOT owner declaring every inheritable family itself:
    /// root path, query and headers, bearer authentication, a timeout and a
    /// rate policy (V3-R09 root binding).
    client RuntimeRootApi {
        base "https://localhost/api/";
        timeout 12s;
        values { tenant: String; }
        credentials {
            secret token;
            credential access = bearer(secret.token);
        }
        rate_limits {
            policy root_reads(tenant) {
                id "runtime.root.reads";
                key [origin, "tenant" = tenant];
                allow 100 per 1m;
            }
        }
        path "v3";
        query { "tenant" = tenant; "view" = "root"; }
        headers { "X-Tenant" = tenant; "X-Root" = "root-header"; }
        auth { bearer access; }
        rate_limit root_reads(tenant = tenant);

        GET declared() -> () via Empty {
            id "runtime.root.declared";
            path "declared";
        }
    }
}

/// One limiter admission as the limiter saw it.
#[derive(Debug)]
struct CapturedAdmission {
    endpoint: String,
    logical_origin: Option<String>,
    rules: CapturedPlan,
}

/// A limiter recording the endpoint, logical origin and complete protected
/// canonical keys of every admission.
#[derive(Default)]
struct AdmissionCapturingLimiter {
    admissions: Mutex<Vec<CapturedAdmission>>,
}

impl RateLimiter for AdmissionCapturingLimiter {
    fn capability(&self) -> LimiterCapability {
        LimiterCapability::new(Enforcement::Local, "test.runtime.admissions", 1)
    }

    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        let rules = request
            .plan
            .policies()
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
            .collect();
        self.admissions
            .lock()
            .expect("admission capture lock")
            .push(CapturedAdmission {
                endpoint: request.endpoint.to_owned(),
                logical_origin: request.logical_origin.map(str::to_owned),
                rules,
            });
        Box::pin(async { Ok(RateLimitPermit::granted()) })
    }
}

/// Every public header the plan shows is sent with exactly that value.
fn assert_public_headers_sent(
    plan: &concord::endpoint::RequestPlan,
    request: &concord_test_support::tls::TlsRequest,
) {
    for (name, value) in plan.public_headers().iter() {
        match value {
            concord::observation::SanitizedHeaderValue::Text(text) => {
                assert_eq!(request.header(name.as_str()), Some(text), "{name}");
            }
            other => panic!("unexpected non-text public header {name}: {other:?}"),
        }
    }
}

/// V3-R09 at a generated ROOT (RUNTIME-01: root binding uses the same
/// projection as scope binding): `client.bind(runtime_endpoint)` inherits
/// the root path, query, headers, bearer authentication, timeout and rate
/// policy. For an inherited endpoint, a root-level override
/// (`path_replace`, `query_replace`, `header_off`, `timeout`) and a root
/// `auth_off`, the bound plan's target, public headers and timeout equal
/// the actual TLS send, the private Authorization is sent exactly when the
/// plan says authentication applies, and the limiter's captured origin and
/// tenant key equal the plan's destination and the root value — also after
/// the query override, which never changes the protected rate key.
#[tokio::test(flavor = "current_thread")]
async fn runtime_endpoint_bound_at_the_generated_root_matches_the_actual_tls_send() {
    const TOKEN: &str = "runtime-root-credential-8d2e";
    const TENANT: &str = "tenant-root-7c";
    let server = TlsServer::start().expect("tls server");
    let tunnel = ConnectProxy::start(server.address()).expect("tunnel");
    let proxy = SafeProxy::parse(tunnel.url()).expect("proxy");
    let certificate = server.certificate_pem();
    let limiter = Arc::new(AdmissionCapturingLimiter::default());
    let configured_limiter = Arc::clone(&limiter);
    let client = RuntimeRootApi::builder()
        .set_tenant(TENANT)
        .token(secret(TOKEN))
        .configure_http(move |http| {
            http.http1_only()
                .proxy(proxy)
                .add_root_certificate_pem(certificate)
        })
        .configure_runtime(move |runtime| runtime.rate_limiter(configured_limiter))
        .build()
        .expect("runtime root client");
    for _ in 0..3 {
        server.queue_response(200, Vec::new()).expect("queue");
    }
    let x_tenant = concord::http::HeaderName::from_static("x-tenant");
    let x_root = concord::http::HeaderName::from_static("x-root");
    let x_runtime = concord::http::HeaderName::from_static("x-runtime");
    let authorization = concord::http::HeaderName::from_static("authorization");

    // 1. Inherited root program plus local operations and an additive
    //    runtime policy.
    let inherited = || {
        empty(
            RuntimeEndpoint::builder(runtime_id("runtime.root.items"), Method::GET)
                .path("items")
                .path("7")
                .query("page", 2_u32)
                .header("x-runtime", "yes")
                .rate_limit(
                    RateLimitDeclaration::new("runtime.root.extra")
                        .endpoint()
                        .window(10, Duration::from_secs(1)),
                ),
        )
    };
    let inherited_plan = client
        .plan_runtime(&inherited())
        .expect("root-owned runtime plan");
    assert_eq!(
        inherited_plan.observed_url(),
        format!("https://localhost/api/v3/items/7?tenant={TENANT}&view=root&page=2")
    );
    assert_eq!(
        inherited_plan.resolved_timeout(),
        Some(Duration::from_secs(12))
    );
    assert_eq!(inherited_plan.rate_limit_policy_count(), 2);
    let schemas: Vec<(String, Vec<String>)> = inherited_plan
        .rate_limit()
        .policies()
        .iter()
        .map(|policy| {
            (
                policy.schema().id().as_str().to_owned(),
                policy
                    .schema()
                    .dimensions()
                    .iter()
                    .map(|dimension| dimension.label().to_owned())
                    .collect(),
            )
        })
        .collect();
    assert_eq!(
        schemas,
        [
            (
                "runtime.root.reads".to_owned(),
                vec!["origin".to_owned(), "tenant".to_owned()]
            ),
            ("runtime.root.extra".to_owned(), vec!["endpoint".to_owned()]),
        ]
    );
    assert!(inherited_plan.capabilities().authentication());
    assert!(inherited_plan.public_headers().get(&x_tenant).is_some());
    assert!(inherited_plan.public_headers().get(&x_root).is_some());
    assert!(inherited_plan.public_headers().get(&x_runtime).is_some());
    assert!(
        inherited_plan
            .public_headers()
            .get(&authorization)
            .is_none()
    );
    assert!(!format!("{inherited_plan:?}").contains(TOKEN));
    let bound = client
        .bind(inherited())
        .expect("runtime endpoint binds at the root");
    assert_eq!(
        bound.plan().expect("bound plan").observed_url(),
        inherited_plan.observed_url()
    );
    let facts = bound
        .description()
        .runtime_binding()
        .expect("runtime binding facts");
    assert_eq!(facts.owner(), Some("RuntimeRootApi"));
    assert_eq!(facts.inherited_auth_placements(), 1);
    assert_eq!(facts.effective_auth_placements(), 1);
    assert_eq!(facts.effective_rate_policies(), 2);
    bound.await.expect("root-owned runtime request");

    // The root projection is the one a generated root endpoint uses.
    let declared = client.declared().plan().expect("generated root plan");
    assert_eq!(
        declared.observed_url(),
        format!("https://localhost/api/v3/declared?tenant={TENANT}&view=root")
    );
    for name in [&x_tenant, &x_root] {
        assert_eq!(
            format!("{:?}", declared.public_headers().get(name)),
            format!("{:?}", inherited_plan.public_headers().get(name)),
        );
    }

    // 2. Root-level overrides against the inherited root program.
    let overridden = empty(
        RuntimeEndpoint::builder(runtime_id("runtime.root.overridden"), Method::GET)
            .path_replace()
            .path("replaced")
            .query_replace("tenant", "runtime-tenant")
            .header_off()
            .timeout(Duration::from_secs(9)),
    );
    let overridden_plan = client
        .plan_runtime(&overridden)
        .expect("overridden root plan");
    assert_eq!(
        overridden_plan.observed_url(),
        "https://localhost/api/replaced?view=root&tenant=runtime-tenant"
    );
    assert_eq!(
        overridden_plan.resolved_timeout(),
        Some(Duration::from_secs(9))
    );
    assert!(overridden_plan.public_headers().is_empty());
    assert!(overridden_plan.capabilities().authentication());
    assert_eq!(overridden_plan.rate_limit_policy_count(), 1);
    client
        .bind(overridden)
        .expect("overridden runtime endpoint binds at the root")
        .await
        .expect("overridden root-owned runtime request");

    // 3. Removing the root authentication is explicit and prominent.
    let public = empty(
        RuntimeEndpoint::builder(runtime_id("runtime.root.public"), Method::GET)
            .path("public")
            .auth_off(),
    );
    let public_plan = client.plan_runtime(&public).expect("public root plan");
    assert!(!public_plan.capabilities().authentication());
    assert!(
        format!("{public_plan:?}")
            .contains("INHERITED AUTHENTICATION REMOVED by runtime auth_off()"),
        "{public_plan:?}"
    );
    assert_eq!(
        public_plan.observed_url(),
        format!("https://localhost/api/v3/public?tenant={TENANT}&view=root")
    );
    client
        .bind(public)
        .expect("auth-off runtime endpoint binds at the root")
        .await
        .expect("auth-off root-owned runtime request");

    server
        .wait_for_requests(3, Duration::from_secs(5))
        .expect("three tls requests");
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    for (request, plan) in requests
        .iter()
        .zip([&inherited_plan, &overridden_plan, &public_plan])
    {
        assert_eq!(request.method(), "GET");
        assert_eq!(
            format!("https://localhost{}", request.target()),
            plan.observed_url()
        );
        assert_public_headers_sent(plan, request);
    }
    assert_eq!(requests[0].header("x-tenant"), Some(TENANT));
    assert_eq!(requests[0].header("x-root"), Some("root-header"));
    assert_eq!(requests[0].header("x-runtime"), Some("yes"));
    let bearer = format!("Bearer {TOKEN}");
    assert_eq!(requests[0].header("authorization"), Some(bearer.as_str()));
    assert_eq!(requests[1].header("x-tenant"), None);
    assert_eq!(requests[1].header("x-root"), None);
    assert_eq!(requests[1].header("authorization"), Some(bearer.as_str()));
    assert_eq!(requests[2].header("x-tenant"), Some(TENANT));
    assert_eq!(requests[2].header("authorization"), None);

    // The captured limiter keys agree with the plans and the send: the
    // origin is the plan's destination with its explicit effective port and
    // the tenant is the root value (a query override never changes it).
    let origin = "https://localhost:443";
    assert!(
        inherited_plan
            .observed_url()
            .starts_with("https://localhost/")
    );
    let root_rule = || CapturedRule {
        id: "runtime.root.reads".to_owned(),
        parts: vec![
            ("origin".to_owned(), canonical_utf8(origin)),
            ("tenant".to_owned(), canonical_utf8(TENANT)),
        ],
        cost: 1,
        windows: vec![(100, Duration::from_secs(60))],
    };
    let admissions = limiter.admissions.lock().expect("captured admissions");
    assert_eq!(admissions.len(), 3, "{admissions:?}");
    assert_eq!(admissions[0].endpoint, "runtime.root.items");
    assert_eq!(admissions[0].logical_origin.as_deref(), Some(origin));
    assert_eq!(
        admissions[0].rules,
        [
            root_rule(),
            CapturedRule {
                id: "runtime.root.extra".to_owned(),
                parts: vec![("endpoint".to_owned(), canonical_utf8("runtime.root.items"))],
                cost: 1,
                windows: vec![(10, Duration::from_secs(1))],
            },
        ]
    );
    assert_eq!(admissions[1].endpoint, "runtime.root.overridden");
    assert_eq!(admissions[1].logical_origin.as_deref(), Some(origin));
    assert_eq!(admissions[1].rules, [root_rule()]);
    assert_eq!(admissions[2].endpoint, "runtime.root.public");
    assert_eq!(
        admissions[2].rules,
        [root_rule()],
        "RATE-01: auth_off keeps quotas"
    );
}

#[test]
fn runtime_scope_auth_is_inherited_and_can_be_explicitly_disabled() {
    let client = auth_scope_client();
    let scope = client.protected();

    let inherited = scope
        .bind(empty(RuntimeEndpoint::builder(
            runtime_id("runtime.scope.auth.description"),
            Method::GET,
        )))
        .expect("auth-inheriting runtime endpoint");
    assert!(inherited.description().capabilities().authentication());
    let facts = inherited.description().runtime_binding().expect("facts");
    assert_eq!(facts.authentication(), &RuntimeAuthDisposition::Inherited);
    assert_eq!(facts.effective_auth_placements(), 2);
    assert!(
        inherited
            .plan()
            .expect("inherited auth plan")
            .capabilities()
            .authentication()
    );

    // HEAD-01: a public writer cannot target an effective private placement.
    let collision = scope
        .bind(empty(
            RuntimeEndpoint::builder(runtime_id("runtime.scope.auth.inherited"), Method::GET)
                .header("x-runtime-scope-key", "public"),
        ))
        .expect_err("inherited private placement collides with public header");
    assert_eq!(
        collision.configuration_kind(),
        Some(ConfigurationErrorKind::InvalidAuthentication)
    );

    let auth_free = empty(
        RuntimeEndpoint::builder(runtime_id("runtime.scope.auth.off"), Method::GET)
            .auth_off()
            .header("x-runtime-scope-key", "public"),
    );
    let auth_free_plan = scope
        .plan_runtime(&auth_free)
        .expect("auth-free runtime plan");
    assert!(!auth_free_plan.capabilities().authentication());
    assert!(
        format!("{auth_free_plan:?}")
            .contains("INHERITED AUTHENTICATION REMOVED by runtime auth_off()")
    );
    let request = scope
        .bind(auth_free)
        .expect("auth-free runtime endpoint binds");
    assert_eq!(
        request
            .description()
            .runtime_binding()
            .expect("facts")
            .authentication(),
        &RuntimeAuthDisposition::RemovedAll
    );
}

#[tokio::test]
async fn the_same_prepared_definition_uses_each_selected_client_base_and_core() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"alpha".to_vec()),
        ResponseScript::ok(b"beta".to_vec()),
    ])
    .expect("multi-client server");
    let alpha_calls = Arc::new(AtomicUsize::new(0));
    let beta_calls = Arc::new(AtomicUsize::new(0));
    let alpha_selected = Arc::clone(&alpha_calls);
    let beta_selected = Arc::clone(&beta_calls);
    let alpha = PreparedAlphaApi::builder()
        .configure_http({
            let proxy = proxy(&server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime(move |runtime| {
            runtime.operation_observer(Arc::new(CountingHooks {
                calls: alpha_selected,
            }))
        })
        .build()
        .expect("runtime-distinct alpha client");
    let beta = PreparedBetaApi::builder()
        .configure_http({
            let proxy = proxy(&server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime(move |runtime| {
            runtime.operation_observer(Arc::new(CountingHooks {
                calls: beta_selected,
            }))
        })
        .build()
        .expect("runtime-distinct beta client");
    let definition = || {
        RuntimeEndpoint::builder(runtime_id("runtime.portable"), Method::GET)
            .path("portable")
            .response::<Binary, Bytes>()
            .build()
            .expect("portable endpoint")
    };

    assert_eq!(
        alpha
            .bind(definition())
            .expect("runtime endpoint binds to alpha")
            .await
            .expect("alpha"),
        b"alpha"[..]
    );
    assert_eq!(alpha_calls.load(Ordering::SeqCst), 1);
    assert_eq!(beta_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        beta.bind(definition())
            .expect("runtime endpoint binds to beta")
            .await
            .expect("beta"),
        b"beta"[..]
    );
    assert_eq!(alpha_calls.load(Ordering::SeqCst), 1);
    assert_eq!(beta_calls.load(Ordering::SeqCst), 1);
    let requests = server.requests();
    assert!(
        requests[0]
            .target()
            .contains("alpha.prepared.example.test/root/portable")
    );
    assert!(
        requests[1]
            .target()
            .contains("beta.prepared.example.test/other/portable")
    );
}

// ---------------------------------------------------------------------------
// Bodies, observers and limits (§9, §16.2 Body row).
// ---------------------------------------------------------------------------

/// All managed bodies use the typed `body_repr::<B, T>(input)` selection
/// (§16.2 Body row).
#[tokio::test]
async fn all_prepared_body_forms_use_the_existing_lazy_body_architecture() {
    let server = ScriptedServer::start((0..7).map(|_| ResponseScript::ok(Vec::new())))
        .expect("prepared body server");
    let client = alpha(&server);
    let factory_calls = Arc::new(AtomicUsize::new(0));
    let multipart_factory_calls = Arc::new(AtomicUsize::new(0));
    let post = |id: &str| RuntimeEndpoint::builder(runtime_id(id), Method::POST);

    let bodies = [
        empty(post("runtime.empty").body_repr::<PreparedBody, _>(PreparedBody::empty())),
        empty(post("runtime.bytes").body_repr::<Binary, _>(Bytes::from_static(b"bytes-body"))),
        empty(
            post("runtime.stream").body_repr::<Binary, _>(RequestStream::from_bytes(
                Bytes::from_static(b"stream-body"),
            )),
        ),
        empty(
            post("runtime.factory").body_repr::<PreparedBody, _>(PreparedBody::factory(
                FreshFactory {
                    calls: Arc::clone(&factory_calls),
                },
            )),
        ),
        empty(post("runtime.codec").body_repr::<EchoCodec, _>("codec-body".to_owned())),
        empty(
            post("runtime.multipart").body_repr::<Multipart, _>(
                MultipartBody::new()
                    .part(
                        "field",
                        MultipartPart::text("multipart-body").expect("bounded multipart text"),
                    )
                    .expect("multipart part"),
            ),
        ),
        empty(
            post("runtime.multipart_factory").body_repr::<PreparedBody, _>(
                PreparedBody::multipart_factory(FreshMultipartFactory {
                    calls: Arc::clone(&multipart_factory_calls),
                })
                .expect("multipart factory body"),
            ),
        ),
    ];

    assert_eq!(factory_calls.load(Ordering::SeqCst), 0);
    assert_eq!(multipart_factory_calls.load(Ordering::SeqCst), 0);
    for endpoint in bodies {
        let bound = client
            .bind(endpoint)
            .expect("runtime endpoint binds to client");
        // Binding and planning never prepare a body (V3-S12).
        let before = (
            factory_calls.load(Ordering::SeqCst),
            multipart_factory_calls.load(Ordering::SeqCst),
        );
        let _ = bound.plan().expect("inert plan");
        assert_eq!(
            (
                factory_calls.load(Ordering::SeqCst),
                multipart_factory_calls.load(Ordering::SeqCst),
            ),
            before
        );
        bound.await.expect("prepared body send");
    }
    assert_eq!(factory_calls.load(Ordering::SeqCst), 1);
    assert_eq!(multipart_factory_calls.load(Ordering::SeqCst), 1);
    let requests = server.requests();
    assert_eq!(requests[0].body(), b"");
    assert_eq!(requests[1].body(), b"bytes-body");
    assert_eq!(requests[2].body(), b"stream-body");
    assert_eq!(requests[3].body(), b"factory-body");
    assert_eq!(requests[4].body(), b"codec-body");
    assert!(
        requests[5]
            .body()
            .windows(b"multipart-body".len())
            .any(|window| window == b"multipart-body")
    );
    assert!(
        requests[6]
            .body()
            .windows(b"multipart-factory".len())
            .any(|window| window == b"multipart-factory")
    );
}

#[tokio::test]
async fn observer_panic_cannot_reject_runtime_endpoint_work() {
    let server = ScriptedServer::start([
        ResponseScript::ok(Vec::new()),
        ResponseScript::ok(Vec::new()),
    ])
    .expect("observer panic server");
    let calls = Arc::new(AtomicUsize::new(0));
    let client = PreparedAlphaApi::builder()
        .configure_http({
            let proxy = proxy(&server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime(|runtime| runtime.operation_observer(Arc::new(RejectingHooks)))
        .build()
        .expect("hooked client");
    let endpoint = empty(
        RuntimeEndpoint::builder(runtime_id("runtime.observed"), Method::POST)
            .body_repr::<PreparedBody, _>(PreparedBody::factory(FreshFactory {
                calls: Arc::clone(&calls),
            })),
    );

    client
        .bind(endpoint)
        .expect("runtime endpoint binds to client")
        .await
        .expect("observer has no veto authority");
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let encode_calls = Arc::new(AtomicUsize::new(0));
    let endpoint = empty(
        RuntimeEndpoint::builder(runtime_id("runtime.codec_observed"), Method::POST)
            .body_repr::<CountingEncodeCodec, _>(Arc::clone(&encode_calls)),
    );
    client
        .bind(endpoint)
        .expect("runtime endpoint binds to client")
        .await
        .expect("observer has no codec veto authority");
    assert_eq!(encode_calls.load(Ordering::SeqCst), 1);
    assert_eq!(server.request_count(), 2);
}

/// v2 runtime endpoints created their own private placements
/// (`.auth(Auth::header(..))`); RUNTIME-02 removes that authority, so the
/// credentials now come from the binding owner and the cleartext rule is
/// enforced at bind, before any observer, body or transport work.
#[tokio::test]
async fn prepared_protected_http_fails_before_observer_or_body_materialization() {
    let server = ScriptedServer::start([]).expect("protected preparation server");
    let hook_calls = Arc::new(AtomicUsize::new(0));
    let selected_hooks = Arc::clone(&hook_calls);
    let client = PreparedAuthApi::builder()
        .token(secret("prepared-credential-15e8d20c"))
        .configure_http({
            let proxy = proxy(&server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime(move |runtime| {
            runtime.operation_observer(Arc::new(CountingHooks {
                calls: selected_hooks,
            }))
        })
        .build()
        .expect("prepared authentication client");
    let factory_calls = Arc::new(AtomicUsize::new(0));
    let endpoint = empty(
        RuntimeEndpoint::builder(runtime_id("runtime.protected_http"), Method::POST)
            .body_repr::<PreparedBody, _>(PreparedBody::factory(FreshFactory {
                calls: Arc::clone(&factory_calls),
            })),
    );

    let error = client
        .bind(endpoint)
        .expect_err("authenticated cleartext must fail");
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(hook_calls.load(Ordering::SeqCst), 0);
    assert_eq!(factory_calls.load(Ordering::SeqCst), 0);
    assert_eq!(server.request_count(), 0);

    let custom_factory_calls = Arc::new(AtomicUsize::new(0));
    let endpoint = empty(
        RuntimeEndpoint::builder(
            runtime_id("runtime.custom_authorization_http"),
            Method::POST,
        )
        .body_repr::<PreparedBody, _>(PreparedBody::factory(FreshFactory {
            calls: Arc::clone(&custom_factory_calls),
        })),
    );
    let error = client
        .custom()
        .bind(endpoint)
        .expect_err("custom Authorization over cleartext must fail");
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(hook_calls.load(Ordering::SeqCst), 0);
    assert_eq!(custom_factory_calls.load(Ordering::SeqCst), 0);
    assert_eq!(server.request_count(), 0);

    // Removing every inherited placement leaves an auth-free cleartext
    // request, which binds.
    client
        .custom()
        .bind(empty(
            RuntimeEndpoint::builder(runtime_id("runtime.cleartext_off"), Method::GET).auth_off(),
        ))
        .expect("auth-free cleartext binding");
}

#[tokio::test]
async fn prepared_bodies_and_responses_cannot_bypass_runtime_limits() {
    let request_server = ScriptedServer::start([]).expect("prepared request-limit server");
    let request_client = PreparedAlphaApi::builder()
        .configure_http({
            let proxy = proxy(&request_server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime(|runtime| runtime.request_body_limit(3))
        .build()
        .expect("request-limited client");
    for endpoint in [
        empty(
            RuntimeEndpoint::builder(runtime_id("runtime.limited_bytes"), Method::POST)
                .body_repr::<Binary, _>(Bytes::from_static(b"four")),
        ),
        empty(
            RuntimeEndpoint::builder(runtime_id("runtime.limited_codec"), Method::POST)
                .body_repr::<EchoCodec, _>("four".to_owned()),
        ),
    ] {
        let error = request_client
            .bind(endpoint)
            .expect("runtime endpoint binds to client")
            .await
            .expect_err("prepared request body limit");
        assert_eq!(error.category(), ErrorCategory::RequestBody);
    }
    assert_eq!(request_server.request_count(), 0);

    let response_server = ScriptedServer::start([ResponseScript::ok(b"four".to_vec())])
        .expect("prepared response-limit server");
    let response_client = PreparedAlphaApi::builder()
        .configure_http({
            let proxy = proxy(&response_server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime(|runtime| runtime.response_body_limit(3))
        .build()
        .expect("response-limited client");
    let endpoint = RuntimeEndpoint::builder(runtime_id("runtime.limited_response"), Method::GET)
        .response::<Binary, Bytes>()
        .build()
        .expect("limited response endpoint");
    let error = response_client
        .bind(endpoint)
        .expect("runtime endpoint binds to client")
        .await
        .expect_err("prepared response body limit");
    assert_eq!(error.category(), ErrorCategory::ResponseBody);
    assert_eq!(response_server.request_count(), 1);
}

// ---------------------------------------------------------------------------
// Rate policies (RATE-01, V3-R11).
// ---------------------------------------------------------------------------

/// v2 runtime rules were resolved `RateLimitPolicy` values (including a
/// dimensionless global key); v3 runtime policies are explicit
/// `RateLimitDeclaration`s whose built-in dimensions resolve from the bound
/// head, and every v3 key has at least one dimension (§12.3).
#[tokio::test]
async fn prepared_custom_limiter_inspects_ids_keys_costs_and_windows_in_call_order() {
    let server = ScriptedServer::start([
        ResponseScript::ok(Vec::new()),
        ResponseScript::ok(Vec::new()),
    ])
    .expect("prepared limiter server");
    let endpoint = || {
        empty(
            RuntimeEndpoint::builder(runtime_id("runtime.limited"), Method::GET)
                .rate_limit(
                    RateLimitDeclaration::new("global")
                        .window(10, Duration::from_secs(1))
                        .window(100, Duration::from_secs(10))
                        .origin(),
                )
                .rate_limit(
                    RateLimitDeclaration::new("one-part")
                        .cost(2)
                        .window(20, Duration::from_secs(2))
                        .part("tenant", "west"),
                )
                .rate_limit(
                    RateLimitDeclaration::new("several-parts")
                        .cost(3)
                        .window(30, Duration::from_secs(3))
                        .window(300, Duration::from_secs(30))
                        .endpoint()
                        .part("tenant", "east")
                        .part("region", "eu"),
                ),
        )
    };

    let explicit_noop = PreparedAlphaApi::builder()
        .configure_http({
            let proxy = proxy(&server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(NoopRateLimiter::new())))
        .build()
        .expect("explicit no-op client");
    let noop_plan = explicit_noop
        .plan_runtime(&endpoint())
        .expect("explicit no-op plan");
    assert_eq!(
        noop_plan.rate_limit().enforcement_label(),
        "declared policies; enforcement explicitly disabled by application selection"
    );
    explicit_noop
        .bind(endpoint())
        .expect("runtime endpoint binds to client")
        .await
        .expect("explicit no-op authorizes plan");

    let limiter = Arc::new(CapturingLimiter::default());
    let selected = Arc::clone(&limiter);
    let custom = PreparedAlphaApi::builder()
        .configure_http({
            let proxy = proxy(&server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime(move |runtime| runtime.rate_limiter(selected))
        .build()
        .expect("custom limiter client");
    let bound = custom
        .bind(endpoint())
        .expect("runtime endpoint binds to client");
    assert!(limiter.plans.lock().expect("captured plans").is_empty());
    bound.await.expect("custom limiter authorizes plan");
    assert_eq!(
        limiter.plans.lock().expect("captured plans").as_slice(),
        [vec![
            CapturedRule {
                id: "global".to_owned(),
                parts: vec![(
                    "origin".to_owned(),
                    canonical_utf8("http://alpha.prepared.example.test:80"),
                )],
                cost: 1,
                windows: vec![(10, Duration::from_secs(1)), (100, Duration::from_secs(10))],
            },
            CapturedRule {
                id: "one-part".to_owned(),
                parts: vec![("tenant".to_owned(), canonical_utf8("west"))],
                cost: 2,
                windows: vec![(20, Duration::from_secs(2))],
            },
            CapturedRule {
                id: "several-parts".to_owned(),
                parts: vec![
                    ("endpoint".to_owned(), canonical_utf8("runtime.limited")),
                    ("tenant".to_owned(), canonical_utf8("east")),
                    ("region".to_owned(), canonical_utf8("eu")),
                ],
                cost: 3,
                windows: vec![(30, Duration::from_secs(3)), (300, Duration::from_secs(30))],
            },
        ]]
    );
}

#[test]
fn missing_limiter_cannot_bypass_bind() {
    let client = offline_alpha();
    let governed = || {
        empty(
            RuntimeEndpoint::builder(runtime_id("runtime.governed"), Method::GET)
                .rate_limit(window_rule("runtime.governed").origin()),
        )
    };
    for error in [
        client
            .plan_runtime(&governed())
            .expect_err("plan without limiter"),
        client.bind(governed()).expect_err("bind without limiter"),
    ] {
        assert_eq!(
            error.configuration_kind(),
            Some(ConfigurationErrorKind::InvalidRateLimit)
        );
        assert_eq!(error.category(), ErrorCategory::Configuration);
    }
    // An ungoverned runtime endpoint on the same client binds.
    client
        .bind(empty(RuntimeEndpoint::builder(
            runtime_id("runtime.ungoverned"),
            Method::GET,
        )))
        .expect("ungoverned runtime endpoint binds");
}

#[test]
fn inherited_rate_policies_cannot_be_removed_or_redefined() {
    let client = RuntimeScopeApi::builder()
        .set_tenant("tenant-a")
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(LocalRateLimiter::new())))
        .build()
        .expect("runtime scope client");
    let scope = client.regional("west");
    // No runtime operation removes a rate policy: every fence leaves it.
    let fenced = empty(
        RuntimeEndpoint::builder(runtime_id("runtime.scope.fenced"), Method::GET)
            .path_off()
            .query_off()
            .header_off()
            .auth_off(),
    );
    let plan = scope.plan_runtime(&fenced).expect("fenced plan");
    assert_eq!(plan.rate_limit_policy_count(), 1);
    assert_eq!(
        plan.observed_url(),
        "http://api.scope-runtime.example.test/root/"
    );
    // A runtime policy is additive; redefining an inherited ID conflicts.
    let additive = empty(
        RuntimeEndpoint::builder(runtime_id("runtime.scope.additive"), Method::GET)
            .rate_limit(window_rule("runtime.scope.extra").endpoint()),
    );
    assert_eq!(
        scope
            .plan_runtime(&additive)
            .expect("additive plan")
            .rate_limit_policy_count(),
        2
    );
    let redefined = empty(
        RuntimeEndpoint::builder(runtime_id("runtime.scope.redefined"), Method::GET)
            .rate_limit(window_rule("runtime.scope.tenant").endpoint()),
    );
    assert_eq!(
        scope
            .bind(redefined)
            .expect_err("redefining an inherited policy")
            .configuration_kind(),
        Some(ConfigurationErrorKind::InvalidRateLimit)
    );
}

// ---------------------------------------------------------------------------
// V3-R10: replacement parity, name bounds, forbidden authority.
// ---------------------------------------------------------------------------

#[test]
fn runtime_public_replacement_parity_matches_generated_programs() {
    let client = ParityApi::new().expect("parity client");
    let runtime = |builder: RuntimeEndpointBuilder| {
        client
            .plan_runtime(&empty(builder.path("items")))
            .expect("runtime parity plan")
            .observed_url()
            .to_owned()
    };
    let base = || RuntimeEndpoint::builder(runtime_id("parity.runtime"), Method::GET);
    let generated = |plan: Result<concord::endpoint::RequestPlan, concord::ApiError>| {
        plan.expect("generated parity plan")
            .observed_url()
            .to_owned()
    };
    let url = |query: &str| format!("http://parity.example.test/items?{query}");

    let cases = [
        (
            generated(client.when_present_locale().plan()),
            runtime(base().query_when_present("locale", Option::<String>::None)),
            url("a=1&locale=fr&z=9"),
        ),
        (
            generated(client.when_present_locale().set_locale("de").plan()),
            runtime(base().query_when_present("locale", Some("de".to_owned()))),
            url("a=1&z=9&locale=de"),
        ),
        (
            generated(client.replace_locale().plan()),
            runtime(base().query_replace_optional("locale", Option::<String>::None)),
            url("a=1&z=9"),
        ),
        (
            generated(client.append_locale().set_locale("de").plan()),
            runtime(base().query_append_optional("locale", Some("de".to_owned()))),
            url("a=1&locale=fr&z=9&locale=de"),
        ),
        (
            generated(client.append_locale().plan()),
            runtime(base().query_append_optional("locale", Option::<String>::None)),
            url("a=1&locale=fr&z=9"),
        ),
        (
            generated(client.replace_each(Vec::new()).plan()),
            runtime(base().query_replace_each("locale", Vec::<String>::new())),
            url("a=1&z=9"),
        ),
        (
            generated(
                client
                    .replace_each(vec!["x".to_owned(), "y".to_owned()])
                    .plan(),
            ),
            runtime(base().query_replace_each("locale", vec!["x".to_owned(), "y".to_owned()])),
            url("a=1&z=9&locale=x&locale=y"),
        ),
        (
            generated(client.when_present_each().plan()),
            runtime(base().query_when_present_each("locale", Option::<Vec<String>>::None)),
            url("a=1&locale=fr&z=9"),
        ),
        (
            generated(client.when_present_each().set_locales(Vec::new()).plan()),
            runtime(base().query_when_present_each("locale", Some(Vec::<String>::new()))),
            url("a=1&z=9"),
        ),
        (
            generated(client.append_each(vec!["x".to_owned()]).plan()),
            runtime(base().query_append_each("locale", vec!["x".to_owned()])),
            url("a=1&locale=fr&z=9&locale=x"),
        ),
        (
            generated(client.removed().plan()),
            runtime(base().query_remove("locale")),
            url("a=1&z=9"),
        ),
    ];
    for (generated, runtime, expected) in cases {
        assert_eq!(generated, expected);
        assert_eq!(runtime, expected);
    }
    // Whole-family runtime fences.
    assert_eq!(
        runtime(base().query_off()),
        "http://parity.example.test/items"
    );
    assert_eq!(
        runtime(base().query_replace_all().query("b", "2")),
        url("b=2")
    );
}

#[test]
fn runtime_key_names_are_statically_bounded() {
    let base = || RuntimeEndpoint::builder(runtime_id("runtime.names"), Method::GET);
    let exact = "k".repeat(MAX_RUNTIME_KEY_BYTES);
    let excessive = "k".repeat(MAX_RUNTIME_KEY_BYTES + 1);
    assert_eq!(
        build_kind(base().query(&exact, "v").header(&exact, "v")),
        None
    );
    assert_eq!(
        build_kind(base().query(&excessive, "v")),
        Some(ConfigurationErrorKind::InvalidValue)
    );
    assert_eq!(
        build_kind(base().query("", "v")),
        Some(ConfigurationErrorKind::InvalidValue)
    );
    assert_eq!(
        build_kind(base().header(&excessive, "v")),
        Some(ConfigurationErrorKind::InvalidHeader)
    );
    assert_eq!(
        build_kind(base().header("bad header", "v")),
        Some(ConfigurationErrorKind::InvalidHeader)
    );
    // HEAD-01 singleton request headers cannot repeat.
    assert_eq!(
        build_kind(base().header_each("User-Agent", vec!["a", "b"])),
        Some(ConfigurationErrorKind::InvalidHeader)
    );
}

#[test]
fn runtime_endpoints_cannot_reach_another_origin_or_private_source() {
    let client = offline_alpha();
    // There is no runtime origin/scheme/port/absolute-URL operation; a path
    // atom is exactly one encoded segment under the fixed base.
    let plan = client
        .plan_runtime(&empty(
            RuntimeEndpoint::builder(runtime_id("runtime.origin"), Method::GET)
                .path("evil.example.test:443"),
        ))
        .expect("segment plan");
    assert_eq!(
        plan.observed_url(),
        "http://alpha.prepared.example.test/root/evil.example.test:443"
    );
    for smuggled in ["//evil.example.test", "https://evil.example.test", ".."] {
        let error = client
            .plan_runtime(&empty(
                RuntimeEndpoint::builder(runtime_id("runtime.origin"), Method::GET).path(smuggled),
            ))
            .expect_err("not one relative segment");
        assert_eq!(
            error.configuration_kind(),
            Some(ConfigurationErrorKind::InvalidRoute),
            "{smuggled}"
        );
    }
    let base = || RuntimeEndpoint::builder(runtime_id("runtime.reserved"), Method::GET);
    for reserved in [
        "Host",
        "authorization",
        "Proxy-Authorization",
        "Cookie",
        "Content-Type",
    ] {
        assert_eq!(
            build_kind(base().header(reserved, "value")),
            Some(ConfigurationErrorKind::InvalidHeader),
            "{reserved}"
        );
    }
    for method in ["CONNECT", "TRACE"] {
        assert_eq!(
            build_kind(RuntimeEndpoint::builder(
                runtime_id("runtime.method"),
                Method::from_bytes(method.as_bytes()).expect("method"),
            )),
            Some(ConfigurationErrorKind::InvalidValue)
        );
    }
}

/// v2 validated collisions among runtime-created placements at build; v3
/// runtime endpoints cannot create placements (RUNTIME-02), so the canonical
/// collision rules now apply at bind between runtime public writers and the
/// owner's effective private placements (HEAD-01), and removal is by safe
/// target identity only.
#[test]
fn prepared_authentication_uses_canonical_collision_and_challenge_validation() {
    let client = auth_scope_client();
    let scope = client.protected();
    let bind = |builder: RuntimeEndpointBuilder| {
        scope
            .bind(empty(builder))
            .err()
            .and_then(|error| error.configuration_kind())
    };
    let base = || RuntimeEndpoint::builder(runtime_id("runtime.auth"), Method::GET);
    let auth = Some(ConfigurationErrorKind::InvalidAuthentication);

    // Public writers cannot target a private header or query placement,
    // even through an absent optional source or an append.
    assert_eq!(bind(base().header("X-RUNTIME-SCOPE-KEY", "public")), auth);
    assert_eq!(
        bind(base().header_optional("x-runtime-scope-key", Option::<&str>::None)),
        auth
    );
    assert_eq!(bind(base().query("token", "public")), auth);
    assert_eq!(
        bind(base().query_append_each("token", vec!["public"])),
        auth
    );
    // Query keys are case-sensitive; unrelated names are compatible.
    assert_eq!(
        bind(base().query("TOKEN", "public").header("x-other", "public")),
        None
    );
    // Removing the placement by its identity frees the target.
    assert_eq!(
        bind(
            base()
                .auth_remove(AuthTarget::query("token").expect("target"))
                .query("token", "public")
        ),
        None
    );
    // A target no effective placement owns cannot be removed.
    assert_eq!(bind(base().auth_remove(AuthTarget::authorization())), auth);
    // No reintroduction or double removal.
    assert_eq!(
        build_kind(
            base()
                .auth_remove(AuthTarget::query("token").expect("target"))
                .auth_remove(AuthTarget::query("token").expect("target"))
        ),
        auth
    );
    assert_eq!(
        build_kind(base().auth_off().auth_remove(AuthTarget::authorization())),
        auth
    );
    assert_eq!(build_kind(base().auth_off().auth_off()), auth);
    assert!(AuthTarget::header("Authorization").is_err());
    assert!(AuthTarget::query("").is_err());
}

// ---------------------------------------------------------------------------
// V3-R11: nothing bypasses bind.
// ---------------------------------------------------------------------------

#[test]
fn accepted_statuses_cannot_overlap_an_effective_challenge() {
    let client = auth_scope_client();
    let challenged = client.challenged();
    let endpoint = |codes: &[u16]| {
        empty(
            RuntimeEndpoint::builder(runtime_id("runtime.overlap"), Method::GET).status_codes(
                codes
                    .iter()
                    .map(|code| StatusCode::from_u16(*code).expect("status")),
            ),
        )
    };
    for error in [
        challenged
            .plan_runtime(&endpoint(&[200, 401]))
            .expect_err("overlap plan"),
        challenged
            .bind(endpoint(&[200, 401]))
            .expect_err("overlap bind"),
    ] {
        assert_eq!(
            error.configuration_kind(),
            Some(ConfigurationErrorKind::InvalidAuthentication)
        );
    }
    challenged
        .bind(endpoint(&[200, 403]))
        .expect("no overlap with an unauthorized-only challenge");
    // Removing the challenging placement removes the overlap.
    challenged
        .bind(empty(
            RuntimeEndpoint::builder(runtime_id("runtime.overlap"), Method::GET)
                .auth_remove(AuthTarget::authorization())
                .status_codes([StatusCode::UNAUTHORIZED]),
        ))
        .expect("no effective challenge remains");
    // The never-challenging scope accepts 401 explicitly.
    client
        .protected()
        .bind(endpoint(&[401]))
        .expect("never-challenge placements do not overlap");
}

#[test]
fn generated_endpoint_id_collisions_cannot_bind() {
    let client = RuntimeScopeApi::builder()
        .set_tenant("tenant-a")
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(LocalRateLimiter::new())))
        .build()
        .expect("runtime scope client");
    let colliding = || {
        empty(RuntimeEndpoint::builder(
            runtime_id("runtime.scope.declared"),
            Method::GET,
        ))
    };
    for error in [
        client
            .regional("west")
            .bind(colliding())
            .expect_err("scope collision"),
        client.bind(colliding()).expect_err("root collision"),
        client
            .regional("west")
            .plan_runtime(&colliding())
            .expect_err("plan collision"),
    ] {
        assert_eq!(
            error.configuration_kind(),
            Some(ConfigurationErrorKind::InvalidValue)
        );
    }
    client
        .regional("west")
        .bind(empty(RuntimeEndpoint::builder(
            runtime_id("runtime.scope.dynamic"),
            Method::GET,
        )))
        .expect("distinct runtime ID binds");
}
