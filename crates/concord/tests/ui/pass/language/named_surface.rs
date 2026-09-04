use concord::{Bytes, Text, api};

api! {
    client SurfaceApi {
        timeout 30s;
        base "http://surface.example.test/";


    scope resources(scope_value: String, region?: String) {
        path ["resources", resources.scope_value];
        GET fetch(id: String, verbose?: bool, limit: u32 = 25) -> Bytes via Binary {
            path [endpoint.id];
        }
        POST update(id: String, body body: String via Text) -> Bytes via Binary {
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
        .set_region(Some("eu".to_owned()))
        .omit_region()
        .fetch("resource".to_owned())
        .set_verbose(Some(true))
        .omit_verbose()
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
    let _: surface_api::resources::UpdateRequest = client.request(update);
}
