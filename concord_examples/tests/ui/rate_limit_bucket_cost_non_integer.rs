use concord_macros::api;

api! {
    client UiRateLimitBucketCostNonInteger {
        scheme: https,
        host: "example.com",
    }

    GET Ping
    -> Json<()>
    {
        rate_limit {
            bucket method by [route.host] {
                cost true // ERROR: expected integer literal
                limit 30 every 10 seconds
            }
        }
    }
}

fn main() {}
