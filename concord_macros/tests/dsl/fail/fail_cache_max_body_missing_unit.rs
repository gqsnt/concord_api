use concord_macros::api;

api! {
    client CacheMaxBodyMissingUnitApi {
        base "https://example.com"

        cache bad {
            max_body 10
        }
    }

    GET Ping
    path ["ping"]
    -> Text<String>
}

fn main() {}
