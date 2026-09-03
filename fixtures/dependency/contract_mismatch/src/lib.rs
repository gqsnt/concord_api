use runtime::api;

api! {
    client MismatchApi {
        timeout off;
        base "https://mismatch.example.test";


    scope resources(scope_value: String) {
        path ["scopes", resources.scope_value, "resources"];
        GET by_id(endpoint_value: String) -> () using ::runtime::NoContent {
            path [endpoint.endpoint_value];
        }
    }
}}

pub fn otherwise_complete_surface() {
    if let Ok(client) = MismatchApi::new() {
        let request = client
            .resources("mismatch-scope".to_owned())
            .by_id("mismatch-endpoint".to_owned());
        let _ = request;
    }
}
