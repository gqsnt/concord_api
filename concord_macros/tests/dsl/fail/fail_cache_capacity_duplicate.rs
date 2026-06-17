use concord_macros::api;

api! {
    client CacheCapacityDuplicateApi {
        base "https://example.com"

        cache bad {
            capacity 10 entries
            capacity 20 entries
        }
    }

    GET Ping
    path ["ping"]
    -> Text<String>
}

fn main() {}
