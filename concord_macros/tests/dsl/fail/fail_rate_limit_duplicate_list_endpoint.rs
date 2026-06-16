use concord_macros::api;

api! {
    client DuplicateRateLimitListEndpointApi {
        base "https://example.com"

        rate_limit app {
            bucket application by [host] {
                10 / 1s
            }
        }
    }

    GET Ping
    path ["ping"]
    rate_limit [app, app]
    -> Text<String>
}

fn main() {}
