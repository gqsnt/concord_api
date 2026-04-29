use concord_macros::api;

api! {
    client UiRetryUnknownProfileSuggestion {
        base https "example.com"

        retry read {
            max_attempts 2
            methods [GET]
            on [500]
        }
    }

    GET Ping
        retry raed
        -> Json<()>;
}

fn main() {}

