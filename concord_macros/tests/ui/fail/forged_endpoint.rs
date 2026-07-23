use concord_macros::api;

api! {
    client SealedApi {
        base "https://sealed.example.test";
    }

    scope health() {
        path ["health"];
        GET Ping() -> NoContent;
    }
}

struct ForgedEndpoint;

impl sealed_api::SealedApiEndpoint for ForgedEndpoint {}

fn main() {}
