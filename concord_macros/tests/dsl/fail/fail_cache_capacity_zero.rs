use concord_macros::api;

api! {
    client CacheCapacityZeroApi {
        base "https://example.com"

        cache bad {
            capacity 0 entries
        }
    }

    GET Ping
    path ["ping"]
    -> Text<String>
}

fn main() {}
