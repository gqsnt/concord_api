//! Native header values (H1–H10): a `reqwest::header::HeaderValue` is sent byte for byte — owned,
//! static, optional, initialized, repeated, not text, natively sensitive — without `Display`,
//! formatting or reparsing; its sensitivity flag is kept and Concord's classification is added.
//! Wire assertions use raw bytes (the text view of the test server would hide non-UTF-8 bytes);
//! sensitivity flags are checked separately from value equality, and every secret is also checked
//! to reach the server. Cross-placement reuse (H10) and non-header types (H9) are compile-fail
//! cases in `tests/ui/native_header_*.rs`.
#![deny(warnings)]

use concord::Text;
mod support;

use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};

use concord::reqwest::header::HeaderValue;
use concord::reqwest::header::HeaderValue as NativeValue;
use concord::{ErrorKind, reqwest};
use support::{Reply, Server};

/// H8: an alias.
type Hv = HeaderValue;

/// H2: a static native value, validated at compile time, reused across requests.
static ACCEPT_JSON: HeaderValue = HeaderValue::from_static("application/json");
const PLAIN: Hv = Hv::from_static("text/plain");

/// H5: bytes valid for a header value but neither UTF-8 nor visible ASCII.
fn latin1() -> HeaderValue {
    HeaderValue::from_bytes(b"caf\xe9").unwrap()
}

/// H6: a value whose native sensitivity flag is set by the application.
fn flagged(text: &'static str) -> HeaderValue {
    let mut value = HeaderValue::from_static(text);
    value.set_sensitive(true);
    value
}

static INITIALIZED: AtomicUsize = AtomicUsize::new(0);

/// An initializer with a visible side effect: evaluated when the client is created, never by the
/// macro and never per request.
fn counted() -> HeaderValue {
    INITIALIZED.fetch_add(1, Ordering::SeqCst);
    HeaderValue::from_static("init")
}

/// H9: a domain type with its own header spelling (no `Display`).
#[derive(Clone, Copy)]
pub enum Detail {
    /// `short`.
    Short,
    /// `full`.
    Full,
}

impl fmt::Display for Detail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Detail::Short => "short",
            Detail::Full => "full",
        })
    }
}

/// H9: a type with a `Display` spelling, sent through a template.
#[derive(Clone)]
pub struct Region(pub &'static str);

impl fmt::Display for Region {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

concord::api! {
    /// Native header fixture.
    pub client Native(
        /// H1: an owned native value supplied at construction.
        header("x-owned") owned: HeaderValue,
        /// H2: a static native value as an initializer, through an alias.
        header("accept") accept: Hv = ACCEPT_JSON.clone(),
        /// H3: optional, through a qualified path.
        header("x-opt") opt?: concord::reqwest::header::HeaderValue,
        /// H3: optional and initialized, through a renamed import.
        header("x-init") init?: NativeValue = HeaderValue::from_static("init"),
        /// H4: repeated native values.
        header("x-list") each list: Vec<HeaderValue> = Vec::new(),
        /// H9: an integer.
        header("x-n") n: u32 = 7,
    ) {
        base "http://native.invalid";
        headers { "x-lit" = "literal"; }

        /// Client values only.
        GET plain();
        /// H2: a static value per call.
        GET per_call(header("x-call") call: HeaderValue);
        /// H5.
        GET bytes(header("x-bytes") v: HeaderValue);
        /// H6: flagged natively; no marker; a name not on the safety list.
        GET secret(header("x-unlisted") v: HeaderValue);
        /// H6: flagged natively and marked.
        GET marked(#[sensitive] header("x-marked") v: HeaderValue);
        /// H4 per call; `each` over an `Option` (zero or one).
        GET each_call(header("x-each") each v: Vec<HeaderValue>, header("x-one") each one: Option<HeaderValue>);
        /// Empty versus absent.
        GET empty(header("x-empty") v: HeaderValue, header("x-absent") absent?: HeaderValue);
        /// H9: integers, `bool`, text; domain types (`Display`) through templates.
        GET scalars(
            header("x-u8") a: u8,
            header("x-i64") b: i64,
            header("x-bool") c: bool,
            header("x-text") d: String,
            e: Detail,
            region: Region,
        ) {
            headers { "x-detail" = "{e}"; "x-region" = "{region}"; }
        }

        /// Borrowed and shared forms forward to the value they hold.
        GET forwarded(
            header("x-str") s: &'static str,
            header("x-cow") c: std::borrow::Cow<'static, str>,
            header("x-arc") a: std::sync::Arc<str>,
            header("x-boxed") b: Box<HeaderValue>,
        );

        /// H7: inherited composition.
        scope inherit(header("x-scope") scoped?: HeaderValue) {
            GET overridden(v: HeaderValue) { headers { override "x-owned" = v; } }
            GET appended(v: HeaderValue) { headers { append "x-owned" = v; } }
            GET removed() { headers { remove "x-owned"; } }
            GET kept(v?: HeaderValue) { headers { override "x-owned" = v else inherit; } }
            GET replaced(header("x-owned") v: HeaderValue) { headers { remove "x-owned"; } }
        }

        /// A native `Content-Type` wins over the body representation's.
        POST upload(header("content-type") ct: HeaderValue, body b: String) { request Text; }
        /// An optional native header does not decide whether a body exists.
        POST maybe_body(header("x-h") h?: HeaderValue, body b?: String) { request Text; }

        /// Disabled: its value type does not exist; its literal is still a valid static.
        #[cfg(any())]
        GET gone(header("x-gone") v: DoesNotExist) { headers { "x-gone-lit" = "gone"; } }
    }
}

concord::api! {
    /// Initializers are evaluated per client, never by the macro (own counter, so no other test
    /// interferes).
    pub client Counted(
        header("x-init") init?: HeaderValue = counted(),
        header("accept") accept: Hv = &ACCEPT_JSON,
    ) {
        base "http://counted.invalid";
        GET ping();
    }
}

concord::api! {
    /// Concord's classification is added to native values (sticky through composition).
    pub client Sticky(header("x-k") k: HeaderValue) {
        base "http://sticky.invalid";
        GET public();
        GET marked(#[sensitive] extra: HeaderValue) { headers { append "x-k" = extra; } }
        GET listed(header("authorization") token: HeaderValue);
    }
}

fn http() -> reqwest::Client {
    reqwest::Client::new()
}

fn native(owned: HeaderValue) -> Native {
    Native::new(http(), NativeConfig { owned })
}

async fn served(owned: HeaderValue) -> (Server, Native) {
    let server = Server::reply(Reply::new(204)).await;
    let api = native(owned).with_base_url(server.url()).unwrap();
    (server, api)
}

/// `(bytes, sensitive)` of every value of `name` in the prepared request, in order.
fn prepared_values<T>(p: &concord::Prepared<T>, name: &str) -> Vec<(Vec<u8>, bool)> {
    p.request()
        .headers()
        .get_all(name)
        .iter()
        .map(|v| (v.as_bytes().to_vec(), v.is_sensitive()))
        .collect()
}

fn bytes_of(values: &[(Vec<u8>, bool)]) -> Vec<&[u8]> {
    values.iter().map(|(b, _)| b.as_slice()).collect()
}

fn flags_of(values: &[(Vec<u8>, bool)]) -> Vec<bool> {
    values.iter().map(|(_, s)| *s).collect()
}

/// A native initializer runs once per client creation, not per request, and is not hoisted.
#[test]
fn initializers_run_per_client() {
    assert_eq!(INITIALIZED.load(Ordering::SeqCst), 0);
    let api = Counted::new(http());
    assert_eq!(INITIALIZED.load(Ordering::SeqCst), 1);
    for _ in 0..3 {
        let p = api.ping().prepare().unwrap();
        assert_eq!(bytes_of(&prepared_values(&p, "x-init")), [b"init"]);
        assert_eq!(
            bytes_of(&prepared_values(&p, "accept")),
            [b"application/json"]
        );
    }
    assert_eq!(INITIALIZED.load(Ordering::SeqCst), 1);
    let _second = Counted::new(http());
    assert_eq!(INITIALIZED.load(Ordering::SeqCst), 2);
}

/// H1/H2/H3/H4: client values reach the wire unchanged, flags untouched; the literal is a static.
#[tokio::test]
async fn h1_h4_client_values_on_the_wire() {
    let (server, api) = served(HeaderValue::from_static("owned-1")).await;
    let api = api.set_opt(PLAIN).set_list(vec![
        HeaderValue::from_static("a"),
        latin1(),
        HeaderValue::from_static("c"),
    ]);
    api.plain().send().await.unwrap();

    let r = &server.requests()[0];
    assert_eq!(r.header_bytes_all("x-owned"), [b"owned-1"]);
    assert_eq!(r.header_bytes_all("accept"), [b"application/json"]);
    assert_eq!(r.header_bytes_all("x-opt"), [b"text/plain"]);
    assert_eq!(r.header_bytes_all("x-init"), [b"init"]);
    assert_eq!(
        r.header_bytes_all("x-list"),
        [&b"a"[..], &b"caf\xe9"[..], &b"c"[..]]
    );
    assert_eq!(r.header_bytes_all("x-n"), [b"7"]);
    assert_eq!(r.header_bytes_all("x-lit"), [b"literal"]);

    let p = api.plain().prepare().unwrap();
    for name in [
        "x-owned", "accept", "x-opt", "x-init", "x-list", "x-n", "x-lit",
    ] {
        assert!(
            flags_of(&prepared_values(&p, name)).iter().all(|s| !s),
            "{name}"
        );
    }
    // Payload reuse, not a copy: the request's bytes are the static's bytes.
    let headers = p.request().headers();
    assert_eq!(
        headers["accept"].as_bytes().as_ptr(),
        ACCEPT_JSON.as_bytes().as_ptr()
    );
    assert_eq!(
        headers["x-lit"].as_bytes().as_ptr(),
        __CONCORD_Native_HEADER_VALUE_0.as_bytes().as_ptr()
    );
}

/// H1: setters replace a native value on a new handle; earlier scopes and requests keep theirs.
#[test]
fn h1_setter_and_snapshots() {
    let api = native(HeaderValue::from_static("v1"));
    let scope = api.inherit();
    let request = api.plain();
    let api2 = api.clone().set_owned(HeaderValue::from_static("v2"));
    let owned = |p: concord::Prepared<()>| bytes_of(&prepared_values(&p, "x-owned"))[0].to_vec();
    assert_eq!(owned(api.plain().prepare().unwrap()), b"v1");
    assert_eq!(owned(request.prepare().unwrap()), b"v1");
    assert_eq!(
        owned(scope.kept().prepare().unwrap()),
        b"v1",
        "the scope captured the client"
    );
    assert_eq!(owned(api2.plain().prepare().unwrap()), b"v2");
}

/// H2: one static used as a default, a per-call value (cloned or borrowed) and a setter value.
#[tokio::test]
async fn h2_static_reused() {
    let (server, api) = served(ACCEPT_JSON.clone()).await;
    api.per_call(ACCEPT_JSON.clone()).send().await.unwrap();
    api.per_call(&ACCEPT_JSON).send().await.unwrap();
    api.clone()
        .set_accept(&ACCEPT_JSON)
        .plain()
        .send()
        .await
        .unwrap();
    for r in server.requests() {
        assert_eq!(r.header_bytes_all("accept"), [b"application/json"]);
    }
    assert_eq!(
        server.requests()[0].header_bytes_all("x-call"),
        [b"application/json"]
    );
    assert!(!ACCEPT_JSON.is_sensitive());
}

/// H3: absent optional values send nothing; an initialized optional value can be made absent.
#[test]
fn h3_optional_and_initialized() {
    let api = native(HeaderValue::from_static("o"));
    let p = api.plain().prepare().unwrap();
    assert!(prepared_values(&p, "x-opt").is_empty());
    assert_eq!(bytes_of(&prepared_values(&p, "x-init")), [b"init"]);
    let p = api.clone().maybe_init(None).plain().prepare().unwrap();
    assert!(prepared_values(&p, "x-init").is_empty());
    let p = api.maybe_opt(Some(PLAIN)).plain().prepare().unwrap();
    assert_eq!(bytes_of(&prepared_values(&p, "x-opt")), [b"text/plain"]);
}

/// H4: per-call repeated values keep their order and their individual flags.
#[tokio::test]
async fn h4_each_order_and_flags() {
    let (server, api) = served(HeaderValue::from_static("o")).await;
    let values = vec![
        HeaderValue::from_static("first"),
        flagged("second-secret"),
        HeaderValue::from_static("third"),
    ];
    let p = api.each_call(values.clone(), None).prepare().unwrap();
    let got = prepared_values(&p, "x-each");
    assert_eq!(
        bytes_of(&got),
        [&b"first"[..], &b"second-secret"[..], &b"third"[..]]
    );
    assert_eq!(flags_of(&got), [false, true, false]);
    assert!(prepared_values(&p, "x-one").is_empty());
    p.send().await.unwrap();
    api.each_call(Vec::new(), Some(PLAIN)).send().await.unwrap();
    let r = server.requests();
    // Positive control: the flagged value reaches the server.
    assert_eq!(
        r[0].header_bytes_all("x-each"),
        [&b"first"[..], &b"second-secret"[..], &b"third"[..]]
    );
    assert!(r[1].header_bytes_all("x-each").is_empty());
    assert_eq!(r[1].header_bytes_all("x-one"), [b"text/plain"]);
}

/// H5: bytes that are not text are sent exactly; Concord's output escapes them.
#[tokio::test]
async fn h5_non_text_bytes() {
    let (server, api) = served(HeaderValue::from_static("o")).await;
    let request = api.bytes(latin1());
    assert_eq!(format!("{request:?}"), r#"NativeBytes { v: "caf\xe9" }"#);
    let p = request.prepare().unwrap();
    assert!(format!("{p:?}").contains(r#"x-bytes: "caf\xe9""#), "{p:?}");
    p.send().await.unwrap();
    let r = server.single();
    assert_eq!(r.header_bytes_all("x-bytes"), [b"caf\xe9"]);
    // The text view would have hidden it.
    assert_eq!(r.header("x-bytes"), Some("caf\u{fffd}"));
}

/// H6: a natively flagged value on an unlisted, unmarked header stays flagged and is withheld from
/// every Concord output; it still reaches the server.
#[tokio::test]
async fn h6_native_sensitivity_without_marker() {
    let (server, api) = served(HeaderValue::from_static("o")).await;
    let request = api.secret(flagged("s3cr3t"));
    let debug = format!("{request:?}");
    assert_eq!(debug, "NativeSecret { v: <redacted> }");
    let p = request.prepare().unwrap();
    let got = prepared_values(&p, "x-unlisted");
    assert_eq!(bytes_of(&got), [b"s3cr3t"]);
    assert_eq!(flags_of(&got), [true]);
    let shown = format!("{p:?}");
    assert!(shown.contains("x-unlisted: <redacted>"), "{shown}");
    assert!(!shown.contains("s3cr3t"), "{shown}");
    // reqwest's own `Debug` masks it too, because the flag is kept.
    assert!(!format!("{:?}", p.request()).contains("s3cr3t"));
    p.send().await.unwrap();
    assert_eq!(server.single().header_bytes_all("x-unlisted"), [b"s3cr3t"]);
}

/// H6: native flag and declaration marker combine (OR); neither clears the other.
#[test]
fn h6_native_and_declared_sensitivity_combine() {
    let api = native(HeaderValue::from_static("o"));
    for (value, label) in [
        (HeaderValue::from_static("public"), "unflagged"),
        (flagged("secret"), "flagged"),
    ] {
        let request = api.marked(value);
        assert_eq!(
            format!("{request:?}"),
            "NativeMarked { v: <redacted> }",
            "{label}"
        );
        let p = request.prepare().unwrap();
        assert_eq!(
            flags_of(&prepared_values(&p, "x-marked")),
            [true],
            "{label}"
        );
    }
}

/// Concord's static classification (marker, sticky composition, safety-list name) is added to a
/// native value that is not flagged; the caller's value is not mutated.
#[test]
fn declaration_classification_is_added() {
    let k = HeaderValue::from_static("plain-k");
    let api = Sticky::new(http(), StickyConfig { k: k.clone() });
    // `x-k` reads a `#[sensitive]` value on `marked`'s chain: sticky for every chain.
    let p = api.public().prepare().unwrap();
    assert_eq!(flags_of(&prepared_values(&p, "x-k")), [true]);
    let p = api
        .marked(HeaderValue::from_static("extra"))
        .prepare()
        .unwrap();
    let got = prepared_values(&p, "x-k");
    assert_eq!(bytes_of(&got), [&b"plain-k"[..], &b"extra"[..]]);
    assert_eq!(flags_of(&got), [true, true]);
    let p = api
        .listed(HeaderValue::from_static("Bearer t"))
        .prepare()
        .unwrap();
    assert_eq!(flags_of(&prepared_values(&p, "authorization")), [true]);
    assert!(!k.is_sensitive());
    assert_eq!(
        format!("{api:?}"),
        r#"Sticky { base: "http://sticky.invalid/", k: <redacted> }"#
    );
}

/// H7: a natively flagged inherited value through `override`, `append`, `remove`,
/// `override .. else inherit` and exact replacement. The flag belongs to each value.
#[tokio::test]
async fn h7_inherited_composition() {
    let (server, api) = served(flagged("client-secret")).await;
    let scope = api.inherit();

    let p = scope
        .appended(HeaderValue::from_static("extra"))
        .prepare()
        .unwrap();
    let got = prepared_values(&p, "x-owned");
    assert_eq!(bytes_of(&got), [&b"client-secret"[..], &b"extra"[..]]);
    assert_eq!(flags_of(&got), [true, false]);
    let shown = format!("{p:?}");
    assert!(
        shown.contains("x-owned: <redacted>") && shown.contains("x-owned: extra"),
        "{shown}"
    );
    assert!(!shown.contains("client-secret"), "{shown}");
    p.send().await.unwrap();

    let p = scope
        .overridden(HeaderValue::from_static("public"))
        .prepare()
        .unwrap();
    let got = prepared_values(&p, "x-owned");
    assert_eq!(
        (bytes_of(&got), flags_of(&got)),
        (vec![&b"public"[..]], vec![false])
    );

    let p = scope
        .overridden(flagged("endpoint-secret"))
        .prepare()
        .unwrap();
    assert_eq!(flags_of(&prepared_values(&p, "x-owned")), [true]);

    let p = scope.removed().prepare().unwrap();
    assert!(prepared_values(&p, "x-owned").is_empty());

    let p = scope.kept().prepare().unwrap();
    let got = prepared_values(&p, "x-owned");
    assert_eq!(
        (bytes_of(&got), flags_of(&got)),
        (vec![&b"client-secret"[..]], vec![true])
    );
    let p = scope.kept().set_v(PLAIN).prepare().unwrap();
    let got = prepared_values(&p, "x-owned");
    assert_eq!(
        (bytes_of(&got), flags_of(&got)),
        (vec![&b"text/plain"[..]], vec![false])
    );

    let p = scope.replaced(flagged("exact")).prepare().unwrap();
    let got = prepared_values(&p, "x-owned");
    assert_eq!(
        (bytes_of(&got), flags_of(&got)),
        (vec![&b"exact"[..]], vec![true])
    );

    // Positive control: the inherited secret reached the server.
    assert_eq!(
        server.single().header_bytes_all("x-owned"),
        [&b"client-secret"[..], &b"extra"[..]]
    );
}

/// Generated `Debug` of every owner type with native values: no `Display` needed, flags honored.
#[test]
fn generated_debug_of_every_owner() {
    let config = NativeConfig {
        owned: flagged("cfg-secret"),
    };
    assert_eq!(format!("{config:?}"), "NativeConfig { owned: <redacted> }");
    let config = NativeConfig { owned: latin1() };
    assert_eq!(
        format!("{config:?}"),
        r#"NativeConfig { owned: "caf\xe9" }"#
    );

    let api = Native::new(http(), config)
        .set_opt(flagged("opt-secret"))
        .set_list(vec![HeaderValue::from_static("a"), flagged("b")]);
    assert_eq!(
        format!("{api:?}"),
        r#"Native { base: "http://native.invalid/", owned: "caf\xe9", accept: "application/json", opt: Some(<redacted>), init: Some("init"), list: ["a", <redacted>], n: "7" }"#
    );
    let scope = api.inherit().set_scoped(flagged("scope-secret"));
    assert_eq!(
        format!("{scope:?}"),
        "NativeInherit { scoped: Some(<redacted>) }"
    );
    let scope = scope.maybe_scoped(None);
    assert_eq!(format!("{scope:?}"), "NativeInherit { scoped: None }");

    let request = api.each_call(vec![flagged("x"), PLAIN], Some(latin1()));
    assert_eq!(
        format!("{request:?}"),
        r#"NativeEachCall { v: [<redacted>, "text/plain"], one: ["caf\xe9"] }"#
    );
    let request = api.empty(HeaderValue::from_static(""));
    assert_eq!(
        format!("{request:?}"),
        r#"NativeEmpty { v: "", absent: None }"#
    );
    let request = api.scalars(3, -4, true, "line\nbreak", Detail::Full, Region("eu"));
    // A text value that is not a valid header value is shown as such (the request will fail).
    assert_eq!(
        format!("{request:?}"),
        r#"NativeScalars { a: "3", b: "-4", c: "true", d: <invalid header value>, e: "full", region: "eu" }"#
    );
}

/// `&'static str`, `Cow`, `Arc` and `Box` forward to the text or native value they hold.
#[test]
fn forwarded_forms() {
    let p = native(HeaderValue::from_static("o"))
        .forwarded(
            "lit",
            std::borrow::Cow::Borrowed("cow"),
            std::sync::Arc::from("arc"),
            Box::new(flagged("boxed")),
        )
        .prepare()
        .unwrap();
    for (name, bytes, sensitive) in [
        ("x-str", &b"lit"[..], false),
        ("x-cow", b"cow", false),
        ("x-arc", b"arc", false),
        ("x-boxed", b"boxed", true),
    ] {
        let got = prepared_values(&p, name);
        assert_eq!(
            (bytes_of(&got), flags_of(&got)),
            (vec![bytes], vec![sensitive]),
            "{name}"
        );
    }
}

/// Empty versus absent: an empty native value is sent as an empty header line.
#[tokio::test]
async fn empty_versus_absent() {
    let (server, api) = served(HeaderValue::from_static("o")).await;
    api.empty(HeaderValue::from_static(""))
        .send()
        .await
        .unwrap();
    api.empty(HeaderValue::from_static(""))
        .set_absent(HeaderValue::from_static(""))
        .send()
        .await
        .unwrap();
    let r = server.requests();
    assert_eq!(r[0].header_bytes_all("x-empty"), [b""]);
    assert!(r[0].header_bytes_all("x-absent").is_empty());
    assert_eq!(r[1].header_bytes_all("x-absent"), [b""]);
}

/// H9: integers and `bool` use native conversions; text goes through `HeaderValue::from_str`
/// (native acceptance, UTF-8 included); `Display` types are formatted through templates.
#[tokio::test]
async fn h9_scalars() {
    let (server, api) = served(HeaderValue::from_static("o")).await;
    api.scalars(
        255,
        i64::MIN,
        false,
        "a\tb c",
        Detail::Short,
        Region("eu-west"),
    )
    .send()
    .await
    .unwrap();
    let r = server.single();
    assert_eq!(r.header_bytes_all("x-u8"), [b"255"]);
    assert_eq!(r.header_bytes_all("x-i64"), [b"-9223372036854775808"]);
    assert_eq!(r.header_bytes_all("x-bool"), [b"false"]);
    assert_eq!(r.header_bytes_all("x-text"), [b"a\tb c"]);
    assert_eq!(r.header_bytes_all("x-detail"), [b"short"]);
    assert_eq!(r.header_bytes_all("x-region"), [b"eu-west"]);

    // What `HeaderValue::from_str` accepts is sent as is, beyond ASCII too (H09).
    let p = api
        .scalars(1, 1, true, "café", Detail::Full, Region("é"))
        .prepare()
        .unwrap();
    assert_eq!(
        p.request().headers()["x-text"].as_bytes(),
        "café".as_bytes()
    );
    assert_eq!(p.request().headers()["x-region"].as_bytes(), "é".as_bytes());
    for bad in ["a\r\nx-injected: 1", "\u{7f}", "nul\0"] {
        let error = api
            .scalars(1, 1, true, bad, Detail::Full, Region("eu"))
            .prepare()
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Build);
        let text = error.to_string();
        assert!(text.contains("header `x-text`"), "{text}");
        assert!(text.contains("rejected by `http::HeaderValue`"), "{text}");
        assert!(
            !text.contains("injected") && !text.contains("nul"),
            "{text}"
        );
    }
    let error = api
        .scalars(1, 1, true, "ok", Detail::Full, Region("line\nbreak"))
        .prepare()
        .unwrap_err();
    assert!(error.to_string().contains("header `x-region`"), "{error}");
}

/// A native `Content-Type` replaces the body representation's; an optional native header does not
/// decide whether a body (or its content type) exists.
#[tokio::test]
async fn native_headers_and_bodies() {
    let (server, api) = served(HeaderValue::from_static("o")).await;
    api.upload(HeaderValue::from_static("text/csv; charset=utf-8"), "a,b")
        .send()
        .await
        .unwrap();
    api.maybe_body().send().await.unwrap();
    api.maybe_body().set_h(PLAIN).send().await.unwrap();
    api.maybe_body().set_b("x").send().await.unwrap();
    api.maybe_body()
        .set_h(PLAIN)
        .set_b("x")
        .send()
        .await
        .unwrap();
    let r = server.requests();
    assert_eq!(
        r[0].header_bytes_all("content-type"),
        [b"text/csv; charset=utf-8"]
    );
    assert_eq!(r[0].body, b"a,b");
    for (i, body) in [(1, false), (2, false), (3, true), (4, true)] {
        let content_type = r[i].header_bytes_all("content-type");
        if body {
            assert_eq!(content_type, [b"text/plain; charset=utf-8"], "{i}");
            assert_eq!(r[i].body, b"x", "{i}");
        } else {
            assert!(content_type.is_empty(), "{i}");
            assert!(r[i].body.is_empty(), "{i}");
        }
        let h = r[i].header_bytes_all("x-h");
        assert_eq!(h.len(), usize::from(i == 2 || i == 4), "{i}");
    }
}

/// A full native header map is a build error for native values too (white-box, as in
/// `native_limits.rs`).
#[test]
fn full_header_map_with_native_values() {
    use concord::reqwest::header::HeaderName;
    use concord_core::__private::{Base, EndpointMeta, Parts};

    static META: EndpointMeta = EndpointMeta {
        name: "Native.fill",
        method: "GET",
        labels: 0,
        sensitive_labels: &[],
        segments: 0,
        sensitive_segments: &[],
        sensitive_query: &[],
        sensitive_content_type: false,
        sensitive_accept: false,
        accept_removed: false,
    };
    let base = Base::parse("http://capacity.example.test").unwrap();
    let mut parts = Parts::new(&META, &base);
    let value = flagged("v");
    for i in 0.. {
        let name = HeaderName::from_bytes(format!("x-{i}").as_bytes()).unwrap();
        if let Err(e) = parts.header(name, "header `x-n`", &value, false) {
            assert_eq!(e.kind(), ErrorKind::Build);
            assert!(e.to_string().contains("does not fit"), "{e}");
            assert!(i > 1000, "{i}");
            return;
        }
    }
}

// A sensitive value sent both as a header and as text: every placement — header, query, path —
// is withheld from Concord's output. (`ToHeaderValue` is closed, so the only value that can carry
// a native flag is a `HeaderValue`, which is never text: the 66a4478 P1 combination cannot be
// written; `ui/to_header_value_sealed.rs`.)
concord::api! {
    pub client Both(
        #[sensitive] token: String,
        #[sensitive] header("x-tags") each tags: Vec<String> = Vec::new(),
        version: String = "7",
    ) {
        base "http://both.test";
        headers { "x-private" = token; "x-copy" = "{token}"; "x-version" = version; }
        GET safe() { path "safe"; }
        GET search() {
            path "search/{client.token}/v{client.version}";
            query { "q" = client.token; "t" = each client.tags; "v" = client.version; }
        }
    }
}

#[test]
fn dual_use_values_are_withheld_everywhere_or_are_text() {
    let both = Both::new(
        http(),
        BothConfig {
            token: "S3CR3T-both".into(),
        },
    )
    .set_tags(vec!["S3CR3T-tag".to_owned()]);
    let shown = format!("{both:?}");
    assert!(!shown.contains("S3CR3T"), "{shown}");
    assert!(shown.contains("token: <redacted>"), "{shown}");
    let prepared = both.search().prepare().unwrap();
    let shown = format!("{prepared:?}");
    assert!(!shown.contains("S3CR3T"), "{shown}");
    // A plain text value (`String`) may be both a header and text, and stays visible.
    assert!(
        shown.contains("x-version: 7") && shown.contains("v=7"),
        "{shown}"
    );
    // Positive control: the secret is really sent, flagged, as headers, path and query text.
    let headers = prepared.request().headers();
    assert_eq!(headers["x-private"].as_bytes(), b"S3CR3T-both");
    assert!(headers["x-private"].is_sensitive());
    assert!(headers["x-copy"].is_sensitive());
    assert_eq!(
        prepared.request().url().as_str(),
        "http://both.test/search/S3CR3T-both/v7?q=S3CR3T-both&t=S3CR3T-tag&v=7"
    );
}
