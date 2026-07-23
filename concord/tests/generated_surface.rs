use concord::{Bytes, NoContent, Text, api};

api! {
    client FacadeApi {
        base "http://facade.example.test/";
    }

    scope resources(
        scope_value: String,
        region: Option<String>,
        scope_limit: u32 = 50,
    ) {
        path ["resources", scope.scope_value];
        GET Fetch(
            id: String,
            verbose: Option<bool>,
            page_size: u32 = 25,
        ) -> Bytes {
            path [ep.id];
        }
        POST Update(id: String, body: Text) -> Bytes {
            path [ep.id];
        }
        GET Delete(id: String) -> NoContent {
            path [ep.id];
        }
    }
}

#[test]
fn one_facade_dependency_exposes_predictable_named_inherent_surface() {
    let client = FacadeApi::new().expect("zero-input façade client");
    let request = client
        .resources("tenant".to_owned())
        .region("eu".to_owned())
        .scope_limit(75)
        .reset_scope_limit()
        .fetch("resource".to_owned())
        .verbose(true)
        .clear_verbose()
        .page_size(100)
        .reset_page_size();
    assert_eq!(format!("{request:?}"), "FetchRequest { .. }");
    let _: facade_api::resources::FetchRequest = request;

    let _: facade_api::resources::DeleteRequest = client
        .resources("tenant".to_owned())
        .delete("resource".to_owned());
    let _: facade_api::resources::UpdateRequest = client
        .resources("tenant".to_owned())
        .update("resource".to_owned(), "body".to_owned());

    let endpoint = facade_api::resources::Fetch::new("tenant".to_owned(), "resource".to_owned())
        .verbose(true)
        .page_size(100);
    let _: facade_api::resources::Fetch = endpoint.clone();
    let _: facade_api::resources::FetchRequest = client.request(endpoint);

    let body_endpoint = facade_api::resources::Update::new(
        "tenant".to_owned(),
        "resource".to_owned(),
        "body".to_owned(),
    );
    let _: facade_api::resources::Update = body_endpoint.clone();
    let _: facade_api::resources::UpdateRequest = client.request(body_endpoint);

    fn bind<E>(client: &FacadeApi, endpoint: E) -> E::Request
    where
        E: facade_api::Endpoint,
    {
        client.request(endpoint)
    }

    fn assert_bytes_response<E>()
    where
        E: facade_api::Endpoint<Response = Bytes>,
    {
    }

    assert_bytes_response::<facade_api::resources::Fetch>();
    let _: facade_api::resources::FetchRequest = bind(
        &client,
        facade_api::resources::Fetch::new("tenant".to_owned(), "resource".to_owned()),
    );
}
