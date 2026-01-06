// Path: concord_macros/tests/ex04_client_headers_format_optional_gating.rs
mod common;
use common::*;

use concord_core::prelude::*;
use concord_macros::api;

api! {
    client Client {
        scheme: https,
        host: "example.com",
        params { token?: String; }
        headers {
            "x-static" => "1",
            "authorization" => ["Bearer ", token],
        }
    }

    path "ping" {
        GET Ping "" -> TextEncoding<String>;
    }
}

#[test]
fn client_header_format_is_gated_by_optional_vars() {
    let ep = client::endpoints::Ping::new();

    // token = None => header absent
    let vars = client::ClientVars::new();
    let (_r, p) = build_route_and_policy::<client::ClientCx, _>(vars, &ep);
    assert_eq!(header(&p, "x-static").as_deref(), Some("1"));
    assert!(header(&p, "authorization").is_none());

    // token = Some => header present
    let mut vars = client::ClientVars::new();
    vars.token = Some("ABC".to_string());
    let (_r, p) = build_route_and_policy::<client::ClientCx, _>(vars, &ep);
    assert_eq!(header(&p, "authorization").as_deref(), Some("Bearer ABC"));
}
