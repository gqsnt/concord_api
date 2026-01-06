// Path: concord_macros/tests/ex08_query_keys_optionals_defaults.rs
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

    path "posts" {
        GET GetPosts ""
        query {
            "userId" => user_id?: u32,
            page?: u32 = 1,
            "debug":x_debug: bool = false,
        }
        -> TextEncoding<String>;
    }
}

#[test]
fn query_keys_optionals_and_defaults_behave() {
    let vars = client::ClientVars::new();

    // defaults only
    let ep = client::endpoints::GetPosts::new();
    let (_r, p) = build_route_and_policy::<client::ClientCx, _>(vars.clone(), &ep);
    assert_eq!(
        *p.query(),
        vec![
            ("page".to_string(), "1".to_string()),
            ("debug".to_string(), "false".to_string()),
        ]
    );

    // user_id set + debug override
    let ep = client::endpoints::GetPosts::new().user_id(7).x_debug(true);
    let (_r, p) = build_route_and_policy::<client::ClientCx, _>(vars, &ep);
    assert_eq!(
        *p.query(),
        vec![
            ("userId".to_string(), "7".to_string()),
            ("page".to_string(), "1".to_string()),
            ("debug".to_string(), "true".to_string()),
        ]
    );
}
