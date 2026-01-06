// Path: concord_macros/tests/ex07_route_bracket_form.rs
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

    path ["api", "v1"] {
        GET GetUserPosts ["users", {id: u32}, "posts"] -> TextEncoding<String>;
    }
}

#[test]
fn bracket_route_form_builds_expected_path() {
    let vars = client::ClientVars::new();
    let ep = client::endpoints::GetUserPosts::new(42);

    let (route, _policy) = build_route_and_policy::<client::ClientCx, _>(vars, &ep);
    assert_eq!(route.path().as_str(), "/api/v1/users/42/posts");
}
