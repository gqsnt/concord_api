use concord::api;
use std::fmt::{self, Display, Formatter};

struct ScopeValue(&'static str);

impl Display for ScopeValue {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

struct EndpointValue(&'static str);

impl Display for EndpointValue {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

api! {
    client MissingScopeApi {
        timeout off;
        base "https://missing-scope.example.test";


    scope resources(scope_value: crate::ScopeValue) {
        path ["scopes", resources.scope_value, "resources"];
        GET by_id(endpoint_value: crate::EndpointValue) -> () using NoContent {
            path [endpoint.endpoint_value];
        }
    }
}}

fn main() {
    let _endpoint = MissingScopeApi::unbound()
        .resources(EndpointValue("endpoint-in-scope-position"))
        .by_id(EndpointValue("endpoint-value"));
}
