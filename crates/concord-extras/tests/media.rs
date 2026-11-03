//! The response head checks of the standard formats, on heads built by hand (no network): exact
//! accepted and refused `Content-Type` forms (C04, C05, C11), and the adapter that skips only
//! that check.

use concord_core::bytes::Bytes;
use concord_core::reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderValue};
use concord_core::reqwest::{StatusCode, Version};
use concord_core::{Decode, ResponseHead};
use concord_extras::{Binary, IgnoreContentType, Text};

fn headers(content_types: &[&[u8]]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for ct in content_types {
        map.append(CONTENT_TYPE, HeaderValue::from_bytes(ct).unwrap());
    }
    map
}

/// The format's verdict on a 200 with these `Content-Type` field lines: `Ok` or the static label.
fn verdict<F: Decode<T>, T>(content_types: &[&[u8]]) -> Result<(), &'static str> {
    let map = headers(content_types);
    let head = ResponseHead::new(StatusCode::OK, Version::HTTP_11, &map);
    F::check_content_type(&head)
        .and_then(|()| F::context(&head).map(drop))
        .map_err(|e| e.label())
}

#[cfg(feature = "json")]
mod json {
    use super::*;
    use concord_extras::Json;

    type V = serde_json_value::Value;
    mod serde_json_value {
        /// Any JSON document.
        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        #[allow(dead_code)]
        pub enum Value {
            Null,
            Bool(bool),
            Number(f64),
            Text(String),
            List(Vec<Value>),
            Object(std::collections::BTreeMap<String, Value>),
        }
    }

    #[test]
    fn accepted_media_types() {
        for ct in [
            &b"application/json"[..],
            b"application/json; charset=utf-8",
            b"application/json;charset=UTF-8",
            b"APPLICATION/JSON",
            b"Application/Json; Charset=\"utf-8\"",
            b"application/problem+json",
            b"application/vnd.github+json",
            b"application/vnd.acme.v2+json; profile=x",
            b"application/json; charset=us-ascii",
        ] {
            assert_eq!(
                verdict::<Json, V>(&[ct]),
                Ok(()),
                "{}",
                String::from_utf8_lossy(ct)
            );
        }
    }

    #[test]
    fn refused_media_types_with_their_labels() {
        let not_json =
            "the response Content-Type is not application/json or an application/*+json type";
        let cases: [(&[&[u8]], &str); 14] = [
            (&[], "the response has no Content-Type"),
            (&[b"text/html"], not_json),
            (&[b"text/json"], not_json),
            (&[b"text/plain; charset=utf-8"], not_json),
            (&[b"application/json-seq"], not_json),
            (&[b"application/x-ndjson"], not_json),
            (&[b"application/json+xml"], not_json),
            (
                &[b"application/*"],
                "the response Content-Type is a media range, not a media type",
            ),
            (
                &[b"*/*"],
                "the response Content-Type is a media range, not a media type",
            ),
            (
                &[b"application/json; charset=iso-8859-1"],
                "the JSON response declares a charset other than utf-8",
            ),
            (
                &[b"application/json, text/html"],
                "the response Content-Type is not a media type",
            ),
            (&[b"json"], "the response Content-Type is not a media type"),
            (
                &[b"application/json", b"application/json"],
                "the response has more than one Content-Type field",
            ),
            (
                &[b"application/json", b"text/html"],
                "the response has more than one Content-Type field",
            ),
        ];
        for (cts, label) in cases {
            assert_eq!(verdict::<Json, V>(cts), Err(label), "{cts:?}");
        }
        // A value that is not visible ASCII (obs-text): refused, not guessed.
        assert_eq!(
            verdict::<Json, V>(&[b"application/json; x=\xe9"]),
            Err("the response Content-Type is not visible ASCII")
        );
    }

    /// The adapter skips exactly the `Content-Type` check; the body is still `Json`'s.
    #[test]
    fn ignore_content_type_skips_only_the_media_check() {
        assert_eq!(
            verdict::<IgnoreContentType<Json>, V>(&[b"text/html"]),
            Ok(())
        );
        assert_eq!(verdict::<IgnoreContentType<Json>, V>(&[]), Ok(()));
        assert_eq!(
            <IgnoreContentType<Json> as Decode<V>>::ACCEPT,
            Some("application/json")
        );
        assert!(
            <IgnoreContentType<Json> as Decode<V>>::decode_body((), Bytes::from_static(b"<html>"))
                .is_err()
        );
        assert!(
            <IgnoreContentType<Json> as Decode<V>>::decode_body((), Bytes::from_static(b"[1]"))
                .is_ok()
        );
    }

    /// Empty and whitespace bodies are not JSON, for every target type; `null` is.
    #[test]
    fn empty_bodies_are_not_null() {
        for body in [&b""[..], b" \r\n\t"] {
            let e =
                <Json as Decode<()>>::decode_body((), Bytes::copy_from_slice(body)).unwrap_err();
            assert!(
                e.label()
                    .starts_with("the body is empty or only whitespace"),
                "{e}"
            );
            assert!(
                <Json as Decode<Option<u8>>>::decode_body((), Bytes::copy_from_slice(body))
                    .is_err()
            );
        }
        <Json as Decode<()>>::decode_body((), Bytes::from_static(b"null")).unwrap();
        assert_eq!(
            <Json as Decode<Option<u8>>>::decode_body((), Bytes::from_static(b"null")).unwrap(),
            None
        );
    }

    /// A parser error that quotes the input is attached, never shown (E02).
    #[test]
    fn decode_errors_show_positions_not_content() {
        let e = <Json as Decode<u8>>::decode_body((), Bytes::from_static(b"\"token=S3CR3T\""))
            .unwrap_err();
        assert_eq!(e.label(), "the JSON does not match the expected type");
        assert_eq!((e.get("line"), e.get("column")), (Some(1), Some(14)));
        let shown = format!("{e} {e:?}");
        assert!(!shown.contains("S3CR3T"), "{shown}");
        assert!(
            e.source_unredacted()
                .unwrap()
                .to_string()
                .contains("S3CR3T")
        );
        assert!(std::error::Error::source(&e).is_none());
    }
}

#[cfg(feature = "form")]
mod form {
    use super::*;
    use concord_extras::Form;
    use std::collections::BTreeMap;

    type V = BTreeMap<String, String>;

    #[test]
    fn form_response_head() {
        assert_eq!(
            verdict::<Form, V>(&[b"application/x-www-form-urlencoded"]),
            Ok(())
        );
        assert_eq!(
            verdict::<Form, V>(&[b"Application/X-WWW-Form-Urlencoded; charset=utf-8"]),
            Ok(())
        );
        assert_eq!(
            verdict::<Form, V>(&[]),
            Err("the response has no Content-Type")
        );
        assert_eq!(
            verdict::<Form, V>(&[b"application/json"]),
            Err("the response Content-Type is not application/x-www-form-urlencoded")
        );
        assert_eq!(
            verdict::<Form, V>(&[b"application/x-www-form-urlencoded; charset=latin1"]),
            Err("the form response declares a charset other than utf-8")
        );
        assert_eq!(
            verdict::<IgnoreContentType<Form>, V>(&[b"text/plain"]),
            Ok(())
        );
    }
}

/// `Text` names a decoder, not `text/plain`: any media type, none, or a malformed one; only a
/// declared non-UTF-8 charset is refused.
#[test]
fn text_accepts_any_media_type_but_a_foreign_charset() {
    for cts in [
        &[][..],
        &[&b"text/plain"[..]],
        &[b"text/csv; charset=utf-8"],
        &[b"application/xml"],
        &[b"text/plain; charset=US-ASCII"],
        &[b"not a media type"],
        &[b"text/plain", b"text/html"],
    ] {
        assert_eq!(verdict::<Text, String>(cts), Ok(()), "{cts:?}");
    }
    assert_eq!(
        verdict::<Text, String>(&[b"text/plain; charset=iso-8859-1"]),
        Err("the text response declares a charset other than utf-8")
    );
    assert_eq!(<Text as Decode<String>>::ACCEPT, None);
    let e = <Text as Decode<String>>::decode_body((), Bytes::from_static(b"ok\xff")).unwrap_err();
    assert_eq!(
        (e.label(), e.get("byte")),
        ("the body is not valid UTF-8", Some(2))
    );
}

/// `Binary` checks nothing: an image is bytes, whatever its label (C11).
#[test]
fn binary_checks_nothing() {
    for cts in [
        &[][..],
        &[&b"image/png"[..]],
        &[b"application/octet-stream"],
        &[b"text/html"],
        &[b"a", b"b"],
    ] {
        assert_eq!(verdict::<Binary, Bytes>(cts), Ok(()), "{cts:?}");
        assert_eq!(verdict::<Binary, Vec<u8>>(cts), Ok(()), "{cts:?}");
    }
    assert_eq!(<Binary as Decode<Bytes>>::ACCEPT, None);
    let png = Bytes::from_static(b"\x89PNG\r\n\x1a\n");
    assert_eq!(
        <Binary as Decode<Bytes>>::decode_body((), png.clone()).unwrap(),
        png
    );
}

/// `ResponseHead`'s `Debug` names the headers, never their values.
#[test]
fn response_head_debug_withholds_values() {
    let mut map = HeaderMap::new();
    map.insert("set-cookie", HeaderValue::from_static("session=S3CR3T"));
    let shown = format!(
        "{:?}",
        ResponseHead::new(StatusCode::OK, Version::HTTP_11, &map)
    );
    assert!(
        shown.contains("set-cookie") && !shown.contains("S3CR3T"),
        "{shown}"
    );
}

/// A format narrowed to one exact vendor media type: `Json`'s own head checks first (one field, a
/// `+json` type, UTF-8), then the exact essence, so `application/problem+json` is refused too.
/// Parameters (`; profile=...`) are allowed; the body is decoded by `Json`.
#[cfg(feature = "json")]
mod vendor {
    use super::*;
    use concord_core::{CodecError, ContentTypeField};
    use concord_extras::Json;

    /// `application/vnd.acme.v2+json` and nothing else.
    pub struct AcmeV2;

    const ACME_V2: &str = "application/vnd.acme.v2+json";

    impl<T: serde::de::DeserializeOwned> Decode<T> for AcmeV2 {
        type Context = ();
        const ACCEPT: Option<&'static str> = Some(ACME_V2);

        fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
            <Json as Decode<T>>::check_content_type(head)?;
            let ContentTypeField::One(value) = head.content_type() else {
                return Err(CodecError::new("the response has no single Content-Type"));
            };
            let media: mime::Mime = value
                .to_str()
                .ok()
                .and_then(|text| text.parse().ok())
                .ok_or(CodecError::new(
                    "the response Content-Type is not a media type",
                ))?;
            if media.essence_str() != ACME_V2 {
                return Err(CodecError::new(
                    "the response Content-Type is not application/vnd.acme.v2+json",
                ));
            }
            Ok(())
        }

        fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> {
            Ok(())
        }

        fn decode_body((): (), body: Bytes) -> Result<T, CodecError> {
            <Json as Decode<T>>::decode_body((), body)
        }
    }

    #[test]
    fn an_exact_vendor_media_type() {
        type V = Vec<u8>;
        assert_eq!(
            verdict::<AcmeV2, V>(&[b"application/vnd.acme.v2+json"]),
            Ok(())
        );
        assert_eq!(
            verdict::<AcmeV2, V>(&[b"Application/Vnd.Acme.V2+JSON; profile=full"]),
            Ok(())
        );
        let other = Err("the response Content-Type is not application/vnd.acme.v2+json");
        assert_eq!(verdict::<AcmeV2, V>(&[b"application/json"]), other);
        assert_eq!(
            verdict::<AcmeV2, V>(&[b"application/vnd.acme.v3+json"]),
            other
        );
        assert_eq!(verdict::<AcmeV2, V>(&[b"application/problem+json"]), other);
        // `Json`'s own refusals come first.
        assert_eq!(
            verdict::<AcmeV2, V>(&[b"text/html"]),
            Err("the response Content-Type is not application/json or an application/*+json type")
        );
    }
}

/// Review F1/F2 (final executable audit): `charset` parameters are read correctly whatever their
/// position and quoting (the pinned `mime` 0.3.17 misreads a quoted value followed by another
/// parameter), and a repeated `charset` is refused as ambiguous, by every format that checks it.
#[cfg(all(feature = "json", feature = "form"))]
mod charset {
    use super::*;
    use concord_extras::{Form, Json};
    use std::collections::BTreeMap;

    #[test]
    fn quoted_utf8_before_another_parameter_is_accepted() {
        for ct in [
            &b"application/json; charset=\"utf-8\"; profile=x"[..],
            b"application/json; profile=x; charset=\"utf-8\"",
            b"application/json; charset=\"UTF-8\" ; profile=\"a;b\"",
            b"application/json; charset=\"utf\\-8\"",
        ] {
            assert_eq!(
                verdict::<Json, u8>(&[ct]),
                Ok(()),
                "{}",
                String::from_utf8_lossy(ct)
            );
        }
        assert_eq!(
            verdict::<Text, String>(&[b"text/plain; charset=\"utf-8\"; format=flowed"]),
            Ok(())
        );
        assert_eq!(
            verdict::<Form, BTreeMap<String, String>>(&[
                b"application/x-www-form-urlencoded; charset=\"utf-8\"; x=y"
            ]),
            Ok(())
        );
    }

    #[test]
    fn a_repeated_charset_is_refused() {
        let repeated = Err("the response Content-Type declares charset more than once");
        for ct in [
            &b"application/json; charset=utf-8; charset=iso-8859-1"[..],
            b"application/json; charset=iso-8859-1; charset=utf-8",
            b"application/json; charset=utf-8; charset=utf-8",
        ] {
            assert_eq!(
                verdict::<Json, u8>(&[ct]),
                repeated,
                "{}",
                String::from_utf8_lossy(ct)
            );
        }
        assert_eq!(
            verdict::<Text, String>(&[b"text/plain; charset=utf-8; charset=iso-8859-1"]),
            repeated
        );
        assert_eq!(
            verdict::<Form, BTreeMap<String, String>>(&[
                b"application/x-www-form-urlencoded; charset=utf-8; charset=latin1"
            ]),
            repeated
        );
        // A foreign charset after another parameter is still seen.
        assert_eq!(
            verdict::<Json, u8>(&[b"application/json; profile=x; charset=\"iso-8859-1\""]),
            Err("the JSON response declares a charset other than utf-8")
        );
    }
}
