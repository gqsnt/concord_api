//! A value whose `Display` returns `fmt::Error` (user code breaking the `Display` contract). Every
//! placement reports a build error naming the placement — none sends partial text and none panics —
//! and Concord's own `Debug` shows a placeholder. Reproduced first by the core track: path segments
//! and labels kept the partial text silently; query, header and `Debug` panicked.

use std::fmt;

use concord::{ErrorKind, reqwest};

/// Writes `abc`, then fails.
#[derive(Clone)]
pub struct Faulty;

impl fmt::Display for Faulty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("abc")?;
        Err(fmt::Error)
    }
}

concord::api! {
    /// Placement fixture.
    pub client F {
        base "http://f.example.test";
        /// Path segment.
        GET seg(v: Faulty) { path "items/{v}"; }
        /// Host label.
        GET label(v: Faulty) { subdomain "{v}"; }
        /// Query value.
        GET query(query v: Faulty);
        /// Repeated query value.
        GET query_each(query each v: Vec<Faulty>);
        /// Header template (a header value placed directly is not formatted: `ToHeaderValue`).
        GET header(v: Faulty) { headers { "x-v" = "{v}"; } }
        /// Template text.
        GET template(v: Faulty) { query { "t" = "x-{v}"; } }
    }
}

fn http() -> reqwest::Client {
    reqwest::Client::new()
}

fn build_error(result: Result<concord::Prepared<()>, concord::Error>, place: &str) {
    let e = result.expect_err("a failing Display must not produce a request");
    assert_eq!(e.kind(), ErrorKind::Build);
    let text = e.to_string();
    assert!(text.contains(place), "{text}");
    assert!(text.contains("could not be formatted"), "{text}");
    assert!(!text.contains("abc"), "{text}");
}

#[test]
fn every_placement_reports_a_build_error() {
    let api = F::new(http());
    build_error(api.seg(Faulty).prepare(), "path segment `{v}`");
    build_error(api.label(Faulty).prepare(), "subdomain label `{v}`");
    build_error(api.query(Faulty).prepare(), "query parameter `v`");
    build_error(
        api.query_each(vec![Faulty]).prepare(),
        "query parameter `v`",
    );
    build_error(api.header(Faulty).prepare(), "header `x-v`");
    build_error(api.template(Faulty).prepare(), "query parameter `t`");
}

#[test]
fn generated_debug_shows_a_placeholder() {
    let endpoint = F::new(http()).seg(Faulty);
    assert_eq!(format!("{endpoint:?}"), "FSeg { v: <Display failed> }");
    let each = F::new(http()).query_each(vec![Faulty]);
    assert_eq!(format!("{each:?}"), "FQueryEach { v: [<Display failed>] }");
}
