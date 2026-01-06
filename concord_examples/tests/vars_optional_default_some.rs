// Path: concord_macros/tests/ex02_vars_optional_default_some.rs
mod common;
use common::*;

use concord_core::prelude::*;
use concord_macros::api;

api! {
    client Client {
        scheme: https,
        host: "example.com",
        params {
            locale?: String = "fr_FR".to_string();
        }
        headers {
            "x-locale" => locale,
        }
    }

    path "ping" {
        GET Ping "" -> TextEncoding<String>;
    }
}

#[test]
fn optional_var_with_default_is_some_and_emits_header() {
    let vars = client::ClientVars::new();
    assert_eq!(vars.locale.as_deref(), Some("fr_FR"));

    let ep = client::endpoints::Ping::new();
    let (_route, policy) = build_route_and_policy::<client::ClientCx, _>(vars, &ep);

    assert_eq!(header(&policy, "x-locale").as_deref(), Some("fr_FR"));
}
