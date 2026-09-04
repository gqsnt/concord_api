use std::error::Error;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use http::{HeaderMap, HeaderValue, Method};
use url::Url;

use super::identity::{CanonicalIdentity, EncodingLimits, HeaderField, IdentityInput};
use super::limits::CycleDetector;
use super::*;
use crate::auth::{AccessToken, Auth, CredentialRef, SlotSelector};
use crate::error::PaginationErrorKind;
use crate::hooks::DebugLevel;
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
    assert_eq!(PageLimit::Prefix(0), PageLimit::Prefix(0));
    assert_eq!(PageLimit::Prefix(7), PageLimit::Prefix(7));
    assert_eq!(
        PageLimit::CompleteWithin(NonZeroU32::new(3).expect("nonzero")),
        PageLimit::CompleteWithin(NonZeroU32::new(3).expect("nonzero"))
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
fn identity_bytes_remain_deterministic_and_match_request_head_encoding() {
    let vector = identity(
        b"E",
        b"GET",
        b"u",
        &[HeaderField::new("X-B", b"2"), HeaderField::new("a", b"1")],
    );
    let expected = [
        &b"CONCORD-PAGE-IDENTITY\x01"[..],
        &b"\x01\0\0\0\0\0\0\0\x01E"[..],
        &b"\x02\0\0\0\0\0\0\0\x03GET"[..],
        &b"\x03\0\0\0\0\0\0\0\x01u"[..],
        &b"\x04\0\0\0\0\0\0\0\x02"[..],
        &b"\x05\0\0\0\0\0\0\0\x01a"[..],
        &b"\x06\0\0\0\0\0\0\0\x011"[..],
        &b"\x05\0\0\0\0\0\0\0\x03x-b"[..],
        &b"\x06\0\0\0\0\0\0\0\x012"[..],
    ]
    .concat();
    assert_eq!(vector.as_bytes(), expected);

    let url = Url::parse("https://example.test/items?a=1").expect("URL");
    let mut headers = HeaderMap::new();
    headers.append("x-zeta", HeaderValue::from_static("z"));
    headers.append("x-alpha", HeaderValue::from_static("one"));
    headers.append("x-alpha", HeaderValue::from_static("two"));
    let legacy = identity(
        b"Endpoint",
        b"GET",
        url.as_str().as_bytes(),
        &[
            HeaderField::new("X-ZETA", b"z"),
            HeaderField::new("X-ALPHA", b"one"),
            HeaderField::new("x-alpha", b"two"),
        ],
    );
    let direct = CanonicalIdentity::from_request_head_with_limit(
        "Endpoint",
        &Method::GET,
        &url,
        &headers,
        legacy.len(),
    )
    .expect("exact remaining identity budget");
    assert_eq!(direct.as_bytes(), legacy.as_bytes());
    assert_eq!(direct.len(), legacy.len());

    assert_eq!(
        CanonicalIdentity::from_request_head_with_limit(
            "Endpoint",
            &Method::GET,
            &url,
            &headers,
            legacy.len() - 1,
        )
        .expect_err("first excess canonical component byte")
        .kind(),
        PaginationErrorKind::IdentityStorageCeilingExceeded
    );
}

#[test]
fn authentication_plan_state_is_absent_from_canonical_logical_identity() {
    fn protected_head(token: &'static str) -> crate::request::PreparedHead {
        let selector = SlotSelector::static_value(
            AccessToken::try_new(token).expect("bounded test access token"),
            Duration::ZERO,
        );
        let mut builder = HeadBuilder::new(
            &Url::parse("https://pagination.example.test/v1/").expect("base"),
            RequestMeta::new("pagination_api.protected", Method::GET, None),
            &RequestOptions::new(),
            DebugLevel::None,
            None,
        );
        builder.push_static("items").expect("path");
        builder
            .push_auth(Auth::bearer(CredentialRef::new(Arc::clone(&selector))))
            .expect("authentication plan");
        builder.finish().expect("bounded protected head")
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
fn identity_limit_counts_per_header_overhead() {
    let endpoint = b"E";
    let method = b"GET";
    let url = b"u";
    let without_headers = identity(endpoint, method, url, &[]);
    let headers = [HeaderField::new("x", b"")];
    let with_header = identity(endpoint, method, url, &headers);
    assert_eq!(
        with_header.len() - without_headers.len(),
        2 * (1 + std::mem::size_of::<u64>()) + 1,
        "one header retains two field tags, two length prefixes, its name, and its value"
    );
    assert_eq!(
        CanonicalIdentity::encode_with_limits(
            IdentityInput {
                endpoint_discriminator: endpoint,
                method_token: method,
                canonical_logical_url: url,
                public_headers: &headers,
            },
            EncodingLimits {
                max_retained_length: with_header.len() - 1,
                ..EncodingLimits::default()
            },
        )
        .expect_err("per-header encoded overhead must be charged"),
        super::identity::IdentityEncodingError::IdentityStorageCeilingExceeded
    );
}

#[test]
fn cycle_detector_counter_equals_sum_of_stored_lengths() {
    let headers = [
        HeaderField::new("X-One", b"abc"),
        HeaderField::new("x-one", [0xff, 0xfe]),
    ];
    let endpoint = b"Endpoint";
    let method = b"GET";
    let url = b"https://example.test/items";
    let first_identity = identity(endpoint, method, url, &headers);
    let second_identity = identity(b"Other", method, url, &headers);
    let expected = first_identity.len() + second_identity.len();
    let mut detector = CycleDetector::default();
    detector
        .check_and_retain(first_identity, expected)
        .expect("first identity");
    detector
        .check_and_retain(second_identity, expected)
        .expect("second identity");

    assert_eq!(detector.retained_identity_bytes(), expected);
    assert_eq!(
        detector.retained_identity_bytes(),
        detector.stored_identity_bytes_sum()
    );
}

#[test]
fn remaining_identity_bytes_uses_encoded_storage() {
    let first = identity(b"E", b"GET", b"u", &[HeaderField::new("x", b"")]);
    let limit = first.len() + 17;
    let mut detector = CycleDetector::default();
    detector
        .check_and_retain(first, limit)
        .expect("first identity");

    assert_eq!(
        detector
            .remaining_identity_bytes(limit)
            .expect("remaining encoded bytes"),
        17
    );
}

#[test]
fn many_empty_or_short_headers_cannot_bypass_byte_limit() {
    let headers = (0..64)
        .map(|_| HeaderField::new("x", b""))
        .collect::<Vec<_>>();
    let input = IdentityInput {
        endpoint_discriminator: b"E",
        method_token: b"GET",
        canonical_logical_url: b"u",
        public_headers: &headers,
    };
    let logical_value_bytes = b"E".len() + b"GET".len() + b"u".len() + headers.len();
    let encoded = CanonicalIdentity::encode(input.clone()).expect("complete identity");
    assert!(encoded.len() > logical_value_bytes);
    assert_eq!(
        CanonicalIdentity::encode_with_limits(
            input,
            EncodingLimits {
                max_retained_length: logical_value_bytes,
                ..EncodingLimits::default()
            },
        )
        .expect_err("structural bytes prevent bypass"),
        super::identity::IdentityEncodingError::IdentityStorageCeilingExceeded
    );
}

#[test]
fn collision_detection_remains_correct() {
    let headers = [
        HeaderField::new("X-One", b"abc"),
        HeaderField::new("x-one", [0xff, 0xfe]),
    ];
    let method = b"GET";
    let url = b"https://example.test/items";
    let first_identity = identity(b"Endpoint", method, url, &headers);

    let repeated = first_identity.clone();
    let allocation = first_identity.as_bytes().as_ptr() as usize;
    let mut detector = CycleDetector::new(|_| 7);
    let other = identity(b"Other", method, url, &headers);
    detector
        .check_and_retain(first_identity, usize::MAX)
        .expect("first retain");
    assert!(
        detector.contains_allocation(allocation),
        "cycle detector must take ownership of the encoded allocation"
    );
    detector
        .check_and_retain(other, usize::MAX)
        .expect("digest collision with distinct bytes");
    assert_eq!(detector.retained_identity_count(), 2);
    assert_eq!(
        detector.retained_identity_bytes(),
        detector.stored_identity_bytes_sum()
    );
    assert_eq!(
        detector
            .check_and_retain(repeated, usize::MAX)
            .expect_err("full equality repeats")
            .kind(),
        PaginationErrorKind::RepeatedRequest
    );
}

#[test]
fn first_identity_above_the_byte_ceiling_is_not_retained() {
    let first_identity = identity(b"Endpoint", b"GET", b"https://example.test/items", &[]);
    let mut detector = CycleDetector::default();
    let limit = first_identity.len() - 1;
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
