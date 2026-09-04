use concord::api;

api! {
    client SealedApi {
        timeout 30s;
        base "https://sealed.example.test";


    scope health() {
        path ["health"];
        GET ping() -> () via Empty;
    }
}}

struct ForgedEndpoint;

impl sealed_api::Endpoint for ForgedEndpoint {
    type Request = ();
    type Response = concord::Empty;

    fn description(&self) -> &concord::endpoint::EndpointDescription {
        unreachable!()
    }

    fn __concord_bind(self, _client: &SealedApi) -> Self::Request {}

    fn __concord_plan(
        &self,
        _client: &SealedApi,
    ) -> Result<
        concord::endpoint::RequestPlan,
        concord::ApiError,
    > {
        unreachable!()
    }
}

fn main() {}
