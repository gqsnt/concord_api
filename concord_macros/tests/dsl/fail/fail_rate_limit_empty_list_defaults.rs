use concord_macros::api;

api! {
    client EmptyRateLimitListDefaultsApi {
        base "https://example.com"

        defaults {
            rate_limit []
        }
    }

    GET Ping
    path ["ping"]
    -> Text<String>
}

fn main() {}
