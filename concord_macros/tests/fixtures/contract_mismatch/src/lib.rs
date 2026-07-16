use concord_macros::api;

api! {
    client MismatchApi {
        base "https://mismatch.example.test";
        scope resources(scope_value: String) {
            path ["scopes", scope.scope_value, "resources"];
            GET ById(endpoint_value: String) -> NoContent {
                path [ep.endpoint_value];
            }
        }
    }
}

pub fn otherwise_complete_surface() {
    let endpoint = MismatchApi::new()
        .resources("mismatch-scope".to_owned())
        .by_id("mismatch-endpoint".to_owned());
    assert_eq!(endpoint.__concord_request_head().method(), "GET");
}
