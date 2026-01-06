// Path: concord_macros/tests/ex14_map_transform.rs
use concord_core::internal::ResponseSpec;
use concord_core::prelude::*;
use concord_macros::api;

api! {
    client Client {
        scheme: https,
        host: "example.com",
        params { }
        headers { }
    }

    path "len" {
        GET Get "" -> TextEncoding<String> | usize =>  r.len() ;
    }
}

#[test]
fn mapped_response_spec_applies_transform() {
    type E = client::endpoints::Get;

    let out =
        <<E as Endpoint<client::ClientCx>>::Response as ResponseSpec>::map("abcd".to_string())
            .unwrap();
    assert_eq!(out, 4);
}
