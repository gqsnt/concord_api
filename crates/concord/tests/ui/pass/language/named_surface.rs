// §7.1 / §7.3: the named generated surface. Scope handles, `<Name>Endpoint`
// unbound intents (Clone when body-free, VALUE-05), `<Name>Request` requests,
// binding through the client-specific `GeneratedEndpoint` trait (V3-L12),
// and the value-control matrix (VALUE-03). v2 path atom lists become relative
// templates and `scope.x`/`endpoint.x` self-references become bare names
// (LANG-03).
use concord::{Binary, Bytes, Text, api};

api! {
    client SurfaceApi {
        timeout 30s;
        base "http://surface.example.test/";

        scope resources(scope_value: String, region?: String) {
            path "resources/{scope_value}";
            query { "region" = region; }

            GET fetch(id: String, verbose?: bool, limit: u32 = 25) -> Bytes via Binary {
                path "{id}";
                query {
                    "verbose" = verbose;
                    "limit" = limit;
                }
            }
            POST update(id: String, body body: String via Text) -> Bytes via Binary {
                path "{id}";
            }
        }
    }
}

fn bind<E>(client: &SurfaceApi, endpoint: E) -> E::Request
where
    E: surface_api::GeneratedEndpoint,
{
    client.request(endpoint)
}

fn main() {
    let client = SurfaceApi::new().expect("zero-input generated client");
    let _: surface_api::resources::Resources = client.resources("tenant");
    let _: surface_api::resources::FetchRequest = client
        .resources("tenant".to_owned())
        .set_region("eu")
        .replace_region(Some("eu".to_owned()))
        .omit_region()
        .fetch("resource".to_owned())
        .set_verbose(true)
        .replace_verbose(Some(true))
        .omit_verbose()
        .set_limit(50)
        .reset_limit();
    let endpoint = SurfaceApi::unbound()
        .resources("tenant".to_owned())
        .fetch("resource".to_owned());
    let _: surface_api::resources::FetchEndpoint = endpoint.clone();
    let _: surface_api::resources::FetchRequest = bind(&client, endpoint);
    let update = SurfaceApi::unbound()
        .resources("tenant".to_owned())
        .update("resource".to_owned(), "body".to_owned());
    let _: surface_api::resources::UpdateRequest = client.request(update);
}
