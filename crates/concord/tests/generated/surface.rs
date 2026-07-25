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
        GET fetch(
            id: String,
            verbose: Option<bool>,
            page_size: u32 = 25,
        ) -> Bytes {
            path [ep.id];
        }
        POST update(id: String, #[body] payload: Text, revision: u32) -> Bytes {
            path [ep.id];
            headers { "X-Revision" = ep.revision; }
        }
        GET delete(id: String) -> NoContent {
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
    let _: facade_api::resources::UpdateRequest =
        client
            .resources("tenant".to_owned())
            .update("resource".to_owned(), "body".to_owned(), 7);

    let endpoint = facade_api::resources::Fetch::new("tenant".to_owned(), "resource".to_owned())
        .verbose(true)
        .page_size(100);
    let cloned = endpoint.clone();
    let stored = [endpoint.clone(), cloned];
    assert_eq!(stored.len(), 2);
    let description = endpoint.description();
    assert_eq!(description.id(), "facade_api.resources.fetch");
    assert_eq!(description.source_path(), "FacadeApi::resources::fetch");
    assert_eq!(
        description.route_shape(),
        "/resources/{scope.scope_value}/{ep.id}"
    );
    assert_eq!(description.method(), concord::http::Method::GET);
    assert_eq!(
        description.response_acquisition(),
        concord::endpoint::ResponseAcquisition::Buffered
    );
    let _: facade_api::resources::FetchRequest = client.request(endpoint);

    let body_endpoint = facade_api::resources::Update::new(
        "tenant".to_owned(),
        "resource".to_owned(),
        "body".to_owned(),
        7,
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

    fn describe<E>(endpoint: &E) -> &concord::endpoint::EndpointDescription
    where
        E: facade_api::Endpoint,
    {
        E::description(endpoint)
    }

    assert_bytes_response::<facade_api::resources::Fetch>();
    let reusable = facade_api::resources::Fetch::new("tenant".to_owned(), "resource".to_owned());
    assert_eq!(describe(&reusable).id(), "facade_api.resources.fetch");
    let _: facade_api::resources::FetchRequest = bind(&client, reusable);
}
