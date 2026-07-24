use concord::api;

api! {
    client SealedApi {
        base "https://sealed.example.test";
    }

    scope health() {
        path ["health"];
        GET ping() -> NoContent;
    }
}

struct ForgedEndpoint;

impl sealed_api::Endpoint for ForgedEndpoint {
    type Request = ();
    type Response = concord::NoContent;

    fn description(&self) -> &concord::endpoint::EndpointDescription {
        unreachable!()
    }

    fn __concord_bind(self, _client: &SealedApi) -> Self::Request {}

    fn __concord_plan(
        &self,
        _client: &SealedApi,
    ) -> Result<
        concord::endpoint::SafeRequestPlan,
        concord::ApiError,
    > {
        unreachable!()
    }
}

fn main() {}
