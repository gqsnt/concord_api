mod common;
use common::*;

use concord_core::prelude::*;
use concord_macros::api;

#[tokio::test]
async fn path_concat_and_percent_encoding_and_alias() {
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

    let (transport, recorded) = MockTransport::new(vec![MockReply::ok_json(json_bytes(&()))]);
    let api = ApiPath::new_with_transport( transport);

    // alias field name: match_id
    let _ = api.execute(endpoints::GetMatch::new("a/b".to_string())).await.unwrap();

    let req = &recorded.lock().unwrap()[0];
    assert_eq!(req.url.path(), "/lol/matches/a%2Fb");
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

    let (transport, recorded) = MockTransport::new(vec![MockReply::ok_json(json_bytes(&()))]);
    let api = ApiPathFmt::new_with_transport(transport);

    let _ = api.execute(endpoints::One::new("a/b".to_string())).await.unwrap();
    let req = &recorded.lock().unwrap()[0];
    assert_eq!(req.url.path(), "/x/pa%2Fb");
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

    let (transport, recorded) = MockTransport::new(vec![
        MockReply::ok_json(json_bytes(&())),
        MockReply::ok_json(json_bytes(&())),
    ]);
    let api = ApiPathFmtOpt::new_with_transport(transport);

    let _ = api.execute(endpoints::One::new()).await.unwrap();
    let _ = api.execute(endpoints::One::new().v("z".to_string())).await.unwrap();

    let reqs = recorded.lock().unwrap();
    assert_eq!(reqs[0].url.path(), "/x/y");
    assert_eq!(reqs[1].url.path(), "/x/pz/y");
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
    let (transport, recorded) = MockTransport::new(vec![MockReply::ok_json(json_bytes(&()))]);
    let api = ApiOptSeg::new_with_transport( transport);
    let _ = api.execute(endpoints::One::new()).await.unwrap();
    let req = &recorded.lock().unwrap()[0];
    assert_eq!(req.url.path(), "/x/y");

    // opt=Some("z") => "/x/z/y"
    let (transport, recorded) = MockTransport::new(vec![MockReply::ok_json(json_bytes(&()))]);
    let api =ApiOptSeg::new_with_transport(transport);

    let _ = api.execute(endpoints::One::new().opt("z".to_string())).await.unwrap();
    let req = &recorded.lock().unwrap()[0];
    assert_eq!(req.url.path(), "/x/z/y");
}
