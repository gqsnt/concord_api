#![cfg(feature = "multipart")]

use std::fmt::{self, Display, Formatter};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use concord::api;
use concord::body::{Multipart, MultipartBody, MultipartPart, Stream, StreamBody};
use concord::codec::OctetStream;
use concord::endpoint::{EndpointId, MAX_FORMATTED_PUBLIC_VALUE_BYTES, RuntimeEndpoint};
use concord::error::{
    ClientBuildErrorKind, ClientInputKind, ConfigurationErrorKind, ErrorCategory,
};
use concord::http::{Method, SafeProxy};
use concord::response::{ApiResponse, ByteStream};
use concord::{Bytes, NoContent, Text};
use concord_test_support::server::{BodyGate, ResponseScript, ScriptedServer};
use futures_core::Stream as FuturesStream;
use tokio::sync::{Mutex, MutexGuard};

static CLIENT_DEFAULT_DROPS: AtomicUsize = AtomicUsize::new(0);
static SCOPE_DEFAULT_DROPS: AtomicUsize = AtomicUsize::new(0);
static ENDPOINT_DEFAULT_DROPS: AtomicUsize = AtomicUsize::new(0);
static EXPLICIT_DROPS: AtomicUsize = AtomicUsize::new(0);
static SHAPING_FORMATS: AtomicUsize = AtomicUsize::new(0);
static CLIENT_TRAIT_DEFAULTS: AtomicUsize = AtomicUsize::new(0);
static SCOPE_TRAIT_DEFAULTS: AtomicUsize = AtomicUsize::new(0);
static ENDPOINT_TRAIT_DEFAULTS: AtomicUsize = AtomicUsize::new(0);
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

pub struct FormatProbe {
    output: String,
    calls: Arc<AtomicUsize>,
    panic_if_formatted: bool,
}

impl FormatProbe {
    fn output(output: String, calls: Arc<AtomicUsize>) -> Self {
        Self {
            output,
            calls,
            panic_if_formatted: false,
        }
    }

    fn panic(calls: Arc<AtomicUsize>) -> Self {
        Self {
            output: String::new(),
            calls,
            panic_if_formatted: true,
        }
    }
}

impl Display for FormatProbe {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert!(!self.panic_if_formatted, "later format value was inspected");
        formatter.write_str(&self.output)
    }
}

pub struct FailingFormatProbe;

impl Display for FailingFormatProbe {
    fn fmt(&self, _formatter: &mut Formatter<'_>) -> fmt::Result {
        Err(fmt::Error)
    }
}

pub struct ClientTraitDefaultProbe;

impl Default for ClientTraitDefaultProbe {
    fn default() -> Self {
        CLIENT_TRAIT_DEFAULTS.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

pub struct ScopeTraitDefaultProbe;

impl Default for ScopeTraitDefaultProbe {
    fn default() -> Self {
        SCOPE_TRAIT_DEFAULTS.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

pub struct EndpointTraitDefaultProbe;

impl Default for EndpointTraitDefaultProbe {
    fn default() -> Self {
        ENDPOINT_TRAIT_DEFAULTS.fetch_add(1, Ordering::SeqCst);
        Self
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
pub const OPTIONAL_CLIENT_DEFAULT: &str = "client-default";
pub const OPTIONAL_SCOPE_DEFAULT: &str = "scope-default";
pub const OPTIONAL_ENDPOINT_DEFAULT: &str = "endpoint-default";
pub const OPTIONAL_VALUES_DEFAULT: Vec<&str> = Vec::new();

api! {
    client ShapeApi {
        base "http://request-shaping.example.test:4317/fixed/";
        values {
            tenant: String;
            locale?: String;
            client_marker: crate::DropProbe = crate::CLIENT_DEFAULT;
        }
        subdomain replace ["api"];
        path ["v1"];
        query {
            "tenant" = client.tenant;
            "locale" = client.locale;
        }
        headers {
            "X-Client-Marker" = client.client_marker;
        }
        timeout 5s;


    GET root(verbose?: bool) -> Bytes using Bytes {
        path ["root"];
        query { "verbose" = endpoint.verbose; }
    }

    GET search(tags: Vec<String>, filters?: Vec<String>) -> Bytes using Bytes {
        path ["search"];
        query {
            "tag" = each endpoint.tags;
            "filter" = each endpoint.filters;
        }
    }

    GET accept(accept: String) -> String using Text {
        path ["accept"];
        accept endpoint.accept;
    }

    POST text_body(body body: String using Text) -> Bytes using Bytes {
        path ["text-body"];
    }

    POST stream_body(body body: ::concord::body::StreamBody using Stream<OctetStream>) -> Bytes using Bytes {
        path ["stream-body"];
    }

    POST multipart_body(body body: ::concord::body::MultipartBody using Multipart) -> () using NoContent {
        path ["multipart-body"];
    }

    GET streamed(verbose?: bool) -> ::concord::response::ByteStream using Stream<OctetStream> {
        path ["streamed"];
        query { "verbose" = endpoint.verbose; }
    }

    GET unsafe_path(segment: crate::ShapeProbe) -> Bytes using Bytes {
        path ["unsafe-path", endpoint.segment];
    }

    GET unsafe_host(label: crate::ShapeProbe) -> Bytes using Bytes {
        subdomain replace [endpoint.label];
        path ["unsafe-host"];
    }

    GET unsafe_header(value: crate::ShapeProbe) -> Bytes using Bytes {
        path ["unsafe-header"];
        headers { "X-Dynamic" = endpoint.value; }
    }

    GET bounded_format_path(first: crate::FormatProbe, tail: crate::FormatProbe) -> Bytes using Bytes {
        path [concat [concat [endpoint.first], concat [endpoint.tail]]];
    }

    GET bounded_format_host(first: crate::FormatProbe, tail: crate::FormatProbe) -> Bytes using Bytes {
        subdomain replace [concat [concat [endpoint.first], concat [endpoint.tail]]];
    }

    GET bounded_format_query(first: crate::FormatProbe, tail: crate::FormatProbe) -> Bytes using Bytes {
        query { "formatted" = concat [concat [endpoint.first], concat [endpoint.tail]]; }
    }

    GET bounded_format_header(first: crate::FormatProbe, tail: crate::FormatProbe) -> Bytes using Bytes {
        headers { "X-Formatted" = concat [concat [endpoint.first], concat [endpoint.tail]]; }
    }

    GET failing_format(value: crate::FailingFormatProbe) -> Bytes using Bytes {
        path [concat ["prefix-", endpoint.value]];
    }

    scope regions(
        region: String,
        language?: String,
        scope_marker: crate::DropProbe = crate::SCOPE_DEFAULT,
    ) {
        subdomain replace [regions.region, "api"];
        path ["regions", regions.region];
        query {
            "language" = regions.language;
            "scope_marker" = regions.scope_marker;
        }
        headers { "X-Region" = regions.region; }

        scope users(account: String) {
            path ["users", users.account];
            headers { "X-Account" = users.account; }

            METHOD("PROPFIND") fetch(
                id: String,
                verbose?: bool,
                endpoint_marker: crate::DropProbe = crate::ENDPOINT_DEFAULT,
            ) -> Bytes using Bytes {
                path [concat ["id-", endpoint.id]];
                query {
                    "verbose" = endpoint.verbose;
                    "endpoint_marker" = endpoint.endpoint_marker;
                }
                headers { "X-Endpoint-Marker" = endpoint.endpoint_marker; }
                timeout 2s;
            }
        }
    }
}}

api! {
    client OtherApi {
        timeout off;
        base "http://other.request-shaping.example.test/";
        values {
            label?: String;
        }


    GET echo(verbose?: bool) -> Bytes using Bytes {
        path ["echo"];
        query {
            "label" = client.label;
            "verbose" = endpoint.verbose;
        }
    }
    PUT put() -> () using NoContent { path ["put"]; }
    PATCH patch() -> () using NoContent { path ["patch"]; }
    DELETE delete() -> () using NoContent { path ["delete"]; }
    HEAD head() -> () using NoContent { path ["head"]; }
    OPTIONS options() -> () using NoContent { path ["options"]; }
    METHOD("TRACE") trace() -> () using NoContent { path ["trace"]; }
    GET r#type() -> () using NoContent { path ["keyword"]; }
}}

api! {
    client TimeoutApi {
        base "http://timeout.request-shaping.example.test/";
        timeout 20ms;


    scope timed() {
        timeout 10ms;
        GET scope_inherited() -> Bytes using Bytes { path ["scope-inherited"]; }
        GET endpoint_override() -> Bytes using Bytes {
            path ["endpoint-override"];
            timeout 200ms;
        }
        GET endpoint_off() -> Bytes using Bytes {
            path ["endpoint-off"];
            timeout off;
        }
    }
}}

api! {
    client OptionalControlApi {
        timeout off;
        base "http://optional-controls.example.test/";
        values {
            client_label?: &'static str = crate::OPTIONAL_CLIENT_DEFAULT;
        }
        query {
            "client_label" = client.client_label;
        }


    scope groups(
        scope_label?: &'static str = crate::OPTIONAL_SCOPE_DEFAULT,
    ) {
        query {
            "scope_label" = groups.scope_label;
        }

        GET inspect(
            endpoint_label?: &'static str = crate::OPTIONAL_ENDPOINT_DEFAULT,
            values?: Vec<&'static str> = crate::OPTIONAL_VALUES_DEFAULT,
        ) -> Bytes using Bytes {
            query {
                "endpoint_label" = endpoint.endpoint_label;
                "value" = each endpoint.values;
            }
        }
    }
}}

api! {
    client TraitDefaultTimingApi {
        timeout off;
        base "http://trait-default-timing.example.test/";
        values {
            #[default]
            scalar: u32;
            #[default]
            optional?: String;
            #[default]
            vector: Vec<String>;
            #[default]
            client_probe: crate::ClientTraitDefaultProbe;
        }


    scope group(#[default] scope_probe: crate::ScopeTraitDefaultProbe) {
        GET read(#[default] endpoint_probe: crate::EndpointTraitDefaultProbe) -> Bytes using Bytes;
    }
}}

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
        .set_tenant("tenant-a".to_owned())
        .with_locale("fr".to_owned())
        .set_client_marker(DropProbe::new("client-explicit", explicit_drops))
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("shape client")
}

fn optional_control_url(
    client: &OptionalControlApi,
    endpoint: &optional_control_api::groups::Inspect,
) -> String {
    client
        .plan(endpoint)
        .expect("optional-control safe plan")
        .observed_url()
        .to_owned()
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
    assert_eq!(missing.input_field(), Some("tenant"));
    assert_eq!(missing.input_kind(), Some(ClientInputKind::Value));

    let before_default = CLIENT_DEFAULT_DROPS.load(Ordering::SeqCst);
    let before_explicit = EXPLICIT_DROPS.load(Ordering::SeqCst);
    let explicit = ShapeApi::builder()
        .set_tenant("tenant".to_owned())
        .set_client_marker(DropProbe::new("explicit", explicit_drops))
        .build()
        .expect("explicit default override");
    let clone = explicit.clone();
    drop(explicit);
    assert_eq!(EXPLICIT_DROPS.load(Ordering::SeqCst), before_explicit);
    drop(clone);
    assert_eq!(EXPLICIT_DROPS.load(Ordering::SeqCst), before_explicit + 1);
    assert_eq!(CLIENT_DEFAULT_DROPS.load(Ordering::SeqCst), before_default);

    let selected = ShapeApi::builder()
        .set_tenant("tenant".to_owned())
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

#[test]
fn defaulted_option_client_clear_and_reset_are_distinct_override_states() {
    let default_client = OptionalControlApi::new().expect("default optional-control client");
    let default_url = optional_control_url(
        &default_client,
        &OptionalControlApi::unbound().groups().inspect(),
    );
    assert!(default_url.contains("client_label=client-default"));

    let selected = OptionalControlApi::builder()
        .with_client_label("client-chosen")
        .build()
        .expect("selected client label");
    let selected_url =
        optional_control_url(&selected, &OptionalControlApi::unbound().groups().inspect());
    assert!(selected_url.contains("client_label=client-chosen"));

    let cleared = OptionalControlApi::builder()
        .clear_client_label()
        .build()
        .expect("explicitly absent client label");
    let cleared_url =
        optional_control_url(&cleared, &OptionalControlApi::unbound().groups().inspect());
    assert!(!cleared_url.contains("client_label="));

    let reset = OptionalControlApi::builder()
        .clear_client_label()
        .reset_client_label()
        .build()
        .expect("reset client label");
    let reset_url = optional_control_url(&reset, &OptionalControlApi::unbound().groups().inspect());
    assert!(reset_url.contains("client_label=client-default"));
}

#[test]
fn trait_default_evaluation_preserves_owner_and_reset_timing() {
    let client_before = CLIENT_TRAIT_DEFAULTS.load(Ordering::SeqCst);
    let scope_before = SCOPE_TRAIT_DEFAULTS.load(Ordering::SeqCst);
    let endpoint_before = ENDPOINT_TRAIT_DEFAULTS.load(Ordering::SeqCst);

    let overridden = TraitDefaultTimingApi::builder()
        .set_scalar(7)
        .with_optional("chosen".to_owned())
        .set_vector(vec!["chosen".to_owned()])
        .set_client_probe(ClientTraitDefaultProbe)
        .build()
        .expect("overridden trait defaults");
    assert_eq!(CLIENT_TRAIT_DEFAULTS.load(Ordering::SeqCst), client_before);
    drop(overridden);

    let reset = TraitDefaultTimingApi::builder()
        .set_scalar(7)
        .reset_scalar()
        .with_optional("chosen".to_owned())
        .clear_optional()
        .reset_optional()
        .set_vector(vec!["chosen".to_owned()])
        .reset_vector()
        .set_client_probe(ClientTraitDefaultProbe)
        .reset_client_probe()
        .build()
        .expect("reset trait defaults");
    assert_eq!(
        CLIENT_TRAIT_DEFAULTS.load(Ordering::SeqCst),
        client_before + 1
    );
    drop(reset);

    let client = TraitDefaultTimingApi::new().expect("trait-default client");
    assert_eq!(
        CLIENT_TRAIT_DEFAULTS.load(Ordering::SeqCst),
        client_before + 2
    );
    let clone = client.clone();
    drop(clone);
    assert_eq!(
        CLIENT_TRAIT_DEFAULTS.load(Ordering::SeqCst),
        client_before + 2
    );

    let scope = client.group();
    assert_eq!(
        SCOPE_TRAIT_DEFAULTS.load(Ordering::SeqCst),
        scope_before + 1
    );
    let scope = scope
        .set_scope_probe(ScopeTraitDefaultProbe)
        .reset_scope_probe();
    assert_eq!(
        SCOPE_TRAIT_DEFAULTS.load(Ordering::SeqCst),
        scope_before + 1
    );

    let request = scope.read();
    assert_eq!(
        ENDPOINT_TRAIT_DEFAULTS.load(Ordering::SeqCst),
        endpoint_before + 1
    );
    let request = request
        .set_endpoint_probe(EndpointTraitDefaultProbe)
        .reset_endpoint_probe();
    assert_eq!(
        ENDPOINT_TRAIT_DEFAULTS.load(Ordering::SeqCst),
        endpoint_before + 1
    );
    drop(request);

    let explicit_scope = TraitDefaultTimingApi::unbound().group();
    assert_eq!(
        SCOPE_TRAIT_DEFAULTS.load(Ordering::SeqCst),
        scope_before + 2
    );
    assert_eq!(
        ENDPOINT_TRAIT_DEFAULTS.load(Ordering::SeqCst),
        endpoint_before + 1
    );
    let explicit_scope = explicit_scope
        .set_scope_probe(ScopeTraitDefaultProbe)
        .reset_scope_probe();
    assert_eq!(
        SCOPE_TRAIT_DEFAULTS.load(Ordering::SeqCst),
        scope_before + 2
    );
    let explicit = explicit_scope.read();
    assert_eq!(
        ENDPOINT_TRAIT_DEFAULTS.load(Ordering::SeqCst),
        endpoint_before + 2
    );
    let explicit = explicit
        .set_endpoint_probe(EndpointTraitDefaultProbe)
        .reset_endpoint_probe();
    assert_eq!(
        SCOPE_TRAIT_DEFAULTS.load(Ordering::SeqCst),
        scope_before + 2
    );
    assert_eq!(
        ENDPOINT_TRAIT_DEFAULTS.load(Ordering::SeqCst),
        endpoint_before + 2
    );
    drop(explicit);
}

#[test]
fn explicit_snapshots_restore_defaulted_option_and_option_vec_values() {
    let client = OptionalControlApi::new().expect("optional-control client");

    let default_url =
        optional_control_url(&client, &OptionalControlApi::unbound().groups().inspect());
    assert!(default_url.contains("scope_label=scope-default"));
    assert!(default_url.contains("endpoint_label=endpoint-default"));
    assert!(!default_url.contains("value="));

    let selected = OptionalControlApi::unbound()
        .groups()
        .with_scope_label("scope-chosen")
        .inspect()
        .with_endpoint_label("endpoint-chosen")
        .with_values(vec!["first", "second"]);
    let selected_url = optional_control_url(&client, &selected);
    assert!(selected_url.contains("scope_label=scope-chosen"));
    assert!(selected_url.contains("endpoint_label=endpoint-chosen"));
    assert!(selected_url.contains("value=first&value=second"));

    let cleared = OptionalControlApi::unbound()
        .groups()
        .clear_scope_label()
        .inspect()
        .clear_endpoint_label()
        .clear_values();
    let cleared_url = optional_control_url(&client, &cleared);
    assert!(!cleared_url.contains("scope_label="));
    assert!(!cleared_url.contains("endpoint_label="));
    assert!(!cleared_url.contains("value="));

    let reset = OptionalControlApi::unbound()
        .groups()
        .clear_scope_label()
        .reset_scope_label()
        .inspect()
        .clear_endpoint_label()
        .reset_endpoint_label()
        .with_values(vec!["discarded"])
        .reset_values();
    let reset_url = optional_control_url(&client, &reset);
    assert!(reset_url.contains("scope_label=scope-default"));
    assert!(reset_url.contains("endpoint_label=endpoint-default"));
    assert!(!reset_url.contains("value="));
}

#[tokio::test(flavor = "current_thread")]
async fn scope_and_request_controls_share_defaulted_option_snapshot_semantics() {
    let _guard = request_shaping_test_guard().await;
    let server = ScriptedServer::start((0..10).map(|_| ResponseScript::ok(b"ok".to_vec())))
        .expect("optional-control server");
    let selected_proxy = proxy(&server);
    let client = OptionalControlApi::builder()
        .configure_http(move |http| http.proxy(selected_proxy))
        .build()
        .expect("proxied optional-control client");

    client
        .groups()
        .inspect()
        .await
        .expect("default scope selection");
    client
        .groups()
        .with_scope_label("scope-chosen")
        .inspect()
        .await
        .expect("selected scope value");
    client
        .groups()
        .clear_scope_label()
        .inspect()
        .await
        .expect("cleared scope value");
    client
        .groups()
        .clear_scope_label()
        .reset_scope_label()
        .inspect()
        .await
        .expect("reset scope value");
    client
        .groups()
        .inspect()
        .with_endpoint_label("endpoint-chosen")
        .await
        .expect("selected request value");
    client
        .groups()
        .inspect()
        .clear_endpoint_label()
        .await
        .expect("cleared request value");
    client
        .groups()
        .inspect()
        .clear_endpoint_label()
        .reset_endpoint_label()
        .await
        .expect("reset request value");
    client
        .groups()
        .inspect()
        .with_values(vec!["first", "second"])
        .await
        .expect("selected request vector");
    client
        .groups()
        .inspect()
        .clear_values()
        .await
        .expect("cleared request vector");
    client
        .groups()
        .inspect()
        .with_values(vec!["discarded"])
        .reset_values()
        .await
        .expect("reset request vector");

    let requests = server.requests();
    assert_eq!(requests.len(), 10);
    assert!(requests[0].target().contains("scope_label=scope-default"));
    assert!(requests[1].target().contains("scope_label=scope-chosen"));
    assert!(!requests[2].target().contains("scope_label="));
    assert!(requests[3].target().contains("scope_label=scope-default"));
    assert!(
        requests[4]
            .target()
            .contains("endpoint_label=endpoint-chosen")
    );
    assert!(!requests[5].target().contains("endpoint_label="));
    assert!(
        requests[6]
            .target()
            .contains("endpoint_label=endpoint-default")
    );
    assert!(requests[7].target().contains("value=first&value=second"));
    assert!(!requests[8].target().contains("value="));
    assert!(!requests[9].target().contains("value="));
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
        .with_label("chosen".to_owned())
        .configure_http(move |http| http.proxy(other_proxy))
        .build()
        .expect("other client");
    let reset_other = OtherApi::builder()
        .with_label("unused".to_owned())
        .clear_label()
        .build()
        .expect("cleared optional client value");
    drop(reset_other);

    let regions = client
        .regions("eu".to_owned())
        .with_language("de".to_owned());
    let users = regions.users("account-a".to_owned());
    let facade = users
        .fetch("42".to_owned())
        .with_verbose(true)
        .set_endpoint_marker(DropProbe::new("temporary", explicit_drops))
        .reset_endpoint_marker()
        .await
        .expect("facade endpoint");
    assert_eq!(facade, b"facade"[..]);

    let explicit = ShapeApi::unbound()
        .regions("eu".to_owned())
        .with_language("de".to_owned())
        .users("account-a".to_owned())
        .fetch("42".to_owned())
        .with_verbose(true);
    let explicit = client.request(explicit).await.expect("explicit endpoint");
    assert_eq!(explicit, b"explicit"[..]);

    let metadata: ApiResponse<Bytes> = client
        .root()
        .timeout(Duration::from_secs(1))
        .with_verbose(true)
        .inherit_timeout()
        .response()
        .await
        .expect("metadata after generated extension setter");
    assert_eq!(metadata.value(), &b"root"[..]);

    let other_value = other
        .echo()
        .with_verbose(true)
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
        .set_scope_marker(DropProbe::new("temporary", explicit_drops))
        .reset_scope_marker();
    drop(source);
    assert_eq!(SCOPE_DEFAULT_DROPS.load(Ordering::SeqCst), scope_before);

    let request: shape_api::regions::users::FetchRequest = reset
        .users("account".to_owned())
        .fetch("id".to_owned())
        .set_endpoint_marker(DropProbe::new("temporary", explicit_drops))
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

    let selected = client.root().with_verbose(true);
    let cleared = client.root().with_verbose(false).clear_verbose();
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
    let source = client
        .regions("eu".to_owned())
        .with_language("fr".to_owned());
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
        .set_tenant("discarded".to_owned())
        .set_tenant("tenant-one".to_owned())
        .with_locale("discarded".to_owned())
        .clear_locale()
        .set_client_marker(DropProbe::new("discarded", explicit_drops))
        .reset_client_marker()
        .configure_http(move |http| http.proxy(first_proxy))
        .build()
        .expect("first client");
    let second_proxy = proxy(&server);
    let second = ShapeApi::builder()
        .set_tenant("tenant-two".to_owned())
        .with_locale("discarded".to_owned())
        .with_locale("es".to_owned())
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
        .chain((0..7).map(|_| ResponseScript::status(204, Vec::new()))),
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
            .with_filters(vec!["x".to_owned(), "y".to_owned()])
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
    other.r#type().await.expect("raw-keyword alias");

    let requests = server.requests();
    assert_eq!(requests.len(), 9);
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
        ["PUT", "PATCH", "DELETE", "HEAD", "OPTIONS", "TRACE", "GET"]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn response_accept_and_body_specs_own_media_headers() {
    let _guard = request_shaping_test_guard().await;
    let server = ScriptedServer::start([
        ResponseScript::ok(b"accepted".to_vec()),
        ResponseScript::Chunked {
            status: 200,
            headers: vec![(
                "Content-Type".to_owned(),
                "application/octet-stream".to_owned(),
            )],
            chunks: vec![b"streamed".to_vec()],
        },
        ResponseScript::ok(b"text-body".to_vec()),
        ResponseScript::ok(b"stream-body".to_vec()),
        ResponseScript::status(204, Vec::new()),
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
    let mut streamed: ByteStream = client
        .streamed()
        .with_verbose(true)
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

    let text = client
        .text_body("body".to_owned())
        .await
        .expect("Text body specification");
    assert_eq!(text, b"text-body"[..]);

    let stream = client
        .stream_body(StreamBody::from_bytes(Bytes::from_static(b"stream")))
        .await
        .expect("stream body specification");
    assert_eq!(stream, b"stream-body"[..]);

    let multipart = MultipartBody::new()
        .part(
            "field",
            MultipartPart::text("value").expect("bounded multipart text"),
        )
        .expect("multipart recipe");
    client
        .multipart_body(multipart)
        .await
        .expect("multipart body specification");

    let requests = server.requests();
    assert_eq!(requests.len(), 5);
    assert_eq!(
        requests[0].header("accept"),
        Some("application/vnd.concord.request-shaping")
    );
    assert_eq!(
        requests[2].header("content-type"),
        Some("text/plain; charset=utf-8")
    );
    assert_eq!(
        requests[3].header("content-type"),
        Some("application/octet-stream")
    );
    assert!(
        requests[4]
            .header("content-type")
            .is_some_and(|value| value.starts_with("multipart/form-data; boundary="))
    );
    assert!(
        requests[1]
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
async fn generated_nested_formats_stop_at_the_core_bound_and_return_structured_errors() {
    let _guard = request_shaping_test_guard().await;
    let server = ScriptedServer::start([]).expect("bounded format trap");
    let client = shaped_client(&server);
    let oversized = "x".repeat(MAX_FORMATTED_PUBLIC_VALUE_BYTES + 1);

    let first_calls = Arc::new(AtomicUsize::new(0));
    let tail_calls = Arc::new(AtomicUsize::new(0));
    let error = client
        .bounded_format_path(
            FormatProbe::output(oversized.clone(), Arc::clone(&first_calls)),
            FormatProbe::panic(Arc::clone(&tail_calls)),
        )
        .await
        .expect_err("formatted path first excess");
    assert_eq!(
        error.configuration_kind(),
        Some(ConfigurationErrorKind::InvalidRoute)
    );
    assert_eq!(first_calls.load(Ordering::SeqCst), 1);
    assert_eq!(tail_calls.load(Ordering::SeqCst), 0);

    let first_calls = Arc::new(AtomicUsize::new(0));
    let tail_calls = Arc::new(AtomicUsize::new(0));
    let error = client
        .bounded_format_host(
            FormatProbe::output(oversized.clone(), Arc::clone(&first_calls)),
            FormatProbe::panic(Arc::clone(&tail_calls)),
        )
        .await
        .expect_err("formatted host first excess");
    assert_eq!(
        error.configuration_kind(),
        Some(ConfigurationErrorKind::InvalidRoute)
    );
    assert_eq!(first_calls.load(Ordering::SeqCst), 1);
    assert_eq!(tail_calls.load(Ordering::SeqCst), 0);

    let first_calls = Arc::new(AtomicUsize::new(0));
    let tail_calls = Arc::new(AtomicUsize::new(0));
    let error = client
        .bounded_format_query(
            FormatProbe::output(oversized.clone(), Arc::clone(&first_calls)),
            FormatProbe::panic(Arc::clone(&tail_calls)),
        )
        .await
        .expect_err("formatted query first excess");
    assert_eq!(
        error.configuration_kind(),
        Some(ConfigurationErrorKind::InvalidRoute)
    );
    assert_eq!(first_calls.load(Ordering::SeqCst), 1);
    assert_eq!(tail_calls.load(Ordering::SeqCst), 0);

    let first_calls = Arc::new(AtomicUsize::new(0));
    let tail_calls = Arc::new(AtomicUsize::new(0));
    let error = client
        .bounded_format_header(
            FormatProbe::output(oversized, Arc::clone(&first_calls)),
            FormatProbe::panic(Arc::clone(&tail_calls)),
        )
        .await
        .expect_err("formatted header first excess");
    assert_eq!(
        error.configuration_kind(),
        Some(ConfigurationErrorKind::InvalidHeader)
    );
    assert_eq!(first_calls.load(Ordering::SeqCst), 1);
    assert_eq!(tail_calls.load(Ordering::SeqCst), 0);

    let error = client
        .failing_format(FailingFormatProbe)
        .await
        .expect_err("Display failure must not panic");
    assert_eq!(
        error.configuration_kind(),
        Some(ConfigurationErrorKind::InvalidRoute)
    );
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
            let endpoint = RuntimeEndpoint::builder(
                EndpointId::try_from("timeout_api.runtime").expect("valid runtime endpoint ID"),
                Method::GET,
            )
            .accepted_bytes()
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
