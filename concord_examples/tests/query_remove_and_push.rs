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
    path "search" {
        GET Search ""
        query {
            "a" => "1",
            -"a",
            "b" => "2",
            "b" => "3",
            "c" += "1",
            "c" += "2",
        }
        -> TextEncoding<String>;
    }
}

#[test]
fn query_remove_set_and_push_semantics() {
    let vars = client::ClientVars::new();
    let ep = client::endpoints::Search::new();
    let (_r, p) = build_route_and_policy::<client::ClientCx, _>(vars, &ep);

    assert_eq!(
        *p.query(),
        vec![
            ("b".to_string(), "3".to_string()),
            ("c".to_string(), "1".to_string()),
            ("c".to_string(), "2".to_string()),
        ]
    );
}
