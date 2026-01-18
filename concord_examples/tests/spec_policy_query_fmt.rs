use concord_core::prelude::*;
use concord_macros::api;
use concord_test_support::*;

#[tokio::test(flavor = "current_thread")]
async fn query_fmt_set_required_var() {
    api! {
        client ApiQueryFmtReq {
            scheme: https,
            host: "example.com",
        }

        GET One "x"
        query { "q" = fmt["a:", {v:String}] }
        -> Json<()>;
    }

    use api_query_fmt_req::*;

    let (transport, h) = mock()
        .reply(MockReply::ok_json(json_bytes(&())))
        .build();

    let api = ApiQueryFmtReq::new_with_transport(transport);
    let _ = api
        .request(endpoints::One::new("z".to_string()))
        .execute()
        .await
        .unwrap();

    h.assert_recorded_len(1);
    let reqs = h.recorded();
    assert_request(&reqs[0]).query_has("q", "a:z").query_absent("v");

    h.finish();
}

#[tokio::test(flavor = "current_thread")]
async fn query_fmt_require_all_optional_removes_key_when_missing() {
    api! {
        client ApiQueryFmtOpt {
            scheme: https,
            host: "example.com",
        }

        GET One "x"
        query { "q" = fmt?["a:", {v?:String}] }
        -> Json<()>;
    }

    use api_query_fmt_opt::*;

    let (transport, h) = mock()
        .replies([
            MockReply::ok_json(json_bytes(&())),
            MockReply::ok_json(json_bytes(&())),
        ])
        .build();

    let api = ApiQueryFmtOpt::new_with_transport(transport);

    // v=None => "q" removed
    let _ = api.request(endpoints::One::new()).execute().await.unwrap();
    // v=Some => "q" present
    let _ = api
        .request(endpoints::One::new().v("z".to_string()))
        .execute()
        .await
        .unwrap();

    h.assert_recorded_len(2);
    let reqs = h.recorded();

    assert_request(&reqs[0]).query_absent("q");
    assert_request(&reqs[1]).query_has("q", "a:z");

    h.finish();
}

#[tokio::test(flavor = "current_thread")]
async fn query_fmt_push_appends_duplicate_keys_in_order() {
    api! {
        client ApiQueryFmtPush {
            scheme: https,
            host: "example.com",
        }

        GET One "x"
        query {
            "dup" += fmt["p:", {v:String}],
            "dup" += "s"
        }
        -> Json<()>;
    }

    use api_query_fmt_push::*;

    let (transport, h) = mock()
        .reply(MockReply::ok_json(json_bytes(&())))
        .build();

    let api = ApiQueryFmtPush::new_with_transport(transport);
    let _ = api
        .request(endpoints::One::new("z".to_string()))
        .execute()
        .await
        .unwrap();

    h.assert_recorded_len(1);
    let reqs = h.recorded();

    // Order matters for dup => use query_values (order-preserving)
    assert_request(&reqs[0])
        .query_values("dup", &["p:z", "s"])
        .query_absent("v");

    h.finish();
}
