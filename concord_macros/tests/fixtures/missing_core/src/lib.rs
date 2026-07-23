use concord_macros::api;

api! {
    client MissingCoreApi {
        base "https://missing-core.example.test";
    }

    scope resources(scope_value: String) {
        path ["scopes", scope.scope_value, "resources"];
        GET by_id(endpoint_value: String) -> NoContent {
            path [ep.endpoint_value];
        }
    }
}
