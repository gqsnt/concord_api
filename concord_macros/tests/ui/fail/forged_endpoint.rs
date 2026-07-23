use concord_macros::api;

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
    type Response = concord_core::advanced::NoContent;

    fn description(&self) -> &concord_core::advanced::EndpointDescription {
        unreachable!()
    }

    fn __concord_bind(self, _client: &SealedApi) -> Self::Request {}

    fn __concord_plan(
        &self,
        _client: &SealedApi,
    ) -> Result<
        concord_core::advanced::SafeRequestPlan,
        concord_core::advanced::ApiError,
    > {
        unreachable!()
    }
}

fn main() {}
