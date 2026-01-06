// Path: concord_macros/tests/ex10_header_remove_override.rs
mod common;
use common::*;

use concord_core::prelude::*;
use concord_macros::api;

api! {
    client Client {
        scheme: https,
        host: "example.com",
        params { }
        headers {
            "x-a" => "1",
            "x-b" => "2",
        }
    }

    path "ping" {
        GET Ping ""
        headers {
            -"x-a",
            "x-b" => "3",
        }
        -> TextEncoding<String>;
    }
}

#[test]
fn endpoint_can_remove_and_override_headers() {
    let vars = client::ClientVars::new();
    let ep = client::endpoints::Ping::new();

    let (_r, p) = build_route_and_policy::<client::ClientCx, _>(vars, &ep);

    assert!(header(&p, "x-a").is_none());
    assert_eq!(header(&p, "x-b").as_deref(), Some("3"));
}
