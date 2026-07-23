use concord::api;

api! {
    client MalformedApi {
        base "https://malformed.example.test";
    }

    scope resources(scope_value: String) {
        path ["scopes", scope.scope_value, "resources"];
        FETCH ById(endpoint_value: String) -> NoContent {
            path [ep.endpoint_value];
        }
    }
}

fn main() {}
