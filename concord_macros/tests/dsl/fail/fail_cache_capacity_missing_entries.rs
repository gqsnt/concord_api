use concord_macros::api;

api! {
    client CacheCapacityMissingEntriesApi {
        base "https://example.com"

        cache bad {
            capacity 10
        }
    }

    GET Ping
    path ["ping"]
    -> Text<String>
}

fn main() {}
