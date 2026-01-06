// Path: concord_macros/tests/ex05_host_prefix_order.rs
mod common;
use common::*;

use concord_core::prelude::*;
use concord_macros::api;

api! {
    client Client {
        scheme: https,
        host: "example.com",
        params { }
        headers { test2="header_value"}
    }

    prefix "v1" {
        headers { test4:String="header_value" }
        prefix {tenant: String} {
             headers { test3="header_value"}
            path "ping" {
                GET Ping "" -> TextEncoding<String>;
            }
        }
    }
}

#[test]
fn host_prefix_is_reversed_in_display_semantics() {
    let vars = client::ClientVars::new();
    let ep = client::endpoints::Ping::new("acme");

    let (route, _policy) = build_route_and_policy::<client::ClientCx, _>(vars, &ep);
    let host = route
        .host()
        .join(<client::ClientCx as ClientContext>::DOMAIN);

    assert_eq!(host, "acme.v1.example.com");
}
