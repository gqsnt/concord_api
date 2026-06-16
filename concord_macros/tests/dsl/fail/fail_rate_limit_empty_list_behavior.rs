use concord_macros::api;

api! {
    client EmptyRateLimitListBehaviorApi {
        base "https://example.com"

        behavior bad {
            rate_limit []
        }

        defaults {
            behavior bad
        }
    }

    GET Ping
    path ["ping"]
    -> Text<String>
}

fn main() {}
