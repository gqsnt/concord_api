use concord::{Binary, Bytes, Empty, Text, api};

api! {
    client FacadeApi {
        timeout 30s;
        base "http://facade.example.test/";


    scope resources(
        scope_value: String,
        region?: String,
        scope_limit: u32 = 50,
    ) {
        path ["resources", resources.scope_value];
        GET fetch(
            id: String,
            verbose?: bool,
            page_size: u32 = 25,
        ) -> Bytes via Binary {
            path [endpoint.id];
        }
        POST update(id: String, body payload: String via Text, revision: u32) -> Bytes via Binary {
            path [endpoint.id];
            headers { "X-Revision" = endpoint.revision; }
        }
        GET delete(id: String) -> () via Empty {
            path [endpoint.id];
        }
    }
}}

#[test]
fn one_facade_dependency_exposes_predictable_named_inherent_surface() {
    let client = FacadeApi::new().expect("zero-input façade client");
    let request = client
        .resources("tenant".to_owned())
        .with_region("eu".to_owned())
        .set_scope_limit(75)
        .reset_scope_limit()
        .fetch("resource".to_owned())
        .with_verbose(true)
        .omit_verbose()
        .set_page_size(100)
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

    let endpoint = FacadeApi::unbound()
        .resources("tenant".to_owned())
        .fetch("resource".to_owned())
        .with_verbose(true)
        .set_page_size(100);
    let cloned = endpoint.clone();
    let stored = [endpoint.clone(), cloned];
    assert_eq!(stored.len(), 2);
    let description = endpoint.description();
    assert_eq!(description.id(), "facade_api.resources.fetch");
    assert_eq!(description.source_path(), "FacadeApi::resources::fetch");
    assert_eq!(
        description.route_shape(),
        "/resources/{resources.scope_value}/{endpoint.id}"
    );
    assert_eq!(description.method(), concord::http::Method::GET);
    assert_eq!(
        description.response_acquisition(),
        concord::endpoint::ResponseAcquisition::Buffered
    );
    let _: facade_api::resources::FetchRequest = client.request(endpoint);

    let body_endpoint = FacadeApi::unbound().resources("tenant".to_owned()).update(
        "resource".to_owned(),
        "body".to_owned(),
        7,
    );
    let _: facade_api::resources::UpdateRequest = client.request(body_endpoint);

    fn bind<E>(client: &FacadeApi, endpoint: E) -> E::Request
    where
        E: facade_api::Endpoint,
    {
        client.request(endpoint)
    }

    fn assert_bytes_response<E>()
    where
        E: facade_api::Endpoint<Response = Binary>,
    {
    }

    fn describe<E>(endpoint: &E) -> &concord::endpoint::EndpointDescription
    where
        E: facade_api::Endpoint,
    {
        E::description(endpoint)
    }

    assert_bytes_response::<facade_api::resources::Fetch>();
    let reusable = FacadeApi::unbound()
        .resources("tenant".to_owned())
        .fetch("resource".to_owned());
    assert_eq!(describe(&reusable).id(), "facade_api.resources.fetch");
    let _: facade_api::resources::FetchRequest = bind(&client, reusable);
}
