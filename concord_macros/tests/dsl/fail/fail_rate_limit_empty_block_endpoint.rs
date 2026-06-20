use concord_macros::api;

api! {
    client EmptyRateLimitBlockEndpointApi {
        base "https://example.com"
    }

    GET Ping
        path ["ping"]
        rate_limit {}
        -> concord_core::prelude::Json<String>
}

fn main() {}
