//! Public custom-format head checks work without enabling any codec feature.

use concord_codecs::{Charset, check_content_type};
use concord_core::ResponseHead;
use concord_core::reqwest::{
    StatusCode, Version,
    header::{CONTENT_TYPE, HeaderMap, HeaderValue},
};

fn verdict(fields: &[&[u8]], accepted: &[&str], charset: Charset) -> Result<(), &'static str> {
    let mut headers = HeaderMap::new();
    for field in fields {
        headers.append(CONTENT_TYPE, HeaderValue::from_bytes(field).unwrap());
    }
    let head = ResponseHead::new(StatusCode::OK, Version::HTTP_11, &headers);
    check_content_type(&head, accepted, charset).map_err(|e| e.label())
}

#[test]
fn exact_types_and_rfc_parameters() {
    for field in [
        &b"Text/XML"[..],
        b"application/xml; charset=US-ASCII",
        b"text/xml ; charset=\"utf-8\"; profile=x",
        b"text/xml; ; CHARSET=\"UTF-8\";;",
        b"text/xml; charset=\"utf\\-8\"; profile=\"a;b\"",
    ] {
        assert_eq!(
            verdict(&[field], &["text/xml", "APPLICATION/XML"], Charset::Utf8),
            Ok(())
        );
    }
}

#[test]
fn exact_and_suffix_rules_do_not_confuse_representations() {
    for field in ["application/problem+json", "APPLICATION/VND.ACME+JSON"] {
        assert_eq!(
            verdict(&[field.as_bytes()], &["application/*+json"], Charset::Utf8),
            Ok(())
        );
    }
    for (field, accepted) in [
        ("application/json+xml", "application/json"),
        ("application/json+xml", "application/*+json"),
        ("text/problem+json", "application/*+json"),
        ("application/json", "application/*+json"),
        ("application/xml", "application/*"),
        ("application/xml", "*/*"),
        ("application/xml", "application/xml; charset=utf-8"),
    ] {
        assert_eq!(
            verdict(&[field.as_bytes()], &[accepted], Charset::Utf8),
            Err("the response Content-Type is not an accepted media type")
        );
    }
    assert_eq!(
        verdict(&[b"application/xml"], &[], Charset::Utf8),
        Err("the response Content-Type is not an accepted media type")
    );
}

#[test]
fn strict_field_failures_keep_standard_labels() {
    let cases: &[(&[&[u8]], &str)] = &[
        (&[], "the response has no Content-Type"),
        (
            &[b"text/xml", b"text/xml"],
            "the response has more than one Content-Type field",
        ),
        (
            &[b"text/xml, text/xml"],
            "the response Content-Type is not a media type",
        ),
        (&[b"text/"], "the response Content-Type is not a media type"),
        (
            &[b"text/xml; charset = utf-8"],
            "the response Content-Type is not a media type",
        ),
        (
            &[b"text/xml; charset=\"utf-8"],
            "the response Content-Type is not a media type",
        ),
        (
            &[b"text/xml; x=\xff"],
            "the response Content-Type is not visible ASCII",
        ),
        (
            &[b"text/*"],
            "the response Content-Type is a media range, not a media type",
        ),
        (
            &[b"application/*+json"],
            "the response Content-Type is a media range, not a media type",
        ),
        (
            &[b"*/*"],
            "the response Content-Type is a media range, not a media type",
        ),
    ];
    for (fields, label) in cases {
        assert_eq!(
            verdict(fields, &["text/xml", "application/*+json"], Charset::Utf8),
            Err(*label)
        );
    }
}

#[test]
fn charset_policy_is_explicit() {
    for field in [
        &b"text/xml; charset=utf-8; charset=utf-8"[..],
        b"text/xml; charset=latin1; charset=utf-8",
    ] {
        assert_eq!(
            verdict(&[field], &["text/xml"], Charset::Utf8),
            Err("the response Content-Type declares charset more than once")
        );
        assert_eq!(verdict(&[field], &["text/xml"], Charset::Ignore), Ok(()));
    }
    assert_eq!(
        verdict(&[b"text/xml; charset=latin1"], &["text/xml"], Charset::Utf8),
        Err("the response declares a charset other than utf-8")
    );
    assert_eq!(
        verdict(
            &[b"text/xml; charset=latin1"],
            &["text/xml"],
            Charset::Ignore
        ),
        Ok(())
    );
    assert_eq!(
        verdict(&[b"text/xml; charset="], &["text/xml"], Charset::Ignore),
        Err("the response Content-Type is not a media type")
    );
    assert_eq!(
        verdict(&[], &["text/xml"], Charset::Ignore),
        Err("the response has no Content-Type")
    );
}
