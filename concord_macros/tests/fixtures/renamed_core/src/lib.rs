use concord_macros::api;

api! {
    client RenamedApi {
        base "https://renamed.example.test";
        scope resources(scope_value: String) {
            path ["scopes", scope.scope_value, "resources"];
            GET ById(endpoint_value: String) -> NoContent {
                path [ep.endpoint_value];
            }
        }
    }
}

pub fn proof() {
    let endpoint = RenamedApi::new()
        .resources("renamed-scope".to_owned())
        .by_id("renamed-endpoint".to_owned());
    assert_eq!(
        runtime::__private::EndpointSpec::request_meta(&endpoint).method(),
        &runtime::__private::Method::GET
    );
}
