//! EXPLAIN-01 (§18.1): static metadata of a real generated client evaluates
//! no payload default, invokes no converter and encodes no value.
//!
//! `StaticApi` is used by exactly one test in this target, so the first call
//! to `StaticApi::contract()` (its lazy initialization) happens inside that
//! test, between two counter snapshots. Every application hook the client
//! reaches is counted: root, scope and endpoint `#[default]` payloads, the
//! drop of a constant initializer's retained copy (materialization), public
//! atoms, an explicit atom adapter, a rate-key encoder, the request-body
//! representation and the response representation.

#![forbid(unsafe_code)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use concord::body::{
    BodyDescription, BodyError, PreparedBody, RequestBodyContext, RequestRepresentation,
};
use concord::rate_limit::{CanonicalKeyWriter, NoopRateLimiter, RateKeyEncoder, RateLimitError};
use concord::response::{Buffered, ResponseBodyError, ResponseHead, ResponseRepresentation};
use concord::{AtomEncoder, AtomError, BoundedAtomWriter, Bytes, RequestAtom, api};
use concord_inspect::{explain_endpoint, explain_policy, export_string};

static ROOT_DEFAULTS: AtomicUsize = AtomicUsize::new(0);
static SCOPE_DEFAULTS: AtomicUsize = AtomicUsize::new(0);
static ENDPOINT_DEFAULTS: AtomicUsize = AtomicUsize::new(0);
static MARKER_DROPS: AtomicUsize = AtomicUsize::new(0);
static ATOM_WRITES: AtomicUsize = AtomicUsize::new(0);
static ADAPTER_WRITES: AtomicUsize = AtomicUsize::new(0);
static KEY_WRITES: AtomicUsize = AtomicUsize::new(0);
static BODY_PREPARES: AtomicUsize = AtomicUsize::new(0);
static RESPONSE_CALLS: AtomicUsize = AtomicUsize::new(0);

/// Every counter, in a fixed order, with its name.
fn counters() -> [(&'static str, usize); 9] {
    [
        ("root #[default]", ROOT_DEFAULTS.load(Ordering::SeqCst)),
        ("scope #[default]", SCOPE_DEFAULTS.load(Ordering::SeqCst)),
        (
            "endpoint #[default]",
            ENDPOINT_DEFAULTS.load(Ordering::SeqCst),
        ),
        (
            "constant initializer drops",
            MARKER_DROPS.load(Ordering::SeqCst),
        ),
        (
            "RequestAtom::write_atom",
            ATOM_WRITES.load(Ordering::SeqCst),
        ),
        ("AtomEncoder::write", ADAPTER_WRITES.load(Ordering::SeqCst)),
        (
            "RateKeyEncoder::write_canonical",
            KEY_WRITES.load(Ordering::SeqCst),
        ),
        (
            "RequestRepresentation::prepare",
            BODY_PREPARES.load(Ordering::SeqCst),
        ),
        (
            "ResponseRepresentation callbacks",
            RESPONSE_CALLS.load(Ordering::SeqCst),
        ),
    ]
}

macro_rules! counted_default {
    ($name:ident, $counter:ident, $text:literal) => {
        /// A payload whose `Default` and atom encoding are counted.
        pub struct $name;

        impl Default for $name {
            fn default() -> Self {
                $counter.fetch_add(1, Ordering::SeqCst);
                Self
            }
        }

        impl RequestAtom for $name {
            fn write_atom(&self, output: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
                ATOM_WRITES.fetch_add(1, Ordering::SeqCst);
                output.push_str($text)
            }
        }
    };
}

counted_default!(RootProbe, ROOT_DEFAULTS, "root");
counted_default!(ScopeProbe, SCOPE_DEFAULTS, "scope");
counted_default!(EndpointProbe, ENDPOINT_DEFAULTS, "endpoint");

/// A constant initializer whose materialized copies are counted on drop.
pub struct DropMarker;

impl Drop for DropMarker {
    fn drop(&mut self) {
        MARKER_DROPS.fetch_add(1, Ordering::SeqCst);
    }
}

impl RequestAtom for DropMarker {
    fn write_atom(&self, output: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        ATOM_WRITES.fetch_add(1, Ordering::SeqCst);
        output.push_str("marker")
    }
}

/// The client's constant initializer.
pub const MARKER: DropMarker = DropMarker;

/// An explicit atom adapter that counts its calls.
pub struct CountingEncoder;

impl AtomEncoder<String> for CountingEncoder {
    fn write(value: &String, output: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        ADAPTER_WRITES.fetch_add(1, Ordering::SeqCst);
        output.push_str(value)
    }
}

/// A rate-key encoder that counts its calls.
pub struct CountingKey;

impl RateKeyEncoder<String> for CountingKey {
    const ENCODING_ID: &'static str = "test.static-metadata/1";

    fn write_canonical(value: &String, out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError> {
        KEY_WRITES.fetch_add(1, Ordering::SeqCst);
        out.write_tagged_bytes("test.static", value.as_bytes())
    }
}

/// The request body input.
pub struct Input(pub &'static [u8]);

/// A request representation that counts preparations.
pub struct CountingBody;

impl RequestRepresentation<Input> for CountingBody {
    const DESCRIPTION: BodyDescription = BodyDescription::buffered(Some("application/x-static"));

    fn prepare(input: Input, _context: RequestBodyContext<'_>) -> Result<PreparedBody, BodyError> {
        BODY_PREPARES.fetch_add(1, Ordering::SeqCst);
        PreparedBody::bytes(Bytes::from_static(input.0)).content_type("application/x-static")
    }
}

/// The decoded response.
pub struct Label(pub String);

/// A response representation with static negotiation that counts callbacks.
pub struct CountingResponse;

impl ResponseRepresentation<Label> for CountingResponse {
    type Driver = Buffered;
    const ACCEPT: Option<&'static str> = Some("application/x-static");

    fn validate(_head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        RESPONSE_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn from_body(body: Bytes, _head: &ResponseHead<'_>) -> Result<Label, ResponseBodyError> {
        RESPONSE_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(Label(String::from_utf8_lossy(&body).into_owned()))
    }
}

api! {
    /// Static-metadata probe client.
    client StaticApi {
        base "https://static.example.test/";
        timeout 5s;

        values {
            tenant: String;
            #[default]
            header("X-Root") root_probe: RootProbe;
            header("X-Marker") marker: DropMarker = crate::MARKER;
            adapted: String = "adapted";
        }

        headers {
            "X-Adapted" = adapted via CountingEncoder;
        }

        scope group(#[default] header("X-Scope") scope_probe: ScopeProbe) {
            path "group";

            /// Writes one input.
            POST write(
                #[default] header("X-Endpoint") endpoint_probe: EndpointProbe,
                body input: Input via CountingBody,
            ) -> Label via CountingResponse {
                id "static.group.write";
                path "write";
                rate_limit {
                    id "static.write";
                    key [origin, "tenant" = client.tenant via CountingKey];
                    allow 5 per 1s;
                }
            }
        }
    }
}

/// EXPLAIN-01: the first `StaticApi::contract()` call, a canonical export
/// and the endpoint/policy explanations run none of the counted hooks; an
/// unbound intent evaluates exactly its scope and endpoint defaults once (the
/// §4.2 owner boundary), and its `description()` evaluates and encodes
/// nothing more.
#[test]
fn static_metadata_evaluates_no_default_and_runs_no_extension_code() {
    let untouched = counters();
    assert!(
        untouched.iter().all(|(_, count)| *count == 0),
        "{untouched:?}"
    );

    let contract = StaticApi::contract();
    assert!(
        std::ptr::eq(contract, StaticApi::contract()),
        "allocated once"
    );
    let export = export_string(contract);
    assert!(export.contains("\"static.group.write\""));
    assert!(
        export.contains("application/x-static"),
        "static metadata only"
    );
    for endpoint in contract.endpoints() {
        let text = explain_endpoint(contract, endpoint);
        assert!(text.contains(endpoint.id()), "{text}");
        let _ = contract.url_template(endpoint);
    }
    for policy in contract.policies() {
        assert!(explain_policy(contract, policy).contains("static.write"));
    }
    assert_eq!(counters(), untouched, "contract(), export and explanations");

    let intent = StaticApi::unbound().group().write(Input(b"body"));
    let constructed = counters();
    assert_eq!(constructed[1], ("scope #[default]", 1));
    assert_eq!(constructed[2], ("endpoint #[default]", 1));
    for (index, (name, count)) in constructed.iter().enumerate() {
        if index != 1 && index != 2 {
            assert_eq!(*count, 0, "constructing an unbound intent ran {name}");
        }
    }

    let description = intent.description();
    assert_eq!(description.id(), "static.group.write");
    assert_eq!(counters(), constructed, "description()");
    drop(intent);
    assert_eq!(
        counters()[..3],
        constructed[..3],
        "dropping the intent evaluates no default"
    );

    // Positive control: the same hooks do run once a client is built and a
    // request is planned, so the zero counts above are meaningful. Planning
    // still prepares no body and converts no response.
    let client = StaticApi::builder()
        .set_tenant("tenant-a")
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(NoopRateLimiter)))
        .build()
        .expect("client");
    assert_eq!(ROOT_DEFAULTS.load(Ordering::SeqCst), 1, "builder creation");
    let plan = client.group().write(Input(b"body")).plan().expect("plan");
    assert_eq!(plan.endpoint_id(), "static.group.write");
    assert!(
        ATOM_WRITES.load(Ordering::SeqCst) > 0,
        "planning encodes atoms"
    );
    assert!(
        ADAPTER_WRITES.load(Ordering::SeqCst) > 0,
        "planning runs adapters"
    );
    assert!(
        KEY_WRITES.load(Ordering::SeqCst) > 0,
        "planning encodes rate keys"
    );
    assert_eq!(
        BODY_PREPARES.load(Ordering::SeqCst),
        0,
        "plans prepare no body"
    );
    assert_eq!(RESPONSE_CALLS.load(Ordering::SeqCst), 0);
}
