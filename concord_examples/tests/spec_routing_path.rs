use concord_core::prelude::*;
use concord_macros::api;
use concord_test_support::*;

#[tokio::test]
async fn path_concat_percent_encoding_and_alias() {
    api! {
        client ApiPath {
            scheme: https,
            host: "example.com",
        }

        path "lol" {
            GET GetMatch "matches" / {matchId as match_id:String} -> Json<()>;
        }
    }

    use api_path::*;

    let (transport, h) = mock()
        .reply(MockReply::ok_json(json_bytes(&())))
        .build();

    let api = ApiPath::new_with_transport(transport);

    // alias field name: match_id
    let _ = api
        .request(endpoints::GetMatch::new("a/b".to_string()))
        .execute()
        .await
        .unwrap();

    let reqs = h.recorded();
    assert_request(&reqs[0]).path("/lol/matches/a%2Fb");

    h.finish();
}

#[tokio::test]
async fn path_fmt_builds_single_segment_and_encodes() {
    api! {
        client ApiPathFmt {
            scheme: https,
            host: "example.com",
        }

        GET One "x" / fmt["p", {v:String}] -> Json<()>;
    }

    use api_path_fmt::*;

    let (transport, h) = mock()
        .reply(MockReply::ok_json(json_bytes(&())))
        .build();

    let api = ApiPathFmt::new_with_transport(transport);

    let _ = api
        .request(endpoints::One::new("a/b".to_string()))
        .execute()
        .await
        .unwrap();

    let reqs = h.recorded();
    assert_request(&reqs[0]).path("/x/pa%2Fb");

    h.finish();
}

#[tokio::test]
async fn path_fmt_require_all_optional_omits_segment_when_missing() {
    api! {
        client ApiPathFmtOpt {
            scheme: https,
            host: "example.com",
        }

        GET One "x" / fmt?["p", {v?:String}] / "y" -> Json<()>;
    }

    use api_path_fmt_opt::*;

    let (transport, h) = mock()
        .replies([
            MockReply::ok_json(json_bytes(&())),
            MockReply::ok_json(json_bytes(&())),
        ])
        .build();

    let api = ApiPathFmtOpt::new_with_transport(transport);

    let _ = api.request(endpoints::One::new()).execute().await.unwrap();
    let _ = api
        .request(endpoints::One::new().v("z".to_string()))
        .execute()
        .await
        .unwrap();

    let reqs = h.recorded();
    assert_request(&reqs[0]).path("/x/y");
    assert_request(&reqs[1]).path("/x/pz/y");

    h.finish();
}

#[tokio::test]
async fn optional_path_segment_omitted_no_double_slash() {
    api! {
        client ApiOptSeg {
            scheme: https,
            host: "example.com",
        }

        // endpoint path contains an optional segment
        GET One "x" / {opt?:String} / "y" -> Json<()>;
    }

    use api_opt_seg::*;

    // opt=None => "/x/y"
    {
        let (transport, h) = mock()
            .reply(MockReply::ok_json(json_bytes(&())))
            .build();

        let api = ApiOptSeg::new_with_transport(transport);
        let _ = api.request(endpoints::One::new()).execute().await.unwrap();

        let reqs = h.recorded();
        assert_request(&reqs[0]).path("/x/y");

        h.finish();
    }

    // opt=Some("z") => "/x/z/y"
    {
        let (transport, h) = mock()
            .reply(MockReply::ok_json(json_bytes(&())))
            .build();

        let api = ApiOptSeg::new_with_transport(transport);
        let _ = api
            .request(endpoints::One::new().opt("z".to_string()))
            .execute()
            .await
            .unwrap();

        let reqs = h.recorded();
        assert_request(&reqs[0]).path("/x/z/y");

        h.finish();
    }
}
