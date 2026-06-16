use concord_macros::api;

api! {
    client DuplicateDefaultsApi {
        base "https://example.com"

        default {
            retry off
        }

        defaults {
            retry off
        }
    }

    GET Ping
    path ["ping"]
    -> Text<String>
}

fn main() {}
