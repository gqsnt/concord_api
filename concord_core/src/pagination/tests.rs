use std::error::Error;
use std::num::{NonZeroU32, NonZeroU64};
use std::sync::Arc;
use std::time::Duration;

use http::{HeaderMap, HeaderName, HeaderValue, Method};
use url::Url;

use super::identity::{CanonicalIdentity, HeaderField, IdentityInput};
use super::limits::CycleDetector;
use super::*;
use crate::auth::{AccessToken, Auth, CredentialRef, SlotSelector};
use crate::error::PaginationErrorKind;
use crate::hooks::DebugLevel;
use crate::redaction::SanitizedHeaderValue;
use crate::request::{HeadBuilder, RequestMeta, RequestOptions};

fn identity(
    endpoint: &[u8],
    method: &[u8],
    url: &[u8],
    headers: &[HeaderField],
) -> CanonicalIdentity {
    CanonicalIdentity::encode(IdentityInput {
        endpoint_discriminator: endpoint,
        method_token: method,
        canonical_logical_url: url,
        public_headers: headers,
    })
    .expect("canonical identity")
}

#[test]
fn page_limits_and_safe_errors_have_exact_stable_structure() {
    assert_eq!(PageLimit::from(0), PageLimit::Take(0));
    assert_eq!(PageLimit::from(7), PageLimit::Take(7));
    assert_eq!(
        PageLimit::HardCap(NonZeroU32::new(3).expect("nonzero")),
        PageLimit::HardCap(NonZeroU32::new(3).expect("nonzero"))
    );

    let error = PaginationError::with_safe_message(
        PaginationErrorKind::Controller,
        "safe controller detail",
    );
    assert_eq!(error.kind(), PaginationErrorKind::Controller);
    assert_eq!(
        error.to_string(),
        "pagination failed: safe controller detail"
    );
    assert!(format!("{error:?}").contains("Controller"));
    assert!(error.source().is_none());
}

#[derive(Clone)]
struct CursorPage {
    next: Option<String>,
}

impl HasNextCursor<String> for CursorPage {
    fn next_cursor(&self) -> Option<&String> {
        self.next.as_ref()
    }
}

#[test]
fn built_in_controllers_progress_checked_and_preserve_absent_vs_empty_cursor() {
    let offset = OffsetPagination::new(7, NonZeroU32::new(4).expect("nonzero"));
    let next = <OffsetPagination as PaginationController<()>>::next(&offset, &())
        .expect("offset next")
        .expect("offset continues");
    assert_eq!(next.offset(), 11);
    assert_eq!(next.page_size().get(), 4);
    let overflow = OffsetPagination::new(u64::MAX, NonZeroU32::MIN);
    let Err(error) = <OffsetPagination as PaginationController<()>>::next(&overflow, &()) else {
        panic!("offset overflow must fail");
    };
    assert_eq!(error.kind(), PaginationErrorKind::Overflow);

    let page = PagePagination::new(
        NonZeroU64::new(9).expect("nonzero"),
        NonZeroU32::new(2).expect("nonzero"),
    );
    let next = <PagePagination as PaginationController<()>>::next(&page, &())
        .expect("page next")
        .expect("page continues");
    assert_eq!(next.page().get(), 10);
    assert_eq!(next.page_size().get(), 2);
    let overflow =
        PagePagination::new(NonZeroU64::new(u64::MAX).expect("nonzero"), NonZeroU32::MIN);
    let Err(error) = <PagePagination as PaginationController<()>>::next(&overflow, &()) else {
        panic!("page overflow must fail");
    };
    assert_eq!(error.kind(), PaginationErrorKind::Overflow);

    let cursor = CursorPagination::new(None::<String>);
    let empty = cursor
        .next(&CursorPage {
            next: Some(String::new()),
        })
        .expect("cursor next")
        .expect("present empty cursor continues");
    assert_eq!(empty.cursor().map(String::as_str), Some(""));
    assert!(
        empty
            .next(&CursorPage { next: None })
            .expect("absent cursor")
            .is_none()
    );
}

#[test]
fn custom_patch_changes_logical_identity_but_redacts_every_safe_view() {
    const QUERY_SECRET: &str = "query-pagination-sentinel-96d152";
    const HEADER_SECRET: &str = "header-pagination-sentinel-b8473e";
    let mut patch = PaginationPatch::default();
    let mut request = PaginationRequest::new(&mut patch);
    request
        .set_query("cursor", QUERY_SECRET)
        .expect("query patch");
    request
        .set_header(HeaderName::from_static("x-page"), HEADER_SECRET)
        .expect("header patch");

    let mut builder = HeadBuilder::new(
        &Url::parse("https://pagination.example.test/v1?keep=yes&cursor=old").expect("base"),
        RequestMeta::new("PaginationApi::items", Method::GET, None),
        &RequestOptions::new(),
        DebugLevel::None,
    );
    builder
        .insert_header_value("x-public", &"visible")
        .expect("public header");
    let mut head = builder.finish();
    head.apply_pagination_patch(patch).expect("apply patch");

    assert_eq!(
        head.logical_url().query(),
        Some("keep=yes&cursor=query-pagination-sentinel-96d152")
    );
    assert_eq!(head.public_headers()["x-page"], HEADER_SECRET);
    assert!(!head.safe_url().contains(QUERY_SECRET));
    assert!(head.safe_url().contains("%3Credacted%3E"));
    assert_eq!(
        head.sanitized_public_headers()
            .get(&HeaderName::from_static("x-page")),
        Some(SanitizedHeaderValue::Redacted)
    );
    let debug = format!("{:?}", head.sanitized_public_headers());
    assert!(!debug.contains(HEADER_SECRET));

    let identity = CanonicalIdentity::from_request_head(
        head.endpoint(),
        head.method(),
        head.logical_url(),
        head.public_headers(),
    )
    .expect("identity");
    assert!(
        identity
            .bytes()
            .windows(QUERY_SECRET.len())
            .any(|bytes| bytes == QUERY_SECRET.as_bytes())
    );
    assert!(
        identity
            .bytes()
            .windows(HEADER_SECRET.len())
            .any(|bytes| bytes == HEADER_SECRET.as_bytes())
    );
}

#[test]
fn custom_patch_rejects_reserved_or_invalid_values_without_retaining_them() {
    let mut patch = PaginationPatch::default();
    let mut request = PaginationRequest::new(&mut patch);
    assert_eq!(
        request
            .set_query("", "value")
            .expect_err("empty query name")
            .kind(),
        PaginationErrorKind::Controller
    );
    assert_eq!(
        request
            .set_header(http::header::AUTHORIZATION, "secret")
            .expect_err("reserved header")
            .kind(),
        PaginationErrorKind::Controller
    );
    assert_eq!(
        request
            .set_header(HeaderName::from_static("x-page"), "bad\r\nvalue")
            .expect_err("invalid value")
            .kind(),
        PaginationErrorKind::Controller
    );
}

#[test]
fn canonical_identity_sorts_names_lowercase_and_preserves_repeated_value_order() {
    let first = identity(
        b"Endpoint",
        b"GET",
        b"https://example.test/items?a=1&b=2",
        &[
            HeaderField::new("X-Zeta", b"z"),
            HeaderField::new("x-alpha", b"one"),
            HeaderField::new("X-ALPHA", b"two"),
        ],
    );
    let same = identity(
        b"Endpoint",
        b"GET",
        b"https://example.test/items?a=1&b=2",
        &[
            HeaderField::new("x-alpha", b"one"),
            HeaderField::new("x-alpha", b"two"),
            HeaderField::new("x-zeta", b"z"),
        ],
    );
    assert_eq!(first, same);

    let reversed_values = identity(
        b"Endpoint",
        b"GET",
        b"https://example.test/items?a=1&b=2",
        &[
            HeaderField::new("x-alpha", b"two"),
            HeaderField::new("x-alpha", b"one"),
            HeaderField::new("x-zeta", b"z"),
        ],
    );
    assert_ne!(first, reversed_values);
    assert_ne!(
        first,
        identity(
            b"OtherEndpoint",
            b"GET",
            b"https://example.test/items?a=1&b=2",
            &[
                HeaderField::new("x-alpha", b"one"),
                HeaderField::new("x-alpha", b"two"),
                HeaderField::new("x-zeta", b"z"),
            ],
        )
    );
    assert_ne!(
        first,
        identity(
            b"Endpoint",
            b"POST",
            b"https://example.test/items?a=1&b=2",
            &[
                HeaderField::new("x-alpha", b"one"),
                HeaderField::new("x-alpha", b"two"),
                HeaderField::new("x-zeta", b"z"),
            ],
        )
    );
}

#[test]
fn authentication_plan_state_is_absent_from_canonical_logical_identity() {
    fn protected_head(token: &'static str) -> crate::request::PreparedHead {
        let selector = SlotSelector::static_value(AccessToken::new(token), Duration::ZERO);
        let mut builder = HeadBuilder::new(
            &Url::parse("https://pagination.example.test/v1/").expect("base"),
            RequestMeta::new("PaginationApi::protected", Method::GET, None),
            &RequestOptions::new(),
            DebugLevel::None,
        );
        builder.push_static("items").expect("path");
        builder
            .push_auth(Auth::bearer(CredentialRef::new(Arc::clone(&selector))))
            .expect("authentication plan");
        builder.finish()
    }

    let first = protected_head("authentication-state-sentinel-first-53c1");
    let second = protected_head("authentication-state-sentinel-second-f2a0");
    assert_eq!(first.logical_url(), second.logical_url());
    assert_eq!(first.public_headers(), second.public_headers());
    assert_eq!(
        CanonicalIdentity::from_request_head(
            first.endpoint(),
            first.method(),
            first.logical_url(),
            first.public_headers(),
        )
        .expect("first identity"),
        CanonicalIdentity::from_request_head(
            second.endpoint(),
            second.method(),
            second.logical_url(),
            second.public_headers(),
        )
        .expect("second identity"),
        "credential and authentication state never enter the logical cycle identity"
    );
}

#[test]
fn canonical_url_query_order_encoding_and_non_utf8_header_bytes_participate() {
    let default_port = Url::parse("https://example.test:443/items").expect("default port");
    let implicit_port = Url::parse("https://example.test/items").expect("implicit port");
    assert_eq!(default_port.as_str(), implicit_port.as_str());
    assert_eq!(
        CanonicalIdentity::from_request_head(
            "Endpoint",
            &Method::GET,
            &default_port,
            &HeaderMap::new(),
        )
        .expect("default-port identity"),
        CanonicalIdentity::from_request_head(
            "Endpoint",
            &Method::GET,
            &implicit_port,
            &HeaderMap::new(),
        )
        .expect("implicit-port identity")
    );

    let encoded_path = Url::parse("https://example.test/%7Eitems").expect("encoded path");
    let literal_path = Url::parse("https://example.test/~items").expect("literal path");
    let encoded_identity = CanonicalIdentity::from_request_head(
        "Endpoint",
        &Method::GET,
        &encoded_path,
        &HeaderMap::new(),
    )
    .expect("encoded identity");
    let literal_identity = CanonicalIdentity::from_request_head(
        "Endpoint",
        &Method::GET,
        &literal_path,
        &HeaderMap::new(),
    )
    .expect("literal identity");
    assert_eq!(
        encoded_identity == literal_identity,
        encoded_path.as_str() == literal_path.as_str(),
        "identity follows url's canonical serialization"
    );

    let first_url = Url::parse("https://example.test:443/%7Eitems?a=1&a=2").expect("url");
    let second_url = Url::parse("https://example.test/~items?a=2&a=1").expect("url");
    assert_eq!(first_url.port(), None);
    let mut first_headers = HeaderMap::new();
    first_headers.append(
        "x-binary",
        HeaderValue::from_bytes(&[0xff, 0xfe]).expect("binary header"),
    );
    let first =
        CanonicalIdentity::from_request_head("Endpoint", &Method::GET, &first_url, &first_headers)
            .expect("first");
    let second =
        CanonicalIdentity::from_request_head("Endpoint", &Method::GET, &second_url, &first_headers)
            .expect("second");
    assert_ne!(first, second, "repeated query order participates");

    let ordered = Url::parse("https://example.test/items?a=1&b=2").expect("ordered query");
    let reordered = Url::parse("https://example.test/items?b=2&a=1").expect("reordered query");
    assert_ne!(
        CanonicalIdentity::from_request_head(
            "Endpoint",
            &Method::GET,
            &ordered,
            &HeaderMap::new(),
        )
        .expect("ordered identity"),
        CanonicalIdentity::from_request_head(
            "Endpoint",
            &Method::GET,
            &reordered,
            &HeaderMap::new(),
        )
        .expect("reordered identity"),
        "distinct query order participates"
    );

    let mut other_headers = HeaderMap::new();
    other_headers.append(
        "x-binary",
        HeaderValue::from_bytes(&[0xff, 0xfd]).expect("binary header"),
    );
    let other =
        CanonicalIdentity::from_request_head("Endpoint", &Method::GET, &first_url, &other_headers)
            .expect("other");
    assert_ne!(first, other);

    let mut repeated = HeaderMap::new();
    repeated.append("x-repeat", HeaderValue::from_static("first"));
    repeated.append("x-repeat", HeaderValue::from_static("second"));
    repeated.append("x-distinct", HeaderValue::from_static("value"));
    let mut reordered_names = HeaderMap::new();
    reordered_names.append("x-distinct", HeaderValue::from_static("value"));
    reordered_names.append("x-repeat", HeaderValue::from_static("first"));
    reordered_names.append("x-repeat", HeaderValue::from_static("second"));
    assert_eq!(
        CanonicalIdentity::from_request_head("Endpoint", &Method::GET, &first_url, &repeated)
            .expect("repeated headers"),
        CanonicalIdentity::from_request_head(
            "Endpoint",
            &Method::GET,
            &first_url,
            &reordered_names,
        )
        .expect("reordered distinct names"),
        "distinct header names sort canonically"
    );
    let mut reversed_values = HeaderMap::new();
    reversed_values.append("x-repeat", HeaderValue::from_static("second"));
    reversed_values.append("x-repeat", HeaderValue::from_static("first"));
    reversed_values.append("x-distinct", HeaderValue::from_static("value"));
    assert_ne!(
        CanonicalIdentity::from_request_head("Endpoint", &Method::GET, &first_url, &repeated)
            .expect("repeated headers"),
        CanonicalIdentity::from_request_head(
            "Endpoint",
            &Method::GET,
            &first_url,
            &reversed_values,
        )
        .expect("reversed repeated values"),
        "repeated header value order participates"
    );
}

#[test]
fn identity_accounting_counts_only_canonical_components_and_collisions_use_full_equality() {
    let headers = [
        HeaderField::new("X-One", b"abc"),
        HeaderField::new("x-one", [0xff, 0xfe]),
    ];
    let endpoint = b"Endpoint";
    let method = b"GET";
    let url = b"https://example.test/items";
    let first_identity = identity(endpoint, method, url, &headers);
    assert_eq!(
        first_identity.retained_byte_len(),
        endpoint.len()
            + method.len()
            + url.len()
            + b"x-one".len()
            + b"abc".len()
            + b"x-one".len()
            + 2
    );

    let mut detector = CycleDetector::new(|_| 7);
    let other = identity(b"Other", method, url, &headers);
    detector
        .check_and_retain(first_identity.clone(), usize::MAX)
        .expect("first retain");
    detector
        .check_and_retain(other, usize::MAX)
        .expect("digest collision with distinct bytes");
    assert_eq!(detector.retained_identity_count(), 2);
    assert_eq!(
        detector
            .check_and_retain(first_identity, usize::MAX)
            .expect_err("full equality repeats")
            .kind(),
        PaginationErrorKind::RepeatedRequest
    );
}

#[test]
fn first_identity_above_the_byte_ceiling_is_not_retained() {
    let first_identity = identity(b"Endpoint", b"GET", b"https://example.test/items", &[]);
    let mut detector = CycleDetector::default();
    let limit = first_identity.retained_byte_len() - 1;
    assert_eq!(
        detector
            .check_and_retain(first_identity, limit)
            .expect_err("identity ceiling")
            .kind(),
        PaginationErrorKind::IdentityStorageCeilingExceeded
    );
    assert_eq!(detector.retained_identity_count(), 0);
    assert_eq!(detector.retained_identity_bytes(), 0);

    let identity = identity(b"Endpoint", b"GET", b"https://example.test/items", &[]);
    detector.set_retained_identity_bytes(usize::MAX);
    assert_eq!(
        detector
            .preflight(&identity, usize::MAX)
            .expect_err("identity byte arithmetic overflow")
            .kind(),
        PaginationErrorKind::Overflow
    );
}
