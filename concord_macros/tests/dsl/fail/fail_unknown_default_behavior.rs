use concord_macros::api;

api! {
    client UnknownDefaultBehaviorApi {
        base "https://example.com"

        default {
            behavior missing
        }
    }

    GET Ping
    path ["ping"]
    -> Text<String>
}

fn main() {}
