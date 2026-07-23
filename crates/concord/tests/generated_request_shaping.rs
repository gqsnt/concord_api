#![cfg(feature = "multipart")]

use std::fmt::{self, Display, Formatter};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use concord::advanced::{
    ClientBuildErrorKind, ClientInputKind, Method, MultipartBody, MultipartPart, RuntimeEndpoint,
    SafeProxy,
};
use concord::api;
use concord::prelude::{ApiResponse, ByteStream, Bytes, ErrorCategory, OctetStream, StreamBody};
use concord_test_support::server::{BodyGate, ResponseScript, ScriptedServer};
use futures_core::Stream as FuturesStream;
use tokio::sync::{Mutex, MutexGuard};

static CLIENT_DEFAULT_DROPS: AtomicUsize = AtomicUsize::new(0);
static SCOPE_DEFAULT_DROPS: AtomicUsize = AtomicUsize::new(0);
static ENDPOINT_DEFAULT_DROPS: AtomicUsize = AtomicUsize::new(0);
static EXPLICIT_DROPS: AtomicUsize = AtomicUsize::new(0);
static SHAPING_FORMATS: AtomicUsize = AtomicUsize::new(0);
static REQUEST_SHAPING_TEST_LOCK: Mutex<()> = Mutex::const_new(());

async fn request_shaping_test_guard() -> MutexGuard<'static, ()> {
    REQUEST_SHAPING_TEST_LOCK.lock().await
}

pub struct DropProbe {
    value: &'static str,
    drops: fn() -> &'static AtomicUsize,
}

impl DropProbe {
    const fn new(value: &'static str, drops: fn() -> &'static AtomicUsize) -> Self {
        Self { value, drops }
    }
}

impl Display for DropProbe {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.value)
    }
}

impl Drop for DropProbe {
    fn drop(&mut self) {
        (self.drops)().fetch_add(1, Ordering::SeqCst);
    }
}

pub struct ShapeProbe(&'static str);

impl Display for ShapeProbe {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        SHAPING_FORMATS.fetch_add(1, Ordering::SeqCst);
        formatter.write_str(self.0)
    }
}

fn client_default_drops() -> &'static AtomicUsize {
    &CLIENT_DEFAULT_DROPS
}

fn scope_default_drops() -> &'static AtomicUsize {
    &SCOPE_DEFAULT_DROPS
}

fn endpoint_default_drops() -> &'static AtomicUsize {
    &ENDPOINT_DEFAULT_DROPS
}

fn explicit_drops() -> &'static AtomicUsize {
    &EXPLICIT_DROPS
}

pub const CLIENT_DEFAULT: DropProbe = DropProbe::new("client-default", client_default_drops);
pub const SCOPE_DEFAULT: DropProbe = DropProbe::new("scope-default", scope_default_drops);
pub const ENDPOINT_DEFAULT: DropProbe = DropProbe::new("endpoint-default", endpoint_default_drops);

api! {
    client ShapeApi {
        base "http://request-shaping.example.test:4317/fixed/";
        vars {
            tenant: String;
            locale: Option<String>;
            client_marker: crate::DropProbe = crate::CLIENT_DEFAULT;
        }
        retry off;
        host ["api"];
        path ["v1"];
        query {
            "tenant" = vars.tenant;
            "locale" = vars.locale;
        }
        headers {
            "X-Client-Marker" = vars.client_marker;
        }
        timeout 5s;
    }

    GET root(verbose: Option<bool>) -> Bytes {
        path ["root"];
        query { "verbose" = ep.verbose; }
    }

    GET search(tags: Vec<String>, filters: Option<Vec<String>>) -> Bytes {
        path ["search"];
        query {
            "tag" = ep.tags;
            "filter" = ep.filters;
        }
    }

    GET accept(accept: String) -> Text {
        path ["accept"];
        headers { "Accept" = ep.accept; }
    }

    GET public_content_type() -> Bytes {
        path ["public-content-type"];
        headers { "Content-Type" = "application/example"; }
    }

    POST text_conflict(#[body] body: Text) -> Bytes {
        path ["text-conflict"];
        headers { "Content-Type" = "application/example"; }
    }

    POST stream_conflict(#[body] body: Stream<OctetStream>) -> Bytes {
        path ["stream-conflict"];
        headers { "Content-Type" = "application/example"; }
    }

    POST multipart_conflict(#[body] body: Multipart) -> NoContent {
        path ["multipart-conflict"];
        headers { "Content-Type" = "application/example"; }
    }

    GET streamed(verbose: Option<bool>) -> Stream<OctetStream> {
        path ["streamed"];
        query { "verbose" = ep.verbose; }
    }

    GET unsafe_path(segment: crate::ShapeProbe) -> Bytes {
        path ["unsafe-path", ep.segment];
    }

    GET unsafe_host(label: crate::ShapeProbe) -> Bytes {
        host [ep.label];
        path ["unsafe-host"];
    }

    GET unsafe_header(value: crate::ShapeProbe) -> Bytes {
        path ["unsafe-header"];
        headers { "X-Dynamic" = ep.value; }
    }

    scope regions(
        region: String,
        language: Option<String>,
        scope_marker: crate::DropProbe = crate::SCOPE_DEFAULT,
    ) {
        host [scope.region, "api"];
        path ["regions", scope.region];
        query {
            "language" = scope.language;
            "scope_marker" = scope.scope_marker;
        }
        headers { "X-Region" = scope.region; }

        scope users(account: String) {
            path ["users", scope.account];
            headers { "X-Account" = scope.account; }

            METHOD("PROPFIND") fetch(
                id: String,
                verbose: Option<bool>,
                endpoint_marker: crate::DropProbe = crate::ENDPOINT_DEFAULT,
            ) -> Bytes {
                path [fmt["id-", ep.id]];
                query {
                    "verbose" = ep.verbose;
                    "endpoint_marker" = ep.endpoint_marker;
                }
                headers { "X-Endpoint-Marker" = ep.endpoint_marker; }
                timeout 2s;
            }
        }
    }
}

api! {
    client OtherApi {
        base "http://other.request-shaping.example.test/";
        vars {
            label: Option<String> = None;
        }
    }

    GET echo(verbose: Option<bool>) -> Bytes {
        path ["echo"];
        query {
            "label" = vars.label;
            "verbose" = ep.verbose;
        }
    }
    PUT put() -> NoContent { path ["put"]; }
    PATCH patch() -> NoContent { path ["patch"]; }
    DELETE delete() -> NoContent { path ["delete"]; }
    HEAD head() -> NoContent { path ["head"]; }
    OPTIONS options() -> NoContent { path ["options"]; }
    TRACE trace() -> NoContent { path ["trace"]; }
    CONNECT connect() -> NoContent { path ["connect"]; }
    GET r#type() -> NoContent { path ["keyword"]; }
}

api! {
    client TimeoutApi {
        base "http://timeout.request-shaping.example.test/";
        timeout 20ms;
    }

    scope timed() {
        timeout 10ms;
        GET scope_inherited() -> Bytes { path ["scope-inherited"]; }
        GET endpoint_override() -> Bytes {
            path ["endpoint-override"];
            timeout 200ms;
        }
        GET endpoint_off() -> Bytes {
            path ["endpoint-off"];
            timeout off;
        }
    }
}

fn proxy(server: &ScriptedServer) -> SafeProxy {
    SafeProxy::parse(server.url("")).expect("local proxy")
}

async fn wait_for_requests(server: Arc<ScriptedServer>, expected: usize) {
    tokio::task::spawn_blocking(move || server.wait_for_requests(expected, Duration::from_secs(3)))
        .await
        .expect("request barrier task")
        .expect("request barrier");
}

fn shaped_client(server: &ScriptedServer) -> ShapeApi {
    let proxy = proxy(server);
    ShapeApi::builder()
        .tenant("tenant-a".to_owned())
        .locale("fr".to_owned())
        .client_marker(DropProbe::new("client-explicit", explicit_drops))
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("shape client")
}

async fn next_stream_chunk(
    response: &mut ByteStream,
) -> Option<Result<Bytes, concord::prelude::ApiError>> {
    std::future::poll_fn(|context| Pin::new(&mut *response).poll_next(context)).await
}

#[tokio::test(flavor = "current_thread")]
async fn required_optional_defaulted_client_values_and_missing_input_are_stable() {
    let _guard = request_shaping_test_guard().await;
    let missing = ShapeApi::builder()
        .build()
        .err()
        .expect("required tenant must be absent");
    assert_eq!(missing.kind(), ClientBuildErrorKind::MissingInput);
    assert_eq!(missing.missing_field(), Some("tenant"));
    assert_eq!(missing.missing_input_kind(), Some(ClientInputKind::Value));

    let before_default = CLIENT_DEFAULT_DROPS.load(Ordering::SeqCst);
    let before_explicit = EXPLICIT_DROPS.load(Ordering::SeqCst);
    let explicit = ShapeApi::builder()
        .tenant("tenant".to_owned())
        .client_marker(DropProbe::new("explicit", explicit_drops))
        .build()
        .expect("explicit default override");
    let clone = explicit.clone();
    drop(explicit);
    assert_eq!(EXPLICIT_DROPS.load(Ordering::SeqCst), before_explicit);
    drop(clone);
    assert_eq!(EXPLICIT_DROPS.load(Ordering::SeqCst), before_explicit + 1);
    assert_eq!(CLIENT_DEFAULT_DROPS.load(Ordering::SeqCst), before_default);

    let selected = ShapeApi::builder()
        .tenant("tenant".to_owned())
        .build()
        .expect("named client builder");
    drop(selected);
    assert_eq!(
        CLIENT_DEFAULT_DROPS.load(Ordering::SeqCst),
        before_default + 1
    );

    let defaults_only = OtherApi::new().expect("optional/defaulted inputs preserve zero-input new");
    drop(defaults_only);
}

#[tokio::test(flavor = "current_thread")]
async fn nested_facade_extension_setters_and_explicit_endpoint_have_head_parity() {
    let _guard = request_shaping_test_guard().await;
    let server = ScriptedServer::start([
        ResponseScript::ok(b"facade".to_vec()),
        ResponseScript::ok(b"explicit".to_vec()),
        ResponseScript::ok(b"root".to_vec()),
        ResponseScript::ok(b"other".to_vec()),
    ])
    .expect("request-shaping server");
    let client = shaped_client(&server);
    let other_proxy = proxy(&server);
    let other = OtherApi::builder()
        .label(Some("chosen".to_owned()))
        .configure_http(move |http| http.proxy(other_proxy))
        .build()
        .expect("other client");
    let reset_other = OtherApi::builder()
        .label(Some("unused".to_owned()))
        .reset_label()
        .build()
        .expect("defaulted Option client value");
    drop(reset_other);

    let regions = client.regions("eu".to_owned()).language("de".to_owned());
    let users = regions.users("account-a".to_owned());
    let facade = users
        .fetch("42".to_owned())
        .verbose(true)
        .endpoint_marker(DropProbe::new("temporary", explicit_drops))
        .reset_endpoint_marker()
        .await
        .expect("facade endpoint");
    assert_eq!(facade, b"facade"[..]);

    let explicit = shape_api::regions::users::Fetch::new(
        "eu".to_owned(),
        "account-a".to_owned(),
        "42".to_owned(),
    )
    .language("de".to_owned())
    .verbose(true);
    let explicit = client.request(explicit).await.expect("explicit endpoint");
    assert_eq!(explicit, b"explicit"[..]);

    let metadata: ApiResponse<Bytes> = client
        .root()
        .timeout(Duration::from_secs(1))
        .verbose(true)
        .inherit_timeout()
        .response()
        .await
        .expect("metadata after generated extension setter");
    assert_eq!(metadata.value(), &b"root"[..]);

    let other_value = other
        .echo()
        .verbose(true)
        .await
        .expect("second-client prelude setter");
    assert_eq!(other_value, b"other"[..]);

    let requests = server.requests();
    assert_eq!(requests.len(), 4);
    assert_eq!(requests[0].method(), "PROPFIND");
    assert_eq!(requests[0].target(), requests[1].target());
    assert!(requests[0].target().contains(
        "http://eu.api.request-shaping.example.test:4317/fixed/v1/regions/eu/users/account-a/id-42?tenant=tenant-a&locale=fr&language=de&scope_marker=scope-default&verbose=true&endpoint_marker=endpoint-default"
    ));
    assert_eq!(
        requests[0].header("x-client-marker"),
        Some("client-explicit")
    );
    assert_eq!(requests[0].header("x-region"), Some("eu"));
    assert_eq!(requests[0].header("x-account"), Some("account-a"));
    assert_eq!(
        requests[0].header("x-endpoint-marker"),
        Some("endpoint-default")
    );
    assert!(
        requests[2]
            .target()
            .contains("/fixed/v1/root?tenant=tenant-a&locale=fr&verbose=true")
    );
    assert!(
        requests[3]
            .target()
            .contains("/echo?label=chosen&verbose=true")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn scope_and_endpoint_resets_reuse_one_retained_default_without_clone_bounds() {
    let _guard = request_shaping_test_guard().await;
    let server = ScriptedServer::start([]).expect("drop-only server");
    let client = shaped_client(&server);
    let scope_before = SCOPE_DEFAULT_DROPS.load(Ordering::SeqCst);
    let endpoint_before = ENDPOINT_DEFAULT_DROPS.load(Ordering::SeqCst);
    let explicit_before = EXPLICIT_DROPS.load(Ordering::SeqCst);

    let facade = client.regions("eu".to_owned());
    let source = facade.clone();
    let reset = facade
        .scope_marker(DropProbe::new("temporary", explicit_drops))
        .reset_scope_marker();
    drop(source);
    assert_eq!(SCOPE_DEFAULT_DROPS.load(Ordering::SeqCst), scope_before);

    let request: shape_api::regions::users::FetchRequest = reset
        .users("account".to_owned())
        .fetch("id".to_owned())
        .endpoint_marker(DropProbe::new("temporary", explicit_drops))
        .reset_endpoint_marker();
    drop(request);
    assert_eq!(
        ENDPOINT_DEFAULT_DROPS.load(Ordering::SeqCst),
        endpoint_before + 1
    );
    drop(reset);
    assert_eq!(SCOPE_DEFAULT_DROPS.load(Ordering::SeqCst), scope_before + 1);
    assert_eq!(EXPLICIT_DROPS.load(Ordering::SeqCst), explicit_before + 2);
    drop(client);
    assert_eq!(EXPLICIT_DROPS.load(Ordering::SeqCst), explicit_before + 3);
}

#[tokio::test(flavor = "current_thread")]
async fn consuming_request_setters_keep_sibling_snapshots_isolated() {
    let _guard = request_shaping_test_guard().await;
    let server = ScriptedServer::start([
        ResponseScript::ok(b"selected".to_vec()),
        ResponseScript::ok(b"cleared".to_vec()),
    ])
    .expect("sibling server");
    let client = shaped_client(&server);

    let selected = client.root().verbose(true);
    let cleared = client.root().verbose(false).clear_verbose();
    assert_eq!(selected.await.expect("selected sibling"), b"selected"[..]);
    assert_eq!(cleared.await.expect("cleared sibling"), b"cleared"[..]);

    let requests = server.requests();
    assert!(requests[0].target().contains("verbose=true"));
    assert!(!requests[1].target().contains("verbose="));
}

#[tokio::test(flavor = "current_thread")]
async fn facade_setters_leave_the_source_scope_snapshot_unchanged() {
    let _guard = request_shaping_test_guard().await;
    let server = ScriptedServer::start([
        ResponseScript::ok(b"source".to_vec()),
        ResponseScript::ok(b"cleared".to_vec()),
    ])
    .expect("facade snapshot server");
    let client = shaped_client(&server);
    let source = client.regions("eu".to_owned()).language("fr".to_owned());
    let cleared = source.clone().clear_language();

    assert_eq!(
        source
            .users("account".to_owned())
            .fetch("id".to_owned())
            .await
            .expect("source facade"),
        b"source"[..]
    );
    assert_eq!(
        cleared
            .users("account".to_owned())
            .fetch("id".to_owned())
            .await
            .expect("cleared facade"),
        b"cleared"[..]
    );
    let requests = server.requests();
    assert!(requests[0].target().contains("language=fr"));
    assert!(!requests[1].target().contains("language="));
}

#[tokio::test(flavor = "current_thread")]
async fn independently_built_clients_retain_independent_value_snapshots() {
    let _guard = request_shaping_test_guard().await;
    let server = ScriptedServer::start([
        ResponseScript::ok(b"first".to_vec()),
        ResponseScript::ok(b"second".to_vec()),
    ])
    .expect("independent client server");
    let first_proxy = proxy(&server);
    let first = ShapeApi::builder()
        .tenant("discarded".to_owned())
        .tenant("tenant-one".to_owned())
        .locale("discarded".to_owned())
        .clear_locale()
        .client_marker(DropProbe::new("discarded", explicit_drops))
        .reset_client_marker()
        .configure_http(move |http| http.proxy(first_proxy))
        .build()
        .expect("first client");
    let second_proxy = proxy(&server);
    let second = ShapeApi::builder()
        .tenant("tenant-two".to_owned())
        .locale("discarded".to_owned())
        .locale("es".to_owned())
        .configure_http(move |http| http.proxy(second_proxy))
        .build()
        .expect("second client");

    assert_eq!(first.root().await.expect("first request"), b"first"[..]);
    assert_eq!(second.root().await.expect("second request"), b"second"[..]);
    let requests = server.requests();
    assert!(requests[0].target().contains("tenant=tenant-one"));
    assert!(!requests[0].target().contains("locale="));
    assert!(requests[1].target().contains("tenant=tenant-two"));
    assert!(requests[1].target().contains("locale=es"));
}

#[tokio::test(flavor = "current_thread")]
async fn collection_queries_and_every_standard_method_preserve_runtime_order() {
    let _guard = request_shaping_test_guard().await;
    let server = ScriptedServer::start(
        [
            ResponseScript::ok(b"search-values".to_vec()),
            ResponseScript::ok(b"search-empty".to_vec()),
        ]
        .into_iter()
        .chain((0..8).map(|_| ResponseScript::status(204, Vec::new()))),
    )
    .expect("query and method server");
    let client = shaped_client(&server);
    let other_proxy = proxy(&server);
    let other = OtherApi::builder()
        .configure_http(move |http| http.proxy(other_proxy))
        .build()
        .expect("method client");

    assert_eq!(
        client
            .search(vec!["one two".to_owned(), "three".to_owned()])
            .filters(vec!["x".to_owned(), "y".to_owned()])
            .await
            .expect("vector query"),
        b"search-values"[..]
    );
    assert_eq!(
        client.search(Vec::new()).await.expect("empty vector query"),
        b"search-empty"[..]
    );

    other.put().await.expect("PUT");
    other.patch().await.expect("PATCH");
    other.delete().await.expect("DELETE");
    other.head().await.expect("HEAD");
    other.options().await.expect("OPTIONS");
    other.trace().await.expect("TRACE");
    other.connect().await.expect("CONNECT");
    other.r#type().await.expect("raw-keyword alias");

    let requests = server.requests();
    assert_eq!(requests.len(), 10);
    assert!(requests[0].target().contains(
        "/fixed/v1/search?tenant=tenant-a&locale=fr&tag=one+two&tag=three&filter=x&filter=y"
    ));
    assert!(
        requests[1]
            .target()
            .contains("/fixed/v1/search?tenant=tenant-a&locale=fr")
    );
    assert_eq!(
        requests[2..]
            .iter()
            .map(|request| request.method())
            .collect::<Vec<_>>(),
        [
            "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS", "TRACE", "CONNECT", "GET"
        ]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn generated_public_media_headers_obey_single_map_ownership() {
    let _guard = request_shaping_test_guard().await;
    let server = ScriptedServer::start([
        ResponseScript::ok(b"accepted".to_vec()),
        ResponseScript::ok(b"bodyless".to_vec()),
        ResponseScript::Chunked {
            status: 200,
            headers: vec![(
                "Content-Type".to_owned(),
                "application/octet-stream".to_owned(),
            )],
            chunks: vec![b"streamed".to_vec()],
        },
    ])
    .expect("media server");
    let client = shaped_client(&server);

    assert_eq!(
        client
            .accept("application/vnd.concord.request-shaping".to_owned())
            .await
            .expect("public Accept"),
        "accepted"
    );
    assert_eq!(
        client
            .public_content_type()
            .await
            .expect("bodyless Content-Type"),
        b"bodyless"[..]
    );

    let mut streamed: ByteStream = client
        .streamed()
        .verbose(true)
        .await
        .expect("direct stream await after generated setter");
    assert_eq!(
        next_stream_chunk(&mut streamed)
            .await
            .expect("stream chunk")
            .expect("bounded stream bytes"),
        b"streamed"[..]
    );
    assert!(next_stream_chunk(&mut streamed).await.is_none());

    let text_error = client
        .text_conflict("body".to_owned())
        .await
        .expect_err("Text owns Content-Type");
    assert_eq!(text_error.category(), ErrorCategory::Configuration);

    let stream_error = client
        .stream_conflict(StreamBody::from_bytes(Bytes::from_static(b"stream")))
        .await
        .expect_err("stream media owns Content-Type");
    assert_eq!(stream_error.category(), ErrorCategory::Configuration);

    let multipart = MultipartBody::new()
        .part("field", MultipartPart::text("value"))
        .expect("multipart recipe");
    let multipart_error = client
        .multipart_conflict(multipart)
        .await
        .expect_err("multipart owns its top-level boundary header");
    assert_eq!(multipart_error.category(), ErrorCategory::Configuration);

    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(
        requests[0].header("accept"),
        Some("application/vnd.concord.request-shaping")
    );
    assert_eq!(
        requests[1].header("content-type"),
        Some("application/example")
    );
    assert!(
        requests[2]
            .target()
            .contains("/streamed?tenant=tenant-a&locale=fr&verbose=true")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn dynamic_request_shaping_is_deferred_and_fails_before_native_execution() {
    let _guard = request_shaping_test_guard().await;
    SHAPING_FORMATS.store(0, Ordering::SeqCst);
    let server = ScriptedServer::start([]).expect("shaping trap");
    let client = shaped_client(&server);

    let path = client.unsafe_path(ShapeProbe("two/segments"));
    let host = client.unsafe_host(ShapeProbe("two.labels"));
    let header = client.unsafe_header(ShapeProbe("bad\r\nvalue"));
    assert_eq!(SHAPING_FORMATS.load(Ordering::SeqCst), 0);

    let error = path.await.expect_err("dynamic path must fail closed");
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(SHAPING_FORMATS.load(Ordering::SeqCst), 1);

    let error = host.await.expect_err("dynamic host must fail closed");
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(SHAPING_FORMATS.load(Ordering::SeqCst), 2);

    let error = header.await.expect_err("dynamic header must fail closed");
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(SHAPING_FORMATS.load(Ordering::SeqCst), 3);
    assert_eq!(server.request_count(), 0);
}

#[tokio::test(flavor = "current_thread")]
async fn generated_timeout_inheritance_off_and_request_overrides_remain_distinct() {
    let _guard = request_shaping_test_guard().await;
    let gate = BodyGate::closed();
    let server = Arc::new(
        ScriptedServer::start((0..7).map(|_| ResponseScript::GatedBody {
            status: 200,
            body: b"done".to_vec(),
            gate: gate.clone(),
        }))
        .expect("timeout server"),
    );
    let timeout_proxy = proxy(&server);
    let client = TimeoutApi::builder()
        .configure_http(move |http| http.proxy(timeout_proxy))
        .build()
        .expect("timeout client");

    let inherited = tokio::spawn({
        let client = client.clone();
        async move { client.timed().scope_inherited().await }
    });
    let endpoint_override = tokio::spawn({
        let client = client.clone();
        async move { client.timed().endpoint_override().await }
    });
    let endpoint_off = tokio::spawn({
        let client = client.clone();
        async move { client.timed().endpoint_off().await }
    });
    let local = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .timed()
                .endpoint_off()
                .timeout(Duration::from_millis(10))
                .await
        }
    });
    let disabled = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .timed()
                .endpoint_off()
                .timeout(Duration::from_millis(10))
                .disable_timeout()
                .await
        }
    });
    let reinherited = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .timed()
                .endpoint_override()
                .disable_timeout()
                .inherit_timeout()
                .await
        }
    });
    let runtime_inherited = tokio::spawn({
        let client = client.clone();
        async move {
            let endpoint = RuntimeEndpoint::builder("timeout_api.runtime".into(), Method::GET)
                .response_bytes()
                .build()
                .expect("runtime endpoint");
            client.request(endpoint).await
        }
    });

    wait_for_requests(Arc::clone(&server), 7).await;

    let inherited = inherited
        .await
        .expect("scope timeout task")
        .expect_err("scope timeout must inherit");
    assert_eq!(inherited.category(), ErrorCategory::Timeout);
    let local = local
        .await
        .expect("request-local timeout task")
        .expect_err("request-local timeout must override endpoint off");
    assert_eq!(local.category(), ErrorCategory::Timeout);
    let runtime_inherited = runtime_inherited
        .await
        .expect("runtime endpoint timeout task")
        .expect_err("runtime endpoint must inherit the client timeout snapshot");
    assert_eq!(runtime_inherited.category(), ErrorCategory::Timeout);

    gate.release();
    assert_eq!(
        endpoint_override
            .await
            .expect("endpoint timeout override task")
            .expect("endpoint timeout override"),
        b"done"[..]
    );
    assert_eq!(
        endpoint_off
            .await
            .expect("endpoint timeout off task")
            .expect("endpoint timeout off"),
        b"done"[..]
    );
    assert_eq!(
        disabled
            .await
            .expect("request-local disable task")
            .expect("request-local disable"),
        b"done"[..]
    );
    assert_eq!(
        reinherited
            .await
            .expect("return to endpoint inheritance task")
            .expect("return to endpoint inheritance"),
        b"done"[..]
    );
    assert_eq!(server.request_count(), 7);
}
