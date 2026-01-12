mod common;
use common::*;

use concord_core::prelude::*;
use concord_macros::api;

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct Item { id: String }

#[tokio::test]
async fn map_closure_variant() {
    api! {
      client ApiMap {
        scheme: https,
        host: "example.com",
      }

      GET Ids "ids"
      -> Json<Vec<Item>> | Vec<String>  => {
            r.into_iter().map(|x| x.id).collect::<Vec<_>>()
        };
    }
    use api_map::*;

    let reply = vec![Item { id: "a".into() }, Item { id: "b".into() }];
    let (transport, _recorded) = MockTransport::new(vec![MockReply::ok_json(json_bytes(&reply))]);
    let api = ApiMap::new_with_transport(transport);
    let out: Vec<String> = api.execute(endpoints::Ids::new()).await.unwrap();
    assert_eq!(out, vec!["a".to_string(), "b".to_string()]);
}
