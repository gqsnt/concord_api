use concord_macros::api;

api! {
    client DuplicateBehaviorRetryApi {
        base "https://example.com"

        retry read {
            max_attempts 2
            methods [GET]
        }

        behavior bad {
            retry read
            retry off
        }
    }

    GET Ping
    path ["ping"]
    -> Text<String>
}

fn main() {}
