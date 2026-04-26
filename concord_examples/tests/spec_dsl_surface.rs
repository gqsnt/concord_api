use concord_core::prelude::*;
use concord_macros::api;
use concord_test_support::*;
use http::header::CONTENT_TYPE;
use std::future::Future;
use std::pin::Pin;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

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
            base https "example.com"
        }

        scope platform(region: Region = Region::Euw) {
            host [region, "api"]

            scope status {
                path ["status"]

                GET Ping -> Json<()> {
                    path ["ping"]
                }
            }
        }
    }

    use api_surface_scope::*;

    let (transport, h) = mock().reply(MockReply::ok_json(json_bytes(&()))).build();

    let api = ApiSurfaceScope::new_with_transport(transport);
    api.request(endpoints::platform::status::Ping::new().region(Region::Na))
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
            base https "example.com"
        }

        POST CreatePost(id: String, body: Json<CreateBody>) -> Json<CreateBody>
                map String {
            r.id
        }
            {
            path ["posts", id]
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

#[tokio::test]
async fn same_endpoint_name_under_different_scopes_is_valid() {
    api! {
        client ApiSurfaceDuplicateNames {
            base https "example.com"
        }

        scope alpha {
            path ["alpha"]

            GET Ping -> Json<()> {
                path ["ping"]
            }
        }

        scope beta {
            path ["beta"]

            GET Ping -> Json<()> {
                path ["ping"]
            }
        }
    }

    use api_surface_duplicate_names::*;

    let (transport, h) = mock()
        .reply(MockReply::ok_json(json_bytes(&())))
        .reply(MockReply::ok_json(json_bytes(&())))
        .build();

    let api = ApiSurfaceDuplicateNames::new_with_transport(transport);
    api.request(endpoints::alpha::Ping::new())
        .execute()
        .await
        .unwrap();
    api.request(endpoints::beta::Ping::new())
        .execute()
        .await
        .unwrap();

    let reqs = h.recorded();
    assert_request(&reqs[0]).path("/alpha/ping");
    assert_request(&reqs[1]).path("/beta/ping");

    h.finish();
}

#[derive(Default)]
struct CountingHooks {
    pre_send_count: Arc<AtomicUsize>,
}

impl RuntimeHooks for CountingHooks {
    fn pre_send<'a>(
        &'a self,
        _ctx: PreSendHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ApiClientError>> + Send + 'a>> {
        Box::pin(async move {
            self.pre_send_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    }
}

#[tokio::test]
async fn generated_runtime_hooks_are_used_by_clones_when_installed_before_clone() {
    api! {
        client ApiSurfaceHooks {
            base https "example.com"
        }

        GET Ping -> Json<()> {
            path ["ping"]
        }
    }

    use api_surface_hooks::*;

    let (transport, h) = mock().reply(MockReply::ok_json(json_bytes(&()))).build();
    let pre_send_count = Arc::new(AtomicUsize::new(0));
    let api = ApiSurfaceHooks::new_with_transport(transport).with_runtime_hooks(Arc::new(
        CountingHooks {
            pre_send_count: pre_send_count.clone(),
        },
    ));
    let clone = api.clone();

    clone
        .request(endpoints::Ping::new())
        .execute()
        .await
        .unwrap();

    assert_eq!(pre_send_count.load(Ordering::SeqCst), 1);
    h.finish();
}

#[tokio::test]
async fn v3_tree_facade_inline_leaf_and_await_work() {
    api! {
        client ApiSurfaceV3 {
            base https "example.com"
            secret api_key: String
            credential upstream = api_key(secret.api_key)

            header "x-client" = "v3"
        }

        scope protected {
            path ["me"]
            auth header "X-Api-Key" = upstream

            GET Me
                as me
                path ["profile"]
                query verbose = true
                -> Json<String>
        }
    }

    use api_surface_v3::*;

    let (transport, h) = mock()
        .reply(MockReply::ok_json(json_bytes(&"alice".to_string())))
        .build();

    let api = ApiSurfaceV3::new_with_transport("secret".to_string(), transport);
    let out = api.protected().me().await.unwrap();

    assert_eq!(out, "alice");
    let reqs = h.recorded();
    assert_request(&reqs[0])
        .path("/me/profile")
        .query_has("verbose", "true")
        .header("x-client", "v3")
        .header("x-api-key", "secret");

    h.finish();
}
