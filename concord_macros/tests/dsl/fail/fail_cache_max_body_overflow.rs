use concord_macros::api;

api! {
    client CacheMaxBodyOverflowApi {
        base "https://example.com"

        cache bad {
            max_body 18_446_744_073_709_551_615 gib
        }
    }

    GET Ping
    path ["ping"]
    -> Text<String>
}

fn main() {}
