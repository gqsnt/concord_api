//! Runtime benchmark scenarios: Concord V4 vs equivalent hand-written reqwest.
//!
//! The scenarios are defined once, here, and driven by two runners: the Criterion suite
//! (`benches/runtime.rs`, wall time) and the deterministic allocation counter
//! (`src/bin/alloc_count.rs`). See `README.md` for the benchmark IDs and how to record baselines.
#![allow(missing_docs)]

pub mod apis;
pub mod manual;
pub mod payload;
pub mod server;

use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use bytes::Bytes;
use concord_baselines as base;
use concord_example_shapes as shapes;

use crate::payload::{SIZES, Size};

/// What a case's throughput is counted in.
#[derive(Clone, Copy, Debug)]
pub enum Throughput {
    /// Header values, query values...
    Elements(u64),
    /// Body bytes.
    Bytes(u64),
}

/// One benchmark case: `input` builds a fresh input (per-call input construction); `run`
/// consumes it (prepare / build / encode / decode). Runners measure them as separate IDs:
/// `<id>/input` (unless `trivial_input`) and `<id>/<run_stage>`, plus `<id>/input_<run_stage>`
/// when `total` is set.
#[derive(Clone, Debug)]
pub struct Case {
    /// `<scenario>/<implementation>`, stable across revisions.
    pub id: String,
    /// Name of the measured consuming stage (`prepare`, `decode`).
    pub run_stage: &'static str,
    /// Throughput unit for the case, if sizes vary within the group.
    pub throughput: Option<Throughput>,
    /// Inputs are large (≥ 1 MB): batch them sparingly.
    pub large_input: bool,
    /// Also measure input construction and the run stage together.
    pub total: bool,
    /// The input is a constant or a reference (nothing is constructed per call): no `input`
    /// stage is measured.
    pub trivial_input: bool,
}

impl Case {
    fn new(id: impl Into<String>, run_stage: &'static str) -> Self {
        Self {
            id: id.into(),
            run_stage,
            throughput: None,
            large_input: false,
            total: false,
            trivial_input: false,
        }
    }

    fn trivial_input(mut self) -> Self {
        self.trivial_input = true;
        self
    }

    fn total(mut self) -> Self {
        self.total = true;
        self
    }

    fn throughput(mut self, t: Throughput) -> Self {
        self.throughput = Some(t);
        self
    }

    fn large(mut self, large: bool) -> Self {
        self.large_input = large;
        self
    }
}

/// Drives cases. Implemented by the Criterion bench (one benchmark group per scenario family)
/// and by the allocation counter.
pub trait Runner {
    /// Measures one case.
    fn case<I, O>(&mut self, case: Case, input: impl FnMut() -> I, run: impl FnMut(I) -> O);
}

/// Completes a future that needs no I/O (a response built in memory), polling it once.
///
/// # Panics
///
/// If the future is not ready after one poll.
pub fn complete<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("future needed I/O; only in-memory responses are supported"),
    }
}

/// An in-memory `200 OK` response with `body`, as reqwest would hand it to a decoder.
pub fn response(content_type: &'static str, body: Bytes) -> reqwest::Response {
    reqwest::Response::from(
        http::Response::builder()
            .status(200)
            .header(http::header::CONTENT_TYPE, content_type)
            .header(http::header::CONTENT_LENGTH, body.len())
            .body(body)
            .expect("valid response"),
    )
}

/// Every client used by the construction, header, composition and body groups, created once
/// (setup, never measured).
pub struct Fixtures {
    pub http: reqwest::Client,
    pub s1: shapes::s1_simple::SimpleApi,
    pub simple_manual: manual::Simple,
    pub s2: shapes::s2_scoped::CatalogApi,
    pub s2_manual: base::s2_scoped::CatalogApi,
    pub s3: shapes::s3_query::ShopApi,
    pub s3_manual: base::s3_query::ShopApi,
    pub tracker: apis::tracker::Tracker,
    pub tracker_manual: manual::Tracker,
    pub headers: apis::headers::Headers,
    pub headers_manual: manual::Headers,
    pub deep: apis::deep::Deep,
    pub deep_manual: manual::Deep,
    pub compose: apis::compose::Compose,
    pub compose_manual: manual::Compose,
    pub bodies: apis::bodies::Bodies,
    pub bodies_manual: manual::Bodies,
    pub keys_literal: apis::keys::Literal,
    pub keys_native: apis::keys::Native,
    pub keys_manual: manual::Keys,
    pub riot: apis::keys::Riot,
    pub riot_manual: manual::Riot,
}

/// The client every fixture shares: no proxy lookup, so loopback and construction are
/// environment-independent.
pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("reqwest client")
}

impl Fixtures {
    pub fn new(http: reqwest::Client) -> Self {
        Self {
            s1: shapes::s1_simple::SimpleApi::new(http.clone()),
            simple_manual: manual::Simple::new(http.clone(), "https://api.example.com/v1"),
            s2: shapes::s2_scoped::CatalogApi::new(
                http.clone(),
                shapes::s2_scoped::CatalogApiConfig {
                    api_key: "key".into(),
                },
            ),
            s2_manual: base::s2_scoped::CatalogApi::with_client(
                http.clone(),
                "https://api.example.com/v2/",
                "key",
            )
            .expect("valid"),
            s3: shapes::s3_query::ShopApi::new(
                http.clone(),
                shapes::s3_query::ShopApiConfig {
                    token: "token".into(),
                },
            ),
            s3_manual: base::s3_query::ShopApi::new(
                http.clone(),
                url::Url::parse("https://api.example.com/v1/").expect("valid"),
                "token",
            )
            .expect("valid"),
            tracker: apis::tracker::Tracker::new(
                http.clone(),
                apis::tracker::TrackerConfig {
                    token: "Bearer t0ken".into(),
                },
            ),
            tracker_manual: manual::Tracker::new(
                http.clone(),
                "https://api.example.com/v3",
                "Bearer t0ken",
            ),
            headers: apis::headers::Headers::new(http.clone()),
            headers_manual: manual::Headers::new(http.clone()),
            deep: apis::deep::Deep::new(http.clone()),
            deep_manual: manual::Deep::new(http.clone()),
            compose: apis::compose::Compose::new(http.clone()),
            compose_manual: manual::Compose::new(http.clone()),
            bodies: apis::bodies::Bodies::new(http.clone()),
            bodies_manual: manual::Bodies::new(http.clone(), "https://api.example.com/v1"),
            keys_literal: apis::keys::Literal::new(
                http.clone(),
                apis::keys::LiteralConfig {
                    token: "Bearer t0ken".into(),
                    a: "a".into(),
                },
            ),
            keys_native: apis::keys::Native::new(
                http.clone(),
                apis::keys::NativeConfig {
                    token: "Bearer t0ken".into(),
                    a: "a".into(),
                },
            ),
            keys_manual: manual::Keys::new(http.clone(), "Bearer t0ken", "a"),
            riot: apis::keys::Riot::new(
                http.clone(),
                apis::keys::RiotConfig {
                    key: "RGAPI-bench".into(),
                },
            ),
            riot_manual: manual::Riot::new(http.clone(), "RGAPI-bench"),
            http,
        }
    }
}

fn s(v: &str) -> String {
    v.to_owned()
}

// ---------------------------------------------------------------------------------------------
// Group `construct`: request construction, per-call input separated from `prepare()`.

/// Minimal (S1 health), S3 search (9 query values), S2 product (2 scopes + subdomain + 3
/// headers), realistic tracker (all optional values present / absent).
pub fn construct(fx: &Fixtures, r: &mut impl Runner) {
    r.case(
        Case::new("minimal/concord", "prepare").total(),
        || fx.s1.health(),
        |i| i.prepare().expect("valid"),
    );
    r.case(
        Case::new("minimal/manual", "prepare")
            .total()
            .trivial_input(),
        || (),
        |()| fx.simple_manual.health().expect("valid"),
    );

    r.case(
        Case::new("s3_search_9_query/concord", "prepare").total(),
        || {
            fx.s3
                .search()
                .set_q("red shoe")
                .set_tags(vec![s("a"), s("b")])
                .set_category("c")
                .set_min_price(1)
                .set_max_price(9)
                .set_in_stock(true)
                .set_sort(shapes::s3_query::Sort::PriceAsc)
                .set_cursor("cur")
                .set_limit(50)
        },
        |i| i.prepare().expect("valid"),
    );
    r.case(
        Case::new("s3_search_9_query/manual", "prepare").total(),
        || {
            fx.s3_manual
                .search()
                .q("red shoe")
                .tag("a")
                .tag("b")
                .category("c")
                .min_price(1)
                .max_price(9)
                .in_stock(true)
                .sort(base::s3_query::Sort::PriceAsc)
                .cursor(Some(s("cur")))
                .limit(50)
        },
        |i| i.into_request().build().expect("valid"),
    );

    r.case(
        Case::new("s2_product_scoped/concord", "prepare").total(),
        || {
            fx.s2
                .regional(shapes::s2_scoped::Region::Eu)
                .org("acme")
                .product("p-1")
        },
        |i| i.prepare().expect("valid"),
    );
    r.case(
        Case::new("s2_product_scoped/manual", "prepare")
            .total()
            .trivial_input(),
        // The baseline takes borrowed arguments: nothing to construct per call.
        || (),
        |()| {
            fx.s2_manual
                .request(
                    reqwest::Method::GET,
                    base::s2_scoped::Region::Eu,
                    &["orgs", "acme", "catalog", "products", "p-1"],
                )
                .build()
                .expect("valid")
        },
    );

    r.case(
        Case::new("realistic_all_present/concord", "prepare").total(),
        || tracker_full(fx),
        |i| i.prepare().expect("valid"),
    );
    r.case(
        Case::new("realistic_all_present/manual", "prepare").total(),
        || tracker_full_manual(fx),
        |i| i.build().expect("valid"),
    );
    r.case(
        Case::new("realistic_optional_absent/concord", "prepare").total(),
        || fx.tracker.org("acme").project("rocket").issues(42),
        |i| i.prepare().expect("valid"),
    );
    r.case(
        Case::new("realistic_optional_absent/manual", "prepare").total(),
        || fx.tracker_manual.org("acme").project("rocket").issues(42),
        |i| i.build().expect("valid"),
    );
}

/// The realistic request with every optional value present (10 query values).
pub fn tracker_full(fx: &Fixtures) -> apis::tracker::TrackerOrgProjectIssues {
    fx.tracker
        .org("acme")
        .project("rocket")
        .issues(42)
        .set_state("open")
        .set_labels(vec![s("bug"), s("p1")])
        .set_assignee("octo")
        .set_since(1_700_000_000)
        .set_sort("updated")
        .set_direction("desc")
        .set_include_closed(false)
        .set_page(3)
        .set_per_page(100)
}

/// Hand-written equivalent of [`tracker_full`].
pub fn tracker_full_manual(fx: &Fixtures) -> manual::Issues<'_> {
    let mut i = fx.tracker_manual.org("acme").project("rocket").issues(42);
    i.state = Some(s("open"));
    i.labels = vec![s("bug"), s("p1")];
    i.assignee = Some(s("octo"));
    i.since = Some(1_700_000_000);
    i.sort = Some(s("updated"));
    i.direction = Some(s("desc"));
    i.include_closed = Some(false);
    i.page = 3;
    i.per_page = 100;
    i
}

// ---------------------------------------------------------------------------------------------
// Group `headers`: 1, 4 and 16 header values per request.

/// Header counts measured.
pub const HEADER_COUNTS: [usize; 3] = [1, 4, 16];

fn texts<const N: usize>() -> [String; N] {
    std::array::from_fn(|i| s(manual::HEADER_TEXT[i]))
}

fn numbers<const N: usize>() -> [u64; N] {
    std::array::from_fn(|i| manual::HEADER_NUMBERS[i])
}

fn header_case(n: usize, variant: &str) -> Case {
    Case::new(format!("{n}/{variant}"), "prepare").throughput(Throughput::Elements(n as u64))
}

/// Every header variant, for each count. A variant is one function.
pub fn headers(fx: &Fixtures, r: &mut impl Runner) {
    for n in HEADER_COUNTS {
        headers_concord_text(fx, r, n);
        headers_concord_numeric(fx, r, n);
        headers_concord_native(fx, r, n);
        headers_concord_literal(fx, r, n);
        headers_manual_from_str(fx, r, n);
        headers_manual_numeric(fx, r, n);
        headers_manual_prebuilt(fx, r, n);
        headers_manual_from_static(fx, r, n);
    }
}

/// Concord, owned `String` values formatted with `Display` at `prepare`.
pub fn headers_concord_text(fx: &Fixtures, r: &mut impl Runner, n: usize) {
    let h = &fx.headers;
    let case = header_case(n, "concord_text");
    match n {
        1 => r.case(
            case,
            || h.text_1(s(manual::HEADER_TEXT[0])),
            |i| i.prepare().expect("valid"),
        ),
        4 => r.case(
            case,
            || {
                let [a, b, c, d] = texts::<4>();
                h.text_4(a, b, c, d)
            },
            |i| i.prepare().expect("valid"),
        ),
        16 => r.case(
            case,
            || {
                let [a, b, c, d, e, f, g, h0, i, j, k, l, m, n0, o, p] = texts::<16>();
                h.text_16(a, b, c, d, e, f, g, h0, i, j, k, l, m, n0, o, p)
            },
            |i| i.prepare().expect("valid"),
        ),
        _ => unreachable!("header count"),
    }
}

/// Concord, `u64` values formatted with `Display` at `prepare`.
pub fn headers_concord_numeric(fx: &Fixtures, r: &mut impl Runner, n: usize) {
    let h = &fx.headers;
    let case = header_case(n, "concord_numeric");
    let v = numbers::<16>();
    match n {
        1 => r.case(case, || h.numeric_1(v[0]), |i| i.prepare().expect("valid")),
        4 => r.case(
            case,
            || h.numeric_4(v[0], v[1], v[2], v[3]),
            |i| i.prepare().expect("valid"),
        ),
        16 => r.case(
            case,
            || {
                h.numeric_16(
                    v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7], v[8], v[9], v[10], v[11],
                    v[12], v[13], v[14], v[15],
                )
            },
            |i| i.prepare().expect("valid"),
        ),
        _ => unreachable!("header count"),
    }
}

/// Concord, native `HeaderValue`s passed per call as `&HeaderValue` (cloned: payload reuse, no
/// formatting or validation), the counterpart of `manual_prebuilt_clone`.
pub fn headers_concord_native(fx: &Fixtures, r: &mut impl Runner, n: usize) {
    let h = &fx.headers;
    let v = &fx.headers_manual.prebuilt;
    let case = header_case(n, "concord_native");
    match n {
        1 => r.case(case, || h.native_1(&v[0]), |i| i.prepare().expect("valid")),
        4 => r.case(
            case,
            || h.native_4(&v[0], &v[1], &v[2], &v[3]),
            |i| i.prepare().expect("valid"),
        ),
        16 => r.case(
            case,
            || {
                h.native_16(
                    &v[0], &v[1], &v[2], &v[3], &v[4], &v[5], &v[6], &v[7], &v[8], &v[9], &v[10],
                    &v[11], &v[12], &v[13], &v[14], &v[15],
                )
            },
            |i| i.prepare().expect("valid"),
        ),
        _ => unreachable!("header count"),
    }
}

/// Concord, literal header values (`headers { "x-h00" = "..."; }`), lowered to compile-time
/// statics: the counterpart of `manual_from_static`.
pub fn headers_concord_literal(fx: &Fixtures, r: &mut impl Runner, n: usize) {
    let h = &fx.headers;
    let case = header_case(n, "concord_literal").trivial_input();
    match n {
        1 => r.case(case, || h.literal_1(), |i| i.prepare().expect("valid")),
        4 => r.case(case, || h.literal_4(), |i| i.prepare().expect("valid")),
        16 => r.case(case, || h.literal_16(), |i| i.prepare().expect("valid")),
        _ => unreachable!("header count"),
    }
}

/// Hand-written, owned `String` values through `HeaderValue::from_str`.
pub fn headers_manual_from_str(fx: &Fixtures, r: &mut impl Runner, n: usize) {
    let h = &fx.headers_manual;
    let case = header_case(n, "manual_from_str");
    match n {
        1 => r.case(case, texts::<1>, |i| h.text(i).expect("valid")),
        4 => r.case(case, texts::<4>, |i| h.text(i).expect("valid")),
        16 => r.case(case, texts::<16>, |i| h.text(i).expect("valid")),
        _ => unreachable!("header count"),
    }
}

/// Hand-written, `u64` values through `HeaderValue::from(u64)`.
pub fn headers_manual_numeric(fx: &Fixtures, r: &mut impl Runner, n: usize) {
    let h = &fx.headers_manual;
    let case = header_case(n, "manual_numeric");
    match n {
        1 => r.case(case, numbers::<1>, |i| h.numeric(i).expect("valid")),
        4 => r.case(case, numbers::<4>, |i| h.numeric(i).expect("valid")),
        16 => r.case(case, numbers::<16>, |i| h.numeric(i).expect("valid")),
        _ => unreachable!("header count"),
    }
}

/// Hand-written, clones of `HeaderValue`s validated once (payload reuse).
pub fn headers_manual_prebuilt(fx: &Fixtures, r: &mut impl Runner, n: usize) {
    let h = &fx.headers_manual;
    r.case(
        header_case(n, "manual_prebuilt_clone").trivial_input(),
        || n,
        |n| h.prebuilt(n).expect("valid"),
    );
}

/// Hand-written, `HeaderValue::from_static` per request.
pub fn headers_manual_from_static(fx: &Fixtures, r: &mut impl Runner, n: usize) {
    let h = &fx.headers_manual;
    r.case(
        header_case(n, "manual_from_static").trivial_input(),
        || n,
        |n| h.from_static(n).expect("valid"),
    );
}

// ---------------------------------------------------------------------------------------------
// Group `compose`: deep scope chain and inherited header operations.

fn deep_values() -> [String; 8] {
    std::array::from_fn(|i| format!("value{i}"))
}

/// Deep chain (navigated per call, and from a reused level-8 scope handle) and the inherited
/// override / append / remove operations from a reused scope handle.
pub fn compose(fx: &Fixtures, r: &mut impl Runner) {
    r.case(
        Case::new("deep8_navigate/concord", "prepare").total(),
        || {
            let [a, b, c, d, e, f, g, h] = deep_values();
            fx.deep
                .l1(a)
                .l2(b)
                .l3(c)
                .l4(d)
                .l5(e)
                .l6(f)
                .l7(g)
                .l8(h)
                .leaf()
        },
        |i| i.prepare().expect("valid"),
    );
    r.case(
        Case::new("deep8_navigate/manual", "prepare").total(),
        deep_values,
        |v| fx.deep_manual.leaf(v).expect("valid"),
    );

    let [a, b, c, d, e, f, g, h] = deep_values();
    let scope = fx.deep.l1(a).l2(b).l3(c).l4(d).l5(e).l6(f).l7(g).l8(h);
    let scope_manual = fx.deep_manual.scope(deep_values());
    r.case(
        Case::new("deep8_reused_scope/concord", "prepare").total(),
        || scope.leaf(),
        |i| i.prepare().expect("valid"),
    );
    r.case(
        Case::new("deep8_reused_scope/manual", "prepare")
            .total()
            .trivial_input(),
        || &scope_manual,
        |sc| sc.leaf().expect("valid"),
    );

    let team = fx.compose.team("core");
    let team_manual = fx.compose_manual.team("core");
    r.case(
        Case::new("inherit/concord", "prepare"),
        || team.inherit(),
        |i| i.prepare().expect("valid"),
    );
    r.case(
        Case::new("override_present/concord", "prepare"),
        || team.override_value().set_v("endpoint-a"),
        |i| i.prepare().expect("valid"),
    );
    r.case(
        Case::new("override_absent/concord", "prepare"),
        || team.override_value(),
        |i| i.prepare().expect("valid"),
    );
    r.case(
        Case::new("append/concord", "prepare"),
        || team.append_value("endpoint-b"),
        |i| i.prepare().expect("valid"),
    );
    r.case(
        Case::new("remove/concord", "prepare"),
        || team.remove_value(),
        |i| i.prepare().expect("valid"),
    );

    let build = |op: manual::Op| team_manual.request(op).expect("valid");
    r.case(
        Case::new("inherit/manual", "prepare"),
        || manual::Op::Inherit,
        build,
    );
    r.case(
        Case::new("override_present/manual", "prepare"),
        || manual::Op::Override(Some(s("endpoint-a"))),
        build,
    );
    r.case(
        Case::new("override_absent/manual", "prepare"),
        || manual::Op::Override(None),
        build,
    );
    r.case(
        Case::new("append/manual", "prepare"),
        || manual::Op::Append(s("endpoint-b")),
        build,
    );
    r.case(
        Case::new("remove/manual", "prepare"),
        || manual::Op::Remove,
        build,
    );
}

// ---------------------------------------------------------------------------------------------
// Group `body_encode`: request bodies, encoded inside `prepare()` / `build()`.

/// JSON (serde struct), text and bytes bodies of each size.
pub fn body_encode(fx: &Fixtures, r: &mut impl Runner) {
    for size in SIZES {
        let large = size.target >= payload::LARGE.target;
        let json = payload::payload(size.target);
        let json_len = serde_json::to_vec(&json).expect("serializable").len() as u64;
        let case = |id: String, len: u64| {
            Case::new(id, "prepare")
                .throughput(Throughput::Bytes(len))
                .large(large)
        };
        r.case(
            case(format!("json/{}/concord", size.label), json_len),
            || fx.bodies.post_json(json.clone()),
            |i| i.prepare().expect("valid"),
        );
        r.case(
            case(format!("json/{}/manual", size.label), json_len),
            || json.clone(),
            |p| fx.bodies_manual.post_json(&p).expect("valid"),
        );

        let text = payload::text(size.target);
        let len = text.len() as u64;
        r.case(
            case(format!("text/{}/concord", size.label), len),
            || fx.bodies.post_text(text.clone()),
            |i| i.prepare().expect("valid"),
        );
        r.case(
            case(format!("text/{}/manual", size.label), len),
            || text.clone(),
            |t| fx.bodies_manual.post_text(t).expect("valid"),
        );

        // `Bytes` clones share the buffer: the input is a reference-count increment.
        let data = payload::binary(size.target);
        r.case(
            case(format!("bytes/{}/concord", size.label), len),
            || fx.bodies.post_bytes(data.clone()),
            |i| i.prepare().expect("valid"),
        );
        r.case(
            case(format!("bytes/{}/manual", size.label), len),
            || data.clone(),
            |d| fx.bodies_manual.post_bytes(d).expect("valid"),
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Group `decode`: response decoding without network.

/// `Decoder::decode` vs `error_for_status` + `Response::{json,text,bytes}` on in-memory
/// responses of each size. The input stage is building the `reqwest::Response`.
pub fn decode(fx: &Fixtures, r: &mut impl Runner) {
    let json_decoder = fx
        .bodies
        .get_json()
        .prepare()
        .expect("valid")
        .into_parts()
        .2;
    let text_decoder = fx
        .bodies
        .get_text()
        .prepare()
        .expect("valid")
        .into_parts()
        .2;
    let bytes_decoder = fx
        .bodies
        .get_bytes()
        .prepare()
        .expect("valid")
        .into_parts()
        .2;
    for size in SIZES {
        let large = size.target >= payload::LARGE.target;
        let case = |id: String, len: usize| {
            Case::new(id, "decode")
                .throughput(Throughput::Bytes(len as u64))
                .large(large)
        };
        let json = payload::json_bytes(size.target);
        let resp = || response("application/json", json.clone());
        r.case(
            case(format!("json/{}/concord", size.label), json.len()),
            resp,
            |resp| complete(json_decoder.decode(resp)).expect("decodes"),
        );
        r.case(
            case(format!("json/{}/manual", size.label), json.len()),
            resp,
            |resp| complete(manual::decode::json(resp)).expect("decodes"),
        );

        let text = Bytes::from(payload::text(size.target));
        let resp = || response("text/plain; charset=utf-8", text.clone());
        r.case(
            case(format!("text/{}/concord", size.label), text.len()),
            resp,
            |resp| complete(text_decoder.decode(resp)).expect("decodes"),
        );
        r.case(
            case(format!("text/{}/manual", size.label), text.len()),
            resp,
            |resp| complete(manual::decode::text(resp)).expect("decodes"),
        );

        let data = payload::binary(size.target);
        let resp = || response("application/octet-stream", data.clone());
        r.case(
            case(format!("bytes/{}/concord", size.label), data.len()),
            resp,
            |resp| complete(bytes_decoder.decode(resp)).expect("decodes"),
        );
        r.case(
            case(format!("bytes/{}/manual", size.label), data.len()),
            resp,
            |resp| complete(manual::decode::bytes(resp)).expect("decodes"),
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Group `keys`: native header names against literal names and hand-written reqwest.

/// Six inherited headers from a reused scope handle (literal names, composed at compile time;
/// native names, composed by `HeaderPlan` when the request is built; hand-written), the same with
/// one header replaced through another spelling, and a Riot-shaped request from the client.
pub fn keys(fx: &Fixtures, r: &mut impl Runner) {
    let literal = fx.keys_literal.team("rocket");
    let native = fx.keys_native.team("rocket");
    r.case(
        Case::new("six_inherited/concord_literal", "prepare").total(),
        || literal.get(),
        |i| i.prepare().expect("valid"),
    );
    r.case(
        Case::new("six_inherited/concord_native", "prepare").total(),
        || native.get(),
        |i| i.prepare().expect("valid"),
    );
    r.case(
        Case::new("six_inherited/manual", "prepare").trivial_input(),
        || (),
        |()| fx.keys_manual.items("rocket", None).expect("valid"),
    );
    r.case(
        Case::new("one_replaced/concord_literal", "prepare").total(),
        || literal.replaced(s("v")),
        |i| i.prepare().expect("valid"),
    );
    r.case(
        Case::new("one_replaced/concord_native", "prepare").total(),
        || native.replaced(s("v")),
        |i| i.prepare().expect("valid"),
    );
    r.case(
        Case::new("one_replaced/manual", "prepare").trivial_input(),
        || (),
        |()| fx.keys_manual.items("rocket", Some("v")).expect("valid"),
    );
    r.case(
        Case::new("riot_match_ids/concord", "prepare").total(),
        || fx.riot.matches(s("europe")).ids(s(PUUID)).set_count(5),
        |i| i.prepare().expect("valid"),
    );
    r.case(
        Case::new("riot_match_ids/manual", "prepare").trivial_input(),
        || (),
        |()| fx.riot_manual.ids("europe", PUUID, Some(5)).expect("valid"),
    );
}

/// A PUUID-shaped value (78 characters).
pub const PUUID: &str =
    "SYNTHETIC-218020e66304e3a79839e7c707074ffa757f3e6537eb19c3d676d50a55d6c89f151e";

/// Every synchronous group, in suite order, with its Criterion group name.
pub const GROUPS: [&str; 6] = [
    "construct",
    "headers",
    "compose",
    "body_encode",
    "decode",
    "keys",
];

/// Runs the synchronous group `name`.
pub fn run_group(name: &str, fx: &Fixtures, r: &mut impl Runner) {
    match name {
        "construct" => construct(fx, r),
        "headers" => headers(fx, r),
        "compose" => compose(fx, r),
        "body_encode" => body_encode(fx, r),
        "decode" => decode(fx, r),
        "keys" => keys(fx, r),
        _ => panic!("unknown group {name}"),
    }
}

/// Size of the loopback round trip's large response.
pub const ROUNDTRIP_LARGE: Size = payload::MEDIUM;

#[cfg(test)]
mod tests;
