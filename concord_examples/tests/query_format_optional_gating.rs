// Path: concord_macros/tests/ex11_query_format_optional_gating.rs
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
            "filter" => ["a=", {a?: u32}, ",b=", {b?: u32}],
        }
        -> TextEncoding<String>;
    }
}

#[test]
fn query_format_is_gated_by_optional_fields() {
    let vars = client::ClientVars::new();

    // none => not emitted
    let ep = client::endpoints::Search::new();
    let (_r, p) = build_route_and_policy::<client::ClientCx, _>(vars.clone(), &ep);
    assert!(p.query().is_empty());

    // partial => not emitted
    let ep = client::endpoints::Search::new().a(1);
    let (_r, p) = build_route_and_policy::<client::ClientCx, _>(vars.clone(), &ep);
    assert!(p.query().is_empty());

    // full => emitted
    let ep = client::endpoints::Search::new().a(1).b(2);
    let (_r, p) = build_route_and_policy::<client::ClientCx, _>(vars, &ep);
    assert_eq!(
        *p.query(),
        vec![("filter".to_string(), "a=1,b=2".to_string())]
    );
}
