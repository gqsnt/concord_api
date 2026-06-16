use concord_macros::api;

api! {
    client EmptyBehaviorListDefaultsApi {
        base "https://example.com"

        defaults {
            behavior []
        }
    }

    GET Ping
    path ["ping"]
    -> Text<String>
}

fn main() {}
