//! Request composition: paths, subdomains, query/header slots and their operations.
//! Scenario IDs refer to `v4/TEST_MATRIX.md`; Qn/Hn refer to the composition research matrix.

mod support;

use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};

use concord::reqwest;
use support::{Reply, Server};

fn http() -> reqwest::Client {
    reqwest::Client::new()
}

fn url<T>(p: &concord::Prepared<T>) -> String {
    p.request().url().as_str().to_owned()
}

// ---------------------------------------------------------------------------------------------
// Paths

concord::api! {
    /// Path fixture.
    pub client Paths {
        base "http://paths.invalid/root";
        path "a";
        scope one(x: String) {
            path "b/{x}";
            scope two {
                path "c";
                GET leaf(id: String) { path "d/{id}"; }
                GET mixed(id: String) { path "item-{id}.json"; }
                GET colon(name: String) { path "{name}:undelete"; }
                GET slash() { path "list/"; }
            }
            GET here();
        }
    }
}

/// C01: base path + root + two scopes + endpoint, in order.
#[test]
fn c01_segments_compose_in_order() {
    let p = Paths::new(http())
        .one("x1")
        .two()
        .leaf("7")
        .prepare()
        .unwrap();
    assert_eq!(url(&p), "http://paths.invalid/root/a/b/x1/c/d/7");
}

/// C02: values are one segment each, percent-encoded exactly once.
#[test]
fn c02_dynamic_segments_are_encoded_once() {
    let cases = [
        ("a/b", "a%2Fb"),
        ("%2F", "%252F"),
        ("a b", "a%20b"),
        ("café", "caf%C3%A9"),
        ("a;b", "a%3Bb"),
        ("a?b#c", "a%3Fb%23c"),
        ("a\\b", "a%5Cb"),
        ("..x", "..x"),
        ("~._-", "~._-"),
    ];
    for (value, encoded) in cases {
        let p = Paths::new(http())
            .one("x")
            .two()
            .leaf(value)
            .prepare()
            .unwrap();
        assert_eq!(
            url(&p),
            format!("http://paths.invalid/root/a/b/x/c/d/{encoded}"),
            "{value:?}"
        );
    }
}

/// Literal text around a hole stays verbatim; the value is encoded.
#[test]
fn c02_mixed_segments_and_literal_pchar() {
    let p = Paths::new(http())
        .one("x")
        .two()
        .mixed("a b")
        .prepare()
        .unwrap();
    assert!(url(&p).ends_with("/item-a%20b.json"), "{}", url(&p));
    let p = Paths::new(http())
        .one("x")
        .two()
        .colon("proj")
        .prepare()
        .unwrap();
    assert!(url(&p).ends_with("/proj:undelete"), "{}", url(&p));
}

/// C03: a value producing an empty, `.` or `..` segment is a build error that does not show the
/// value.
#[test]
fn c03_dot_and_empty_segments_are_rejected() {
    for value in ["", ".", ".."] {
        let error = Paths::new(http())
            .one("x")
            .two()
            .leaf(value)
            .prepare()
            .unwrap_err();
        assert_eq!(error.kind(), concord::ErrorKind::Build);
        assert_eq!(
            error.to_string(),
            "GET Paths.one.two.leaf: request not built: path segment `{id}` is empty, `.` or `..`"
        );
    }
}

/// An endpoint may end its own path with `/`; an endpoint without a path uses its parents'.
#[test]
fn c13_trailing_slash_and_inherited_path() {
    let p = Paths::new(http()).one("x").two().slash().prepare().unwrap();
    assert_eq!(url(&p), "http://paths.invalid/root/a/b/x/c/list/");
    let p = Paths::new(http()).one("x").here().prepare().unwrap();
    assert_eq!(url(&p), "http://paths.invalid/root/a/b/x");
}

/// C13: base URL variants: port, trailing slash, no path; runtime override validation.
#[test]
fn c13_base_url_variants() {
    let leaf = |base: &str| {
        let api = Paths::new(http()).with_base_url(base).unwrap();
        url(&api.one("x").two().leaf("1").prepare().unwrap())
    };
    assert_eq!(leaf("http://h.test"), "http://h.test/a/b/x/c/d/1");
    assert_eq!(leaf("http://h.test/"), "http://h.test/a/b/x/c/d/1");
    assert_eq!(
        leaf("http://h.test:8080/root"),
        "http://h.test:8080/root/a/b/x/c/d/1"
    );
    assert_eq!(
        leaf("https://h.test/root/"),
        "https://h.test/root/a/b/x/c/d/1"
    );
    for bad in [
        "ftp://h.test",
        "http://u:p@h.test",
        "http://h.test/?q=1",
        "http://h.test/#f",
        "/relative",
    ] {
        let error = Paths::new(http()).with_base_url(bad).unwrap_err();
        assert_eq!(error.kind(), concord::ErrorKind::Build, "{bad}");
        assert!(!error.to_string().contains("h.test"), "{error}");
    }
}

// ---------------------------------------------------------------------------------------------
// Subdomains

concord::api! {
    /// Subdomain fixture.
    pub client Hosts {
        base "https://example.test";
        subdomain "api";
        scope region(region: String) {
            subdomain "{region}";
            scope shard(n: u32) {
                subdomain "s{n}";
                GET deep();
            }
            GET status();
        }
    }
}

/// C04: labels prepend to the base host, deepest owner leftmost; values are lowercased.
#[test]
fn c04_labels_prepend_deepest_first() {
    let p = Hosts::new(http())
        .region("EU")
        .shard(3)
        .deep()
        .prepare()
        .unwrap();
    assert_eq!(url(&p), "https://s3.eu.api.example.test/");
    let p = Hosts::new(http())
        .region("eu-west-1")
        .status()
        .prepare()
        .unwrap();
    assert_eq!(url(&p), "https://eu-west-1.api.example.test/");
}

/// C04: a value cannot inject separators, ports, non-ASCII or invalid labels.
#[test]
fn c04_label_values_are_validated() {
    for bad in [
        "",
        "a.b",
        "evil.test:1",
        "a_b",
        "-a",
        "a-",
        "café",
        "ｅｕ",
        "a%2eb",
        &"x".repeat(64),
    ] {
        let error = Hosts::new(http())
            .region(bad)
            .status()
            .prepare()
            .unwrap_err();
        assert_eq!(error.kind(), concord::ErrorKind::Build, "{bad:?}");
        assert!(
            error.to_string().contains("subdomain label `{region}`"),
            "{error}"
        );
    }
}

/// Subdomains require a domain base, not an IP address.
#[test]
fn c04_ip_base_with_subdomain_is_rejected() {
    let api = Hosts::new(http())
        .with_base_url("http://127.0.0.1:9")
        .unwrap();
    let error = api.region("eu").status().prepare().unwrap_err();
    assert_eq!(error.kind(), concord::ErrorKind::Build);
}

// ---------------------------------------------------------------------------------------------
// Query slots and operations

concord::api! {
    /// Query-operation fixture.
    pub client Q {
        base "http://q.invalid";
        query { "k" = "a"; "z" = "9"; }

        // Q2: an override with a total value replaces the slot in place.
        GET q2() { query { override "k" = "b"; } }

        scope opt(over?: String) {
            query { override "k" = over else inherit; }
            // Q5: nested optional overrides fall back outward.
            scope opt2(over2?: String) {
                query { override "k" = over2 else inherit; }
                GET q5();
            }
            // Q4: an absent optional override keeps the inherited occurrences.
            GET q4();
        }

        scope rm {
            query { remove "k"; }
            scope again {
                query { "k" = "c"; }
                // Q6: remove, reintroduce, append.
                GET q6() { query { append "k" = "d"; } }
            }
            // Exact replacement is `remove` + introduce: an absent value leaves nothing.
            GET exact(k?: String) { query { "k" = k; } }
        }

        // Q10: a present but empty `each` override removes the inherited occurrences.
        GET q10(tags: Vec<String>) { query { override "k" = each tags; } }

        GET repeated(tags: Vec<String>, cursor?: String) {
            query { "tag" = each tags; "cursor" = cursor; append "tag" = "last"; }
        }

        GET cased() { query { "K" = "upper"; } }
    }
}

fn q<T>(p: &concord::Prepared<T>) -> String {
    p.request().url().query().unwrap_or("<none>").to_owned()
}

/// C08/Q2: an override with a total value replaces the slot in place.
#[test]
fn q2_override_total_in_place() {
    assert_eq!(q(&Q::new(http()).q2().prepare().unwrap()), "k=b&z=9");
}

/// C08/Q4: an absent optional override keeps the inherited value (D4).
#[test]
fn q4_override_absent_keeps_inherited() {
    assert_eq!(q(&Q::new(http()).opt().q4().prepare().unwrap()), "k=a&z=9");
    assert_eq!(
        q(&Q::new(http()).opt().set_over("b").q4().prepare().unwrap()),
        "k=b&z=9"
    );
}

/// C08/Q5: nested optional overrides: the innermost present value wins.
#[test]
fn q5_nested_overrides() {
    let api = Q::new(http());
    assert_eq!(q(&api.opt().opt2().q5().prepare().unwrap()), "k=a&z=9");
    assert_eq!(
        q(&api.opt().set_over("b").opt2().q5().prepare().unwrap()),
        "k=b&z=9"
    );
    assert_eq!(
        q(&api
            .opt()
            .set_over("b")
            .opt2()
            .set_over2("c")
            .q5()
            .prepare()
            .unwrap()),
        "k=c&z=9"
    );
    assert_eq!(
        q(&api.opt().opt2().set_over2("c").q5().prepare().unwrap()),
        "k=c&z=9"
    );
}

/// C08/Q6: removal ends ownership; a reintroduced key keeps its first slot; append adds to it.
#[test]
fn q6_remove_reintroduce_append() {
    assert_eq!(
        q(&Q::new(http()).rm().again().q6().prepare().unwrap()),
        "k=c&k=d&z=9"
    );
}

/// D4: exact replacement (`remove` + introduce) with an absent value leaves no occurrence.
#[test]
fn remove_then_introduce_absent_removes() {
    assert_eq!(q(&Q::new(http()).rm().exact().prepare().unwrap()), "z=9");
    assert_eq!(
        q(&Q::new(http()).rm().exact().set_k("e").prepare().unwrap()),
        "k=e&z=9"
    );
}

/// C08/Q10: a present, empty `each` override still replaces (with zero occurrences).
#[test]
fn q10_present_empty_each_override_removes() {
    assert_eq!(q(&Q::new(http()).q10(vec![]).prepare().unwrap()), "z=9");
    assert_eq!(
        q(&Q::new(http())
            .q10(vec!["x".into(), "y".into()])
            .prepare()
            .unwrap()),
        "k=x&k=y&z=9"
    );
}

/// C06: repeated keys keep order and repetitions; `append` joins the key's slot.
#[test]
fn c06_repeated_keys() {
    let p = Q::new(http())
        .repeated(vec!["a".into(), "b".into(), "a".into()])
        .set_cursor("")
        .prepare()
        .unwrap();
    assert_eq!(q(&p), "k=a&z=9&tag=a&tag=b&tag=a&tag=last&cursor=");
}

/// Query keys are case-sensitive.
#[test]
fn query_keys_are_case_sensitive() {
    assert_eq!(
        q(&Q::new(http()).cased().prepare().unwrap()),
        "k=a&z=9&K=upper"
    );
}

/// Form encoding of query values (space as `+`, reserved characters escaped).
#[test]
fn query_values_are_form_encoded() {
    let p = Q::new(http())
        .repeated(vec!["a b".into(), "x&y=z+%".into()])
        .prepare()
        .unwrap();
    assert_eq!(q(&p), "k=a&z=9&tag=a+b&tag=x%26y%3Dz%2B%25&tag=last");
}

/// No `?` is produced when nothing survives: every contribution is absent or removed.
#[test]
fn no_query_string_when_nothing_survives() {
    let p = Q::new(http()).rm().exact().prepare().unwrap();
    assert_eq!(q(&p), "z=9", "fixture sanity: only z survives");
    let p = Lazy::new(http()).removed().prepare().unwrap();
    assert_eq!(p.request().url().query(), None);
    assert!(!url(&p).contains('?'), "{}", url(&p));
}

// ---------------------------------------------------------------------------------------------
// Suppressed contributions are never formatted

static FORMATTED: AtomicUsize = AtomicUsize::new(0);

/// A value that counts how often it is formatted.
#[derive(Clone, Debug)]
pub struct Counted;

impl fmt::Display for Counted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        FORMATTED.fetch_add(1, Ordering::SeqCst);
        f.write_str("counted")
    }
}

concord::api! {
    /// Encoder-suppression fixture.
    pub client Lazy(expensive: Counted = Counted) {
        base "http://lazy.invalid";
        query { "k" = expensive; }
        GET replaced() { query { override "k" = "cheap"; } }
        GET removed() { query { remove "k"; } }
        GET kept();
    }
}

/// C08: a replaced or removed contribution is never formatted; a surviving one exactly once.
#[test]
fn c08_suppressed_contributions_are_not_formatted() {
    let api = Lazy::new(http());
    let before = FORMATTED.load(Ordering::SeqCst);
    assert_eq!(q(&api.replaced().prepare().unwrap()), "k=cheap");
    assert_eq!(q(&api.removed().prepare().unwrap()), "<none>");
    assert_eq!(FORMATTED.load(Ordering::SeqCst), before);
    assert_eq!(q(&api.kept().prepare().unwrap()), "k=counted");
    assert_eq!(FORMATTED.load(Ordering::SeqCst), before + 1);
}

// ---------------------------------------------------------------------------------------------
// Headers

concord::api! {
    /// Header fixture.
    pub client H(#[sensitive] token?: String) {
        base "http://h.invalid";
        headers { "X-Tag" = "a"; "x-other" = "o"; "authorization" = "Bearer {token}"; }

        GET appended() { headers { append "x-tag" = "b"; } }
        GET overridden() { headers { override "X-TAG" = "c"; } }
        GET removed() { headers { remove "Authorization"; } }
        GET content_type(text payload: String) { headers { "content-type" = "text/csv"; } }
    }
}

/// C09: header names are case-insensitive; `append` adds values after the inherited ones under
/// the same name; `override` replaces the name's values. Only the order of values *within* one
/// name is asserted: `http::HeaderMap` does not promise an order between different names, so
/// neither does Concord (L2).
#[tokio::test]
async fn c09_header_slots_on_the_wire() {
    let server = Server::reply(Reply::new(204)).await;
    let api = H::new(http()).with_base_url(server.url()).unwrap();

    api.appended().send().await.unwrap();
    api.overridden().send().await.unwrap();
    let requests = server.requests();

    let names = |r: &support::Recorded| -> Vec<String> {
        let mut names: Vec<String> = r
            .headers
            .iter()
            .map(|(n, _)| n.clone())
            .filter(|n| n.starts_with('x'))
            .collect();
        names.sort();
        names
    };
    assert_eq!(requests[0].header_all("x-tag"), ["a", "b"]);
    assert_eq!(requests[0].header_all("x-other"), ["o"]);
    assert_eq!(names(&requests[0]), ["x-other", "x-tag", "x-tag"]);
    assert_eq!(requests[1].header_all("x-tag"), ["c"]);
    assert_eq!(requests[1].header_all("x-other"), ["o"]);
    assert_eq!(names(&requests[1]), ["x-other", "x-tag"]);
}

/// C10: `Authorization` is an ordinary header; an absent optional value omits the whole entry.
#[tokio::test]
async fn c10_authorization_is_an_ordinary_header() {
    let server = Server::reply(Reply::new(204)).await;
    let api = H::new(http()).with_base_url(server.url()).unwrap();

    api.clone()
        .set_token("s3cr3t")
        .appended()
        .send()
        .await
        .unwrap();
    api.appended().send().await.unwrap();
    api.set_token("s3cr3t").removed().send().await.unwrap();
    let requests = server.requests();

    assert_eq!(requests[0].header("authorization"), Some("Bearer s3cr3t"));
    assert_eq!(requests[1].header("authorization"), None);
    assert_eq!(requests[2].header("authorization"), None);
}

/// A declared `Content-Type` wins over the body representation's default.
#[tokio::test]
async fn declared_content_type_wins() {
    let server = Server::reply(Reply::new(204)).await;
    let api = H::new(http()).with_base_url(server.url()).unwrap();
    api.content_type("a,b".to_owned()).send().await.unwrap();
    let request = server.single();
    assert_eq!(request.header("content-type"), Some("text/csv"));
    assert_eq!(request.body, b"a,b");
}

/// L02: header values are validated when the request is built; the error names the header, not
/// the value.
#[test]
fn l02_invalid_header_value_is_a_build_error() {
    let error = H::new(http())
        .set_token("a\r\nx-injected: 1")
        .appended()
        .prepare()
        .unwrap_err();
    assert_eq!(error.kind(), concord::ErrorKind::Build);
    let text = error.to_string();
    assert!(text.contains("authorization"), "{text}");
    assert!(!text.contains("injected"), "{text}");
}

/// reqwest client default headers only fill names the request does not set.
#[tokio::test]
async fn client_default_headers_fill_only_missing_names() {
    let server = Server::reply(Reply::new(204)).await;
    let mut defaults = reqwest::header::HeaderMap::new();
    defaults.insert("x-tag", "client".parse().unwrap());
    defaults.insert("x-client-only", "yes".parse().unwrap());
    let http = reqwest::Client::builder()
        .default_headers(defaults)
        .build()
        .unwrap();
    let api = H::new(http).with_base_url(server.url()).unwrap();
    api.overridden().send().await.unwrap();
    let request = server.single();
    assert_eq!(request.header_all("x-tag"), ["c"]);
    assert_eq!(request.header("x-client-only"), Some("yes"));
}

// ---------------------------------------------------------------------------------------------
// Names

concord::api! {
    /// Ancestor-qualification fixture.
    pub client Names(locale: String = "root") {
        base "http://names.invalid";
        query { "root_locale" = locale; }
        scope sub(locale: String = "sub") {
            query { "sub_locale" = locale; }
            GET both() { query { "from_client" = client.locale; "from_sub" = sub.locale; } }
        }
    }
}

/// C05: a bare name binds the current owner; ancestors are qualified explicitly.
#[test]
fn c05_bare_names_bind_the_current_owner() {
    let p = Names::new(http())
        .set_locale("r")
        .sub()
        .set_locale("s")
        .both()
        .prepare()
        .unwrap();
    assert_eq!(q(&p), "root_locale=r&sub_locale=s&from_client=r&from_sub=s");
}

// ---------------------------------------------------------------------------------------------
// Regressions from the Gate 2 Codex review

concord::api! {
    /// C4-R-002 / C4-R-003 / body-order fixture.
    pub client Regress {
        base "http://regress.invalid";
        GET braces(x: String) { query { "q" = "{{ok}}"; "r" = "{{{x}}}"; } headers { "x-note" = "}}{{"; } }
        scope repo(x: String) {
            GET repo() { path "{repo.x}"; }
        }
        POST ordered(id: u64, text payload: String, tag: String) {
            path "o/{id}";
            query { "tag" = tag; }
        }
    }
}

/// C4-R-002: escaped braces mean the same with and without holes in the template.
#[test]
fn c4_r_002_escaped_braces() {
    let p = Regress::new(http()).braces("v").prepare().unwrap();
    assert_eq!(q(&p), "q=%7Bok%7D&r=%7Bv%7D");
    assert_eq!(p.request().headers()["x-note"], "}{");
}

/// C4-R-003: an endpoint may share its name with an enclosing scope and still qualify it.
#[test]
fn c4_r_003_endpoint_named_like_scope() {
    let p = Regress::new(http())
        .repo("from-scope")
        .repo()
        .prepare()
        .unwrap();
    assert_eq!(p.request().url().path(), "/from-scope");
}

/// A required body is a positional argument in declaration order.
#[test]
fn required_body_is_positional_in_declaration_order() {
    let p = Regress::new(http())
        .ordered(7, "text".to_owned(), "t")
        .prepare()
        .unwrap();
    assert_eq!(url(&p), "http://regress.invalid/o/7?tag=t");
}

concord::api! {
    /// C4-R-004: base URL forms accepted by the strict declaration subset.
    pub client V6 {
        base "http://[::1]:8080/v1/api-x_y~z!$&'()*+,;=:@";
        GET ping();
    }
}

#[test]
fn c4_r_004_strict_base_subset_constructs() {
    let p = V6::new(http()).ping().prepare().unwrap();
    assert_eq!(url(&p), "http://[::1]:8080/v1/api-x_y~z!$&'()*+,;=:@");
}

/// An invalid punycode host given at runtime is an error, not a panic.
#[test]
fn runtime_base_with_invalid_punycode_is_an_error() {
    let error = Paths::new(http())
        .with_base_url("http://a.b.c.xn--pokxncvks")
        .unwrap_err();
    assert_eq!(error.kind(), concord::ErrorKind::Build);
    // The same label as a subdomain value: URL host validation rejects it at build time.
    let error = Hosts::new(http())
        .region("xn--pokxncvks")
        .status()
        .prepare()
        .unwrap_err();
    assert_eq!(error.kind(), concord::ErrorKind::Build);
    assert!(!error.to_string().contains("pokxncvks"), "{error}");
}

concord::api! {
    /// C4-R-013 fixture: competing build failures.
    pub client Order {
        base "http://order.example.test";
        scope region(region: String) {
            subdomain "{region}";
            GET item(id: String) { path "items/{id}"; headers { "x-id" = "{id}"; } }
        }
    }
}

/// C4-R-013: build checks run in the documented order: body, host (labels, IP base, length),
/// path, query, headers — the first failure is reported.
#[test]
fn c4_r_013_build_error_precedence() {
    // Invalid label and invalid path value: the label error wins.
    let e = Order::new(http())
        .region("a.b")
        .item("..")
        .prepare()
        .unwrap_err();
    assert!(e.to_string().contains("subdomain label"), "{e}");
    // IP base (host error) and invalid path value: the host error wins.
    let api = Order::new(http())
        .with_base_url("http://127.0.0.1:9")
        .unwrap();
    let e = api.region("eu").item("..").prepare().unwrap_err();
    assert!(
        e.to_string()
            .contains("subdomain requires a base URL with a domain name"),
        "{e}"
    );
    // Invalid path value and invalid header value: the path error wins.
    let e = Order::new(http())
        .region("eu")
        .item("..")
        .prepare()
        .unwrap_err();
    assert!(e.to_string().contains("path segment"), "{e}");
    let e = Order::new(http())
        .region("eu")
        .item("a\nb")
        .prepare()
        .unwrap_err();
    assert!(e.to_string().contains("x-id"), "{e}");
}

// ---------------------------------------------------------------------------------------------
// Gaps found by the test-matrix audit

concord::api! {
    /// `each` over a required `Option` under `override`.
    pub client Each(query("p") p: u32 = 1) {
        base "http://each.invalid";
        query { "b" = "blk"; "k" = "a"; }
        GET clr(o: Option<String>) { query { override "k" = each o; } }
    }
}

/// SEMANTICS §5: `each` iterates any `IntoIterator`; a required `Option` that is `None` yields no
/// occurrence and, under `override`, clears the inherited key.
#[test]
fn each_over_option_under_override() {
    assert_eq!(
        q(&Each::new(http()).clr(None).prepare().unwrap()),
        "p=1&b=blk"
    );
    assert_eq!(
        q(&Each::new(http())
            .clr(Some("z".to_owned()))
            .prepare()
            .unwrap()),
        "p=1&b=blk&k=z"
    );
}

concord::api! {
    /// No owner has a path.
    pub client Bare {
        base "http://bare.invalid";
        GET root();
    }
}

/// SEMANTICS §4: with no generated segments the base path is used exactly as given; with
/// segments, one trailing slash of the base is dropped before joining.
#[test]
fn base_path_without_generated_segments_is_unchanged() {
    let url_of = |base: &str| {
        url(&Bare::new(http())
            .with_base_url(base)
            .unwrap()
            .root()
            .prepare()
            .unwrap())
    };
    assert_eq!(url_of("http://h.test"), "http://h.test/");
    assert_eq!(url_of("http://h.test/root"), "http://h.test/root");
    assert_eq!(url_of("http://h.test/root/"), "http://h.test/root/");
    assert_eq!(url_of("http://h.test/root//"), "http://h.test/root//");
    let joined = |base: &str| {
        url(&Paths::new(http())
            .with_base_url(base)
            .unwrap()
            .one("x")
            .here()
            .prepare()
            .unwrap())
    };
    assert_eq!(joined("http://h.test/root//"), "http://h.test/root//a/b/x");
    let error = Bare::new(http())
        .with_base_url("http://h.test:0")
        .unwrap_err();
    assert_eq!(error.kind(), concord::ErrorKind::Build);
}

/// SEMANTICS §4: a 63-byte label is accepted; a host longer than 253 bytes is a build error.
#[test]
fn dns_length_limits() {
    let label63 = "a".repeat(63);
    let p = Hosts::new(http())
        .region(&label63)
        .status()
        .prepare()
        .unwrap();
    assert!(url(&p).starts_with(&format!("https://{label63}.api.example.test")));
    let long_base = format!("https://{}.example.test", vec!["b".repeat(60); 3].join("."));
    let api = Hosts::new(http()).with_base_url(long_base).unwrap();
    let error = api.region(&label63).status().prepare().unwrap_err();
    assert_eq!(error.kind(), concord::ErrorKind::Build);
    assert!(
        error.to_string().contains("host is longer than 253 bytes"),
        "{error}"
    );
}

concord::api! {
    /// Header value policy.
    pub client Hv(header("x-note") note: String) {
        base "http://hv.invalid";
        GET e();
    }
}

/// Header text follows `http::HeaderValue::from_str`: what it accepts is sent (UTF-8 beyond ASCII
/// included), anything else (a line break) is a build error naming the header but not the value.
#[test]
fn header_value_policy() {
    let p = Hv::new(
        http(),
        HvConfig {
            note: "a\tb c".into(),
        },
    )
    .e()
    .prepare()
    .unwrap();
    assert_eq!(p.request().headers()["x-note"].as_bytes(), b"a\tb c");
    let p = Hv::new(
        http(),
        HvConfig {
            note: "café".into(),
        },
    )
    .e()
    .prepare()
    .unwrap();
    assert_eq!(
        p.request().headers()["x-note"].as_bytes(),
        "café".as_bytes()
    );
    let error = Hv::new(
        http(),
        HvConfig {
            note: "caf\r\nx-injected: 1".into(),
        },
    )
    .e()
    .prepare()
    .unwrap_err();
    assert_eq!(error.kind(), concord::ErrorKind::Build);
    let text = error.to_string();
    assert!(text.contains("x-note") && !text.contains("caf"), "{text}");
}

concord::api! {
    /// D04: per-call inherit / replace / omit, exact replacement through a direct placement, and
    /// the slot position of a reintroduced key.
    pub client Locale(query locale: String = "en") {
        base "http://locale.invalid";
        query { "v" = "1"; }

        GET inherit_or_set(page_locale?: String) {
            query { override "locale" = page_locale else inherit; }
        }
        GET exact(query locale?: String) { query { remove "locale"; } }
        GET tri(locale?: Option<String>) { query { override "locale" = each locale else inherit; } }
        GET reintroduced() { query { remove "locale"; "locale" = "nl"; } }
    }
}

/// D04 truth table: `else inherit` keeps the inherited value when the source is absent; `remove`
/// plus a direct placement is exact replacement (absence sends nothing); `x?: Option<T>` with
/// `each ... else inherit` gives the caller all three choices; a reintroduced key keeps the slot
/// position of its first appearance.
#[test]
fn d04_override_truth_table() {
    let api = Locale::new(http());
    assert_eq!(q(&api.inherit_or_set().prepare().unwrap()), "locale=en&v=1");
    let p = api
        .inherit_or_set()
        .set_page_locale("de")
        .prepare()
        .unwrap();
    assert_eq!(q(&p), "locale=de&v=1");

    assert_eq!(q(&api.exact().prepare().unwrap()), "v=1");
    assert_eq!(
        q(&api.exact().set_locale("de").prepare().unwrap()),
        "locale=de&v=1"
    );

    assert_eq!(q(&api.tri().prepare().unwrap()), "locale=en&v=1");
    let p = api
        .tri()
        .set_locale(Some("fr".to_owned()))
        .prepare()
        .unwrap();
    assert_eq!(q(&p), "locale=fr&v=1");
    assert_eq!(q(&api.tri().set_locale(None).prepare().unwrap()), "v=1");
    let p = api
        .tri()
        .set_locale(None)
        .maybe_locale(None)
        .prepare()
        .unwrap();
    assert_eq!(q(&p), "locale=en&v=1");

    assert_eq!(q(&api.reintroduced().prepare().unwrap()), "locale=nl&v=1");
}
