#![cfg(feature = "multipart")]

use std::future::poll_fn;
use std::num::{NonZeroU32, NonZeroUsize};
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use concord::api;
use concord::auth::{Auth, ChallengePolicy};
use concord::body::{
    BodyError, BodyFactory, MultipartBody, MultipartFactory, MultipartPart, RequestBody,
    RequestStream,
};
use concord::codec::{
    CodecError, CodecErrorKind, DecodeBody, DecodeContext, EncodeBody, EncodeContext,
};
use concord::endpoint::{EndpointId, RuntimeEndpoint, RuntimeEndpointBuilder, RuntimeValue};
use concord::error::{ConfigurationErrorKind, ErrorCategory};
use concord::http::{HeaderName, Method, SafeProxy};
use concord::observation::{OperationEvent, OperationObserver};
use concord::rate_limit::{
    BoxFuture, NoopRateLimiter, RateLimitError, RateLimitId, RateLimitKey, RateLimitPermit,
    RateLimitPolicy, RateLimitRequest, RateLimitWindow, RateLimiter,
};
use concord::response::ByteStream;
use concord::{ApiResponse, Binary, Bytes, Discard, Empty};
use concord_test_support::server::{ResponseScript, ScriptedServer};
use futures_core::Stream;

fn runtime_id(value: &str) -> EndpointId {
    EndpointId::try_from(value).expect("valid runtime endpoint ID")
}

api! {
    client PreparedAlphaApi {
        timeout 30s;
        base "http://alpha.prepared.example.test/root/";


    GET explicit() -> Bytes via Binary { path ["explicit"]; }
}}

api! {
    client PreparedBetaApi {
        timeout 30s;
        base "http://beta.prepared.example.test/other/";


    GET explicit() -> Bytes via Binary { path ["explicit"]; }
}}

api! {
    client PreparedAuthApi {
        timeout 30s;
        base "http://auth.prepared.example.test/";
        credentials {
            secret token;
            credential api_key = api_key(secret.token);
        }


    GET explicit() -> () via Empty;
}}

api! {
    client RuntimeScopeApi {
        timeout 30s;
        base "http://scope-runtime.example.test/root/";
        values { tenant: String; }
        rate_limits {
            policy tenant_reads(tenant) {
                id "runtime.scope.tenant";
                key [endpoint, tenant];
                cost 1;
                allow 10 per 1m;
            }
        }
        path ["v2"];
        query { "tenant" = client.tenant; }

        scope regional(region: String) {
            timeout 5s;
            subdomain ["api"];
            path ["regions", regional.region];
            query { append "view" = "scope"; }
            headers { "X-Scope" = regional.region; }
            rate_limit tenant_reads(tenant = client.tenant);

            GET declared() -> () via Empty;
        }
    }
}

api! {
    client RuntimeAuthScopeApi {
        timeout 30s;
        base "https://runtime-auth-scope.example.test/";
        credentials {
            secret token;
            credential api_key = api_key(secret.token);
        }

        scope protected() {
            auth { header "X-Runtime-Scope-Key" = api_key challenge never; }
            GET declared() -> () via Empty;
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

fn rule(id: &str, key: RateLimitKey, cost: u32, windows: &[(u32, u64)]) -> RateLimitPolicy {
    RateLimitPolicy::new(
        RateLimitId::new(id).expect("rate ID"),
        key,
        NonZeroU32::new(cost).expect("rate cost"),
        windows.iter().map(|(capacity, seconds)| {
            RateLimitWindow::new(
                NonZeroU32::new(*capacity).expect("rate capacity"),
                Duration::from_secs(*seconds),
            )
            .expect("rate window")
        }),
    )
    .expect("rate rule")
}

fn exact_builder_contracts() {
    let default: RuntimeEndpointBuilder<Empty> =
        RuntimeEndpoint::builder(runtime_id("runtime.default"), Method::GET);
    let buffered: RuntimeEndpointBuilder<EchoCodec, String> =
        default.response::<EchoCodec, String>();
    let bytes: RuntimeEndpointBuilder<Binary, Bytes> = buffered.accepted_bytes();
    let stream: RuntimeEndpointBuilder<Binary, ByteStream> = bytes.response::<Binary, ByteStream>();
    let empty: RuntimeEndpointBuilder<Empty> = stream.response_empty();
    let discard: RuntimeEndpointBuilder<Discard> = empty.response_discard();
    let _: RuntimeEndpoint<Discard> = discard.build().expect("exact endpoint");
}

#[test]
fn exact_runtime_endpoint_types_are_available_from_endpoint_domain() {
    exact_builder_contracts();
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
        .query("first", 1)
        .query_optional("omitted", Option::<u8>::None)
        .query_each("repeat", [2, 3])
        .header(HeaderName::from_static("x-prepared-endpoint"), "new")
        .body_codec::<EchoCodec>("encoded".to_owned())
        .response::<EchoCodec, String>()
        .build()
        .expect("buffered runtime endpoint");
    assert_eq!(
        buffered
            .try_bind(&client)
            .expect("runtime endpoint binds to client")
            .await
            .expect("buffered response"),
        "decoded"
    );

    let bytes = RuntimeEndpoint::builder(runtime_id("runtime.bytes"), Method::GET)
        .path("bytes")
        .accepted_bytes()
        .build()
        .expect("bytes runtime endpoint");
    assert_eq!(
        bytes
            .try_bind(&client)
            .expect("runtime endpoint binds to client")
            .await
            .expect("bytes response"),
        b"bytes"[..]
    );

    let no_content = RuntimeEndpoint::builder(runtime_id("runtime.none"), Method::DELETE)
        .path("none")
        .response_discard()
        .build()
        .expect("no-content runtime endpoint");
    no_content
        .try_bind(&client)
        .expect("runtime endpoint binds to client")
        .await
        .expect("no-content response");

    let stream = RuntimeEndpoint::builder(runtime_id("runtime.stream"), Method::GET)
        .path("stream")
        .response::<Binary, ByteStream>()
        .build()
        .expect("stream runtime endpoint");
    let mut response: ByteStream = stream
        .try_bind(&client)
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
    let server = ScriptedServer::start([ResponseScript::ok(b"runtime-binary".to_vec())
        .with_header("content-type", "application/octet-stream")])
    .expect("runtime binary server");
    let client = alpha(&server);
    let endpoint = RuntimeEndpoint::builder(runtime_id("runtime.accepted_bytes"), Method::GET)
        .path("binary")
        .accepted_bytes()
        .build()
        .expect("runtime binary endpoint");

    let response: ApiResponse<Bytes> = endpoint
        .try_bind(&client)
        .expect("runtime endpoint binds to client")
        .response()
        .await
        .expect("runtime endpoint binary response");
    assert_eq!(response.value(), b"runtime-binary".as_slice());
    assert_eq!(response.status(), concord::http::StatusCode::OK);
    assert_eq!(response.metadata().endpoint(), "runtime.accepted_bytes");
    assert_eq!(
        server.requests()[0].header("accept"),
        Some("application/octet-stream")
    );
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
    let endpoint = RuntimeEndpoint::builder(runtime_id("runtime.sensitive"), Method::GET)
        .path_value(RuntimeValue::sensitive(PATH_SENTINEL))
        .query_value("secret", RuntimeValue::sensitive(QUERY_SENTINEL))
        .query_optional_value("optional", RuntimeValue::sensitive(Some(OPTIONAL_SENTINEL)))
        .query_each_values(
            "each",
            RuntimeValue::sensitive(vec![EACH_SENTINEL, EACH_SENTINEL]),
        )
        .header_value(
            HeaderName::from_static("x-runtime-sensitive"),
            RuntimeValue::sensitive(HEADER_SENTINEL),
        )
        .header_each_values(
            HeaderName::from_static("x-runtime-sensitive-each"),
            RuntimeValue::sensitive(vec![HEADER_EACH_SENTINEL, HEADER_EACH_SENTINEL]),
        )
        .build()
        .expect("runtime sensitive endpoint");

    let plan = endpoint.plan(&client).expect("runtime sensitive plan");
    let evidence = format!(
        "{plan:?}\n{}\n{:?}",
        plan.observed_url(),
        plan.public_headers()
    );
    for sentinel in [
        PATH_SENTINEL,
        QUERY_SENTINEL,
        OPTIONAL_SENTINEL,
        EACH_SENTINEL,
        HEADER_SENTINEL,
        HEADER_EACH_SENTINEL,
    ] {
        assert!(!evidence.contains(sentinel));
    }
    endpoint
        .try_bind(&client)
        .expect("runtime sensitive endpoint binds")
        .await
        .expect("runtime sensitive endpoint executes");

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
        .set_tenant("tenant-a".to_owned())
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.rate_limiter(configured_limiter))
        .build()
        .expect("runtime scope client");
    let scope = client.regional("west".to_owned());

    let inherited = RuntimeEndpoint::builder(runtime_id("runtime.scope.inherited"), Method::GET)
        .path("items")
        .build()
        .expect("inherited runtime endpoint");
    let plan = inherited.plan(&scope).expect("scope-owned runtime plan");
    assert_eq!(plan.resolved_timeout(), Some(Duration::from_secs(5)));
    assert_eq!(plan.rate_limit_policy_count(), 1);
    assert!(plan.capabilities().rate_limits());
    inherited
        .try_bind(&scope)
        .expect("runtime endpoint binds to scope")
        .await
        .expect("scope-owned runtime request");

    let replaced = RuntimeEndpoint::builder(runtime_id("runtime.scope.replaced"), Method::GET)
        .replace_path(["replacement"])
        .query("tenant", "runtime")
        .headers_off()
        .build()
        .expect("replaced runtime endpoint");
    replaced
        .try_bind(&scope)
        .expect("patched runtime endpoint binds to scope")
        .await
        .expect("patched scope-owned runtime request");

    let requests = server.requests();
    assert!(requests[0].target().contains(
        "api.scope-runtime.example.test/root/v2/regions/west/items?tenant=tenant-a&view=scope"
    ));
    assert_eq!(requests[0].header("x-scope"), Some("west"));
    assert!(
        requests[1]
            .target()
            .contains("api.scope-runtime.example.test/root/replacement?view=scope&tenant=runtime")
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
}

#[test]
fn runtime_scope_auth_is_inherited_and_can_be_explicitly_disabled() {
    let client = RuntimeAuthScopeApi::builder()
        .token(
            concord::prelude::SecretString::try_new("scope-secret").expect("bounded scope secret"),
        )
        .build()
        .expect("runtime scope auth client");
    let scope = client.protected();

    let inherited =
        RuntimeEndpoint::builder(runtime_id("runtime.scope.auth.description"), Method::GET)
            .build()
            .expect("auth-inheriting runtime endpoint");
    let inherited_plan = inherited.plan(&scope).expect("inherited auth plan");
    assert!(inherited_plan.capabilities().authentication());

    let collision =
        RuntimeEndpoint::builder(runtime_id("runtime.scope.auth.inherited"), Method::GET)
            .header(HeaderName::from_static("x-runtime-scope-key"), "public")
            .build()
            .expect("auth-inheriting runtime endpoint")
            .try_bind(&scope)
            .err()
            .expect("inherited private placement collides with public header");
    assert_eq!(
        collision.configuration_kind(),
        Some(ConfigurationErrorKind::InvalidAuthentication)
    );
    let auth_free = RuntimeEndpoint::builder(runtime_id("runtime.scope.auth.off"), Method::GET)
        .auth_off()
        .header(HeaderName::from_static("x-runtime-scope-key"), "public")
        .build()
        .expect("auth-free runtime endpoint");
    let auth_free_plan = auth_free.plan(&scope).expect("auth-free runtime plan");
    assert!(!auth_free_plan.capabilities().authentication());
    let _request = auth_free
        .try_bind(&scope)
        .expect("auth-free runtime endpoint binds");
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
            .accepted_bytes()
            .build()
            .expect("portable endpoint")
    };

    assert_eq!(
        definition()
            .try_bind(&alpha)
            .expect("runtime endpoint binds to alpha")
            .await
            .expect("alpha"),
        b"alpha"[..]
    );
    assert_eq!(alpha_calls.load(Ordering::SeqCst), 1);
    assert_eq!(beta_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        definition()
            .try_bind(&beta)
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

#[tokio::test]
async fn all_prepared_body_forms_use_the_existing_lazy_body_architecture() {
    let server = ScriptedServer::start((0..7).map(|_| ResponseScript::ok(Vec::new())))
        .expect("prepared body server");
    let client = alpha(&server);
    let factory_calls = Arc::new(AtomicUsize::new(0));
    let multipart_factory_calls = Arc::new(AtomicUsize::new(0));

    let bodies = [
        RuntimeEndpoint::builder(runtime_id("runtime.empty"), Method::POST)
            .body(RequestBody::empty())
            .build()
            .expect("empty"),
        RuntimeEndpoint::builder(runtime_id("runtime.bytes"), Method::POST)
            .body(RequestBody::bytes(Bytes::from_static(b"bytes-body")))
            .build()
            .expect("bytes"),
        RuntimeEndpoint::builder(runtime_id("runtime.stream"), Method::POST)
            .body(RequestBody::stream(RequestStream::from_bytes(
                Bytes::from_static(b"stream-body"),
            )))
            .build()
            .expect("stream"),
        RuntimeEndpoint::builder(runtime_id("runtime.factory"), Method::POST)
            .body(RequestBody::factory(FreshFactory {
                calls: Arc::clone(&factory_calls),
            }))
            .build()
            .expect("factory"),
        RuntimeEndpoint::builder(runtime_id("runtime.codec"), Method::POST)
            .body_codec::<EchoCodec>("codec-body".to_owned())
            .build()
            .expect("codec"),
        RuntimeEndpoint::builder(runtime_id("runtime.multipart"), Method::POST)
            .body(RequestBody::multipart(
                MultipartBody::new()
                    .part(
                        "field",
                        MultipartPart::text("multipart-body").expect("bounded multipart text"),
                    )
                    .expect("multipart part"),
            ))
            .build()
            .expect("multipart"),
        RuntimeEndpoint::builder(runtime_id("runtime.multipart_factory"), Method::POST)
            .body(RequestBody::multipart_factory(FreshMultipartFactory {
                calls: Arc::clone(&multipart_factory_calls),
            }))
            .build()
            .expect("multipart factory"),
    ];

    assert_eq!(factory_calls.load(Ordering::SeqCst), 0);
    assert_eq!(multipart_factory_calls.load(Ordering::SeqCst), 0);
    for endpoint in bodies {
        endpoint
            .try_bind(&client)
            .expect("runtime endpoint binds to client")
            .await
            .expect("prepared body send");
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
    let endpoint = RuntimeEndpoint::builder(runtime_id("runtime.observed"), Method::POST)
        .body(RequestBody::factory(FreshFactory {
            calls: Arc::clone(&calls),
        }))
        .build()
        .expect("lazy factory endpoint");

    endpoint
        .try_bind(&client)
        .expect("runtime endpoint binds to client")
        .await
        .expect("observer has no veto authority");
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let encode_calls = Arc::new(AtomicUsize::new(0));
    let endpoint = RuntimeEndpoint::builder(runtime_id("runtime.codec_observed"), Method::POST)
        .body_codec::<CountingEncodeCodec>(Arc::clone(&encode_calls))
        .build()
        .expect("lazy codec endpoint");
    endpoint
        .try_bind(&client)
        .expect("runtime endpoint binds to client")
        .await
        .expect("observer has no codec veto authority");
    assert_eq!(encode_calls.load(Ordering::SeqCst), 1);
    assert_eq!(server.request_count(), 2);
}

#[tokio::test]
async fn prepared_protected_http_fails_before_observer_or_body_materialization() {
    let server = ScriptedServer::start([]).expect("protected preparation server");
    let hook_calls = Arc::new(AtomicUsize::new(0));
    let selected_hooks = Arc::clone(&hook_calls);
    let client = PreparedAuthApi::builder()
        .token(
            concord::prelude::SecretString::try_new("prepared-credential-15e8d20c")
                .expect("bounded prepared credential"),
        )
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
    let endpoint = RuntimeEndpoint::builder(runtime_id("runtime.protected_http"), Method::POST)
        .body(RequestBody::factory(FreshFactory {
            calls: Arc::clone(&factory_calls),
        }))
        .auth(
            Auth::header(
                HeaderName::from_static("x-api-key"),
                client.credentials().api_key().credential(),
            )
            .expect("valid header placement"),
        )
        .build()
        .expect("structurally valid protected endpoint");

    let error = endpoint
        .try_bind(&client)
        .err()
        .expect("authenticated cleartext must fail");
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(hook_calls.load(Ordering::SeqCst), 0);
    assert_eq!(factory_calls.load(Ordering::SeqCst), 0);
    assert_eq!(server.request_count(), 0);

    let custom_factory_calls = Arc::new(AtomicUsize::new(0));
    let endpoint = RuntimeEndpoint::builder(
        runtime_id("runtime.custom_authorization_http"),
        Method::POST,
    )
    .body(RequestBody::factory(FreshFactory {
        calls: Arc::clone(&custom_factory_calls),
    }))
    .auth(
        Auth::authorization_with_scheme("Token", client.credentials().api_key().credential())
            .expect("valid static Authorization scheme"),
    )
    .build()
    .expect("structurally valid custom Authorization endpoint");
    let error = endpoint
        .try_bind(&client)
        .err()
        .expect("custom Authorization over cleartext must fail");
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(hook_calls.load(Ordering::SeqCst), 0);
    assert_eq!(custom_factory_calls.load(Ordering::SeqCst), 0);
    assert_eq!(server.request_count(), 0);
}

#[tokio::test]
async fn prepared_bodies_and_responses_cannot_bypass_runtime_limits() {
    let request_server = ScriptedServer::start([]).expect("prepared request-limit server");
    let request_client = PreparedAlphaApi::builder()
        .configure_http({
            let proxy = proxy(&request_server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime(|runtime| {
            runtime.request_body_limit(NonZeroUsize::new(3).expect("positive request limit"))
        })
        .build()
        .expect("request-limited client");
    for endpoint in [
        RuntimeEndpoint::builder(runtime_id("runtime.limited_bytes"), Method::POST)
            .body(RequestBody::bytes(Bytes::from_static(b"four")))
            .build()
            .expect("limited bytes endpoint"),
        RuntimeEndpoint::builder(runtime_id("runtime.limited_codec"), Method::POST)
            .body_codec::<EchoCodec>("four".to_owned())
            .build()
            .expect("limited codec endpoint"),
    ] {
        let error = endpoint
            .try_bind(&request_client)
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
        .configure_runtime(|runtime| {
            runtime.response_body_limit(NonZeroUsize::new(3).expect("positive response limit"))
        })
        .build()
        .expect("response-limited client");
    let endpoint = RuntimeEndpoint::builder(runtime_id("runtime.limited_response"), Method::GET)
        .accepted_bytes()
        .build()
        .expect("limited response endpoint");
    let error = endpoint
        .try_bind(&response_client)
        .expect("runtime endpoint binds to client")
        .await
        .expect_err("prepared response body limit");
    assert_eq!(error.category(), ErrorCategory::ResponseBody);
    assert_eq!(response_server.request_count(), 1);
}

#[tokio::test]
async fn prepared_custom_limiter_inspects_ids_keys_costs_and_windows_in_call_order() {
    let server = ScriptedServer::start([
        ResponseScript::ok(Vec::new()),
        ResponseScript::ok(Vec::new()),
    ])
    .expect("prepared limiter server");
    let endpoint = || {
        RuntimeEndpoint::builder(runtime_id("runtime.limited"), Method::GET)
            .rate_limit(rule(
                "global",
                RateLimitKey::global(),
                1,
                &[(10, 1), (100, 10)],
            ))
            .rate_limit(rule(
                "one-part",
                RateLimitKey::from_canonical_values([("tenant", "west")])
                    .expect("one-part rate key"),
                2,
                &[(20, 2)],
            ))
            .rate_limit(rule(
                "several-parts",
                RateLimitKey::from_canonical_values([("tenant", "east"), ("region", "eu")])
                    .expect("several-parts rate key"),
                3,
                &[(30, 3), (300, 30)],
            ))
            .build()
            .expect("prepared rate plan")
    };

    let explicit_noop = PreparedAlphaApi::builder()
        .configure_http({
            let proxy = proxy(&server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(NoopRateLimiter)))
        .build()
        .expect("explicit no-op client");
    endpoint()
        .try_bind(&explicit_noop)
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
    endpoint()
        .try_bind(&custom)
        .expect("runtime endpoint binds to client")
        .await
        .expect("custom limiter authorizes plan");
    assert_eq!(
        limiter.plans.lock().expect("captured plans").as_slice(),
        [vec![
            CapturedRule {
                id: "global".to_owned(),
                parts: vec![],
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
fn prepared_authentication_uses_canonical_collision_and_challenge_validation() {
    let client = PreparedAuthApi::builder()
        .token(
            concord::prelude::SecretString::try_new("credential-sentinel")
                .expect("bounded prepared credential"),
        )
        .build()
        .expect("auth client");
    let credential = client.credentials().api_key().credential();

    let header_collision = RuntimeEndpoint::builder(runtime_id("runtime.auth"), Method::GET)
        .header(HeaderName::from_static("x-api-key"), "public")
        .auth(
            Auth::header(HeaderName::from_static("x-api-key"), credential.clone())
                .expect("valid header placement"),
        )
        .build()
        .err()
        .expect("public/auth header collision");
    assert_eq!(
        header_collision.kind(),
        ConfigurationErrorKind::InvalidAuthentication
    );

    let query_collision = RuntimeEndpoint::builder(runtime_id("runtime.auth"), Method::GET)
        .query("token", "public")
        .auth(Auth::query("token", credential.clone()).expect("valid query placement"))
        .build()
        .err()
        .expect("public/auth query collision");
    assert_eq!(
        query_collision.kind(),
        ConfigurationErrorKind::InvalidAuthentication
    );

    let two_challenges = RuntimeEndpoint::builder(runtime_id("runtime.auth"), Method::GET)
        .auth(
            Auth::header(HeaderName::from_static("x-first"), credential.clone())
                .expect("valid header placement")
                .challenge(ChallengePolicy::Unauthorized),
        )
        .auth(
            Auth::query("second", credential.clone())
                .expect("valid query placement")
                .challenge(ChallengePolicy::UnauthorizedOrForbidden),
        )
        .build()
        .err()
        .expect("two challenge placements");
    assert_eq!(
        two_challenges.kind(),
        ConfigurationErrorKind::InvalidAuthentication
    );

    let authorization_collision = RuntimeEndpoint::builder(runtime_id("runtime.auth"), Method::GET)
        .auth(Auth::authorization(credential.clone()))
        .auth(
            Auth::authorization_with_scheme("Token", credential.clone())
                .expect("valid static Authorization scheme"),
        )
        .build()
        .err()
        .expect("two custom Authorization owners");
    assert_eq!(
        authorization_collision.kind(),
        ConfigurationErrorKind::InvalidAuthentication
    );

    let bearer_authorization_collision =
        RuntimeEndpoint::builder(runtime_id("runtime.auth"), Method::GET)
            .auth(Auth::bearer(credential.clone()))
            .auth(Auth::authorization(credential.clone()))
            .build()
            .err()
            .expect("Bearer and exact Authorization collide");
    assert_eq!(
        bearer_authorization_collision.kind(),
        ConfigurationErrorKind::InvalidAuthentication
    );

    RuntimeEndpoint::builder(runtime_id("runtime.auth"), Method::GET)
        .auth(
            Auth::header(HeaderName::from_static("x-first"), credential.clone())
                .expect("valid header placement"),
        )
        .auth(Auth::query("second", credential).expect("valid query placement"))
        .build()
        .expect("compatible ordered placements");
}
