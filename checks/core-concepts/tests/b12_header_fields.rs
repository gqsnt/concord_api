//! Between waves 1 and 2, candidate C: strict single-field reading on the response head. SDKs
//! parsed rate-limit and pagination counters with `str::parse` (which accepts `+5`) and
//! conflated "absent" with "unparsable" (SDK22 maintainer; GitHub/GitLab metadata).

use concord::reqwest::header::{HeaderMap, HeaderValue};
use concord::reqwest::{StatusCode, Version};
use concord::{FieldError, ResponseHead};

fn head(pairs: &[(&'static str, &'static [u8])]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (name, value) in pairs {
        map.append(*name, HeaderValue::from_bytes(value).unwrap());
    }
    map
}

#[test]
fn unsigned_distinguishes_every_outcome() {
    let cases: &[(&[(&str, &[u8])], Result<Option<u64>, FieldError>)] = &[
        (&[], Ok(None)),
        (&[("x-remaining", b"42")], Ok(Some(42))),
        (&[("x-remaining", b" 7\t")], Ok(Some(7))),
        (&[("x-remaining", b"0")], Ok(Some(0))),
        (&[("x-remaining", b"+5")], Err(FieldError::Invalid)),
        (&[("x-remaining", b"-1")], Err(FieldError::Invalid)),
        (&[("x-remaining", b"1.5")], Err(FieldError::Invalid)),
        (&[("x-remaining", b"")], Err(FieldError::Invalid)),
        (&[("x-remaining", b"0x10")], Err(FieldError::Invalid)),
        (&[("x-remaining", b"\xff")], Err(FieldError::Invalid)),
        (&[("x-remaining", b"18446744073709551616")], Err(FieldError::Overflow)),
        (&[("x-remaining", b"1"), ("x-remaining", b"1")], Err(FieldError::Repeated)),
    ];
    for (pairs, expected) in cases {
        let map = head(pairs);
        let h = ResponseHead::new(StatusCode::OK, Version::HTTP_11, &map);
        assert_eq!(&h.unsigned("x-remaining"), expected, "{pairs:?}");
    }
}

#[test]
fn single_reports_repetition_and_errors_never_quote_the_value() {
    let map = head(&[("x-request-id", b"a"), ("x-request-id", b"b")]);
    let h = ResponseHead::new(StatusCode::OK, Version::HTTP_11, &map);
    assert_eq!(h.single("x-request-id"), Err(FieldError::Repeated));
    let map = head(&[("x-limit", b"secret-token")]);
    let h = ResponseHead::new(StatusCode::OK, Version::HTTP_11, &map);
    let error = h.unsigned("x-limit").unwrap_err();
    assert!(!format!("{error} {error:?}").contains("secret"));
}
