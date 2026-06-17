use concord_macros::api;

api! {
    client CacheMaxBodyDuplicateApi {
        base "https://example.com"

        cache bad {
            max_body 10 kb
            max_body 20 kb
        }
    }

    GET Ping
    path ["ping"]
    -> Text<String>
}

fn main() {}
