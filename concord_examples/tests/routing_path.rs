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
    let api = ApiClient::<api_path::Cx>::with_transport(api_path::Vars::new(), transport);

    // alias field name: match_id
    let _ = api.execute(endpoints::GetMatch::new("a/b".to_string())).await.unwrap();

    let req = &recorded.lock().unwrap()[0];
    assert_eq!(req.url.path(), "/lol/matches/a%2Fb");
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
    let api = ApiClient::<api_opt_seg::Cx>::with_transport(api_opt_seg::Vars::new(), transport);
    let _ = api.execute(endpoints::One::new()).await.unwrap();
    let req = &recorded.lock().unwrap()[0];
    assert_eq!(req.url.path(), "/x/y");

    // opt=Some("z") => "/x/z/y"
    let (transport, recorded) = MockTransport::new(vec![MockReply::ok_json(json_bytes(&()))]);
    let api = ApiClient::<Cx>::with_transport(Vars::new(), transport);

    let _ = api.execute(endpoints::One::new().opt("z".to_string())).await.unwrap();
    let req = &recorded.lock().unwrap()[0];
    assert_eq!(req.url.path(), "/x/z/y");
}
