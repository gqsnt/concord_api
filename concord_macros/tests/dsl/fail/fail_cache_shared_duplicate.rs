use concord_macros::api;

api! {
    client CacheSharedDuplicateApi {
        base "https://example.com"

        cache bad {
            shared
            shared
        }
    }

    GET Ping
    path ["ping"]
    -> Text<String>
}

fn main() {}
