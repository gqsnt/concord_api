use runtime::api;

api! {
    client MismatchApi {
        timeout 30s;
        base "https://mismatch.example.test/";

        scope resources(scope_value: String) {
            path "scopes/{scope_value}/resources";

            GET by_id(endpoint_value: String) -> () via ::runtime::Empty {
                path "{endpoint_value}";
            }
        }
    }
}

pub fn otherwise_complete_surface() {
    if let Ok(client) = MismatchApi::new() {
        let request = client
            .resources("mismatch-scope".to_owned())
            .by_id("mismatch-endpoint".to_owned());
        let _ = request;
    }
}
