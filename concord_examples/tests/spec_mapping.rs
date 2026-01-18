use concord_core::prelude::*;
use concord_macros::api;
use concord_test_support::*;

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct Item {
    id: String,
}

#[tokio::test]
async fn mapping_closure_variant_maps_ids() {
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

    let (transport, h) = mock()
        .reply(MockReply::ok_json(json_bytes(&reply)))
        .build();

    let api = ApiMap::new_with_transport(transport);
    let out: Vec<String> = api.request(endpoints::Ids::new()).execute().await.unwrap();

    assert_eq!(out, vec!["a".to_string(), "b".to_string()]);

    h.assert_recorded_len(1);
    h.finish();
}
