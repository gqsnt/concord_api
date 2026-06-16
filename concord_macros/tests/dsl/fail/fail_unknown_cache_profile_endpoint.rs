use concord_macros::api;

api! {
    client UnknownCacheProfileEndpointApi {
        base "https://example.com"
    }

    GET Ping
    path ["ping"]
    cache missing
    -> Text<String>
}

fn main() {}
