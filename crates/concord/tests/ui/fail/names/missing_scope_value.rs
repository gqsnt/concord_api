// Required uninitialized scope/endpoint parameters are positional, typed
// arguments in declaration order (§4.3): an endpoint value cannot be passed
// in the scope position, even when both types are valid path atoms.
use concord::{AtomError, BoundedAtomWriter, Empty, RequestAtom, api};

pub struct ScopeValue(&'static str);

impl RequestAtom for ScopeValue {
    fn write_atom(&self, out: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        out.push_str(self.0)
    }
}

pub struct EndpointValue(&'static str);

impl RequestAtom for EndpointValue {
    fn write_atom(&self, out: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        out.push_str(self.0)
    }
}

api! {
    client MissingScopeApi {
        base "https://missing-scope.example.test";

        scope resources(scope_value: crate::ScopeValue) {
            path "scopes/{scope_value}/resources";

            GET by_id(endpoint_value: crate::EndpointValue) -> () via Empty {
                path "{endpoint_value}";
            }
        }
    }
}

fn main() {
    let _endpoint = MissingScopeApi::unbound()
        .resources(EndpointValue("endpoint-in-scope-position"))
        .by_id(EndpointValue("endpoint-value"));
}
