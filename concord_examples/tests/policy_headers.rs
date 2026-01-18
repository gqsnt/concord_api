mod common;
use common::*;

use concord_core::prelude::*;
use concord_macros::api;
use http::header::{ACCEPT, USER_AGENT};

#[tokio::test]
async fn header_key_variants_kebab_string_bind_remove_override() {
    api! {
      client ApiHeaders {
        scheme: https,
        host: "example.com",

        headers {
          // ident => kebab-case
          user_agent as user_agent: String = "ua".to_string(),
          x_debug = "caribou",            // => "x-debug"
          "x-static" = "s",              // string key
          "x-flag" as flag: bool = true // bind + default => emitted
        }
      }

      // override x-debug and remove x-static at layer below
      path "p" {
        headers {
          "x-debug" = "override",
          -"x-static"
        }

        // endpoint removes x-flag
        GET One "" headers { -"x-flag" } -> Json<()>;
      }
    }
    use api_headers::*;

    let (transport, recorded) = MockTransport::new(vec![MockReply::ok_json(json_bytes(&()))]);
    let api = ApiHeaders::new_with_transport(transport);

    let _ = api.request(endpoints::One::new()).await.unwrap();
    let req = &recorded.lock().unwrap()[0];

    assert_eq!(req.headers.get(USER_AGENT).unwrap().to_str().unwrap(), "ua");
    assert_eq!(
        req.headers.get("x-debug").unwrap().to_str().unwrap(),
        "override"
    );
    assert!(req.headers.get("x-static").is_none());
    assert!(req.headers.get("x-flag").is_none());
}

#[tokio::test]
async fn header_value_from_cx_to_string_and_invalid_header_value_error() {
    api! {
      client ApiHeaderInvalid {
        scheme: https,
        host: "example.com",

        headers {
          "x-bad" as bad: String,      // client var
          "x-bool" as trace: bool = false,
          "x-bad" = cx.bad,            // uses cx
          "x-bool" = cx.trace         // ToString => "false"
        }
      }

      GET One "" -> Json<()>;
    }
    use api_header_invalid::*;

    // Case OK: trace false emits "false"
    {
        let (transport, recorded) = MockTransport::new(vec![MockReply::ok_json(json_bytes(&()))]);
        let api = ApiHeaderInvalid::new_with_transport("ok".to_string(), transport);

        let _ = api.request(endpoints::One::new()).await.unwrap();
        let req = &recorded.lock().unwrap()[0];
        assert_eq!(
            req.headers.get("x-bool").unwrap().to_str().unwrap(),
            "false"
        );
    }

    // Case invalid header value (newline) => ApiClientError::InvalidParam("header:x-bad") (wrapped)
    {
        let (transport, _recorded) = MockTransport::new(vec![]);
        let api = ApiHeaderInvalid::new_with_transport("a\nb".to_string(), transport);

        let err = api.request(endpoints::One::new()).await.unwrap_err();
        match err {
            ApiClientError::InEndpoint { source, .. } => match *source {
                ApiClientError::InvalidParam(s) => {
                    assert!(s.contains("header"));
                    assert!(s.contains("x-bad"));
                }
                other => panic!("unexpected inner error: {other:?}"),
            },
            other => panic!("unexpected error: {other:?}"),
        }
    }
}

#[tokio::test]
async fn accept_injection_runtime_vs_endpoint_explicit_and_remove() {
    api! {
      client ApiAccept {
        scheme: https,
        host: "example.com",
        headers { "accept" = "text/plain" } // set at client
      }

      // runtime should override to application/json for Json response
      GET A "" -> Json<()>;

      // endpoint explicit set should block runtime override
      GET B "" headers { "accept" = "text/plain" } -> Json<()>;

      // endpoint remove should block runtime injection (Accept absent)
      GET C "" headers { -"accept" } -> Json<()>;
    }
    use api_accept::*;

    // A => accept application/json
    {
        let (transport, recorded) = MockTransport::new(vec![MockReply::ok_json(json_bytes(&()))]);
        let api = ApiAccept::new_with_transport(transport);
        let _ = api.request(endpoints::A::new()).await.unwrap();
        let req = &recorded.lock().unwrap()[0];
        assert_eq!(
            req.headers.get(ACCEPT).unwrap().to_str().unwrap(),
            "application/json"
        );
    }

    // B => accept text/plain
    {
        let (transport, recorded) = MockTransport::new(vec![MockReply::ok_json(json_bytes(&()))]);
        let api = ApiAccept::new_with_transport(transport);
        let _ = api.request(endpoints::B::new()).await.unwrap();
        let req = &recorded.lock().unwrap()[0];
        assert_eq!(
            req.headers.get(ACCEPT).unwrap().to_str().unwrap(),
            "text/plain"
        );
    }

    // C => no accept
    {
        let (transport, recorded) = MockTransport::new(vec![MockReply::ok_json(json_bytes(&()))]);
        let api = ApiAccept::new_with_transport(transport);
        let _ = api.request(endpoints::C::new()).await.unwrap();
        let req = &recorded.lock().unwrap()[0];
        assert!(req.headers.get(ACCEPT).is_none());
    }
}
