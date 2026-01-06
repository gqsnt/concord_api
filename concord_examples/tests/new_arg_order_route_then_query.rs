// Path: concord_macros/tests/ex12_new_arg_order_route_then_query.rs
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

    path "users" {
        GET GetUser {user_id: u32}
        query { token: String }
        -> TextEncoding<String>;
    }
}

#[test]
fn new_signature_orders_route_required_then_other_required() {
    let vars = client::ClientVars::new();

    // must be (user_id, token)
    let ep = client::endpoints::GetUser::new(7, "TKN");
    let (route, policy) = build_route_and_policy::<client::ClientCx, _>(vars, &ep);

    assert_eq!(route.path().as_str(), "/users/7");
    assert_eq!(
        *policy.query(),
        vec![("token".to_string(), "TKN".to_string())]
    );
}
