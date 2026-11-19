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

/// The media type before its parameters, compared without case: `application/json`, or a
/// type with the structured syntax suffix `+json`.
fn json_model(label: Option<&str>) -> bool {
    let essence = label
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim_matches([' ', '\t'])
        .to_ascii_lowercase();
    essence == "application/json" || essence.ends_with("+json")
}

fn label() -> impl Strategy<Value = Option<String>> {
    prop::option::weighted(
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
    )
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
