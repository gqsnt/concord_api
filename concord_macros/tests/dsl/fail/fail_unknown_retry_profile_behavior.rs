use concord_macros::api;

api! {
    client UnknownRetryProfileBehaviorApi {
        base "https://example.com"

        behavior bad {
            retry missing
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
