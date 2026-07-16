mod outer {
    pub mod inner {
        use concord_macros::api;

        api! {
            client NestedApi {
                base "https://nested.example.test";
                scope resources(scope_value: ::std::string::String) {
                    path ["scopes", scope.scope_value, "resources"];
                    GET ById(endpoint_value: ::std::string::String) -> NoContent {
                        path [ep.endpoint_value];
                    }
                }
            }
        }
    }
}

pub fn proof() {
    let facade = outer::inner::NestedApi::new().resources("nested-scope".to_owned());
    let endpoint = facade.by_id("nested-endpoint".to_owned());
    assert_eq!(endpoint.__concord_request_head().method(), "GET");

    let explicit = outer::inner::nested_api::endpoints::resources::ById::new(
        "nested-scope".to_owned(),
        "nested-endpoint".to_owned(),
    );
    assert_eq!(endpoint.__concord_request_head(), explicit.__concord_request_head());
}
