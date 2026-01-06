// Path: concord_macros/tests/ex03_client_headers_lit_ref.rs
mod common;
use common::*;

use concord_core::prelude::*;
use concord_macros::api;

api! {
    client Client {
        scheme: https,
        host: "example.com",
        params { api_key: String; }
        headers {
            "x-static" => "v",
            "x-key" => api_key,
        }
    }

    path "ping" {
        GET Ping "" -> TextEncoding<String>;
    }
}

#[test]
fn client_headers_literal_and_ref_work() {
    let vars = client::ClientVars::new("K".to_string());
    let ep = client::endpoints::Ping::new();
    let (_route, policy) = build_route_and_policy::<client::ClientCx, _>(vars, &ep);

    assert_eq!(header(&policy, "x-static").as_deref(), Some("v"));
    assert_eq!(header(&policy, "x-key").as_deref(), Some("K"));
}
