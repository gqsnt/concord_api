use concord_macros::api;

api! {
    client EmptyRateLimitBlockScopeApi {
        base "https://example.com"
    }

    scope limited {
        rate_limit {}

        GET Ping
            path ["ping"]
            -> concord_core::prelude::Json<String>
    }
}

fn main() {}
