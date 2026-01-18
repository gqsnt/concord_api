use concord_core::prelude::*;
use concord_macros::api;
use concord_test_support::*;

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

    let (transport, h) = mock()
        .reply(MockReply::ok_json(json_bytes(&())))
        .build();

    let api = ApiQuery::new_with_transport(transport);
    let _ = api.request(endpoints::One::new()).execute().await.unwrap();

    let reqs = h.recorded();
    let r = &reqs[0];

    assert_request(r)
        .query_absent("sdk")
        .query_values("dup", &["e1"])
        .query_has("n", "12")
        .query_has("b", "false");

    h.finish();
}
