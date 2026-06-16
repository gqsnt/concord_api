use concord_macros::api;

api! {
    client EmptyRateLimitListEndpointApi {
        base "https://example.com"
    }

    GET Ping
    path ["ping"]
    rate_limit []
    -> Text<String>
}

fn main() {}
