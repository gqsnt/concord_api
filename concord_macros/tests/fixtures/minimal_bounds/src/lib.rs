use concord_macros::api;
use std::fmt::{self, Display, Formatter};

pub struct MinimalValue(&'static str);

impl Display for MinimalValue {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

api! {
    client BoundsApi {
        base "https://bounds.example.test";
        scope resources(
            scope_value: crate::MinimalValue,
        ) {
            path ["scopes", scope.scope_value];
            GET ById(
                endpoint_value: crate::MinimalValue,
            ) -> NoContent {
                path ["resources", ep.endpoint_value];
            }
        }
    }
}

pub fn proof() {
    let facade = BoundsApi::new().resources(MinimalValue("scope-value"));
    let cloned_facade = facade.clone();
    let endpoint = cloned_facade.by_id(MinimalValue("endpoint-value"));
    assert_eq!(
        endpoint.__concord_request_head().logical_url(),
        "https://bounds.example.test/scopes/scope-value/resources/endpoint-value"
    );

    let explicit = bounds_api::endpoints::resources::ById::new(
        MinimalValue("scope-value"),
        MinimalValue("endpoint-value"),
    );
    assert_eq!(
        endpoint.__concord_request_head(),
        explicit.__concord_request_head()
    );
}
