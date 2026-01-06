// Path: concord_macros/tests/ex13_string_into_new_and_setter.rs
mod common;
use common::*;

use concord_core::prelude::*;
use concord_macros::api;

api! {
    client Client {
        scheme: https,
        host: "example.com",
        params { }
        headers { }
    }

    path "hello" {
        GET Hello {name as fname : String}
        query { lang?: String }
        -> TextEncoding<String>;
    }
}

#[test]
fn required_string_uses_into_in_new_and_optional_string_setter_uses_into() {
    let vars = client::ClientVars::new();

    let ep = client::endpoints::Hello::new("bob").lang("fr_FR");
    let (route, policy) = build_route_and_policy::<client::ClientCx, _>(vars, &ep);

    assert_eq!(route.path().as_str(), "/hello/bob");
    assert_eq!(
        *policy.query(),
        vec![("lang".to_string(), "fr_FR".to_string())]
    );
}
