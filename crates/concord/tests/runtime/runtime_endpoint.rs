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
    Stream as StreamSpec, StreamBody,
};
use concord::codec::{CodecError, DecodeBody, DecodeContext, EncodeBody, EncodeContext, MediaType};
use concord::endpoint::{EndpointId, RuntimeEndpoint, RuntimeEndpointBuilder};
use concord::error::{ConfigurationErrorKind, ErrorCategory, HookError};
use concord::hooks::{BeforeSend, BoxFuture, RuntimeHooks};
use concord::http::{HeaderName, Method, SafeProxy};
use concord::rate_limit::{
    NoopRateLimiter, RateLimitError, RateLimitId, RateLimitKey, RateLimitKeyPart, RateLimitPermit,
    RateLimitRequest, RateLimitRule, RateLimitWindow, RateLimiter,
};
use concord::response::ByteStream;
use concord::{Bytes, NoContent};
use concord_test_support::server::{ResponseScript, ScriptedServer};
use futures_core::Stream;

fn runtime_id(value: &str) -> EndpointId {
    EndpointId::try_from(value).expect("valid runtime endpoint ID")
}

api! {
    client PreparedAlphaApi {
        base "http://alpha.prepared.example.test/root/";
    }

    GET explicit() -> Bytes { path ["explicit"]; }
}

api! {
    client PreparedBetaApi {
        base "http://beta.prepared.example.test/other/";
    }

    GET explicit() -> Bytes { path ["explicit"]; }
}

api! {
    client PreparedAuthApi {
        base "http://auth.prepared.example.test/";
        credentials {
            secret token: String;
            credential api_key = api_key(secret.token);
        }
    }

    GET explicit() -> NoContent;
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
            Err(CodecError::new("prepared media mismatch"))
        }
    }

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        String::from_utf8(body.to_vec()).map_err(|_| CodecError::new("prepared utf8"))
    }
}

struct VendorMedia;

impl MediaType for VendorMedia {
    const MEDIA_TYPE: &'static str = "application/x-concord-prepared-stream";
}

struct FreshFactory {
    calls: Arc<AtomicUsize>,
}

impl BodyFactory for FreshFactory {
    fn create(&self) -> Result<StreamBody, BodyError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(StreamBody::from_bytes(Bytes::from_static(b"factory-body")))
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

type CapturedKey = Vec<(String, String)>;

#[derive(Debug, PartialEq, Eq)]
struct CapturedRule {
    id: String,
    parts: CapturedKey,
    cost: u32,
    windows: Vec<(u32, Duration)>,
}

type CapturedPlan = Vec<CapturedRule>;

impl RateLimiter for CapturingLimiter {
    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        let plan = request
            .plan
            .rules()
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

impl RuntimeHooks for RejectingHooks {
    fn before_send<'a>(&'a self, _context: BeforeSend<'a>) -> BoxFuture<'a, Result<(), HookError>> {
        Box::pin(async { Err(HookError::new("prepared hook veto")) })
    }
}

struct CountingHooks {
    calls: Arc<AtomicUsize>,
}

impl RuntimeHooks for CountingHooks {
    fn before_send<'a>(&'a self, _context: BeforeSend<'a>) -> BoxFuture<'a, Result<(), HookError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(()) })
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

fn rule(id: &str, key: RateLimitKey, cost: u32, windows: &[(u32, u64)]) -> RateLimitRule {
    RateLimitRule::new(
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
    let default: RuntimeEndpointBuilder<NoContent> =
        RuntimeEndpoint::builder(runtime_id("runtime.default"), Method::GET);
    let buffered: RuntimeEndpointBuilder<EchoCodec> = default.response::<EchoCodec>();
    let bytes: RuntimeEndpointBuilder<Bytes> = buffered.response_bytes();
    let stream: RuntimeEndpointBuilder<StreamSpec<VendorMedia>> =
        bytes.response_stream::<VendorMedia>();
    let no_content: RuntimeEndpointBuilder<NoContent> = stream.response_no_content();
    let _: RuntimeEndpoint<NoContent> = no_content.build().expect("exact endpoint");
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
            .with_header("content-type", "application/x-concord-prepared-stream"),
    ])
    .expect("prepared mode server");
    let client = alpha(&server);

    let buffered = RuntimeEndpoint::builder(runtime_id("runtime.buffered"), Method::POST)
        .path_segment("items")
        .path_segment("slash/value")
        .path_segment("..")
        .path_segment("tail?query#fragment%")
        .query("first", 1)
        .query_optional("omitted", Option::<u8>::None)
        .query_values("repeat", [2, 3])
        .header(HeaderName::from_static("x-prepared-endpoint"), "new")
        .body_codec::<EchoCodec>("encoded".to_owned())
        .response::<EchoCodec>()
        .build()
        .expect("buffered runtime endpoint");
    assert_eq!(
        client.request(buffered).await.expect("buffered response"),
        "decoded"
    );

    let bytes = RuntimeEndpoint::builder(runtime_id("runtime.bytes"), Method::GET)
        .path_segment("bytes")
        .response_bytes()
        .build()
        .expect("bytes runtime endpoint");
    assert_eq!(
        client.request(bytes).await.expect("bytes response"),
        b"bytes"[..]
    );

    let no_content = RuntimeEndpoint::builder(runtime_id("runtime.none"), Method::DELETE)
        .path_segment("none")
        .build()
        .expect("no-content runtime endpoint");
    client
        .request(no_content)
        .await
        .expect("no-content response");

    let stream = RuntimeEndpoint::builder(runtime_id("runtime.stream"), Method::GET)
        .path_segment("stream")
        .response_stream::<VendorMedia>()
        .build()
        .expect("stream runtime endpoint");
    let mut response: ByteStream = client.request(stream).await.expect("stream response");
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
        "/root/items/slash%2Fvalue/%252E%252E/tail%3Fquery%23fragment%25?first=1&repeat=2&repeat=3"
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
            runtime.hooks(Arc::new(CountingHooks {
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
            runtime.hooks(Arc::new(CountingHooks {
                calls: beta_selected,
            }))
        })
        .build()
        .expect("runtime-distinct beta client");
    let definition = || {
        RuntimeEndpoint::builder(runtime_id("runtime.portable"), Method::GET)
            .path_segment("portable")
            .response_bytes()
            .build()
            .expect("portable endpoint")
    };

    assert_eq!(
        alpha.request(definition()).await.expect("alpha"),
        b"alpha"[..]
    );
    assert_eq!(alpha_calls.load(Ordering::SeqCst), 1);
    assert_eq!(beta_calls.load(Ordering::SeqCst), 0);
    assert_eq!(beta.request(definition()).await.expect("beta"), b"beta"[..]);
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
            .body(RequestBody::stream(StreamBody::from_bytes(
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
        client.request(endpoint).await.expect("prepared body send");
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
async fn hook_rejection_prevents_prepared_factory_and_native_work() {
    let server = ScriptedServer::start([]).expect("hook veto server");
    let calls = Arc::new(AtomicUsize::new(0));
    let client = PreparedAlphaApi::builder()
        .configure_http({
            let proxy = proxy(&server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime(|runtime| runtime.hooks(Arc::new(RejectingHooks)))
        .build()
        .expect("hooked client");
    let endpoint = RuntimeEndpoint::builder(runtime_id("runtime.veto"), Method::POST)
        .body(RequestBody::factory(FreshFactory {
            calls: Arc::clone(&calls),
        }))
        .build()
        .expect("lazy factory endpoint");

    let error = client
        .request(endpoint)
        .await
        .expect_err("hook must reject");
    assert_eq!(error.category(), ErrorCategory::Hook);
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    let encode_calls = Arc::new(AtomicUsize::new(0));
    let endpoint = RuntimeEndpoint::builder(runtime_id("runtime.codec_veto"), Method::POST)
        .body_codec::<CountingEncodeCodec>(Arc::clone(&encode_calls))
        .build()
        .expect("lazy codec endpoint");
    let error = client
        .request(endpoint)
        .await
        .expect_err("hook must reject codec attempt");
    assert_eq!(error.category(), ErrorCategory::Hook);
    assert_eq!(encode_calls.load(Ordering::SeqCst), 0);
    assert_eq!(server.request_count(), 0);
}

#[tokio::test]
async fn prepared_protected_http_fails_before_hooks_or_body_materialization() {
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
            runtime.hooks(Arc::new(CountingHooks {
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

    let error = client
        .request(endpoint)
        .await
        .expect_err("authenticated cleartext must fail");
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(hook_calls.load(Ordering::SeqCst), 0);
    assert_eq!(factory_calls.load(Ordering::SeqCst), 0);
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
        let error = request_client
            .request(endpoint)
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
        .response_bytes()
        .build()
        .expect("limited response endpoint");
    let error = response_client
        .request(endpoint)
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
                RateLimitKey::from_parts([
                    RateLimitKeyPart::new("tenant", "west").expect("tenant key part")
                ])
                .expect("one-part rate key"),
                2,
                &[(20, 2)],
            ))
            .rate_limit(rule(
                "several-parts",
                RateLimitKey::from_parts([
                    RateLimitKeyPart::new("tenant", "east").expect("tenant key part"),
                    RateLimitKeyPart::new("region", "eu").expect("region key part"),
                ])
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
    explicit_noop
        .request(endpoint())
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
    custom
        .request(endpoint())
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
                parts: vec![("tenant".to_owned(), "west".to_owned())],
                cost: 2,
                windows: vec![(20, Duration::from_secs(2))],
            },
            CapturedRule {
                id: "several-parts".to_owned(),
                parts: vec![
                    ("tenant".to_owned(), "east".to_owned()),
                    ("region".to_owned(), "eu".to_owned()),
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

    RuntimeEndpoint::builder(runtime_id("runtime.auth"), Method::GET)
        .auth(
            Auth::header(HeaderName::from_static("x-first"), credential.clone())
                .expect("valid header placement"),
        )
        .auth(Auth::query("second", credential).expect("valid query placement"))
        .build()
        .expect("compatible ordered placements");
}
