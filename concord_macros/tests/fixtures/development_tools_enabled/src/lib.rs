use concord_macros::api;

api! {
    client DevelopmentToolsApi {
        base "http://development-tools.example.test/";
    }

    scope resources(id: String) {
        path ["resources", scope.id];
        GET fetch(value: String) -> NoContent {
            path [ep.value];
        }
    }
}

pub fn proof() -> Result<DevelopmentToolsApi, concord_core::prelude::ClientBuildError> {
    DevelopmentToolsApi::new()
}
