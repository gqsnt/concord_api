use concord_macros::api;

api! {
    client UnknownRateLimitProfileEndpointApi {
        base "https://example.com"
    }

    GET Ping
    path ["ping"]
    rate_limit missing
    -> Text<String>
}

fn main() {}
