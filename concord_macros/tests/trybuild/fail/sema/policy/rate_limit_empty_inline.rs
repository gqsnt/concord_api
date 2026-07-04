use concord_macros::api;

api! {
    client EmptyInlineRateLimitApi {
        base "https://example.com"
    }

    GET Ping
        path ["ping"]
        rate_limit {
        }
        -> Json<String>
}

fn main() {}
