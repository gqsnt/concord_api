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

  prefix "v1"{
    headers {
      "x-a" => "1",
      "x-x" => "outer",
    }
    query {
      "p" => "1",
      "q" => "outer",
    }

    path "users" {
      headers {
        "x-b" => "2",
        "x-x" => "inner",
      }
      query {
        "q" => "inner",
        "r" => "2",
      }
      GET GetUser {id: u32}
      headers {
        -"x-a",
        "x-c" => "3",
      }
      query {
        -p,
        "r" => "3",
      }
      -> TextEncoding<String>;
    }
  }
}

#[test]
fn prefix_and_path_policy_apply_before_endpoint_and_can_be_undone() {
    let vars = client::ClientVars::new();
    let ep = client::endpoints::GetUser::new(7);

    let (_route, policy) = build_route_and_policy::<client::ClientCx, _>(vars, &ep);

    // headers: outer->inner, endpoint last
    assert!(header(&policy, "x-a").is_none()); // removed by endpoint
    assert_eq!(header(&policy, "x-b").as_deref(), Some("2"));
    assert_eq!(header(&policy, "x-c").as_deref(), Some("3"));
    assert_eq!(header(&policy, "x-x").as_deref(), Some("inner")); // inner overrides outer

    // query: prefix defaults overridden/removed by inner/endpoint
    assert_eq!(
        *policy.query(),
        vec![
            ("q".to_string(), "inner".to_string()),
            ("r".to_string(), "3".to_string()),
        ]
    );
}
