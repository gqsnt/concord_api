#![cfg(feature = "json")]

use concord_core::codec::{DecodeBody, DecodeContext, EncodeBody, EncodeContext};
use concord_core::contracts::{Bytes, Method, StatusCode};
use concord_extras::codec::Json;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
struct Message {
    id: u32,
}

#[test]
fn json_uses_the_public_buffered_codec_contract_and_redacts_parser_text() {
    let encoded = Json::<Message>::encode(
        Message { id: 7 },
        EncodeContext {
            endpoint: "extras.public_contract",
            method: &Method::POST,
            body_limit: Some(1_024),
        },
    )
    .expect("public JSON request encoding");
    let decoded = Json::<Message>::decode(
        encoded,
        DecodeContext {
            endpoint: "extras.public_contract",
            method: &Method::POST,
            status: StatusCode::OK,
            content_type: Some("application/problem+json"),
        },
    )
    .expect("public JSON response decoding");
    assert_eq!(decoded, Message { id: 7 });

    let error = Json::<Message>::decode(
        Bytes::from_static(b"json-parser-sentinel-6729"),
        DecodeContext {
            endpoint: "extras.public_contract",
            method: &Method::POST,
            status: StatusCode::OK,
            content_type: Some("application/json"),
        },
    )
    .expect_err("invalid JSON should fail safely");
    for rendered in [error.to_string(), format!("{error:?}")] {
        assert!(!rendered.contains("json-parser-sentinel-6729"));
    }
}
