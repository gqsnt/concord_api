//! Runtime endpoint construction, binding and evaluation tests (§16).
//!
//! Obligations: V3-R09 (root/scope projection, plan agrees with the send),
//! V3-R10 (replacement parity, absent/empty/each, name bounds, forbidden
//! authority), V3-R11 (challenge overlap, generated-ID collision, missing
//! limiter), V3-S03 (suppressed adapters never run, survivors once per
//! preparation), V3-S11 (first-error latch), V3-S12 (plan/bind inertness).

use std::future::IntoFuture;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use bytes::Bytes;
use concord_test_support::executor::DeterministicExecutor;
use concord_test_support::script::{ExpectedRequest, ScriptedResponse};
use http::{HeaderValue, Method, StatusCode};
use url::Url;

use super::test_owner::{self, TestOwner};
use super::*;
use crate::atom::{AtomError, AtomErrorKind, BoundedAtomWriter};
use crate::body::{PreparedBody, RequestBodyContext};
use crate::client::ClientCore;
use crate::codec::{Binary, Text};
use crate::controls::{CallOptions, CancellationToken};
use crate::endpoint::{MAX_REQUEST_HEAD_COMPONENTS, RequestPlan};
use crate::error::ErrorCategory;
use crate::hooks::BoxFuture;
use crate::http::HttpConfigBuilder;
use crate::rate_limit::{
    Enforcement, LimiterCapability, MAX_RATE_LIMIT_POLICIES_PER_PLAN, NoopRateLimiter,
    RateLimitError, RateLimitPermit, RateLimitRequest, RateLimiter,
};
use crate::request::program::{PatchFamily, PatchKind};
use crate::response::{ByteStream, Discard, Empty};
use crate::runtime::RuntimeConfigBuilder;

const SENTINEL: &str = "runtime-value-sentinel-4d1e";
const RATE_ID_SENTINEL: &str = "runtime-rate-id-sentinel-527d";
const RATE_KEY_SENTINEL: &str = "runtime-rate-key-sentinel-28b1";

fn id(value: &'static str) -> EndpointId {
    EndpointId::from_static(value)
}

fn core_with(executor: &DeterministicExecutor, runtime: RuntimeConfigBuilder) -> ClientCore {
    ClientCore::build_with_development(
        "http://runtime.example.test/base/",
        HttpConfigBuilder::new().http1_only(),
        runtime,
        executor,
    )
    .expect("development core")
}

fn core(executor: &DeterministicExecutor) -> ClientCore {
    core_with(executor, RuntimeConfigBuilder::new())
}

fn owner(core: &ClientCore) -> Arc<TestOwner> {
    TestOwner::new(core.clone()).into_projection()
}

fn pairs(plan: &RequestPlan) -> Vec<(String, String)> {
    Url::parse(plan.observed_url())
        .expect("observed URL")
        .query_pairs()
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .collect()
}

fn pair_list(values: &[(&str, &str)]) -> Vec<(String, String)> {
    values
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect()
}

fn rule(id: &str, value: &str) -> RateLimitDeclaration {
    RateLimitDeclaration::new(id)
        .window(10, Duration::from_secs(1))
        .part("tenant", value)
}

fn run<R, O>(request: BoundRuntimeRequest<R, O>) -> O
where
    R: ResponseRepresentation<O>,
    O: Send + 'static,
{
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(request.into_future())
        .expect("runtime execution")
}

/// Counts every canonical encoding.
struct Tracked {
    text: &'static str,
    calls: Arc<AtomicUsize>,
}

impl RequestAtom for Tracked {
    fn write_atom(&self, output: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        output.push_str(self.text)
    }
}

/// Fails the test if a construction/binding step ever encodes it.
struct PanicAtom;

impl RequestAtom for PanicAtom {
    fn write_atom(&self, _output: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        panic!("a runtime value was encoded outside plan/execution")
    }
}

/// Fails the test if a sequence is ever iterated.
struct PanicSequence;

impl OrderedValues for PanicSequence {
    type Item = &'static str;
    type Iter<'a> = std::iter::Empty<&'a &'static str>;

    fn ordered_values(&self) -> Self::Iter<'_> {
        panic!("a runtime sequence was iterated outside plan/execution")
    }
}

/// An ordered sequence whose iterator refuses to go past `limit` items.
struct BoundedSequence {
    values: Vec<Tracked>,
    next_calls: Arc<AtomicUsize>,
    limit: usize,
}

struct BoundedIter<'a> {
    values: std::slice::Iter<'a, Tracked>,
    next_calls: &'a AtomicUsize,
    limit: usize,
}

impl<'a> Iterator for BoundedIter<'a> {
    type Item = &'a Tracked;

    fn next(&mut self) -> Option<Self::Item> {
        let call = self.next_calls.fetch_add(1, Ordering::SeqCst);
        assert!(
            call < self.limit,
            "iterator consumed beyond the first excess"
        );
        self.values.next()
    }
}

impl OrderedValues for BoundedSequence {
    type Item = Tracked;
    type Iter<'a> = BoundedIter<'a>;

    fn ordered_values(&self) -> Self::Iter<'_> {
        BoundedIter {
            values: self.values.iter(),
            next_calls: &self.next_calls,
            limit: self.limit,
        }
    }
}

/// Upper-cases through an explicit adapter.
struct Upper;

impl AtomEncoder<&'static str> for Upper {
    fn write(value: &&'static str, output: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        output.push_str(&value.to_ascii_uppercase())
    }
}

/// An adapter that always fails.
struct Failing;

impl AtomEncoder<&'static str> for Failing {
    fn write(_value: &&'static str, _output: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        Err(AtomError::new(AtomErrorKind::Encode))
    }
}

struct CountingBodyInput {
    calls: Arc<AtomicUsize>,
}

struct CountingBody;

impl RequestRepresentation<CountingBodyInput> for CountingBody {
    fn prepare(
        input: CountingBodyInput,
        _context: RequestBodyContext<'_>,
    ) -> Result<PreparedBody, crate::body::BodyError> {
        input.calls.fetch_add(1, Ordering::SeqCst);
        Ok(PreparedBody::bytes(Bytes::from_static(b"counted-body")))
    }
}

#[derive(Default)]
struct CountingLimiter {
    acquisitions: AtomicUsize,
}

impl RateLimiter for CountingLimiter {
    fn capability(&self) -> LimiterCapability {
        LimiterCapability::new(Enforcement::Local, "test.runtime.counting", 1)
    }

    fn acquire<'a>(
        &'a self,
        _request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        self.acquisitions.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(RateLimitPermit::granted()) })
    }
}

fn build_kind(builder: RuntimeEndpointBuilder) -> Option<ConfigurationErrorKind> {
    builder
        .response::<Empty, ()>()
        .build()
        .err()
        .map(|error| error.kind())
}

// ---------------------------------------------------------------------------
// Construction: exact typestate, symbolic recording, first-error latch.
// ---------------------------------------------------------------------------

#[test]
fn canonical_construction_compiles_and_selects_the_output_pair_once() {
    fn unselected(_: &RuntimeEndpointBuilder) {}
    fn text(_: &RuntimeEndpointBuilder<Text, String>) {}
    fn stream(_: &RuntimeEndpoint<Binary, ByteStream>) {}

    let builder = RuntimeEndpoint::builder(id("catalog.dynamic.lookup"), Method::GET);
    unselected(&builder);
    let builder = builder
        .path("items")
        .path("42")
        .query("view", "summary")
        .response::<Text, String>();
    text(&builder);
    let endpoint = builder.build().expect("§16.1 construction");
    assert_eq!(endpoint.description().id(), "catalog.dynamic.lookup");
    assert_eq!(endpoint.description().response_accept(), Some("text/plain"));
    let streaming = RuntimeEndpoint::builder(id("runtime.stream"), Method::GET)
        .response::<Binary, ByteStream>()
        .build()
        .expect("streaming pair");
    stream(&streaming);
    let _discard: RuntimeEndpoint<Discard, ()> =
        RuntimeEndpoint::builder(id("runtime.discard"), Method::DELETE)
            .response::<Discard, ()>()
            .build()
            .expect("discard pair");
}

#[test]
fn construction_and_binding_never_encode_iterate_or_prepare() {
    let executor = DeterministicExecutor::new();
    let core = core(&executor);
    let body_calls = Arc::new(AtomicUsize::new(0));
    let endpoint = RuntimeEndpoint::builder(id("runtime.inert"), Method::POST)
        .path(PanicAtom)
        .query("scalar", PanicAtom)
        .query_optional("optional", Some(PanicAtom))
        .query_each("each", PanicSequence)
        .header("x-panic", PanicAtom)
        .header_each_optional("x-panic-each", Some(PanicSequence))
        .body_repr::<CountingBody, _>(CountingBodyInput {
            calls: Arc::clone(&body_calls),
        })
        .response::<Empty, ()>()
        .build()
        .expect("symbolic runtime endpoint");
    let bound = test_owner::bind(&owner(&core), endpoint).expect("structural binding");
    assert_eq!(body_calls.load(Ordering::SeqCst), 0);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    drop(bound);
    assert_eq!(body_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn runtime_values_are_encoded_once_per_plan_or_execution_preparation() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::new()))
        .expect("response");
    let core = core(&executor);
    let path_calls = Arc::new(AtomicUsize::new(0));
    let query_calls = Arc::new(AtomicUsize::new(0));
    let endpoint = RuntimeEndpoint::builder(id("runtime.encode_once"), Method::GET)
        .path(Tracked {
            text: "segment",
            calls: Arc::clone(&path_calls),
        })
        .query(
            "q",
            Tracked {
                text: "value",
                calls: Arc::clone(&query_calls),
            },
        )
        .response::<Empty, ()>()
        .build()
        .expect("tracked endpoint");
    assert_eq!(path_calls.load(Ordering::SeqCst), 0);
    let bound = test_owner::bind(&owner(&core), endpoint).expect("binding");
    assert_eq!(query_calls.load(Ordering::SeqCst), 0);
    let plan = bound.plan().expect("plan");
    assert!(plan.observed_url().ends_with("/base/segment?q=value"));
    assert_eq!(path_calls.load(Ordering::SeqCst), 1);
    assert_eq!(query_calls.load(Ordering::SeqCst), 1);
    run(bound);
    assert_eq!(path_calls.load(Ordering::SeqCst), 2);
    assert_eq!(query_calls.load(Ordering::SeqCst), 2);
}

#[test]
fn runtime_path_atoms_are_validated_and_encoded_once_at_plan() {
    let executor = DeterministicExecutor::new();
    let core = core(&executor);
    let endpoint = RuntimeEndpoint::builder(id("runtime.route"), Method::GET)
        .path("ordinary")
        .path("space value")
        .path("query?value")
        .path("fragment#value")
        .path("percent%value")
        .path("encoded%2Fvalue")
        .path_via::<&'static str, Upper>("adapted")
        .response::<Empty, ()>()
        .build()
        .expect("runtime route");
    let plan = test_owner::plan(&owner(&core), &endpoint).expect("plan");
    let url = Url::parse(plan.observed_url()).expect("observed URL");
    assert_eq!(
        url.path_segments().expect("segments").collect::<Vec<_>>(),
        [
            "base",
            "ordinary",
            "space%20value",
            "query%3Fvalue",
            "fragment%23value",
            "percent%25value",
            // PATH §5.3: percent signs are ordinary data encoded exactly once.
            "encoded%252Fvalue",
            "ADAPTED",
        ]
    );
    assert_eq!(url.query(), None);
    assert_eq!(url.fragment(), None);
    // Values are symbolic until plan/execution: an invalid segment builds and
    // binds structurally, then fails its first preparation.
    for invalid in ["", ".", "..", "slash/value", "back\\value"] {
        let endpoint = RuntimeEndpoint::builder(id("runtime.invalid_path"), Method::GET)
            .path(invalid)
            .response::<Empty, ()>()
            .build()
            .expect("symbolic path atom");
        let error = test_owner::plan(&owner(&core), &endpoint).expect_err("invalid segment");
        assert_eq!(
            error.configuration_kind(),
            Some(ConfigurationErrorKind::InvalidRoute)
        );
    }
}

#[test]
fn keys_methods_and_forbidden_targets_are_validated_at_construction() {
    let base = || RuntimeEndpoint::builder(id("runtime.keys"), Method::GET);
    for forbidden in crate::inspection::FORBIDDEN_PUBLIC_HEADERS {
        let upper = forbidden.to_ascii_uppercase();
        assert_eq!(
            build_kind(base().header(&upper, SENTINEL)),
            Some(ConfigurationErrorKind::InvalidHeader),
            "{forbidden}"
        );
    }
    assert_eq!(
        build_kind(base().header("bad header", "x")),
        Some(ConfigurationErrorKind::InvalidHeader)
    );
    let long = "k".repeat(MAX_RUNTIME_KEY_BYTES + 1);
    assert_eq!(
        build_kind(base().header(&long, "x")),
        Some(ConfigurationErrorKind::InvalidHeader)
    );
    assert_eq!(
        build_kind(base().query("", "x")),
        Some(ConfigurationErrorKind::InvalidValue)
    );
    assert_eq!(
        build_kind(base().query(&long, "x")),
        Some(ConfigurationErrorKind::InvalidValue)
    );
    let exact = "k".repeat(MAX_RUNTIME_KEY_BYTES);
    assert_eq!(
        build_kind(base().query(&exact, "x").header(&exact, "x")),
        None
    );
    for method in ["CONNECT", "trace"] {
        let method = Method::from_bytes(method.as_bytes()).expect("method token");
        assert_eq!(
            build_kind(RuntimeEndpoint::builder(id("runtime.method"), method)),
            Some(ConfigurationErrorKind::InvalidValue)
        );
    }
    let error = base()
        .header("authorization", SENTINEL)
        .response::<Empty, ()>()
        .build()
        .expect_err("reserved header");
    assert!(!error.to_string().contains(SENTINEL));
    assert!(!format!("{error:?}").contains(SENTINEL));
}

/// One runtime keyed statement applied to a builder under a given key.
type KeyedForm = fn(RuntimeEndpointBuilder, &str) -> RuntimeEndpointBuilder;

/// Names one [`KeyedForm`] (the parameter type drives closure inference).
fn form(name: &'static str, apply: KeyedForm) -> (&'static str, KeyedForm) {
    (name, apply)
}

/// Every runtime header statement shape that can carry an **absent**
/// outer-optional source: introduction, exact replacement, conditional
/// replacement and append; scalar and ordered sequence; plain, sensitive and
/// adapter spellings. Each value is a never-encoded panic atom/sequence, so a
/// rejection cannot come from evaluating it.
fn absent_header_forms() -> Vec<(&'static str, KeyedForm)> {
    vec![
        form("header_optional", |builder, key| {
            builder.header_optional(key, Option::<PanicAtom>::None)
        }),
        form("header_optional_sensitive", |builder, key| {
            builder.header_optional_sensitive(key, Option::<PanicAtom>::None)
        }),
        form("header_optional_via", |builder, key| {
            builder.header_optional_via::<&'static str, Failing>(key, None)
        }),
        form("header_optional_sensitive_via", |builder, key| {
            builder.header_optional_sensitive_via::<&'static str, Failing>(key, None)
        }),
        form("header_each_optional", |builder, key| {
            builder.header_each_optional(key, Option::<PanicSequence>::None)
        }),
        form("header_each_optional_sensitive_via", |builder, key| {
            builder.header_each_optional_sensitive_via::<PanicSequence, Failing>(key, None)
        }),
        form("header_replace_optional", |builder, key| {
            builder.header_replace_optional(key, Option::<PanicAtom>::None)
        }),
        form("header_replace_each_optional", |builder, key| {
            builder.header_replace_each_optional(key, Option::<PanicSequence>::None)
        }),
        form("header_when_present", |builder, key| {
            builder.header_when_present(key, Option::<PanicAtom>::None)
        }),
        form("header_when_present_each", |builder, key| {
            builder.header_when_present_each(key, Option::<PanicSequence>::None)
        }),
        form("header_append_optional", |builder, key| {
            builder.header_append_optional(key, Option::<PanicAtom>::None)
        }),
        form("header_append_each_optional", |builder, key| {
            builder.header_append_each_optional(key, Option::<PanicSequence>::None)
        }),
    ]
}

/// HEAD-01 / V3-S08 (runtime negative; body-media, negotiation and transport
/// families): a public header writer claims its target structurally, before
/// presence is known. An absent optional source therefore never makes a
/// `Content-Type`/`Content-Length`/`Accept`/`Host`/... writer legal: every
/// absent-capable shape is rejected when the descriptor is constructed, so it
/// can never reach binding, planning or a send, and no value is encoded.
#[test]
fn absent_optional_writers_cannot_target_media_negotiation_or_transport_fields() {
    let base = || RuntimeEndpoint::builder(id("runtime.reserved_absent"), Method::POST);
    let rejected = Some(ConfigurationErrorKind::InvalidHeader);
    let forms = absent_header_forms();
    // The complete versioned reserved set, including the families named by
    // V3-S08: body media (`content-type`, `content-encoding`,
    // `content-length`), negotiation (`accept`, `accept-encoding`) and
    // transport (`host`, `transfer-encoding`, `connection`, `te`, ...).
    for family_member in [
        "content-type",
        "content-length",
        "accept",
        "host",
        "transfer-encoding",
    ] {
        assert!(
            crate::inspection::FORBIDDEN_PUBLIC_HEADERS.contains(&family_member),
            "{family_member} is reserved"
        );
    }
    for reserved in crate::inspection::FORBIDDEN_PUBLIC_HEADERS {
        let spellings = [
            (*reserved).to_owned(),
            reserved.to_ascii_uppercase(),
            // Mixed case (`Content-Type`): header names compare
            // case-insensitively.
            reserved
                .split('-')
                .map(|word| {
                    let mut chars = word.chars();
                    chars.next().map_or_else(String::new, |first| {
                        first.to_ascii_uppercase().to_string() + chars.as_str()
                    })
                })
                .collect::<Vec<_>>()
                .join("-"),
        ];
        for spelling in &spellings {
            for (form, apply) in &forms {
                assert_eq!(
                    build_kind(apply(base(), spelling)),
                    rejected,
                    "{form}(\"{spelling}\", None) must be rejected"
                );
            }
        }
    }

    // A present body owns the media header: an absent optional public
    // `Content-Type` writer beside it is rejected, and the representation is
    // never prepared.
    let body_calls = Arc::new(AtomicUsize::new(0));
    let error = RuntimeEndpoint::builder(id("runtime.reserved_absent"), Method::POST)
        .body_repr::<CountingBody, _>(CountingBodyInput {
            calls: Arc::clone(&body_calls),
        })
        .header_optional("Content-Type", Option::<PanicAtom>::None)
        .response::<Empty, ()>()
        .build()
        .expect_err("an absent public Content-Type writer beside a body");
    assert_eq!(error.kind(), ConfigurationErrorKind::InvalidHeader);
    assert_eq!(body_calls.load(Ordering::SeqCst), 0);

    // The first rejection latches: a later valid statement cannot erase it.
    assert_eq!(
        build_kind(
            base()
                .header_optional("Accept", Option::<PanicAtom>::None)
                .header("x-trace", "later")
        ),
        rejected
    );

    // Control: the identical absent shapes targeting an ordinary extension
    // field construct (override shapes defer their target check to bind), so
    // the rejections above are caused by the reserved target alone.
    for (form, apply) in &forms {
        assert_eq!(
            build_kind(apply(base(), "X-Extension")),
            None,
            "{form}(\"X-Extension\", None) is structurally valid locally"
        );
    }
}

#[test]
fn method_limit_is_checked_first_and_charged_to_the_retained_budget() {
    let exact_text = "M".repeat(MAX_RUNTIME_ENDPOINT_METHOD_BYTES);
    let exact = Method::from_bytes(exact_text.as_bytes()).expect("exact method token");
    let endpoint = RuntimeEndpoint::builder(id("runtime.method.exact"), exact.clone())
        .response::<Empty, ()>()
        .build()
        .expect("exact method");
    assert_eq!(endpoint.description().method(), &exact);

    // Retained descriptor bytes are the method plus every key, rate
    // declaration and auth target. One custom rate part retains its ID, its
    // label and its canonically framed value, so a single declaration can
    // land exactly on the aggregate limit; one more byte is the first excess.
    let declaration = |value: &str| {
        RateLimitDeclaration::new("r")
            .window(10, Duration::from_secs(1))
            .part("tenant", value)
    };
    let framing = declaration("v").retained_bytes().expect("probe") - 1;
    let exact_value = "v".repeat(MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES - exact_text.len() - framing);
    assert_eq!(
        exact_text.len() + declaration(&exact_value).retained_bytes().expect("exact"),
        MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES
    );
    let excess_value = format!("{exact_value}v");
    let governed = |method: Method, value: &str| {
        RuntimeEndpoint::builder(id("runtime.method.budget"), method).rate_limit(declaration(value))
    };
    assert_eq!(build_kind(governed(exact.clone(), &exact_value)), None);
    assert_eq!(
        build_kind(governed(exact, &excess_value)),
        Some(ConfigurationErrorKind::InvalidRateLimit)
    );
    // The method is what is charged: a three-byte method leaves room for
    // the same excess declaration.
    assert_eq!(build_kind(governed(Method::GET, &excess_value)), None);

    let excessive_text = "N".repeat(MAX_RUNTIME_ENDPOINT_METHOD_BYTES + 1);
    let excessive = Method::from_bytes(excessive_text.as_bytes()).expect("excess method token");
    let error = RuntimeEndpoint::builder(id("runtime.method.excess"), excessive)
        .path(PanicAtom)
        .response::<Empty, ()>()
        .build()
        .expect_err("first excess method remains latched");
    assert_eq!(error.kind(), ConfigurationErrorKind::InvalidValue);
    assert!(!format!("{error:?}").contains(&excessive_text));
}

struct Retained(Arc<()>);

impl RequestAtom for Retained {
    fn write_atom(&self, _output: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        let _ = &self.0;
        panic!("latched builder encoded a later value")
    }
}

#[test]
fn first_error_latch_drops_later_arguments_without_work() {
    let retained = Arc::new(());
    let builder = RuntimeEndpoint::builder(id("runtime.latched"), Method::GET)
        .header("authorization", "reserved")
        .query_each("later", PanicSequence)
        .path(Retained(Arc::clone(&retained)))
        .header_sensitive("x-later", Retained(Arc::clone(&retained)))
        .rate_limit(rule(RATE_ID_SENTINEL, RATE_KEY_SENTINEL))
        .status_codes(std::iter::repeat_with(|| -> StatusCode {
            panic!("latched builder iterated a later status sequence")
        }));
    // Every later argument was dropped immediately; nothing is retained.
    assert_eq!(Arc::strong_count(&retained), 1);
    let error = builder
        .response::<Empty, ()>()
        .build()
        .expect_err("the first failure remains latched");
    assert_eq!(error.kind(), ConfigurationErrorKind::InvalidHeader);
    assert!(!format!("{error:?}").contains(RATE_KEY_SENTINEL));
}

#[test]
fn recorded_operation_limit_rejects_the_first_excess_without_retention() {
    // The method is one recorded operation.
    let fill = |mut builder: RuntimeEndpointBuilder| {
        for index in 0..(MAX_RUNTIME_ENDPOINT_COMPONENTS - 1) {
            builder = builder.query_append(&format!("k{index}"), "v");
        }
        builder
    };
    assert_eq!(
        build_kind(fill(RuntimeEndpoint::builder(
            id("runtime.components"),
            Method::GET
        ))),
        None
    );
    let retained = Arc::new(());
    let excessive = fill(RuntimeEndpoint::builder(
        id("runtime.components"),
        Method::GET,
    ))
    .path(Retained(Arc::clone(&retained)));
    assert_eq!(Arc::strong_count(&retained), 1);
    assert_eq!(
        build_kind(excessive),
        Some(ConfigurationErrorKind::InvalidRoute)
    );
}

#[test]
fn plan_iteration_stops_at_the_first_head_excess() {
    let executor = DeterministicExecutor::new();
    let core = core(&executor);
    let next_calls = Arc::new(AtomicUsize::new(0));
    let formatted = Arc::new(AtomicUsize::new(0));
    let limit = MAX_REQUEST_HEAD_COMPONENTS + 1;
    let endpoint = RuntimeEndpoint::builder(id("runtime.each_limit"), Method::GET)
        .query_each(
            "k",
            BoundedSequence {
                values: (0..limit + 10)
                    .map(|_| Tracked {
                        text: "v",
                        calls: Arc::clone(&formatted),
                    })
                    .collect(),
                next_calls: Arc::clone(&next_calls),
                limit,
            },
        )
        .response::<Empty, ()>()
        .build()
        .expect("one recorded each operation");
    let bound = test_owner::bind(&owner(&core), endpoint).expect("structural binding");
    assert_eq!(next_calls.load(Ordering::SeqCst), 0);
    let error = bound.plan().expect_err("head component limit");
    assert_eq!(error.category(), ErrorCategory::Configuration);
    // The 257th occurrence is the first excess: read and encoded once, never
    // retained, and nothing after it is read.
    assert_eq!(next_calls.load(Ordering::SeqCst), limit);
    assert_eq!(formatted.load(Ordering::SeqCst), limit);
}

#[test]
fn endpoint_local_selections_are_validated_singletons() {
    let base = || RuntimeEndpoint::builder(id("runtime.selections"), Method::POST);
    let invalid = Some(ConfigurationErrorKind::InvalidValue);
    assert_eq!(build_kind(base().timeout(Duration::ZERO)), invalid);
    assert_eq!(build_kind(base().timeout(Duration::MAX)), invalid);
    assert_eq!(
        build_kind(
            base()
                .timeout(Duration::from_secs(1))
                .timeout(Duration::from_secs(2))
        ),
        invalid
    );
    assert_eq!(
        build_kind(base().effect(Effect::Safe).effect(Effect::Idempotent)),
        invalid
    );
    assert_eq!(build_kind(base().status_codes([])), invalid);
    assert_eq!(
        build_kind(base().status_codes([StatusCode::from_u16(199).expect("status")])),
        invalid
    );
    assert_eq!(
        build_kind(base().status_codes([StatusCode::OK, StatusCode::OK])),
        invalid
    );
    assert_eq!(
        build_kind(
            base()
                .status_codes([StatusCode::OK])
                .status(AcceptedStatus::Success)
        ),
        invalid
    );
    assert_eq!(
        build_kind(
            base()
                .body_repr::<Text, _>("one".to_owned())
                .body_repr::<Text, _>("two".to_owned())
        ),
        invalid
    );
    let endpoint = base()
        .timeout(Duration::from_secs(7))
        .effect(Effect::NonIdempotent)
        .status_codes([StatusCode::CREATED, StatusCode::OK])
        .body_repr::<Text, _>("selected".to_owned())
        .response::<Empty, ()>()
        .build()
        .expect("valid selections");
    let description = endpoint.description();
    assert_eq!(
        description.timeout(),
        TimeoutSelection::Explicit(Duration::from_secs(7))
    );
    assert_eq!(description.effect(), Effect::NonIdempotent);
    assert_eq!(description.effect_source(), EffectSource::Explicit);
    assert_eq!(description.status_source(), StatusSource::Explicit);
    assert_eq!(
        description.accepted_status().explicit_codes(),
        Some(&[StatusCode::OK, StatusCode::CREATED][..])
    );
    assert_eq!(
        description.body_category(),
        RequestBodyCategory::Specification
    );
    assert_eq!(
        description.body_description().media_hint(),
        Some("text/plain; charset=utf-8")
    );
}

// ---------------------------------------------------------------------------
// Structural patch rules (PATCH-02, HEAD-01) at construction and bind.
// ---------------------------------------------------------------------------

#[test]
fn local_structural_rules_are_rejected_at_construction() {
    let base = || RuntimeEndpoint::builder(id("runtime.structure"), Method::GET);
    let query = Some(ConfigurationErrorKind::InvalidValue);
    let header = Some(ConfigurationErrorKind::InvalidHeader);
    // A plain introduction never overwrites an earlier potential writer.
    assert_eq!(build_kind(base().query("k", "1").query("k", "2")), query);
    assert_eq!(
        build_kind(base().header("X-K", "1").header("x-k", "2")),
        header
    );
    // Query keys are case-sensitive.
    assert_eq!(build_kind(base().query("k", "1").query("K", "2")), None);
    // Family fences precede every same-family statement and appear once.
    assert_eq!(
        build_kind(base().query("k", "1").query_replace_all()),
        query
    );
    assert_eq!(build_kind(base().query_remove("k").query_off()), query);
    assert_eq!(build_kind(base().query_replace_all().query_off()), query);
    assert_eq!(build_kind(base().query_off().query_append("k", "1")), query);
    assert_eq!(build_kind(base().header("x-k", "1").header_off()), header);
    assert_eq!(
        build_kind(base().header_off().header_append("x-k", "1")),
        header
    );
    // A fence is family-local.
    assert_eq!(build_kind(base().header("x-k", "1").query_off()), None);
    // After a local fence the inherited program is gone: an override needs
    // an earlier runtime writer.
    assert_eq!(
        build_kind(base().query_replace_all().query_replace("k", "1")),
        query
    );
    assert_eq!(
        build_kind(base().query_replace_all().query_remove("k")),
        query
    );
    assert_eq!(
        build_kind(
            base()
                .query_replace_all()
                .query("k", "1")
                .query_when_present("k", Some("2"))
        ),
        None
    );
    // Singleton request headers cannot repeat.
    assert_eq!(build_kind(base().header_append("User-Agent", "a")), header);
    assert_eq!(build_kind(base().header_each("referer", vec!["a"])), header);
    assert_eq!(
        build_kind(base().header_when_present_each("Range", Option::<Vec<&str>>::None)),
        header
    );
    assert_eq!(build_kind(base().header("user-agent", "one")), None);
    // Path fences precede every runtime path atom.
    let route = Some(ConfigurationErrorKind::InvalidRoute);
    assert_eq!(build_kind(base().path("a").path_replace()), route);
    assert_eq!(build_kind(base().path_off().path("a")), route);
    assert_eq!(build_kind(base().path_replace().path_off()), route);
    // Authentication removal rules.
    let auth = Some(ConfigurationErrorKind::InvalidAuthentication);
    assert_eq!(build_kind(base().auth_off().auth_off()), auth);
    assert_eq!(
        build_kind(base().auth_off().auth_remove(AuthTarget::authorization())),
        auth
    );
    assert_eq!(
        build_kind(
            base()
                .auth_remove(AuthTarget::authorization())
                .auth_remove(AuthTarget::authorization())
        ),
        auth
    );
    assert_eq!(
        build_kind(base().auth_remove(AuthTarget::authorization()).auth_off()),
        auth
    );
}

fn locale_owner(core: &ClientCore) -> TestOwner {
    TestOwner::new(core.clone())
        .query("a", "1")
        .query("locale", "fr")
        .query("z", "9")
}

#[test]
fn owner_level_structure_is_validated_at_bind() {
    let executor = DeterministicExecutor::new();
    let core = core(&executor);
    let owner = locale_owner(&core)
        .header("X-Scope", "west")
        .into_projection();
    let bind = |builder: RuntimeEndpointBuilder| {
        test_owner::bind(
            &owner,
            builder
                .response::<Empty, ()>()
                .build()
                .expect("structurally local-valid"),
        )
        .err()
        .and_then(|error| error.configuration_kind())
    };
    let base = || RuntimeEndpoint::builder(id("runtime.bind_structure"), Method::GET);
    let query = Some(ConfigurationErrorKind::InvalidValue);
    let header = Some(ConfigurationErrorKind::InvalidHeader);
    // PATCH-02: introducing an inherited key is a conflict for every shape.
    assert_eq!(bind(base().query("locale", "de")), query);
    assert_eq!(
        bind(base().query_optional("locale", Option::<&str>::None)),
        query
    );
    assert_eq!(bind(base().query_each("locale", Vec::<&str>::new())), query);
    assert_eq!(bind(base().header("x-scope", "east")), header);
    // Override targets need an earlier potential writer.
    assert_eq!(bind(base().query_replace("missing", "x")), query);
    assert_eq!(bind(base().query_when_present("missing", Some("x"))), query);
    assert_eq!(bind(base().query_remove("missing")), query);
    assert_eq!(bind(base().header_replace("x-missing", "x")), header);
    // Valid owner-relative statements bind.
    assert_eq!(bind(base().query_replace("locale", "de")), None);
    assert_eq!(
        bind(base().query_remove("locale").query("locale", "de")),
        None
    );
    assert_eq!(bind(base().query_append("locale", "de")), None);
    assert_eq!(bind(base().header_replace("x-SCOPE", "east")), None);
    assert_eq!(bind(base().query("Locale", "de")), None);
    // A family fence clears the inherited ownership.
    assert_eq!(bind(base().query_replace_all().query("locale", "de")), None);
    assert_eq!(bind(base().header_off()), None);
}

#[test]
fn runtime_replacement_parity_matches_static_programs() {
    type Case = (
        RuntimeEndpointBuilder,
        &'static [(&'static str, &'static str)],
        PatchKind,
        Option<&'static [&'static str]>,
    );
    let executor = DeterministicExecutor::new();
    let core = core(&executor);
    let owner = locale_owner(&core).into_projection();
    let plan = |builder: RuntimeEndpointBuilder| {
        test_owner::plan(
            &owner,
            &builder
                .response::<Empty, ()>()
                .build()
                .expect("runtime endpoint"),
        )
        .expect("runtime plan")
    };
    let base = || RuntimeEndpoint::builder(id("runtime.parity"), Method::GET);
    let cases: Vec<Case> = vec![
        (
            base().query_when_present("locale", Option::<&str>::None),
            &[("a", "1"), ("locale", "fr"), ("z", "9")],
            PatchKind::WhenPresent,
            None,
        ),
        (
            base().query_when_present("locale", Some("de")),
            &[("a", "1"), ("z", "9"), ("locale", "de")],
            PatchKind::WhenPresent,
            Some(&["de"]),
        ),
        (
            base().query_replace_optional("locale", Option::<&str>::None),
            &[("a", "1"), ("z", "9")],
            PatchKind::Replace,
            None,
        ),
        (
            base().query_append_optional("locale", Some("de")),
            &[("a", "1"), ("locale", "fr"), ("z", "9"), ("locale", "de")],
            PatchKind::Append,
            Some(&["de"]),
        ),
        (
            base().query_replace_each("locale", Vec::<&str>::new()),
            &[("a", "1"), ("z", "9")],
            PatchKind::Replace,
            Some(&[]),
        ),
        (
            base().query_when_present_each("locale", Some(Vec::<&str>::new())),
            &[("a", "1"), ("z", "9")],
            PatchKind::WhenPresent,
            Some(&[]),
        ),
        (
            base().query_when_present_each("locale", Option::<Vec<&str>>::None),
            &[("a", "1"), ("locale", "fr"), ("z", "9")],
            PatchKind::WhenPresent,
            None,
        ),
        (
            base().query_replace_each_optional("locale", Option::<Vec<&str>>::None),
            &[("a", "1"), ("z", "9")],
            PatchKind::Replace,
            None,
        ),
        (
            base().query_append_each("locale", vec!["x", "y"]),
            &[
                ("a", "1"),
                ("locale", "fr"),
                ("z", "9"),
                ("locale", "x"),
                ("locale", "y"),
            ],
            PatchKind::Append,
            Some(&["x", "y"]),
        ),
        (
            base().query_append_each_optional("locale", Option::<Vec<&str>>::None),
            &[("a", "1"), ("locale", "fr"), ("z", "9")],
            PatchKind::Append,
            None,
        ),
    ];
    for (builder, expected, kind, value) in cases {
        let runtime = plan(builder);
        assert_eq!(pairs(&runtime), pair_list(expected));
        // The same statement as a static owner operation yields the same
        // ordered program (one evaluator, PATCH-03).
        let static_owner = locale_owner(&core)
            .op(PatchFamily::Query, kind, "locale", value, false)
            .into_projection();
        let static_plan = test_owner::plan(
            &static_owner,
            &base()
                .response::<Empty, ()>()
                .build()
                .expect("empty runtime"),
        )
        .expect("static plan");
        assert_eq!(pairs(&static_plan), pair_list(expected));
    }
    assert!(pairs(&plan(base().query_off())).is_empty());
    assert_eq!(
        pairs(&plan(base().query_replace_all().query("b", "2"))),
        pair_list(&[("b", "2")])
    );
    assert_eq!(
        pairs(&plan(
            base()
                .query_remove("a")
                .query_via::<&'static str, Upper>("a", "up")
        )),
        pair_list(&[("locale", "fr"), ("z", "9"), ("a", "UP")])
    );
}

#[test]
fn suppressed_owner_encoders_never_run_and_survivors_run_once() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::new()))
        .expect("response");
    let core = core(&executor);
    // Owner emission 0 fails if its encoder is ever invoked (V3-S03).
    let owner = TestOwner::new(core.clone())
        .query("k", "owner")
        .query("kept", "yes")
        .failing(0)
        .into_projection();
    let runtime_calls = Arc::new(AtomicUsize::new(0));
    let endpoint = RuntimeEndpoint::builder(id("runtime.suppression"), Method::GET)
        .query_replace(
            "k",
            Tracked {
                text: "runtime",
                calls: Arc::clone(&runtime_calls),
            },
        )
        .query_append_optional_via::<&'static str, Failing>("absent", None)
        .response::<Empty, ()>()
        .build()
        .expect("runtime endpoint");
    let bound = test_owner::bind(&owner, endpoint).expect("binding");
    let plan = bound
        .plan()
        .expect("suppressed owner encoder cannot fail the plan");
    assert_eq!(
        pairs(&plan),
        pair_list(&[("kept", "yes"), ("k", "runtime")])
    );
    assert_eq!(owner.encoded(0), 0);
    assert_eq!(owner.encoded(1), 1);
    assert_eq!(runtime_calls.load(Ordering::SeqCst), 1);
    run(bound);
    assert_eq!(owner.encoded(0), 0);
    assert_eq!(owner.encoded(1), 2);
    assert_eq!(runtime_calls.load(Ordering::SeqCst), 2);

    // A surviving failing runtime adapter fails the preparation.
    let failing = RuntimeEndpoint::builder(id("runtime.failing_adapter"), Method::GET)
        .query_via::<&'static str, Failing>("f", "x")
        .response::<Empty, ()>()
        .build()
        .expect("runtime endpoint");
    let error = test_owner::plan(&owner, &failing).expect_err("surviving adapter failure");
    assert_eq!(error.category(), ErrorCategory::Configuration);
}

#[test]
fn sensitivity_is_sticky_and_path_segments_redact_whole() {
    let executor = DeterministicExecutor::new();
    let core = core(&executor);
    let owner = TestOwner::new(core.clone())
        .op(
            PatchFamily::Query,
            PatchKind::Introduce,
            "tenant",
            Some(&["owner-tenant"]),
            true,
        )
        .into_projection();
    let endpoint = RuntimeEndpoint::builder(id("runtime.sticky"), Method::GET)
        .path_sensitive(SENTINEL)
        .path_sensitive_via::<&'static str, Upper>("hidden")
        .query_replace("tenant", "public-replacement")
        .query_sensitive("secret", SENTINEL)
        .header_sensitive("x-private", SENTINEL)
        .header_each_sensitive_via::<Vec<&'static str>, Upper>("x-private-each", vec![SENTINEL])
        .response::<Empty, ()>()
        .build()
        .expect("sensitive runtime endpoint");
    let facts = endpoint
        .description()
        .runtime_binding()
        .expect("runtime facts")
        .clone();
    assert_eq!(facts.sensitive_path_segments(), 2);
    assert_eq!(
        facts
            .operations()
            .iter()
            .filter(|op| op.sensitive())
            .count(),
        3
    );
    let plan = test_owner::plan(&owner, &endpoint).expect("plan");
    let url = plan.observed_url();
    assert!(!url.contains(SENTINEL) && !url.contains("HIDDEN"), "{url}");
    assert!(
        url.contains("/base/%3Credacted%3E/%3Credacted%3E?"),
        "{url}"
    );
    // A later public override cannot declassify the owner's sensitive key.
    assert!(url.contains("tenant=%3Credacted%3E"), "{url}");
    assert!(!url.contains("public-replacement"), "{url}");
    let evidence = format!("{plan:?}{:?}", endpoint.description());
    assert!(!evidence.contains(SENTINEL));
}

// ---------------------------------------------------------------------------
// Authentication authority (RUNTIME-02, HEAD-01, V3-R10/R11).
// ---------------------------------------------------------------------------

#[cfg(feature = "default-tls")]
mod protected {
    use super::*;
    use crate::auth::{AccessToken, Auth, ChallengePolicy, CredentialRef, SlotSelector};

    fn https_core(executor: &DeterministicExecutor) -> ClientCore {
        ClientCore::build_with_development(
            "https://protected.example.test/v1/",
            HttpConfigBuilder::new().http1_only(),
            RuntimeConfigBuilder::new(),
            executor,
        )
        .expect("protected development core")
    }

    fn credential() -> CredentialRef<AccessToken> {
        CredentialRef::new(SlotSelector::static_value(
            AccessToken::try_new(SENTINEL).expect("bounded token"),
            Duration::ZERO,
        ))
    }

    fn protected_owner(core: &ClientCore, challenge: ChallengePolicy) -> Arc<TestOwner> {
        TestOwner::new(core.clone())
            .display("TestApi::protected")
            .auth(Auth::bearer(credential()).challenge(challenge))
            .auth(
                Auth::header(http::HeaderName::from_static("x-api-key"), credential())
                    .expect("header placement"),
            )
            .auth(Auth::query("api_key", credential()).expect("query placement"))
            .into_projection()
    }

    #[test]
    fn public_writers_cannot_target_private_auth_even_when_absent() {
        let executor = DeterministicExecutor::new();
        let core = https_core(&executor);
        let owner = protected_owner(&core, ChallengePolicy::Never);
        let bind = |builder: RuntimeEndpointBuilder| {
            test_owner::bind(
                &owner,
                builder
                    .response::<Empty, ()>()
                    .build()
                    .expect("runtime endpoint"),
            )
            .err()
            .and_then(|error| error.configuration_kind())
        };
        let base = || RuntimeEndpoint::builder(id("runtime.auth_targets"), Method::GET);
        let auth = Some(ConfigurationErrorKind::InvalidAuthentication);
        assert_eq!(
            bind(base().header_optional("X-Api-Key", Option::<&str>::None)),
            auth
        );
        assert_eq!(bind(base().header_append("x-api-key", "public")), auth);
        assert_eq!(
            bind(base().query_optional("api_key", Option::<&str>::None)),
            auth
        );
        assert_eq!(bind(base().query_append_each("api_key", vec!["x"])), auth);
        // Query keys stay case-sensitive; removing the placement frees it.
        assert_eq!(bind(base().query("API_KEY", "public")), None);
        assert_eq!(
            bind(
                base()
                    .auth_remove(AuthTarget::header("X-API-KEY").expect("target"))
                    .header("x-api-key", "public")
            ),
            None
        );
        assert_eq!(bind(base().auth_off().query("api_key", "public")), None);
    }

    /// Every runtime query statement shape that can carry an absent
    /// outer-optional source and introduces or appends a potential writer.
    fn absent_query_forms() -> Vec<(&'static str, KeyedForm)> {
        vec![
            form("query_optional", |builder, key| {
                builder.query_optional(key, Option::<PanicAtom>::None)
            }),
            form("query_optional_sensitive", |builder, key| {
                builder.query_optional_sensitive(key, Option::<PanicAtom>::None)
            }),
            form("query_optional_via", |builder, key| {
                builder.query_optional_via::<&'static str, Failing>(key, None)
            }),
            form("query_each_optional", |builder, key| {
                builder.query_each_optional(key, Option::<PanicSequence>::None)
            }),
            form("query_each_optional_sensitive_via", |builder, key| {
                builder.query_each_optional_sensitive_via::<PanicSequence, Failing>(key, None)
            }),
            form("query_append_optional", |builder, key| {
                builder.query_append_optional(key, Option::<PanicAtom>::None)
            }),
            form("query_append_each_optional", |builder, key| {
                builder.query_append_each_optional(key, Option::<PanicSequence>::None)
            }),
            // An absent conditional/exact override of an absent local
            // introduction: still two potential writers of the target.
            form("query_optional+query_when_present", |builder, key| {
                builder
                    .query_optional(key, Option::<PanicAtom>::None)
                    .query_when_present(key, Option::<PanicAtom>::None)
            }),
            form(
                "query_each_optional+query_replace_each_optional",
                |builder, key| {
                    builder
                        .query_each_optional(key, Option::<PanicSequence>::None)
                        .query_replace_each_optional(key, Option::<PanicSequence>::None)
                },
            ),
        ]
    }

    /// HEAD-01 / V3-S08 (runtime negative, private-auth family): every
    /// absent-capable public writer shape targeting an effective private
    /// placement is rejected by both `bind` and runtime planning at the owner
    /// although its value is absent. No value is encoded (panic atoms), and
    /// nothing is sent.
    #[test]
    fn every_absent_public_writer_shape_is_rejected_against_private_auth_targets() {
        let executor = DeterministicExecutor::new();
        let core = https_core(&executor);
        let owner = protected_owner(&core, ChallengePolicy::Never);
        let outcome = |builder: RuntimeEndpointBuilder| {
            let endpoint = builder
                .response::<Empty, ()>()
                .build()
                .expect("structurally valid runtime endpoint");
            let planned = test_owner::plan(&owner, &endpoint)
                .err()
                .and_then(|error| error.configuration_kind());
            let bound = test_owner::bind(&owner, endpoint)
                .err()
                .and_then(|error| error.configuration_kind());
            (planned, bound)
        };
        let base = || RuntimeEndpoint::builder(id("runtime.auth_absent"), Method::GET);
        let auth = Some(ConfigurationErrorKind::InvalidAuthentication);

        let mut header_forms: Vec<(&'static str, KeyedForm)> = absent_header_forms()
            .into_iter()
            // Override shapes need an earlier public writer; they are covered
            // below as overrides of an absent local introduction.
            .filter(|(name, _)| !name.contains("replace") && !name.contains("when_present"))
            .collect();
        header_forms.push(form(
            "header_optional+header_when_present",
            |builder, key| {
                builder
                    .header_optional(key, Option::<PanicAtom>::None)
                    .header_when_present(key, Option::<PanicAtom>::None)
            },
        ));
        header_forms.push(form(
            "header_optional+header_replace_optional",
            |builder, key| {
                builder
                    .header_optional(key, Option::<PanicAtom>::None)
                    .header_replace_optional(key, Option::<PanicAtom>::None)
            },
        ));
        for (form, apply) in &header_forms {
            // The private header target compares case-insensitively.
            for key in ["x-api-key", "X-API-KEY"] {
                assert_eq!(
                    outcome(apply(base(), key)),
                    (auth, auth),
                    "{form}(\"{key}\", None) against the private header placement"
                );
            }
            // Control: the same absent shape on a free header binds.
            assert_eq!(
                outcome(apply(base(), "x-public-hint")),
                (None, None),
                "{form} on a free header"
            );
        }
        for (form, apply) in absent_query_forms() {
            assert_eq!(
                outcome(apply(base(), "api_key")),
                (auth, auth),
                "{form}(\"api_key\", None) against the private query placement"
            );
            // Control: a case-distinct query key is a different target.
            assert_eq!(
                outcome(apply(base(), "API_KEY")),
                (None, None),
                "{form} on a case-distinct free query key"
            );
        }
        assert_eq!(
            executor
                .application()
                .request_count()
                .expect("request count"),
            0,
            "no rejected descriptor reached the transport"
        );
    }

    #[test]
    fn auth_removal_is_by_target_identity_and_displayed_prominently() {
        let executor = DeterministicExecutor::new();
        let core = https_core(&executor);
        let owner = protected_owner(&core, ChallengePolicy::Unauthorized);
        let base = || RuntimeEndpoint::builder(id("runtime.auth_removal"), Method::GET);

        let inherited = test_owner::bind(
            &owner,
            base().response::<Empty, ()>().build().expect("endpoint"),
        )
        .expect("inherited binding");
        let facts = inherited.description().runtime_binding().expect("facts");
        assert_eq!(facts.authentication(), &RuntimeAuthDisposition::Inherited);
        assert_eq!(facts.inherited_auth_placements(), 3);
        assert_eq!(facts.effective_auth_placements(), 3);
        assert_eq!(facts.owner(), Some("TestApi::protected"));
        assert!(inherited.description().capabilities().authentication());

        let partial = test_owner::bind(
            &owner,
            base()
                .auth_remove(AuthTarget::query("api_key").expect("target"))
                .auth_remove(AuthTarget::authorization())
                .response::<Empty, ()>()
                .build()
                .expect("endpoint"),
        )
        .expect("partial removal");
        let facts = partial.description().runtime_binding().expect("facts");
        assert!(facts.authentication().removes_inherited());
        assert_eq!(facts.effective_auth_placements(), 1);
        let plan = partial.plan().expect("plan");
        let rendered = format!("{plan:?}");
        assert!(
            rendered.starts_with(
                "RequestPlan { runtime_authentication: INHERITED AUTHENTICATION PARTIALLY REMOVED"
            ),
            "{rendered}"
        );
        assert!(rendered.contains("private query `api_key`"));
        assert!(!rendered.contains(SENTINEL));

        let off = test_owner::bind(
            &owner,
            base()
                .auth_off()
                .response::<Empty, ()>()
                .build()
                .expect("endpoint"),
        )
        .expect("auth off");
        let plan = off.plan().expect("plan");
        assert!(!plan.capabilities().authentication());
        assert_eq!(
            plan.runtime_binding().expect("facts").authentication(),
            &RuntimeAuthDisposition::RemovedAll
        );
        assert!(
            format!("{plan:?}").contains("INHERITED AUTHENTICATION REMOVED by runtime auth_off()")
        );

        // Removing a target no effective placement owns is rejected.
        let missing = test_owner::bind(
            &owner,
            base()
                .auth_remove(AuthTarget::header("x-other").expect("target"))
                .response::<Empty, ()>()
                .build()
                .expect("endpoint"),
        )
        .expect_err("missing removal target");
        assert_eq!(
            missing.configuration_kind(),
            Some(ConfigurationErrorKind::InvalidAuthentication)
        );
        assert!(AuthTarget::header("authorization").is_err());
        assert!(AuthTarget::query("").is_err());
    }

    #[test]
    fn accepted_statuses_cannot_overlap_an_effective_challenge() {
        let executor = DeterministicExecutor::new();
        let core = https_core(&executor);
        let status = |challenge, codes: &[u16]| {
            let owner = protected_owner(&core, challenge);
            let endpoint = RuntimeEndpoint::builder(id("runtime.overlap"), Method::GET)
                .status_codes(
                    codes
                        .iter()
                        .map(|code| StatusCode::from_u16(*code).expect("status")),
                )
                .response::<Empty, ()>()
                .build()
                .expect("endpoint");
            (
                test_owner::plan(&owner, &endpoint)
                    .err()
                    .and_then(|error| error.configuration_kind()),
                test_owner::bind(&owner, endpoint)
                    .err()
                    .and_then(|error| error.configuration_kind()),
            )
        };
        let rejected = Some(ConfigurationErrorKind::InvalidAuthentication);
        assert_eq!(
            status(ChallengePolicy::Unauthorized, &[200, 401]),
            (rejected, rejected)
        );
        assert_eq!(
            status(ChallengePolicy::UnauthorizedOrForbidden, &[403]),
            (rejected, rejected)
        );
        assert_eq!(status(ChallengePolicy::Unauthorized, &[403]), (None, None));
        assert_eq!(status(ChallengePolicy::Never, &[401]), (None, None));
        // Removing the challenging placement removes the overlap.
        let owner = protected_owner(&core, ChallengePolicy::Unauthorized);
        let endpoint = RuntimeEndpoint::builder(id("runtime.overlap"), Method::GET)
            .auth_remove(AuthTarget::authorization())
            .status_codes([StatusCode::UNAUTHORIZED])
            .response::<Empty, ()>()
            .build()
            .expect("endpoint");
        test_owner::bind(&owner, endpoint).expect("no effective challenge remains");
    }

    #[test]
    fn credentials_require_https_at_bind() {
        let executor = DeterministicExecutor::new();
        let cleartext = core(&executor);
        let owner = TestOwner::new(cleartext)
            .auth(Auth::bearer(credential()))
            .into_projection();
        let body_calls = Arc::new(AtomicUsize::new(0));
        let error = test_owner::bind(
            &owner,
            RuntimeEndpoint::builder(id("runtime.cleartext"), Method::POST)
                .body_repr::<CountingBody, _>(CountingBodyInput {
                    calls: Arc::clone(&body_calls),
                })
                .response::<Empty, ()>()
                .build()
                .expect("endpoint"),
        )
        .expect_err("credentials over cleartext");
        assert_eq!(
            error.configuration_kind(),
            Some(ConfigurationErrorKind::InvalidAuthentication)
        );
        assert_eq!(body_calls.load(Ordering::SeqCst), 0);
        assert_eq!(executor.application().request_count().expect("count"), 0);
        // Without inherited credentials the same cleartext owner binds.
        test_owner::bind(
            &owner,
            RuntimeEndpoint::builder(id("runtime.cleartext"), Method::POST)
                .auth_off()
                .response::<Empty, ()>()
                .build()
                .expect("endpoint"),
        )
        .expect("auth-free cleartext binding");
    }
}

// ---------------------------------------------------------------------------
// Identity, rate policies and limiter presence (V3-R11, RATE-01).
// ---------------------------------------------------------------------------

#[test]
fn generated_endpoint_id_collisions_cannot_bind_or_plan() {
    let executor = DeterministicExecutor::new();
    let core = core(&executor);
    let owner = TestOwner::new(core.clone())
        .generated_ids(&["test_api.items.list", "test_api.items.get"])
        .into_projection();
    let endpoint = || {
        RuntimeEndpoint::builder(id("test_api.items.get"), Method::GET)
            .response::<Empty, ()>()
            .build()
            .expect("endpoint")
    };
    for error in [
        test_owner::plan(&owner, &endpoint()).expect_err("collision plan"),
        test_owner::bind(&owner, endpoint()).expect_err("collision bind"),
    ] {
        assert_eq!(
            error.configuration_kind(),
            Some(ConfigurationErrorKind::InvalidValue)
        );
    }
    test_owner::bind(
        &owner,
        RuntimeEndpoint::builder(id("test_api.items.dynamic"), Method::GET)
            .response::<Empty, ()>()
            .build()
            .expect("endpoint"),
    )
    .expect("distinct runtime ID binds");
}

#[test]
fn missing_limiter_cannot_bypass_bind_and_noop_is_disclosed() {
    let executor = DeterministicExecutor::new();
    let endpoint = || {
        RuntimeEndpoint::builder(id("runtime.governed"), Method::GET)
            .rate_limit(rule("runtime.governed", "tenant-a"))
            .response::<Empty, ()>()
            .build()
            .expect("governed endpoint")
    };
    let unselected = owner(&core(&executor));
    for error in [
        test_owner::plan(&unselected, &endpoint()).expect_err("plan without limiter"),
        test_owner::bind(&unselected, endpoint()).expect_err("bind without limiter"),
    ] {
        assert_eq!(
            error.configuration_kind(),
            Some(ConfigurationErrorKind::InvalidRateLimit)
        );
    }
    let noop = owner(&core_with(
        &executor,
        RuntimeConfigBuilder::new().rate_limiter(Arc::new(NoopRateLimiter::new())),
    ));
    let plan = test_owner::plan(&noop, &endpoint()).expect("explicit noop");
    assert_eq!(plan.rate_limit_policy_count(), 1);
    assert_eq!(
        plan.rate_limit().enforcement_label(),
        "declared policies; enforcement explicitly disabled by application selection"
    );
}

#[test]
fn runtime_rate_policies_obey_duplicate_and_structural_rules() {
    let many = |mut builder: RuntimeEndpointBuilder| {
        for index in 0..MAX_RATE_LIMIT_POLICIES_PER_PLAN {
            builder = builder.rate_limit(rule(&format!("rule-{index}"), "tenant"));
        }
        builder
    };
    assert_eq!(
        build_kind(many(RuntimeEndpoint::builder(
            id("runtime.rate"),
            Method::GET
        ))),
        None
    );
    let error = many(RuntimeEndpoint::builder(id("runtime.rate"), Method::GET))
        .rate_limit(rule(RATE_ID_SENTINEL, RATE_KEY_SENTINEL))
        .response::<Empty, ()>()
        .build()
        .expect_err("first excess policy");
    assert_eq!(error.kind(), ConfigurationErrorKind::InvalidRateLimit);
    assert!(!format!("{error:?}").contains(RATE_ID_SENTINEL));

    let conflict = RuntimeEndpoint::builder(id("runtime.rate"), Method::GET)
        .rate_limit(rule(RATE_ID_SENTINEL, RATE_KEY_SENTINEL))
        .rate_limit(rule(RATE_ID_SENTINEL, "other"))
        .response::<Empty, ()>()
        .build()
        .expect_err("conflicting runtime ID");
    assert_eq!(conflict.kind(), ConfigurationErrorKind::InvalidRateLimit);
    assert!(!format!("{conflict:?}").contains(RATE_KEY_SENTINEL));
    let exact = RuntimeEndpoint::builder(id("runtime.rate"), Method::GET)
        .rate_limit(rule(RATE_ID_SENTINEL, RATE_KEY_SENTINEL))
        .rate_limit(rule(RATE_ID_SENTINEL, RATE_KEY_SENTINEL))
        .response::<Empty, ()>()
        .build()
        .expect("exact repetition deduplicates");
    assert_eq!(
        exact
            .description()
            .runtime_binding()
            .expect("facts")
            .rate_policies()
            .len(),
        1
    );
    for invalid in [
        RateLimitDeclaration::new(RATE_ID_SENTINEL).part("tenant", RATE_KEY_SENTINEL),
        RateLimitDeclaration::new(RATE_ID_SENTINEL).window(1, Duration::from_secs(1)),
        rule(RATE_ID_SENTINEL, RATE_KEY_SENTINEL).part("origin", RATE_KEY_SENTINEL),
        rule(RATE_ID_SENTINEL, RATE_KEY_SENTINEL).window(9, Duration::from_secs(1)),
        rule(RATE_ID_SENTINEL, RATE_KEY_SENTINEL).cost(11),
    ] {
        let error = RuntimeEndpoint::builder(id("runtime.rate"), Method::GET)
            .rate_limit(invalid)
            .response::<Empty, ()>()
            .build()
            .expect_err("invalid declaration");
        assert_eq!(error.kind(), ConfigurationErrorKind::InvalidRateLimit);
        assert!(!format!("{error:?}").contains(RATE_KEY_SENTINEL));
    }
}

#[test]
fn inherited_policies_remain_and_deduplicate_or_conflict_at_bind() {
    let executor = DeterministicExecutor::new();
    let limiter = Arc::new(CountingLimiter::default());
    let core = core_with(
        &executor,
        RuntimeConfigBuilder::new().rate_limiter(limiter.clone()),
    );
    let inherited = || {
        RateLimitDeclaration::new("owner.tenant")
            .window(5, Duration::from_secs(1))
            .origin()
            .part("tenant", "tenant-a")
    };
    let owner = TestOwner::new(core.clone())
        .host("eu")
        .rate(inherited())
        .into_projection();
    let endpoint = |declaration: RateLimitDeclaration| {
        RuntimeEndpoint::builder(id("runtime.inherited_rates"), Method::GET)
            .rate_limit(declaration)
            .rate_limit(
                RateLimitDeclaration::new("runtime.dynamic")
                    .window(10, Duration::from_secs(1))
                    .origin()
                    .endpoint(),
            )
            .response::<Empty, ()>()
            .build()
            .expect("runtime rates")
    };
    let bound = test_owner::bind(&owner, endpoint(inherited())).expect("exact duplicate");
    let facts = bound.description().runtime_binding().expect("facts");
    assert_eq!(facts.effective_rate_policies(), 2);
    let plan = bound.plan().expect("plan");
    assert_eq!(plan.rate_limit_policy_count(), 2);
    assert_eq!(
        plan.rate_limit().policies()[0].schema().id().as_str(),
        "owner.tenant"
    );
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 0);
    let conflict = test_owner::bind(&owner, endpoint(inherited().cost(2)))
        .expect_err("a runtime attachment cannot redefine an inherited policy");
    assert_eq!(
        conflict.configuration_kind(),
        Some(ConfigurationErrorKind::InvalidRateLimit)
    );
}

// ---------------------------------------------------------------------------
// Projection, plan/send agreement, inertness and the bound request surface.
// ---------------------------------------------------------------------------

fn scope_owner(core: &ClientCore) -> Arc<TestOwner> {
    TestOwner::new(core.clone())
        .display("TestApi::regional")
        .timeout(Duration::from_secs(5))
        .host("api")
        .segment("v2")
        .segment("regions")
        .segment("west")
        .query("tenant", "tenant-a")
        .op(
            PatchFamily::Query,
            PatchKind::Append,
            "view",
            Some(&["scope"]),
            false,
        )
        .header("x-scope", "west")
        .rate(rule("runtime.scope.tenant", "tenant-a"))
        .into_projection()
}

#[test]
fn owner_projection_is_carried_and_the_plan_matches_the_send() {
    let executor = DeterministicExecutor::new();
    let limiter = Arc::new(CountingLimiter::default());
    let core = core_with(
        &executor,
        RuntimeConfigBuilder::new().rate_limiter(limiter.clone()),
    );
    let owner = scope_owner(&core);
    let endpoint = RuntimeEndpoint::builder(id("runtime.scope.items"), Method::GET)
        .path("items")
        .query("page", 2_u32)
        .header("x-runtime", "yes")
        .response::<Empty, ()>()
        .build()
        .expect("scope runtime endpoint");
    let bound = test_owner::bind(&owner, endpoint).expect("scope binding");
    let plan = bound.plan().expect("scope plan");
    assert_eq!(
        plan.observed_url(),
        "http://api.runtime.example.test/base/v2/regions/west/items?tenant=tenant-a&view=scope&page=2"
    );
    assert_eq!(plan.resolved_timeout(), Some(Duration::from_secs(5)));
    assert_eq!(plan.rate_limit_policy_count(), 1);
    assert!(plan.capabilities().rate_limits());
    assert_eq!(
        plan.description().source_path(),
        "TestApi::regional::<runtime>"
    );
    assert_eq!(
        plan.description().route_shape(),
        "<TestApi::regional route>/<runtime-segment>"
    );
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 0);

    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .observed_url(plan.observed_url().to_owned())
                .header(
                    http::HeaderName::from_static("x-scope"),
                    HeaderValue::from_static("west"),
                )
                .header(
                    http::HeaderName::from_static("x-runtime"),
                    HeaderValue::from_static("yes"),
                ),
            ScriptedResponse::success(Bytes::new()),
        )
        .expect("expected send");
    run(bound);
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(executor.application().request_count().expect("count"), 1);

    // Replacement and fences keep the fixed origin and base path.
    let replaced = RuntimeEndpoint::builder(id("runtime.scope.replaced"), Method::GET)
        .path_replace()
        .path("replacement")
        .query_replace("tenant", "runtime")
        .header_off()
        .timeout(Duration::from_secs(9))
        .response::<Empty, ()>()
        .build()
        .expect("replaced runtime endpoint");
    let plan = test_owner::plan(&owner, &replaced).expect("replaced plan");
    assert_eq!(
        plan.observed_url(),
        "http://api.runtime.example.test/base/replacement?view=scope&tenant=runtime"
    );
    assert!(
        plan.public_headers()
            .get(&http::HeaderName::from_static("x-scope"))
            .is_none()
    );
    assert_eq!(plan.resolved_timeout(), Some(Duration::from_secs(9)));
    let off = RuntimeEndpoint::builder(id("runtime.scope.off"), Method::GET)
        .path_off()
        .query_off()
        .response::<Empty, ()>()
        .build()
        .expect("path off");
    assert_eq!(
        test_owner::plan(&owner, &off)
            .expect("off plan")
            .observed_url(),
        "http://api.runtime.example.test/base/"
    );
}

#[test]
fn plan_and_bind_do_no_body_credential_limiter_or_transport_work() {
    let executor = DeterministicExecutor::new();
    let limiter = Arc::new(CountingLimiter::default());
    let core = core_with(
        &executor,
        RuntimeConfigBuilder::new().rate_limiter(limiter.clone()),
    );
    let owner = scope_owner(&core);
    let body_calls = Arc::new(AtomicUsize::new(0));
    let endpoint = RuntimeEndpoint::builder(id("runtime.inert_plan"), Method::POST)
        .body_repr::<CountingBody, _>(CountingBodyInput {
            calls: Arc::clone(&body_calls),
        })
        .rate_limit(rule("runtime.inert", "tenant-a"))
        .response::<Binary, Bytes>()
        .expose_accepted_bytes()
        .build()
        .expect("governed body endpoint");
    let plan = test_owner::plan(&owner, &endpoint).expect("plan");
    assert_eq!(plan.body_category(), RequestBodyCategory::Specification);
    let bound = test_owner::bind(&owner, endpoint).expect("bind");
    let _ = bound.plan().expect("bound plan");
    assert_eq!(body_calls.load(Ordering::SeqCst), 0);
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 0);
    assert_eq!(executor.application().request_count().expect("count"), 0);

    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"accepted")))
        .expect("response");
    let bytes = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(
            bound
                .accepted_bytes()
                .expect("exposed accepted bytes")
                .into_future(),
        )
        .expect("accepted bytes execution");
    assert_eq!(bytes.value(), &Bytes::from_static(b"accepted"));
    assert_eq!(body_calls.load(Ordering::SeqCst), 1);
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 1);
}

#[test]
fn accepted_bytes_exposure_is_explicit_on_the_descriptor() {
    let executor = DeterministicExecutor::new();
    let core = core(&executor);
    let unexposed = test_owner::root(
        &core,
        RuntimeEndpoint::builder(id("runtime.unexposed"), Method::GET)
            .response::<Text, String>()
            .build()
            .expect("endpoint"),
    );
    assert!(!unexposed.description().capabilities().accepted_bytes());
    let error = unexposed
        .accepted_bytes()
        .expect_err("not exposed by the descriptor");
    assert_eq!(
        error.configuration_kind(),
        Some(ConfigurationErrorKind::InvalidValue)
    );
    assert_eq!(executor.application().request_count().expect("count"), 0);
    let twice = RuntimeEndpoint::builder(id("runtime.twice"), Method::GET)
        .response::<Text, String>()
        .expose_accepted_bytes()
        .expose_accepted_bytes()
        .build()
        .expect_err("singleton exposure");
    assert_eq!(twice.kind(), ConfigurationErrorKind::InvalidValue);
}

#[test]
fn bound_requests_expose_the_common_control_interface() {
    fn generic<T: crate::controls::RequestControls>(request: T) -> T {
        request
            .with_options(&CallOptions::new().cap_sends(2))
            .cancel_with(CancellationToken::new())
            .expect("one cancellation source")
    }
    let executor = DeterministicExecutor::new();
    let core = core(&executor);
    let owner = TestOwner::new(core.clone())
        .timeout(Duration::from_secs(20))
        .into_projection();
    let bound = test_owner::bind(
        &owner,
        RuntimeEndpoint::builder(id("runtime.controls"), Method::GET)
            .query_sensitive("secret", SENTINEL)
            .response::<Empty, ()>()
            .build()
            .expect("endpoint"),
    )
    .expect("binding");
    let bound = generic(bound)
        .timeout(Duration::from_secs(3))
        .expect("finite timeout");
    let summary = format!("{:?}", bound.control_summary());
    assert!(summary.contains("20s"), "{summary}");
    assert!(summary.contains("3s"), "{summary}");
    let reset = bound.reset_timeout();
    assert!(reset.timeout(Duration::ZERO).is_err());
    let rendered = format!(
        "{:?}",
        test_owner::root(
            &core,
            RuntimeEndpoint::builder(id("runtime.debug"), Method::GET)
                .query_sensitive("secret", SENTINEL)
                .response::<Empty, ()>()
                .build()
                .expect("endpoint"),
        )
    );
    assert!(rendered.contains("runtime.debug"));
    assert!(!rendered.contains(SENTINEL));
}

#[test]
fn descriptions_contain_shape_and_names_but_no_values() {
    let endpoint = RuntimeEndpoint::builder(id("runtime.description"), Method::POST)
        .path(SENTINEL)
        .query("q", SENTINEL)
        .query_when_present_each_via::<Vec<&'static str>, Upper>("w", Some(vec![SENTINEL]))
        .header("x-public", SENTINEL)
        .body_repr::<Text, _>(SENTINEL.to_owned())
        .response::<Binary, Bytes>()
        .build()
        .expect("description endpoint");
    let description = endpoint.description();
    let rendered = format!("{description:?}{endpoint:?}");
    assert!(!rendered.contains(SENTINEL));
    assert_eq!(description.route_shape(), "<owner-route>/<runtime-segment>");
    let facts = description.runtime_binding().expect("facts");
    assert_eq!(facts.owner(), None);
    let operations = facts.operations();
    assert_eq!(operations.len(), 3);
    assert_eq!(operations[1].key(), "w");
    assert_eq!(operations[1].operation(), RuntimeOperation::WhenPresent);
    assert!(operations[1].repeated() && operations[1].optional());
    assert!(
        operations[1]
            .adapter()
            .is_some_and(|name| name.ends_with("Upper"))
    );
    assert_eq!(operations[2].family(), RuntimeFamily::Header);
    assert!(
        description
            .body_specification()
            .is_some_and(|name| name.ends_with("Text"))
    );
}
