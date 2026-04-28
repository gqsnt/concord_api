use concord_macros::api;

api! {
    client UiRateLimitUnknownProfileSuggestion {
        base https "example.com"

        rate_limit app {
            bucket application by [host] {
                10 / 1s
            }
        }
    }

    GET Ping
        rate_limit ap
        -> Json<()>;
}

fn main() {}

