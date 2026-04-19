use concord_core::prelude::*;
use concord_macros::api;
use concord_test_support::*;
use http::header::CONTENT_TYPE;

#[derive(Clone, Debug)]
pub enum Region {
    Euw,
    Na,
}

impl core::fmt::Display for Region {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Region::Euw => f.write_str("euw1"),
            Region::Na => f.write_str("na1"),
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct CreateBody {
    id: String,
}

#[tokio::test]
async fn inline_scope_params_and_nested_endpoint_modules_work() {
    api! {
        client ApiSurfaceScope {
            scheme: https,
            host: "example.com",
        }

        scope platform(region: Region = Region::Euw) {
            host[region, "api"]

            scope status {
                path["status"]

                GET Ping -> Json<()> {
                    path["ping"]
                }
            }
        }
    }

    use api_surface_scope::*;

    let (transport, h) = mock().reply(MockReply::ok_json(json_bytes(&()))).build();

    let api = ApiSurfaceScope::new_with_transport(transport);
    let _ = api
        .request(endpoints::platform::status::Ping::new().region(Region::Na))
        .execute()
        .await
        .unwrap();

    let reqs = h.recorded();
    assert_request(&reqs[0])
        .host("na1.api.example.com")
        .path("/status/ping");

    h.finish();
}

#[tokio::test]
async fn signature_style_endpoint_supports_params_body_and_mapping() {
    api! {
        client ApiSurfaceEndpoint {
            scheme: https,
            host: "example.com",
        }

        POST CreatePost(id: String, body: Json<CreateBody>) -> Json<CreateBody> | String => {
            r.id
        } {
            path["posts", id]
        }
    }

    use api_surface_endpoint::*;

    let reply = CreateBody {
        id: "server-id".into(),
    };
    let (transport, h) = mock().reply(MockReply::ok_json(json_bytes(&reply))).build();

    let api = ApiSurfaceEndpoint::new_with_transport(transport);
    let out = api
        .request(endpoints::CreatePost::new(
            "client-id".to_string(),
            CreateBody {
                id: "body-id".into(),
            },
        ))
        .execute()
        .await
        .unwrap();

    assert_eq!(out, "server-id".to_string());

    let reqs = h.recorded();
    assert_request(&reqs[0])
        .path("/posts/client-id")
        .header(CONTENT_TYPE, "application/json")
        .body_present();

    h.finish();
}
