mod common;
use common::*;

use concord_core::prelude::*;
use concord_macros::api;
use http::header::CONTENT_TYPE;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct NewObj {
    id: String,
}

#[tokio::test]
async fn timeout_layering_client_path_endpoint() {
    api! {
      client ApiTimeout {
        scheme: https,
        host: "example.com",
        timeout: core::time::Duration::from_secs(30)
      }

      path "x" {
        timeout: core::time::Duration::from_secs(10),
        GET A "" timeout: core::time::Duration::from_secs(2) -> Json<()>;
      }
    }
    use api_timeout::*;

    let (transport, recorded) = MockTransport::new(vec![MockReply::ok_json(json_bytes(&()))]);
    let api = ApiClient::<Cx>::with_transport(Vars::new(), transport);

    let _ = api.execute(endpoints::A::new()).await.unwrap();
    let req = &recorded.lock().unwrap()[0];
    assert_eq!(req.timeout, Some(core::time::Duration::from_secs(2)));
}

#[tokio::test]
async fn content_type_injection_only_when_missing_and_body_present() {
    api! {
      client ApiBody {
        scheme: https,
        host: "example.com",
      }

      POST A "x" body Json<NewObj> -> Json<()>;

      // Explicit content-type should not be overridden
      POST B "y"
      headers { "content-type" = "text/plain" }
      body Json<NewObj>
      -> Json<()>;

      // GET (no body) must not inject Content-Type
      GET C "z" -> Json<()>;
    }
    use api_body::*;

    // A => inject application/json
    {
        let (transport, recorded) = MockTransport::new(vec![MockReply::ok_json(json_bytes(&()))]);
        let api = ApiClient::<api_body::Cx>::with_transport(api_body::Vars::new(), transport);

        let _ = api.execute(endpoints::A::new(NewObj { id: "1".into() })).await.unwrap();
        let req = &recorded.lock().unwrap()[0];
        assert_eq!(req.headers.get(CONTENT_TYPE).unwrap().to_str().unwrap(), "application/json");
        assert!(req.body.is_some());
    }

    // B => keep text/plain
    {
        let (transport, recorded) = MockTransport::new(vec![MockReply::ok_json(json_bytes(&()))]);
        let api = ApiClient::<Cx>::with_transport(Vars::new(), transport);

        let _ = api.execute(endpoints::B::new(NewObj { id: "1".into() })).await.unwrap();
        let req = &recorded.lock().unwrap()[0];
        assert_eq!(req.headers.get(CONTENT_TYPE).unwrap().to_str().unwrap(), "text/plain");
        assert!(req.body.is_some());
    }

    // C => no Content-Type injected
    {
        let (transport, recorded) = MockTransport::new(vec![MockReply::ok_json(json_bytes(&()))]);
        let api = ApiClient::<Cx>::with_transport(Vars::new(), transport);

        let _ = api.execute(endpoints::C::new()).await.unwrap();
        let req = &recorded.lock().unwrap()[0];
        assert!(req.headers.get(CONTENT_TYPE).is_none());
        assert!(req.body.is_none());
    }
}
