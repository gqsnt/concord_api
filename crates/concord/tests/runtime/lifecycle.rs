//! Generated construction, configuration, representations, streams and
//! multipart through one managed runtime path; lazy request construction and
//! hygienic, isolated generated paths.

#![cfg(feature = "multipart")]

use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use concord::api;
use concord::body::{Multipart, MultipartBody, MultipartPart, PreparedBody, RequestStream};
use concord::codec::{
    CodecError, CodecErrorKind, DecodeBody, DecodeContext, EncodeBody, EncodeContext,
};
use concord::endpoint::ResponseRequest;
use concord::error::ClientBuildErrorKind;
use concord::http::{SafeProxy, StatusCode};
use concord::observation::{CompletionOutcome, DebugLevel, OperationEvent, OperationObserver};
use concord::response::{ApiResponse, ByteStream};
use concord::{Binary, Bytes, ClientBuildError, Discard, Empty, Text};
use concord_test_support::server::{ResponseScript, ScriptedServer};
use futures_core::Stream as FuturesStream;

api! {
    client ExampleApi {
        timeout 30s;
        base "http://generated.example.test/v1/";

        scope health() {
            path "health";
            GET bytes() -> Bytes via Binary;
            GET ping() -> () via Discard;
            GET item(id: String) -> Bytes via Binary {
                path "items/{id}";
            }
        }
    }
}

api! {
    client StreamingApi {
        timeout 30s;
        base "http://streaming.generated.example.test/v1/";

        scope transfers() {
            path "transfers";
            POST upload(body body: RequestStream via Binary) -> Bytes via Binary;
            POST vendor_upload(body body: RequestStream via Binary) -> Bytes via Binary;
            GET download() -> ByteStream via Binary;
            GET vendor_download() -> ByteStream via Binary;
            POST multipart_upload(body body: MultipartBody via Multipart) -> Bytes via Binary;
        }
    }
}

pub struct WireCodec;

impl EncodeBody for WireCodec {
    type Input = String;

    const CONTENT_TYPE: Option<&'static str> = Some("application/x-concord-wire");

    fn encode(input: Self::Input, _context: EncodeContext<'_>) -> Result<Bytes, CodecError> {
        Ok(Bytes::from(input))
    }
}

impl DecodeBody for WireCodec {
    type Output = String;

    const ACCEPT: Option<&'static str> = Some("application/x-concord-wire");

    fn validate_content_type(content_type: Option<&str>) -> Result<(), CodecError> {
        if content_type == Some("application/x-concord-wire") {
            Ok(())
        } else {
            Err(CodecError::new(CodecErrorKind::Decode))
        }
    }

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        String::from_utf8(body.to_vec()).map_err(|error| {
            drop(error);
            CodecError::new(CodecErrorKind::Decode)
        })
    }
}

/// Test-local buffered codec deliberately named `Json` to prove that type
/// spelling is not macro semantics (REP-01).
pub struct Json;

impl EncodeBody for Json {
    type Input = Vec<u32>;

    const CONTENT_TYPE: Option<&'static str> = Some("application/json");

    fn encode(input: Self::Input, _context: EncodeContext<'_>) -> Result<Bytes, CodecError> {
        serde_json::to_vec(&input)
            .map(Bytes::from)
            .map_err(|_| CodecError::new(CodecErrorKind::Encode))
    }
}

impl DecodeBody for Json {
    type Output = Vec<u32>;

    const ACCEPT: Option<&'static str> = Some("application/json");

    fn validate_content_type(content_type: Option<&str>) -> Result<(), CodecError> {
        match content_type.and_then(|value| value.split(';').next()) {
            Some(media) if media.trim().eq_ignore_ascii_case("application/json") => Ok(()),
            _ => Err(CodecError::new(CodecErrorKind::Decode)),
        }
    }

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        serde_json::from_slice(&body).map_err(|_| CodecError::new(CodecErrorKind::Decode))
    }
}

api! {
    client BufferedApi {
        timeout 30s;
        base "http://buffered.generated.example.test/v1/";

        scope messages() {
            path "messages";
            POST text_echo(body body: String via Text) -> String via Text;
            POST json_echo(body body: Vec<u32> via Json) -> Vec<u32> via Json;
            POST custom_echo(body body: String via WireCodec) -> String via WireCodec;
            POST bytes_echo(body body: String via Text) -> Bytes via Binary;
            POST raw_bytes(body body: Bytes via Binary) -> Bytes via Binary;
            POST managed_body(body body: PreparedBody via PreparedBody) -> Bytes via Binary;
            POST no_content_echo(body body: String via Text) -> () via Discard;
        }
    }
}

/// Atom counting every encoder invocation.
pub struct CountingValue {
    text: &'static str,
    calls: Arc<AtomicUsize>,
}

impl concord::RequestAtom for CountingValue {
    fn write_atom(
        &self,
        output: &mut concord::BoundedAtomWriter<'_>,
    ) -> Result<(), concord::AtomError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        output.push_str(self.text)
    }
}

api! {
    client LazyApi {
        timeout 30s;
        base "http://lazy.generated.example.test/";

        scope resources(scope_value: CountingValue) {
            path "scopes/{scope_value}/resources";
            GET by_id(endpoint_value: CountingValue) -> Bytes via Binary {
                path "{endpoint_value}";
            }
            GET ping() -> () via Empty;
        }
    }
}

mod nested_case {
    pub mod inner {
        use concord::api;
        use concord::body::RequestStream;
        use concord::response::ByteStream;
        use concord::{Binary, Empty, Text};

        api! {
            client NestedApi {
                timeout 30s;
                base "http://nested.generated.example.test/";

                scope health() {
                    path "health";
                    GET ping() -> () via Empty;
                    POST echo(body body: String via Text) -> String via Text;
                    POST upload(body body: RequestStream via Binary) -> () via Empty;
                    GET download() -> ByteStream via Binary;
                }
            }
        }
    }
}

mod repeated_case {
    #![allow(dead_code)]

    use concord::api;
    use concord::{Binary, Empty, Text};

    struct Result;
    struct Option;
    struct String;
    struct Vec;
    struct Method;
    struct HeadBuilder;
    struct Arc;
    struct Box;
    mod core {}
    mod runtime {}
    mod std {}

    api! {
        client AlphaApi {
            timeout 30s;
            base "http://alpha.generated.example.test/";

            scope health() {
                path "health";
                GET ping() -> () via Empty;
                POST echo(
                    body body: ::std::string::String via Text,
                ) -> ::std::string::String via Text;
                GET download() -> ::concord::ByteStream via Binary;
            }
        }
    }

    api! {
        client BetaApi {
            timeout 30s;
            base "http://beta.generated.example.test/";

            scope health() {
                path "health";
                GET ping() -> () via Empty;
                POST echo(
                    body body: ::std::string::String via Text,
                ) -> ::std::string::String via Text;
                GET download() -> ::concord::ByteStream via Binary;
            }
        }
    }

    pub fn requests_are_isolated() {
        let alpha = AlphaApi::new().expect("alpha client");
        let beta = BetaApi::new().expect("beta client");
        let _: alpha_api::health::PingRequest = alpha.health().ping();
        let _: beta_api::health::PingRequest = beta.health().ping();
        let _: alpha_api::health::EchoRequest = alpha.health().echo("alpha".to_owned());
        let _: beta_api::health::EchoRequest = beta.health().echo("beta".to_owned());
        let _: alpha_api::health::DownloadRequest = alpha.health().download();
        let _: beta_api::health::DownloadRequest = beta.health().download();

        fn accepts_alpha_endpoint<E: alpha_api::GeneratedEndpoint>(_: &E) {}
        fn accepts_beta_endpoint<E: beta_api::GeneratedEndpoint>(_: &E) {}

        let alpha_endpoint = AlphaApi::unbound().health().ping();
        accepts_alpha_endpoint(&alpha_endpoint);
        let _: alpha_api::health::PingRequest = alpha.request(alpha_endpoint);

        let beta_endpoint = BetaApi::unbound().health().ping();
        accepts_beta_endpoint(&beta_endpoint);
        let _: beta_api::health::PingRequest = beta.request(beta_endpoint);
    }
}

struct CountingHooks {
    before: AtomicUsize,
    response: AtomicUsize,
    complete: AtomicUsize,
    outcomes: Mutex<Vec<CompletionOutcome>>,
}

impl CountingHooks {
    const fn new() -> Self {
        Self {
            before: AtomicUsize::new(0),
            response: AtomicUsize::new(0),
            complete: AtomicUsize::new(0),
            outcomes: Mutex::new(Vec::new()),
        }
    }

    fn outcomes(&self) -> Vec<CompletionOutcome> {
        self.outcomes.lock().expect("outcome lock").clone()
    }
}

impl OperationObserver for CountingHooks {
    fn observe(&self, event: OperationEvent<'_>) {
        match event {
            OperationEvent::AttemptStarted(_) => {
                self.before.fetch_add(1, Ordering::SeqCst);
            }
            OperationEvent::ResponseHeadObserved(_) => {
                self.response.fetch_add(1, Ordering::SeqCst);
            }
            OperationEvent::OperationFinished(context) => {
                self.complete.fetch_add(1, Ordering::SeqCst);
                self.outcomes
                    .lock()
                    .expect("outcome lock")
                    .push(context.outcome);
            }
            _ => {}
        }
    }
}

fn proxy(server: &ScriptedServer) -> SafeProxy {
    SafeProxy::parse(server.url("")).expect("local proxy origin")
}

fn proxied_example(server: &ScriptedServer, hooks: Option<Arc<CountingHooks>>) -> ExampleApi {
    let proxy = proxy(server);
    let builder = ExampleApi::builder().configure_http(move |http| http.proxy(proxy));
    let builder = match hooks {
        Some(hooks) => builder.configure_runtime(move |runtime| {
            runtime
                .operation_observer(hooks.clone())
                .operation_observer(hooks.clone())
                .operation_observer(hooks)
                .debug(DebugLevel::Headers)
        }),
        None => builder,
    };
    builder.build().expect("proxied generated client")
}

fn proxied_buffered(server: &ScriptedServer) -> BufferedApi {
    let proxy = proxy(server);
    BufferedApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("proxied buffered generated client")
}

fn proxied_streaming(server: &ScriptedServer, hooks: Arc<CountingHooks>) -> StreamingApi {
    let proxy = proxy(server);
    StreamingApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| {
            runtime
                .operation_observer(hooks.clone())
                .operation_observer(hooks.clone())
                .operation_observer(hooks)
        })
        .build()
        .expect("proxied streaming generated client")
}

async fn next_stream_chunk(response: &mut ByteStream) -> Option<Result<Bytes, concord::ApiError>> {
    std::future::poll_fn(|context| Pin::new(&mut *response).poll_next(context)).await
}

fn exact_new_shape() -> Result<(), ClientBuildError> {
    let client = ExampleApi::new()?;
    let _: example_api::health::BytesRequest = client.health().bytes();
    let _: example_api::health::PingRequest = client.health().ping();
    Ok(())
}

fn build_error(result: Result<ExampleApi, ClientBuildError>) -> ClientBuildError {
    match result {
        Ok(_) => panic!("generated build unexpectedly succeeded"),
        Err(error) => error,
    }
}

#[test]
fn new_builder_and_public_request_shapes_are_final() {
    exact_new_shape().expect("fallible no-input constructor");
    let from_builder = ExampleApi::builder().build().expect("client builder");
    let from_named_builder = example_api::ExampleApiBuilder::new()
        .build()
        .expect("named builder");
    let from_default = example_api::ExampleApiBuilder::default()
        .build()
        .expect("default builder");
    let _: example_api::health::BytesRequest = from_builder.health().bytes();
    let _: example_api::health::PingRequest = from_named_builder.health().ping();
    let _: example_api::health::ItemRequest = from_default.health().item("id");
    let _: ResponseRequest<Bytes> = from_builder.health().bytes().response();
    let _: example_api::health::BytesRequest =
        from_builder.request(ExampleApi::unbound().health().bytes());
    let _: example_api::health::BytesEndpoint = ExampleApi::unbound().health().bytes();
}

#[test]
fn repeated_configuration_closures_compose_immediately_in_call_order() {
    let http_order = AtomicUsize::new(0);
    let builder = ExampleApi::builder()
        .configure_http(|http| {
            assert_eq!(http_order.fetch_add(1, Ordering::SeqCst), 0);
            http.require_https().protocol_recovery(false)
        })
        .configure_http(|http| {
            assert_eq!(http_order.fetch_add(1, Ordering::SeqCst), 1);
            http.allow_http().protocol_recovery(true)
        });
    assert_eq!(http_order.load(Ordering::SeqCst), 2);
    builder
        .build()
        .expect("later HTTP selections replace earlier selections");

    let runtime_order = AtomicUsize::new(0);
    let builder = ExampleApi::builder()
        .configure_runtime(|runtime| {
            assert_eq!(runtime_order.fetch_add(1, Ordering::SeqCst), 0);
            runtime
                .runtime_endpoint_timeout(Duration::from_secs(11))
                .debug(DebugLevel::Basic)
        })
        .configure_runtime(|runtime| {
            assert_eq!(runtime_order.fetch_add(1, Ordering::SeqCst), 1);
            runtime.debug(DebugLevel::Headers)
        });
    assert_eq!(runtime_order.load(Ordering::SeqCst), 2);
    builder
        .build()
        .expect("later runtime selections replace earlier selections");
}

#[test]
fn fixed_base_https_policy_failure_is_precise_during_generated_build() {
    let error = build_error(
        ExampleApi::builder()
            .configure_http(|http| http.require_https())
            .build(),
    );
    assert_eq!(error.kind(), ClientBuildErrorKind::HttpsRequired);
}

#[tokio::test]
async fn exact_generated_direct_and_metadata_workflows_execute_through_local_proxy() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"healthy".to_vec()),
        ResponseScript::status(205, b"ignored-and-unpolled".to_vec()),
    ])
    .expect("generated proxy");
    let hooks = Arc::new(CountingHooks::new());
    let client = proxied_example(&server, Some(hooks.clone()));

    let bytes: Bytes = client.health().bytes().await.expect("generated bytes");
    assert_eq!(bytes, b"healthy"[..]);
    let response: ApiResponse<()> = client
        .health()
        .ping()
        .response()
        .await
        .expect("generated metadata");
    assert_eq!(response.status().as_u16(), 205);
    assert_eq!(response.value(), &());
    assert_eq!(hooks.before.load(Ordering::SeqCst), 2);
    assert_eq!(hooks.response.load(Ordering::SeqCst), 2);
    assert_eq!(hooks.complete.load(Ordering::SeqCst), 2);
    assert_eq!(server.request_count(), 2);
}

#[tokio::test]
async fn generated_buffered_facade_and_explicit_endpoints_use_typed_codecs_end_to_end() {
    let server = ScriptedServer::start([
        ResponseScript::ok("facade-text-response".as_bytes().to_vec()),
        ResponseScript::ok("explicit-text-response".as_bytes().to_vec()),
        ResponseScript::ok(b"[8,13,21]".to_vec())
            .with_header("Content-Type", "application/json; charset=utf-8"),
        ResponseScript::ok(b"custom-facade-response".to_vec())
            .with_header("Content-Type", "application/x-concord-wire"),
        ResponseScript::ok(b"custom-explicit-response".to_vec())
            .with_header("Content-Type", "application/x-concord-wire"),
        ResponseScript::ok(b"bytes-response".to_vec()),
        ResponseScript::ok(b"raw-bytes-response".to_vec()),
        ResponseScript::ok(b"managed-body-response".to_vec()),
        ResponseScript::status(205, b"successful-body-is-ignored".to_vec()),
    ])
    .expect("generated buffered proxy");
    let client = proxied_buffered(&server);

    let text: String = client
        .messages()
        .text_echo("facade-text-request".to_owned())
        .await
        .expect("facade text");
    assert_eq!(text, "facade-text-response");

    let endpoint = BufferedApi::unbound()
        .messages()
        .text_echo("explicit-text-request".to_owned());
    let text: ApiResponse<String> = client
        .request(endpoint)
        .response()
        .await
        .expect("explicit text metadata");
    assert_eq!(text.value(), "explicit-text-response");

    let json: ApiResponse<Vec<u32>> = client
        .messages()
        .json_echo(vec![1, 2, 3])
        .response()
        .await
        .expect("JSON metadata");
    assert_eq!(json.value(), &[8, 13, 21]);

    let custom: String = client
        .messages()
        .custom_echo("custom-facade-request".to_owned())
        .await
        .expect("custom facade");
    assert_eq!(custom, "custom-facade-response");
    let custom_endpoint = BufferedApi::unbound()
        .messages()
        .custom_echo("custom-explicit-request".to_owned());
    let custom = client
        .request(custom_endpoint)
        .response()
        .await
        .expect("custom explicit metadata");
    assert_eq!(custom.value(), "custom-explicit-response");

    let bytes: Bytes = client
        .messages()
        .bytes_echo("bytes-request".to_owned())
        .await
        .expect("bytes regression");
    assert_eq!(bytes, b"bytes-response"[..]);
    let raw_bytes: Bytes = client
        .messages()
        .raw_bytes(Bytes::from_static(b"raw-bytes-request"))
        .await
        .expect("raw bytes request specification");
    assert_eq!(raw_bytes, b"raw-bytes-response"[..]);
    let managed_body: Bytes = client
        .messages()
        .managed_body(PreparedBody::bytes(Bytes::from_static(
            b"managed-body-request",
        )))
        .await
        .expect("managed request body specification");
    assert_eq!(managed_body, b"managed-body-response"[..]);
    let no_content: ApiResponse<()> = client
        .messages()
        .no_content_echo("no-content-request".to_owned())
        .response()
        .await
        .expect("no-content regression");
    assert_eq!(no_content.status().as_u16(), 205);

    server
        .wait_for_requests(9, Duration::from_secs(1))
        .expect("all generated requests recorded");
    let requests = server.requests();
    let expected_bodies: [&[u8]; 9] = [
        b"facade-text-request",
        b"explicit-text-request",
        b"[1,2,3]",
        b"custom-facade-request",
        b"custom-explicit-request",
        b"bytes-request",
        b"raw-bytes-request",
        b"managed-body-request",
        b"no-content-request",
    ];
    for (request, expected_body) in requests.iter().zip(expected_bodies) {
        assert_eq!(request.method(), "POST");
        assert_eq!(request.body(), expected_body);
    }
    assert_eq!(requests[0].header("accept"), Some("text/plain"));
    assert_eq!(
        requests[0].header("content-type"),
        Some("text/plain; charset=utf-8")
    );
    assert_eq!(requests[2].header("accept"), Some("application/json"));
    assert_eq!(requests[2].header("content-type"), Some("application/json"));
    assert_eq!(
        requests[3].header("accept"),
        Some("application/x-concord-wire")
    );
    assert_eq!(
        requests[3].header("content-type"),
        Some("application/x-concord-wire")
    );
    // A managed prepared body without a declared media type owns none.
    assert_eq!(requests[7].header("content-type"), None);
}

#[tokio::test]
async fn generated_stream_and_multipart_facade_and_explicit_endpoints_use_one_runtime_path() {
    let stream_headers =
        |content_type: &str| vec![("Content-Type".to_owned(), content_type.to_owned())];
    let server = ScriptedServer::start([
        ResponseScript::ok(b"upload-ok".to_vec()),
        ResponseScript::ok(b"vendor-upload-ok".to_vec()),
        ResponseScript::Chunked {
            status: 200,
            headers: stream_headers("application/octet-stream"),
            chunks: vec![b"facade-".to_vec(), b"download".to_vec()],
        },
        ResponseScript::Chunked {
            status: 200,
            headers: stream_headers("application/octet-stream; framing=chunks"),
            chunks: vec![b"explicit-".to_vec(), b"vendor-download".to_vec()],
        },
        ResponseScript::ok(b"multipart-ok".to_vec()),
    ])
    .expect("generated streaming proxy");
    let hooks = Arc::new(CountingHooks::new());
    let client = proxied_streaming(&server, hooks.clone());

    let upload: Bytes = client
        .transfers()
        .upload(RequestStream::from_bytes(Bytes::from_static(
            b"generated-stream-upload",
        )))
        .await
        .expect("generated stream upload");
    assert_eq!(upload, b"upload-ok"[..]);

    let vendor_endpoint =
        StreamingApi::unbound()
            .transfers()
            .vendor_upload(RequestStream::from_bytes(Bytes::from_static(
                b"generated-vendor-upload",
            )));
    let vendor_upload: Bytes = client
        .request(vendor_endpoint)
        .await
        .expect("explicit generated vendor stream upload");
    assert_eq!(vendor_upload, b"vendor-upload-ok"[..]);

    let response: ApiResponse<ByteStream> = client
        .transfers()
        .download()
        .response()
        .await
        .expect("generated stream metadata response");
    assert_eq!(response.status(), StatusCode::OK);
    let mut download = response.into_value();
    let mut downloaded = Vec::new();
    while let Some(chunk) = next_stream_chunk(&mut download).await {
        downloaded.extend_from_slice(&chunk.expect("generated stream chunk"));
    }
    assert_eq!(downloaded, b"facade-download");

    let vendor_endpoint = StreamingApi::unbound().transfers().vendor_download();
    let response: ApiResponse<ByteStream> = client
        .request(vendor_endpoint)
        .response()
        .await
        .expect("explicit generated vendor stream metadata response");
    let mut vendor = response.into_value();
    let mut vendor_downloaded = Vec::new();
    while let Some(chunk) = next_stream_chunk(&mut vendor).await {
        vendor_downloaded.extend_from_slice(&chunk.expect("vendor stream chunk"));
    }
    assert_eq!(vendor_downloaded, b"explicit-vendor-download");

    let multipart = MultipartBody::new()
        .part(
            "text",
            MultipartPart::text("generated-multipart-text-secret")
                .expect("bounded generated multipart text"),
        )
        .expect("generated multipart part")
        .part(
            "bytes",
            MultipartPart::bytes(Bytes::from_static(b"generated-multipart-bytes-secret"))
                .expect("bounded generated multipart bytes")
                .file_name("generated-secret-name.bin")
                .expect("generated multipart filename"),
        )
        .expect("generated multipart bytes part");
    let multipart_result: Bytes = client
        .transfers()
        .multipart_upload(multipart)
        .await
        .expect("generated multipart upload");
    assert_eq!(multipart_result, b"multipart-ok"[..]);

    server
        .wait_for_requests(5, Duration::from_secs(1))
        .expect("generated streaming requests recorded");
    let requests = server.requests();
    assert_eq!(requests[0].body(), b"generated-stream-upload");
    assert_eq!(
        requests[0].header("content-type"),
        Some("application/octet-stream")
    );
    assert_eq!(requests[1].body(), b"generated-vendor-upload");
    assert_eq!(
        requests[1].header("content-type"),
        Some("application/octet-stream")
    );
    assert_eq!(
        requests[2].header("accept"),
        Some("application/octet-stream")
    );
    assert_eq!(
        requests[3].header("accept"),
        Some("application/octet-stream")
    );
    let multipart_content_type = requests[4]
        .header("content-type")
        .expect("generated multipart content type");
    let boundary = multipart_content_type
        .strip_prefix("multipart/form-data; boundary=")
        .expect("generated multipart media type");
    let multipart_wire = String::from_utf8_lossy(requests[4].body());
    assert!(multipart_wire.contains(&format!("--{boundary}")));
    assert!(multipart_wire.contains("generated-multipart-text-secret"));
    assert!(multipart_wire.contains("generated-multipart-bytes-secret"));
    assert!(multipart_wire.contains("generated-secret-name.bin"));

    assert_eq!(hooks.before.load(Ordering::SeqCst), 5);
    assert_eq!(hooks.response.load(Ordering::SeqCst), 5);
    assert_eq!(hooks.complete.load(Ordering::SeqCst), 5);
    assert_eq!(
        hooks.outcomes(),
        vec![
            CompletionOutcome::Success,
            CompletionOutcome::Success,
            CompletionOutcome::StreamOpened,
            CompletionOutcome::StreamOpened,
            CompletionOutcome::Success,
        ]
    );
}

#[tokio::test]
async fn facade_and_explicit_endpoints_execute_with_identical_encoded_paths() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"facade".to_vec()),
        ResponseScript::ok(b"explicit".to_vec()),
    ])
    .expect("parity proxy");
    let client = proxied_example(&server, None);
    let dynamic = "team alpha%2F".to_owned();

    // v2 `debug(...)` on generated requests is removed (§25): debug levels
    // are runtime configuration only.
    let facade = client
        .health()
        .item(&dynamic)
        .timeout(Duration::from_secs(5))
        .expect("finite timeout")
        .await
        .expect("facade request");
    let explicit = ExampleApi::unbound().health().item(dynamic);
    let explicit = client
        .request(explicit)
        .reset_timeout()
        .await
        .expect("explicit request");
    assert_eq!(facade, b"facade"[..]);
    assert_eq!(explicit, b"explicit"[..]);

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].target(), requests[1].target());
    // One template hole is one segment, percent-encoded exactly once; a
    // user-supplied `%2F` is data, never decoded into a slash (PATH-01).
    assert!(
        requests[0]
            .target()
            .contains("/v1/health/items/team%20alpha%252F"),
        "{}",
        requests[0].target()
    );

    // PATH-01: a hole value containing `/` is not silently split or
    // re-encoded into another segment; it is rejected before any work.
    let error = client
        .health()
        .item("team/alpha")
        .await
        .expect_err("a slash inside one path segment is rejected");
    assert_eq!(
        error.configuration_kind(),
        Some(concord::error::ConfigurationErrorKind::InvalidRoute)
    );
    assert_eq!(
        error.dispatch_knowledge(),
        concord::error::DispatchKnowledge::NotDispatched
    );
    assert!(!format!("{error} {error:?}").contains("team/alpha"));
    assert_eq!(server.request_count(), 2);
}

#[tokio::test]
async fn request_creation_is_lazy_and_generated_client_clone_shares_runtime_configuration() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"first".to_vec()),
        ResponseScript::ok(b"second".to_vec()),
    ])
    .expect("lazy proxy");
    let proxy = proxy(&server);
    let hooks = Arc::new(CountingHooks::new());
    let observation = hooks.clone();
    let client = LazyApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| {
            runtime
                .operation_observer(observation.clone())
                .operation_observer(observation.clone())
                .operation_observer(observation)
        })
        .build()
        .expect("lazy client");
    let clone = client.clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let facade = client.resources(CountingValue {
        text: "scope-value",
        calls: Arc::clone(&calls),
    });
    let request = facade.clone().by_id(CountingValue {
        text: "endpoint-value",
        calls: Arc::clone(&calls),
    });
    let sibling = facade.ping();
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(server.request_count(), 0);
    assert_eq!(hooks.before.load(Ordering::SeqCst), 0);
    assert_eq!(hooks.response.load(Ordering::SeqCst), 0);
    assert_eq!(hooks.complete.load(Ordering::SeqCst), 0);
    drop(sibling);
    assert_eq!(request.await.expect("lazy request"), b"first"[..]);
    // Each surviving atom is encoded exactly once for the execution.
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(
        server.requests()[0]
            .target()
            .contains("/scopes/scope-value/resources/endpoint-value")
    );

    let second = clone
        .resources(CountingValue {
            text: "other-scope",
            calls: Arc::clone(&calls),
        })
        .by_id(CountingValue {
            text: "other-endpoint",
            calls: Arc::clone(&calls),
        })
        .reset_timeout()
        .await
        .expect("clone request");
    assert_eq!(second, b"second"[..]);
    assert_eq!(hooks.before.load(Ordering::SeqCst), 2);
    assert_eq!(hooks.complete.load(Ordering::SeqCst), 2);
}

#[test]
fn nested_and_repeated_invocations_keep_generated_paths_isolated() {
    let nested = nested_case::inner::NestedApi::new().expect("nested client");
    let _: nested_case::inner::nested_api::health::PingRequest = nested.health().ping();
    let _: nested_case::inner::nested_api::health::EchoRequest =
        nested.health().echo("nested".to_owned());
    let _: nested_case::inner::nested_api::health::UploadRequest =
        nested
            .health()
            .upload(RequestStream::from_bytes(Bytes::from_static(
                b"nested-stream",
            )));
    let _: nested_case::inner::nested_api::health::DownloadRequest = nested.health().download();
    let _: nested_case::inner::nested_api::health::PingRequest =
        nested.request(nested_case::inner::NestedApi::unbound().health().ping());
    repeated_case::requests_are_isolated();
}
