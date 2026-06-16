use concord_macros::api;

api! {
    client BadPoliciesDefaultRateLimitApi {
        base "https://example.com"

        rate_limit app {
            bucket application by [host] {
                10 / 1s
            }
        }

        policies {
            rate_limit app
        }
    }

    GET Ping
    path ["ping"]
    -> Text<String>
}

fn main() {}
