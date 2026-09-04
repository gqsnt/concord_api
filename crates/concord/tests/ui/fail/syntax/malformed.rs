use concord::api;

api! {
    client MalformedApi {
        timeout 30s;
        base "https://malformed.example.test";


    scope resources(scope_value: String) {
        path ["scopes", resources.scope_value, "resources"];
        FETCH ById(endpoint_value: String) -> Empty {
            path [endpoint.endpoint_value];
        }
    }
}}

fn main() {}
