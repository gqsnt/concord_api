// Path: concord_macros/tests/ex06_path_join_normalization.rs
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

    path "api"/"v1"/"posts" {
        GET GetPostComments {id: i32}/"comments/" -> TextEncoding<String>;
    }
}

#[test]
fn url_path_push_raw_normalizes_slashes_and_trailing() {
    let vars = client::ClientVars::new();
    let ep = client::endpoints::GetPostComments::new(1);

    let (route, _policy) = build_route_and_policy::<client::ClientCx, _>(vars, &ep);

    assert_eq!(route.path().as_str(), "/api/v1/posts/1/comments");
}
