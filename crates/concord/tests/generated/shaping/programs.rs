//! Symbolic query/header patch programs, suppressed adapters, header key
//! semantics and privacy of generated requests (§6.1–§6.4).

use std::error::Error as _;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use concord::endpoint::RequestPlan;
use concord::error::{ConfigurationErrorKind, ErrorCategory};
use concord::http::HeaderName;
use concord::inspection::{Sensitivity, ValueLiveness};
use concord::observation::{OperationEvent, OperationObserver, SanitizedHeaderValue};
use concord::retry::{RetryDecision, RetryPolicy};
use concord::{AtomEncoder, AtomError, AtomErrorKind, Binary, BoundedAtomWriter, Bytes, api};
use concord_test_support::server::{ResponseScript, ScriptedServer};

use super::request_shaping_test_guard;
use crate::support::loopback_proxy;

api! {
    client ProgramApi {
        base "http://programs.example.test/";
        timeout 5s;

        query {
            "a" = "1";
            "locale" = "fr";
            "z" = "9";
            "tag" = "p1";
            append "tag" = "p2";
        }
        headers {
            "X-Mode" = "parent";
        }

        GET conditional(locale?: String, tags?: Vec<String>, mode?: String) -> Bytes via Binary {
            query {
                when_present "locale" = locale;
                when_present "tag" = each tags;
            }
            // §6.2: header names compare case-insensitively, so this targets
            // the inherited `X-Mode`.
            headers { when_present "x-mode" = mode; }
        }

        GET replaced(locale?: String, tags?: Vec<String>, mode?: String) -> Bytes via Binary {
            query {
                replace "locale" = locale;
                replace "tag" = each tags;
            }
            headers { replace "x-mode" = mode; }
        }

        GET appended(locale?: String, tags?: Vec<String>, mode?: String) -> Bytes via Binary {
            query {
                append "locale" = locale;
                append "tag" = each tags;
            }
            headers { append "X-MODE" = mode; }
        }

        GET removed() -> Bytes via Binary {
            query {
                remove "locale";
                remove "tag";
            }
            headers { remove "x-mode"; }
        }

        GET introduced(fresh?: String, many?: Vec<String>, scalar: String) -> Bytes via Binary {
            query {
                "fresh" = fresh;
                "many" = each many;
                // §6.2: query keys are case-sensitive, so `Locale` is a new
                // key beside the inherited `locale`, not a conflict.
                "Locale" = scalar;
            }
        }
    }
}

api! {
    client NormalizedApi {
        base "http://normalized.example.test/";
        timeout 5s;

        query { "root" = "r"; }

        // §6.1: direct placements are step 2 and explicit block statements
        // step 3, whatever their source order.
        scope ordered(query("direct") direct: String) {
            query { "block" = "b"; }

            GET read(query("leaf_direct") leaf: String) -> Bytes via Binary {
                query { "leaf_block" = "lb"; }
            }
        }

        // `query replace` clears inheritance before both the direct
        // placement and the replacement block's own statements.
        scope fenced(query("direct") direct: String) {
            query replace { "only" = "this"; }

            GET read() -> Bytes via Binary;
        }
    }
}

static SUPPRESSED_ADAPTER_CALLS: AtomicUsize = AtomicUsize::new(0);
static SURVIVING_ADAPTER_CALLS: AtomicUsize = AtomicUsize::new(0);
static ITEM_ADAPTER_CALLS: AtomicUsize = AtomicUsize::new(0);

/// A parent adapter that counts calls and always fails: if a suppressed
/// ancestor emission ever ran it, the request would fail (PATCH-03).
pub struct SuppressedParent;

impl AtomEncoder<String> for SuppressedParent {
    fn write(_value: &String, _out: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        SUPPRESSED_ADAPTER_CALLS.fetch_add(1, Ordering::SeqCst);
        Err(AtomError::new(AtomErrorKind::Encode))
    }
}

/// A surviving scalar adapter that counts its invocations.
pub struct SurvivingAdapter;

impl AtomEncoder<String> for SurvivingAdapter {
    fn write(value: &String, out: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        SURVIVING_ADAPTER_CALLS.fetch_add(1, Ordering::SeqCst);
        out.push_str(&value.to_ascii_uppercase())
    }
}

/// A per-item adapter for `each` sources that counts its invocations.
pub struct CountingItem;

impl AtomEncoder<String> for CountingItem {
    fn write(value: &String, out: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        ITEM_ADAPTER_CALLS.fetch_add(1, Ordering::SeqCst);
        out.push_str(value)
    }
}

api! {
    client AdapterApi {
        base "http://adapters.example.test/";
        timeout 5s;

        values {
            tenant: String = "tenant-a";
        }

        query { "tenant" = tenant via SuppressedParent; }
        headers { "X-Tenant" = tenant via SuppressedParent; }

        GET inherited() -> Bytes via Binary;

        GET replaced(label: String, items: Vec<String>) -> Bytes via Binary {
            query {
                replace "tenant" = label via SurvivingAdapter;
                "item" = each items via CountingItem;
            }
            headers { remove "X-Tenant"; }
        }

        scope fenced() {
            query replace { "label" = client.tenant via SurvivingAdapter; }
            headers off;

            GET read() -> Bytes via Binary;
        }
    }
}

const TENANT_SENTINEL: &str = "tenant-sentinel-4b1e";
const TAG_SENTINEL: &str = "tag-sentinel-93aa";
const REDACTED: &str = "%3Credacted%3E";

api! {
    client PrivacyApi {
        base "http://privacy.example.test/root/";
        timeout 5s;

        values {
            #[sensitive] tenant: String;
            #[sensitive] tags?: Vec<String>;
            region: String = "eu";
        }

        query {
            "tenant" = tenant;
            "tag" = each tags;
        }
        headers {
            "X-Tenant" = concat ["t-", tenant];
        }

        GET inherited() -> Bytes via Binary {
            path "inherited";
        }

        GET replaced(label: String) -> Bytes via Binary {
            path "replaced";
            query {
                replace "tenant" = label;
                replace "tag" = label;
            }
            headers { replace "X-Tenant" = label; }
        }

        scope fenced() {
            query replace { "tenant" = client.region; }
            headers off;

            GET read() -> Bytes via Binary {
                path "fenced";
            }
        }

        scope hosted() {
            subdomain [client.tenant];

            GET read(id: String) -> Bytes via Binary {
                path "items/{id}-{client.tenant}/{id}";
            }
        }
    }
}

fn header_values(plan: &RequestPlan, name: &'static str) -> Vec<String> {
    let name = HeaderName::from_static(name);
    plan.public_headers()
        .iter()
        .filter(|(candidate, _)| **candidate == name)
        .map(|(_, value)| match value {
            SanitizedHeaderValue::Text(text) => text.to_owned(),
            other => format!("{other:?}"),
        })
        .collect()
}

fn query_of(plan: &RequestPlan) -> &str {
    plan.observed_url()
        .split_once('?')
        .map_or("", |(_, query)| query)
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

/// V3-S01/V3-S04: every operation over a scalar, an absent source, a
/// present-empty `each` and an ordered `each`, with exact final order. The
/// parent program is `a=1, locale=fr, z=9, tag=p1, tag=p2` and `X-Mode:
/// parent` (the §6.3 example extended to sequences and headers).
#[test]
fn patch_operations_follow_the_selected_laws_in_exact_order() {
    let client = ProgramApi::new().expect("program client");
    let plan = |request: program_api::ConditionalRequest| request.plan().expect("plan");

    // when_present: absent preserves original occurrences and positions.
    let absent = plan(client.conditional());
    assert_eq!(query_of(&absent), "a=1&locale=fr&z=9&tag=p1&tag=p2");
    assert_eq!(header_values(&absent, "x-mode"), ["parent"]);
    // Some replaces at the new position; present-empty `each` clears.
    let present = plan(
        client
            .conditional()
            .set_locale("de")
            .set_tags(Vec::new())
            .set_mode("child"),
    );
    assert_eq!(query_of(&present), "a=1&z=9&locale=de");
    assert_eq!(header_values(&present, "x-mode"), ["child"]);
    let sequence = plan(client.conditional().set_tags(strings(&["x", "y"])));
    assert_eq!(query_of(&sequence), "a=1&locale=fr&z=9&tag=x&tag=y");
    // Exact `None` through `replace_*` is outer absence: parent retained.
    let exact_none = plan(client.conditional().set_locale("de").replace_locale(None));
    assert_eq!(query_of(&exact_none), "a=1&locale=fr&z=9&tag=p1&tag=p2");

    // replace: absent and present-empty both remove previous occurrences.
    let replaced = client.replaced().plan().expect("replace absent");
    assert_eq!(query_of(&replaced), "a=1&z=9");
    assert!(header_values(&replaced, "x-mode").is_empty());
    let replaced = client
        .replaced()
        .set_locale("de")
        .set_tags(Vec::new())
        .set_mode("child")
        .plan()
        .expect("replace present");
    assert_eq!(query_of(&replaced), "a=1&z=9&locale=de");
    assert_eq!(header_values(&replaced, "x-mode"), ["child"]);
    let replaced = client
        .replaced()
        .set_tags(strings(&["x"]))
        .plan()
        .expect("replace sequence");
    assert_eq!(query_of(&replaced), "a=1&z=9&tag=x");

    // append: absent and present-empty change nothing; present appends.
    let appended = client.appended().plan().expect("append absent");
    assert_eq!(query_of(&appended), "a=1&locale=fr&z=9&tag=p1&tag=p2");
    assert_eq!(header_values(&appended, "x-mode"), ["parent"]);
    let appended = client
        .appended()
        .set_locale("de")
        .set_tags(Vec::new())
        .set_mode("child")
        .plan()
        .expect("append present");
    assert_eq!(
        query_of(&appended),
        "a=1&locale=fr&z=9&tag=p1&tag=p2&locale=de"
    );
    assert_eq!(header_values(&appended, "x-mode"), ["parent", "child"]);
    let appended = client
        .appended()
        .set_tags(strings(&["x", "y"]))
        .plan()
        .expect("append sequence");
    assert_eq!(
        query_of(&appended),
        "a=1&locale=fr&z=9&tag=p1&tag=p2&tag=x&tag=y"
    );

    // remove: structural, independent of presence.
    let removed = client.removed().plan().expect("remove");
    assert_eq!(query_of(&removed), "a=1&z=9");
    assert!(header_values(&removed, "x-mode").is_empty());

    // introduce: absent emits none; present-empty `each` emits none; an
    // ordered `each` emits every item in order; `Locale` is a distinct key.
    let introduced = client.introduced("S").plan().expect("introduce absent");
    assert_eq!(
        query_of(&introduced),
        "a=1&locale=fr&z=9&tag=p1&tag=p2&Locale=S"
    );
    let introduced = client
        .introduced("S")
        .set_fresh("f")
        .set_many(Vec::new())
        .plan()
        .expect("introduce present");
    assert_eq!(
        query_of(&introduced),
        "a=1&locale=fr&z=9&tag=p1&tag=p2&fresh=f&Locale=S"
    );
    let introduced = client
        .introduced("S")
        .set_many(strings(&["m1", "m2"]))
        .plan()
        .expect("introduce sequence");
    assert_eq!(
        query_of(&introduced),
        "a=1&locale=fr&z=9&tag=p1&tag=p2&many=m1&many=m2&Locale=S"
    );
}

/// V3-S04 on the wire: `when_present(None)` preserves the original order,
/// `Some` replaces at the new position, present-empty clears.
#[tokio::test(flavor = "current_thread")]
async fn conditional_replacement_order_reaches_the_wire_unchanged() {
    let _guard = request_shaping_test_guard().await;
    let server = ScriptedServer::start((0..3).map(|_| ResponseScript::ok(b"ok".to_vec())))
        .expect("program server");
    let proxy = loopback_proxy(&server);
    let client = ProgramApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("proxied program client");

    client.conditional().await.expect("absent override");
    client
        .conditional()
        .set_locale("de")
        .set_mode("child")
        .await
        .expect("present override");
    client
        .conditional()
        .set_tags(Vec::new())
        .await
        .expect("present-empty override");

    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(
        requests[0].target(),
        "http://programs.example.test/?a=1&locale=fr&z=9&tag=p1&tag=p2"
    );
    assert_eq!(requests[0].header("x-mode"), Some("parent"));
    assert_eq!(
        requests[1].target(),
        "http://programs.example.test/?a=1&z=9&tag=p1&tag=p2&locale=de"
    );
    assert_eq!(requests[1].header("x-mode"), Some("child"));
    assert_eq!(
        requests[2].target(),
        "http://programs.example.test/?a=1&locale=fr&z=9"
    );
}

api! {
    client DefaultedEachApi {
        base "http://defaulted-each.example.test/";
        timeout 5s;

        query { "a" = "1"; "tag" = "parent"; "z" = "9"; }
        headers { "X-Tag" = "parent"; }

        GET tagged(#[default] tags?: Vec<String>) -> Bytes via Binary {
            query { when_present "tag" = each tags; }
            headers { when_present "x-tag" = each tags; }
        }
    }
}

/// V3-L08 (§4.1 `#[default] x?: T`, §6.2/§6.3): a defaulted optional empty
/// vector is outer-PRESENT with zero items. Under `when_present` it therefore
/// removes the inherited occurrences and emits none (present-empty clears),
/// unlike an omitted slot, which preserves them. Reset restores the retained
/// present-empty slot. Shown on actual plans and on the wire.
#[tokio::test(flavor = "current_thread")]
async fn defaulted_empty_vector_is_present_for_when_present_and_emits_zero_occurrences() {
    let _guard = request_shaping_test_guard().await;
    let server = ScriptedServer::start((0..3).map(|_| ResponseScript::ok(b"ok".to_vec())))
        .expect("defaulted each server");
    let proxy = loopback_proxy(&server);
    let client = DefaultedEachApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("defaulted each client");
    let plan = |request: defaulted_each_api::TaggedRequest| request.plan().expect("plan");

    // Initial state: the retained `Vec::default()` is present and empty.
    let defaulted = plan(client.tagged());
    assert_eq!(
        defaulted.observed_url(),
        "http://defaulted-each.example.test/?a=1&z=9"
    );
    assert!(header_values(&defaulted, "x-tag").is_empty());
    // Outer absence preserves the inherited occurrences and positions.
    let omitted = plan(client.tagged().omit_tags());
    assert_eq!(
        omitted.observed_url(),
        "http://defaulted-each.example.test/?a=1&tag=parent&z=9"
    );
    assert_eq!(header_values(&omitted, "x-tag"), ["parent"]);
    assert_eq!(
        plan(client.tagged().replace_tags(None)).observed_url(),
        omitted.observed_url()
    );
    // Reset restores the retained present-empty slot, not absence.
    let reset = plan(client.tagged().omit_tags().reset_tags());
    assert_eq!(reset.observed_url(), defaulted.observed_url());
    assert!(header_values(&reset, "x-tag").is_empty());
    // A non-empty selection replaces at the operation's position.
    let selected = plan(client.tagged().set_tags(strings(&["x", "y"])));
    assert_eq!(
        selected.observed_url(),
        "http://defaulted-each.example.test/?a=1&z=9&tag=x&tag=y"
    );
    assert_eq!(header_values(&selected, "x-tag"), ["x", "y"]);

    client.tagged().await.expect("defaulted request");
    client.tagged().omit_tags().await.expect("omitted request");
    client
        .tagged()
        .omit_tags()
        .reset_tags()
        .await
        .expect("reset request");
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].target(), defaulted.observed_url());
    assert_eq!(requests[0].header("x-tag"), None);
    assert_eq!(requests[1].target(), omitted.observed_url());
    assert_eq!(requests[1].header("x-tag"), Some("parent"));
    assert_eq!(requests[2].target(), defaulted.observed_url());
    assert_eq!(requests[2].header("x-tag"), None);
}

/// V3-L04: inline placements, blocks and family replacement normalize in the
/// §6.1 order at each owner.
#[test]
fn inline_placements_blocks_and_family_replacement_normalize_in_order() {
    let client = NormalizedApi::new().expect("normalized client");
    let ordered = client.ordered("d").read("ld").plan().expect("ordered plan");
    assert_eq!(
        query_of(&ordered),
        "root=r&direct=d&block=b&leaf_direct=ld&leaf_block=lb"
    );

    let fenced = client.fenced("d").read().plan().expect("fenced plan");
    assert_eq!(query_of(&fenced), "direct=d&only=this");

    let contract = NormalizedApi::contract();
    let endpoint = contract
        .endpoint("normalized_api.ordered.read")
        .expect("ordered endpoint contract");
    let rendered = format!("{endpoint:?}");
    let direct = rendered.find("\"direct\"").expect("direct placement shown");
    let block = rendered.find("\"block\"").expect("block statement shown");
    assert!(
        direct < block,
        "normalized order is inspectable: {rendered}"
    );
}

/// V3-S03: a suppressed parent adapter (which fails whenever called) is never
/// invoked by plan or execution; surviving adapters run exactly once per
/// preparation, and each selected item once.
#[tokio::test(flavor = "current_thread")]
async fn suppressed_parent_adapters_never_run_and_survivors_run_once_per_preparation() {
    let _guard = request_shaping_test_guard().await;
    SUPPRESSED_ADAPTER_CALLS.store(0, Ordering::SeqCst);
    SURVIVING_ADAPTER_CALLS.store(0, Ordering::SeqCst);
    ITEM_ADAPTER_CALLS.store(0, Ordering::SeqCst);
    let server = ScriptedServer::start((0..2).map(|_| ResponseScript::ok(b"ok".to_vec())))
        .expect("adapter server");
    let proxy = loopback_proxy(&server);
    let client = AdapterApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("adapter client");

    let replaced = client.replaced("label", strings(&["i1", "i2", "i3"]));
    let plan = replaced
        .plan()
        .expect("suppressed parent does not fail the plan");
    assert_eq!(query_of(&plan), "tenant=LABEL&item=i1&item=i2&item=i3");
    assert!(header_values(&plan, "x-tenant").is_empty());
    assert_eq!(SUPPRESSED_ADAPTER_CALLS.load(Ordering::SeqCst), 0);
    assert_eq!(SURVIVING_ADAPTER_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(ITEM_ADAPTER_CALLS.load(Ordering::SeqCst), 3);

    replaced
        .await
        .expect("suppressed parent does not fail execution");
    assert_eq!(SUPPRESSED_ADAPTER_CALLS.load(Ordering::SeqCst), 0);
    assert_eq!(SURVIVING_ADAPTER_CALLS.load(Ordering::SeqCst), 2);
    assert_eq!(ITEM_ADAPTER_CALLS.load(Ordering::SeqCst), 6);

    let fenced = client.fenced().read();
    assert_eq!(
        query_of(&fenced.plan().expect("fenced plan")),
        "label=TENANT-A"
    );
    fenced.await.expect("fenced execution");
    assert_eq!(SUPPRESSED_ADAPTER_CALLS.load(Ordering::SeqCst), 0);
    assert_eq!(SURVIVING_ADAPTER_CALLS.load(Ordering::SeqCst), 4);

    // Control: when the parent emission survives, its adapter does run and
    // its failure is a structured configuration error before any dispatch.
    let error = client
        .inherited()
        .plan()
        .expect_err("surviving failing adapter");
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(
        error.configuration_kind(),
        Some(ConfigurationErrorKind::InvalidRoute)
    );
    assert_eq!(SUPPRESSED_ADAPTER_CALLS.load(Ordering::SeqCst), 1);

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0].target(),
        "http://adapters.example.test/?tenant=LABEL&item=i1&item=i2&item=i3"
    );
    assert_eq!(requests[0].header("x-tenant"), None);
    assert_eq!(
        requests[1].target(),
        "http://adapters.example.test/?label=TENANT-A"
    );
}

/// V3-S09: sensitivity is sticky per key along the owner chain, including a
/// sensitive contribution replaced by a public value or blocked by a family
/// fence; a sensitive component taints `each`, `concat`, path and host atoms.
/// The real request keeps the actual values.
#[tokio::test(flavor = "current_thread")]
async fn sensitivity_is_sticky_and_propagates_through_every_atom_role() {
    let _guard = request_shaping_test_guard().await;
    let server = ScriptedServer::start((0..4).map(|_| ResponseScript::ok(b"ok".to_vec())))
        .expect("privacy server");
    let proxy = loopback_proxy(&server);
    let client = PrivacyApi::builder()
        .set_tenant(TENANT_SENTINEL)
        .set_tags(vec![TAG_SENTINEL.to_owned(), "public-tag".to_owned()])
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("privacy client");

    let inherited = client.inherited().plan().expect("inherited plan");
    assert_eq!(
        inherited.observed_url(),
        format!(
            "http://privacy.example.test/root/inherited?tenant={REDACTED}&tag={REDACTED}&tag={REDACTED}"
        )
    );
    assert_eq!(header_values(&inherited, "x-tenant"), ["Redacted"]);

    let replaced = client
        .replaced("public-label")
        .plan()
        .expect("replaced plan");
    assert_eq!(
        replaced.observed_url(),
        format!("http://privacy.example.test/root/replaced?tenant={REDACTED}&tag={REDACTED}"),
        "a public replacement cannot declassify a sensitive key"
    );
    assert_eq!(header_values(&replaced, "x-tenant"), ["Redacted"]);

    let fenced = client.fenced().read().plan().expect("fenced plan");
    assert_eq!(
        fenced.observed_url(),
        format!("http://privacy.example.test/root/fenced?tenant={REDACTED}"),
        "a family fence does not declassify a sensitive key"
    );

    let hosted = client.hosted().read("pub").plan().expect("hosted plan");
    assert_eq!(
        hosted.observed_url(),
        format!(
            "http://x.privacy.example.test/root/items/{REDACTED}/pub?tenant={REDACTED}&tag={REDACTED}&tag={REDACTED}"
        ),
        "a sensitive host label and concatenated segment are withheld whole"
    );

    client.inherited().await.expect("inherited request");
    client
        .replaced("public-label")
        .await
        .expect("replaced request");
    client.fenced().read().await.expect("fenced request");
    client.hosted().read("pub").await.expect("hosted request");
    let requests = server.requests();
    assert_eq!(requests.len(), 4);
    assert_eq!(
        requests[0].target(),
        format!(
            "http://privacy.example.test/root/inherited?tenant={TENANT_SENTINEL}&tag={TAG_SENTINEL}&tag=public-tag"
        )
    );
    assert_eq!(
        requests[0].header("x-tenant"),
        Some(format!("t-{TENANT_SENTINEL}").as_str())
    );
    assert_eq!(
        requests[1].target(),
        "http://privacy.example.test/root/replaced?tenant=public-label&tag=public-label"
    );
    assert_eq!(requests[1].header("x-tenant"), Some("public-label"));
    assert_eq!(
        requests[2].target(),
        "http://privacy.example.test/root/fenced?tenant=eu"
    );
    assert_eq!(requests[2].header("x-tenant"), None);
    assert_eq!(
        requests[3].target(),
        format!(
            "http://{TENANT_SENTINEL}.privacy.example.test/root/items/pub-{TENANT_SENTINEL}/pub?tenant={TENANT_SENTINEL}&tag={TAG_SENTINEL}&tag=public-tag"
        )
    );

    let contract = PrivacyApi::contract();
    let tenant = contract.client().value("tenant").expect("tenant value");
    assert_eq!(tenant.sensitivity(), Sensitivity::Sensitive);
    assert_eq!(tenant.liveness(), ValueLiveness::Live);
}

#[derive(Default)]
struct Capture {
    events: Mutex<Vec<String>>,
    classifier: Mutex<Vec<String>>,
}

struct CapturingObserver(Arc<Capture>);

impl OperationObserver for CapturingObserver {
    fn observe(&self, event: OperationEvent<'_>) {
        self.0
            .events
            .lock()
            .expect("event capture")
            .push(format!("{event:?}"));
    }
}

fn error_chain(error: &concord::ApiError) -> String {
    let mut rendered = format!("{error}\n{error:?}");
    let mut source = error.source();
    while let Some(next) = source {
        rendered.push_str(&format!("\n{next}\n{next:?}"));
        source = next.source();
    }
    if let Some(response) = error.http_response() {
        rendered.push_str(&format!("\n{response:?}\n{:?}", response.headers()));
    }
    rendered
}

/// V3-S10: sensitive sentinels are absent from every automatic Debug and
/// Display, error and source chain, plan, observer event, snapshot,
/// contract and the retry classifier's host, while the real request carries
/// them in their declared places.
#[tokio::test(flavor = "current_thread")]
async fn sentinels_never_reach_automatic_surfaces_but_reach_the_wire() {
    let _guard = request_shaping_test_guard().await;
    let server = ScriptedServer::start([
        ResponseScript::ok(b"ok".to_vec()),
        ResponseScript::status(503, b"unavailable".to_vec()),
    ])
    .expect("sentinel server");
    let proxy = loopback_proxy(&server);
    let capture = Arc::new(Capture::default());
    let observer = Arc::new(CapturingObserver(Arc::clone(&capture)));
    let classifier_capture = Arc::clone(&capture);
    let policy = RetryPolicy::builder()
        .classify_fn(move |attempt| {
            classifier_capture
                .classifier
                .lock()
                .expect("classifier capture")
                .push(format!("{attempt:?} host={:?}", attempt.host()));
            RetryDecision::Stop
        })
        .build()
        .expect("classifier policy");
    let builder = PrivacyApi::builder()
        .set_tenant(TENANT_SENTINEL)
        .set_tags(vec![TAG_SENTINEL.to_owned()])
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.operation_observer(observer))
        .retry(policy);
    let builder_debug = format!("{builder:?}");
    let client = builder.build().expect("sentinel client");

    let scope = client.hosted();
    let request = scope.read("pub");
    let plan = request.plan().expect("sentinel plan");
    let response = client
        .inherited()
        .response()
        .await
        .expect("successful sentinel response");
    let error = request.await.expect_err("rejected sentinel status");
    assert_eq!(error.category(), ErrorCategory::HttpStatus);

    let classifier = capture
        .classifier
        .lock()
        .expect("classifier capture")
        .clone();
    assert_eq!(classifier.len(), 1, "the rejected status was classified");
    assert!(
        classifier[0].ends_with("host=None"),
        "a sensitive host label withholds the classifier host: {}",
        classifier[0]
    );
    let events = capture.events.lock().expect("event capture").clone();
    assert!(!events.is_empty());

    let corpus = [
        builder_debug,
        format!("{client:?}"),
        format!("{scope:?}"),
        format!("{:?}", client.hosted().read("pub")),
        format!("{:?}", PrivacyApi::unbound().hosted().read("pub")),
        format!("{plan:?}"),
        plan.observed_url().to_owned(),
        format!("{:?}", plan.public_headers()),
        format!("{response:?}"),
        format!("{:?}", response.metadata()),
        error_chain(&error),
        events.join("\n"),
        classifier.join("\n"),
        format!("{:?}", PrivacyApi::contract()),
    ]
    .join("\n");
    for sentinel in [TENANT_SENTINEL, TAG_SENTINEL] {
        assert!(
            !corpus.contains(sentinel),
            "automatic surface leaked {sentinel}:\n{corpus}"
        );
    }

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert!(requests[0].target().contains(TENANT_SENTINEL));
    assert_eq!(
        requests[0].header("x-tenant"),
        Some(format!("t-{TENANT_SENTINEL}").as_str())
    );
    assert!(requests[1].target().starts_with(&format!(
        "http://{TENANT_SENTINEL}.privacy.example.test/root/items/pub-{TENANT_SENTINEL}/pub?"
    )));
}
