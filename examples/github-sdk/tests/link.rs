//! The `Link` parser against GitHub's documented shapes and against malformed input.
//!
//! In-memory: the parser is the subject, not the network. The documented shape (pagination
//! guide, retrieved 2026-11-04) is `<URL>; rel="prev", <URL>; rel="next", <URL>; rel="last",
//! <URL>; rel="first"`, with `prev` omitted on the first page and `last` omitted when GitHub
//! cannot calculate it.

use concord::reqwest::Url;
use concord::reqwest::header::HeaderValue;
use concord_example_github_sdk::link::{LinkError, parse, parse_str};

fn base() -> Url {
    Url::parse("https://api.github.com/repos/octocat/Hello-World/issues?per_page=2").unwrap()
}

/// The guide's example shape: all four relations, repository-id URLs.
#[test]
fn documented_example_shape() {
    let header = "<https://api.github.com/repositories/1300192/issues?page=2>; rel=\"prev\", \
                  <https://api.github.com/repositories/1300192/issues?page=4>; rel=\"next\", \
                  <https://api.github.com/repositories/1300192/issues?page=515>; rel=\"last\", \
                  <https://api.github.com/repositories/1300192/issues?page=1>; rel=\"first\"";
    let links = parse_str(header, &base()).unwrap();
    assert_eq!(
        links.next().unwrap().as_str(),
        "https://api.github.com/repositories/1300192/issues?page=4"
    );
    assert!(links.prev().is_some() && links.first().is_some());
    assert_eq!(
        links.last().unwrap().query(),
        Some("page=515"),
        "last is reported, never used to compute pages"
    );
}

/// First page without `prev`, and `last` absent (GitHub could not calculate it).
#[test]
fn absent_prev_and_absent_last() {
    let header = "<https://api.github.com/repositories/1/issues?after=Y3Vyc29yOnYyOpK0&per_page=2>; rel=\"next\"";
    let links = parse_str(header, &base()).unwrap();
    assert!(links.next().is_some());
    assert!(links.last().is_none() && links.prev().is_none());
}

/// Last page: `prev`/`first` only, no `next`.
#[test]
fn last_page_has_no_next() {
    let header = "<https://api.github.com/repositories/1/issues?page=1>; rel=\"first\", \
                  <https://api.github.com/repositories/1/issues?page=2>; rel=\"prev\"";
    let links = parse_str(header, &base()).unwrap();
    assert!(links.next().is_none());
    assert!(!links.is_empty());
}

#[test]
fn no_header_is_no_links() {
    let links = parse(std::iter::empty(), &base()).unwrap();
    assert!(links.is_empty() && links.next().is_none());
}

/// Several header lines are one field; relative targets resolve against the response URL; the
/// relation token and parameter names are case-insensitive; a relation list counts each name;
/// unknown parameters and relations are ignored; a comma inside the target is not a separator.
#[test]
fn rfc_8288_details() {
    let lines = [
        HeaderValue::from_static(
            "</repositories/1/issues?page=3&labels=a,b>; title=\"x, y\"; REL=\"NEXT alternate\"",
        ),
        HeaderValue::from_static(
            "<https://api.github.com/repositories/1/issues?page=9>; rel=last; anchor=\"#z\"",
        ),
        HeaderValue::from_static("<https://example.test/other>; rel=\"help\""),
    ];
    let links = parse(lines.iter(), &base()).unwrap();
    assert_eq!(
        links.next().unwrap().as_str(),
        "https://api.github.com/repositories/1/issues?page=3&labels=a,b"
    );
    assert_eq!(links.last().unwrap().query(), Some("page=9"));
}

/// The same URL twice for a relation is harmless; two different URLs are a conflict.
#[test]
fn duplicate_relations() {
    let same = "<https://api.github.com/r?page=2>; rel=\"next\", <https://api.github.com/r?page=2>; rel=\"next\"";
    assert!(parse_str(same, &base()).unwrap().next().is_some());
    let conflict = "<https://api.github.com/r?page=2>; rel=\"next\", <https://api.github.com/r?page=3>; rel=\"next\"";
    assert_eq!(
        parse_str(conflict, &base()).unwrap_err(),
        LinkError::Conflict { rel: "next" }
    );
}

/// Malformed headers are errors, never "no next page".
#[test]
fn malformed_headers_are_errors() {
    let cases = [
        ("https://api.github.com/r?page=2; rel=\"next\"", "no <"),
        (
            "<https://api.github.com/r?page=2; rel=\"next\"",
            "unterminated target",
        ),
        (
            "<https://api.github.com/r?page=2>; rel=\"next",
            "unterminated string",
        ),
        (
            "<https://api.github.com/r?page=2> rel=\"next\"",
            "missing ;",
        ),
        ("<https://api.github.com/r?page=2>; =next", "empty name"),
        ("<http://[::1>; rel=\"next\"", "bad URL"),
    ];
    for (header, why) in cases {
        let error = parse_str(header, &base()).expect_err(why);
        // Display carries a static description and a byte offset, never the header text.
        let shown = format!("{error} {error:?}");
        assert!(!shown.contains("api.github.com"), "{why}: {shown}");
    }
    let not_text =
        HeaderValue::from_bytes(b"<https://api.github.com/r>; rel=\"n\xffext\"").unwrap();
    assert_eq!(parse([&not_text], &base()).unwrap_err(), LinkError::NotText);
}

/// `Debug` of the links says which relations exist, not where they point.
#[test]
fn debug_withholds_targets() {
    let links = parse_str(
        "<https://api.github.com/r?after=SECRET_CURSOR>; rel=\"next\"",
        &base(),
    )
    .unwrap();
    let shown = format!("{links:?}");
    assert!(shown.contains("next: true"));
    assert!(!shown.contains("SECRET_CURSOR"));
}
