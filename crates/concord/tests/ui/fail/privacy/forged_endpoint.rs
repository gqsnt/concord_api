// V3-L12 / §7.3: the generated intent trait is sealed. A downstream type that
// supplies every visible item still cannot implement `GeneratedEndpoint`
// (the `Sealed` supertrait lives in a private module), and naming that seal
// directly fails because the module is private.
use concord::{Empty, api};

api! {
    client SealedApi {
        timeout 30s;
        base "https://sealed.example.test";

        scope health() {
            path "health";

            GET ping() -> () via Empty;
        }
    }
}

struct ForgedEndpoint;

impl sealed_api::GeneratedEndpoint for ForgedEndpoint {
    type Request = ();

    fn __concord_bind(self, _client: &SealedApi) -> Self::Request {}

    fn __concord_plan(
        &self,
        _client: &SealedApi,
    ) -> Result<concord::endpoint::RequestPlan, concord::ApiError> {
        unreachable!()
    }
}

struct ForgedSeal;

impl sealed_api::__concord_seal::Sealed for ForgedSeal {}

fn main() {}
