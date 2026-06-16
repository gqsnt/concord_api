use concord_macros::api;

api! {
    client DuplicateBehaviorRateLimitApi {
        base "https://example.com"

        rate_limit app {
            bucket application by [host] {
                10 / 1s
            }
        }

        behavior bad {
            rate_limit app
            rate_limit off
        }
    }

    GET Ping
    path ["ping"]
    -> Text<String>
}

fn main() {}
