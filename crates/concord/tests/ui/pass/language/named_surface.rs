use concord::{Bytes, Text, api};

api! {
    client SurfaceApi {
        base "http://surface.example.test/";
    }

    scope resources(scope_value: String, region: Option<String>) {
        path ["resources", scope.scope_value];
        GET fetch(id: String, verbose: Option<bool>, limit: u32 = 25) -> Bytes {
            path [ep.id];
        }
        POST update(id: String, #[body] body: Text) -> Bytes {
            path [ep.id];
        }
    }
}

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
        .set_region("eu".to_owned())
        .clear_region()
        .fetch("resource".to_owned())
        .set_verbose(true)
        .clear_verbose()
        .set_limit(50)
        .reset_limit();
    let endpoint =
        surface_api::resources::Fetch::new("tenant".to_owned(), "resource".to_owned());
    let _: surface_api::resources::Fetch = endpoint.clone();
    let _: surface_api::resources::FetchRequest = bind(&client, endpoint);
    let update = surface_api::resources::Update::new(
        "tenant".to_owned(),
        "resource".to_owned(),
        "body".to_owned(),
    );
    let _: surface_api::resources::Update = update.clone();
    let _: surface_api::resources::UpdateRequest = client.request(update);
}
