use concord_macros::api;

api! {
    client CacheMaxBodyInvalidUnitApi {
        base "https://example.com"

        cache bad {
            max_body 10 bananas
        }
    }

    GET Ping
    path ["ping"]
    -> Text<String>
}

fn main() {}
