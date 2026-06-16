use concord_macros::api;

api! {
    client UnknownRateLimitProfileBehaviorApi {
        base "https://example.com"

        behavior bad {
            rate_limit missing
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
