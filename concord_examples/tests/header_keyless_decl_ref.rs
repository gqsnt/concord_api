// Path: concord_macros/tests/ex09_header_keyless_decl_ref.rs
mod common;
use common::*;

use concord_core::prelude::*;
use concord_macros::api;

api! {
    client Client {
        scheme: https,
        host: "example.com",
        params { user_agent: String; }
        headers { }
    }

    path "ping" {
        GET Ping ""
        headers {
            user_agent,
            "x-debug":x_debug?: bool = true,
            {x_trace?: bool},
        }
        -> TextEncoding<String>;
    }
}

#[test]
fn keyless_headers_decl_and_ref() {
    let vars = client::ClientVars::new("UA/2.0".to_string());
    let ep = client::endpoints::Ping::new();

    let (_r, p) = build_route_and_policy::<client::ClientCx, _>(vars, &ep);

    assert_eq!(header(&p, "user-agent").as_deref(), Some("UA/2.0"));
    assert_eq!(header(&p, "x-debug").as_deref(), Some("true"));
    assert!(header(&p, "x-trace").is_none());
}
