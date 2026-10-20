//! Values, initialization, snapshots, routes, DNS and deferred public
//! shaping of generated requests (§4, §5, §6.3, §8.1, §9.2).
//!
//! Symbolic patch programs, adapter suppression and privacy live in
//! `shaping/programs.rs`; plan side effects and bodies in `shaping/bodies.rs`.

use std::fmt::{self, Display, Formatter};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use concord::api;
use concord::body::RequestStream;
use concord::endpoint::{EndpointId, MAX_FORMATTED_PUBLIC_VALUE_BYTES, RuntimeEndpoint};
use concord::error::{
    ClientBuildErrorKind, ClientInputKind, ConfigurationErrorKind, ErrorCategory,
};
use concord::http::Method;
use concord::response::{ApiResponse, ByteStream};
use concord::{Binary, Bytes, Empty, Text};
use concord_test_support::server::{BodyGate, ResponseScript, ScriptedServer};
use futures_core::Stream as FuturesStream;
use tokio::sync::{Mutex, MutexGuard};

use crate::support::loopback_proxy;

mod bodies;
mod programs;

static CLIENT_DEFAULT_DROPS: AtomicUsize = AtomicUsize::new(0);
static SCOPE_DEFAULT_DROPS: AtomicUsize = AtomicUsize::new(0);
static ENDPOINT_DEFAULT_DROPS: AtomicUsize = AtomicUsize::new(0);
static EXPLICIT_DROPS: AtomicUsize = AtomicUsize::new(0);
static SHAPING_FORMATS: AtomicUsize = AtomicUsize::new(0);
static CLIENT_TRAIT_DEFAULTS: AtomicUsize = AtomicUsize::new(0);
static SCOPE_TRAIT_DEFAULTS: AtomicUsize = AtomicUsize::new(0);
static ENDPOINT_TRAIT_DEFAULTS: AtomicUsize = AtomicUsize::new(0);
static REQUEST_SHAPING_TEST_LOCK: Mutex<()> = Mutex::const_new(());
static TRAIT_DEFAULT_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

async fn request_shaping_test_guard() -> MutexGuard<'static, ()> {
    REQUEST_SHAPING_TEST_LOCK.lock().await
}

fn trait_default_test_guard() -> std::sync::MutexGuard<'static, ()> {
    TRAIT_DEFAULT_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A non-`Clone` ordinary payload that counts its drops.
pub struct DropProbe {
    value: &'static str,
    drops: fn() -> &'static AtomicUsize,
}

impl DropProbe {
    const fn new(value: &'static str, drops: fn() -> &'static AtomicUsize) -> Self {
        Self { value, drops }
    }
}

impl concord::RequestAtom for DropProbe {
    fn write_atom(
        &self,
        output: &mut concord::BoundedAtomWriter<'_>,
    ) -> Result<(), concord::AtomError> {
        output.push_str(self.value)
    }
}

impl Drop for DropProbe {
    fn drop(&mut self) {
        (self.drops)().fetch_add(1, Ordering::SeqCst);
    }
}

pub struct ShapeProbe(&'static str);

impl concord::RequestAtom for ShapeProbe {
    fn write_atom(
        &self,
        output: &mut concord::BoundedAtomWriter<'_>,
    ) -> Result<(), concord::AtomError> {
        SHAPING_FORMATS.fetch_add(1, Ordering::SeqCst);
        output.push_str(self.0)
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

impl concord::RequestAtom for FormatProbe {
    fn write_atom(
        &self,
        output: &mut concord::BoundedAtomWriter<'_>,
    ) -> Result<(), concord::AtomError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert!(!self.panic_if_formatted, "later atom value was inspected");
        output.push_str(&self.output)
    }
}

pub struct FailingFormatProbe;

impl concord::RequestAtom for FailingFormatProbe {
    fn write_atom(
        &self,
        _output: &mut concord::BoundedAtomWriter<'_>,
    ) -> Result<(), concord::AtomError> {
        Err(concord::AtomError::new(concord::AtomErrorKind::Encode))
    }
}

macro_rules! counted_default_probe {
    ($name:ident, $counter:ident, $text:literal) => {
        /// A non-`Clone` payload whose `Default` counts evaluations.
        pub struct $name;

        impl Default for $name {
            fn default() -> Self {
                $counter.fetch_add(1, Ordering::SeqCst);
                Self
            }
        }

        impl concord::RequestAtom for $name {
            fn write_atom(
                &self,
                output: &mut concord::BoundedAtomWriter<'_>,
            ) -> Result<(), concord::AtomError> {
                output.push_str($text)
            }
        }
    };
}

counted_default_probe!(
    ClientTraitDefaultProbe,
    CLIENT_TRAIT_DEFAULTS,
    "client-probe"
);
counted_default_probe!(ScopeTraitDefaultProbe, SCOPE_TRAIT_DEFAULTS, "scope-probe");
counted_default_probe!(
    EndpointTraitDefaultProbe,
    ENDPOINT_TRAIT_DEFAULTS,
    "endpoint-probe"
);

crate::assert_not_impl!(DropProbe: Clone);
crate::assert_not_impl!(ClientTraitDefaultProbe: Clone);

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

/// An opaque alias: its role is that of `String` without token recognition.
pub type Locale = String;

api! {
    client ShapeApi {
        base "http://request-shaping.example.test:4317/fixed/";
        timeout 5s;

        values {
            tenant: String;
            locale?: String;
            client_marker: DropProbe = CLIENT_DEFAULT;
        }

        subdomain ["api"];
        path "v1";
        query {
            "tenant" = tenant;
            "locale" = locale;
        }
        headers {
            "X-Client-Marker" = client_marker;
        }

        GET root(verbose?: bool) -> Bytes via Binary {
            path "root";
            query { "verbose" = verbose; }
        }

        GET search(tags: Vec<String>, filters?: Vec<String>) -> Bytes via Binary {
            path "search";
            query {
                "tag" = each tags;
                "filter" = each filters;
            }
        }

        GET accept() -> String via Text {
            path "accept";
        }

        POST text_body(body input: String via Text) -> Bytes via Binary {
            path "text-body";
        }

        POST stream_body(body input: RequestStream via Binary) -> Bytes via Binary {
            path "stream-body";
        }

        GET streamed(verbose?: bool) -> ByteStream via Binary {
            path "streamed";
            query { "verbose" = verbose; }
        }

        GET unsafe_path(segment: ShapeProbe) -> Bytes via Binary {
            path "unsafe-path/{segment}";
        }

        GET unsafe_header(value: ShapeProbe) -> Bytes via Binary {
            path "unsafe-header";
            headers { "X-Dynamic" = value; }
        }

        GET bounded_format_path(first: FormatProbe, tail: FormatProbe) -> Bytes via Binary {
            path "{first}{tail}";
        }

        GET bounded_format_query(first: FormatProbe, tail: FormatProbe) -> Bytes via Binary {
            query { "formatted" = concat [concat [first], concat [tail]]; }
        }

        GET bounded_format_header(first: FormatProbe, tail: FormatProbe) -> Bytes via Binary {
            headers { "X-Formatted" = concat [concat [first], concat [tail]]; }
        }

        GET failing_format(value: FailingFormatProbe) -> Bytes via Binary {
            path "prefix-{value}";
        }

        // ORIGIN-02: an endpoint cannot own a subdomain clause, so the v2
        // endpoint-level host probes are scope-owned in v3.
        scope unsafe_hosts(label: ShapeProbe) {
            subdomain replace [label];

            GET unsafe_host() -> Bytes via Binary {
                path "unsafe-host";
            }
        }

        scope bounded_hosts(first: FormatProbe, tail: FormatProbe) {
            subdomain replace [concat [concat [first], concat [tail]]];

            GET bounded_format_host() -> Bytes via Binary;
        }

        scope regions(
            region: String,
            language?: String,
            scope_marker: DropProbe = SCOPE_DEFAULT,
        ) {
            subdomain replace [region, "api"];
            path "regions/{region}";
            query {
                "language" = language;
                "scope_marker" = scope_marker;
            }
            headers { "X-Region" = region; }

            scope users(account: String) {
                path "users/{account}";
                headers { "X-Account" = account; }

                METHOD("PROPFIND") fetch(
                    id: String,
                    verbose?: bool,
                    endpoint_marker: DropProbe = ENDPOINT_DEFAULT,
                ) -> Bytes via Binary {
                    path "id-{id}";
                    query {
                        "verbose" = verbose;
                        "endpoint_marker" = endpoint_marker;
                    }
                    headers { "X-Endpoint-Marker" = endpoint_marker; }
                    timeout 2s;
                }
            }
        }
    }
}

api! {
    client OtherApi {
        base "http://other.request-shaping.example.test/";
        timeout 30s;

        values {
            label?: String;
        }

        GET echo(verbose?: bool) -> Bytes via Binary {
            path "echo";
            query {
                "label" = client.label;
                "verbose" = verbose;
            }
        }
        PUT put() -> () via Empty { path "put"; }
        PATCH patch() -> () via Empty { path "patch"; }
        DELETE delete() -> () via Empty { path "delete"; }
        HEAD head() -> () via Empty { path "head"; }
        OPTIONS options() -> () via Empty { path "options"; }
        METHOD("PROPFIND") propfind() -> () via Empty { path "propfind"; }
        GET r#type() -> () via Empty { path "keyword"; }
    }
}

api! {
    client TimeoutApi {
        base "http://timeout.request-shaping.example.test/";
        timeout 20ms;

        scope timed() {
            timeout 10ms;

            GET scope_inherited() -> Bytes via Binary { path "scope-inherited"; }
            GET endpoint_override() -> Bytes via Binary {
                path "endpoint-override";
                timeout 200ms;
            }
            GET endpoint_long() -> Bytes via Binary {
                path "endpoint-long";
                timeout 30s;
            }
        }
    }
}

api! {
    client OptionalControlApi {
        base "http://optional-controls.example.test/";
        timeout 30s;

        values {
            client_label?: &'static str = OPTIONAL_CLIENT_DEFAULT;
        }
        query {
            "client_label" = client_label;
        }

        scope groups(
            scope_label?: &'static str = OPTIONAL_SCOPE_DEFAULT,
        ) {
            query {
                "scope_label" = scope_label;
            }

            GET inspect(
                endpoint_label?: &'static str = OPTIONAL_ENDPOINT_DEFAULT,
                values?: Vec<&'static str> = OPTIONAL_VALUES_DEFAULT,
            ) -> Bytes via Binary {
                query {
                    "endpoint_label" = endpoint_label;
                    "value" = each values;
                }
            }
        }
    }
}

api! {
    client TraitDefaultTimingApi {
        base "http://trait-default-timing.example.test/";
        timeout 30s;

        values {
            #[default]
            query scalar: u32;
            #[default]
            query optional?: String;
            #[default]
            query("vector") each vector: Vec<String>;
            #[default]
            header("X-Client-Probe") client_probe: ClientTraitDefaultProbe;
            query locale: Locale = "en";
            #[default]
            query("tag") each tags?: Vec<String>;
        }

        scope group(#[default] header("X-Scope-Probe") scope_probe: ScopeTraitDefaultProbe) {
            GET read(
                #[default] header("X-Endpoint-Probe") endpoint_probe: EndpointTraitDefaultProbe,
            ) -> Bytes via Binary;
        }
    }
}

api! {
    client DnsApi {
        base "http://dns.example.test/root";
        timeout 5s;

        scope tenant(label: String) {
            subdomain [label, "api"];

            GET get() -> Bytes via Binary { path "get"; }
        }

        scope qualified(label: String) {
            scope inner() {
                subdomain replace [qualified.label];

                GET get() -> Bytes via Binary;
            }
        }

        scope deep(a: String, b: String, c: String, d: String) {
            subdomain [a, b, c, d];

            GET get() -> Bytes via Binary;
        }
    }
}

/// Uppercases a path value: a per-hole scalar adapter (§5.3).
pub struct Upper;

impl concord::AtomEncoder<String> for Upper {
    fn write(
        value: &String,
        out: &mut concord::BoundedAtomWriter<'_>,
    ) -> Result<(), concord::AtomError> {
        out.push_str(&value.to_ascii_uppercase())
    }
}

api! {
    client PathApi {
        base "http://path.example.test/root";
        timeout 5s;

        values {
            version: String = "v1";
        }

        scope regional(region: String) {
            path "regions/{region}";

            GET file(name: String, query q?: String) -> Bytes via Binary {
                path "{{literal}}/{name via Upper}/{client.version}-{regional.region}/100%";
            }

            GET replaced(name: String) -> Bytes via Binary {
                path replace "only/{name}";
            }

            GET off() -> Bytes via Binary {
                path off;
            }
        }
    }
}

api! {
    client ConversionApi {
        base "http://conversions.example.test/";
        timeout 5s;

        values {
            name: String;
            locale?: Locale;
        }
        query {
            "name" = name;
            "locale" = locale;
        }

        GET read(count: u64, tags: Vec<String>, offset?: i16) -> Bytes via Binary {
            query {
                "count" = count;
                "tag" = each tags;
                "offset" = offset;
            }
        }
    }
}

api! {
    client LivenessApi {
        base "http://liveness.example.test/";
        timeout 5s;

        values {
            #[deprecated(note = "no longer sent")]
            legacy?: String;
            used: String = "u";
        }
        query { "used" = used; }

        GET read() -> Bytes via Binary;
    }
}

async fn wait_for_requests(server: Arc<ScriptedServer>, expected: usize) {
    tokio::task::spawn_blocking(move || server.wait_for_requests(expected, Duration::from_secs(3)))
        .await
        .expect("request barrier task")
        .expect("request barrier");
}

fn shaped_client(server: &ScriptedServer) -> ShapeApi {
    let proxy = loopback_proxy(server);
    ShapeApi::builder()
        .set_tenant("tenant-a")
        .set_locale("fr")
        .set_client_marker(DropProbe::new("client-explicit", explicit_drops))
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("shape client")
}

fn optional_control_url(
    client: &OptionalControlApi,
    endpoint: &optional_control_api::groups::InspectEndpoint,
) -> String {
    client
        .plan(endpoint)
        .expect("optional-control safe plan")
        .observed_url()
        .to_owned()
}

async fn next_stream_chunk(response: &mut ByteStream) -> Option<Result<Bytes, concord::ApiError>> {
    std::future::poll_fn(|context| Pin::new(&mut *response).poll_next(context)).await
}

/// V3-L07 (root boundary): the retained root initializer is evaluated once
/// when the builder is created — even when a later explicit value replaces
/// it — and `build` does not evaluate it again (§4.2). v2 skipped evaluation
/// of overridden defaults; the v3 builder owns one retained initial payload
/// that is released when the builder is consumed.
#[tokio::test(flavor = "current_thread")]
async fn required_optional_defaulted_client_values_and_missing_input_are_stable() {
    let _guard = request_shaping_test_guard().await;
    let missing = ShapeApi::builder()
        .build()
        .expect_err("required tenant must be absent");
    assert_eq!(missing.kind(), ClientBuildErrorKind::MissingInput);
    assert_eq!(missing.input_field(), Some("set_tenant"));
    assert_eq!(missing.input_kind(), Some(ClientInputKind::Value));
    assert_eq!(
        missing.missing_fields(),
        [("set_tenant", ClientInputKind::Value)]
    );

    let before_default = CLIENT_DEFAULT_DROPS.load(Ordering::SeqCst);
    let before_explicit = EXPLICIT_DROPS.load(Ordering::SeqCst);
    let explicit = ShapeApi::builder()
        .set_tenant("tenant")
        .set_client_marker(DropProbe::new("explicit", explicit_drops))
        .build()
        .expect("explicit default override");
    assert_eq!(
        CLIENT_DEFAULT_DROPS.load(Ordering::SeqCst),
        before_default + 1,
        "the builder's retained default is released once by build"
    );
    let clone = explicit.clone();
    drop(explicit);
    assert_eq!(EXPLICIT_DROPS.load(Ordering::SeqCst), before_explicit);
    drop(clone);
    assert_eq!(EXPLICIT_DROPS.load(Ordering::SeqCst), before_explicit + 1);
    assert_eq!(
        CLIENT_DEFAULT_DROPS.load(Ordering::SeqCst),
        before_default + 1
    );

    let selected = ShapeApi::builder()
        .set_tenant("tenant")
        .build()
        .expect("named client builder");
    assert_eq!(
        CLIENT_DEFAULT_DROPS.load(Ordering::SeqCst),
        before_default + 1,
        "a selected default is shared into the client snapshot, not copied"
    );
    drop(selected);
    assert_eq!(
        CLIENT_DEFAULT_DROPS.load(Ordering::SeqCst),
        before_default + 2
    );

    let defaults_only = OtherApi::new().expect("optional/defaulted inputs preserve zero-input new");
    drop(defaults_only);
}

#[test]
fn defaulted_option_client_omit_and_reset_are_distinct_override_states() {
    let default_client = OptionalControlApi::new().expect("default optional-control client");
    let default_url = optional_control_url(
        &default_client,
        &OptionalControlApi::unbound().groups().inspect(),
    );
    assert!(default_url.contains("client_label=client-default"));

    let selected = OptionalControlApi::builder()
        .set_client_label("client-chosen")
        .build()
        .expect("selected client label");
    let selected_url =
        optional_control_url(&selected, &OptionalControlApi::unbound().groups().inspect());
    assert!(selected_url.contains("client_label=client-chosen"));

    let cleared = OptionalControlApi::builder()
        .omit_client_label()
        .build()
        .expect("explicitly absent client label");
    let cleared_url =
        optional_control_url(&cleared, &OptionalControlApi::unbound().groups().inspect());
    assert!(!cleared_url.contains("client_label="));

    let replaced = OptionalControlApi::builder()
        .replace_client_label(None)
        .build()
        .expect("exact absent client label");
    let replaced_url =
        optional_control_url(&replaced, &OptionalControlApi::unbound().groups().inspect());
    assert!(!replaced_url.contains("client_label="));

    let reset = OptionalControlApi::builder()
        .omit_client_label()
        .reset_client_label()
        .build()
        .expect("reset client label");
    let reset_url = optional_control_url(&reset, &OptionalControlApi::unbound().groups().inspect());
    assert!(reset_url.contains("client_label=client-default"));
}

/// V3-L07: every default initializer runs exactly once at its owner boundary
/// (root builder, scope selection, endpoint construction); set/reset,
/// client clone and unbound binding never re-evaluate it (§4.2, VALUE-02).
#[test]
fn trait_default_evaluation_preserves_owner_and_reset_timing() {
    let _guard = trait_default_test_guard();
    let client_before = CLIENT_TRAIT_DEFAULTS.load(Ordering::SeqCst);
    let scope_before = SCOPE_TRAIT_DEFAULTS.load(Ordering::SeqCst);
    let endpoint_before = ENDPOINT_TRAIT_DEFAULTS.load(Ordering::SeqCst);

    let overridden = TraitDefaultTimingApi::builder()
        .set_scalar(7)
        .set_optional("chosen")
        .set_vector(vec!["chosen".to_owned()])
        .set_client_probe(ClientTraitDefaultProbe)
        .build()
        .expect("overridden trait defaults");
    assert_eq!(
        CLIENT_TRAIT_DEFAULTS.load(Ordering::SeqCst),
        client_before + 1,
        "the root default is evaluated once when the builder is created"
    );
    drop(overridden);

    let reset = TraitDefaultTimingApi::builder()
        .set_scalar(7)
        .reset_scalar()
        .set_optional("chosen")
        .omit_optional()
        .reset_optional()
        .set_vector(vec!["chosen".to_owned()])
        .reset_vector()
        .set_client_probe(ClientTraitDefaultProbe)
        .reset_client_probe()
        .build()
        .expect("reset trait defaults");
    assert_eq!(
        CLIENT_TRAIT_DEFAULTS.load(Ordering::SeqCst),
        client_before + 2,
        "reset restores the retained payload without re-running Default"
    );
    drop(reset);

    let client = TraitDefaultTimingApi::new().expect("trait-default client");
    assert_eq!(
        CLIENT_TRAIT_DEFAULTS.load(Ordering::SeqCst),
        client_before + 3
    );
    let clone = client.clone();
    drop(clone);
    assert_eq!(
        CLIENT_TRAIT_DEFAULTS.load(Ordering::SeqCst),
        client_before + 3
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
    let planned = request.plan().expect("trait-default plan");
    let headers = format!("{:?}", planned.public_headers());
    for probe in ["client-probe", "scope-probe", "endpoint-probe"] {
        assert!(headers.contains(probe), "{probe}: {headers}");
    }
    let cloned_request = request.clone();
    drop(request);
    drop(cloned_request);

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
    let _ = client.plan(&explicit).expect("unbound plan");
    let bound = client.request(explicit);
    let _ = bound.plan().expect("bound plan");
    assert_eq!(
        SCOPE_TRAIT_DEFAULTS.load(Ordering::SeqCst),
        scope_before + 2,
        "binding does not re-evaluate captured scope defaults"
    );
    assert_eq!(
        ENDPOINT_TRAIT_DEFAULTS.load(Ordering::SeqCst),
        endpoint_before + 2,
        "binding does not re-evaluate captured endpoint defaults"
    );
    assert_eq!(
        CLIENT_TRAIT_DEFAULTS.load(Ordering::SeqCst),
        client_before + 3,
        "binding uses the built root snapshot"
    );
    drop(bound);
}

/// V3-L08: a string-literal initializer through the closed conversion set,
/// a defaulted optional empty vector (present, zero occurrences), an opaque
/// alias and non-`Clone` snapshot payloads.
#[test]
fn string_literal_defaults_empty_vectors_and_aliases_are_ordinary_values() {
    let _guard = trait_default_test_guard();
    let client = TraitDefaultTimingApi::new().expect("trait-default client");
    let plan = client.group().read().plan().expect("default plan");
    assert_eq!(
        plan.observed_url(),
        "http://trait-default-timing.example.test/?scalar=0&optional=&locale=en"
    );

    let selected = TraitDefaultTimingApi::builder()
        .set_locale(String::from("de"))
        .set_tags(vec!["a".to_owned(), "b".to_owned()])
        .set_vector(Vec::new())
        .build()
        .expect("selected client");
    assert_eq!(
        selected
            .group()
            .read()
            .plan()
            .expect("selected plan")
            .observed_url(),
        "http://trait-default-timing.example.test/?scalar=0&optional=&locale=de&tag=a&tag=b"
    );

    let contract = TraitDefaultTimingApi::contract();
    let locale = contract.client().value("locale").expect("locale value");
    assert_eq!(locale.payload_type(), "Locale");
    assert_eq!(
        locale.initializer(),
        concord::inspection::ValueInitializer::StableValue(
            concord::inspection::InitializerExpression::Literal("\"en\"")
        )
    );
    let tags = contract.client().value("tags").expect("tags value");
    assert_eq!(
        tags.presence(),
        concord::inspection::ValuePresence::Optional
    );
    assert_eq!(
        tags.initializer(),
        concord::inspection::ValueInitializer::TraitDefault
    );
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
        .set_scope_label("scope-chosen")
        .inspect()
        .set_endpoint_label("endpoint-chosen")
        .set_values(vec!["first", "second"]);
    let selected_url = optional_control_url(&client, &selected);
    assert!(selected_url.contains("scope_label=scope-chosen"));
    assert!(selected_url.contains("endpoint_label=endpoint-chosen"));
    assert!(selected_url.contains("value=first&value=second"));

    let cleared = OptionalControlApi::unbound()
        .groups()
        .omit_scope_label()
        .inspect()
        .omit_endpoint_label()
        .omit_values();
    let cleared_url = optional_control_url(&client, &cleared);
    assert!(!cleared_url.contains("scope_label="));
    assert!(!cleared_url.contains("endpoint_label="));
    assert!(!cleared_url.contains("value="));

    let reset = OptionalControlApi::unbound()
        .groups()
        .omit_scope_label()
        .reset_scope_label()
        .inspect()
        .omit_endpoint_label()
        .reset_endpoint_label()
        .set_values(vec!["discarded"])
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
    let selected_proxy = loopback_proxy(&server);
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
        .set_scope_label("scope-chosen")
        .inspect()
        .await
        .expect("selected scope value");
    client
        .groups()
        .omit_scope_label()
        .inspect()
        .await
        .expect("cleared scope value");
    client
        .groups()
        .omit_scope_label()
        .reset_scope_label()
        .inspect()
        .await
        .expect("reset scope value");
    client
        .groups()
        .inspect()
        .set_endpoint_label("endpoint-chosen")
        .await
        .expect("selected request value");
    client
        .groups()
        .inspect()
        .omit_endpoint_label()
        .await
        .expect("cleared request value");
    client
        .groups()
        .inspect()
        .omit_endpoint_label()
        .reset_endpoint_label()
        .await
        .expect("reset request value");
    client
        .groups()
        .inspect()
        .set_values(vec!["first", "second"])
        .await
        .expect("selected request vector");
    client
        .groups()
        .inspect()
        .omit_values()
        .await
        .expect("cleared request vector");
    client
        .groups()
        .inspect()
        .set_values(vec!["discarded"])
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
    let other_proxy = loopback_proxy(&server);
    let other = OtherApi::builder()
        .set_label("chosen")
        .configure_http(move |http| http.proxy(other_proxy))
        .build()
        .expect("other client");
    let reset_other = OtherApi::builder()
        .set_label("unused")
        .omit_label()
        .build()
        .expect("cleared optional client value");
    drop(reset_other);

    let regions = client.regions("eu").set_language("de");
    let users = regions.users("account-a");
    let facade = users
        .fetch("42")
        .set_verbose(true)
        .set_endpoint_marker(DropProbe::new("temporary", explicit_drops))
        .reset_endpoint_marker()
        .await
        .expect("facade endpoint");
    assert_eq!(facade, b"facade"[..]);

    let explicit = ShapeApi::unbound()
        .regions("eu")
        .set_language("de")
        .users("account-a")
        .fetch("42")
        .set_verbose(true);
    let explicit = client.request(explicit).await.expect("explicit endpoint");
    assert_eq!(explicit, b"explicit"[..]);

    let metadata: ApiResponse<Bytes> = client
        .root()
        .timeout(Duration::from_secs(1))
        .expect("finite timeout")
        .set_verbose(true)
        .reset_timeout()
        .response()
        .await
        .expect("metadata after generated extension setter");
    assert_eq!(metadata.value(), &b"root"[..]);

    let other_value = other
        .echo()
        .set_verbose(true)
        .await
        .expect("second-client setter");
    assert_eq!(other_value, b"other"[..]);

    let requests = server.requests();
    assert_eq!(requests.len(), 4);
    assert_eq!(requests[0].method(), "PROPFIND");
    assert_eq!(requests[0].target(), requests[1].target());
    assert!(requests[0].target().contains(
        "http://eu.api.request-shaping.example.test:4317/fixed/v1/regions/eu/users/account-a/id-42?tenant=tenant-a&locale=fr&language=de&scope_marker=scope-default&verbose=true&endpoint_marker=endpoint-default"
    ), "{}", requests[0].target());
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

    let facade = client.regions("eu");
    let source = facade.clone();
    let reset = facade
        .set_scope_marker(DropProbe::new("temporary", explicit_drops))
        .reset_scope_marker();
    drop(source);
    assert_eq!(SCOPE_DEFAULT_DROPS.load(Ordering::SeqCst), scope_before);

    let request: shape_api::regions::users::FetchRequest = reset
        .users("account")
        .fetch("id")
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

    let selected = client.root().set_verbose(true);
    let cleared = client.root().set_verbose(false).omit_verbose();
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
    let source = client.regions("eu").set_language("fr");
    let cleared = source.clone().omit_language();

    assert_eq!(
        source
            .users("account")
            .fetch("id")
            .await
            .expect("source facade"),
        b"source"[..]
    );
    assert_eq!(
        cleared
            .users("account")
            .fetch("id")
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
    let first_proxy = loopback_proxy(&server);
    let first = ShapeApi::builder()
        .set_tenant("discarded")
        .set_tenant("tenant-one")
        .set_locale("discarded")
        .omit_locale()
        .set_client_marker(DropProbe::new("discarded", explicit_drops))
        .reset_client_marker()
        .configure_http(move |http| http.proxy(first_proxy))
        .build()
        .expect("first client");
    let second_proxy = loopback_proxy(&server);
    let second = ShapeApi::builder()
        .set_tenant("tenant-two")
        .set_locale("discarded")
        .set_locale("es")
        .configure_http(move |http| http.proxy(second_proxy))
        .build()
        .expect("second client");

    assert_eq!(first.root().await.expect("first request"), b"first"[..]);
    assert_eq!(second.root().await.expect("second request"), b"second"[..]);
    let requests = server.requests();
    assert!(requests[0].target().contains("tenant=tenant-one"));
    assert!(!requests[0].target().contains("locale="));
    assert_eq!(
        requests[0].header("x-client-marker"),
        Some("client-default")
    );
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
    let other_proxy = loopback_proxy(&server);
    let other = OtherApi::builder()
        .configure_http(move |http| http.proxy(other_proxy))
        .build()
        .expect("method client");

    assert_eq!(
        client
            .search(vec!["one two".to_owned(), "three".to_owned()])
            .set_filters(vec!["x".to_owned(), "y".to_owned()])
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
    other.propfind().await.expect("PROPFIND");
    other.r#type().await.expect("raw-keyword endpoint");

    let requests = server.requests();
    assert_eq!(requests.len(), 9);
    assert!(
        requests[0].target().contains(
            "/fixed/v1/search?tenant=tenant-a&locale=fr&tag=one+two&tag=three&filter=x&filter=y"
        ),
        "{}",
        requests[0].target()
    );
    assert!(
        requests[1]
            .target()
            .ends_with("/fixed/v1/search?tenant=tenant-a&locale=fr")
    );
    assert_eq!(
        requests[2..]
            .iter()
            .map(|request| request.method())
            .collect::<Vec<_>>(),
        [
            "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS", "PROPFIND", "GET"
        ]
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
    ])
    .expect("media server");
    let client = shaped_client(&server);

    assert_eq!(
        client.accept().await.expect("representation-owned Accept"),
        "accepted"
    );
    let mut streamed: ByteStream = client
        .streamed()
        .set_verbose(true)
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
        .stream_body(RequestStream::from_bytes(Bytes::from_static(b"stream")))
        .await
        .expect("stream body specification");
    assert_eq!(stream, b"stream-body"[..]);

    let requests = server.requests();
    assert_eq!(requests.len(), 4);
    assert_eq!(requests[0].header("accept"), Some("text/plain"));
    assert_eq!(
        requests[2].header("content-type"),
        Some("text/plain; charset=utf-8")
    );
    assert_eq!(requests[2].body(), b"body");
    assert_eq!(
        requests[3].header("content-type"),
        Some("application/octet-stream")
    );
    assert_eq!(requests[3].body(), b"stream");
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
    let host = client.unsafe_hosts(ShapeProbe("two.labels")).unsafe_host();
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
    for secret in ["two/segments", "two.labels", "bad\r\nvalue"] {
        assert!(!format!("{error:?} {error}").contains(secret));
    }
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
        .bounded_hosts(
            FormatProbe::output(oversized.clone(), Arc::clone(&first_calls)),
            FormatProbe::panic(Arc::clone(&tail_calls)),
        )
        .bounded_format_host()
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
        .expect_err("atom failure must not panic");
    assert_eq!(
        error.configuration_kind(),
        Some(ConfigurationErrorKind::InvalidRoute)
    );
    assert_eq!(server.request_count(), 0);
}

#[tokio::test(flavor = "current_thread")]
async fn generated_finite_timeout_inheritance_and_request_reset_remain_distinct() {
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
    let timeout_proxy = loopback_proxy(&server);
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
    let endpoint_long = tokio::spawn({
        let client = client.clone();
        async move { client.timed().endpoint_long().await }
    });
    let local = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .timed()
                .endpoint_long()
                .timeout(Duration::from_millis(10))
                .expect("finite timeout")
                .await
        }
    });
    let reset = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .timed()
                .endpoint_long()
                .timeout(Duration::from_millis(10))
                .expect("finite timeout")
                .reset_timeout()
                .await
        }
    });
    let reinherited = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .timed()
                .endpoint_override()
                .timeout(Duration::from_millis(5))
                .expect("finite timeout")
                .reset_timeout()
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
            .response::<concord::Binary, concord::Bytes>()
            .build()
            .expect("runtime endpoint");
            client
                .bind(endpoint)
                .expect("runtime endpoint binds to client")
                .await
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
        .expect_err("request-local timeout must override the endpoint deadline");
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
        endpoint_long
            .await
            .expect("long endpoint timeout task")
            .expect("long endpoint timeout"),
        b"done"[..]
    );
    assert_eq!(
        reset
            .await
            .expect("request-local reset task")
            .expect("request-local reset"),
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

/// ORIGIN-01 (§5.2): a base literal without a terminal slash is normalized as
/// a directory prefix, and the normalization is visible in the static
/// explanation, not only in runtime plans: the contract's base path prefix,
/// every endpoint's logical URL template, and the templates agree with the
/// planned URLs.
#[test]
fn slashless_base_normalization_is_visible_in_the_static_explanation() {
    let contract = PathApi::contract();
    let base = contract.base();
    assert_eq!(base.literal(), "http://path.example.test/root");
    assert_eq!(base.path_prefix(), "/root/");
    assert_eq!(base.scheme(), "http");
    assert_eq!(base.host(), "path.example.test");
    assert_eq!(base.port(), 80);
    assert_eq!(base.origin(), "http://path.example.test:80");
    assert_eq!(contract.endpoints().len(), 3);
    for endpoint in contract.endpoints() {
        let template = contract.url_template(endpoint);
        assert!(
            template.starts_with("http://path.example.test/root/"),
            "{template}"
        );
    }
    let template = |id: &str| {
        contract.url_template(
            contract
                .endpoint(id)
                .unwrap_or_else(|| panic!("{id} endpoint")),
        )
    };
    assert_eq!(
        template("path_api.regional.replaced"),
        "http://path.example.test/root/only/{endpoint.name}"
    );
    assert_eq!(
        template("path_api.regional.off"),
        "http://path.example.test/root/"
    );
    // The static templates agree with the runtime plans.
    let client = PathApi::new().expect("path client");
    let regional = client.regional("eu");
    assert_eq!(
        regional
            .replaced("x")
            .plan()
            .expect("replace plan")
            .observed_url(),
        "http://path.example.test/root/only/x"
    );
    assert_eq!(
        regional.off().plan().expect("off plan").observed_url(),
        "http://path.example.test/root/"
    );

    // The same normalization applies under generated subdomain labels.
    let dns = DnsApi::contract();
    assert_eq!(dns.base().literal(), "http://dns.example.test/root");
    assert_eq!(dns.base().path_prefix(), "/root/");
    for endpoint in dns.endpoints() {
        let template = dns.url_template(endpoint);
        assert!(template.contains("dns.example.test/root/"), "{template}");
    }
}

/// V3-S05: relative templates with owner-qualified holes, per-hole adapters,
/// escaped braces, a literal percent sign, `replace` and `off`; the fixed base
/// path (`/root` normalized to `/root/`) is always retained. Invalid runtime
/// segments are rejected before any dispatch.
#[tokio::test(flavor = "current_thread")]
async fn relative_templates_encode_each_segment_exactly_once() {
    let _guard = request_shaping_test_guard().await;
    let server =
        ScriptedServer::start([ResponseScript::ok(b"file".to_vec())]).expect("path server");
    let proxy = loopback_proxy(&server);
    let client = PathApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("path client");
    let regional = client.regional("eu");

    let file = regional.file("a b?c#d%2Fe").set_q("x&y=z+%");
    let planned = file.plan().expect("file plan").observed_url().to_owned();
    assert_eq!(
        planned,
        "http://path.example.test/root/regions/eu/%7Bliteral%7D/A%20B%3FC%23D%252FE/v1-eu/100%25?q=x%26y%3Dz%2B%25"
    );
    assert_eq!(
        regional
            .replaced("x")
            .plan()
            .expect("replace plan")
            .observed_url(),
        "http://path.example.test/root/only/x"
    );
    assert_eq!(
        regional.off().plan().expect("off plan").observed_url(),
        "http://path.example.test/root/"
    );

    // V3-S07: the proxy routes the connection only; the logical origin and
    // the once-encoded target reach the wire unchanged.
    file.await.expect("proxied file request");
    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].target(), planned);

    for invalid in ["", ".", "..", "a/b", "a\\b"] {
        let error = regional
            .file(invalid)
            .plan()
            .expect_err("invalid generated segment");
        assert_eq!(
            error.configuration_kind(),
            Some(ConfigurationErrorKind::InvalidRoute),
            "{invalid:?}"
        );
    }
    assert_eq!(server.request_count(), 1);
}

/// V3-S06: every generated label is exactly one lowercase ASCII DNS label; a
/// qualified reference is one label (never split); total host length is
/// checked; the fixed suffix is always retained.
#[test]
fn dns_labels_are_single_validated_ascii_labels() {
    let client = DnsApi::new().expect("dns client");
    assert_eq!(
        client
            .tenant("EU")
            .get()
            .plan()
            .expect("normalized label")
            .observed_url(),
        "http://eu.api.dns.example.test/root/get"
    );
    assert_eq!(
        client
            .qualified("Zone-7")
            .inner()
            .get()
            .plan()
            .expect("qualified label")
            .observed_url(),
        "http://zone-7.dns.example.test/root/"
    );
    let max_label = "a".repeat(63);
    assert!(client.tenant(max_label.as_str()).get().plan().is_ok());
    for invalid in [
        "two.labels".to_owned(),
        String::new(),
        "-leading".to_owned(),
        "trailing-".to_owned(),
        "é".to_owned(),
        "under_score".to_owned(),
        "a".repeat(64),
    ] {
        let error = client
            .tenant(invalid.as_str())
            .get()
            .plan()
            .expect_err("invalid label");
        assert_eq!(
            error.configuration_kind(),
            Some(ConfigurationErrorKind::InvalidRoute),
            "{invalid:?}"
        );
        let error = client
            .qualified(invalid.as_str())
            .inner()
            .get()
            .plan()
            .expect_err("invalid qualified label");
        assert_eq!(
            error.category(),
            ErrorCategory::Configuration,
            "{invalid:?}"
        );
    }

    let short = "b".repeat(20);
    assert!(
        client
            .deep(
                short.as_str(),
                short.as_str(),
                short.as_str(),
                short.as_str()
            )
            .get()
            .plan()
            .is_ok()
    );
    let long = "c".repeat(63);
    let error = client
        .deep(long.as_str(), long.as_str(), long.as_str(), long.as_str())
        .get()
        .plan()
        .expect_err("complete host exceeds DNS limits");
    assert_eq!(error.category(), ErrorCategory::Configuration);
}

/// V3-L09 (runtime part; the negative cases are compile-fail UI tests): the
/// closed conversion set accepts identity, `&str` and `&String` (also for an
/// opaque alias), unsuffixed integers infer the declared type, and an exact
/// `None` clears a previously selected optional value (VALUE-04).
#[test]
fn closed_conversions_accept_borrowed_strings_integers_and_exact_none() {
    let owned = String::from("owned");
    let client = ConversionApi::builder()
        .set_name(&owned)
        .set_locale("fr")
        .replace_locale(None)
        .build()
        .expect("borrowed String and exact None");
    assert_eq!(
        client
            .read(25, Vec::new())
            .set_offset(-3)
            .plan()
            .expect("integer inference")
            .observed_url(),
        "http://conversions.example.test/?name=owned&count=25&offset=-3"
    );
    assert_eq!(
        client
            .read(7, vec![owned.clone()])
            .set_offset(4)
            .replace_offset(None)
            .plan()
            .expect("exact None")
            .observed_url(),
        "http://conversions.example.test/?name=owned&count=7&tag=owned"
    );

    let alias = ConversionApi::builder()
        .set_name("literal")
        .set_locale(&owned)
        .build()
        .expect("&String into an opaque String alias");
    assert_eq!(
        alias
            .read(1, vec!["a".to_owned(), "b".to_owned()])
            .plan()
            .expect("alias plan")
            .observed_url(),
        "http://conversions.example.test/?name=literal&locale=owned&count=1&tag=a&tag=b"
    );
}

/// V3-L13 (description part; dead required inputs are compile-fail UI
/// tests): an explicitly deprecated optional input with no semantic use is a
/// disclosed deprecated no-op value and never reaches the request (VALUE-06).
#[test]
#[allow(deprecated)]
fn deprecated_no_op_inputs_are_disclosed_and_never_sent() {
    let contract = LivenessApi::contract();
    let legacy = contract.client().value("legacy").expect("legacy value");
    assert_eq!(
        legacy.liveness(),
        concord::inspection::ValueLiveness::DeprecatedNoOp
    );
    assert_eq!(
        legacy
            .deprecation()
            .and_then(|deprecation| deprecation.note_text()),
        Some("no longer sent")
    );
    let used = contract.client().value("used").expect("used value");
    assert_eq!(used.liveness(), concord::inspection::ValueLiveness::Live);
    assert!(
        contract
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.subject() == "legacy"
                && diagnostic.severity() == concord::inspection::DiagnosticSeverity::Warning
                && diagnostic.message().contains("no-op")),
        "{:?}",
        contract.diagnostics()
    );

    let client = LivenessApi::builder()
        .set_legacy("legacy-sentinel")
        .build()
        .expect("deprecated input still accepted");
    assert_eq!(
        client.read().plan().expect("plan").observed_url(),
        "http://liveness.example.test/?used=u"
    );
}
