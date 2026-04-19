use concord_macros::api;

api! {
    client UiRateLimitUnknownKey {
        scheme: https,
        host: "example.com",
    }

    GET Ping
    -> Json<()>
    {
        rate_limit {
            bucket method by [region, endpoint] { // ERROR: unknown key
                limit 30 every 10 seconds
            }
        }
    }
}

fn main() {}
