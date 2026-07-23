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

    fn __concord_bind(self, _client: &SealedApi) -> Self::Request {}
}

fn main() {}
