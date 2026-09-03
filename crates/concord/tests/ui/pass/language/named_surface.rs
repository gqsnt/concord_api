use concord::{Bytes, Text, api};

api! {
    client SurfaceApi {
        timeout off;
        base "http://surface.example.test/";


    scope resources(scope_value: String, region?: String) {
        path ["resources", resources.scope_value];
        GET fetch(id: String, verbose?: bool, limit: u32 = 25) -> Bytes using Bytes {
            path [endpoint.id];
        }
        POST update(id: String, body body: String using Text) -> Bytes using Bytes {
            path [endpoint.id];
        }
    }
}}

fn bind<E>(client: &SurfaceApi, endpoint: E) -> E::Request
where
    E: surface_api::Endpoint,
{
    client.request(endpoint)
}

fn main() {
    let client = SurfaceApi::new().expect("zero-input generated client");
    let _: surface_api::resources::FetchRequest = client
        .resources("tenant".to_owned())
        .with_region("eu".to_owned())
        .clear_region()
        .fetch("resource".to_owned())
        .with_verbose(true)
        .clear_verbose()
        .set_limit(50)
        .reset_limit();
    let endpoint = SurfaceApi::unbound()
        .resources("tenant".to_owned())
        .fetch("resource".to_owned());
    let _: surface_api::resources::Fetch = endpoint.clone();
    let _: surface_api::resources::FetchRequest = bind(&client, endpoint);
    let update = SurfaceApi::unbound()
        .resources("tenant".to_owned())
        .update("resource".to_owned(), "body".to_owned());
    let _: surface_api::resources::Update = update.clone();
    let _: surface_api::resources::UpdateRequest = client.request(update);
}
