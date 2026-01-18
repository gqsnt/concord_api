// concord_examples/tests/routing_layers_fmt.rs

mod common;

use common::*;
use concord_core::prelude::*;
use concord_macros::api;

#[tokio::test]
async fn prefix_layer_fmt_adds_one_host_label() {
    api! {
        client ApiPrefixLayerFmt {
            scheme: https,
            host: "example.com",
        }

        // Layer prefix: "api".fmt["t", {id:String}]
        // Expected host: api.t42.example.com (DSL order preserved by reverse push logic)
        prefix "api".fmt["t", {id:String}] {
            GET One "x" -> Json<()>;
        }
    }

    use api_prefix_layer_fmt::*;

    let (transport, recorded) = MockTransport::new(vec![MockReply::ok_json(json_bytes(&()))]);
    let api = ApiPrefixLayerFmt::new_with_transport(transport);

    let _ = api
        .request(endpoints::One::new("42".to_string()))
        .execute()
        .await
        .unwrap();

    let req = &recorded.lock().unwrap()[0];
    assert_eq!(req.url.host_str().unwrap(), "api.t42.example.com");
    assert_eq!(req.url.path(), "/x");
}

#[tokio::test]
async fn prefix_layer_fmt_require_all_omits_label_when_missing() {
    api! {
        client ApiPrefixLayerFmtOpt {
            scheme: https,
            host: "example.com",
        }

        // fmt? with optional var: omit whole label if missing
        prefix "api".fmt?["t", {id?:String}] {
            GET One "x" -> Json<()>;
        }
    }

    use api_prefix_layer_fmt_opt::*;

    let (transport, recorded) = MockTransport::new(vec![
        MockReply::ok_json(json_bytes(&())),
        MockReply::ok_json(json_bytes(&())),
    ]);
    let api = ApiPrefixLayerFmtOpt::new_with_transport(transport);

    // id=None => host is api.example.com
    let _ = api.request(endpoints::One::new()).execute().await.unwrap();

    // id=Some => host is api.tz.example.com
    let _ = api
        .request(endpoints::One::new().id("z".to_string()))
        .execute()
        .await
        .unwrap();

    let reqs = recorded.lock().unwrap();
    assert_eq!(reqs[0].url.host_str().unwrap(), "api.example.com");
    assert_eq!(reqs[1].url.host_str().unwrap(), "api.tz.example.com");
}

#[tokio::test]
async fn path_layer_fmt_builds_single_segment_and_encodes() {
    api! {
        client ApiPathLayerFmt {
            scheme: https,
            host: "example.com",
        }

        // Layer path adds: "v1" / fmt["p", {v:String}]
        path "v1" / fmt["p", {v:String}] {
            GET One "x" -> Json<()>;
        }
    }

    use api_path_layer_fmt::*;

    let (transport, recorded) = MockTransport::new(vec![MockReply::ok_json(json_bytes(&()))]);
    let api = ApiPathLayerFmt::new_with_transport(transport);

    // v contains '/', must remain a single segment => %2F
    let _ = api
        .request(endpoints::One::new("a/b".to_string()))
        .execute()
        .await
        .unwrap();

    let req = &recorded.lock().unwrap()[0];
    assert_eq!(req.url.path(), "/v1/pa%2Fb/x");
}

#[tokio::test]
async fn path_layer_fmt_require_all_omits_segment_no_double_slash() {
    api! {
        client ApiPathLayerFmtOpt {
            scheme: https,
            host: "example.com",
        }

        // Layer path: "v1" / fmt?["p", {v?:String}] / "z"
        path "v1" / fmt?["p", {v?:String}] / "z" {
            GET One "x" -> Json<()>;
        }
    }

    use api_path_layer_fmt_opt::*;

    let (transport, recorded) = MockTransport::new(vec![
        MockReply::ok_json(json_bytes(&())),
        MockReply::ok_json(json_bytes(&())),
    ]);
    let api = ApiPathLayerFmtOpt::new_with_transport(transport);

    // v=None => omit fmt segment => "/v1/z/x"
    let _ = api.request(endpoints::One::new()).execute().await.unwrap();

    // v=Some => include fmt segment => "/v1/pk/z/x"
    let _ = api
        .request(endpoints::One::new().v("k".to_string()))
        .execute()
        .await
        .unwrap();

    let reqs = recorded.lock().unwrap();
    assert_eq!(reqs[0].url.path(), "/v1/z/x");
    assert_eq!(reqs[1].url.path(), "/v1/pk/z/x");
}
