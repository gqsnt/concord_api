mod common;
use common::*;

use concord_core::prelude::*;
use concord_macros::api;

#[tokio::test]
async fn query_set_remove_push_override_and_tostring() {
    api! {
      client ApiQuery {
        scheme: https,
        host: "example.com",
        query {
          "sdk" = "concord",
          "dup" += "c0"
        }
      }

      path "x" {
        query {
          -"sdk",            // remove client sdk
          "dup" += "p1",     // keep dup from client + add
          "n" = 12u64,       // ToString
          "b" = false       // ToString
        }

        // endpoint overrides dup key (set => remove all dup then add)
        GET One "" query { "dup" = "e1" } -> Json<()>;
      }
    }
    use api_query::*;

    let (transport, recorded) = MockTransport::new(vec![MockReply::ok_json(json_bytes(&()))]);
    let api = ApiClient::<api_query::Cx>::with_transport(api_query::Vars::new(), transport);

    let _ = api.execute(endpoints::One::new()).await.unwrap();
    let req = &recorded.lock().unwrap()[0];

    let qp: Vec<(String, String)> = req
        .url
        .query_pairs()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    assert!(!qp.iter().any(|(k, _)| k == "sdk"));

    // dup overridden by endpoint set
    let dups: Vec<String> = qp.iter().filter(|(k, _)| k == "dup").map(|(_, v)| v.clone()).collect();
    assert_eq!(dups, vec!["e1".to_string()]);

    assert!(qp.iter().any(|(k, v)| k == "n" && v == "12"));
    assert!(qp.iter().any(|(k, v)| k == "b" && v == "false"));
}
