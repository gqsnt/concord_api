use concord_macros::api;

api! {
    client DuplicateRateLimitListBehaviorApi {
        base "https://example.com"

        rate_limit app {
            bucket application by [host] {
                10 / 1s
            }
        }

        behavior bad {
            rate_limit [app, app]
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
