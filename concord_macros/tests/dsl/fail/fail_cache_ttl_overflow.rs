use concord_macros::api;

api! {
    client OverflowCacheTtlApi {
        base "https://example.com"

        cache huge {
            http
            ttl 18446744073709551615m
        }
    }

    GET Ping
    path ["ping"]
    cache huge
    -> Text<String>
}

fn main() {}
