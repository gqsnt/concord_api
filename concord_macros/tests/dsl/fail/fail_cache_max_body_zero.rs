use concord_macros::api;

api! {
    client CacheMaxBodyZeroApi {
        base "https://example.com"

        cache bad {
            max_body 0 bytes
        }
    }

    GET Ping
    path ["ping"]
    -> Text<String>
}

fn main() {}
