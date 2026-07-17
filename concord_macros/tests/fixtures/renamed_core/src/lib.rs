use concord_macros::api;

api! {
    client RenamedApi {
        base "http://renamed.example.test";
        scope resources(scope_value: String) {
            path ["scopes", scope.scope_value, "resources"];
            GET ById(endpoint_value: String) -> NoContent {
                path [ep.endpoint_value];
            }
        }
    }
}

pub fn proof() -> Result<(), runtime::prelude::ClientBuildError> {
    let client = RenamedApi::new()?;
    let request = client
        .resources("renamed-scope".to_owned())
        .by_id("renamed-endpoint".to_owned());
    let _: runtime::Request<_> = request;
    let endpoint = renamed_api::endpoints::resources::ById::new(
        "renamed-scope".to_owned(),
        "renamed-endpoint".to_owned(),
    );
    let _: runtime::Request<_> = client.request(endpoint);
    Ok(())
}
