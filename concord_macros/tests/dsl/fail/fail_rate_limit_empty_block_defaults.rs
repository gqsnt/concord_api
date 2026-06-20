use concord_macros::api;

api! {
    client EmptyRateLimitBlockDefaultsApi {
        base "https://example.com"

        defaults {
            rate_limit {}
        }
    }

    GET Ping
        path ["ping"]
        -> concord_core::prelude::Json<String>
}

fn main() {}
