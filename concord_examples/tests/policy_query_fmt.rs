mod common;
use common::*;

use concord_core::prelude::*;
use concord_macros::api;

#[tokio::test]
async fn query_fmt_set_required_var() {
    api! {
        client ApiQueryFmtReq {
            scheme: https,
            host: "example.com",
        }
        GET One "x"
        query {
            "q" = fmt["a:", {v:String}];
        }
        -> Json<()>;
    }
    use api_query_fmt_req::*;

    let (transport, recorded) = MockTransport::new(vec![MockReply::ok_json(json_bytes(&()))]);
    let api = ApiClient::<Cx>::with_transport(Vars::new(), transport);

    let _ = api.execute(endpoints::One::new("z".to_string())).await.unwrap();

    let reqs = recorded.lock().unwrap();
    assert_eq!(reqs.len(), 1);
    let qp: Vec<(String, String)> = reqs[0]
        .url
        .query_pairs()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    assert!(qp.iter().any(|(k, v)| k == "q" && v == "a:z"));
    assert!(!qp.iter().any(|(k, _)| k == "v"));
}

#[tokio::test]
async fn query_fmt_require_all_optional_removes_key_when_missing() {
    api! {
        client ApiQueryFmtOpt {
            scheme: https,
            host: "example.com",
        }
        GET One "x"
        query {
            "q" = fmt?["a:", {v?:String}];
        }
        -> Json<()>;
    }
    use api_query_fmt_opt::*;

    let (transport, recorded) = MockTransport::new(vec![
        MockReply::ok_json(json_bytes(&())),
        MockReply::ok_json(json_bytes(&())),
    ]);
    let api = ApiClient::<Cx>::with_transport(Vars::new(), transport);

    // v=None => "q" removed
    let _ = api.execute(endpoints::One::new()).await.unwrap();
    // v=Some => "q" present
    let _ = api
        .execute(endpoints::One::new().v("z".to_string()))
        .await
        .unwrap();

    let reqs = recorded.lock().unwrap();
    assert_eq!(reqs.len(), 2);

    // request 0: no "q"
    {
        let qp: Vec<(String, String)> = reqs[0]
            .url
            .query_pairs()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        assert!(!qp.iter().any(|(k, _)| k == "q"));
    }
    // request 1: q=a:z
    {
        let qp: Vec<(String, String)> = reqs[1]
            .url
            .query_pairs()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        assert!(qp.iter().any(|(k, v)| k == "q" && v == "a:z"));
    }
}

#[tokio::test]
async fn query_fmt_push_appends_duplicate_keys() {
    api! {
        client ApiQueryFmtPush {
            scheme: https,
            host: "example.com",
        }
        GET One "x"
        query {
            "dup" += fmt["p:", {v:String}];
            "dup" += "s";
        }
        -> Json<()>;
    }
    use api_query_fmt_push::*;

    let (transport, recorded) = MockTransport::new(vec![MockReply::ok_json(json_bytes(&()))]);
    let api = ApiClient::<Cx>::with_transport(Vars::new(), transport);

    let _ = api.execute(endpoints::One::new("z".to_string())).await.unwrap();

    let reqs = recorded.lock().unwrap();
    assert_eq!(reqs.len(), 1);
    let dups: Vec<String> = reqs[0]
        .url
        .query_pairs()
        .filter(|(k, _)| k == "dup")
        .map(|(_, v)| v.to_string())
        .collect();
    assert_eq!(dups, vec!["p:z".to_string(), "s".to_string()]);
}