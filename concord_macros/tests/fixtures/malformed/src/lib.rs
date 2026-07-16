use concord_macros::api;

api! {
    client MalformedApi {
        base "https://malformed.example.test";
        scope resources(scope_value: String) {
            path ["scopes", scope.scope_value, "resources"];
            POST ById(endpoint_value: String) -> NoContent {
                path [ep.endpoint_value];
            }
        }
    }
}
