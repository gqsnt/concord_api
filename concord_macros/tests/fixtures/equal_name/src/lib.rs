use concord_macros::api;
use std::fmt::{self, Display, Formatter};

pub struct ScopeId(&'static str);

impl Display for ScopeId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

pub struct EndpointId(&'static str);

impl Display for EndpointId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

api! {
    client EqualNameApi {
        base "https://equal-name.example.test";
        scope resources(
            id: crate::ScopeId,
        ) {
            path ["scopes", scope.id];
            GET ById(
                id: crate::EndpointId,
            ) -> NoContent {
                path ["resources", ep.id];
            }
        }
    }
}

pub fn proof() {
    let facade_endpoint = EqualNameApi::new()
        .resources(ScopeId("scope-fixture-id"))
        .by_id(EndpointId("endpoint-fixture-id"));
    let explicit_endpoint = equal_name_api::endpoints::resources::ById::new(
        ScopeId("scope-fixture-id"),
        EndpointId("endpoint-fixture-id"),
    );

    assert_eq!(
        facade_endpoint.__concord_request_head(),
        explicit_endpoint.__concord_request_head()
    );
}
