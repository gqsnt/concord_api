//! C5.2: `Json.strict()` reads a body only under a JSON content type, in every spelling of one.
#![cfg(feature = "json")]
#![allow(clippy::unwrap_used)]

use concord_core::{
    __private::read_body,
    error::Kind,
    http::{HeaderValue, Response},
    reqwest,
    transport::TryInfo,
    Error,
};
use concord_extras::read::Json;
use proptest::prelude::*;

/// RFC 9110, section 8.3.1: a media type is `type "/" subtype`, each a token, before its
/// parameters, compared without case. JSON is `application/json`, or a subtype with the
/// structured syntax suffix `+json` (RFC 6839).
fn json_model(label: Option<&str>) -> bool {
    let token = |text: &str| {
        !text.is_empty()
            && text
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
    };
    let essence = label
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim_matches([' ', '\t'])
        .to_ascii_lowercase();
    let Some((kind, subtype)) = essence.split_once('/') else {
        return false;
    };
    token(kind) && token(subtype) && (essence == "application/json" || subtype.ends_with("+json"))
}

fn label() -> impl Strategy<Value = Option<String>> {
    let spelled = prop::option::weighted(
        0.9,
        (
            prop::sample::select(vec!["", " ", "\t"]),
            prop::sample::select(vec![
                "application",
                "APPLICATION",
                "Application",
                "text",
                "image",
            ]),
            prop::sample::select(vec![
                "json",
                "JSON",
                "Json",
                "problem+json",
                "vnd.api+JSON",
                "json5",
                "x-json",
                "jsonx",
                "json+xml",
                "ld+json",
                "json ",
                "js on",
            ]),
            prop::sample::select(vec![
                "",
                "; charset=utf-8",
                ";charset=\"utf-8\"",
                " ; q=1",
                ";",
                "; a=b; c=+json",
            ]),
            prop::sample::select(vec!["", " ", "\t"]),
        )
            .prop_map(|(before, kind, sub, params, after)| {
                format!("{before}{kind}/{sub}{params}{after}")
            }),
    );
    // Texts that are no media type: no `/`, or an empty or broken type or subtype.
    let broken = prop::sample::select(vec![
        "bogus+json",
        "+json",
        "json",
        "application",
        "/json",
        "application/",
        "/+json",
        "app lication/json",
        "application/js on+json",
        "application//json",
        "a/b/+json",
        "bogus+json; charset=utf-8",
    ])
    .prop_map(|label| Some(label.to_owned()));
    prop_oneof![3 => spelled, 1 => broken]
}

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn c5_2_a_strict_json_reader_reads_every_spelling_of_a_json_content_type(label in label()) {
        let mut response = Response::new(reqwest::Body::from("[1]"));
        if let Some(label) = &label {
            response.headers_mut().insert("content-type", HeaderValue::from_str(label).unwrap());
        }
        let rt = tokio::runtime::Builder::new_current_thread().enable_time().build().unwrap();
        let read: Result<Vec<u8>, Error> = rt.block_on(read_body(
            &Json.strict(),
            reqwest::Response::from(response),
            &TryInfo::new("props.label", 1),
        ));
        match read {
            Ok(value) => {
                prop_assert!(json_model(label.as_deref()), "{:?}", label);
                prop_assert_eq!(value, vec![1]);
            }
            Err(error) => {
                prop_assert!(!json_model(label.as_deref()), "{:?}", label);
                prop_assert_eq!(error.kind(), Kind::Mismatch);
            }
        }
    }
}

#[test]
fn c5_2_a_content_type_without_a_slash_is_not_json() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    for label in [
        "bogus+json",
        "+json",
        "application/js on+json",
        "application/json",
    ] {
        let mut response = Response::new(reqwest::Body::from("[1]"));
        response
            .headers_mut()
            .insert("content-type", HeaderValue::from_static(label));
        let read: Result<Vec<u8>, Error> = rt.block_on(read_body(
            &Json.strict(),
            reqwest::Response::from(response),
            &TryInfo::new("props.label", 1),
        ));
        assert_eq!(read.is_ok(), label == "application/json", "{label}");
    }
}
