use concord_macros::api;
use std::fmt::{self, Display, Formatter};

pub struct ScopeValue(&'static str);

impl Display for ScopeValue {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

pub struct EndpointValue(&'static str);

impl Display for EndpointValue {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

api! {
    client MissingScopeApi {
        base "https://missing-scope.example.test";
        scope resources(scope_value: crate::ScopeValue) {
            path ["scopes", scope.scope_value, "resources"];
            GET ById(endpoint_value: crate::EndpointValue) -> NoContent {
                path [ep.endpoint_value];
            }
        }
    }
}

pub fn must_not_compile() {
    let _endpoint = missing_scope_api::endpoints::resources::ById::new(
        EndpointValue("endpoint-in-scope-position"),
        EndpointValue("endpoint-value"),
    );
}
